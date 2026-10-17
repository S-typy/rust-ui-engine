use super::*;
use rust_desktop_ui_core::Modifiers;
impl Controls {
    /// Invoke one enabled tile. Selection/source validation and queue capacity
    /// are checked before activating its optional application command.
    pub fn invoke_tile(&mut self, id: WidgetId, key: &str) -> Result<(), ControlError> {
        self.check_tile_parts(id)?;
        if !self.tile_available(id) {
            return Err(tile_error("Tile collection is unavailable"));
        }
        let e = &self.tiles.entries[&id];
        let pos = *e
            .geometry
            .positions
            .get(key)
            .ok_or_else(|| tile_error("Unknown tile key"))?;
        let item = &e.model.items()[e.geometry.order[pos]];
        if !item.item.enabled {
            return Err(tile_error("Disabled tiles cannot be invoked"));
        }
        let command = item.command.clone();
        let mut keys = if e.settings.selection == SelectionMode::Multiple {
            e.keys.clone()
        } else {
            Vec::new()
        };
        if !keys.iter().any(|k| k == key) {
            keys.push(key.into());
        }
        self.validate_tile_selection(id, &e.model, &e.settings, &keys)?;
        let changed = e.keys != keys;
        if e.events.len() + usize::from(changed) + 1 > 256 {
            return Err(tile_error("Drain tile events before invoking"));
        }
        if let Some(command) = &command
            && !self.commands.activate(command)
        {
            return Err(tile_error("Tile command is unavailable"));
        }
        let e = self.tiles.entries.get_mut(&id).unwrap();
        e.cursor = Some(key.into());
        e.keys = keys.clone();
        e.error = None;
        self.controls.get_mut(&id).unwrap().validation_error = None;
        if changed {
            e.events.push(TileEventKind::SelectionChanged(keys));
        }
        e.events.push(TileEventKind::Activated {
            key: key.into(),
            command,
        });
        self.refresh_tile_selection(id)
    }
    pub(in crate::controls) fn activate_tile(
        &mut self,
        widget: WidgetId,
    ) -> Result<bool, ControlError> {
        let Some((id, key, invoke)) = self.tiles.actions.get(&widget).cloned() else {
            return Ok(false);
        };
        if !self.tile_available(id) {
            return Ok(true);
        }
        let result = if invoke {
            self.invoke_tile(id, &key)
        } else {
            let e = &self.tiles.entries[&id];
            let mut keys = if e.settings.selection == SelectionMode::Multiple {
                e.keys.clone()
            } else {
                Vec::new()
            };
            if let Some(i) = keys.iter().position(|k| k == &key) {
                keys.remove(i);
            } else {
                keys.push(key.clone());
            }
            self.select_tiles(id, keys, true)
        };
        self.tile_user_result(id, result)?;
        Ok(true)
    }
    fn tile_user_result(
        &mut self,
        id: WidgetId,
        result: Result<(), ControlError>,
    ) -> Result<(), ControlError> {
        match result {
            Ok(()) => Ok(()),
            Err(ControlError::Invalid(message)) => {
                let message = if message.len() <= 4096 && !message.chars().any(char::is_control) {
                    message
                } else {
                    "Действие плитки отклонено".into()
                };
                if let Some(e) = self.tiles.entries.get_mut(&id) {
                    e.error = Some(message.clone());
                }
                if let Some(control) = self.controls.get_mut(&id) {
                    control.validation_error = Some(message);
                }
                self.scene_dirty = true;
                Ok(())
            }
            Err(error) => Err(error),
        }
    }
    pub(in crate::controls) fn tile_before_input(
        &mut self,
        event: &InputEvent,
    ) -> Result<bool, ControlError> {
        let InputEvent::KeyDown {
            key,
            modifiers,
            repeat,
        } = event
        else {
            return Ok(false);
        };
        let Some(focused) = self.runtime.focused() else {
            return Ok(false);
        };
        let id = if self.tiles.entries.contains_key(&focused) {
            focused
        } else if let Some((owner, _, _)) = self.tiles.actions.get(&focused) {
            *owner
        } else {
            return Ok(false);
        };
        let navigation = matches!(
            key,
            Key::ArrowLeft
                | Key::ArrowRight
                | Key::ArrowUp
                | Key::ArrowDown
                | Key::Home
                | Key::End
                | Key::PageUp
                | Key::PageDown
        );
        let invoke = matches!(key, Key::Enter | Key::Space);
        if !navigation && !invoke {
            return Ok(false);
        }
        if modifiers.alt || modifiers.super_key {
            return Ok(false);
        }
        if !self.tile_available(id) {
            return Ok(true);
        }
        self.check_tile_parts(id)?;
        let e = &self.tiles.entries[&id];
        let current = self
            .tiles
            .actions
            .get(&focused)
            .and_then(|(_, key, _)| e.geometry.positions.get(key).copied())
            .or_else(|| {
                e.cursor
                    .as_deref()
                    .and_then(|key| e.geometry.positions.get(key).copied())
            });
        if invoke {
            if !repeat
                && *modifiers == Modifiers::default()
                && let Some(pos) = current
            {
                let key = e.model.items()[e.geometry.order[pos]].item.key.clone();
                let result = self.invoke_tile(id, &key);
                self.tile_user_result(id, result)?;
            }
            return Ok(true);
        }
        let next = if e.geometry.order.is_empty() {
            None
        } else {
            let position = current.unwrap_or(0);
            match key {
                Key::Home => Some(0),
                Key::End => Some(e.geometry.order.len() - 1),
                Key::PageUp | Key::PageDown => {
                    let rect = e.geometry.rects[e.geometry.order[position]];
                    let height = self.runtime.bounds(id).map_or(320.0, |r| r.height);
                    let y = if *key == Key::PageUp {
                        (rect.y - height).max(0.0)
                    } else {
                        rect.y + height
                    };
                    let row = e
                        .geometry
                        .shelves
                        .partition_point(|row| row.y + row.height <= y)
                        .min(e.geometry.shelves.len().saturating_sub(1));
                    e.geometry.shelves.get(row).map(|row| row.items.start)
                }
                _ if current.is_none() => Some(0),
                _ => e.geometry.neighbor(position, key.clone()),
            }
        };
        let Some(mut next) = next else {
            return Ok(true);
        };
        let forward = !matches!(key, Key::ArrowLeft | Key::ArrowUp | Key::PageUp | Key::End);
        while !e.model.items()[e.geometry.order[next]].item.enabled {
            if forward {
                if next + 1 >= e.geometry.order.len() {
                    return Ok(true);
                }
                next += 1;
            } else {
                let Some(previous) = next.checked_sub(1) else {
                    return Ok(true);
                };
                next = previous;
            }
        }
        if modifiers.control && modifiers.shift {
            if *repeat {
                return Ok(true);
            }
            let Some(current) = current else {
                return Ok(true);
            };
            let key = e.model.items()[e.geometry.order[current]].item.key.clone();
            let destination = &e.model.items()[e.geometry.order[next]];
            let group = destination.group.clone();
            let before = if next < current {
                Some(destination.item.key.clone())
            } else {
                e.geometry
                    .order
                    .iter()
                    .skip(next + 1)
                    .map(|&i| &e.model.items()[i])
                    .find(|tile| tile.group == group && tile.item.key != key)
                    .map(|tile| tile.item.key.clone())
            };
            let result = self.move_tile(id, &key, &group, before.as_deref());
            let moved = result.is_ok();
            self.tile_user_result(id, result)?;
            if moved {
                self.focus_tile(id, &key)?;
            }
        } else if *modifiers == Modifiers::default() {
            let key = e.model.items()[e.geometry.order[next]].item.key.clone();
            self.focus_tile(id, &key)?;
        }
        Ok(true)
    }
    pub fn focus_tile(&mut self, id: WidgetId, key: &str) -> Result<(), ControlError> {
        self.check_tile_parts(id)?;
        let e = &self.tiles.entries[&id];
        let pos = *e
            .geometry
            .positions
            .get(key)
            .ok_or_else(|| tile_error("Unknown tile key"))?;
        if !e.model.items()[e.geometry.order[pos]].item.enabled {
            return Err(tile_error("Disabled tiles cannot receive cursor"));
        }
        let rect = e.geometry.rects[e.geometry.order[pos]];
        self.scroll_rect_into_view(id, rect)?;
        self.realize_tiles(id)?;
        let widget = self
            .tile_item_widget(id, key)
            .ok_or_else(|| tile_error("Tile was not materialized"))?;
        self.tiles.entries.get_mut(&id).unwrap().cursor = Some(key.into());
        self.runtime.apply(Mutation::Focus(Some(widget)))?;
        self.synchronize_editor_focus()?;
        self.scene_dirty = true;
        Ok(())
    }
}
