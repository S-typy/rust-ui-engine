//! One retained typed edit session; storage writes remain optimistic and atomic.
use super::*;
use rust_desktop_ui_data::FieldSchema;

/// Application-owned in-place editor. The grid owns commit, validation and optimistic writes.
pub trait GridCellEditor {
    /// Validate a typed draft before the optimistic storage write.
    fn validate(&self, _row: i64, _field: &FieldSchema, _value: &CellValue) -> Result<(), String> {
        Ok(())
    }
    /// Called after a successful write or an explicit cancellation. Rejected drafts remain open.
    fn closed(&self, _row: i64, _field: &FieldSchema, _value: &CellValue, _committed: bool) {}
    fn choice(&self) -> Option<&GridChoiceEditor> {
        None
    }

    /// Create the owner directly under parent and return (owner, focusable editor).
    fn open(
        &self,
        ui: &mut Controls,
        parent: WidgetId,
        row: i64,
        field: &FieldSchema,
        value: &CellValue,
    ) -> Result<(WidgetId, WidgetId), ControlError>;
    fn value(
        &self,
        ui: &Controls,
        owner: WidgetId,
        editor: WidgetId,
        field: &FieldSchema,
        _original: &CellValue,
    ) -> Result<CellValue, String>;
    fn assign(
        &self,
        ui: &mut Controls,
        owner: WidgetId,
        editor: WidgetId,
        value: &CellValue,
    ) -> Result<(), ControlError>;
}

/// Additional editors for string fields. All values pass through the grid's cell transaction.
#[derive(Clone)]
pub enum GridTextCellEditor {
    Masked(MaskSettings),
    Multiline,
    Buttons(Vec<EditorButton>),
}
impl GridCellEditor for GridTextCellEditor {
    fn open(
        &self,
        ui: &mut Controls,
        parent: WidgetId,
        _row: i64,
        field: &FieldSchema,
        value: &CellValue,
    ) -> Result<(WidgetId, WidgetId), ControlError> {
        if field.kind != rust_desktop_ui_data::FieldType::Text {
            return Err(error("A text cell editor requires a text field"));
        }
        let text = if let CellValue::Text(s) = value {
            s.as_str()
        } else {
            ""
        };
        match self {
            Self::Masked(settings) => {
                let id = ui.masked_edit(
                    parent,
                    &field.title,
                    text,
                    settings.clone(),
                    LayoutStyle::default(),
                )?;
                Ok((id, id))
            }
            Self::Multiline => {
                let id = ui.text_box(parent, &field.title, text, LayoutStyle::default())?;
                Ok((id, id))
            }
            Self::Buttons(buttons) => {
                let parts = ui.button_edit(
                    parent,
                    &field.title,
                    text,
                    buttons.clone(),
                    LayoutStyle::default(),
                )?;
                Ok((parts.container, parts.editor))
            }
        }
    }
    fn value(
        &self,
        ui: &Controls,
        _owner: WidgetId,
        editor: WidgetId,
        _field: &FieldSchema,
        original: &CellValue,
    ) -> Result<CellValue, String> {
        let text = ui.document(editor).ok_or("Missing text editor")?.text();
        let value = match self {
            Self::Masked(settings) => {
                let value = settings.parse_display(text)?;
                if !value.complete {
                    return Err("Incomplete masked value".into());
                }
                value.storage
            }
            _ => text.to_owned(),
        };
        if value.is_empty() && *original == CellValue::Null {
            Ok(CellValue::Null)
        } else {
            Ok(CellValue::Text(value))
        }
    }
    fn assign(
        &self,
        ui: &mut Controls,
        _owner: WidgetId,
        editor: WidgetId,
        value: &CellValue,
    ) -> Result<(), ControlError> {
        let raw = match value {
            CellValue::Text(s) => s.as_str(),
            CellValue::Null => "",
            _ => return Err(error("Expected a text value")),
        };
        let display = match self {
            Self::Masked(settings) => settings.format(raw)?,
            _ => raw.to_owned(),
        };
        ui.set_text(editor, &display)
    }
}
pub(super) type ConditionalEditor = (String, CellValue, Rc<dyn GridCellEditor>);

/// Lookup rows expose a typed key field and one or more display columns.
#[derive(Clone)]
pub struct GridChoiceEditor {
    pub source: DataSourceHandle,
    pub value_field: String,
    pub display_fields: Vec<String>,
    pub searchable: bool,
}
#[derive(Clone)]
pub(super) struct ChoiceLabels {
    pub editor: GridChoiceEditor,
    revision: Option<u64>,
    labels: Rc<HashMap<String, String>>,
    duplicate_captions: Rc<HashSet<String>>,
}
impl SnapshotSource {
    pub(super) fn reload_choices(&mut self) -> Result<(), ControlError> {
        for (field, cache) in &mut self.choice_labels {
            let revision = cache
                .editor
                .source
                .try_borrow()
                .map_err(|_| error("Lookup source is busy"))?
                .revision();
            if cache.revision != Some(revision) {
                let schema = self
                    .table
                    .schema()
                    .field(field)
                    .ok_or_else(|| error("Missing lookup field"))?;
                let (items, _) = cache.editor.members(schema)?;
                let mut seen = HashSet::new();
                cache.duplicate_captions = Rc::new(
                    items
                        .iter()
                        .filter(|i| !seen.insert(i.text.as_str()))
                        .map(|i| i.text.clone())
                        .collect(),
                );
                cache.labels = Rc::new(items.into_iter().map(|i| (i.key, i.text)).collect());
                cache.revision = Some(revision);
            }
        }
        Ok(())
    }
    pub(super) fn choices_changed(&self) -> Result<bool, ControlError> {
        for cache in self.choice_labels.values() {
            if cache.revision
                != Some(
                    cache
                        .editor
                        .source
                        .try_borrow()
                        .map_err(|_| error("Lookup source is busy"))?
                        .revision(),
                )
            {
                return Ok(true);
            }
        }
        Ok(false)
    }
    pub(super) fn group_key_label(&self, field: &str, value: &CellValue) -> String {
        let caption = self.cell_label(field, value);
        if self
            .choice_labels
            .get(field)
            .is_some_and(|c| c.duplicate_captions.contains(&caption))
        {
            format!("{caption} [{}]", value.display())
        } else {
            caption
        }
    }
    pub(super) fn cell_label(&self, field: &str, value: &CellValue) -> String {
        let raw = value.display();
        self.choice_labels
            .get(field)
            .and_then(|c| c.labels.get(&projection::value_identity(value)))
            .cloned()
            .unwrap_or(raw)
    }
}
impl GridChoiceEditor {
    fn members(
        &self,
        field: &FieldSchema,
    ) -> Result<(Vec<SelectionItem>, Vec<String>), ControlError> {
        let source = self
            .source
            .try_borrow()
            .map_err(|_| error("Grid lookup source is busy"))?;
        let table = source.table();
        if self.display_fields.is_empty()
            || self.display_fields.len() > 32
            || table
                .schema()
                .field(&self.value_field)
                .is_none_or(|f| f.kind != field.kind)
        {
            return Err(error(
                "Lookup requires a matching key type and display fields",
            ));
        }
        let headers = self
            .display_fields
            .iter()
            .map(|f| {
                table
                    .schema()
                    .field(f)
                    .map(|f| f.title.clone())
                    .ok_or_else(|| error("Unknown lookup display field"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut items = Vec::new();
        let mut keys = HashSet::new();
        for i in 0..source.len() {
            let row = source.row_key(i).expect("visible row");
            let key = table
                .value(row, &self.value_field)
                .expect("validated field");
            if *key == CellValue::Null {
                return Err(error("Lookup keys cannot be NULL"));
            }
            let token = projection::value_identity(key);
            if !keys.insert(token.clone()) {
                return Err(error("Duplicate lookup key"));
            }
            let mut values = self
                .display_fields
                .iter()
                .map(|f| table.value(row, f).expect("validated field").display());
            let mut item = SelectionItem::new(token, values.next().expect("display field"));
            item.columns = values.collect();
            items.push(item);
        }
        Ok((items, headers))
    }
    pub fn validate(&self, field: &FieldSchema) -> Result<(), ControlError> {
        self.members(field).map(|_| ())
    }
}
impl GridCellEditor for GridChoiceEditor {
    fn choice(&self) -> Option<&GridChoiceEditor> {
        Some(self)
    }
    fn open(
        &self,
        ui: &mut Controls,
        parent: WidgetId,
        _row: i64,
        field: &FieldSchema,
        value: &CellValue,
    ) -> Result<(WidgetId, WidgetId), ControlError> {
        let (items, columns) = self.members(field)?;
        let parts = if self.searchable {
            ui.lookup_box(parent, &field.title, items, columns, LayoutStyle::default())?
        } else {
            ui.combo_box(
                parent,
                &field.title,
                items,
                ComboBoxSettings::default(),
                LayoutStyle::default(),
            )?
        };
        self.assign(ui, parts.container, parts.editor, value)?;
        Ok((parts.container, parts.editor))
    }
    fn value(
        &self,
        ui: &Controls,
        owner: WidgetId,
        _editor: WidgetId,
        field: &FieldSchema,
        _original: &CellValue,
    ) -> Result<CellValue, String> {
        let keys = ui.selected_keys(owner).ok_or("Missing lookup editor")?;
        let Some(token) = keys.first() else {
            return Ok(CellValue::Null);
        };
        let source = self.source.try_borrow().map_err(|_| "Lookup source busy")?;
        let value = (0..source.len())
            .filter_map(|i| source.row_key(i))
            .filter_map(|k| source.table().value(k, &self.value_field))
            .find(|v| projection::value_identity(v) == *token)
            .ok_or("Selected lookup key no longer exists")?
            .clone();
        field.validate(&value).map_err(|e| e.to_string())?;
        Ok(value)
    }
    fn assign(
        &self,
        ui: &mut Controls,
        owner: WidgetId,
        _editor: WidgetId,
        value: &CellValue,
    ) -> Result<(), ControlError> {
        let keys = if *value == CellValue::Null {
            vec![]
        } else {
            vec![projection::value_identity(value)]
        };
        ui.set_selected_keys(owner, &keys)?;
        Ok(())
    }
}
#[derive(Clone)]
pub(super) enum GridRecipe {
    BuiltIn(DataFormEditorFactory),
    Custom(Rc<dyn GridCellEditor>),
}

/// A search applies only to the same draft, editor instance and pending request.
#[derive(Clone, Debug)]
pub struct GridLookupRequest {
    grid: WidgetId,
    editor: WidgetId,
    sequence: u64,
    pub query: String,
}
pub(super) struct TypedEditor {
    sequence: u64,
    pending_search: Option<u64>,
    pub host: WidgetId,
    pub owner: WidgetId,
    pub editor: WidgetId,
    pub recipe: GridRecipe,
    pub schema: FieldSchema,
}

impl Controls {
    /// The application may perform a remote search and return matching typed keys on the UI thread.
    pub fn request_grid_lookup(&mut self, id: WidgetId) -> Result<GridLookupRequest, ControlError> {
        let g = self
            .grids
            .get(&id)
            .ok_or_else(|| error("Unknown GridControl"))?;
        let host = g
            .typed_editor
            .as_ref()
            .ok_or_else(|| error("No active lookup editor"))?;
        let is_lookup = match &host.recipe {
            GridRecipe::Custom(editor) => editor.choice().is_some_and(|choice| choice.searchable),
            GridRecipe::BuiltIn(_) => false,
        };
        if g.session.is_none() || !is_lookup {
            return Err(error("No active lookup editor"));
        }
        let editor = host.editor;
        let query = self.lookup_query(host.owner);
        let host = self
            .grids
            .get_mut(&id)
            .unwrap()
            .typed_editor
            .as_mut()
            .unwrap();
        host.sequence = host
            .sequence
            .checked_add(1)
            .ok_or_else(|| error("Lookup request id exhausted"))?;
        host.pending_search = Some(host.sequence);
        Ok(GridLookupRequest {
            grid: id,
            editor,
            sequence: host.sequence,
            query,
        })
    }
    pub fn complete_grid_lookup(
        &mut self,
        request: &GridLookupRequest,
        keys: &[CellValue],
    ) -> Result<bool, ControlError> {
        let Some(grid) = self.grids.get(&request.grid) else {
            return Ok(false);
        };
        let Some(host) = grid.typed_editor.as_ref().filter(|h| {
            grid.session.is_some()
                && h.editor == request.editor
                && h.pending_search == Some(request.sequence)
        }) else {
            return Ok(false);
        };
        if self.lookup_query(host.owner) != request.query {
            return Ok(false);
        }
        let GridRecipe::Custom(custom) = &host.recipe else {
            return Ok(false);
        };
        let Some(choice) = custom.choice() else {
            return Ok(false);
        };
        let owner = host.owner;
        let (items, _) = choice.members(&host.schema)?;
        if keys.len() > items.len() {
            return Err(error("Too many lookup results"));
        }
        let mut requested = HashSet::new();
        for key in keys {
            host.schema.validate(key).map_err(error)?;
            if *key == CellValue::Null || !requested.insert(projection::value_identity(key)) {
                return Err(error("Invalid or duplicate lookup result"));
            }
        }
        let selected = self.selected_keys(owner).unwrap_or_default();
        let mut matched = 0;
        let visible = items
            .into_iter()
            .filter(|item| {
                if requested.contains(&item.key) {
                    matched += 1;
                    true
                } else {
                    selected.contains(&item.key)
                }
            })
            .collect();
        if matched != requested.len() {
            return Err(error("Unknown lookup result"));
        }
        self.set_selection_items(owner, visible)?;
        self.grids
            .get_mut(&request.grid)
            .unwrap()
            .typed_editor
            .as_mut()
            .unwrap()
            .pending_search = None;
        Ok(true)
    }
    /// Validate a recipe for a schema field without creating retained controls.
    pub fn validate_grid_editor_factory(
        source: &DataSourceHandle,
        field: &str,
        name: &str,
        factories: &DataFormFactories,
    ) -> Result<(), ControlError> {
        let data = source
            .try_borrow()
            .map_err(|_| error("Grid source is already borrowed"))?;
        let field = data
            .table()
            .schema()
            .field(field)
            .ok_or_else(|| error("Unknown grid field"))?;
        factories.resolve(name, field)?;
        Ok(())
    }

    /// Replace the active draft with a typed value, without publishing it to
    /// the source. Ordinary commit/validation/conflict rules still apply.
    pub fn set_grid_edit_value(
        &mut self,
        id: WidgetId,
        value: CellValue,
    ) -> Result<(), ControlError> {
        let grid = self
            .grids
            .get(&id)
            .ok_or_else(|| error("Unknown GridControl"))?;
        let session = grid
            .session
            .as_ref()
            .ok_or_else(|| error("No active grid cell edit"))?;
        if grid.settings.is_read_only {
            return Err(error("Grid cell is read-only"));
        }
        let field = grid
            .source
            .table
            .schema()
            .field(&session.field)
            .ok_or_else(|| error("Unknown field"))?;
        field.validate(&value).map_err(error)?;
        let editor = grid.editor;
        if let Some(host) = grid.typed_editor.as_ref().filter(|h| h.editor == editor) {
            self.check_grid_typed_parts(id).map_err(error)?;
            let recipe = match &host.recipe {
                GridRecipe::BuiltIn(recipe) => recipe,
                GridRecipe::Custom(custom) => {
                    return custom.clone().assign(self, host.owner, editor, &value);
                }
            };
            if matches!(recipe, DataFormEditorFactory::CheckBox) {
                let state = match value {
                    CellValue::Null => CheckState::Indeterminate,
                    CellValue::Bool(true) => CheckState::Checked,
                    CellValue::Bool(false) => CheckState::Unchecked,
                    _ => return Err(error("Checkbox requires nullable Boolean")),
                };
                return self.set_check_state(editor, state);
            }
            let text = super::super::data_form::checked_format(recipe, field, &value)?;
            self.set_text(editor, &text)
        } else {
            self.set_text(editor, &value.display())
        }
    }

    /// Select a bounded existing editor recipe. A custom name is resolved from
    /// the supplied registry; replacing recipes never discards an active draft.
    pub fn set_grid_editor_factory(
        &mut self,
        id: WidgetId,
        field: &str,
        name: &str,
        factories: &DataFormFactories,
    ) -> Result<(), ControlError> {
        let grid = self
            .grids
            .get(&id)
            .ok_or_else(|| error("Unknown GridControl"))?;
        if grid.session.is_some() {
            return Err(error(
                "Finish the active cell edit before changing editor recipes",
            ));
        }
        let index = grid
            .settings
            .columns
            .iter()
            .position(|c| c.field == field)
            .ok_or_else(|| error("Unknown grid field"))?;
        let schema = grid
            .source
            .table
            .schema()
            .field(field)
            .ok_or_else(|| error("Unknown grid schema field"))?;
        let recipe = factories.resolve(name, schema)?;
        self.install_grid_recipe(id, index, GridRecipe::BuiltIn(recipe))
    }
    pub fn set_grid_cell_editor(
        &mut self,
        id: WidgetId,
        field: &str,
        editor: Rc<dyn GridCellEditor>,
    ) -> Result<(), ControlError> {
        let grid = self
            .grids
            .get(&id)
            .ok_or_else(|| error("Unknown GridControl"))?;
        if grid.session.is_some() {
            return Err(error(
                "Finish the active cell edit before changing editor recipes",
            ));
        }
        let index = grid
            .settings
            .columns
            .iter()
            .position(|c| c.field == field && !c.read_only)
            .ok_or_else(|| error("Unknown or read-only grid field"))?;
        self.install_grid_recipe(id, index, GridRecipe::Custom(editor))
    }
    /// Override Auto for rows whose typed field equals the supplied value.
    pub fn set_grid_conditional_cell_editor(
        &mut self,
        id: WidgetId,
        field: &str,
        when: &str,
        equals: CellValue,
        editor: Rc<dyn GridCellEditor>,
    ) -> Result<(), ControlError> {
        let grid = self
            .grids
            .get(&id)
            .ok_or_else(|| error("Unknown GridControl"))?;
        let index = grid
            .settings
            .columns
            .iter()
            .position(|c| c.field == field && !c.read_only)
            .ok_or_else(|| error("Unknown or read-only grid field"))?;
        grid.source
            .table
            .schema()
            .field(when)
            .ok_or_else(|| error("Unknown row condition field"))?
            .validate(&equals)
            .map_err(error)?;
        if grid.session.is_some() {
            return Err(error(
                "Finish the active edit before changing a conditional editor",
            ));
        }
        if !grid.editor_recipes.contains_key(&ColumnId(index as u64)) {
            self.set_grid_editor_factory(id, field, "Auto", &DataFormFactories::default())?;
        }
        self.grids
            .get_mut(&id)
            .unwrap()
            .conditional_editors
            .insert(ColumnId(index as u64), (when.into(), equals, editor));
        Ok(())
    }
    fn install_grid_recipe(
        &mut self,
        id: WidgetId,
        index: usize,
        recipe: GridRecipe,
    ) -> Result<(), ControlError> {
        let grid = &self.grids[&id];
        let field = &grid.settings.columns[index].field;
        let mut source = grid.source.clone();
        if let GridRecipe::Custom(editor) = &recipe
            && let Some(choice) = editor.choice()
        {
            source.choice_labels.insert(
                field.clone(),
                ChoiceLabels {
                    editor: choice.clone(),
                    revision: None,
                    labels: Rc::default(),
                    duplicate_captions: Rc::default(),
                },
            );
        } else {
            source.choice_labels.remove(field);
        }
        let previous = self
            .grids
            .get_mut(&id)
            .unwrap()
            .editor_recipes
            .insert(ColumnId(index as u64), recipe);
        let previous_height = self.grids[&id].editor_height;
        self.grids.get_mut(&id).unwrap().editor_height = self.theme.tokens().control_height + 4.0;

        let arrangement = self.grid_arrangement(id).unwrap();
        if let Err(err) = self.install_grid_presentation(id, source, arrangement) {
            let grid = self.grids.get_mut(&id).unwrap();
            grid.editor_height = previous_height;
            if let Some(recipe) = previous {
                grid.editor_recipes.insert(ColumnId(index as u64), recipe);
            } else {
                grid.editor_recipes.remove(&ColumnId(index as u64));
            }
            return Err(err);
        }

        Ok(())
    }
    pub(super) fn begin_typed_grid_edit(
        &mut self,
        id: WidgetId,
        key: i64,
        index: usize,
        expected: CellValue,
    ) -> Result<(), ControlError> {
        let grid = &self.grids[&id];
        let field = grid.settings.columns[index].field.clone();
        let schema = grid
            .source
            .table
            .schema()
            .field(&field)
            .ok_or_else(|| error("Unknown field"))?
            .clone();
        let column = ColumnId(index as u64);
        let recipe = grid
            .conditional_editors
            .get(&column)
            .filter(|(name, v, _)| grid.source.table.value(key, name) == Some(v))
            .map_or_else(
                || grid.editor_recipes[&column].clone(),
                |(_, _, editor)| GridRecipe::Custom(editor.clone()),
            );
        grid.validate_selection(Some(key))?;
        if let GridRecipe::BuiltIn(recipe) = &recipe {
            let text = super::super::data_form::checked_format(recipe, &schema, &expected)?;
            self.measure_text(&text, 4096.0, false)?;
        }
        let host = self.transparent_panel(
            id,
            LayoutStyle {
                kind: LayoutKind::Overlay,
                ..Default::default()
            },
        )?;
        let created = match &recipe {
            GridRecipe::BuiltIn(recipe) => super::super::data_form::create_editor(
                self,
                host,
                &schema.title,
                &schema,
                recipe,
                &expected,
            ),
            GridRecipe::Custom(custom) => custom.open(self, host, key, &schema, &expected),
        };
        let (owner, editor) = match created {
            Ok(parts) => parts,
            Err(e) => {
                self.remove(host)?;
                return Err(e);
            }
        };
        if self.runtime.tree().parent(owner) != Some(host)
            || !std::iter::successors(Some(editor), |id| self.runtime.tree().parent(*id))
                .any(|id| id == owner)
            || !self.controls.contains_key(&editor)
        {
            self.remove(host)?;
            return Err(error("Grid editor must return widgets inside its host"));
        }
        self.edit_props(owner, |p| {
            p.style.width = Length::Percent(1.0);
            p.style.height = Length::Percent(1.0);
        })?;
        if let Some(old) = self.grids.get_mut(&id).unwrap().typed_editor.take() {
            self.remove(old.host)?;
        }
        let grid = self.grids.get_mut(&id).unwrap();
        grid.model
            .focus_cell(
                &grid.source,
                row_key(key),
                ColumnId(index as u64),
                SelectionModifiers::default(),
            )
            .map_err(error)?;
        grid.model
            .begin_edit(&grid.source, row_key(key), ColumnId(index as u64))
            .map_err(error)?;
        grid.set_current(Some(key));
        grid.editor = editor;
        grid.error = None;
        grid.typed_editor = Some(TypedEditor {
            sequence: 0,
            pending_search: None,
            host,
            owner,
            editor,
            recipe,
            schema,
        });
        grid.records.emit(GridRecordEvent::Begin(key));
        grid.session = Some(CellSession {
            row: key,
            field,
            expected,
        });
        self.sync_grids()?;
        self.runtime.apply(Mutation::Focus(Some(editor)))?;
        self.scene_dirty = true;
        Ok(())
    }
    pub(super) fn grid_typed_value(&self, id: WidgetId) -> Result<CellValue, String> {
        let grid = &self.grids[&id];
        let host = grid.typed_editor.as_ref().ok_or("Missing typed editor")?;
        self.check_grid_typed_parts(id)?;
        let recipe = match &host.recipe {
            GridRecipe::Custom(custom) => {
                let session = grid.session.as_ref().ok_or("Missing cell session")?;
                let value = custom.value(
                    self,
                    host.owner,
                    host.editor,
                    &host.schema,
                    &session.expected,
                )?;
                custom.validate(session.row, &host.schema, &value)?;
                host.schema.validate(&value).map_err(|e| e.to_string())?;
                return Ok(value);
            }
            GridRecipe::BuiltIn(recipe) => recipe,
        };
        let value = if matches!(recipe, DataFormEditorFactory::CheckBox) {
            match self
                .check_state(host.editor)
                .ok_or("Grid checkbox was removed")?
            {
                CheckState::Checked => CellValue::Bool(true),
                CheckState::Unchecked => CellValue::Bool(false),
                CheckState::Indeterminate => CellValue::Null,
            }
        } else {
            let text = self
                .document(host.editor)
                .ok_or("Grid editor was removed")?
                .text();
            let baseline = &grid
                .session
                .as_ref()
                .ok_or("Missing cell session")?
                .expected;
            if text
                == super::super::data_form::checked_format(recipe, &host.schema, baseline)
                    .map_err(|e| e.to_string())?
            {
                baseline.clone()
            } else {
                super::super::data_form::parse_value(recipe, &host.schema, text)?
            }
        };
        host.schema.validate(&value).map_err(|e| e.to_string())?;
        Ok(value)
    }
    fn check_grid_typed_parts(&self, id: WidgetId) -> Result<(), String> {
        let grid = &self.grids[&id];
        let host = grid.typed_editor.as_ref().ok_or("Missing typed editor")?;
        if self.runtime.tree().parent(host.host) != Some(id)
            || self.runtime.tree().parent(host.owner) != Some(host.host)
            || !std::iter::successors(Some(host.editor), |id| self.runtime.tree().parent(*id))
                .any(|id| id == host.owner)
        {
            return Err("Grid typed editor parts were removed or reparented".into());
        }
        let GridRecipe::BuiltIn(recipe) = &host.recipe else {
            return Ok(());
        };
        let changed = match recipe {
            DataFormEditorFactory::Numeric(settings) => {
                self.numeric_settings(host.owner) != Some(settings)
            }
            DataFormEditorFactory::Date(settings) => {
                self.date_settings(host.owner) != Some(settings)
            }
            DataFormEditorFactory::Time(settings) => {
                self.time_settings(host.owner) != Some(settings)
            }
            DataFormEditorFactory::Text(settings) => self
                .editor_settings(host.editor)
                .is_none_or(|actual| actual.max_length != settings.max_length),
            DataFormEditorFactory::CheckBox => self.check_state(host.editor).is_none(),
        };
        if changed {
            return Err("Grid typed editor settings changed outside its recipe; cancel and reopen the editor".into());
        }
        Ok(())
    }
    pub(super) fn set_grid_editor_read_only(
        &mut self,
        id: WidgetId,
        value: bool,
    ) -> Result<(), ControlError> {
        let grid = &self.grids[&id];
        let editor = grid.editor;
        if let Some(h) = grid.typed_editor.as_ref().filter(|h| h.editor == editor) {
            let (owner, checkbox) = (
                h.owner,
                matches!(
                    h.recipe,
                    GridRecipe::BuiltIn(DataFormEditorFactory::CheckBox) | GridRecipe::Custom(_)
                ),
            );
            if checkbox {
                self.set_enabled(owner, !value)?;
            } else if self.rich_owner(editor).is_some() {
                self.set_composite_read_only(owner, value)?;
            } else {
                self.set_read_only(editor, value)?;
            }
        } else {
            self.set_read_only(editor, value)?;
        }
        Ok(())
    }
    pub(super) fn grid_edit_failure(&mut self, id: WidgetId, message: String) -> CommitOutcome {
        let editor = self.grids[&id].editor;
        self.grids.get_mut(&id).unwrap().error = Some(message.clone());
        self.controls.get_mut(&editor).unwrap().validation_error = Some(message.clone());
        self.edit_events.push(EditEvent::ValidationFailed {
            id: editor,
            message: message.clone(),
        });
        self.scene_dirty = true;
        CommitOutcome::Invalid(message)
    }
}
