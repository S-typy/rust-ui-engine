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

#[test]
fn subscribers_see_committed_snapshot_and_independent_cursors() {
    use std::{cell::RefCell, rc::Rc};
    let collection = collection();
    let calls = Rc::new(RefCell::new(Vec::new()));
    let mut subscriptions = Vec::new();
    for id in [1, 2] {
        let weak = collection.downgrade();
        let calls = calls.clone();
        subscriptions.push(
            collection
                .subscribe(move |revision| {
                    let collection = weak.upgrade().unwrap();
                    assert_eq!(collection.revision(), revision);
                    assert_eq!(
                        collection.snapshot().value(10, "Count"),
                        Some(&CellValue::Integer(20))
                    );
                    calls.borrow_mut().push((id, collection.changes_since(0)));
                })
                .unwrap(),
        );
    }
    assert!(calls.borrow().is_empty());
    collection
        .update_cell(10, "Count", CellValue::Integer(20))
        .unwrap();
    let calls = calls.borrow();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0].0, 1);
    assert_eq!(calls[1].0, 2);
    assert_eq!(calls[0].1, calls[1].1);
    assert_eq!(calls[0].1.revision, 1);
}

#[test]
fn failed_and_noop_batches_never_wake_subscribers() {
    use std::{cell::Cell, rc::Rc};
    let collection = collection();
    let calls = Rc::new(Cell::new(0));
    let called = calls.clone();
    let _subscription = collection
        .subscribe(move |_| called.set(called.get() + 1))
        .unwrap();
    assert!(
        !collection
            .update_cell(10, "Count", CellValue::Integer(10))
            .unwrap()
    );
    assert!(
        !collection
            .batch(vec![
                CollectionMutation::Add(row(3, 3)),
                CollectionMutation::Remove { key: 3 }
            ])
            .unwrap()
    );
    assert!(
        collection
            .batch(vec![
                CollectionMutation::Add(row(3, 3)),
                CollectionMutation::Remove { key: 999 }
            ])
            .is_err()
    );
    assert_eq!(calls.get(), 0);
    collection
        .batch(vec![
            CollectionMutation::Add(row(3, 3)),
            CollectionMutation::Move { key: 3, index: 0 },
        ])
        .unwrap();
    assert_eq!(calls.get(), 1);
}

#[test]
fn subscriptions_unsubscribe_during_delivery_and_drop_with_owner() {
    use std::{
        cell::{Cell, RefCell},
        rc::Rc,
    };
    let collection = collection();
    let later = Rc::new(RefCell::new(None::<CollectionSubscription>));
    let later_capture = later.clone();
    let first = collection
        .subscribe(move |_| {
            later_capture.borrow_mut().take();
        })
        .unwrap();
    let calls = Rc::new(Cell::new(0));
    let called = calls.clone();
    *later.borrow_mut() = Some(
        collection
            .subscribe(move |_| called.set(called.get() + 1))
            .unwrap(),
    );
    collection.remove(10).unwrap();
    assert_eq!(calls.get(), 0);
    assert!(first.is_active());
    first.unsubscribe();
    first.unsubscribe();
    assert!(!first.is_active());
    let weak = collection.downgrade();
    let token = collection.subscribe(|_| {}).unwrap();
    drop(collection);
    assert!(weak.upgrade().is_none());
    assert!(!token.is_active());
    token.unsubscribe();
}

#[test]
fn callback_can_remove_itself_without_borrow_or_drop_panics() {
    use std::{cell::RefCell, rc::Rc};
    let collection = collection();
    let token = Rc::new(RefCell::new(None::<CollectionSubscription>));
    let capture = token.clone();
    *token.borrow_mut() = Some(
        collection
            .subscribe(move |_| {
                capture.borrow_mut().take();
            })
            .unwrap(),
    );
    collection.remove(10).unwrap();
    assert!(token.borrow().is_none());
    collection.remove(i64::MIN).unwrap();
}

#[test]
fn reentrant_commits_are_queued_and_coalesced_without_losing_journal() {
    use std::{cell::RefCell, rc::Rc};
    let collection = collection();
    let weak = collection.downgrade();
    let calls = Rc::new(RefCell::new(Vec::new()));
    let first_calls = calls.clone();
    let _first = collection
        .subscribe(move |revision| {
            first_calls.borrow_mut().push((1, revision));
            if revision == 1 {
                let collection = weak.upgrade().unwrap();
                collection.add(row(40, 40)).unwrap();
                collection.add(row(50, 50)).unwrap();
            }
        })
        .unwrap();
    let second_calls = calls.clone();
    let _second = collection
        .subscribe(move |revision| second_calls.borrow_mut().push((2, revision)))
        .unwrap();
    collection.remove(10).unwrap();
    assert_eq!(*calls.borrow(), [(1, 1), (2, 1), (1, 3), (2, 3)]);
    assert_eq!(collection.changes_since(0).changes.len(), 3);
}

#[test]
fn newly_added_subscriptions_start_on_the_next_delivery_round() {
    use std::{cell::RefCell, rc::Rc};
    let collection = collection();
    let weak = collection.downgrade();
    let tokens = Rc::new(RefCell::new(Vec::new()));
    let added_tokens = tokens.clone();
    let calls = Rc::new(RefCell::new(Vec::new()));
    let added_calls = calls.clone();
    let _first = collection
        .subscribe(move |revision| {
            if revision == 1 {
                let calls = added_calls.clone();
                let collection = weak.upgrade().unwrap();
                added_tokens.borrow_mut().push(
                    collection
                        .subscribe(move |revision| calls.borrow_mut().push(revision))
                        .unwrap(),
                );
                collection.add(row(40, 40)).unwrap();
            }
        })
        .unwrap();
    collection.remove(10).unwrap();
    assert_eq!(*calls.borrow(), [2]);
}

#[test]
fn callback_panic_does_not_erase_commit_or_break_other_observers() {
    use std::{cell::Cell, rc::Rc};
    let collection = collection();
    let first = collection
        .subscribe(|_| panic!("application callback"))
        .unwrap();
    let calls = Rc::new(Cell::new(0));
    let called = calls.clone();
    let _second = collection
        .subscribe(move |_| called.set(called.get() + 1))
        .unwrap();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| collection.remove(10)));
    assert!(result.is_err());
    assert_eq!(collection.revision(), 1);
    assert_eq!(calls.get(), 1);
    first.unsubscribe();
    collection.remove(i64::MIN).unwrap();
    assert_eq!(calls.get(), 2);
}

#[test]
fn self_triggering_feedback_is_bounded_and_rejected_write_is_atomic() {
    use std::{cell::Cell, rc::Rc};
    let collection = collection();
    let weak = collection.downgrade();
    let rejected = Rc::new(Cell::new(0));
    let failures = rejected.clone();
    let _token = collection
        .subscribe(move |revision| {
            let collection = weak.upgrade().unwrap();
            let result =
                collection.update_cell(10, "Count", CellValue::Integer(revision as i64 + 50));
            if let Err(error) = result {
                assert!(matches!(error, DataError::Capacity(_)));
                assert_eq!(collection.revision(), revision);
                failures.set(failures.get() + 1);
            }
        })
        .unwrap();
    collection
        .update_cell(10, "Count", CellValue::Integer(50))
        .unwrap();
    assert_eq!(
        collection.revision(),
        MAX_REENTRANT_COLLECTION_COMMITS as u64 + 1
    );
    assert_eq!(rejected.get(), 1);
}

#[test]
fn subscription_limit_is_released_on_drop() {
    let collection = collection();
    let mut subscriptions = (0..MAX_COLLECTION_SUBSCRIPTIONS)
        .map(|_| collection.subscribe(|_| {}).unwrap())
        .collect::<Vec<_>>();
    assert!(matches!(
        collection.subscribe(|_| {}),
        Err(DataError::Capacity(_))
    ));
    subscriptions.pop();
    assert!(collection.subscribe(|_| {}).is_ok());
}

#[test]
fn explicit_schema_reset_is_atomic_and_wakes_after_publication() {
    use std::{cell::Cell, rc::Rc};
    let collection = collection();
    let original = collection.snapshot();
    let schema = TableSchema::new(vec![FieldSchema::new("Other", FieldType::Bool)]).unwrap();
    let calls = Rc::new(Cell::new(0));
    let called = calls.clone();
    let weak = collection.downgrade();
    let _token = collection
        .subscribe(move |revision| {
            let collection = weak.upgrade().unwrap();
            assert_eq!(revision, collection.revision());
            assert_eq!(collection.snapshot().schema().fields()[0].name, "Other");
            called.set(called.get() + 1);
        })
        .unwrap();
    assert!(
        collection
            .replace_schema(schema.clone(), vec![row(1, 1)])
            .is_err()
    );
    assert_eq!(collection.snapshot(), original);
    assert_eq!(calls.get(), 0);
    collection
        .replace_schema(
            schema.clone(),
            vec![DataRow::new(1, vec![CellValue::Bool(true)])],
        )
        .unwrap();
    assert_eq!(calls.get(), 1);
    assert_eq!(collection.changes_since(0).changes, vec![DataChange::Reset]);
    assert_eq!(collection.snapshot().limits(), original.limits());
    assert!(
        !collection
            .replace_schema(schema, vec![DataRow::new(1, vec![CellValue::Bool(true)])])
            .unwrap()
    );
    assert_eq!(calls.get(), 1);
}

#[test]
fn generic_values_keep_stable_keys_atomic_batches_and_shared_snapshots() {
    #[derive(Debug, PartialEq)]
    struct Product {
        name: String,
        count: i64,
    }
    let product = |name: &str, count| Product {
        name: name.into(),
        count,
    };
    let list = ObservableList::new(
        vec![(i64::MIN, product("One", 1)), (i64::MAX, product("Two", 2))],
        |value: &Product| value.name.len(),
    )
    .unwrap();
    let first = list.snapshot();
    list.batch(vec![
        ListMutation::Add {
            key: 3,
            value: product("Three", 3),
        },
        ListMutation::Move { key: 3, index: 0 },
        ListMutation::Replace {
            key: i64::MAX,
            value: product("Changed", 20),
        },
    ])
    .unwrap();
    assert_eq!(list.revision(), 1);
    assert_eq!(
        list.snapshot()
            .iter()
            .map(ObservableItem::key)
            .collect::<Vec<_>>(),
        [3, i64::MIN, i64::MAX]
    );
    assert_eq!(first[1].value().name, "Two");
    assert!(std::rc::Rc::ptr_eq(
        &first[0].shared_value(),
        &list.get(i64::MIN).unwrap()
    ));
    let before = list.snapshot();
    assert!(
        list.batch(vec![
            ListMutation::Remove { key: 3 },
            ListMutation::Remove { key: 999 }
        ])
        .is_err()
    );
    assert_eq!(list.snapshot(), before);
    assert_eq!(list.revision(), 1);
    drop(list);
    assert_eq!(first[0].value().count, 1);
}

#[test]
fn generic_noop_notifications_limits_and_cursors_follow_collection_contract() {
    use std::{cell::Cell, rc::Rc};
    let list = ObservableList::with_limits(
        vec![(1, "One".to_owned())],
        ListLimits {
            max_items: 2,
            max_item_bytes: 64,
            max_bytes: 256,
        },
        String::len,
    )
    .unwrap();
    let calls = Rc::new(Cell::new(0));
    let called = calls.clone();
    let _token = list
        .subscribe(move |_| called.set(called.get() + 1))
        .unwrap();
    assert!(!list.replace(1, "One".into()).unwrap());
    assert!(
        !list
            .batch(vec![
                ListMutation::Add {
                    key: 2,
                    value: "Two".into()
                },
                ListMutation::Remove { key: 2 }
            ])
            .unwrap()
    );
    assert!(list.add(1, "Duplicate".into()).is_err());
    assert!(list.insert(2, 2, "Bad index".into()).is_err());
    assert!(list.replace(1, "x".repeat(65)).is_err());
    assert!(
        list.batch(vec![
            ListMutation::Add {
                key: 2,
                value: "Two".into()
            },
            ListMutation::Add {
                key: 3,
                value: "Three".into()
            }
        ])
        .is_err()
    );
    assert_eq!(calls.get(), 0);
    for index in 0..MAX_CHANGE_REVISIONS + 2 {
        list.replace(1, index.to_string()).unwrap();
    }
    assert_eq!(calls.get(), MAX_CHANGE_REVISIONS + 2);
    assert_eq!(list.changes_since(0).changes, [DataChange::Reset]);
    assert_eq!(list.changes_since(2).changes.len(), MAX_CHANGE_REVISIONS);
    let cursor = list.revision();
    assert!(list.changes_since(cursor).changes.is_empty());
    assert_eq!(list.changes_since(cursor + 1).changes, [DataChange::Reset]);
}

#[test]
fn generic_reset_reentrant_mutation_and_weak_lifetime() {
    let list = ObservableList::new(vec![(1, 1i64)], |_| 0).unwrap();
    let weak = list.downgrade();
    let capture = weak.clone();
    let token = list
        .subscribe(move |revision| {
            let list = capture.upgrade().unwrap();
            assert!(list.get(2).is_some());
            if revision == 1 {
                list.add(3, 3).unwrap();
            }
        })
        .unwrap();
    list.reset(vec![(2, 2)]).unwrap();
    assert_eq!(list.revision(), 2);
    assert_eq!(
        list.changes_since(1).changes,
        [DataChange::Add { key: 3, index: 1 }]
    );
    drop(list);
    assert!(weak.upgrade().is_none());
    assert!(!token.is_active());
}

#[test]
fn generic_retained_byte_limits_and_oversized_measurements_fail_atomically() {
    let list = ObservableList::with_limits(
        vec![(1, "One".to_owned())],
        ListLimits {
            max_items: 3,
            max_item_bytes: 64,
            max_bytes: 70,
        },
        String::len,
    )
    .unwrap();
    let original = list.snapshot();
    assert!(list.add(2, "Two".into()).is_err());
    assert_eq!(list.snapshot(), original);
    assert_eq!(list.revision(), 0);
    assert!(ObservableList::new(vec![(1, 1u8)], |_| usize::MAX).is_err());
    assert!(ObservableList::new(vec![(1, 1), (1, 2)], |_| 0).is_err());
    assert!(
        ObservableList::<i32>::with_limits(
            vec![],
            ListLimits {
                max_items: usize::MAX,
                ..ListLimits::default()
            },
            |_| 0
        )
        .is_err()
    );
}

#[test]
fn generic_value_comparison_and_destruction_run_outside_state_borrows() {
    use std::{cell::RefCell, rc::Rc};
    type Owner = Rc<RefCell<Option<WeakObservableList<Value>>>>;
    struct Value {
        number: i32,
        owner: Owner,
        dropped: Rc<RefCell<Vec<i32>>>,
    }
    impl PartialEq for Value {
        fn eq(&self, other: &Self) -> bool {
            if let Some(owner) = self
                .owner
                .borrow()
                .as_ref()
                .and_then(WeakObservableList::upgrade)
            {
                let _snapshot = owner.snapshot();
            }
            self.number == other.number
        }
    }
    impl Drop for Value {
        fn drop(&mut self) {
            if let Some(owner) = self
                .owner
                .borrow()
                .as_ref()
                .and_then(WeakObservableList::upgrade)
            {
                let _snapshot = owner.snapshot();
            }
            self.dropped.borrow_mut().push(self.number);
        }
    }
    let owner: Owner = Rc::new(RefCell::new(None));
    let dropped = Rc::new(RefCell::new(Vec::new()));
    let value = |number| Value {
        number,
        owner: owner.clone(),
        dropped: dropped.clone(),
    };
    let list = ObservableList::new(vec![(1, value(1))], |_| 0).unwrap();
    *owner.borrow_mut() = Some(list.downgrade());
    list.replace(1, value(2)).unwrap();
    assert_eq!(*dropped.borrow(), [1]);
    assert!(
        list.batch(vec![
            ListMutation::Add {
                key: 2,
                value: value(3)
            },
            ListMutation::Remove { key: 999 }
        ])
        .is_err()
    );
    assert_eq!(*dropped.borrow(), [1, 3]);
    list.clear().unwrap();
    assert_eq!(*dropped.borrow(), [1, 3, 2]);
}

#[test]
fn generic_comparison_side_effect_cannot_overwrite_newer_revision() {
    use std::{
        cell::{Cell, RefCell},
        rc::Rc,
    };
    type Owner = Rc<RefCell<Option<WeakObservableList<Value>>>>;
    struct Value {
        number: i32,
        armed: Rc<Cell<bool>>,
        owner: Owner,
    }
    impl PartialEq for Value {
        fn eq(&self, other: &Self) -> bool {
            if self.armed.replace(false) {
                let list = self.owner.borrow().as_ref().unwrap().upgrade().unwrap();
                list.add(
                    2,
                    Self {
                        number: 3,
                        armed: self.armed.clone(),
                        owner: self.owner.clone(),
                    },
                )
                .unwrap();
            }
            self.number == other.number
        }
    }
    let owner: Owner = Rc::new(RefCell::new(None));
    let armed = Rc::new(Cell::new(false));
    let value = |number| Value {
        number,
        armed: armed.clone(),
        owner: owner.clone(),
    };
    let list = ObservableList::new(vec![(1, value(1))], |_| 0).unwrap();
    *owner.borrow_mut() = Some(list.downgrade());
    armed.set(true);
    assert!(matches!(
        list.replace(1, value(2)),
        Err(DataError::Conflict(_))
    ));
    assert_eq!(list.get(1).unwrap().number, 1);
    assert_eq!(list.get(2).unwrap().number, 3);
    assert_eq!(list.revision(), 1);
}

#[test]
fn binary_payload_limits_are_checked_before_any_publication() {
    use std::{cell::Cell, rc::Rc};
    let collection = ObservableCollection::new(
        DataTable::with_limits(
            TableSchema::new(vec![FieldSchema::new("Bytes", FieldType::Binary)]).unwrap(),
            vec![DataRow::new(1, vec![CellValue::Binary(vec![1])])],
            DataLimits {
                max_binary_bytes: 4,
                ..DataLimits::default()
            },
        )
        .unwrap(),
    );
    let calls = Rc::new(Cell::new(0));
    let called = calls.clone();
    let _token = collection
        .subscribe(move |_| called.set(called.get() + 1))
        .unwrap();
    assert!(
        collection
            .batch(vec![
                CollectionMutation::UpdateCell {
                    key: 1,
                    field: "Bytes".into(),
                    value: CellValue::Binary(vec![2])
                },
                CollectionMutation::UpdateCell {
                    key: 1,
                    field: "Bytes".into(),
                    value: CellValue::Binary(vec![0; 5])
                },
            ])
            .is_err()
    );
    assert_eq!(
        collection.snapshot().value(1, "Bytes"),
        Some(&CellValue::Binary(vec![1]))
    );
    assert_eq!(calls.get(), 0);
    collection
        .update_cell(1, "Bytes", CellValue::Binary(vec![2; 4]))
        .unwrap();
    assert_eq!(calls.get(), 1);
}

#[test]
fn reset_schema_respects_receiving_limits_without_partial_wake() {
    use std::{cell::Cell, rc::Rc};
    let original = collection().snapshot();
    let collection = ObservableCollection::new(
        DataTable::with_limits(
            original.schema().clone(),
            original.rows().to_vec(),
            DataLimits {
                max_rows: 3,
                max_fields: 2,
                max_binary_bytes: 4,
                ..DataLimits::default()
            },
        )
        .unwrap(),
    );
    let before = collection.snapshot();
    let count = Rc::new(Cell::new(0));
    let counter = count.clone();
    let _token = collection
        .subscribe(move |_| counter.set(counter.get() + 1))
        .unwrap();
    let binary = TableSchema::new(vec![FieldSchema::new("Payload", FieldType::Binary)]).unwrap();
    assert!(
        collection
            .replace_schema(
                binary.clone(),
                vec![DataRow::new(1, vec![CellValue::Binary(vec![0; 5])])]
            )
            .is_err()
    );
    assert!(
        collection
            .replace_schema(
                binary.clone(),
                (0..4)
                    .map(|key| DataRow::new(key, vec![CellValue::Binary(vec![])]))
                    .collect()
            )
            .is_err()
    );
    assert!(
        collection
            .replace_schema(
                TableSchema::new(vec![
                    FieldSchema::new("A", FieldType::Bool),
                    FieldSchema::new("B", FieldType::Bool),
                    FieldSchema::new("C", FieldType::Bool)
                ])
                .unwrap(),
                vec![]
            )
            .is_err()
    );
    assert_eq!(collection.snapshot(), before);
    assert_eq!(collection.revision(), 0);
    assert_eq!(count.get(), 0);
    collection
        .replace_schema(
            binary,
            vec![DataRow::new(1, vec![CellValue::Binary(vec![0; 4])])],
        )
        .unwrap();
    assert_eq!(count.get(), 1);
    assert_eq!(collection.snapshot().limits(), before.limits());
}

#[test]
fn generic_move_clear_and_reset_preserve_snapshot_and_noop_semantics() {
    let list = ObservableList::new(vec![(i64::MIN, 1), (0, 2), (i64::MAX, 3)], |_| 0).unwrap();
    let before = list.snapshot();
    assert!(!list.move_item(0, 1).unwrap());
    assert!(list.move_item(0, 3).is_err());
    assert!(list.move_item(999, 0).is_err());
    assert!(list.reset(vec![(2, 1), (2, 2)]).is_err());
    assert!(
        !list
            .batch(vec![
                ListMutation::Clear,
                ListMutation::Reset(vec![(i64::MIN, 1), (0, 2), (i64::MAX, 3)])
            ])
            .unwrap()
    );
    assert_eq!(list.revision(), 0);
    list.move_item(i64::MAX, 0).unwrap();
    assert_eq!(
        list.snapshot()
            .iter()
            .map(ObservableItem::key)
            .collect::<Vec<_>>(),
        [i64::MAX, i64::MIN, 0]
    );
    assert_eq!(
        list.changes_since(0).changes,
        [DataChange::Move {
            key: i64::MAX,
            from: 2,
            to: 0
        }]
    );
    list.clear().unwrap();
    assert!(!list.clear().unwrap());
    assert_eq!(list.revision(), 2);
    assert_eq!(
        before.iter().map(ObservableItem::key).collect::<Vec<_>>(),
        [i64::MIN, 0, i64::MAX]
    );
    assert!(list.is_empty());
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
