use super::*;
use rust_desktop_ui_core::Modifiers;

impl Controls {
    fn navigation_action(&self, id: WidgetId) -> Option<(WidgetId, u8, String)> {
        self.navigations.entries.iter().find_map(|(&owner, e)| {
            if id == e.parts.back_button {
                Some((owner, 0, String::new()))
            } else if id == e.parts.forward_button {
                Some((owner, 1, String::new()))
            } else if id == e.parts.menu_button {
                Some((owner, 2, String::new()))
            } else if let Some(p) = e.pages.iter().find(|p| p.selector == id) {
                Some((owner, 3, p.route.clone()))
            } else {
                e.sections
                    .iter()
                    .find(|s| s.header == id || s.office == id)
                    .map(|s| (owner, 4, s.id.clone()))
            }
        })
    }
    pub(in crate::controls) fn activate_navigation(
        &mut self,
        id: WidgetId,
    ) -> Result<bool, ControlError> {
        let Some((owner, action, target)) = self.navigation_action(id) else {
            return Ok(false);
        };
        self.check_navigation(owner)?;
        match action {
            0 => {
                let result = self.navigation_back(owner);
                self.navigation_user_result(owner, result)?;
            }
            1 => {
                let result = self.navigation_forward(owner);
                self.navigation_user_result(owner, result)?;
            }
            2 => {
                let e = self.navigations.entries.get_mut(&owner).unwrap();
                e.hamburger_open = !e.hamburger_open;
                self.present_navigation(owner)?;
            }
            3 => {
                let result = self.navigate(owner, &target, NavigationMode::Push);
                self.navigation_user_result(owner, result)?;
            }
            _ => {
                let e = self.navigations.entries.get_mut(&owner).unwrap();
                if e.settings.view == NavigationView::Accordion && e.expanded_section == target {
                    e.expanded_section.clear();
                } else {
                    e.expanded_section = target;
                }
                self.present_navigation(owner)?;
            }
        }
        Ok(true)
    }
    pub(in crate::controls) fn navigation_before_input(
        &mut self,
        event: &InputEvent,
    ) -> Result<bool, ControlError> {
        if let InputEvent::PointerDown {
            position,
            button: PointerButton::Primary,
            ..
        } = event
            && let Some(id) = self.runtime.hit_test(*position)
            && let Some((owner, _, _)) = self.navigation_action(id)
            && self.activation_available(id)
        {
            self.check_navigation(owner)?;
            // Route selectors are navigation, not an editor acceptance action.
            // Record the focused descendant before pointer focus moves to chrome.
            let focus = self.runtime.focused();
            let remembered = self.navigations.entries[&owner].pages.iter().find_map(|p| {
                focus
                    .filter(|id| self.flyout_descendant(*id, p.host))
                    .or_else(|| self.flyout_focus_anchor_in(p.host))
                    .map(|id| (p.route.clone(), id))
            });
            if let Some((route, focus)) = remembered {
                self.navigations
                    .entries
                    .get_mut(&owner)
                    .unwrap()
                    .pages
                    .iter_mut()
                    .find(|p| p.route == route)
                    .unwrap()
                    .focus = Some(focus);
                self.observed_editor_focus = None;
            }
            let report = self.runtime.dispatch(event.clone());
            self.prune_removed_controls()?;
            if let Some(error) = report.errors.into_iter().next() {
                return Err(error.into());
            }
            if !report.default_prevented && self.activation_available(id) {
                self.navigations.pointer = Some(id);
                self.pressed = Some(id);
                self.keyboard_pressed = false;
                self.runtime.apply(Mutation::CapturePointer(Some(id)))?;
            }
            self.scene_dirty = true;
            return Ok(true);
        }
        if let InputEvent::PointerUp {
            position,
            button: PointerButton::Primary,
            ..
        } = event
            && let Some(id) = self.navigations.pointer.take()
        {
            let hit = self.runtime.hit_test(*position);
            let report = self.runtime.dispatch(event.clone());
            self.prune_removed_controls()?;
            self.pressed = None;
            self.runtime.apply(Mutation::CapturePointer(None))?;
            if let Some(error) = report.errors.into_iter().next() {
                return Err(error.into());
            }
            if !report.default_prevented && hit == Some(id) && self.activation_available(id) {
                self.activate_navigation(id)?;
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
        let Some(owner) = self
            .navigations
            .entries
            .keys()
            .copied()
            .find(|owner| self.flyout_descendant(focused, *owner))
        else {
            return Ok(false);
        };
        if modifiers.alt
            && !modifiers.control
            && !modifiers.super_key
            && matches!(key, Key::ArrowLeft | Key::ArrowRight)
        {
            if !repeat {
                if self
                    .document(focused)
                    .is_some_and(|d| d.composition().is_some())
                {
                    self.cancel_preedit();
                } else if *key == Key::ArrowLeft {
                    let result = self.navigation_back(owner);
                    self.navigation_user_result(owner, result)?;
                } else {
                    let result = self.navigation_forward(owner);
                    self.navigation_user_result(owner, result)?;
                }
            }
            return Ok(true);
        }
        if self.navigation_action(focused).is_some()
            && *modifiers == Modifiers::default()
            && matches!(key, Key::ArrowDown | Key::ArrowUp | Key::Home | Key::End)
        {
            let candidates: Vec<_> = self
                .runtime
                .tree()
                .preorder()
                .into_iter()
                .filter(|id| {
                    self.navigation_action(*id)
                        .is_some_and(|(o, action, _)| o == owner && action >= 3)
                        && self.is_enabled(*id)
                })
                .collect();
            if !candidates.is_empty() {
                let current = candidates.iter().position(|id| *id == focused).unwrap_or(0);
                let index = match key {
                    Key::Home => 0,
                    Key::End => candidates.len() - 1,
                    Key::ArrowUp => current.saturating_sub(1),
                    _ => (current + 1).min(candidates.len() - 1),
                };
                self.runtime
                    .apply(Mutation::Focus(Some(candidates[index])))?;
                self.bring_into_view(candidates[index])?;
                self.synchronize_editor_focus()?;
            }
            return Ok(true);
        }
        Ok(false)
    }
}
