//! Small, explicitly typed chart observations for the gallery.
use rust_desktop_ui_controls::{
    ChartFamily, ChartFlowLink, ChartFlowNode, ChartHierarchyNode, ChartSlice, ChartView,
    HeatmapAxisItem, HeatmapCell,
};

pub(super) fn sample(view: ChartView, generation: u32) -> ChartFamily {
    let extra = f64::from(generation);
    match view {
        ChartView::Pie => ChartFamily::Pie(
            [
                ("Разработка", 5.0),
                ("Поддержка", 3.0),
                ("Исследования", 2.0),
            ]
            .into_iter()
            .enumerate()
            .map(|(i, (label, value))| ChartSlice {
                id: i as i64,
                label: label.into(),
                value: value + extra,
            })
            .collect(),
        ),
        ChartView::Heatmap => ChartFamily::Heatmap {
            columns: ["Пн", "Вт", "Ср", "Чт", "Пт"]
                .into_iter()
                .enumerate()
                .map(|(i, label)| HeatmapAxisItem {
                    id: i as i64,
                    label: label.into(),
                })
                .collect(),
            rows: ["Утро", "День", "Вечер"]
                .into_iter()
                .enumerate()
                .map(|(i, label)| HeatmapAxisItem {
                    id: i as i64,
                    label: label.into(),
                })
                .collect(),
            cells: (0..15)
                .map(|i| HeatmapCell {
                    id: i,
                    column: i % 5,
                    row: i / 5,
                    value: if i == 7 {
                        None
                    } else {
                        Some((i * 7 % 17) as f64 - 4.0 + extra)
                    },
                })
                .collect(),
        },
        ChartView::Treemap | ChartView::Sunburst => ChartFamily::Hierarchy(vec![
            ChartHierarchyNode {
                id: 0,
                parent: None,
                label: "Продукт".into(),
                value: None,
            },
            ChartHierarchyNode {
                id: 1,
                parent: Some(0),
                label: "Клиент".into(),
                value: None,
            },
            ChartHierarchyNode {
                id: 2,
                parent: Some(0),
                label: "Сервер".into(),
                value: None,
            },
            ChartHierarchyNode {
                id: 3,
                parent: Some(1),
                label: "Формы".into(),
                value: Some(4.0 + extra),
            },
            ChartHierarchyNode {
                id: 4,
                parent: Some(1),
                label: "Диаграммы".into(),
                value: Some(3.0),
            },
            ChartHierarchyNode {
                id: 5,
                parent: Some(2),
                label: "Запросы".into(),
                value: Some(2.0 + extra),
            },
            ChartHierarchyNode {
                id: 6,
                parent: Some(2),
                label: "Хранение".into(),
                value: Some(1.0),
            },
        ]),
        ChartView::Sankey => ChartFamily::Flow {
            nodes: ["Запросы", "Обработка", "Кэш", "Ответ"]
                .into_iter()
                .enumerate()
                .map(|(i, label)| ChartFlowNode {
                    id: i as i64,
                    label: label.into(),
                })
                .collect(),
            links: vec![
                ChartFlowLink {
                    id: 0,
                    source: 0,
                    target: 1,
                    value: 7.0 + extra,
                },
                ChartFlowLink {
                    id: 1,
                    source: 0,
                    target: 2,
                    value: 3.0,
                },
                ChartFlowLink {
                    id: 2,
                    source: 1,
                    target: 3,
                    value: 7.0 + extra,
                },
                ChartFlowLink {
                    id: 3,
                    source: 2,
                    target: 3,
                    value: 3.0,
                },
            ],
        },
        _ => unreachable!("Cartesian views use the table fixture"),
    }
}
