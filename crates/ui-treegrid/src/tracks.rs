//! Optional physical tracks for columns stacked within a band and fixed trailing columns.
use super::*;
use rust_desktop_ui_core::Rect;
use std::collections::{HashMap, HashSet};
pub(crate) struct Slot {
    pub zone: usize,
    pub start: f64,
    pub width: f64,
    row: usize,
    rows: usize,
}
pub(crate) struct ColumnTracks {
    stack: HashMap<ColumnId, ColumnId>,
    right: HashSet<ColumnId>,
    pub slots: HashMap<ColumnId, Slot>,
    pub widths: [f64; 3],
}
impl ColumnTracks {
    pub fn rebuild(&mut self, columns: &[GridColumn]) {
        self.slots.clear();
        self.widths = [0.; 3];
        let mut tracks: Vec<(usize, ColumnId, f64, Vec<ColumnId>)> = Vec::new();
        for col in columns {
            let zone = if self.right.contains(&col.id) {
                2
            } else if col.pinned {
                0
            } else {
                1
            };
            let track = self.stack.get(&col.id).copied().unwrap_or(col.id);
            if let Some((_, _, width, ids)) = tracks
                .iter_mut()
                .find(|(z, id, _, _)| *z == zone && *id == track)
            {
                *width = width.max(f64::from(col.width));
                ids.push(col.id);
            } else {
                tracks.push((zone, track, f64::from(col.width), vec![col.id]));
            }
        }
        for (zone, root, width, mut ids) in tracks {
            if let Some(root_index) = ids.iter().position(|id| *id == root) {
                ids[..=root_index].rotate_right(1);
            }
            let rows = ids.len();
            for (row, id) in ids.into_iter().enumerate() {
                self.slots.insert(
                    id,
                    Slot {
                        zone,
                        start: self.widths[zone],
                        width,
                        row,
                        rows,
                    },
                );
            }
            self.widths[zone] += width;
        }
    }
    pub fn visible(&self, grid: &TreeGrid, bounds: Rect) -> Vec<VisibleColumn> {
        let left = (self.widths[0] as f32).min(bounds.width);
        let right = (self.widths[2] as f32).min(bounds.width - left);
        let mid = bounds.width - left - right;
        let origins = [
            bounds.x,
            bounds.x + left - grid.scroll_x as f32,
            bounds.x + bounds.width - self.widths[2] as f32,
        ];
        let clips = [
            Rect::new(bounds.x, bounds.y, left, bounds.height),
            Rect::new(bounds.x + left, bounds.y, mid, bounds.height),
            Rect::new(
                bounds.x + bounds.width - right,
                bounds.y,
                right,
                bounds.height,
            ),
        ];
        grid.columns
            .iter()
            .enumerate()
            .filter_map(|(index, col)| {
                let slot = &self.slots[&col.id];
                let rect = Rect::new(
                    origins[slot.zone] + slot.start as f32,
                    bounds.y,
                    slot.width as f32,
                    bounds.height,
                );
                let clip = clips[slot.zone].intersection(rect)?;
                (clip.width > 0.).then_some(VisibleColumn {
                    id: col.id,
                    index,
                    bounds: rect,
                    clip,
                    pinned: slot.zone != 1,
                })
            })
            .collect()
    }
}
impl TreeGrid {
    pub(crate) fn header_track(&self, column: &GridColumn) -> (usize, ColumnId) {
        match &self.tracks {
            Some(tracks) => (
                tracks.slots[&column.id].zone,
                tracks.stack.get(&column.id).copied().unwrap_or(column.id),
            ),
            None => (usize::from(!column.pinned), column.id),
        }
    }

    /// Stack fields onto a shared horizontal track. Right-fixed fields stay at the trailing edge.
    pub fn set_column_tracks(
        &mut self,
        stack: HashMap<ColumnId, ColumnId>,
        right: HashSet<ColumnId>,
    ) -> Result<(), GridError> {
        if stack.len() > 64
            || right.len() > 64
            || (!stack.is_empty() || !right.is_empty()) && self.columns.len() > 64
        {
            return Err(GridError::Capacity);
        }
        if stack
            .iter()
            .any(|(id, parent)| id == parent || stack.contains_key(parent))
        {
            return Err(GridError::InvalidColumn("ambiguous stack root"));
        }
        self.tracks = if stack.is_empty() && right.is_empty() {
            None
        } else {
            let mut tracks = ColumnTracks {
                stack,
                right,
                slots: HashMap::new(),
                widths: [0.; 3],
            };
            tracks.rebuild(&self.columns);
            Some(tracks)
        };
        self.clamp_scroll();
        Ok(())
    }
    /// Widths of the scrolling region and both fixed regions in logical pixels.
    pub fn column_extents(&self) -> (f64, f64) {
        self.tracks.as_ref().map_or(
            (
                self.column_ends.last().copied().unwrap_or(0.),
                self.pinned_width,
            ),
            |t| (t.widths[1], t.widths[0] + t.widths[2]),
        )
    }
    /// Logical cell order shared by keyboard navigation and accessibility.
    pub fn display_column_order(&self) -> Vec<ColumnId> {
        if let Some(tracks) = &self.tracks {
            let mut order: Vec<_> = self.columns.iter().map(|c| c.id).collect();
            order.sort_by(|a, b| {
                let (a, b) = (&tracks.slots[a], &tracks.slots[b]);
                a.zone
                    .cmp(&b.zone)
                    .then_with(|| a.start.total_cmp(&b.start))
                    .then(a.row.cmp(&b.row))
            });
            order
        } else {
            self.pinned
                .iter()
                .chain(&self.regular)
                .map(|i| self.columns[*i].id)
                .collect()
        }
    }
    pub fn column_scroll_start(&self, id: ColumnId) -> Option<f64> {
        if let Some(t) = &self.tracks {
            let s = t.slots.get(&id)?;
            return Some(if s.zone == 1 { s.start } else { self.scroll_x });
        }
        let position = self.regular.iter().position(|i| self.columns[*i].id == id);
        if let Some(p) = position {
            Some(if p == 0 { 0. } else { self.column_ends[p - 1] })
        } else {
            self.columns
                .iter()
                .any(|c| c.id == id)
                .then_some(self.scroll_x)
        }
    }
    /// Map a column track to its field's logical cell within a row, including stacked fields.
    pub fn track_cell(&self, id: ColumnId, x: f32, y: f32, width: f32, height: f32) -> Rect {
        if let Some(s) = self.tracks.as_ref().and_then(|t| t.slots.get(&id)) {
            let h = height / s.rows as f32;
            Rect::new(x, y + s.row as f32 * h, width, h)
        } else {
            Rect::new(x, y, width, height)
        }
    }
}
