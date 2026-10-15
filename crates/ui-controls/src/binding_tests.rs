use super::*;

fn model() -> BindingContext {
    BindingContext::new(DataValue::object([
        (
            "Customer",
            DataValue::object([("Name", "Anna".into()), ("Active", true.into())]),
        ),
        (
            "Rows",
            DataValue::List(vec![DataValue::object([("Amount", 5.0.into())])]),
        ),
        ("Minimum", 0.0.into()),
        ("Maximum", 10.0.into()),
    ]))
    .unwrap()
}

#[test]
fn paths_are_structural_and_canonical() {
    let field = BindingPath::parse("Rows[01].Amount").unwrap();
    assert_eq!(field.as_str(), "Rows[1].Amount");
    assert!(BindingPath::parse("Rows").unwrap().contains(&field));
    assert!(!BindingPath::parse("Row").unwrap().overlaps(&field));
    assert!(BindingPath::parse(".").unwrap().contains(&field));
    assert_eq!(
        BindingPath::parse("Данные.Имя").unwrap().segments().len(),
        2
    );
    for invalid in [
        "A.",
        "A..B",
        "A[-1]",
        "A[1",
        "A[]",
        "A[ 1]",
        "A.[0]",
        "A B",
        "1A",
        "A.B()",
        "A[0]B",
        " A",
        "A[184467440737095516160]",
    ] {
        assert!(BindingPath::parse(invalid).is_err(), "{invalid}");
    }
    assert!(BindingPath::parse(&vec!["A"; MAX_BINDING_DEPTH + 1].join(".")).is_err());
}

#[test]
fn updates_preserve_siblings_and_track_parent_and_child_dependencies() {
    let mut data = model();
    assert!(data.set("Rows[0].Amount", 8.0.into()).unwrap());
    assert_eq!(data.get("Customer.Name").unwrap().as_str(), Some("Anna"));
    let changes = data.changes_since(0).unwrap();
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].revision, 1);
    assert!(changes[0].affects(&BindingPath::parse("Rows").unwrap()));
    assert!(changes[0].affects(&BindingPath::parse("Rows[0].Amount").unwrap()));
    assert!(!changes[0].affects(&BindingPath::parse("Customer").unwrap()));
}

#[test]
fn no_op_and_round_trip_batch_do_not_notify() {
    let mut data = model();
    assert!(!data.set("Customer.Name", "Anna".into()).unwrap());
    assert!(
        !data
            .set_many([("Minimum", 8.0.into()), ("Minimum", 0.0.into())])
            .unwrap()
    );
    assert_eq!(data.revision(), 0);
    assert!(data.changes_since(0).unwrap().is_empty());
}

#[test]
fn missing_path_rolls_back_whole_batch() {
    let mut data = model();
    let original = data.root().clone();
    assert!(matches!(
        data.set_many([
            ("Customer.Name", "Changed".into()),
            ("Rows[1].Amount", 9.0.into())
        ]),
        Err(BindingError::MissingPath(_))
    ));
    assert_eq!(data.root(), &original);
    assert_eq!(data.revision(), 0);
    assert!(data.set("NewField", DataValue::Null).is_err());
}

#[test]
fn replacements_add_remove_and_reorder_collections() {
    let mut data = model();
    let replacement = DataValue::List(vec![
        DataValue::object([("Amount", 42.0.into())]),
        DataValue::object([("Amount", 3.0.into())]),
    ]);
    assert!(
        data.set_many([("Rows", replacement), ("Rows[1].Amount", 4.0.into())])
            .unwrap()
    );
    assert_eq!(data.get("Rows[1].Amount").unwrap().as_number(), Some(4.0));
    let changes = data.changes_since(0).unwrap();
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].paths, [BindingPath::parse("Rows").unwrap()]);
    assert!(data.set("Rows", DataValue::List(Vec::new())).unwrap());
    assert!(data.get("Rows[0]").is_err());
}

#[test]
fn schema_is_atomic_and_diagnostics_identify_nested_values() {
    let mut data = BindingContext::new(DataValue::object([
        ("Name", DataValue::Null),
        ("Rows", DataValue::List(vec![1.0.into()])),
    ]))
    .unwrap();
    let schema = ValueSchema::object([
        ("Name", ValueSchema::Nullable(Box::new(ValueSchema::String))),
        ("Rows", ValueSchema::List(Box::new(ValueSchema::Number))),
    ]);
    data.set_schema(schema.clone()).unwrap();
    data.set("Name", "Anna".into()).unwrap();
    assert!(
        matches!(data.set_many([("Name", "Changed".into()), ("Rows[0]", false.into())]), Err(BindingError::Schema { path, .. }) if path == "Rows[0]")
    );
    assert_eq!(data.get("Name").unwrap().as_str(), Some("Anna"));
    assert!(data.set_schema(ValueSchema::Bool).is_err());
    assert_eq!(data.schema(), &schema);
    assert_eq!(data.revision(), 1);
    assert!(
        data.set(".", DataValue::object([("Name", "Anna".into())]))
            .is_err()
    );
}

#[test]
fn cross_field_validation_observes_only_complete_atomic_batch() {
    let mut data = model();
    data.set_validator(Some(BindingValidator::new(|root| {
        let fields = root.as_object().unwrap();
        if fields["Minimum"].as_number().unwrap() <= fields["Maximum"].as_number().unwrap() {
            Ok(())
        } else {
            Err("Minimum must not exceed Maximum".into())
        }
    })))
    .unwrap();
    assert!(data.set("Minimum", 20.0.into()).is_err());
    assert_eq!(data.revision(), 0);
    assert!(
        data.set_many([("Minimum", 20.0.into()), ("Maximum", 30.0.into())])
            .unwrap()
    );
    assert_eq!(data.revision(), 1);
    assert!(
        data.set_validator(Some(BindingValidator::new(|_| Err("Rejected".into()))))
            .is_err()
    );
    assert!(data.set("Minimum", 31.0.into()).is_err());
}

#[test]
fn preflight_uses_same_rules_without_changing_history() {
    let mut data = model();
    data.set_schema(ValueSchema::object([
        ("Customer", ValueSchema::Any),
        ("Rows", ValueSchema::Any),
        ("Minimum", ValueSchema::Number),
        ("Maximum", ValueSchema::Number),
    ]))
    .unwrap();
    assert!(data.validate_set("Minimum", &"wrong".into()).is_err());
    data.validate_set("Minimum", &42.0.into()).unwrap();
    data.validate_many([("Minimum", &42.0.into()), ("Maximum", &56.0.into())])
        .unwrap();
    assert_eq!(data.revision(), 0);
    assert_eq!(data.get("Minimum").unwrap().as_number(), Some(0.0));
}

#[test]
fn field_validation_survives_parent_replacement_and_registration_is_atomic() {
    let mut data = model();
    data.set_field_validator(
        "Customer.Name",
        Some(BindingValidator::new(|value| {
            if value.as_str().is_some_and(|text| !text.is_empty()) {
                Ok(())
            } else {
                Err("A name is required".into())
            }
        })),
    )
    .unwrap();
    assert!(data.validate_set("Customer.Name", &"".into()).is_err());
    assert!(
        data.set(
            "Customer",
            DataValue::object([("Name", "".into()), ("Active", true.into())])
        )
        .is_err()
    );
    assert!(
        data.set_field_validator(
            "Customer.Name",
            Some(BindingValidator::new(|_| Err("Reject all".into())))
        )
        .is_err()
    );
    data.set("Customer.Name", "Boris".into()).unwrap();
    assert!(
        data.set("Customer", DataValue::object([("Active", true.into())]))
            .is_err()
    );
    data.set_field_validator("Customer.Name", None).unwrap();
    data.set("Customer", DataValue::object([("Active", true.into())]))
        .unwrap();
}

#[test]
fn non_finite_numbers_and_root_type_are_rejected_before_publication() {
    let mut data = model();
    for number in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(data.set("Minimum", DataValue::Number(number)).is_err());
        assert!(
            BindingContext::new(DataValue::object([("N", DataValue::Number(number))])).is_err()
        );
    }
    assert!(data.set("", DataValue::List(Vec::new())).is_err());
    assert!(BindingContext::new(DataValue::Null).is_err());
    assert_eq!(data.revision(), 0);
}

#[test]
fn representation_limits_apply_to_updates_and_schemas() {
    let mut value = DataValue::Null;
    let mut schema = ValueSchema::Any;
    for _ in 0..=MAX_BINDING_DEPTH {
        value = DataValue::List(vec![value]);
        schema = ValueSchema::Nullable(Box::new(schema));
    }
    assert!(value.validate().is_err());
    assert!(schema.validate(&DataValue::Null).is_err());
    assert!(
        DataValue::List(vec![DataValue::Null; MAX_BINDING_NODES])
            .validate()
            .is_err()
    );
    assert!(
        DataValue::String("x".repeat(MAX_BINDING_BYTES + 1))
            .validate()
            .is_err()
    );
    let mut data = model();
    assert!(
        data.set_many((0..=MAX_BINDING_UPDATES).map(|_| ("Minimum", 1.0.into())))
            .is_err()
    );
    assert_eq!(data.revision(), 0);
}

#[test]
fn independent_subscribers_recover_when_history_is_evicted_or_drained() {
    let mut data = model();
    for value in 1..=MAX_BINDING_HISTORY + 1 {
        data.set("Minimum", (value as f64).into()).unwrap();
    }
    assert!(data.changes_since(0).is_none());
    assert_eq!(data.changes_since(1).unwrap().len(), MAX_BINDING_HISTORY);
    assert_eq!(data.changes_since(1).unwrap().len(), MAX_BINDING_HISTORY);
    let current = data.revision();
    assert!(data.changes_since(current + 1).is_none());
    assert_eq!(data.drain_changes().len(), MAX_BINDING_HISTORY);
    assert!(data.changes_since(current - 1).is_none());
    assert!(data.changes_since(current).unwrap().is_empty());
}

#[test]
fn root_replacement_is_one_notification_and_invalidates_all_descendants() {
    let mut data = model();
    data.set("", DataValue::object([("Other", true.into())]))
        .unwrap();
    let change = data.changes_since(0).unwrap().remove(0);
    assert!(change.affects(&BindingPath::parse("Customer.Name").unwrap()));
    assert!(data.get("Customer.Name").is_err());
    assert_eq!(data.get("Other").unwrap().as_bool(), Some(true));
}

#[test]
fn converters_are_bidirectional_and_share_callbacks_when_cloned() {
    let mut registry = BindingConverterRegistry::default();
    registry
        .register(
            "NumberText",
            BindingConverter::new(|value| {
                value
                    .as_number()
                    .map(|number| number.to_string().into())
                    .ok_or("Expected number".into())
            })
            .with_reverse(|value| {
                value
                    .as_str()
                    .and_then(|text| text.parse::<f64>().ok())
                    .map(DataValue::Number)
                    .ok_or("Expected numeric text".into())
            }),
        )
        .unwrap();
    assert_eq!(
        registry.convert("NumberText", &42.0.into()).unwrap(),
        "42".into()
    );
    assert_eq!(
        registry
            .clone()
            .convert_back("NumberText", &"12.5".into())
            .unwrap(),
        12.5.into()
    );
    assert!(registry.convert_back("NumberText", &"NaN".into()).is_err());
    assert!(registry.convert_back("NumberText", &"bad".into()).is_err());
    assert!(registry.supports_reverse("NumberText"));
}

#[test]
fn converter_failures_are_explicit_and_invalid_registration_is_atomic() {
    let mut registry = BindingConverterRegistry::default();
    registry
        .register("Display", BindingConverter::new(|_| Ok("Shown".into())))
        .unwrap();
    assert!(matches!(
        registry.convert("Missing", &DataValue::Null),
        Err(BindingError::UnknownConverter(_))
    ));
    assert!(matches!(
        registry.convert_back("Display", &DataValue::Null),
        Err(BindingError::MissingReverseConverter(_))
    ));
    assert!(
        registry
            .register(" invalid", BindingConverter::new(|_| Ok(DataValue::Null)))
            .is_err()
    );
    assert_eq!(
        registry.convert("Display", &DataValue::Null).unwrap(),
        "Shown".into()
    );
    registry
        .register("Fail", BindingConverter::new(|_| Err("я".repeat(3000))))
        .unwrap();
    assert!(
        matches!(registry.convert("Fail", &DataValue::Null), Err(BindingError::Conversion { message, .. }) if message.len() == 4096)
    );
}
