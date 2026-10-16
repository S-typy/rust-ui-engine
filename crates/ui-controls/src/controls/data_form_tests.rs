use super::*;
use rust_desktop_ui_data::{
    CellValue, CollectionAdapter, DataRow, DataSource, DataTable, FieldSchema, FieldType,
    TableSchema,
};
use std::{cell::RefCell, rc::Rc};

fn fixture() -> (Controls, DataForm, DataSourceHandle) {
    let mut name = FieldSchema::new("name", FieldType::Text);
    name.nullable = true;
    let mut active = FieldSchema::new("active", FieldType::Bool);
    active.nullable = true;
    let schema = TableSchema::new(vec![
        name,
        FieldSchema::new("count", FieldType::Integer),
        active,
    ])
    .unwrap();
    let source = Rc::new(RefCell::new(
        DataSource::new(CollectionAdapter::new(
            DataTable::new(
                schema,
                vec![DataRow::new(
                    7,
                    vec![
                        CellValue::Null,
                        CellValue::Integer(i64::MAX),
                        CellValue::Null,
                    ],
                )],
            )
            .unwrap(),
        ))
        .unwrap(),
    ));
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let root = ui.root();
    let form = DataForm::mount(
        &mut ui,
        root,
        source.clone(),
        7,
        DataFormSettings::default(),
        &DataFormFactories::default(),
        LayoutStyle {
            width: Length::Px(640.0),
            ..Default::default()
        },
    )
    .unwrap();
    (ui, form, source)
}
#[test]
fn metadata_exact_integer_and_nullable_values_remain_distinct() {
    let (mut ui, mut form, source) = fixture();
    assert_eq!(
        form.value(&ui, "count").unwrap(),
        CellValue::Integer(i64::MAX)
    );
    assert_eq!(form.value(&ui, "name").unwrap(), CellValue::Null);
    assert_eq!(form.value(&ui, "active").unwrap(), CellValue::Null);
    form.set_field_value(&mut ui, "count", CellValue::Integer(i64::MIN))
        .unwrap();
    form.set_field_value(&mut ui, "name", CellValue::Text(String::new()))
        .unwrap();
    form.set_field_value(&mut ui, "active", CellValue::Bool(false))
        .unwrap();
    assert_eq!(form.commit(&mut ui).unwrap(), DataFormCommit::Saved);
    assert_eq!(
        source.borrow().value(7, "count"),
        Some(&CellValue::Integer(i64::MIN))
    );
    assert_eq!(
        source.borrow().value(7, "name"),
        Some(&CellValue::Text(String::new()))
    );
    assert_eq!(
        source.borrow().value(7, "active"),
        Some(&CellValue::Bool(false))
    );
    form.set_field_null(&mut ui, "name").unwrap();
    assert_eq!(form.commit(&mut ui).unwrap(), DataFormCommit::Saved);
    assert_eq!(source.borrow().value(7, "name"), Some(&CellValue::Null));
}

#[test]
fn validation_and_conflict_never_partially_write_or_erase_drafts() {
    let (mut ui, mut form, source) = fixture();
    form.set_field_value(&mut ui, "name", CellValue::Text("Draft".into()))
        .unwrap();
    let count = form.field_editor("count").unwrap();
    ui.set_text(count, "not an integer").unwrap();
    assert!(matches!(
        form.commit(&mut ui).unwrap(),
        DataFormCommit::Invalid(_)
    ));
    assert_eq!(source.borrow().value(7, "name"), Some(&CellValue::Null));
    assert_eq!(ui.document(count).unwrap().text(), "not an integer");
    ui.set_text(count, "42").unwrap();
    source
        .borrow_mut()
        .set_cell(7, "count", CellValue::Integer(11))
        .unwrap();
    assert!(matches!(
        form.commit(&mut ui).unwrap(),
        DataFormCommit::Conflict(_)
    ));
    assert_eq!(source.borrow().value(7, "name"), Some(&CellValue::Null));
    assert_eq!(ui.document(count).unwrap().text(), "42");
    assert!(form.is_dirty(&ui));
}

#[test]
fn arrangement_preserves_ids_drafts_focus_and_source_refresh_conflicts() {
    let (mut ui, mut form, source) = fixture();
    let count = form.field_editor("count").unwrap();
    ui.update(Size::new(800.0, 500.0)).unwrap();
    ui.runtime.apply(Mutation::Focus(Some(count))).unwrap();
    ui.set_text(count, "43").unwrap();
    let mut arrangement = form.arrangement();
    arrangement.reverse();
    form.arrange(&mut ui, &arrangement, 190.0).unwrap();
    ui.update(Size::new(800.0, 500.0)).unwrap();
    assert_eq!(form.field_editor("count"), Some(count));
    assert_eq!(ui.runtime.focused(), Some(count));
    assert_eq!(ui.document(count).unwrap().text(), "43");
    source
        .borrow_mut()
        .set_cell(7, "count", CellValue::Integer(9))
        .unwrap();
    source
        .borrow_mut()
        .set_cell(7, "active", CellValue::Bool(true))
        .unwrap();
    form.refresh(&mut ui).unwrap();
    assert_eq!(form.value(&ui, "active").unwrap(), CellValue::Bool(true));
    assert_eq!(ui.document(count).unwrap().text(), "43");
    assert_eq!(form.issues().len(), 1);
    form.revert(&mut ui).unwrap();
    assert_eq!(form.value(&ui, "count").unwrap(), CellValue::Integer(9));
    assert!(!form.is_dirty(&ui));
}

#[test]
fn all_metadata_types_and_named_editor_recipes_round_trip_without_loss() {
    let fields = vec![
        FieldSchema::new("decimal", FieldType::Decimal),
        FieldSchema::new("date", FieldType::Date),
        FieldSchema::new("time", FieldType::Time),
        FieldSchema::new("binary", FieldType::Binary),
        FieldSchema::new("real", FieldType::Real),
    ];
    let values = vec![
        CellValue::Decimal(DecimalValue::new(i128::MAX, 0).unwrap()),
        CellValue::Date("2026-10-17".parse().unwrap()),
        CellValue::Time("13:42:59".parse().unwrap()),
        CellValue::Binary(vec![0, 255, 15]),
        CellValue::Real(0.1),
    ];
    let source = Rc::new(RefCell::new(
        DataSource::new(CollectionAdapter::new(
            DataTable::new(
                TableSchema::new(fields).unwrap(),
                vec![DataRow::new(1, values.clone())],
            )
            .unwrap(),
        ))
        .unwrap(),
    ));
    let mut factories = DataFormFactories::default();
    factories
        .register(
            "ExactText",
            DataFormEditorFactory::Text(EditorSettings::default()),
        )
        .unwrap();
    assert!(
        factories
            .register("ExactText", DataFormEditorFactory::CheckBox)
            .is_err()
    );
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let root = ui.root();
    let settings = DataFormSettings {
        fields: ["decimal", "date", "time", "binary", "real"]
            .into_iter()
            .map(|name| {
                let mut field = DataFormField::new(name);
                if name == "binary" {
                    field.editor = "ExactText".into();
                }
                field
            })
            .collect(),
        ..Default::default()
    };
    let mut form = DataForm::mount(
        &mut ui,
        root,
        source,
        1,
        settings,
        &factories,
        LayoutStyle::default(),
    )
    .unwrap();
    for (field, value) in ["decimal", "date", "time", "binary", "real"]
        .into_iter()
        .zip(values)
    {
        assert_eq!(form.value(&ui, field).unwrap(), value);
    }
    assert_eq!(form.commit(&mut ui).unwrap(), DataFormCommit::Unchanged);
}

#[test]
fn invalid_factories_and_arrangements_fail_without_mutating_the_tree() {
    let (mut ui, mut form, source) = fixture();
    let root = ui.root();
    let children = ui.runtime.tree().children(root).unwrap().to_vec();
    let mut field = DataFormField::new("name");
    field.editor = "Numeric".into();
    assert!(
        DataForm::mount(
            &mut ui,
            root,
            source,
            7,
            DataFormSettings {
                fields: vec![field],
                ..Default::default()
            },
            &DataFormFactories::default(),
            LayoutStyle::default()
        )
        .is_err()
    );
    assert_eq!(ui.runtime.tree().children(root).unwrap(), children);
    let arrangement = form.arrangement();
    let mut invalid = arrangement.clone();
    invalid[1].field = invalid[0].field.clone();
    assert!(form.arrange(&mut ui, &invalid, 200.0).is_err());
    assert_eq!(form.arrangement(), arrangement);
    let name = form.field_editor("name").unwrap();
    ui.remove(name).unwrap();
    assert!(form.commit(&mut ui).is_err());
    assert!(form.refresh(&mut ui).is_err());
}

#[test]
fn readonly_metadata_rejects_application_drafts_and_clean_form_save_is_noop() {
    let (mut ui, _, source) = fixture();
    let root = ui.root();
    let mut form = DataForm::mount(
        &mut ui,
        root,
        source.clone(),
        7,
        DataFormSettings {
            read_only: true,
            ..Default::default()
        },
        &DataFormFactories::default(),
        LayoutStyle::default(),
    )
    .unwrap();
    assert_eq!(form.commit(&mut ui).unwrap(), DataFormCommit::Unchanged);
    let editor = form.field_editor("count").unwrap();
    assert_eq!(ui.is_read_only(editor), Some(true));
    let owner = form.field("count").unwrap();
    assert_eq!(ui.composite_read_only(owner), Some(true));
    ui.step_value(owner, -1).unwrap();
    assert_eq!(
        form.value(&ui, "count").unwrap(),
        CellValue::Integer(i64::MAX)
    );
    ui.set_text(editor, "1").unwrap();
    assert!(matches!(
        form.commit(&mut ui).unwrap(),
        DataFormCommit::Invalid(_)
    ));
    assert_eq!(
        source.borrow().value(7, "count"),
        Some(&CellValue::Integer(i64::MAX))
    );
}

#[test]
fn incompatible_recipe_precision_is_rejected_before_mount() {
    let schema = TableSchema::new(vec![FieldSchema::new("time", FieldType::Time)]).unwrap();
    let source = Rc::new(RefCell::new(
        DataSource::new(CollectionAdapter::new(
            DataTable::new(
                schema,
                vec![DataRow::new(
                    1,
                    vec![CellValue::Time("12:30:42".parse().unwrap())],
                )],
            )
            .unwrap(),
        ))
        .unwrap(),
    ));
    let mut factories = DataFormFactories::default();
    factories
        .register(
            "Minutes",
            DataFormEditorFactory::Time(TimeEditSettings::default()),
        )
        .unwrap();
    let mut field = DataFormField::new("time");
    field.editor = "Minutes".into();
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let root = ui.root();
    let before = ui.runtime.tree().children(root).unwrap().to_vec();
    assert!(
        DataForm::mount(
            &mut ui,
            root,
            source,
            1,
            DataFormSettings {
                fields: vec![field],
                ..Default::default()
            },
            &factories,
            LayoutStyle::default()
        )
        .is_err()
    );
    assert_eq!(ui.runtime.tree().children(root).unwrap(), before);
}

#[test]
fn explicit_null_save_ignores_hidden_draft_but_keeps_it_for_later_editing() {
    let (mut ui, mut form, source) = fixture();
    form.set_field_value(&mut ui, "name", CellValue::Text("Initial".into()))
        .unwrap();
    assert_eq!(form.commit(&mut ui).unwrap(), DataFormCommit::Saved);
    let editor = form.field_editor("name").unwrap();
    ui.set_text(editor, "local buffer").unwrap();
    let null = form.field_null_toggle("name").unwrap();
    ui.set_checked(null, true).unwrap();
    form.update(&mut ui).unwrap();
    assert_eq!(form.commit(&mut ui).unwrap(), DataFormCommit::Saved);
    assert!(!form.is_dirty(&ui));
    assert_eq!(source.borrow().value(7, "name"), Some(&CellValue::Null));
    assert_eq!(ui.document(editor).unwrap().text(), "local buffer");
    ui.set_checked(null, false).unwrap();
    form.update(&mut ui).unwrap();
    assert!(form.is_dirty(&ui));
}

#[test]
fn asynchronous_validation_finishes_before_the_explicit_source_transaction() {
    use std::{
        sync::{Arc, Mutex, mpsc},
        time::Duration,
    };
    let (mut ui, mut form, source) = fixture();
    form.set_field_value(&mut ui, "name", CellValue::Text("old".into()))
        .unwrap();
    form.commit(&mut ui).unwrap();
    let editor = form.field_editor("name").unwrap();
    let (release, wait) = mpsc::channel();
    let wait = Arc::new(Mutex::new(wait));
    let (notify, finished) = mpsc::channel();
    ui.set_editor_validation_waker(Some(Arc::new(move || {
        notify.send(()).unwrap();
    })));
    ui.set_async_editor_validator(
        editor,
        Some(crate::AsyncEditorValidator::new(move |_| {
            wait.lock().unwrap().recv().unwrap();
            Ok(())
        })),
    )
    .unwrap();
    ui.set_text(editor, "new").unwrap();
    assert_eq!(form.commit(&mut ui).unwrap(), DataFormCommit::Pending);
    assert_eq!(
        source.borrow().value(7, "name"),
        Some(&CellValue::Text("old".into()))
    );
    release.send(()).unwrap();
    finished.recv_timeout(Duration::from_secs(3)).unwrap();
    ui.poll_editor_validation().unwrap();
    assert_eq!(
        source.borrow().value(7, "name"),
        Some(&CellValue::Text("old".into()))
    );
    assert_eq!(form.commit(&mut ui).unwrap(), DataFormCommit::Saved);
    assert_eq!(
        source.borrow().value(7, "name"),
        Some(&CellValue::Text("new".into()))
    );
}

#[test]
fn replacing_a_typed_recipe_cannot_silently_change_form_parsing() {
    let (mut ui, mut form, source) = fixture();
    let owner = form.field("count").unwrap();
    let mut settings = ui.numeric_settings(owner).unwrap().clone();
    settings.culture = crate::UiCulture::English;
    ui.set_numeric_settings(owner, settings).unwrap();
    assert!(form.commit(&mut ui).is_err());
    assert!(form.value(&ui, "count").is_err());
    assert_eq!(
        source.borrow().value(7, "count"),
        Some(&CellValue::Integer(i64::MAX))
    );
}

#[test]
fn auto_decimal_has_compact_display_without_losing_future_precision() {
    let initial: DecimalValue = "12345.67".parse().unwrap();
    let source = Rc::new(RefCell::new(
        DataSource::new(CollectionAdapter::new(
            DataTable::new(
                TableSchema::new(vec![FieldSchema::new("amount", FieldType::Decimal)]).unwrap(),
                vec![DataRow::new(1, vec![CellValue::Decimal(initial)])],
            )
            .unwrap(),
        ))
        .unwrap(),
    ));
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let root = ui.root();
    let mut form = DataForm::mount(
        &mut ui,
        root,
        source.clone(),
        1,
        DataFormSettings::default(),
        &DataFormFactories::default(),
        LayoutStyle::default(),
    )
    .unwrap();
    let editor = form.field_editor("amount").unwrap();
    assert_eq!(ui.document(editor).unwrap().text(), "12345,67");
    assert!(!form.is_dirty(&ui));
    assert_eq!(
        ui.numeric_settings(form.field("amount").unwrap())
            .unwrap()
            .precision,
        28
    );
    let exact: DecimalValue = "0.1234567890123456789012345678".parse().unwrap();
    ui.set_text(editor, "0,1234567890123456789012345678")
        .unwrap();
    assert_eq!(form.commit(&mut ui).unwrap(), DataFormCommit::Saved);
    assert_eq!(
        source.borrow().value(1, "amount"),
        Some(&CellValue::Decimal(exact))
    );
    assert!(!form.is_dirty(&ui));
    form.revert(&mut ui).unwrap();
    assert_eq!(
        ui.document(editor).unwrap().text(),
        "0,1234567890123456789012345678"
    );
}
