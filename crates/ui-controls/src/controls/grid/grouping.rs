//! Group-panel geometry and input over the same presentation as programmatic grouping.
use super::*;
use rust_desktop_ui_treegrid::GridHitKind;
impl GridPresentationSettings {
    pub(super) fn panel_height(&self, bounds: Rect) -> f32 {
        if self.show_group_panel {
            bounds.height.min(30.0)
        } else {
            0.0
        }
    }
    pub(super) fn group_slot(&self, bounds: Rect, index: usize) -> Rect {
        let width = (bounds.width / (self.group_fields.len() + 1) as f32).min(160.0);
        Rect::new(
            bounds.x + index as f32 * width,
            bounds.y,
            width,
            self.panel_height(bounds),
        )
    }
}
impl Controls {
    pub(super) fn grid_group_panel_input(
        &mut self,
        id: WidgetId,
        event: &InputEvent,
    ) -> Result<bool, ControlError> {
        let grid = &self.grids[&id];
        let options = &grid.source.presentation;
        let Some(bounds) = grid.bounds.filter(|_| options.show_group_panel) else {
            return Ok(false);
        };
        let area = Rect::new(
            bounds.x,
            bounds.y,
            bounds.width,
            options.panel_height(bounds),
        );
        let slot = |p: Point| {
            (0..options.group_fields.len())
                .find(|i| options.group_slot(bounds, *i).contains(p.x, p.y))
        };
        let mut settings = options.clone();
        match event {
            InputEvent::KeyDown {
                key: Key::Character(k),
                modifiers,
                repeat: false,
            } if modifiers.alt
                && !modifiers.control
                && !modifiers.super_key
                && ["s", "d", "f"].iter().any(|v| k.eq_ignore_ascii_case(v)) =>
            {
                if k.eq_ignore_ascii_case("s") {
                    let next = settings.group_sort_summary.map_or(0, |i| i + 1);
                    settings.group_sort_summary =
                        (next < settings.summary_titles().count()).then_some(next);
                } else if k.eq_ignore_ascii_case("d") {
                    settings.group_descending = !settings.group_descending;
                } else {
                    settings.group_footer = !settings.group_footer;
                }
            }
            InputEvent::KeyDown {
                key,
                modifiers,
                repeat: false,
            } if modifiers.alt && !modifiers.super_key && !modifiers.control => {
                let Some(column) = grid.model.focused_column() else {
                    return Ok(false);
                };
                let field = &grid.source.fields[column.0 as usize];
                let old = settings.group_fields.iter().position(|f| f == field);
                match key {
                    Key::Character(k) if k.eq_ignore_ascii_case("g") => {
                        if let Some(i) = old {
                            settings.group_fields.remove(i);
                        } else {
                            settings.group_fields.push(field.clone());
                        }
                    }
                    Key::ArrowLeft | Key::ArrowRight if old.is_some() => {
                        let i = old.unwrap();
                        let j = if *key == Key::ArrowLeft {
                            i.saturating_sub(1)
                        } else {
                            (i + 1).min(settings.group_fields.len() - 1)
                        };
                        settings.group_fields.swap(i, j);
                    }
                    _ => return Ok(false),
                }
            }
            InputEvent::PointerDown {
                position,
                button: PointerButton::Primary,
                ..
            } => {
                if area.contains(position.x, position.y) {
                    if let Some(i) = slot(*position) {
                        let column = grid
                            .source
                            .fields
                            .iter()
                            .position(|f| f == &options.group_fields[i])
                            .expect("validated group field");
                        self.grids.get_mut(&id).unwrap().group_drag =
                            Some((ColumnId(column as u64), true));
                        self.runtime.apply(Mutation::Focus(Some(id)))?;
                        self.runtime.apply(Mutation::CapturePointer(Some(id)))?;
                    }
                    return Ok(true);
                }
                if let Some(GridHitKind::Header(col)) = grid.frame.hit_test(*position) {
                    self.grids.get_mut(&id).unwrap().group_drag = Some((col, false));
                }
                return Ok(false);
            }
            InputEvent::PointerUp {
                position,
                button: PointerButton::Primary,
                ..
            } if grid.group_drag.is_some() => {
                let (col, from_panel) = grid.group_drag.unwrap();
                let target = slot(*position);
                let inside = area.contains(position.x, position.y);
                let field = grid.source.fields[col.0 as usize].clone();
                let grid = self.grids.get_mut(&id).unwrap();
                grid.group_drag = None;
                if !inside && !from_panel {
                    return Ok(false);
                }
                grid.cancel_pointer();
                self.runtime.apply(Mutation::CapturePointer(None))?;
                settings.group_fields.retain(|f| f != &field);
                if inside {
                    settings.group_fields.insert(
                        target
                            .unwrap_or(settings.group_fields.len())
                            .min(settings.group_fields.len()),
                        field,
                    );
                }
            }
            InputEvent::PointerMoved { .. }
                if grid.group_drag.is_some_and(|(_, from_panel)| from_panel) =>
            {
                return Ok(true);
            }
            _ => return Ok(false),
        }
        if let Err(e) = self.set_grid_presentation(id, settings) {
            self.grids.get_mut(&id).unwrap().error = Some(e.to_string());
        }
        self.scene_dirty = true;
        Ok(true)
    }
}
