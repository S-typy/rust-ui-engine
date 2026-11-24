use super::*;

#[test]
fn hierarchy_uses_prefixes_preserves_collapsed_branches_and_sorts_subtotals() {
    let (mut ui, id, source) = fixture();
    let mut settings = grouped();
    settings.group_fields.push("amount".into());
    settings.hierarchical_groups = true;
    settings.group_footer = true;
    settings.group_sort_summary = Some(0);
    settings.group_descending = true;
    ui.set_grid_presentation(id, settings).unwrap();
    ui.update(Size::new(600.0, 400.0)).unwrap();
    let p = &ui.grids[&id].source.projection;
    assert_eq!(p.groups.len(), 5);
    let b = p.roots[0];
    let a = p.roots[1];
    assert!(p.synthetic[&b].label.contains("B"));
    assert_eq!(p.children[&a].len(), 3);
    let leaf = p.children[&a][0];
    assert_eq!(p.children[&leaf][0], row_key(i64::MAX));
    assert_eq!(
        p.synthetic[p.children[&a].last().unwrap()].label,
        "Total: 5"
    );
    ui.set_grid_expanded(id, leaf, false).unwrap();
    ui.set_grid_expanded(id, a, false).unwrap();
    ui.set_grid_expanded(id, a, true).unwrap();
    assert!(!ui.grids[&id].model.is_expanded(leaf));
    ui.set_grid_expanded(id, a, false).unwrap();
    source
        .borrow_mut()
        .set_cell(0, "amount", CellValue::Integer(6))
        .unwrap();
    ui.update(Size::new(600.0, 400.0)).unwrap();
    let grid = &ui.grids[&id];
    assert!(!grid.model.is_expanded(a));
    assert_eq!(grid.source.projection.roots[0], a);
    assert_eq!(
        grid.source.projection.synthetic[grid.source.projection.children[&a].last().unwrap()].label,
        "Total: 9"
    );
}

#[test]
fn calculated_grid_filter_sort_edit_and_invalid_write_share_one_projection() {
    let (_, _, source) = fixture();
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let mut total = GridControlColumn::new("total", "Total", 150.);
    total.expression = Some(r#"mul(to_decimal(field("amount")),decimal("2.75"))"#.into());
    let settings = GridControlSettings {
        columns: vec![GridControlColumn::new("amount", "Amount", 100.), total],
        is_read_only: false,
    };
    let id = ui
        .grid_control(ui.root(), source.clone(), settings, Default::default())
        .unwrap();
    ui.update(Size::new(600., 400.)).unwrap();
    ui.set_grid_filter(id, "5.5").unwrap();
    assert_eq!(ui.grids[&id].source.keys, [0]);
    assert_eq!(source.borrow().len(), 3);
    ui.set_grid_filter(id, "").unwrap();
    let mut s = grouped();
    s.group_fields = vec!["total".into()];
    s.summaries[0].aggregate.expression = Some(QueryExpression::field("total"));
    ui.set_grid_presentation(id, s).unwrap();
    assert_eq!(ui.grid_summaries(id).unwrap()[0].display(), "33");
    ui.begin_grid_edit(id, 0, "amount").unwrap();
    ui.set_grid_edit_value(id, CellValue::Integer(4)).unwrap();
    assert!(ui.commit_grid_edit(id).unwrap());
    ui.update(Size::new(600., 400.)).unwrap();
    assert_eq!(ui.grid_summaries(id).unwrap()[0].display(), "38.5");
    assert!(ui.begin_grid_edit(id, 0, "total").is_err());
}

#[test]
fn grid_choice_editor_writes_typed_key_instead_of_label_and_rejects_deleted_choice() {
    let (mut ui, id, source) = fixture();
    let choices = table(vec![
        DataRow::new(
            100,
            vec![CellValue::Text("Two".into()), CellValue::Integer(2)],
        ),
        DataRow::new(
            200,
            vec![CellValue::Text("Three".into()), CellValue::Integer(3)],
        ),
    ]);
    for searchable in [false, true] {
        ui.set_grid_cell_editor(
            id,
            "amount",
            Rc::new(GridChoiceEditor {
                source: choices.clone(),
                value_field: "amount".into(),
                display_fields: vec!["category".into()],
                searchable,
            }),
        )
        .unwrap();
        ui.begin_grid_edit(id, 0, "amount").unwrap();
        let editor = ui.grid_editor(id).unwrap();
        assert_eq!(ui.document(editor).unwrap().text(), "Two");
        ui.set_grid_edit_value(id, CellValue::Integer(3)).unwrap();
        assert_eq!(ui.document(editor).unwrap().text(), "Three");
        ui.cancel_grid_edit(id).unwrap();
        assert_eq!(
            source.borrow().value(0, "amount"),
            Some(&CellValue::Integer(2))
        );
    }
    ui.begin_grid_edit(id, 0, "amount").unwrap();
    ui.set_grid_edit_value(id, CellValue::Integer(3)).unwrap();
    choices
        .borrow_mut()
        .set_cell(200, "amount", CellValue::Integer(9))
        .unwrap();
    assert!(!ui.commit_grid_edit(id).unwrap());
    assert_eq!(
        source.borrow().value(0, "amount"),
        Some(&CellValue::Integer(2))
    );
    choices
        .borrow_mut()
        .set_cell(200, "amount", CellValue::Integer(3))
        .unwrap();
    assert!(ui.commit_grid_edit(id).unwrap());
    assert_eq!(
        source.borrow().value(0, "amount"),
        Some(&CellValue::Integer(3))
    );
}

fn point_in(rect: Rect) -> Point {
    Point::new(rect.x + rect.width * 0.5, rect.y + rect.height * 0.5)
}
fn pointer(point: Point, down: bool) -> InputEvent {
    if down {
        InputEvent::PointerDown {
            position: point,
            button: PointerButton::Primary,
            modifiers: Default::default(),
        }
    } else {
        InputEvent::PointerUp {
            position: point,
            button: PointerButton::Primary,
            modifiers: Default::default(),
        }
    }
}
fn press(key: Key, alt: bool) -> InputEvent {
    InputEvent::KeyDown {
        key,
        repeat: false,
        modifiers: rust_desktop_ui_core::Modifiers {
            alt,
            ..Default::default()
        },
    }
}
fn band(title: &str, fields: &[&str]) -> GridBand {
    GridBand {
        title: title.into(),
        fields: fields.iter().map(|f| f.to_string()).collect(),
    }
}
#[test]
fn group_panel_reorders_chips_from_keyboard_and_accepts_header_drop() {
    let (mut ui, id, _) = fixture();
    ui.set_grid_presentation(
        id,
        GridPresentationSettings {
            show_group_panel: true,
            hierarchical_groups: true,
            ..Default::default()
        },
    )
    .unwrap();
    ui.set_grid_selected_key(id, Some(0)).unwrap();
    ui.update(Size::new(600., 400.)).unwrap();
    let header = ui.grids[&id]
        .frame
        .hits
        .iter()
        .find(|hit| hit.kind == rust_desktop_ui_treegrid::GridHitKind::Header(ColumnId(1)))
        .unwrap()
        .bounds;
    let bounds = ui.grids[&id].bounds.unwrap();
    let chip = Point::new(bounds.x + 12., bounds.y + 10.);
    ui.grid_before_input(&pointer(point_in(header), true))
        .unwrap();
    ui.grid_before_input(&pointer(chip, false)).unwrap();
    assert_eq!(ui.grid_presentation(id).unwrap().group_fields, ["amount"]);
    ui.grid_focus_cell(id, 0, Some("category")).unwrap();
    ui.grid_before_input(&press(Key::Character("g".into()), true))
        .unwrap();
    assert_eq!(
        ui.grid_presentation(id).unwrap().group_fields,
        ["amount", "category"]
    );
    ui.grid_before_input(&press(Key::ArrowLeft, true)).unwrap();
    assert_eq!(
        ui.grid_presentation(id).unwrap().group_fields,
        ["category", "amount"]
    );
    ui.grid_before_input(&pointer(chip, true)).unwrap();
    ui.grid_before_input(&pointer(point_in(header), false))
        .unwrap();
    assert_eq!(ui.grid_presentation(id).unwrap().group_fields, ["amount"]);
    let mut s = ui.grid_presentation(id).unwrap().clone();
    s.summaries = grouped().summaries;
    ui.set_grid_presentation(id, s).unwrap();
    for key in ["s", "d", "f"] {
        ui.grid_before_input(&press(Key::Character(key.into()), true))
            .unwrap();
    }
    let s = ui.grid_presentation(id).unwrap();
    assert_eq!(s.group_sort_summary, Some(0));
    assert!(s.group_descending && s.group_footer);
}
#[test]
fn lookup_captions_group_by_key_or_display_and_drop_obsolete_searches() {
    let (mut ui, id, _) = fixture();
    let choices = table(
        [2, 3, 7]
            .map(|n| {
                DataRow::new(
                    n,
                    vec![
                        CellValue::Text(if n == 7 { "Seven" } else { "Same" }.into()),
                        CellValue::Integer(n),
                    ],
                )
            })
            .to_vec(),
    );
    ui.set_grid_cell_editor(
        id,
        "amount",
        Rc::new(GridChoiceEditor {
            source: choices.clone(),
            value_field: "amount".into(),
            display_fields: vec!["category".into(), "amount".into()],
            searchable: true,
        }),
    )
    .unwrap();
    let mut s = grouped();
    s.group_fields = vec!["amount".into()];
    ui.set_grid_presentation(id, s.clone()).unwrap();
    let p = &ui.grids[&id].source.projection;
    assert_eq!(p.groups.len(), 3);
    assert!(p.synthetic.values().any(|r| r.label.contains("Same [2]")));
    s.group_expressions = vec![("amount".into(), "@display".into())];
    ui.set_grid_presentation(id, s).unwrap();
    assert_eq!(ui.grids[&id].source.projection.groups.len(), 2);
    ui.begin_grid_edit(id, 0, "amount").unwrap();
    let host = ui.grids[&id].typed_editor.as_ref().unwrap();
    let (owner, editor) = (host.owner, host.editor);
    ui.set_text(editor, "EVe").unwrap();
    ui.open_editor_popup(owner).unwrap();
    ui.update(Size::new(600., 400.)).unwrap();
    let list = ui.rich.popup.as_ref().unwrap().list.unwrap();
    let matches = ui.selection_items(list).unwrap();
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].text, "Seven");
    assert_eq!(matches[0].columns, ["7"]);
    let first = ui.request_grid_lookup(id).unwrap();
    let second = ui.request_grid_lookup(id).unwrap();
    assert!(!ui.complete_grid_lookup(&first, &[]).unwrap());
    assert!(
        ui.complete_grid_lookup(&second, &[CellValue::Integer(7)])
            .unwrap()
    );
    let stale = ui.request_grid_lookup(id).unwrap();
    ui.cancel_grid_edit(id).unwrap();
    assert!(ui.rich.popup.is_none());
    ui.begin_grid_edit(id, i64::MAX, "amount").unwrap();
    assert!(
        !ui.complete_grid_lookup(&stale, &[CellValue::Integer(2)])
            .unwrap()
    );
    ui.cancel_grid_edit(id).unwrap();
    choices
        .borrow_mut()
        .set_cell(3, "category", CellValue::Text("Three".into()))
        .unwrap();
    ui.update(Size::new(600., 400.)).unwrap();
    assert_eq!(ui.grids[&id].source.projection.groups.len(), 3);
}
#[test]
fn totals_over_unequal_groups_and_custom_aggregate_follow_filter_and_edit() {
    let (mut ui, id, _) = fixture();
    let mut s = grouped();
    s.hierarchical_groups = true;
    s.group_fields.push("amount".into());
    s.group_footer = true;
    s.summaries = [
        AggregateFunction::CountRows,
        AggregateFunction::Sum,
        AggregateFunction::Minimum,
        AggregateFunction::Maximum,
        AggregateFunction::Average,
    ]
    .into_iter()
    .map(|function| GridSummary {
        title: format!("{function:?}"),
        aggregate: Aggregate {
            function,
            expression: (function != AggregateFunction::CountRows)
                .then(|| QueryExpression::field("amount")),
        },
    })
    .collect();
    s.custom_summaries.push(GridCustomSummary::new("Even",|table,keys|Ok(CellValue::Integer(keys.iter().filter(|key|matches!(table.value(**key,"amount"),Some(CellValue::Integer(n)) if n%2==0)).count() as i64))));
    s.summary_columns = vec![(1, "amount".into()), (5, "category".into())];
    ui.set_grid_presentation(id, s).unwrap();
    ui.update(Size::new(600., 400.)).unwrap();
    let scene = &ui.custom_scenes[&id];
    let anchored = scene.texts.iter().find(|t| t.text == "Sum: 12").unwrap();
    let general = scene
        .texts
        .iter()
        .find(|t| t.text.starts_with("CountRows: 3"))
        .unwrap();
    let column = ui
        .grid_frame(id)
        .unwrap()
        .columns
        .iter()
        .find(|c| c.id == ColumnId(1))
        .unwrap();
    assert_eq!(anchored.clip.x, column.clip.x);
    assert_eq!(anchored.clip.width, column.clip.width);
    assert!(general.clip.y >= anchored.clip.y + anchored.clip.height);
    let values = |u: &Controls| {
        u.grid_summaries(id)
            .unwrap()
            .iter()
            .map(CellValue::display)
            .collect::<Vec<_>>()
    };
    assert_eq!(values(&ui), ["3", "12", "2", "7", "4", "1"]);
    let p = &ui.grids[&id].source.projection;
    assert!(
        p.synthetic.values().any(|r| r.label
            == "CountRows: 2 · Sum: 5 · Minimum: 2 · Maximum: 3 · Average: 2.5 · Even: 1")
    );
    ui.set_grid_filter(id, "A").unwrap();
    assert_eq!(values(&ui), ["2", "5", "2", "3", "2.5", "1"]);
    ui.begin_grid_edit(id, 0, "amount").unwrap();
    ui.set_grid_edit_value(id, CellValue::Integer(5)).unwrap();
    ui.commit_grid_edit(id).unwrap();
    ui.update(Size::new(600., 400.)).unwrap();
    assert_eq!(values(&ui), ["2", "8", "3", "5", "4", "0"]);
    ui.set_grid_filter(id, "unknown").unwrap();
    assert_eq!(values(&ui), ["0", "", "", "", "", "0"]);
}
#[test]
fn unbound_column_writes_by_signed_key_after_sort_and_keeps_master_unchanged() {
    let (_, _, master) = fixture();
    let aux = table(
        [i64::MAX, 0, i64::MIN]
            .into_iter()
            .enumerate()
            .map(|(i, key)| {
                DataRow::new(
                    key,
                    vec![
                        CellValue::Text("".into()),
                        CellValue::Integer(100 + i as i64),
                    ],
                )
            })
            .collect(),
    );
    let mut extra = GridControlColumn::new("local", "Local", 120.);
    extra.unbound = Some(GridUnboundColumn {
        source: aux.clone(),
        member: "amount".into(),
    });
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let id = ui
        .grid_control(
            ui.root(),
            master.clone(),
            GridControlSettings {
                columns: vec![GridControlColumn::new("amount", "Amount", 120.), extra],
                is_read_only: false,
            },
            Default::default(),
        )
        .unwrap();
    ui.set_grid_sort(
        id,
        vec![SortDescriptor {
            field: "local".into(),
            direction: rust_desktop_ui_data::SortDirection::Ascending,
        }],
    )
    .unwrap();
    assert_eq!(ui.grids[&id].source.keys, [i64::MAX, 0, i64::MIN]);
    ui.begin_grid_edit(id, i64::MIN, "local").unwrap();
    ui.set_grid_edit_value(id, CellValue::Integer(1)).unwrap();
    assert!(ui.commit_grid_edit(id).unwrap());
    ui.update(Size::new(600., 400.)).unwrap();
    assert_eq!(ui.grids[&id].source.keys, [i64::MIN, i64::MAX, 0]);
    assert_eq!(
        aux.borrow().value(i64::MIN, "amount"),
        Some(&CellValue::Integer(1))
    );
    assert_eq!(
        master.borrow().value(i64::MIN, "amount"),
        Some(&CellValue::Integer(7))
    );
}

#[test]
fn nested_tracks_use_the_same_cells_for_focus_hit_edit_and_column_chooser() {
    let (_, _, source) = fixture();
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let mut tail = GridControlColumn::new("tail", "Tail", 100.);
    tail.expression = Some(r#"field("amount")"#.into());
    let mut wide = GridControlColumn::new("wide", "Wide", 650.);
    wide.expression = Some(r#"field("amount")"#.into());
    let columns = vec![
        GridControlColumn::new("category", "Category", 110.),
        tail,
        wide,
        GridControlColumn::new("amount", "Amount", 110.),
    ];
    let id = ui
        .grid_control(
            ui.root(),
            source,
            GridControlSettings {
                columns,
                is_read_only: false,
            },
            LayoutStyle {
                width: Length::Px(420.),
                height: Length::Px(300.),
                ..Default::default()
            },
        )
        .unwrap();
    let s = GridPresentationSettings {
        bands: vec![band("Sales", &["category", "amount", "wide", "tail"])],
        band_rows: vec![
            vec![
                band("2026", &["category", "amount"]),
                band("Extra", &["wide", "tail"]),
            ],
            vec![band("Plan", &["category", "amount"])],
        ],
        stacked_fields: vec![("amount".into(), "category".into())],
        right_pinned_fields: vec!["tail".into()],
        ..Default::default()
    };
    ui.set_grid_presentation(id, s.clone()).unwrap();
    ui.update(Size::new(600., 400.)).unwrap();
    let frames = ui.grid_frame(id).unwrap();
    let first = frames.cell_bounds(row_key(0), ColumnId(0)).unwrap();
    let second = frames.cell_bounds(row_key(0), ColumnId(3)).unwrap();
    assert_eq!(first.x, second.x);
    assert!(second.y >= first.y + first.height - 0.1);
    assert_eq!(
        frames.hit_test(point_in(second)),
        Some(rust_desktop_ui_treegrid::GridHitKind::Cell {
            row: row_key(0),
            column: ColumnId(3)
        })
    );
    let tail_before = frames.cell_bounds(row_key(0), ColumnId(1)).unwrap();
    assert_eq!(
        ui.grid_accessible_columns(id).unwrap(),
        [ColumnId(0), ColumnId(3), ColumnId(2), ColumnId(1)]
    );
    ui.grid_focus_cell(id, 0, Some("category")).unwrap();
    ui.dispatch(press(Key::ArrowRight, false)).unwrap();
    assert_eq!(ui.grid_focused_column(id), Some(ColumnId(3)));
    ui.begin_grid_edit(id, 0, "amount").unwrap();
    assert_eq!(ui.grid_focused_column(id), Some(ColumnId(3)));
    ui.cancel_grid_edit(id).unwrap();
    ui.grid_scroll_by(id, 270., 0.).unwrap();
    ui.update(Size::new(600., 400.)).unwrap();
    assert_eq!(
        ui.grid_frame(id)
            .unwrap()
            .cell_bounds(row_key(0), ColumnId(1))
            .unwrap()
            .x,
        tail_before.x
    );
    let mut layout = ui.grid_arrangement(id).unwrap();
    layout
        .iter_mut()
        .find(|c| c.field == "amount")
        .unwrap()
        .visible = false;
    ui.arrange_grid_columns(id, layout).unwrap();
    ui.set_grid_column_chooser_open(id, true).unwrap();
    ui.grid_before_input(&press(
        Key::Character("Sales › 2026 › Plan › Amount".into()),
        false,
    ))
    .unwrap();
    ui.grid_before_input(&press(Key::Enter, false)).unwrap();
    ui.grid_before_input(&press(Key::Escape, false)).unwrap();
    assert!(
        ui.grid_arrangement(id)
            .unwrap()
            .iter()
            .find(|c| c.field == "amount")
            .unwrap()
            .visible
    );
    assert_eq!(ui.grid_presentation(id).unwrap().band_rows, s.band_rows);
}
#[test]
fn band_moves_reparent_field_or_branch_and_veto_keeps_geometry() {
    let (mut ui, id, _) = fixture();
    let settings = GridPresentationSettings {
        bands: vec![band("A", &["category"]), band("B", &["amount"])],
        band_rows: vec![vec![band("Leaf", &["category"])]],
        ..Default::default()
    };
    ui.set_grid_presentation(id, settings.clone()).unwrap();
    let leaf = GridBandAddress { level: 1, index: 0 };
    let dest = GridBandAddress { level: 0, index: 1 };
    let change = GridBandChange::Move {
        item: GridBandItem::Band(leaf),
        parent: Some(dest),
        before_field: Some("amount".into()),
    };
    let initial = ui.grid_arrangement(id).unwrap();
    assert!(
        ui.change_grid_band(id, change.clone(), |_, _| false)
            .is_err()
    );
    assert_eq!(ui.grid_presentation(id), Some(&settings));
    assert_eq!(ui.grid_arrangement(id).unwrap(), initial);
    assert!(
        ui.change_grid_band(
            id,
            GridBandChange::Move {
                item: GridBandItem::Band(GridBandAddress { level: 0, index: 0 }),
                parent: Some(leaf),
                before_field: None
            },
            |_, _| true
        )
        .is_err()
    );
    ui.change_grid_band(id, change, |_, _| true).unwrap();
    let updated = ui.grid_presentation(id).unwrap();
    assert_eq!(updated.bands, vec![band("B", &["category", "amount"])]);
    assert_eq!(updated.band_rows[0], vec![band("Leaf", &["category"])]);
    ui.change_grid_band(
        id,
        GridBandChange::Move {
            item: GridBandItem::Field("amount".into()),
            parent: Some(leaf),
            before_field: Some("category".into()),
        },
        |_, _| true,
    )
    .unwrap();
    assert_eq!(
        ui.grid_presentation(id).unwrap().band_rows[0][0].fields,
        ["amount", "category"]
    );
    ui.change_grid_band(
        id,
        GridBandChange::Resize {
            band: leaf,
            factor: 1.25,
        },
        |_, _| true,
    )
    .unwrap();
    assert!(ui.grid_arrangement(id).unwrap().iter().all(|c| {
        initial
            .iter()
            .any(|i| i.field == c.field && c.width > i.width)
    }));
}
#[test]
fn formulas_resolve_forward_refs_reject_bad_updates_and_recover_from_overflow() {
    let (_, _, source) = fixture();
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let mut total = GridControlColumn::new("total", "Total", 120.);
    total.expression = Some(r#"mul(field("later"),integer("2"))"#.into());
    let mut later = GridControlColumn::new("later", "Input", 120.);
    later.expression = Some(r#"field("amount")"#.into());
    let id = ui
        .grid_control(
            ui.root(),
            source.clone(),
            GridControlSettings {
                columns: vec![
                    GridControlColumn::new("amount", "Amount", 100.),
                    total,
                    later,
                ],
                is_read_only: false,
            },
            Default::default(),
        )
        .unwrap();
    ui.set_grid_sort(
        id,
        vec![SortDescriptor {
            field: "total".into(),
            direction: rust_desktop_ui_data::SortDirection::Descending,
        }],
    )
    .unwrap();
    assert_eq!(ui.grids[&id].source.keys, [i64::MIN, i64::MAX, 0]);
    for invalid in [r#"field("missing")"#, r#"field("total")"#, "add("] {
        assert!(
            ui.set_grid_calculated_expression(id, "later", invalid)
                .is_err()
        );
        assert_eq!(
            ui.grids[&id].source.table.value(0, "total"),
            Some(&CellValue::Integer(4))
        );
    }
    ui.begin_grid_edit(id, 0, "amount").unwrap();
    ui.set_grid_edit_value(id, CellValue::Integer(i64::MAX))
        .unwrap();
    assert!(!ui.commit_grid_edit(id).unwrap());
    assert_eq!(
        source.borrow().value(0, "amount"),
        Some(&CellValue::Integer(2))
    );
    ui.set_grid_edit_value(id, CellValue::Integer(10)).unwrap();
    assert!(ui.commit_grid_edit(id).unwrap());
    ui.update(Size::new(600., 400.)).unwrap();
    assert_eq!(ui.grids[&id].source.keys, [0, i64::MIN, i64::MAX]);
    ui.set_grid_calculated_expression(id, "total", r#"year(date("2024-02-29"))"#)
        .unwrap();
    assert_eq!(
        ui.grids[&id].source.table.value(0, "total"),
        Some(&CellValue::Integer(2024))
    );
    ui.set_grid_calculated_expression(
        id,
        "total",
        r#"lower(coalesce(field("category"),text("OTHER")))"#,
    )
    .unwrap();
    assert_eq!(
        ui.grids[&id].source.table.value(0, "total"),
        Some(&CellValue::Text("a".into()))
    );
}
#[test]
fn date_group_rules_keep_calendar_year_and_null_distinct() {
    use rust_desktop_ui_data::DateValue;
    let mut date = FieldSchema::new("date", FieldType::Date);
    date.nullable = true;
    let rows = [
        Some((2024, 2, 1)),
        Some((2024, 2, 29)),
        Some((2025, 2, 1)),
        None,
    ];
    let t = DataTable::new(
        TableSchema::new(vec![date]).unwrap(),
        rows.into_iter()
            .enumerate()
            .map(|(i, date)| {
                DataRow::new(
                    i as i64,
                    vec![date.map_or(CellValue::Null, |(y, m, d)| {
                        CellValue::Date(DateValue::new(y, m, d).unwrap())
                    })],
                )
            })
            .collect(),
    )
    .unwrap();
    let source = Rc::new(RefCell::new(
        DataSource::new(CollectionAdapter::new(t)).unwrap(),
    ));
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let id = ui
        .grid_control(ui.root(), source, Default::default(), Default::default())
        .unwrap();
    for rule in [r#"month_start(field("date"))"#, r#"year(field("date"))"#] {
        ui.set_grid_presentation(
            id,
            GridPresentationSettings {
                group_fields: vec!["date".into()],
                group_expressions: vec![("date".into(), rule.into())],
                hierarchical_groups: true,
                ..Default::default()
            },
        )
        .unwrap();
        let p = &ui.grids[&id].source.projection;
        assert_eq!(p.roots.len(), 3);
        assert!(
            p.roots
                .iter()
                .any(|k| p.children[k] == [row_key(0), row_key(1)])
        );
    }
}

#[test]
fn custom_edit_lifecycle_keeps_invalid_draft_and_invokes_only_the_accepted_outcome() {
    struct Recording(Rc<RefCell<Vec<(i64, CellValue, bool)>>>);
    impl GridCellEditor for Recording {
        fn open(
            &self,
            u: &mut Controls,
            p: WidgetId,
            k: i64,
            f: &FieldSchema,
            v: &CellValue,
        ) -> Result<(WidgetId, WidgetId), ControlError> {
            GridTextCellEditor::Multiline.open(u, p, k, f, v)
        }
        fn value(
            &self,
            u: &Controls,
            o: WidgetId,
            e: WidgetId,
            f: &FieldSchema,
            v: &CellValue,
        ) -> Result<CellValue, String> {
            GridTextCellEditor::Multiline.value(u, o, e, f, v)
        }
        fn assign(
            &self,
            u: &mut Controls,
            o: WidgetId,
            e: WidgetId,
            v: &CellValue,
        ) -> Result<(), ControlError> {
            GridTextCellEditor::Multiline.assign(u, o, e, v)
        }
        fn validate(&self, _: i64, _: &FieldSchema, v: &CellValue) -> Result<(), String> {
            if *v == CellValue::Text("!".into()) {
                Err("Invalid entry".into())
            } else {
                Ok(())
            }
        }
        fn closed(&self, row: i64, _: &FieldSchema, v: &CellValue, committed: bool) {
            self.0.borrow_mut().push((row, v.clone(), committed));
        }
    }
    let (mut ui, id, source) = fixture();
    let events = Rc::new(RefCell::new(vec![]));
    ui.set_grid_cell_template(
        id,
        "category",
        Some(GridCellTemplate {
            prefix: "[".into(),
            suffix: "]".into(),
            ..Default::default()
        }),
    )
    .unwrap();
    ui.set_grid_cell_editor(id, "category", Rc::new(Recording(events.clone())))
        .unwrap();
    ui.begin_grid_edit(id, 0, "category").unwrap();
    assert_eq!(
        ui.document(ui.grid_editor(id).unwrap()).unwrap().text(),
        "A"
    );
    ui.set_grid_edit_value(id, CellValue::Text("!".into()))
        .unwrap();
    assert!(!ui.commit_grid_edit(id).unwrap());
    assert!(events.borrow().is_empty());
    ui.grid_before_input(&press(Key::Escape, false)).unwrap();
    assert_eq!(ui.runtime.focused(), Some(id));
    ui.begin_grid_edit(id, 0, "category").unwrap();
    ui.set_grid_edit_value(id, CellValue::Text("one\ntwo".into()))
        .unwrap();
    ui.grid_before_input(&press(Key::Enter, false)).unwrap();
    assert_eq!(ui.runtime.focused(), Some(id));
    assert_eq!(
        &*events.borrow(),
        &[
            (0, CellValue::Text("A".into()), false),
            (0, CellValue::Text("one\ntwo".into()), true)
        ]
    );
    assert_eq!(
        source.borrow().value(0, "category"),
        Some(&CellValue::Text("one\ntwo".into()))
    );
}
#[test]
fn conditional_masks_buttons_and_multiline_use_the_original_record_and_focus() {
    let (mut ui, id, source) = fixture();
    source
        .borrow_mut()
        .set_cell(0, "category", CellValue::Text("AB".into()))
        .unwrap();
    ui.update(Size::new(600., 400.)).unwrap();
    ui.set_grid_cell_editor(
        id,
        "category",
        Rc::new(GridTextCellEditor::Buttons(vec![EditorButton {
            label: "Clear".into(),
            icon: None,
            action: EditorButtonAction::Clear,
        }])),
    )
    .unwrap();
    ui.set_grid_conditional_cell_editor(
        id,
        "category",
        "amount",
        CellValue::Integer(2),
        Rc::new(GridTextCellEditor::Masked(MaskSettings::new("LL").unwrap())),
    )
    .unwrap();
    ui.begin_grid_edit(id, 0, "category").unwrap();
    let editor = ui.grid_editor(id).unwrap();
    assert!(ui.editor_mask_settings(editor).is_some());
    ui.set_grid_edit_value(id, CellValue::Text("XY".into()))
        .unwrap();
    ui.grid_before_input(&press(Key::Enter, false)).unwrap();
    assert_eq!(
        source.borrow().value(0, "category"),
        Some(&CellValue::Text("XY".into()))
    );
    ui.begin_grid_edit(id, i64::MAX, "category").unwrap();
    let host = ui.grids[&id].typed_editor.as_ref().unwrap();
    assert_ne!(host.owner, host.editor);
    assert!(ui.editor_mask_settings(host.editor).is_none());
    ui.grid_before_input(&press(Key::Escape, false)).unwrap();
    assert_eq!(ui.runtime.focused(), Some(id));
    ui.set_grid_cell_editor(id, "category", Rc::new(GridTextCellEditor::Multiline))
        .unwrap();
    ui.begin_grid_edit(id, i64::MAX, "category").unwrap();
    ui.set_grid_edit_value(id, CellValue::Text("first".into()))
        .unwrap();
    ui.dispatch(press(Key::End, false)).unwrap();
    ui.dispatch(InputEvent::KeyDown {
        key: Key::Enter,
        repeat: false,
        modifiers: rust_desktop_ui_core::Modifiers {
            shift: true,
            ..Default::default()
        },
    })
    .unwrap();
    assert_eq!(
        ui.document(ui.grid_editor(id).unwrap()).unwrap().text(),
        "first\n"
    );
    ui.set_grid_edit_value(id, CellValue::Text("first\nsecond".into()))
        .unwrap();
    ui.grid_before_input(&press(Key::Enter, false)).unwrap();
    assert_eq!(
        source.borrow().value(i64::MAX, "category"),
        Some(&CellValue::Text("first\nsecond".into()))
    );
}
#[test]
fn hundred_thousand_rows_in_two_group_levels_keep_only_the_viewport_mounted() {
    let rows = (0..100_000)
        .map(|k| {
            DataRow::new(
                k,
                vec![
                    CellValue::Text(format!("R{}", k % 10)),
                    CellValue::Integer((k / 10) % 5),
                ],
            )
        })
        .collect();
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let id = ui
        .grid_control(
            ui.root(),
            table(rows),
            Default::default(),
            LayoutStyle {
                width: Length::Px(640.),
                height: Length::Px(360.),
                ..Default::default()
            },
        )
        .unwrap();
    let widgets = ui.runtime.tree().preorder().len();
    let mut s = grouped();
    s.group_fields.push("amount".into());
    s.hierarchical_groups = true;
    ui.set_grid_presentation(id, s).unwrap();
    for offset in [0., 1_000_000., 3_000_000.] {
        ui.grid_scroll_to(id, 0., offset).unwrap();
        ui.update(Size::new(800., 600.)).unwrap();
        let frame = ui.grid_frame(id).unwrap();
        assert_eq!(frame.stats.logical_rows, 100_060);
        assert!(frame.semantics.rows.len() < 22);
        assert_eq!(ui.runtime.tree().preorder().len(), widgets);
    }
    assert_eq!(ui.grid_summaries(id).unwrap(), [CellValue::Integer(200000)]);
}

#[test]
fn band_gesture_uses_rendered_fragment_and_chooser_click_restores_hidden_field() {
    let (mut ui, id, _) = fixture();
    ui.set_grid_presentation(
        id,
        GridPresentationSettings {
            bands: vec![band("Left", &["category"]), band("Right", &["amount"])],
            ..Default::default()
        },
    )
    .unwrap();
    ui.update(Size::new(600., 400.)).unwrap();
    let g = &ui.grids[&id];
    let b = g.bounds.unwrap();
    let at = |col: usize| Point::new(g.frame.columns[col].clip.x + 8., b.y + 10.);
    let from = at(0);
    let to = at(1);
    ui.grid_before_input(&pointer(from, true)).unwrap();
    let mut drop = pointer(to, false);
    if let InputEvent::PointerUp { modifiers, .. } = &mut drop {
        modifiers.alt = true;
    }
    ui.grid_before_input(&drop).unwrap();
    ui.update(Size::new(600., 400.)).unwrap();
    assert_eq!(ui.grid_presentation(id).unwrap().bands.len(), 1);
    assert_eq!(
        ui.grid_presentation(id).unwrap().band_rows[0][0].title,
        "Left"
    );
    let mut layout = ui.grid_arrangement(id).unwrap();
    layout
        .iter_mut()
        .find(|c| c.field == "category")
        .unwrap()
        .visible = false;
    ui.arrange_grid_columns(id, layout).unwrap();
    ui.set_grid_column_chooser_open(id, true).unwrap();
    ui.grid_before_input(&press(
        Key::Character("Right › Left › category".into()),
        false,
    ))
    .unwrap();
    ui.update(Size::new(600., 400.)).unwrap();
    let p = Point::new(b.x + b.width - 25., b.y + 40.);
    ui.grid_before_input(&pointer(p, true)).unwrap();
    ui.grid_before_input(&pointer(p, false)).unwrap();
    assert!(
        ui.grid_arrangement(id)
            .unwrap()
            .iter()
            .find(|c| c.field == "category")
            .unwrap()
            .visible
    );
}
#[test]
fn nullable_calculated_field_and_application_group_key_follow_auxiliary_values() {
    let (_, _, master) = fixture();
    let mut f = FieldSchema::new("value", FieldType::Integer);
    f.nullable = true;
    let raw = DataTable::new(
        TableSchema::new(vec![f]).unwrap(),
        vec![
            DataRow::new(i64::MIN, vec![CellValue::Null]),
            DataRow::new(0, vec![CellValue::Integer(9)]),
            DataRow::new(i64::MAX, vec![CellValue::Integer(9)]),
        ],
    )
    .unwrap();
    let extra = Rc::new(RefCell::new(
        DataSource::new(CollectionAdapter::new(raw)).unwrap(),
    ));
    let mut tier = GridControlColumn::new("tier", "Tier", 130.);
    tier.unbound = Some(GridUnboundColumn {
        source: extra.clone(),
        member: "value".into(),
    });
    let mut derived = GridControlColumn::new("tax", "Tax", 130.);
    derived.expression = Some(r#"mul(to_decimal(field("tier")),decimal("0.125"))"#.into());
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let id = ui
        .grid_control(
            ui.root(),
            master,
            GridControlSettings {
                columns: vec![tier, derived],
                is_read_only: false,
            },
            Default::default(),
        )
        .unwrap();
    ui.set_grid_presentation(
        id,
        GridPresentationSettings {
            group_fields: vec!["tier".into()],
            hierarchical_groups: true,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(
        ui.grids[&id].source.table.value(i64::MIN, "tax"),
        Some(&CellValue::Null)
    );
    assert_eq!(
        ui.grids[&id]
            .source
            .table
            .value(0, "tax")
            .unwrap()
            .display(),
        "1.125"
    );
    assert_eq!(ui.grids[&id].source.projection.roots.len(), 2);
    extra
        .borrow_mut()
        .set_cell(i64::MIN, "value", CellValue::Integer(9))
        .unwrap();
    ui.update(Size::new(600., 400.)).unwrap();
    assert_eq!(ui.grids[&id].source.projection.roots.len(), 1);
    assert_eq!(
        ui.grids[&id]
            .source
            .table
            .value(i64::MIN, "tax")
            .unwrap()
            .display(),
        "1.125"
    );
}

#[test]
fn active_filter_sort_groups_and_footer_move_together_when_a_derived_value_changes() {
    let (_, _, source) = fixture();
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let mut total = GridControlColumn::new("total", "Total", 140.);
    total.expression = Some(r#"mul(to_decimal(field("amount")),decimal("2.75"))"#.into());
    let id = ui
        .grid_control(
            ui.root(),
            source.clone(),
            GridControlSettings {
                columns: vec![GridControlColumn::new("amount", "Amount", 110.), total],
                is_read_only: false,
            },
            Default::default(),
        )
        .unwrap();
    let mut s = grouped();
    s.group_fields = vec!["total".into()];
    s.summaries[0].aggregate.expression = Some(QueryExpression::field("total"));
    s.hierarchical_groups = true;
    ui.set_grid_presentation(id, s).unwrap();
    ui.set_grid_sort(
        id,
        vec![SortDescriptor {
            field: "total".into(),
            direction: rust_desktop_ui_data::SortDirection::Descending,
        }],
    )
    .unwrap();
    ui.set_grid_filter(id, ".25").unwrap();
    assert_eq!(ui.grids[&id].source.keys, [i64::MIN, i64::MAX]);
    assert_eq!(ui.grid_summaries(id).unwrap()[0].display(), "27.5");
    ui.begin_grid_edit(id, i64::MIN, "amount").unwrap();
    ui.set_grid_edit_value(id, CellValue::Integer(4)).unwrap();
    assert!(ui.commit_grid_edit(id).unwrap());
    ui.update(Size::new(600., 400.)).unwrap();
    let grid = &ui.grids[&id];
    assert_eq!(grid.source.keys, [i64::MAX]);
    assert_eq!(grid.source.projection.groups.len(), 1);
    assert_eq!(ui.grid_summaries(id).unwrap()[0].display(), "8.25");
    assert_eq!(
        source.borrow().value(i64::MIN, "amount"),
        Some(&CellValue::Integer(4))
    );
}
