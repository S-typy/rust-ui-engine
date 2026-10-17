//! Indexed two-dimensional shelves, independent of commands and retained widgets.
use super::tiles_model::*;
use super::*;
use std::ops::Range;

#[derive(Clone, Debug)]
pub(super) struct TileShelf {
    pub y: f32,
    pub height: f32,
    pub items: Range<usize>,
}
#[derive(Clone, Debug)]
pub(super) struct TileHeader {
    pub group: usize,
    pub bounds: Rect,
}
#[derive(Clone, Debug)]
pub(super) struct TileGeometry {
    pub width: f32,
    pub extent: f32,
    pub order: Vec<usize>,
    pub rects: Vec<Rect>,
    pub positions: HashMap<String, usize>,
    pub shelves: Vec<TileShelf>,
    pub headers: Vec<TileHeader>,
}
#[derive(Clone, Debug)]
pub(super) struct TileAnchor {
    pub key: String,
    pub index: usize,
    pub clipped: f32,
}
impl TileGeometry {
    pub fn new(
        model: &TileModel,
        settings: &TileSettings,
        width: f32,
    ) -> Result<Self, ControlError> {
        settings.validate()?;
        if !width.is_finite() || !(0.0..=65536.0).contains(&width) {
            return Err(tile_error("Invalid tile viewport width"));
        }
        let width = width.floor().max(1.0);
        let columns = (((width + settings.gap) / (settings.cell_width + settings.gap)).floor()
            as usize)
            .clamp(1, 64);
        let cell = ((width - settings.gap * (columns - 1) as f32) / columns as f32)
            .floor()
            .max(1.0);
        let mut groups = vec![Vec::new(); model.groups().len() + 1];
        let group_positions: HashMap<_, _> = model
            .groups()
            .iter()
            .enumerate()
            .map(|(i, g)| (g.key.as_str(), i + 1))
            .collect();
        for (i, tile) in model.items().iter().enumerate() {
            groups[if tile.group.is_empty() {
                0
            } else {
                group_positions[tile.group.as_str()]
            }]
            .push(i);
        }
        let mut result = Self {
            width,
            extent: 0.0,
            order: Vec::with_capacity(model.items().len()),
            rects: vec![Rect::new(0.0, 0.0, 0.0, 0.0); model.items().len()],
            positions: HashMap::with_capacity(model.items().len()),
            shelves: Vec::new(),
            headers: Vec::new(),
        };
        let mut y = 0.0f64;
        for (group, items) in groups.into_iter().enumerate() {
            if items.is_empty() {
                continue;
            }
            if group > 0 {
                if y > 0.0 {
                    y += f64::from(settings.gap);
                }
                result.headers.push(TileHeader {
                    group: group - 1,
                    bounds: Rect::new(0.0, y as f32, width, settings.group_header_height),
                });
                y += f64::from(settings.group_header_height + settings.gap);
            }
            let mut col = 0;
            let mut height = 0.0f32;
            let mut start = result.order.len();
            for index in items {
                let (span, height_span) = model.items()[index].size.span();
                let span = span.min(columns);
                if col + span > columns {
                    result.shelves.push(TileShelf {
                        y: y as f32,
                        height,
                        items: start..result.order.len(),
                    });
                    y += f64::from(height + settings.gap);
                    col = 0;
                    height = 0.0;
                    start = result.order.len();
                }
                let h = settings.cell_height * height_span as f32
                    + settings.gap * (height_span - 1) as f32;
                let rect = Rect::new(
                    col as f32 * (cell + settings.gap),
                    y as f32,
                    cell * span as f32 + settings.gap * (span - 1) as f32,
                    h,
                );
                result.rects[index] = rect;
                result
                    .positions
                    .insert(model.items()[index].item.key.clone(), result.order.len());
                result.order.push(index);
                height = height.max(h);
                col += span;
                if y + f64::from(height) > MAX_TILE_EXTENT {
                    return Err(tile_error(
                        "Tile content exceeds its exact-coordinate extent budget",
                    ));
                }
            }
            if start < result.order.len() {
                result.shelves.push(TileShelf {
                    y: y as f32,
                    height,
                    items: start..result.order.len(),
                });
                y += f64::from(height + settings.gap);
            }
        }
        result.extent = (y - f64::from(settings.gap)).max(0.0) as f32;
        Ok(result)
    }
    pub fn visible(
        &self,
        offset: f32,
        height: f32,
        overscan: f32,
        limit: usize,
    ) -> Result<Range<usize>, ControlError> {
        let top = (offset - overscan).max(0.0);
        let bottom = offset + height.max(0.0) + overscan;
        let first = self
            .shelves
            .partition_point(|row| row.y + row.height <= top);
        let last = self.shelves.partition_point(|row| row.y < bottom);
        if first >= last {
            return Ok(0..0);
        }
        let range = self.shelves[first].items.start..self.shelves[last - 1].items.end;
        if range.len() > limit {
            return Err(tile_error(
                "Visible tile shelves exceed the realization budget",
            ));
        }
        Ok(range)
    }
    pub fn anchor(&self, model: &TileModel, offset: f32) -> Option<TileAnchor> {
        let row = self.shelves.get(
            self.shelves
                .partition_point(|row| row.y + row.height <= offset),
        )?;
        let position = row.items.start;
        let index = self.order[position];
        Some(TileAnchor {
            key: model.items()[index].item.key.clone(),
            index: position,
            clipped: offset - row.y,
        })
    }
    pub fn restore(&self, anchor: Option<&TileAnchor>) -> f32 {
        let Some(anchor) = anchor else {
            return 0.0;
        };
        let position = self.positions.get(&anchor.key).copied().unwrap_or(
            anchor
                .index
                .saturating_sub(1)
                .min(self.order.len().saturating_sub(1)),
        );
        self.order
            .get(position)
            .map_or(0.0, |&i| (self.rects[i].y + anchor.clipped).max(0.0))
    }
    pub fn neighbor(&self, position: usize, key: Key) -> Option<usize> {
        let rect = *self.rects.get(*self.order.get(position)?)?;
        if matches!(key, Key::ArrowLeft) {
            return position.checked_sub(1);
        }
        if matches!(key, Key::ArrowRight) {
            return (position + 1 < self.order.len()).then_some(position + 1);
        }
        let center = rect.x + rect.width / 2.0;
        let row = self.shelves.partition_point(|row| row.y < rect.y);
        let next = if key == Key::ArrowUp {
            row.checked_sub(1)?
        } else {
            row + 1
        };
        let row = self.shelves.get(next)?;
        row.items.clone().min_by(|&a, &b| {
            let a = self.rects[self.order[a]];
            let b = self.rects[self.order[b]];
            (a.x + a.width / 2.0 - center)
                .abs()
                .total_cmp(&(b.x + b.width / 2.0 - center).abs())
        })
    }
}
