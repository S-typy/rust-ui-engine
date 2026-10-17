use super::*;
use rust_desktop_ui_core::Modifiers;
impl Controls {
    fn gallery_user_result(
        &mut self,
        id: WidgetId,
        result: Result<(), ControlError>,
    ) -> Result<(), ControlError> {
        match result {
            Ok(()) => Ok(()),
            Err(ControlError::Invalid(message)) => {
                let message = if message.len() <= 4096
                    && !message.chars().any(char::is_control)
                    && self.measure_text(&message, 4096.0, false).is_ok()
                {
                    message
                } else {
                    "Выбор образца отклонён".into()
                };
                if let Some(e) = self.galleries.entries.get_mut(&id) {
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
    pub(in crate::controls) fn activate_gallery(
        &mut self,
        widget: WidgetId,
    ) -> Result<bool, ControlError> {
        let Some((id, key)) = self.galleries.actions.get(&widget).cloned() else {
            return Ok(false);
        };
        if !self.gallery_available(id) {
            return Ok(true);
        }
        let result = self.accept_gallery(id, Some(key), true);
        let accepted = result.is_ok();
        self.gallery_user_result(id, result)?;
        if accepted
            && let Some(handle) = self.galleries.entries[&id]
                .popup
                .as_ref()
                .filter(|p| p.open)
                .map(|p| p.handle)
        {
            self.close_flyout(handle)?;
        }
        Ok(true)
    }
    pub(in crate::controls) fn gallery_before_input(
        &mut self,
        event: &InputEvent,
    ) -> Result<bool, ControlError> {
        if let InputEvent::PointerMoved { position } = event {
            let sample = self
                .runtime
                .hit_test(*position)
                .and_then(|widget| self.galleries.actions.get(&widget).cloned());
            for id in self.galleries.entries.keys().copied().collect::<Vec<_>>() {
                if let Some((owner, key)) = &sample
                    && *owner == id
                    && self.is_enabled(id)
                    && self.galleries.entries[&id]
                        .model
                        .item(key)
                        .is_some_and(|i| i.item.enabled)
                {
                    self.preview_gallery_item(id, key)?;
                } else {
                    self.cancel_gallery_preview(id)?;
                }
            }
            return Ok(false);
        }
        if matches!(
            event,
            InputEvent::PointerLeft | InputEvent::WindowFocused(false)
        ) {
            for id in self.galleries.entries.keys().copied().collect::<Vec<_>>() {
                self.cancel_gallery_preview(id)?;
            }
            self.galleries.pointer = None;
            return Ok(false);
        }
        if let InputEvent::PointerDown {
            position,
            button: PointerButton::Primary,
            ..
        } = event
            && let Some(widget) = self.runtime.hit_test(*position)
            && let Some((id, key)) = self.galleries.actions.get(&widget).cloned()
        {
            self.check_gallery(id)?;
            let report = self.runtime.dispatch(event.clone());
            self.prune_removed_controls()?;
            if let Some(error) = report.errors.into_iter().next() {
                return Err(error.into());
            }
            if !report.default_prevented && self.activation_available(widget) {
                self.galleries.pointer = Some((id, key.clone(), self.galleries.entries[&id].epoch));
                self.pressed = Some(widget);
                self.keyboard_pressed = false;
                self.runtime.apply(Mutation::CapturePointer(Some(widget)))?;
                self.preview_gallery_item(id, &key)?;
            }
            self.synchronize_editor_focus()?;
            self.scene_dirty = true;
            return Ok(true);
        }
        if let InputEvent::PointerUp {
            position,
            button: PointerButton::Primary,
            ..
        } = event
            && let Some((id, key, epoch)) = self.galleries.pointer.take()
        {
            let hit = self.runtime.hit_test(*position);
            let report = self.runtime.dispatch(event.clone());
            self.prune_removed_controls()?;
            self.pressed = None;
            self.runtime.apply(Mutation::CapturePointer(None))?;
            if let Some(error) = report.errors.into_iter().next() {
                return Err(error.into());
            }
            if !report.default_prevented
                && self
                    .galleries
                    .entries
                    .get(&id)
                    .is_some_and(|e| e.epoch == epoch)
                && let Some(widget) = hit
                && self.galleries.actions.get(&widget) == Some(&(id, key))
                && self.activation_available(widget)
            {
                self.activate_gallery(widget)?;
            }
            self.scene_dirty = true;
            return Ok(true);
        }
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
        let id = if self.galleries.entries.contains_key(&focused) {
            focused
        } else if let Some((id, _)) = self.galleries.actions.get(&focused) {
            *id
        } else {
            return Ok(false);
        };
        if *modifiers != Modifiers::default() {
            return Ok(false);
        }
        self.check_gallery(id)?;
        if *key == Key::Escape {
            self.cancel_gallery_preview(id)?;
            if let Some(p) = self.galleries.entries[&id]
                .popup
                .as_ref()
                .filter(|p| p.open)
                .map(|p| p.handle)
            {
                self.close_flyout(p)?;
            }
            return Ok(true);
        }
        if matches!(key, Key::Enter | Key::Space) {
            if !repeat
                && let Some(key) = self.galleries.entries[&id].cursor.clone().or_else(|| {
                    self.galleries
                        .actions
                        .get(&focused)
                        .map(|(_, key)| key.clone())
                })
                && let Some(widget) = self.gallery_item_widget(id, &key)
            {
                self.activate_gallery(widget)?;
            }
            return Ok(true);
        }
        if !matches!(
            key,
            Key::ArrowLeft
                | Key::ArrowRight
                | Key::ArrowUp
                | Key::ArrowDown
                | Key::Home
                | Key::End
                | Key::PageUp
                | Key::PageDown
        ) {
            return Ok(false);
        }
        if !self.is_enabled(id) {
            return Ok(true);
        }
        let e = &self.galleries.entries[&id];
        let order = e.model.order();
        if order.is_empty() {
            return Ok(true);
        }
        let current = e
            .cursor
            .as_deref()
            .or(e.selected.as_deref())
            .and_then(|key| {
                order
                    .iter()
                    .position(|&i| e.model.items()[i].item.key == key)
            });
        let c = current.unwrap_or(0);
        let page = (self.runtime.bounds(id).map_or(240.0, |r| r.height)
            / (e.settings.sample_height + e.settings.gap))
            .floor()
            .max(1.0) as usize
            * e.columns;
        let mut next = match key {
            Key::Home => 0,
            Key::End => order.len() - 1,
            _ if current.is_none() => 0,
            Key::ArrowLeft => c.saturating_sub(1),
            Key::ArrowUp => c.saturating_sub(e.columns),
            Key::PageUp => c.saturating_sub(page),
            Key::ArrowDown => (c + e.columns).min(order.len() - 1),
            Key::PageDown => (c + page).min(order.len() - 1),
            _ => (c + 1).min(order.len() - 1),
        };
        let forward = !matches!(key, Key::ArrowLeft | Key::ArrowUp | Key::PageUp | Key::End);
        while !e.model.items()[order[next]].item.enabled {
            if forward {
                if next + 1 >= order.len() {
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
        let key = e.model.items()[order[next]].item.key.clone();
        let widget = e.samples[&key].widget;
        self.preview_gallery_item(id, &key)?;
        self.runtime.apply(Mutation::Focus(Some(widget)))?;
        self.bring_into_view(widget)?;
        self.synchronize_editor_focus()?;
        self.scene_dirty = true;
        Ok(true)
    }
}
