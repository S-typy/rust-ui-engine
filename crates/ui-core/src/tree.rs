use std::error::Error;
use std::fmt;

use crate::arena::Arena;
use crate::{DirtyFlags, LayoutStyle, NodeProps, Paint, WidgetId};

/// A retained node. Structural links are maintained exclusively by [`UiTree`].
///
/// The tree returns shared references only. Mutations must go through its
/// setters so validation and invalidation cannot be bypassed.
#[derive(Debug)]
pub struct Node {
    pub props: NodeProps,
    parent: Option<WidgetId>,
    children: Vec<WidgetId>,
    dirty: DirtyFlags,
}

/// Rejected tree operation. Rejection leaves the tree unchanged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TreeError {
    InvalidId(WidgetId),
    RootProtected,
    Cycle,
    InvalidProps(&'static str),
}

impl fmt::Display for TreeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidId(id) => write!(formatter, "invalid or stale widget ID: {id:?}"),
            Self::RootProtected => formatter.write_str("the tree root cannot be removed or moved"),
            Self::Cycle => formatter.write_str("reparenting would introduce a tree cycle"),
            Self::InvalidProps(reason) => write!(formatter, "invalid node properties: {reason}"),
        }
    }
}

impl Error for TreeError {}

/// An owned, ordered retained tree with a permanent root.
///
/// Child order is insertion order. Reparenting appends to the new parent;
/// reparenting within the same parent moves the node to the end. Traversals
/// return ID snapshots, permitting callers to defer edits until traversal ends.
pub struct UiTree {
    nodes: Arena<Node>,
    root: WidgetId,
    pending: DirtyFlags,
}

impl Default for UiTree {
    fn default() -> Self {
        Self::new()
    }
}

impl UiTree {
    /// Creates a tree containing one root with default properties.
    pub fn new() -> Self {
        let mut nodes = Arena::new();
        let root = nodes.insert(Node {
            props: NodeProps::default(),
            parent: None,
            children: Vec::new(),
            dirty: DirtyFlags::ALL,
        });
        Self {
            nodes,
            root,
            pending: DirtyFlags::ALL,
        }
    }

    pub fn root(&self) -> WidgetId {
        self.root
    }

    pub fn node(&self, id: WidgetId) -> Option<&Node> {
        self.nodes.get(id)
    }

    pub fn contains(&self, id: WidgetId) -> bool {
        self.node(id).is_some()
    }

    /// Returns ordered children, or `None` for an invalid ID.
    pub fn children(&self, id: WidgetId) -> Option<&[WidgetId]> {
        self.node(id).map(|node| node.children.as_slice())
    }

    /// Returns the parent, or `None` for the root or an invalid ID.
    pub fn parent(&self, id: WidgetId) -> Option<WidgetId> {
        self.node(id).and_then(|node| node.parent)
    }

    /// Includes the permanent root.
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Always false: every tree owns its permanent root.
    pub fn is_empty(&self) -> bool {
        false
    }

    pub fn insert(&mut self, parent: WidgetId, props: NodeProps) -> Result<WidgetId, TreeError> {
        self.require(parent)?;
        props.validate().map_err(TreeError::InvalidProps)?;
        let id = self.nodes.insert(Node {
            props,
            parent: Some(parent),
            children: Vec::new(),
            dirty: DirtyFlags::ALL,
        });
        self.nodes
            .get_mut(parent)
            .expect("validated parent")
            .children
            .push(id);
        self.mark_valid_dirty(parent, DirtyFlags::ALL);
        Ok(id)
    }

    /// Removes a complete subtree and returns its former IDs in preorder.
    ///
    /// Every returned ID becomes invalid before this method returns. The root
    /// cannot be removed. The traversal is iterative, including for deep trees.
    pub fn remove(&mut self, id: WidgetId) -> Result<Vec<WidgetId>, TreeError> {
        self.require(id)?;
        if id == self.root {
            return Err(TreeError::RootProtected);
        }
        let parent = self.parent(id).expect("non-root has a parent");
        let removed = self.subtree_preorder(id);
        self.nodes
            .get_mut(parent)
            .expect("live parent")
            .children
            .retain(|child| *child != id);
        for &removed_id in &removed {
            self.nodes.remove(removed_id);
        }
        self.mark_valid_dirty(parent, DirtyFlags::ALL);
        Ok(removed)
    }

    pub fn reparent(&mut self, id: WidgetId, parent: WidgetId) -> Result<(), TreeError> {
        self.require(id)?;
        self.require(parent)?;
        if id == self.root {
            return Err(TreeError::RootProtected);
        }
        let mut ancestor = Some(parent);
        while let Some(candidate) = ancestor {
            if candidate == id {
                return Err(TreeError::Cycle);
            }
            ancestor = self.parent(candidate);
        }
        let previous = self.parent(id).expect("non-root has a parent");
        if previous == parent && self.children(parent).and_then(|items| items.last()) == Some(&id) {
            return Ok(());
        }
        self.nodes
            .get_mut(previous)
            .expect("live previous parent")
            .children
            .retain(|child| *child != id);
        self.nodes
            .get_mut(parent)
            .expect("validated parent")
            .children
            .push(id);
        self.nodes.get_mut(id).expect("validated node").parent = Some(parent);
        self.mark_valid_dirty(previous, DirtyFlags::ALL);
        self.mark_valid_dirty(parent, DirtyFlags::ALL);
        self.mark_valid_dirty(id, DirtyFlags::ALL);
        Ok(())
    }

    pub fn set_props(&mut self, id: WidgetId, props: NodeProps) -> Result<(), TreeError> {
        let previous = &self.require(id)?.props;
        props.validate().map_err(TreeError::InvalidProps)?;
        if previous == &props {
            return Ok(());
        }
        let style_changed = previous.style != props.style;
        let paint_changed = previous.paint != props.paint;
        let mut compared = previous.clone();
        compared.style = props.style.clone();
        compared.paint = props.paint;
        let metadata_changed = compared != props;
        self.nodes.get_mut(id).expect("validated node").props = props;
        let mut dirty = DirtyFlags::NONE;
        if style_changed {
            dirty |= DirtyFlags::ALL;
        }
        if paint_changed {
            dirty |= DirtyFlags::PAINT;
        }
        if metadata_changed {
            dirty |= DirtyFlags::PAINT | DirtyFlags::SEMANTICS;
        }
        self.mark_valid_dirty(id, dirty);
        Ok(())
    }

    pub fn set_style(&mut self, id: WidgetId, style: LayoutStyle) -> Result<(), TreeError> {
        let mut props = self.require(id)?.props.clone();
        props.style = style;
        self.set_props(id, props)
    }

    pub fn set_paint(&mut self, id: WidgetId, paint: Paint) -> Result<(), TreeError> {
        let mut props = self.require(id)?.props.clone();
        props.paint = paint;
        self.set_props(id, props)
    }

    /// Returns pending node work, or `NONE` for an invalid ID.
    pub fn dirty(&self, id: WidgetId) -> DirtyFlags {
        self.node(id).map_or(DirtyFlags::NONE, |node| node.dirty)
    }

    pub fn pending_dirty(&self) -> DirtyFlags {
        self.pending
    }

    /// Invalidates work on a live node.
    ///
    /// Measure implies layout and paint; layout implies paint. These geometry
    /// flags propagate to ancestors because child geometry can change their
    /// layout. Paint and semantics alone stay local to this node.
    pub fn mark_dirty(&mut self, id: WidgetId, flags: DirtyFlags) -> Result<(), TreeError> {
        self.require(id)?;
        self.mark_valid_dirty(id, flags);
        Ok(())
    }

    /// Acknowledges all pending work after a successful frame update.
    pub fn clear_dirty(&mut self) {
        if self.pending.is_empty() {
            return;
        }
        for id in self.preorder() {
            self.nodes.get_mut(id).expect("live traversal ID").dirty = DirtyFlags::NONE;
        }
        self.pending = DirtyFlags::NONE;
    }

    /// Returns a stable parent-before-children snapshot in child insertion order.
    pub fn preorder(&self) -> Vec<WidgetId> {
        self.subtree_preorder(self.root)
    }

    fn require(&self, id: WidgetId) -> Result<&Node, TreeError> {
        self.node(id).ok_or(TreeError::InvalidId(id))
    }

    fn subtree_preorder(&self, root: WidgetId) -> Vec<WidgetId> {
        let mut result = Vec::new();
        let mut pending = vec![root];
        while let Some(id) = pending.pop() {
            result.push(id);
            let node = self.node(id).expect("live traversal ID");
            pending.extend(node.children.iter().rev().copied());
        }
        result
    }

    fn mark_valid_dirty(&mut self, id: WidgetId, flags: DirtyFlags) {
        let geometry = if flags.contains(DirtyFlags::MEASURE) {
            DirtyFlags::MEASURE | DirtyFlags::LAYOUT | DirtyFlags::PAINT
        } else if flags.contains(DirtyFlags::LAYOUT) {
            DirtyFlags::LAYOUT | DirtyFlags::PAINT
        } else {
            DirtyFlags::NONE
        };
        self.pending |= flags | geometry;
        let node = self.nodes.get_mut(id).expect("validated node");
        node.dirty |= flags | geometry;
        if geometry.is_empty() {
            return;
        }
        let mut parent = node.parent;
        while let Some(id) = parent {
            let node = self.nodes.get_mut(id).expect("live ancestor");
            node.dirty |= geometry;
            parent = node.parent;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn insert(tree: &mut UiTree, parent: WidgetId) -> WidgetId {
        tree.insert(parent, NodeProps::default()).unwrap()
    }

    #[test]
    fn removing_a_subtree_invalidates_all_ids_and_preserves_siblings() {
        let mut tree = UiTree::new();
        let root = tree.root();
        let branch = insert(&mut tree, root);
        let child = insert(&mut tree, branch);
        let grandchild = insert(&mut tree, child);
        let sibling = insert(&mut tree, root);
        assert_eq!(
            tree.remove(branch).unwrap(),
            vec![branch, child, grandchild]
        );
        for stale in [branch, child, grandchild] {
            assert!(!tree.contains(stale));
            assert_eq!(tree.children(stale), None);
            assert_eq!(tree.parent(stale), None);
            assert_eq!(tree.dirty(stale), DirtyFlags::NONE);
        }
        assert_eq!(tree.len(), 2);
        assert_eq!(tree.children(root), Some([sibling].as_slice()));
        assert_eq!(tree.parent(sibling), Some(root));
        let replacement = insert(&mut tree, root);
        assert!(![branch, child, grandchild].contains(&replacement));
        assert_eq!(tree.preorder(), vec![root, sibling, replacement]);
    }

    #[test]
    fn invalid_structure_changes_are_atomic() {
        let mut tree = UiTree::new();
        let root = tree.root();
        let first = insert(&mut tree, root);
        let second = insert(&mut tree, first);
        let foreign = UiTree::new().root();
        tree.clear_dirty();
        let before = tree.preorder();
        assert_eq!(tree.remove(root), Err(TreeError::RootProtected));
        assert_eq!(tree.reparent(root, first), Err(TreeError::RootProtected));
        assert_eq!(tree.reparent(first, second), Err(TreeError::Cycle));
        assert_eq!(tree.reparent(first, first), Err(TreeError::Cycle));
        assert_eq!(
            tree.reparent(first, foreign),
            Err(TreeError::InvalidId(foreign))
        );
        assert_eq!(
            tree.insert(foreign, NodeProps::default()),
            Err(TreeError::InvalidId(foreign))
        );
        assert_eq!(tree.remove(foreign), Err(TreeError::InvalidId(foreign)));
        assert_eq!(
            tree.mark_dirty(foreign, DirtyFlags::ALL),
            Err(TreeError::InvalidId(foreign))
        );
        assert_eq!(tree.preorder(), before);
        assert_eq!(tree.parent(first), Some(root));
        assert_eq!(tree.parent(second), Some(first));
        assert_eq!(tree.len(), 3);
        assert!(tree.pending_dirty().is_empty());
    }

    #[test]
    fn reparent_preserves_identity_and_orders_children() {
        let mut tree = UiTree::new();
        let root = tree.root();
        let first = insert(&mut tree, root);
        let second = insert(&mut tree, root);
        let child = insert(&mut tree, first);
        tree.reparent(child, second).unwrap();
        assert_eq!(tree.parent(child), Some(second));
        assert_eq!(tree.children(first), Some([].as_slice()));
        assert_eq!(tree.children(second), Some([child].as_slice()));
        tree.reparent(first, root).unwrap();
        assert_eq!(tree.preorder(), vec![root, second, child, first]);
        assert_eq!(tree.len(), 4);
        tree.clear_dirty();
        tree.reparent(first, root).unwrap();
        assert!(tree.pending_dirty().is_empty());
    }

    #[test]
    fn geometry_propagates_but_paint_and_semantics_stay_local() {
        let mut tree = UiTree::new();
        let root = tree.root();
        let parent = insert(&mut tree, root);
        let child = insert(&mut tree, parent);
        let sibling = insert(&mut tree, root);
        tree.clear_dirty();
        tree.mark_dirty(child, DirtyFlags::PAINT | DirtyFlags::SEMANTICS)
            .unwrap();
        assert_eq!(tree.dirty(child), DirtyFlags::PAINT | DirtyFlags::SEMANTICS);
        assert_eq!(tree.dirty(parent), DirtyFlags::NONE);
        assert_eq!(tree.dirty(root), DirtyFlags::NONE);
        assert_eq!(tree.dirty(sibling), DirtyFlags::NONE);
        tree.clear_dirty();
        tree.mark_dirty(child, DirtyFlags::MEASURE).unwrap();
        let geometry = DirtyFlags::MEASURE | DirtyFlags::LAYOUT | DirtyFlags::PAINT;
        for id in [root, parent, child] {
            assert_eq!(tree.dirty(id), geometry);
        }
        assert_eq!(tree.dirty(sibling), DirtyFlags::NONE);
        assert_eq!(tree.pending_dirty(), geometry);
        tree.clear_dirty();
        assert!(tree.pending_dirty().is_empty());
        assert!(tree.preorder().iter().all(|id| tree.dirty(*id).is_empty()));
    }

    #[test]
    fn no_op_setters_keep_the_tree_clean() {
        let mut tree = UiTree::new();
        let root = tree.root();
        let props = tree.node(root).unwrap().props.clone();
        tree.clear_dirty();
        tree.set_props(root, props.clone()).unwrap();
        tree.set_style(root, props.style).unwrap();
        tree.set_paint(root, props.paint).unwrap();
        assert!(tree.pending_dirty().is_empty());
        assert_eq!(tree.len(), 1);
        assert!(!tree.is_empty());
    }

    #[test]
    fn setters_validate_before_mutating_and_classify_dirty_work() {
        use crate::{Color, Length, Point};

        let mut tree = UiTree::new();
        let root = tree.root();
        let child = insert(&mut tree, root);
        tree.clear_dirty();
        let original = tree.node(child).unwrap().props.clone();
        let mut invalid = original.clone();
        invalid.style.width = Length::Px(f32::NAN);
        assert!(matches!(
            tree.set_props(child, invalid.clone()),
            Err(TreeError::InvalidProps(_))
        ));
        assert!(matches!(
            tree.insert(root, invalid),
            Err(TreeError::InvalidProps(_))
        ));
        let invalid_paint = Paint {
            background: Some(Color {
                r: 2.0,
                g: 0.0,
                b: 0.0,
                a: 1.0,
            }),
            ..Paint::default()
        };
        assert!(matches!(
            tree.set_paint(child, invalid_paint),
            Err(TreeError::InvalidProps(_))
        ));
        let mut invalid = original.clone();
        invalid.translation = Point::new(f32::INFINITY, 0.0);
        assert!(matches!(
            tree.set_props(child, invalid),
            Err(TreeError::InvalidProps(_))
        ));
        assert_eq!(tree.node(child).unwrap().props, original);
        assert_eq!(tree.len(), 2);
        assert!(tree.pending_dirty().is_empty());

        let paint = Paint {
            background: Some(Color::rgb(50, 60, 70)),
            ..Paint::default()
        };
        tree.set_paint(child, paint).unwrap();
        assert_eq!(tree.dirty(child), DirtyFlags::PAINT);
        assert_eq!(tree.dirty(root), DirtyFlags::NONE);
        assert_eq!(tree.pending_dirty(), DirtyFlags::PAINT);
        tree.clear_dirty();
        let mut props = tree.node(child).unwrap().props.clone();
        props.focusable = true;
        props.visible = false;
        tree.set_props(child, props).unwrap();
        assert_eq!(
            tree.pending_dirty(),
            DirtyFlags::PAINT | DirtyFlags::SEMANTICS
        );
        assert_eq!(tree.dirty(root), DirtyFlags::NONE);
        tree.clear_dirty();
        let mut style = original.style;
        style.width = Length::Px(10.0);
        tree.set_style(child, style).unwrap();
        assert_eq!(tree.dirty(child), DirtyFlags::ALL);
        assert_eq!(
            tree.dirty(root),
            DirtyFlags::MEASURE | DirtyFlags::LAYOUT | DirtyFlags::PAINT
        );
    }

    #[test]
    fn deleted_ids_cannot_mutate_replacement_nodes() {
        let mut tree = UiTree::new();
        let root = tree.root();
        let stale = insert(&mut tree, root);
        tree.remove(stale).unwrap();
        let replacement = insert(&mut tree, root);
        tree.clear_dirty();
        assert_eq!(
            tree.set_props(stale, NodeProps::default()),
            Err(TreeError::InvalidId(stale))
        );
        assert_eq!(
            tree.set_style(stale, LayoutStyle::default()),
            Err(TreeError::InvalidId(stale))
        );
        assert_eq!(
            tree.set_paint(stale, Paint::default()),
            Err(TreeError::InvalidId(stale))
        );
        assert_eq!(tree.reparent(stale, root), Err(TreeError::InvalidId(stale)));
        assert!(tree.contains(replacement));
        assert_eq!(tree.children(root), Some([replacement].as_slice()));
        assert!(tree.pending_dirty().is_empty());
    }

    #[test]
    fn deep_tree_traversal_and_removal_are_iterative() {
        let mut tree = UiTree::new();
        let root = tree.root();
        let branch = insert(&mut tree, root);
        let mut parent = branch;
        for _ in 0..1_024 {
            parent = insert(&mut tree, parent);
        }
        assert_eq!(tree.preorder().len(), 1_026);
        assert_eq!(tree.remove(branch).unwrap().len(), 1_025);
        assert_eq!(tree.preorder(), vec![root]);
    }
}
