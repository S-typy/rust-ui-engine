use super::*;
use rust_desktop_ui_data::{DataRow, FieldSchema, TableSchema};

fn series(id: u64, points: &[(f64, Option<f64>)]) -> ChartSeries {
    ChartSeries {
        id,
        label: format!("Series {id}"),
        points: points
            .iter()
            .enumerate()
            .map(|(i, (x, y))| ChartPoint {
                id: i as i64,
                x: *x,
                y: *y,
            })
            .collect(),
    }
}
fn plot() -> Rect {
    Rect::new(20.0, 30.0, 400.0, 200.0)
}
#[test]
fn chart_model_null_breaks_lines_and_shared_hit_matches_geometry() {
    for view in [
        ChartView::Line,
        ChartView::Sparkline,
        ChartView::Scatter,
        ChartView::Bar,
    ] {
        let m = ChartModel::new(
            vec![series(
                9,
                &[
                    (0.0, Some(1.0)),
                    (1.0, Some(3.0)),
                    (2.0, None),
                    (3.0, Some(-1.0)),
                    (4.0, Some(2.0)),
                ],
            )],
            ChartSettings {
                view,
                ..Default::default()
            },
        )
        .unwrap();
        let g = m.geometry(plot()).unwrap();
        assert_eq!(g.points.len(), 4);
        assert_eq!(
            g.segments.len(),
            if matches!(view, ChartView::Line | ChartView::Sparkline) {
                2
            } else {
                0
            }
        );
        assert_eq!(g.bars.len(), if view == ChartView::Bar { 4 } else { 0 });
        for p in &g.points {
            assert_eq!(g.hit_test(p.position, 0.01), Some(p.key));
        }
        assert_eq!(g.hit_test(Point::new(-1.0, -1.0), 1000.0), None);
        for line in g.segments {
            for p in [line.start, line.end] {
                assert!(p.x >= 20.0 && p.x <= 420.0 && p.y >= 30.0 && p.y <= 230.0);
            }
        }
    }
}
#[test]
fn chart_model_manual_ranges_clip_crossings_without_bridging_nulls() {
    let settings = ChartSettings {
        x_range: Some(ChartRange {
            minimum: 0.0,
            maximum: 10.0,
        }),
        y_range: Some(ChartRange {
            minimum: 0.0,
            maximum: 10.0,
        }),
        ..Default::default()
    };
    let m = ChartModel::new(
        vec![series(
            0,
            &[
                (-10.0, Some(5.0)),
                (20.0, Some(5.0)),
                (2.0, None),
                (-10.0, Some(20.0)),
                (20.0, Some(20.0)),
            ],
        )],
        settings,
    )
    .unwrap();
    let g = m.geometry(plot()).unwrap();
    assert!(g.points.is_empty());
    assert_eq!(g.segments.len(), 1);
    assert_eq!(g.segments[0].start, Point::new(20.0, 130.0));
    assert_eq!(g.segments[0].end, Point::new(420.0, 130.0));
}
#[test]
fn chart_model_selection_survives_keyed_reorder_and_failure_is_atomic() {
    let mut m = ChartModel::new(
        vec![
            series(1, &[(0.0, Some(1.0)), (1.0, Some(2.0))]),
            series(2, &[(0.0, Some(3.0))]),
        ],
        Default::default(),
    )
    .unwrap();
    let key = ChartPointKey {
        series: 1,
        point: 1,
    };
    m.select(Some(key)).unwrap();
    let mut data = m.series().to_vec();
    data.reverse();
    data[1].points.reverse();
    m.replace(data.clone(), Default::default()).unwrap();
    assert_eq!(m.selected(), Some(key));
    let old = m.series().to_vec();
    data[0].points[0].x = f64::NAN;
    assert_eq!(
        m.replace(data, Default::default()),
        Err(ChartError::NonFinite)
    );
    assert_eq!(m.series(), old);
    assert_eq!(m.selected(), Some(key));
    let mut data = old;
    data[1].points[0].y = None;
    m.replace(data, Default::default()).unwrap();
    assert_eq!(m.selected(), None);
}
#[test]
fn chart_model_finite_extremes_empty_constants_and_public_range_guard() {
    let empty = ChartModel::new(Vec::new(), Default::default()).unwrap();
    assert_eq!(
        empty.y_range(),
        ChartRange {
            minimum: 0.0,
            maximum: 1.0
        }
    );
    assert!(empty.geometry(plot()).unwrap().points.is_empty());
    for n in [0.0, 1e-300, 1e300, -1e300] {
        let m = ChartModel::new(vec![series(1, &[(n, Some(n))])], Default::default()).unwrap();
        assert_eq!(m.geometry(plot()).unwrap().points.len(), 1);
    }
    assert!(
        ChartModel::new(
            vec![series(1, &[(-f64::MAX, Some(0.0)), (f64::MAX, Some(1.0))])],
            Default::default()
        )
        .is_err()
    );
    for r in [
        ChartRange {
            minimum: 0.0,
            maximum: 0.0,
        },
        ChartRange {
            minimum: f64::NAN,
            maximum: 1.0,
        },
        ChartRange {
            minimum: -f64::MAX,
            maximum: f64::MAX,
        },
    ] {
        assert!(r.ticks().is_err());
    }
    let m = ChartModel::new(
        vec![series(0, &[(1e16, Some(1.0)), (1e16 + 2.0, Some(2.0))])],
        Default::default(),
    )
    .unwrap();
    let g = m.geometry(plot()).unwrap();
    assert_eq!(g.points[0].position.x, 20.0);
    assert_eq!(g.points[1].position.x, 420.0);
    assert!(m.geometry(Rect::new(1e30, 0.0, 1.0, 100.0)).is_err());
}
#[test]
fn chart_model_limits_identity_and_conditioning_are_checked_before_publish() {
    assert!(ChartModel::new(vec![series(0, &[]); 17], Default::default()).is_err());
    let mut s = series(1, &[]);
    s.points = (0..4096)
        .map(|i| ChartPoint {
            id: i,
            x: i as f64,
            y: Some((i % 10) as f64),
        })
        .collect();
    let m = ChartModel::new(vec![s.clone()], Default::default()).unwrap();
    assert_eq!(m.geometry(plot()).unwrap().points.len(), 4096);
    s.points.push(ChartPoint {
        id: 4096,
        x: 4096.0,
        y: Some(1.0),
    });
    assert_eq!(
        ChartModel::new(vec![s], Default::default()).unwrap_err(),
        ChartError::Capacity
    );
    assert_eq!(
        ChartModel::new(vec![series(0, &[]), series(0, &[])], Default::default()).unwrap_err(),
        ChartError::DuplicateIdentity
    );
    let mut s = series(1, &[(0.0, Some(1.0)), (1.0, Some(2.0))]);
    s.points[1].id = 0;
    assert_eq!(
        ChartModel::new(vec![s], Default::default()).unwrap_err(),
        ChartError::DuplicateIdentity
    );
    assert_eq!(
        ChartModel::new(
            vec![series(0, &[(1e300, Some(0.0))])],
            ChartSettings {
                x_range: Some(ChartRange {
                    minimum: 0.0,
                    maximum: 1.0
                }),
                ..Default::default()
            }
        )
        .unwrap_err(),
        ChartError::CoordinateOverflow
    );
}
#[test]
fn chart_model_bar_baseline_negative_and_grouped_bars_are_inside_plot() {
    let m = ChartModel::new(
        vec![
            series(1, &[(0.0, Some(-3.0)), (1.0, Some(4.0))]),
            series(2, &[(0.0, Some(-1.0)), (1.0, Some(2.0))]),
        ],
        ChartSettings {
            view: ChartView::Bar,
            ..Default::default()
        },
    )
    .unwrap();
    let g = m.geometry(plot()).unwrap();
    assert_eq!(g.bars.len(), 4);
    assert!(m.y_range().minimum < 0.0 && m.y_range().maximum > 0.0);
    for b in &g.bars {
        assert!(
            b.bounds.x >= plot().x
                && b.bounds.y >= plot().y
                && b.bounds.x + b.bounds.width <= 420.01
                && b.bounds.y + b.bounds.height <= 230.01
        );
        let p = Point::new(
            b.bounds.x + b.bounds.width / 2.0,
            b.bounds.y + b.bounds.height / 2.0,
        );
        assert_eq!(g.hit_test(p, 0.0), Some(b.key));
    }
    assert!(g.bars[0].bounds.x + g.bars[0].bounds.width <= g.bars[2].bounds.x + 0.01);
    for count in [2, 3, 4] {
        let points: Vec<_> = (0..count).map(|x| (x as f64, Some(3.0))).collect();
        let m = ChartModel::new(
            vec![series(1, &points), series(2, &points)],
            ChartSettings {
                view: ChartView::Bar,
                ..Default::default()
            },
        )
        .unwrap();
        let g = m.geometry(plot()).unwrap();
        let width = g.bars[0].bounds.width;
        for b in &g.bars {
            assert!(
                (b.bounds.width - width).abs() < 0.001,
                "Endpoint bar was clipped: {b:?}"
            );
        }
        for p in &g.points {
            let bar = g.bars.iter().find(|b| b.key == p.key).unwrap();
            assert!((p.position.x - bar.bounds.x - bar.bounds.width / 2.0).abs() < 0.001);
        }
    }
    for xs in [[1e16, 1e16 + 2.0], [f64::from_bits(1), f64::from_bits(2)]] {
        let m = ChartModel::new(
            vec![series(1, &[(xs[0], Some(1.0)), (xs[1], Some(1.0))])],
            ChartSettings {
                view: ChartView::Bar,
                ..Default::default()
            },
        )
        .unwrap();
        let g = m.geometry(plot()).unwrap();
        assert_eq!(g.bars.len(), 2);
        assert!((g.bars[0].bounds.width - g.bars[1].bounds.width).abs() < 0.001);
        assert!(g.bars[0].bounds.x > plot().x);
        assert!(g.bars[1].bounds.x + g.bars[1].bounds.width < plot().x + plot().width);
    }
}
#[test]
fn chart_model_table_adapter_retains_full_keys_and_refuses_lossy_values() {
    let mut y = FieldSchema::new("y", FieldType::Real);
    y.nullable = true;
    let schema = TableSchema::new(vec![FieldSchema::new("x", FieldType::Integer), y]).unwrap();
    let table = DataTable::new(
        schema.clone(),
        vec![
            DataRow::new(i64::MIN, vec![CellValue::Integer(0), CellValue::Null]),
            DataRow::new(
                i64::MAX,
                vec![
                    CellValue::Integer(9_007_199_254_740_992),
                    CellValue::Real(2.0),
                ],
            ),
        ],
    )
    .unwrap();
    let s = ChartSeries::from_table(&table, 7, "Test", "x", "y").unwrap();
    assert_eq!(s.points[0].id, i64::MIN);
    assert_eq!(s.points[1].id, i64::MAX);
    assert_eq!(s.points[0].y, None);
    let table = DataTable::new(
        schema,
        vec![DataRow::new(
            1,
            vec![
                CellValue::Integer(9_007_199_254_740_993),
                CellValue::Real(2.0),
            ],
        )],
    )
    .unwrap();
    assert_eq!(
        ChartSeries::from_table(&table, 0, "Test", "x", "y"),
        Err(ChartError::InexactInteger)
    );
}
