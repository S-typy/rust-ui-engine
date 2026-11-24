use super::*;
use rust_desktop_ui_core::Modifiers;
use rust_desktop_ui_treegrid::GridHitKind;
fn key(ui: &mut Controls, key: Key, mods: Modifiers) {
    ui.dispatch(InputEvent::KeyDown {
        key,
        modifiers: mods,
        repeat: false,
    })
    .unwrap();
}
fn click(ui: &mut Controls, p: Point, mods: Modifiers) {
    ui.dispatch(InputEvent::PointerDown {
        position: p,
        button: PointerButton::Primary,
        modifiers: mods,
    })
    .unwrap();
    ui.dispatch(InputEvent::PointerUp {
        position: p,
        button: PointerButton::Primary,
        modifiers: mods,
    })
    .unwrap();
}
fn center(r: Rect) -> Point {
    Point::new(r.x + r.width / 2., r.y + r.height / 2.)
}
#[test]
fn header_priorities_filters_facets_and_find_are_live() {
    let (mut ui, id, _) = fixture();
    ui.set_grid_workflow_rows(id, true, true).unwrap();
    let size = Size::new(640., 400.);
    ui.update(size).unwrap();
    for (i, shift) in [(0, false), (1, true)] {
        let h = ui
            .grid_frame(id)
            .unwrap()
            .hits
            .iter()
            .find(|h| h.kind == GridHitKind::Header(ColumnId(i)))
            .unwrap()
            .bounds;
        click(
            &mut ui,
            center(h),
            Modifiers {
                shift,
                ..Default::default()
            },
        );
        ui.update(size).unwrap();
    }
    assert_eq!(ui.grids[&id].model.query().sort.len(), 2);
    assert!(
        ui.grid_frame(id)
            .unwrap()
            .scene
            .texts
            .iter()
            .any(|t| t.text.contains("2↑"))
    );
    ui.set_grid_sort_priority(id, "category", vec![CellValue::Text("B".into())])
        .unwrap();
    assert_eq!(ui.grids[&id].source.keys, vec![i64::MIN, 0, i64::MAX]);
    ui.set_grid_auto_filter(id, "amount", "2..3").unwrap();
    assert_eq!(
        ui.grid_facet_values(id, "category").unwrap(),
        vec![CellValue::Text("A".into())]
    );
    ui.set_grid_facet(id, "category", Some(vec![CellValue::Text("B".into())]))
        .unwrap();
    assert!(ui.grids[&id].source.keys.is_empty());
    assert!(ui.set_grid_auto_filter(id, "amount", "7..2").is_err());
    ui.set_grid_auto_filter(id, "amount", "").unwrap();
    assert_eq!(ui.grids[&id].source.keys, [i64::MIN]);
    ui.set_grid_facet(id, "category", None).unwrap();
    ui.set_grid_criteria(id, "not(false())").unwrap();
    assert!(ui.set_grid_criteria(id, "missing(").is_err());
    ui.set_grid_criteria_enabled(id, false).unwrap();
    assert_eq!(ui.grid_criteria_enabled(id), Some(false));
    ui.set_grid_search(id, "3").unwrap();
    assert!(ui.find_grid_match(id, true, false).unwrap());
    assert_eq!(ui.grid_selected_key(id), Some(i64::MAX));
    ui.begin_grid_edit(id, i64::MAX, "amount").unwrap();
    assert!(!ui.find_grid_match(id, false, false).unwrap());
}
#[test]
fn row_drafts_validate_conflict_cancel_and_commit_exactly_once() {
    let (mut ui, id, source) = fixture();
    ui.set_grid_row_validator(id, |_, r| {
        if r.values[0] == CellValue::Text("limit".into()) && r.values[1] != CellValue::Integer(1) {
            Err(GridRowIssue {
                field: "amount".into(),
                message: "must be one".into(),
            })
        } else {
            Ok(())
        }
    })
    .unwrap();
    let old = source.borrow().revision();
    ui.begin_grid_row_edit(id, 0).unwrap();
    ui.set_grid_draft_cell(id, "category", CellValue::Text("limit".into()))
        .unwrap();
    assert!(ui.commit_grid_record(id).is_err());
    assert_eq!(ui.grid_row_issue(id).unwrap().field, "amount");
    assert_eq!(source.borrow().revision(), old);
    ui.set_grid_draft_cell(id, "amount", CellValue::Integer(1))
        .unwrap();
    assert!(ui.commit_grid_record(id).unwrap());
    assert_eq!(source.borrow().revision(), old + 1);
    assert_eq!(
        ui.take_grid_record_events(id),
        [GridRecordEvent::Begin(0), GridRecordEvent::Saved(0)]
    );
    ui.begin_grid_row_edit(id, 0).unwrap();
    source
        .borrow_mut()
        .set_cell(0, "category", CellValue::Text("external".into()))
        .unwrap();
    assert!(ui.commit_grid_record(id).is_err());
    ui.cancel_grid_record(id).unwrap();
    let row = DataRow::new(
        17,
        vec![CellValue::Text("new".into()), CellValue::Integer(2)],
    );
    ui.begin_grid_insert(id, row.clone()).unwrap();
    ui.cancel_grid_record(id).unwrap();
    assert!(source.borrow().table().row(17).is_none());
    ui.begin_grid_insert(id, row).unwrap();
    ui.commit_grid_record(id).unwrap();
    ui.delete_grid_record(id, 17).unwrap();
    assert!(source.borrow().table().row(17).is_none());
    ui.set_grid_read_only(id, true).unwrap();
    assert!(ui.begin_grid_row_edit(id, 0).is_err());
}
#[test]
fn ranges_and_transfer_preserve_keys_atomic_types_and_formats() {
    let (mut ui, id, source) = fixture();
    ui.select_grid_range(id, (i64::MIN, "category"), (0, "amount"), false)
        .unwrap();
    ui.select_grid_range(id, (i64::MAX, "category"), (i64::MAX, "category"), true)
        .unwrap();
    let t = ui.copy_grid_selection(id, true).unwrap();
    assert!(t.tsv.contains("category\tamount\r\n"));
    assert!(t.html.contains("<th>category</th>"));
    assert!(t.html.contains("<td></td>"));
    let old = source.borrow().revision();
    assert!(
        ui.paste_grid_tsv(
            id,
            (i64::MIN, "category"),
            "updated\t8\r\nbad\tnumber",
            false
        )
        .is_err()
    );
    assert_eq!(source.borrow().revision(), old);
    ui.paste_grid_tsv(id, (i64::MIN, "category"), "\"A\t<&>\"\t8", false)
        .unwrap();
    let t = ui.copy_grid_selection(id, false).unwrap();
    assert!(t.tsv.contains("\"A\t<&>\""));
    assert!(t.html.contains("&lt;&amp;&gt;"));
    let dest = table(vec![]);
    let to = ui
        .grid_control(
            ui.root(),
            dest.clone(),
            Default::default(),
            Default::default(),
        )
        .unwrap();
    ui.copy_grid_rows_to(id, to, &[0, i64::MIN]).unwrap();
    assert_eq!(dest.borrow().table().rows().len(), 2);
    assert!(ui.copy_grid_rows_to(id, to, &[i64::MAX, 0]).is_err());
    assert!(dest.borrow().table().row(i64::MAX).is_none());
    ui.move_grid_rows(id, &[i64::MIN], Some(0), "amount")
        .unwrap();
    assert!(
        matches!(source.borrow().table().value(i64::MIN,"amount"),Some(CellValue::Integer(n)) if *n<2)
    );
}
fn source(fields: Vec<FieldSchema>, rows: Vec<DataRow>) -> DataSourceHandle {
    Rc::new(RefCell::new(
        DataSource::new(CollectionAdapter::new(
            DataTable::new(TableSchema::new(fields).unwrap(), rows).unwrap(),
        ))
        .unwrap(),
    ))
}
#[test]
fn tree_loads_once_and_relation_tabs_preserve_independent_edits() {
    let (mut ui, id, _) = fixture();
    let mut parent = FieldSchema::new("parent", FieldType::Integer);
    parent.nullable = true;
    let hierarchy = source(
        vec![parent, FieldSchema::new("label", FieldType::Text)],
        vec![DataRow::new(
            1,
            vec![CellValue::Null, CellValue::Text("root".into())],
        )],
    );
    ui.replace_grid_source(id, hierarchy.clone()).unwrap();
    ui.set_grid_parent_field(id, Some("parent")).unwrap();
    ui.set_grid_child_loader(id, |key| {
        Ok(vec![DataRow::new(
            2,
            vec![CellValue::Integer(key), CellValue::Text("child".into())],
        )])
    })
    .unwrap();
    ui.load_grid_branch(id, 1).unwrap();
    ui.load_grid_branch(id, 1).unwrap();
    assert_eq!(ui.grids[&id].model.row_count(), 2);
    ui.set_grid_auto_filter(id, "label", "child").unwrap();
    assert_eq!(ui.grids[&id].model.row_count(), 2);
    ui.begin_grid_row_edit(id, 2).unwrap();
    ui.set_grid_draft_cell(id, "parent", CellValue::Integer(2))
        .unwrap();
    assert!(ui.commit_grid_record(id).is_err());
    ui.cancel_grid_record(id).unwrap();
    let (mut ui, master, _) = fixture();
    let lines = table(vec![
        DataRow::new(
            80,
            vec![CellValue::Text("A".into()), CellValue::Integer(12)],
        ),
        DataRow::new(
            81,
            vec![CellValue::Text("B".into()), CellValue::Integer(19)],
        ),
    ]);
    for name in ["Items", "Taxes"] {
        ui.register_grid_relation(
            master,
            GridRelation {
                name: name.into(),
                master_field: "category".into(),
                foreign_field: "category".into(),
                source: lines.clone(),
                settings: Default::default(),
            },
        )
        .unwrap();
    }
    let host = ui.panel(ui.root(), Default::default()).unwrap();
    let child = ui
        .open_grid_relation(master, 0, "Items", host, Default::default())
        .unwrap();
    assert_eq!(ui.grids[&child].source.keys, [80]);
    ui.set_grid_selected_key(child, Some(80)).unwrap();
    ui.begin_grid_row_edit(child, 80).unwrap();
    assert!(
        ui.set_grid_draft_cell(child, "category", CellValue::Text("B".into()))
            .is_err()
    );
    ui.set_grid_draft_cell(child, "amount", CellValue::Integer(15))
        .unwrap();
    let tax = ui
        .open_grid_relation(master, 0, "Taxes", host, Default::default())
        .unwrap();
    ui.set_grid_auto_filter(tax, "amount", ">100").unwrap();
    assert!(ui.grids[&tax].source.keys.is_empty());
    assert_eq!(
        child,
        ui.open_grid_relation(master, 0, "Items", host, Default::default())
            .unwrap()
    );
    ui.commit_grid_record(child).unwrap();
    assert_eq!(
        lines.borrow().table().value(80, "amount"),
        Some(&CellValue::Integer(15))
    );
    ui.register_grid_relation(
        child,
        GridRelation {
            name: "Charges".into(),
            master_field: "category".into(),
            foreign_field: "category".into(),
            source: lines,
            settings: Default::default(),
        },
    )
    .unwrap();
    let nested_host = ui.panel(ui.root(), Default::default()).unwrap();
    let nested = ui
        .open_grid_relation(child, 80, "Charges", nested_host, Default::default())
        .unwrap();
    assert_eq!(ui.grids[&nested].source.keys, [80]);
}
#[test]
fn wrapped_card_view_keeps_key_and_clips_hit_geometry() {
    let (mut ui, id, _) = fixture();
    ui.set_grid_selected_key(id, Some(0)).unwrap();
    ui.edit_props(id, |p| p.style.width = Length::Px(960.))
        .unwrap();
    ui.set_grid_presentation(
        id,
        GridPresentationSettings {
            view: GridViewMode::Cards,
            ..Default::default()
        },
    )
    .unwrap();
    ui.update(Size::new(1020., 420.)).unwrap();
    let r = &ui.grid_frame(id).unwrap().semantics.rows;
    assert_eq!(r[0].bounds.y, r[1].bounds.y);
    assert!(r[1].bounds.x > r[0].bounds.x + r[0].bounds.width);
    let p = center(r[1].cells[0].bounds);
    click(&mut ui, p, Default::default());
    assert_eq!(ui.grid_selected_key(id), Some(0));
    ui.set_grid_presentation(id, Default::default()).unwrap();
    assert_eq!(ui.grid_selected_key(id), Some(0));
}
#[test]
fn filter_row_and_selector_accept_pointer_and_committed_text() {
    let (mut ui, id, _) = fixture();
    ui.set_grid_workflow_rows(id, true, true).unwrap();
    ui.update(Size::new(560., 330.)).unwrap();
    let g = &ui.grids[&id];
    let r = g.frame.columns[1].clip;
    let y = g.presentation_bounds(g.bounds.unwrap()).y - 8.;
    click(&mut ui, Point::new(r.x + 10., y), Default::default());
    ui.text_input(">=3").unwrap();
    key(&mut ui, Key::Enter, Default::default());
    ui.update(Size::new(560., 330.)).unwrap();
    assert_eq!(ui.grids[&id].source.keys.len(), 2);
    key(
        &mut ui,
        Key::Character("a".into()),
        Modifiers {
            control: true,
            ..Default::default()
        },
    );
    assert_eq!(ui.grids[&id].model.selected().len(), 2);
}

#[test]
fn typed_dates_empty_values_and_facet_popup_keep_distinct_identities() {
    use rust_desktop_ui_data::DateValue;
    let mut field = FieldSchema::new("tag", FieldType::Text);
    field.nullable = true;
    let date = |day| CellValue::Date(DateValue::new(2026, 11, day).unwrap());
    let src = source(
        vec![field, FieldSchema::new("day", FieldType::Date)],
        vec![
            DataRow::new(-1, vec![CellValue::Null, date(2)]),
            DataRow::new(0, vec![CellValue::Text(String::new()), date(3)]),
            DataRow::new(1, vec![CellValue::Text("X".into()), date(4)]),
        ],
    );
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let grid = ui
        .grid_control(ui.root(), src, Default::default(), Default::default())
        .unwrap();
    ui.set_grid_auto_filter(grid, "day", "02.11.2026..03.11.2026")
        .unwrap();
    assert_eq!(
        ui.grid_facet_values(grid, "tag").unwrap(),
        [CellValue::Null, CellValue::Text(String::new())]
    );
    ui.update(Size::new(500., 350.)).unwrap();
    let header = ui
        .grid_frame(grid)
        .unwrap()
        .hits
        .iter()
        .find(|h| h.kind == GridHitKind::Header(ColumnId(0)))
        .unwrap()
        .bounds;
    ui.dispatch(InputEvent::PointerDown {
        position: center(header),
        button: PointerButton::Secondary,
        modifiers: Default::default(),
    })
    .unwrap();
    key(&mut ui, Key::Space, Default::default());
    key(&mut ui, Key::Enter, Default::default());
    assert_eq!(ui.grids[&grid].source.keys, [0]);
    ui.set_grid_facet(grid, "tag", None).unwrap();
    ui.set_grid_auto_filter(grid, "tag", "NULL").unwrap();
    assert_eq!(ui.grids[&grid].source.keys, [-1]);
}
#[test]
fn range_gestures_and_native_drop_route_through_visible_cells() {
    let (mut ui, id, source) = fixture();
    let get = |ui: &Controls, key, field| {
        let g = &ui.grids[&id];
        center(
            g.frame
                .semantics
                .rows
                .iter()
                .find(|r| r.key == row_key(key))
                .unwrap()
                .cells[field as usize]
                .bounds,
        )
    };
    let a = get(&ui, i64::MIN, 0);
    let b = get(&ui, 0, 1);
    ui.dispatch(InputEvent::PointerDown {
        position: a,
        button: PointerButton::Primary,
        modifiers: Default::default(),
    })
    .unwrap();
    ui.dispatch(InputEvent::PointerMoved { position: b })
        .unwrap();
    ui.dispatch(InputEvent::PointerUp {
        position: b,
        button: PointerButton::Primary,
        modifiers: Default::default(),
    })
    .unwrap();
    assert_eq!(ui.grid_cell_ranges(id).unwrap()[0].rows, [i64::MIN, 0]);
    assert_eq!(
        ui.grid_cell_ranges(id).unwrap()[0].fields,
        ["category", "amount"]
    );
    assert!(ui.grid_text_drop(b, None));
    assert!(ui.grid_text_drop(b, Some("11")));
    assert_eq!(
        source.borrow().table().value(0, "amount"),
        Some(&CellValue::Integer(11))
    );
    assert!(!ui.grid_text_drop(b, Some("invalid")));
    ui.set_grid_read_only(id, true).unwrap();
    assert!(!ui.grid_text_drop(b, None));
}
#[test]
fn accepted_selection_survives_hide_restore_and_source_schema_change() {
    let (mut ui, id, data) = fixture();
    ui.set_grid_workflow_rows(id, false, true).unwrap();
    ui.update(Size::new(600., 400.)).unwrap();
    ui.runtime.apply(Mutation::Focus(Some(id))).unwrap();
    key(
        &mut ui,
        Key::Character("a".into()),
        Modifiers {
            control: true,
            ..Default::default()
        },
    );
    assert_eq!(ui.grids[&id].model.selected().len(), 3);
    ui.set_grid_auto_filter(id, "category", "B").unwrap();
    ui.set_grid_auto_filter(id, "category", "").unwrap();
    assert_eq!(ui.grids[&id].model.selected().len(), 3);
    let schema = TableSchema::new(vec![FieldSchema::new("new_name", FieldType::Text)]).unwrap();
    let other = CollectionAdapter::new(
        DataTable::new(
            schema,
            vec![DataRow::new(0, vec![CellValue::Text("updated".into())])],
        )
        .unwrap(),
    );
    data.borrow_mut()
        .replace_adapter(other, Default::default())
        .unwrap();
    ui.update(Size::new(600., 400.)).unwrap();
    assert_eq!(ui.grid_settings(id).unwrap().columns[0].field, "new_name");
}

#[test]
fn native_move_checks_originals_and_cancel_keeps_records() {
    let (mut ui, id, data) = fixture();
    ui.allow_grid_external_move(id, true).unwrap();
    let begin = |ui: &mut Controls| {
        ui.update(Size::new(600., 400.)).unwrap();
        let p = center(
            ui.grid_frame(id)
                .unwrap()
                .semantics
                .rows
                .iter()
                .find(|r| r.key == row_key(0))
                .unwrap()
                .cells[0]
                .bounds,
        );
        ui.dispatch(InputEvent::PointerDown {
            position: p,
            button: PointerButton::Primary,
            modifiers: Modifiers {
                alt: true,
                control: true,
                ..Default::default()
            },
        })
        .unwrap();
        ui.dispatch(InputEvent::PointerMoved {
            position: Point::new(p.x + 15., p.y),
        })
        .unwrap();
        let (origin, text, allow) = ui.take_grid_external_drag().unwrap();
        assert_eq!(origin, id);
        assert!(allow);
        assert!(text.contains('\t'));
    };
    begin(&mut ui);
    ui.finish_grid_external_drag(id, false).unwrap();
    assert!(data.borrow().table().row(0).is_some());
    begin(&mut ui);
    data.borrow_mut()
        .set_cell(0, "category", CellValue::Text("new".into()))
        .unwrap();
    assert!(ui.finish_grid_external_drag(id, true).is_err());
    assert!(data.borrow().table().row(0).is_some());
    begin(&mut ui);
    ui.finish_grid_external_drag(id, true).unwrap();
    assert!(data.borrow().table().row(0).is_none());
}
#[test]
fn keyboard_ranges_skip_hidden_fields_and_keep_their_keys_after_scroll() {
    let (mut ui, id, _) = fixture();
    ui.grid_focus_cell(id, i64::MIN, Some("category")).unwrap();
    ui.runtime.apply(Mutation::Focus(Some(id))).unwrap();
    key(&mut ui, Key::ArrowDown, Default::default());
    key(
        &mut ui,
        Key::ArrowRight,
        Modifiers {
            shift: true,
            ..Default::default()
        },
    );
    assert_eq!(
        ui.grid_cell_ranges(id).unwrap()[0].fields,
        ["category", "amount"]
    );
    let mut layout = ui.grid_arrangement(id).unwrap();
    layout[1].visible = false;
    ui.arrange_grid_columns(id, layout).unwrap();
    ui.grid_scroll_by(id, 100., 180.).unwrap();
    let transfer = ui.copy_grid_selection(id, true).unwrap();
    assert!(!transfer.html.contains("<th>amount</th>"));
}
#[test]
fn deleted_master_prevents_saved_detail_from_writing_into_another_order() {
    let (mut ui, id, master) = fixture();
    let detail = table(vec![DataRow::new(
        1,
        vec![CellValue::Text("A".into()), CellValue::Integer(1)],
    )]);
    ui.register_grid_relation(
        id,
        GridRelation {
            name: "lines".into(),
            master_field: "category".into(),
            foreign_field: "category".into(),
            settings: Default::default(),
            source: detail.clone(),
        },
    )
    .unwrap();
    let host = ui.panel(ui.root(), Default::default()).unwrap();
    let child = ui
        .open_grid_relation(id, 0, "lines", host, Default::default())
        .unwrap();
    ui.begin_grid_row_edit(child, 1).unwrap();
    ui.set_grid_draft_cell(child, "amount", CellValue::Integer(11))
        .unwrap();
    let expected = master.borrow().table().row(0).unwrap().clone();
    master
        .borrow_mut()
        .write_batch(&[rust_desktop_ui_data::DataWrite::Delete { expected }])
        .unwrap();
    assert!(ui.commit_grid_record(child).is_err());
    assert_eq!(
        detail.borrow().table().value(1, "amount"),
        Some(&CellValue::Integer(1))
    );
}
