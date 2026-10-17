use super::*;
use crate::{
    BindingContext, BindingProperty, ControlBinding, ControlBindingMode, ControlBindings, DataValue,
};
use std::{cell::RefCell, rc::Rc};
fn settings(view: GaugeView) -> GaugeSettings {
    GaugeSettings {
        view,
        value: 35.0,
        unit: "°C".into(),
        ranges: vec![
            GaugeRange {
                start: 0.0,
                end: 60.0,
                label: "Норма".into(),
                tone: GaugeTone::Good,
            },
            GaugeRange {
                start: 60.0,
                end: 80.0,
                label: "Внимание".into(),
                tone: GaugeTone::Warning,
            },
            GaugeRange {
                start: 80.0,
                end: 100.0,
                label: "Опасно".into(),
                tone: GaugeTone::Danger,
            },
        ],
        ..Default::default()
    }
}
fn fixture(view: GaugeView) -> (Controls, WidgetId) {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let id = ui
        .gauge(
            ui.root(),
            settings(view),
            LayoutStyle {
                width: Length::Px(320.0),
                height: Length::Px(280.0),
                flex_shrink: 0.0,
                ..Default::default()
            },
        )
        .unwrap();
    ui.update(Size::new(800.0, 500.0)).unwrap();
    (ui, id)
}
#[test]
fn gauge_range_boundaries_and_gaps_are_deterministic() {
    let mut s = settings(GaugeView::State);
    for (value, label) in [
        (0.0, "Норма"),
        (59.99, "Норма"),
        (60.0, "Внимание"),
        (80.0, "Опасно"),
        (100.0, "Опасно"),
    ] {
        s.value = value;
        s.validate().unwrap();
        assert_eq!(s.state_label(), label);
    }
    s.ranges.remove(1);
    s.value = 70.0;
    assert_eq!(s.state_label(), "Без диапазона");
    let old = s.clone();
    s.ranges[1].start = 59.0;
    assert!(s.validate().is_err());
    s = old;
    s.ranges.reverse();
    assert!(s.validate().is_err());
}
#[test]
fn gauge_ticks_exact_endpoints_bounded_and_finite_for_extreme_scales() {
    for (min, max) in [
        (-1e300, 1e300),
        (1.0, 1.0000000000000002),
        (-1e-300, 1e-300),
        (0.0, 100.0),
    ] {
        let s = GaugeSettings {
            minimum: min,
            maximum: max,
            value: min,
            major_ticks: 21,
            minor_ticks: 4,
            ..Default::default()
        };
        s.validate().unwrap();
        let ticks = s.ticks();
        assert!(ticks.len() <= 101);
        assert_eq!(ticks.first(), Some(&(min, true)));
        assert_eq!(ticks.last(), Some(&(max, true)));
        assert!(ticks.windows(2).all(|w| w[0].0 < w[1].0));
        assert!(
            ticks
                .iter()
                .all(|t| t.0.is_finite() && (min..=max).contains(&t.0))
        );
    }
    let s = GaugeSettings {
        minimum: -f64::MAX,
        maximum: f64::MAX,
        ..Default::default()
    };
    assert!(s.validate().is_err());
    let s = GaugeSettings {
        minimum: 0.0,
        maximum: 1.0,
        value: 1e-30,
        ..Default::default()
    };
    assert!(s.value_text().contains('e'));
}
#[test]
fn gauge_atomic_settings_value_and_shaped_text_rejection() {
    let (mut ui, id) = fixture(GaugeView::Circular);
    let before = ui.gauge_settings(id).unwrap().clone();
    let nodes = ui.runtime.tree().len();
    for value in [f64::NAN, f64::INFINITY, -1.0, 101.0] {
        assert!(ui.set_gauge_value(id, value).is_err());
        assert_eq!(ui.gauge_settings(id), Some(&before));
    }
    let mut bad = before.clone();
    bad.ranges[1].start = 59.0;
    assert!(ui.set_gauge(id, bad).is_err());
    let mut bad = before.clone();
    bad.unit = format!("A{}", "\u{301}".repeat(40));
    assert!(ui.set_gauge(id, bad).is_err());
    assert_eq!(ui.gauge_settings(id), Some(&before));
    assert_eq!(ui.runtime.tree().len(), nodes);
    ui.set_gauge_value(id, 80.0).unwrap();
    assert_eq!(ui.gauge_settings(id).unwrap().state_label(), "Опасно");
}
#[test]
fn gauge_all_views_resize_preserve_value_and_do_not_clip_scale_geometry() {
    for theme in [Theme::FluentLight, Theme::FluentDark] {
        for view in [
            GaugeView::Circular,
            GaugeView::Linear,
            GaugeView::Digital,
            GaugeView::State,
        ] {
            let (mut ui, id) = fixture(view);
            ui.set_theme(theme).unwrap();
            for (width, height) in [(320.0, 280.0), (220.0, 220.0), (180.0, 100.0), (80.0, 30.0)] {
                ui.edit_props(id, |p| {
                    p.style.width = Length::Px(width);
                    p.style.height = Length::Px(height);
                })
                .unwrap();
                ui.update(Size::new(800.0, 500.0)).unwrap();
                let bounds = ui.runtime.bounds(id).unwrap();
                assert_eq!(ui.gauge_value(id), Some(35.0));
                assert!(ui.scene().texts.iter().all(|r| r.is_valid()));
                if ui.gauge_display_status(id) == Some(GaugeDisplayStatus::Compact) {
                    assert!(ui.scene().stroke_paths.is_empty());
                } else {
                    for path in &ui.scene().stroke_paths {
                        let b = path.bounds().unwrap();
                        assert!(
                            b.x >= bounds.x - 0.01
                                && b.y >= bounds.y - 0.01
                                && b.x + b.width <= bounds.x + bounds.width + 0.01
                                && b.y + b.height <= bounds.y + bounds.height + 0.01,
                            "{view:?} {b:?} {bounds:?}"
                        );
                    }
                }
            }
        }
    }
}
#[test]
fn gauge_semantics_are_read_only_measurement_and_survive_disabled() {
    let (mut ui, id) = fixture(GaugeView::State);
    ui.set_enabled(id, false).unwrap();
    ui.update(Size::new(800.0, 500.0)).unwrap();
    let node = ui.semantics().into_iter().find(|n| n.id == id).unwrap();
    assert_eq!(node.role, Role::Meter);
    assert!(node.read_only && node.disabled);
    assert!(node.actions.is_empty());
    assert_eq!(node.value.as_deref(), Some("35.0 °C · Норма"));
    ui.remove(id).unwrap();
    assert!(ui.gauge_settings(id).is_none());
}
#[test]
fn gauge_one_way_binding_updates_and_invalid_value_preserves_target() {
    let (mut ui, id) = fixture(GaugeView::Linear);
    let model = Rc::new(RefCell::new(
        BindingContext::new(DataValue::object([("Value", DataValue::Number(75.0))])).unwrap(),
    ));
    model
        .borrow_mut()
        .set("Value", DataValue::Number(75.0))
        .unwrap();
    let mut bindings = ControlBindings::new(model.clone());
    bindings
        .add(ControlBinding::new(id, BindingProperty::Value, "Value").unwrap())
        .unwrap();
    bindings.initialize(&mut ui).unwrap();
    assert_eq!(ui.gauge_value(id), Some(75.0));
    model
        .borrow_mut()
        .set("Value", DataValue::Number(150.0))
        .unwrap();
    bindings.sync(&mut ui);
    assert_eq!(ui.gauge_value(id), Some(75.0));
    assert!(!bindings.diagnostics().is_empty());
    model
        .borrow_mut()
        .set("Value", DataValue::Number(85.0))
        .unwrap();
    bindings.sync(&mut ui);
    assert_eq!(ui.gauge_value(id), Some(85.0));
    let mut two = ControlBindings::new(model);
    let mut binding = ControlBinding::new(id, BindingProperty::Value, "Value").unwrap();
    binding.mode = ControlBindingMode::TwoWay;
    two.add(binding).unwrap();
    assert!(two.initialize(&mut ui).is_err());
}

#[test]
fn gauge_public_ticks_reject_invalid_recipe_without_panicking() {
    for major in [0, 1, 22, 255] {
        let s = GaugeSettings {
            major_ticks: major,
            ..Default::default()
        };
        assert!(s.ticks().is_empty());
    }
    let s = GaugeSettings {
        minimum: f64::NAN,
        ..Default::default()
    };
    assert!(s.ticks().is_empty());
    let s = GaugeSettings {
        minimum: 100.0,
        maximum: 100.0,
        ..Default::default()
    };
    assert!(s.ticks().is_empty());
}

#[test]
fn gauge_fractional_placement_and_ancestor_clip_choose_full_or_compact() {
    let s = GaugeSettings::default();
    let b = Rect::new(700.3, 11.1, 220.1, 230.2);
    assert_eq!(
        gauge_paint::status(&s, b, Rect::new(0.0, 0.0, 2000.0, 1000.0)),
        GaugeDisplayStatus::Full
    );
    assert_eq!(
        gauge_paint::status(&s, b, Rect::new(710.0, 0.0, 2000.0, 1000.0)),
        GaugeDisplayStatus::Compact
    );
}
