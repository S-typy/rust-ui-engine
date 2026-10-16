use super::{ControlError, Controls};
use crate::IconSymbol;
use rust_desktop_ui_core::{
    Bitmap, ImagePrimitive, InputEvent, Rect, Size, TextAlign, TextRun, WidgetId,
};
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

const HOVER_DELAY: Duration = Duration::from_millis(500);
const MAX_TOOLTIP_BYTES: usize = 4096;
const PADDING: f32 = 8.0;

/// Noninteractive help. `image_description` describes information in an image;
/// all text is also exposed on the anchor, without waiting for pointer hover.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RichToolTip {
    pub header: String,
    pub text: String,
    pub icon: Option<IconSymbol>,
    pub image: Option<Bitmap>,
    pub image_description: String,
}

#[derive(Clone, Copy)]
struct PendingTooltip {
    anchor: WidgetId,
    deadline: Instant,
}

pub(super) struct TooltipState {
    rich: HashMap<WidgetId, RichToolTip>,
    pending: Option<PendingTooltip>,
    visible: Option<WidgetId>,
    bounds: Option<Rect>,
    window_focused: bool,
}

impl Default for TooltipState {
    fn default() -> Self {
        Self {
            rich: HashMap::new(),
            pending: None,
            visible: None,
            bounds: None,
            window_focused: true,
        }
    }
}

impl Controls {
    /// Atomically installs header/body/icon/bitmap help. Text including image
    /// description is limited to 4096 UTF-8 bytes; at most 256 rich tips and
    /// 16MiB unique bitmap storage are retained in one Controls instance.
    pub fn set_rich_tooltip(&mut self, id: WidgetId, tip: RichToolTip) -> Result<(), ControlError> {
        let bytes = tip
            .header
            .len()
            .checked_add(tip.text.len())
            .and_then(|bytes| bytes.checked_add(tip.image_description.len()));
        if bytes.is_none_or(|bytes| bytes > MAX_TOOLTIP_BYTES) || tip.header.len() > 1024 {
            return Err(ControlError::Invalid(
                "Rich tooltip text exceeds its UTF-8 byte limit".into(),
            ));
        }
        let description = [&tip.header, &tip.text, &tip.image_description]
            .into_iter()
            .filter(|text| !text.trim().is_empty())
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join("\n");
        if description.len() > MAX_TOOLTIP_BYTES
            || tip.header.len() > 1024
            || (tip.image.is_some() && tip.image_description.trim().is_empty())
        {
            return Err(ControlError::Invalid(
                "Rich tooltip requires bounded text and an image description".into(),
            ));
        }
        if !self.tooltip_state.rich.contains_key(&id) && self.tooltip_state.rich.len() >= 256 {
            return Err(ControlError::Invalid(
                "At most 256 rich tooltips are supported".into(),
            ));
        }
        let mut seen = HashSet::new();
        let mut bytes = 0usize;
        for image in self
            .tooltip_state
            .rich
            .iter()
            .filter(|(key, _)| **key != id)
            .filter_map(|(_, tip)| tip.image.as_ref())
            .chain(tip.image.as_ref())
        {
            if seen.insert(image.id()) {
                bytes += image.pixels().len();
            }
        }
        if bytes > 16 * 1024 * 1024 {
            return Err(ControlError::Invalid(
                "Rich tooltip bitmap budget exceeds 16MiB".into(),
            ));
        }
        self.set_tooltip(id, description)?;
        if self.tooltip(id).is_some() {
            self.tooltip_state.rich.insert(id, tip);
        }
        self.scene_dirty = true;
        Ok(())
    }
    pub fn rich_tooltip(&self, id: WidgetId) -> Option<&RichToolTip> {
        self.runtime.tree().node(id)?;
        self.tooltip_state.rich.get(&id)
    }
    pub(super) fn prune_rich_tooltips(&mut self) {
        self.tooltip_state
            .rich
            .retain(|id, _| self.runtime.tree().contains(*id));
    }
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
        self.tooltip_state.rich.remove(&id);
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
        let rich = self.rich_tooltip(anchor).cloned();
        let text = rich.as_ref().map_or_else(
            || self.tooltip(anchor).unwrap().to_owned(),
            |tip| tip.text.clone(),
        );
        let available = (self.viewport.width.min(320.0) - PADDING * 2.0).max(1.0);
        let measured = self.measure_text(&text, available, true)?;
        let header = rich
            .as_ref()
            .filter(|tip| !tip.header.is_empty())
            .map(|tip| {
                self.measure_text(
                    &tip.header,
                    (available - if tip.icon.is_some() { 24.0 } else { 0.0 }).max(1.0),
                    true,
                )
            })
            .transpose()?;
        let header_height = header.map_or(
            if rich.as_ref().is_some_and(|tip| tip.icon.is_some()) {
                24.0
            } else {
                0.0
            },
            |size| size.height.max(24.0),
        ) + if header.is_some() { 4.0 } else { 0.0 };
        let image_height = rich
            .as_ref()
            .and_then(|tip| tip.image.as_ref())
            .map_or(0.0, |image| {
                (available / image.width() as f32)
                    .min(1.0)
                    .min(120.0 / image.height() as f32)
                    * image.height() as f32
                    + 6.0
            });
        let desired = Size::new(
            (measured
                .width
                .ceil()
                .max(header.map_or(0.0, |size| size.width + 24.0))
                .max(if image_height > 0.0 { available } else { 0.0 })
                + PADDING * 2.0)
                .min(self.viewport.width.min(320.0)),
            (measured.height.ceil() + header_height + image_height + PADDING * 2.0)
                .min(self.viewport.height),
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
        let mut content = Rect::new(
            bounds.x + padding_x,
            bounds.y + padding_y,
            (bounds.width - padding_x * 2.0).max(0.0),
            (bounds.height - padding_y * 2.0).max(0.0),
        );
        let full_content = content;
        if let Some(tip) = &rich {
            if let Some(icon) = tip.icon {
                crate::primitives::paint_icon(
                    &mut self.scene,
                    icon,
                    Rect::new(content.x, content.y, 20.0, 20.0),
                    full_content,
                    tokens.text,
                );
            }
            if !tip.header.is_empty() {
                let inset = if tip.icon.is_some() { 24.0 } else { 0.0 };
                self.scene.text(TextRun {
                    bounds: Rect::new(
                        content.x + inset,
                        content.y,
                        (content.width - inset).max(0.0),
                        header_height,
                    ),
                    clip: full_content,
                    text: tip.header.clone(),
                    font_size: tokens.font_size,
                    color: tokens.text,
                    weight: 600,
                    align: TextAlign::Start,
                    wrap: true,
                });
            }
            content.y += header_height;
            content.height = (content.height - header_height).max(0.0);
            if let Some(image) = &tip.image {
                let height = (image_height - 6.0).max(0.0);
                let width = height * image.width() as f32 / image.height() as f32;
                let mut primitive = ImagePrimitive::new(
                    image.clone(),
                    Rect::new(content.x, content.y, width, height),
                );
                primitive.clip = full_content;
                self.scene.image(primitive);
                content.y += image_height;
                content.height = (content.height - image_height).max(0.0);
            }
        }
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
