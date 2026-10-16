//! Selection rows and composite chrome share one retained interaction model.
use super::*;

impl Controls {
    pub(super) fn paint_rich_surface(
        &mut self,
        id: WidgetId,
        bounds: Rect,
        clip: Rect,
    ) -> Result<bool, ControlError> {
        if let Some(composite) = self.rich.composites.get(&id) {
            let editor = composite.parts.editor;
            let control = &self.controls[&editor];
            let enabled = self.is_enabled(id);
            let focused = self
                .runtime
                .focused()
                .is_some_and(|focused| self.rich_focus_editor(focused) == Some(editor));
            let hovered = self
                .runtime
                .hovered()
                .is_some_and(|hovered| self.rich_owner(hovered) == Some(id));
            let state =
                EditBoxVisualState::from_flags(enabled, composite.read_only, focused, hovered);
            let mut style = control.edit_box_style.resolve(self.theme, state);
            if enabled && control.validation_error.is_some() {
                style.apply_invalid(self.theme, control.edit_box_style.invalid);
            }
            let radius = style
                .corner_radius
                .min(bounds.width.min(bounds.height) * 0.5);
            editor_fill(&mut self.scene, bounds, clip, radius, style.background);
            let border = if focused && !self.theme.is_fluent() {
                style.accent
            } else {
                style.border
            };
            let width = if focused && !self.theme.is_fluent() {
                style.border_width.max(2.0)
            } else {
                style.border_width
            };
            shape_border(&mut self.scene, bounds, clip, radius, width, border);
            if focused && self.theme.is_fluent() {
                let indent = radius.max(2.0).min(bounds.width * 0.5);
                let thickness = 2.0_f32.min(bounds.height);
                fill_clipped(
                    &mut self.scene,
                    Rect::new(
                        bounds.x + indent,
                        bounds.y + bounds.height - thickness,
                        (bounds.width - 2.0 * indent).max(0.0),
                        thickness,
                    ),
                    clip,
                    style.accent,
                );
            }
            return Ok(true);
        }
        if self.rich.lists.contains_key(&id) {
            if self
                .rich
                .popup
                .as_ref()
                .is_some_and(|popup| popup.list == Some(id))
            {
                return Ok(true);
            }
            let chrome = self.theme.chrome();
            let radius = if self.theme.is_fluent() { 4.0 } else { 0.0 };
            editor_fill(
                &mut self.scene,
                bounds,
                clip,
                radius,
                self.theme.tokens().surface,
            );
            shape_border(
                &mut self.scene,
                bounds,
                clip,
                radius,
                1.0,
                chrome.control_border,
            );
            return Ok(true);
        }
        let Some(control) = self.controls.get(&id) else {
            return Ok(false);
        };
        if control.role != Some(Role::ListItem) {
            return Ok(false);
        }
        let enabled = self.is_enabled(id);
        let selected = control.checked;
        let calendar_cell = self.is_calendar_cell(id);
        let hovered = self.runtime.hovered() == Some(id) && enabled;
        let owner = self.rich.actions.get(&id).and_then(|a| match a {
            selection::RichAction::Select { owner, .. } => Some(*owner),
            _ => None,
        });
        let cursor_focused = owner.is_some_and(|owner| {
            self.runtime.focused() == Some(owner)
                && self.rich.lists.get(&owner).is_some_and(|list| {
                    list.cursor
                        .and_then(|index| index.checked_sub(list.row_start))
                        .and_then(|index| list.rows.get(index))
                        .copied()
                        == Some(id)
                })
        });
        let focused = self.runtime.focused() == Some(id) || cursor_focused;
        let chrome = self.theme.chrome();
        let tokens = self.theme.tokens();
        let radius = if self.theme.is_fluent() { 4.0 } else { 0.0 };
        if selected {
            editor_fill(&mut self.scene, bounds, clip, radius, tokens.selected);
        }
        if hovered {
            editor_fill(
                &mut self.scene,
                bounds,
                clip,
                radius,
                if self.pressed == Some(id) {
                    chrome.subtle_pressed
                } else {
                    chrome.subtle_hover
                },
            );
        }
        if selected && !calendar_cell {
            let height = 16.0_f32.min(bounds.height);
            let width = 3.0_f32.min(bounds.width);
            self.scene.rounded_fill(
                Rect::new(
                    bounds.x + 3.0_f32.min((bounds.width - width).max(0.0)),
                    bounds.y + (bounds.height - height) * 0.5,
                    width,
                    height,
                ),
                clip,
                1.5,
                if enabled {
                    tokens.focus
                } else {
                    chrome.disabled_text
                },
            );
        }
        if focused && enabled {
            shape_border(&mut self.scene, bounds, clip, radius, 1.0, tokens.focus);
        }
        if !control.label.is_empty() {
            let text_bounds = Rect::new(
                bounds.x
                    + if calendar_cell {
                        4.0_f32.min(bounds.width)
                    } else {
                        10.0_f32.min(bounds.width)
                    },
                bounds.y,
                (bounds.width - if calendar_cell { 8.0 } else { 18.0 }).max(0.0),
                bounds.height,
            );
            let mut run = TextRun {
                sensitive: false,
                bounds: text_bounds,
                clip: text_bounds
                    .intersection(clip)
                    .unwrap_or(Rect::new(0.0, 0.0, 0.0, 0.0)),
                text: control.label.clone(),
                font_size: tokens.font_size,
                color: if enabled {
                    tokens.text
                } else {
                    chrome.disabled_text
                },
                weight: 400,
                align: if calendar_cell {
                    TextAlign::Center
                } else {
                    TextAlign::Start
                },
                wrap: false,
            };
            let height = self
                .engine
                .prepare(&run, 1.0)
                .map_err(|e| ControlError::Text(e.to_string()))?
                .height;
            let offset = ((text_bounds.height - height) * 0.5).max(0.0);
            run.bounds.y += offset;
            run.bounds.height = (text_bounds.height - offset).max(0.0);
            self.scene.text(run);
        }
        Ok(true)
    }
}
