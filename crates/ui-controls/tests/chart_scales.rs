use rust_desktop_ui_controls::*;
use rust_desktop_ui_core::*;

fn series(values: &[f64]) -> Vec<ChartSeries> {
    vec![ChartSeries {
        id: 1,
        label: "Powers".into(),
        points: values
            .iter()
            .enumerate()
            .map(|(i, v)| ChartPoint {
                id: i as i64,
                x: *v,
                y: Some(*v),
            })
            .collect(),
    }]
}
fn settings() -> ChartSettings {
    ChartSettings {
        x_scale: ChartScale::Log10,
        y_scale: ChartScale::Log10,
        ..Default::default()
    }
}
fn model() -> ChartModel {
    ChartModel::new(series(&[1.0, 10.0, 100.0, 1000.0]), settings()).unwrap()
}
fn close(a: f64, b: f64) {
    assert!(
        (a - b).abs() <= 1e-10 * a.abs().max(b.abs()).max(1e-300),
        "{a} != {b}"
    );
}

#[test]
fn equal_decades_share_drawing_hits_and_raw_major_ticks() {
    let model = model();
    let geometry = model.geometry(Rect::new(0.0, 0.0, 300.0, 300.0)).unwrap();
    assert_eq!(geometry.x_ticks, vec![1.0, 10.0, 100.0, 1000.0]);
    assert_eq!(geometry.y_ticks, geometry.x_ticks);
    assert_eq!(geometry.points.len(), 4);
    for (index, point) in geometry.points.iter().enumerate() {
        assert!((point.position.x - index as f32 * 100.0).abs() < 0.001);
        assert!((point.position.y - (300.0 - index as f32 * 100.0)).abs() < 0.001);
        assert_eq!(geometry.hit_test(point.position, 4.0), Some(point.key));
    }
    assert_eq!(model.series(), series(&[1.0, 10.0, 100.0, 1000.0]));
}

#[test]
fn log_navigation_uses_domain_spans_and_keeps_data_and_selection() {
    let mut model = model();
    let original = model.series().to_vec();
    let key = ChartPointKey {
        series: 1,
        point: 1,
    };
    model.select(Some(key)).unwrap();
    model.zoom_at(2.0, (0.0, 1.0)).unwrap();
    close(model.x_range().minimum, 1.0);
    close(model.x_range().maximum, 1000.0f64.sqrt());
    close(model.y_range().minimum, 1000.0f64.sqrt());
    close(model.y_range().maximum, 1000.0);
    model.pan(1.0, -1.0).unwrap();
    close(model.x_range().minimum, 1000.0f64.sqrt());
    close(model.x_range().maximum, 1000.0);
    close(model.y_range().minimum, 1.0);
    close(model.y_range().maximum, 1000.0f64.sqrt());
    assert_eq!(model.series(), original);
    assert_eq!(model.selected(), Some(key));
    model.set_view_ranges(None, None).unwrap();
    assert_eq!(
        model.x_range(),
        ChartRange {
            minimum: 1.0,
            maximum: 1000.0
        }
    );
    assert_eq!(model.selected(), Some(key));
}

#[test]
fn positive_extrema_and_singletons_do_not_drop_original_points() {
    for values in [
        vec![],
        vec![1.0],
        vec![f64::from_bits(1)],
        vec![f64::MAX],
        vec![1e-300, 1e300],
        vec![f64::from_bits(1), f64::from_bits(2)],
        vec![3.125, 999.999],
    ] {
        let model = ChartModel::new(series(&values), settings()).unwrap();
        let geometry = model.geometry(Rect::new(0.0, 0.0, 800.0, 400.0)).unwrap();
        assert_eq!(geometry.points.len(), values.len(), "{values:?}");
        for ticks in [&geometry.x_ticks, &geometry.y_ticks] {
            assert!(ticks.len() <= 5);
            assert!(ticks.iter().all(|v| v.is_finite() && *v > 0.0));
            assert!(ticks.windows(2).all(|p| p[0] < p[1]));
        }
    }
}

#[test]
fn dense_major_decades_keep_endpoints_without_overcrowding_axis_labels() {
    for first in [-300, -8, 0, 20, 292] {
        for count in 6..=9 {
            let low = 10.0f64.powi(first);
            let high = 10.0f64.powi(first + count - 1);
            let model = ChartModel::new(series(&[low, high]), settings()).unwrap();
            let ticks = model.x_ticks().unwrap();
            assert!(ticks.len() <= 5);
            close(*ticks.first().unwrap(), low);
            close(*ticks.last().unwrap(), high);
            assert!(ticks.windows(2).all(|p| p[0] < p[1]));
        }
    }
}

#[test]
fn invalid_log_candidates_leave_old_ranges_data_and_selection_intact() {
    let mut model = model();
    let before = model.series().to_vec();
    let ranges = (model.x_range(), model.y_range());
    for values in [
        vec![0.0, 1.0],
        vec![-1.0, 10.0],
        vec![f64::NAN],
        vec![f64::INFINITY],
    ] {
        assert!(model.replace(series(&values), settings()).is_err());
        assert_eq!(model.series(), before);
        assert_eq!((model.x_range(), model.y_range()), ranges);
    }
    assert!(
        model
            .set_view_ranges(
                Some(ChartRange {
                    minimum: 0.0,
                    maximum: 100.0
                }),
                None
            )
            .is_err()
    );
    assert!(model.pan(1e300, 0.0).is_err());
    assert_eq!((model.x_range(), model.y_range()), ranges);
    for view in [
        ChartView::Bar,
        ChartView::Area,
        ChartView::StackedBar,
        ChartView::StackedArea,
    ] {
        assert!(ChartModel::new(before.clone(), ChartSettings { view, ..settings() }).is_err());
    }
}

#[test]
fn null_breaks_lines_and_explicit_viewport_clips_in_log_coordinates() {
    let mut input = series(&[1.0, 10.0, 100.0, 1000.0]);
    input[0].points[1].y = None;
    let mut model = ChartModel::new(input, settings()).unwrap();
    let rect = Rect::new(0.0, 0.0, 300.0, 300.0);
    assert_eq!(model.geometry(rect).unwrap().segments.len(), 1);
    model
        .set_view_ranges(
            Some(ChartRange {
                minimum: 10.0,
                maximum: 1000.0,
            }),
            Some(ChartRange {
                minimum: 10.0,
                maximum: 1000.0,
            }),
        )
        .unwrap();
    let geometry = model.geometry(rect).unwrap();
    assert_eq!(geometry.points.len(), 2);
    assert!((geometry.points[0].position.x - 150.0).abs() < 0.001);
    assert!((geometry.points[0].position.y - 150.0).abs() < 0.001);
}

#[test]
fn axes_are_independent_and_unsupported_or_collapsed_domains_are_explicit() {
    let mut data = series(&[1.0, 10.0, 100.0]);
    data[0].points[0].y = Some(-1.0);
    let mut model = ChartModel::new(
        data.clone(),
        ChartSettings {
            y_scale: ChartScale::Linear,
            ..settings()
        },
    )
    .unwrap();
    assert_eq!(model.y_range().minimum, -1.0);
    model.pan(0.0, 0.1).unwrap();
    assert_eq!(
        model.x_range(),
        ChartRange {
            minimum: 1.0,
            maximum: 100.0
        }
    );
    assert!(ChartModel::new(data, settings()).is_err());
    assert!(ChartModel::new(series(&[1e300, 1e300f64.next_up()]), settings()).is_err());
    assert!(
        ChartModel::from_family(
            ChartFamily::Pie(vec![ChartSlice {
                id: 1,
                label: "One".into(),
                value: 1.0,
            }]),
            ChartSettings {
                view: ChartView::Pie,
                ..settings()
            }
        )
        .is_err()
    );
}

#[test]
fn captured_log_pan_cancels_to_original_automatic_view_and_raw_accessibility() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let id = ui
        .chart(
            ui.root(),
            model(),
            LayoutStyle {
                width: Length::Percent(1.0),
                height: Length::Percent(1.0),
                ..Default::default()
            },
        )
        .unwrap();
    ui.update(Size::new(800.0, 600.0)).unwrap();
    let key = ChartPointKey {
        series: 1,
        point: 1,
    };
    ui.set_chart_selected(id, Some(key)).unwrap();
    let value = ui.chart_semantic_value(id).unwrap().to_owned();
    assert!(value.contains("X 10, Y 10"));
    assert!(
        ui.chart_view_description(id)
            .unwrap()
            .contains("основание 10")
    );
    let plot = ui.chart_geometry(id).unwrap().unwrap().plot;
    let start = Point::new(plot.x + plot.width / 2.0, plot.y + plot.height / 2.0);
    ui.dispatch(InputEvent::PointerDown {
        position: start,
        button: PointerButton::Primary,
        modifiers: Modifiers {
            control: true,
            ..Default::default()
        },
    })
    .unwrap();
    ui.dispatch(InputEvent::PointerMoved {
        position: Point::new(start.x + plot.width / 3.0, start.y),
    })
    .unwrap();
    close(ui.chart_model(id).unwrap().x_range().minimum, 0.1);
    assert_eq!(ui.runtime.captured(), Some(id));
    ui.dispatch(InputEvent::KeyDown {
        key: Key::Escape,
        modifiers: Default::default(),
        repeat: false,
    })
    .unwrap();
    assert_eq!(ui.runtime.captured(), None);
    assert!(ui.chart_model(id).unwrap().settings().x_range.is_none());
    assert_eq!(ui.chart_model(id).unwrap().selected(), Some(key));
    assert_eq!(ui.chart_semantic_value(id), Some(value.as_str()));
}
