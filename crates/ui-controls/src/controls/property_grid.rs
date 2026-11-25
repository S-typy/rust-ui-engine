//! Retained property inspection over one transactional source and explicit keys.
use super::data_form::{checked_format, create_editor, parse_value};
use super::property_grid_model::{aggregate, invalid};
use super::*;
use rust_desktop_ui_core::{Align, GridTrack, Modifiers};
use rust_desktop_ui_data::{CellValue, DataWrite, FieldSchema, TableSchema};

#[path = "property_grid/values.rs"]
mod values;
use values::*;
#[path = "property_grid/presentation.rs"]
mod presentation;
pub(super) use presentation::InspectorPart;
use presentation::{action_icon, show, vertical_stack};

struct Property {
    descriptor: PropertyDescriptor,
    schema: FieldSchema,
    recipe: DataFormEditorFactory,
    row: WidgetId,
    host: WidgetId,
    owner: WidgetId,
    editor: WidgetId,
    active: bool,
    value_label: Option<WidgetId>,
    measure_value_width: bool,
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
    search: WidgetId,
    categorized: WidgetId,
    sorted: WidgetId,
    flat_host: WidgetId,
    viewport: Option<ScrollViewer>,
    empty: WidgetId,
    filter: String,
    flat: bool,
    saved_expansion: Vec<(WidgetId, bool)>,
    grouped_order: Vec<(WidgetId, WidgetId)>,
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
            let seed = if descriptor.choices.is_empty() {
                neutral_value(&recipe, &field, descriptor.default.as_ref())?
            } else {
                if field.kind != rust_desktop_ui_data::FieldType::Text
                    || !matches!(recipe, DataFormEditorFactory::Text(_))
                    || !matches!(descriptor.metadata.editor.as_str(), "Auto" | "Text")
                {
                    return Err(invalid(
                        "Choices requires an Auto or Text editor on a text field",
                    ));
                }
                for value in baseline.iter().chain(descriptor.default.as_ref()) {
                    validate_choice(&descriptor.choices, value)?;
                }
                descriptor.default.clone().unwrap_or_else(|| {
                    if field.nullable {
                        CellValue::Null
                    } else {
                        CellValue::Text(descriptor.choices[0].clone())
                    }
                })
            };
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
        if settings.scrollable {
            style.kind = LayoutKind::Grid;
            style.grid_columns = vec![GridTrack::Fr(1.)];
            style.grid_rows = vec![GridTrack::Auto, GridTrack::Fr(1.), GridTrack::Auto];
        } else {
            style.kind = LayoutKind::Stack(Axis::Vertical);
        }
        style.gap = 0.0;
        let container = ui.transparent_panel(parent, style)?;
        ui.set_role(container, Role::Group);
        ui.set_automation_name(container, "Свойства объектов")?;
        let result = (|| {
            let (search, categorized, sorted) = presentation::toolbar(ui, container)?;
            let viewport = if settings.scrollable {
                let scroll = ui.scroll_view(container, LayoutStyle::default(), 0.)?;
                ui.set_surface(scroll.viewport, SurfaceKind::Transparent)?;
                ui.controls
                    .get_mut(&scroll.viewport)
                    .unwrap()
                    .inspector_part = Some(InspectorPart::Viewport);
                ui.set_surface(scroll.content, SurfaceKind::Transparent)?;
                ui.edit_props(scroll.content, |n| {
                    n.clip = false;
                    n.style = vertical_stack();
                    n.style.padding = Edges {
                        left: 1.,
                        top: 1.,
                        right: 10.,
                        bottom: 1.,
                    };
                })?;
                Some(scroll)
            } else {
                None
            };
            let body = viewport.as_ref().map_or(container, |scroll| scroll.content);
            let flat_host = ui.transparent_panel(body, vertical_stack())?;
            show(ui, flat_host, false)?;
            let mut groups: Vec<(Vec<String>, Expander)> = Vec::new();
            let mut properties = Vec::new();
            for (descriptor, schema, recipe, baseline, seed) in definitions {
                let path: Vec<_> = std::iter::once(descriptor.category.clone())
                    .chain(descriptor.groups.clone())
                    .collect();
                let mut host = body;
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
                                padding: Edges::default(),
                                ..Default::default()
                            },
                            true,
                            LayoutStyle {
                                width: Length::Percent(1.0),
                                ..Default::default()
                            },
                        )?;
                        ui.controls.get_mut(&group.header).unwrap().inspector_part =
                            Some(InspectorPart::Category {
                                owner: group.container,
                                depth: depth - 1,
                            });
                        ui.edit_props(group.header, |p| p.style.height = Length::Px(28.0))?;
                        ui.edit_props(group.content, |p| {
                            p.style.kind = LayoutKind::Stack(Axis::Vertical);
                            p.style.gap = 0.0;
                        })?;
                        host = group.content;
                        groups.push((prefix, group));
                    }
                }
                let split = if descriptor.full_width {
                    0.
                } else {
                    settings.label_width
                };
                let row = ui.transparent_panel(
                    host,
                    LayoutStyle {
                        kind: LayoutKind::Grid,
                        grid_columns: vec![
                            GridTrack::Px(split),
                            GridTrack::Fr(1.0),
                            GridTrack::Auto,
                            GridTrack::Auto,
                        ],
                        min_size: Size::new(0., 32.),
                        padding: Edges {
                            bottom: 1.,
                            ..Default::default()
                        },
                        width: Length::Percent(1.0),
                        ..Default::default()
                    },
                )?;
                ui.controls.get_mut(&row).unwrap().inspector_part = Some(InspectorPart::Row {
                    split,
                    alternate: properties.len() % 2 != 0,
                });
                let label = descriptor
                    .metadata
                    .label
                    .as_deref()
                    .unwrap_or(&schema.title);
                let name_label = ui.label(
                    row,
                    label,
                    LayoutStyle {
                        height: Length::Px(20.),
                        margin: Edges {
                            left: 16.,
                            ..Default::default()
                        },
                        vertical_alignment: Some(Align::Center),
                        ..Default::default()
                    },
                )?;
                if descriptor.full_width {
                    show(ui, name_label, false)?;
                }
                let host = ui.transparent_panel(
                    row,
                    LayoutStyle {
                        // Stack deliberately ignores child flex_grow. The
                        // editor and mixed-value caption share this remaining
                        // Grid column, while the NULL toggle keeps its width.
                        grid_column: rust_desktop_ui_core::GridPlacement {
                            start: Some(2),
                            span: 1,
                        },
                        kind: LayoutKind::Flex(Axis::Horizontal),
                        margin: Edges {
                            left: 1.,
                            ..Default::default()
                        },
                        gap: 2.0,
                        ..Default::default()
                    },
                )?;
                let state = aggregate(&baseline);
                let value = match &state {
                    PropertyValue::Uniform(v) => v,
                    PropertyValue::Mixed => &seed,
                };
                let (owner, editor) = if descriptor.choices.is_empty() {
                    create_editor(ui, host, label, &schema, &recipe, value)?
                } else {
                    let choices = descriptor
                        .choices
                        .iter()
                        .map(|v| SelectionItem::new(v, v))
                        .collect();
                    let parts = ui.combo_box(
                        host,
                        label,
                        choices,
                        ComboBoxSettings {
                            editable: false,
                            read_only: descriptor.metadata.read_only,
                        },
                        LayoutStyle {
                            height: Length::Px(32.),
                            ..Default::default()
                        },
                    )?;
                    if let CellValue::Text(v) = value {
                        ui.set_selected_keys(parts.container, std::slice::from_ref(v))?;
                    }
                    (parts.container, parts.editor)
                };
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
                let edit = ui.button(
                    host,
                    "",
                    None,
                    LayoutStyle {
                        flex_grow: 1.,
                        ..Default::default()
                    },
                )?;
                ui.controls.get_mut(&edit).unwrap().inspector_part = Some(InspectorPart::Value);
                ui.set_button_chrome(edit, ButtonChrome::Flat)?;
                let apply = action_icon(
                    ui,
                    row,
                    IconSymbol::Check,
                    &format!("{label}: применить ко всем"),
                )?;
                let reset = action_icon(
                    ui,
                    row,
                    IconSymbol::Eraser,
                    &format!("{label}: значение по умолчанию"),
                )?;
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
                    active: false,
                    value_label: None,
                    measure_value_width: false,
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
            let empty = ui.label(body, "Свойства не найдены", LayoutStyle::default())?;
            show(
                ui,
                empty,
                !properties.iter().any(|p| p.descriptor.metadata.visible),
            )?;
            let grouped_order = groups
                .iter()
                .flat_map(|(_, group)| {
                    ui.runtime
                        .tree()
                        .children(group.content)
                        .unwrap_or(&[])
                        .iter()
                        .map(move |&id| (id, group.content))
                })
                .collect();
            let summary = ui.label(container, "", LayoutStyle::default())?;
            ui.set_automation_name(summary, "Результат изменения свойств")?;
            ui.set_text_auto_height(summary, true)?;
            show(ui, summary, false)?;
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
                search,
                categorized,
                sorted,
                flat_host,
                viewport,
                empty,
                grouped_order,
                flat: false,
                filter: String::new(),
                saved_expansion: Vec::new(),
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
    /// Size the value button to its installed display template. An optional text
    /// slot receives the accepted value. Auto-sized slots in horizontal stacks
    /// are remeasured when that value changes.
    pub fn configure_display_template(
        &mut self,
        ui: &mut Controls,
        field: &str,
        slot: Option<(WidgetId, bool)>,
    ) -> Result<(), ControlError> {
        let i = self.index(field)?;
        let p = &mut self.properties[i];
        let root = ui
            .button_content(p.edit)
            .ok_or_else(|| invalid("Property has no display template"))?;
        if let Some((label, auto_width)) = slot {
            if ui.text_style(label).is_none() || !ui.flyout_descendant(label, root) {
                return Err(invalid(
                    "Value label must belong to this property's display template",
                ));
            }
            p.measure_value_width = auto_width
                && ui
                    .runtime
                    .tree()
                    .parent(label)
                    .and_then(|id| ui.runtime.tree().node(id))
                    .is_some_and(|parent| {
                        parent.props.style.kind == LayoutKind::Stack(Axis::Horizontal)
                    });
            if p.descriptor.metadata.read_only {
                ui.controls.get_mut(&label).unwrap().inspector_part =
                    Some(InspectorPart::ReadOnlyText { host: p.host });
            }
        }
        p.value_label = slot.map(|(id, _)| id);
        ui.edit_props(p.edit, |n| {
            n.style.kind = LayoutKind::Grid;
            n.style.grid_columns = vec![GridTrack::Fr(1.)];
            n.style.grid_rows = vec![GridTrack::Fr(1.)];
            n.style.height = Length::Auto;
            n.style.min_size.height = 32.;
        })?;
        present(ui, p)
    }
    /// Search box and the categorized/alphabetical view selectors in tab order.
    pub fn toolbar_parts(&self) -> (WidgetId, WidgetId, WidgetId) {
        (self.search, self.categorized, self.sorted)
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
        }
        self.activate(ui, i)?;
        let p = &self.properties[i];
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
        self.activate(ui, i)
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
        p.active = false;
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
        if p.descriptor.choices.is_empty()
            && ui.document(p.editor).is_some()
            && p.null.is_none_or(|id| ui.checked(id) != Some(true))
        {
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
        show(ui, self.summary, false)?;
        self.error = None;
        let result = self
            .source
            .try_borrow_mut()
            .map_err(|_| invalid("Property source is borrowed"))?
            .write_batch(&operations);
        if let Err(e) = result {
            return self.reject(ui, &e.to_string());
        }
        self.properties[i].active = false;
        if !changed {
            present(ui, &mut self.properties[i])?;
            return Ok(PropertyCommit::Unchanged);
        }
        self.properties[i].baseline = vec![value.clone(); self.objects.len()];
        present(ui, &mut self.properties[i])?;
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
        show(ui, self.summary, true)?;
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
            p.active &= editing;
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
        let mut view_mode = self.flat;
        let mut actions = Vec::new();
        ui.activated.retain(|id| {
            if *id == self.categorized || *id == self.sorted {
                view_mode = *id == self.sorted;
                return false;
            }
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
        let query = ui
            .document(self.search)
            .unwrap()
            .text()
            .trim()
            .to_lowercase();
        let changed = !actions.is_empty() || view_mode != self.flat || query != self.filter;
        self.arrange(ui, view_mode, query)?;
        if ui.checked(self.categorized) != Some(!view_mode) {
            ui.set_checked(self.categorized, !view_mode)?;
        }
        if ui.checked(self.sorted) != Some(view_mode) {
            ui.set_checked(self.sorted, view_mode)?;
        }
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
        for p in self
            .properties
            .iter_mut()
            .filter(|p| p.active && p.null.is_some())
        {
            present(ui, p)?;
        }
        self.update_scroll_extent(ui)?;
        Ok(changed)
    }
    /// Optional pre-dispatch Escape route: composition first, then this draft.
    pub fn input(&mut self, ui: &mut Controls, event: &InputEvent) -> Result<bool, ControlError> {
        let InputEvent::KeyDown {
            key,
            modifiers,
            repeat: false,
        } = event
        else {
            return Ok(false);
        };
        if !matches!(key, Key::Escape | Key::Enter)
            || *modifiers != Modifiers::default()
            || ui.popup_open()
        {
            return Ok(false);
        }
        let Some(id) = ui.runtime.focused() else {
            return Ok(false);
        };
        let Some(p) = self
            .properties
            .iter()
            .find(|p| p.active && ui.flyout_descendant(id, p.host))
        else {
            return Ok(false);
        };
        if *key == Key::Enter
            && (id != p.editor
                || ui.document(id).is_none()
                || ui.document(id).is_some_and(|d| d.composition().is_some()))
        {
            return Ok(false);
        }
        let name = p.schema.name.clone();
        let button = p.edit;
        if *key == Key::Escape {
            self.cancel(ui, &name)?;
        } else {
            self.commit(ui, &name)?;
        }
        if !self.properties[self.index(&name)?].active && ui.window_focused {
            match ui.runtime.apply(Mutation::Focus(Some(button))) {
                Ok(()) | Err(RuntimeError::IneligibleFocus(_)) => {}
                Err(error) => return Err(error.into()),
            }
        }
        Ok(true)
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
