use super::*;
impl Controls {
    pub(in crate::controls) fn cancel_image_gesture(&mut self) -> Result<(), ControlError> {
        if let Some(d) = self.image_editors.drag.take() {
            if let Some(e) = self.image_editors.entries.get_mut(&d.owner) {
                e.view = d.original_view;
                e.crop = d.prior_crop;
                if self.runtime.captured() == Some(e.parts.viewport) {
                    self.runtime.apply(Mutation::CapturePointer(None))?;
                }
            }
            self.scene_dirty = true;
        }
        Ok(())
    }
    pub(super) fn cancel_image_owner_gesture(&mut self, id: WidgetId) -> Result<(), ControlError> {
        if self.image_editors.drag.is_some_and(|d| d.owner == id) {
            self.cancel_image_gesture()?;
        }
        Ok(())
    }
    fn image_editor_hit(&self, point: Point) -> Option<WidgetId> {
        let hit = self.runtime.hit_test(point)?;
        self.image_editors.entries.iter().find_map(|(id, e)| {
            (hit == e.parts.viewport && self.effective_enabled(*id)).then_some(*id)
        })
    }
    fn crop_from_drag(&self, d: Drag) -> Option<PixelCrop> {
        let (r, scale) = self.image_editor_mapping(d.owner)?;
        let image = self.image_editors.entries[&d.owner].session.draft()?;
        let x0 = ((d.start.x.min(d.current.x) - r.x) / scale)
            .floor()
            .clamp(0.0, image.width() as f32) as u32;
        let y0 = ((d.start.y.min(d.current.y) - r.y) / scale)
            .floor()
            .clamp(0.0, image.height() as f32) as u32;
        let x1 = ((d.start.x.max(d.current.x) - r.x) / scale)
            .ceil()
            .clamp(0.0, image.width() as f32) as u32;
        let y1 = ((d.start.y.max(d.current.y) - r.y) / scale)
            .ceil()
            .clamp(0.0, image.height() as f32) as u32;
        (x1 > x0 && y1 > y0).then_some(PixelCrop {
            x: x0,
            y: y0,
            width: x1 - x0,
            height: y1 - y0,
        })
    }
    pub(in crate::controls) fn image_editor_before_input(
        &mut self,
        event: &InputEvent,
    ) -> Result<bool, ControlError> {
        if matches!(event, InputEvent::WindowFocused(false)) {
            self.cancel_image_gesture()?;
            return Ok(false);
        }
        self.sync_image_editors()?;
        if let Some(mut d) = self.image_editors.drag {
            let viewport = self.image_editors.entries[&d.owner].parts.viewport;
            if self.runtime.captured() != Some(viewport) {
                self.cancel_image_gesture()?;
            } else {
                match event {
                    InputEvent::PointerMoved { position } if position.is_finite() => {
                        d.current = *position;
                        self.image_editors.drag = Some(d);
                        if d.crop {
                            let crop = self.crop_from_drag(d);
                            self.image_editors.entries.get_mut(&d.owner).unwrap().crop = crop;
                        } else {
                            let e = self.image_editors.entries.get_mut(&d.owner).unwrap();
                            e.view.fit = false;
                            e.view.pan = Point::new(
                                (d.pan.x + position.x - d.start.x).clamp(-262144.0, 262144.0),
                                (d.pan.y + position.y - d.start.y).clamp(-262144.0, 262144.0),
                            );
                        }
                        self.scene_dirty = true;
                        return Ok(true);
                    }
                    InputEvent::PointerUp {
                        position,
                        button: PointerButton::Primary,
                        ..
                    } => {
                        let inside = self
                            .runtime
                            .bounds(viewport)
                            .zip(self.runtime.clip(viewport))
                            .and_then(|(a, b)| a.intersection(b))
                            .is_some_and(|r| r.contains(position.x, position.y));
                        if inside {
                            self.image_editors.drag = None;
                            self.runtime.apply(Mutation::CapturePointer(None))?;
                        } else {
                            self.cancel_image_gesture()?;
                        }
                        return Ok(true);
                    }
                    InputEvent::KeyDown {
                        key: Key::Escape, ..
                    } => {
                        self.cancel_image_gesture()?;
                        return Ok(true);
                    }
                    _ => {}
                }
            }
        }
        match event {
            InputEvent::PointerDown {
                position,
                button: PointerButton::Primary,
                ..
            } => {
                if self.popup.is_some() && self.image_editor_hit(*position).is_some() {
                    self.popup_pointer_down(*position)?;
                    self.update(self.viewport)?;
                }
                let Some(id) = self.image_editor_hit(*position) else {
                    return Ok(false);
                };
                let e = &self.image_editors.entries[&id];
                if e.session.draft().is_none() {
                    return Ok(false);
                }
                let original_view = e.view;
                let prior_crop = e.crop;
                let mut view = e.view;
                if view.fit {
                    view.zoom = self
                        .image_editor_mapping(id)
                        .map_or(1.0, |m| m.1)
                        .clamp(0.05, 32.0);
                    view.fit = false;
                }
                let crop = e.crop_mode && self.image_editor_editable(id);
                let viewport = e.parts.viewport;
                if !self.try_popup_focus(viewport)? {
                    return Ok(false);
                }
                self.runtime
                    .apply(Mutation::CapturePointer(Some(viewport)))?;
                self.image_editors.entries.get_mut(&id).unwrap().view = view;
                self.image_editors.drag = Some(Drag {
                    owner: id,
                    start: *position,
                    current: *position,
                    pan: view.pan,
                    original_view,
                    prior_crop,
                    crop,
                });
                Ok(true)
            }
            InputEvent::Wheel {
                position,
                delta,
                modifiers,
            } => {
                let Some(id) = self.image_editor_hit(*position) else {
                    return Ok(false);
                };
                if !delta.is_finite() {
                    return Ok(true);
                }
                if modifiers.control {
                    self.image_editor_action(
                        id,
                        if delta.y >= 0.0 {
                            Action::ZoomIn
                        } else {
                            Action::ZoomOut
                        },
                    )?;
                } else {
                    let mut view = self.image_editors.entries[&id].view;
                    if view.fit {
                        view.zoom = self
                            .image_editor_mapping(id)
                            .map_or(1.0, |m| m.1)
                            .clamp(0.05, 32.0);
                    }
                    view.fit = false;
                    view.pan.x = (view.pan.x + delta.x).clamp(-262144.0, 262144.0);
                    view.pan.y = (view.pan.y + delta.y).clamp(-262144.0, 262144.0);
                    self.set_image_editor_viewport(id, view)?;
                }
                Ok(true)
            }
            InputEvent::KeyDown {
                key,
                modifiers,
                repeat,
            } => {
                let Some(id) = self.image_editors.entries.iter().find_map(|(id, e)| {
                    (self.runtime.focused() == Some(e.parts.viewport)
                        && self.effective_enabled(*id))
                    .then_some(*id)
                }) else {
                    return Ok(false);
                };
                let action = match key {
                    Key::Home => Some(Action::Fit),
                    Key::Escape => Some(Action::Cancel),
                    Key::Enter if modifiers.control => Some(Action::Apply),
                    Key::Character(c) if modifiers.control && c.eq_ignore_ascii_case("z") => {
                        Some(Action::Undo)
                    }
                    Key::Character(c) if modifiers.control && c.eq_ignore_ascii_case("y") => {
                        Some(Action::Redo)
                    }
                    Key::Character(c) if c == "+" || c == "=" => Some(Action::ZoomIn),
                    Key::Character(c) if c == "-" => Some(Action::ZoomOut),
                    _ => None,
                };
                if let Some(action) = action {
                    if !repeat {
                        match self.image_editor_action(id, action) {
                            Err(ControlError::Invalid(e)) => {
                                self.image_editor_report_error(id, &e)?
                            }
                            result => result?,
                        }
                    }
                    return Ok(true);
                }
                if matches!(key, Key::Delete) && self.image_editor_editable(id) {
                    self.set_image_editor_draft(id, None)?;
                    return Ok(true);
                }
                let delta = match key {
                    Key::ArrowLeft => Point::new(24.0, 0.0),
                    Key::ArrowRight => Point::new(-24.0, 0.0),
                    Key::ArrowUp => Point::new(0.0, 24.0),
                    Key::ArrowDown => Point::new(0.0, -24.0),
                    _ => return Ok(false),
                };
                let mut view = self.image_editors.entries[&id].view;
                if view.fit {
                    view.zoom = self
                        .image_editor_mapping(id)
                        .map_or(1.0, |m| m.1)
                        .clamp(0.05, 32.0);
                }
                view.fit = false;
                view.pan.x = (view.pan.x + delta.x).clamp(-262144.0, 262144.0);
                view.pan.y = (view.pan.y + delta.y).clamp(-262144.0, 262144.0);
                self.set_image_editor_viewport(id, view)?;
                Ok(true)
            }
            _ => Ok(false),
        }
    }
}
