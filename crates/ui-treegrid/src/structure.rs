//! Structural operations share the same sparse TreeGrid model.
use crate::{GridError, IndexedTreeSource, RequestId, RowKey, TreeDataSource, TreeGrid, TreeNode};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TreeMoveTarget {
    Before(RowKey),
    After(RowKey),
    /// Append inside a loaded parent; None appends to roots.
    Into(Option<RowKey>),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TreeScrollAnchor {
    pub key: Option<RowKey>,
    pub fallback_index: usize,
    pub within_row: f64,
}

impl TreeGrid {
    pub fn capture_scroll_anchor(&self, source: &impl TreeDataSource) -> TreeScrollAnchor {
        let height = f64::from(self.config().row_height);
        let index = (self.scroll_offset().1 / height).floor() as usize;
        TreeScrollAnchor {
            key: self
                .row(source, index)
                .ok()
                .flatten()
                .and_then(|r| r.key.or(r.parent)),
            fallback_index: index,
            within_row: self.scroll_offset().1 % height,
        }
    }
    /// Surviving keys retain their viewport location; a hidden key resolves to
    /// its nearest visible ancestor. Deleted keys use the clamped previous index.
    pub fn restore_scroll_anchor(
        &mut self,
        source: &impl TreeDataSource,
        anchor: TreeScrollAnchor,
    ) -> Result<(), GridError> {
        if !anchor.within_row.is_finite()
            || anchor.within_row < 0.0
            || anchor.within_row >= f64::from(self.config().row_height)
        {
            return Err(GridError::InvalidGeometry("invalid tree anchor fraction"));
        }
        let mut current = anchor.key;
        let mut index = None;
        let mut seen = std::collections::HashSet::new();
        while let Some(key) = current {
            if !seen.insert(key) || seen.len() > 256 {
                break;
            }
            if let Some(position) = self.visible_index(source, key) {
                index = Some(position);
                break;
            }
            current = self.parent_key(source, key);
        }
        let index = index
            .unwrap_or(anchor.fallback_index)
            .min(self.row_count().saturating_sub(1));
        let y = if self.row_count() == 0 {
            0.0
        } else {
            index as f64 * f64::from(self.config().row_height) + anchor.within_row
        };
        self.scroll_to(self.scroll_offset().0, y)
    }
    /// Atomically installs a direct-child reply into the built-in accepted
    /// source. Stale/cancelled/foreign tokens cannot modify either source or model.
    pub fn complete_indexed_children(
        &mut self,
        source: &mut IndexedTreeSource,
        request: RequestId,
        children: Vec<TreeNode>,
    ) -> Result<bool, GridError> {
        if !self.pending_requests().contains(&request) {
            return Ok(false);
        }
        let anchor = self.capture_scroll_anchor(source);
        let keys = children.iter().map(|n| n.key).collect();
        let mut candidate = source.clone();
        candidate.replace_children(request.parent(), children)?;
        if !self.complete_children(&candidate, request, Ok(keys))? {
            return Ok(false);
        }
        *source = candidate;
        self.restore_scroll_anchor(source, anchor)?;
        Ok(true)
    }
}
