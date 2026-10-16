//! Reusable, validated structural descriptions, independent of control data.

use std::collections::HashMap;

use crate::{NodeProps, TreeError, UiTree, WidgetId};

/// A template node uses a preceding node's index as parent. The first node is
/// the sole root and has no parent. Names are local to each instantiation.
#[derive(Debug, Clone, PartialEq)]
pub struct TemplateNode {
    pub name: Option<String>,
    pub parent: Option<usize>,
    pub props: NodeProps,
}

/// Validated reusable tree structure. Contains no runtime IDs or callbacks.
#[derive(Debug, Clone)]
pub struct SubtreeTemplate {
    nodes: Vec<TemplateNode>,
}

/// IDs of one template instance. Cloning the handle does not clone ownership:
/// `remove(root)` disposes the instance and invalidates all copies of its IDs.
#[derive(Debug, Clone)]
pub struct MountedSubtree {
    root: WidgetId,
    nodes: Vec<WidgetId>,
    names: HashMap<String, WidgetId>,
}

impl SubtreeTemplate {
    pub fn new(nodes: Vec<TemplateNode>) -> Result<Self, TreeError> {
        if nodes.is_empty() {
            return Err(TreeError::InvalidTemplate("a root node is required"));
        }
        let mut names = std::collections::HashSet::new();
        for (index, node) in nodes.iter().enumerate() {
            if (index == 0 && node.parent.is_some())
                || (index > 0 && node.parent.is_none_or(|parent| parent >= index))
            {
                return Err(TreeError::InvalidTemplate(
                    "parents must precede their children",
                ));
            }
            node.props.validate().map_err(TreeError::InvalidProps)?;
            if let Some(name) = &node.name
                && (name.is_empty() || !names.insert(name))
            {
                return Err(TreeError::InvalidTemplate(
                    "names must be nonempty and unique",
                ));
            }
        }
        Ok(Self { nodes })
    }

    pub fn nodes(&self) -> &[TemplateNode] {
        &self.nodes
    }
}

impl MountedSubtree {
    pub fn root(&self) -> WidgetId {
        self.root
    }
    pub fn nodes(&self) -> &[WidgetId] {
        &self.nodes
    }
    pub fn named(&self, name: &str) -> Option<WidgetId> {
        self.names.get(name).copied()
    }
    /// Structural edits may invalidate individual parts without invalidating
    /// this handle. Check before using it after edits outside the component.
    pub fn is_live(&self, tree: &UiTree) -> bool {
        self.nodes.iter().all(|id| tree.contains(*id))
    }
}

impl UiTree {
    /// Instantiate the entire validated description atomically. Every instance
    /// receives fresh IDs, even when inserted under the same parent.
    pub fn instantiate(
        &mut self,
        parent: WidgetId,
        template: &SubtreeTemplate,
    ) -> Result<MountedSubtree, TreeError> {
        if !self.contains(parent) {
            return Err(TreeError::InvalidId(parent));
        }
        let mut next = self.snapshot();
        let mut nodes = Vec::with_capacity(template.nodes.len());
        let mut names = HashMap::new();
        for node in &template.nodes {
            let parent = node.parent.map_or(parent, |index| nodes[index]);
            let id = next.insert(parent, node.props.clone())?;
            nodes.push(id);
            if let Some(name) = &node.name {
                names.insert(name.clone(), id);
            }
        }
        *self = next;
        Ok(MountedSubtree {
            root: nodes[0],
            nodes,
            names,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn template() -> SubtreeTemplate {
        SubtreeTemplate::new(vec![
            TemplateNode {
                name: Some("root".into()),
                parent: None,
                props: NodeProps::default(),
            },
            TemplateNode {
                name: Some("content".into()),
                parent: Some(0),
                props: NodeProps::default(),
            },
        ])
        .unwrap()
    }

    #[test]
    fn instances_are_independent_detachable_and_disposable() {
        let mut tree = UiTree::new();
        let first = tree.instantiate(tree.root(), &template()).unwrap();
        let second = tree.instantiate(tree.root(), &template()).unwrap();
        assert_ne!(first.named("content"), second.named("content"));
        tree.detach(first.root()).unwrap();
        assert!(first.is_live(&tree));
        assert!(!tree.is_attached(first.named("content").unwrap()));
        assert_eq!(
            tree.preorder(),
            vec![tree.root(), second.root(), second.named("content").unwrap()]
        );
        tree.clear_dirty();
        tree.set_paint(
            first.root(),
            crate::Paint {
                background: Some(crate::Color::rgb(1, 2, 3)),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(tree.pending_dirty().is_empty());
        tree.reparent(first.root(), second.root()).unwrap();
        assert!(tree.is_attached(first.named("content").unwrap()));
        tree.remove(first.root()).unwrap();
        assert!(!first.is_live(&tree));
        assert!(second.is_live(&tree));
        let third = tree.instantiate(tree.root(), &template()).unwrap();
        assert!(third.nodes().iter().all(|id| !first.nodes().contains(id)));
    }

    #[test]
    fn invalid_descriptions_never_mount_partial_content() {
        let valid = template();
        let mut duplicate = valid.nodes().to_vec();
        duplicate[1].name = duplicate[0].name.clone();
        assert!(SubtreeTemplate::new(duplicate).is_err());
        let mut cycle = valid.nodes().to_vec();
        cycle[1].parent = Some(1);
        assert!(SubtreeTemplate::new(cycle).is_err());
        assert!(SubtreeTemplate::new(Vec::new()).is_err());
        let mut tree = UiTree::new();
        tree.clear_dirty();
        let before = tree.preorder();
        assert!(tree.instantiate(UiTree::new().root(), &valid).is_err());
        assert_eq!(tree.preorder(), before);
        assert!(tree.pending_dirty().is_empty());
    }
}
