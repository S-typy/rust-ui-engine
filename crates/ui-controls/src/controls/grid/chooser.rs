//! Locally searchable column and band chooser for a grid's accepted layout.
use super::*;
#[derive(Default)]
pub(super) struct Chooser {
    query: String,
    cursor: usize,
}
struct Entry {
    label: String,
    fields: Vec<String>,
    visible: bool,
}
impl GridState {
    fn chooser_entries(&self, query: &str) -> Vec<Entry> {
        let options = &self.source.presentation;
        let bands: Vec<_> = options
            .bands
            .iter()
            .chain(options.band_rows.iter().flatten())
            .collect();
        let layout = self.current_arrangement();
        let shown = |f: &str| layout.iter().any(|c| c.field == f && c.visible);
        let mut result: Vec<_> = bands
            .iter()
            .map(|b| Entry {
                label: b.title.clone(),
                visible: b.fields.iter().all(|f| shown(f)),
                fields: b.fields.clone(),
            })
            .collect();
        for c in &self.settings.columns {
            let mut path: Vec<_> = bands
                .iter()
                .filter(|b| b.fields.contains(&c.field))
                .map(|b| b.title.as_str())
                .collect();
            path.push(&c.title);
            result.push(Entry {
                label: path.join(" › "),
                fields: vec![c.field.clone()],
                visible: shown(&c.field),
            });
        }
        let q = query.to_lowercase();
        result.retain(|e| {
            e.label.to_lowercase().contains(&q)
                || e.fields.iter().any(|f| f.to_lowercase().contains(&q))
        });
        result
    }
    fn chooser_area(&self, bounds: Rect) -> Rect {
        let w = bounds.width.min(400.);
        Rect::new(
            bounds.x + bounds.width - w,
            bounds.y,
            w,
            bounds.height.min(270.),
        )
    }
    fn chooser_page(&self, area: Rect) -> usize {
        ((area.height - 30.).max(0.) / 26.) as usize
    }
    pub(super) fn paint_chooser(&self, bounds: Rect, scene: &mut Scene) {
        let Some(state) = &self.chooser else { return };
        let area = self.chooser_area(bounds);
        let palette = self.model.palette;
        scene.fill(area, palette.header);
        let header = Rect::new(area.x, area.y, area.width, area.height.min(30.));
        views::draw_text(
            scene,
            header,
            header,
            format!("  Колонки и полосы · Поиск: {}", state.query),
            palette.text,
            600,
            14.,
        );
        let entries = self.chooser_entries(&state.query);
        let count = self.chooser_page(area).max(1);
        let start = (state.cursor / count) * count;
        for (i, e) in entries.iter().enumerate().skip(start).take(count) {
            let row = Rect::new(
                area.x,
                header.y + 30. + (i - start) as f32 * 26.,
                area.width,
                26.,
            );
            let Some(clip) = row.intersection(area) else {
                continue;
            };
            scene.fill(
                clip,
                if i == state.cursor {
                    palette.selection
                } else {
                    palette.background
                },
            );
            views::draw_text(
                scene,
                row,
                clip,
                format!("  {}  {}", if e.visible { "☑" } else { "☐" }, e.label),
                palette.text,
                400,
                13.,
            );
        }
    }
}
impl Controls {
    /// Open a searchable chooser over all fields, including hidden ones.
    pub fn set_grid_column_chooser_open(
        &mut self,
        id: WidgetId,
        open: bool,
    ) -> Result<(), ControlError> {
        let grid = self
            .grids
            .get_mut(&id)
            .ok_or_else(|| error("Unknown GridControl"))?;
        if open && grid.session.is_some() {
            return Err(error("Finish the active editor before opening the chooser"));
        }
        grid.chooser = open.then(Chooser::default);
        if open {
            self.runtime.apply(Mutation::Focus(Some(id)))?;
        }
        self.scene_dirty = true;
        Ok(())
    }
    pub(super) fn grid_chooser_input(
        &mut self,
        id: WidgetId,
        event: &InputEvent,
    ) -> Result<bool, ControlError> {
        let grid = &self.grids[&id];
        if matches!(event,InputEvent::KeyDown{key:Key::Character(c),modifiers,repeat:false} if c.eq_ignore_ascii_case("c") && modifiers.control && modifiers.shift && !modifiers.alt)
        {
            self.set_grid_column_chooser_open(id, grid.chooser.is_none())?;
            return Ok(true);
        }
        let Some(state) = &grid.chooser else {
            return Ok(false);
        };
        let Some(bounds) = grid.bounds else {
            return Ok(false);
        };
        let area = grid.chooser_area(bounds);
        let entries = grid.chooser_entries(&state.query);
        let count = entries.len();
        let page = grid.chooser_page(area).max(1);
        let mut index = state.cursor.min(count.saturating_sub(1));
        let mut toggle = false;
        let state = self.grids.get_mut(&id).unwrap().chooser.as_mut().unwrap();
        match event {
            InputEvent::KeyDown { key, modifiers, .. } => match key {
                Key::Escape => {
                    self.set_grid_column_chooser_open(id, false)?;
                    return Ok(true);
                }
                Key::Enter | Key::Space => toggle = true,
                Key::ArrowDown => index = index.saturating_add(1),
                Key::ArrowUp => index = index.saturating_sub(1),
                Key::PageDown => index = index.saturating_add(page),
                Key::PageUp => index = index.saturating_sub(page),
                Key::Home => index = 0,
                Key::End => index = count.saturating_sub(1),
                Key::Backspace => {
                    state.query.pop();
                    index = 0;
                }
                Key::Character(c)
                    if !modifiers.control && !modifiers.alt && !modifiers.super_key =>
                {
                    if state.query.len() + c.len() <= 256 {
                        state.query.push_str(c);
                    }
                    index = 0;
                }
                _ => return Ok(false),
            },
            InputEvent::Wheel {
                position, delta, ..
            } if area.contains(position.x, position.y) => {
                if delta.y < 0. {
                    index = index.saturating_sub(1)
                } else if delta.y > 0. {
                    index = index.saturating_add(1)
                }
            }
            InputEvent::PointerDown {
                position,
                button: PointerButton::Primary,
                ..
            } => {
                if !area.contains(position.x, position.y) {
                    self.set_grid_column_chooser_open(id, false)?;
                    return Ok(true);
                }
                if position.y >= area.y + 30. {
                    let target =
                        (index / page) * page + ((position.y - area.y - 30.) / 26.) as usize;
                    if target < count {
                        index = target;
                        toggle = true;
                    }
                }
            }
            InputEvent::PointerUp { .. } | InputEvent::PointerMoved { .. } => return Ok(true),
            _ => return Ok(false),
        }
        state.cursor = index.min(count.saturating_sub(1));
        if toggle && let Some(entry) = entries.get(index) {
            let mut arrangement = self.grid_arrangement(id).expect("registered grid");
            for col in &mut arrangement {
                if entry.fields.contains(&col.field) {
                    col.visible = !entry.visible;
                }
            }
            if let Err(e) = self.arrange_grid_columns(id, arrangement) {
                self.grids.get_mut(&id).unwrap().error = Some(e.to_string());
            }
        }
        self.scene_dirty = true;
        Ok(true)
    }
}
