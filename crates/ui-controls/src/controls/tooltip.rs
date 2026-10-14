use super::{ControlError, Controls};
use rust_desktop_ui_core::{InputEvent, Rect, Size, TextAlign, TextRun, WidgetId};
use std::time::{Duration, Instant};

const HOVER_DELAY: Duration = Duration::from_millis(500);
const MAX_TOOLTIP_BYTES: usize = 4096;
const PADDING: f32 = 8.0;

#[derive(Clone, Copy)]
struct PendingTooltip {
    anchor: WidgetId,
    deadline: Instant,
}

pub(super) struct TooltipState {
    pending: Option<PendingTooltip>,
    visible: Option<WidgetId>,
    bounds: Option<Rect>,
    window_focused: bool,
}

impl Default for TooltipState {
    fn default() -> Self {
        Self {
            pending: None,
            visible: None,
            bounds: None,
            window_focused: true,
        }
    }
}

impl Controls {
    /// Sets the anchor's semantic and visual tooltip. Whitespace-only text
    /// clears it; more than 4096 UTF-8 bytes is rejected without changing state.
    pub fn set_tooltip(
        &mut self,
        id: WidgetId,
        text: impl Into<String>,
    ) -> Result<(), ControlError> {
        let text = text.into();
        if text.len() > MAX_TOOLTIP_BYTES {
            return Err(ControlError::Invalid(
                "Tooltip text must not exceed 4096 UTF-8 bytes".into(),
            ));
        }
        if self.runtime.tree().node(id).is_none() || !self.controls.contains_key(&id) {
            return Err(ControlError::Invalid("Unknown tooltip anchor".into()));
        }
        let text = (!text.trim().is_empty()).then_some(text);
        if self.controls[&id].tooltip != text {
            self.controls.get_mut(&id).unwrap().tooltip = text;
            if self.tooltip_state.visible == Some(id)
                || self
                    .tooltip_state
                    .pending
                    .is_some_and(|pending| pending.anchor == id)
            {
                self.dismiss_tooltip();
            }
            self.scene_dirty = true;
        }
        Ok(())
    }

    /// Tooltip attached to a live control, whether or not currently visible.
    pub fn tooltip(&self, id: WidgetId) -> Option<&str> {
        self.runtime.tree().node(id)?;
        self.controls.get(&id)?.tooltip.as_deref()
    }

    /// Next hover deadline for the host's event-loop timer. No idle polling is
    /// needed when this returns `None`.
    pub fn tooltip_deadline(&self) -> Option<Instant> {
        let pending = self.tooltip_state.pending?;
        self.tooltip_eligible(pending.anchor)
            .then_some(pending.deadline)
    }

    /// Bounds of the tooltip most recently painted, in logical pixels.
    pub fn tooltip_bounds(&self) -> Option<Rect> {
        let anchor = self.tooltip_state.visible?;
        self.tooltip_eligible(anchor)
            .then_some(self.tooltip_state.bounds)
            .flatten()
    }

    /// Advances hover timing and prepares the overlay. Returns whether its
    /// visible bounds changed; callers can request a redraw when true.
    pub fn tooltip_tick(&mut self, now: Instant) -> Result<bool, ControlError> {
        let previous = (self.tooltip_state.visible, self.tooltip_state.bounds);
        self.update(self.viewport)?;
        if let Some(pending) = self.tooltip_state.pending
            && now >= pending.deadline
        {
            self.tooltip_state.pending = None;
            if self.tooltip_eligible(pending.anchor) {
                self.tooltip_state.visible = Some(pending.anchor);
                self.scene_dirty = true;
                self.update(self.viewport)?;
            }
        }
        Ok(previous != (self.tooltip_state.visible, self.tooltip_state.bounds))
    }

    /// Hides the overlay and cancels its hover deadline without changing focus
    /// or pointer state. Hosts that consume input before `dispatch` should call
    /// this for those key, button, wheel, leave, blur, or captured-drag events.
    pub fn dismiss_tooltip(&mut self) {
        self.tooltip_state.pending = None;
        if self.tooltip_state.visible.take().is_some() {
            self.scene_dirty = true;
        }
        self.tooltip_state.bounds = None;
    }

    pub(super) fn tooltip_before_input(&mut self, event: &InputEvent) {
        if let InputEvent::WindowFocused(focused) = event {
            self.tooltip_state.window_focused = *focused;
        }
        if !matches!(
            event,
            InputEvent::PointerMoved { .. } | InputEvent::WindowFocused(true)
        ) {
            self.dismiss_tooltip();
        }
    }

    pub(super) fn tooltip_after_input(&mut self, event: &InputEvent) {
        if !matches!(event, InputEvent::PointerMoved { .. }) {
            return;
        }
        let anchor = self.tooltip_hover_anchor();
        let current = self
            .tooltip_state
            .visible
            .or(self.tooltip_state.pending.map(|p| p.anchor));
        if anchor == current {
            return;
        }
        self.dismiss_tooltip();
        if let Some(anchor) = anchor {
            self.tooltip_state.pending = Some(PendingTooltip {
                anchor,
                deadline: Instant::now() + HOVER_DELAY,
            });
        }
    }

    pub(super) fn tooltip_sync(&mut self) {
        let anchor = self
            .tooltip_state
            .visible
            .or(self.tooltip_state.pending.map(|p| p.anchor));
        if anchor.is_some_and(|id| !self.tooltip_eligible(id)) {
            self.dismiss_tooltip();
        }
    }

    fn tooltip_hover_anchor(&self) -> Option<WidgetId> {
        let mut candidate = self.runtime.hovered();
        while let Some(id) = candidate {
            if self.tooltip(id).is_some() && self.tooltip_eligible(id) {
                return Some(id);
            }
            candidate = self.runtime.tree().parent(id);
        }
        None
    }

    fn tooltip_eligible(&self, anchor: WidgetId) -> bool {
        if !self.tooltip_state.window_focused
            || self.popup_open()
            || self.runtime.captured().is_some()
            || self.pressed.is_some()
            || self.selecting.is_some()
            || !self.is_enabled(anchor)
            || self.tooltip(anchor).is_none()
        {
            return false;
        }
        let mut hovered = self.runtime.hovered();
        while let Some(id) = hovered {
            if id == anchor {
                return self.tooltip_anchor_bounds(anchor).is_some();
            }
            hovered = self.runtime.tree().parent(id);
        }
        false
    }

    fn tooltip_anchor_bounds(&self, anchor: WidgetId) -> Option<Rect> {
        self.runtime
            .bounds(anchor)?
            .intersection(self.runtime.clip(anchor)?)?
            .intersection(Rect::new(
                0.0,
                0.0,
                self.viewport.width,
                self.viewport.height,
            ))
    }

    pub(super) fn paint_tooltip(&mut self) -> Result<(), ControlError> {
        self.tooltip_state.bounds = None;
        let Some(anchor) = self.tooltip_state.visible else {
            return Ok(());
        };
        if !self.tooltip_eligible(anchor) {
            self.dismiss_tooltip();
            return Ok(());
        }
        let Some(anchor_bounds) = self.tooltip_anchor_bounds(anchor) else {
            return Ok(());
        };
        let text = self.tooltip(anchor).unwrap().to_owned();
        let available = (self.viewport.width.min(320.0) - PADDING * 2.0).max(1.0);
        let measured = self.measure_text(&text, available, true)?;
        let desired = Size::new(
            (measured.width.ceil() + PADDING * 2.0).min(self.viewport.width.min(320.0)),
            (measured.height.ceil() + PADDING * 2.0).min(self.viewport.height),
        );
        let bounds = super::popup::place_overlay(anchor_bounds, desired, self.viewport, false);
        let clip = Rect::new(0.0, 0.0, self.viewport.width, self.viewport.height);
        let tokens = self.theme.tokens();
        if self.theme.is_fluent() {
            super::appearance::paint_flyout(&mut self.scene, bounds, clip, self.theme);
        } else {
            let radius = 4.0_f32.min(bounds.width.min(bounds.height) * 0.5);
            self.scene
                .rounded_fill(bounds, clip, radius, tokens.surface);
            self.scene
                .rounded_stroke(bounds, clip, radius, 1.0, tokens.border);
        }
        let padding_x = PADDING.min(bounds.width * 0.5);
        let padding_y = PADDING.min(bounds.height * 0.5);
        let content = Rect::new(
            bounds.x + padding_x,
            bounds.y + padding_y,
            (bounds.width - padding_x * 2.0).max(0.0),
            (bounds.height - padding_y * 2.0).max(0.0),
        );
        self.scene.text(TextRun {
            bounds: content,
            clip: content,
            text,
            font_size: tokens.font_size,
            color: tokens.text,
            weight: 400,
            align: TextAlign::Start,
            wrap: true,
        });
        self.tooltip_state.bounds = Some(bounds);
        Ok(())
    }
}

#[cfg(test)]
#[path = "tooltip_tests.rs"]
mod tests;
