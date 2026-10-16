use std::{collections::HashMap, rc::Rc};

use crate::{CellValue, DataError, DataQuery, DataRow, DataSource, DataTable, source::query_index};

/// Independent read-only query over one source's accepted snapshot. Construct
/// and synchronize at update boundaries; getters and queries never access the
/// adapter. Each view retains a bounded table copy, its own filter/order and
/// revision. Views do not own or poll the source and cannot write editor drafts.
pub struct DataView {
    identity: Rc<()>,
    table: DataTable,
    query: DataQuery,
    visible: Vec<usize>,
    positions: HashMap<i64, usize>,
    source_revision: u64,
    revision: u64,
    last_error: Option<DataError>,
}

impl DataView {
    pub fn new(source: &DataSource, query: DataQuery) -> Result<Self, DataError> {
        let (visible, positions) = query_index(source.table(), &query)?;
        Ok(Self {
            identity: source.identity.clone(),
            table: source.table().clone(),
            query,
            visible,
            positions,
            source_revision: source.revision(),
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
    pub fn source_revision(&self) -> u64 {
        self.source_revision
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

    /// Change only this view's query over its retained snapshot.
    pub fn set_query(&mut self, query: DataQuery) -> Result<bool, DataError> {
        let result = self.accept(None, query);
        self.last_error = result.as_ref().err().cloned();
        result
    }

    /// Accept the source's current snapshot using this view's existing query.
    /// Invalid schema/query changes preserve all accepted view state. Source
    /// query changes alone only advance the source cursor, not view revision.
    pub fn synchronize(&mut self, source: &DataSource) -> Result<bool, DataError> {
        if !Rc::ptr_eq(&self.identity, &source.identity) {
            let error = DataError::Validation("view belongs to another data source".into());
            self.last_error = Some(error.clone());
            return Err(error);
        }
        if self.source_revision == source.revision() {
            self.last_error = None;
            return Ok(false);
        }
        self.synchronize_with_query(source, self.query.clone())
    }

    /// Atomically accept a new snapshot and query, for example after removing
    /// a previously sorted field. This never calls source.refresh or load.
    pub fn synchronize_with_query(
        &mut self,
        source: &DataSource,
        query: DataQuery,
    ) -> Result<bool, DataError> {
        let result = if Rc::ptr_eq(&self.identity, &source.identity) {
            self.accept(Some(source), query)
        } else {
            Err(DataError::Validation(
                "view belongs to another data source".into(),
            ))
        };
        self.last_error = result.as_ref().err().cloned();
        result
    }

    fn accept(&mut self, source: Option<&DataSource>, query: DataQuery) -> Result<bool, DataError> {
        let table = source.map_or(&self.table, DataSource::table);
        let (visible, positions) = query_index(table, &query)?;
        if *table == self.table && query == self.query {
            if let Some(source) = source {
                self.source_revision = source.revision();
            }
            return Ok(false);
        }
        let revision = self
            .revision
            .checked_add(1)
            .ok_or(DataError::RevisionOverflow)?;
        if let Some(source) = source {
            self.table = source.table().clone();
            self.source_revision = source.revision();
        }
        self.query = query;
        self.visible = visible;
        self.positions = positions;
        self.revision = revision;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CollectionAdapter, FieldSchema, FieldType, TableSchema};

    #[test]
    fn exhausted_view_revision_preserves_query_snapshot_and_source_cursor() {
        let schema = TableSchema::new(vec![FieldSchema::new("N", FieldType::Integer)]).unwrap();
        let table =
            DataTable::new(schema, vec![DataRow::new(1, vec![CellValue::Integer(1)])]).unwrap();
        let mut source = DataSource::new(CollectionAdapter::new(table)).unwrap();
        let mut view = DataView::new(&source, DataQuery::default()).unwrap();
        view.revision = u64::MAX;
        source.set_cell(1, "N", CellValue::Integer(2)).unwrap();
        assert_eq!(view.synchronize(&source), Err(DataError::RevisionOverflow));
        assert_eq!(view.source_revision(), 0);
        assert_eq!(view.value(1, "N"), Some(&CellValue::Integer(1)));
        assert_eq!(
            view.set_query(DataQuery {
                filter: "missing".into(),
                sort: vec![]
            }),
            Err(DataError::RevisionOverflow)
        );
        assert_eq!(view.query(), &DataQuery::default());
        assert_eq!(view.len(), 1);
    }
}
