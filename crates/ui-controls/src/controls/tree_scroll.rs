//! Scroll chrome uses f64 logical offsets and local f32 thumb geometry.
use super::*;
use rust_desktop_ui_treegrid::{GridError, TreeGrid};
#[derive(Clone, Copy)]
pub(super) struct Bar {
    pub track: Rect,
    pub thumb: Rect,
    horizontal: bool,
    maximum: f64,
}
#[derive(Clone, Copy)]
struct Drag {
    bar: Bar,
    start: f32,
    offset: f64,
}
#[derive(Default)]
pub(super) struct TreeScroll {
    pub bars: Vec<Bar>,
    drag: Option<Drag>,
}
impl TreeScroll {
    pub fn captured(&self) -> bool {
        self.drag.is_some()
    }
    pub fn cancel(&mut self) {
        self.drag = None;
    }
    pub fn geometry(&mut self, model: &TreeGrid, bounds: Rect, headerless: bool) -> Rect {
        let thickness = 12.0_f32.min(bounds.width).min(bounds.height);
        let total_height = model.row_count() as f64 * f64::from(model.config().row_height);
        let total_width: f64 = model.columns().iter().map(|c| f64::from(c.width)).sum();
        let pinned: f64 = model
            .columns()
            .iter()
            .filter(|c| c.pinned)
            .map(|c| f64::from(c.width))
            .sum();
        let mut vertical =
            total_height > f64::from((bounds.height - model.config().header_height).max(0.0));
        let horizontal = !headerless
            && total_width > f64::from(bounds.width - if vertical { thickness } else { 0.0 });
        vertical |= total_height
            > f64::from(
                (bounds.height
                    - model.config().header_height
                    - if horizontal { thickness } else { 0.0 })
                .max(0.0),
            );
        let viewport = Rect::new(
            bounds.x,
            bounds.y,
            (bounds.width - if vertical { thickness } else { 0.0 }).max(0.0),
            (bounds.height - if horizontal { thickness } else { 0.0 }).max(0.0),
        );
        self.bars.clear();
        let (x, y) = model.scroll_offset();
        if vertical {
            self.add(
                Rect::new(
                    viewport.x + viewport.width,
                    viewport.y,
                    thickness,
                    viewport.height,
                ),
                false,
                f64::from((viewport.height - model.config().header_height).max(0.0)),
                total_height,
                y,
            );
        }
        if horizontal {
            self.add(
                Rect::new(
                    viewport.x,
                    viewport.y + viewport.height,
                    viewport.width,
                    thickness,
                ),
                true,
                (f64::from(viewport.width) - pinned).max(0.0),
                (total_width - pinned).max(0.0),
                x,
            );
        }
        viewport
    }
    fn add(&mut self, track: Rect, horizontal: bool, visible: f64, total: f64, offset: f64) {
        let length = if horizontal {
            track.width
        } else {
            track.height
        };
        let thumb_length = (length * (visible / total.max(1.0)) as f32)
            .max(20.0)
            .min(length);
        let maximum = (total - visible).max(0.0);
        let start = if maximum > 0.0 {
            (length - thumb_length) * (offset / maximum).clamp(0.0, 1.0) as f32
        } else {
            0.0
        };
        let thumb = if horizontal {
            Rect::new(
                track.x + start,
                track.y + 3.0,
                thumb_length,
                (track.height - 6.0).max(0.0),
            )
        } else {
            Rect::new(
                track.x + 3.0,
                track.y + start,
                (track.width - 6.0).max(0.0),
                thumb_length,
            )
        };
        self.bars.push(Bar {
            track,
            thumb,
            horizontal,
            maximum,
        });
    }
    pub fn input(&mut self, model: &mut TreeGrid, event: &InputEvent) -> Result<bool, GridError> {
        match event {
            InputEvent::PointerDown {
                position,
                button: PointerButton::Primary,
                ..
            } => {
                let Some(bar) = self
                    .bars
                    .iter()
                    .find(|bar| bar.track.contains(position.x, position.y))
                    .copied()
                else {
                    return Ok(false);
                };
                let coord = if bar.horizontal {
                    position.x
                } else {
                    position.y
                };
                let (x, y) = model.scroll_offset();
                let mut offset = if bar.horizontal { x } else { y };
                if !bar.thumb.contains(position.x, position.y) {
                    let (start, length, thumb) = if bar.horizontal {
                        (bar.track.x, bar.track.width, bar.thumb.width)
                    } else {
                        (bar.track.y, bar.track.height, bar.thumb.height)
                    };
                    offset = f64::from(
                        ((coord - start - thumb / 2.0) / (length - thumb).max(1.0)).clamp(0.0, 1.0),
                    ) * bar.maximum;
                    model.scroll_to(
                        if bar.horizontal { offset } else { x },
                        if bar.horizontal { y } else { offset },
                    )?;
                }
                self.drag = Some(Drag {
                    bar,
                    start: coord,
                    offset,
                });
                Ok(true)
            }
            InputEvent::PointerMoved { position } if self.drag.is_some() => {
                let drag = self.drag.unwrap();
                let (coord, travel) = if drag.bar.horizontal {
                    (position.x, drag.bar.track.width - drag.bar.thumb.width)
                } else {
                    (position.y, drag.bar.track.height - drag.bar.thumb.height)
                };
                let offset = drag.offset
                    + f64::from((coord - drag.start) / travel.max(1.0)) * drag.bar.maximum;
                let (x, y) = model.scroll_offset();
                model.scroll_to(
                    if drag.bar.horizontal { offset } else { x },
                    if drag.bar.horizontal { y } else { offset },
                )?;
                Ok(true)
            }
            InputEvent::PointerUp {
                button: PointerButton::Primary,
                ..
            } if self.drag.is_some() => {
                self.drag = None;
                Ok(true)
            }
            _ => Ok(false),
        }
    }
}
