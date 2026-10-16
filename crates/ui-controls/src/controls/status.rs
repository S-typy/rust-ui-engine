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

/// Side occupied by a registered retained status section.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum StatusPlacement {
    #[default]
    Left,
    Right,
}

/// A fixed section never shrinks; stretch sections share remaining room after
/// every visible section's minimum has been reserved. Sections that do not fit
/// move to the retained overflow surface.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum StatusItemWidth {
    Fixed(f32),
    Stretch { minimum: f32 },
}

#[derive(Clone, Debug, PartialEq)]
pub struct StatusBarItemSettings {
    pub key: String,
    pub label: String,
    pub placement: StatusPlacement,
    pub width: StatusItemWidth,
    pub visible: bool,
    pub can_hide: bool,
}
impl StatusBarItemSettings {
    pub fn new(key: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            label: label.into(),
            placement: StatusPlacement::Left,
            width: StatusItemWidth::Stretch { minimum: 80.0 },
            visible: true,
            can_hide: true,
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StatusBarOptions {
    pub customizable: bool,
    pub resize_grip: bool,
}

/// An explicit request for the native host. Controls never resizes an OS window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowResizeDirection {
    BottomRight,
}

#[derive(Clone)]
struct StatusItem {
    host: WidgetId,
    settings: StatusBarItemSettings,
    checkbox: WidgetId,
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

#[derive(Clone)]
struct StatusState {
    parts: StatusBar,
    ribbon: bool,
    automatic_height: Option<f32>,
    options: StatusBarOptions,
    items: Vec<StatusItem>,
    overflow: WidgetId,
    overflow_flyout: Flyout,
    overflow_viewer: ScrollViewer,
    customize: WidgetId,
    customize_flyout: Flyout,
    customize_viewer: ScrollViewer,
    grip: WidgetId,
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
    resize_request: Option<WindowResizeDirection>,
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
        let automatic_height = (style.height == Length::Auto)
            .then(|| self.theme.tokens().control_height - if ribbon { 0.0 } else { 4.0 });
        if let Some(height) = automatic_height {
            style.height = Length::Px(height);
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
        let (overflow, overflow_flyout, overflow_viewer) =
            self.status_disclosure(container, "More status sections", IconSymbol::ChevronDown)?;
        let (customize, customize_flyout, customize_viewer) = self.status_disclosure(
            container,
            "Customize status sections",
            IconSymbol::ChevronDown,
        )?;
        let grip = self.button(
            container,
            "Resize window",
            None,
            LayoutStyle {
                width: Length::Px(20.0),
                height: Length::Percent(1.0),
                flex_shrink: 0.0,
                ..Default::default()
            },
        )?;
        self.set_button_chrome(grip, ButtonChrome::Flat)?;
        self.set_button_content_layout(grip, ButtonContentLayout::IconOnly)?;
        self.edit_props(grip, |p| {
            p.visible = false;
            p.collapsed = true;
        })?;
        self.status.bars.insert(
            container,
            StatusState {
                parts,
                ribbon,
                automatic_height,
                options: StatusBarOptions::default(),
                items: Vec::new(),
                overflow,
                overflow_flyout,
                overflow_viewer,
                customize,
                customize_flyout,
                customize_viewer,
                grip,
            },
        );
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
        if self.status.bars.values().any(|bar| {
            bar.parts.left == id
                || bar.parts.right == id
                || bar.overflow == id
                || bar.customize == id
                || bar.grip == id
        }) {
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
        if let InputEvent::PointerDown {
            position,
            button: PointerButton::Primary,
            ..
        } = event
            && let Some(grip) = self.runtime.hit_test(*position).filter(|id| {
                self.status
                    .bars
                    .values()
                    .any(|bar| bar.grip == *id && bar.options.resize_grip)
                    && self.is_enabled(*id)
            })
        {
            if self.route_zoom_event(event)? && self.is_enabled(grip) {
                self.status.resize_request = Some(WindowResizeDirection::BottomRight);
            }
            return Ok(true);
        }
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
        if self.status.bars.values().any(|bar| bar.grip == id) {
            let chrome = self.theme.chrome();
            let color = if self.is_enabled(id) {
                chrome.control_bottom_border
            } else {
                chrome.disabled_text
            };
            for row in 0..3 {
                for col in 0..=row {
                    fill_clipped(
                        &mut self.scene,
                        Rect::new(
                            bounds.x + bounds.width - 5.0 - col as f32 * 4.0,
                            bounds.y + bounds.height - 5.0 - (2 - row) as f32 * 4.0,
                            2.0,
                            2.0,
                        ),
                        clip,
                        color,
                    );
                }
            }
            if self.runtime.focused() == Some(id) {
                shape_border(
                    &mut self.scene,
                    bounds,
                    clip,
                    3.0,
                    1.0,
                    self.theme.tokens().focus,
                );
            }
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

impl Controls {
    fn status_disclosure(
        &mut self,
        parent: WidgetId,
        label: &str,
        icon: IconSymbol,
    ) -> Result<(WidgetId, Flyout, ScrollViewer), ControlError> {
        let button = self.button(
            parent,
            label,
            None,
            LayoutStyle {
                width: Length::Px(28.0),
                height: Length::Percent(1.0),
                flex_shrink: 0.0,
                ..Default::default()
            },
        )?;
        self.set_button_icon(button, Some(ButtonIcon::new(icon)))?;
        self.set_button_content_layout(button, ButtonContentLayout::IconOnly)?;
        self.set_button_chrome(button, ButtonChrome::Flat)?;
        self.edit_props(button, |p| {
            p.visible = false;
            p.collapsed = true;
        })?;
        let flyout = self.flyout(button, Size::new(300.0, 240.0))?;
        self.set_button_flyout(button, flyout)?;
        let viewer = self.scroll_view(
            flyout.content,
            LayoutStyle {
                width: Length::Percent(1.0),
                height: Length::Percent(1.0),
                ..Default::default()
            },
            240.0,
        )?;
        Ok((button, flyout, viewer))
    }

    /// Opt-in settings for a retained status bar. A resize grip emits an explicit
    /// native resize request; the application host is responsible for handling it.
    pub fn configure_status_bar(
        &mut self,
        bar: StatusBar,
        options: StatusBarOptions,
    ) -> Result<(), ControlError> {
        let state = self
            .status
            .bars
            .get_mut(&bar.container)
            .filter(|s| s.parts == bar)
            .ok_or_else(|| ControlError::Invalid("Unknown status bar".into()))?;
        state.options = options;
        self.scene_dirty = true;
        Ok(())
    }

    /// Creates a retained content host registered for overflow and customization.
    pub fn status_bar_item(
        &mut self,
        bar: StatusBar,
        settings: StatusBarItemSettings,
    ) -> Result<WidgetId, ControlError> {
        self.validate_status_item(bar, &settings, None)?;
        let parent = if settings.placement == StatusPlacement::Left {
            bar.left
        } else {
            bar.right
        };
        let host = self.transparent_panel(
            parent,
            LayoutStyle {
                height: Length::Percent(1.0),
                ..Default::default()
            },
        )?;
        self.register_status_bar_item(bar, host, settings)?;
        Ok(host)
    }

    fn validate_status_item(
        &self,
        bar: StatusBar,
        settings: &StatusBarItemSettings,
        host: Option<WidgetId>,
    ) -> Result<(), ControlError> {
        let state = self
            .status
            .bars
            .get(&bar.container)
            .filter(|s| s.parts == bar)
            .ok_or_else(|| ControlError::Invalid("Unknown status bar".into()))?;
        let width = match settings.width {
            StatusItemWidth::Fixed(width) | StatusItemWidth::Stretch { minimum: width } => width,
        };
        if settings.key.trim().is_empty()
            || settings.key.len() > 256
            || settings.label.trim().is_empty()
            || settings.label.len() > 4096
            || !width.is_finite()
            || !(1.0..=16_384.0).contains(&width)
            || (!settings.can_hide && !settings.visible)
            || state.items.len() >= 256
            || state
                .items
                .iter()
                .any(|item| item.settings.key == settings.key || Some(item.host) == host)
        {
            return Err(ControlError::Invalid("Status sections require unique keys, labels, finite widths, and at most 256 entries; required sections remain visible".into()));
        }
        Ok(())
    }

    /// Registers an existing direct left/right host without recreating content.
    /// A stable key identifies the section in visibility customization.
    pub fn register_status_bar_item(
        &mut self,
        bar: StatusBar,
        host: WidgetId,
        settings: StatusBarItemSettings,
    ) -> Result<(), ControlError> {
        self.validate_status_item(bar, &settings, Some(host))?;
        if !self.controls.contains_key(&host)
            || !matches!(self.runtime.tree().parent(host),Some(parent) if parent==bar.left || parent==bar.right)
            || self
                .status
                .bars
                .values()
                .any(|state| state.items.iter().any(|item| item.host == host))
        {
            return Err(ControlError::Invalid(
                "Status section must be an unregistered direct child of a status host".into(),
            ));
        }
        let parent = self.status.bars[&bar.container].customize_viewer.content;
        let checkbox = self.checkbox(
            parent,
            settings.label.clone(),
            settings.visible,
            LayoutStyle {
                width: Length::Percent(1.0),
                height: Length::Px(self.theme.tokens().control_height),
                ..Default::default()
            },
        )?;
        self.set_enabled(checkbox, settings.can_hide)?;
        self.status
            .bars
            .get_mut(&bar.container)
            .unwrap()
            .items
            .push(StatusItem {
                host,
                settings,
                checkbox,
            });
        self.scene_dirty = true;
        Ok(())
    }

    pub fn status_item_visible(&self, bar: StatusBar, key: &str) -> Option<bool> {
        self.status
            .bars
            .get(&bar.container)
            .filter(|s| s.parts == bar)?
            .items
            .iter()
            .find(|i| i.settings.key == key)
            .map(|i| i.settings.visible)
    }

    /// Visibility changes preserve the content and its bindings. A hidden editor
    /// loses focus/preedit, without silently committing its current draft.
    pub fn set_status_item_visible(
        &mut self,
        bar: StatusBar,
        key: &str,
        visible: bool,
    ) -> Result<(), ControlError> {
        let state = self
            .status
            .bars
            .get_mut(&bar.container)
            .filter(|s| s.parts == bar)
            .ok_or_else(|| ControlError::Invalid("Unknown status bar".into()))?;
        let item = state
            .items
            .iter_mut()
            .find(|i| i.settings.key == key)
            .ok_or_else(|| ControlError::Invalid("Unknown status section key".into()))?;
        if !visible && !item.settings.can_hide {
            return Err(ControlError::Invalid(
                "Required status section cannot be hidden".into(),
            ));
        }
        item.settings.visible = visible;
        let (host, checkbox) = (item.host, item.checkbox);
        self.set_checked(checkbox, visible)?;
        self.edit_props(host, |p| {
            p.visible = visible;
            p.collapsed = !visible;
        })?;
        self.sync_popup_availability()?;
        self.synchronize_editor_focus()?;
        self.cancel_inactive_preedit();
        self.scene_dirty = true;
        Ok(())
    }

    pub fn status_bar_overflow_button(&self, bar: StatusBar) -> Option<WidgetId> {
        Some(
            self.status
                .bars
                .get(&bar.container)
                .filter(|s| s.parts == bar)?
                .overflow,
        )
    }
    pub fn status_bar_customize_button(&self, bar: StatusBar) -> Option<WidgetId> {
        Some(
            self.status
                .bars
                .get(&bar.container)
                .filter(|s| s.parts == bar)?
                .customize,
        )
    }
    pub fn status_bar_resize_grip(&self, bar: StatusBar) -> Option<WidgetId> {
        Some(
            self.status
                .bars
                .get(&bar.container)
                .filter(|s| s.parts == bar)?
                .grip,
        )
    }

    /// Consumes one coalesced request generated by the grip's pointer/activation.
    /// A native host should start its OS resize operation once and clear it.
    pub fn take_window_resize_request(&mut self) -> Option<WindowResizeDirection> {
        self.status.resize_request.take()
    }
    pub(super) fn activate_status_grip(&mut self, id: WidgetId) -> Result<bool, ControlError> {
        if self
            .status
            .bars
            .values()
            .any(|bar| bar.grip == id && bar.options.resize_grip)
        {
            self.status.resize_request = Some(WindowResizeDirection::BottomRight);
            self.activated.push(id);
            return Ok(true);
        }
        Ok(false)
    }

    pub(super) fn sync_status_bars(&mut self) -> Result<(), ControlError> {
        let owners: Vec<_> = self.status.bars.keys().copied().collect();
        for owner in owners {
            let removed: Vec<_> = self.status.bars[&owner]
                .items
                .iter()
                .filter(|item| !self.runtime.tree().contains(item.host))
                .map(|item| item.checkbox)
                .collect();
            self.status
                .bars
                .get_mut(&owner)
                .unwrap()
                .items
                .retain(|item| self.runtime.tree().contains(item.host));
            for checkbox in removed {
                if self.runtime.tree().contains(checkbox) {
                    self.remove(checkbox)?;
                }
            }
            let changes: Vec<_> = self.status.bars[&owner]
                .items
                .iter()
                .filter_map(|item| {
                    self.checked(item.checkbox)
                        .filter(|checked| *checked != item.settings.visible)
                        .map(|checked| (item.settings.key.clone(), checked))
                })
                .collect();
            let bar = self.status.bars[&owner].parts;
            for (key, visible) in changes {
                self.set_status_item_visible(bar, &key, visible)?;
            }
            let state = self.status.bars[&owner].clone();
            if let Some(last) = state.automatic_height {
                let current = self.runtime.tree().node(owner).unwrap().props.style.height;
                if current == Length::Px(last) || current == Length::Auto {
                    let height = if state.items.is_empty() {
                        self.theme.tokens().control_height - if state.ribbon { 0.0 } else { 4.0 }
                    } else {
                        // Registered hosts accept ordinary controls with their
                        // complete border and focus geometry.
                        self.theme.tokens().control_height + 2.0
                    };
                    self.edit_props(owner, |p| p.style.height = Length::Px(height))?;
                    self.status.bars.get_mut(&owner).unwrap().automatic_height = Some(height);
                } else {
                    self.status.bars.get_mut(&owner).unwrap().automatic_height = None;
                }
            }
            let Some(bounds) = self.runtime.bounds(owner) else {
                continue;
            };
            self.edit_props(state.customize, |p| {
                p.visible = state.options.customizable;
                p.collapsed = !state.options.customizable;
            })?;
            self.edit_props(state.grip, |p| {
                p.visible = state.options.resize_grip;
                p.collapsed = !state.options.resize_grip;
            })?;
            if !state.options.customizable && self.flyout_is_open(state.customize_flyout) {
                self.close_flyout(state.customize_flyout)?;
            }
            if state.items.is_empty() {
                continue;
            }
            let height = self.theme.tokens().control_height;
            let widths: Vec<_> = state
                .items
                .iter()
                .map(|item| match item.settings.width {
                    StatusItemWidth::Fixed(w) | StatusItemWidth::Stretch { minimum: w } => w,
                })
                .collect();
            let total: f32 = state
                .items
                .iter()
                .zip(&widths)
                .filter(|(item, _)| item.settings.visible)
                .map(|(_, w)| *w + 4.0)
                .sum();
            let available = (bounds.width
                - 16.0
                - if state.options.customizable {
                    28.0
                } else {
                    0.0
                }
                - if state.options.resize_grip { 20.0 } else { 0.0 })
            .max(0.0);
            let overflowed = total > available;
            let available = (available - if overflowed { 28.0 } else { 0.0 }).max(0.0);
            let mut fit = vec![false; state.items.len()];
            let mut used = 0.0;
            // Right-side sections retain priority when space is scarce.
            for placement in [StatusPlacement::Right, StatusPlacement::Left] {
                for (index, item) in state.items.iter().enumerate() {
                    if item.settings.visible
                        && item.settings.placement == placement
                        && used + widths[index] + 4.0 <= available
                    {
                        fit[index] = true;
                        used += widths[index] + 4.0;
                    }
                }
            }
            let stretches = state
                .items
                .iter()
                .enumerate()
                .filter(|(index, item)| {
                    fit[*index] && matches!(item.settings.width, StatusItemWidth::Stretch { .. })
                })
                .count();
            let share = if stretches > 0 {
                (available - used) / stretches as f32
            } else {
                0.0
            };
            let rearrange = state.items.iter().enumerate().any(|(index, item)| {
                let target = if item.settings.visible && !fit[index] {
                    state.overflow_viewer.content
                } else if item.settings.placement == StatusPlacement::Left {
                    bar.left
                } else {
                    bar.right
                };
                self.runtime.tree().parent(item.host) != Some(target)
            });
            let mut overflow_count = 0;
            let mut mutations = Vec::new();
            for (index, item) in state.items.iter().enumerate() {
                let in_overflow = item.settings.visible && !fit[index];
                if in_overflow {
                    overflow_count += 1;
                }
                let target = if in_overflow {
                    state.overflow_viewer.content
                } else if item.settings.placement == StatusPlacement::Left {
                    bar.left
                } else {
                    bar.right
                };
                if rearrange {
                    mutations.push(Mutation::Reparent {
                        node: item.host,
                        parent: target,
                    });
                }
                let mut props = self.runtime.tree().node(item.host).unwrap().props.clone();
                props.visible = item.settings.visible;
                props.collapsed = !item.settings.visible;
                props.style.width = if in_overflow {
                    Length::Percent(1.0)
                } else {
                    Length::Px(
                        widths[index]
                            + if matches!(item.settings.width, StatusItemWidth::Stretch { .. }) {
                                share
                            } else {
                                0.0
                            },
                    )
                };
                props.style.height = Length::Px(if in_overflow {
                    height
                } else {
                    (bounds.height - 1.0).max(0.0)
                });
                props.style.flex_grow = 0.0;
                props.style.flex_shrink = 0.0;
                mutations.push(Mutation::SetProps {
                    node: item.host,
                    props,
                });
                let mut check = self
                    .runtime
                    .tree()
                    .node(item.checkbox)
                    .unwrap()
                    .props
                    .clone();
                check.style.height = Length::Px(height);
                mutations.push(Mutation::SetProps {
                    node: item.checkbox,
                    props: check,
                });
            }
            self.runtime.apply(Mutation::Batch(mutations))?;
            self.edit_props(state.overflow, |p| {
                p.visible = overflow_count > 0;
                p.collapsed = overflow_count == 0;
            })?;
            self.set_scroll_extent(
                state.overflow_viewer.viewport,
                (overflow_count as f32 * (height + 6.0) + 12.0).max(1.0),
            )?;
            self.set_flyout_placement(
                state.overflow_flyout,
                Size::new(
                    (widths.iter().copied().fold(260.0, f32::max) + 28.0).min(16_384.0),
                    (overflow_count as f32 * (height + 6.0) + 28.0).max(48.0),
                ),
                FlyoutPlacement::Below,
            )?;
            self.set_scroll_extent(
                state.customize_viewer.viewport,
                state.items.len() as f32 * (height + 6.0) + 12.0,
            )?;
            self.set_flyout_placement(
                state.customize_flyout,
                Size::new(
                    320.0,
                    (state.items.len() as f32 * (height + 6.0) + 28.0).max(48.0),
                ),
                FlyoutPlacement::Below,
            )?;
            if overflow_count == 0 && self.flyout_is_open(state.overflow_flyout) {
                self.close_flyout(state.overflow_flyout)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod section_tests {
    use super::*;
    use rust_desktop_ui_core::Modifiers;
    fn frame(ui: &mut Controls, width: f32) {
        ui.update(Size::new(width, 300.0)).unwrap();
    }
    fn item(
        key: &str,
        width: StatusItemWidth,
        placement: StatusPlacement,
    ) -> StatusBarItemSettings {
        StatusBarItemSettings {
            width,
            placement,
            ..StatusBarItemSettings::new(key, key)
        }
    }
    #[test]
    fn overflow_retains_editor_and_live_values_then_restores_hosts_on_resize() {
        let mut ui = Controls::new(Theme::FluentLight).unwrap();
        let bar = ui.status_bar(ui.root(), LayoutStyle::default()).unwrap();
        let left = ui
            .status_bar_item(
                bar,
                item(
                    "Message",
                    StatusItemWidth::Stretch { minimum: 140.0 },
                    StatusPlacement::Left,
                ),
            )
            .unwrap();
        let right = ui
            .status_bar_item(
                bar,
                item(
                    "Zoom",
                    StatusItemWidth::Fixed(180.0),
                    StatusPlacement::Right,
                ),
            )
            .unwrap();
        let editor = ui
            .edit_box(
                left,
                "Message",
                "kept",
                LayoutStyle {
                    height: Length::Percent(1.0),
                    ..Default::default()
                },
            )
            .unwrap();
        let zoom = ui
            .zoom_control(
                right,
                ZoomControlSettings::default(),
                LayoutStyle {
                    height: Length::Percent(1.0),
                    ..Default::default()
                },
            )
            .unwrap();
        frame(&mut ui, 700.0);
        assert_eq!(ui.runtime.tree().parent(left), Some(bar.left));
        ui.runtime.apply(Mutation::Focus(Some(editor))).unwrap();
        ui.text_input(" draft").unwrap();
        let draft = ui.document(editor).unwrap().text().to_owned();
        frame(&mut ui, 260.0);
        assert_ne!(ui.runtime.tree().parent(left), Some(bar.left));
        assert_eq!(ui.runtime.tree().parent(right), Some(bar.right));
        ui.set_zoom_value(zoom, 175.0).unwrap();
        let overflow = ui.status_bar_overflow_button(bar).unwrap();
        ui.activate(overflow).unwrap();
        frame(&mut ui, 260.0);
        assert!(ui.is_enabled(editor));
        assert_eq!(ui.document(editor).unwrap().text(), draft);
        frame(&mut ui, 700.0);
        assert_eq!(ui.runtime.tree().parent(left), Some(bar.left));
        assert_eq!(ui.zoom_value(zoom), Some(175.0));
        assert_eq!(ui.document(editor).unwrap().text(), draft);
        assert!(!ui.flyout_is_open(ui.status.bars[&bar.container].overflow_flyout));
    }
    #[test]
    fn customization_is_keyboard_accessible_hides_without_destroying_and_rejects_required() {
        let mut ui = Controls::new(Theme::FluentDark).unwrap();
        let bar = ui.status_bar(ui.root(), LayoutStyle::default()).unwrap();
        ui.configure_status_bar(
            bar,
            StatusBarOptions {
                customizable: true,
                resize_grip: false,
            },
        )
        .unwrap();
        let host = ui
            .status_bar_item(
                bar,
                item(
                    "Message",
                    StatusItemWidth::Fixed(160.0),
                    StatusPlacement::Left,
                ),
            )
            .unwrap();
        let child = ui.label(host, "Live text", LayoutStyle::default()).unwrap();
        let required = StatusBarItemSettings {
            can_hide: false,
            ..item(
                "Required",
                StatusItemWidth::Fixed(100.0),
                StatusPlacement::Right,
            )
        };
        ui.status_bar_item(bar, required).unwrap();
        frame(&mut ui, 600.0);
        ui.activate(ui.status_bar_customize_button(bar).unwrap())
            .unwrap();
        frame(&mut ui, 600.0);
        let checkbox = ui.status.bars[&bar.container].items[0].checkbox;
        assert!(
            ui.semantics()
                .iter()
                .any(|n| n.id == checkbox && n.role == Role::CheckBox)
        );
        ui.runtime.apply(Mutation::Focus(Some(checkbox))).unwrap();
        ui.dispatch(InputEvent::KeyDown {
            key: Key::Enter,
            modifiers: Modifiers::default(),
            repeat: false,
        })
        .unwrap();
        frame(&mut ui, 600.0);
        assert_eq!(ui.status_item_visible(bar, "Message"), Some(false));
        assert!(ui.runtime.bounds(child).is_none());
        assert!(ui.runtime.tree().contains(child));
        ui.set_label(child, "Updated while hidden").unwrap();
        ui.set_status_item_visible(bar, "Message", true).unwrap();
        frame(&mut ui, 600.0);
        assert!(ui.runtime.bounds(child).is_some());
        assert!(ui.set_status_item_visible(bar, "Required", false).is_err());
        assert_eq!(ui.status_item_visible(bar, "Required"), Some(true));
    }
    #[test]
    fn grip_emits_one_native_request_from_pointer_and_accessible_activation() {
        let mut ui = Controls::new(Theme::FluentLight).unwrap();
        let bar = ui
            .ribbon_status_bar(ui.root(), LayoutStyle::default())
            .unwrap();
        ui.configure_status_bar(
            bar,
            StatusBarOptions {
                customizable: false,
                resize_grip: true,
            },
        )
        .unwrap();
        frame(&mut ui, 640.0);
        let grip = ui.status_bar_resize_grip(bar).unwrap();
        let r = ui.runtime.bounds(grip).unwrap();
        assert!(ui.semantics().iter().any(|node| node.id == grip
            && node.role == Role::Button
            && node.label == "Resize window"));
        ui.dispatch(InputEvent::PointerDown {
            position: Point::new(r.x + r.width / 2.0, r.y + r.height / 2.0),
            button: PointerButton::Primary,
            modifiers: Modifiers::default(),
        })
        .unwrap();
        assert_eq!(
            ui.take_window_resize_request(),
            Some(WindowResizeDirection::BottomRight)
        );
        assert_eq!(ui.take_window_resize_request(), None);
        ui.perform_action(grip, SemanticAction::Activate, None)
            .unwrap();
        assert_eq!(
            ui.take_window_resize_request(),
            Some(WindowResizeDirection::BottomRight)
        );
        ui.configure_status_bar(bar, StatusBarOptions::default())
            .unwrap();
        frame(&mut ui, 640.0);
        ui.activate(grip).unwrap();
        assert_eq!(ui.take_window_resize_request(), None);
    }
    #[test]
    fn status_registration_validation_is_atomic_and_explicit_height_survives_density() {
        let mut ui = Controls::new(Theme::FluentLight).unwrap();
        let bar = ui
            .status_bar(
                ui.root(),
                LayoutStyle {
                    height: Length::Px(52.0),
                    ..Default::default()
                },
            )
            .unwrap();
        ui.status_bar_item(
            bar,
            item("One", StatusItemWidth::Fixed(90.0), StatusPlacement::Left),
        )
        .unwrap();
        let ids = ui.runtime.tree().preorder();
        assert!(
            ui.status_bar_item(
                bar,
                item("One", StatusItemWidth::Fixed(90.0), StatusPlacement::Left)
            )
            .is_err()
        );
        assert!(
            ui.status_bar_item(
                bar,
                item(
                    "Bad",
                    StatusItemWidth::Fixed(f32::NAN),
                    StatusPlacement::Left
                )
            )
            .is_err()
        );
        assert_eq!(ui.runtime.tree().preorder(), ids);
        ui.set_theme(Theme::FluentDark).unwrap();
        frame(&mut ui, 640.0);
        assert_eq!(ui.runtime.bounds(bar.container).unwrap().height, 52.0);
    }
}
#[cfg(test)]
mod retained_edge_tests {
    use super::*;
    use crate::{ThemeConfig, ThemeDensity, ThemeMode};
    #[test]
    fn registered_status_height_contains_full_children_after_density_changes_and_idle_settles() {
        let mut ui = Controls::new(Theme::FluentLight).unwrap();
        let bar = ui.status_bar(ui.root(), LayoutStyle::default()).unwrap();
        let host = ui
            .status_bar_item(
                bar,
                StatusBarItemSettings {
                    width: StatusItemWidth::Fixed(180.0),
                    ..StatusBarItemSettings::new("command", "Command")
                },
            )
            .unwrap();
        let button = ui
            .button(host, "Action", None, LayoutStyle::default())
            .unwrap();
        for density in [
            ThemeDensity::Comfortable,
            ThemeDensity::Touch,
            ThemeDensity::Compact,
            ThemeDensity::Touch,
        ] {
            ui.set_theme(Theme::Configured(
                ThemeConfig::new(ThemeMode::Light).with_density(density),
            ))
            .unwrap();
            let height = ui.theme.tokens().control_height;
            ui.edit_props(button, |p| p.style.height = Length::Px(height))
                .unwrap();
            ui.update(Size::new(600.0, 200.0)).unwrap();
            let bounds = ui.runtime.bounds(button).unwrap();
            let clip = ui.runtime.clip(button).unwrap();
            let visible = bounds.intersection(clip).unwrap();
            assert!(
                (bounds.width - visible.width).abs() < 0.01
                    && (bounds.height - visible.height).abs() < 0.01,
                "{bounds:?} {clip:?}"
            );
            assert_eq!(
                ui.runtime.bounds(bar.container).unwrap().height,
                height + 2.0
            );
            let idle = ui.update(Size::new(600.0, 200.0)).unwrap();
            assert_eq!(idle.layout_passes, 0);
            assert_eq!(idle.paint_passes, 0);
        }
    }
    #[test]
    fn status_sections_restore_source_order_and_removal_does_not_leave_overflow_orphans() {
        let mut ui = Controls::new(Theme::FluentLight).unwrap();
        let bar = ui.status_bar(ui.root(), LayoutStyle::default()).unwrap();
        let mut hosts = Vec::new();
        for key in ["First", "Second", "Third"] {
            hosts.push(
                ui.status_bar_item(
                    bar,
                    StatusBarItemSettings {
                        placement: StatusPlacement::Right,
                        width: StatusItemWidth::Fixed(90.0),
                        ..StatusBarItemSettings::new(key, key)
                    },
                )
                .unwrap(),
            );
        }
        ui.update(Size::new(500.0, 200.0)).unwrap();
        ui.update(Size::new(150.0, 200.0)).unwrap();
        ui.update(Size::new(500.0, 200.0)).unwrap();
        assert_eq!(ui.runtime.tree().children(bar.right).unwrap(), hosts);
        ui.update(Size::new(100.0, 200.0)).unwrap();
        let overflow = ui.status_bar_overflow_button(bar).unwrap();
        ui.activate(overflow).unwrap();
        ui.update(Size::new(100.0, 200.0)).unwrap();
        ui.remove(bar.container).unwrap();
        assert!(hosts.iter().all(|host| !ui.runtime.tree().contains(*host)));
        assert!(ui.status.bars.is_empty());
    }
}
