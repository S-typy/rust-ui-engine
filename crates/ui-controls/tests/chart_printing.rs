use rust_desktop_ui_controls::{
    ChartFamily, ChartModel, ChartPoint, ChartPointKey, ChartRange, ChartScale, ChartSeries,
    ChartSettings, ChartSlice, ChartView, Controls, Theme,
    pdf::PdfError,
    printing::{ChartPrintOptions, export_pdf, from_chart},
};
use rust_desktop_ui_core::{LayoutStyle, Length, Rect, Size};
use rust_desktop_ui_text::TextEngine;

fn cartesian(view: ChartView) -> ChartModel {
    ChartModel::new(
        vec![ChartSeries {
            id: 7,
            label: "Выручка".into(),
            points: (0..5)
                .map(|i| ChartPoint {
                    id: i,
                    x: i as f64,
                    y: Some((i + 1) as f64),
                })
                .collect(),
        }],
        ChartSettings {
            view,
            x_title: "Месяц".into(),
            y_title: "Сумма".into(),
            ..Default::default()
        },
    )
    .unwrap()
}

fn unchanged(model: &ChartModel, before: &ChartModel) {
    assert_eq!(model.series(), before.series());
    assert_eq!(model.family(), before.family());
    assert_eq!(model.settings(), before.settings());
    assert_eq!(model.x_range(), before.x_range());
    assert_eq!(model.y_range(), before.y_range());
    assert_eq!(model.selected(), before.selected());
}

#[test]
fn snapshot_keeps_viewport_selection_and_has_the_same_ink_as_the_control() {
    for theme in [Theme::FluentLight, Theme::FluentDark] {
        for view in [
            ChartView::Line,
            ChartView::Scatter,
            ChartView::Bar,
            ChartView::StackedBar,
            ChartView::Area,
            ChartView::StackedArea,
            ChartView::Sparkline,
        ] {
            let mut model = cartesian(view);
            model
                .select(Some(ChartPointKey {
                    series: 7,
                    point: 2,
                }))
                .unwrap();
            model
                .set_view_ranges(
                    Some(ChartRange {
                        minimum: 1.0,
                        maximum: 3.5,
                    }),
                    Some(ChartRange {
                        minimum: 1.0,
                        maximum: 4.0,
                    }),
                )
                .unwrap();
            let before = model.clone();
            let size = Size::new(800.0, 500.0);
            let doc = from_chart(
                &model,
                &mut TextEngine::new(),
                "Снимок",
                ChartPrintOptions {
                    page_size: size,
                    margin: 8.0,
                    theme,
                    include_selection: true,
                },
                || false,
            )
            .unwrap();
            unchanged(&model, &before);
            assert_eq!(doc.title(), "Снимок");
            assert_eq!(doc.pages().len(), 1);
            let mut ui = Controls::new(theme).unwrap();
            ui.chart(
                ui.root(),
                model,
                LayoutStyle {
                    width: Length::Percent(1.0),
                    height: Length::Percent(1.0),
                    ..Default::default()
                },
            )
            .unwrap();
            ui.update(size).unwrap();
            let page = &doc.pages()[0].scene;
            // The default host has 8px padding. Export clips at its margin;
            // the mounted control inherits the larger ancestor clip.
            let body = Rect::new(8.0, 8.0, size.width - 16.0, size.height - 16.0);
            let mut mounted = ui.scene().clone();
            for path in &mut mounted.stroke_paths {
                path.clip = path.clip.intersection(body).unwrap();
            }
            for rect in &mut mounted.rounded_rectangles {
                rect.clip = rect.clip.intersection(body).unwrap();
            }
            assert_eq!(page.stroke_paths, mounted.stroke_paths, "{view:?}");
            assert_eq!(page.fill_paths, mounted.fill_paths, "{view:?}");
            assert_eq!(
                page.rounded_rectangles, mounted.rounded_rectangles,
                "{view:?}"
            );
            if view != ChartView::Sparkline {
                for label in ["Выручка", "Месяц", "Сумма"] {
                    assert!(page.texts.iter().any(|t| t.text == label));
                }
            }
        }
    }
}

#[test]
fn logarithmic_grid_and_point_positions_match_mounted_and_printed_scenes() {
    for theme in [Theme::FluentLight, Theme::FluentDark] {
        let model = ChartModel::new(
            vec![ChartSeries {
                id: 1,
                label: "Powers".into(),
                points: [1.0, 10.0, 100.0, 1000.0]
                    .into_iter()
                    .enumerate()
                    .map(|(i, v)| ChartPoint {
                        id: i as i64,
                        x: v,
                        y: Some(v),
                    })
                    .collect(),
            }],
            ChartSettings {
                x_scale: ChartScale::Log10,
                y_scale: ChartScale::Log10,
                ..Default::default()
            },
        )
        .unwrap();
        let size = Size::new(800.0, 500.0);
        let doc = from_chart(
            &model,
            &mut TextEngine::new(),
            "Log10",
            ChartPrintOptions {
                page_size: size,
                margin: 8.0,
                theme,
                include_selection: false,
            },
            || false,
        )
        .unwrap();
        let mut ui = Controls::new(theme).unwrap();
        let id = ui
            .chart(
                ui.root(),
                model,
                LayoutStyle {
                    width: Length::Percent(1.0),
                    height: Length::Percent(1.0),
                    ..Default::default()
                },
            )
            .unwrap();
        ui.update(size).unwrap();
        let plot = ui.chart_geometry(id).unwrap().unwrap().plot;
        let page = &doc.pages()[0].scene;
        let mut mounted = ui.scene().clone();
        let body = Rect::new(8.0, 8.0, 784.0, 484.0);
        for path in &mut mounted.stroke_paths {
            path.clip = path.clip.intersection(body).unwrap();
        }
        assert_eq!(page.stroke_paths, mounted.stroke_paths);
        // Independent expected grid positions, not merely equality of two users
        // of the same painter: raw 10 and100 must lie at one and two thirds.
        for i in 0..=3 {
            let y = plot.y + plot.height * (1.0 - i as f32 / 3.0);
            assert!(page.stroke_paths.iter().flat_map(|p| &p.segments).any(
                |s| (s.start.y - y).abs() < 0.001
                    && (s.end.y - y).abs() < 0.001
                    && (s.start.x - plot.x).abs() < 0.001
                    && (s.end.x - plot.x - plot.width).abs() < 0.001
            ));
        }
        assert_eq!(page.texts.iter().filter(|t| t.text == "log10").count(), 2);
        assert!(
            export_pdf(&doc, &mut TextEngine::new(), || false)
                .unwrap()
                .starts_with(b"%PDF-")
        );
    }
}

#[test]
fn selection_is_opt_in_and_never_changes_the_source() {
    let mut model = cartesian(ChartView::Line);
    let key = ChartPointKey {
        series: 7,
        point: 2,
    };
    model.select(Some(key)).unwrap();
    let before = model.clone();
    let mut engine = TextEngine::new();
    let plain = from_chart(
        &model,
        &mut engine,
        "Plain",
        ChartPrintOptions::default(),
        || false,
    )
    .unwrap();
    let selected = from_chart(
        &model,
        &mut engine,
        "Selected",
        ChartPrintOptions {
            include_selection: true,
            ..Default::default()
        },
        || false,
    )
    .unwrap();
    unchanged(&model, &before);
    let description = model.describe(key).unwrap();
    assert!(
        !plain.pages()[0]
            .scene
            .texts
            .iter()
            .any(|t| t.text == description)
    );
    assert!(
        selected.pages()[0]
            .scene
            .texts
            .iter()
            .any(|t| t.text == description)
    );
    assert_eq!(
        selected.pages()[0].scene.rounded_rectangles.len(),
        plain.pages()[0].scene.rounded_rectangles.len() + 2
    );
}

#[test]
fn invalid_or_compact_pages_and_cancellation_do_not_publish_a_document() {
    let model = cartesian(ChartView::Line);
    let before = model.clone();
    let mut engine = TextEngine::new();
    for options in [
        ChartPrintOptions {
            page_size: Size::new(f32::NAN, 400.0),
            ..Default::default()
        },
        ChartPrintOptions {
            page_size: Size::new(19201.0, 400.0),
            ..Default::default()
        },
        ChartPrintOptions {
            page_size: Size::new(400.0, 400.0),
            ..Default::default()
        },
        ChartPrintOptions {
            margin: f32::INFINITY,
            ..Default::default()
        },
        ChartPrintOptions {
            margin: -1.0,
            ..Default::default()
        },
        ChartPrintOptions {
            margin: 600.0,
            ..Default::default()
        },
    ] {
        assert!(from_chart(&model, &mut engine, "Rejected", options, || false).is_err());
    }
    for title in ["bad\0title".to_string(), "x".repeat(1025)] {
        assert!(
            from_chart(
                &model,
                &mut engine,
                &title,
                ChartPrintOptions::default(),
                || false
            )
            .is_err()
        );
    }
    assert!(matches!(
        from_chart(
            &model,
            &mut engine,
            "Cancel",
            ChartPrintOptions::default(),
            || true
        ),
        Err(PdfError::Cancelled)
    ));
    let mut calls = 0;
    assert!(matches!(
        from_chart(
            &model,
            &mut engine,
            "Cancel after layout",
            ChartPrintOptions::default(),
            || {
                calls += 1;
                calls > 1
            }
        ),
        Err(PdfError::Cancelled)
    ));
    assert_eq!(calls, 2);
    unchanged(&model, &before);
    assert!(
        from_chart(
            &cartesian(ChartView::Sparkline),
            &mut engine,
            "Small spark",
            ChartPrintOptions {
                page_size: Size::new(80.0, 40.0),
                margin: 4.0,
                ..Default::default()
            },
            || false
        )
        .is_ok()
    );
}

#[test]
fn pdf_preserves_vector_geometry_page_size_and_unicode_text_objects() {
    let model = ChartModel::from_family(
        ChartFamily::Pie(vec![
            ChartSlice {
                id: 1,
                label: "Север".into(),
                value: 3.0,
            },
            ChartSlice {
                id: 2,
                label: "Юг".into(),
                value: 1.0,
            },
        ]),
        ChartSettings {
            view: ChartView::Pie,
            ..Default::default()
        },
    )
    .unwrap();
    let mut engine = TextEngine::new();
    let doc = from_chart(
        &model,
        &mut engine,
        "Распределение",
        ChartPrintOptions::default(),
        || false,
    )
    .unwrap();
    assert_eq!(doc.pages()[0].scene.fill_paths.len(), 2);
    let bytes = export_pdf(&doc, &mut engine, || false).unwrap();
    let parsed = lopdf::Document::load_mem(&bytes).unwrap();
    let page = *parsed.get_pages().values().next().unwrap();
    let media = parsed
        .get_dictionary(page)
        .unwrap()
        .get(b"MediaBox")
        .unwrap()
        .as_array()
        .unwrap();
    // Logical pixels are 1/96 inch; PDF points are 1/72 inch.
    assert_eq!(media[2].as_float().unwrap(), 768.0);
    assert_eq!(media[3].as_float().unwrap(), 480.0);
    let content = parsed.get_page_content(page);
    let ops = lopdf::content::Content::decode(&content).unwrap();
    assert!(ops.operations.iter().filter(|o| o.operator == "f").count() >= 2);
    assert!(ops.operations.iter().any(|o| {
        o.operator == "BDC"
            && o.operands
                .iter()
                .any(|v| v.as_dict().is_ok_and(|d| d.has(b"ActualText")))
    }));
}
