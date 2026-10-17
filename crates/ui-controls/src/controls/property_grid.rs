//! Retained property inspection over one transactional source and explicit keys.
use super::data_form::{checked_format, create_editor, parse_value};
use super::property_grid_model::{aggregate, invalid};
use super::*;
use rust_desktop_ui_core::{Align, GridTrack};
use rust_desktop_ui_data::{CellValue, DataWrite, FieldSchema, TableSchema};

#[path = "property_grid/values.rs"]
mod values;
use values::*;

struct Property {
    descriptor: PropertyDescriptor,
    schema: FieldSchema,
    recipe: DataFormEditorFactory,
    row: WidgetId,
    host: WidgetId,
    owner: WidgetId,
    editor: WidgetId,
    mixed: WidgetId,
    edit: WidgetId,
    apply: WidgetId,
    reset: WidgetId,
    null: Option<WidgetId>,
    baseline: Vec<CellValue>,
    seed: CellValue,
    editing: bool,
}

/// Owns inspector state beside Controls. Call update after input and before
/// draining generic activations. It consumes only its own action buttons.
/// Accepted values are source/business values; editor preparation never changes
/// them. A failed source setter preserves accepted values and the editable draft.
pub struct PropertyGrid {
    pub container: WidgetId,
    pub summary: WidgetId,
    source: DataSourceHandle,
    schema: TableSchema,
    objects: Vec<i64>,
    properties: Vec<Property>,
    groups: Vec<(Vec<String>, Expander)>,
    parents: Vec<(WidgetId, WidgetId)>,
    error: Option<String>,
    changes: Vec<PropertyChanged>,
}
impl PropertyGrid {
    pub fn mount(
        ui: &mut Controls,
        parent: WidgetId,
        source: DataSourceHandle,
        objects: &[i64],
        settings: PropertyGridSettings,
        factories: &DataFormFactories,
        mut style: LayoutStyle,
    ) -> Result<Self, ControlError> {
        settings.validate()?;
        if objects.is_empty()
            || objects.len() > 64
            || objects.iter().collect::<HashSet<_>>().len() != objects.len()
        {
            return Err(invalid("PropertyGrid requires 1..64 unique object keys"));
        }
        let data = source
            .try_borrow()
            .map_err(|_| invalid("Property source is borrowed"))?;
        let schema = data.table().schema().clone();
        let metadata = if settings.properties.is_empty() {
            schema
                .fields()
                .iter()
                .map(|f| PropertyDescriptor::new(&f.name))
                .collect()
        } else {
            settings.properties.clone()
        };
        if metadata.is_empty() || metadata.len() > 128 {
            return Err(invalid("PropertyGrid requires 1..128 properties"));
        }
        let mut definitions = Vec::new();
        let mut metadata_bytes = 0usize;
        for mut descriptor in metadata {
            let field = schema
                .field(&descriptor.metadata.field)
                .ok_or_else(|| invalid("Unknown property field"))?
                .clone();
            descriptor.metadata.read_only |= settings.read_only || field.read_only;
            let recipe = factories.resolve(&descriptor.metadata.editor, &field)?;
            if let DataFormEditorFactory::Text(s) = &recipe {
                descriptor.metadata.read_only |= s.is_read_only;
            }
            let baseline = objects
                .iter()
                .map(|key| {
                    data.value(*key, &field.name)
                        .cloned()
                        .ok_or_else(|| invalid("Unknown inspected object"))
                })
                .collect::<Result<Vec<_>, _>>()?;
            for value in &baseline {
                field.validate(value).map_err(|e| invalid(e.to_string()))?;
                checked_format(&recipe, &field, value)?;
            }
            if let Some(default) = &descriptor.default {
                field
                    .validate(default)
                    .map_err(|e| invalid(e.to_string()))?;
                checked_format(&recipe, &field, default)?;
            }
            let seed = neutral_value(&recipe, &field, descriptor.default.as_ref())?;
            metadata_bytes += descriptor.category.len()
                + descriptor.groups.iter().map(String::len).sum::<usize>()
                + field.name.len()
                + descriptor
                    .metadata
                    .label
                    .as_deref()
                    .unwrap_or(&field.title)
                    .len()
                + descriptor.metadata.editor.len();
            if metadata_bytes > 32768 {
                return Err(invalid("Property metadata exceeds 32 KiB"));
            }
            for label in
                std::iter::once(descriptor.metadata.label.as_deref().unwrap_or(&field.title))
                    .chain(std::iter::once(descriptor.category.as_str()))
                    .chain(descriptor.groups.iter().map(String::as_str))
            {
                ui.measure_text(label, 4096.0, false)?;
            }
            definitions.push((descriptor, field, recipe, baseline, seed));
        }
        drop(data);
        style.kind = LayoutKind::Stack(Axis::Vertical);
        style.gap = 8.0;
        let container = ui.transparent_panel(parent, style)?;
        ui.set_role(container, Role::Group);
        ui.set_automation_name(container, "Свойства объектов")?;
        let result = (|| {
            let mut groups: Vec<(Vec<String>, Expander)> = Vec::new();
            let mut properties = Vec::new();
            for (descriptor, schema, recipe, baseline, seed) in definitions {
                let path: Vec<_> = std::iter::once(descriptor.category.clone())
                    .chain(descriptor.groups.clone())
                    .collect();
                let mut host = container;
                for depth in 1..=path.len() {
                    let prefix = path[..depth].to_vec();
                    if let Some((_, group)) = groups.iter().find(|(key, _)| *key == prefix) {
                        host = group.content;
                    } else {
                        if groups.len() >= 256 {
                            return Err(invalid(
                                "PropertyGrid supports at most 256 expandable groups",
                            ));
                        }
                        let group = ui.expander(
                            host,
                            GroupSettings {
                                header: path[depth - 1].clone(),
                                appearance: GroupAppearance::Plain,
                                header_style: GroupHeaderStyle::Light,
                                padding: Edges::all(6.0),
                                ..Default::default()
                            },
                            true,
                            LayoutStyle {
                                width: Length::Percent(1.0),
                                ..Default::default()
                            },
                        )?;
                        ui.edit_props(group.content, |p| {
                            p.style.kind = LayoutKind::Stack(Axis::Vertical);
                            p.style.gap = 6.0;
                        })?;
                        host = group.content;
                        groups.push((prefix, group));
                    }
                }
                let row = ui.transparent_panel(
                    host,
                    LayoutStyle {
                        kind: LayoutKind::Grid,
                        grid_columns: vec![
                            GridTrack::Px(settings.label_width),
                            GridTrack::Fr(1.0),
                            GridTrack::Auto,
                            GridTrack::Auto,
                            GridTrack::Auto,
                        ],
                        gap: 6.0,
                        width: Length::Percent(1.0),
                        ..Default::default()
                    },
                )?;
                let label = descriptor
                    .metadata
                    .label
                    .as_deref()
                    .unwrap_or(&schema.title);
                ui.label(
                    row,
                    label,
                    LayoutStyle {
                        vertical_alignment: Some(Align::Center),
                        ..Default::default()
                    },
                )?;
                let host = ui.transparent_panel(
                    row,
                    LayoutStyle {
                        // Stack deliberately ignores child flex_grow. The
                        // editor and mixed-value caption share this remaining
                        // Grid column, while the NULL toggle keeps its width.
                        kind: LayoutKind::Flex(Axis::Horizontal),
                        gap: 6.0,
                        ..Default::default()
                    },
                )?;
                let state = aggregate(&baseline);
                let value = match &state {
                    PropertyValue::Uniform(v) => v,
                    PropertyValue::Mixed => &seed,
                };
                let (owner, editor) = create_editor(ui, host, label, &schema, &recipe, value)?;
                ui.edit_props(owner, |p| {
                    p.style.flex_grow = 1.0;
                })?;
                ui.set_automation_name(editor, label)?;
                let null = if schema.nullable && !matches!(recipe, DataFormEditorFactory::CheckBox)
                {
                    // Keep enough room for the Fluent checkbox indicator,
                    // chrome insets and the full caption at the largest theme font.
                    let id = ui.checkbox(
                        host,
                        "NULL",
                        *value == CellValue::Null,
                        LayoutStyle {
                            min_size: Size::new(84.0, 0.0),
                            flex_shrink: 0.0,
                            ..button_style(84.0)
                        },
                    )?;
                    ui.set_automation_name(id, format!("{label}: значение отсутствует"))?;
                    Some(id)
                } else {
                    None
                };
                let mixed = ui.label(
                    host,
                    "Разные значения",
                    LayoutStyle {
                        flex_grow: 1.0,
                        vertical_alignment: Some(Align::Center),
                        ..Default::default()
                    },
                )?;
                ui.set_text_auto_height(mixed, true)?;
                ui.set_automation_name(mixed, format!("{label}: разные значения"))?;
                let edit = ui.button(row, "Изменить", None, button_style(90.0))?;
                let apply = ui.button(row, "Применить", None, button_style(104.0))?;
                let reset = ui.button(row, "Сбросить", None, button_style(94.0))?;
                for (id, action) in [
                    (edit, "изменить"),
                    (apply, "применить ко всем"),
                    (reset, "значение по умолчанию"),
                ] {
                    ui.set_automation_name(id, format!("{label}: {action}"))?;
                }
                if matches!(recipe, DataFormEditorFactory::CheckBox) {
                    ui.set_enabled(owner, !descriptor.metadata.read_only)?;
                } else if owner != editor {
                    ui.set_composite_read_only(owner, descriptor.metadata.read_only)?;
                } else {
                    ui.set_read_only(editor, descriptor.metadata.read_only)?;
                }
                ui.edit_props(row, |p| {
                    p.visible = descriptor.metadata.visible;
                    p.collapsed = !descriptor.metadata.visible;
                })?;
                let mut property = Property {
                    descriptor,
                    schema,
                    recipe,
                    row,
                    host,
                    owner,
                    editor,
                    mixed,
                    edit,
                    apply,
                    reset,
                    null,
                    baseline,
                    seed,
                    editing: matches!(state, PropertyValue::Uniform(_)),
                };
                present(ui, &mut property)?;
                properties.push(property);
            }
            let summary = ui.label(container, "", LayoutStyle::default())?;
            ui.set_automation_name(summary, "Результат изменения свойств")?;
            ui.set_text_auto_height(summary, true)?;
            let mut parents = Vec::new();
            let mut stack = ui
                .runtime
                .tree()
                .children(container)
                .unwrap_or(&[])
                .to_vec();
            while let Some(id) = stack.pop() {
                parents.push((id, ui.runtime.tree().parent(id).unwrap()));
                stack.extend_from_slice(ui.runtime.tree().children(id).unwrap_or(&[]));
            }
            Ok(Self {
                container,
                summary,
                source,
                schema,
                objects: objects.to_vec(),
                properties,
                groups,
                parents,
                error: None,
                changes: Vec::new(),
            })
        })();
        if result.is_err() {
            let _ = ui.remove(container);
        }
        result
    }
    pub fn object_keys(&self) -> &[i64] {
        &self.objects
    }
    pub fn property(&self, name: &str) -> Option<WidgetId> {
        Some(
            self.properties
                .iter()
                .find(|p| p.schema.name == name)?
                .owner,
        )
    }
    pub fn property_editor(&self, name: &str) -> Option<WidgetId> {
        Some(
            self.properties
                .iter()
                .find(|p| p.schema.name == name)?
                .editor,
        )
    }
    pub fn property_row(&self, name: &str) -> Option<WidgetId> {
        Some(self.properties.iter().find(|p| p.schema.name == name)?.row)
    }
    pub fn property_actions(&self, name: &str) -> Option<(WidgetId, WidgetId, WidgetId)> {
        let p = self.properties.iter().find(|p| p.schema.name == name)?;
        Some((p.edit, p.apply, p.reset))
    }
    pub fn property_null_toggle(&self, name: &str) -> Option<WidgetId> {
        self.properties.iter().find(|p| p.schema.name == name)?.null
    }
    pub fn group(&self, path: &[&str]) -> Option<Expander> {
        self.groups
            .iter()
            .find(|(p, _)| p.iter().map(String::as_str).eq(path.iter().copied()))
            .map(|(_, g)| *g)
    }
    pub fn accepted(&self, name: &str) -> Option<PropertyValue> {
        Some(aggregate(
            &self
                .properties
                .iter()
                .find(|p| p.schema.name == name)?
                .baseline,
        ))
    }
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }
    pub fn take_changes(&mut self) -> Vec<PropertyChanged> {
        std::mem::take(&mut self.changes)
    }
    pub fn draft(&self, ui: &Controls, name: &str) -> Result<Option<CellValue>, ControlError> {
        self.check(ui)?;
        let p = &self.properties[self.index(name)?];
        if !p.editing {
            return Ok(None);
        }
        read(ui, p).map(Some).map_err(invalid)
    }
    fn index(&self, name: &str) -> Result<usize, ControlError> {
        self.properties
            .iter()
            .position(|p| p.schema.name == name)
            .ok_or_else(|| invalid("Unknown property"))
    }
    fn check(&self, ui: &Controls) -> Result<(), ControlError> {
        if !ui.runtime.tree().contains(self.container)
            || self
                .parents
                .iter()
                .any(|(id, parent)| ui.runtime.tree().parent(*id) != Some(*parent))
        {
            return Err(invalid(
                "PropertyGrid required parts were removed or reparented",
            ));
        }
        for p in &self.properties {
            check_recipe(ui, p)?;
        }
        let source = self
            .source
            .try_borrow()
            .map_err(|_| invalid("Property source is borrowed"))?;
        if source.table().schema() != &self.schema
            || self.objects.iter().any(|key| source.row(*key).is_none())
        {
            return Err(invalid(
                "Inspected schema or object identities changed; reselect explicitly",
            ));
        }
        Ok(())
    }
    pub fn begin_edit(&mut self, ui: &mut Controls, name: &str) -> Result<(), ControlError> {
        self.check(ui)?;
        let i = self.index(name)?;
        let p = &mut self.properties[i];
        if p.descriptor.metadata.read_only {
            return Err(invalid("Property is read-only"));
        }
        if !p.editing {
            let seed = p.seed.clone();
            load(ui, p, &seed)?;
            p.editing = true;
            present(ui, p)?;
        }
        if ui.window_focused
            && ui.effective_enabled(p.editor)
            && ui
                .runtime
                .tree()
                .node(p.editor)
                .is_some_and(|node| node.props.focusable)
        {
            match ui.runtime.apply(Mutation::Focus(Some(p.editor))) {
                Ok(()) | Err(RuntimeError::IneligibleFocus(_)) => {}
                Err(error) => return Err(error.into()),
            }
        }
        Ok(())
    }
    /// Prepare an explicit typed draft. This does not write the source.
    pub fn set_draft(
        &mut self,
        ui: &mut Controls,
        name: &str,
        value: CellValue,
    ) -> Result<(), ControlError> {
        self.check(ui)?;
        let i = self.index(name)?;
        let p = &mut self.properties[i];
        if p.descriptor.metadata.read_only {
            return Err(invalid("Property is read-only"));
        }
        load(ui, p, &value)?;
        p.editing = true;
        present(ui, p)
    }
    pub fn cancel(&mut self, ui: &mut Controls, name: &str) -> Result<(), ControlError> {
        self.check(ui)?;
        let i = self.index(name)?;
        let p = &mut self.properties[i];
        let value = match aggregate(&p.baseline) {
            PropertyValue::Uniform(v) => v,
            PropertyValue::Mixed => p.seed.clone(),
        };
        if ui
            .document(p.editor)
            .is_some_and(|d| d.composition().is_some())
        {
            ui.cancel_preedit();
            return Ok(());
        }
        load(ui, p, &value)?;
        p.editing = matches!(aggregate(&p.baseline), PropertyValue::Uniform(_));
        present(ui, p)
    }
    pub fn reset(&mut self, ui: &mut Controls, name: &str) -> Result<PropertyCommit, ControlError> {
        self.check(ui)?;
        let i = self.index(name)?;
        let value = self.properties[i]
            .descriptor
            .default
            .clone()
            .ok_or_else(|| invalid("Property has no default"))?;
        self.set_draft(ui, name, value)?;
        self.commit(ui, name)
    }
    pub fn commit(
        &mut self,
        ui: &mut Controls,
        name: &str,
    ) -> Result<PropertyCommit, ControlError> {
        self.check(ui)?;
        let i = self.index(name)?;
        let p = &self.properties[i];
        if p.descriptor.metadata.read_only {
            return self.reject(ui, "Property is read-only");
        }
        if !p.editing {
            return Ok(PropertyCommit::Unchanged);
        }
        if self.changes.len() >= 256 {
            return self.reject(
                ui,
                "Consume property change events before applying more changes",
            );
        }
        if ui
            .document(p.editor)
            .is_some_and(|d| d.composition().is_some())
        {
            return Ok(PropertyCommit::Pending);
        }
        let value = match read(ui, p) {
            Ok(v) => v,
            Err(e) => return self.reject(ui, &e),
        };
        if ui.document(p.editor).is_some() && p.null.is_none_or(|id| ui.checked(id) != Some(true)) {
            match ui.commit_edit(p.editor)? {
                CommitOutcome::Invalid(e) => return self.reject(ui, &e),
                CommitOutcome::Pending | CommitOutcome::CompositionActive => {
                    return Ok(PropertyCommit::Pending);
                }
                _ => {}
            }
        }
        // Read accepted preparation again: a typed editor may normalize its display.
        let p = &self.properties[i];
        let accepted = read(ui, p).map_err(invalid)?;
        if accepted != value {
            return self.reject(ui, "Editor normalization changed the prepared typed value");
        }
        let changed = p.baseline.iter().any(|old| *old != value);
        // Include unchanged targets too: their optimistic expectations still
        // belong to the single multi-object transaction.
        let operations: Vec<_> = self
            .objects
            .iter()
            .zip(&p.baseline)
            .map(|(key, old)| DataWrite::Update {
                key: *key,
                field: p.schema.name.clone(),
                expected: old.clone(),
                value: value.clone(),
            })
            .collect();
        // All UI validation happens before storage. Success publishes only owned data.
        ui.set_label(self.summary, "")?;
        self.error = None;
        let result = self
            .source
            .try_borrow_mut()
            .map_err(|_| invalid("Property source is borrowed"))?
            .write_batch(&operations);
        if let Err(e) = result {
            return self.reject(ui, &e.to_string());
        }
        if !changed {
            return Ok(PropertyCommit::Unchanged);
        }
        self.properties[i].baseline = vec![value.clone(); self.objects.len()];
        self.changes.push(PropertyChanged {
            field: name.into(),
            value,
            objects: self.objects.clone(),
        });
        Ok(PropertyCommit::Saved)
    }
    fn reject(&mut self, ui: &mut Controls, message: &str) -> Result<PropertyCommit, ControlError> {
        let mut message = bounded_error(message);
        if ui.measure_text(&message, 4096.0, false).is_err() {
            message = "Property change was rejected".into();
        }
        ui.set_label(self.summary, &message)?;
        self.error = Some(message.clone());
        Ok(PropertyCommit::Rejected(message))
    }
    /// Refresh from the current source snapshot only; no adapter I/O. Dirty
    /// properties keep their baseline and draft and report conflicts. Refresh
    /// applies per property, not as an all-property UI transaction: if a later
    /// editor rejects loading, earlier successful clean refreshes remain valid.
    pub fn refresh(&mut self, ui: &mut Controls) -> Result<(), ControlError> {
        self.check(ui)?;
        let source = self
            .source
            .try_borrow()
            .map_err(|_| invalid("Property source is borrowed"))?;
        let values: Vec<Vec<CellValue>> = self
            .properties
            .iter()
            .map(|p| {
                self.objects
                    .iter()
                    .map(|k| source.value(*k, &p.schema.name).unwrap().clone())
                    .collect()
            })
            .collect();
        drop(source);
        let mut conflict = false;
        for (p, new) in self.properties.iter_mut().zip(values) {
            if new == p.baseline {
                continue;
            }
            if dirty(ui, p) {
                conflict = true;
                continue;
            }
            let (value, editing) = match aggregate(&new) {
                PropertyValue::Uniform(v) => (v, true),
                PropertyValue::Mixed => (p.seed.clone(), false),
            };
            load(ui, p, &value)?;
            p.baseline = new;
            p.editing = editing;
            present(ui, p)?;
        }
        if conflict {
            let _ = self.reject(
                ui,
                "Source changed while a property has a prepared draft; cancel or retry explicitly",
            )?;
        }
        Ok(())
    }
    /// Consumes only this inspector's action buttons; other activations remain.
    pub fn update(&mut self, ui: &mut Controls) -> Result<bool, ControlError> {
        self.check(ui)?;
        let mut actions = Vec::new();
        ui.activated.retain(|id| {
            if let Some((i, action)) = self.properties.iter().enumerate().find_map(|(i, p)| {
                [(p.edit, 0), (p.apply, 1), (p.reset, 2)]
                    .iter()
                    .find(|(button, _)| button == id)
                    .map(|(_, a)| (i, *a))
            }) {
                actions.push((i, action));
                false
            } else {
                true
            }
        });
        let changed = !actions.is_empty();
        for (i, action) in actions {
            let name = self.properties[i].schema.name.clone();
            let result = match action {
                0 => self.begin_edit(ui, &name).map(|_| ()),
                1 => self.commit(ui, &name).map(|_| ()),
                _ => self.reset(ui, &name).map(|_| ()),
            };
            if let Err(e) = result {
                let _ = self.reject(ui, &e.to_string())?;
            }
        }
        for p in &mut self.properties {
            present(ui, p)?;
        }
        Ok(changed)
    }
    /// Optional pre-dispatch Escape route: composition first, then this draft.
    pub fn input(&mut self, ui: &mut Controls, event: &InputEvent) -> Result<bool, ControlError> {
        if !matches!(
            event,
            InputEvent::KeyDown {
                key: Key::Escape,
                repeat: false,
                ..
            }
        ) {
            return Ok(false);
        }
        let name = ui.runtime.focused().and_then(|id| {
            self.properties
                .iter()
                .find(|p| ui.flyout_descendant(id, p.host))
                .map(|p| p.schema.name.clone())
        });
        if let Some(name) = name {
            self.cancel(ui, &name)?;
            return Ok(true);
        }
        Ok(false)
    }
}
fn button_style(width: f32) -> LayoutStyle {
    LayoutStyle {
        width: Length::Px(width),
        height: Length::Px(32.0),
        vertical_alignment: Some(Align::Center),
        ..Default::default()
    }
}
fn bounded_error(message: &str) -> String {
    // Bound each grapheme before shaping, even for hostile adapter errors.
    let mut result = String::new();
    for ch in message.chars().filter(|c| !c.is_control()).take(256) {
        if ch.is_ascii()
            || ch.is_alphanumeric()
            || ch.is_whitespace()
            || "—.,:;!?()[]+-/".contains(ch)
        {
            result.push(ch);
        }
    }
    if result.is_empty() {
        "Property change was rejected".into()
    } else {
        result
    }
}
