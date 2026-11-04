use super::*;
use rust_desktop_ui_core::{FillPath, FillRule};

fn settings(view: ChartView) -> ChartSettings {
    ChartSettings {
        view,
        ..Default::default()
    }
}
fn key(point: i64) -> ChartPointKey {
    ChartPointKey { series: 0, point }
}
fn slices() -> Vec<ChartSlice> {
    vec![
        ChartSlice {
            id: 1,
            label: "One".into(),
            value: 3.0,
        },
        ChartSlice {
            id: 2,
            label: "Two".into(),
            value: 1.0,
        },
        ChartSlice {
            id: 3,
            label: "Zero".into(),
            value: 0.0,
        },
    ]
}
fn hierarchy() -> Vec<ChartHierarchyNode> {
    vec![
        ChartHierarchyNode {
            id: 10,
            parent: None,
            label: "Root".into(),
            value: None,
        },
        ChartHierarchyNode {
            id: 11,
            parent: Some(10),
            label: "A".into(),
            value: Some(3.0),
        },
        ChartHierarchyNode {
            id: 12,
            parent: Some(10),
            label: "B".into(),
            value: Some(1.0),
        },
    ]
}
fn heatmap() -> ChartFamily {
    ChartFamily::Heatmap {
        columns: vec![
            HeatmapAxisItem {
                id: 1,
                label: "Mon".into(),
            },
            HeatmapAxisItem {
                id: 2,
                label: "Tue".into(),
            },
        ],
        rows: vec![
            HeatmapAxisItem {
                id: 1,
                label: "AM".into(),
            },
            HeatmapAxisItem {
                id: 2,
                label: "PM".into(),
            },
        ],
        cells: vec![
            HeatmapCell {
                id: 10,
                column: 1,
                row: 1,
                value: Some(-2.0),
            },
            HeatmapCell {
                id: 11,
                column: 2,
                row: 1,
                value: None,
            },
            HeatmapCell {
                id: 12,
                column: 1,
                row: 2,
                value: Some(0.0),
            },
            HeatmapCell {
                id: 13,
                column: 2,
                row: 2,
                value: Some(2.0),
            },
        ],
    }
}
fn flow() -> ChartFamily {
    ChartFamily::Flow {
        nodes: (1..=4)
            .map(|id| ChartFlowNode {
                id,
                label: format!("N{id}"),
            })
            .collect(),
        links: vec![
            ChartFlowLink {
                id: 10,
                source: 1,
                target: 2,
                value: 3.0,
            },
            ChartFlowLink {
                id: 11,
                source: 1,
                target: 3,
                value: 1.0,
            },
            ChartFlowLink {
                id: 12,
                source: 2,
                target: 4,
                value: 3.0,
            },
            ChartFlowLink {
                id: 13,
                source: 3,
                target: 4,
                value: 1.0,
            },
        ],
    }
}
fn models() -> Vec<ChartModel> {
    vec![
        ChartModel::from_family(ChartFamily::Pie(slices()), settings(ChartView::Pie)).unwrap(),
        ChartModel::from_family(heatmap(), settings(ChartView::Heatmap)).unwrap(),
        ChartModel::from_family(
            ChartFamily::Hierarchy(hierarchy()),
            settings(ChartView::Treemap),
        )
        .unwrap(),
        ChartModel::from_family(
            ChartFamily::Hierarchy(hierarchy()),
            settings(ChartView::Sunburst),
        )
        .unwrap(),
        ChartModel::from_family(flow(), settings(ChartView::Sankey)).unwrap(),
    ]
}
#[test]
fn region_anchors_are_real_hit_targets_and_paths_fit_shared_renderer_budget() {
    for model in models() {
        assert!(model.series().is_empty());
        let plot = Rect::new(10.0, 20.0, 600.0, 300.0);
        let g = model.geometry(plot).unwrap();
        assert!(g.x_ticks.is_empty() && g.y_ticks.is_empty());
        assert!(!g.regions.is_empty());
        for region in &g.regions {
            assert_eq!(
                g.hit_test(region.anchor, 0.0),
                Some(region.key),
                "{:?} {:?}",
                model.settings().view,
                region.key
            );
            assert!(model.describe(region.key).is_some());
            if let ChartRegionShape::Polygon(points) = &region.shape {
                let path = FillPath {
                    contours: vec![points.clone()],
                    clip: plot,
                    rule: FillRule::NonZero,
                    color: Color::rgb(0, 0, 0),
                };
                assert!(path.is_valid());
                assert!(path.edge_count().unwrap() <= 256);
            }
        }
        assert_eq!(g.hit_test(Point::new(-1.0, -1.0), 1000.0), None);
    }
}
#[test]
fn pie_ratios_zero_and_atomic_replacement_are_not_cartesian_observations() {
    let mut model = models().remove(0);
    assert!(model.point(key(1)).is_none());
    assert_eq!(model.describe(key(1)).unwrap(), "One: 3; 75%");
    model.select(Some(key(1))).unwrap();
    assert_eq!(model.select(Some(key(3))), Err(ChartError::UnknownPoint));
    let before = model.geometry(Rect::new(0.0, 0.0, 300.0, 300.0)).unwrap();
    assert_eq!(before.regions.len(), 2);
    let mut bad = slices();
    bad[0].value = -1.0;
    assert_eq!(
        model.replace_family(ChartFamily::Pie(bad), settings(ChartView::Pie)),
        Err(ChartError::InvalidWeight)
    );
    assert_eq!(model.selected(), Some(key(1)));
    assert_eq!(model.geometry(before.plot).unwrap().regions, before.regions);
    let mut changed = slices();
    changed.reverse();
    changed[2].value = 9.0;
    model
        .replace_family(ChartFamily::Pie(changed), settings(ChartView::Pie))
        .unwrap();
    assert_eq!(model.selected(), Some(key(1)));
    model
        .replace_family(heatmap(), settings(ChartView::Heatmap))
        .unwrap();
    assert_eq!(model.selected(), None);
    assert_eq!(model.describe(key(13)).unwrap(), "PM / Tue: 2");
}
#[test]
fn topology_duplicates_overflow_and_wrong_view_reject_without_phantom_data() {
    let mut nodes = hierarchy();
    nodes[0].parent = Some(12);
    assert!(matches!(
        ChartModel::from_family(ChartFamily::Hierarchy(nodes), settings(ChartView::Treemap)),
        Err(ChartError::InvalidTopology)
    ));
    let mut nodes = hierarchy();
    nodes[0].value = Some(4.0);
    assert!(matches!(
        ChartModel::from_family(ChartFamily::Hierarchy(nodes), settings(ChartView::Sunburst)),
        Err(ChartError::InvalidWeight)
    ));
    let mut data = flow();
    let ChartFamily::Flow { links, .. } = &mut data else {
        unreachable!()
    };
    links.push(ChartFlowLink {
        id: 14,
        source: 4,
        target: 1,
        value: 1.0,
    });
    assert!(matches!(
        ChartModel::from_family(data, settings(ChartView::Sankey)),
        Err(ChartError::InvalidTopology)
    ));
    let mut data = heatmap();
    let ChartFamily::Heatmap { cells, .. } = &mut data else {
        unreachable!()
    };
    cells[2].column = 1;
    cells[2].row = 1;
    assert!(matches!(
        ChartModel::from_family(data, settings(ChartView::Heatmap)),
        Err(ChartError::DuplicateIdentity)
    ));
    let mut data = slices();
    data[0].value = f64::MAX;
    data[1].value = f64::MAX;
    assert!(matches!(
        ChartModel::from_family(ChartFamily::Pie(data), settings(ChartView::Pie)),
        Err(ChartError::CoordinateOverflow)
    ));
    assert!(matches!(
        ChartModel::new(vec![], settings(ChartView::Pie)),
        Err(ChartError::InvalidView)
    ));
    assert!(matches!(
        ChartModel::from_family(flow(), settings(ChartView::Pie)),
        Err(ChartError::InvalidView)
    ));
}
#[test]
fn heatmap_missing_values_and_flow_widths_keep_typed_meaning() {
    let model = models().remove(1);
    let g = model.geometry(Rect::new(0.0, 0.0, 200.0, 100.0)).unwrap();
    assert_eq!(
        g.regions
            .iter()
            .map(|r| r.intensity.unwrap())
            .collect::<Vec<_>>(),
        vec![0.0, 0.5, 1.0]
    );
    assert_eq!(g.hit_test(Point::new(150.0, 25.0), 100.0), None);
    let model = models().pop().unwrap();
    let g = model.geometry(Rect::new(0.0, 0.0, 600.0, 300.0)).unwrap();
    let shape_height = |id| {
        let r = g
            .regions
            .iter()
            .find(|r| {
                r.key
                    == ChartPointKey {
                        series: 1,
                        point: id,
                    }
            })
            .unwrap();
        let ChartRegionShape::Polygon(p) = &r.shape else {
            unreachable!()
        };
        p.last().unwrap().y - p[0].y
    };
    assert!((shape_height(10) / shape_height(11) - 3.0).abs() < 0.0001);
    assert_eq!(model.describe(key(1)).unwrap(), "N1: вход 0; выход 4");
}
#[test]
fn subnormal_flow_weights_are_normalized_before_pixel_scaling() {
    let data = ChartFamily::Flow {
        nodes: vec![
            ChartFlowNode {
                id: 1,
                label: "A".into(),
            },
            ChartFlowNode {
                id: 2,
                label: "B".into(),
            },
        ],
        links: vec![ChartFlowLink {
            id: 10,
            source: 1,
            target: 2,
            value: f64::from_bits(1),
        }],
    };
    let model = ChartModel::from_family(data, settings(ChartView::Sankey)).unwrap();
    let geometry = model.geometry(Rect::new(0.0, 0.0, 600.0, 300.0)).unwrap();
    assert_eq!(geometry.regions.len(), 3);
    assert_eq!(
        geometry.hit_test(Point::new(300.0, 150.0), 0.0),
        Some(ChartPointKey {
            series: 1,
            point: 10
        })
    );
}
#[test]
fn family_views_paint_select_and_cancel_stale_captured_presses() {
    for theme in [Theme::FluentLight, Theme::FluentDark] {
        for model in models() {
            let mut ui = Controls::new(theme).unwrap();
            let id = ui
                .chart(
                    ui.root(),
                    model,
                    LayoutStyle {
                        width: Length::Px(640.0),
                        height: Length::Px(400.0),
                        ..Default::default()
                    },
                )
                .unwrap();
            ui.update(Size::new(680.0, 440.0)).unwrap();
            let g = ui.chart_geometry(id).unwrap().unwrap();
            let first = g.points[0];
            let down = InputEvent::PointerDown {
                position: first.position,
                button: PointerButton::Primary,
                modifiers: Default::default(),
            };
            ui.dispatch(down.clone()).unwrap();
            ui.dispatch(InputEvent::PointerUp {
                position: first.position,
                button: PointerButton::Primary,
                modifiers: Default::default(),
            })
            .unwrap();
            assert_eq!(ui.chart_model(id).unwrap().selected(), Some(first.key));
            assert_eq!(
                ui.chart_semantic_value(id),
                ui.chart_model(id).unwrap().describe(first.key).as_deref()
            );
            ui.dispatch(down).unwrap();
            let replacement = ui.chart_model(id).unwrap().clone();
            ui.set_chart_model(id, replacement).unwrap();
            assert_eq!(ui.runtime.captured(), None);
            ui.update(Size::new(680.0, 440.0)).unwrap();
            assert!(ui.scene().fill_paths.iter().all(|p| p.is_valid()));
        }
    }
}
