//! Backend-independent docking organization and bounded layout geometry.
//!
//! Pane identifiers belong to the application. Moving a pane changes placement
//! only; this module never owns, recreates, or serializes its controls or content.

use rust_desktop_ui_core::{Axis, Rect, Size};
use std::collections::HashSet;
use std::{error::Error, fmt};
mod auto_hide;
pub use auto_hide::*;

pub const MAX_DOCK_PANES: usize = 64;
pub const MAX_DOCK_FLOATING_WINDOWS: usize = 16;
pub const MAX_DOCK_DEPTH: usize = 32;
pub const DOCK_SNAPSHOT_VERSION: u32 = 2;
pub const DOCK_TAB_HEIGHT: f32 = 32.0;
pub const DOCK_SPLITTER_WIDTH: f32 = 6.0;

/// An application-owned stable key, independent of a widget or native window.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DockPaneId(String);

impl DockPaneId {
    pub fn new(value: impl Into<String>) -> Result<Self, DockError> {
        let value = value.into();
        if value.is_empty() || value.len() > 128 || value.chars().any(char::is_control) {
            return Err(invalid(
                "Pane IDs require 1..=128 UTF-8 bytes without control characters",
            ));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for DockPaneId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DockGroupId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DockSplitId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DockWindowId(pub u64);

impl DockWindowId {
    pub const MAIN: Self = Self(0);
}

#[derive(Clone, Debug, PartialEq)]
pub struct DockPane {
    pub id: DockPaneId,
    pub title: String,
    pub closable: bool,
    /// Minimum content size, excluding the docking tab strip.
    pub min_size: Size,
}

#[derive(Clone, Debug, PartialEq)]
pub enum DockNode {
    Tabs {
        id: DockGroupId,
        panes: Vec<DockPaneId>,
        active: DockPaneId,
    },
    Split {
        id: DockSplitId,
        /// Horizontal means left/right children; Vertical means top/bottom.
        axis: Axis,
        /// Requested first-child share. Layout clamps to available minimums.
        ratio: f32,
        first: Box<DockNode>,
        second: Box<DockNode>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct DockFloating {
    pub id: DockWindowId,
    pub root: DockNode,
    /// Logical screen coordinates. Negative monitor origins are valid.
    pub bounds: Rect,
}

/// Versioned placement data. Document contents and widget IDs are excluded.
#[derive(Clone, Debug, PartialEq)]
pub struct DockSnapshot {
    pub version: u32,
    pub root: Option<DockNode>,
    pub floating: Vec<DockFloating>,
    pub closed: Vec<DockPaneId>,
    /// Auto-hide is presentation metadata; the original tab placement stays intact.
    pub auto_hide: Vec<DockAutoHide>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DockEdge {
    Left,
    Right,
    Top,
    Bottom,
}

impl DockEdge {
    fn axis(self) -> Axis {
        match self {
            Self::Left | Self::Right => Axis::Horizontal,
            Self::Top | Self::Bottom => Axis::Vertical,
        }
    }

    fn before(self) -> bool {
        matches!(self, Self::Left | Self::Top)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DockPlacement {
    /// Insertion slot in the target group's list before removing the source.
    Tab {
        group: DockGroupId,
        index: usize,
    },
    Split {
        group: DockGroupId,
        edge: DockEdge,
    },
    Root {
        window: DockWindowId,
        edge: DockEdge,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DockError(pub String);

impl fmt::Display for DockError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Error for DockError {}

fn invalid(message: &str) -> DockError {
    DockError(message.to_owned())
}

/// A validated docking layout. Every mutation either completes or changes nothing.
#[derive(Clone, Debug)]
pub struct DockLayout {
    panes: Vec<DockPane>,
    snapshot: DockSnapshot,
    next_group: Option<u64>,
    next_split: Option<u64>,
    next_window: Option<u64>,
    revealed: std::collections::HashMap<DockWindowId, DockPaneId>,
}

impl DockLayout {
    /// Panels omitted from the initial tree begin closed.
    pub fn new(panes: Vec<DockPane>, root: impl Into<Option<DockNode>>) -> Result<Self, DockError> {
        validate_catalog(&panes)?;
        let root = root.into();
        let mut open = HashSet::new();
        let mut pending: Vec<_> = root.iter().map(|node| (node, 1)).collect();
        let mut nodes = 0;
        while let Some((node, depth)) = pending.pop() {
            nodes += 1;
            if depth > MAX_DOCK_DEPTH || nodes > 2 * MAX_DOCK_PANES {
                return Err(invalid("Docking tree exceeds its size or depth limit"));
            }
            match node {
                DockNode::Tabs { panes, .. } => {
                    if panes.len() > MAX_DOCK_PANES {
                        return Err(invalid("Too many panes in a docking tab group"));
                    }
                    open.extend(panes.iter().cloned());
                }
                DockNode::Split { first, second, .. } => {
                    pending.extend([(first.as_ref(), depth + 1), (second.as_ref(), depth + 1)]);
                }
            }
        }
        let closed = panes
            .iter()
            .filter(|pane| !open.contains(&pane.id))
            .map(|pane| pane.id.clone())
            .collect();
        Self::from_snapshot(
            panes,
            DockSnapshot {
                version: DOCK_SNAPSHOT_VERSION,
                root,
                floating: Vec::new(),
                closed,
                auto_hide: Vec::new(),
            },
        )
    }

    pub fn from_snapshot(
        panes: Vec<DockPane>,
        mut snapshot: DockSnapshot,
    ) -> Result<Self, DockError> {
        migrate_snapshot(&mut snapshot)?;
        validate_catalog(&panes)?;
        let counters = validate_snapshot(&panes, &snapshot)?;
        Ok(Self {
            panes,
            snapshot,
            next_group: counters.0,
            next_split: counters.1,
            next_window: counters.2,
            revealed: Default::default(),
        })
    }

    pub fn panes(&self) -> &[DockPane] {
        &self.panes
    }

    pub fn pane(&self, id: &DockPaneId) -> Option<&DockPane> {
        self.panes.iter().find(|pane| &pane.id == id)
    }

    pub fn snapshot(&self) -> DockSnapshot {
        self.snapshot.clone()
    }

    pub fn floating(&self) -> &[DockFloating] {
        &self.snapshot.floating
    }

    pub fn root(&self, window: DockWindowId) -> Option<&DockNode> {
        if window == DockWindowId::MAIN {
            self.snapshot.root.as_ref()
        } else {
            self.snapshot
                .floating
                .iter()
                .find(|item| item.id == window)
                .map(|item| &item.root)
        }
    }

    pub fn group_for(&self, pane: &DockPaneId) -> Option<DockGroupId> {
        self.roots().find_map(|(_, root)| pane_group(root, pane))
    }

    pub fn window_for(&self, pane: &DockPaneId) -> Option<DockWindowId> {
        self.roots()
            .find_map(|(window, root)| pane_group(root, pane).map(|_| window))
    }

    pub fn pane_location(&self, pane: &DockPaneId) -> Option<(DockWindowId, DockGroupId)> {
        self.roots()
            .find_map(|(window, root)| pane_group(root, pane).map(|group| (window, group)))
    }

    /// Pane order follows the split tree and the tab order within each group.
    pub fn window_panes(&self, window: DockWindowId) -> Result<Vec<DockPaneId>, DockError> {
        if !self.has_window(window) {
            return Err(invalid("Unknown docking window"));
        }
        let mut panes = Vec::new();
        let mut pending: Vec<_> = self.root(window).into_iter().collect();
        while let Some(node) = pending.pop() {
            match node {
                DockNode::Tabs { panes: group, .. } => panes.extend(group.iter().cloned()),
                DockNode::Split { first, second, .. } => {
                    pending.push(second.as_ref());
                    pending.push(first.as_ref());
                }
            }
        }
        Ok(panes)
    }

    pub fn is_closed(&self, pane: &DockPaneId) -> bool {
        self.snapshot.closed.contains(pane)
    }

    pub fn restore_snapshot(&mut self, mut snapshot: DockSnapshot) -> Result<bool, DockError> {
        migrate_snapshot(&mut snapshot)?;
        let counters = validate_snapshot(&self.panes, &snapshot)?;
        let changed = self.snapshot != snapshot || !self.revealed.is_empty();
        self.snapshot = snapshot;
        self.revealed.clear();
        // Do not recycle IDs removed earlier in this live layout's lifetime.
        self.next_group = advance_counter(self.next_group, counters.0);
        self.next_split = advance_counter(self.next_split, counters.1);
        self.next_window = advance_counter(self.next_window, counters.2);
        Ok(changed)
    }

    pub fn activate(&mut self, pane: &DockPaneId) -> Result<bool, DockError> {
        if self.auto_hide(pane).is_some() {
            return self.reveal_auto_hide(pane);
        }
        let group = self
            .group_for(pane)
            .ok_or_else(|| invalid("Pane is not open"))?;
        let DockNode::Tabs { active, .. } = self.group_mut(group).expect("located live group")
        else {
            unreachable!()
        };
        let changed = active != pane;
        *active = pane.clone();
        Ok(changed)
    }

    /// Moves an open pane. Drop preview should call this on a cloned layout.
    pub fn move_pane(
        &mut self,
        pane: &DockPaneId,
        placement: DockPlacement,
    ) -> Result<bool, DockError> {
        let source_group = self
            .group_for(pane)
            .ok_or_else(|| invalid("Pane is not open"))?;
        self.validate_placement(placement)?;
        if let DockPlacement::Tab { group, index } = placement
            && group == source_group
        {
            let mut next = self.clone();
            next.unmark_auto_hide(pane);
            let DockNode::Tabs { panes, active, .. } =
                next.group_mut(group).expect("validated group")
            else {
                unreachable!()
            };
            let old = panes
                .iter()
                .position(|id| id == pane)
                .expect("located pane");
            let id = panes.remove(old);
            panes.insert(if index > old { index - 1 } else { index }, id);
            *active = pane.clone();
            return self.commit(next);
        }
        if let DockPlacement::Split { group, .. } = placement
            && group == source_group
            && matches!(self.group(group), Some(DockNode::Tabs { panes, .. }) if panes.len() == 1)
        {
            return self.set_auto_hide(pane, None);
        }
        if let DockPlacement::Root { window, .. } = placement
            && self.window_for(pane) == Some(window)
            && matches!(self.root(window), Some(DockNode::Tabs { panes, .. }) if panes.len() == 1)
        {
            return self.set_auto_hide(pane, None);
        }
        let mut next = self.clone();
        next.remove_open(pane);
        next.insert(pane.clone(), placement)?;
        self.commit(next)
    }

    /// Creates a floating container, or updates an already isolated floating pane.
    /// A closed pane may also be opened directly in a floating container.
    pub fn float_pane(
        &mut self,
        pane: &DockPaneId,
        bounds: Rect,
    ) -> Result<DockWindowId, DockError> {
        self.require_pane(pane)?;
        validate_floating_bounds(bounds)?;
        if let Some(window) = self.window_for(pane)
            && window != DockWindowId::MAIN
            && matches!(self.root(window), Some(DockNode::Tabs { panes, .. }) if panes.len() == 1)
        {
            self.set_auto_hide(pane, None)?;
            self.update_floating_bounds(window, bounds)?;
            return Ok(window);
        }
        if self.snapshot.floating.len() >= MAX_DOCK_FLOATING_WINDOWS {
            return Err(invalid("Too many floating docking windows"));
        }
        let mut next = self.clone();
        next.remove_open(pane);
        next.snapshot.closed.retain(|id| id != pane);
        let id = DockWindowId(allocate(&mut next.next_window)?);
        let root = next.single_group(pane.clone())?;
        next.snapshot
            .floating
            .push(DockFloating { id, root, bounds });
        self.commit(next)?;
        Ok(id)
    }

    /// Hides a closable pane. Its application-owned content remains alive.
    pub fn close(&mut self, pane: &DockPaneId) -> Result<bool, DockError> {
        if !self.require_pane(pane)?.closable {
            return Err(invalid("Pane cannot be closed"));
        }
        if self.is_closed(pane) {
            return Ok(false);
        }
        let mut next = self.clone();
        next.remove_open(pane);
        next.snapshot.closed.push(pane.clone());
        self.commit(next)
    }

    /// Returns an entire floating container to the main workspace, preserving
    /// its groups and split ratios. Suitable for a native floating-window close.
    pub fn redock_window(&mut self, window: DockWindowId) -> Result<bool, DockError> {
        self.redock_window_at(window, DockEdge::Right)
    }

    pub fn redock_window_at(
        &mut self,
        window: DockWindowId,
        edge: DockEdge,
    ) -> Result<bool, DockError> {
        let index = self
            .snapshot
            .floating
            .iter()
            .position(|item| item.id == window)
            .ok_or_else(|| invalid("Unknown floating docking window"))?;
        let mut next = self.clone();
        let floating = next.snapshot.floating.remove(index);
        next.revealed.remove(&window);
        next.snapshot.root = Some(if let Some(old) = next.snapshot.root.take() {
            split_node(
                DockSplitId(allocate(&mut next.next_split)?),
                edge,
                floating.root,
                old,
            )
        } else {
            floating.root
        });
        self.commit(next)
    }

    pub fn restore(
        &mut self,
        pane: &DockPaneId,
        placement: DockPlacement,
    ) -> Result<bool, DockError> {
        self.require_pane(pane)?;
        if !self.is_closed(pane) {
            return Err(invalid("Only a closed pane can be restored"));
        }
        self.validate_placement(placement)?;
        let mut next = self.clone();
        next.snapshot.closed.retain(|id| id != pane);
        next.insert(pane.clone(), placement)?;
        self.commit(next)
    }

    pub fn resize(&mut self, split: DockSplitId, ratio: f32) -> Result<bool, DockError> {
        validate_ratio(ratio)?;
        let root = self
            .snapshot
            .root
            .as_mut()
            .and_then(|node| find_split_mut(node, split))
            .or_else(|| {
                self.snapshot
                    .floating
                    .iter_mut()
                    .find_map(|item| find_split_mut(&mut item.root, split))
            })
            .ok_or_else(|| invalid("Unknown docking splitter"))?;
        let DockNode::Split { ratio: current, .. } = root else {
            unreachable!()
        };
        let changed = *current != ratio;
        *current = ratio;
        Ok(changed)
    }

    pub fn update_floating_bounds(
        &mut self,
        window: DockWindowId,
        bounds: Rect,
    ) -> Result<bool, DockError> {
        validate_floating_bounds(bounds)?;
        let item = self
            .snapshot
            .floating
            .iter_mut()
            .find(|item| item.id == window)
            .ok_or_else(|| invalid("Unknown floating docking window"))?;
        let changed = item.bounds != bounds;
        item.bounds = bounds;
        Ok(changed)
    }

    /// Arranges one window's workspace in local logical coordinates.
    /// Shrinking below its minimum never changes the saved split ratios.
    pub fn arrange(&self, window: DockWindowId, bounds: Rect) -> Result<DockGeometry, DockError> {
        if !bounds.is_valid() {
            return Err(invalid("Invalid docking workspace bounds"));
        }
        if !self.has_window(window) {
            return Err(invalid("Unknown docking window"));
        }
        let mut result = DockGeometry {
            window,
            bounds,
            minimum_size: Size::ZERO,
            groups: Vec::new(),
            splitters: Vec::new(),
            workspace: bounds,
            auto_hide_rails: Vec::new(),
            auto_hide_panel: None,
        };
        let workspace = self.arrange_auto_hide(window, bounds, &mut result);
        result.workspace = workspace;
        if let Some(root) = self.root(window).and_then(|root| self.visible_root(root)) {
            result.minimum_size = self.minimum(&root);
            self.arrange_node(&root, workspace, &mut result);
        }
        Ok(result)
    }

    fn roots(&self) -> impl Iterator<Item = (DockWindowId, &DockNode)> {
        self.snapshot
            .root
            .iter()
            .map(|root| (DockWindowId::MAIN, root))
            .chain(
                self.snapshot
                    .floating
                    .iter()
                    .map(|item| (item.id, &item.root)),
            )
    }

    fn require_pane(&self, pane: &DockPaneId) -> Result<&DockPane, DockError> {
        self.pane(pane)
            .ok_or_else(|| invalid("Unknown docking pane"))
    }

    fn has_window(&self, window: DockWindowId) -> bool {
        window == DockWindowId::MAIN || self.snapshot.floating.iter().any(|item| item.id == window)
    }

    fn group(&self, group: DockGroupId) -> Option<&DockNode> {
        self.roots().find_map(|(_, root)| find_group(root, group))
    }

    fn group_mut(&mut self, group: DockGroupId) -> Option<&mut DockNode> {
        self.snapshot
            .root
            .as_mut()
            .and_then(|root| find_group_mut(root, group))
            .or_else(|| {
                self.snapshot
                    .floating
                    .iter_mut()
                    .find_map(|item| find_group_mut(&mut item.root, group))
            })
    }

    fn validate_placement(&self, placement: DockPlacement) -> Result<(), DockError> {
        match placement {
            DockPlacement::Tab { group, index } => {
                let Some(DockNode::Tabs { panes, .. }) = self.group(group) else {
                    return Err(invalid("Unknown target tab group"));
                };
                if index > panes.len() {
                    return Err(invalid("Target tab index is out of range"));
                }
            }
            DockPlacement::Split { group, .. } => {
                if self.group(group).is_none() {
                    return Err(invalid("Unknown target tab group"));
                }
            }
            DockPlacement::Root { window, .. } => {
                if !self.has_window(window) {
                    return Err(invalid("Unknown target docking window"));
                }
            }
        }
        Ok(())
    }

    fn commit(&mut self, mut next: Self) -> Result<bool, DockError> {
        validate_snapshot(&next.panes, &next.snapshot)?;
        next.prune_revealed();
        let changed = self.snapshot != next.snapshot || self.revealed != next.revealed;
        *self = next;
        Ok(changed)
    }

    fn remove_open(&mut self, pane: &DockPaneId) {
        self.unmark_auto_hide(pane);
        self.snapshot.root = self
            .snapshot
            .root
            .take()
            .and_then(|root| remove_pane(root, pane));
        self.snapshot.floating = std::mem::take(&mut self.snapshot.floating)
            .into_iter()
            .filter_map(|item| {
                remove_pane(item.root, pane).map(|root| DockFloating { root, ..item })
            })
            .collect();
    }

    fn single_group(&mut self, pane: DockPaneId) -> Result<DockNode, DockError> {
        Ok(DockNode::Tabs {
            id: DockGroupId(allocate(&mut self.next_group)?),
            panes: vec![pane.clone()],
            active: pane,
        })
    }

    fn insert(&mut self, pane: DockPaneId, placement: DockPlacement) -> Result<(), DockError> {
        if let DockPlacement::Tab { group, index } = placement {
            let Some(DockNode::Tabs { panes, active, .. }) = self.group_mut(group) else {
                return Err(invalid("Target tab group is no longer available"));
            };
            if index > panes.len() {
                return Err(invalid("Target tab index is out of range"));
            }
            panes.insert(index, pane.clone());
            *active = pane;
            return Ok(());
        }
        let fresh = self.single_group(pane)?;
        match placement {
            DockPlacement::Split { group, edge } => {
                let split = DockSplitId(allocate(&mut self.next_split)?);
                let target = self
                    .group_mut(group)
                    .ok_or_else(|| invalid("Target tab group is no longer available"))?;
                let old = std::mem::replace(target, fresh.clone());
                *target = split_node(split, edge, fresh, old);
            }
            DockPlacement::Root { window, edge } => {
                if window == DockWindowId::MAIN {
                    self.snapshot.root = Some(if let Some(old) = self.snapshot.root.take() {
                        split_node(
                            DockSplitId(allocate(&mut self.next_split)?),
                            edge,
                            fresh,
                            old,
                        )
                    } else {
                        fresh
                    });
                } else {
                    let split = DockSplitId(allocate(&mut self.next_split)?);
                    let target = self
                        .snapshot
                        .floating
                        .iter_mut()
                        .find(|item| item.id == window)
                        .ok_or_else(|| {
                            invalid("Target floating docking window is no longer available")
                        })?;
                    let old = std::mem::replace(&mut target.root, fresh.clone());
                    target.root = split_node(split, edge, fresh, old);
                }
            }
            DockPlacement::Tab { .. } => unreachable!(),
        }
        Ok(())
    }

    fn minimum(&self, node: &DockNode) -> Size {
        match node {
            DockNode::Tabs { panes, .. } => {
                let mut minimum = Size::new(0.0, DOCK_TAB_HEIGHT);
                for pane in panes.iter().filter_map(|id| self.pane(id)) {
                    minimum.width = minimum.width.max(pane.min_size.width);
                    minimum.height = minimum.height.max(pane.min_size.height + DOCK_TAB_HEIGHT);
                }
                minimum
            }
            DockNode::Split {
                axis,
                first,
                second,
                ..
            } => {
                let first = self.minimum(first);
                let second = self.minimum(second);
                match axis {
                    Axis::Horizontal => Size::new(
                        first.width + second.width + DOCK_SPLITTER_WIDTH,
                        first.height.max(second.height),
                    ),
                    Axis::Vertical => Size::new(
                        first.width.max(second.width),
                        first.height + second.height + DOCK_SPLITTER_WIDTH,
                    ),
                }
            }
        }
    }

    fn arrange_node(&self, node: &DockNode, bounds: Rect, output: &mut DockGeometry) {
        match node {
            DockNode::Tabs { id, panes, active } => {
                let header = DOCK_TAB_HEIGHT.min(bounds.height);
                let full = match self.group(*id) {
                    Some(DockNode::Tabs { panes, .. }) => panes.as_slice(),
                    _ => panes.as_slice(),
                };
                let insertion_slots = panes
                    .iter()
                    .map(|pane| {
                        full.iter()
                            .position(|id| id == pane)
                            .expect("visible pane belongs to model group")
                    })
                    .chain(std::iter::once(full.len()))
                    .collect();
                output.groups.push(DockGroupGeometry {
                    id: *id,
                    panes: panes.clone(),
                    insertion_slots,
                    active: active.clone(),
                    bounds,
                    tab_strip: Rect::new(bounds.x, bounds.y, bounds.width, header),
                    content: Rect::new(
                        bounds.x,
                        bounds.y + header,
                        bounds.width,
                        (bounds.height - header).max(0.0),
                    ),
                });
            }
            DockNode::Split {
                id,
                axis,
                ratio,
                first,
                second,
            } => {
                let first_min = self.minimum(first);
                let second_min = self.minimum(second);
                let (length, minimum_first, minimum_second) = match axis {
                    Axis::Horizontal => (bounds.width, first_min.width, second_min.width),
                    Axis::Vertical => (bounds.height, first_min.height, second_min.height),
                };
                let divider = DOCK_SPLITTER_WIDTH.min(length);
                let available = (length - divider).max(0.0);
                let total_minimum = minimum_first + minimum_second;
                let size = if available >= total_minimum {
                    // Rounding the sum and then subtracting the second minimum
                    // can place the upper bound one ULP below the first minimum.
                    let upper = (available - minimum_second).max(minimum_first);
                    (available * ratio).clamp(minimum_first, upper)
                } else if total_minimum > 0.0 {
                    available * (minimum_first / total_minimum)
                } else {
                    available * ratio
                };
                let remainder = (available - size).max(0.0);
                let (a, divider_bounds, b) = match axis {
                    Axis::Horizontal => (
                        Rect::new(bounds.x, bounds.y, size, bounds.height),
                        Rect::new(bounds.x + size, bounds.y, divider, bounds.height),
                        Rect::new(
                            bounds.x + size + divider,
                            bounds.y,
                            remainder,
                            bounds.height,
                        ),
                    ),
                    Axis::Vertical => (
                        Rect::new(bounds.x, bounds.y, bounds.width, size),
                        Rect::new(bounds.x, bounds.y + size, bounds.width, divider),
                        Rect::new(bounds.x, bounds.y + size + divider, bounds.width, remainder),
                    ),
                };
                output.splitters.push(DockSplitterGeometry {
                    id: *id,
                    axis: *axis,
                    ratio: *ratio,
                    effective_ratio: if available > 0.0 {
                        size / available
                    } else {
                        *ratio
                    },
                    bounds: divider_bounds,
                    container: bounds,
                });
                self.arrange_node(first, a, output);
                self.arrange_node(second, b, output);
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct DockGeometry {
    pub window: DockWindowId,
    pub bounds: Rect,
    pub minimum_size: Size,
    pub groups: Vec<DockGroupGeometry>,
    pub splitters: Vec<DockSplitterGeometry>,
    /// Pinned pane area after reserving edge rails.
    pub workspace: Rect,
    pub auto_hide_rails: Vec<DockAutoHideRailGeometry>,
    /// Transient overlay; never serialized in DockSnapshot.
    pub auto_hide_panel: Option<DockAutoHidePanelGeometry>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DockGroupGeometry {
    pub id: DockGroupId,
    pub panes: Vec<DockPaneId>,
    /// Model insertion indices before each visible tab, followed by the end slot.
    /// Hidden edge tabs remain in the model and are therefore counted here.
    pub insertion_slots: Vec<usize>,
    pub active: DockPaneId,
    pub bounds: Rect,
    pub tab_strip: Rect,
    pub content: Rect,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DockSplitterGeometry {
    pub id: DockSplitId,
    pub axis: Axis,
    pub ratio: f32,
    pub effective_ratio: f32,
    pub bounds: Rect,
    pub container: Rect,
}

fn validate_catalog(panes: &[DockPane]) -> Result<(), DockError> {
    if panes.len() > MAX_DOCK_PANES {
        return Err(invalid("Too many docking panes"));
    }
    let mut ids = HashSet::new();
    for pane in panes {
        if !ids.insert(&pane.id) {
            return Err(invalid("Duplicate pane ID in the docking catalog"));
        }
        if pane.title.len() > 4096
            || !pane.min_size.is_valid()
            || pane.min_size.width > 32768.0
            || pane.min_size.height > 32768.0
        {
            return Err(invalid("Invalid docking pane title or minimum size"));
        }
    }
    Ok(())
}

type Counters = (Option<u64>, Option<u64>, Option<u64>);

fn validate_snapshot(panes: &[DockPane], snapshot: &DockSnapshot) -> Result<Counters, DockError> {
    if snapshot.version != DOCK_SNAPSHOT_VERSION {
        return Err(invalid("Unsupported docking snapshot version"));
    }
    if snapshot.floating.len() > MAX_DOCK_FLOATING_WINDOWS || snapshot.closed.len() > MAX_DOCK_PANES
    {
        return Err(invalid("Docking snapshot exceeds its container limits"));
    }
    let catalog: HashSet<_> = panes.iter().map(|pane| &pane.id).collect();
    let mut seen = HashSet::new();
    let mut groups = HashSet::new();
    let mut splits = HashSet::new();
    let mut windows = HashSet::new();
    let mut pending: Vec<_> = snapshot.root.iter().map(|node| (node, 1)).collect();
    for item in &snapshot.floating {
        validate_floating_bounds(item.bounds)?;
        if item.id == DockWindowId::MAIN || !windows.insert(item.id.0) {
            return Err(invalid("Invalid or duplicate floating window ID"));
        }
        pending.push((&item.root, 1));
    }
    let mut nodes = 0;
    while let Some((node, depth)) = pending.pop() {
        nodes += 1;
        if depth > MAX_DOCK_DEPTH || nodes > 2 * MAX_DOCK_PANES {
            return Err(invalid("Docking tree exceeds its size or depth limit"));
        }
        match node {
            DockNode::Tabs { id, panes, active } => {
                if !groups.insert(id.0)
                    || panes.is_empty()
                    || panes.len() > MAX_DOCK_PANES
                    || !panes.contains(active)
                {
                    return Err(invalid("Invalid or duplicate docking tab group"));
                }
                for pane in panes {
                    if !catalog.contains(pane) || !seen.insert(pane) {
                        return Err(invalid("Unknown or multiply placed docking pane"));
                    }
                }
            }
            DockNode::Split {
                id,
                ratio,
                first,
                second,
                ..
            } => {
                validate_ratio(*ratio)?;
                if !splits.insert(id.0) {
                    return Err(invalid("Duplicate docking split ID"));
                }
                pending.extend([(first.as_ref(), depth + 1), (second.as_ref(), depth + 1)]);
            }
        }
    }
    for pane in &snapshot.closed {
        if !catalog.contains(pane) || !seen.insert(pane) {
            return Err(invalid("Unknown or multiply placed closed docking pane"));
        }
    }
    if seen.len() != catalog.len() {
        return Err(invalid("Docking snapshot omits registered panes"));
    }
    validate_auto_hide(snapshot)?;
    Ok((next_after(groups), next_after(splits), next_after(windows)))
}

fn migrate_snapshot(snapshot: &mut DockSnapshot) -> Result<(), DockError> {
    if snapshot.version == 1 {
        if !snapshot.auto_hide.is_empty() {
            return Err(invalid("Version one cannot contain auto-hide metadata"));
        }
        snapshot.version = DOCK_SNAPSHOT_VERSION;
    }
    Ok(())
}

fn next_after(ids: HashSet<u64>) -> Option<u64> {
    ids.into_iter().max().unwrap_or(0).checked_add(1)
}

fn advance_counter(previous: Option<u64>, requested: Option<u64>) -> Option<u64> {
    previous.zip(requested).map(|(a, b)| a.max(b))
}

fn allocate(counter: &mut Option<u64>) -> Result<u64, DockError> {
    let value = counter.ok_or_else(|| invalid("Docking identifier space is exhausted"))?;
    *counter = value.checked_add(1);
    Ok(value)
}

fn validate_ratio(ratio: f32) -> Result<(), DockError> {
    if !ratio.is_finite() || !(0.0..1.0).contains(&ratio) || ratio == 0.0 {
        return Err(invalid(
            "Docking split ratio must be finite and strictly between zero and one",
        ));
    }
    Ok(())
}

fn validate_floating_bounds(bounds: Rect) -> Result<(), DockError> {
    if !bounds.is_valid() || bounds.width <= 0.0 || bounds.height <= 0.0 {
        return Err(invalid(
            "Floating window bounds require finite coordinates and positive size",
        ));
    }
    Ok(())
}

fn pane_group(node: &DockNode, pane: &DockPaneId) -> Option<DockGroupId> {
    match node {
        DockNode::Tabs { id, panes, .. } => panes.contains(pane).then_some(*id),
        DockNode::Split { first, second, .. } => {
            pane_group(first, pane).or_else(|| pane_group(second, pane))
        }
    }
}

fn find_group(node: &DockNode, group: DockGroupId) -> Option<&DockNode> {
    match node {
        DockNode::Tabs { id, .. } => (*id == group).then_some(node),
        DockNode::Split { first, second, .. } => {
            find_group(first, group).or_else(|| find_group(second, group))
        }
    }
}

fn find_group_mut(node: &mut DockNode, group: DockGroupId) -> Option<&mut DockNode> {
    match node {
        DockNode::Tabs { id, .. } if *id == group => Some(node),
        DockNode::Split { first, second, .. } => {
            find_group_mut(first, group).or_else(|| find_group_mut(second, group))
        }
        _ => None,
    }
}

fn find_split_mut(node: &mut DockNode, split: DockSplitId) -> Option<&mut DockNode> {
    match node {
        DockNode::Split { id, .. } if *id == split => Some(node),
        DockNode::Split { first, second, .. } => {
            find_split_mut(first, split).or_else(|| find_split_mut(second, split))
        }
        _ => None,
    }
}

fn split_node(id: DockSplitId, edge: DockEdge, fresh: DockNode, old: DockNode) -> DockNode {
    let (first, second) = if edge.before() {
        (fresh, old)
    } else {
        (old, fresh)
    };
    DockNode::Split {
        id,
        axis: edge.axis(),
        ratio: 0.5,
        first: Box::new(first),
        second: Box::new(second),
    }
}

fn remove_pane(node: DockNode, pane: &DockPaneId) -> Option<DockNode> {
    match node {
        DockNode::Tabs {
            id,
            mut panes,
            mut active,
        } => {
            if let Some(index) = panes.iter().position(|id| id == pane) {
                panes.remove(index);
                if panes.is_empty() {
                    return None;
                }
                if &active == pane {
                    active = panes[index.min(panes.len() - 1)].clone();
                }
            }
            Some(DockNode::Tabs { id, panes, active })
        }
        DockNode::Split {
            id,
            axis,
            ratio,
            first,
            second,
        } => match (remove_pane(*first, pane), remove_pane(*second, pane)) {
            (Some(first), Some(second)) => Some(DockNode::Split {
                id,
                axis,
                ratio,
                first: Box::new(first),
                second: Box::new(second),
            }),
            (Some(node), None) | (None, Some(node)) => Some(node),
            (None, None) => None,
        },
    }
}

#[cfg(test)]
#[path = "model_tests.rs"]
mod tests;
