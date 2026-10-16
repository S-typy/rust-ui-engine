//! Validation and incremental delivery of complete accessibility snapshots.
use accesskit::{ActionRequest, Node, NodeId, Role, TreeId, TreeInfo, TreeUpdate};
use std::collections::{HashMap, HashSet};

/// The accepted, connected native accessibility tree. Application snapshots
/// are checked before publication, then only changed nodes are delivered.
/// Removed descendants disappear through their updated parent's child list.
#[derive(Default, Debug)]
pub struct AccessibilitySnapshot {
    nodes: HashMap<NodeId, Node>,
    tree: Option<TreeInfo>,
    focus: Option<NodeId>,
    unavailable: HashSet<NodeId>,
}
impl AccessibilitySnapshot {
    /// Clear after AccessKit deactivation or a request for the initial tree.
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    /// Accept a complete ROOT tree atomically. Errors preserve the last valid
    /// snapshot. IDs may be reused only for the same live semantic identity;
    /// applications must allocate new IDs when replacing removed controls.
    pub fn update(&mut self, mut full: TreeUpdate) -> Result<TreeUpdate, String> {
        if full.tree_id != TreeId::ROOT {
            return Err("Accessibility host requires ROOT tree".into());
        }
        let info = full
            .tree
            .as_ref()
            .ok_or("Complete accessibility snapshot requires TreeInfo")?;
        if full.nodes.is_empty() || full.nodes.len() > 1_000_000 {
            return Err("Accessibility snapshot must contain 1..=1000000 nodes".into());
        }
        let mut nodes = HashMap::with_capacity(full.nodes.len());
        for (id, node) in &full.nodes {
            if nodes.insert(*id, node).is_some() {
                return Err("Duplicate accessibility node ID".into());
            }
            if let Some(bounds) = node.bounds()
                && (![bounds.x0, bounds.y0, bounds.x1, bounds.y1]
                    .iter()
                    .all(|n| n.is_finite())
                    || bounds.x1 < bounds.x0
                    || bounds.y1 < bounds.y0)
            {
                return Err("Invalid accessibility bounds".into());
            }
        }
        let mut visited = HashSet::with_capacity(nodes.len());
        let mut unavailable = HashSet::new();
        let mut pending = vec![(info.root, false)];
        while let Some((id, inherited_unavailable)) = pending.pop() {
            if !visited.insert(id) {
                return Err("Accessibility tree has a cycle or multiple parents".into());
            }
            let node = nodes.get(&id).ok_or("Accessibility child is absent")?;
            let blocked = inherited_unavailable || node.is_hidden() || node.is_disabled();
            if blocked {
                unavailable.insert(id);
            }
            pending.extend(node.children().iter().map(|id| (*id, blocked)));
        }
        if visited.len() != nodes.len() {
            return Err("Accessibility snapshot contains disconnected nodes".into());
        }
        if !visited.contains(&full.focus) {
            return Err("Accessibility focus is absent".into());
        }
        for node in nodes.values() {
            for target in node
                .labelled_by()
                .iter()
                .chain(node.described_by())
                .chain(node.controls())
                .chain(node.details())
                .chain(node.flow_to())
                .chain(node.owns())
                .chain(node.radio_group())
            {
                if !nodes.contains_key(target) {
                    return Err("Accessibility relation target is absent".into());
                }
            }
            for target in [
                node.error_message(),
                node.active_descendant(),
                node.in_page_link_target(),
                node.member_of(),
                node.next_on_line(),
                node.previous_on_line(),
                node.popup_for(),
            ]
            .into_iter()
            .flatten()
            {
                if !nodes.contains_key(&target) {
                    return Err("Accessibility relation target is absent".into());
                }
            }
            if let Some(selection) = node.text_selection() {
                for position in [&selection.anchor, &selection.focus] {
                    let run = nodes
                        .get(&position.node)
                        .ok_or("Accessibility text selection target is absent")?;
                    if position.character_index > run.character_lengths().len() {
                        return Err("Accessibility text selection is out of range".into());
                    }
                }
            }
        }
        let accepted = full.nodes.iter().cloned().collect();
        full.nodes
            .retain(|(id, node)| self.nodes.get(id) != Some(node));
        self.nodes = accepted;
        self.tree = full.tree.clone();
        self.focus = Some(full.focus);
        self.unavailable = unavailable;
        Ok(full)
    }

    /// Reject stale, disabled and unadvertised native requests. The control
    /// adapter must also validate current state and action payloads at dispatch.
    pub fn permits(&self, request: &ActionRequest) -> bool {
        request.target_tree == TreeId::ROOT
            && self.nodes.get(&request.target_node).is_some_and(|node| {
                !self.unavailable.contains(&request.target_node)
                    && node.supports_action(request.action)
                    && !(node.is_read_only()
                        && matches!(
                            request.action,
                            accesskit::Action::SetValue
                                | accesskit::Action::ReplaceSelectedText
                                | accesskit::Action::Increment
                                | accesskit::Action::Decrement
                        ))
            })
    }

    pub(crate) fn unchanged(&self) -> TreeUpdate {
        if let Some(tree) = &self.tree {
            TreeUpdate {
                nodes: Vec::new(),
                tree: Some(tree.clone()),
                tree_id: TreeId::ROOT,
                focus: self.focus.unwrap_or(tree.root),
            }
        } else {
            TreeUpdate {
                nodes: vec![(NodeId(1), Node::new(Role::Window))],
                tree: Some(TreeInfo::new(NodeId(1))),
                tree_id: TreeId::ROOT,
                focus: NodeId(1),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use accesskit::Action;
    fn tree(children: &[u64]) -> TreeUpdate {
        let mut root = Node::new(Role::Window);
        root.set_children(children.iter().map(|id| NodeId(*id)).collect::<Vec<_>>());
        let mut nodes = vec![(NodeId(1), root)];
        nodes.extend(children.iter().map(|id| {
            let mut node = Node::new(Role::Button);
            node.add_action(Action::Click);
            (NodeId(*id), node)
        }));
        TreeUpdate {
            nodes,
            tree: Some(TreeInfo::new(NodeId(1))),
            tree_id: TreeId::ROOT,
            focus: NodeId(1),
        }
    }
    fn click(id: u64) -> ActionRequest {
        ActionRequest {
            action: Action::Click,
            target_node: NodeId(id),
            target_tree: TreeId::ROOT,
            data: None,
        }
    }
    #[test]
    fn incremental_removal_reappearance_and_initial_reset() {
        let mut state = AccessibilitySnapshot::default();
        assert_eq!(state.update(tree(&[2, 3])).unwrap().nodes.len(), 3);
        assert!(state.update(tree(&[2, 3])).unwrap().nodes.is_empty());
        assert!(state.permits(&click(3)));
        assert_eq!(state.update(tree(&[2])).unwrap().nodes.len(), 1);
        assert!(!state.permits(&click(3)));
        assert_eq!(state.update(tree(&[2, 3])).unwrap().nodes.len(), 2);
        state.clear();
        assert!(!state.permits(&click(2)));
        assert_eq!(state.update(tree(&[2, 3])).unwrap().nodes.len(), 3);
    }
    #[test]
    fn malformed_tree_never_replaces_accepted_snapshot() {
        let mut state = AccessibilitySnapshot::default();
        state.update(tree(&[2])).unwrap();
        let mut disconnected = tree(&[2]);
        disconnected
            .nodes
            .push((NodeId(3), Node::new(Role::Button)));
        let mut missing = tree(&[2]);
        missing.nodes.pop();
        let mut cycle = tree(&[2]);
        cycle.nodes[1].1.set_children(vec![NodeId(1)]);
        let mut focus = tree(&[2]);
        focus.focus = NodeId(999);
        let duplicate = tree(&[2, 2]);
        let mut relation = tree(&[2]);
        relation.nodes[1].1.set_labelled_by(vec![NodeId(999)]);
        for bad in [disconnected, missing, cycle, focus, duplicate, relation] {
            assert!(state.update(bad).is_err());
            assert!(state.permits(&click(2)));
            assert!(state.update(tree(&[2])).unwrap().nodes.is_empty());
        }
        let mut disabled = tree(&[2]);
        disabled.nodes[1].1.set_disabled();
        state.update(disabled).unwrap();
        assert!(!state.permits(&click(2)));
        assert!(!state.permits(&click(1)));
        let mut hidden_parent = tree(&[2]);
        hidden_parent.nodes[0].1.set_hidden();
        state.update(hidden_parent).unwrap();
        assert!(!state.permits(&click(2)));
        let mut readonly = tree(&[2]);
        readonly.nodes[1].1.set_read_only();
        readonly.nodes[1].1.add_action(Action::SetValue);
        readonly.nodes[1].1.add_action(Action::Focus);
        state.update(readonly).unwrap();
        let mut request = click(2);
        request.action = Action::SetValue;
        assert!(!state.permits(&request));
        request.action = Action::Focus;
        assert!(state.permits(&request));
    }
}
