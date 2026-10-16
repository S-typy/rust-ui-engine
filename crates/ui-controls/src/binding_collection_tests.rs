use super::*;
use rust_desktop_ui_data::{CollectionMutation, DataRow, FieldSchema, FieldType, TableSchema};

fn collection(rows: &[(i64, &str)]) -> ObservableCollection {
    ObservableCollection::new(
        DataTable::new(
            TableSchema::new(vec![FieldSchema::new("Name", FieldType::Text)]).unwrap(),
            rows.iter()
                .map(|(key, text)| DataRow::new(*key, vec![CellValue::Text((*text).into())]))
                .collect(),
        )
        .unwrap(),
    )
}

fn context() -> BindingContext {
    BindingContext::new(DataValue::object([
        ("Rows", DataValue::List(vec![])),
        ("Other", DataValue::List(vec![])),
        ("Number", 0.0.into()),
    ]))
    .unwrap()
}

#[test]
fn exact_integer_keys_and_fields_preserve_types_and_shared_handle() {
    let mut nullable = FieldSchema::new("Missing", FieldType::Text);
    nullable.nullable = true;
    let table = DataTable::new(
        TableSchema::new(vec![
            FieldSchema::new("Count", FieldType::Integer),
            FieldSchema::new("Active", FieldType::Bool),
            FieldSchema::new("Price", FieldType::Real),
            nullable,
        ])
        .unwrap(),
        vec![DataRow::new(
            i64::MAX,
            vec![
                CellValue::Integer(i64::MIN),
                CellValue::Bool(true),
                CellValue::Real(1.25),
                CellValue::Null,
            ],
        )],
    )
    .unwrap();
    let source = ObservableCollection::new(table);
    let mut model = context();
    model
        .register_collection("Rows", source.clone(), "Id")
        .unwrap();
    assert_eq!(
        model.get("Rows[0].Id").unwrap().as_str(),
        Some("9223372036854775807")
    );
    assert_eq!(
        model.get("Rows[0].Count").unwrap().as_str(),
        Some("-9223372036854775808")
    );
    assert_eq!(model.get("Rows[0].Active").unwrap(), &DataValue::Bool(true));
    assert_eq!(
        model.get("Rows[0].Price").unwrap(),
        &DataValue::Number(1.25)
    );
    assert_eq!(model.get("Rows[0].Missing").unwrap(), &DataValue::Null);
    let handle = model.observable_collection("Rows").unwrap();
    handle
        .update_cell(i64::MAX, "Count", CellValue::Integer(3))
        .unwrap();
    assert_eq!(source.revision(), 1);
    assert!(model.synchronize_collections().unwrap());
    assert_eq!(model.get("Rows[0].Count").unwrap().as_str(), Some("3"));
    assert!(!model.synchronize_collections().unwrap());
}

#[test]
fn registered_ancestor_descendant_writes_reject_atomically_and_unregister_releases() {
    let source = collection(&[(1, "A")]);
    let mut model = BindingContext::new(DataValue::object([
        (
            "Group",
            DataValue::object([("Rows", DataValue::List(vec![]))]),
        ),
        ("Other", 0.0.into()),
    ]))
    .unwrap();
    model
        .register_collection("Group.Rows", source.clone(), "Id")
        .unwrap();
    let before = model.root().clone();
    for path in [
        ".",
        "Group",
        "Group.Rows",
        "Group.Rows[0]",
        "Group.Rows[0].Name",
    ] {
        let value = model.get(path).unwrap().clone();
        assert!(model.set(path, value.clone()).is_err(), "{path}");
        assert!(model.validate_set(path, &value).is_err(), "{path}");
    }
    assert!(
        model
            .set_many([("Other", 2.0.into()), ("Group.Rows[0].Name", "bad".into())])
            .is_err()
    );
    assert_eq!(model.root(), &before);
    assert!(
        model
            .register_collection("Group.Rows", source, "Id")
            .is_err()
    );
    assert!(model.unregister_collection("Group.Rows").unwrap());
    assert!(!model.unregister_collection("Group.Rows").unwrap());
    model.set("Group.Rows[0].Name", "ordinary".into()).unwrap();
    assert!(model.observable_collection("Group.Rows").is_none());
}

#[test]
fn registration_failures_never_install_a_handle_or_change_the_model() {
    let mut model = context();
    let before = model.root().clone();
    for (path, key) in [
        (".", "Id"),
        ("Missing", "Id"),
        ("Number", "Id"),
        ("Rows", "Name"),
        ("Rows", "x.y"),
        ("Rows", "[0]"),
    ] {
        assert!(
            model
                .register_collection(path, collection(&[(1, "A")]), key)
                .is_err(),
            "{path}/{key}"
        );
        assert_eq!(model.root(), &before);
        assert!(model.observable_collection("Rows").is_none());
    }
    model
        .set_field_validator(
            "Rows",
            Some(BindingValidator::new(|value| {
                if value.as_list().unwrap().is_empty() {
                    Ok(())
                } else {
                    Err("must stay empty".into())
                }
            })),
        )
        .unwrap();
    assert!(
        model
            .register_collection("Rows", collection(&[(1, "A")]), "Id")
            .is_err()
    );
    assert_eq!(model.revision(), 0);
    assert!(model.observable_collection("Rows").is_none());
}

#[test]
fn cell_batch_publishes_one_revision_with_only_changed_paths_and_noop_suppression() {
    let source = collection(&[(10, "A"), (20, "B")]);
    let mut model = context();
    model
        .register_collection("Rows", source.clone(), "Id")
        .unwrap();
    let revision = model.revision();
    source
        .batch(vec![
            CollectionMutation::UpdateCell {
                key: 20,
                field: "Name".into(),
                value: CellValue::Text("B1".into()),
            },
            CollectionMutation::UpdateCell {
                key: 20,
                field: "Name".into(),
                value: CellValue::Text("B2".into()),
            },
            CollectionMutation::UpdateCell {
                key: 10,
                field: "Name".into(),
                value: CellValue::Text("A1".into()),
            },
            CollectionMutation::UpdateCell {
                key: 10,
                field: "Name".into(),
                value: CellValue::Text("A".into()),
            },
        ])
        .unwrap();
    assert!(model.synchronize_collections().unwrap());
    assert_eq!(model.revision(), revision + 1);
    let changes = model.changes_since(revision).unwrap();
    assert_eq!(changes.len(), 1);
    assert_eq!(
        changes[0].paths,
        vec![BindingPath::parse("Rows[1].Name").unwrap()]
    );
    assert!(!model.synchronize_collections().unwrap());
}

#[test]
fn refresh_failure_is_atomic_across_collections_and_retries_same_revisions() {
    let first = collection(&[(1, "A")]);
    let second = collection(&[(2, "B")]);
    let mut model = context();
    model
        .register_collection("Rows", first.clone(), "Id")
        .unwrap();
    model
        .register_collection("Other", second.clone(), "Id")
        .unwrap();
    model
        .set_field_validator(
            "Other",
            Some(BindingValidator::new(|value| {
                if value.as_list().unwrap()[0].as_object().unwrap()["Name"].as_str() == Some("bad")
                {
                    Err("rejected".into())
                } else {
                    Ok(())
                }
            })),
        )
        .unwrap();
    let before = model.root().clone();
    let revision = model.revision();
    first
        .update_cell(1, "Name", CellValue::Text("A2".into()))
        .unwrap();
    second
        .update_cell(2, "Name", CellValue::Text("bad".into()))
        .unwrap();
    for _ in 0..2 {
        assert!(model.synchronize_collections().is_err());
    }
    assert_eq!(model.root(), &before);
    assert_eq!(model.revision(), revision);
    model.set_field_validator("Other", None).unwrap();
    assert!(model.synchronize_collections().unwrap());
    assert_eq!(model.revision(), revision + 1);
    assert_eq!(model.get("Rows[0].Name").unwrap().as_str(), Some("A2"));
    assert_eq!(model.get("Other[0].Name").unwrap().as_str(), Some("bad"));
}

#[test]
fn structural_and_expired_journal_updates_refresh_final_order_and_keys() {
    let source = collection(&[(1, "A"), (2, "B")]);
    let mut model = context();
    model
        .register_collection("Rows", source.clone(), "Id")
        .unwrap();
    source
        .batch(vec![
            CollectionMutation::Move { key: 2, index: 0 },
            CollectionMutation::Remove { key: 1 },
            CollectionMutation::Add(DataRow::new(i64::MIN, vec![CellValue::Text("C".into())])),
        ])
        .unwrap();
    let revision = model.revision();
    model.synchronize_collections().unwrap();
    assert_eq!(model.get("Rows[0].Id").unwrap().as_str(), Some("2"));
    assert_eq!(
        model.get("Rows[1].Id").unwrap().as_str(),
        Some("-9223372036854775808")
    );
    assert_eq!(
        model.changes_since(revision).unwrap()[0].paths,
        vec![BindingPath::parse("Rows").unwrap()]
    );
    for n in 0..300 {
        source
            .update_cell(2, "Name", CellValue::Text(n.to_string()))
            .unwrap();
    }
    let revision = model.revision();
    model.synchronize_collections().unwrap();
    assert_eq!(model.get("Rows[0].Name").unwrap().as_str(), Some("299"));
    assert_eq!(
        model.changes_since(revision).unwrap()[0].paths,
        vec![BindingPath::parse("Rows").unwrap()]
    );
}

#[test]
fn byte_limit_failure_keeps_projection_and_cursor_until_collection_is_repaired() {
    let source = collection(&[(1, "A")]);
    let mut model = context();
    model
        .register_collection("Rows", source.clone(), "Id")
        .unwrap();
    let before = model.root().clone();
    let oversized: Vec<_> = (0..5)
        .map(|key| DataRow::new(key, vec![CellValue::Text("x".repeat(1024 * 1024))]))
        .collect();
    source.reset(oversized).unwrap();
    assert!(model.synchronize_collections().is_err());
    assert_eq!(model.root(), &before);
    source
        .reset(vec![DataRow::new(3, vec![CellValue::Text("fixed".into())])])
        .unwrap();
    assert!(model.synchronize_collections().unwrap());
    assert_eq!(model.get("Rows[0].Name").unwrap().as_str(), Some("fixed"));
}

#[test]
fn field_names_outside_binding_grammar_use_atomic_full_projection() {
    let source = ObservableCollection::new(
        DataTable::new(
            TableSchema::new(vec![FieldSchema::new("a.b", FieldType::Text)]).unwrap(),
            vec![DataRow::new(1, vec![CellValue::Text("A".into())])],
        )
        .unwrap(),
    );
    let mut model = context();
    model
        .register_collection("Rows", source.clone(), "Id")
        .unwrap();
    let revision = model.revision();
    source
        .update_cell(1, "a.b", CellValue::Text("B".into()))
        .unwrap();
    assert!(model.synchronize_collections().unwrap());
    assert_eq!(
        model.get("Rows[0]").unwrap().as_object().unwrap()["a.b"],
        DataValue::String("B".into())
    );
    assert_eq!(
        model.changes_since(revision).unwrap()[0].paths,
        vec![BindingPath::parse("Rows").unwrap()]
    );
}

#[test]
fn aggregate_projection_budget_bounds_pending_payloads_before_publication() {
    let first = collection(&[(1, "A")]);
    let second = collection(&[(2, "B")]);
    let mut model = context();
    model
        .register_collection("Rows", first.clone(), "Id")
        .unwrap();
    model
        .register_collection("Other", second.clone(), "Id")
        .unwrap();
    let before = model.root().clone();
    let revision = model.revision();
    let expanded = || {
        (0..3)
            .map(|key| DataRow::new(key, vec![CellValue::Text("x".repeat(750_000))]))
            .collect()
    };
    first.reset(expanded()).unwrap();
    second.reset(expanded()).unwrap();
    // Each source fits individually, but retaining all projected update
    // payloads would exceed the final context's four-megabyte budget.
    for _ in 0..2 {
        assert!(
            model
                .synchronize_collections()
                .unwrap_err()
                .to_string()
                .contains("Combined collection projections")
        );
    }
    assert_eq!(model.root(), &before);
    assert_eq!(model.revision(), revision);
    first
        .reset(vec![DataRow::new(1, vec![CellValue::Text("fixed".into())])])
        .unwrap();
    assert!(model.synchronize_collections().unwrap());
    assert_eq!(model.get("Rows[0].Name").unwrap().as_str(), Some("fixed"));
    assert_eq!(model.get("Other").unwrap().as_list().unwrap().len(), 3);
}

#[test]
fn too_many_precise_cell_updates_fall_back_to_bounded_full_list_publication() {
    let source = ObservableCollection::new(
        DataTable::new(
            TableSchema::new(vec![FieldSchema::new("Name", FieldType::Text)]).unwrap(),
            (0..1100)
                .map(|key| DataRow::new(key, vec![CellValue::Text("old".into())]))
                .collect(),
        )
        .unwrap(),
    );
    let mut model = context();
    model
        .register_collection("Rows", source.clone(), "Id")
        .unwrap();
    for keys in [0..600, 600..1100] {
        source
            .batch(
                keys.map(|key| CollectionMutation::UpdateCell {
                    key,
                    field: "Name".into(),
                    value: CellValue::Text("new".into()),
                })
                .collect(),
            )
            .unwrap();
    }
    let revision = model.revision();
    assert!(model.synchronize_collections().unwrap());
    assert_eq!(model.get("Rows[1099].Name").unwrap().as_str(), Some("new"));
    assert_eq!(
        model.changes_since(revision).unwrap()[0].paths,
        vec![BindingPath::parse("Rows").unwrap()]
    );
}
