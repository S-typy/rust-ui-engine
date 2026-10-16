use super::*;
use rust_desktop_ui_core::Modifiers;
use rust_desktop_ui_data::{
    CellValue, CollectionAdapter, DataRow, DataSource, DataTable, FieldSchema, FieldType,
    TableSchema,
};
use rust_desktop_ui_treegrid::{ColumnId, GridHitKind, RowKey};
use std::{cell::RefCell, rc::Rc};

fn source(count: usize) -> DataSourceHandle {
    let schema = TableSchema::new(vec![
        FieldSchema::new("name", FieldType::Text),
        FieldSchema::new("amount", FieldType::Integer),
    ])
    .unwrap();
    let rows = (0..count)
        .map(|i| {
            DataRow::new(
                i as i64 - 10,
                vec![
                    CellValue::Text(format!("Row {i:06}")),
                    CellValue::Integer(i as i64),
                ],
            )
        })
        .collect();
    Rc::new(RefCell::new(
        DataSource::new(CollectionAdapter::new(
            DataTable::new(schema, rows).unwrap(),
        ))
        .unwrap(),
    ))
}
fn setup(count: usize) -> (Controls, WidgetId, DataSourceHandle) {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let source = source(count);
    let id = ui
        .grid_control(
            ui.root(),
            Rc::clone(&source),
            GridControlSettings::default(),
            LayoutStyle {
                width: Length::Px(420.0),
                height: Length::Px(280.0),
                ..Default::default()
            },
        )
        .unwrap();
    frame(&mut ui);
    (ui, id, source)
}
fn frame(ui: &mut Controls) {
    ui.update(Size::new(640.0, 420.0)).unwrap();
}
fn key(ui: &mut Controls, key: Key) {
    ui.dispatch(InputEvent::KeyDown {
        key,
        modifiers: Modifiers::default(),
        repeat: false,
    })
    .unwrap();
}
fn click(ui: &mut Controls, point: Point) {
    ui.dispatch(InputEvent::PointerDown {
        position: point,
        button: PointerButton::Primary,
        modifiers: Modifiers::default(),
    })
    .unwrap();
    ui.dispatch(InputEvent::PointerUp {
        position: point,
        button: PointerButton::Primary,
        modifiers: Modifiers::default(),
    })
    .unwrap();
}
fn hit(ui: &Controls, id: WidgetId, kind: GridHitKind) -> Point {
    let bounds = ui
        .grid_frame(id)
        .unwrap()
        .hits
        .iter()
        .find(|h| h.kind == kind)
        .unwrap()
        .bounds;
    Point::new(
        bounds.x + bounds.width / 2.0,
        bounds.y + bounds.height / 2.0,
    )
}

#[test]
fn grid_materializes_a_viewport_of_a_hundred_thousand_rows() {
    let (mut ui, id, _) = setup(100_000);
    let before = ui.runtime.tree().preorder().len();
    assert_eq!(ui.grid_frame(id).unwrap().stats.logical_rows, 100_000);
    assert!(ui.grid_frame(id).unwrap().stats.materialized_rows < 20);
    assert!(ui.grid_frame(id).unwrap().stats.cells_read < 40);
    ui.grid_scroll_to(id, 0.0, 2_000_000.0).unwrap();
    frame(&mut ui);
    assert!(ui.grid_frame(id).unwrap().rows[0].index > 50_000);
    assert_eq!(ui.runtime.tree().preorder().len(), before);
    assert!(ui.grid_frame(id).unwrap().stats.materialized_rows < 20);
}

#[test]
fn grid_header_sort_and_filter_keep_signed_identity() {
    let (mut ui, id, source) = setup(20);
    ui.set_grid_selected_key(id, Some(-7)).unwrap();
    let point = hit(&ui, id, GridHitKind::Header(ColumnId(1)));
    click(&mut ui, point);
    click(&mut ui, point);
    frame(&mut ui);
    assert_eq!(source.borrow().row_key(0), Some(9));
    assert_eq!(ui.grid_selected_key(id), Some(-7));
    ui.set_grid_filter(id, "Row 000003").unwrap();
    frame(&mut ui);
    assert_eq!(ui.grid_selected_key(id), Some(-7));
    assert_eq!(ui.grid_frame(id).unwrap().stats.logical_rows, 1);
    assert_eq!(ui.grid_filter(id), Some("Row 000003"));
}

#[test]
fn grid_keyboard_routes_selection_edit_commit_and_cancel() {
    let (mut ui, id, source) = setup(20);
    ui.runtime.apply(Mutation::Focus(Some(id))).unwrap();
    key(&mut ui, Key::ArrowDown);
    key(&mut ui, Key::ArrowRight);
    key(&mut ui, Key::F2);
    let editor = ui.grid_editor(id).unwrap();
    ui.set_text(editor, "456").unwrap();
    key(&mut ui, Key::Enter);
    assert_eq!(
        source.borrow().value(-10, "amount"),
        Some(&CellValue::Integer(456))
    );
    assert!(ui.grid_editor(id).is_none());
    key(&mut ui, Key::F2);
    ui.set_text(editor, "123").unwrap();
    key(&mut ui, Key::Escape);
    assert!(ui.grid_editor(id).is_none());
    assert_eq!(
        source.borrow().value(-10, "amount"),
        Some(&CellValue::Integer(456))
    );
    key(&mut ui, Key::End);
    assert_eq!(ui.grid_selected_key(id), Some(9));
    key(&mut ui, Key::PageUp);
    assert!(ui.grid_selected_key(id).unwrap() < 9);
    key(&mut ui, Key::Home);
    assert_eq!(ui.grid_selected_key(id), Some(-10));
}

#[test]
fn invalid_grid_edit_keeps_document_baseline_and_error_until_cancel() {
    let (mut ui, id, source) = setup(3);
    ui.begin_grid_edit(id, -10, "amount").unwrap();
    let editor = ui.grid_editor(id).unwrap();
    ui.set_text(editor, "incorrect").unwrap();
    assert!(!ui.commit_grid_edit(id).unwrap());
    assert_eq!(ui.document(editor).unwrap().text(), "incorrect");
    assert_eq!(ui.accepted_value(editor), Some("0"));
    assert_eq!(
        source.borrow().value(-10, "amount"),
        Some(&CellValue::Integer(0))
    );
    assert!(ui.grid_error(id).is_some());
    assert!(ui.validation_error(editor).is_some());
    assert!(ui.set_grid_filter(id, "Row").is_err());
    ui.cancel_grid_edit(id).unwrap();
    assert!(ui.grid_error(id).is_none());
}

#[test]
fn shared_source_change_defers_snapshot_and_rejects_conflicting_draft() {
    let (mut ui, id, source) = setup(3);
    ui.begin_grid_edit(id, -10, "amount").unwrap();
    let editor = ui.grid_editor(id).unwrap();
    ui.set_text(editor, "77").unwrap();
    source
        .borrow_mut()
        .set_cell(-10, "amount", CellValue::Integer(99))
        .unwrap();
    frame(&mut ui);
    assert_eq!(ui.document(editor).unwrap().text(), "77");
    assert!(!ui.commit_grid_edit(id).unwrap());
    assert_eq!(
        source.borrow().value(-10, "amount"),
        Some(&CellValue::Integer(99))
    );
    ui.cancel_grid_edit(id).unwrap();
    frame(&mut ui);
    assert!(
        ui.grid_frame(id)
            .unwrap()
            .semantics
            .rows
            .iter()
            .any(|row| row.cells.iter().any(|cell| cell.label == "99"))
    );
}

#[test]
fn unrelated_data_edits_allow_commit_and_live_revision_updates() {
    let (mut ui, id, source) = setup(3);
    ui.begin_grid_edit(id, -10, "amount").unwrap();
    let editor = ui.grid_editor(id).unwrap();
    ui.set_text(editor, "77").unwrap();
    source
        .borrow_mut()
        .set_cell(-9, "name", CellValue::Text("Other".into()))
        .unwrap();
    frame(&mut ui);
    assert!(ui.commit_grid_edit(id).unwrap());
    frame(&mut ui);
    assert_eq!(
        source.borrow().value(-10, "amount"),
        Some(&CellValue::Integer(77))
    );
    assert!(
        ui.grid_frame(id)
            .unwrap()
            .semantics
            .rows
            .iter()
            .any(|row| row.cells.iter().any(|cell| cell.label == "Other"))
    );
}

#[test]
fn grid_read_only_changes_are_reversible_and_do_not_lose_draft() {
    let (mut ui, id, source) = setup(3);
    ui.set_grid_read_only(id, true).unwrap();
    assert!(ui.begin_grid_edit(id, -10, "amount").is_err());
    ui.set_grid_read_only(id, false).unwrap();
    ui.begin_grid_edit(id, -10, "amount").unwrap();
    let editor = ui.grid_editor(id).unwrap();
    ui.set_text(editor, "77").unwrap();
    ui.set_grid_read_only(id, true).unwrap();
    assert!(!ui.commit_grid_edit(id).unwrap());
    assert_eq!(ui.document(editor).unwrap().text(), "77");
    ui.set_grid_read_only(id, false).unwrap();
    assert!(ui.commit_grid_edit(id).unwrap());
    assert_eq!(
        source.borrow().value(-10, "amount"),
        Some(&CellValue::Integer(77))
    );
}

#[test]
fn grid_registry_and_invalid_columns_are_atomic() {
    let (mut ui, _, source) = setup(3);
    ui.register_data_source("Rows", Rc::clone(&source)).unwrap();
    assert!(Rc::ptr_eq(&ui.data_source("Rows").unwrap(), &source));
    assert!(ui.register_data_source("Rows", Rc::clone(&source)).is_err());
    assert!(
        ui.register_data_source("bad source", Rc::clone(&source))
            .is_err()
    );
    for column in [
        GridControlColumn::new("missing", "Bad", 100.0),
        GridControlColumn::new("name", "Bad", f32::NAN),
    ] {
        let before = ui.runtime.tree().preorder().len();
        assert!(
            ui.grid_control(
                ui.root(),
                Rc::clone(&source),
                GridControlSettings {
                    columns: vec![column],
                    ..Default::default()
                },
                LayoutStyle::default()
            )
            .is_err()
        );
        assert_eq!(ui.runtime.tree().preorder().len(), before);
    }
}

#[test]
fn grid_column_resize_capture_cancel_and_pin_work() {
    let (mut ui, id, _) = setup(3);
    let point = hit(&ui, id, GridHitKind::ResizeColumn(ColumnId(0)));
    ui.dispatch(InputEvent::PointerDown {
        position: point,
        button: PointerButton::Primary,
        modifiers: Modifiers::default(),
    })
    .unwrap();
    ui.dispatch(InputEvent::PointerMoved {
        position: Point::new(point.x + 40.0, point.y),
    })
    .unwrap();
    frame(&mut ui);
    let width = ui
        .grid_frame(id)
        .unwrap()
        .columns
        .iter()
        .find(|c| c.id == ColumnId(0))
        .unwrap()
        .bounds
        .width;
    assert!((width - 200.0).abs() < 0.1);
    ui.cancel_pointer_interaction().unwrap();
    ui.dispatch(InputEvent::PointerMoved {
        position: Point::new(point.x + 100.0, point.y),
    })
    .unwrap();
    frame(&mut ui);
    assert_eq!(
        ui.grid_frame(id)
            .unwrap()
            .columns
            .iter()
            .find(|c| c.id == ColumnId(0))
            .unwrap()
            .bounds
            .width,
        width
    );
    ui.pin_grid_column(id, "name", true).unwrap();
    frame(&mut ui);
    assert!(
        ui.grid_frame(id)
            .unwrap()
            .columns
            .iter()
            .any(|c| c.id == ColumnId(0) && c.pinned)
    );
}

#[test]
fn clicking_cell_and_losing_focus_commits_through_source_once() {
    let (mut ui, id, source) = setup(3);
    let point = hit(
        &ui,
        id,
        GridHitKind::Cell {
            row: RowKey((-9_i64) as u64),
            column: ColumnId(1),
        },
    );
    click(&mut ui, point);
    assert_eq!(ui.grid_selected_key(id), Some(-9));
    key(&mut ui, Key::F2);
    let editor = ui.grid_editor(id).unwrap();
    ui.set_text(editor, "55").unwrap();
    let other = ui
        .button(ui.root(), "Other", None, LayoutStyle::default())
        .unwrap();
    frame(&mut ui);
    let b = ui.runtime.bounds(other).unwrap();
    click(&mut ui, Point::new(b.x + 4.0, b.y + 4.0));
    assert_eq!(
        source.borrow().value(-9, "amount"),
        Some(&CellValue::Integer(55))
    );
    assert!(ui.grid_editor(id).is_none());
    assert_eq!(source.borrow().revision(), 1);
}

#[test]
fn grid_removal_owns_editor_and_leaves_source_alive() {
    let (mut ui, id, source) = setup(3);
    ui.begin_grid_edit(id, -10, "name").unwrap();
    let editor = ui.grid_editor(id).unwrap();
    assert!(ui.remove(editor).is_err());
    ui.remove(id).unwrap();
    frame(&mut ui);
    assert!(ui.grid_frame(id).is_none());
    assert!(ui.document(editor).is_none());
    assert_eq!(source.borrow().len(), 3);
}

#[test]
fn grid_empty_source_has_real_empty_state() {
    let (ui, id, _) = setup(0);
    assert_eq!(ui.grid_frame(id).unwrap().stats.logical_rows, 0);
    assert!(ui.grid_frame(id).unwrap().semantics.rows.is_empty());
    assert!(ui.grid_error(id).is_none());
}

#[test]
fn grid_initial_bound_selection_reveals_first_row_after_viewport_exists() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let id = ui
        .grid_control(
            ui.root(),
            source(1000),
            GridControlSettings::default(),
            LayoutStyle {
                height: Length::Px(280.0),
                ..Default::default()
            },
        )
        .unwrap();
    ui.set_grid_selected_key(id, Some(-10)).unwrap();
    frame(&mut ui);
    let frame = ui.grid_frame(id).unwrap();
    assert_eq!(frame.semantics.rows[0].key, RowKey((-10_i64) as u64));
    assert!(frame.semantics.rows[0].selected);
    assert!((frame.semantics.rows[0].bounds.height - 28.0).abs() < 0.1);
}

#[test]
fn grid_scrollbar_track_and_drag_reach_rows_and_cancel_capture() {
    let (mut ui, id, _) = setup(1000);
    let bounds = ui.runtime.bounds(id).unwrap();
    let point = Point::new(
        bounds.x + bounds.width - 6.0,
        bounds.y + bounds.height * 0.7,
    );
    click(&mut ui, point);
    frame(&mut ui);
    assert!(ui.grid_frame(id).unwrap().rows[0].index > 500);
    ui.dispatch(InputEvent::PointerDown {
        position: point,
        button: PointerButton::Primary,
        modifiers: Modifiers::default(),
    })
    .unwrap();
    assert_eq!(ui.runtime.captured(), Some(id));
    ui.cancel_pointer_interaction().unwrap();
    assert_eq!(ui.runtime.captured(), None);
    frame(&mut ui);
    let index = ui.grid_frame(id).unwrap().rows[0].index;
    ui.dispatch(InputEvent::PointerMoved {
        position: Point::new(point.x, point.y + 60.0),
    })
    .unwrap();
    frame(&mut ui);
    assert_eq!(ui.grid_frame(id).unwrap().rows[0].index, index);
}

#[test]
fn grid_null_text_unchanged_commit_and_multiline_rejection_are_lossless() {
    let mut field = FieldSchema::new("Text field", FieldType::Text);
    field.nullable = true;
    let schema = TableSchema::new(vec![field]).unwrap();
    let source = Rc::new(RefCell::new(
        DataSource::new(CollectionAdapter::new(
            DataTable::new(
                schema,
                vec![
                    DataRow::new(i64::MAX, vec![CellValue::Null]),
                    DataRow::new(i64::MIN, vec![CellValue::Text("first\nsecond".into())]),
                ],
            )
            .unwrap(),
        ))
        .unwrap(),
    ));
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let id = ui
        .grid_control(
            ui.root(),
            Rc::clone(&source),
            GridControlSettings::default(),
            LayoutStyle::default(),
        )
        .unwrap();
    frame(&mut ui);
    ui.begin_grid_edit(id, i64::MAX, "Text field").unwrap();
    assert!(ui.commit_grid_edit(id).unwrap());
    assert_eq!(
        source.borrow().value(i64::MAX, "Text field"),
        Some(&CellValue::Null)
    );
    assert!(ui.begin_grid_edit(id, i64::MIN, "Text field").is_err());
    assert!(ui.grid_editor(id).is_none());
    assert_eq!(
        source.borrow().value(i64::MIN, "Text field"),
        Some(&CellValue::Text("first\nsecond".into()))
    );
}

#[test]
fn grid_selected_value_and_filter_bindings_preflight_model_validation() {
    use crate::{
        BindingContext, BindingProperty, BindingValidator, ControlBinding, ControlBindingMode,
        ControlBindings, DataValue,
    };
    let (mut ui, id, _) = setup(3);
    let data = Rc::new(RefCell::new(
        BindingContext::new(DataValue::object([
            ("Selected", DataValue::String("-10".into())),
            ("Filter", DataValue::String(String::new())),
        ]))
        .unwrap(),
    ));
    data.borrow_mut()
        .set_validator(Some(BindingValidator::new(|root| {
            let object = root.as_object().unwrap();
            if object["Selected"].as_str() == Some("-9")
                || object["Filter"].as_str() == Some("forbidden")
            {
                Err("Rejected".into())
            } else {
                Ok(())
            }
        })))
        .unwrap();
    let mut bindings = ControlBindings::new(Rc::clone(&data));
    for (property, path) in [
        (BindingProperty::SelectedValue, "Selected"),
        (BindingProperty::FilterText, "Filter"),
    ] {
        let mut binding = ControlBinding::new(id, property, path).unwrap();
        binding.mode = ControlBindingMode::TwoWay;
        bindings.add(binding).unwrap();
    }
    bindings.initialize(&mut ui).unwrap();
    assert_eq!(ui.grid_selected_key(id), Some(-10));
    assert!(ui.set_grid_selected_key(id, Some(-9)).is_err());
    ui.runtime.apply(Mutation::Focus(Some(id))).unwrap();
    key(&mut ui, Key::ArrowDown);
    assert_eq!(ui.grid_selected_key(id), Some(-10));
    assert!(ui.set_grid_filter(id, "forbidden").is_err());
    assert_eq!(ui.grid_filter(id), Some(""));
    ui.set_grid_selected_key(id, Some(-8)).unwrap();
    bindings.sync(&mut ui);
    assert_eq!(data.borrow().get("Selected").unwrap().as_str(), Some("-8"));
    ui.set_grid_filter(id, "Row 000002").unwrap();
    bindings.sync(&mut ui);
    assert_eq!(
        data.borrow().get("Filter").unwrap().as_str(),
        Some("Row 000002")
    );
}

#[test]
fn grid_rendering_never_calls_adapter_and_failed_refresh_keeps_rows_and_draft() {
    use rust_desktop_ui_data::{DataAdapter, DataError};
    use std::cell::Cell;
    struct Adapter {
        table: DataTable,
        loads: Rc<Cell<usize>>,
        fail: Rc<Cell<bool>>,
    }
    impl DataAdapter for Adapter {
        fn load(&mut self) -> Result<DataTable, DataError> {
            self.loads.set(self.loads.get() + 1);
            if self.fail.get() {
                Err(DataError::Adapter("Disconnected".into()))
            } else {
                Ok(self.table.clone())
            }
        }
        fn write_cell(
            &mut self,
            _: i64,
            _: &str,
            _: &CellValue,
            _: &CellValue,
        ) -> Result<DataTable, DataError> {
            Err(DataError::ReadOnly("source".into()))
        }
    }
    let original = source(8);
    let loads = Rc::new(Cell::new(0));
    let fail = Rc::new(Cell::new(false));
    let source = Rc::new(RefCell::new(
        DataSource::new(Adapter {
            table: original.borrow().table().clone(),
            loads: Rc::clone(&loads),
            fail: Rc::clone(&fail),
        })
        .unwrap(),
    ));
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let id = ui
        .grid_control(
            ui.root(),
            Rc::clone(&source),
            GridControlSettings::default(),
            LayoutStyle::default(),
        )
        .unwrap();
    for _ in 0..5 {
        frame(&mut ui);
    }
    assert_eq!(loads.get(), 1);
    ui.begin_grid_edit(id, -10, "name").unwrap();
    let editor = ui.grid_editor(id).unwrap();
    ui.set_text(editor, "Draft").unwrap();
    fail.set(true);
    assert!(source.borrow_mut().refresh().is_err());
    frame(&mut ui);
    assert_eq!(loads.get(), 2);
    assert!(ui.grid_error(id).unwrap().contains("Disconnected"));
    assert_eq!(ui.grid_frame(id).unwrap().stats.logical_rows, 8);
    assert_eq!(ui.document(editor).unwrap().text(), "Draft");
    assert!(!ui.commit_grid_edit(id).unwrap());
    assert_eq!(ui.document(editor).unwrap().text(), "Draft");
}

#[test]
fn grid_source_revision_cancels_pending_header_click_and_late_release() {
    let (mut ui, id, source) = setup(4);
    let point = hit(&ui, id, GridHitKind::Header(ColumnId(0)));
    ui.dispatch(InputEvent::PointerDown {
        position: point,
        button: PointerButton::Primary,
        modifiers: Modifiers::default(),
    })
    .unwrap();
    assert_eq!(ui.runtime.captured(), Some(id));
    source
        .borrow_mut()
        .set_cell(-10, "amount", CellValue::Integer(8))
        .unwrap();
    frame(&mut ui);
    assert_eq!(ui.runtime.captured(), None);
    ui.dispatch(InputEvent::PointerUp {
        position: point,
        button: PointerButton::Primary,
        modifiers: Modifiers::default(),
    })
    .unwrap();
    assert!(source.borrow().query().sort.is_empty());
}

#[test]
fn grid_horizontal_keyboard_and_shift_wheel_reveal_columns_beyond_viewport() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let id = ui
        .grid_control(
            ui.root(),
            source(20),
            GridControlSettings {
                columns: vec![
                    GridControlColumn::new("name", "Name", 380.0),
                    GridControlColumn::new("amount", "Amount", 380.0),
                ],
                is_read_only: false,
            },
            LayoutStyle {
                width: Length::Px(420.0),
                height: Length::Px(280.0),
                ..Default::default()
            },
        )
        .unwrap();
    frame(&mut ui);
    ui.runtime.apply(Mutation::Focus(Some(id))).unwrap();
    key(&mut ui, Key::ArrowDown);
    key(&mut ui, Key::ArrowRight);
    frame(&mut ui);
    let b = ui
        .grid_frame(id)
        .unwrap()
        .columns
        .iter()
        .find(|c| c.id == ColumnId(1))
        .unwrap();
    assert!(b.clip.width > 370.0);
    let bounds = ui.runtime.bounds(id).unwrap();
    ui.dispatch(InputEvent::Wheel {
        position: Point::new(bounds.x + 10.0, bounds.y + 50.0),
        delta: Point::new(0.0, 150.0),
        modifiers: Modifiers {
            shift: true,
            ..Default::default()
        },
    })
    .unwrap();
    frame(&mut ui);
    assert!(
        ui.grid_frame(id)
            .unwrap()
            .columns
            .iter()
            .any(|c| c.id == ColumnId(0) && c.clip.width > 100.0)
    );
}

#[test]
fn grid_clipped_parent_hides_editor_without_accepting_draft() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let group = ui
        .expander(
            ui.root(),
            GroupSettings::default(),
            true,
            LayoutStyle::default(),
        )
        .unwrap();
    let source = source(3);
    let id = ui
        .grid_control(
            group.content,
            Rc::clone(&source),
            GridControlSettings::default(),
            LayoutStyle::default(),
        )
        .unwrap();
    frame(&mut ui);
    ui.begin_grid_edit(id, -10, "name").unwrap();
    let editor = ui.grid_editor(id).unwrap();
    ui.set_text(editor, "Draft").unwrap();
    ui.set_expander_expanded(group.container, false).unwrap();
    frame(&mut ui);
    assert_eq!(ui.document(editor).unwrap().text(), "Draft");
    assert_eq!(
        source.borrow().value(-10, "name"),
        Some(&CellValue::Text("Row 000000".into()))
    );
    ui.set_expander_expanded(group.container, true).unwrap();
    frame(&mut ui);
    assert_eq!(ui.grid_editor(id), Some(editor));
}

#[test]
fn grid_filter_hides_selection_without_changing_its_stable_key() {
    let (mut ui, id, _) = setup(10);
    ui.set_grid_selected_key(id, Some(-8)).unwrap();
    ui.set_grid_filter(id, "Row 000009").unwrap();
    frame(&mut ui);
    assert_eq!(ui.grid_selected_key(id), Some(-8));
    assert!(
        ui.grid_frame(id)
            .unwrap()
            .semantics
            .rows
            .iter()
            .all(|row| !row.selected)
    );
    ui.set_grid_selected_key(id, Some(-7)).unwrap();
    assert_eq!(ui.grid_selected_key(id), Some(-7));
    ui.set_grid_selected_key(id, Some(-8)).unwrap();
    ui.set_grid_filter(id, "").unwrap();
    frame(&mut ui);
    assert_eq!(ui.grid_selected_key(id), Some(-8));
    assert!(
        ui.grid_frame(id)
            .unwrap()
            .semantics
            .rows
            .iter()
            .any(|row| row.selected && row.key == RowKey((-8_i64) as u64))
    );
    ui.begin_grid_edit(id, -8, "name").unwrap();
    let editor = ui.grid_editor(id).unwrap();
    ui.set_text(editor, "Draft").unwrap();
    ui.set_grid_selected_key(id, Some(-8)).unwrap();
    assert_eq!(ui.document(editor).unwrap().text(), "Draft");
}

#[test]
fn grid_filter_preserves_live_source_sort_on_a_non_displayed_field() {
    use rust_desktop_ui_data::{DataQuery, SortDescriptor, SortDirection};
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let source = source(10);
    let id = ui
        .grid_control(
            ui.root(),
            Rc::clone(&source),
            GridControlSettings {
                columns: vec![GridControlColumn::new("name", "Name", 200.0)],
                ..Default::default()
            },
            LayoutStyle::default(),
        )
        .unwrap();
    frame(&mut ui);
    let sort = vec![SortDescriptor {
        field: "amount".into(),
        direction: SortDirection::Descending,
    }];
    source
        .borrow_mut()
        .set_query(DataQuery {
            sort: sort.clone(),
            filter: String::new(),
        })
        .unwrap();
    // Deliberately no Controls update before changing the filter.
    ui.set_grid_filter(id, "Row").unwrap();
    assert_eq!(source.borrow().query().sort, sort);
    assert_eq!(source.borrow().row_key(0), Some(-1));
}

#[test]
fn grid_rejects_more_than_64_auto_columns_before_mutating_the_tree() {
    let schema = TableSchema::new(
        (0..65)
            .map(|i| FieldSchema::new(format!("F{i}"), FieldType::Text))
            .collect(),
    )
    .unwrap();
    let source = Rc::new(RefCell::new(
        DataSource::new(CollectionAdapter::new(
            DataTable::new(schema, Vec::new()).unwrap(),
        ))
        .unwrap(),
    ));
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let count = ui.runtime.tree().preorder().len();
    assert!(
        ui.grid_control(
            ui.root(),
            source,
            GridControlSettings::default(),
            LayoutStyle::default()
        )
        .is_err()
    );
    assert_eq!(ui.runtime.tree().preorder().len(), count);
}

#[test]
fn grid_height_binding_null_restores_the_320_pixel_default() {
    use crate::{BindingContext, BindingProperty, ControlBinding, ControlBindings, DataValue};
    let (mut ui, id, _) = setup(3);
    let context = Rc::new(RefCell::new(
        BindingContext::new(DataValue::object([("Height", DataValue::Null)])).unwrap(),
    ));
    let mut bindings = ControlBindings::new(Rc::clone(&context));
    bindings
        .add(ControlBinding::new(id, BindingProperty::Height, "Height").unwrap())
        .unwrap();
    bindings.initialize(&mut ui).unwrap();
    frame(&mut ui);
    assert!((ui.runtime.bounds(id).unwrap().height - 320.0).abs() < 0.1);
    context
        .borrow_mut()
        .set("Height", DataValue::Number(100.0))
        .unwrap();
    bindings.sync(&mut ui);
    frame(&mut ui);
    assert!((ui.runtime.bounds(id).unwrap().height - 100.0).abs() < 0.1);
    context.borrow_mut().set("Height", DataValue::Null).unwrap();
    bindings.sync(&mut ui);
    frame(&mut ui);
    assert!((ui.runtime.bounds(id).unwrap().height - 320.0).abs() < 0.1);
}

#[test]
fn grid_source_deletion_clears_selected_key_only_after_draft_is_finished() {
    use rust_desktop_ui_data::{DataAdapter, DataError};
    struct SharedTable(Rc<RefCell<DataTable>>);
    impl DataAdapter for SharedTable {
        fn load(&mut self) -> Result<DataTable, DataError> {
            Ok(self.0.borrow().clone())
        }
        fn write_cell(
            &mut self,
            _: i64,
            _: &str,
            _: &CellValue,
            _: &CellValue,
        ) -> Result<DataTable, DataError> {
            Err(DataError::Conflict("Deleted".into()))
        }
    }
    let table = Rc::new(RefCell::new(source(3).borrow().table().clone()));
    let source = Rc::new(RefCell::new(
        DataSource::new(SharedTable(Rc::clone(&table))).unwrap(),
    ));
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let id = ui
        .grid_control(
            ui.root(),
            Rc::clone(&source),
            GridControlSettings::default(),
            LayoutStyle::default(),
        )
        .unwrap();
    frame(&mut ui);
    ui.begin_grid_edit(id, -10, "name").unwrap();
    let editor = ui.grid_editor(id).unwrap();
    ui.set_text(editor, "Draft").unwrap();
    let schema = table.borrow().schema().clone();
    *table.borrow_mut() = DataTable::new(schema, Vec::new()).unwrap();
    source.borrow_mut().refresh().unwrap();
    frame(&mut ui);
    assert_eq!(ui.grid_selected_key(id), Some(-10));
    assert_eq!(ui.document(editor).unwrap().text(), "Draft");
    assert!(!ui.commit_grid_edit(id).unwrap());
    ui.cancel_grid_edit(id).unwrap();
    frame(&mut ui);
    assert_eq!(ui.grid_selected_key(id), None);
    assert_eq!(ui.grid_frame(id).unwrap().stats.logical_rows, 0);
}

fn observable_grid(
    count: usize,
) -> (
    Controls,
    WidgetId,
    DataSourceHandle,
    rust_desktop_ui_data::CollectionHandle,
) {
    let table = source(count).borrow().table().clone();
    let adapter = CollectionAdapter::new(table);
    let collection = adapter.handle();
    let source = Rc::new(RefCell::new(DataSource::new(adapter).unwrap()));
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let id = ui
        .grid_control(
            ui.root(),
            Rc::clone(&source),
            GridControlSettings::default(),
            LayoutStyle {
                width: Length::Px(420.0),
                height: Length::Px(280.0),
                ..Default::default()
            },
        )
        .unwrap();
    frame(&mut ui);
    (ui, id, source, collection)
}

#[test]
fn observable_grid_adopts_add_remove_move_and_field_changes_without_refresh() {
    let (mut ui, id, source, collection) = observable_grid(8);
    collection
        .add(DataRow::new(
            1000,
            vec![CellValue::Text("Added".into()), CellValue::Integer(1000)],
        ))
        .unwrap();
    frame(&mut ui);
    assert_eq!(source.borrow().len(), 9);
    assert_eq!(ui.grid_frame(id).unwrap().stats.logical_rows, 9);
    collection.move_row(1000, 0).unwrap();
    frame(&mut ui);
    assert_eq!(source.borrow().row_key(0), Some(1000));
    ui.grid_focus_cell(id, 1000, Some("name")).unwrap();
    collection
        .update_cell(1000, "name", CellValue::Text("Changed".into()))
        .unwrap();
    frame(&mut ui);
    assert_eq!(ui.grid_selected_key(id), Some(1000));
    assert!(
        ui.grid_frame(id)
            .unwrap()
            .semantics
            .rows
            .iter()
            .any(|row| row.cells.iter().any(|cell| cell.label == "Changed"))
    );
    collection.remove(1000).unwrap();
    frame(&mut ui);
    assert_eq!(ui.grid_selected_key(id), None);
    assert_eq!(ui.grid_frame(id).unwrap().stats.logical_rows, 8);
}

#[test]
fn observable_grid_keeps_top_row_identity_fractional_offset_and_offscreen_selection() {
    let (mut ui, id, _, collection) = observable_grid(1000);
    ui.set_grid_selected_key(id, Some(-5)).unwrap();
    frame(&mut ui);
    ui.grid_scroll_to(id, 0.0, 500.0 * 28.0 + 7.0).unwrap();
    frame(&mut ui);
    let first = ui.grid_frame(id).unwrap().semantics.rows[0].clone();
    collection
        .add(DataRow::new(
            1000,
            vec![CellValue::Text("Inserted".into()), CellValue::Integer(0)],
        ))
        .unwrap();
    collection.move_row(1000, 0).unwrap();
    frame(&mut ui);
    let current = &ui.grid_frame(id).unwrap().semantics.rows[0];
    assert_eq!(current.key, first.key);
    assert_eq!(current.bounds, first.bounds);
    assert_eq!(ui.grid_selected_key(id), Some(-5));
    collection.remove(1000).unwrap();
    frame(&mut ui);
    assert_eq!(ui.grid_frame(id).unwrap().semantics.rows[0].key, first.key);
    collection.move_row(first.key.0 as i64, 800).unwrap();
    frame(&mut ui);
    assert_eq!(ui.grid_frame(id).unwrap().semantics.rows[0].key, first.key);
    assert_eq!(
        ui.grid_frame(id).unwrap().semantics.rows[0].bounds,
        first.bounds
    );
    let following = ui.grid_frame(id).unwrap().semantics.rows[1].key;
    collection.remove(first.key.0 as i64).unwrap();
    frame(&mut ui);
    assert_eq!(ui.grid_frame(id).unwrap().semantics.rows[0].key, following);
    assert!(ui.grid_frame(id).unwrap().stats.materialized_rows < 20);
}

#[test]
fn observable_grid_shared_source_updates_other_view_while_draft_stays_frozen() {
    let (mut ui, id, source, collection) = observable_grid(8);
    let other = ui
        .grid_control(
            ui.root(),
            Rc::clone(&source),
            GridControlSettings::default(),
            LayoutStyle {
                width: Length::Px(420.0),
                height: Length::Px(120.0),
                ..Default::default()
            },
        )
        .unwrap();
    frame(&mut ui);
    ui.begin_grid_edit(id, -10, "name").unwrap();
    let editor = ui.grid_editor(id).unwrap();
    ui.set_text(editor, "Draft").unwrap();
    collection
        .update_cell(-10, "name", CellValue::Text("External".into()))
        .unwrap();
    collection
        .add(DataRow::new(
            1000,
            vec![CellValue::Text("Added".into()), CellValue::Integer(9)],
        ))
        .unwrap();
    frame(&mut ui);
    assert_eq!(ui.grid_frame(id).unwrap().stats.logical_rows, 8);
    assert_eq!(ui.grid_frame(other).unwrap().stats.logical_rows, 9);
    assert!(
        ui.grid_frame(other)
            .unwrap()
            .semantics
            .rows
            .iter()
            .any(|row| row.cells.iter().any(|cell| cell.label == "External"))
    );
    assert_eq!(ui.document(editor).unwrap().text(), "Draft");
    assert!(!ui.commit_grid_edit(id).unwrap());
    assert_eq!(ui.document(editor).unwrap().text(), "Draft");
    ui.cancel_grid_edit(id).unwrap();
    frame(&mut ui);
    assert_eq!(ui.grid_frame(id).unwrap().stats.logical_rows, 9);
    assert!(
        ui.grid_frame(id)
            .unwrap()
            .semantics
            .rows
            .iter()
            .any(|row| row.cells.iter().any(|cell| cell.label == "External"))
    );
}

#[test]
fn observable_grid_unrelated_mutation_preserves_commit_and_move_keeps_row_identity() {
    let (mut ui, id, source, collection) = observable_grid(20);
    ui.begin_grid_edit(id, -10, "name").unwrap();
    let editor = ui.grid_editor(id).unwrap();
    ui.set_text(editor, "Accepted").unwrap();
    collection
        .update_cell(-9, "amount", CellValue::Integer(700))
        .unwrap();
    collection.move_row(-10, 12).unwrap();
    frame(&mut ui);
    assert_eq!(ui.document(editor).unwrap().text(), "Accepted");
    assert!(ui.commit_grid_edit(id).unwrap());
    frame(&mut ui);
    assert_eq!(
        source.borrow().value(-10, "name"),
        Some(&CellValue::Text("Accepted".into()))
    );
    assert_eq!(source.borrow().position(-10), Some(12));
    assert_eq!(
        source.borrow().value(-9, "amount"),
        Some(&CellValue::Integer(700))
    );
    assert_eq!(ui.grid_selected_key(id), Some(-10));
}

#[test]
fn observable_grid_deleted_edit_target_keeps_draft_until_cancel_without_refresh() {
    let (mut ui, id, _, collection) = observable_grid(8);
    ui.begin_grid_edit(id, -10, "amount").unwrap();
    let editor = ui.grid_editor(id).unwrap();
    ui.set_text(editor, "77").unwrap();
    collection.remove(-10).unwrap();
    frame(&mut ui);
    assert_eq!(ui.grid_selected_key(id), Some(-10));
    assert_eq!(ui.document(editor).unwrap().text(), "77");
    assert!(!ui.commit_grid_edit(id).unwrap());
    ui.cancel_grid_edit(id).unwrap();
    frame(&mut ui);
    assert_eq!(ui.grid_selected_key(id), None);
    assert_eq!(ui.grid_frame(id).unwrap().stats.logical_rows, 7);
}

#[test]
fn observable_grid_accepts_new_selected_key_before_first_layout() {
    let adapter = CollectionAdapter::new(source(2).borrow().table().clone());
    let collection = adapter.handle();
    let source = Rc::new(RefCell::new(DataSource::new(adapter).unwrap()));
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let id = ui
        .grid_control(
            ui.root(),
            source,
            GridControlSettings::default(),
            LayoutStyle::default(),
        )
        .unwrap();
    collection
        .add(DataRow::new(
            1000,
            vec![CellValue::Text("Added".into()), CellValue::Integer(0)],
        ))
        .unwrap();
    ui.set_grid_selected_key(id, Some(1000)).unwrap();
    frame(&mut ui);
    assert_eq!(ui.grid_selected_key(id), Some(1000));
    assert!(
        ui.grid_frame(id)
            .unwrap()
            .semantics
            .rows
            .iter()
            .any(|row| row.selected && row.key == RowKey(1000))
    );
}
