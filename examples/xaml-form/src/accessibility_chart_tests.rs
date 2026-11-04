use super::*;
use rust_desktop_ui_controls::{
    ChartModel, ChartPoint, ChartPointKey, ChartSeries, ChartSettings, Theme,
};
use rust_desktop_ui_core::{LayoutStyle, Size};

#[test]
fn chart_name_tracks_selection_without_losing_identity_custom_label_or_focus() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let model = ChartModel::new(
        vec![ChartSeries {
            id: 10,
            label: "Revenue".into(),
            points: vec![
                ChartPoint {
                    id: 20,
                    x: 1.0,
                    y: Some(8.0),
                },
                ChartPoint {
                    id: 21,
                    x: 2.0,
                    y: Some(-5.0),
                },
            ],
        }],
        ChartSettings::default(),
    )
    .unwrap();
    let chart = ui.chart(ui.root(), model, LayoutStyle::default()).unwrap();
    let mut view = MountedView::default();
    view.automation_names.insert(chart, "Sales".into());
    let mut a11y = Accessibility::default();
    ui.update(Size::new(900.0, 600.0)).unwrap();
    ui.perform_action(chart, SemanticAction::Focus, None)
        .unwrap();
    let empty = a11y.tree(&ui, &view, "Charts", 1.5);
    let id = a11y.id(chart).unwrap();
    assert_eq!(empty.focus, id);
    assert!(
        empty
            .nodes
            .iter()
            .find(|(n, _)| *n == id)
            .unwrap()
            .1
            .label()
            .unwrap()
            .starts_with("Sales. ")
    );
    for (point, expected) in [
        (20, "Sales. Revenue: X 1, Y 8"),
        (21, "Sales. Revenue: X 2, Y -5"),
    ] {
        ui.set_chart_selected(chart, Some(ChartPointKey { series: 10, point }))
            .unwrap();
        let tree = a11y.tree(&ui, &view, "Charts", 1.5);
        let node = &tree.nodes.iter().find(|(n, _)| *n == id).unwrap().1;
        assert_eq!(node.label(), Some(expected));
        assert_eq!(node.role(), Role::Group);
        assert_eq!(tree.focus, id);
        assert_eq!(tree.nodes.len(), empty.nodes.len());
    }
    ui.set_chart_selected(chart, None).unwrap();
    let tree = a11y.tree(&ui, &view, "Charts", 1.5);
    assert_eq!(
        tree.nodes.iter().find(|(n, _)| *n == id).unwrap().1.label(),
        empty
            .nodes
            .iter()
            .find(|(n, _)| *n == id)
            .unwrap()
            .1
            .label()
    );
}
