//! Bounded, indexed hierarchy storage without query or database execution.
use crate::{ColumnId, GridError, RowKey, TreeDataSource, TreeMoveTarget};
use std::collections::{BTreeMap, HashMap, HashSet};

pub const MAX_TREE_NODES: usize = 1_000_000;
pub const MAX_TREE_TEXT_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_TREE_CELL_BYTES: usize = 1024 * 1024;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TreeChildren {
    #[default]
    Known,
    Unloaded,
}

/// Input order determines sibling order. Parents may appear after children.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TreeNode {
    pub key: RowKey,
    pub parent: Option<RowKey>,
    pub cells: BTreeMap<ColumnId, String>,
    pub children: TreeChildren,
}
impl TreeNode {
    pub fn new(key: RowKey, parent: Option<RowKey>, text: impl Into<String>) -> Self {
        Self {
            key,
            parent,
            cells: BTreeMap::from([(ColumnId(0), text.into())]),
            children: TreeChildren::Known,
        }
    }
}

/// Complete accepted graph with O(1) key, parent, sibling position and child
/// access. Explicit writes validate a candidate before publishing it; viewport
/// reads never traverse all records. This type does not execute GridQuery.
#[derive(Clone, Debug)]
pub struct IndexedTreeSource {
    nodes: Vec<TreeNode>,
    by_key: HashMap<RowKey, usize>,
    children: HashMap<Option<RowKey>, Vec<RowKey>>,
    positions: HashMap<RowKey, usize>,
    depths: HashMap<RowKey, usize>,
    heights: HashMap<RowKey, usize>,
    revision: u64,
}
impl IndexedTreeSource {
    pub fn new(nodes: Vec<TreeNode>) -> Result<Self, GridError> {
        if nodes.len() > MAX_TREE_NODES {
            return Err(GridError::Capacity);
        }
        let mut by_key = HashMap::with_capacity(nodes.len());
        let mut bytes = 0usize;
        for (i, node) in nodes.iter().enumerate() {
            if by_key.insert(node.key, i).is_some() {
                return Err(GridError::InvalidHierarchy("duplicate node key"));
            }
            if node.cells.len() > 64 {
                return Err(GridError::Capacity);
            }
            for cell in node.cells.values() {
                if cell.len() > MAX_TREE_CELL_BYTES || cell.contains('\0') {
                    return Err(GridError::Capacity);
                }
                bytes = bytes.checked_add(cell.len()).ok_or(GridError::Capacity)?;
                if bytes > MAX_TREE_TEXT_BYTES {
                    return Err(GridError::Capacity);
                }
            }
        }
        let mut children: HashMap<Option<RowKey>, Vec<RowKey>> = HashMap::new();
        let mut positions = HashMap::with_capacity(nodes.len());
        for node in &nodes {
            if let Some(parent) = node.parent {
                let parent = by_key
                    .get(&parent)
                    .ok_or(GridError::InvalidHierarchy("missing parent"))?;
                if nodes[*parent].children == TreeChildren::Unloaded {
                    return Err(GridError::InvalidHierarchy(
                        "unloaded parent already has child records",
                    ));
                }
            }
            let siblings = children.entry(node.parent).or_default();
            positions.insert(node.key, siblings.len());
            siblings.push(node.key);
        }
        // Memoized depths make validation linear even for shared long lineages.
        let mut depths = HashMap::<RowKey, usize>::with_capacity(nodes.len());
        for node in &nodes {
            if depths.contains_key(&node.key) {
                continue;
            }
            let mut path = Vec::new();
            let mut active = HashSet::new();
            let mut current = Some(node.key);
            let mut depth = 0;
            while let Some(key) = current {
                if let Some(&known) = depths.get(&key) {
                    depth = known;
                    break;
                }
                if !active.insert(key) || path.len() >= 256 {
                    return Err(GridError::InvalidHierarchy(
                        "cycle or hierarchy depth above 256",
                    ));
                }
                path.push(key);
                current = nodes[by_key[&key]].parent;
            }
            for key in path.into_iter().rev() {
                depth += 1;
                if depth > 256 {
                    return Err(GridError::InvalidHierarchy("hierarchy depth above 256"));
                }
                depths.insert(key, depth);
            }
        }
        let mut levels = vec![Vec::new(); 257];
        for (&key, &depth) in &depths {
            levels[depth].push(key);
        }
        let mut heights = HashMap::<RowKey, usize>::with_capacity(nodes.len());
        for level in levels.into_iter().rev() {
            for key in level {
                let height = *heights.entry(key).or_insert(1);
                if let Some(parent) = nodes[by_key[&key]].parent {
                    let parent_height = heights.entry(parent).or_insert(1);
                    *parent_height = (*parent_height).max(height + 1);
                }
            }
        }
        Ok(Self {
            nodes,
            by_key,
            children,
            positions,
            depths,
            heights,
            revision: 0,
        })
    }
    pub fn nodes(&self) -> &[TreeNode] {
        &self.nodes
    }
    pub fn node(&self, key: RowKey) -> Option<&TreeNode> {
        self.by_key.get(&key).map(|&i| &self.nodes[i])
    }
    pub fn replace(&mut self, nodes: Vec<TreeNode>) -> Result<bool, GridError> {
        if self.nodes == nodes {
            return Ok(false);
        }
        let revision = self.revision.checked_add(1).ok_or(GridError::Capacity)?;
        let mut next = Self::new(nodes)?;
        next.revision = revision;
        *self = next;
        Ok(true)
    }
    pub fn replace_children(
        &mut self,
        parent: RowKey,
        children: Vec<TreeNode>,
    ) -> Result<bool, GridError> {
        let parent_index = *self
            .by_key
            .get(&parent)
            .ok_or(GridError::UnknownRow(parent))?;
        if self.nodes[parent_index].children != TreeChildren::Unloaded {
            return Err(GridError::InvalidHierarchy("children are already loaded"));
        }
        if children.iter().any(|node| node.parent != Some(parent)) {
            return Err(GridError::InvalidHierarchy(
                "lazy reply must contain direct children",
            ));
        }
        let mut nodes = self.nodes.clone();
        nodes[parent_index].children = TreeChildren::Known;
        nodes.extend(children);
        self.replace(nodes)
    }
    fn move_parent(&self, target: TreeMoveTarget) -> Result<Option<RowKey>, GridError> {
        match target {
            TreeMoveTarget::Before(key) | TreeMoveTarget::After(key) => self
                .node(key)
                .map(|node| node.parent)
                .ok_or(GridError::UnknownRow(key)),
            TreeMoveTarget::Into(Some(key)) => {
                let node = self.node(key).ok_or(GridError::UnknownRow(key))?;
                if node.children == TreeChildren::Unloaded {
                    return Err(GridError::InvalidHierarchy(
                        "load destination children before moving",
                    ));
                }
                Ok(Some(key))
            }
            TreeMoveTarget::Into(None) => Ok(None),
        }
    }
    pub fn validate_node_move(
        &self,
        rows: &[RowKey],
        target: TreeMoveTarget,
    ) -> Result<(), GridError> {
        if rows.is_empty() || rows.len() > 4096 {
            return Err(GridError::Capacity);
        }
        let requested: HashSet<_> = rows.iter().copied().collect();
        if requested.len() != rows.len() {
            return Err(GridError::InvalidHierarchy("duplicate move key"));
        }
        let parent = self.move_parent(target)?;
        let destination_depth = parent.map_or(0, |key| self.depths[&key]);
        if rows.iter().any(|key| {
            self.heights
                .get(key)
                .is_some_and(|height| destination_depth + height > 256)
        }) {
            return Err(GridError::InvalidHierarchy(
                "move exceeds hierarchy depth 256",
            ));
        }
        if matches!(target,TreeMoveTarget::Before(key)|TreeMoveTarget::After(key) if requested.contains(&key))
        {
            return Err(GridError::InvalidHierarchy(
                "move destination belongs to moved rows",
            ));
        }
        for &key in rows {
            let mut current = self.node(key).ok_or(GridError::UnknownRow(key))?.parent;
            while let Some(ancestor) = current {
                if requested.contains(&ancestor) {
                    return Err(GridError::InvalidHierarchy(
                        "move contains both ancestor and descendant",
                    ));
                }
                current = self.node(ancestor).unwrap().parent;
            }
        }
        let mut current = parent;
        while let Some(ancestor) = current {
            if requested.contains(&ancestor) {
                return Err(GridError::InvalidHierarchy(
                    "cannot move a node into its descendant",
                ));
            }
            current = self.node(ancestor).unwrap().parent;
        }
        self.revision.checked_add(1).ok_or(GridError::Capacity)?;
        Ok(())
    }
    pub fn move_rows(
        &mut self,
        rows: &[RowKey],
        target: TreeMoveTarget,
    ) -> Result<bool, GridError> {
        self.validate_node_move(rows, target)?;
        let parent = self.move_parent(target)?;
        let moved: HashSet<_> = rows.iter().copied().collect();
        let mut nodes: Vec<_> = self
            .nodes
            .iter()
            .filter(|n| !moved.contains(&n.key))
            .cloned()
            .collect();
        let index = match target {
            TreeMoveTarget::Before(key) => nodes.iter().position(|n| n.key == key).unwrap(),
            TreeMoveTarget::After(key) => nodes.iter().position(|n| n.key == key).unwrap() + 1,
            TreeMoveTarget::Into(_) => nodes
                .iter()
                .rposition(|n| n.parent == parent)
                .map_or(nodes.len(), |i| i + 1),
        };
        let moving = rows.iter().map(|&key| {
            let mut node = self.node(key).unwrap().clone();
            node.parent = parent;
            node
        });
        nodes.splice(index..index, moving);
        self.replace(nodes)
    }
}
impl TreeDataSource for IndexedTreeSource {
    fn child_count(&self, parent: Option<RowKey>) -> Option<usize> {
        if parent.is_some_and(|key| {
            self.node(key)
                .is_none_or(|n| n.children == TreeChildren::Unloaded)
        }) {
            return None;
        }
        Some(self.children.get(&parent).map_or(0, Vec::len))
    }
    fn child_at(&self, parent: Option<RowKey>, index: usize) -> Option<RowKey> {
        self.children.get(&parent)?.get(index).copied()
    }
    fn parent(&self, row: RowKey) -> Option<RowKey> {
        self.node(row)?.parent
    }
    fn position(&self, row: RowKey) -> Option<usize> {
        self.positions.get(&row).copied()
    }
    fn cell(&self, row: RowKey, column: ColumnId) -> String {
        self.node(row)
            .and_then(|n| n.cells.get(&column))
            .cloned()
            .unwrap_or_default()
    }
    fn revision(&self) -> u64 {
        self.revision
    }
    fn validate_move(&self, rows: &[RowKey], target: TreeMoveTarget) -> Result<(), String> {
        self.validate_node_move(rows, target)
            .map_err(|e| e.to_string())
    }
    fn move_nodes(&mut self, rows: &[RowKey], target: TreeMoveTarget) -> Result<(), String> {
        self.move_rows(rows, target)
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn revision_overflow_rejects_writes_without_changing_graph() {
        let mut source = IndexedTreeSource::new(vec![
            TreeNode::new(RowKey(1), None, "A"),
            TreeNode::new(RowKey(2), None, "B"),
        ])
        .unwrap();
        source.revision = u64::MAX;
        let before = source.nodes().to_vec();
        assert!(
            source
                .move_rows(&[RowKey(2)], TreeMoveTarget::Before(RowKey(1)))
                .is_err()
        );
        assert_eq!(source.nodes(), before);
        let mut changed = before.clone();
        changed[0].cells.insert(ColumnId(0), "changed".into());
        assert!(source.replace(changed).is_err());
        assert_eq!(source.nodes(), before);
        assert_eq!(source.revision(), u64::MAX);
    }
}
