//! Validate real retained/virtual projections at the platform publication boundary.
//! These are deterministic model tests; they do not generate native input.
use crate::{
    accessibility::Accessibility,
    docking_accessibility::{PaneAccessibility, merge_accessibility},
    view::MountedView,
};
use rust_desktop_ui_controls::{Controls, DockPaneId, GridControlSettings, GroupSettings, Theme};
use rust_desktop_ui_core::{LayoutStyle, Length, Point, Rect, Size, WidgetId};
use rust_desktop_ui_data::{
    CellValue, CollectionAdapter, DataRow, DataSource, DataTable, FieldSchema, FieldType,
    TableSchema,
};
use rust_desktop_ui_platform_winit::{
    AccessibilitySnapshot,
    accesskit::{
        Action, ActionData, ActionRequest, Node, NodeId, Role, TreeId, TreeInfo, TreeUpdate,
    },
};
use std::{cell::RefCell, rc::Rc};

const SIZE: Size = Size::new(620.0, 300.0);

fn request(id: NodeId, action: Action) -> ActionRequest {
    ActionRequest {
        target_tree: TreeId::ROOT,
        target_node: id,
        action,
        data: if action == Action::SetValue {
            Some(ActionData::Value("Replacement".into()))
        } else {
            None
        },
    }
}

fn project(ui: &mut Controls, accessibility: &mut Accessibility) -> TreeUpdate {
    ui.update(SIZE).unwrap();
    accessibility.tree(ui, &MountedView::default(), "Foundation accessibility", 1.5)
}

fn node(tree: &TreeUpdate, id: NodeId) -> &Node {
    &tree
        .nodes
        .iter()
        .find(|(candidate, _)| *candidate == id)
        .unwrap()
        .1
}

fn cell(tree: &TreeUpdate, label: &str) -> NodeId {
    tree.nodes
        .iter()
        .find_map(|(id, node)| {
            (node.role() == Role::Cell && node.label() == Some(label)).then_some(*id)
        })
        .unwrap()
}

#[test]
fn retained_structural_parents_collapse_reexpand_and_remove_stay_connected() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let group = ui
        .expander(
            ui.root(),
            GroupSettings {
                header: "Details".into(),
                ..Default::default()
            },
            true,
            LayoutStyle {
                width: Length::Px(400.0),
                height: Length::Px(200.0),
                ..Default::default()
            },
        )
        .unwrap();
    let editor = ui
        .edit_box(
            group.content,
            "Name",
            "Original",
            LayoutStyle {
                height: Length::Px(36.0),
                ..Default::default()
            },
        )
        .unwrap();
    let mut accessibility = Accessibility::default();
    let mut accepted = AccessibilitySnapshot::default();
    let initial = project(&mut ui, &mut accessibility);
    let editor_id = accessibility.id(editor).unwrap();
    accepted.update(initial.clone()).unwrap();
    assert!(accepted.permits(&request(editor_id, Action::SetValue)));
    assert!(accepted.update(initial).unwrap().nodes.is_empty());

    ui.set_expander_expanded(group.container, false).unwrap();
    let collapsed = project(&mut ui, &mut accessibility);
    assert!(!collapsed.nodes.iter().any(|(id, _)| *id == editor_id));
    let delta = accepted.update(collapsed).unwrap();
    assert!(!delta.nodes.is_empty());
    assert!(!accepted.permits(&request(editor_id, Action::SetValue)));
    accessibility
        .action(&mut ui, request(editor_id, Action::SetValue))
        .unwrap();
    assert_eq!(ui.document(editor).unwrap().text(), "Original");

    ui.set_expander_expanded(group.container, true).unwrap();
    let expanded = project(&mut ui, &mut accessibility);
    assert_eq!(accessibility.id(editor), Some(editor_id));
    accepted.update(expanded).unwrap();
    assert!(accepted.permits(&request(editor_id, Action::Focus)));
    ui.remove(group.container).unwrap();
    accepted
        .update(project(&mut ui, &mut accessibility))
        .unwrap();
    assert!(!accepted.permits(&request(editor_id, Action::SetValue)));
    let new_editor = ui
        .edit_box(ui.root(), "New", "New value", LayoutStyle::default())
        .unwrap();
    accepted
        .update(project(&mut ui, &mut accessibility))
        .unwrap();
    assert_ne!(accessibility.id(new_editor), Some(editor_id));
    accessibility
        .action(&mut ui, request(editor_id, Action::SetValue))
        .unwrap();
    assert_eq!(ui.document(new_editor).unwrap().text(), "New value");
}

#[test]
fn disabled_and_read_only_state_changes_reject_old_native_requests() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let editor = ui
        .edit_box(ui.root(), "Name", "Original", LayoutStyle::default())
        .unwrap();
    let mut accessibility = Accessibility::default();
    let mut accepted = AccessibilitySnapshot::default();
    accepted
        .update(project(&mut ui, &mut accessibility))
        .unwrap();
    let id = accessibility.id(editor).unwrap();
    ui.set_read_only(editor, true).unwrap();
    // The host's last snapshot is older than the actual control state.
    assert!(accepted.permits(&request(id, Action::SetValue)));
    accessibility
        .action(&mut ui, request(id, Action::SetValue))
        .unwrap();
    assert_eq!(ui.document(editor).unwrap().text(), "Original");
    let tree = project(&mut ui, &mut accessibility);
    assert!(node(&tree, id).is_read_only());
    accepted.update(tree).unwrap();
    assert!(!accepted.permits(&request(id, Action::SetValue)));
    assert!(accepted.permits(&request(id, Action::Focus)));
    ui.set_enabled(editor, false).unwrap();
    accessibility
        .action(&mut ui, request(id, Action::Focus))
        .unwrap();
    accepted
        .update(project(&mut ui, &mut accessibility))
        .unwrap();
    assert!(!accepted.permits(&request(id, Action::Focus)));
    assert!(!accepted.permits(&request(id, Action::SetValue)));
    assert_eq!(ui.document(editor).unwrap().text(), "Original");
}

fn grid(count: usize) -> (Controls, WidgetId) {
    let schema = TableSchema::new(vec![FieldSchema::new("Name", FieldType::Text)]).unwrap();
    let table = DataTable::new(
        schema,
        (0..count)
            .map(|index| {
                DataRow::new(
                    if index == 0 { i64::MIN } else { index as i64 },
                    vec![CellValue::Text(format!("Record {index}"))],
                )
            })
            .collect(),
    )
    .unwrap();
    let source = Rc::new(RefCell::new(
        DataSource::new(CollectionAdapter::new(table)).unwrap(),
    ));
    let mut ui = Controls::new(Theme::FluentDark).unwrap();
    let widget = ui
        .grid_control(
            ui.root(),
            source,
            GridControlSettings::default(),
            LayoutStyle {
                width: Length::Px(600.0),
                height: Length::Px(220.0),
                ..Default::default()
            },
        )
        .unwrap();
    (ui, widget)
}

#[test]
fn virtual_scroll_keeps_tree_bounded_and_never_retargets_retired_cell_ids() {
    let (mut ui, grid) = grid(10_000);
    let mut accessibility = Accessibility::default();
    let mut accepted = AccessibilitySnapshot::default();
    let first = project(&mut ui, &mut accessibility);
    let old_cell = cell(&first, "Record 0");
    let header = first
        .nodes
        .iter()
        .find_map(|(id, node)| (node.role() == Role::ColumnHeader).then_some(*id))
        .unwrap();
    accepted.update(first.clone()).unwrap();
    assert!(
        accepted
            .update(project(&mut ui, &mut accessibility))
            .unwrap()
            .nodes
            .is_empty()
    );
    for offset in [1000.0, 5000.0, 15000.0, 80000.0] {
        ui.grid_scroll_to(grid, 0.0, offset).unwrap();
        let tree = project(&mut ui, &mut accessibility);
        assert!(tree.nodes.len() < 100);
        assert_eq!(node(&tree, header).role(), Role::ColumnHeader);
        accepted.update(tree).unwrap();
        assert!(!accepted.permits(&request(old_cell, Action::Click)));
        accessibility
            .action(&mut ui, request(old_cell, Action::Click))
            .unwrap();
        assert_eq!(ui.grid_selected_key(grid), None);
    }
    ui.grid_scroll_to(grid, 0.0, 0.0).unwrap();
    let returned = project(&mut ui, &mut accessibility);
    let new_cell = cell(&returned, "Record 0");
    assert_ne!(old_cell, new_cell);
    accepted.update(returned).unwrap();
    assert!(!accepted.permits(&request(old_cell, Action::Click)));
    assert!(accepted.permits(&request(new_cell, Action::Click)));
    accessibility
        .action(&mut ui, request(new_cell, Action::Click))
        .unwrap();
    assert_eq!(ui.grid_selected_key(grid), Some(i64::MIN));
    ui.remove(grid).unwrap();
    accepted
        .update(project(&mut ui, &mut accessibility))
        .unwrap();
    assert!(!accepted.permits(&request(new_cell, Action::SetValue)));
}

fn window(root: u64) -> TreeUpdate {
    TreeUpdate {
        nodes: vec![(NodeId(root), Node::new(Role::Window))],
        tree: Some(TreeInfo::new(NodeId(root))),
        tree_id: TreeId::ROOT,
        focus: NodeId(root),
    }
}

fn pane(tree: TreeUpdate) -> PaneAccessibility {
    PaneAccessibility {
        id: DockPaneId::new("document").unwrap(),
        root: tree.tree.as_ref().unwrap().root,
        offset: Point::new(0.0, 0.0),
        clip: Rect::new(0.0, 0.0, 1000.0, 1000.0),
        tree,
    }
}

#[test]
fn detached_pane_has_one_native_owner_and_keeps_its_semantic_ids() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let editor = ui
        .edit_box(
            ui.root(),
            "Document",
            "Retained draft",
            LayoutStyle::default(),
        )
        .unwrap();
    let mut accessibility = Accessibility::with_base(100);
    let pane_tree = project(&mut ui, &mut accessibility);
    let editor_id = accessibility.id(editor).unwrap();
    let mut main_state = AccessibilitySnapshot::default();
    let mut floating_state = AccessibilitySnapshot::default();
    let (main, owners) = merge_accessibility(window(1), vec![pane(pane_tree.clone())], None);
    main_state.update(main).unwrap();
    assert!(owners.contains_key(&editor_id));
    assert!(main_state.permits(&request(editor_id, Action::SetValue)));
    let (detached, owners) = merge_accessibility(window(1), vec![], None);
    main_state.update(detached).unwrap();
    assert!(owners.is_empty());
    assert!(!main_state.permits(&request(editor_id, Action::SetValue)));
    let (floating, owners) = merge_accessibility(window(10_000), vec![pane(pane_tree)], None);
    floating_state.update(floating).unwrap();
    assert_eq!(
        owners.get(&editor_id),
        Some(&DockPaneId::new("document").unwrap())
    );
    assert!(floating_state.permits(&request(editor_id, Action::SetValue)));
    assert_eq!(accessibility.id(editor), Some(editor_id));
    assert_eq!(ui.document(editor).unwrap().text(), "Retained draft");
    floating_state.update(window(10_000)).unwrap();
    assert!(!floating_state.permits(&request(editor_id, Action::SetValue)));
}

#[test]
fn malformed_pane_relationship_cannot_poison_the_host_tree() {
    for relation in 0..14 {
        let mut tree = window(100);
        tree.nodes[0].1.set_children([NodeId(101)]);
        let mut child = Node::new(Role::TextInput);
        match relation {
            0 => child.set_described_by([NodeId(999)]),
            1 => child.set_labelled_by([NodeId(999)]),
            2 => child.set_error_message(NodeId(999)),
            3 => child.set_controls([NodeId(999)]),
            4 => child.set_details([NodeId(999)]),
            5 => child.set_flow_to([NodeId(999)]),
            6 => child.set_owns([NodeId(999)]),
            7 => child.set_radio_group([NodeId(999)]),
            8 => child.set_active_descendant(NodeId(999)),
            9 => child.set_in_page_link_target(NodeId(999)),
            10 => child.set_member_of(NodeId(999)),
            11 => child.set_next_on_line(NodeId(999)),
            12 => child.set_previous_on_line(NodeId(999)),
            _ => child.set_popup_for(NodeId(999)),
        }
        tree.nodes.push((NodeId(101), child));
        let original = window(1);
        let (merged, owners) = merge_accessibility(original.clone(), vec![pane(tree)], None);
        assert_eq!(merged, original);
        assert!(owners.is_empty());
        AccessibilitySnapshot::default().update(merged).unwrap();
    }
}

#[test]
fn clipping_an_ancestor_removes_descendant_actions_and_focus_from_pane_routing() {
    let mut tree = window(100);
    tree.nodes[0].1.set_children([NodeId(101)]);
    let mut container = Node::new(Role::GenericContainer);
    container.set_bounds(rust_desktop_ui_platform_winit::accesskit::Rect::new(
        200.0, 200.0, 300.0, 300.0,
    ));
    container.set_children([NodeId(102)]);
    let mut child = Node::new(Role::TextInput);
    child.set_bounds(rust_desktop_ui_platform_winit::accesskit::Rect::new(
        0.0, 0.0, 20.0, 20.0,
    ));
    child.add_action(Action::SetValue);
    child.add_action(Action::Focus);
    tree.nodes
        .extend([(NodeId(101), container), (NodeId(102), child)]);
    tree.focus = NodeId(102);
    let mut pane = pane(tree);
    pane.clip = Rect::new(0.0, 0.0, 100.0, 100.0);
    let id = pane.id.clone();
    let (merged, owners) = merge_accessibility(window(1), vec![pane], Some(&id));
    assert!(node(&merged, NodeId(102)).is_hidden());
    assert!(!owners.contains_key(&NodeId(102)));
    assert_eq!(merged.focus, NodeId(100));
    let mut accepted = AccessibilitySnapshot::default();
    accepted.update(merged).unwrap();
    assert!(!accepted.permits(&request(NodeId(102), Action::SetValue)));
    assert!(!accepted.permits(&request(NodeId(102), Action::Focus)));
}
