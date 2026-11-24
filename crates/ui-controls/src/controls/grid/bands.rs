//! Transactional changes to band membership, display order and widths.
use super::*;

/// Coordinate in the current band layout. Coordinates are resolved before each change.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GridBandAddress {
    pub level: usize,
    pub index: usize,
}
#[derive(Clone, Debug)]
pub enum GridBandItem {
    Field(String),
    Band(GridBandAddress),
}
#[derive(Clone, Debug)]
pub enum GridBandChange {
    /// Move to a parent, or to the top level. Position is relative to a leaf field.
    Move {
        item: GridBandItem,
        parent: Option<GridBandAddress>,
        before_field: Option<String>,
    },
    /// Scale the member columns together. Each individual column retains its size limits.
    Resize { band: GridBandAddress, factor: f32 },
}

struct Branch {
    title: String,
    parent: Option<usize>,
    fields: Vec<String>,
}

impl Controls {
    /// Preview a band operation and publish geometry only when the application accepts it.
    /// Empty branches disappear. A rejected move or resize preserves the entire layout.
    pub fn change_grid_band(
        &mut self,
        id: WidgetId,
        change: GridBandChange,
        permit: impl FnOnce(&GridPresentationSettings, &[GridColumnArrangement]) -> bool,
    ) -> Result<(), ControlError> {
        let grid = self
            .grids
            .get(&id)
            .ok_or_else(|| error("Unknown GridControl"))?;
        let mut source = grid.source.clone();
        let options = &mut source.presentation;
        let mut order = grid.current_arrangement();
        let levels: Vec<_> = std::iter::once(&options.bands)
            .chain(&options.band_rows)
            .collect();
        let resolve = |a: GridBandAddress| -> Result<usize, ControlError> {
            levels
                .get(a.level)
                .and_then(|l| l.get(a.index))
                .ok_or_else(|| error("Unknown grid band"))?;
            Ok(levels[..a.level].iter().map(|l| l.len()).sum::<usize>() + a.index)
        };
        let mut branches = Vec::new();
        for (level, row) in levels.iter().enumerate() {
            for band in row.iter() {
                let parent = if level == 0 {
                    None
                } else {
                    let index = levels[level - 1]
                        .iter()
                        .position(|b| b.fields.contains(&band.fields[0]))
                        .expect("validated hierarchy");
                    Some(resolve(GridBandAddress {
                        level: level - 1,
                        index,
                    })?)
                };
                branches.push(Branch {
                    title: band.title.clone(),
                    parent,
                    fields: band.fields.clone(),
                });
            }
        }
        match change {
            GridBandChange::Resize { band, factor } => {
                if !factor.is_finite() || factor <= 0. {
                    return Err(error("Invalid band width factor"));
                }
                let fields = &branches[resolve(band)?].fields;
                for entry in order.iter_mut().filter(|e| fields.contains(&e.field)) {
                    let c = grid
                        .settings
                        .columns
                        .iter()
                        .find(|c| c.field == entry.field)
                        .expect("known field");
                    entry.width = (entry.width * factor).clamp(c.min_width, c.max_width);
                }
            }
            GridBandChange::Move {
                item,
                parent,
                before_field,
            } => {
                let dest = parent.map(resolve).transpose()?;
                let (moved, old, branch) = match item {
                    GridBandItem::Field(field) => {
                        if !order.iter().any(|e| e.field == field) {
                            return Err(error("Unknown moved field"));
                        }
                        let old = branches.iter().rposition(|b| b.fields.contains(&field));
                        (vec![field], old, None)
                    }
                    GridBandItem::Band(address) => {
                        let i = resolve(address)?;
                        (branches[i].fields.clone(), branches[i].parent, Some(i))
                    }
                };
                let mut ancestor = dest;
                while let Some(i) = ancestor {
                    if Some(i) == branch {
                        return Err(error("A band cannot be its own ancestor"));
                    }
                    ancestor = branches[i].parent;
                }
                if let Some(before) = &before_field
                    && (moved.contains(before)
                        || !order.iter().any(|e| &e.field == before)
                        || dest.is_some_and(|i| !branches[i].fields.contains(before)))
                {
                    return Err(error("Invalid insertion field"));
                }
                let mut ancestor = old;
                while let Some(i) = ancestor {
                    branches[i].fields.retain(|f| !moved.contains(f));
                    ancestor = branches[i].parent;
                }
                let moving: Vec<_> = order
                    .iter()
                    .filter(|e| moved.contains(&e.field))
                    .cloned()
                    .collect();
                order.retain(|e| !moved.contains(&e.field));
                let at = if let Some(field) = before_field {
                    order
                        .iter()
                        .position(|e| e.field == field)
                        .expect("validated position")
                } else {
                    dest.and_then(|i| {
                        order
                            .iter()
                            .rposition(|e| branches[i].fields.contains(&e.field))
                    })
                    .map_or(order.len(), |n| n + 1)
                };
                order.splice(at..at, moving);
                let mut ancestor = dest;
                while let Some(i) = ancestor {
                    branches[i].fields.extend(moved.iter().cloned());
                    ancestor = branches[i].parent;
                }
                if let Some(i) = branch {
                    branches[i].parent = dest;
                }
                let mut tiers: Vec<Vec<GridBand>> = vec![vec![]];
                for b in branches.iter().filter(|b| !b.fields.is_empty()) {
                    let mut depth = 0;
                    let mut ancestor = b.parent;
                    while let Some(i) = ancestor {
                        depth += 1;
                        ancestor = branches[i].parent;
                    }
                    tiers.resize_with(tiers.len().max(depth + 1), Vec::new);
                    let fields = order
                        .iter()
                        .filter(|e| b.fields.contains(&e.field))
                        .map(|e| e.field.clone())
                        .collect();
                    tiers[depth].push(GridBand {
                        title: b.title.clone(),
                        fields,
                    });
                }
                for tier in &mut tiers {
                    tier.sort_by_key(|band| order.iter().position(|e| e.field == band.fields[0]));
                }
                options.bands = tiers.remove(0);
                options.band_rows = tiers;
            }
        }
        options.validate(&grid.settings.columns)?;
        grid.validate_arrangement(&order)?;
        if grid
            .band_policy
            .as_ref()
            .is_some_and(|p| !p(options, &order))
            || !permit(options, &order)
        {
            return Err(error("Grid layout change declined"));
        }
        self.install_grid_presentation(id, source, order)
    }
}

pub(super) type BandPolicy =
    Rc<dyn Fn(&GridPresentationSettings, &[GridColumnArrangement]) -> bool>;
pub(super) struct BandDrag {
    item: GridBandItem,
    start: f32,
    resize_width: Option<f32>,
}

impl GridState {
    fn band_at(&self, p: Point) -> Option<(GridBandAddress, String, f32)> {
        let bounds = self.bounds?;
        let settings = &self.source.presentation;
        if settings.view != GridViewMode::Table || !bounds.contains(p.x, p.y) {
            return None;
        }
        let y = p.y - bounds.y - settings.panel_height(bounds);
        if y < 0. {
            return None;
        }
        let level = (y / 24.).floor() as usize;
        let row = std::iter::once(&settings.bands)
            .chain(&settings.band_rows)
            .nth(level)?;
        let column = self
            .frame
            .columns
            .iter()
            .find(|c| p.x >= c.clip.x && p.x < c.clip.x + c.clip.width)?;
        let field = &self.source.fields[column.id.0 as usize];
        let index = row.iter().position(|b| b.fields.contains(field))?;
        // Stacked fields share their track and are counted only once.
        let mut spans: Vec<_> = self
            .frame
            .columns
            .iter()
            .filter(|c| {
                row[index]
                    .fields
                    .contains(&self.source.fields[c.id.0 as usize])
            })
            .map(|c| (c.clip.x, c.clip.width))
            .collect();
        spans.sort_by(|a, b| a.0.total_cmp(&b.0));
        spans.dedup();
        Some((
            GridBandAddress { level, index },
            field.clone(),
            spans.iter().map(|s| s.1).sum(),
        ))
    }
}
impl Controls {
    /// Apply the same permission check to pointer gestures and application commands.
    pub fn set_grid_band_policy(
        &mut self,
        id: WidgetId,
        policy: impl Fn(&GridPresentationSettings, &[GridColumnArrangement]) -> bool + 'static,
    ) -> Result<(), ControlError> {
        self.grids
            .get_mut(&id)
            .ok_or_else(|| error("Unknown GridControl"))?
            .band_policy = Some(Rc::new(policy));
        Ok(())
    }
    pub(super) fn grid_band_input(
        &mut self,
        id: WidgetId,
        event: &InputEvent,
    ) -> Result<bool, ControlError> {
        let grid = &self.grids[&id];
        match event {
            InputEvent::PointerDown {
                position,
                button: PointerButton::Primary,
                modifiers,
            } => {
                let drag = if let Some((band, _, width)) = grid.band_at(*position) {
                    BandDrag {
                        item: GridBandItem::Band(band),
                        start: position.x,
                        resize_width: modifiers.shift.then_some(width.max(1.)),
                    }
                } else if modifiers.alt
                    && let Some(rust_desktop_ui_treegrid::GridHitKind::Header(c)) =
                        grid.frame.hit_test(*position)
                {
                    BandDrag {
                        item: GridBandItem::Field(grid.source.fields[c.0 as usize].clone()),
                        start: position.x,
                        resize_width: None,
                    }
                } else {
                    return Ok(false);
                };
                self.grids.get_mut(&id).unwrap().band_drag = Some(drag);
                self.runtime.apply(Mutation::Focus(Some(id)))?;
                self.runtime.apply(Mutation::CapturePointer(Some(id)))?;
            }
            InputEvent::PointerMoved { .. } if grid.band_drag.is_some() => {}
            InputEvent::PointerUp {
                position,
                button: PointerButton::Primary,
                modifiers,
            } if grid.band_drag.is_some() => {
                let target = grid.band_at(*position);
                let drag = self.grids.get_mut(&id).unwrap().band_drag.take().unwrap();
                self.runtime.apply(Mutation::CapturePointer(None))?;
                let change = if let (GridBandItem::Band(band), Some(width)) =
                    (&drag.item, drag.resize_width)
                {
                    GridBandChange::Resize {
                        band: *band,
                        factor: (1. + (position.x - drag.start) / width).max(0.01),
                    }
                } else if let Some((band, field, _)) = target {
                    if matches!(&drag.item,GridBandItem::Band(start) if *start==band) {
                        return Ok(true);
                    }
                    let settings = &self.grids[&id].source.presentation;
                    let nest = modifiers.alt || matches!(drag.item, GridBandItem::Field(_));
                    let parent = if nest {
                        Some(band)
                    } else if band.level == 0 {
                        None
                    } else {
                        let row = std::iter::once(&settings.bands)
                            .chain(&settings.band_rows)
                            .nth(band.level - 1)
                            .unwrap();
                        Some(GridBandAddress {
                            level: band.level - 1,
                            index: row.iter().position(|b| b.fields.contains(&field)).unwrap(),
                        })
                    };
                    GridBandChange::Move {
                        item: drag.item,
                        parent,
                        before_field: (!nest).then_some(field),
                    }
                } else {
                    return Ok(true);
                };
                if let Err(e) = self.change_grid_band(id, change, |_, _| true) {
                    self.grids.get_mut(&id).unwrap().error = Some(e.to_string());
                }
                self.scene_dirty = true;
            }
            _ => return Ok(false),
        }
        Ok(true)
    }
}
