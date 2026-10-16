use std::{
    cell::RefCell,
    collections::{HashSet, VecDeque},
    rc::Rc,
};

use crate::{CellValue, DataError, DataRow, DataTable};

/// Maximum number of operations in one atomic collection mutation.
pub const MAX_COLLECTION_BATCH_OPERATIONS: usize = 1024;
/// Incoming row/cell payload per batch, independent of retained table limits.
pub const MAX_COLLECTION_BATCH_BYTES: usize = 64 * 1024 * 1024;
/// Retained collection/source revisions. Consumers keep independent cursors.
pub const MAX_CHANGE_REVISIONS: usize = 256;
/// Retained change records, without copies of row or cell values.
pub const MAX_CHANGE_RECORDS: usize = 4096;

/// Metadata describing a committed change. Collection indices describe the
/// sequential operations in that batch. Source indices describe its query view.
/// `Reset` means the consumer must read a current snapshot rather than replay
/// any records in that set. `Query` is emitted only by a source changing its
/// local query.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DataChange {
    Add { key: i64, index: usize },
    Remove { key: i64, index: usize },
    Move { key: i64, from: usize, to: usize },
    Replace { key: i64, index: usize },
    Cell { key: i64, field: String },
    Reset,
    Query,
}

/// Changes after a consumer's cursor through the current revision. An expired
/// or future cursor yields a single `Reset`; an up-to-date cursor yields none.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DataChangeSet {
    pub from_revision: u64,
    pub revision: u64,
    pub changes: Vec<DataChange>,
}

#[derive(Debug, Default)]
pub(crate) struct ChangeJournal {
    entries: VecDeque<(u64, Vec<DataChange>)>,
    records: usize,
}

impl ChangeJournal {
    pub(crate) fn push(&mut self, revision: u64, mut changes: Vec<DataChange>) {
        if changes.len() > MAX_CHANGE_RECORDS {
            changes = vec![DataChange::Reset];
        }
        self.records += changes.len();
        self.entries.push_back((revision, changes));
        while self.entries.len() > MAX_CHANGE_REVISIONS || self.records > MAX_CHANGE_RECORDS {
            if let Some((_, changes)) = self.entries.pop_front() {
                self.records -= changes.len();
            }
        }
    }

    pub(crate) fn since(&self, cursor: u64, revision: u64) -> DataChangeSet {
        let changes = if cursor == revision {
            Vec::new()
        } else if cursor > revision
            || self
                .entries
                .front()
                .is_none_or(|(first, _)| cursor < first - 1)
        {
            vec![DataChange::Reset]
        } else {
            self.entries
                .iter()
                .filter(|(at, _)| *at > cursor)
                .flat_map(|(_, changes)| changes.iter().cloned())
                .collect()
        };
        DataChangeSet {
            from_revision: cursor,
            revision,
            changes,
        }
    }
}

/// Operations are applied in order to a private candidate and published once.
/// `Move.index` is the final zero-based position after removing that row.
/// Row replacement keeps its identity; field edits enforce `read_only`.
/// App-owned row replacement/reset can populate read-only display fields.
#[derive(Clone, Debug, PartialEq)]
pub enum CollectionMutation {
    Add(DataRow),
    Insert {
        index: usize,
        row: DataRow,
    },
    Remove {
        key: i64,
    },
    Move {
        key: i64,
        index: usize,
    },
    Replace {
        key: i64,
        row: DataRow,
    },
    UpdateCell {
        key: i64,
        field: String,
        value: CellValue,
    },
    UpdateCellExpected {
        key: i64,
        field: String,
        expected: CellValue,
        value: CellValue,
    },
    Clear,
    Reset(Vec<DataRow>),
}

#[derive(Debug)]
struct CollectionState {
    table: DataTable,
    revision: u64,
    journal: ChangeJournal,
}

/// Cloneable application-owned collection with atomic, bounded notifications.
/// Mutations and polling run on the owning thread; no callbacks, worker or I/O
/// run from getters. The schema and capacity limits are fixed at construction.
#[derive(Clone, Debug)]
pub struct ObservableCollection {
    state: Rc<RefCell<CollectionState>>,
}

/// Compatibility name for the application-owned observable collection handle.
pub type CollectionHandle = ObservableCollection;

impl ObservableCollection {
    pub fn new(table: DataTable) -> Self {
        Self {
            state: Rc::new(RefCell::new(CollectionState {
                table,
                revision: 0,
                journal: ChangeJournal::default(),
            })),
        }
    }
    pub fn snapshot(&self) -> DataTable {
        self.state.borrow().table.clone()
    }
    pub fn revision(&self) -> u64 {
        self.state.borrow().revision
    }
    pub fn changes_since(&self, cursor: u64) -> DataChangeSet {
        let state = self.state.borrow();
        state.journal.since(cursor, state.revision)
    }
    pub fn add(&self, row: DataRow) -> Result<bool, DataError> {
        self.batch(vec![CollectionMutation::Add(row)])
    }
    pub fn insert(&self, index: usize, row: DataRow) -> Result<bool, DataError> {
        self.batch(vec![CollectionMutation::Insert { index, row }])
    }
    pub fn remove(&self, key: i64) -> Result<bool, DataError> {
        self.batch(vec![CollectionMutation::Remove { key }])
    }
    pub fn move_row(&self, key: i64, index: usize) -> Result<bool, DataError> {
        self.batch(vec![CollectionMutation::Move { key, index }])
    }
    pub fn replace_row(&self, key: i64, row: DataRow) -> Result<bool, DataError> {
        self.batch(vec![CollectionMutation::Replace { key, row }])
    }
    pub fn update_cell(&self, key: i64, field: &str, value: CellValue) -> Result<bool, DataError> {
        self.batch(vec![CollectionMutation::UpdateCell {
            key,
            field: field.into(),
            value,
        }])
    }
    pub fn update_cell_expected(
        &self,
        key: i64,
        field: &str,
        expected: &CellValue,
        value: CellValue,
    ) -> Result<bool, DataError> {
        self.batch(vec![CollectionMutation::UpdateCellExpected {
            key,
            field: field.into(),
            expected: expected.clone(),
            value,
        }])
    }
    pub fn clear(&self) -> Result<bool, DataError> {
        self.batch(vec![CollectionMutation::Clear])
    }
    pub fn reset(&self, rows: Vec<DataRow>) -> Result<bool, DataError> {
        self.batch(vec![CollectionMutation::Reset(rows)])
    }
    pub fn replace(&self, table: DataTable) -> Result<bool, DataError> {
        if self.state.borrow().table.schema() != table.schema() {
            return Err(DataError::Schema(
                "collection replacement changed its schema".into(),
            ));
        }
        // The receiving collection's limits, not the caller's, remain in force.
        self.reset(table.rows().to_vec())
    }

    /// Publishes all operations or none. Empty and net-unchanged batches do not
    /// increment the revision. Each intermediate row count and incoming payload
    /// are bounded; the final candidate validates types, keys and total bytes.
    pub fn batch(&self, operations: Vec<CollectionMutation>) -> Result<bool, DataError> {
        if operations.len() > MAX_COLLECTION_BATCH_OPERATIONS {
            return Err(DataError::Capacity(
                "collection batch operation count exceeded".into(),
            ));
        }
        if operations.is_empty() {
            return Ok(false);
        }
        let mut state = self.state.borrow_mut();
        validate_payload(&state.table, &operations)?;
        let mut rows = state.table.rows().to_vec();
        let mut changes = Vec::new();
        for operation in operations {
            match operation {
                CollectionMutation::Add(row) => {
                    let index = rows.len();
                    insert_row(&state.table, &mut rows, index, row, &mut changes)?;
                }
                CollectionMutation::Insert { index, row } => {
                    insert_row(&state.table, &mut rows, index, row, &mut changes)?;
                }
                CollectionMutation::Remove { key } => {
                    let index = row_index(&rows, key)?;
                    rows.remove(index);
                    changes.push(DataChange::Remove { key, index });
                }
                CollectionMutation::Move { key, index } => {
                    let from = row_index(&rows, key)?;
                    if index >= rows.len() {
                        return Err(DataError::Validation(
                            "collection move index out of bounds".into(),
                        ));
                    }
                    if from != index {
                        let row = rows.remove(from);
                        rows.insert(index, row);
                        changes.push(DataChange::Move {
                            key,
                            from,
                            to: index,
                        });
                    }
                }
                CollectionMutation::Replace { key, row } => {
                    let index = row_index(&rows, key)?;
                    if row.key != key {
                        return Err(DataError::Validation(
                            "row replacement changed its stable key".into(),
                        ));
                    }
                    if rows[index] != row {
                        rows[index] = row;
                        changes.push(DataChange::Replace { key, index });
                    }
                }
                CollectionMutation::UpdateCell { key, field, value } => {
                    edit_cell(
                        &state.table,
                        &mut rows,
                        key,
                        field,
                        None,
                        value,
                        &mut changes,
                    )?;
                }
                CollectionMutation::UpdateCellExpected {
                    key,
                    field,
                    expected,
                    value,
                } => {
                    edit_cell(
                        &state.table,
                        &mut rows,
                        key,
                        field,
                        Some(expected),
                        value,
                        &mut changes,
                    )?;
                }
                CollectionMutation::Clear => {
                    if !rows.is_empty() {
                        rows.clear();
                        changes.push(DataChange::Reset);
                    }
                }
                CollectionMutation::Reset(next) => {
                    if rows != next {
                        rows = next;
                        changes.push(DataChange::Reset);
                    }
                }
            }
        }
        let table =
            DataTable::with_limits(state.table.schema().clone(), rows, state.table.limits())?;
        if state.table == table {
            return Ok(false);
        }
        let revision = state
            .revision
            .checked_add(1)
            .ok_or(DataError::RevisionOverflow)?;
        state.table = table;
        state.revision = revision;
        state.journal.push(revision, changes);
        Ok(true)
    }
}

fn row_index(rows: &[DataRow], key: i64) -> Result<usize, DataError> {
    rows.iter()
        .position(|row| row.key == key)
        .ok_or(DataError::UnknownRow(key))
}

fn insert_row(
    table: &DataTable,
    rows: &mut Vec<DataRow>,
    index: usize,
    row: DataRow,
    changes: &mut Vec<DataChange>,
) -> Result<(), DataError> {
    if index > rows.len() {
        return Err(DataError::Validation(
            "collection insert index out of bounds".into(),
        ));
    }
    if rows.len() >= table.limits().max_rows {
        return Err(DataError::Capacity("collection row count exceeded".into()));
    }
    if rows.iter().any(|current| current.key == row.key) {
        return Err(DataError::Validation("duplicate collection row key".into()));
    }
    changes.push(DataChange::Add {
        key: row.key,
        index,
    });
    rows.insert(index, row);
    Ok(())
}

fn edit_cell(
    table: &DataTable,
    rows: &mut [DataRow],
    key: i64,
    field: String,
    expected: Option<CellValue>,
    value: CellValue,
    changes: &mut Vec<DataChange>,
) -> Result<(), DataError> {
    let column = table
        .schema()
        .field_index(&field)
        .ok_or_else(|| DataError::UnknownField(field.clone()))?;
    let schema = &table.schema().fields()[column];
    if schema.read_only {
        return Err(DataError::ReadOnly(field));
    }
    schema.validate(&value)?;
    let index = row_index(rows, key)?;
    let current = &mut rows[index].values[column];
    if expected.is_some_and(|expected| *current != expected) {
        return Err(DataError::Conflict(
            "collection value changed since editing began".into(),
        ));
    }
    if *current != value {
        *current = value;
        changes.push(DataChange::Cell { key, field });
    }
    Ok(())
}

fn validate_payload(table: &DataTable, operations: &[CollectionMutation]) -> Result<(), DataError> {
    let mut bytes = 0usize;
    for operation in operations {
        match operation {
            CollectionMutation::Add(row)
            | CollectionMutation::Insert { row, .. }
            | CollectionMutation::Replace { row, .. } => {
                validate_row(table, row)?;
                charge_payload(&mut bytes, size_of::<DataRow>())?;
                for cell in &row.values {
                    charge_value(table, &mut bytes, cell)?;
                }
            }
            CollectionMutation::Reset(rows) => {
                if rows.len() > table.limits().max_rows {
                    return Err(DataError::Capacity(
                        "collection reset row count exceeded".into(),
                    ));
                }
                let mut keys = HashSet::with_capacity(rows.len());
                for row in rows {
                    if !keys.insert(row.key) {
                        return Err(DataError::Validation(
                            "duplicate collection reset row key".into(),
                        ));
                    }
                    validate_row(table, row)?;
                    charge_payload(&mut bytes, size_of::<DataRow>())?;
                    for cell in &row.values {
                        charge_value(table, &mut bytes, cell)?;
                    }
                }
            }
            CollectionMutation::UpdateCell {
                field, value: cell, ..
            } => {
                validate_field(table, field)?;
                charge_payload(&mut bytes, field.len())?;
                charge_value(table, &mut bytes, cell)?;
            }
            CollectionMutation::UpdateCellExpected {
                field,
                expected,
                value: cell,
                ..
            } => {
                validate_field(table, field)?;
                charge_payload(&mut bytes, field.len())?;
                charge_value(table, &mut bytes, expected)?;
                charge_value(table, &mut bytes, cell)?;
            }
            _ => {}
        }
    }
    Ok(())
}

fn charge_payload(bytes: &mut usize, amount: usize) -> Result<(), DataError> {
    *bytes = bytes.saturating_add(amount);
    if *bytes > MAX_COLLECTION_BATCH_BYTES {
        Err(DataError::Capacity(
            "collection batch payload exceeded".into(),
        ))
    } else {
        Ok(())
    }
}

fn charge_value(table: &DataTable, bytes: &mut usize, value: &CellValue) -> Result<(), DataError> {
    let text_bytes = if let CellValue::Text(text) = value {
        text.len()
    } else {
        0
    };
    if text_bytes > table.limits().max_text_bytes {
        return Err(DataError::Capacity("collection cell text exceeded".into()));
    }
    charge_payload(bytes, size_of::<CellValue>().saturating_add(text_bytes))
}

fn validate_row(table: &DataTable, row: &DataRow) -> Result<(), DataError> {
    if row.values.len() != table.schema().fields().len() {
        return Err(DataError::Validation(
            "collection row width does not match schema".into(),
        ));
    }
    for (field, value) in table.schema().fields().iter().zip(&row.values) {
        field.validate(value)?;
    }
    Ok(())
}

fn validate_field(table: &DataTable, field: &str) -> Result<(), DataError> {
    if field.len() > 256 {
        return Err(DataError::Capacity("collection field name exceeded".into()));
    }
    table
        .schema()
        .field_index(field)
        .map(|_| ())
        .ok_or_else(|| DataError::UnknownField(field.into()))
}
