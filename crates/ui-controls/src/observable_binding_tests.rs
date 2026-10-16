use super::*;
use crate::{BindingConverter, BindingValidator, ComboBoxSettings, SelectionSettings, Theme};
use rust_desktop_ui_core::LayoutStyle;
use rust_desktop_ui_data::{
    CellValue, CollectionMutation, DataRow, DataTable, FieldSchema, FieldType,
    ObservableCollection, TableSchema,
};

fn row(key: i64, name: &str) -> DataRow {
    DataRow::new(
        key,
        vec![
            CellValue::Text(name.into()),
            CellValue::Text(format!("C{key}")),
        ],
    )
}

fn setup(
    rows: Vec<DataRow>,
) -> (
    Controls,
    Rc<RefCell<BindingContext>>,
    ObservableCollection,
    ControlBindings,
) {
    let source = ObservableCollection::new(
        DataTable::new(
            TableSchema::new(vec![
                FieldSchema::new("Name", FieldType::Text),
                FieldSchema::new("Code", FieldType::Text),
            ])
            .unwrap(),
            rows,
        )
        .unwrap(),
    );
    let mut context = BindingContext::new(DataValue::object([
        ("Products", DataValue::List(vec![])),
        ("Selected", DataValue::Null),
        ("Other", 0.0.into()),
    ]))
    .unwrap();
    context
        .register_collection("Products", source.clone(), "Id")
        .unwrap();
    let context = Rc::new(RefCell::new(context));
    let host = ControlBindings::new(Rc::clone(&context));
    (
        Controls::new(Theme::FluentLight).unwrap(),
        context,
        source,
        host,
    )
}

fn items_binding(id: WidgetId) -> ControlBinding {
    let mut link = ControlBinding::new(id, BindingProperty::ItemsSource, "Products").unwrap();
    link.items.display_member_path = Some(BindingPath::parse("Name").unwrap());
    link.items.value_member_path = Some(BindingPath::parse("Id").unwrap());
    link.items.columns = vec![BindingPath::parse("Code").unwrap()];
    link
}

fn selected_binding(id: WidgetId) -> ControlBinding {
    let mut link = ControlBinding::new(id, BindingProperty::SelectedValue, "Selected").unwrap();
    link.mode = ControlBindingMode::TwoWay;
    link
}

fn list(ui: &mut Controls) -> WidgetId {
    ui.list_box(
        ui.root(),
        "Products",
        vec![],
        SelectionSettings::default(),
        LayoutStyle::default(),
    )
    .unwrap()
    .container
}

#[test]
fn one_collection_updates_three_selection_controls_and_round_trips_exact_keys() {
    let (mut ui, context, source, mut host) = setup(vec![row(i64::MIN, "A"), row(2, "B")]);
    let list = list(&mut ui);
    let combo = ui
        .combo_box(
            ui.root(),
            "Combo",
            vec![],
            ComboBoxSettings::default(),
            LayoutStyle::default(),
        )
        .unwrap();
    let lookup = ui
        .lookup_box(
            ui.root(),
            "Lookup",
            vec![],
            vec!["Name".into(), "Code".into()],
            LayoutStyle::default(),
        )
        .unwrap();
    let ids = [list, combo.container, lookup.container];
    for id in ids {
        host.add(selected_binding(id)).unwrap();
        host.add(items_binding(id)).unwrap();
    }
    context
        .borrow_mut()
        .set("Selected", i64::MIN.to_string().into())
        .unwrap();
    host.initialize(&mut ui).unwrap();
    for id in ids {
        assert_eq!(ui.selected_keys(id).unwrap(), &[i64::MIN.to_string()]);
    }
    source
        .batch(vec![
            CollectionMutation::Add(row(i64::MAX, "New")),
            CollectionMutation::Move {
                key: i64::MAX,
                index: 0,
            },
        ])
        .unwrap();
    context
        .borrow_mut()
        .set("Selected", i64::MAX.to_string().into())
        .unwrap();
    host.sync(&mut ui);
    for id in ids {
        assert_eq!(ui.selection_items(id).unwrap()[0].text, "New");
        assert_eq!(ui.selected_keys(id).unwrap(), &[i64::MAX.to_string()]);
    }
    source
        .update_cell(i64::MAX, "Name", CellValue::Text("Renamed".into()))
        .unwrap();
    host.sync(&mut ui);
    assert_eq!(ui.accepted_value(combo.editor), Some("Renamed"));
    assert_eq!(ui.accepted_value(lookup.editor), Some("Renamed"));
    ui.set_selected_keys(list, &["2".into()]).unwrap();
    host.sync(&mut ui);
    assert_eq!(
        context.borrow().get("Selected").unwrap().as_str(),
        Some("2")
    );
    for id in ids {
        assert_eq!(ui.selected_keys(id).unwrap(), &["2"]);
    }
    source.remove(2).unwrap();
    host.sync(&mut ui);
    assert_eq!(context.borrow().get("Selected").unwrap(), &DataValue::Null);
    for id in ids {
        assert!(ui.selected_keys(id).unwrap().is_empty());
    }
    assert!(host.diagnostics().is_empty(), "{:?}", host.diagnostics());
}

#[test]
fn unchanged_large_items_and_unrelated_paths_do_not_run_the_converter() {
    let (mut ui, context, source, mut host) =
        setup((0..5000).map(|key| row(key, "Item")).collect());
    let list = list(&mut ui);
    let calls = Rc::new(Cell::new(0));
    let calls_clone = Rc::clone(&calls);
    host.converters()
        .borrow_mut()
        .register(
            "Count",
            BindingConverter::new(move |value| {
                calls_clone.set(calls_clone.get() + 1);
                Ok(value.clone())
            }),
        )
        .unwrap();
    let mut binding = items_binding(list);
    binding.converter = Some("Count".into());
    host.add(binding).unwrap();
    host.initialize(&mut ui).unwrap();
    assert_eq!(calls.get(), 1);
    for n in 1..20 {
        assert!(!host.sync(&mut ui));
        context
            .borrow_mut()
            .set("Other", (n as f64).into())
            .unwrap();
        assert!(!host.sync(&mut ui));
    }
    assert_eq!(calls.get(), 1);
    source
        .update_cell(7, "Name", CellValue::Text("Changed".into()))
        .unwrap();
    assert!(host.sync(&mut ui));
    assert_eq!(calls.get(), 2);
    assert_eq!(ui.selection_items(list).unwrap()[7].text, "Changed");
}

#[test]
fn precise_cell_changes_do_not_reconvert_other_rows_or_fields() {
    let (mut ui, _, source, mut host) = setup(vec![row(1, "A"), row(2, "B")]);
    let mut counters = Vec::new();
    let mut editors = Vec::new();
    for (index, path) in ["Products[0].Name", "Products[0].Code", "Products[1].Name"]
        .iter()
        .enumerate()
    {
        let calls = Rc::new(Cell::new(0));
        let cloned = Rc::clone(&calls);
        let name = format!("Count{index}");
        host.converters()
            .borrow_mut()
            .register(
                &name,
                BindingConverter::new(move |value| {
                    cloned.set(cloned.get() + 1);
                    Ok(value.clone())
                }),
            )
            .unwrap();
        let editor = ui
            .edit_box(ui.root(), "Cell", "", LayoutStyle::default())
            .unwrap();
        let mut binding = ControlBinding::new(editor, BindingProperty::Text, path).unwrap();
        binding.converter = Some(name);
        host.add(binding).unwrap();
        counters.push(calls);
        editors.push(editor);
    }
    host.initialize(&mut ui).unwrap();
    source
        .batch(vec![
            CollectionMutation::UpdateCell {
                key: 1,
                field: "Name".into(),
                value: CellValue::Text("Changed1".into()),
            },
            CollectionMutation::UpdateCell {
                key: 1,
                field: "Name".into(),
                value: CellValue::Text("Changed2".into()),
            },
        ])
        .unwrap();
    host.sync(&mut ui);
    assert_eq!(
        counters.iter().map(|c| c.get()).collect::<Vec<_>>(),
        vec![2, 1, 1]
    );
    assert_eq!(ui.document(editors[0]).unwrap().text(), "Changed2");
}

#[test]
fn independent_hosts_one_time_links_and_expired_context_history_resynchronize() {
    let (mut ui, context, source, mut first) = setup(vec![row(1, "A")]);
    let one = list(&mut ui);
    let two = list(&mut ui);
    let once = list(&mut ui);
    first.add(items_binding(one)).unwrap();
    let mut once_link = items_binding(once);
    once_link.mode = ControlBindingMode::OneTime;
    first.add(once_link).unwrap();
    let mut second = ControlBindings::new(Rc::clone(&context));
    second.add(items_binding(two)).unwrap();
    first.initialize(&mut ui).unwrap();
    second.initialize(&mut ui).unwrap();
    source.add(row(2, "B")).unwrap();
    first.sync(&mut ui);
    assert_eq!(ui.selection_items(two).unwrap().len(), 1);
    for n in 1..300 {
        context
            .borrow_mut()
            .set("Other", (n as f64).into())
            .unwrap();
    }
    second.sync(&mut ui);
    assert_eq!(ui.selection_items(two).unwrap().len(), 2);
    assert_eq!(ui.selection_items(one).unwrap().len(), 2);
    assert_eq!(ui.selection_items(once).unwrap().len(), 1);
    assert!(second.diagnostics().is_empty());
}

#[test]
fn collection_projection_failure_preserves_controls_deduplicates_diagnostics_and_retries() {
    let (mut ui, context, source, mut host) = setup(vec![row(1, "A")]);
    let list = list(&mut ui);
    host.add(items_binding(list)).unwrap();
    context
        .borrow_mut()
        .set_field_validator(
            "Products",
            Some(BindingValidator::new(|value| {
                if value.as_list().unwrap().len() > 1 {
                    Err("one row only".into())
                } else {
                    Ok(())
                }
            })),
        )
        .unwrap();
    host.initialize(&mut ui).unwrap();
    source.add(row(2, "B")).unwrap();
    assert!(!host.sync(&mut ui));
    assert!(!host.sync(&mut ui));
    assert_eq!(host.diagnostics().len(), 1);
    assert_eq!(ui.selection_items(list).unwrap().len(), 1);
    context
        .borrow_mut()
        .set_field_validator("Products", None)
        .unwrap();
    assert!(host.sync(&mut ui));
    assert_eq!(ui.selection_items(list).unwrap().len(), 2);
    assert!(host.links[0].error.is_none());
}

#[test]
fn removing_a_selected_row_preserves_combo_draft_composition_and_updates_cancel_baseline() {
    let (mut ui, context, source, mut host) = setup(vec![row(1, "A"), row(2, "B")]);
    let combo = ui
        .combo_box(
            ui.root(),
            "Combo",
            vec![],
            ComboBoxSettings {
                editable: true,
                read_only: false,
            },
            LayoutStyle::default(),
        )
        .unwrap();
    host.add(items_binding(combo.container)).unwrap();
    host.add(selected_binding(combo.container)).unwrap();
    context.borrow_mut().set("Selected", "1".into()).unwrap();
    host.initialize(&mut ui).unwrap();
    ui.runtime
        .apply(Mutation::Focus(Some(combo.editor)))
        .unwrap();
    ui.set_text(combo.editor, "Draft").unwrap();
    ui.ime_preedit("候補", None).unwrap();
    let selection = ui.document(combo.editor).unwrap().selection();
    let undo = ui.document(combo.editor).unwrap().can_undo();
    source
        .update_cell(1, "Name", CellValue::Text("Renamed".into()))
        .unwrap();
    host.sync(&mut ui);
    assert_eq!(ui.document(combo.editor).unwrap().text(), "Draft");
    assert_eq!(ui.accepted_value(combo.editor), Some("Renamed"));
    source.remove(1).unwrap();
    host.sync(&mut ui);
    assert_eq!(ui.document(combo.editor).unwrap().text(), "Draft");
    assert!(ui.document(combo.editor).unwrap().composition().is_some());
    assert_eq!(ui.document(combo.editor).unwrap().selection(), selection);
    assert_eq!(ui.document(combo.editor).unwrap().can_undo(), undo);
    assert_eq!(ui.accepted_value(combo.editor), Some(""));
    assert!(ui.selected_keys(combo.container).unwrap().is_empty());
    assert_eq!(context.borrow().get("Selected").unwrap(), &DataValue::Null);
    ui.cancel_edit(combo.editor).unwrap();
    assert_eq!(ui.document(combo.editor).unwrap().text(), "");
    assert!(host.diagnostics().is_empty(), "{:?}", host.diagnostics());
}

#[test]
fn replacing_a_converter_invalidates_an_unchanged_source() {
    let (mut ui, _, _, mut host) = setup(vec![row(1, "A")]);
    let editor = ui
        .edit_box(ui.root(), "Caption", "", LayoutStyle::default())
        .unwrap();
    host.converters()
        .borrow_mut()
        .register("Caption", BindingConverter::new(|_| Ok("First".into())))
        .unwrap();
    let mut binding =
        ControlBinding::new(editor, BindingProperty::Text, "Products[0].Name").unwrap();
    binding.converter = Some("Caption".into());
    host.add(binding).unwrap();
    host.initialize(&mut ui).unwrap();
    assert_eq!(ui.document(editor).unwrap().text(), "First");
    host.converters()
        .borrow_mut()
        .register("Caption", BindingConverter::new(|_| Ok("Second".into())))
        .unwrap();
    assert!(host.sync(&mut ui));
    assert_eq!(ui.document(editor).unwrap().text(), "Second");
}

#[test]
fn converter_model_mutations_remain_pending_for_forward_and_reverse_transfers() {
    let (mut ui, context, _, mut host) = setup(vec![row(1, "A")]);
    let captured = Rc::clone(&context);
    host.converters()
        .borrow_mut()
        .register(
            "Mutating",
            BindingConverter::new(move |value| {
                let number = value.as_number().unwrap();
                // Initial forward read and the read following a reverse publication
                // both deliberately change their own source after taking the value.
                if number == 0.0 || number == 2.0 {
                    captured
                        .borrow_mut()
                        .set("Other", (number + 1.0).into())
                        .unwrap();
                }
                Ok(number.to_string().into())
            })
            .with_reverse(|value| Ok(value.as_str().unwrap().parse::<f64>().unwrap().into())),
        )
        .unwrap();
    let editor = ui
        .edit_box(ui.root(), "Value", "", LayoutStyle::default())
        .unwrap();
    let mut binding = ControlBinding::new(editor, BindingProperty::Text, "Other").unwrap();
    binding.converter = Some("Mutating".into());
    binding.mode = ControlBindingMode::TwoWay;
    host.add(binding).unwrap();
    host.initialize(&mut ui).unwrap();
    assert_eq!(ui.document(editor).unwrap().text(), "0");
    assert_eq!(
        context.borrow().get("Other").unwrap(),
        &DataValue::Number(1.0)
    );
    assert!(host.sync(&mut ui));
    assert_eq!(ui.document(editor).unwrap().text(), "1");
    assert!(!host.sync(&mut ui));
    ui.set_text(editor, "2").unwrap();
    assert!(host.sync(&mut ui));
    assert_eq!(ui.document(editor).unwrap().text(), "2");
    assert_eq!(
        context.borrow().get("Other").unwrap(),
        &DataValue::Number(3.0)
    );
    assert!(host.sync(&mut ui));
    assert_eq!(ui.document(editor).unwrap().text(), "3");
    assert!(!host.sync(&mut ui));
    assert!(host.diagnostics().is_empty());
}
