use super::*;
use crate::{AsyncEditorValidator, EditorValidationStatus};
use rust_desktop_ui_data::{
    Aggregate, AggregateFunction, CollectionAdapter, DataRow, FieldSchema, FieldType,
    QueryExpression, TableSchema,
};
fn table(rows: Vec<DataRow>) -> DataSourceHandle {
    Rc::new(RefCell::new(
        DataSource::new(CollectionAdapter::new(
            DataTable::new(
                TableSchema::new(vec![
                    FieldSchema::new("category", FieldType::Text),
                    FieldSchema::new("amount", FieldType::Integer),
                ])
                .unwrap(),
                rows,
            )
            .unwrap(),
        ))
        .unwrap(),
    ))
}
fn fixture() -> (Controls, WidgetId, DataSourceHandle) {
    let source = table(vec![
        DataRow::new(
            i64::MIN,
            vec![CellValue::Text("B".into()), CellValue::Integer(7)],
        ),
        DataRow::new(0, vec![CellValue::Text("A".into()), CellValue::Integer(2)]),
        DataRow::new(
            i64::MAX,
            vec![CellValue::Text("A".into()), CellValue::Integer(3)],
        ),
    ]);
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let id = ui
        .grid_control(
            ui.root(),
            source.clone(),
            GridControlSettings::default(),
            LayoutStyle {
                width: Length::Px(500.0),
                height: Length::Px(320.0),
                ..Default::default()
            },
        )
        .unwrap();
    ui.update(Size::new(600.0, 400.0)).unwrap();
    (ui, id, source)
}
fn grouped() -> GridPresentationSettings {
    GridPresentationSettings {
        group_fields: vec!["category".into()],
        summaries: vec![GridSummary {
            title: "Total".into(),
            aggregate: Aggregate {
                function: AggregateFunction::Sum,
                expression: Some(QueryExpression::field("amount")),
            },
        }],
        ..Default::default()
    }
}
#[test]
fn grouped_totals_use_exact_query_and_preserve_full_signed_record_keys() {
    let (mut ui, id, source) = fixture();
    ui.set_grid_selected_key(id, Some(i64::MAX)).unwrap();
    ui.set_grid_presentation(id, grouped()).unwrap();
    ui.update(Size::new(600.0, 400.0)).unwrap();
    assert_eq!(
        ui.grid_summaries(id),
        Some([CellValue::Integer(12)].as_slice())
    );
    assert_eq!(ui.grid_selected_key(id), Some(i64::MAX));
    let frame = ui.grid_frame(id).unwrap();
    assert_eq!(frame.stats.logical_rows, 5);
    for key in [i64::MIN, 0, i64::MAX] {
        assert_eq!(
            ui.grid_row_kind(id, row_key(key)),
            Some(GridRowKind::Record(key))
        );
    }
    let groups: Vec<_> = frame
        .rows
        .iter()
        .filter_map(|r| r.key)
        .filter(|k| matches!(ui.grid_row_kind(id, *k), Some(GridRowKind::Group { .. })))
        .collect();
    assert_eq!(groups.len(), 2);
    assert!(
        groups
            .iter()
            .all(|k| source.borrow().table().row(data_key(*k)).is_none())
    );
    ui.set_grid_expanded(id, groups[0], false).unwrap();
    ui.update(Size::new(600.0, 400.0)).unwrap();
    assert_eq!(ui.grid_selected_key(id), Some(i64::MAX));
    source
        .borrow_mut()
        .set_cell(0, "amount", CellValue::Integer(4))
        .unwrap();
    ui.update(Size::new(600.0, 400.0)).unwrap();
    assert_eq!(
        ui.grid_summaries(id),
        Some([CellValue::Integer(14)].as_slice())
    );
    assert!(!ui.grids[&id].model.is_expanded(groups[0]));
}
#[test]
fn bad_aggregate_and_detail_relation_leave_presentation_and_selection_unchanged() {
    let (mut ui, id, _) = fixture();
    ui.set_grid_selected_key(id, Some(0)).unwrap();
    let mut bad = grouped();
    bad.summaries[0].aggregate.expression = Some(QueryExpression::field("category"));
    assert!(ui.set_grid_presentation(id, bad).is_err());
    assert_eq!(
        ui.grid_presentation(id),
        Some(&GridPresentationSettings::default())
    );
    let detail = table(vec![DataRow::new(
        1,
        vec![CellValue::Text("A".into()), CellValue::Integer(2)],
    )]);
    assert!(
        ui.set_grid_detail(
            id,
            Some(GridDetailSettings {
                source: detail.clone(),
                master_field: "category".into(),
                foreign_field: "amount".into(),
                columns: vec![GridControlColumn::new("category", "Child", 120.0)]
            })
        )
        .is_err()
    );
    assert_eq!(ui.grid_selected_key(id), Some(0));
    assert!(ui.grids[&id].source.detail.is_none());
    ui.set_grid_detail(
        id,
        Some(GridDetailSettings {
            source: detail,
            master_field: "category".into(),
            foreign_field: "category".into(),
            columns: vec![
                GridControlColumn::new("category", "Child", 120.0),
                GridControlColumn::new("amount", "Value", 100.0),
            ],
        }),
    )
    .unwrap();
    ui.set_grid_expanded(id, row_key(0), true).unwrap();
    ui.update(Size::new(600.0, 400.0)).unwrap();
    let frame = ui.grid_frame(id).unwrap();
    assert_eq!(frame.stats.logical_rows, 5);
    assert!(
        frame
            .rows
            .iter()
            .filter_map(|r| r.key)
            .any(|k| ui.grid_row_kind(id, k) == Some(GridRowKind::Detail { master: 0, key: 1 }))
    );
    assert!(
        frame
            .semantics
            .rows
            .iter()
            .flat_map(|r| &r.cells)
            .any(|c| c.label == "Child" && !c.editable)
    );
}
#[test]
fn cards_and_column_customization_are_virtual_and_preserve_edit_draft() {
    let (mut ui, id, _) = fixture();
    let mut arrangement = ui.grid_arrangement(id).unwrap();
    arrangement.swap(0, 1);
    arrangement[1].visible = false;
    ui.arrange_grid_columns(id, arrangement.clone()).unwrap();
    ui.set_grid_presentation(
        id,
        GridPresentationSettings {
            view: GridViewMode::Cards,
            ..Default::default()
        },
    )
    .unwrap();
    ui.update(Size::new(600.0, 400.0)).unwrap();
    assert_eq!(ui.grid_frame(id).unwrap().semantics.column_count, 1);
    assert!(
        ui.grid_frame(id)
            .unwrap()
            .scene
            .texts
            .iter()
            .any(|t| t.text == "amount: 2")
    );
    ui.begin_grid_edit(id, 0, "amount").unwrap();
    let editor = ui.grid_editor(id).unwrap();
    ui.set_text(editor, "44").unwrap();
    assert!(ui.set_grid_presentation(id, Default::default()).is_err());
    assert!(ui.arrange_grid_columns(id, arrangement).is_err());
    assert_eq!(ui.document(editor).unwrap().text(), "44");
    ui.cancel_grid_edit(id).unwrap();
}

#[test]
fn typed_numeric_editor_keeps_exact_integer_conflict_draft_and_uses_one_owned_host() {
    let (mut ui, id, source) = fixture();
    source
        .borrow_mut()
        .set_cell(0, "amount", CellValue::Integer(9_007_199_254_740_993))
        .unwrap();
    ui.set_grid_editor_factory(id, "amount", "Auto", &DataFormFactories::default())
        .unwrap();
    ui.begin_grid_edit(id, 0, "amount").unwrap();
    let editor = ui.grid_editor(id).unwrap();
    assert_eq!(ui.document(editor).unwrap().text(), "9007199254740993");
    ui.set_text(editor, "9007199254740995").unwrap();
    source
        .borrow_mut()
        .set_cell(0, "amount", CellValue::Integer(6))
        .unwrap();
    assert!(!ui.commit_grid_edit(id).unwrap());
    assert_eq!(ui.document(editor).unwrap().text(), "9007199254740995");
    assert_eq!(ui.accepted_value(editor), Some("9007199254740993"));
    ui.cancel_grid_edit(id).unwrap();
    ui.update(Size::new(600.0, 400.0)).unwrap();
    let count = ui.runtime.tree().preorder().len();
    for key in [0, i64::MIN, i64::MAX, 0] {
        ui.begin_grid_edit(id, key, "amount").unwrap();
        let current = ui.grid_editor(id).unwrap();
        ui.set_text(current, "17").unwrap();
        assert!(ui.commit_grid_edit(id).unwrap());
        assert_eq!(
            source.borrow().value(key, "amount"),
            Some(&CellValue::Integer(17))
        );
        assert!(ui.runtime.tree().preorder().len() <= count);
    }
    let typed = ui.grids[&id].typed_editor.as_ref().unwrap();
    let (host, owner, editor) = (typed.host, typed.owner, typed.editor);
    ui.remove(id).unwrap();
    assert!(ui.runtime.tree().node(host).is_none());
    assert!(ui.runtime.tree().node(owner).is_none());
    assert!(ui.runtime.tree().node(editor).is_none());
    // Controls keeps its shared overlay root independently of editor ownership.
    assert_eq!(ui.runtime.tree().preorder().len(), 2);
    assert!(ui.runtime.tree().preorder().contains(&ui.root()));
    assert!(ui.runtime.tree().preorder().contains(&ui.overlay));
}
#[test]
fn typed_checkbox_nullable_commit_cancel_and_readonly_do_not_touch_source_early() {
    let mut field = FieldSchema::new("active", FieldType::Bool);
    field.nullable = true;
    let source = Rc::new(RefCell::new(
        DataSource::new(CollectionAdapter::new(
            DataTable::new(
                TableSchema::new(vec![field]).unwrap(),
                vec![DataRow::new(1, vec![CellValue::Null])],
            )
            .unwrap(),
        ))
        .unwrap(),
    ));
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let id = ui
        .grid_control(
            ui.root(),
            source.clone(),
            Default::default(),
            LayoutStyle::default(),
        )
        .unwrap();
    ui.set_grid_editor_factory(id, "active", "Auto", &DataFormFactories::default())
        .unwrap();
    ui.update(Size::new(640.0, 400.0)).unwrap();
    ui.begin_grid_edit(id, 1, "active").unwrap();
    let editor = ui.grid_editor(id).unwrap();
    assert_eq!(ui.check_state(editor), Some(CheckState::Indeterminate));
    ui.set_check_state(editor, CheckState::Checked).unwrap();
    assert_eq!(source.borrow().value(1, "active"), Some(&CellValue::Null));
    ui.set_grid_read_only(id, true).unwrap();
    assert!(!ui.commit_grid_edit(id).unwrap());
    assert_eq!(ui.check_state(editor), Some(CheckState::Checked));
    ui.set_grid_read_only(id, false).unwrap();
    assert!(ui.commit_grid_edit(id).unwrap());
    assert_eq!(
        source.borrow().value(1, "active"),
        Some(&CellValue::Bool(true))
    );
    ui.begin_grid_edit(id, 1, "active").unwrap();
    let editor = ui.grid_editor(id).unwrap();
    ui.set_check_state(editor, CheckState::Unchecked).unwrap();
    ui.cancel_grid_edit(id).unwrap();
    assert_eq!(
        source.borrow().value(1, "active"),
        Some(&CellValue::Bool(true))
    );
}
#[test]
fn synchronous_grid_validator_rejection_preserves_source_and_editor_history() {
    let (mut ui, id, source) = fixture();
    ui.begin_grid_edit(id, 0, "amount").unwrap();
    let editor = ui.grid_editor(id).unwrap();
    ui.set_editor_validator(
        editor,
        Some(EditorValidator::new(|value| {
            if value == "42" {
                Err("denied".into())
            } else {
                Ok(())
            }
        })),
    )
    .unwrap();
    ui.set_text(editor, "42").unwrap();
    assert!(!ui.commit_grid_edit(id).unwrap());
    assert_eq!(ui.document(editor).unwrap().text(), "42");
    assert_eq!(ui.accepted_value(editor), Some("2"));
    assert_eq!(
        source.borrow().value(0, "amount"),
        Some(&CellValue::Integer(2))
    );
}
#[test]
fn borrowed_source_rejects_presentation_and_update_without_panicking_or_losing_state() {
    let (mut ui, id, source) = fixture();
    let borrow = source.borrow_mut();
    assert!(ui.set_grid_presentation(id, grouped()).is_err());
    assert!(ui.update(Size::new(600.0, 400.0)).is_err());
    assert_eq!(
        ui.grid_presentation(id),
        Some(&GridPresentationSettings::default())
    );
    drop(borrow);
    ui.update(Size::new(600.0, 400.0)).unwrap();
    assert_eq!(ui.grid_frame(id).unwrap().stats.logical_rows, 3);
}

#[test]
fn grouped_projection_reassigns_colliding_synthetic_key_without_losing_expansion() {
    let (mut ui, id, source) = fixture();
    ui.set_grid_presentation(id, grouped()).unwrap();
    ui.update(Size::new(600.0, 400.0)).unwrap();
    let group = ui.grids[&id].source.projection.groups[0];
    let identity = ui.grid_row_kind(id, group).unwrap();
    ui.set_grid_expanded(id, group, false).unwrap();
    source
        .borrow_mut()
        .write_batch(&[rust_desktop_ui_data::DataWrite::Insert(DataRow::new(
            data_key(group),
            vec![CellValue::Text("C".into()), CellValue::Integer(4)],
        ))])
        .unwrap();
    ui.update(Size::new(600.0, 400.0)).unwrap();
    assert_eq!(
        ui.grid_row_kind(id, group),
        Some(GridRowKind::Record(data_key(group)))
    );
    let replacement = ui.grids[&id]
        .source
        .projection
        .groups
        .iter()
        .copied()
        .find(|key| ui.grid_row_kind(id, *key) == Some(identity.clone()))
        .unwrap();
    assert_ne!(replacement, group);
    assert!(!ui.grids[&id].model.is_expanded(replacement));
    assert_eq!(
        ui.grid_summaries(id),
        Some([CellValue::Integer(16)].as_slice())
    );
}

#[test]
fn aggregate_overflow_keeps_previous_presentation_query_selection_and_scroll() {
    let (mut ui, id, source) = fixture();
    source
        .borrow_mut()
        .set_cell(0, "amount", CellValue::Integer(i64::MAX))
        .unwrap();
    ui.set_grid_selected_key(id, Some(i64::MIN)).unwrap();
    ui.grid_scroll_to(id, 0.0, 12.0).unwrap();
    let scroll = ui.grids[&id].model.scroll_offset();
    let query = source.borrow().query().clone();
    assert!(ui.set_grid_presentation(id, grouped()).is_err());
    assert_eq!(
        ui.grid_presentation(id),
        Some(&GridPresentationSettings::default())
    );
    assert_eq!(ui.grid_selected_key(id), Some(i64::MIN));
    assert_eq!(ui.grids[&id].model.scroll_offset(), scroll);
    assert_eq!(source.borrow().query(), &query);
}

#[test]
fn column_bands_merge_adjacent_cells_and_group_disclosure_survives_hidden_tree_column() {
    let (mut ui, id, _) = fixture();
    let mut settings = grouped();
    settings.bands.push(GridBand {
        title: "Values".into(),
        fields: vec!["category".into(), "amount".into()],
    });
    ui.set_grid_presentation(id, settings).unwrap();
    ui.update(Size::new(600.0, 400.0)).unwrap();
    let mut outer = Scene::default();
    ui.grids[&id].decorate_outer(ui.grids[&id].bounds.unwrap(), &mut outer);
    assert_eq!(outer.texts.iter().filter(|t| t.text == "Values").count(), 1);
    let mut arrangement = ui.grid_arrangement(id).unwrap();
    arrangement[0].visible = false;
    ui.arrange_grid_columns(id, arrangement).unwrap();
    ui.set_grid_read_only(id, true).unwrap();
    ui.update(Size::new(600.0, 400.0)).unwrap();
    let frame = ui.grid_frame(id).unwrap();
    let group = frame
        .semantics
        .rows
        .iter()
        .find(|r| matches!(ui.grid_row_kind(id, r.key), Some(GridRowKind::Group { .. })))
        .unwrap();
    let hit=frame.hits.iter().find(|h|matches!(h.kind,rust_desktop_ui_treegrid::GridHitKind::Expander(key) if key==group.key)).unwrap();
    assert!((hit.bounds.x - group.bounds.x).abs() < 0.5);
    assert!(!group.cells[0].editable);
}

#[test]
fn borrowed_source_rejects_keyboard_before_focus_selection_or_capture_changes() {
    let (mut ui, id, source) = fixture();
    ui.grid_focus_cell(id, 0, None).unwrap();
    let selected = ui.grid_selected_key(id);
    let focused = ui.runtime.focused();
    let guard = source.borrow_mut();
    assert!(
        ui.grid_before_input(&InputEvent::KeyDown {
            key: Key::ArrowDown,
            repeat: false,
            modifiers: Default::default()
        })
        .is_err()
    );
    assert_eq!(ui.grid_selected_key(id), selected);
    assert_eq!(ui.runtime.focused(), focused);
    drop(guard);
}

#[test]
fn asynchronous_typed_grid_commit_is_atomic_and_cancel_ignores_old_completion() {
    use std::sync::{Arc, Mutex, mpsc};
    use std::time::{Duration, Instant};
    for cancel in [false, true] {
        let (mut ui, id, source) = fixture();
        ui.set_grid_editor_factory(id, "amount", "Auto", &DataFormFactories::default())
            .unwrap();
        ui.begin_grid_edit(id, 0, "amount").unwrap();
        let editor = ui.grid_editor(id).unwrap();
        let (release, wait) = mpsc::channel();
        let (done, completed) = mpsc::channel();
        let wait = Arc::new(Mutex::new(wait));
        ui.set_editor_validation_waker(Some(Arc::new(move || {
            let _ = done.send(());
        })));
        ui.set_async_editor_validator(
            editor,
            Some(AsyncEditorValidator::new(move |_| {
                wait.lock()
                    .unwrap()
                    .recv_timeout(Duration::from_secs(3))
                    .unwrap();
                Ok(())
            })),
        )
        .unwrap();
        ui.set_text(editor, "42").unwrap();
        assert_eq!(ui.commit_grid_editor(id).unwrap(), CommitOutcome::Pending);
        assert_eq!(
            source.borrow().value(0, "amount"),
            Some(&CellValue::Integer(2))
        );
        if cancel {
            ui.cancel_grid_edit(id).unwrap();
        }
        release.send(()).unwrap();
        completed.recv_timeout(Duration::from_secs(3)).unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        // The worker sends its completion after the callback returns.
        while Instant::now() < deadline {
            ui.poll_editor_validation().unwrap();
            if (!cancel && ui.grid_editor(id).is_none())
                || (cancel && ui.editor_validation_status(editor) == EditorValidationStatus::Idle)
            {
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        ui.poll_editor_validation().unwrap();
        assert!(ui.grid_editor(id).is_none());
        assert_eq!(
            source.borrow().value(0, "amount"),
            Some(&CellValue::Integer(if cancel { 2 } else { 42 }))
        );
    }
}

#[test]
fn a_new_earlier_group_cannot_steal_existing_group_identity_or_collapsed_state() {
    let (mut ui, id, source) = fixture();
    ui.set_grid_presentation(id, grouped()).unwrap();
    ui.update(Size::new(600.0, 400.0)).unwrap();
    let old: Vec<_> = ui.grids[&id]
        .source
        .projection
        .groups
        .iter()
        .map(|key| (*key, ui.grid_row_kind(id, *key).unwrap()))
        .collect();
    ui.set_grid_expanded(id, old[0].0, false).unwrap();
    source
        .borrow_mut()
        .write_batch(&[rust_desktop_ui_data::DataWrite::Insert(DataRow::new(
            90,
            vec![CellValue::Text("0".into()), CellValue::Integer(1)],
        ))])
        .unwrap();
    ui.update(Size::new(600.0, 400.0)).unwrap();
    for (key, identity) in old {
        assert_eq!(ui.grid_row_kind(id, key), Some(identity));
    }
    assert!(
        !ui.grids[&id]
            .model
            .is_expanded(ui.grids[&id].source.projection.groups[1])
    );
}

#[test]
fn changed_typed_recipe_rejects_commit_without_publishing_or_losing_draft() {
    let (mut ui, id, source) = fixture();
    ui.set_grid_editor_factory(id, "amount", "Auto", &DataFormFactories::default())
        .unwrap();
    ui.begin_grid_edit(id, 0, "amount").unwrap();
    let host = ui.grids[&id].typed_editor.as_ref().unwrap().owner;
    let editor = ui.grid_editor(id).unwrap();
    let mut settings = ui.numeric_settings(host).unwrap().clone();
    settings.maximum = "100".parse().unwrap();
    ui.set_numeric_settings(host, settings).unwrap();
    ui.set_text(editor, "42").unwrap();
    assert!(!ui.commit_grid_edit(id).unwrap());
    assert_eq!(
        source.borrow().value(0, "amount"),
        Some(&CellValue::Integer(2))
    );
    assert_eq!(ui.document(editor).unwrap().text(), "42");
    ui.cancel_grid_edit(id).unwrap();
    ui.begin_grid_edit(id, 0, "amount").unwrap();
    ui.set_grid_edit_value(id, CellValue::Integer(9_007_199_254_740_993))
        .unwrap();
    assert!(ui.commit_grid_edit(id).unwrap());
}

#[test]
fn numeric_calculator_inside_grid_commits_once_and_keeps_its_host_until_callback_finishes() {
    let (mut ui, id, source) = fixture();
    ui.set_grid_editor_factory(id, "amount", "Auto", &DataFormFactories::default())
        .unwrap();
    ui.begin_grid_edit(id, 0, "amount").unwrap();
    let host = ui.grids[&id].typed_editor.as_ref().unwrap().owner;
    ui.update(Size::new(600.0, 600.0)).unwrap();
    ui.open_numeric_calculator(host).unwrap();
    ui.press_numeric_calculator(host, CalculatorKey::Clear)
        .unwrap();
    ui.press_numeric_calculator(host, CalculatorKey::Digit(9))
        .unwrap();
    assert_eq!(
        ui.apply_numeric_calculator(host).unwrap(),
        CommitOutcome::Committed
    );
    assert_eq!(
        source.borrow().value(0, "amount"),
        Some(&CellValue::Integer(9))
    );
    assert!(ui.grid_editor(id).is_none());
    assert!(ui.runtime.tree().contains(host));
    assert!(!ui.numeric_calculator_is_open(host));
    ui.update(Size::new(600.0, 400.0)).unwrap();
    ui.begin_grid_edit(id, i64::MAX, "amount").unwrap();
    assert!(!ui.runtime.tree().contains(host));
}

#[test]
fn grouped_cards_of_one_hundred_thousand_records_keep_bounded_retained_hosts_and_cells() {
    let source = table(
        (0..100_000)
            .map(|key| {
                DataRow::new(
                    key,
                    vec![
                        CellValue::Text(format!("Group {}", key % 10)),
                        CellValue::Integer(key),
                    ],
                )
            })
            .collect(),
    );
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let id = ui
        .grid_control(
            ui.root(),
            source,
            Default::default(),
            LayoutStyle {
                width: Length::Px(600.0),
                height: Length::Px(320.0),
                ..Default::default()
            },
        )
        .unwrap();
    let retained = ui.runtime.tree().preorder().len();
    let mut settings = grouped();
    settings.view = GridViewMode::Cards;
    ui.set_grid_presentation(id, settings).unwrap();
    ui.update(Size::new(640.0, 480.0)).unwrap();
    assert_eq!(ui.grid_frame(id).unwrap().stats.logical_rows, 100_010);
    for offset in [0.0, 2_000_000.0, 9_000_000.0] {
        ui.grid_scroll_to(id, 0.0, offset).unwrap();
        ui.update(Size::new(640.0, 480.0)).unwrap();
        let frame = ui.grid_frame(id).unwrap();
        assert!(frame.semantics.rows.len() < 20);
        assert!(frame.scene.texts.len() < 80);
        assert_eq!(ui.runtime.tree().preorder().len(), retained);
    }
    assert_eq!(
        ui.grid_summaries(id),
        Some([CellValue::Integer(4_999_950_000)].as_slice())
    );
}

#[test]
fn detail_snapshot_refresh_is_deferred_during_master_edit_then_reconciles_keys() {
    let (mut ui, id, _) = fixture();
    let detail = table(vec![DataRow::new(
        10,
        vec![CellValue::Text("A".into()), CellValue::Integer(8)],
    )]);
    ui.set_grid_detail(
        id,
        Some(GridDetailSettings {
            source: detail.clone(),
            master_field: "category".into(),
            foreign_field: "category".into(),
            columns: vec![
                GridControlColumn::new("category", "Category", 150.0),
                GridControlColumn::new("amount", "Detail amount", 150.0),
            ],
        }),
    )
    .unwrap();
    ui.set_grid_expanded(id, row_key(0), true).unwrap();
    ui.update(Size::new(600.0, 400.0)).unwrap();
    let initial = ui
        .grid_frame(id)
        .unwrap()
        .semantics
        .rows
        .iter()
        .flat_map(|r| &r.cells)
        .any(|c| c.label == "8");
    assert!(initial);
    ui.begin_grid_edit(id, 0, "amount").unwrap();
    let editor = ui.grid_editor(id).unwrap();
    ui.set_text(editor, "42").unwrap();
    detail
        .borrow_mut()
        .set_cell(10, "amount", CellValue::Integer(99))
        .unwrap();
    ui.update(Size::new(600.0, 400.0)).unwrap();
    assert!(!ui.grid_snapshot_is_current(id).unwrap());
    assert!(
        ui.grid_frame(id)
            .unwrap()
            .semantics
            .rows
            .iter()
            .flat_map(|r| &r.cells)
            .any(|c| c.label == "8")
    );
    assert_eq!(ui.document(editor).unwrap().text(), "42");
    ui.cancel_grid_edit(id).unwrap();
    ui.update(Size::new(600.0, 400.0)).unwrap();
    assert!(ui.grid_snapshot_is_current(id).unwrap());
    assert!(
        ui.grid_frame(id)
            .unwrap()
            .semantics
            .rows
            .iter()
            .flat_map(|r| &r.cells)
            .any(|c| c.label == "99")
    );
}

#[test]
fn hidden_column_edit_refuses_before_loading_text_or_replacing_typed_host() {
    for typed in [false, true] {
        let (mut ui, id, _) = fixture();
        if typed {
            ui.set_grid_editor_factory(id, "amount", "Auto", &DataFormFactories::default())
                .unwrap();
        }
        ui.begin_grid_edit(id, 0, "amount").unwrap();
        let editor = ui.grid_editor(id).unwrap();
        ui.cancel_grid_edit(id).unwrap();
        let accepted = ui.accepted_value(editor).unwrap().to_owned();
        let text = ui.document(editor).unwrap().text().to_owned();
        let mut arrangement = ui.grid_arrangement(id).unwrap();
        arrangement
            .iter_mut()
            .find(|a| a.field == "amount")
            .unwrap()
            .visible = false;
        ui.arrange_grid_columns(id, arrangement).unwrap();
        let nodes = ui.runtime.tree().live_ids();
        let selected = ui.grid_selected_key(id);
        let focused = ui.runtime.focused();
        assert!(ui.begin_grid_edit(id, i64::MIN, "amount").is_err());
        assert_eq!(ui.runtime.tree().live_ids(), nodes);
        assert_eq!(ui.grid_selected_key(id), selected);
        assert_eq!(ui.runtime.focused(), focused);
        assert_eq!(ui.accepted_value(editor), Some(accepted.as_str()));
        assert_eq!(ui.document(editor).unwrap().text(), text);
    }
}

#[test]
fn decorated_card_text_is_preflighted_as_a_complete_string_before_presentation_publish() {
    let source = table(vec![DataRow::new(
        0,
        vec![CellValue::Text("\u{301}".repeat(32)), CellValue::Integer(1)],
    )]);
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let id = ui
        .grid_control(
            ui.root(),
            source,
            Default::default(),
            LayoutStyle {
                width: Length::Px(400.0),
                height: Length::Px(240.0),
                ..Default::default()
            },
        )
        .unwrap();
    ui.update(Size::new(500.0, 300.0)).unwrap();
    let nodes = ui.runtime.tree().live_ids();
    let result = ui.set_grid_presentation(
        id,
        GridPresentationSettings {
            view: GridViewMode::Cards,
            ..Default::default()
        },
    );
    assert!(result.is_err());
    assert_eq!(
        ui.grid_presentation(id),
        Some(&GridPresentationSettings::default())
    );
    assert_eq!(ui.runtime.tree().live_ids(), nodes);
    ui.update(Size::new(500.0, 300.0)).unwrap();
}

#[test]
fn decorated_cells_preserve_zebra_lines_selection_and_shaped_vertical_alignment() {
    for theme in [Theme::FluentLight, Theme::FluentDark] {
        for view in [GridViewMode::Table, GridViewMode::Cards] {
            let (mut ui, id, _) = fixture();
            ui.set_theme(theme).unwrap();
            ui.set_grid_selected_key(id, Some(i64::MIN)).unwrap();
            ui.set_grid_presentation(
                id,
                GridPresentationSettings {
                    view,
                    ..Default::default()
                },
            )
            .unwrap();
            ui.update(Size::new(600.0, 400.0)).unwrap();
            let before = ui.grid_frame(id).unwrap().scene.clone();
            ui.set_grid_cell_template(
                id,
                "category",
                Some(GridCellTemplate {
                    prefix: "[".into(),
                    suffix: "]".into(),
                    icon: Some(IconSymbol::Edit),
                    emphasis: true,
                }),
            )
            .unwrap();
            ui.set_grid_cell_template(
                id,
                "amount",
                Some(GridCellTemplate {
                    suffix: " units".into(),
                    ..Default::default()
                }),
            )
            .unwrap();
            ui.update(Size::new(600.0, 400.0)).unwrap();
            let frame = ui.grid_frame(id).unwrap().clone();
            // No extra opaque rectangle may cover stripes, borders or focus.
            assert_eq!(frame.scene.rectangles, before.rectangles);
            assert_eq!(frame.scene.rounded_rectangles, before.rounded_rectangles);
            assert_eq!(frame.scene.texts.len(), before.texts.len());
            for row in &frame.semantics.rows {
                for cell in &row.cells {
                    let run = frame
                        .scene
                        .texts
                        .iter()
                        .find(|run| run.clip == cell.bounds)
                        .unwrap();
                    assert!(
                        (run.bounds.y + run.bounds.height * 0.5
                            - (cell.bounds.y + cell.bounds.height * 0.5))
                            .abs()
                            < 0.01
                    );
                    let mut measure = run.clone();
                    measure.clip = Rect::new(0.0, 0.0, 0.0, 0.0);
                    assert_eq!(
                        run.bounds.height,
                        ui.engine.prepare(&measure, 1.0).unwrap().height
                    );
                    if cell.column == ColumnId(0) {
                        assert!(run.text.contains(&format!("[{}]", cell.label)));
                        assert_eq!(run.weight, 600);
                    } else {
                        assert!(run.text.ends_with(" units"));
                    }
                    if view == GridViewMode::Cards {
                        assert!(run.text.starts_with(if cell.column == ColumnId(0) {
                            "category: "
                        } else {
                            "amount: "
                        }));
                    }
                }
            }
        }
    }
}

#[test]
fn card_template_setter_preflights_column_title_and_prefix_as_one_string() {
    let (mut ui, id, _) = fixture();
    ui.set_grid_presentation(
        id,
        GridPresentationSettings {
            view: GridViewMode::Cards,
            ..Default::default()
        },
    )
    .unwrap();
    ui.update(Size::new(600.0, 400.0)).unwrap();
    let old = ui.grid_frame(id).unwrap().scene.clone();
    let result = ui.set_grid_cell_template(
        id,
        "category",
        Some(GridCellTemplate {
            prefix: "\u{301}".repeat(32),
            ..Default::default()
        }),
    );
    assert!(result.is_err());
    assert!(ui.grids[&id].cell_templates.is_empty());
    ui.update(Size::new(600.0, 400.0)).unwrap();
    assert_eq!(ui.grid_frame(id).unwrap().scene, old);
}

#[test]
fn partially_scrolled_cards_keep_full_geometry_and_text_translates_without_compression() {
    let (mut ui, id, _) = fixture();
    ui.edit_props(id, |props| props.style.height = Length::Px(120.0))
        .unwrap();
    ui.set_grid_presentation(
        id,
        GridPresentationSettings {
            view: GridViewMode::Cards,
            ..Default::default()
        },
    )
    .unwrap();
    ui.update(Size::new(600.0, 400.0)).unwrap();
    let frame = ui.grid_frame(id).unwrap();
    let before = frame
        .scene
        .texts
        .iter()
        .find(|run| run.text == "category: B")
        .unwrap()
        .bounds;
    let card = frame.scene.rounded_rectangles[0].bounds;
    ui.grid_scroll_by(id, 0.0, 17.0).unwrap();
    ui.update(Size::new(600.0, 400.0)).unwrap();
    let frame = ui.grid_frame(id).unwrap();
    let after = frame
        .scene
        .texts
        .iter()
        .find(|run| run.text == "category: B")
        .unwrap()
        .bounds;
    assert!((before.y - after.y - 17.0).abs() < 0.01);
    assert_eq!(before.height, after.height);
    let scrolled = frame.scene.rounded_rectangles[0].bounds;
    assert_eq!(scrolled.height, card.height);
    assert!((card.y - scrolled.y - 17.0).abs() < 0.01);
}
