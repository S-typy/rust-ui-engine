use super::presentation_tests::{fixture, grouped};
use super::*;
use rust_desktop_ui_core::Modifiers;
use rust_desktop_ui_data::*;
use std::time::Instant;
fn size() -> Size {
    Size::new(600., 400.)
}
fn rule(field: &str, kind: GridFormatKind) -> GridFormatRule {
    GridFormatRule {
        field: field.into(),
        kind,
        whole_row: false,
        enabled: true,
    }
}
#[test]
fn formatting_reacts_to_filter_and_edits_without_replacing_accepted_rules() {
    let (mut ui, id, source) = fixture();
    let rules = vec![
        rule(
            "amount",
            GridFormatKind::Expression(r#"gt(field("amount"),integer("6"))"#.into()),
        ),
        rule("category", GridFormatKind::Duplicates),
        rule("amount", GridFormatKind::Top(1)),
        rule("amount", GridFormatKind::AboveAverage),
    ];
    ui.set_grid_format_rules(id, rules.clone()).unwrap();
    let matched =
        |ui: &Controls, i: usize, k: i64| ui.grids[&id].source.projection.formats[i].contains(k);
    assert!(matched(&ui, 0, i64::MIN));
    assert!(matched(&ui, 1, 0));
    assert!(matched(&ui, 2, i64::MIN));
    assert!(matched(&ui, 3, i64::MIN));
    ui.set_grid_auto_filter(id, "amount", "<=3").unwrap();
    assert!(matched(&ui, 2, i64::MAX));
    assert!(matched(&ui, 3, i64::MAX));
    source
        .borrow_mut()
        .set_cell(i64::MIN, "amount", CellValue::Integer(1))
        .unwrap();
    ui.update(size()).unwrap();
    assert!(!matched(&ui, 0, i64::MIN));
    assert!(
        ui.set_grid_format_rules(id, vec![rule("category", GridFormatKind::DataBar)])
            .is_err()
    );
    assert_eq!(ui.grid_presentation(id).unwrap().format_rules, rules);
    let mut r = rules;
    r.reverse();
    r[0].enabled = false;
    r.pop();
    ui.set_grid_format_rules(id, r).unwrap();
    assert!(!matched(&ui, 0, i64::MAX));
}
#[test]
fn null_signed_and_equal_values_paint_and_template_semantics_follow_viewport() {
    let mut f = FieldSchema::new("n", FieldType::Integer);
    f.nullable = true;
    let t = DataTable::new(
        TableSchema::new(vec![f]).unwrap(),
        [Some(-4), Some(0), Some(4), None]
            .into_iter()
            .enumerate()
            .map(|(i, n)| {
                DataRow::new(
                    i as i64,
                    vec![n.map_or(CellValue::Null, CellValue::Integer)],
                )
            })
            .collect(),
    )
    .unwrap();
    let mut ui = Controls::new(Theme::FluentDark).unwrap();
    let id = ui
        .grid_control(
            ui.root(),
            Rc::new(RefCell::new(
                DataSource::new(CollectionAdapter::new(t)).unwrap(),
            )),
            Default::default(),
            LayoutStyle {
                height: Length::Px(280.),
                ..Default::default()
            },
        )
        .unwrap();
    for format in [
        GridFormatKind::DataBar,
        GridFormatKind::ColorScale,
        GridFormatKind::Icons,
    ] {
        ui.set_grid_format_rules(id, vec![rule("n", format.clone())])
            .unwrap();
        ui.update(size()).unwrap();
        let frame = ui.grid_frame(id).unwrap();
        if format == GridFormatKind::DataBar {
            assert!(
                frame
                    .scene
                    .rectangles
                    .iter()
                    .any(|r| r.color == Color::rgb(206, 86, 71) && r.bounds.width > 1.)
            );
        }
        if format == GridFormatKind::Icons {
            assert!(frame.scene.stroke_paths.len() >= 3);
        }
        ui.set_grid_auto_filter(id, "n", "=4").unwrap();
        ui.update(size()).unwrap();
        ui.grid_frame(id)
            .unwrap()
            .scene
            .texts
            .iter()
            .for_each(|r| assert!(r.is_valid()));
        ui.set_grid_auto_filter(id, "n", "").unwrap();
    }
    ui.set_grid_cell_template(
        id,
        "n",
        Some(GridCellTemplate {
            prefix: "[".into(),
            suffix: "]".into(),
            ..Default::default()
        }),
    )
    .unwrap();
    assert_eq!(ui.grid_cell_accessible_label(id, ColumnId(0), "4"), "[4]");
}
#[test]
fn named_profile_restores_query_bands_rules_and_handles_schema_growth_atomically() {
    let (mut ui, id, _) = fixture();
    let mut p = grouped();
    p.bands = vec![GridBand {
        title: "Data".into(),
        fields: vec!["category".into(), "amount".into()],
    }];
    p.format_rules = vec![rule("amount", GridFormatKind::DataBar)];
    ui.set_grid_presentation(id, p).unwrap();
    ui.set_grid_sort(
        id,
        vec![SortDescriptor {
            field: "amount".into(),
            direction: rust_desktop_ui_data::SortDirection::Descending,
        }],
    )
    .unwrap();
    ui.set_grid_facet(id, "category", Some(vec![CellValue::Text("A".into())]))
        .unwrap();
    ui.set_grid_auto_filter(id, "amount", ">=3").unwrap();
    let json = ui.save_grid_layout(id, "orders").unwrap();
    let mut fields = ui
        .grid_source(id)
        .unwrap()
        .borrow()
        .table()
        .schema()
        .fields()
        .to_vec();
    fields.push(FieldSchema::new("new", FieldType::Text));
    let t = DataTable::new(
        TableSchema::new(fields).unwrap(),
        vec![DataRow::new(
            11,
            vec![
                CellValue::Text("A".into()),
                CellValue::Integer(3),
                CellValue::Text("n".into()),
            ],
        )],
    )
    .unwrap();
    let other = ui
        .grid_control(
            ui.root(),
            Rc::new(RefCell::new(
                DataSource::new(CollectionAdapter::new(t)).unwrap(),
            )),
            Default::default(),
            Default::default(),
        )
        .unwrap();
    ui.restore_grid_layout(other, "orders", &json).unwrap();
    assert_eq!(ui.grids[&other].source.keys, [11]);
    assert_eq!(
        ui.grid_arrangement(other).unwrap().last().unwrap().field,
        "new"
    );
    let saved = ui.save_grid_layout(other, "orders").unwrap();
    let mut broken: serde_json::Value = serde_json::from_str(&saved).unwrap();
    broken["columns"][0]["width"] = 1.into();
    assert!(
        ui.restore_grid_layout(other, "orders", &broken.to_string())
            .is_err()
    );
    assert_eq!(saved, ui.save_grid_layout(other, "orders").unwrap());
    assert!(ui.restore_grid_layout(other, "wrong", &json).is_err());
    assert!(ui.restore_grid_layout(other, "orders", "{").is_err());
}
#[test]
fn output_follows_filtered_order_precision_and_page_intersections() {
    let (mut ui, id, source) = fixture();
    let text = "№, \"A\"\r\n<cat>";
    source
        .borrow_mut()
        .set_cell(0, "category", CellValue::Text(text.into()))
        .unwrap();
    ui.update(size()).unwrap();
    ui.set_grid_sort(
        id,
        vec![SortDescriptor {
            field: "amount".into(),
            direction: rust_desktop_ui_data::SortDirection::Descending,
        }],
    )
    .unwrap();
    ui.set_grid_auto_filter(id, "amount", "<=3").unwrap();
    let out = ui
        .grid_output(id, &["amount".into(), "category".into()], false)
        .unwrap();
    assert_eq!(out.record_count(), 2);
    assert_eq!(
        out.csv(),
        "amount,category\r\n3,A\r\n2,\"№, \"\"A\"\"\r\n<cat>\"\r\n"
    );
    assert!(out.html().contains("&lt;cat&gt;"));
    assert!(out.rtf().contains("\\u8470?"));
    assert_eq!(&out.xls().unwrap()[..2], &[9, 4]);
    assert!(rust_desktop_ui_data::workbook::Workbook::from_xlsx(&out.xlsx().unwrap()).is_ok());
    let opt = GridPrintOptions {
        paper: Size::new(190., 94.),
        margin: 8.,
        row_height: 24.,
        range: None,
    };
    let doc = out.print_document(opt, || false).unwrap();
    assert_eq!(doc.pages().len(), 4);
    for (i, page) in doc.pages().iter().enumerate() {
        assert!(
            page.scene
                .texts
                .iter()
                .any(|t| t.text == format!("{} / 4", i + 1))
        );
        assert!(page.scene.texts.iter().all(|t| t.clip.x >= 8.
            && t.clip.y >= 8.
            && t.clip.x + t.clip.width <= 182.
            && t.clip.y + t.clip.height <= 86.));
    }
    let r = out
        .print_document(
            GridPrintOptions {
                range: Some((2, 3)),
                ..opt
            },
            || false,
        )
        .unwrap();
    assert_eq!(r.pages().len(), 2);
    ui.set_grid_selected_key(id, Some(0)).unwrap();
    assert_eq!(ui.grid_output(id, &[], true).unwrap().record_count(), 1);
    assert!(out.print_document(opt, || true).is_err());
    assert_eq!(ui.grid_selected_key(id), Some(0));
}
#[test]
fn million_row_page_windows_cancel_retry_recycle_and_preserve_logical_selection() {
    let schema = TableSchema::new(
        (0..32)
            .map(|i| FieldSchema::new(format!("f{i}"), FieldType::Integer))
            .collect(),
    )
    .unwrap();
    let pages = PagedDataSource::new(
        schema.clone(),
        Default::default(),
        PageCacheSettings {
            page_size: 64,
            max_pages: 3,
            max_rows: 192,
            ..Default::default()
        },
        QueryCapabilities::complete(),
    )
    .unwrap();
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let root = ui.root();
    let mut v = GridPageView::mount(
        &mut ui,
        root,
        pages,
        LayoutStyle {
            width: Length::Px(480.),
            height: Length::Px(240.),
            ..Default::default()
        },
    )
    .unwrap();
    let tx = v.source().completion_sender();
    let make = |req: &PageRequest| PageResponse {
        table: DataTable::new(
            schema.clone(),
            (req.offset..req.offset + 64)
                .map(|n| DataRow::new(n as i64, vec![CellValue::Integer(n as i64); 32]))
                .collect(),
        )
        .unwrap(),
        snapshot_revision: 7,
        total: PageTotal::Known(1_000_000),
        has_more: req.offset + 64 < 1_000_000,
    };
    let req = v.navigate(&mut ui, 0).unwrap().pop().unwrap();
    tx.try_complete(&req, Ok(make(&req))).unwrap();
    v.poll(&mut ui).unwrap();
    v.select(&mut ui, Some(2)).unwrap();
    let req = v.navigate(&mut ui, 4096).unwrap().pop().unwrap();
    let far = v.navigate(&mut ui, 999_999).unwrap().pop().unwrap();
    assert!(req.cancellation.is_cancelled());
    assert!(tx.try_complete(&req, Ok(make(&req))).is_err());
    tx.try_complete(&far, Err("offline".into())).unwrap();
    v.poll(&mut ui).unwrap();
    assert!(matches!(v.status(), PageStatus::Failed(_)));
    let retry = v.retry().unwrap();
    tx.try_complete(&retry, Ok(make(&retry))).unwrap();
    v.poll(&mut ui).unwrap();
    assert_eq!(v.source().selected_key(), Some(2));
    assert!(
        v.accept_totals(
            retry.token,
            7,
            vec![("Count".into(), CellValue::Integer(1_000_000))]
        )
        .unwrap()
    );
    let branch = v
        .open_path(&mut ui, vec![("f0".into(), CellValue::Integer(1))])
        .unwrap()
        .pop()
        .unwrap();
    assert!(branch.query.filter.is_some());
    assert!(v.totals().is_empty());
    let root_request = v.open_path(&mut ui, vec![]).unwrap().pop().unwrap();
    assert!(branch.cancellation.is_cancelled());
    assert!(!v.accept_totals(retry.token, 7, vec![]).unwrap());
    tx.try_complete(&root_request, Ok(make(&root_request)))
        .unwrap();
    v.poll(&mut ui).unwrap();
    assert_eq!(ui.grid_selected_key(v.id()), Some(2));
    ui.set_grid_selected_key(v.id(), None).unwrap();
    let mut times = Vec::new();
    let mut max_cells = 0;
    let mut requests = 0;
    for i in 1..=40 {
        let start = Instant::now();
        let batch = v.navigate(&mut ui, i * 2048).unwrap();
        requests += batch.len();
        for req in batch {
            tx.try_complete(&req, Ok(make(&req))).unwrap()
        }
        v.poll(&mut ui).unwrap();
        ui.update(size()).unwrap();
        times.push(start.elapsed().as_micros());
        let frame = ui.grid_frame(v.id()).unwrap();
        max_cells = max_cells.max(
            frame
                .semantics
                .rows
                .iter()
                .map(|r| r.cells.len())
                .sum::<usize>(),
        );
        assert!(v.source().stats().rows <= 192);
    }
    times.sort();
    println!(
        "GRID_MILLION pages={requests} cells={max_cells} retained={} p50_us={} p95_us={}",
        v.source().stats().rows,
        times[20],
        times[38]
    );
    assert_eq!(requests, 40);
    assert_eq!(v.source().selected_key(), None);
    assert!(max_cells <= 100);
}
#[test]
fn local_hundred_thousand_records_bound_realized_geometry() {
    let schema = TableSchema::new(vec![
        FieldSchema::new("id", FieldType::Integer),
        FieldSchema::new("region", FieldType::Text),
    ])
    .unwrap();
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let mut preparation = Vec::new();
    let mut scroll = Vec::new();
    let mut peak = 0;
    for _ in 0..3 {
        let start = Instant::now();
        let rows = (0..100_000)
            .map(|n| {
                DataRow::new(
                    n,
                    vec![
                        CellValue::Integer(n),
                        CellValue::Text(format!("Region {}", n % 7)),
                    ],
                )
            })
            .collect();
        let source = DataSource::new(CollectionAdapter::new(
            DataTable::new(schema.clone(), rows).unwrap(),
        ))
        .unwrap();
        let id = ui
            .grid_control(
                ui.root(),
                Rc::new(RefCell::new(source)),
                Default::default(),
                LayoutStyle {
                    width: Length::Px(480.),
                    height: Length::Px(240.),
                    ..Default::default()
                },
            )
            .unwrap();
        ui.update(size()).unwrap();
        preparation.push(start.elapsed().as_millis());
        for i in 0..30 {
            let t = Instant::now();
            ui.grid_scroll_to(id, 0., (i * 90000) as f64).unwrap();
            ui.update(size()).unwrap();
            scroll.push(t.elapsed().as_micros());
            let frame = ui.grid_frame(id).unwrap();
            assert_eq!(frame.stats.logical_rows, 100000);
            peak = peak.max(
                frame
                    .semantics
                    .rows
                    .iter()
                    .map(|r| r.cells.len())
                    .sum::<usize>(),
            );
        }
        ui.remove(id).unwrap();
    }
    preparation.sort();
    scroll.sort();
    println!(
        "GRID_LOCAL prepare_p50_ms={} prepare_p95_ms={} scroll_p50_us={} scroll_p95_us={} cells={peak}",
        preparation[1], preparation[2], scroll[45], scroll[85]
    );
    assert!(peak <= 40);
}

#[test]
fn measured_rows_context_and_pinned_navigation_preserve_logical_identity() {
    let (mut ui, id, source) = fixture();
    let long = "один два три четыре пять шесть ".repeat(10);
    source
        .borrow_mut()
        .set_cell(0, "category", CellValue::Text(long.clone()))
        .unwrap();
    source
        .borrow_mut()
        .set_cell(i64::MAX, "category", CellValue::Text(long.clone()))
        .unwrap();
    let mut p = GridPresentationSettings {
        wrap_fields: vec!["category".into()],
        merge_fields: vec!["category".into()],
        ..Default::default()
    };
    p.text_templates
        .insert("Header".into(), ("«".into(), "»".into()));
    ui.set_grid_presentation(id, p).unwrap();
    ui.update(size()).unwrap();
    let g = &ui.grids[&id];
    let index = g.model.visible_index(&g.source, row_key(0)).unwrap();
    assert!(g.model.record_height(index) > g.model.config().row_height);
    assert_eq!(
        g.model.first_at_offset(g.model.record_top(index) + 5.),
        index
    );
    let rendered = g
        .frame
        .scene
        .texts
        .iter()
        .filter(|t| t.text == long)
        .count();
    assert_eq!(rendered, 1);
    ui.commands_mut().register("Inspect", "Inspect row");
    ui.set_grid_context_menus(
        id,
        vec![MenuItem::new("Inspect", "Inspect")],
        vec![],
        vec![MenuItem::new("Inspect", "Inspect")],
    )
    .unwrap();
    ui.grid_focus_cell(id, 0, None).unwrap();
    assert!(
        ui.grid_custom_input(
            id,
            &InputEvent::KeyDown {
                key: Key::F10,
                modifiers: Modifiers {
                    shift: true,
                    ..Default::default()
                },
                repeat: false
            }
        )
        .unwrap()
    );
    assert!(ui.popup_open());
    assert_eq!(
        ui.grid_context_target(id),
        Some(&GridContextTarget::Record(0))
    );
    ui.close_popup().unwrap();
    ui.set_grid_navigation_rows(id, vec![0], vec![i64::MAX])
        .unwrap();
    ui.update(size()).unwrap();
    assert!(ui.grids[&id].pinned_height() > 0.);
    assert_eq!(ui.grid_selected_key(id), Some(0));
    assert!(ui.auto_fit_grid_column(id, "category").unwrap() > 160.);
    ui.set_grid_filter(id, "B").unwrap();
    ui.update(size()).unwrap();
    assert_eq!(ui.grids[&id].pinned_height(), 0.);
}

#[test]
fn collapsing_multiple_provider_branches_cancels_descendants_and_keeps_siblings() {
    let schema = TableSchema::new(vec![
        FieldSchema::new("region", FieldType::Integer),
        FieldSchema::new("city", FieldType::Text),
    ])
    .unwrap();
    let cache = PagedDataSource::new(
        schema,
        Default::default(),
        Default::default(),
        QueryCapabilities::complete(),
    )
    .unwrap();
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let mut groups = GridPageBranches::new(ui.root(), &cache);
    let north = vec![("region".into(), CellValue::Integer(1))];
    let south = vec![("region".into(), CellValue::Integer(2))];
    let mut city = north.clone();
    city.push(("city".into(), CellValue::Text("Омск".into())));
    assert!(groups.expand(&mut ui, city.clone()).is_err());
    let n = groups
        .expand(&mut ui, north.clone())
        .unwrap()
        .pop()
        .unwrap()
        .0;
    let s = groups
        .expand(&mut ui, south.clone())
        .unwrap()
        .pop()
        .unwrap()
        .0;
    let c = groups
        .expand(&mut ui, city.clone())
        .unwrap()
        .pop()
        .unwrap()
        .0;
    assert!(matches!(c.query.filter, Some(Criteria::All(_))));
    assert!(groups.expand(&mut ui, city.clone()).unwrap().is_empty());
    groups.collapse(&mut ui, &north).unwrap();
    assert!(n.cancellation.is_cancelled() && c.cancellation.is_cancelled());
    assert!(!s.cancellation.is_cancelled());
    assert!(groups.page(&city).is_none());
    groups.collapse(&mut ui, &[]).unwrap();
    assert!(s.cancellation.is_cancelled());
    assert!(groups.page(&south).is_none());
}
