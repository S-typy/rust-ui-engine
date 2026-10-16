use super::*;
use crate::{
    BindingContext, BindingEndpoint, BindingProperty, BindingSource, BindingValidator,
    ControlBinding, ControlBindingMode, ControlBindings, DataValue,
};
use rust_desktop_ui_core::Modifiers;
use std::{cell::RefCell, rc::Rc};

fn frame(ui: &mut Controls) {
    ui.update(Size::new(600.0, 400.0)).unwrap();
}
fn fixture(settings: RatingSettings) -> (Controls, WidgetId) {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let id = ui
        .rating(
            ui.root(),
            "Quality",
            settings,
            LayoutStyle {
                width: Length::Px(240.0),
                height: Length::Px(40.0),
                flex_shrink: 0.0,
                ..Default::default()
            },
        )
        .unwrap();
    frame(&mut ui);
    ui.runtime.apply(Mutation::Focus(Some(id))).unwrap();
    (ui, id)
}
fn key(ui: &mut Controls, key: Key) {
    ui.dispatch(InputEvent::KeyDown {
        key,
        modifiers: Modifiers::default(),
        repeat: false,
    })
    .unwrap();
}
fn point(ui: &Controls, id: WidgetId) -> Point {
    let b = ui.runtime.bounds(id).unwrap();
    Point::new(b.x + 43.0, b.y + b.height / 2.0)
}
fn down(ui: &mut Controls, position: Point) {
    ui.dispatch(InputEvent::PointerDown {
        position,
        button: PointerButton::Primary,
        modifiers: Modifiers::default(),
    })
    .unwrap();
}
fn up(ui: &mut Controls, position: Point) {
    ui.dispatch(InputEvent::PointerUp {
        position,
        button: PointerButton::Primary,
        modifiers: Modifiers::default(),
    })
    .unwrap();
}

#[test]
fn invalid_settings_are_atomic_and_nullable_zero_fraction_are_distinct() {
    let (mut ui, id) = fixture(Default::default());
    let ids = ui.runtime.tree().live_ids();
    let bounds = ui.runtime.bounds(id);
    for settings in [
        RatingSettings {
            maximum: 0,
            ..Default::default()
        },
        RatingSettings {
            maximum: 21,
            ..Default::default()
        },
        RatingSettings {
            step: f64::NAN,
            ..Default::default()
        },
        RatingSettings {
            step: 0.0,
            ..Default::default()
        },
        RatingSettings {
            glyph_size: f32::INFINITY,
            ..Default::default()
        },
        RatingSettings {
            allow_empty: false,
            ..Default::default()
        },
        RatingSettings {
            value: Some(6.0),
            ..Default::default()
        },
    ] {
        assert!(ui.set_rating_settings(id, settings).is_err());
        assert!(
            ui.rating(ui.root(), "Invalid", settings, Default::default())
                .is_err()
        );
        assert_eq!(ui.rating_value(id), Some(None));
        assert_eq!(ui.runtime.tree().live_ids(), ids);
        assert_eq!(ui.runtime.bounds(id), bounds);
    }
    for value in [Some(0.0), Some(2.34567890123), None] {
        ui.set_rating_value(id, value).unwrap();
        assert_eq!(ui.rating_value(id), Some(value));
    }
    assert!(ui.set_rating_value(id, Some(f64::NAN)).is_err());
    assert!(ui.drain_rating_changed().is_empty());
}

#[test]
fn keyboard_preview_does_not_publish_binding_and_commits_once() {
    let (mut ui, id) = fixture(RatingSettings {
        step: 0.5,
        ..Default::default()
    });
    let model = Rc::new(RefCell::new(
        BindingContext::new(DataValue::object([("Score", DataValue::Null)])).unwrap(),
    ));
    let mut bindings = ControlBindings::new(Rc::clone(&model));
    let mut binding = ControlBinding::new(id, BindingProperty::Value, "Score").unwrap();
    binding.mode = ControlBindingMode::TwoWay;
    bindings.add(binding).unwrap();
    bindings.initialize(&mut ui).unwrap();
    key(&mut ui, Key::ArrowRight);
    bindings.sync(&mut ui);
    assert_eq!(ui.rating_preview(id), Some(RatingPreview::Value(Some(0.5))));
    assert_eq!(ui.rating_value(id), Some(None));
    assert_eq!(model.borrow().get("Score").unwrap(), &DataValue::Null);
    key(&mut ui, Key::Escape);
    assert_eq!(ui.rating_preview(id), Some(RatingPreview::None));
    key(&mut ui, Key::Home);
    key(&mut ui, Key::Enter);
    bindings.sync(&mut ui);
    assert_eq!(
        model.borrow().get("Score").unwrap(),
        &DataValue::Number(0.0)
    );
    assert_eq!(
        ui.drain_rating_changed(),
        vec![RatingChanged {
            id,
            value: Some(0.0)
        }]
    );
    key(&mut ui, Key::Enter);
    assert!(ui.drain_rating_changed().is_empty());
    key(&mut ui, Key::Delete);
    bindings.sync(&mut ui);
    assert_eq!(model.borrow().get("Score").unwrap(), &DataValue::Null);
    assert_eq!(
        ui.drain_rating_changed(),
        vec![RatingChanged { id, value: None }]
    );
}

#[test]
fn pointer_preview_release_outside_and_capture_loss_never_commit() {
    let (mut ui, id) = fixture(Default::default());
    let p = point(&ui, id);
    ui.dispatch(InputEvent::PointerMoved { position: p })
        .unwrap();
    assert!(matches!(
        ui.rating_preview(id),
        Some(RatingPreview::Value(_))
    ));
    assert_eq!(ui.rating_value(id), Some(None));
    down(&mut ui, p);
    assert_eq!(ui.runtime.captured(), Some(id));
    up(&mut ui, Point::new(500.0, 250.0));
    assert_eq!(ui.rating_value(id), Some(None));
    assert_eq!(ui.runtime.captured(), None);
    down(&mut ui, p);
    ui.runtime.apply(Mutation::CapturePointer(None)).unwrap();
    frame(&mut ui);
    up(&mut ui, p);
    assert_eq!(ui.rating_value(id), Some(None));
    assert!(ui.drain_rating_changed().is_empty());
    down(&mut ui, p);
    up(&mut ui, p);
    assert_eq!(ui.rating_value(id), Some(Some(2.0)));
    assert_eq!(ui.drain_rating_changed().len(), 1);
}

#[test]
fn blur_readonly_hide_remove_and_focus_change_clear_transient_state() {
    let (mut ui, id) = fixture(Default::default());
    let p = point(&ui, id);
    down(&mut ui, p);
    ui.dispatch(InputEvent::WindowFocused(false)).unwrap();
    assert_eq!(ui.runtime.captured(), None);
    assert_eq!(ui.rating_preview(id), Some(RatingPreview::None));
    ui.dispatch(InputEvent::WindowFocused(true)).unwrap();
    ui.runtime.apply(Mutation::Focus(Some(id))).unwrap();
    key(&mut ui, Key::End);
    ui.set_rating_read_only(id, true).unwrap();
    key(&mut ui, Key::Enter);
    assert_eq!(ui.rating_value(id), Some(None));
    ui.set_rating_read_only(id, false).unwrap();
    key(&mut ui, Key::End);
    ui.runtime.apply(Mutation::Focus(None)).unwrap();
    frame(&mut ui);
    assert_eq!(ui.rating_preview(id), Some(RatingPreview::None));
    down(&mut ui, p);
    ui.edit_props(id, |p| p.visible = false).unwrap();
    frame(&mut ui);
    assert_eq!(ui.runtime.captured(), None);
    ui.remove(id).unwrap();
    frame(&mut ui);
    assert_eq!(ui.rating_value(id), None);
    assert!(ui.drain_rating_changed().is_empty());
}

#[test]
fn semantic_actions_validate_before_accepted_value_and_honor_readonly() {
    let (mut ui, id) = fixture(RatingSettings {
        value: Some(1.0),
        step: 0.5,
        ..Default::default()
    });
    ui.set_rating_validator(
        id,
        Some(RatingValidator::new(|v| {
            if v.is_some_and(|v| v > 2.0) {
                Err("Maximum two".into())
            } else {
                Ok(())
            }
        })),
    )
    .unwrap();
    for value in ["NaN", "Infinity", "-1", "6", "2.5"] {
        ui.perform_action(id, SemanticAction::SetValue, Some(value))
            .unwrap();
        assert_eq!(ui.rating_value(id), Some(Some(1.0)));
    }
    ui.perform_action(id, SemanticAction::Increment, None)
        .unwrap();
    assert_eq!(ui.rating_value(id), Some(Some(1.5)));
    ui.perform_action(id, SemanticAction::Decrement, None)
        .unwrap();
    assert_eq!(ui.rating_value(id), Some(Some(1.0)));
    ui.perform_action(id, SemanticAction::SetValue, Some("null"))
        .unwrap();
    assert_eq!(ui.rating_value(id), Some(None));
    ui.set_rating_read_only(id, true).unwrap();
    ui.perform_action(id, SemanticAction::Increment, None)
        .unwrap();
    assert_eq!(ui.rating_value(id), Some(None));
    let semantic = ui.semantics().into_iter().find(|s| s.id == id).unwrap();
    assert_eq!(semantic.role, Role::Rating);
    assert!(semantic.read_only);
    assert!(!semantic.actions.contains(&SemanticAction::SetValue));
    assert!(semantic.value.as_deref().unwrap().contains('5'));
}

#[test]
fn model_validator_and_live_element_source_limits_reject_before_commit() {
    let (mut ui, id) = fixture(Default::default());
    let model = Rc::new(RefCell::new(
        BindingContext::new(DataValue::object([("Score", DataValue::Null)])).unwrap(),
    ));
    model
        .borrow_mut()
        .set_field_validator(
            "Score",
            Some(BindingValidator::new(|v| {
                if matches!(v,DataValue::Number(n) if *n>2.0) {
                    Err("Maximum two".into())
                } else {
                    Ok(())
                }
            })),
        )
        .unwrap();
    let mut bindings = ControlBindings::new(model.clone());
    let mut b = ControlBinding::new(id, BindingProperty::Value, "Score").unwrap();
    b.mode = ControlBindingMode::TwoWay;
    bindings.add(b).unwrap();
    bindings.initialize(&mut ui).unwrap();
    key(&mut ui, Key::End);
    key(&mut ui, Key::Enter);
    bindings.sync(&mut ui);
    assert_eq!(ui.rating_value(id), Some(None));
    assert_eq!(model.borrow().get("Score").unwrap(), &DataValue::Null);
    assert!(ui.rating_error(id).is_some());
    let source = ui
        .rating(ui.root(), "Source", Default::default(), Default::default())
        .unwrap();
    let target = ui
        .rating(ui.root(), "Target", Default::default(), Default::default())
        .unwrap();
    let mut links = ControlBindings::new(model);
    let mut b = ControlBinding::new(target, BindingProperty::Value, "Score").unwrap();
    b.mode = ControlBindingMode::TwoWay;
    b.source = BindingSource::Element(BindingEndpoint {
        widget: source,
        property: BindingProperty::Value,
    });
    links.add(b).unwrap();
    links.initialize(&mut ui).unwrap();
    ui.set_rating_settings(
        source,
        RatingSettings {
            maximum: 2,
            ..Default::default()
        },
    )
    .unwrap();
    frame(&mut ui);
    ui.runtime.apply(Mutation::Focus(Some(target))).unwrap();
    key(&mut ui, Key::End);
    key(&mut ui, Key::Enter);
    assert_eq!(ui.rating_value(target), Some(None));
    assert_eq!(ui.rating_value(source), Some(None));
    assert!(ui.rating_error(target).is_some());
}

#[test]
fn event_backpressure_preserves_value_and_theme_resize_preserve_identity() {
    let (mut ui, id) = fixture(Default::default());
    for i in 0..256 {
        ui.perform_action(
            id,
            SemanticAction::SetValue,
            Some(if i % 2 == 0 { "1" } else { "2" }),
        )
        .unwrap();
    }
    ui.perform_action(id, SemanticAction::SetValue, Some("3"))
        .unwrap();
    assert_eq!(ui.rating_value(id), Some(Some(2.0)));
    assert_eq!(ui.drain_rating_changed().len(), 256);
    ui.perform_action(id, SemanticAction::SetValue, Some("3"))
        .unwrap();
    assert_eq!(ui.rating_value(id), Some(Some(3.0)));
    let ids = ui.runtime.tree().live_ids();
    ui.set_theme(Theme::FluentDark).unwrap();
    ui.update(Size::new(120.0, 200.0)).unwrap();
    assert_eq!(ui.runtime.tree().live_ids(), ids);
    assert_eq!(ui.rating_value(id), Some(Some(3.0)));
}

#[test]
fn fractional_input_steps_do_not_accumulate_binary_rounding_in_the_visible_score() {
    let (mut ui, id) = fixture(RatingSettings {
        value: Some(0.0),
        step: 0.1,
        ..Default::default()
    });
    for _ in 0..20 {
        ui.perform_action(id, SemanticAction::Increment, None)
            .unwrap();
    }
    assert_eq!(ui.rating_value(id), Some(Some(2.0)));
    for _ in 0..10 {
        key(&mut ui, Key::ArrowRight);
    }
    key(&mut ui, Key::Enter);
    assert_eq!(ui.rating_value(id), Some(Some(3.0)));
    ui.set_rating_value(id, Some(2.123456789123)).unwrap();
    assert_eq!(ui.rating_value(id), Some(Some(2.123456789123)));
}

#[test]
fn required_score_rejects_empty_and_invalid_settings_preserve_active_preview() {
    let (mut ui, id) = fixture(RatingSettings {
        value: Some(2.5),
        allow_empty: false,
        ..Default::default()
    });
    key(&mut ui, Key::End);
    let before = ui.runtime.tree().node(id).unwrap().props.style.clone();
    assert!(
        ui.set_rating_settings(
            id,
            RatingSettings {
                value: Some(2.5),
                maximum: 2,
                allow_empty: false,
                ..Default::default()
            }
        )
        .is_err()
    );
    assert_eq!(ui.rating_preview(id), Some(RatingPreview::Value(Some(5.0))));
    assert_eq!(ui.runtime.tree().node(id).unwrap().props.style, before);
    assert!(ui.set_rating_value(id, None).is_err());
    assert_eq!(ui.rating_value(id), Some(Some(2.5)));
    key(&mut ui, Key::Delete);
    assert_eq!(ui.rating_value(id), Some(Some(2.5)));
    ui.perform_action(id, SemanticAction::SetValue, Some("null"))
        .unwrap();
    assert_eq!(ui.rating_value(id), Some(Some(2.5)));
    assert!(ui.rating_error(id).is_some());
    assert!(ui.drain_rating_changed().is_empty());
}
