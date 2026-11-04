use rust_desktop_ui_controls::{
    ChartError, ChartModel, ChartPoint, ChartPointKey, ChartRange, ChartSeries, ChartSettings,
    ChartView,
};
use rust_desktop_ui_core::{Color, FillPath, FillRule, Point, Rect};

fn series(id: u64, points: &[(f64, Option<f64>)]) -> ChartSeries {
    ChartSeries {
        id,
        label: format!("Series {id}"),
        points: points
            .iter()
            .enumerate()
            .map(|(i, &(x, y))| ChartPoint { id: i as i64, x, y })
            .collect(),
    }
}
fn settings(view: ChartView) -> ChartSettings {
    ChartSettings {
        view,
        ..Default::default()
    }
}
fn plot() -> Rect {
    Rect::new(20.0, 30.0, 400.0, 200.0)
}
fn key(series: u64, point: i64) -> ChartPointKey {
    ChartPointKey { series, point }
}

#[test]
fn signed_stacks_share_categories_and_keep_raw_observations() {
    let model = ChartModel::new(
        vec![
            series(7, &[(0.0, Some(3.0)), (1.0, Some(-2.0)), (2.0, None)]),
            series(9, &[(2.0, Some(4.0)), (0.0, Some(5.0)), (1.0, Some(-6.0))]),
            series(11, &[(1.0, Some(3.0)), (0.0, Some(-1.0))]),
        ],
        settings(ChartView::StackedBar),
    )
    .unwrap();
    assert_eq!(
        model.y_range(),
        ChartRange {
            minimum: -8.0,
            maximum: 8.0
        }
    );
    assert_eq!(model.point(key(9, 1)).unwrap().y, Some(5.0));
    assert_eq!(
        model.describe(key(9, 1)).unwrap(),
        "Series 9: X 0, Y 5; интервал 3 … 8"
    );
    let geometry = model.geometry(plot()).unwrap();
    assert_eq!(geometry.bars.len(), 7);
    let first = geometry.bars.iter().find(|b| b.key == key(7, 0)).unwrap();
    let second = geometry.bars.iter().find(|b| b.key == key(9, 1)).unwrap();
    assert_eq!(first.bounds.x, second.bounds.x);
    assert_eq!(first.bounds.width, second.bounds.width);
    assert!((first.bounds.y - second.bounds.y - second.bounds.height).abs() < 0.001);
    for bar in &geometry.bars {
        let center = Point::new(
            bar.bounds.x + bar.bounds.width / 2.0,
            bar.bounds.y + bar.bounds.height / 2.0,
        );
        assert_eq!(geometry.hit_test(center, 0.0), Some(bar.key));
    }
}

#[test]
fn rejected_stacks_leave_selection_and_data_unchanged() {
    let mut model = ChartModel::new(
        vec![series(1, &[(0.0, Some(2.0))])],
        settings(ChartView::StackedBar),
    )
    .unwrap();
    model.select(Some(key(1, 0))).unwrap();
    for (data, expected) in [
        (
            vec![series(1, &[(0.0, Some(2.0)), (-0.0, None)])],
            ChartError::DuplicateCategory,
        ),
        (
            vec![
                series(1, &[(0.0, Some(f64::MAX))]),
                series(2, &[(0.0, Some(f64::MAX))]),
            ],
            ChartError::NonFinite,
        ),
        (
            vec![
                series(1, &[(0.0, Some(1e100))]),
                series(2, &[(0.0, Some(1.0))]),
            ],
            ChartError::CoordinateOverflow,
        ),
    ] {
        assert_eq!(
            model.replace(data, settings(ChartView::StackedBar)),
            Err(expected)
        );
        assert_eq!(model.selected(), Some(key(1, 0)));
        assert_eq!(model.point(key(1, 0)).unwrap().y, Some(2.0));
    }
}

#[test]
fn explicit_ranges_clip_the_stack_span_instead_of_drawing_to_zero() {
    let model = ChartModel::new(
        vec![
            series(1, &[(0.0, Some(3.0))]),
            series(2, &[(0.0, Some(5.0))]),
        ],
        ChartSettings {
            y_range: Some(ChartRange {
                minimum: 4.0,
                maximum: 6.0,
            }),
            ..settings(ChartView::StackedBar)
        },
    )
    .unwrap();
    let g = model.geometry(plot()).unwrap();
    assert_eq!(g.bars.len(), 1);
    assert_eq!(g.bars[0].key, key(2, 0));
    assert_eq!(g.bars[0].bounds.height, plot().height);
    assert_eq!(g.bars[0].bounds.y, plot().y);
}

#[test]
fn areas_split_sign_changes_and_preserve_null_gaps() {
    let model = ChartModel::new(
        vec![series(
            1,
            &[
                (0.0, Some(-2.0)),
                (1.0, Some(2.0)),
                (2.0, None),
                (3.0, Some(3.0)),
                (4.0, Some(1.0)),
            ],
        )],
        settings(ChartView::Area),
    )
    .unwrap();
    let g = model.geometry(plot()).unwrap();
    assert_eq!(g.areas.len(), 3);
    assert_eq!(g.segments.len(), 2);
    assert_eq!(g.points.len(), 4);
    for a in &g.areas {
        assert!(a.start.point != 1 && a.start.point != 2);
        let path = FillPath {
            contours: vec![a.contour.clone()],
            clip: plot(),
            rule: FillRule::NonZero,
            color: Color::rgb(0, 0, 0),
        };
        assert!(path.is_valid());
        let n = a.contour.len() as f32;
        let center = Point::new(
            a.contour.iter().map(|p| p.x).sum::<f32>() / n,
            a.contour.iter().map(|p| p.y).sum::<f32>() / n,
        );
        assert!(path.contains(center));
        assert_eq!(
            g.hit_test(center, 0.0),
            Some(if f64::from(center.x) <= a.split_x {
                a.start
            } else {
                a.end
            })
        );
    }
    assert_eq!(g.hit_test(Point::new(220.0, 200.0), 0.0), None);
}

#[test]
fn area_clipping_retains_visible_fill_when_both_observations_are_offscreen() {
    let model = ChartModel::new(
        vec![series(1, &[(-10.0, Some(5.0)), (20.0, Some(5.0))])],
        ChartSettings {
            x_range: Some(ChartRange {
                minimum: 0.0,
                maximum: 10.0,
            }),
            y_range: Some(ChartRange {
                minimum: 1.0,
                maximum: 4.0,
            }),
            ..settings(ChartView::Area)
        },
    )
    .unwrap();
    let g = model.geometry(plot()).unwrap();
    assert!(g.points.is_empty());
    assert!(g.segments.is_empty());
    assert_eq!(g.areas.len(), 1);
    assert_eq!(g.hit_test(Point::new(100.0, 100.0), 0.0), Some(key(1, 0)));
    assert_eq!(g.hit_test(Point::new(350.0, 100.0), 0.0), Some(key(1, 1)));
    for p in &g.areas[0].contour {
        assert!(p.x >= 20.0 && p.x <= 420.0 && p.y >= 30.0 && p.y <= 230.0);
    }
}

#[test]
fn area_rejects_unordered_categories_and_retains_exact_marker_selection() {
    for points in [
        vec![(1.0, Some(1.0)), (0.0, Some(2.0))],
        vec![(0.0, Some(1.0)), (-0.0, None)],
    ] {
        assert!(matches!(
            ChartModel::new(vec![series(1, &points)], settings(ChartView::Area)),
            Err(ChartError::InvalidTopology)
        ));
    }
    let m = ChartModel::new(
        vec![
            series(1, &[(0.0, Some(1.0)), (1.0, Some(2.0))]),
            series(2, &[(0.0, Some(3.0)), (1.0, Some(4.0))]),
        ],
        settings(ChartView::Area),
    )
    .unwrap();
    let g = m.geometry(plot()).unwrap();
    for p in &g.points {
        assert_eq!(g.hit_test(p.position, 0.1), Some(p.key));
    }
    assert_eq!(g.hit_test(Point::new(300.0, 210.0), 0.0), Some(key(2, 1)));
}

#[test]
fn dense_area_batches_stay_within_shared_path_limits() {
    use rust_desktop_ui_controls::{Controls, Theme};
    use rust_desktop_ui_core::{LayoutStyle, Length, MAX_FILL_EDGES, Size};
    let observations: Vec<_> = (0..4096)
        .map(|i| (i as f64, Some(if i % 2 == 0 { 1.0 } else { -1.0 })))
        .collect();
    let model = ChartModel::new(vec![series(1, &observations)], settings(ChartView::Area)).unwrap();
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    ui.chart(
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
    let paths = &ui.scene().fill_paths;
    assert!(paths.len() > 1 && paths.len() < 128, "{}", paths.len());
    assert!(
        paths
            .iter()
            .all(|p| p.is_valid() && p.edge_count().unwrap() <= MAX_FILL_EDGES)
    );
    assert_eq!(
        paths.iter().map(|p| p.contours.len()).sum::<usize>(),
        4095 * 2
    );
}
