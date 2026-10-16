//! One retained typed edit session; storage writes remain optimistic and atomic.
use super::*;
use rust_desktop_ui_data::FieldSchema;

pub(super) struct TypedEditor {
    pub host: WidgetId,
    pub owner: WidgetId,
    pub editor: WidgetId,
    pub recipe: DataFormEditorFactory,
    pub schema: FieldSchema,
}

impl Controls {
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
            if matches!(host.recipe, DataFormEditorFactory::CheckBox) {
                let state = match value {
                    CellValue::Null => CheckState::Indeterminate,
                    CellValue::Bool(true) => CheckState::Checked,
                    CellValue::Bool(false) => CheckState::Unchecked,
                    _ => return Err(error("Checkbox requires nullable Boolean")),
                };
                return self.set_check_state(editor, state);
            }
            let text = super::super::data_form::checked_format(&host.recipe, field, &value)?;
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
        let previous = self
            .grids
            .get_mut(&id)
            .unwrap()
            .editor_recipes
            .insert(ColumnId(index as u64), recipe);
        let previous_height = self.grids[&id].editor_height;
        self.grids.get_mut(&id).unwrap().editor_height = self.theme.tokens().control_height + 4.0;
        let source = self.grids[&id].source.clone();
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
        let recipe = grid.editor_recipes[&ColumnId(index as u64)].clone();
        let text = super::super::data_form::checked_format(&recipe, &schema, &expected)?;
        grid.validate_selection(Some(key))?;
        self.measure_text(&text, 4096.0, false)?;
        let host = self.transparent_panel(
            id,
            LayoutStyle {
                kind: LayoutKind::Overlay,
                ..Default::default()
            },
        )?;
        let created = super::super::data_form::create_editor(
            self,
            host,
            &schema.title,
            &schema,
            &recipe,
            &expected,
        );
        let (owner, editor) = match created {
            Ok(parts) => parts,
            Err(e) => {
                self.remove(host)?;
                return Err(e);
            }
        };
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
        grid.selected_key = Some(key);
        grid.editor = editor;
        grid.error = None;
        grid.typed_editor = Some(TypedEditor {
            host,
            owner,
            editor,
            recipe,
            schema,
        });
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
        let value = if matches!(host.recipe, DataFormEditorFactory::CheckBox) {
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
                == super::super::data_form::checked_format(&host.recipe, &host.schema, baseline)
                    .map_err(|e| e.to_string())?
            {
                baseline.clone()
            } else {
                super::super::data_form::parse_value(&host.recipe, &host.schema, text)?
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
            || !self.runtime.tree().contains(host.editor)
        {
            return Err("Grid typed editor parts were removed or reparented".into());
        }
        let changed = match &host.recipe {
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
            let (owner, checkbox) = (h.owner, matches!(h.recipe, DataFormEditorFactory::CheckBox));
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
