use std::{cell::RefCell, cmp::Ordering, collections::HashMap, rc::Rc};

use crate::{CellValue, DataError, DataRow, DataTable};

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
}

/// Owned Rust collection. Applications can keep a copy of accepted source
/// snapshots; mutations go through the source to preserve validation/query state.
pub struct CollectionAdapter {
    handle: CollectionHandle,
}

/// Application-owned collection handle. Replacement validates the full table
/// before publication and preserves the declared schema. Sources observe it
/// on explicit `refresh` or a successful write; existing edit drafts stay valid.
#[derive(Clone)]
pub struct CollectionHandle {
    table: Rc<RefCell<DataTable>>,
}

impl CollectionHandle {
    pub fn snapshot(&self) -> DataTable {
        self.table.borrow().clone()
    }
    pub fn replace(&self, table: DataTable) -> Result<bool, DataError> {
        let mut current = self.table.borrow_mut();
        if current.schema() != table.schema() {
            return Err(DataError::Schema(
                "collection replacement changed its schema".into(),
            ));
        }
        // Retain the collection's configured limits even if a replacement was
        // constructed with the more generous global defaults.
        let table = if table.limits() == current.limits() {
            table
        } else {
            DataTable::with_limits(
                table.schema().clone(),
                table.rows().to_vec(),
                current.limits(),
            )?
        };
        if *current == table {
            return Ok(false);
        }
        *current = table;
        Ok(true)
    }
}

impl CollectionAdapter {
    pub fn new(table: DataTable) -> Self {
        Self {
            handle: CollectionHandle {
                table: Rc::new(RefCell::new(table)),
            },
        }
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
        let next = self
            .handle
            .table
            .borrow()
            .edited(key, field, expected, value)?;
        *self.handle.table.borrow_mut() = next.clone();
        Ok(next)
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
    last_error: Option<DataError>,
}

impl DataSource {
    pub fn new(mut adapter: impl DataAdapter + 'static) -> Result<Self, DataError> {
        let table = adapter.load()?;
        let query = DataQuery::default();
        let (visible, positions) = query_index(&table, &query)?;
        Ok(Self {
            adapter: Box::new(adapter),
            table,
            query,
            visible,
            positions,
            revision: 0,
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
    fn refresh_inner(&mut self) -> Result<bool, DataError> {
        let revision = self
            .revision
            .checked_add(1)
            .ok_or(DataError::RevisionOverflow)?;
        let table = self.adapter.load()?;
        self.check_schema(&table)?;
        let (visible, positions) = query_index(&table, &self.query)?;
        if table == self.table {
            return Ok(false);
        }
        self.table = table;
        self.visible = visible;
        self.positions = positions;
        self.revision = revision;
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
        Ok(true)
    }
    pub fn set_cell(&mut self, key: i64, field: &str, value: CellValue) -> Result<bool, DataError> {
        let expected = self.value(key, field).cloned().ok_or_else(|| {
            if self.row(key).is_none() {
                DataError::UnknownRow(key)
            } else {
                DataError::UnknownField(field.into())
            }
        });
        match expected {
            Ok(expected) => self.set_cell_expected(key, field, &expected, value),
            Err(error) => self.record(Err(error)),
        }
    }
    pub fn set_cell_expected(
        &mut self,
        key: i64,
        field: &str,
        expected: &CellValue,
        value: CellValue,
    ) -> Result<bool, DataError> {
        let result = self.edit_inner(key, field, expected, value);
        self.record(result)
    }
    fn edit_inner(
        &mut self,
        key: i64,
        field: &str,
        expected: &CellValue,
        value: CellValue,
    ) -> Result<bool, DataError> {
        let revision = self
            .revision
            .checked_add(1)
            .ok_or(DataError::RevisionOverflow)?;
        // Validate candidate limits before the backing store can be mutated.
        self.table.edited(key, field, expected, &value)?;
        if *expected == value {
            return Ok(false);
        }
        let table = self.adapter.write_cell(key, field, expected, &value)?;
        self.check_schema(&table)?;
        let (visible, positions) = query_index(&table, &self.query)?;
        self.table = table;
        self.visible = visible;
        self.positions = positions;
        self.revision = revision;
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
