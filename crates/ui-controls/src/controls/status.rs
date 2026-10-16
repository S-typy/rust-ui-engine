//! Retained status regions, progress indicators and a compact zoom slider.
use super::*;
use rust_desktop_ui_core::{Align, Justify};

/// Both hosts clip their children. The left region yields space to the right.
/// Remove the container or an ancestor, never either required host alone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StatusBar {
    pub container: WidgetId,
    pub left: WidgetId,
    pub right: WidgetId,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProgressBarSettings {
    pub minimum: f64,
    pub maximum: f64,
    pub value: f64,
}

impl Default for ProgressBarSettings {
    fn default() -> Self {
        Self {
            minimum: 0.0,
            maximum: 100.0,
            value: 0.0,
        }
    }
}

impl ProgressBarSettings {
    /// All numbers and the range span must be finite; minimum < maximum.
    /// Values outside the inclusive range are rejected, never silently clamped.
    pub fn validate(&self) -> Result<(), ControlError> {
        validate_range(self.minimum, self.maximum, self.value)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ZoomControlSettings {
    pub minimum: f64,
    pub maximum: f64,
    pub value: f64,
    pub step: f64,
}

impl Default for ZoomControlSettings {
    fn default() -> Self {
        Self {
            minimum: 25.0,
            maximum: 400.0,
            value: 100.0,
            step: 10.0,
        }
    }
}

impl ZoomControlSettings {
    pub fn validate(&self) -> Result<(), ControlError> {
        validate_range(self.minimum, self.maximum, self.value)?;
        if !self.step.is_finite() || self.step <= 0.0 {
            return Err(ControlError::Invalid(
                "Zoom Step must be finite and positive".into(),
            ));
        }
        Ok(())
    }
}

fn validate_range(minimum: f64, maximum: f64, value: f64) -> Result<(), ControlError> {
    if !minimum.is_finite()
        || !maximum.is_finite()
        || minimum >= maximum
        || !(maximum - minimum).is_finite()
        || !value.is_finite()
        || !(minimum..=maximum).contains(&value)
    {
        return Err(ControlError::Invalid(
            "A range requires finite minimum < maximum, a finite span and Value within its bounds"
                .into(),
        ));
    }
    Ok(())
}

struct StatusState {
    parts: StatusBar,
    ribbon: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ZoomPart {
    Track,
    Decrease,
    Increase,
}

#[derive(Clone, Copy)]
struct ZoomGesture {
    id: WidgetId,
    part: ZoomPart,
}

#[derive(Default)]
pub(super) struct StatusControls {
    bars: HashMap<WidgetId, StatusState>,
    progress: HashMap<WidgetId, ProgressBarSettings>,
    zoom: HashMap<WidgetId, ZoomControlSettings>,
    gesture: Option<ZoomGesture>,
}

#[derive(Clone, Copy)]
struct ZoomGeometry {
    minus: Rect,
    track: Rect,
    plus: Rect,
    caption: Rect,
}

impl ZoomGeometry {
    fn new(bounds: Rect) -> Self {
        let caption_width = if bounds.width >= 140.0 { 44.0 } else { 0.0 };
        let content_width = (bounds.width - caption_width).max(0.0);
        let button_width = 24.0_f32.min(content_width / 3.0);
        let gap = 7.0_f32.min((content_width - button_width * 2.0).max(0.0) / 3.0);
        let plus = Rect::new(
            bounds.x + content_width - button_width,
            bounds.y,
            button_width,
            bounds.height,
        );
        Self {
            minus: Rect::new(bounds.x, bounds.y, button_width, bounds.height),
            track: Rect::new(
                bounds.x + button_width + gap,
                bounds.y + bounds.height / 2.0 - 2.0,
                (content_width - button_width * 2.0 - gap * 2.0).max(0.0),
                4.0,
            ),
            plus,
            caption: Rect::new(
                bounds.x + content_width,
                bounds.y,
                caption_width,
                bounds.height,
            ),
        }
    }

    fn part(self, point: Point) -> ZoomPart {
        if self.minus.contains(point.x, point.y) {
            ZoomPart::Decrease
        } else if self.plus.contains(point.x, point.y) {
            ZoomPart::Increase
        } else {
            ZoomPart::Track
        }
    }
}

impl Controls {
    pub fn status_bar(
        &mut self,
        parent: WidgetId,
        style: LayoutStyle,
    ) -> Result<StatusBar, ControlError> {
        self.add_status_bar(parent, style, false)
    }

    /// A status bar using Ribbon chrome and a slightly taller command row.
    /// It shares commands and state through ordinary child controls.
    pub fn ribbon_status_bar(
        &mut self,
        parent: WidgetId,
        style: LayoutStyle,
    ) -> Result<StatusBar, ControlError> {
        self.add_status_bar(parent, style, true)
    }

    fn add_status_bar(
        &mut self,
        parent: WidgetId,
        mut style: LayoutStyle,
        ribbon: bool,
    ) -> Result<StatusBar, ControlError> {
        style.kind = LayoutKind::Flex(Axis::Horizontal);
        if style.height == Length::Auto {
            style.height = Length::Px(if ribbon { 36.0 } else { 32.0 });
        }
        style.flex_shrink = 0.0;
        style.gap = 0.0;
        style.padding = Edges {
            left: 8.0,
            right: 8.0,
            top: 1.0,
            bottom: 0.0,
        };
        style.align = Align::Stretch;
        let container = self.transparent_panel(parent, style)?;
        self.set_role(container, Role::StatusBar);
        let left = self.transparent_panel(
            container,
            LayoutStyle {
                kind: LayoutKind::Flex(Axis::Horizontal),
                width: Length::Px(0.0),
                flex_grow: 1.0,
                align: Align::Center,
                gap: 4.0,
                ..Default::default()
            },
        )?;
        let right = self.transparent_panel(
            container,
            LayoutStyle {
                kind: LayoutKind::Flex(Axis::Horizontal),
                align: Align::Center,
                justify: Justify::End,
                gap: 4.0,
                ..Default::default()
            },
        )?;
        let parts = StatusBar {
            container,
            left,
            right,
        };
        self.status
            .bars
            .insert(container, StatusState { parts, ribbon });
        self.scene_dirty = true;
        Ok(parts)
    }

    pub fn progress_bar(
        &mut self,
        parent: WidgetId,
        settings: ProgressBarSettings,
        mut style: LayoutStyle,
    ) -> Result<WidgetId, ControlError> {
        settings.validate()?;
        if style.width == Length::Auto {
            style.width = Length::Px(120.0);
        }
        if style.height == Length::Auto {
            style.height = Length::Px(20.0);
        }
        let id = self.transparent_panel(parent, style)?;
        self.set_role(id, Role::ProgressBar);
        self.edit_props(id, |props| props.hit_test = false)?;
        self.status.progress.insert(id, settings);
        self.scene_dirty = true;
        Ok(id)
    }

    pub fn progress_settings(&self, id: WidgetId) -> Option<&ProgressBarSettings> {
        self.runtime.tree().node(id)?;
        self.status.progress.get(&id)
    }

    pub fn progress_value(&self, id: WidgetId) -> Option<f64> {
        self.progress_settings(id).map(|s| s.value)
    }

    pub fn set_progress_value(&mut self, id: WidgetId, value: f64) -> Result<(), ControlError> {
        let settings = *self
            .progress_settings(id)
            .ok_or_else(|| ControlError::Invalid("Control is not a ProgressBar".into()))?;
        validate_range(settings.minimum, settings.maximum, value)?;
        if settings.value != value {
            self.status.progress.get_mut(&id).unwrap().value = value;
            self.scene_dirty = true;
        }
        Ok(())
    }

    /// One focusable slider with pointer dragging, plus/minus buttons and a
    /// percentage caption. Arrows change Step; Home/End select range bounds.
    pub fn zoom_control(
        &mut self,
        parent: WidgetId,
        settings: ZoomControlSettings,
        mut style: LayoutStyle,
    ) -> Result<WidgetId, ControlError> {
        settings.validate()?;
        if style.width == Length::Auto {
            style.width = Length::Px(210.0);
        }
        if style.height == Length::Auto {
            style.height = Length::Px(28.0);
        }
        let id = self.transparent_panel(parent, style)?;
        self.set_role(id, Role::Slider);
        self.edit_props(id, |props| props.focusable = true)?;
        self.status.zoom.insert(id, settings);
        self.scene_dirty = true;
        Ok(id)
    }

    pub fn zoom_settings(&self, id: WidgetId) -> Option<&ZoomControlSettings> {
        self.runtime.tree().node(id)?;
        self.status.zoom.get(&id)
    }

    pub fn zoom_value(&self, id: WidgetId) -> Option<f64> {
        self.zoom_settings(id).map(|s| s.value)
    }

    /// Model updates terminate any pending gesture, so a late release cannot
    /// overwrite a new value. Invalid updates preserve both value and gesture.
    pub fn set_zoom_value(&mut self, id: WidgetId, value: f64) -> Result<(), ControlError> {
        let settings = *self
            .zoom_settings(id)
            .ok_or_else(|| ControlError::Invalid("Control is not a ZoomControl".into()))?;
        validate_range(settings.minimum, settings.maximum, value)?;
        if self.status.gesture.is_some_and(|g| g.id == id) {
            self.cancel_zoom_gesture()?;
        }
        self.change_zoom_value(id, value);
        Ok(())
    }

    fn change_zoom_value(&mut self, id: WidgetId, value: f64) {
        if let Some(settings) = self.status.zoom.get_mut(&id)
            && settings.value != value
        {
            settings.value = value;
            self.scene_dirty = true;
        }
    }

    pub(super) fn preflight_status_removal(&self, id: WidgetId) -> Result<(), ControlError> {
        if self
            .status
            .bars
            .values()
            .any(|bar| bar.parts.left == id || bar.parts.right == id)
        {
            return Err(ControlError::Invalid(
                "Required status bar regions cannot be removed alone".into(),
            ));
        }
        Ok(())
    }

    pub(super) fn retain_status_controls(&mut self) -> Result<(), ControlError> {
        self.status
            .bars
            .retain(|id, _| self.controls.contains_key(id));
        self.status
            .progress
            .retain(|id, _| self.controls.contains_key(id));
        self.status
            .zoom
            .retain(|id, _| self.controls.contains_key(id));
        self.sync_zoom_gesture()
    }

    pub(super) fn cancel_zoom_gesture(&mut self) -> Result<(), ControlError> {
        if let Some(gesture) = self.status.gesture.take() {
            if self.runtime.captured() == Some(gesture.id) {
                self.runtime.apply(Mutation::CapturePointer(None))?;
            }
            self.scene_dirty = true;
        }
        Ok(())
    }

    pub(super) fn sync_zoom_gesture(&mut self) -> Result<(), ControlError> {
        if self.status.gesture.is_some_and(|g| {
            !self.is_enabled(g.id)
                || self.runtime.captured() != Some(g.id)
                || self.runtime.focused() != Some(g.id)
                || !self.window_focused
                || self.zoom_settings(g.id).is_none()
        }) {
            self.cancel_zoom_gesture()?;
        }
        Ok(())
    }

    fn zoom_step(&mut self, id: WidgetId, increase: bool) {
        let s = self.status.zoom[&id];
        let value = if increase {
            (s.value + s.step).min(s.maximum)
        } else {
            (s.value - s.step).max(s.minimum)
        };
        self.change_zoom_value(id, value);
    }

    fn zoom_drag_to(&mut self, id: WidgetId, position: Point) {
        if !position.is_finite() {
            return;
        }
        let Some(bounds) = self.runtime.bounds(id) else {
            return;
        };
        let geometry = ZoomGeometry::new(bounds);
        if geometry.track.width <= 0.0 {
            return;
        }
        let fraction =
            ((position.x - geometry.track.x) / geometry.track.width).clamp(0.0, 1.0) as f64;
        let s = self.status.zoom[&id];
        let value = (s.minimum + fraction * (s.maximum - s.minimum)).clamp(s.minimum, s.maximum);
        self.change_zoom_value(id, value);
    }

    pub(super) fn status_before_input(&mut self, event: &InputEvent) -> Result<bool, ControlError> {
        self.sync_zoom_gesture()?;
        if !self.window_focused {
            return Ok(false);
        }
        if let Some(gesture) = self.status.gesture {
            match event {
                InputEvent::PointerMoved { position } => {
                    let allowed = self.route_zoom_event(event)?;
                    self.sync_zoom_gesture()?;
                    if allowed && self.status.gesture.is_some() && gesture.part == ZoomPart::Track {
                        self.zoom_drag_to(gesture.id, *position);
                    }
                    return Ok(true);
                }
                InputEvent::PointerUp {
                    position,
                    button: PointerButton::Primary,
                    ..
                } => {
                    let allowed = self.route_zoom_event(event)?;
                    if allowed
                        && self.is_enabled(gesture.id)
                        && self.runtime.focused() == Some(gesture.id)
                        && gesture.part != ZoomPart::Track
                        && self.runtime.hit_test(*position) == Some(gesture.id)
                        && self.runtime.bounds(gesture.id).is_some_and(|bounds| {
                            ZoomGeometry::new(bounds).part(*position) == gesture.part
                        })
                    {
                        self.zoom_step(gesture.id, gesture.part == ZoomPart::Increase);
                    }
                    self.cancel_zoom_gesture()?;
                    return Ok(true);
                }
                InputEvent::KeyDown {
                    key: Key::Escape, ..
                }
                | InputEvent::WindowFocused(false) => {
                    self.cancel_zoom_gesture()?;
                }
                _ => {}
            }
        }
        match event {
            InputEvent::PointerDown {
                position,
                button: PointerButton::Primary,
                ..
            } => {
                let Some(id) = self
                    .runtime
                    .hit_test(*position)
                    .filter(|id| self.status.zoom.contains_key(id) && self.is_enabled(*id))
                else {
                    return Ok(false);
                };
                let geometry = ZoomGeometry::new(self.runtime.bounds(id).unwrap());
                self.popup_pointer_down(*position)?;
                if !self.route_zoom_event(event)?
                    || !self.is_enabled(id)
                    || self.runtime.focused() != Some(id)
                {
                    return Ok(true);
                }
                let part = geometry.part(*position);
                // The percentage is display-only; clicking it focuses the slider.
                if !geometry.caption.contains(position.x, position.y) {
                    self.status.gesture = Some(ZoomGesture { id, part });
                    self.runtime.apply(Mutation::CapturePointer(Some(id)))?;
                    if part == ZoomPart::Track {
                        self.zoom_drag_to(id, *position);
                    }
                }
                self.scene_dirty = true;
                Ok(true)
            }
            InputEvent::KeyDown { key, modifiers, .. }
                if !modifiers.control && !modifiers.alt && !modifiers.super_key =>
            {
                let Some(id) = self
                    .runtime
                    .focused()
                    .filter(|id| self.status.zoom.contains_key(id) && self.is_enabled(*id))
                else {
                    return Ok(false);
                };
                if !matches!(
                    key,
                    Key::ArrowRight
                        | Key::ArrowUp
                        | Key::ArrowLeft
                        | Key::ArrowDown
                        | Key::Home
                        | Key::End
                ) {
                    return Ok(false);
                }
                if !self.route_zoom_event(event)?
                    || !self.is_enabled(id)
                    || self.runtime.focused() != Some(id)
                {
                    return Ok(true);
                }
                match key {
                    Key::ArrowRight | Key::ArrowUp => self.zoom_step(id, true),
                    Key::ArrowLeft | Key::ArrowDown => self.zoom_step(id, false),
                    Key::Home => self.change_zoom_value(id, self.status.zoom[&id].minimum),
                    Key::End => self.change_zoom_value(id, self.status.zoom[&id].maximum),
                    _ => return Ok(false),
                }
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    // Consumed events still take exactly one normal route. This updates pointer
    // and hover state, honors preview cancellation and applies callback commands.
    fn route_zoom_event(&mut self, event: &InputEvent) -> Result<bool, ControlError> {
        let report = self.runtime.dispatch(event.clone());
        if let Some(error) = report.errors.into_iter().next() {
            return Err(error.into());
        }
        self.synchronize_editor_focus()?;
        self.cancel_inactive_preedit();
        self.tooltip_after_input(event);
        self.scene_dirty = true;
        Ok(!report.default_prevented)
    }

    pub(super) fn paint_status_control(
        &mut self,
        id: WidgetId,
        bounds: Rect,
        clip: Rect,
    ) -> Result<bool, ControlError> {
        if self.paint_busy_progress(id, bounds, clip) {
            return Ok(true);
        }
        let is_status = self.status.bars.contains_key(&id)
            || self.status.progress.contains_key(&id)
            || self.status.zoom.contains_key(&id);
        if !is_status {
            return Ok(false);
        }
        // Runtime clip describes ancestors. Tiny standalone controls must also
        // constrain their own glyphs, track and thumb to the hit-test bounds.
        let Some(clip) = bounds.intersection(clip) else {
            return Ok(true);
        };
        let chrome = self.theme.chrome();
        let tokens = self.theme.tokens();
        if let Some(bar) = self.status.bars.get(&id) {
            fill_clipped(
                &mut self.scene,
                bounds,
                clip,
                if bar.ribbon {
                    chrome.ribbon_surface
                } else {
                    tokens.canvas
                },
            );
            fill_clipped(
                &mut self.scene,
                Rect::new(bounds.x, bounds.y, bounds.width, 1.0),
                clip,
                chrome.divider,
            );
            return Ok(true);
        }
        let accent = if self.is_enabled(id) {
            tokens.focus
        } else {
            chrome.disabled_text
        };
        if let Some(s) = self.status.progress.get(&id) {
            let height = 4.0_f32.min(bounds.height);
            let track = Rect::new(
                bounds.x,
                bounds.y + (bounds.height - height) / 2.0,
                bounds.width,
                height,
            );
            editor_fill(&mut self.scene, track, clip, height / 2.0, chrome.divider);
            let fraction = ((s.value - s.minimum) / (s.maximum - s.minimum)) as f32;
            if fraction > 0.0 {
                editor_fill(
                    &mut self.scene,
                    Rect::new(track.x, track.y, track.width * fraction, track.height),
                    clip,
                    height / 2.0,
                    accent,
                );
            }
            return Ok(true);
        }
        let Some(s) = self.status.zoom.get(&id).copied() else {
            return Ok(false);
        };
        let geometry = ZoomGeometry::new(bounds);
        let color = if self.is_enabled(id) {
            tokens.text
        } else {
            chrome.disabled_text
        };
        for (area, part, icon) in [
            (geometry.minus, ZoomPart::Decrease, IconSymbol::Remove),
            (geometry.plus, ZoomPart::Increase, IconSymbol::Add),
        ] {
            if self
                .status
                .gesture
                .is_some_and(|g| g.id == id && g.part == part)
            {
                editor_fill(&mut self.scene, area, clip, 4.0, chrome.subtle_pressed);
            }
            let size = 12.0_f32.min(area.width).min(area.height);
            crate::primitives::paint_icon(
                &mut self.scene,
                icon,
                Rect::new(
                    area.x + (area.width - size) / 2.0,
                    area.y + (area.height - size) / 2.0,
                    size,
                    size,
                ),
                clip,
                color,
            );
        }
        editor_fill(
            &mut self.scene,
            geometry.track,
            clip,
            2.0,
            chrome.control_bottom_border,
        );
        let fraction = ((s.value - s.minimum) / (s.maximum - s.minimum)) as f32;
        let active = Rect::new(
            geometry.track.x,
            geometry.track.y,
            geometry.track.width * fraction,
            geometry.track.height,
        );
        editor_fill(&mut self.scene, active, clip, 2.0, accent);
        let x = active.x + active.width;
        let thumb_size = 14.0_f32.min(bounds.height);
        let thumb = Rect::new(
            x - thumb_size / 2.0,
            bounds.y + (bounds.height - thumb_size) / 2.0,
            thumb_size,
            thumb_size,
        );
        editor_fill(
            &mut self.scene,
            thumb,
            clip,
            thumb_size / 2.0,
            chrome.card_surface,
        );
        shape_border(
            &mut self.scene,
            thumb,
            clip,
            thumb_size / 2.0,
            1.0,
            chrome.control_border,
        );
        let inner = Rect::new(
            thumb.x + 3.0,
            thumb.y + 3.0,
            (thumb.width - 6.0).max(0.0),
            (thumb.height - 6.0).max(0.0),
        );
        editor_fill(&mut self.scene, inner, clip, 4.0, accent);
        if geometry.caption.width > 0.0 {
            let mut run = TextRun {
                bounds: geometry.caption,
                clip: geometry
                    .caption
                    .intersection(clip)
                    .unwrap_or(Rect::new(0.0, 0.0, 0.0, 0.0)),
                text: format!("{:.0}%", s.value),
                font_size: 12.0,
                color,
                weight: 400,
                align: TextAlign::Center,
                wrap: false,
            };
            let prepared = self
                .engine
                .prepare(&run, 1.0)
                .map_err(|error| ControlError::Text(error.to_string()))?;
            run.bounds.y += ((bounds.height - prepared.height) / 2.0).max(0.0);
            self.scene.text(run);
        }
        if self.runtime.focused() == Some(id) && self.is_enabled(id) {
            shape_border(&mut self.scene, bounds, clip, 4.0, 1.0, tokens.focus);
        }
        Ok(true)
    }
}
