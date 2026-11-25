//! Pointer feedback for movable table headers and the surrounding group and band headers.
use crate::{ColumnId, GridFrame, GridHitKind, GridPalette, TreeGrid};
use rust_desktop_ui_core::{Point, Rect, Scene, TextAlign, TextRun};

#[derive(Clone, Copy, Debug)]
pub struct HeaderDragMotion {
    origin: Point,
    position: Option<Point>,
}

impl HeaderDragMotion {
    pub fn new(origin: Point) -> Self {
        Self {
            origin,
            position: None,
        }
    }

    /// Latch the same four-DIP threshold that separates a header click from a drag.
    pub fn update(&mut self, point: Point) {
        if point.is_finite()
            && (self.position.is_some()
                || (point.x - self.origin.x).abs() > 4.
                || (point.y - self.origin.y).abs() > 4.)
        {
            self.position = Some(point);
        }
    }

    pub fn position(&self) -> Option<Point> {
        self.position
    }
}

/// A paint-only caption and an optional insertion mark in viewport coordinates.
pub struct HeaderDragPreview<'a> {
    pub title: &'a str,
    pub position: Point,
    pub width: f32,
    pub marker: Option<Rect>,
}

impl HeaderDragPreview<'_> {
    pub fn paint(&self, scene: &mut Scene, bounds: Rect, palette: GridPalette) {
        if !bounds.contains(self.position.x, self.position.y)
            || bounds.width <= 0.
            || bounds.height <= 0.
        {
            return;
        }
        let width = self.width.clamp(80., 240.).min(bounds.width);
        let height = 28f32.min(bounds.height);
        let x = self.position.x + 12.;
        let y = self.position.y + 12.;
        let badge = Rect::new(
            x.min(bounds.x + bounds.width - width).max(bounds.x),
            (if y + height <= bounds.y + bounds.height {
                y
            } else {
                self.position.y - height - 8.
            })
            .max(bounds.y),
            width,
            height,
        );
        scene.rounded_fill(badge, bounds, 4., palette.background);
        scene.rounded_stroke(badge, bounds, 4., 1.5, palette.focus);
        if width > 16. {
            scene.text(TextRun {
                sensitive: false,
                bounds: Rect::new(
                    badge.x + 8.,
                    badge.y + 4.,
                    width - 16.,
                    (height - 8.).max(0.),
                ),
                clip: badge,
                text: self.title.to_owned(),
                font_size: 13.,
                weight: 600,
                color: palette.text,
                align: TextAlign::Start,
                wrap: false,
            });
        }
        if let Some(mark) = self.marker.and_then(|r| r.intersection(bounds)) {
            scene.fill(mark, palette.focus);
        }
    }
}

pub(crate) struct HeaderDrop {
    target: ColumnId,
    after: bool,
    marker: Rect,
}

impl TreeGrid {
    pub(crate) fn header_drop(
        &self,
        id: ColumnId,
        frame: &GridFrame,
        point: Point,
    ) -> Option<HeaderDrop> {
        let header = frame.header_bounds?;
        if !header.contains(point.x, point.y) {
            return None;
        }
        let source = self.columns.iter().find(|c| c.id == id)?;
        let source_track = self.header_track(source);
        let hit = frame.hit_test(point);
        let (target, after) = match hit {
            Some(GridHitKind::Header(id) | GridHitKind::ResizeColumn(id)) => {
                let target = frame.columns.iter().find(|c| c.id == id)?;
                (target, point.x >= target.clip.x + target.clip.width * 0.5)
            }
            None => {
                // Empty space after the last field in the scrollable header.
                if source_track.0 != 1 {
                    return None;
                }
                let last = frame
                    .columns
                    .iter()
                    .filter(|c| !c.pinned)
                    .max_by(|a, b| a.clip.x.total_cmp(&b.clip.x))?;
                if point.x < last.clip.x + last.clip.width {
                    return None;
                }
                (last, true)
            }
            _ => return None,
        };
        let column = self
            .columns
            .get(target.index)
            .filter(|c| c.id == target.id)?;
        let target_track = self.header_track(column);
        if source_track.0 != target_track.0 || source_track == target_track {
            return None;
        }
        let x = if after {
            target.clip.x + target.clip.width
        } else {
            target.clip.x
        };
        let width = 3f32.min(header.width);
        Some(HeaderDrop {
            target: target.id,
            after,
            marker: Rect::new(
                header.x + (x - header.x - width / 2.).clamp(0., header.width - width),
                header.y,
                width,
                header.height,
            ),
        })
    }

    /// Resolve the drop against the same hits and clipped tracks used at release.
    pub fn header_drag_preview<'a>(&'a self, frame: &GridFrame) -> Option<HeaderDragPreview<'a>> {
        let (id, position) = self.dragged_header()?;
        let col = self.columns.iter().find(|c| c.id == id)?;
        Some(HeaderDragPreview {
            title: &col.title,
            position,
            width: col.width,
            marker: self.header_drop(id, frame, position).map(|d| d.marker),
        })
    }

    pub(crate) fn move_header_track(&mut self, id: ColumnId, drop: HeaderDrop) {
        // The first member determines where a stacked track is laid out. Move its
        // members together, retaining their internal order and pinned region.
        let track = self.header_track(self.columns.iter().find(|c| c.id == id).unwrap());
        let anchor = self.header_track(self.columns.iter().find(|c| c.id == drop.target).unwrap());
        let (moving, mut rest): (Vec<_>, Vec<_>) = std::mem::take(&mut self.columns)
            .into_iter()
            .partition(|c| self.header_track(c) == track);
        let at = rest
            .iter()
            .position(|c| self.header_track(c) == anchor)
            .unwrap()
            + usize::from(drop.after);
        rest.splice(at..at, moving);
        self.columns = rest;
        self.index_columns();
    }
}

impl GridFrame {
    pub(crate) fn paint_header_feedback(&mut self, grid: &TreeGrid, bounds: Rect) {
        if let Some(preview) = grid.header_drag_preview(self) {
            preview.paint(&mut self.scene, bounds, grid.palette);
        }
    }
}
