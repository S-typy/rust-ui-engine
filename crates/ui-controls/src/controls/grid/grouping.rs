//! Group-panel geometry and input over the same presentation as programmatic grouping.
use super::*;
use rust_desktop_ui_treegrid::GridHitKind;
pub(super) struct GroupDrag {
    column: ColumnId,
    from_panel: bool,
    motion: HeaderDragMotion,
}

impl GridState {
    pub(super) fn header_feedback(&self, bounds: Rect) -> Option<HeaderDragPreview<'_>> {
        if self.band_drag.is_some() {
            return self.band_feedback();
        }
        let mut preview = match &self.group_drag {
            Some(drag) if drag.from_panel => {
                let c = &self.settings.columns[drag.column.0 as usize];
                HeaderDragPreview {
                    title: &c.title,
                    width: c.width,
                    position: drag.motion.position()?,
                    marker: None,
                }
            }
            _ => self.model.header_drag_preview(&self.frame)?,
        };
        if let Some(drag) = &self.group_drag {
            let s = &self.source.presentation;
            if let Some(index) = s.group_drop_slot(bounds, preview.position) {
                let old = s
                    .group_fields
                    .iter()
                    .position(|f| f == &self.source.fields[drag.column.0 as usize]);
                let mut slot = index;
                if old.is_some_and(|i| i < index) {
                    slot = (index + 1).min(s.group_fields.len());
                }
                let r = s.group_slot(bounds, slot);
                preview.marker = if old == Some(index) {
                    None
                } else {
                    Some(Rect::new(r.x, r.y + 2., 3., (r.height - 4.).max(0.)))
                };
            }
        }
        Some(preview)
    }
}

impl SnapshotSource {
    pub(super) fn group_descending(&self, field: &str) -> bool {
        if self.presentation.group_sort_summary.is_none()
            && let Some(order) = self.data_query.sort.iter().find(|o| o.field == field)
        {
            order.direction == rust_desktop_ui_data::SortDirection::Descending
        } else {
            self.presentation.group_descending
        }
    }
}

impl GridPresentationSettings {
    fn group_drop_slot(&self, bounds: Rect, position: Point) -> Option<usize> {
        if !Rect::new(bounds.x, bounds.y, bounds.width, self.panel_height(bounds))
            .contains(position.x, position.y)
        {
            return None;
        }
        Some(
            (0..self.group_fields.len())
                .find(|i| self.group_slot(bounds, *i).contains(position.x, position.y))
                .unwrap_or(self.group_fields.len()),
        )
    }

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
        if let InputEvent::PointerMoved { position } = event {
            let Some(drag) = self
                .grids
                .get_mut(&id)
                .unwrap()
                .group_drag
                .as_mut()
                .filter(|d| d.from_panel)
            else {
                return Ok(false);
            };
            drag.motion.update(*position);
            self.scene_dirty = true;
            return Ok(true);
        }
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
                if let Some(i) = options.group_drop_slot(bounds, *position) {
                    if i < options.group_fields.len() {
                        let column = grid
                            .source
                            .fields
                            .iter()
                            .position(|f| f == &options.group_fields[i])
                            .expect("validated group field");
                        self.grids.get_mut(&id).unwrap().group_drag = Some(GroupDrag {
                            column: ColumnId(column as u64),
                            from_panel: true,
                            motion: HeaderDragMotion::new(*position),
                        });
                        self.runtime.apply(Mutation::Focus(Some(id)))?;
                        self.runtime.apply(Mutation::CapturePointer(Some(id)))?;
                    }
                    return Ok(true);
                }
                if let Some(GridHitKind::Header(col)) = grid.frame.hit_test(*position) {
                    self.grids.get_mut(&id).unwrap().group_drag = Some(GroupDrag {
                        column: col,
                        from_panel: false,
                        motion: HeaderDragMotion::new(*position),
                    });
                }
                return Ok(false);
            }
            InputEvent::PointerUp {
                position,
                button: PointerButton::Primary,
                ..
            } if grid.group_drag.is_some() => {
                let drag = grid.group_drag.as_ref().unwrap();
                let (col, from_panel) = (drag.column, drag.from_panel);
                let target = options.group_drop_slot(bounds, *position);
                let field = grid.source.fields[col.0 as usize].clone();
                let mut motion = drag.motion;
                motion.update(*position);
                let click = from_panel
                    && motion.position().is_none()
                    && target.and_then(|i| settings.group_fields.get(i)) == Some(&field);
                let descending = grid.source.group_descending(&field);
                let horizontal = grid.model.scroll_offset().0;
                let grid = self.grids.get_mut(&id).unwrap();
                grid.group_drag = None;
                if target.is_none() && !from_panel {
                    return Ok(false);
                }
                grid.cancel_pointer();
                self.runtime.apply(Mutation::CapturePointer(None))?;
                if click {
                    if settings.group_sort_summary.is_some() {
                        settings.group_descending = !descending;
                        self.set_grid_presentation(id, settings)?;
                    } else {
                        use rust_desktop_ui_data::SortDirection::{Ascending, Descending};
                        let mut sort = grid.source.data_query.sort.clone();
                        let direction = if descending { Ascending } else { Descending };
                        if let Some(order) = sort.iter_mut().find(|s| s.field == field) {
                            order.direction = direction;
                        } else {
                            sort.push(SortDescriptor { field, direction });
                        }
                        self.set_grid_sort(id, sort)?;
                    }
                    self.grid_scroll_to(id, horizontal, 0.)?;
                    self.grids.get_mut(&id).unwrap().reveal_selected = false;
                    return Ok(true);
                }
                settings.group_fields.retain(|f| f != &field);
                if let Some(i) = target {
                    settings
                        .group_fields
                        .insert(i.min(settings.group_fields.len()), field);
                }
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
