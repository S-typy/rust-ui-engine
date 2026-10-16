use super::*;
use rust_desktop_ui_core::Modifiers;
use rust_desktop_ui_treegrid::{
    ColumnId, GridColumn, GridHitKind, IndexedTreeSource, RowKey, TreeChildren, TreeDataSource,
    TreeMoveTarget, TreeNode,
};
use std::{cell::RefCell, rc::Rc};

#[test]
fn single_default_multiple_opt_in_and_validation_restore_focus_scroll_and_events() {
    let (mut ui, id, _) = setup(false);
    ui.set_tree_selected_key(id, Some(RowKey(1))).unwrap();
    ui.runtime.apply(Mutation::Focus(Some(id))).unwrap();
    key(&mut ui, Key::ArrowDown, true);
    assert_eq!(ui.tree_selected_keys(id).unwrap(), [RowKey(2)]);
    ui.set_tree_selected_key(id, Some(RowKey(1))).unwrap();
    ui.take_tree_selection_changes();
    let scroll = ui.tree_model(id).unwrap().scroll_offset();
    ui.set_tree_selection_validator(
        id,
        Some(SelectionValidator::new(|keys| {
            if keys.iter().any(|key| key == "2") {
                Err("Node 2 is restricted".into())
            } else {
                Ok(())
            }
        })),
    )
    .unwrap();
    key(&mut ui, Key::ArrowDown, false);
    assert_eq!(ui.tree_selected_keys(id).unwrap(), [RowKey(1)]);
    assert_eq!(ui.tree_model(id).unwrap().focused(), Some(RowKey(1)));
    assert_eq!(ui.tree_model(id).unwrap().scroll_offset(), scroll);
    assert!(ui.take_tree_selection_changes().is_empty());
    assert!(ui.tree_error(id).unwrap().contains("restricted"));
    assert!(
        ui.tree_perform_action(id, RowKey(2), None, SemanticAction::Focus)
            .is_err()
    );
    assert!(ui.set_tree_selected_key(id, Some(RowKey(2))).is_err());
    ui.set_tree_selection_validator(id, None).unwrap();
    ui.trees.hosts.get_mut(&id).unwrap().settings.selection_mode = SelectionMode::Multiple;
    let p = point(&ui, id, 2);
    ui.dispatch(InputEvent::PointerDown {
        position: p,
        button: PointerButton::Primary,
        modifiers: Modifiers {
            control: true,
            ..Default::default()
        },
    })
    .unwrap();
    assert_eq!(ui.tree_selected_keys(id).unwrap(), [RowKey(1), RowKey(2)]);
}

#[test]
fn selected_key_binding_preserves_exact_u64_hidden_keys_and_rejects_invalid_updates() {
    use crate::{
        BindingContext, BindingProperty, BindingValidator, ControlBinding, ControlBindingMode,
        ControlBindings, DataValue,
    };
    let (mut ui, id, source) = setup(false);
    let mut records = source.borrow().nodes().to_vec();
    records.push(node(u64::MAX, None));
    source.borrow_mut().replace(records).unwrap();
    frame(&mut ui);
    let model = Rc::new(RefCell::new(
        BindingContext::new(DataValue::object([("Key", "3".into())])).unwrap(),
    ));
    let mut bindings = ControlBindings::new(Rc::clone(&model));
    let mut binding = ControlBinding::new(id, BindingProperty::SelectedKey, "Key").unwrap();
    binding.mode = ControlBindingMode::TwoWay;
    bindings.add(binding).unwrap();
    bindings.initialize(&mut ui).unwrap();
    assert_eq!(ui.tree_selected_keys(id).unwrap(), [RowKey(3)]);
    assert!(!ui.tree_model(id).unwrap().is_expanded(RowKey(1)));
    model
        .borrow_mut()
        .set("Key", u64::MAX.to_string().into())
        .unwrap();
    bindings.sync(&mut ui);
    assert_eq!(ui.tree_selected_keys(id).unwrap(), [RowKey(u64::MAX)]);
    for invalid in ["01", "+1", "1.0", "999999999999999999999", "999"] {
        model.borrow_mut().set("Key", invalid.into()).unwrap();
        bindings.sync(&mut ui);
        assert_eq!(ui.tree_selected_keys(id).unwrap(), [RowKey(u64::MAX)]);
    }
    model.borrow_mut().set("Key", "1".into()).unwrap();
    bindings.sync(&mut ui);
    model
        .borrow_mut()
        .set_field_validator(
            "Key",
            Some(BindingValidator::new(|value| {
                if *value == DataValue::from("2") {
                    Err("Restricted".into())
                } else {
                    Ok(())
                }
            })),
        )
        .unwrap();
    ui.runtime.apply(Mutation::Focus(Some(id))).unwrap();
    key(&mut ui, Key::ArrowDown, false);
    bindings.sync(&mut ui);
    assert_eq!(model.borrow().get("Key").unwrap(), &DataValue::from("1"));
    assert_eq!(ui.tree_selected_keys(id).unwrap(), [RowKey(1)]);
    ui.set_tree_selected_key(id, None).unwrap();
    bindings.sync(&mut ui);
    assert_eq!(model.borrow().get("Key").unwrap(), &DataValue::Null);
}
fn node(key: u64, parent: Option<u64>) -> TreeNode {
    let mut node = TreeNode::new(RowKey(key), parent.map(RowKey), format!("Node {key}"));
    node.cells.insert(ColumnId(1), format!("Value {key}"));
    node
}

#[test]
fn external_source_borrow_rejects_update_input_and_writes_before_tree_mutation() {
    let (mut ui, id, source) = setup(false);
    ui.set_tree_selected_key(id, Some(RowKey(1))).unwrap();
    let p = point(&ui, id, 2);
    let scroll = ui.tree_model(id).unwrap().scroll_offset();
    let borrowed = source.borrow_mut();
    assert!(ui.update(Size::new(640.0, 480.0)).is_err());
    assert!(
        ui.dispatch(InputEvent::PointerDown {
            position: p,
            button: PointerButton::Primary,
            modifiers: Modifiers::default()
        })
        .is_err()
    );
    assert!(ui.tree_refresh(id).is_err());
    assert!(ui.tree_set_expanded(id, RowKey(1), true).is_err());
    assert!(
        ui.tree_move_nodes(id, &[RowKey(2)], TreeMoveTarget::Before(RowKey(1)))
            .is_err()
    );
    assert!(ui.set_tree_selected_key(id, Some(RowKey(2))).is_err());
    assert_eq!(ui.tree_selected_keys(id).unwrap(), [RowKey(1)]);
    assert_eq!(ui.tree_model(id).unwrap().scroll_offset(), scroll);
    assert!(!ui.tree_model(id).unwrap().is_expanded(RowKey(1)));
    assert!(ui.runtime.captured().is_none());
    drop(borrowed);
    frame(&mut ui);
    ui.set_tree_selected_key(id, Some(RowKey(2))).unwrap();
}

#[test]
fn stale_lazy_locations_cannot_validate_deleted_keys_before_update() {
    let mut unloaded = node(1, None);
    unloaded.children = TreeChildren::Unloaded;
    let source = Rc::new(RefCell::new(
        IndexedTreeSource::new(vec![unloaded]).unwrap(),
    ));
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let id = ui
        .tree_view(
            ui.root(),
            TreeSourceHandle::from_shared(Rc::clone(&source)),
            ColumnId(0),
            LayoutStyle::default(),
        )
        .unwrap();
    let request = ui.tree_set_expanded(id, RowKey(1), true).unwrap().unwrap();
    ui.tree_complete_indexed_children(id, &source, request, vec![node(2, Some(1))])
        .unwrap();
    ui.set_tree_selected_key(id, Some(RowKey(2))).unwrap();
    source.borrow_mut().replace(vec![node(1, None)]).unwrap();
    // Assignment and expansion must consult the new revision, not cached lazy keys.
    assert!(ui.set_tree_selected_key(id, Some(RowKey(2))).is_err());
    assert!(ui.tree_set_expanded(id, RowKey(2), true).is_err());
    // Refresh intentionally preserves application-owned selection until cleared.
    assert_eq!(ui.tree_selected_keys(id).unwrap(), [RowKey(2)]);
    ui.set_tree_selected_key(id, None).unwrap();
    assert!(ui.tree_selected_keys(id).unwrap().is_empty());
}
fn frame(ui: &mut Controls) {
    ui.update(Size::new(640.0, 480.0)).unwrap();
}
fn setup(header: bool) -> (Controls, WidgetId, Rc<RefCell<IndexedTreeSource>>) {
    let source = Rc::new(RefCell::new(
        IndexedTreeSource::new(vec![
            node(1, None),
            node(2, None),
            node(3, Some(1)),
            node(4, Some(1)),
            node(5, None),
        ])
        .unwrap(),
    ));
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let columns = if header {
        vec![
            GridColumn::new(ColumnId(0), "Name", 220.0),
            GridColumn::new(ColumnId(1), "Value", 160.0),
        ]
    } else {
        vec![GridColumn::new(ColumnId(0), "", 240.0)]
    };
    let id = ui
        .tree_grid(
            ui.root(),
            TreeSourceHandle::from_shared(Rc::clone(&source)),
            TreeControlSettings {
                columns,
                show_header: header,
                selection_mode: SelectionMode::Single,
                allow_node_drag: true,
                read_only: false,
            },
            LayoutStyle {
                width: Length::Px(400.0),
                height: Length::Px(250.0),
                ..Default::default()
            },
        )
        .unwrap();
    frame(&mut ui);
    (ui, id, source)
}
fn key(ui: &mut Controls, key: Key, control: bool) {
    ui.dispatch(InputEvent::KeyDown {
        key,
        modifiers: Modifiers {
            control,
            ..Default::default()
        },
        repeat: false,
    })
    .unwrap();
    frame(ui);
}
fn point(ui: &Controls, id: WidgetId, row: u64) -> Point {
    let bounds = ui
        .tree_frame(id)
        .unwrap()
        .cell_bounds(RowKey(row), ColumnId(0))
        .unwrap();
    Point::new(bounds.x + 100.0, bounds.y + bounds.height / 2.0)
}
fn down(ui: &mut Controls, p: Point) {
    ui.dispatch(InputEvent::PointerDown {
        position: p,
        button: PointerButton::Primary,
        modifiers: Modifiers::default(),
    })
    .unwrap();
}
fn movement(ui: &mut Controls, p: Point) {
    ui.dispatch(InputEvent::PointerMoved { position: p })
        .unwrap();
    frame(ui);
}
fn up(ui: &mut Controls, p: Point) {
    ui.dispatch(InputEvent::PointerUp {
        position: p,
        button: PointerButton::Primary,
        modifiers: Modifiers::default(),
    })
    .unwrap();
    frame(ui);
}

#[test]
fn tree_and_grid_share_hierarchy_keyboard_and_expose_bounded_row_cell_semantics() {
    for header in [false, true] {
        let (mut ui, id, _) = setup(header);
        ui.perform_action(id, SemanticAction::Focus, None).unwrap();
        key(&mut ui, Key::Home, false);
        key(&mut ui, Key::ArrowRight, false);
        assert!(ui.tree_model(id).unwrap().is_expanded(RowKey(1)));
        key(&mut ui, Key::ArrowRight, false);
        assert_eq!(ui.tree_selected_keys(id).unwrap(), [RowKey(3)]);
        if header {
            key(&mut ui, Key::ArrowRight, true);
            assert_eq!(
                ui.tree_model(id).unwrap().focused_column(),
                Some(ColumnId(1))
            );
        }
        key(&mut ui, Key::ArrowLeft, false);
        assert_eq!(ui.tree_selected_keys(id).unwrap(), [RowKey(1)]);
        let frame = ui.tree_frame(id).unwrap();
        assert_eq!(frame.semantics.column_count, if header { 2 } else { 1 });
        assert!(
            frame
                .semantics
                .rows
                .iter()
                .any(|r| r.key == RowKey(3) && r.depth == 1)
        );
        assert_eq!(
            ui.semantics().iter().find(|n| n.id == id).unwrap().role,
            if header { Role::Grid } else { Role::Tree }
        );
        assert_eq!(
            frame
                .hits
                .iter()
                .any(|h| matches!(h.kind, GridHitKind::Header(_))),
            header
        );
        ui.tree_perform_action(id, RowKey(1), None, SemanticAction::Collapse)
            .unwrap();
        frame_ui(&mut ui);
        assert!(!ui.tree_model(id).unwrap().is_expanded(RowKey(1)));
        assert!(
            ui.tree_perform_action(id, RowKey(999), None, SemanticAction::Activate)
                .is_err()
        );
    }
}
fn frame_ui(ui: &mut Controls) {
    frame(ui);
}
#[test]
fn node_drag_moves_only_after_threshold_and_current_valid_release_target() {
    for header in [false, true] {
        let (mut ui, id, source) = setup(header);
        let first = point(&ui, id, 1);
        let second = point(&ui, id, 2);
        down(&mut ui, first);
        movement(&mut ui, Point::new(first.x + 2.0, first.y));
        up(&mut ui, second);
        assert_eq!(source.borrow().parent(RowKey(1)), None);
        down(&mut ui, first);
        movement(&mut ui, second);
        assert!(ui.trees.hosts[&id].drag.as_ref().unwrap().target.is_some());
        up(&mut ui, Point::new(630.0, 470.0));
        assert_eq!(
            source.borrow().parent(RowKey(1)),
            None,
            "release outside must not reuse previous target"
        );
        down(&mut ui, first);
        movement(&mut ui, second);
        up(&mut ui, second);
        assert_eq!(source.borrow().parent(RowKey(1)), Some(RowKey(2)));
        assert_eq!(ui.runtime.captured(), None);
        assert_eq!(ui.tree_selected_keys(id).unwrap(), [RowKey(1)]);
    }
}
#[test]
fn drag_cancel_escape_capture_loss_readonly_and_external_source_revision_are_safe() {
    let (mut ui, id, source) = setup(true);
    for cancel in 0..4 {
        let first = point(&ui, id, 1);
        let second = point(&ui, id, 2);
        down(&mut ui, first);
        movement(&mut ui, second);
        match cancel {
            0 => key(&mut ui, Key::Escape, false),
            1 => {
                ui.cancel_pointer_interaction().unwrap();
            }
            2 => {
                ui.set_tree_interaction(id, true, true).unwrap();
            }
            _ => {
                let mut nodes = source.borrow().nodes().to_vec();
                nodes[0].cells.insert(ColumnId(0), "Renamed".into());
                source.borrow_mut().replace(nodes).unwrap();
                frame(&mut ui);
            }
        }
        up(&mut ui, second);
        assert_eq!(source.borrow().parent(RowKey(1)), None);
        assert_eq!(ui.runtime.captured(), None);
        ui.set_tree_interaction(id, true, false).unwrap();
    }
}
#[test]
fn invalid_descendant_drop_preserves_graph_selection_and_accepted_source() {
    let (mut ui, id, source) = setup(true);
    ui.tree_set_expanded(id, RowKey(1), true).unwrap();
    frame(&mut ui);
    let first = point(&ui, id, 1);
    let child = point(&ui, id, 3);
    down(&mut ui, first);
    movement(&mut ui, child);
    assert!(ui.trees.hosts[&id].drag.as_ref().unwrap().target.is_none());
    up(&mut ui, child);
    assert_eq!(source.borrow().parent(RowKey(1)), None);
    assert_eq!(source.borrow().parent(RowKey(3)), Some(RowKey(1)));
    assert_eq!(source.borrow().revision(), 0);
    assert!(
        ui.tree_move_nodes(id, &[RowKey(1)], TreeMoveTarget::Into(Some(RowKey(3))))
            .is_err()
    );
}
#[test]
fn indexed_children_completion_checks_owner_source_and_cancel_before_mutation() {
    let mut parent = node(1, None);
    parent.children = TreeChildren::Unloaded;
    let source = Rc::new(RefCell::new(IndexedTreeSource::new(vec![parent]).unwrap()));
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let id = ui
        .tree_view(
            ui.root(),
            TreeSourceHandle::from_shared(Rc::clone(&source)),
            ColumnId(0),
            LayoutStyle {
                width: Length::Px(400.0),
                height: Length::Px(250.0),
                ..Default::default()
            },
        )
        .unwrap();
    frame(&mut ui);
    let old = ui.tree_set_expanded(id, RowKey(1), true).unwrap().unwrap();
    ui.tree_set_expanded(id, RowKey(1), false).unwrap();
    assert!(
        !ui.tree_complete_indexed_children(id, &source, old, vec![node(3, Some(1))])
            .unwrap()
    );
    assert_eq!(source.borrow().revision(), 0);
    let request = ui.tree_set_expanded(id, RowKey(1), true).unwrap().unwrap();
    let foreign = Rc::new(RefCell::new(source.borrow().clone()));
    assert!(
        ui.tree_complete_indexed_children(id, &foreign, request, vec![node(3, Some(1))])
            .is_err()
    );
    assert!(
        ui.tree_complete_indexed_children(id, &source, request, vec![node(3, Some(1))])
            .unwrap()
    );
    frame(&mut ui);
    assert_eq!(ui.tree_frame(id).unwrap().semantics.rows.len(), 2);
    ui.remove(id).unwrap();
    assert!(!ui.trees.hosts.contains_key(&id));
}
#[test]
fn visible_text_failure_during_construction_creates_no_host_or_orphan() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let before = ui.controls.len();
    let source = IndexedTreeSource::new(vec![TreeNode::new(
        RowKey(1),
        None,
        format!("A{}", "\u{301}".repeat(40)),
    )])
    .unwrap();
    assert!(
        ui.tree_view(
            ui.root(),
            TreeSourceHandle::new(source),
            ColumnId(0),
            Default::default()
        )
        .is_err()
    );
    assert_eq!(ui.controls.len(), before);
    assert!(ui.trees.hosts.is_empty());
}

struct MillionRows;
impl TreeDataSource for MillionRows {
    fn child_count(&self, parent: Option<RowKey>) -> Option<usize> {
        Some(if parent.is_none() { 1_000_000 } else { 0 })
    }
    fn child_at(&self, parent: Option<RowKey>, index: usize) -> Option<RowKey> {
        (parent.is_none() && index < 1_000_000).then_some(RowKey(index as u64))
    }
    fn parent(&self, _: RowKey) -> Option<RowKey> {
        None
    }
    fn position(&self, row: RowKey) -> Option<usize> {
        (row.0 < 1_000_000).then_some(row.0 as usize)
    }
    fn cell(&self, row: RowKey, _: ColumnId) -> String {
        format!("Row {}", row.0)
    }
}
#[test]
fn million_row_tree_materializes_only_viewport_and_scrollbar_reaches_final_key() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let id = ui
        .tree_view(
            ui.root(),
            TreeSourceHandle::new(MillionRows),
            ColumnId(0),
            LayoutStyle {
                width: Length::Px(400.0),
                height: Length::Px(250.0),
                ..Default::default()
            },
        )
        .unwrap();
    frame(&mut ui);
    let count = ui.controls.len();
    assert!(count < 10);
    for index in [0, 100_000, 900_000, 999_900, 50] {
        ui.tree_scroll_to(id, 0.0, index as f64 * 28.0 + 3.0)
            .unwrap();
        frame(&mut ui);
        assert_eq!(ui.controls.len(), count);
        let frame = ui.tree_frame(id).unwrap();
        assert!(frame.stats.materialized_rows <= 14);
        assert!(frame.semantics.rows.len() <= 10);
        let row = &frame.semantics.rows[0];
        assert_eq!(row.set_size, 1_000_000);
        assert_eq!(row.position_in_set, row.key.0 as usize + 1);
    }
    let bar = ui.trees.hosts[&id].scroll.bars[0];
    let end = Point::new(bar.track.x + 6.0, bar.track.y + bar.track.height - 1.0);
    down(&mut ui, end);
    up(&mut ui, end);
    assert!(
        ui.tree_frame(id)
            .unwrap()
            .semantics
            .rows
            .iter()
            .any(|r| r.key == RowKey(999_999))
    );
    let widths: Vec<_> = ui
        .tree_frame(id)
        .unwrap()
        .columns
        .iter()
        .map(|c| c.bounds.width)
        .collect();
    assert_eq!(widths, [388.0]);
}

#[test]
fn tree_column_resize_pin_reorder_and_semantic_action_share_display_geometry() {
    let (mut ui, id, _) = setup(true);
    ui.tree_resize_column(id, ColumnId(0), 300.0).unwrap();
    ui.tree_reorder_column(id, ColumnId(1), Some(ColumnId(0)))
        .unwrap();
    ui.tree_pin_column(id, ColumnId(1), true).unwrap();
    frame(&mut ui);
    assert!(
        ui.tree_frame(id)
            .unwrap()
            .columns
            .iter()
            .any(|c| c.id == ColumnId(1) && c.pinned)
    );
    ui.tree_scroll_to(id, 60.0, 0.0).unwrap();
    frame(&mut ui);
    let pinned = ui
        .tree_frame(id)
        .unwrap()
        .columns
        .iter()
        .find(|c| c.id == ColumnId(1))
        .unwrap();
    assert_eq!(pinned.bounds.x, ui.runtime.bounds(id).unwrap().x);
    ui.tree_perform_action(id, RowKey(2), Some(ColumnId(1)), SemanticAction::Focus)
        .unwrap();
    frame(&mut ui);
    assert_eq!(ui.tree_model(id).unwrap().focused(), Some(RowKey(2)));
    assert_eq!(
        ui.tree_model(id).unwrap().focused_column(),
        Some(ColumnId(1))
    );
    assert_eq!(ui.runtime.focused(), Some(id));
}
