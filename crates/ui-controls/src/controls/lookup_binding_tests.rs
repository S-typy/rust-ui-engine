use super::*;
use crate::{
    BindingContext, BindingPath, BindingProperty, BindingValidator, ControlBinding,
    ControlBindingMode, ControlBindings, DataValue,
};
use rust_desktop_ui_data::{
    CellValue, DataRow, DataTable, FieldSchema, FieldType, ObservableCollection, TableSchema,
};
use std::{cell::RefCell, rc::Rc};

fn bind(
    ui: &mut Controls,
    c: &EditorControl,
    model: Rc<RefCell<BindingContext>>,
    multiple: bool,
) -> ControlBindings {
    let mut bindings = ControlBindings::new(model);
    let mut rows = ControlBinding::new(c.container, BindingProperty::ItemsSource, "Rows").unwrap();
    rows.items.display_member_path = Some(BindingPath::parse("Name").unwrap());
    rows.items.value_member_path = Some(BindingPath::parse("Id").unwrap());
    rows.items.columns = vec![BindingPath::parse("City").unwrap()];
    bindings.add(rows).unwrap();
    let mut selected = ControlBinding::new(
        c.container,
        if multiple {
            BindingProperty::SelectedValues
        } else {
            BindingProperty::SelectedValue
        },
        "Selected",
    )
    .unwrap();
    selected.mode = ControlBindingMode::TwoWay;
    bindings.add(selected).unwrap();
    bindings.initialize(ui).unwrap();
    bindings
}
fn model_row(id: DataValue, name: &str, city: &str) -> DataValue {
    DataValue::object([
        ("Id", id),
        ("Name", name.into()),
        ("City", city.into()),
        ("enabled", true.into()),
    ])
}

#[test]
fn typed_identities_do_not_alias_and_rejected_multi_draft_never_writes_partial_model() {
    let (mut ui, c) = setup(Vec::new());
    ui.set_combo_selection_settings(
        c.container,
        ComboSelectionSettings {
            mode: SelectionMode::Multiple,
            presentation: ComboSelectionPresentation::Tokens,
            ..Default::default()
        },
    )
    .unwrap();
    let model = Rc::new(RefCell::new(
        BindingContext::new(DataValue::object([
            (
                "Rows",
                DataValue::List(vec![
                    model_row("1".into(), "Same", "A"),
                    model_row(1.0.into(), "Same", "B"),
                    model_row(true.into(), "Same", "C"),
                ]),
            ),
            ("Selected", DataValue::List(vec!["1".into()])),
        ]))
        .unwrap(),
    ));
    let mut bindings = bind(&mut ui, &c, Rc::clone(&model), true);
    let items = ui.selection_items(c.container).unwrap().to_vec();
    assert_ne!(items[0].key, items[1].key);
    assert_ne!(items[1].key, items[2].key);
    model
        .borrow_mut()
        .set_field_validator(
            "Selected",
            Some(BindingValidator::new(|value| {
                if value.as_list().unwrap().len() > 1 {
                    Err("One reference only".into())
                } else {
                    Ok(())
                }
            })),
        )
        .unwrap();
    ui.open_editor_popup(c.container).unwrap();
    ui.toggle_combo_draft(c.container, &items[1].key).unwrap();
    assert!(matches!(
        ui.commit_combo_selection(c.container).unwrap(),
        CommitOutcome::Invalid(_)
    ));
    bindings.sync(&mut ui);
    assert_eq!(
        model.borrow().get("Selected").unwrap(),
        &DataValue::List(vec!["1".into()])
    );
    assert_eq!(
        ui.selected_keys(c.container).unwrap(),
        [items[0].key.clone()]
    );
    assert_eq!(ui.combo_selection_draft(c.container).unwrap().len(), 2);
    ui.cancel_combo_selection(c.container).unwrap();
    ui.set_selected_keys(c.container, &[items[1].key.clone()])
        .unwrap();
    bindings.sync(&mut ui);
    assert_eq!(
        model.borrow().get("Selected").unwrap(),
        &DataValue::List(vec![1.0.into()])
    );
}

fn collection(key: i64, name: &str) -> ObservableCollection {
    ObservableCollection::new(
        DataTable::new(
            TableSchema::new(vec![
                FieldSchema::new("Name", FieldType::Text),
                FieldSchema::new("City", FieldType::Text),
                FieldSchema::new("enabled", FieldType::Bool),
            ])
            .unwrap(),
            vec![DataRow::new(
                key,
                vec![
                    CellValue::Text(name.into()),
                    CellValue::Text("Москва".into()),
                    CellValue::Bool(true),
                ],
            )],
        )
        .unwrap(),
    )
}

#[test]
fn collection_swap_uses_new_identity_and_revision_cursor_and_ignores_detached_source() {
    let (mut ui, c) = setup(Vec::new());
    let first = collection(i64::MAX, "First");
    let second = collection(i64::MAX, "Second");
    assert_eq!(first.revision(), second.revision());
    let mut context = BindingContext::new(DataValue::object([
        ("Rows", DataValue::List(vec![])),
        ("Selected", i64::MAX.to_string().into()),
    ]))
    .unwrap();
    context
        .register_collection("Rows", first.clone(), "Id")
        .unwrap();
    let model = Rc::new(RefCell::new(context));
    let mut bindings = bind(&mut ui, &c, Rc::clone(&model), false);
    assert_eq!(ui.accepted_value(c.editor), Some("First"));
    model.borrow_mut().unregister_collection("Rows").unwrap();
    model
        .borrow_mut()
        .register_collection("Rows", second.clone(), "Id")
        .unwrap();
    bindings.sync(&mut ui);
    assert_eq!(ui.accepted_value(c.editor), Some("Second"));
    first
        .update_cell(i64::MAX, "Name", CellValue::Text("Detached".into()))
        .unwrap();
    assert!(!bindings.sync(&mut ui));
    assert_eq!(ui.accepted_value(c.editor), Some("Second"));
    second
        .update_cell(i64::MAX, "enabled", CellValue::Bool(false))
        .unwrap();
    bindings.sync(&mut ui);
    assert_eq!(model.borrow().get("Selected").unwrap(), &DataValue::Null);
    assert!(ui.selected_keys(c.container).unwrap().is_empty());
    assert!(
        bindings.diagnostics().is_empty(),
        "{:?}",
        bindings.diagnostics()
    );
}

#[test]
fn rejected_items_projection_preserves_key_mapping_model_selection_and_row_ids() {
    let (mut ui, c) = setup(Vec::new());
    let rows = vec![
        model_row(1.0.into(), "One", "A"),
        model_row(true.into(), "Two", "B"),
    ];
    let model = Rc::new(RefCell::new(
        BindingContext::new(DataValue::object([
            ("Rows", DataValue::List(rows.clone())),
            ("Selected", 1.0.into()),
        ]))
        .unwrap(),
    ));
    let mut bindings = bind(&mut ui, &c, Rc::clone(&model), false);
    ui.open_editor_popup(c.container).unwrap();
    frame(&mut ui);
    let list = popup(&ui, c.container);
    let live = ui.rich.lists[&list].rows.clone();
    let accepted = ui.selection_items(c.container).unwrap().to_vec();
    let mut malformed = rows;
    malformed[1] = model_row(
        "replacement".into(),
        &format!("A{}", "\u{301}".repeat(40)),
        "B",
    );
    model
        .borrow_mut()
        .set("Rows", DataValue::List(malformed))
        .unwrap();
    bindings.sync(&mut ui);
    assert!(!bindings.diagnostics().is_empty());
    assert_eq!(ui.selection_items(c.container), Some(accepted.as_slice()));
    assert_eq!(ui.rich.lists[&list].rows, live);
    assert_eq!(
        model.borrow().get("Selected").unwrap(),
        &DataValue::Number(1.0)
    );
    // The old bool identity remains mapped even while the new source is rejected.
    ui.set_selected_keys(c.container, &[accepted[1].key.clone()])
        .unwrap();
    bindings.sync(&mut ui);
    assert_eq!(
        model.borrow().get("Selected").unwrap(),
        &DataValue::Bool(true)
    );
}
