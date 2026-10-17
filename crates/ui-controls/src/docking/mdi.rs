//! Bounded client-area child window geometry over existing docking pane identities.
use super::model::*;
use rust_desktop_ui_core::{Point, Rect, Size};
use std::collections::BTreeMap;
#[cfg(test)]
mod tests;
mod view;
pub use view::*;
pub const DOCK_MDI_HEADER_HEIGHT: f32 = 32.;
#[derive(Clone, Debug, PartialEq)]
pub struct DockMdiFrame {
    pub pane: DockPaneId,
    pub bounds: Rect,
    pub header: Rect,
    pub content: Rect,
}
#[derive(Clone, Debug, PartialEq)]
pub struct DockMdiGeometry {
    pub window: DockWindowId,
    pub bounds: Rect,
    pub frames: Vec<DockMdiFrame>,
}
impl DockMdiGeometry {
    pub fn frame_at(&self, p: Point) -> Option<&DockMdiFrame> {
        self.frames
            .iter()
            .rev()
            .find(|f| f.bounds.contains(p.x, p.y))
    }
    /// Visible content rectangles, after subtracting all front child frames.
    pub fn visible_content(&self, id: &DockPaneId) -> Vec<Rect> {
        let Some(index) = self.frames.iter().position(|f| &f.pane == id) else {
            return Vec::new();
        };
        let mut rects = vec![self.frames[index].content];
        for front in &self.frames[index + 1..] {
            rects = rects
                .into_iter()
                .flat_map(|r| subtract(r, front.bounds))
                .collect();
        }
        rects
    }
    pub fn as_dock_geometry(&self) -> DockGeometry {
        DockGeometry {
            window: self.window,
            bounds: self.bounds,
            minimum_size: Size::new(160., 96.),
            workspace: self.bounds,
            groups: self
                .frames
                .iter()
                .enumerate()
                .map(|(i, f)| DockGroupGeometry {
                    id: DockGroupId(i as u64 + 1),
                    panes: vec![f.pane.clone()],
                    insertion_slots: vec![0, 1],
                    active: f.pane.clone(),
                    bounds: f.bounds,
                    tab_strip: f.header,
                    content: f.content,
                })
                .collect(),
            splitters: Vec::new(),
            auto_hide_rails: Vec::new(),
            auto_hide_panel: None,
        }
    }
}
fn subtract(a: Rect, b: Rect) -> Vec<Rect> {
    let Some(i) = a.intersection(b) else {
        return vec![a];
    };
    let mut r = Vec::new();
    for candidate in [
        Rect::new(a.x, a.y, a.width, i.y - a.y),
        Rect::new(
            a.x,
            i.y + i.height,
            a.width,
            a.y + a.height - i.y - i.height,
        ),
        Rect::new(a.x, i.y, i.x - a.x, i.height),
        Rect::new(i.x + i.width, i.y, a.x + a.width - i.x - i.width, i.height),
    ] {
        if candidate.width > 0. && candidate.height > 0. {
            r.push(candidate);
        }
    }
    r
}
#[derive(Clone, Debug, Default)]
pub struct DockMdiLayout {
    rects: BTreeMap<DockPaneId, Rect>,
    order: Vec<DockPaneId>,
    maximized: Option<DockPaneId>,
    revision: u64,
}
fn invalid(s: &str) -> DockError {
    DockError(s.into())
}
fn valid_rect(r: Rect) -> bool {
    r.x.is_finite()
        && r.y.is_finite()
        && r.width.is_finite()
        && r.height.is_finite()
        && r.x >= 0.
        && r.y >= 0.
        && r.width >= 1.
        && r.height >= 1.
        && r.x + r.width <= 32768.
        && r.y + r.height <= 32768.
}
impl DockMdiLayout {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn order(&self) -> &[DockPaneId] {
        &self.order
    }
    pub fn stored_bounds(&self, id: &DockPaneId) -> Option<Rect> {
        self.rects.get(id).copied()
    }
    pub fn maximized(&self) -> Option<&DockPaneId> {
        self.maximized.as_ref()
    }
    fn next(&self) -> Result<u64, DockError> {
        self.revision
            .checked_add(1)
            .ok_or_else(|| invalid("MDI revision exhausted"))
    }
    pub(super) fn validate_layout(
        &self,
        layout: &DockLayout,
        window: DockWindowId,
    ) -> Result<(), DockError> {
        for id in &self.order {
            let pane = layout
                .pane(id)
                .ok_or_else(|| invalid("MDI pane was removed"))?;
            let r = self
                .rects
                .get(id)
                .ok_or_else(|| invalid("Missing MDI frame rectangle"))?;
            if layout.window_for(id) != Some(window)
                || !valid_rect(*r)
                || r.width < pane.min_size.width.max(120.)
                || r.height < pane.min_size.height + DOCK_MDI_HEADER_HEIGHT
            {
                return Err(invalid(
                    "MDI frame cannot satisfy the current pane minimum size",
                ));
            }
        }
        Ok(())
    }
    /// Refresh open membership without losing remembered rectangles of closed panes.
    pub fn synchronize(
        &mut self,
        layout: &DockLayout,
        window: DockWindowId,
    ) -> Result<(), DockError> {
        let panes = layout.window_panes(window)?;
        let mut next = self.clone();
        next.order.retain(|id| panes.contains(id));
        for id in panes {
            if !next.order.contains(&id) {
                let ordinal = next.rects.len() % 10;
                let minimum = layout.pane(&id).unwrap().min_size;
                let initial = Rect::new(
                    24. * ordinal as f32,
                    24. * ordinal as f32,
                    480f32.max(minimum.width),
                    320f32.max(minimum.height + DOCK_MDI_HEADER_HEIGHT),
                );
                if !next.rects.contains_key(&id) && !valid_rect(initial) {
                    return Err(invalid("MDI pane minimum exceeds bounded geometry"));
                }
                next.rects.entry(id.clone()).or_insert(initial);
                next.order.push(id);
            }
        }
        if next
            .maximized
            .as_ref()
            .is_some_and(|id| !next.order.contains(id))
        {
            next.maximized = None;
        }
        next.rects.retain(|id, _| layout.pane(id).is_some());
        next.validate_layout(layout, window)?;
        if next.order != self.order || next.rects != self.rects || next.maximized != self.maximized
        {
            next.revision = self.next()?;
            *self = next;
        }
        Ok(())
    }
    pub fn activate(&mut self, id: &DockPaneId) -> Result<bool, DockError> {
        let i = self
            .order
            .iter()
            .position(|v| v == id)
            .ok_or_else(|| invalid("Unknown open MDI pane"))?;
        if i + 1 == self.order.len() && self.maximized.as_ref().is_none_or(|key| key == id) {
            return Ok(false);
        }
        let revision = self.next()?;
        let id = self.order.remove(i);
        self.order.push(id.clone());
        if self.maximized.is_some() {
            self.maximized = Some(id);
        }
        self.revision = revision;
        Ok(true)
    }
    pub fn set_bounds(
        &mut self,
        layout: &DockLayout,
        id: &DockPaneId,
        bounds: Rect,
    ) -> Result<(), DockError> {
        let pane = layout.pane(id).ok_or_else(|| invalid("Unknown MDI pane"))?;
        if !self.order.contains(id)
            || !valid_rect(bounds)
            || bounds.width < pane.min_size.width.max(120.)
            || bounds.height < pane.min_size.height + DOCK_MDI_HEADER_HEIGHT
        {
            return Err(invalid("Invalid MDI frame bounds"));
        }
        let revision = self.next()?;
        self.rects.insert(id.clone(), bounds);
        self.maximized = None;
        self.revision = revision;
        Ok(())
    }
    pub fn toggle_maximize(&mut self, id: &DockPaneId) -> Result<(), DockError> {
        if !self.order.contains(id) {
            return Err(invalid("Unknown open MDI pane"));
        }
        let revision = self.next()?;
        self.maximized = if self.maximized.as_ref() == Some(id) {
            None
        } else {
            let index = self.order.iter().position(|key| key == id).unwrap();
            let key = self.order.remove(index);
            self.order.push(key);
            Some(id.clone())
        };
        self.revision = revision;
        Ok(())
    }
    pub fn arrange(
        &self,
        window: DockWindowId,
        bounds: Rect,
    ) -> Result<DockMdiGeometry, DockError> {
        if !bounds.x.is_finite()
            || !bounds.y.is_finite()
            || !bounds.width.is_finite()
            || !bounds.height.is_finite()
            || bounds.width < 0.
            || bounds.height < 0.
            || bounds.x.abs() > 32768.
            || bounds.y.abs() > 32768.
            || bounds.width > 32768.
            || bounds.height > 32768.
        {
            return Err(invalid("Invalid MDI viewport"));
        }
        let mut frames = Vec::new();
        for id in &self.order {
            if self.maximized.as_ref().is_some_and(|active| active != id) {
                continue;
            }
            let saved = self.rects[id];
            let r = if self.maximized.is_some() {
                Rect::new(0., 0., bounds.width, bounds.height)
            } else {
                let w = saved.width.min(bounds.width);
                let h = saved.height.min(bounds.height);
                Rect::new(
                    saved.x.min((bounds.width - w).max(0.)),
                    saved.y.min((bounds.height - h).max(0.)),
                    w,
                    h,
                )
            };
            let r = Rect::new(bounds.x + r.x, bounds.y + r.y, r.width, r.height);
            let header = Rect::new(r.x, r.y, r.width, DOCK_MDI_HEADER_HEIGHT.min(r.height));
            frames.push(DockMdiFrame {
                pane: id.clone(),
                bounds: r,
                header,
                content: Rect::new(
                    r.x,
                    r.y + header.height,
                    r.width,
                    (r.height - header.height).max(0.),
                ),
            });
        }
        Ok(DockMdiGeometry {
            window,
            bounds,
            frames,
        })
    }
    pub fn cascade(&mut self, layout: &DockLayout, bounds: Size) -> Result<(), DockError> {
        if !bounds.is_valid() || bounds.width > 32768. || bounds.height > 32768. {
            return Err(invalid("Invalid MDI cascade extent"));
        }
        let mut next = self.clone();
        next.maximized = None;
        for (i, id) in self.order.iter().enumerate() {
            let p = layout.pane(id).ok_or_else(|| invalid("Unknown MDI pane"))?;
            let offset = (i % 10) as f32 * 24.;
            let w = (bounds.width - 240.).max(320.).max(p.min_size.width);
            let h = (bounds.height - 240.)
                .max(200.)
                .max(p.min_size.height + DOCK_MDI_HEADER_HEIGHT);
            let r = Rect::new(offset, offset, w, h);
            if !valid_rect(r) {
                return Err(invalid("MDI cascade exceeds geometry limit"));
            }
            next.rects.insert(id.clone(), r);
        }
        next.revision = self.next()?;
        *self = next;
        Ok(())
    }
    pub fn tile(&mut self, layout: &DockLayout, bounds: Size) -> Result<(), DockError> {
        if !bounds.is_valid() || bounds.width > 32768. || bounds.height > 32768. {
            return Err(invalid("Invalid MDI tile extent"));
        }
        if self.order.is_empty() {
            return Ok(());
        }
        let cols = (self.order.len() as f32).sqrt().ceil() as usize;
        let rows = self.order.len().div_ceil(cols);
        let w = bounds.width / cols as f32;
        let h = bounds.height / rows as f32;
        let mut next = self.clone();
        next.maximized = None;
        for (i, id) in self.order.iter().enumerate() {
            let p = layout.pane(id).ok_or_else(|| invalid("Unknown MDI pane"))?;
            if w < p.min_size.width.max(120.) || h < p.min_size.height + DOCK_MDI_HEADER_HEIGHT {
                return Err(invalid("MDI tile cannot meet document minimum sizes"));
            }
            next.rects.insert(
                id.clone(),
                Rect::new((i % cols) as f32 * w, (i / cols) as f32 * h, w, h),
            );
        }
        next.revision = self.next()?;
        *self = next;
        Ok(())
    }
}
