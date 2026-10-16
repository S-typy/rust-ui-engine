use std::{cmp::Ordering, collections::HashMap};

use crate::{
    CellValue, CollectionHandle, DataChange, DataChangeSet, DataError, DataRow, DataTable,
    MAX_CHANGE_RECORDS, ObservableCollection, observable::ChangeJournal,
};

/// A synchronous adapter returns complete, validated, bounded snapshots.
/// Schema and limits are stable for the adapter lifetime. A failed write leaves
/// the backing store unchanged; a successful write returns the committed snapshot.
/// Reads and writes are explicit operations, never called from cell getters.
pub trait DataAdapter {
    fn load(&mut self) -> Result<DataTable, DataError>;
    fn write_cell(
        &mut self,
        key: i64,
        field: &str,
        expected: &CellValue,
        value: &CellValue,
    ) -> Result<DataTable, DataError>;
    /// In-memory notification cursor. This getter must not perform I/O.
    /// Adapters without notifications retain explicit refresh semantics.
    fn notification_revision(&self) -> Option<u64> {
        None
    }
    /// Non-consuming bounded metadata for independent readers, without I/O.
    fn notification_changes_since(&self, _cursor: u64) -> Option<DataChangeSet> {
        None
    }
}

/// Adapter over an application-owned observable Rust collection.
pub struct CollectionAdapter {
    handle: CollectionHandle,
}

impl CollectionAdapter {
    pub fn new(table: DataTable) -> Self {
        Self::from_collection(ObservableCollection::new(table))
    }
    pub fn from_collection(handle: ObservableCollection) -> Self {
        Self { handle }
    }
    pub fn handle(&self) -> CollectionHandle {
        self.handle.clone()
    }
}

impl DataAdapter for CollectionAdapter {
    fn load(&mut self) -> Result<DataTable, DataError> {
        Ok(self.handle.snapshot())
    }
    fn write_cell(
        &mut self,
        key: i64,
        field: &str,
        expected: &CellValue,
        value: &CellValue,
    ) -> Result<DataTable, DataError> {
        self.handle
            .update_cell_expected(key, field, expected, value.clone())?;
        Ok(self.handle.snapshot())
    }
    fn notification_revision(&self) -> Option<u64> {
        Some(self.handle.revision())
    }
    fn notification_changes_since(&self, cursor: u64) -> Option<DataChangeSet> {
        Some(self.handle.changes_since(cursor))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortDirection {
    Ascending,
    Descending,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SortDescriptor {
    pub field: String,
    pub direction: SortDirection,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DataQuery {
    pub sort: Vec<SortDescriptor>,
    pub filter: String,
}

/// Cached table and local view. Sorting is typed, nulls sort first ascending
/// and last descending; equal sort values use ascending signed key order.
/// Filtering performs Unicode lowercase substring search over displayed cells.
/// Queries execute outside paint and never interpolate expressions into SQL.
pub struct DataSource {
    adapter: Box<dyn DataAdapter>,
    table: DataTable,
    query: DataQuery,
    visible: Vec<usize>,
    positions: HashMap<i64, usize>,
    revision: u64,
    adapter_revision: Option<u64>,
    journal: ChangeJournal,
    last_error: Option<DataError>,
}

impl DataSource {
    pub fn new(mut adapter: impl DataAdapter + 'static) -> Result<Self, DataError> {
        let table = adapter.load()?;
        let query = DataQuery::default();
        let (visible, positions) = query_index(&table, &query)?;
        let adapter_revision = adapter.notification_revision();
        Ok(Self {
            adapter: Box::new(adapter),
            table,
            query,
            visible,
            positions,
            revision: 0,
            adapter_revision,
            journal: ChangeJournal::default(),
            last_error: None,
        })
    }
    pub fn table(&self) -> &DataTable {
        &self.table
    }
    pub fn query(&self) -> &DataQuery {
        &self.query
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn changes_since(&self, cursor: u64) -> DataChangeSet {
        self.journal.since(cursor, self.revision)
    }
    pub fn last_error(&self) -> Option<&DataError> {
        self.last_error.as_ref()
    }
    pub fn len(&self) -> usize {
        self.visible.len()
    }
    pub fn is_empty(&self) -> bool {
        self.visible.is_empty()
    }
    pub fn row_key(&self, index: usize) -> Option<i64> {
        self.visible
            .get(index)
            .map(|index| self.table.rows()[*index].key)
    }
    pub fn position(&self, key: i64) -> Option<usize> {
        self.positions.get(&key).copied()
    }
    pub fn row(&self, key: i64) -> Option<&DataRow> {
        self.table.row(key)
    }
    pub fn value(&self, key: i64, field: &str) -> Option<&CellValue> {
        self.table.value(key, field)
    }
    pub fn refresh(&mut self) -> Result<bool, DataError> {
        let result = self.refresh_inner();
        self.record(result)
    }
    /// Adopts a changed in-memory collection at an explicit update boundary.
    /// Idle polling is O(1), retains any existing error and performs no load.
    /// Adapters without notifications (including SQLite) require `refresh`.
    pub fn synchronize(&mut self) -> Result<bool, DataError> {
        let revision = self.adapter.notification_revision();
        if revision.is_none() || revision == self.adapter_revision {
            return Ok(false);
        }
        let result = self.refresh_inner();
        self.record(result)
    }
    fn refresh_inner(&mut self) -> Result<bool, DataError> {
        let table = self.adapter.load()?;
        self.check_schema(&table)?;
        let (visible, positions) = query_index(&table, &self.query)?;
        if table == self.table {
            self.adapter_revision = self.adapter.notification_revision();
            return Ok(false);
        }
        let revision = self
            .revision
            .checked_add(1)
            .ok_or(DataError::RevisionOverflow)?;
        let changes = accepted_changes(&self.table, &table, &self.visible, &visible);
        self.table = table;
        self.visible = visible;
        self.positions = positions;
        self.revision = revision;
        self.adapter_revision = self.adapter.notification_revision();
        self.journal.push(revision, changes);
        Ok(true)
    }
    pub fn set_query(&mut self, query: DataQuery) -> Result<bool, DataError> {
        let result = self.query_inner(query);
        self.record(result)
    }
    fn query_inner(&mut self, query: DataQuery) -> Result<bool, DataError> {
        if query == self.query {
            return Ok(false);
        }
        let revision = self
            .revision
            .checked_add(1)
            .ok_or(DataError::RevisionOverflow)?;
        let (visible, positions) = query_index(&self.table, &query)?;
        self.query = query;
        self.visible = visible;
        self.positions = positions;
        self.revision = revision;
        self.journal.push(revision, vec![DataChange::Query]);
        Ok(true)
    }
    /// Assigns against the latest observable row. Use `set_cell_expected` for
    /// an edit draft that must detect changes since editing began.
    pub fn set_cell(&mut self, key: i64, field: &str, value: CellValue) -> Result<bool, DataError> {
        let result = self.edit_inner(key, field, None, value);
        self.record(result)
    }
    pub fn set_cell_expected(
        &mut self,
        key: i64,
        field: &str,
        expected: &CellValue,
        value: CellValue,
    ) -> Result<bool, DataError> {
        let result = self.edit_inner(key, field, Some(expected), value);
        self.record(result)
    }
    fn edit_inner(
        &mut self,
        key: i64,
        field: &str,
        expected: Option<&CellValue>,
        value: CellValue,
    ) -> Result<bool, DataError> {
        // Stage pending observable changes without publishing them. A failed
        // draft commit must not change either accepted snapshot or revision.
        let notification = self.adapter.notification_revision();
        let pending = if notification.is_some() && notification != self.adapter_revision {
            let table = self.adapter.load()?;
            self.check_schema(&table)?;
            Some(table)
        } else {
            None
        };
        let current = pending.as_ref().unwrap_or(&self.table);
        let expected = match expected {
            Some(expected) => expected.clone(),
            None => current.value(key, field).cloned().ok_or_else(|| {
                if current.row(key).is_none() {
                    DataError::UnknownRow(key)
                } else {
                    DataError::UnknownField(field.into())
                }
            })?,
        };
        // Validate candidate limits and expected value before a store write,
        // including a nominally unchanged draft against a changed/deleted row.
        current.edited(key, field, &expected, &value)?;
        if expected == value && *current == self.table {
            self.adapter_revision = notification;
            return Ok(false);
        }
        let revision = self
            .revision
            .checked_add(1)
            .ok_or(DataError::RevisionOverflow)?;
        let table = if expected == value {
            // The accepted source changed even though the edited field did not.
            pending.expect("a changed staged snapshot exists")
        } else {
            self.adapter.write_cell(key, field, &expected, &value)?
        };
        self.check_schema(&table)?;
        let (visible, positions) = query_index(&table, &self.query)?;
        let changes = accepted_changes(&self.table, &table, &self.visible, &visible);
        self.table = table;
        self.visible = visible;
        self.positions = positions;
        self.revision = revision;
        self.adapter_revision = self.adapter.notification_revision();
        self.journal.push(revision, changes);
        Ok(true)
    }
    fn check_schema(&self, table: &DataTable) -> Result<(), DataError> {
        if table.schema() != self.table.schema() || table.limits() != self.table.limits() {
            Err(DataError::Schema(
                "adapter changed its schema or limits".into(),
            ))
        } else {
            Ok(())
        }
    }
    fn record(&mut self, result: Result<bool, DataError>) -> Result<bool, DataError> {
        self.last_error = result.as_ref().err().cloned();
        result
    }
}

fn accepted_changes(
    before: &DataTable,
    after: &DataTable,
    before_visible: &[usize],
    after_visible: &[usize],
) -> Vec<DataChange> {
    // Membership/order changes require a new view. The source's revision space
    // is independent of adapter revisions and includes query-only changes.
    if before
        .rows()
        .iter()
        .map(|row| row.key)
        .ne(after.rows().iter().map(|row| row.key))
        || before_visible
            .iter()
            .map(|index| before.rows()[*index].key)
            .ne(after_visible.iter().map(|index| after.rows()[*index].key))
    {
        return vec![DataChange::Reset];
    }
    let mut changes = Vec::new();
    for (before_row, after_row) in before.rows().iter().zip(after.rows()) {
        for (index, (left, right)) in before_row.values.iter().zip(&after_row.values).enumerate() {
            if left != right {
                if changes.len() == MAX_CHANGE_RECORDS {
                    return vec![DataChange::Reset];
                }
                changes.push(DataChange::Cell {
                    key: after_row.key,
                    field: after.schema().fields()[index].name.clone(),
                });
            }
        }
    }
    changes
}

type QueryIndex = (Vec<usize>, HashMap<i64, usize>);

fn query_index(table: &DataTable, query: &DataQuery) -> Result<QueryIndex, DataError> {
    if query.filter.len() > 4096
        || query.filter.contains('\0')
        || query.sort.len() > table.schema().fields().len()
    {
        return Err(DataError::Capacity("query size exceeded".into()));
    }
    let mut sort = Vec::with_capacity(query.sort.len());
    for descriptor in &query.sort {
        let index = table
            .schema()
            .field_index(&descriptor.field)
            .ok_or_else(|| DataError::UnknownField(descriptor.field.clone()))?;
        if sort.iter().any(|(field, _)| *field == index) {
            return Err(DataError::Validation("duplicate sort field".into()));
        }
        sort.push((index, descriptor.direction));
    }
    let filter = query.filter.to_lowercase();
    let mut visible: Vec<usize> = table
        .rows()
        .iter()
        .enumerate()
        .filter(|(_, row)| {
            filter.is_empty()
                || row
                    .values
                    .iter()
                    .any(|value| value.display().to_lowercase().contains(&filter))
        })
        .map(|(index, _)| index)
        .collect();
    if !sort.is_empty() {
        visible.sort_by(|left, right| {
            let left = &table.rows()[*left];
            let right = &table.rows()[*right];
            for &(index, direction) in &sort {
                let order = compare(&left.values[index], &right.values[index]);
                let order = if direction == SortDirection::Descending {
                    order.reverse()
                } else {
                    order
                };
                if order != Ordering::Equal {
                    return order;
                }
            }
            left.key.cmp(&right.key)
        });
    }
    let positions = visible
        .iter()
        .enumerate()
        .map(|(position, index)| (table.rows()[*index].key, position))
        .collect();
    Ok((visible, positions))
}

fn compare(left: &CellValue, right: &CellValue) -> Ordering {
    match (left, right) {
        (CellValue::Null, CellValue::Null) => Ordering::Equal,
        (CellValue::Null, _) => Ordering::Less,
        (_, CellValue::Null) => Ordering::Greater,
        (CellValue::Bool(left), CellValue::Bool(right)) => left.cmp(right),
        (CellValue::Integer(left), CellValue::Integer(right)) => left.cmp(right),
        (CellValue::Real(left), CellValue::Real(right)) => {
            left.partial_cmp(right).unwrap_or(Ordering::Equal)
        }
        (CellValue::Text(left), CellValue::Text(right)) => left.cmp(right),
        _ => Ordering::Equal, // Validated columns have exactly one non-null type.
    }
}
