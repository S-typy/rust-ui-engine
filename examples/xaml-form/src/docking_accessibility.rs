//! Combine retained pane projections into the native window's connected tree.
use rust_desktop_ui_controls::DockPaneId;
use rust_desktop_ui_core::{Point, Rect};
use rust_desktop_ui_platform_winit::accesskit::{self, NodeId, Role, TreeUpdate};
use std::collections::{HashMap, HashSet};

pub(crate) struct PaneAccessibility {
    pub id: DockPaneId,
    pub root: NodeId,
    /// Translation and clip are already in physical client pixels.
    pub offset: Point,
    pub clip: Rect,
    pub tree: TreeUpdate,
}

/// The caller supplies complete trees for visible, active panes only, with
/// disjoint stable NodeId ranges. Relationships and action targets are preserved.
/// Unsupported transforms or invalid trees are omitted as a whole, so they
/// cannot corrupt the native tree or accidentally route actions to another pane.
pub(crate) fn merge_accessibility(
    mut main: TreeUpdate,
    panes: Vec<PaneAccessibility>,
    focused_pane: Option<&DockPaneId>,
) -> (TreeUpdate, HashMap<NodeId, DockPaneId>) {
    let mut owners = HashMap::new();
    let Some(root) = main.tree.as_ref().map(|tree| tree.root) else {
        return (main, owners);
    };
    let Some(root_index) = main.nodes.iter().position(|(id, _)| *id == root) else {
        return (main, owners);
    };
    let mut used: HashSet<_> = main.nodes.iter().map(|(id, _)| *id).collect();
    let mut roots = Vec::new();
    let mut pane_ids = HashSet::new();
    for mut pane in panes {
        if !pane.offset.is_finite()
            || !pane.clip.is_valid()
            || pane.clip.width <= 0.0
            || pane.clip.height <= 0.0
            || pane_ids.contains(&pane.id)
            || !valid_pane(&pane, &used)
        {
            continue;
        }
        let clip = accesskit::Rect::new(
            f64::from(pane.clip.x),
            f64::from(pane.clip.y),
            f64::from(pane.clip.x + pane.clip.width),
            f64::from(pane.clip.y + pane.clip.height),
        );
        let focused = focused_pane == Some(&pane.id);
        let mut visible_focus = pane.root;
        for (id, node) in &mut pane.tree.nodes {
            if *id == pane.root {
                node.set_role(Role::GenericContainer);
                node.set_bounds(clip);
                node.set_clips_children();
            } else if let Some(bounds) = node.bounds() {
                let bounds = accesskit::Rect::new(
                    bounds.x0 + f64::from(pane.offset.x),
                    bounds.y0 + f64::from(pane.offset.y),
                    bounds.x1 + f64::from(pane.offset.x),
                    bounds.y1 + f64::from(pane.offset.y),
                );
                let x0 = bounds.x0.max(clip.x0);
                let y0 = bounds.y0.max(clip.y0);
                let x1 = bounds.x1.min(clip.x1);
                let y1 = bounds.y1.min(clip.y1);
                if x1 > x0 && y1 > y0 {
                    node.set_bounds(accesskit::Rect::new(x0, y0, x1, y1));
                } else {
                    node.set_bounds(accesskit::Rect::new(clip.x0, clip.y0, clip.x0, clip.y0));
                    node.set_hidden();
                }
            }
            if *id == pane.tree.focus && !node.is_hidden() {
                visible_focus = *id;
            }
            used.insert(*id);
            if !node.is_hidden() {
                owners.insert(*id, pane.id.clone());
            }
        }
        if focused {
            main.focus = visible_focus;
        }
        pane_ids.insert(pane.id);
        roots.push(pane.root);
        main.nodes.extend(pane.tree.nodes);
    }
    for root in roots {
        main.nodes[root_index].1.push_child(root);
    }
    (main, owners)
}

fn valid_pane(pane: &PaneAccessibility, used: &HashSet<NodeId>) -> bool {
    if pane
        .tree
        .tree
        .as_ref()
        .is_some_and(|tree| tree.root != pane.root)
    {
        return false;
    }
    let mut ids = HashSet::new();
    for (id, node) in &pane.tree.nodes {
        if used.contains(id) || !ids.insert(*id) || node.transform().is_some() {
            return false;
        }
        if let Some(bounds) = node.bounds()
            && (![bounds.x0, bounds.y0, bounds.x1, bounds.y1]
                .into_iter()
                .all(f64::is_finite)
                || bounds.x1 < bounds.x0
                || bounds.y1 < bounds.y0)
        {
            return false;
        }
    }
    if !ids.contains(&pane.root) || !ids.contains(&pane.tree.focus) {
        return false;
    }
    let nodes: HashMap<_, _> = pane
        .tree
        .nodes
        .iter()
        .map(|(id, node)| (*id, node))
        .collect();
    let mut visited = HashSet::new();
    let mut pending = vec![pane.root];
    while let Some(id) = pending.pop() {
        if !visited.insert(id) {
            return false;
        }
        let Some(node) = nodes.get(&id) else {
            return false;
        };
        pending.extend(node.children());
    }
    visited.len() == ids.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use accesskit::{Action, Node, TreeId, TreeInfo};

    fn main_tree() -> TreeUpdate {
        let mut root = Node::new(Role::Window);
        root.set_children([NodeId(2)]);
        TreeUpdate {
            nodes: vec![(NodeId(1), root), (NodeId(2), Node::new(Role::Button))],
            tree: Some(TreeInfo::new(NodeId(1))),
            tree_id: TreeId::ROOT,
            focus: NodeId(2),
        }
    }

    fn pane(name: &str, base: u64) -> PaneAccessibility {
        let mut root = Node::new(Role::Window);
        root.set_children([NodeId(base + 1), NodeId(base + 2)]);
        let mut editor = Node::new(Role::TextInput);
        editor.set_bounds(accesskit::Rect::new(0.0, 0.0, 100.0, 30.0));
        editor.set_label("Retained editor");
        editor.set_value("Draft value");
        editor.set_described_by([NodeId(base + 2)]);
        editor.add_action(Action::SetValue);
        let mut help = Node::new(Role::Label);
        help.set_label("Help");
        PaneAccessibility {
            id: DockPaneId::new(name).unwrap(),
            root: NodeId(base),
            offset: Point::new(40.0, 60.0),
            clip: Rect::new(50.0, 70.0, 80.0, 50.0),
            tree: TreeUpdate {
                nodes: vec![
                    (NodeId(base), root),
                    (NodeId(base + 1), editor),
                    (NodeId(base + 2), help),
                ],
                tree: Some(TreeInfo::new(NodeId(base))),
                tree_id: TreeId::ROOT,
                focus: NodeId(base + 1),
            },
        }
    }

    #[test]
    fn merge_translates_clips_and_preserves_ids_relationships_and_actions() {
        let pane = pane("editor", 100);
        let id = pane.id.clone();
        let (tree, owners) = merge_accessibility(main_tree(), vec![pane], Some(&id));
        assert_eq!(tree.focus, NodeId(101));
        assert_eq!(tree.nodes[0].1.children(), [NodeId(2), NodeId(100)]);
        let root = &tree
            .nodes
            .iter()
            .find(|(id, _)| *id == NodeId(100))
            .unwrap()
            .1;
        assert_eq!(root.role(), Role::GenericContainer);
        assert!(root.clips_children());
        let editor = &tree
            .nodes
            .iter()
            .find(|(id, _)| *id == NodeId(101))
            .unwrap()
            .1;
        assert_eq!(
            editor.bounds(),
            Some(accesskit::Rect::new(50.0, 70.0, 130.0, 90.0))
        );
        assert_eq!(editor.described_by(), [NodeId(102)]);
        assert!(editor.supports_action(Action::SetValue));
        assert_eq!(editor.value(), Some("Draft value"));
        assert_eq!(owners[&NodeId(101)], id);
        assert!(!owners.contains_key(&NodeId(2)));
    }

    #[test]
    fn only_selected_visible_pane_owns_focus_and_clipped_child_is_hidden() {
        let first = pane("one", 100);
        let mut second = pane("two", 200);
        let id = second.id.clone();
        second.offset = Point::new(300.0, 300.0);
        let (tree, owners) = merge_accessibility(main_tree(), vec![first, second], Some(&id));
        assert!(!owners.contains_key(&NodeId(201)));
        assert_eq!(tree.focus, NodeId(200));
        let editor = &tree
            .nodes
            .iter()
            .find(|(id, _)| *id == NodeId(201))
            .unwrap()
            .1;
        assert!(editor.is_hidden());
        let (tree, _) = merge_accessibility(main_tree(), vec![pane("one", 100)], None);
        assert_eq!(tree.focus, NodeId(2));
        let (tree, _) = merge_accessibility(main_tree(), vec![], Some(&id));
        assert_eq!(tree.focus, NodeId(2));
    }

    #[test]
    fn invalid_or_empty_panes_cannot_expose_nodes_or_steal_actions() {
        let mut invalid = pane("invalid", 100);
        invalid.clip.width = 0.0;
        let mut collision = pane("collision", 200);
        collision.tree.nodes[1].0 = NodeId(2);
        let mut disconnected = pane("disconnected", 300);
        disconnected.tree.nodes[0].1.set_children([NodeId(301)]);
        let mut cycle = pane("cycle", 400);
        cycle.tree.nodes[1].1.set_children([NodeId(400)]);
        let expected = main_tree();
        let (tree, owners) = merge_accessibility(
            expected.clone(),
            vec![invalid, collision, disconnected, cycle],
            None,
        );
        assert_eq!(tree, expected);
        assert!(owners.is_empty());
    }

    #[test]
    fn hiding_a_pane_removes_its_whole_subtree_and_action_map_on_next_projection() {
        let selected = DockPaneId::new("one").unwrap();
        let (first, first_owners) = merge_accessibility(
            main_tree(),
            vec![pane("one", 100), pane("two", 200)],
            Some(&selected),
        );
        assert_eq!(first.focus, NodeId(101));
        assert!(first_owners.contains_key(&NodeId(201)));
        let (next, owners) =
            merge_accessibility(main_tree(), vec![pane("one", 100)], Some(&selected));
        assert_eq!(next.nodes[0].1.children(), [NodeId(2), NodeId(100)]);
        assert!(next.nodes.iter().all(|(id, _)| id.0 < 200));
        assert!(!owners.contains_key(&NodeId(201)));
        assert_eq!(owners[&NodeId(101)], selected);
    }
}
