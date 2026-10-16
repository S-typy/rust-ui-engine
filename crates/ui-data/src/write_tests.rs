use std::{cell::Cell, rc::Rc};

use crate::*;

fn table() -> DataTable {
    let mut code = FieldSchema::new("Code", FieldType::Integer);
    code.read_only = true;
    DataTable::new(
        TableSchema::new(vec![FieldSchema::new("Name", FieldType::Text), code]).unwrap(),
        vec![row(10, "Alpha", 100), row(20, "Beta", 200)],
    )
    .unwrap()
}

fn row(key: i64, name: &str, code: i64) -> DataRow {
    DataRow::new(
        key,
        vec![CellValue::Text(name.into()), CellValue::Integer(code)],
    )
}

fn update(key: i64, before: &str, after: &str) -> DataWrite {
    DataWrite::Update {
        key,
        field: "Name".into(),
        expected: CellValue::Text(before.into()),
        value: CellValue::Text(after.into()),
    }
}

#[test]
fn crud_batch_is_one_collection_and_source_revision_with_stable_keys() {
    let adapter = CollectionAdapter::new(table());
    let handle = adapter.handle();
    let mut source = DataSource::new(adapter).unwrap();
    let deliveries = Rc::new(Cell::new(0));
    let calls = deliveries.clone();
    let _subscription = handle
        .subscribe(move |_| calls.set(calls.get() + 1))
        .unwrap();
    let operations = vec![
        DataWrite::Insert(row(i64::MAX, "New", 300)),
        update(i64::MAX, "New", "Gamma"),
        update(10, "Alpha", "Updated"),
        DataWrite::Delete {
            expected: row(20, "Beta", 200),
        },
    ];
    assert!(source.write_batch(&operations).unwrap());
    assert_eq!(source.revision(), 1);
    assert_eq!(handle.revision(), 1);
    assert_eq!(deliveries.get(), 1);
    assert_eq!(
        source.table().rows(),
        &[row(10, "Updated", 100), row(i64::MAX, "Gamma", 300)]
    );
    assert_eq!(source.table(), &handle.snapshot());
    assert_eq!(handle.changes_since(0).changes, [DataChange::Reset]);
    assert_eq!(source.changes_since(0).changes, [DataChange::Reset]);
    assert_eq!(operations[1], update(i64::MAX, "New", "Gamma"));
    assert!(!source.synchronize().unwrap());
}

#[test]
fn conflict_in_last_operation_rolls_back_store_snapshot_query_and_draft() {
    let adapter = CollectionAdapter::new(table());
    let handle = adapter.handle();
    let mut source = DataSource::new(adapter).unwrap();
    source
        .set_query(DataQuery {
            filter: "alpha".into(),
            sort: vec![],
        })
        .unwrap();
    let revision = source.revision();
    let original = source.table().clone();
    let draft = vec![
        DataWrite::Insert(row(30, "New", 300)),
        update(10, "Alpha", "Changed"),
        DataWrite::Delete {
            expected: row(20, "Beta", 999),
        },
    ];
    let saved_draft = draft.clone();
    assert!(matches!(
        source.write_batch(&draft),
        Err(DataError::Conflict(_))
    ));
    assert_eq!(draft, saved_draft);
    assert_eq!(source.table(), &original);
    assert_eq!(handle.snapshot(), original);
    assert_eq!(handle.revision(), 0);
    assert_eq!(source.revision(), revision);
    assert_eq!(source.query().filter, "alpha");
    assert_eq!(source.row_key(0), Some(10));
    assert!(source.changes_since(revision).changes.is_empty());
    assert!(matches!(source.last_error(), Some(DataError::Conflict(_))));
    assert!(!source.write_batch(&[]).unwrap());
    assert!(source.last_error().is_none());
}

#[test]
fn late_external_change_and_schema_drift_never_publish_pending_snapshot_on_failure() {
    let adapter = CollectionAdapter::new(table());
    let handle = adapter.handle();
    let mut source = DataSource::new(adapter).unwrap();
    handle
        .update_cell(20, "Name", CellValue::Text("External".into()))
        .unwrap();
    assert!(matches!(
        source.write_batch(&[DataWrite::Delete {
            expected: row(20, "Beta", 200)
        }]),
        Err(DataError::Conflict(_))
    ));
    assert_eq!(source.table(), &table());
    assert_eq!(source.revision(), 0);
    assert_eq!(handle.revision(), 1);
    assert!(source.synchronize().unwrap());
    assert_eq!(
        source.value(20, "Name"),
        Some(&CellValue::Text("External".into()))
    );
    let accepted = source.table().clone();
    handle
        .replace_schema(
            TableSchema::new(vec![FieldSchema::new("Other", FieldType::Text)]).unwrap(),
            vec![],
        )
        .unwrap();
    assert!(matches!(
        source.write_batch(&[update(10, "Alpha", "New")]),
        Err(DataError::Conflict(_))
    ));
    assert_eq!(source.table(), &accepted);
    assert_eq!(source.revision(), 1);
}

#[test]
fn writes_enforce_readonly_types_unique_keys_and_bounded_intermediate_rows() {
    let limits = DataLimits {
        max_rows: 2,
        max_text_bytes: 8,
        ..DataLimits::default()
    };
    let table =
        DataTable::with_limits(table().schema().clone(), table().rows().to_vec(), limits).unwrap();
    let mut source = DataSource::new(CollectionAdapter::new(table.clone())).unwrap();
    let bad_batches = vec![
        vec![DataWrite::Update {
            key: 10,
            field: "Code".into(),
            expected: CellValue::Integer(100),
            value: CellValue::Integer(101),
        }],
        vec![DataWrite::Insert(row(10, "New", 123))],
        vec![
            DataWrite::Insert(row(30, "New", 123)),
            DataWrite::Delete {
                expected: row(10, "Alpha", 100),
            },
        ],
        vec![update(10, "Alpha", "too long text")],
        vec![DataWrite::Insert(DataRow::new(
            30,
            vec![CellValue::Text("New".into())],
        ))],
        vec![DataWrite::Update {
            key: 10,
            field: "Name".into(),
            expected: CellValue::Null,
            value: CellValue::Text("New".into()),
        }],
    ];
    for batch in bad_batches {
        assert!(source.write_batch(&batch).is_err());
        assert_eq!(source.table(), &table);
        assert_eq!(source.revision(), 0);
    }
    let oversized = vec![update(10, "Alpha", "Alpha"); MAX_DATA_WRITE_OPERATIONS + 1];
    assert!(matches!(
        source.write_batch(&oversized),
        Err(DataError::Capacity(_))
    ));
    // Deletion before insertion makes room; initial read-only values are valid.
    assert!(
        source
            .write_batch(&[
                DataWrite::Delete {
                    expected: row(10, "Alpha", 100)
                },
                DataWrite::Insert(row(i64::MIN, "New", 999))
            ])
            .unwrap()
    );
    assert_eq!(source.row_key(1), Some(i64::MIN));
}

#[test]
fn net_unchanged_batch_checks_expectations_but_does_not_publish_revision() {
    let adapter = CollectionAdapter::new(table());
    let handle = adapter.handle();
    let mut source = DataSource::new(adapter).unwrap();
    let operations = [update(10, "Alpha", "New"), update(10, "New", "Alpha")];
    assert!(!source.write_batch(&operations).unwrap());
    assert_eq!(source.revision(), 0);
    assert_eq!(handle.revision(), 0);
    assert!(!source.write_batch(&[]).unwrap());
}

#[test]
fn adapter_write_captures_its_own_snapshot_before_reentrant_commit() {
    let adapter = CollectionAdapter::new(table());
    let handle = adapter.handle();
    let weak = handle.downgrade();
    let mut source = DataSource::new(adapter).unwrap();
    let _sub = handle
        .subscribe(move |revision| {
            if revision == 1 {
                weak.upgrade()
                    .unwrap()
                    .update_cell(10, "Name", CellValue::Text("Callback".into()))
                    .unwrap();
            }
        })
        .unwrap();
    assert!(source.write_batch(&[update(10, "Alpha", "Batch")]).unwrap());
    assert_eq!(
        source.value(10, "Name"),
        Some(&CellValue::Text("Batch".into()))
    );
    assert_eq!(handle.revision(), 2);
    assert!(source.synchronize().unwrap());
    assert_eq!(
        source.value(10, "Name"),
        Some(&CellValue::Text("Callback".into()))
    );
    assert_eq!(source.revision(), 2);
}

struct LegacyAdapter {
    loads: Rc<Cell<usize>>,
    writes: Rc<Cell<usize>>,
}

impl DataAdapter for LegacyAdapter {
    fn load(&mut self) -> Result<DataTable, DataError> {
        self.loads.set(self.loads.get() + 1);
        Ok(table())
    }
    fn write_cell(
        &mut self,
        _: i64,
        _: &str,
        _: &CellValue,
        _: &CellValue,
    ) -> Result<DataTable, DataError> {
        self.writes.set(self.writes.get() + 1);
        Err(DataError::Adapter("legacy only".into()))
    }
}

#[test]
fn unsupported_batch_never_falls_back_to_cell_writes_or_loads() {
    let loads = Rc::new(Cell::new(0));
    let writes = Rc::new(Cell::new(0));
    let mut source = DataSource::new(LegacyAdapter {
        loads: loads.clone(),
        writes: writes.clone(),
    })
    .unwrap();
    assert!(!source.write_capabilities().atomic_batch);
    assert!(matches!(
        source.write_batch(&[update(10, "Alpha", "New")]),
        Err(DataError::Adapter(_))
    ));
    assert!(!source.write_batch(&[]).unwrap());
    assert_eq!((loads.get(), writes.get()), (1, 0));
    assert_eq!(source.revision(), 0);
}

#[test]
fn invalid_provider_snapshot_is_rejected_without_changing_accepted_state() {
    struct InvalidAdapter;
    impl DataAdapter for InvalidAdapter {
        fn load(&mut self) -> Result<DataTable, DataError> {
            Ok(table())
        }
        fn write_cell(
            &mut self,
            _: i64,
            _: &str,
            _: &CellValue,
            _: &CellValue,
        ) -> Result<DataTable, DataError> {
            unreachable!()
        }
        fn write_capabilities(&self) -> WriteCapabilities {
            WriteCapabilities::transactional_crud()
        }
        fn write_batch(&mut self, _: &[DataWrite]) -> Result<DataTable, DataError> {
            DataTable::new(
                TableSchema::new(vec![FieldSchema::new("Wrong", FieldType::Bool)]).unwrap(),
                vec![],
            )
        }
    }
    let mut source = DataSource::new(InvalidAdapter).unwrap();
    assert!(source.write_batch(&[update(10, "Alpha", "New")]).is_err());
    assert_eq!(source.table(), &table());
    assert_eq!(source.revision(), 0);
    assert!(source.changes_since(0).changes.is_empty());
}

#[test]
fn independent_views_share_one_source_without_reads_and_preserve_their_queries() {
    let loads = Rc::new(Cell::new(0));
    let mut source = DataSource::new(LegacyAdapter {
        loads: loads.clone(),
        writes: Rc::new(Cell::new(0)),
    })
    .unwrap();
    let mut first = DataView::new(
        &source,
        DataQuery {
            filter: "alpha".into(),
            sort: vec![],
        },
    )
    .unwrap();
    let mut second = DataView::new(
        &source,
        DataQuery {
            filter: String::new(),
            sort: vec![SortDescriptor {
                field: "Code".into(),
                direction: SortDirection::Descending,
            }],
        },
    )
    .unwrap();
    assert_eq!(first.row_key(0), Some(10));
    assert_eq!(second.row_key(0), Some(20));
    source
        .set_query(DataQuery {
            filter: "nothing".into(),
            sort: vec![],
        })
        .unwrap();
    assert!(!first.synchronize(&source).unwrap());
    assert!(!second.synchronize(&source).unwrap());
    assert_eq!((first.revision(), second.revision()), (0, 0));
    assert_eq!(first.source_revision(), 1);
    let mut next = table().rows().to_vec();
    next[1] = row(20, "Alpha two", 200);
    source
        .replace_adapter(
            CollectionAdapter::new(DataTable::new(table().schema().clone(), next).unwrap()),
            DataQuery::default(),
        )
        .unwrap();
    assert!(first.synchronize(&source).unwrap());
    assert!(second.synchronize(&source).unwrap());
    assert_eq!(first.len(), 2);
    assert_eq!(second.row_key(0), Some(20));
    for _ in 0..100 {
        assert!(first.value(20, "Name").is_some());
        assert_eq!(second.position(10), Some(1));
    }
    assert_eq!(loads.get(), 1);
}

#[test]
fn view_failed_schema_sync_keeps_snapshot_and_can_atomically_change_query() {
    let adapter = CollectionAdapter::new(table());
    let handle = adapter.handle();
    let mut source = DataSource::new(adapter).unwrap();
    let original_query = DataQuery {
        filter: String::new(),
        sort: vec![SortDescriptor {
            field: "Code".into(),
            direction: SortDirection::Ascending,
        }],
    };
    let mut view = DataView::new(&source, original_query.clone()).unwrap();
    handle
        .replace_schema(
            TableSchema::new(vec![FieldSchema::new("Other", FieldType::Integer)]).unwrap(),
            vec![DataRow::new(33, vec![CellValue::Integer(7)])],
        )
        .unwrap();
    assert!(source.synchronize().unwrap());
    assert!(matches!(
        view.synchronize(&source),
        Err(DataError::UnknownField(_))
    ));
    assert_eq!(view.table(), &table());
    assert_eq!(view.query(), &original_query);
    assert_eq!(view.source_revision(), 0);
    assert_eq!(view.revision(), 0);
    assert!(
        view.synchronize_with_query(&source, DataQuery::default())
            .unwrap()
    );
    assert_eq!(view.row_key(0), Some(33));
    assert!(view.last_error().is_none());
    let unrelated = DataSource::new(CollectionAdapter::new(table())).unwrap();
    assert!(view.synchronize(&unrelated).is_err());
    assert_eq!(view.row_key(0), Some(33));
    assert!(view.last_error().is_some());
    assert!(!view.synchronize(&source).unwrap());
    assert!(view.last_error().is_none());
}
