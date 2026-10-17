//! Atomic replacement of a read-only aggregate projection without remounting Grid.
use super::*;

impl Controls {
    pub(in crate::controls) fn check_pivot_grid(
        &self,
        id: WidgetId,
        handle: &DataSourceHandle,
        revision: u64,
    ) -> Result<u64, ControlError> {
        let grid = self
            .grids
            .get(&id)
            .ok_or_else(|| error("Pivot grid was removed"))?;
        if !Rc::ptr_eq(handle, &grid.source.handle)
            || !grid.settings.is_read_only
            || grid.source.presentation != GridPresentationSettings::default()
            || grid.source.detail.is_some()
            || !grid.cell_templates.is_empty()
            || grid.model.columns().len() != grid.settings.columns.len()
        {
            return Err(error("Pivot aggregate grid recipe was changed"));
        }
        let source = handle
            .try_borrow()
            .map_err(|_| error("Pivot projection is borrowed"))?;
        if !source.query().filter.is_empty()
            || source
                .changes_since(revision)
                .changes
                .iter()
                .any(|change| !matches!(change, DataChange::Query))
        {
            return Err(error(
                "Pivot aggregate source was changed outside its owner",
            ));
        }
        Ok(source.revision())
    }
    pub(in crate::controls) fn replace_pivot_grid(
        &mut self,
        id: WidgetId,
        handle: DataSourceHandle,
        settings: GridControlSettings,
        selected: Option<i64>,
        focused_field: Option<&str>,
        anchor_fallbacks: &HashMap<i64, i64>,
    ) -> Result<(), ControlError> {
        let old = self
            .grids
            .get(&id)
            .ok_or_else(|| error("Unknown pivot grid"))?;
        if old.session.is_some() || !old.settings.is_read_only || !settings.is_read_only {
            return Err(error("Pivot projection requires an idle read-only grid"));
        }
        let bounds = old.bounds.unwrap_or(Rect::new(0.0, 0.0, 640.0, 320.0));
        let mut anchor = old.model.capture_scroll_anchor(&old.source);
        if let Some(key) = anchor.key
            && let Some(replacement) = anchor_fallbacks.get(&data_key(key))
        {
            anchor.key = Some(row_key(*replacement));
        }
        let columns = settings
            .columns
            .iter()
            .enumerate()
            .map(|(index, c)| GridColumn {
                id: ColumnId(index as u64),
                title: c.title.clone(),
                width: c.width,
                min_width: c.min_width,
                max_width: c.max_width,
                pinned: c.pinned,
                editable: false,
            })
            .collect();
        let mut model = TreeGrid::new(columns).map_err(error)?;
        model.flat_presentation = true;
        let mut source = SnapshotSource::new(
            handle,
            settings.columns.iter().map(|c| c.field.clone()).collect(),
        )?;
        model.refresh(&mut source).map_err(error)?;
        model
            .set_viewport(Size::new(bounds.width, bounds.height))
            .map_err(error)?;
        model
            .assign_selected_key(&source, selected.map(row_key))
            .map_err(error)?;
        if let Some(key) = selected {
            let column = focused_field
                .and_then(|field| settings.columns.iter().position(|c| c.field == field))
                .unwrap_or(0);
            model
                .focus_cell(
                    &source,
                    row_key(key),
                    ColumnId(column as u64),
                    SelectionModifiers::default(),
                )
                .map_err(error)?;
        }
        model
            .restore_scroll_anchor(&source, anchor)
            .map_err(error)?;
        let mut frame = model.paint(&source, bounds).map_err(error)?;
        views::center_scene_text(&mut frame.scene, &mut self.engine)?;
        let arrangement = settings
            .columns
            .iter()
            .map(|c| GridColumnArrangement {
                field: c.field.clone(),
                visible: true,
                width: c.width,
                pinned: c.pinned,
            })
            .collect();
        let grid = self.grids.get_mut(&id).unwrap();
        grid.cancel_pointer();
        grid.settings = settings;
        grid.source = source;
        grid.model = model;
        grid.frame = frame;
        grid.arrangement = arrangement;
        grid.selected_key = selected;
        grid.reveal_selected = false;
        grid.error = None;
        grid.source_error = None;
        grid.cell_templates.clear();
        self.scene_dirty = true;
        Ok(())
    }
}
