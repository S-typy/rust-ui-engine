use super::*;

fn row(key: i64, count: i64) -> DataRow {
    DataRow::new(
        key,
        vec![
            CellValue::Text(format!("Item {key}")),
            CellValue::Integer(count),
        ],
    )
}
fn collection() -> ObservableCollection {
    ObservableCollection::new(
        DataTable::new(
            TableSchema::new(vec![
                FieldSchema::new("Name", FieldType::Text),
                FieldSchema::new("Count", FieldType::Integer),
            ])
            .unwrap(),
            vec![row(i64::MIN, 2), row(10, 10), row(i64::MAX, 1)],
        )
        .unwrap(),
    )
}
fn collection_keys(collection: &ObservableCollection) -> Vec<i64> {
    collection
        .snapshot()
        .rows()
        .iter()
        .map(|row| row.key)
        .collect()
}
fn source_keys(source: &DataSource) -> Vec<i64> {
    (0..source.len())
        .map(|index| source.row_key(index).unwrap())
        .collect()
}

#[test]
fn ordered_mutations_keep_exact_signed_keys_and_record_metadata() {
    let collection = collection();
    collection.insert(1, row(20, 3)).unwrap();
    collection.add(row(30, 4)).unwrap();
    collection.move_row(30, 0).unwrap();
    collection.remove(10).unwrap();
    collection.replace_row(20, row(20, 90)).unwrap();
    collection
        .update_cell(i64::MIN, "Name", CellValue::Text("Updated".into()))
        .unwrap();
    assert_eq!(
        collection_keys(&collection),
        vec![30, i64::MIN, 20, i64::MAX]
    );
    assert_eq!(collection.revision(), 6);
    assert_eq!(
        collection.changes_since(0),
        DataChangeSet {
            from_revision: 0,
            revision: 6,
            changes: vec![
                DataChange::Add { key: 20, index: 1 },
                DataChange::Add { key: 30, index: 4 },
                DataChange::Move {
                    key: 30,
                    from: 4,
                    to: 0
                },
                DataChange::Remove { key: 10, index: 3 },
                DataChange::Replace { key: 20, index: 2 },
                DataChange::Cell {
                    key: i64::MIN,
                    field: "Name".into()
                }
            ],
        }
    );
}

#[test]
fn batch_is_one_revision_and_failed_later_operation_rolls_everything_back() {
    let collection = collection();
    let before = collection.snapshot();
    let failed = collection.batch(vec![
        CollectionMutation::Add(row(42, 8)),
        CollectionMutation::UpdateCell {
            key: 10,
            field: "Count".into(),
            value: CellValue::Text("invalid".into()),
        },
    ]);
    assert!(matches!(failed, Err(DataError::Validation(_))));
    assert_eq!(collection.snapshot(), before);
    assert_eq!(collection.revision(), 0);
    assert!(collection.changes_since(0).changes.is_empty());
    collection
        .batch(vec![
            CollectionMutation::Add(row(42, 8)),
            CollectionMutation::UpdateCell {
                key: 42,
                field: "Name".into(),
                value: CellValue::Text("Added in same batch".into()),
            },
            CollectionMutation::Move { key: 42, index: 0 },
        ])
        .unwrap();
    assert_eq!(collection.revision(), 1);
    assert_eq!(collection.changes_since(0).changes.len(), 3);
    assert_eq!(
        collection.snapshot().value(42, "Name"),
        Some(&CellValue::Text("Added in same batch".into()))
    );
}

#[test]
fn noops_and_net_unchanged_batches_do_not_publish() {
    let collection = collection();
    let original = collection.snapshot();
    assert!(!collection.batch(vec![]).unwrap());
    assert!(!collection.move_row(10, 1).unwrap());
    assert!(
        !collection
            .update_cell(10, "Count", CellValue::Integer(10))
            .unwrap()
    );
    assert!(!collection.replace_row(10, row(10, 10)).unwrap());
    assert!(!collection.replace(original.clone()).unwrap());
    assert!(!collection.reset(original.rows().to_vec()).unwrap());
    assert!(
        !collection
            .batch(vec![
                CollectionMutation::Add(row(42, 8)),
                CollectionMutation::Remove { key: 42 }
            ])
            .unwrap()
    );
    assert!(
        !collection
            .batch(vec![
                CollectionMutation::Clear,
                CollectionMutation::Reset(original.rows().to_vec())
            ])
            .unwrap()
    );
    assert_eq!(collection.revision(), 0);
    assert!(collection.changes_since(0).changes.is_empty());
    assert!(collection.clear().unwrap());
    assert!(!collection.clear().unwrap());
    assert_eq!(collection.revision(), 1);
    assert_eq!(collection.changes_since(0).changes, vec![DataChange::Reset]);
}

#[test]
fn invalid_identity_index_shape_and_schema_leave_collection_unchanged() {
    let collection = collection();
    let original = collection.snapshot();
    assert!(collection.add(row(10, 1)).is_err());
    assert!(collection.remove(999).is_err());
    assert!(collection.move_row(10, 3).is_err());
    assert!(collection.insert(4, row(42, 1)).is_err());
    assert!(collection.replace_row(10, row(11, 1)).is_err());
    assert!(
        collection
            .replace_row(10, DataRow::new(10, vec![]))
            .is_err()
    );
    assert!(collection.reset(vec![row(10, 1), row(10, 2)]).is_err());
    let wrong = DataTable::new(
        TableSchema::new(vec![FieldSchema::new("Other", FieldType::Text)]).unwrap(),
        vec![],
    )
    .unwrap();
    assert!(collection.replace(wrong).is_err());
    assert_eq!(collection.snapshot(), original);
    assert_eq!(collection.revision(), 0);
}

#[test]
fn expected_values_prevent_conflicts_including_equal_requested_value() {
    let collection = collection();
    collection
        .update_cell(10, "Count", CellValue::Integer(20))
        .unwrap();
    let revision = collection.revision();
    assert!(matches!(
        collection.update_cell_expected(
            10,
            "Count",
            &CellValue::Integer(10),
            CellValue::Integer(10)
        ),
        Err(DataError::Conflict(_))
    ));
    assert!(matches!(
        collection.batch(vec![
            CollectionMutation::UpdateCell {
                key: 10,
                field: "Count".into(),
                value: CellValue::Integer(30)
            },
            CollectionMutation::UpdateCellExpected {
                key: 10,
                field: "Count".into(),
                expected: CellValue::Integer(20),
                value: CellValue::Integer(40)
            },
        ]),
        Err(DataError::Conflict(_))
    ));
    assert_eq!(collection.revision(), revision);
    assert_eq!(
        collection.snapshot().value(10, "Count"),
        Some(&CellValue::Integer(20))
    );
}

#[test]
fn readonly_fields_allow_application_row_replacement_but_reject_edits() {
    let mut field = FieldSchema::new("Count", FieldType::Integer);
    field.read_only = true;
    let collection = ObservableCollection::new(
        DataTable::new(
            TableSchema::new(vec![field]).unwrap(),
            vec![DataRow::new(1, vec![CellValue::Integer(1)])],
        )
        .unwrap(),
    );
    assert!(matches!(
        collection.update_cell(1, "Count", CellValue::Integer(2)),
        Err(DataError::ReadOnly(_))
    ));
    collection
        .replace_row(1, DataRow::new(1, vec![CellValue::Integer(2)]))
        .unwrap();
    assert_eq!(
        collection.snapshot().value(1, "Count"),
        Some(&CellValue::Integer(2))
    );
}

#[test]
fn row_text_byte_and_batch_limits_roll_back_without_notifications() {
    let table = collection().snapshot();
    let collection = ObservableCollection::new(
        DataTable::with_limits(
            table.schema().clone(),
            table.rows().to_vec(),
            DataLimits {
                max_rows: 3,
                max_text_bytes: 32,
                max_bytes: 1024,
                ..DataLimits::default()
            },
        )
        .unwrap(),
    );
    assert!(collection.add(row(42, 1)).is_err());
    assert!(
        collection
            .reset(vec![row(1, 1), row(2, 2), row(3, 3), row(4, 4)])
            .is_err()
    );
    assert!(
        collection
            .update_cell(10, "Name", CellValue::Text("Я".repeat(17)))
            .is_err()
    );
    assert!(
        collection
            .batch(vec![
                CollectionMutation::Clear;
                MAX_COLLECTION_BATCH_OPERATIONS + 1
            ])
            .is_err()
    );
    assert_eq!(collection.revision(), 0);
    assert!(collection.changes_since(0).changes.is_empty());
    collection
        .batch(vec![
            CollectionMutation::Remove { key: 10 },
            CollectionMutation::Add(row(42, 1)),
        ])
        .unwrap();
    assert_eq!(collection.snapshot().limits().max_rows, 3);
}

#[test]
fn journal_cursors_are_independent_and_expired_or_future_cursors_reset() {
    let collection = collection();
    for count in 0..(MAX_CHANGE_REVISIONS as i64 + 8) {
        collection
            .update_cell(10, "Count", CellValue::Integer(count))
            .unwrap();
    }
    let revision = collection.revision();
    assert_eq!(collection.changes_since(0).changes, vec![DataChange::Reset]);
    assert_eq!(
        collection.changes_since(revision + 1).changes,
        vec![DataChange::Reset]
    );
    assert!(collection.changes_since(revision).changes.is_empty());
    assert_eq!(
        collection.changes_since(revision - 2),
        collection.clone().changes_since(revision - 2)
    );
    assert_eq!(collection.changes_since(revision - 2).changes.len(), 2);
    assert_eq!(
        collection
            .changes_since(revision - MAX_CHANGE_REVISIONS as u64)
            .changes
            .len(),
        MAX_CHANGE_REVISIONS
    );
}

#[test]
fn journal_bounds_record_count_even_for_large_batches() {
    let collection = collection();
    for batch in 0..5 {
        let operations = (0..MAX_COLLECTION_BATCH_OPERATIONS)
            .map(|index| CollectionMutation::UpdateCell {
                key: 10,
                field: "Count".into(),
                value: CellValue::Integer((batch * MAX_COLLECTION_BATCH_OPERATIONS + index) as i64),
            })
            .collect();
        collection.batch(operations).unwrap();
    }
    assert_eq!(collection.revision(), 5);
    assert_eq!(collection.changes_since(0).changes, vec![DataChange::Reset]);
    assert_eq!(
        collection.changes_since(1).changes.len(),
        MAX_CHANGE_RECORDS
    );
}

#[test]
fn two_sources_independently_synchronize_and_keep_their_queries() {
    let collection = collection();
    let mut first =
        DataSource::new(CollectionAdapter::from_collection(collection.clone())).unwrap();
    let mut second =
        DataSource::new(CollectionAdapter::from_collection(collection.clone())).unwrap();
    second
        .set_query(DataQuery {
            sort: vec![SortDescriptor {
                field: "Count".into(),
                direction: SortDirection::Ascending,
            }],
            filter: String::new(),
        })
        .unwrap();
    collection.add(row(-5, 3)).unwrap();
    assert_eq!(first.len(), 3);
    assert_eq!(second.len(), 3);
    assert!(first.synchronize().unwrap());
    assert_eq!(first.len(), 4);
    assert_eq!(second.len(), 3);
    assert!(second.synchronize().unwrap());
    assert_eq!(source_keys(&first), vec![i64::MIN, 10, i64::MAX, -5]);
    assert_eq!(source_keys(&second), vec![i64::MAX, i64::MIN, -5, 10]);
    assert!(!first.synchronize().unwrap());
    first.set_cell(10, "Count", CellValue::Integer(0)).unwrap();
    assert!(second.synchronize().unwrap());
    assert_eq!(second.row_key(0), Some(10));
    assert_eq!(
        collection.snapshot().value(10, "Count"),
        Some(&CellValue::Integer(0))
    );
}

#[test]
fn source_changes_distinguish_query_reset_and_stable_field_updates() {
    let collection = collection();
    let mut source =
        DataSource::new(CollectionAdapter::from_collection(collection.clone())).unwrap();
    collection
        .update_cell(10, "Count", CellValue::Integer(20))
        .unwrap();
    source.synchronize().unwrap();
    assert_eq!(
        source.changes_since(0).changes,
        vec![DataChange::Cell {
            key: 10,
            field: "Count".into()
        }]
    );
    source
        .set_query(DataQuery {
            filter: "updated".into(),
            ..DataQuery::default()
        })
        .unwrap();
    assert_eq!(source.changes_since(1).changes, vec![DataChange::Query]);
    collection
        .update_cell(10, "Name", CellValue::Text("UPDATED".into()))
        .unwrap();
    source.synchronize().unwrap();
    assert_eq!(source_keys(&source), vec![10]);
    assert_eq!(source.changes_since(2).changes, vec![DataChange::Reset]);
    collection
        .update_cell(10, "Count", CellValue::Integer(30))
        .unwrap();
    source.synchronize().unwrap();
    assert_eq!(
        source.changes_since(3).changes,
        vec![DataChange::Cell {
            key: 10,
            field: "Count".into()
        }]
    );
    collection.move_row(10, 0).unwrap();
    source.synchronize().unwrap();
    assert_eq!(source.changes_since(4).changes, vec![DataChange::Reset]);
}

#[test]
fn source_refresh_recovers_journal_gap_without_losing_sorted_filter_view() {
    let collection = collection();
    let mut source =
        DataSource::new(CollectionAdapter::from_collection(collection.clone())).unwrap();
    source
        .set_query(DataQuery {
            filter: "Item".into(),
            sort: vec![SortDescriptor {
                field: "Count".into(),
                direction: SortDirection::Descending,
            }],
        })
        .unwrap();
    for count in 0..300 {
        collection
            .update_cell(10, "Count", CellValue::Integer(count))
            .unwrap();
    }
    assert_eq!(collection.changes_since(0).changes, vec![DataChange::Reset]);
    assert!(source.synchronize().unwrap());
    assert_eq!(source.row_key(0), Some(10));
    assert_eq!(source.value(10, "Count"), Some(&CellValue::Integer(299)));
    assert_eq!(source.revision(), 2);
}

#[test]
fn failed_unchanged_or_deleted_draft_commit_preserves_accepted_snapshot() {
    let collection = collection();
    let mut source =
        DataSource::new(CollectionAdapter::from_collection(collection.clone())).unwrap();
    collection
        .update_cell(10, "Count", CellValue::Integer(20))
        .unwrap();
    assert!(matches!(
        source.set_cell_expected(10, "Count", &CellValue::Integer(10), CellValue::Integer(10)),
        Err(DataError::Conflict(_))
    ));
    assert_eq!(source.value(10, "Count"), Some(&CellValue::Integer(10)));
    assert_eq!(source.revision(), 0);
    assert!(matches!(source.last_error(), Some(DataError::Conflict(_))));
    collection.remove(10).unwrap();
    assert!(matches!(
        source.set_cell_expected(10, "Count", &CellValue::Integer(20), CellValue::Integer(21)),
        Err(DataError::UnknownRow(10))
    ));
    assert!(source.row(10).is_some());
    assert_eq!(source.revision(), 0);
    assert_eq!(collection.revision(), 2);
    assert!(source.synchronize().unwrap());
    assert!(source.row(10).is_none());
}

#[test]
fn plain_assignment_uses_latest_observable_row_and_supports_new_keys() {
    let collection = collection();
    let mut source =
        DataSource::new(CollectionAdapter::from_collection(collection.clone())).unwrap();
    collection.add(row(42, 1)).unwrap();
    assert!(source.set_cell(42, "Count", CellValue::Integer(2)).unwrap());
    assert_eq!(source.value(42, "Count"), Some(&CellValue::Integer(2)));
    assert_eq!(source.revision(), 1);
    collection
        .update_cell(42, "Count", CellValue::Integer(3))
        .unwrap();
    assert!(source.set_cell(42, "Count", CellValue::Integer(4)).unwrap());
    assert_eq!(
        collection.snapshot().value(42, "Count"),
        Some(&CellValue::Integer(4))
    );
}

#[test]
fn successful_unchanged_draft_adopts_other_external_changes_once() {
    let collection = collection();
    let mut source =
        DataSource::new(CollectionAdapter::from_collection(collection.clone())).unwrap();
    collection.remove(i64::MAX).unwrap();
    assert!(
        source
            .set_cell_expected(10, "Count", &CellValue::Integer(10), CellValue::Integer(10))
            .unwrap()
    );
    assert_eq!(source.len(), 2);
    assert_eq!(source.revision(), 1);
    assert_eq!(collection.revision(), 1);
    assert!(!source.synchronize().unwrap());
    assert!(
        !source
            .set_cell_expected(10, "Count", &CellValue::Integer(10), CellValue::Integer(10))
            .unwrap()
    );
    assert_eq!(source.revision(), 1);
}

#[test]
fn tight_table_limit_does_not_double_count_expected_value_payload() {
    let text = CellValue::Text("x".repeat(1000));
    let table = DataTable::with_limits(
        TableSchema::new(vec![FieldSchema::new("Text", FieldType::Text)]).unwrap(),
        vec![DataRow::new(1, vec![text.clone()])],
        DataLimits {
            max_bytes: 1200,
            ..DataLimits::default()
        },
    )
    .unwrap();
    let collection = ObservableCollection::new(table);
    assert!(
        !collection
            .update_cell_expected(1, "Text", &text, text.clone())
            .unwrap()
    );
    let mut source =
        DataSource::new(CollectionAdapter::from_collection(collection.clone())).unwrap();
    assert!(
        !source
            .set_cell_expected(1, "Text", &text, text.clone())
            .unwrap()
    );
    assert_eq!(collection.revision(), 0);
    assert_eq!(source.revision(), 0);
}

#[test]
fn final_candidate_byte_failure_rolls_back_all_staged_fields() {
    let schema = TableSchema::new(vec![
        FieldSchema::new("A", FieldType::Text),
        FieldSchema::new("B", FieldType::Text),
    ])
    .unwrap();
    let original = DataTable::with_limits(
        schema,
        vec![DataRow::new(
            1,
            vec![
                CellValue::Text(String::new()),
                CellValue::Text(String::new()),
            ],
        )],
        DataLimits {
            max_bytes: 300,
            ..DataLimits::default()
        },
    )
    .unwrap();
    let collection = ObservableCollection::new(original.clone());
    assert!(matches!(
        collection.batch(vec![
            CollectionMutation::UpdateCell {
                key: 1,
                field: "A".into(),
                value: CellValue::Text("a".repeat(100))
            },
            CollectionMutation::UpdateCell {
                key: 1,
                field: "B".into(),
                value: CellValue::Text("b".repeat(100))
            },
        ]),
        Err(DataError::Capacity(_))
    ));
    assert_eq!(collection.snapshot(), original);
    assert_eq!(collection.revision(), 0);
}

#[test]
fn incoming_batch_payload_is_bounded_even_when_final_value_would_be_small() {
    let collection = collection();
    let original = collection.snapshot();
    let text_bytes = MAX_COLLECTION_BATCH_BYTES / MAX_COLLECTION_BATCH_OPERATIONS;
    let operations = (0..MAX_COLLECTION_BATCH_OPERATIONS)
        .map(|_| CollectionMutation::UpdateCell {
            key: 10,
            field: "Name".into(),
            value: CellValue::Text("x".repeat(text_bytes)),
        })
        .collect();
    assert!(matches!(
        collection.batch(operations),
        Err(DataError::Capacity(_))
    ));
    assert_eq!(collection.snapshot(), original);
    assert_eq!(collection.revision(), 0);
}

#[test]
fn rejected_write_with_pending_collection_changes_is_fully_atomic() {
    let collection = collection();
    let mut source =
        DataSource::new(CollectionAdapter::from_collection(collection.clone())).unwrap();
    let accepted = source.table().clone();
    collection.remove(i64::MAX).unwrap();
    assert!(
        source
            .set_cell(10, "Count", CellValue::Text("invalid".into()))
            .is_err()
    );
    assert_eq!(source.table(), &accepted);
    assert_eq!(source.revision(), 0);
    assert_eq!(collection.revision(), 1);
    assert!(source.changes_since(0).changes.is_empty());
    assert!(source.synchronize().unwrap());
    assert_eq!(source.len(), 2);
}

#[test]
fn source_journal_is_bounded_independently_of_collection_cursors() {
    let collection = collection();
    let mut source =
        DataSource::new(CollectionAdapter::from_collection(collection.clone())).unwrap();
    for index in 0..MAX_CHANGE_REVISIONS + 2 {
        source
            .set_query(DataQuery {
                filter: index.to_string(),
                ..DataQuery::default()
            })
            .unwrap();
    }
    assert_eq!(collection.revision(), 0);
    assert_eq!(source.changes_since(0).changes, vec![DataChange::Reset]);
    assert_eq!(
        source.changes_since(2).changes,
        vec![DataChange::Query; MAX_CHANGE_REVISIONS]
    );
    assert_eq!(
        source.changes_since(source.revision() - 1).changes,
        vec![DataChange::Query]
    );
}

#[test]
fn legacy_adapter_idle_synchronize_never_loads_or_erases_failure() {
    use std::{cell::Cell, rc::Rc};
    struct Adapter {
        calls: Rc<Cell<usize>>,
    }
    impl DataAdapter for Adapter {
        fn load(&mut self) -> Result<DataTable, DataError> {
            self.calls.set(self.calls.get() + 1);
            if self.calls.get() > 1 {
                Err(DataError::Adapter("failure".into()))
            } else {
                Ok(collection().snapshot())
            }
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
    }
    let calls = Rc::new(Cell::new(0));
    let mut source = DataSource::new(Adapter {
        calls: calls.clone(),
    })
    .unwrap();
    assert!(!source.synchronize().unwrap());
    assert_eq!(calls.get(), 1);
    assert!(source.refresh().is_err());
    for _ in 0..3 {
        assert!(!source.synchronize().unwrap());
    }
    assert_eq!(calls.get(), 2);
    assert!(matches!(source.last_error(), Some(DataError::Adapter(_))));
    assert_eq!(source.revision(), 0);
}

#[test]
fn notification_adapter_failure_preserves_accepted_snapshot_and_cursor_for_retry() {
    use std::{cell::Cell, rc::Rc};
    struct Adapter {
        collection: ObservableCollection,
        fail: Rc<Cell<bool>>,
        loads: Rc<Cell<usize>>,
    }
    impl DataAdapter for Adapter {
        fn load(&mut self) -> Result<DataTable, DataError> {
            self.loads.set(self.loads.get() + 1);
            if self.fail.get() {
                Err(DataError::Adapter("temporary failure".into()))
            } else {
                Ok(self.collection.snapshot())
            }
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
        fn notification_revision(&self) -> Option<u64> {
            Some(self.collection.revision())
        }
    }
    let collection = collection();
    let fail = Rc::new(Cell::new(false));
    let loads = Rc::new(Cell::new(0));
    let mut source = DataSource::new(Adapter {
        collection: collection.clone(),
        fail: fail.clone(),
        loads: loads.clone(),
    })
    .unwrap();
    collection.remove(10).unwrap();
    fail.set(true);
    assert!(source.synchronize().is_err());
    assert_eq!(source.len(), 3);
    assert_eq!(source.revision(), 0);
    assert!(source.changes_since(0).changes.is_empty());
    fail.set(false);
    assert!(source.synchronize().unwrap());
    assert_eq!(source.len(), 2);
    assert_eq!(source.revision(), 1);
    assert!(source.last_error().is_none());
    assert_eq!(loads.get(), 3);
}
