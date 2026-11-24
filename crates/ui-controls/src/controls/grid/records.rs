//! Transactional record editing over the mounted grid's accepted source.
use super::*;
use rust_desktop_ui_data::{DataRow, DataWrite, TableSchema};
use std::collections::VecDeque;

#[derive(Clone, Debug, PartialEq)]
pub struct GridRowDraft {
    pub row: DataRow,
    original: Option<DataRow>,
}
impl GridRowDraft {
    pub fn is_new(&self) -> bool {
        self.original.is_none()
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GridRowIssue {
    pub field: String,
    pub message: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GridRecordEvent {
    Current(Option<i64>),
    Begin(i64),
    Cancel(i64),
    Saved(i64),
    Deleted(i64),
}
pub(super) type Validator = Rc<dyn Fn(&TableSchema, &DataRow) -> Result<(), GridRowIssue>>;
#[derive(Default)]
pub(super) struct RecordState {
    pub(super) draft: Option<GridRowDraft>,
    issue: Option<GridRowIssue>,
    pub(super) validator: Option<Validator>,
    events: VecDeque<GridRecordEvent>,
    observed: Option<i64>,
}
impl RecordState {
    pub(super) fn emit(&mut self, event: GridRecordEvent) {
        if self.events.len() == 256 {
            self.events.pop_front();
        }
        self.events.push_back(event);
    }
    pub(super) fn observe(&mut self, key: Option<i64>) {
        if self.observed != key {
            self.observed = key;
            self.emit(GridRecordEvent::Current(key));
        }
    }
}
impl Controls {
    fn writable_grid(&self, id: WidgetId) -> Result<&GridState, ControlError> {
        let g = self.grids.get(&id).ok_or_else(|| error("Unknown grid"))?;
        if !self.effective_enabled(id) || g.settings.is_read_only || !self.grid_relation_live(id) {
            return Err(error("Grid is disabled or read-only"));
        }
        if g.session.is_some() {
            return Err(error("Finish the active cell editor first"));
        }
        Ok(g)
    }
    pub fn take_grid_record_events(&mut self, id: WidgetId) -> Vec<GridRecordEvent> {
        self.grids.get_mut(&id).map_or_else(Vec::new, |g| {
            g.records.observe(g.selected_key);
            g.records.events.drain(..).collect()
        })
    }
    pub fn set_grid_row_validator(
        &mut self,
        id: WidgetId,
        validate: impl Fn(&TableSchema, &DataRow) -> Result<(), GridRowIssue> + 'static,
    ) -> Result<(), ControlError> {
        self.grids
            .get_mut(&id)
            .ok_or_else(|| error("Unknown grid"))?
            .records
            .validator = Some(Rc::new(validate));
        Ok(())
    }
    pub fn grid_row_draft(&self, id: WidgetId) -> Option<&GridRowDraft> {
        self.grids.get(&id)?.records.draft.as_ref()
    }
    pub fn grid_row_issue(&self, id: WidgetId) -> Option<&GridRowIssue> {
        self.grids.get(&id)?.records.issue.as_ref()
    }
    /// Start an insert with application-supplied initial values, including generated identifiers.
    pub fn begin_grid_insert(&mut self, id: WidgetId, row: DataRow) -> Result<(), ControlError> {
        self.start_grid_record(id, row, None)
    }
    pub fn begin_grid_row_edit(&mut self, id: WidgetId, key: i64) -> Result<(), ControlError> {
        let g = self.writable_grid(id)?;
        if !g.source.positions.contains_key(&key) {
            return Err(error("Record outside this view"));
        }
        let handle = g.source.handle.try_borrow().map_err(error)?;
        let row = handle
            .table()
            .row(key)
            .ok_or_else(|| error("Missing record"))?
            .clone();
        drop(handle);
        self.start_grid_record(id, row.clone(), Some(row))
    }
    fn start_grid_record(
        &mut self,
        id: WidgetId,
        row: DataRow,
        original: Option<DataRow>,
    ) -> Result<(), ControlError> {
        let g = self.writable_grid(id)?;
        if g.records.draft.is_some() {
            return Err(error("Finish the active record first"));
        }
        let handle = g.source.handle.try_borrow().map_err(error)?;
        let caps = handle.write_capabilities();
        if !caps.atomic_batch
            || if original.is_some() {
                !caps.update
            } else {
                !caps.insert
            }
        {
            return Err(error("Operation unavailable for this source"));
        }
        if row.values.len() != handle.table().schema().fields().len() {
            return Err(error("Invalid record width"));
        }
        if original.is_none() && handle.table().row(row.key).is_some() {
            return Err(error("Duplicate record key"));
        }
        g.source.check_relation_row(handle.table().schema(), &row)?;
        drop(handle);
        let state = &mut self.grids.get_mut(&id).unwrap().records;
        state.issue = None;
        state.emit(GridRecordEvent::Begin(row.key));
        state.draft = Some(GridRowDraft { row, original });
        Ok(())
    }
    pub fn set_grid_draft_cell(
        &mut self,
        id: WidgetId,
        field: &str,
        value: CellValue,
    ) -> Result<(), ControlError> {
        let g = self.writable_grid(id)?;
        if !g
            .settings
            .columns
            .iter()
            .any(|c| c.field == field && !c.read_only)
            || g.source.locked_field(field)
        {
            return Err(error("Field is unavailable for editing"));
        }
        let handle = g.source.handle.try_borrow().map_err(error)?;
        let schema = handle.table().schema();
        let i = schema
            .field_index(field)
            .ok_or_else(|| error("Unknown storage field"))?;
        if schema.fields()[i].read_only {
            return Err(error("Read-only field"));
        }
        schema.fields()[i].validate(&value).map_err(error)?;
        drop(handle);
        let r = &mut self.grids.get_mut(&id).unwrap().records;
        r.draft
            .as_mut()
            .ok_or_else(|| error("No active record"))?
            .row
            .values[i] = value;
        r.issue = None;
        Ok(())
    }
    pub fn cancel_grid_record(&mut self, id: WidgetId) -> Result<(), ControlError> {
        let r = &mut self
            .grids
            .get_mut(&id)
            .ok_or_else(|| error("Unknown grid"))?
            .records;
        if let Some(d) = r.draft.take() {
            r.emit(GridRecordEvent::Cancel(d.row.key));
        }
        r.issue = None;
        Ok(())
    }
    pub fn commit_grid_record(&mut self, id: WidgetId) -> Result<bool, ControlError> {
        let g = self.writable_grid(id)?;
        let d = g
            .records
            .draft
            .as_ref()
            .ok_or_else(|| error("No active record"))?
            .clone();
        let handle = g.source.handle.try_borrow().map_err(error)?;
        let schema = handle.table().schema();
        let mut ops: Vec<_> = if let Some(old) = &d.original {
            if handle.table().row(old.key) != Some(old) {
                return Err(error("Record changed during editing"));
            }
            schema
                .fields()
                .iter()
                .zip(old.values.iter().zip(&d.row.values))
                .filter(|(_, (a, b))| a != b)
                .map(|(f, (a, b))| DataWrite::Update {
                    key: old.key,
                    field: f.name.clone(),
                    expected: a.clone(),
                    value: b.clone(),
                })
                .collect()
        } else {
            vec![DataWrite::Insert(d.row.clone())]
        };
        if let Some(old) = &d.original {
            ops.insert(0, DataWrite::CheckRow(old.clone()));
        }
        drop(handle);
        let changed = self.apply_grid_batch(id, &ops, d.original.as_ref())?;
        let state = &mut self.grids.get_mut(&id).unwrap().records;
        state.draft = None;
        state.issue = None;
        state.emit(GridRecordEvent::Saved(d.row.key));
        Ok(changed)
    }
    pub fn delete_grid_record(&mut self, id: WidgetId, key: i64) -> Result<(), ControlError> {
        let g = self.writable_grid(id)?;
        if !g.source.positions.contains_key(&key) || g.records.draft.is_some() {
            return Err(error("Record cannot be deleted in this view"));
        }
        let expected = g
            .source
            .handle
            .try_borrow()
            .map_err(error)?
            .table()
            .row(key)
            .ok_or_else(|| error("Missing row"))?
            .clone();
        self.apply_grid_batch(id, &[DataWrite::Delete { expected }], None)?;
        self.grids
            .get_mut(&id)
            .unwrap()
            .records
            .emit(GridRecordEvent::Deleted(key));
        Ok(())
    }
    pub(super) fn apply_grid_batch(
        &mut self,
        id: WidgetId,
        operations: &[DataWrite],
        expected: Option<&DataRow>,
    ) -> Result<bool, ControlError> {
        let g = self.writable_grid(id)?;
        let mut data = g.source.handle.try_borrow_mut().map_err(error)?;
        data.synchronize().map_err(error)?;
        if expected.is_some_and(|r| data.table().row(r.key) != Some(r)) {
            return Err(error("Record changed during editing"));
        }
        data.write_capabilities()
            .validate(operations)
            .map_err(error)?;
        let table = data.table().preview_writes(operations).map_err(error)?;
        let mut keys = std::collections::BTreeSet::new();
        for op in operations {
            match op {
                DataWrite::Insert(row) => {
                    keys.insert(row.key);
                }
                DataWrite::Update { key, field, .. } => {
                    if g.source.locked_field(field)
                        || !g
                            .settings
                            .columns
                            .iter()
                            .any(|c| c.field == *field && !c.read_only)
                    {
                        return Err(error("Grid field is read-only"));
                    }
                    keys.insert(*key);
                }
                DataWrite::CheckRow(_) | DataWrite::Delete { .. } => {}
            }
        }
        for key in keys {
            let row = table
                .row(key)
                .ok_or_else(|| error("Missing candidate record"))?;
            g.source.check_relation_row(table.schema(), row)?;
            if let Some(check) = &g.records.validator
                && let Err(issue) = check(table.schema(), row)
            {
                let err = error(format!("{}: {}", issue.field, issue.message));
                drop(data);
                self.grids.get_mut(&id).unwrap().records.issue = Some(issue);
                return Err(err);
            }
        }
        let mut next = g.source.clone();
        next.table = unbound::table(&table, &next.unbound, &next.formulas, None)?;
        next.keys = next.filtered_keys(None)?;
        next.positions = next.keys.iter().enumerate().map(|(i, k)| (*k, i)).collect();
        Projection::build(&next, &g.source.projection)?;
        let changed = data.write_batch(operations).map_err(error)?;
        drop(data);
        self.sync_grids()?;
        self.scene_dirty = true;
        Ok(changed)
    }
    pub fn replace_grid_source(
        &mut self,
        id: WidgetId,
        source: DataSourceHandle,
    ) -> Result<(), ControlError> {
        let g = self.grids.get(&id).ok_or_else(|| error("Unknown grid"))?;
        if g.session.is_some()
            || g.records.draft.is_some()
            || g.source.relation.is_some()
            || !g.relations.children.is_empty()
        {
            return Err(error(
                "Finish editing and close relations before replacing the source",
            ));
        }
        source
            .try_borrow_mut()
            .map_err(error)?
            .synchronize()
            .map_err(error)?;
        let data = source.try_borrow().map_err(error)?;
        let schema = data.table().schema();
        if schema
            != g.source
                .handle
                .try_borrow()
                .map_err(error)?
                .table()
                .schema()
            && (g.records.validator.is_some()
                || !g.editor_recipes.is_empty()
                || !g.relations.definitions.is_empty())
        {
            return Err(error(
                "Replacement schema differs from configured record or editor contracts",
            ));
        }
        let columns = if g.auto_columns {
            schema
                .fields()
                .iter()
                .map(|f| GridControlColumn {
                    read_only: f.read_only,
                    ..GridControlColumn::new(&f.name, &f.title, 160.0)
                })
                .collect()
        } else {
            g.settings.columns.clone()
        };
        if columns.is_empty()
            || columns.len() > 64
            || columns.iter().any(|c| {
                c.expression.is_none() && c.unbound.is_none() && schema.field(&c.field).is_none()
            })
        {
            return Err(error("Incompatible replacement schema"));
        }
        let same = columns
            .iter()
            .map(|c| &c.field)
            .eq(g.settings.columns.iter().map(|c| &c.field));
        if !same
            && (!g.editor_recipes.is_empty()
                || !g.conditional_editors.is_empty()
                || !g.cell_templates.is_empty()
                || g.source.presentation != Default::default())
        {
            return Err(error("Explicit presentation requires matching fields"));
        }
        drop(data);
        let mut next = SnapshotSource::new(source, &columns)?;
        if same {
            next.presentation = g.source.presentation.clone();
            next.detail = g.source.detail.clone();
            next.filters = g.source.filters.clone();
            next.choice_labels = g.source.choice_labels.clone();
            next.reload()?;
        }
        let arrangement = if same {
            g.current_arrangement()
        } else {
            columns
                .iter()
                .map(|c| GridColumnArrangement {
                    field: c.field.clone(),
                    visible: true,
                    width: c.width,
                    pinned: c.pinned,
                })
                .collect()
        };
        let g = self.grids.get_mut(&id).unwrap();
        let old = std::mem::replace(&mut g.settings.columns, columns);
        let model = match g.make_model(&mut next, &arrangement) {
            Ok(m) => m,
            Err(e) => {
                g.settings.columns = old;
                return Err(e);
            }
        };
        g.model = model;
        g.source = next;
        g.arrangement = arrangement;
        g.transfer = Default::default();
        g.cancel_pointer();
        g.restore_selection()?;
        self.sync_data_wakes()?;
        self.scene_dirty = true;
        Ok(())
    }
}
