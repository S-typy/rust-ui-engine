//! A retained metadata form; saving is one optimistic source transaction.
use super::data_form_model::decimal_cell;
use super::*;
use rust_desktop_ui_core::{Align, GridTrack};
use rust_desktop_ui_data::{CellValue, DataError, DataWrite, FieldSchema, TableSchema};

struct Field {
    metadata: DataFormField,
    schema: FieldSchema,
    recipe: DataFormEditorFactory,
    row: WidgetId,
    label: WidgetId,
    owner: WidgetId,
    editor: WidgetId,
    null: Option<WidgetId>,
    baseline: CellValue,
}

/// The application retains this object beside Controls and calls `update` after
/// input. Field controls remain mounted across arrangement and source refresh.
/// A successful local editor commit does not save the form; only `commit` writes
/// the source. Dropping this handle does not remove its controls.
pub struct DataForm {
    pub container: WidgetId,
    pub summary: WidgetId,
    source: DataSourceHandle,
    row_key: i64,
    schema: TableSchema,
    settings: DataFormSettings,
    fields: Vec<Field>,
    issues: Vec<DataFormIssue>,
}

fn invalid(message: impl Into<String>) -> ControlError {
    ControlError::Invalid(message.into())
}
fn data_error(error: DataError) -> ControlError {
    invalid(error.to_string())
}

impl DataForm {
    pub fn mount(
        ui: &mut Controls,
        parent: WidgetId,
        source: DataSourceHandle,
        row_key: i64,
        settings: DataFormSettings,
        factories: &DataFormFactories,
        mut style: LayoutStyle,
    ) -> Result<Self, ControlError> {
        settings.validate()?;
        let data = source
            .try_borrow()
            .map_err(|_| invalid("Data source is already borrowed"))?;
        let schema = data.table().schema().clone();
        let values = data
            .row(row_key)
            .ok_or_else(|| invalid("DataForm row does not exist"))?
            .values
            .clone();
        drop(data);
        let metadata = if settings.fields.is_empty() {
            schema
                .fields()
                .iter()
                .map(|f| DataFormField::new(&f.name))
                .collect::<Vec<_>>()
        } else {
            settings.fields.clone()
        };
        if metadata.is_empty() || metadata.len() > 128 {
            return Err(invalid("DataForm requires 1..128 fields"));
        }
        let mut definitions = Vec::new();
        for mut meta in metadata {
            let index = schema
                .field_index(&meta.field)
                .ok_or_else(|| invalid(format!("Unknown field '{}'", meta.field)))?;
            let field = schema.fields()[index].clone();
            meta.read_only |= settings.read_only || field.read_only;
            let recipe = factories.resolve(&meta.editor, &field)?;
            if let DataFormEditorFactory::Text(recipe) = &recipe {
                meta.read_only |= recipe.is_read_only;
            }
            let value = values[index].clone();
            field.validate(&value).map_err(data_error)?;
            let label = meta.label.as_deref().unwrap_or(&field.title);
            ui.measure_text(label, 4096.0, false)?;
            let text = checked_format(&recipe, &field, &value)?;
            ui.measure_text(&text, 4096.0, false)?;
            definitions.push((meta, field, recipe, value));
        }
        style.kind = LayoutKind::Stack(Axis::Vertical);
        style.gap = settings.row_gap;
        let container = ui.transparent_panel(parent, style)?;
        let result = (|| {
            let mut fields = Vec::new();
            for (meta, schema, recipe, value) in definitions {
                let row = ui.transparent_panel(container, row_style(settings.label_width))?;
                let caption = meta.label.as_deref().unwrap_or(&schema.title);
                let label = ui.label(
                    row,
                    caption,
                    LayoutStyle {
                        vertical_alignment: Some(Align::Center),
                        ..Default::default()
                    },
                )?;
                let (owner, editor) = create_editor(ui, row, caption, &schema, &recipe, &value)?;
                ui.set_automation_name(editor, caption)?;
                let null = if schema.nullable && !matches!(recipe, DataFormEditorFactory::CheckBox)
                {
                    let id = ui.checkbox(
                        row,
                        "NULL",
                        value == CellValue::Null,
                        LayoutStyle {
                            width: Length::Px(76.0),
                            ..Default::default()
                        },
                    )?;
                    ui.set_automation_name(id, format!("{}: значение отсутствует", caption))?;
                    ui.set_enabled(id, !meta.read_only)?;
                    Some(id)
                } else {
                    None
                };
                if matches!(recipe, DataFormEditorFactory::CheckBox) {
                    ui.set_enabled(owner, !meta.read_only)?;
                } else if owner != editor {
                    ui.set_composite_read_only(owner, meta.read_only)?;
                } else {
                    ui.set_read_only(editor, meta.read_only)?;
                }
                ui.edit_props(row, |p| {
                    p.visible = meta.visible;
                    p.collapsed = !meta.visible;
                })?;
                fields.push(Field {
                    metadata: meta,
                    schema,
                    recipe,
                    row,
                    label,
                    owner,
                    editor,
                    null,
                    baseline: value,
                });
            }
            let summary = ui.label(
                container,
                "",
                LayoutStyle {
                    min_size: Size::new(0.0, 0.0),
                    ..Default::default()
                },
            )?;
            ui.set_automation_name(summary, "Ошибки формы")?;
            ui.set_text_auto_height(summary, true)?;
            let mut form = Self {
                container,
                summary,
                source,
                row_key,
                schema,
                settings,
                fields,
                issues: Vec::new(),
            };
            form.update(ui)?;
            Ok(form)
        })();
        if result.is_err() {
            let _ = ui.remove(container);
        }
        result
    }

    pub fn row_key(&self) -> i64 {
        self.row_key
    }
    pub fn field(&self, name: &str) -> Option<WidgetId> {
        self.fields
            .iter()
            .find(|f| f.schema.name == name)
            .map(|f| f.owner)
    }
    pub fn field_editor(&self, name: &str) -> Option<WidgetId> {
        self.fields
            .iter()
            .find(|f| f.schema.name == name)
            .map(|f| f.editor)
    }
    pub fn field_null_toggle(&self, name: &str) -> Option<WidgetId> {
        self.fields.iter().find(|f| f.schema.name == name)?.null
    }
    pub fn issues(&self) -> &[DataFormIssue] {
        &self.issues
    }
    pub fn arrangement(&self) -> Vec<DataFormArrangement> {
        self.fields
            .iter()
            .map(|f| DataFormArrangement {
                field: f.schema.name.clone(),
                visible: f.metadata.visible,
            })
            .collect()
    }
    pub fn value(&self, ui: &Controls, field: &str) -> Result<CellValue, ControlError> {
        self.check_parts(ui)?;
        let field = self
            .fields
            .iter()
            .find(|f| f.schema.name == field)
            .ok_or_else(|| invalid("Unknown form field"))?;
        read_value(ui, field).map_err(invalid)
    }
    pub fn is_dirty(&self, ui: &Controls) -> bool {
        self.check_parts(ui).is_err() || self.fields.iter().any(|field| field_dirty(ui, field))
    }
    pub fn update(&mut self, ui: &mut Controls) -> Result<(), ControlError> {
        self.check_parts(ui)?;
        for field in &self.fields {
            if let Some(null) = field.null {
                let enabled = ui.checked(null) != Some(true);
                if ui.controls[&field.owner].enabled != enabled {
                    ui.set_enabled(field.owner, enabled)?;
                }
            }
        }
        Ok(())
    }

    /// Change order and visibility in one tree transaction. The list must name
    /// every generated field exactly once. Controls, drafts and histories stay.
    pub fn arrange(
        &mut self,
        ui: &mut Controls,
        arrangement: &[DataFormArrangement],
        label_width: f32,
    ) -> Result<(), ControlError> {
        self.check_parts(ui)?;
        if !label_width.is_finite()
            || !(0.0..=4096.0).contains(&label_width)
            || arrangement.len() != self.fields.len()
        {
            return Err(invalid("Invalid DataForm arrangement"));
        }
        let mut seen = HashSet::new();
        let mut indices = Vec::new();
        let mut mutations = Vec::new();
        for item in arrangement {
            let index = self
                .fields
                .iter()
                .position(|f| f.schema.name == item.field)
                .ok_or_else(|| invalid("Arrangement names an unknown field"))?;
            if !seen.insert(index) {
                return Err(invalid("Duplicate arrangement field"));
            }
            indices.push(index);
            let row = self.fields[index].row;
            let mut props = ui.runtime.tree().node(row).unwrap().props.clone();
            props.visible = item.visible;
            props.collapsed = !item.visible;
            props.style.grid_columns[0] = GridTrack::Px(label_width);
            mutations.push(Mutation::SetProps { node: row, props });
            mutations.push(Mutation::Reparent {
                node: row,
                parent: self.container,
            });
        }
        mutations.push(Mutation::Reparent {
            node: self.summary,
            parent: self.container,
        });
        ui.runtime.apply(Mutation::Batch(mutations))?;
        let mut previous = std::mem::take(&mut self.fields)
            .into_iter()
            .map(Some)
            .collect::<Vec<_>>();
        self.fields = indices
            .into_iter()
            .zip(arrangement)
            .map(|(index, item)| {
                let mut field = previous[index].take().unwrap();
                field.metadata.visible = item.visible;
                field
            })
            .collect();
        self.settings.label_width = label_width;
        ui.scene_dirty = true;
        Ok(())
    }

    /// Load a local value, without saving it to the source. Null is explicit and
    /// remains distinct from empty Text and false Bool.
    pub fn set_field_value(
        &mut self,
        ui: &mut Controls,
        name: &str,
        value: CellValue,
    ) -> Result<(), ControlError> {
        self.check_parts(ui)?;
        let field = self
            .fields
            .iter()
            .find(|f| f.schema.name == name)
            .ok_or_else(|| invalid("Unknown form field"))?;
        load_value(ui, field, &value)?;
        self.update(ui)
    }
    pub fn set_field_null(&mut self, ui: &mut Controls, name: &str) -> Result<(), ControlError> {
        self.set_field_value(ui, name, CellValue::Null)
    }

    /// Parse and locally validate every field before one atomic expected-value
    /// batch. Local editor commits can precede a failed Save; the form baseline
    /// remains unchanged, and no field text is lost. Pending async validation
    /// requires a later explicit call. No UI callbacks run after source success.
    pub fn commit(&mut self, ui: &mut Controls) -> Result<DataFormCommit, ControlError> {
        self.check_parts(ui)?;
        self.check_schema()?;
        let mut values = Vec::new();
        let mut issues = Vec::new();
        let mut pending = false;
        for field in &self.fields {
            match read_value(ui, field) {
                Ok(value) => {
                    if field.metadata.read_only && value != field.baseline {
                        issues.push(issue(field, "Field is read-only"));
                    }
                    if active_editor(ui, field) {
                        if ui.document(field.editor).unwrap().composition().is_some() {
                            pending = true;
                        } else if let Err(message) = ui.validate_editor_value(
                            field.editor,
                            ui.document(field.editor).unwrap().text(),
                        ) {
                            issues.push(issue(field, &message));
                        }
                    }
                    values.push(value);
                }
                Err(message) => {
                    issues.push(issue(field, &message));
                    values.push(field.baseline.clone());
                }
            }
        }
        if !issues.is_empty() {
            self.set_issues(ui, issues.clone())?;
            return Ok(DataFormCommit::Invalid(issues));
        }
        if pending {
            return Ok(DataFormCommit::Pending);
        }
        for field in &self.fields {
            if active_editor(ui, field)
                && !field.metadata.read_only
                && ui.is_edit_dirty(field.editor) == Some(true)
            {
                match ui.commit_edit(field.editor)? {
                    CommitOutcome::Invalid(message) => issues.push(issue(field, &message)),
                    CommitOutcome::Pending | CommitOutcome::CompositionActive => pending = true,
                    _ => {}
                }
            }
        }
        if !issues.is_empty() {
            self.set_issues(ui, issues.clone())?;
            return Ok(DataFormCommit::Invalid(issues));
        }
        if pending {
            return Ok(DataFormCommit::Pending);
        }
        let operations: Vec<_> = self
            .fields
            .iter()
            .zip(&values)
            .filter(|(f, v)| f.baseline != **v)
            .map(|(field, value)| DataWrite::Update {
                key: self.row_key,
                field: field.schema.name.clone(),
                expected: field.baseline.clone(),
                value: value.clone(),
            })
            .collect();
        self.set_issues(ui, Vec::new())?;
        if operations.is_empty() {
            return Ok(DataFormCommit::Unchanged);
        }
        let result = self
            .source
            .try_borrow_mut()
            .map_err(|_| invalid("Data source is already borrowed"))?
            .write_batch(&operations);
        match result {
            Ok(_) => {
                for (field, value) in self.fields.iter_mut().zip(values) {
                    field.baseline = value;
                }
                Ok(DataFormCommit::Saved)
            }
            Err(DataError::Conflict(message)) => {
                self.set_issues(
                    ui,
                    vec![DataFormIssue {
                        field: String::new(),
                        message: message.clone(),
                    }],
                )?;
                Ok(DataFormCommit::Conflict(message))
            }
            Err(error) => {
                self.set_issues(
                    ui,
                    vec![DataFormIssue {
                        field: String::new(),
                        message: error.to_string(),
                    }],
                )?;
                Err(data_error(error))
            }
        }
    }

    /// Read the current accepted source snapshot. Dirty fields are preserved;
    /// changes to their baselines become explicit conflicts. This method does
    /// not perform adapter I/O; call DataSource::refresh/synchronize separately.
    pub fn refresh(&mut self, ui: &mut Controls) -> Result<(), ControlError> {
        self.check_parts(ui)?;
        self.check_schema()?;
        let data = self
            .source
            .try_borrow()
            .map_err(|_| invalid("Data source is already borrowed"))?;
        let row = data
            .row(self.row_key)
            .ok_or_else(|| invalid("DataForm row was removed"))?
            .clone();
        drop(data);
        let mut issues = Vec::new();
        for field in &mut self.fields {
            let value = &row.values[self.schema.field_index(&field.schema.name).unwrap()];
            let dirty = field_dirty(ui, field);
            if dirty {
                if *value != field.baseline {
                    issues.push(issue(
                        field,
                        "Source changed while this field has a local draft",
                    ));
                }
            } else if *value != field.baseline {
                match load_value(ui, field, value) {
                    Ok(()) => field.baseline = value.clone(),
                    Err(error) => issues.push(issue(field, &error.to_string())),
                }
            }
        }
        self.set_issues(ui, issues)?;
        self.update(ui)
    }
    /// Explicitly discard local edits and load the current source row.
    /// Loading is validated per field; a rejected field keeps its previous draft.
    pub fn revert(&mut self, ui: &mut Controls) -> Result<(), ControlError> {
        self.check_parts(ui)?;
        self.check_schema()?;
        let row = self
            .source
            .try_borrow()
            .map_err(|_| invalid("Data source is already borrowed"))?
            .row(self.row_key)
            .ok_or_else(|| invalid("DataForm row was removed"))?
            .clone();
        let mut issues = Vec::new();
        for field in &mut self.fields {
            let value = &row.values[self.schema.field_index(&field.schema.name).unwrap()];
            match load_value(ui, field, value) {
                Ok(()) => field.baseline = value.clone(),
                Err(error) => issues.push(issue(field, &error.to_string())),
            }
        }
        self.set_issues(ui, issues)?;
        self.update(ui)
    }
    fn check_schema(&self) -> Result<(), ControlError> {
        let source = self
            .source
            .try_borrow()
            .map_err(|_| invalid("Data source is already borrowed"))?;
        if source.table().schema() != &self.schema {
            return Err(invalid("DataForm schema changed; remount explicitly"));
        }
        if source.row(self.row_key).is_none() {
            return Err(invalid("DataForm row was removed"));
        }
        Ok(())
    }
    fn check_parts(&self, ui: &Controls) -> Result<(), ControlError> {
        if !ui.runtime.tree().contains(self.container)
            || ui.runtime.tree().parent(self.summary) != Some(self.container)
            || self.fields.iter().any(|f| {
                ui.runtime.tree().parent(f.row) != Some(self.container)
                    || ui.runtime.tree().parent(f.label) != Some(f.row)
                    || ui.runtime.tree().parent(f.owner) != Some(f.row)
                    || !ui.runtime.tree().contains(f.editor)
                    || f.null
                        .is_some_and(|id| ui.runtime.tree().parent(id) != Some(f.row))
            })
        {
            return Err(invalid(
                "DataForm required parts were removed or reparented",
            ));
        }
        for field in &self.fields {
            let changed = match &field.recipe {
                DataFormEditorFactory::Numeric(settings) => {
                    ui.numeric_settings(field.owner) != Some(settings)
                }
                DataFormEditorFactory::Date(settings) => {
                    ui.date_settings(field.owner) != Some(settings)
                }
                DataFormEditorFactory::Time(settings) => {
                    ui.time_settings(field.owner) != Some(settings)
                }
                _ => false,
            };
            if changed {
                return Err(invalid(
                    "DataForm typed editor settings changed outside its recipe; remount explicitly",
                ));
            }
        }
        Ok(())
    }
    fn set_issues(
        &mut self,
        ui: &mut Controls,
        issues: Vec<DataFormIssue>,
    ) -> Result<(), ControlError> {
        let text = issues
            .iter()
            .take(128)
            .map(|issue| {
                let message: String = issue.message.chars().take(256).collect();
                if issue.field.is_empty() {
                    message
                } else {
                    format!("{}: {message}", issue.field)
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        ui.set_label(self.summary, text)?;
        self.issues = issues;
        Ok(())
    }
}
fn issue(field: &Field, message: &str) -> DataFormIssue {
    DataFormIssue {
        field: field.schema.name.clone(),
        message: message.into(),
    }
}
fn active_editor(ui: &Controls, field: &Field) -> bool {
    ui.document(field.editor).is_some() && field.null.is_none_or(|id| ui.checked(id) != Some(true))
}
fn field_dirty(ui: &Controls, field: &Field) -> bool {
    read_value(ui, field).map_or(true, |value| value != field.baseline)
        || active_editor(ui, field)
            && (ui.is_edit_dirty(field.editor) == Some(true)
                || ui
                    .document(field.editor)
                    .is_some_and(|d| d.composition().is_some()))
}
fn row_style(label_width: f32) -> LayoutStyle {
    LayoutStyle {
        kind: LayoutKind::Grid,
        grid_columns: vec![
            GridTrack::Px(label_width),
            GridTrack::Fr(1.0),
            GridTrack::Auto,
        ],
        gap: 8.0,
        ..Default::default()
    }
}
fn check(value: &CellValue) -> CheckState {
    match value {
        CellValue::Bool(true) => CheckState::Checked,
        CellValue::Bool(false) => CheckState::Unchecked,
        _ => CheckState::Indeterminate,
    }
}
pub(super) fn checked_format(
    recipe: &DataFormEditorFactory,
    field: &FieldSchema,
    value: &CellValue,
) -> Result<String, ControlError> {
    let text = format_value(recipe, value)?;
    if let DataFormEditorFactory::Text(settings) = recipe {
        editor_limits(settings.max_length)
            .validate(&text)
            .map_err(edit_error)?;
        if normalize_editor_text(&text, false)
            .map_err(edit_error)?
            .as_ref()
            != text
        {
            return Err(invalid(
                "Editor normalization would change the source value",
            ));
        }
    }
    if !matches!(recipe, DataFormEditorFactory::CheckBox)
        && *value != CellValue::Null
        && parse_value(recipe, field, &text).map_err(invalid)? != *value
    {
        return Err(invalid(
            "Editor recipe cannot represent this source value without loss",
        ));
    }
    Ok(text)
}
fn format_value(recipe: &DataFormEditorFactory, value: &CellValue) -> Result<String, ControlError> {
    match recipe {
        DataFormEditorFactory::Numeric(settings) => settings.format(match value {
            CellValue::Null => None,
            CellValue::Integer(value) => Some(DecimalValue::new(*value as i128, 0).unwrap()),
            CellValue::Decimal(value) => Some(*value),
            _ => return Err(invalid("Numeric factory requires Integer or Decimal")),
        }),
        DataFormEditorFactory::Date(settings) => match value {
            CellValue::Null => {
                settings.check(None).map_err(invalid)?;
                Ok(String::new())
            }
            CellValue::Date(value) => {
                let value = DateValue::parse_iso(&value.to_string())?;
                settings.check(Some(value)).map_err(invalid)?;
                Ok(value.to_display(settings.format))
            }
            _ => Err(invalid("Date factory requires Date")),
        },
        DataFormEditorFactory::Time(settings) => match value {
            CellValue::Null => {
                settings.parse("").map_err(invalid)?;
                Ok(String::new())
            }
            CellValue::Time(value) => {
                let value = TimeValue::parse(&value.to_string())?;
                let text = value.to_display(settings.show_seconds);
                settings.parse(&text).map_err(invalid)?;
                Ok(text)
            }
            _ => Err(invalid("Time factory requires Time")),
        },
        _ => Ok(value.display()),
    }
}
pub(super) fn create_editor(
    ui: &mut Controls,
    parent: WidgetId,
    label: &str,
    field: &FieldSchema,
    recipe: &DataFormEditorFactory,
    value: &CellValue,
) -> Result<(WidgetId, WidgetId), ControlError> {
    let text = checked_format(recipe, field, value)?;
    let style = LayoutStyle {
        min_size: Size::new(48.0, 0.0),
        ..Default::default()
    };
    let parts = match recipe {
        DataFormEditorFactory::Text(settings) => {
            let editor = ui.text_box(parent, label, &text, style)?;
            let mut settings = settings.clone();
            settings.commit_on_focus_lost = false;
            ui.set_editor_settings(editor, settings)?;
            return Ok((editor, editor));
        }
        DataFormEditorFactory::CheckBox => {
            let editor = if field.nullable {
                ui.checkbox_tristate(parent, label, check(value), style)?
            } else {
                ui.checkbox(parent, label, *value == CellValue::Bool(true), style)?
            };
            return Ok((editor, editor));
        }
        DataFormEditorFactory::Numeric(settings) => ui.numeric_edit(
            parent,
            label,
            settings.parse(&text).map_err(invalid)?,
            settings.clone(),
            style,
        )?,
        DataFormEditorFactory::Date(settings) => ui.date_edit(
            parent,
            label,
            settings.parse(&text).map_err(invalid)?,
            settings.clone(),
            style,
        )?,
        DataFormEditorFactory::Time(settings) => ui.time_edit(
            parent,
            label,
            settings.parse(&text).map_err(invalid)?,
            settings.clone(),
            style,
        )?,
    };
    let mut settings = ui.editor_settings(parts.editor).unwrap().clone();
    settings.commit_on_focus_lost = false;
    ui.set_editor_settings(parts.editor, settings)?;
    Ok((parts.container, parts.editor))
}
fn read_value(ui: &Controls, field: &Field) -> Result<CellValue, String> {
    if field.null.is_some_and(|id| ui.checked(id) == Some(true)) {
        return Ok(CellValue::Null);
    }
    let value = if matches!(field.recipe, DataFormEditorFactory::CheckBox) {
        match ui
            .check_state(field.owner)
            .ok_or("Form checkbox was removed")?
        {
            CheckState::Checked => CellValue::Bool(true),
            CheckState::Unchecked => CellValue::Bool(false),
            CheckState::Indeterminate => CellValue::Null,
        }
    } else {
        let text = ui
            .document(field.editor)
            .ok_or("Form editor was removed")?
            .text();
        parse_value(&field.recipe, &field.schema, text)?
    };
    field.schema.validate(&value).map_err(|e| e.to_string())?;
    Ok(value)
}
pub(super) fn parse_value(
    recipe: &DataFormEditorFactory,
    field: &FieldSchema,
    text: &str,
) -> Result<CellValue, String> {
    Ok(match recipe {
        DataFormEditorFactory::Numeric(settings) => decimal_cell(field, settings.parse(text)?)?,
        DataFormEditorFactory::Date(settings) => match settings.parse(text)? {
            None => CellValue::Null,
            Some(v) => CellValue::Date(v.to_iso().parse().map_err(|e: DataError| e.to_string())?),
        },
        DataFormEditorFactory::Time(settings) => match settings.parse(text)? {
            None => CellValue::Null,
            Some(v) => CellValue::Time(v.to_iso().parse().map_err(|e: DataError| e.to_string())?),
        },
        _ => field.parse(text).map_err(|e| e.to_string())?,
    })
}
fn load_value(ui: &mut Controls, field: &Field, value: &CellValue) -> Result<(), ControlError> {
    field.schema.validate(value).map_err(data_error)?;
    if matches!(field.recipe, DataFormEditorFactory::CheckBox) {
        ui.set_check_state(field.owner, check(value))?;
    } else {
        let text = checked_format(&field.recipe, &field.schema, value)?;
        match ui.set_editor_value(field.editor, &text)? {
            CommitOutcome::Committed | CommitOutcome::Unchanged => {}
            CommitOutcome::Invalid(message) => return Err(invalid(message)),
            _ => return Err(invalid("Form value could not be loaded")),
        }
    }
    if let Some(null) = field.null {
        ui.set_checked(null, *value == CellValue::Null)?;
    }
    Ok(())
}
