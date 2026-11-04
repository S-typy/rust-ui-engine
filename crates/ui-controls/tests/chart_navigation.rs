use rust_desktop_ui_controls::*;
use rust_desktop_ui_core::*;

fn model() -> ChartModel {
    ChartModel::new(
        vec![ChartSeries {
            id: 7,
            label: "Values".into(),
            points: vec![
                ChartPoint {
                    id: 1,
                    x: 0.0,
                    y: Some(-10.0),
                },
                ChartPoint {
                    id: 2,
                    x: 5.0,
                    y: Some(0.0),
                },
                ChartPoint {
                    id: 3,
                    x: 10.0,
                    y: Some(10.0),
                },
            ],
        }],
        ChartSettings::default(),
    )
    .unwrap()
}
fn key() -> ChartPointKey {
    ChartPointKey {
        series: 7,
        point: 2,
    }
}
fn ranges(m: &ChartModel) -> (ChartRange, ChartRange) {
    (m.x_range(), m.y_range())
}
fn fixture() -> (Controls, WidgetId, Point) {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let root = ui.root();
    let id = ui
        .chart(
            root,
            model(),
            LayoutStyle {
                width: Length::Percent(1.0),
                height: Length::Percent(1.0),
                ..Default::default()
            },
        )
        .unwrap();
    ui.update(Size::new(800.0, 600.0)).unwrap();
    ui.set_chart_selected(id, Some(key())).unwrap();
    let plot = ui.chart_geometry(id).unwrap().unwrap().plot;
    (
        ui,
        id,
        Point::new(plot.x + plot.width / 2.0, plot.y + plot.height / 2.0),
    )
}
fn press(ui: &mut Controls, at: Point) {
    ui.dispatch(InputEvent::PointerDown {
        position: at,
        button: PointerButton::Primary,
        modifiers: Modifiers {
            control: true,
            ..Default::default()
        },
    })
    .unwrap();
}
fn release(ui: &mut Controls, at: Point) {
    ui.dispatch(InputEvent::PointerUp {
        position: at,
        button: PointerButton::Primary,
        modifiers: Default::default(),
    })
    .unwrap();
}
fn keyboard(ui: &mut Controls, key: Key, control: bool) {
    ui.dispatch(InputEvent::KeyDown {
        key,
        modifiers: Modifiers {
            control,
            ..Default::default()
        },
        repeat: false,
    })
    .unwrap();
}

#[test]
fn zoom_anchor_pan_and_reset_preserve_raw_observations_and_selection() {
    let mut m = model();
    let original = m.series().to_vec();
    m.select(Some(key())).unwrap();
    assert!(m.zoom_at(2.0, (0.2, 0.8)).unwrap());
    assert_eq!(
        ranges(&m),
        (
            ChartRange {
                minimum: 1.0,
                maximum: 6.0
            },
            ChartRange {
                minimum: -2.0,
                maximum: 8.0
            }
        )
    );
    m.pan(0.5, -0.25).unwrap();
    assert_eq!(
        ranges(&m),
        (
            ChartRange {
                minimum: 3.5,
                maximum: 8.5
            },
            ChartRange {
                minimum: -4.5,
                maximum: 5.5
            }
        )
    );
    assert_eq!(m.series(), original);
    assert_eq!(m.selected(), Some(key()));
    assert!(m.set_view_ranges(None, None).unwrap());
    assert_eq!(ranges(&m), ranges(&model()));
    assert!(!m.set_view_ranges(None, None).unwrap());
    assert!(!m.pan(0.0, 0.0).unwrap());
    assert!(!m.zoom_at(1.0, (0.5, 0.5)).unwrap());
    assert!(m.settings().x_range.is_none());
}

#[test]
fn rejected_view_transforms_are_atomic_and_noncartesian_data_refuses_navigation() {
    let mut m = model();
    m.select(Some(key())).unwrap();
    let initial = ranges(&m);
    for factor in [0.0, -1.0, f64::NAN, f64::INFINITY, 1e-320, 1e300] {
        assert!(m.zoom_at(factor, (0.5, 0.5)).is_err());
        assert_eq!(ranges(&m), initial);
    }
    for anchor in [(f64::NAN, 0.0), (0.0, 1.1), (-0.1, 0.5)] {
        assert!(m.zoom_at(2.0, anchor).is_err());
    }
    for pan in [(f64::INFINITY, 0.0), (0.0, f64::NAN), (f64::MAX, 0.0)] {
        assert!(m.pan(pan.0, pan.1).is_err());
    }
    assert_eq!(ranges(&m), initial);
    assert!(m.settings().x_range.is_none());
    assert_eq!(m.selected(), Some(key()));
    let mut pie = ChartModel::from_family(
        ChartFamily::Pie(vec![ChartSlice {
            id: 1,
            label: "One".into(),
            value: 1.0,
        }]),
        ChartSettings {
            view: ChartView::Pie,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(pie.zoom_at(2.0, (0.5, 0.5)), Err(ChartError::InvalidView));
    assert_eq!(pie.pan(0.1, 0.0), Err(ChartError::InvalidView));
    assert_eq!(
        pie.set_view_ranges(None, None),
        Err(ChartError::InvalidView)
    );
}

#[test]
fn external_selection_cancels_preview_and_accessible_range_keeps_raw_value() {
    let (mut ui, id, center) = fixture();
    press(&mut ui, center);
    ui.dispatch(InputEvent::PointerMoved {
        position: Point::new(center.x + 40.0, center.y),
    })
    .unwrap();
    ui.set_chart_selected(
        id,
        Some(ChartPointKey {
            series: 7,
            point: 3,
        }),
    )
    .unwrap();
    assert_eq!(ranges(ui.chart_model(id).unwrap()), ranges(&model()));
    assert_eq!(ui.runtime.captured(), None);
    let value = ui.chart_semantic_value(id).unwrap().to_owned();
    ui.zoom_chart(id, 2.0, (0.5, 0.5)).unwrap();
    assert_eq!(ui.chart_semantic_value(id), Some(value.as_str()));
    let description = ui
        .semantics()
        .into_iter()
        .find(|n| n.id == id)
        .unwrap()
        .description
        .unwrap();
    assert!(description.contains("Видимый диапазон: X 2.5 … 7.5; Y -5 … 5"));
    assert!(description.starts_with(&value));
    press(&mut ui, center);
    ui.dispatch(InputEvent::PointerMoved {
        position: Point::new(center.x + 40.0, center.y),
    })
    .unwrap();
    ui.dispatch(InputEvent::WindowFocused(false)).unwrap();
    assert_eq!(ui.runtime.captured(), None);
    assert_eq!(
        ui.chart_model(id).unwrap().x_range(),
        ChartRange {
            minimum: 2.5,
            maximum: 7.5
        }
    );
}

#[test]
fn wheel_and_keyboard_keep_pending_selection_and_reset_only_the_view() {
    let (mut ui, id, center) = fixture();
    ui.runtime.apply(Mutation::Focus(Some(id))).unwrap();
    keyboard(&mut ui, Key::Home, false);
    let selected = ui.chart_model(id).unwrap().selected();
    ui.dispatch(InputEvent::Wheel {
        position: center,
        delta: Point::new(0.0, -120.0),
        modifiers: Modifiers {
            control: true,
            ..Default::default()
        },
    })
    .unwrap();
    assert!(
        ui.chart_model(id).unwrap().x_range().maximum
            - ui.chart_model(id).unwrap().x_range().minimum
            < 10.0
    );
    assert_eq!(ui.chart_model(id).unwrap().selected(), selected);
    assert_eq!(ui.take_chart_selection_changes().len(), 1);
    let before = ranges(ui.chart_model(id).unwrap());
    keyboard(&mut ui, Key::ArrowRight, true);
    assert!(ui.chart_model(id).unwrap().x_range().minimum > before.0.minimum);
    keyboard(&mut ui, Key::Character("0".into()), true);
    assert_eq!(ranges(ui.chart_model(id).unwrap()), ranges(&model()));
    assert_eq!(ui.chart_model(id).unwrap().selected(), selected);
    assert!(ui.take_chart_selection_changes().is_empty());
}

#[test]
fn pan_preview_escape_capture_loss_and_release_have_distinct_lifecycles() {
    let (mut ui, id, center) = fixture();
    let before = ranges(ui.chart_model(id).unwrap());
    let end = Point::new(center.x + 50.0, center.y + 30.0);
    for cancel in [true, false] {
        press(&mut ui, center);
        ui.dispatch(InputEvent::PointerMoved { position: end })
            .unwrap();
        assert_ne!(ranges(ui.chart_model(id).unwrap()), before);
        assert_eq!(ui.runtime.captured(), Some(id));
        if cancel {
            keyboard(&mut ui, Key::Escape, false);
        } else {
            ui.cancel_pointer_interaction().unwrap();
        }
        assert_eq!(ranges(ui.chart_model(id).unwrap()), before);
        assert!(ui.chart_model(id).unwrap().settings().x_range.is_none());
        assert_eq!(ui.chart_model(id).unwrap().selected(), Some(key()));
        assert_eq!(ui.runtime.captured(), None);
        release(&mut ui, end);
    }
    press(&mut ui, center);
    ui.dispatch(InputEvent::PointerMoved { position: end })
        .unwrap();
    let accepted = ranges(ui.chart_model(id).unwrap());
    release(&mut ui, end);
    assert_eq!(ranges(ui.chart_model(id).unwrap()), accepted);
    assert_eq!(ui.runtime.captured(), None);
    assert_eq!(ui.chart_model(id).unwrap().selected(), Some(key()));
    assert!(ui.take_chart_selection_changes().is_empty());
}

#[test]
fn resize_disable_and_external_updates_cancel_pan_without_stale_rollback() {
    let (mut ui, id, center) = fixture();
    let end = Point::new(center.x + 40.0, center.y);
    press(&mut ui, center);
    ui.dispatch(InputEvent::PointerMoved { position: end })
        .unwrap();
    ui.update(Size::new(850.0, 600.0)).unwrap();
    assert_eq!(ranges(ui.chart_model(id).unwrap()), ranges(&model()));
    assert_eq!(ui.runtime.captured(), None);
    press(&mut ui, center);
    ui.dispatch(InputEvent::PointerMoved { position: end })
        .unwrap();
    ui.set_enabled(id, false).unwrap();
    ui.update(Size::new(850.0, 600.0)).unwrap();
    assert_eq!(ranges(ui.chart_model(id).unwrap()), ranges(&model()));
    assert_eq!(ui.runtime.captured(), None);
    ui.set_enabled(id, true).unwrap();
    press(&mut ui, center);
    ui.dispatch(InputEvent::PointerMoved { position: end })
        .unwrap();
    let preview = ranges(ui.chart_model(id).unwrap());
    assert!(ui.zoom_chart(id, 0.0, (0.5, 0.5)).is_err());
    assert_eq!(ranges(ui.chart_model(id).unwrap()), preview);
    assert_eq!(ui.runtime.captured(), Some(id));
    let mut replacement = model();
    replacement.pan(2.0, 1.0).unwrap();
    let expected = ranges(&replacement);
    ui.set_chart_model(id, replacement).unwrap();
    release(&mut ui, end);
    assert_eq!(ranges(ui.chart_model(id).unwrap()), expected);
    assert_eq!(ui.runtime.captured(), None);
}

#[test]
fn untouched_wheel_outside_plot_and_zero_distance_drag_do_not_pin_automatic_ranges() {
    let (mut ui, id, center) = fixture();
    for (position, control) in [(center, false), (Point::new(10.0, 10.0), true)] {
        ui.dispatch(InputEvent::Wheel {
            position,
            delta: Point::new(0.0, 120.0),
            modifiers: Modifiers {
                control,
                ..Default::default()
            },
        })
        .unwrap();
        assert!(ui.chart_model(id).unwrap().settings().x_range.is_none());
    }
    press(&mut ui, center);
    ui.dispatch(InputEvent::PointerMoved { position: center })
        .unwrap();
    release(&mut ui, center);
    assert!(ui.chart_model(id).unwrap().settings().x_range.is_none());
    assert_eq!(ui.chart_model(id).unwrap().selected(), Some(key()));
}
