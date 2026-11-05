use rust_desktop_ui_controls::{
    ChartError, ChartModel, ChartPoint, ChartPointKey, ChartRange, ChartSeries, ChartSettings,
    ChartView, Controls, Theme,
};
use rust_desktop_ui_core::{LayoutStyle, Length, Point, Rect, Size};

fn series(id: u64, values: &[Option<f64>]) -> ChartSeries {
    ChartSeries {
        id,
        label: format!("Series {id}"),
        points: values
            .iter()
            .enumerate()
            .map(|(i, &y)| ChartPoint {
                id: i as i64,
                x: i as f64,
                y,
            })
            .collect(),
    }
}
fn settings() -> ChartSettings {
    ChartSettings {
        view: ChartView::StackedArea,
        ..Default::default()
    }
}
fn key(series: u64, point: i64) -> ChartPointKey {
    ChartPointKey { series, point }
}
fn plot() -> Rect {
    Rect::new(20.0, 30.0, 400.0, 240.0)
}
fn position(m: &ChartModel, x: f64, y: f64) -> Point {
    let xr = m.x_range();
    let yr = m.y_range();
    let p = plot();
    Point::new(
        p.x + ((x - xr.minimum) / (xr.maximum - xr.minimum)) as f32 * p.width,
        p.y + (1.0 - (y - yr.minimum) / (yr.maximum - yr.minimum)) as f32 * p.height,
    )
}

#[test]
fn bands_stack_sloped_baselines_and_return_original_endpoint_keys() {
    let data = vec![
        series(1, &[Some(3.0), Some(5.0)]),
        series(2, &[Some(2.0), Some(4.0)]),
    ];
    let mut m = ChartModel::new(data.clone(), settings()).unwrap();
    assert_eq!(m.series(), data);
    assert_eq!(
        m.y_range(),
        ChartRange {
            minimum: 0.0,
            maximum: 9.0
        }
    );
    let g = m.geometry(plot()).unwrap();
    assert_eq!((g.areas.len(), g.points.len()), (2, 4));
    for (x, y, k) in [
        (0.25, 2.0, key(1, 0)),
        (0.25, 4.5, key(2, 0)),
        (0.5, 5.5, key(2, 0)),
        (0.75, 6.0, key(2, 1)),
    ] {
        assert_eq!(g.hit_test(position(&m, x, y), 0.0), Some(k));
    }
    assert_eq!(
        m.describe(key(2, 1)).unwrap(),
        "Series 2: X 1, Y 4; интервал 5 … 9"
    );
    m.select(Some(key(2, 1))).unwrap();
    m.zoom_at(2.0, (0.5, 0.5)).unwrap();
    assert_eq!(m.selected(), Some(key(2, 1)));
    assert_eq!(m.series(), data);
}

#[test]
fn lower_sign_crossing_creates_a_kink_in_upper_band_instead_of_false_linear_stack() {
    let m = ChartModel::new(
        vec![
            series(1, &[Some(2.0), Some(-2.0)]),
            series(2, &[Some(1.0), Some(1.0)]),
        ],
        settings(),
    )
    .unwrap();
    let g = m.geometry(plot()).unwrap();
    for (x, y, expected) in [
        (0.25, 1.75, Some(key(2, 0))),
        (0.25, 2.25, None),
        (0.75, 0.75, Some(key(2, 1))),
        (0.75, 1.25, None),
    ] {
        assert_eq!(g.hit_test(position(&m, x, y), 0.0), expected);
    }
    assert_eq!(g.points.len(), 4); // No synthetic crossing observations.
}

#[test]
fn changing_sign_uses_separate_positive_and_negative_envelopes() {
    let m = ChartModel::new(
        vec![
            series(1, &[Some(3.0), Some(3.0)]),
            series(2, &[Some(-2.0), Some(2.0)]),
        ],
        settings(),
    )
    .unwrap();
    let g = m.geometry(plot()).unwrap();
    assert_eq!(g.hit_test(position(&m, 0.25, -0.5), 0.0), Some(key(2, 0)));
    assert_eq!(g.hit_test(position(&m, 0.75, 3.5), 0.0), Some(key(2, 1)));
    // The missing region between signed envelopes must not become a bow-tie.
    assert_eq!(g.hit_test(position(&m, 0.25, 3.5), 0.0), None);
    assert_eq!(g.hit_test(position(&m, 0.75, -0.5), 0.0), None);
    assert_eq!(
        m.describe(key(2, 0)).unwrap(),
        "Series 2: X 0, Y -2; интервал 0 … -2"
    );
}

#[test]
fn missing_values_break_only_their_own_and_dependent_bands() {
    let m = ChartModel::new(
        vec![
            series(1, &[Some(2.0); 3]),
            series(2, &[Some(1.0), None, Some(1.0)]),
            series(3, &[Some(4.0); 3]),
        ],
        settings(),
    )
    .unwrap();
    let g = m.geometry(plot()).unwrap();
    assert_eq!(g.areas.len(), 2);
    assert!(g.areas.iter().all(|a| a.start.series == 1));
    assert_eq!(g.points.len(), 7);
    assert!(!g.points.iter().any(|p| p.key == key(3, 1)));
    assert_eq!(m.point(key(3, 1)).unwrap().y, Some(4.0));
    assert!(
        m.describe(key(3, 1))
            .unwrap()
            .ends_with("накопление недоступно")
    );
    assert_eq!(g.hit_test(position(&m, 1.0, 4.0), 0.0), None);
}

#[test]
fn offscreen_bands_clip_to_the_plot_and_keep_selection_on_source_points() {
    let m = ChartModel::new(
        vec![series(1, &[Some(2.0); 2]), series(2, &[Some(3.0); 2])],
        ChartSettings {
            x_range: Some(ChartRange {
                minimum: 0.2,
                maximum: 0.8,
            }),
            y_range: Some(ChartRange {
                minimum: 3.0,
                maximum: 4.0,
            }),
            ..settings()
        },
    )
    .unwrap();
    let g = m.geometry(plot()).unwrap();
    assert!(g.points.is_empty());
    assert_eq!(g.areas.len(), 1);
    assert_eq!(g.hit_test(position(&m, 0.25, 3.5), 0.0), Some(key(2, 0)));
    assert_eq!(g.hit_test(position(&m, 0.75, 3.5), 0.0), Some(key(2, 1)));
    for p in &g.areas[0].contour {
        let b = plot();
        assert!(p.x >= b.x && p.x <= b.x + b.width && p.y >= b.y && p.y <= b.y + b.height);
    }
}

#[test]
fn independently_sampled_signed_sums_match_filled_bands_between_observations() {
    let data = vec![
        series(1, &[Some(-4.0), Some(3.0), Some(2.0), Some(-1.0)]),
        series(2, &[Some(2.0), Some(-3.0), Some(1.0), Some(4.0)]),
        series(3, &[Some(-1.0), Some(4.0), None, Some(3.0)]),
        series(4, &[Some(5.0), Some(-2.0), Some(-1.0), Some(-4.0)]),
    ];
    let m = ChartModel::new(data.clone(), settings()).unwrap();
    let g = m.geometry(plot()).unwrap();
    for column in 0..3 {
        for step in 1..20 {
            let t = f64::from(step) / 20.0;
            let mut positive = 0.0;
            let mut negative = 0.0;
            for s in &data {
                let (Some(a), Some(b)) = (s.points[column].y, s.points[column + 1].y) else {
                    break;
                };
                let value = a + (b - a) * t;
                let base = if value >= 0.0 {
                    &mut positive
                } else {
                    &mut negative
                };
                let center = *base + value * 0.5;
                *base += value;
                if value.abs() > 0.01 {
                    let pointer = position(&m, column as f64 + t, center);
                    // Pointer coordinates are f32. An unrepresentable midpoint
                    // can round to either side of the exact data-space tie.
                    let range = m.x_range();
                    let pointer_x = range.minimum
                        + (f64::from(pointer.x) - f64::from(plot().x)) / f64::from(plot().width)
                            * (range.maximum - range.minimum);
                    assert_eq!(
                        g.hit_test(pointer, 0.0),
                        Some(key(
                            s.id,
                            (column + usize::from(pointer_x > column as f64 + 0.5)) as i64
                        )),
                        "series {}, interval {column}, t {t}",
                        s.id
                    );
                }
            }
        }
    }
}

#[test]
fn topology_precision_and_geometry_budget_errors_do_not_replace_the_model() {
    let original = vec![series(1, &[Some(1.0), Some(2.0)])];
    let mut m = ChartModel::new(original.clone(), settings()).unwrap();
    m.select(Some(key(1, 0))).unwrap();
    let mut shifted = series(2, &[Some(1.0), Some(2.0)]);
    shifted.points[1].x = 2.0;
    let dense = (0..16)
        .map(|s| {
            series(
                s,
                &(0..256)
                    .map(|i| {
                        Some(if i % 2 == 0 {
                            1.0 + s as f64
                        } else {
                            -(17.0 - s as f64)
                        })
                    })
                    .collect::<Vec<_>>(),
            )
        })
        .collect();
    for (data, error) in [
        (
            vec![series(1, &[Some(1.0), Some(2.0)]), shifted],
            ChartError::InvalidTopology,
        ),
        (
            vec![series(1, &[Some(1.0), Some(2.0)]), series(2, &[])],
            ChartError::InvalidTopology,
        ),
        (
            vec![series(1, &[Some(1e100); 2]), series(2, &[Some(1.0); 2])],
            ChartError::CoordinateOverflow,
        ),
        (dense, ChartError::Capacity),
    ] {
        assert_eq!(m.replace(data, settings()), Err(error));
        assert_eq!(m.series(), original);
        assert_eq!(m.selected(), Some(key(1, 0)));
    }
}

#[test]
fn dense_alternating_area_remains_within_existing_scene_budgets() {
    let values = (0..4096)
        .map(|i| Some(if i % 2 == 0 { 1.0 } else { -1.0 }))
        .collect::<Vec<_>>();
    let model = ChartModel::new(vec![series(1, &values)], settings()).unwrap();
    let mut ui = Controls::new(Theme::FluentDark).unwrap();
    let id = ui
        .chart(
            ui.root(),
            model,
            LayoutStyle {
                width: Length::Px(1100.0),
                height: Length::Px(600.0),
                ..Default::default()
            },
        )
        .unwrap();
    ui.update(Size::new(1100.0, 600.0)).unwrap();
    assert_eq!(ui.chart_geometry(id).unwrap().unwrap().points.len(), 4096);
    assert!(ui.scene().fill_paths.len() < 512);
    assert!(ui.scene().fill_paths.iter().all(|p| p.is_valid()));
    assert_eq!(
        ui.scene()
            .fill_paths
            .iter()
            .map(|p| p.contours.len())
            .sum::<usize>(),
        8190
    );
}
