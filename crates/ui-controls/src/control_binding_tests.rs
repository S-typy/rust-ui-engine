use super::*;
use crate::{
    BindingConverter, BindingValidator, DateEditSettings, SelectionSettings, SpinBoxSettings,
    Theme, TimeEditSettings, ValueSchema,
};
use rust_desktop_ui_core::LayoutStyle;

fn setup() -> (Controls, Rc<RefCell<BindingContext>>, ControlBindings) {
    let ui = Controls::new(Theme::FluentLight).unwrap();
    let data = Rc::new(RefCell::new(
        BindingContext::new(DataValue::object([
            ("Name", "Anna".into()),
            ("Other", "Other".into()),
            ("Count", 3.0.into()),
            ("Checked", false.into()),
            ("Date", "2026-10-15".into()),
            ("Time", "12:30:00".into()),
            ("Selected", "a".into()),
            (
                "Items",
                DataValue::List(vec![
                    DataValue::object([("key", "a".into()), ("label", "Alpha".into())]),
                    DataValue::object([("key", "b".into()), ("label", "Beta".into())]),
                ]),
            ),
        ]))
        .unwrap(),
    ));
    let bindings = ControlBindings::new(Rc::clone(&data));
    (ui, data, bindings)
}

fn text_link(
    ui: &mut Controls,
    bindings: &mut ControlBindings,
    mode: ControlBindingMode,
    trigger: BindingUpdateTrigger,
) -> WidgetId {
    let id = ui
        .edit_box(ui.root(), "Name", "Initial", LayoutStyle::default())
        .unwrap();
    let mut link = ControlBinding::new(id, BindingProperty::Text, "Name").unwrap();
    link.mode = mode;
    link.trigger = trigger;
    bindings.add(link).unwrap();
    id
}

#[test]
fn model_updates_target_and_unrelated_updates_preserve_editor_draft() {
    let (mut ui, data, mut bindings) = setup();
    let id = text_link(
        &mut ui,
        &mut bindings,
        ControlBindingMode::TwoWay,
        BindingUpdateTrigger::LostFocus,
    );
    bindings.initialize(&mut ui).unwrap();
    assert_eq!(ui.document(id).unwrap().text(), "Anna");
    ui.set_text(id, "Draft").unwrap();
    data.borrow_mut().set("Other", "Changed".into()).unwrap();
    bindings.sync(&mut ui);
    assert_eq!(ui.document(id).unwrap().text(), "Draft");
    assert_eq!(data.borrow().get("Name").unwrap().as_str(), Some("Anna"));
    data.borrow_mut().set("Name", "Model wins".into()).unwrap();
    bindings.sync(&mut ui);
    assert_eq!(ui.document(id).unwrap().text(), "Model wins");
}

#[test]
fn two_way_accepted_values_publish_once_and_do_not_drain_events() {
    let (mut ui, data, mut bindings) = setup();
    let id = text_link(
        &mut ui,
        &mut bindings,
        ControlBindingMode::TwoWay,
        BindingUpdateTrigger::LostFocus,
    );
    bindings.initialize(&mut ui).unwrap();
    ui.drain_edit_events();
    ui.set_text(id, "Boris").unwrap();
    assert!(!bindings.sync(&mut ui));
    ui.commit_edit(id).unwrap();
    assert!(bindings.sync(&mut ui));
    assert_eq!(data.borrow().get("Name").unwrap().as_str(), Some("Boris"));
    assert_eq!(data.borrow().revision(), 1);
    assert!(!bindings.sync(&mut ui));
    assert!(!ui.drain_edit_events().is_empty());
}

#[test]
fn source_validation_rejects_before_acceptance_and_keeps_draft() {
    let (mut ui, data, mut bindings) = setup();
    data.borrow_mut()
        .set_validator(Some(BindingValidator::new(|root| {
            if root.as_object().unwrap()["Name"].as_str() == Some("Rejected") {
                Err("Name is reserved".into())
            } else {
                Ok(())
            }
        })))
        .unwrap();
    let id = text_link(
        &mut ui,
        &mut bindings,
        ControlBindingMode::TwoWay,
        BindingUpdateTrigger::LostFocus,
    );
    bindings.initialize(&mut ui).unwrap();
    ui.set_text(id, "Rejected").unwrap();
    assert!(matches!(
        ui.commit_edit(id).unwrap(),
        CommitOutcome::Invalid(_)
    ));
    assert_eq!(ui.accepted_value(id), Some("Anna"));
    assert_eq!(ui.document(id).unwrap().text(), "Rejected");
    bindings.sync(&mut ui);
    assert_eq!(data.borrow().get("Name").unwrap().as_str(), Some("Anna"));
}

#[test]
fn property_changed_publishes_valid_draft_without_accepting_it() {
    let (mut ui, data, mut bindings) = setup();
    let id = text_link(
        &mut ui,
        &mut bindings,
        ControlBindingMode::TwoWay,
        BindingUpdateTrigger::PropertyChanged,
    );
    bindings.initialize(&mut ui).unwrap();
    ui.set_text(id, "Draft").unwrap();
    assert!(bindings.sync(&mut ui));
    assert_eq!(ui.accepted_value(id), Some("Anna"));
    assert_eq!(data.borrow().get("Name").unwrap().as_str(), Some("Draft"));
    assert!(!bindings.sync(&mut ui));
}

#[test]
fn explicit_trigger_waits_for_update_sources_and_then_accepts() {
    let (mut ui, data, mut bindings) = setup();
    let id = text_link(
        &mut ui,
        &mut bindings,
        ControlBindingMode::TwoWay,
        BindingUpdateTrigger::Explicit,
    );
    bindings.initialize(&mut ui).unwrap();
    ui.set_text(id, "Explicit").unwrap();
    bindings.sync(&mut ui);
    assert_eq!(data.borrow().get("Name").unwrap().as_str(), Some("Anna"));
    assert!(bindings.update_sources(&mut ui));
    assert_eq!(
        data.borrow().get("Name").unwrap().as_str(),
        Some("Explicit")
    );
    assert_eq!(ui.accepted_value(id), Some("Explicit"));
}

#[test]
fn one_time_and_one_way_to_source_have_distinct_initial_directions() {
    let (mut ui, data, mut bindings) = setup();
    let once = text_link(
        &mut ui,
        &mut bindings,
        ControlBindingMode::OneTime,
        BindingUpdateTrigger::PropertyChanged,
    );
    let source = ui
        .edit_box(ui.root(), "Source", "Initial UI", LayoutStyle::default())
        .unwrap();
    let mut link = ControlBinding::new(source, BindingProperty::Text, "Other").unwrap();
    link.mode = ControlBindingMode::OneWayToSource;
    bindings.add(link).unwrap();
    bindings.initialize(&mut ui).unwrap();
    assert_eq!(
        data.borrow().get("Other").unwrap().as_str(),
        Some("Initial UI")
    );
    data.borrow_mut().set("Name", "Changed".into()).unwrap();
    bindings.sync(&mut ui);
    assert_eq!(ui.document(once).unwrap().text(), "Anna");
}

#[test]
fn converters_round_trip_and_errors_recover_without_mutation() {
    let (mut ui, data, mut bindings) = setup();
    bindings
        .converters()
        .borrow_mut()
        .register(
            "Number",
            BindingConverter::new(|value| {
                value
                    .as_number()
                    .map(|n| n.to_string().into())
                    .ok_or("number expected".into())
            })
            .with_reverse(|value| {
                value
                    .as_str()
                    .and_then(|s| s.parse::<f64>().ok())
                    .map(DataValue::Number)
                    .ok_or("number expected".into())
            }),
        )
        .unwrap();
    let id = ui
        .edit_box(ui.root(), "Count", "", LayoutStyle::default())
        .unwrap();
    let mut link = ControlBinding::new(id, BindingProperty::Text, "Count").unwrap();
    link.mode = ControlBindingMode::TwoWay;
    link.converter = Some("Number".into());
    bindings.add(link).unwrap();
    bindings.initialize(&mut ui).unwrap();
    assert_eq!(ui.document(id).unwrap().text(), "3");
    ui.set_text(id, "bad").unwrap();
    assert!(!bindings.sync(&mut ui));
    assert_eq!(data.borrow().get("Count").unwrap().as_number(), Some(3.0));
    assert_eq!(bindings.drain_diagnostics().len(), 1);
    ui.set_text(id, "12").unwrap();
    assert!(bindings.sync(&mut ui));
    assert_eq!(data.borrow().get("Count").unwrap().as_number(), Some(12.0));
}

#[test]
fn element_bindings_use_dependency_order_and_reject_cycles_and_duplicates() {
    let (mut ui, _, mut bindings) = setup();
    let first = ui.label(ui.root(), "", LayoutStyle::default()).unwrap();
    let second = ui.label(ui.root(), "", LayoutStyle::default()).unwrap();
    let third = ui.label(ui.root(), "", LayoutStyle::default()).unwrap();
    let endpoint = |widget| BindingEndpoint {
        widget,
        property: BindingProperty::Text,
    };
    let mut b = ControlBinding::new(third, BindingProperty::Text, "Name").unwrap();
    b.source = BindingSource::Element(endpoint(second));
    bindings.add(b).unwrap();
    let mut a = ControlBinding::new(second, BindingProperty::Text, "Name").unwrap();
    a.source = BindingSource::Element(endpoint(first));
    bindings.add(a).unwrap();
    let mut cycle = ControlBinding::new(first, BindingProperty::Text, "Name").unwrap();
    cycle.source = BindingSource::Element(endpoint(third));
    assert!(bindings.add(cycle).is_err());
    bindings
        .add(ControlBinding::new(first, BindingProperty::Text, "Name").unwrap())
        .unwrap();
    assert!(
        bindings
            .add(ControlBinding::new(first, BindingProperty::Text, "Other").unwrap())
            .is_err()
    );
    bindings.initialize(&mut ui).unwrap();
    assert_eq!(ui.label_text(third), Some("Anna"));
}

#[test]
fn typed_numeric_date_time_bindings_store_invariant_values() {
    let (mut ui, data, mut bindings) = setup();
    let spin = ui
        .spin_box(
            ui.root(),
            "Count",
            0.0,
            SpinBoxSettings::default(),
            LayoutStyle::default(),
        )
        .unwrap();
    let date = ui
        .date_edit(
            ui.root(),
            "Date",
            None,
            DateEditSettings::default(),
            LayoutStyle::default(),
        )
        .unwrap();
    let time = ui
        .time_edit(
            ui.root(),
            "Time",
            None,
            TimeEditSettings::default(),
            LayoutStyle::default(),
        )
        .unwrap();
    for (widget, path) in [
        (spin.container, "Count"),
        (date.container, "Date"),
        (time.container, "Time"),
    ] {
        let mut link = ControlBinding::new(widget, BindingProperty::Value, path).unwrap();
        link.mode = ControlBindingMode::TwoWay;
        link.trigger = BindingUpdateTrigger::LostFocus;
        bindings.add(link).unwrap();
    }
    bindings.initialize(&mut ui).unwrap();
    assert_eq!(ui.spin_value(spin.container), Some(3.0));
    assert_eq!(ui.document(date.editor).unwrap().text(), "15.10.2026");
    assert_eq!(ui.document(time.editor).unwrap().text(), "12:30");
    ui.set_text(date.editor, "29.02.2028").unwrap();
    ui.commit_edit(date.editor).unwrap();
    bindings.sync(&mut ui);
    assert_eq!(
        data.borrow().get("Date").unwrap().as_str(),
        Some("2028-02-29")
    );
    ui.set_spin_value(spin.container, 5.0).unwrap();
    bindings.sync(&mut ui);
    assert_eq!(data.borrow().get("Count").unwrap().as_number(), Some(5.0));
}

#[test]
fn collection_replacement_preserves_stable_keys_and_selection_is_two_way() {
    let (mut ui, data, mut bindings) = setup();
    let list = ui
        .list_box(
            ui.root(),
            "Items",
            Vec::new(),
            SelectionSettings::default(),
            LayoutStyle::default(),
        )
        .unwrap();
    let mut selected =
        ControlBinding::new(list.container, BindingProperty::SelectedValue, "Selected").unwrap();
    selected.mode = ControlBindingMode::TwoWay;
    bindings.add(selected).unwrap();
    bindings
        .add(ControlBinding::new(list.container, BindingProperty::ItemsSource, "Items").unwrap())
        .unwrap();
    bindings.initialize(&mut ui).unwrap();
    assert_eq!(ui.selected_keys(list.container).unwrap(), &["a"]);
    ui.set_selected_keys(list.container, &["b".into()]).unwrap();
    bindings.sync(&mut ui);
    assert_eq!(data.borrow().get("Selected").unwrap().as_str(), Some("b"));
    let mut rows = data
        .borrow()
        .get("Items")
        .unwrap()
        .as_list()
        .unwrap()
        .to_vec();
    rows.reverse();
    data.borrow_mut()
        .set("Items", DataValue::List(rows))
        .unwrap();
    bindings.sync(&mut ui);
    assert_eq!(ui.selected_keys(list.container).unwrap(), &["b"]);
    assert!(bindings.diagnostics().is_empty());
}

#[test]
fn selection_source_validator_prevents_invalid_user_selection() {
    let (mut ui, data, mut bindings) = setup();
    data.borrow_mut()
        .set_validator(Some(BindingValidator::new(|root| {
            if root.as_object().unwrap()["Selected"].as_str() == Some("b") {
                Err("Beta forbidden".into())
            } else {
                Ok(())
            }
        })))
        .unwrap();
    let list = ui
        .list_box(
            ui.root(),
            "Items",
            Vec::new(),
            SelectionSettings::default(),
            LayoutStyle::default(),
        )
        .unwrap();
    bindings
        .add(ControlBinding::new(list.container, BindingProperty::ItemsSource, "Items").unwrap())
        .unwrap();
    let mut selected =
        ControlBinding::new(list.container, BindingProperty::SelectedValue, "Selected").unwrap();
    selected.mode = ControlBindingMode::TwoWay;
    bindings.add(selected).unwrap();
    bindings.initialize(&mut ui).unwrap();
    assert!(ui.set_selected_keys(list.container, &["b".into()]).is_err());
    assert_eq!(ui.selected_keys(list.container).unwrap(), &["a"]);
}

#[test]
fn typed_item_identities_round_trip_and_custom_member_paths_render_columns() {
    let (mut ui, data, mut bindings) = setup();
    data.borrow_mut()
        .set_many([
            ("Selected", 1.0.into()),
            (
                "Items",
                DataValue::List(vec![
                    DataValue::object([
                        ("Id", 1.0.into()),
                        ("Caption", "First".into()),
                        ("Code", "A1".into()),
                    ]),
                    DataValue::object([
                        ("Id", 2.0.into()),
                        ("Caption", "Second".into()),
                        ("Code", "B2".into()),
                    ]),
                ]),
            ),
        ])
        .unwrap();
    let list = ui
        .list_box(
            ui.root(),
            "Items",
            Vec::new(),
            SelectionSettings::default(),
            LayoutStyle::default(),
        )
        .unwrap();
    let mut rows =
        ControlBinding::new(list.container, BindingProperty::ItemsSource, "Items").unwrap();
    rows.items.value_member_path = Some(BindingPath::parse("Id").unwrap());
    rows.items.display_member_path = Some(BindingPath::parse("Caption").unwrap());
    rows.items.columns = vec![BindingPath::parse("Code").unwrap()];
    bindings.add(rows).unwrap();
    let mut selected =
        ControlBinding::new(list.container, BindingProperty::SelectedValue, "Selected").unwrap();
    selected.mode = ControlBindingMode::TwoWay;
    bindings.add(selected).unwrap();
    bindings.initialize(&mut ui).unwrap();
    let item = ui.selection_items(list.container).unwrap()[1].clone();
    assert_eq!(item.columns, ["B2"]);
    ui.set_selected_keys(list.container, &[item.key]).unwrap();
    bindings.sync(&mut ui);
    assert_eq!(
        data.borrow().get("Selected").unwrap().as_number(),
        Some(2.0)
    );
}

#[test]
fn check_readonly_visibility_and_dimensions_are_typed() {
    let (mut ui, data, mut bindings) = setup();
    let checkbox = ui
        .checkbox(ui.root(), "Check", false, LayoutStyle::default())
        .unwrap();
    let mut checked = ControlBinding::new(checkbox, BindingProperty::IsChecked, "Checked").unwrap();
    checked.mode = ControlBindingMode::TwoWay;
    bindings.add(checked).unwrap();
    let label = ui
        .label(ui.root(), "Caption", LayoutStyle::default())
        .unwrap();
    bindings
        .add(ControlBinding::new(label, BindingProperty::Visibility, "Checked").unwrap())
        .unwrap();
    bindings
        .add(ControlBinding::new(label, BindingProperty::Width, "Count").unwrap())
        .unwrap();
    bindings.initialize(&mut ui).unwrap();
    assert!(!ui.runtime.tree().node(label).unwrap().props.visible);
    assert_eq!(
        ui.runtime.tree().node(label).unwrap().props.style.width,
        Length::Px(3.0)
    );
    ui.set_checked(checkbox, true).unwrap();
    bindings.sync(&mut ui);
    assert_eq!(data.borrow().get("Checked").unwrap().as_bool(), Some(true));
    assert!(ui.runtime.tree().node(label).unwrap().props.visible);
}

#[test]
fn fallback_is_visible_and_error_is_reported_once_until_recovery() {
    let (mut ui, data, mut bindings) = setup();
    let label = ui.label(ui.root(), "", LayoutStyle::default()).unwrap();
    let mut link = ControlBinding::new(label, BindingProperty::Text, "Missing").unwrap();
    link.fallback = Some("Unavailable".into());
    bindings.add(link).unwrap();
    bindings.initialize(&mut ui).unwrap();
    assert_eq!(ui.label_text(label), Some("Unavailable"));
    bindings.sync(&mut ui);
    assert_eq!(bindings.drain_diagnostics().len(), 1);
    let mut root = data.borrow().root().as_object().unwrap().clone();
    root.insert("Missing".into(), "Available".into());
    data.borrow_mut().set("", DataValue::Object(root)).unwrap();
    bindings.sync(&mut ui);
    assert_eq!(ui.label_text(label), Some("Available"));
}

#[test]
fn schema_errors_reject_reverse_conversion_without_accepting_editor() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let mut data = BindingContext::new(DataValue::object([("Name", "Valid".into())])).unwrap();
    data.set_schema(ValueSchema::object([("Name", ValueSchema::String)]))
        .unwrap();
    let data = Rc::new(RefCell::new(data));
    let mut bindings = ControlBindings::new(data);
    bindings
        .converters()
        .borrow_mut()
        .register(
            "WrongBack",
            BindingConverter::new(|value| Ok(value.clone())).with_reverse(|_| Ok(1.0.into())),
        )
        .unwrap();
    let id = ui
        .edit_box(ui.root(), "Name", "", LayoutStyle::default())
        .unwrap();
    let mut link = ControlBinding::new(id, BindingProperty::Text, "Name").unwrap();
    link.mode = ControlBindingMode::TwoWay;
    link.converter = Some("WrongBack".into());
    bindings.add(link).unwrap();
    bindings.initialize(&mut ui).unwrap();
    ui.set_text(id, "Changed").unwrap();
    assert!(matches!(
        ui.commit_edit(id).unwrap(),
        CommitOutcome::Invalid(_)
    ));
    assert_eq!(ui.accepted_value(id), Some("Valid"));
}

#[test]
fn element_two_way_checks_source_model_validator_before_target_acceptance() {
    let (mut ui, data, mut bindings) = setup();
    data.borrow_mut()
        .set_field_validator(
            "Name",
            Some(BindingValidator::new(|value| {
                if value.as_str() == Some("Rejected") {
                    Err("Reserved name".into())
                } else {
                    Ok(())
                }
            })),
        )
        .unwrap();
    let source = text_link(
        &mut ui,
        &mut bindings,
        ControlBindingMode::TwoWay,
        BindingUpdateTrigger::LostFocus,
    );
    let target = ui
        .edit_box(ui.root(), "Mirror", "", LayoutStyle::default())
        .unwrap();
    let mut link = ControlBinding::new(target, BindingProperty::Text, "Name").unwrap();
    link.source = BindingSource::Element(BindingEndpoint {
        widget: source,
        property: BindingProperty::Text,
    });
    link.mode = ControlBindingMode::TwoWay;
    link.trigger = BindingUpdateTrigger::LostFocus;
    bindings.add(link).unwrap();
    bindings.initialize(&mut ui).unwrap();
    ui.set_text(target, "Rejected").unwrap();
    assert!(matches!(
        ui.commit_edit(target).unwrap(),
        CommitOutcome::Invalid(_)
    ));
    assert_eq!(ui.accepted_value(target), Some("Anna"));
    assert_eq!(ui.accepted_value(source), Some("Anna"));
    ui.set_text(target, "Boris").unwrap();
    ui.commit_edit(target).unwrap();
    bindings.sync(&mut ui);
    bindings.sync(&mut ui);
    assert_eq!(data.borrow().get("Name").unwrap().as_str(), Some("Boris"));
}

#[test]
fn removing_selected_item_publishes_null_without_losing_other_items() {
    let (mut ui, data, mut bindings) = setup();
    let list = ui
        .list_box(
            ui.root(),
            "Items",
            Vec::new(),
            SelectionSettings::default(),
            LayoutStyle::default(),
        )
        .unwrap();
    bindings
        .add(ControlBinding::new(list.container, BindingProperty::ItemsSource, "Items").unwrap())
        .unwrap();
    let mut selected =
        ControlBinding::new(list.container, BindingProperty::SelectedValue, "Selected").unwrap();
    selected.mode = ControlBindingMode::TwoWay;
    bindings.add(selected).unwrap();
    bindings.initialize(&mut ui).unwrap();
    data.borrow_mut()
        .set(
            "Items",
            DataValue::List(vec![DataValue::object([
                ("key", "b".into()),
                ("label", "Beta".into()),
            ])]),
        )
        .unwrap();
    bindings.sync(&mut ui);
    assert_eq!(ui.selected_keys(list.container).unwrap(), &[] as &[String]);
    assert_eq!(data.borrow().get("Selected").unwrap(), &DataValue::Null);
    assert_eq!(ui.selection_items(list.container).unwrap().len(), 1);
}

#[test]
fn scalar_identity_encoding_has_no_string_number_collisions() {
    let values = DataValue::List(vec![
        "\u{1f}number:1".into(),
        1.0.into(),
        true.into(),
        "true".into(),
    ]);
    let (rows, state) = map_items(&values, &ItemBindingMapping::default()).unwrap();
    assert_eq!(
        rows.iter()
            .map(|item| &item.key)
            .collect::<HashSet<_>>()
            .len(),
        4
    );
    assert_eq!(
        selected_data(&[rows[0].key.clone()], false, Some(&state)),
        "\u{1f}number:1".into()
    );
    assert!(
        map_items(
            &DataValue::List(vec![DataValue::Number(0.0), DataValue::Number(-0.0)]),
            &ItemBindingMapping::default()
        )
        .is_err()
    );
}

#[test]
fn visibility_collapsed_releases_layout_space_while_hidden_retains_it() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let data = Rc::new(RefCell::new(
        BindingContext::new(DataValue::object([("State", "Hidden".into())])).unwrap(),
    ));
    let mut bindings = ControlBindings::new(data.clone());
    let first = ui
        .label(
            ui.root(),
            "First",
            LayoutStyle {
                height: Length::Px(40.0),
                ..Default::default()
            },
        )
        .unwrap();
    let second = ui
        .label(
            ui.root(),
            "Second",
            LayoutStyle {
                height: Length::Px(30.0),
                ..Default::default()
            },
        )
        .unwrap();
    bindings
        .add(ControlBinding::new(first, BindingProperty::Visibility, "State").unwrap())
        .unwrap();
    bindings.initialize(&mut ui).unwrap();
    ui.update(rust_desktop_ui_core::Size::new(300.0, 200.0))
        .unwrap();
    let hidden_y = ui.runtime.bounds(second).unwrap().y;
    data.borrow_mut().set("State", "Collapsed".into()).unwrap();
    bindings.sync(&mut ui);
    ui.update(rust_desktop_ui_core::Size::new(300.0, 200.0))
        .unwrap();
    let collapsed_y = ui.runtime.bounds(second).unwrap().y;
    assert!(
        hidden_y - collapsed_y >= 39.0,
        "Hidden y={hidden_y}; collapsed y={collapsed_y}"
    );
    assert!(ui.runtime.tree().node(first).unwrap().props.collapsed);
    data.borrow_mut().set("State", "Visible".into()).unwrap();
    bindings.sync(&mut ui);
    ui.update(rust_desktop_ui_core::Size::new(300.0, 200.0))
        .unwrap();
    assert!(!ui.runtime.tree().node(first).unwrap().props.collapsed);
    assert_eq!(ui.runtime.bounds(second).unwrap().y, hidden_y);
}

#[test]
fn alias_properties_cannot_bypass_duplicate_or_cycle_checks() {
    let (mut ui, _, mut bindings) = setup();
    let editor = ui
        .edit_box(ui.root(), "Value", "", LayoutStyle::default())
        .unwrap();
    let mut link = ControlBinding::new(editor, BindingProperty::Text, "Name").unwrap();
    link.source = BindingSource::Element(BindingEndpoint {
        widget: editor,
        property: BindingProperty::Value,
    });
    assert!(bindings.add(link).is_err());
    bindings
        .add(ControlBinding::new(editor, BindingProperty::Text, "Name").unwrap())
        .unwrap();
    assert!(
        bindings
            .add(ControlBinding::new(editor, BindingProperty::Value, "Other").unwrap())
            .is_err()
    );
}
