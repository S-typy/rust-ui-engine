use super::*;
use crate::{
    BindingConverter, BindingExpression, BindingValidator, CheckState, CrossFieldRule,
    SelectionItem, Theme,
};
use rust_desktop_ui_core::LayoutStyle;

fn setup() -> (Controls, Rc<RefCell<BindingContext>>, ControlBindings) {
    let ui = Controls::new(Theme::FluentLight).unwrap();
    let model = Rc::new(RefCell::new(
        BindingContext::new(DataValue::object([
            ("A", 2.0.into()),
            ("B", 3.0.into()),
            ("Other", 0.0.into()),
        ]))
        .unwrap(),
    ));
    let bindings = ControlBindings::new(Rc::clone(&model));
    (ui, model, bindings)
}
fn expression_link(id: WidgetId, inputs: Vec<BindingInput>, expr: &str) -> ControlBinding {
    let mut binding = ControlBinding::new(id, BindingProperty::Text, "A").unwrap();
    binding.source = BindingSource::Expression {
        inputs,
        expression: BindingExpression::parse(expr).unwrap(),
    };
    binding
}
fn path(path: &str) -> BindingInput {
    BindingInput::DataContext(BindingPath::parse(path).unwrap())
}
#[test]
fn expression_dependency_changes_update_only_affected_caption() {
    let (mut ui, model, mut bindings) = setup();
    let caption = ui.label(ui.root(), "", LayoutStyle::default()).unwrap();
    bindings
        .add(expression_link(
            caption,
            vec![path("A"), path("B")],
            "concat('Total ', $0 + $1)",
        ))
        .unwrap();
    bindings.initialize(&mut ui).unwrap();
    assert_eq!(ui.label_text(caption), Some("Total 5"));
    model.borrow_mut().set("Other", 1.0.into()).unwrap();
    assert!(!bindings.sync(&mut ui));
    model.borrow_mut().set("A", 7.0.into()).unwrap();
    assert!(bindings.sync(&mut ui));
    assert_eq!(ui.label_text(caption), Some("Total 10"));
}
#[test]
fn multi_converter_receives_ordered_values_and_replacement_invalidates() {
    let (mut ui, _, mut bindings) = setup();
    let caption = ui.label(ui.root(), "", LayoutStyle::default()).unwrap();
    bindings
        .converters()
        .borrow_mut()
        .register(
            "Sum",
            BindingConverter::new(|value| {
                let values = value.as_list().unwrap();
                Ok(
                    (values[0].as_number().unwrap() + values[1].as_number().unwrap())
                        .to_string()
                        .into(),
                )
            }),
        )
        .unwrap();
    let mut binding = ControlBinding::new(caption, BindingProperty::Text, "A").unwrap();
    binding.source = BindingSource::Multi(vec![path("A"), path("B")]);
    binding.converter = Some("Sum".into());
    bindings.add(binding).unwrap();
    bindings.initialize(&mut ui).unwrap();
    assert_eq!(ui.label_text(caption), Some("5"));
    bindings
        .converters()
        .borrow_mut()
        .register("Sum", BindingConverter::new(|_| Ok("replacement".into())))
        .unwrap();
    assert!(bindings.sync(&mut ui));
    assert_eq!(ui.label_text(caption), Some("replacement"));
}
#[test]
fn multi_element_cycles_fail_before_initial_transfer() {
    let (mut ui, _, mut bindings) = setup();
    let a = ui.label(ui.root(), "A", LayoutStyle::default()).unwrap();
    let b = ui.label(ui.root(), "B", LayoutStyle::default()).unwrap();
    let input = |id| {
        BindingInput::Element(BindingEndpoint {
            widget: id,
            property: BindingProperty::Text,
        })
    };
    bindings
        .add(expression_link(a, vec![input(b)], "$0"))
        .unwrap();
    assert!(
        bindings
            .add(expression_link(b, vec![input(a)], "$0"))
            .is_err()
    );
    assert_eq!(ui.label_text(a), Some("A"));
    let mut reverse = expression_link(b, vec![path("A")], "$0");
    reverse.mode = ControlBindingMode::TwoWay;
    assert!(bindings.add(reverse).is_err());
}
#[test]
fn declarative_cross_field_validation_is_atomic() {
    let (_, model, _) = setup();
    let rule = CrossFieldRule {
        paths: vec![
            BindingPath::parse("A").unwrap(),
            BindingPath::parse("B").unwrap(),
        ],
        condition: BindingExpression::parse("$0 <= $1").unwrap(),
        message: "A must not exceed B".into(),
    };
    model
        .borrow_mut()
        .set_validator(Some(BindingValidator::from_rules(vec![rule]).unwrap()))
        .unwrap();
    assert!(model.borrow_mut().set("A", 4.0.into()).is_err());
    assert_eq!(model.borrow().revision(), 0);
    assert_eq!(model.borrow().get("A").unwrap(), &2.0.into());
    model
        .borrow_mut()
        .set_many([("A", 4.0.into()), ("B", 5.0.into())])
        .unwrap();
    assert_eq!(model.borrow().revision(), 1);
}
#[test]
fn nullable_checks_and_radio_keys_round_trip() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let check = ui
        .checkbox_tristate(
            ui.root(),
            "Mixed",
            CheckState::Unchecked,
            LayoutStyle::default(),
        )
        .unwrap();
    let radio = ui
        .radio_choice_group(
            ui.root(),
            vec![
                SelectionItem::new("a", "Alpha"),
                SelectionItem::new("b", "Beta"),
            ],
            None,
            LayoutStyle::default(),
        )
        .unwrap();
    let model = Rc::new(RefCell::new(
        BindingContext::new(DataValue::object([
            ("Check", DataValue::Null),
            ("Key", "b".into()),
        ]))
        .unwrap(),
    ));
    let mut bindings = ControlBindings::new(Rc::clone(&model));
    for (widget, property, path) in [
        (check, BindingProperty::IsChecked, "Check"),
        (radio.container, BindingProperty::SelectedKey, "Key"),
    ] {
        let mut binding = ControlBinding::new(widget, property, path).unwrap();
        binding.mode = ControlBindingMode::TwoWay;
        bindings.add(binding).unwrap();
    }
    bindings.initialize(&mut ui).unwrap();
    assert_eq!(ui.check_state(check), Some(CheckState::Indeterminate));
    assert_eq!(ui.radio_selected_key(radio.container), Some("b"));
    ui.set_check_state(check, CheckState::Checked).unwrap();
    ui.set_radio_selected_key(radio.container, Some("a"))
        .unwrap();
    bindings.sync(&mut ui);
    assert_eq!(model.borrow().get("Check").unwrap(), &true.into());
    assert_eq!(model.borrow().get("Key").unwrap(), &"a".into());
}

#[test]
fn async_property_changed_does_not_publish_before_acceptance() {
    use crate::{AsyncEditorValidator, CommitOutcome};
    use std::{
        sync::{Arc, Mutex, mpsc},
        time::{Duration, Instant},
    };
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let id = ui
        .edit_box(ui.root(), "Name", "Accepted", LayoutStyle::default())
        .unwrap();
    let model = Rc::new(RefCell::new(
        BindingContext::new(DataValue::object([("Name", "Accepted".into())])).unwrap(),
    ));
    let mut bindings = ControlBindings::new(Rc::clone(&model));
    let mut binding = ControlBinding::new(id, BindingProperty::Text, "Name").unwrap();
    binding.mode = ControlBindingMode::TwoWay;
    bindings.add(binding).unwrap();
    bindings.initialize(&mut ui).unwrap();
    let (send, receive) = mpsc::channel();
    let receive = Arc::new(Mutex::new(receive));
    ui.set_async_editor_validator(
        id,
        Some(AsyncEditorValidator::new(move |_| {
            receive.lock().unwrap().recv().unwrap();
            Ok(())
        })),
    )
    .unwrap();
    ui.set_text(id, "Draft").unwrap();
    assert!(!bindings.sync(&mut ui));
    assert_eq!(ui.commit_edit(id).unwrap(), CommitOutcome::Pending);
    assert_eq!(model.borrow().get("Name").unwrap(), &"Accepted".into());
    send.send(()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    while ui.accepted_value(id) != Some("Draft") && Instant::now() < deadline {
        ui.poll_editor_validation().unwrap();
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(ui.accepted_value(id), Some("Draft"));
    assert!(bindings.sync(&mut ui));
    assert_eq!(model.borrow().get("Name").unwrap(), &"Draft".into());
}

#[test]
fn range_fallback_keeps_missing_path_diagnostic_and_never_publishes_fallback() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let range = ui
        .range_slider(
            ui.root(),
            crate::RangeSliderSettings::default(),
            LayoutStyle::default(),
        )
        .unwrap();
    let model = Rc::new(RefCell::new(
        BindingContext::new(DataValue::object([(
            "Values",
            DataValue::object([("Upper", 20.0.into())]),
        )]))
        .unwrap(),
    ));
    let mut bindings = ControlBindings::new(model.clone());
    let mut lower =
        ControlBinding::new(range, BindingProperty::LowerValue, "Values.Lower").unwrap();
    lower.mode = ControlBindingMode::TwoWay;
    lower.fallback = Some(10.0.into());
    bindings.add(lower).unwrap();
    let mut upper =
        ControlBinding::new(range, BindingProperty::UpperValue, "Values.Upper").unwrap();
    upper.mode = ControlBindingMode::TwoWay;
    bindings.add(upper).unwrap();
    bindings.initialize(&mut ui).unwrap();
    assert_eq!(ui.range_slider_values(range), Some((10.0, 20.0)));
    assert_eq!(model.borrow().revision(), 0);
    assert_eq!(bindings.diagnostics().len(), 1);
    assert!(bindings.diagnostics()[0].message.contains("Values.Lower"));
    assert!(!bindings.sync(&mut ui));
    assert_eq!(bindings.diagnostics().len(), 1);
    model
        .borrow_mut()
        .set(
            "Values",
            DataValue::object([("Lower", 10.0.into()), ("Upper", 20.0.into())]),
        )
        .unwrap();
    bindings.sync(&mut ui);
    assert!(bindings.links.iter().all(|link| link.error.is_none()));
    ui.set_range_slider_values(range, 60.0, 70.0).unwrap();
    assert!(bindings.sync(&mut ui));
    assert_eq!(model.borrow().get("Values.Lower").unwrap(), &60.0.into());
    assert_eq!(model.borrow().get("Values.Upper").unwrap(), &70.0.into());
}
