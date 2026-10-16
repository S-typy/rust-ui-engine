use super::*;
use crate::{
    BindingContext, BindingProperty, BindingUpdateTrigger, ControlBinding, ControlBindingMode,
    ControlBindings, DataValue, UiCulture,
};
use std::{cell::RefCell, rc::Rc};
fn decimal(text: &str) -> DecimalValue {
    text.parse().unwrap()
}
fn setup() -> (Controls, EditorControl) {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let parts = ui
        .numeric_edit(
            ui.root(),
            "Amount",
            Some(decimal("12.3")),
            NumericEditSettings::default(),
            LayoutStyle {
                width: Length::Px(280.0),
                height: Length::Px(36.0),
                ..Default::default()
            },
        )
        .unwrap();
    frame(&mut ui);
    (ui, parts)
}
fn frame(ui: &mut Controls) {
    ui.update(Size::new(640.0, 480.0)).unwrap();
}
fn type_number(ui: &mut Controls, id: WidgetId, digit: u8) {
    ui.press_numeric_calculator(id, CalculatorKey::Clear)
        .unwrap();
    ui.press_numeric_calculator(id, CalculatorKey::Digit(digit))
        .unwrap();
}

#[test]
fn decimal_cultures_roundtrip_extremes_grouping_and_nullable_are_exact() {
    for (culture, text) in [
        (
            UiCulture::Russian,
            "9\u{a0}007\u{a0}199\u{a0}254\u{a0}740\u{a0}993,01",
        ),
        (UiCulture::English, "9,007,199,254,740,993.01"),
        (UiCulture::Arabic, "9٬007٬199٬254٬740٬993٫01"),
    ] {
        let settings = NumericEditSettings {
            minimum: decimal("-999999999999999999999999999999"),
            maximum: decimal("999999999999999999999999999999"),
            culture,
            grouping: true,
            ..Default::default()
        };
        settings.validate().unwrap();
        let value = settings.parse(text).unwrap();
        assert_eq!(value, Some(decimal("9007199254740993.01")));
        assert_eq!(settings.format(value).unwrap(), text);
        assert_eq!(settings.parse("  ").unwrap(), None);
        assert!(settings.parse("NaN").is_err());
        assert!(settings.parse("1e3").is_err());
        assert!(settings.parse("12,34,567").is_err());
        assert!(settings.parse_storage(Some("+1")).is_err());
        assert!(settings.parse_storage(Some("01")).is_err());
        assert!(settings.parse_storage(Some("1.00")).is_err());
        assert_eq!(
            settings.parse_storage(Some("1.01")).unwrap(),
            Some(decimal("1.01"))
        );
    }
    let settings = NumericEditSettings {
        allow_empty: false,
        ..Default::default()
    };
    assert!(settings.parse("").is_err());
    assert!(settings.parse("1,001").is_err());
    assert_eq!(settings.parse("1,000").unwrap(), Some(decimal("1")));
    assert!(
        NumericEditSettings {
            precision: 1,
            step: decimal("0.01"),
            ..Default::default()
        }
        .validate()
        .is_err()
    );
    assert!(
        NumericEditSettings {
            precision: 29,
            ..Default::default()
        }
        .validate()
        .is_err()
    );
}

#[test]
fn calculator_math_exactness_rounding_cancellation_and_overflow() {
    use NumericOperation::*;
    let calc = |a: &str, b: &str, op, p| {
        calculate_decimal(
            decimal(a),
            decimal(b),
            op,
            p,
            NumericRounding::HalfAwayFromZero,
        )
    };
    assert_eq!(calc("0.1", "0.2", Add, 2).unwrap(), decimal("0.3"));
    assert_eq!(
        calc("9007199254740993.01", "0.01", Add, 2).unwrap(),
        decimal("9007199254740993.02")
    );
    assert_eq!(calc("1", "8", Divide, 2).unwrap(), decimal("0.13"));
    assert_eq!(calc("-1", "8", Divide, 2).unwrap(), decimal("-0.13"));
    assert_eq!(
        calc("1", "3", Divide, 28).unwrap(),
        decimal("0.3333333333333333333333333333")
    );
    assert_eq!(calc("1.005", "1", Multiply, 2).unwrap(), decimal("1.01"));
    let max = i128::MAX.to_string();
    let min = i128::MIN.to_string();
    assert_eq!(calc(&max, &max, Subtract, 28).unwrap(), decimal("0"));
    assert_eq!(
        calc(&min, "0.0000000000000000000000000001", Multiply, 28).unwrap(),
        decimal("-17014118346.0469231731687303715884105728")
    );
    assert_eq!(calc(&max, &max, Divide, 28).unwrap(), decimal("1"));
    assert_eq!(
        calc(
            "0.0000000000000000000000000001",
            "0.0000000000000000000000000001",
            Divide,
            28
        )
        .unwrap(),
        decimal("1")
    );
    assert!(calc(&max, "1", Add, 0).is_err());
    assert!(calc("1", "0", Divide, 2).is_err());
    // A small exhaustive integer reference catches carry, sign, tie rounding and division errors.
    for a in -31i128..=31 {
        for b in -19i128..=19 {
            let left = DecimalValue::new(a, 1).unwrap();
            let right = DecimalValue::new(b, 1).unwrap();
            assert_eq!(
                calculate_decimal(left, right, Add, 2, NumericRounding::HalfAwayFromZero).unwrap(),
                DecimalValue::new(a + b, 1).unwrap()
            );
            assert_eq!(
                calculate_decimal(left, right, Multiply, 2, NumericRounding::HalfAwayFromZero)
                    .unwrap(),
                DecimalValue::new(a * b, 2).unwrap()
            );
            if b != 0 {
                let n = a * 100;
                let q = n / b;
                let r = n % b;
                let rounded = q + if r.abs() * 2 >= b.abs() {
                    if (n < 0) == (b < 0) { 1 } else { -1 }
                } else {
                    0
                };
                assert_eq!(
                    calculate_decimal(left, right, Divide, 2, NumericRounding::HalfAwayFromZero)
                        .unwrap(),
                    DecimalValue::new(rounded, 2).unwrap()
                );
            }
        }
    }
}

#[test]
fn numeric_step_saturates_without_intermediate_overflow() {
    let settings = NumericEditSettings {
        minimum: DecimalValue::new(i128::MIN, 0).unwrap(),
        maximum: DecimalValue::new(i128::MAX, 0).unwrap(),
        step: decimal("10"),
        precision: 0,
        ..Default::default()
    };
    assert_eq!(
        settings
            .stepped(Some(DecimalValue::new(i128::MAX - 1, 0).unwrap()), true)
            .unwrap(),
        settings.maximum
    );
    assert_eq!(
        settings
            .stepped(Some(DecimalValue::new(i128::MIN + 1, 0).unwrap()), false)
            .unwrap(),
        settings.minimum
    );
    assert_eq!(settings.stepped(None, false).unwrap(), decimal("0"));
    let settings = NumericEditSettings {
        minimum: decimal("5"),
        maximum: decimal("7"),
        ..Default::default()
    };
    assert_eq!(settings.stepped(None, true).unwrap(), decimal("5"));
}

#[test]
fn calculator_model_error_preserves_pending_operation_and_entry() {
    let mut model = NumericCalculator::new(Some(decimal("9")), 2).unwrap();
    model
        .press(CalculatorKey::Operation(NumericOperation::Divide))
        .unwrap();
    model.press(CalculatorKey::Digit(0)).unwrap();
    let before = model.clone();
    assert!(model.press(CalculatorKey::Equals).is_err());
    assert_eq!(model, before);
    model.press(CalculatorKey::Digit(3)).unwrap();
    model.press(CalculatorKey::Equals).unwrap();
    assert_eq!(model.value().unwrap(), decimal("3"));
    let before = model.clone();
    assert!(model.press(CalculatorKey::Digit(10)).is_err());
    assert_eq!(model, before);
}

#[test]
fn numeric_draft_commit_cancel_step_events_and_legacy_are_independent() {
    let (mut ui, p) = setup();
    assert_eq!(ui.document(p.editor).unwrap().text(), "12,30");
    ui.set_text(p.editor, "0,10").unwrap();
    ui.commit_edit(p.editor).unwrap();
    ui.step_value(p.container, 1).unwrap();
    assert_eq!(ui.numeric_value(p.container), Some(Some(decimal("1.1"))));
    ui.set_text(p.editor, "1,001").unwrap();
    assert!(matches!(
        ui.commit_edit(p.editor).unwrap(),
        CommitOutcome::Invalid(_)
    ));
    assert_eq!(ui.numeric_value(p.container), Some(Some(decimal("1.1"))));
    ui.cancel_edit(p.editor).unwrap();
    assert_eq!(ui.document(p.editor).unwrap().text(), "1,10");
    ui.set_numeric_value(p.container, None).unwrap();
    assert_eq!(ui.numeric_value(p.container), Some(None));
    ui.step_value(p.container, 1).unwrap();
    assert_eq!(ui.numeric_value(p.container), Some(Some(decimal("0"))));
    assert!(
        ui.drain_value_changes()
            .iter()
            .any(|e| e.value == EditorValue::Decimal(Some(decimal("1.1"))))
    );
    let legacy = ui
        .spin_box(
            ui.root(),
            "Legacy",
            0.5,
            SpinBoxSettings::default(),
            LayoutStyle::default(),
        )
        .unwrap();
    ui.step_value(legacy.container, 1).unwrap();
    assert_eq!(ui.spin_value(legacy.container), Some(1.5));
}

#[test]
fn calculator_cancel_before_apply_preserves_original_draft_undo_and_acceptance() {
    let (mut ui, p) = setup();
    ui.set_text(p.editor, "13,50").unwrap();
    let selection = ui.document(p.editor).unwrap().selection();
    let undo = ui.document(p.editor).unwrap().can_undo();
    ui.drain_edit_events();
    ui.open_numeric_calculator(p.container).unwrap();
    type_number(&mut ui, p.container, 8);
    ui.cancel_numeric_calculator(p.container).unwrap();
    assert_eq!(ui.document(p.editor).unwrap().text(), "13,50");
    assert_eq!(ui.document(p.editor).unwrap().selection(), selection);
    assert_eq!(ui.document(p.editor).unwrap().can_undo(), undo);
    assert_eq!(ui.numeric_value(p.container), Some(Some(decimal("12.3"))));
    assert!(ui.drain_edit_events().is_empty());
    ui.open_numeric_calculator(p.container).unwrap();
    type_number(&mut ui, p.container, 9);
    ui.dispatch(InputEvent::WindowFocused(false)).unwrap();
    assert!(!ui.numeric_calculator_is_open(p.container));
    assert_ne!(ui.numeric_value(p.container), Some(Some(decimal("9"))));
}

#[test]
fn calculator_apply_uses_validation_and_stays_open_on_invalid() {
    let (mut ui, p) = setup();
    ui.set_editor_validator(
        p.editor,
        Some(EditorValidator::new(|text| {
            if text == "8,00" {
                Err("Eight is reserved".into())
            } else {
                Ok(())
            }
        })),
    )
    .unwrap();
    ui.open_numeric_calculator(p.container).unwrap();
    type_number(&mut ui, p.container, 8);
    assert!(matches!(
        ui.apply_numeric_calculator(p.container).unwrap(),
        CommitOutcome::Invalid(_)
    ));
    assert!(ui.numeric_calculator_is_open(p.container));
    assert_eq!(
        ui.numeric_calculator(p.container).unwrap().value().unwrap(),
        decimal("8")
    );
    assert_eq!(ui.document(p.editor).unwrap().text(), "12,30");
    type_number(&mut ui, p.container, 9);
    assert_eq!(
        ui.apply_numeric_calculator(p.container).unwrap(),
        CommitOutcome::Committed
    );
    assert!(!ui.numeric_calculator_is_open(p.container));
    assert_eq!(ui.numeric_value(p.container), Some(Some(decimal("9"))));
}

#[test]
fn calculator_lifecycle_readonly_disable_remove_and_protected_parts() {
    let (mut ui, p) = setup();
    // The disclosure is a required part even before its lazy popup is created.
    assert!(ui.remove(p.buttons[2]).is_err());
    let mut settings = ui.numeric_settings(p.container).unwrap().clone();
    settings.culture = UiCulture::English;
    ui.set_numeric_settings(p.container, settings).unwrap();
    assert_eq!(ui.numeric_value(p.container), Some(Some(decimal("12.3"))));
    assert_eq!(ui.document(p.editor).unwrap().text(), "12.30");
    ui.open_numeric_calculator(p.container).unwrap();
    let focused = ui.runtime.focused().unwrap();
    assert!(ui.remove(focused).is_err());
    ui.set_composite_read_only(p.container, true).unwrap();
    frame(&mut ui);
    assert!(!ui.numeric_calculator_is_open(p.container));
    assert!(ui.open_numeric_calculator(p.container).is_err());
    ui.set_composite_read_only(p.container, false).unwrap();
    ui.open_numeric_calculator(p.container).unwrap();
    ui.set_enabled(p.container, false).unwrap();
    frame(&mut ui);
    assert!(!ui.numeric_calculator_is_open(p.container));
    ui.set_enabled(p.container, true).unwrap();
    frame(&mut ui);
    ui.open_numeric_calculator(p.container).unwrap();
    ui.remove(p.container).unwrap();
    frame(&mut ui);
    assert!(ui.numeric_value(p.container).is_none());
}

fn bind(
    ui: &mut Controls,
    p: &EditorControl,
    trigger: BindingUpdateTrigger,
) -> (Rc<RefCell<BindingContext>>, ControlBindings) {
    let data = Rc::new(RefCell::new(
        BindingContext::new(DataValue::object([("Amount", "12.3".into())])).unwrap(),
    ));
    let mut bindings = ControlBindings::new(Rc::clone(&data));
    let mut link = ControlBinding::new(p.container, BindingProperty::Value, "Amount").unwrap();
    link.mode = ControlBindingMode::TwoWay;
    link.trigger = trigger;
    bindings.add(link).unwrap();
    bindings.initialize(ui).unwrap();
    (data, bindings)
}
#[test]
fn decimal_binding_uses_canonical_strings_null_and_rejects_lossy_numbers() {
    let (mut ui, p) = setup();
    let (data, mut bindings) = bind(&mut ui, &p, BindingUpdateTrigger::LostFocus);
    ui.set_text(p.editor, "3,25").unwrap();
    bindings.sync(&mut ui);
    assert_eq!(data.borrow().get("Amount").unwrap().as_str(), Some("12.3"));
    ui.commit_edit(p.editor).unwrap();
    bindings.sync(&mut ui);
    assert_eq!(data.borrow().get("Amount").unwrap().as_str(), Some("3.25"));
    data.borrow_mut().set("Amount", DataValue::Null).unwrap();
    bindings.sync(&mut ui);
    assert_eq!(ui.numeric_value(p.container), Some(None));
    data.borrow_mut()
        .set("Amount", DataValue::Number(9007199254740992.0))
        .unwrap();
    bindings.sync(&mut ui);
    assert_eq!(ui.numeric_value(p.container), Some(None));
}

#[test]
fn async_calculator_cancel_restores_local_session_and_ordinary_binding_notifications() {
    use std::sync::{Arc, Mutex, mpsc};
    use std::time::Duration;
    for trigger in [
        BindingUpdateTrigger::LostFocus,
        BindingUpdateTrigger::PropertyChanged,
    ] {
        for disable in [false, true] {
            let (mut ui, p) = setup();
            let (data, mut bindings) = bind(&mut ui, &p, trigger);
            ui.set_text(p.editor, "13,50").unwrap();
            bindings.sync(&mut ui);
            let before = data.borrow().get("Amount").unwrap().clone();
            let selection = ui.document(p.editor).unwrap().selection();
            let (release, gate) = mpsc::channel();
            let gate = Arc::new(Mutex::new(gate));
            let (wake, done) = mpsc::channel();
            ui.set_editor_validation_waker(Some(Arc::new(move || {
                let _ = wake.send(());
            })));
            ui.set_async_editor_validator(
                p.editor,
                Some(crate::AsyncEditorValidator::new(move |_| {
                    gate.lock().unwrap().recv().unwrap();
                    Ok(())
                })),
            )
            .unwrap();
            ui.open_numeric_calculator(p.container).unwrap();
            type_number(&mut ui, p.container, 7);
            assert_eq!(
                ui.apply_numeric_calculator(p.container).unwrap(),
                CommitOutcome::Pending
            );
            bindings.sync(&mut ui);
            assert_eq!(data.borrow().get("Amount").unwrap(), &before);
            if disable {
                ui.set_enabled(p.container, false).unwrap();
                frame(&mut ui);
            } else {
                ui.cancel_numeric_calculator(p.container).unwrap();
            }
            bindings.sync(&mut ui);
            assert_eq!(data.borrow().get("Amount").unwrap(), &before);
            assert_eq!(ui.document(p.editor).unwrap().text(), "13,50");
            assert_eq!(ui.document(p.editor).unwrap().selection(), selection);
            assert!(ui.document(p.editor).unwrap().can_undo());
            release.send(()).unwrap();
            done.recv_timeout(Duration::from_secs(2)).unwrap();
            frame(&mut ui);
            assert_eq!(ui.numeric_value(p.container), Some(Some(decimal("12.3"))));
            assert!(!ui.numeric_calculator_is_open(p.container));
        }
    }
}

#[test]
fn async_calculator_acceptance_closes_only_after_accepted_value() {
    use std::sync::{Arc, Mutex, mpsc};
    use std::time::Duration;
    let (mut ui, p) = setup();
    let (release, gate) = mpsc::channel();
    let gate = Arc::new(Mutex::new(gate));
    let (wake, done) = mpsc::channel();
    ui.set_editor_validation_waker(Some(Arc::new(move || {
        let _ = wake.send(());
    })));
    ui.set_async_editor_validator(
        p.editor,
        Some(crate::AsyncEditorValidator::new(move |_| {
            gate.lock().unwrap().recv().unwrap();
            Ok(())
        })),
    )
    .unwrap();
    ui.open_numeric_calculator(p.container).unwrap();
    type_number(&mut ui, p.container, 7);
    assert_eq!(
        ui.apply_numeric_calculator(p.container).unwrap(),
        CommitOutcome::Pending
    );
    assert!(ui.numeric_calculator_is_open(p.container));
    assert_eq!(ui.numeric_value(p.container), Some(Some(decimal("12.3"))));
    release.send(()).unwrap();
    done.recv_timeout(Duration::from_secs(2)).unwrap();
    frame(&mut ui);
    assert_eq!(ui.numeric_value(p.container), Some(Some(decimal("7"))));
    assert!(!ui.numeric_calculator_is_open(p.container));
}

#[test]
fn numeric_settings_are_atomic_and_existing_bindings_follow_culture() {
    let (mut ui, p) = setup();
    let (data, mut bindings) = bind(&mut ui, &p, BindingUpdateTrigger::PropertyChanged);
    let mut settings = ui.numeric_settings(p.container).unwrap().clone();
    settings.culture = UiCulture::English;
    settings.grouping = true;
    ui.set_numeric_settings(p.container, settings.clone())
        .unwrap();
    assert_eq!(ui.document(p.editor).unwrap().text(), "12.30");
    ui.set_text(p.editor, "1,234.50").unwrap();
    assert!(bindings.sync(&mut ui));
    assert_eq!(
        data.borrow().get("Amount").unwrap().as_str(),
        Some("1234.5")
    );
    ui.commit_edit(p.editor).unwrap();
    let before = ui.document(p.editor).unwrap().text().to_owned();
    let accepted = ui.numeric_value(p.container);
    settings.maximum = decimal("100");
    assert!(ui.set_numeric_settings(p.container, settings).is_err());
    assert_eq!(ui.document(p.editor).unwrap().text(), before);
    assert_eq!(ui.numeric_value(p.container), accepted);
    ui.set_max_length(p.editor, Some(before.len())).unwrap();
    let mut settings = ui.numeric_settings(p.container).unwrap().clone();
    settings.precision = 10;
    assert!(ui.set_numeric_settings(p.container, settings).is_err());
    assert_eq!(ui.numeric_settings(p.container).unwrap().precision, 2);
    ui.set_text(p.editor, "3.50").unwrap();
    assert!(bindings.sync(&mut ui));
    assert_eq!(data.borrow().get("Amount").unwrap().as_str(), Some("3.5"));
}

#[test]
fn numeric_calculator_keyboard_applies_and_escape_preserves_value() {
    use rust_desktop_ui_core::Modifiers;
    let (mut ui, p) = setup();
    ui.open_numeric_calculator(p.container).unwrap();
    for key in [
        Key::Delete,
        Key::Character("2".into()),
        Key::Character("+".into()),
        Key::Character("3".into()),
        Key::Character("=".into()),
        Key::Enter,
    ] {
        ui.dispatch(InputEvent::KeyDown {
            key,
            modifiers: Modifiers::default(),
            repeat: false,
        })
        .unwrap();
    }
    assert_eq!(ui.numeric_value(p.container), Some(Some(decimal("5"))));
    assert!(!ui.numeric_calculator_is_open(p.container));
    ui.open_numeric_calculator(p.container).unwrap();
    type_number(&mut ui, p.container, 8);
    ui.dispatch(InputEvent::KeyDown {
        key: Key::Escape,
        modifiers: Modifiers::default(),
        repeat: false,
    })
    .unwrap();
    assert_eq!(ui.numeric_value(p.container), Some(Some(decimal("5"))));
}

#[test]
fn numeric_queued_async_result_cannot_win_disable_or_window_blur() {
    use std::sync::{Arc, Mutex, mpsc};
    use std::time::Duration;
    for blur in [false, true] {
        let (mut ui, p) = setup();
        let (release, gate) = mpsc::channel();
        let gate = Arc::new(Mutex::new(gate));
        let (wake, done) = mpsc::channel();
        ui.set_editor_validation_waker(Some(Arc::new(move || {
            let _ = wake.send(());
        })));
        ui.set_async_editor_validator(
            p.editor,
            Some(crate::AsyncEditorValidator::new(move |_| {
                gate.lock().unwrap().recv().unwrap();
                Ok(())
            })),
        )
        .unwrap();
        ui.open_numeric_calculator(p.container).unwrap();
        type_number(&mut ui, p.container, 7);
        assert_eq!(
            ui.apply_numeric_calculator(p.container).unwrap(),
            CommitOutcome::Pending
        );
        release.send(()).unwrap();
        done.recv_timeout(Duration::from_secs(2)).unwrap();
        if blur {
            ui.dispatch(InputEvent::WindowFocused(false)).unwrap();
        } else {
            ui.set_enabled(p.container, false).unwrap();
            frame(&mut ui);
        }
        assert_eq!(ui.numeric_value(p.container), Some(Some(decimal("12.3"))));
        assert_eq!(ui.document(p.editor).unwrap().text(), "12,30");
        assert!(!ui.numeric_calculator_is_open(p.container));
    }
}

#[test]
fn numeric_external_value_wins_pending_calculator_cancellation() {
    use std::sync::{Arc, Mutex, mpsc};
    use std::time::Duration;
    let (mut ui, p) = setup();
    let (release, gate) = mpsc::channel();
    let gate = Arc::new(Mutex::new(gate));
    let (wake, done) = mpsc::channel();
    ui.set_editor_validation_waker(Some(Arc::new(move || {
        let _ = wake.send(());
    })));
    ui.set_async_editor_validator(
        p.editor,
        Some(crate::AsyncEditorValidator::new(move |_| {
            gate.lock().unwrap().recv().unwrap();
            Ok(())
        })),
    )
    .unwrap();
    ui.open_numeric_calculator(p.container).unwrap();
    type_number(&mut ui, p.container, 7);
    ui.apply_numeric_calculator(p.container).unwrap();
    ui.set_numeric_value(p.container, Some(decimal("88")))
        .unwrap();
    ui.cancel_numeric_calculator(p.container).unwrap();
    release.send(()).unwrap();
    done.recv_timeout(Duration::from_secs(2)).unwrap();
    frame(&mut ui);
    assert_eq!(ui.numeric_value(p.container), Some(Some(decimal("88"))));
    assert_eq!(ui.document(p.editor).unwrap().text(), "88,00");
}

#[test]
fn numeric_async_rejection_retains_calculation_and_allows_corrected_apply() {
    use std::sync::{Arc, mpsc};
    use std::time::Duration;
    let (mut ui, p) = setup();
    let (wake, done) = mpsc::channel();
    ui.set_editor_validation_waker(Some(Arc::new(move || {
        let _ = wake.send(());
    })));
    ui.set_async_editor_validator(
        p.editor,
        Some(crate::AsyncEditorValidator::new(|request| {
            if request.draft == "7,00" {
                Err("Seven is unavailable".into())
            } else {
                Ok(())
            }
        })),
    )
    .unwrap();
    ui.open_numeric_calculator(p.container).unwrap();
    type_number(&mut ui, p.container, 7);
    assert_eq!(
        ui.apply_numeric_calculator(p.container).unwrap(),
        CommitOutcome::Pending
    );
    done.recv_timeout(Duration::from_secs(2)).unwrap();
    frame(&mut ui);
    assert!(ui.numeric_calculator_is_open(p.container));
    assert_eq!(
        ui.numeric_calculator(p.container).unwrap().value().unwrap(),
        decimal("7")
    );
    assert_eq!(ui.document(p.editor).unwrap().text(), "12,30");
    type_number(&mut ui, p.container, 8);
    assert_eq!(
        ui.apply_numeric_calculator(p.container).unwrap(),
        CommitOutcome::Pending
    );
    done.recv_timeout(Duration::from_secs(2)).unwrap();
    frame(&mut ui);
    assert!(!ui.numeric_calculator_is_open(p.container));
    assert_eq!(ui.numeric_value(p.container), Some(Some(decimal("8"))));
}

#[test]
fn numeric_calculator_retains_buttons_inside_surface_after_theme_and_resize() {
    for theme in [Theme::FluentLight, Theme::FluentDark] {
        let (mut ui, p) = setup();
        ui.set_theme(theme).unwrap();
        frame(&mut ui);
        ui.open_numeric_calculator(p.container).unwrap();
        for viewport in [Size::new(640.0, 480.0), Size::new(360.0, 440.0)] {
            ui.update(viewport).unwrap();
            let ids = ui.runtime.tree().preorder();
            let buttons: Vec<_> = ids
                .into_iter()
                .filter(|id| {
                    ui.numeric_focus_editor(*id) == Some(p.editor)
                        && ui.runtime.tree().node(*id).unwrap().props.focusable
                })
                .collect();
            assert_eq!(buttons.len(), 21);
            for id in buttons {
                let bounds = ui.runtime.bounds(id).unwrap();
                let clip = ui.runtime.clip(id).unwrap();
                assert!(bounds.width > 0.0 && bounds.height > 0.0);
                assert!(
                    bounds.x >= clip.x - 0.01
                        && bounds.y >= clip.y - 0.01
                        && bounds.x + bounds.width <= clip.x + clip.width + 0.01
                        && bounds.y + bounds.height <= clip.y + clip.height + 0.01,
                    "{bounds:?} is clipped by {clip:?}"
                );
            }
        }
    }
}

#[test]
fn trailing_zero_display_option_preserves_max_precision_grouping_and_exact_roundtrip() {
    for (culture, separator, group) in [
        (UiCulture::Russian, ',', '\u{a0}'),
        (UiCulture::English, '.', ','),
        (UiCulture::Arabic, '٫', '٬'),
    ] {
        let settings = NumericEditSettings {
            precision: 28,
            grouping: true,
            culture,
            trim_trailing_zeros: true,
            ..Default::default()
        };
        for (raw, expected) in [
            ("12345.67", format!("12{group}345{separator}67")),
            ("-12345.67", format!("-12{group}345{separator}67")),
            ("12000", format!("12{group}000")),
            ("-12000", format!("-12{group}000")),
            ("0", "0".into()),
            (
                "0.1234567890123456789012345678",
                format!("0{separator}1234567890123456789012345678"),
            ),
        ] {
            let value = Some(decimal(raw));
            let text = settings.format(value).unwrap();
            assert_eq!(text, expected);
            assert_eq!(settings.parse(&text).unwrap(), value);
        }
        assert_eq!(settings.format(None).unwrap(), "");
        assert_eq!(settings.precision, 28);
    }
    assert_eq!(
        NumericEditSettings::default()
            .format(Some(decimal("12.3")))
            .unwrap(),
        "12,30"
    );
    let (mut ui, p) = setup();
    let mut settings = ui.numeric_settings(p.container).unwrap().clone();
    settings.precision = 28;
    settings.trim_trailing_zeros = true;
    ui.set_numeric_settings(p.container, settings.clone())
        .unwrap();
    assert_eq!(ui.document(p.editor).unwrap().text(), "12,3");
    let next = decimal("0.1234567890123456789012345678");
    ui.set_numeric_value(p.container, Some(next)).unwrap();
    assert_eq!(ui.numeric_value(p.container), Some(Some(next)));
    ui.set_text(p.editor, "7,").unwrap();
    let draft = ui.document(p.editor).unwrap().text().to_owned();
    settings.trim_trailing_zeros = false;
    assert!(ui.set_numeric_settings(p.container, settings).is_err());
    assert_eq!(ui.document(p.editor).unwrap().text(), draft);
}
