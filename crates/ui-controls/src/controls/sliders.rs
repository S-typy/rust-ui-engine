//! Scalar and two-thumb numeric sliders with retained input and accessibility.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SliderSettings {
    pub minimum: f64,
    pub maximum: f64,
    pub value: f64,
    pub step: f64,
    pub axis: Axis,
    pub reversed: bool,
    pub snap_to_ticks: bool,
    pub tick_frequency: Option<f64>,
}
impl Default for SliderSettings {
    fn default() -> Self {
        Self {
            minimum: 0.0,
            maximum: 100.0,
            value: 0.0,
            step: 1.0,
            axis: Axis::Horizontal,
            reversed: false,
            snap_to_ticks: false,
            tick_frequency: None,
        }
    }
}
impl SliderSettings {
    pub fn validate(&self) -> Result<(), ControlError> {
        if ![self.minimum, self.maximum, self.value, self.step]
            .into_iter()
            .all(f64::is_finite)
            || self.minimum >= self.maximum
            || !(self.maximum - self.minimum).is_finite()
            || !(self.minimum..=self.maximum).contains(&self.value)
            || self.step <= 0.0
            || self.tick_frequency.is_some_and(|v| {
                !v.is_finite() || v <= 0.0 || !((self.maximum - self.minimum) / v).is_finite()
            })
        {
            return Err(ControlError::Invalid("Slider requires a finite increasing range, contained value and positive step/tick frequency".into()));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ThumbCrossing {
    #[default]
    Clamp,
    Swap,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RangeSliderSettings {
    pub minimum: f64,
    pub maximum: f64,
    pub lower: f64,
    pub upper: f64,
    pub step: f64,
    pub axis: Axis,
    pub reversed: bool,
    pub snap_to_ticks: bool,
    pub tick_frequency: Option<f64>,
    pub crossing: ThumbCrossing,
}
impl Default for RangeSliderSettings {
    fn default() -> Self {
        Self {
            minimum: 0.0,
            maximum: 100.0,
            lower: 25.0,
            upper: 75.0,
            step: 1.0,
            axis: Axis::Horizontal,
            reversed: false,
            snap_to_ticks: false,
            tick_frequency: None,
            crossing: ThumbCrossing::Clamp,
        }
    }
}
impl RangeSliderSettings {
    fn scalar(self) -> SliderSettings {
        SliderSettings {
            minimum: self.minimum,
            maximum: self.maximum,
            value: self.lower,
            step: self.step,
            axis: self.axis,
            reversed: self.reversed,
            snap_to_ticks: self.snap_to_ticks,
            tick_frequency: self.tick_frequency,
        }
    }
    pub fn validate(&self) -> Result<(), ControlError> {
        self.scalar().validate()?;
        if !self.upper.is_finite() || self.upper < self.lower || self.upper > self.maximum {
            return Err(ControlError::Invalid(
                "RangeSlider requires minimum <= lower <= upper <= maximum".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Copy)]
struct SliderState {
    settings: SliderSettings,
    range: Option<RangeState>,
    owned_width: Option<f32>,
    owned_height: Option<f32>,
}
#[derive(Clone, Copy)]
struct RangeState {
    upper: f64,
    crossing: ThumbCrossing,
    lower_thumb: WidgetId,
    upper_thumb: WidgetId,
}
#[derive(Clone, Copy)]
struct SliderDrag {
    owner: WidgetId,
    upper: bool,
    lower_value: f64,
    upper_value: f64,
}
#[derive(Default)]
pub(super) struct Sliders {
    items: HashMap<WidgetId, SliderState>,
    drag: Option<SliderDrag>,
}

fn slider_ratio(settings: SliderSettings, value: f64) -> f32 {
    let ratio =
        ((value - settings.minimum) / (settings.maximum - settings.minimum)).clamp(0.0, 1.0) as f32;
    if settings.reversed {
        1.0 - ratio
    } else {
        ratio
    }
}
fn snapped(settings: SliderSettings, value: f64) -> f64 {
    let value = value.clamp(settings.minimum, settings.maximum);
    if value == settings.minimum || value == settings.maximum {
        return value;
    }
    let step = interaction_step(settings);
    let ticks = (value - settings.minimum) / step;
    if !ticks.is_finite() {
        return value;
    }
    (settings.minimum + ticks.round() * step).clamp(settings.minimum, settings.maximum)
}
fn interaction_step(settings: SliderSettings) -> f64 {
    if settings.snap_to_ticks {
        settings.tick_frequency.unwrap_or(settings.step)
    } else {
        settings.step
    }
}
fn thumb_size(theme: Theme) -> f32 {
    if theme.tokens().control_height >= 44.0 {
        24.0
    } else {
        20.0
    }
}
fn thumb_rect(bounds: Rect, settings: SliderSettings, value: f64, diameter: f32) -> Rect {
    let ratio = slider_ratio(settings, value);
    match settings.axis {
        Axis::Horizontal => Rect::new(
            bounds.x + (bounds.width - diameter).max(0.0) * ratio,
            bounds.y + (bounds.height - diameter) * 0.5,
            diameter.min(bounds.width),
            diameter.min(bounds.height),
        ),
        Axis::Vertical => Rect::new(
            bounds.x + (bounds.width - diameter) * 0.5,
            bounds.y + (bounds.height - diameter).max(0.0) * (1.0 - ratio),
            diameter.min(bounds.width),
            diameter.min(bounds.height),
        ),
    }
}

impl Controls {
    pub fn slider(
        &mut self,
        parent: WidgetId,
        settings: SliderSettings,
        style: LayoutStyle,
    ) -> Result<WidgetId, ControlError> {
        settings.validate()?;
        let (owned_width, owned_height) = self.slider_owned_sizes(settings.axis, &style);
        let id = self.add_slider_host(parent, settings, style)?;
        self.set_role(id, Role::Slider);
        self.edit_props(id, |p| p.focusable = true)?;
        self.set_automation_name(id, "Value")?;
        self.sliders.items.insert(
            id,
            SliderState {
                settings,
                range: None,
                owned_width,
                owned_height,
            },
        );
        Ok(id)
    }
    pub fn range_slider(
        &mut self,
        parent: WidgetId,
        settings: RangeSliderSettings,
        style: LayoutStyle,
    ) -> Result<WidgetId, ControlError> {
        settings.validate()?;
        let (owned_width, owned_height) = self.slider_owned_sizes(settings.axis, &style);
        let id = self.add_slider_host(parent, settings.scalar(), style)?;
        self.set_role(id, Role::RangeSlider);
        let mut thumbs = Vec::new();
        for name in ["Lower value", "Upper value"] {
            let thumb = self.panel(
                id,
                LayoutStyle {
                    width: Length::Px(20.0),
                    height: Length::Px(20.0),
                    ..Default::default()
                },
            )?;
            self.set_surface(thumb, SurfaceKind::Transparent)?;
            self.set_role(thumb, Role::Slider);
            self.set_automation_name(thumb, name)?;
            self.edit_props(thumb, |p| p.focusable = true)?;
            thumbs.push(thumb);
        }
        self.sliders.items.insert(
            id,
            SliderState {
                settings: settings.scalar(),
                range: Some(RangeState {
                    upper: settings.upper,
                    crossing: settings.crossing,
                    lower_thumb: thumbs[0],
                    upper_thumb: thumbs[1],
                }),
                owned_width,
                owned_height,
            },
        );
        Ok(id)
    }
    fn add_slider_host(
        &mut self,
        parent: WidgetId,
        settings: SliderSettings,
        mut style: LayoutStyle,
    ) -> Result<WidgetId, ControlError> {
        style.kind = LayoutKind::Overlay;
        style.padding = Edges::all(0.0);
        if style.width == Length::Auto {
            style.width = Length::Px(if settings.axis == Axis::Horizontal {
                160.0
            } else {
                self.theme.tokens().control_height
            });
        }
        if style.height == Length::Auto {
            style.height = Length::Px(if settings.axis == Axis::Horizontal {
                self.theme.tokens().control_height
            } else {
                160.0
            });
        }
        let id = self.panel(parent, style)?;
        self.set_surface(id, SurfaceKind::Transparent)?;
        self.edit_props(id, |p| p.clip = true)?;
        Ok(id)
    }
    fn slider_owned_sizes(&self, axis: Axis, style: &LayoutStyle) -> (Option<f32>, Option<f32>) {
        (
            (style.width == Length::Auto).then_some(if axis == Axis::Horizontal {
                160.0
            } else {
                self.theme.tokens().control_height
            }),
            (style.height == Length::Auto).then_some(if axis == Axis::Horizontal {
                self.theme.tokens().control_height
            } else {
                160.0
            }),
        )
    }
    fn slider_owner(&self, id: WidgetId) -> Option<(WidgetId, Option<bool>)> {
        if let Some(item) = self.sliders.items.get(&id) {
            return Some((id, item.range.is_none().then_some(false)));
        }
        self.sliders.items.iter().find_map(|(&owner, item)| {
            let range = item.range?;
            if range.lower_thumb == id {
                Some((owner, Some(false)))
            } else if range.upper_thumb == id {
                Some((owner, Some(true)))
            } else {
                None
            }
        })
    }
    /// A scalar slider or one of a range slider's accessible thumb IDs.
    pub fn slider_settings(&self, id: WidgetId) -> Option<SliderSettings> {
        let (owner, upper) = self.slider_owner(id)?;
        let upper = upper?;
        self.runtime.tree().node(id)?;
        let mut settings = self.sliders.items[&owner].settings;
        if upper {
            settings.value = self.sliders.items[&owner].range?.upper;
        }
        Some(settings)
    }
    pub fn slider_value(&self, id: WidgetId) -> Option<f64> {
        Some(self.slider_settings(id)?.value)
    }
    pub fn range_slider_settings(&self, id: WidgetId) -> Option<RangeSliderSettings> {
        let state = self.sliders.items.get(&id)?;
        let range = state.range?;
        self.runtime.tree().node(id)?;
        let s = state.settings;
        Some(RangeSliderSettings {
            minimum: s.minimum,
            maximum: s.maximum,
            lower: s.value,
            upper: range.upper,
            step: s.step,
            axis: s.axis,
            reversed: s.reversed,
            snap_to_ticks: s.snap_to_ticks,
            tick_frequency: s.tick_frequency,
            crossing: range.crossing,
        })
    }
    pub fn range_slider_values(&self, id: WidgetId) -> Option<(f64, f64)> {
        let s = self.range_slider_settings(id)?;
        Some((s.lower, s.upper))
    }
    pub fn range_slider_thumbs(&self, id: WidgetId) -> Option<(WidgetId, WidgetId)> {
        let r = self.sliders.items.get(&id)?.range?;
        Some((r.lower_thumb, r.upper_thumb))
    }
    pub fn set_slider_value(&mut self, id: WidgetId, value: f64) -> Result<(), ControlError> {
        let (owner, upper) = self
            .slider_owner(id)
            .ok_or_else(|| ControlError::Invalid("Unknown slider".into()))?;
        let upper = upper.ok_or_else(|| {
            ControlError::Invalid("Address a range thumb or use set_range_slider_values".into())
        })?;
        let state = self.sliders.items[&owner];
        if !value.is_finite() || !(state.settings.minimum..=state.settings.maximum).contains(&value)
        {
            return Err(ControlError::Invalid(
                "Slider value is outside the range".into(),
            ));
        }
        self.end_slider_gesture(owner)?;
        self.set_thumb_value(owner, upper, value, false)?;
        Ok(())
    }
    /// Set both endpoints atomically. Programmatic updates require ordered
    /// endpoints and do not snap; crossing policy applies to thumb interaction.
    pub fn set_range_slider_values(
        &mut self,
        id: WidgetId,
        lower: f64,
        upper: f64,
    ) -> Result<(), ControlError> {
        let mut settings = self
            .range_slider_settings(id)
            .ok_or_else(|| ControlError::Invalid("Unknown range slider".into()))?;
        settings.lower = lower;
        settings.upper = upper;
        settings.validate()?;
        self.end_slider_gesture(id)?;
        let state = self.sliders.items.get_mut(&id).unwrap();
        state.settings.value = lower;
        state.range.as_mut().unwrap().upper = upper;
        self.scene_dirty = true;
        Ok(())
    }
    pub fn configure_slider(
        &mut self,
        id: WidgetId,
        settings: SliderSettings,
    ) -> Result<(), ControlError> {
        settings.validate()?;
        let state = self
            .sliders
            .items
            .get(&id)
            .ok_or_else(|| ControlError::Invalid("Unknown slider".into()))?;
        if state.range.is_some() {
            return Err(ControlError::Invalid(
                "Use configure_range_slider for a range".into(),
            ));
        }
        self.end_slider_gesture(id)?;
        self.sliders.items.get_mut(&id).unwrap().settings = settings;
        self.scene_dirty = true;
        Ok(())
    }
    pub fn configure_range_slider(
        &mut self,
        id: WidgetId,
        settings: RangeSliderSettings,
    ) -> Result<(), ControlError> {
        settings.validate()?;
        if self.range_slider_settings(id).is_none() {
            return Err(ControlError::Invalid("Unknown range slider".into()));
        }
        self.end_slider_gesture(id)?;
        let state = self.sliders.items.get_mut(&id).unwrap();
        state.settings = settings.scalar();
        let range = state.range.as_mut().unwrap();
        range.upper = settings.upper;
        range.crossing = settings.crossing;
        self.scene_dirty = true;
        Ok(())
    }
    fn end_slider_gesture(&mut self, owner: WidgetId) -> Result<(), ControlError> {
        if self.sliders.drag.is_some_and(|d| d.owner == owner) {
            self.sliders.drag = None;
            self.runtime.apply(Mutation::CapturePointer(None))?;
        }
        Ok(())
    }
    fn set_thumb_value(
        &mut self,
        owner: WidgetId,
        mut upper: bool,
        value: f64,
        interactive: bool,
    ) -> Result<bool, ControlError> {
        let state = self.sliders.items.get_mut(&owner).unwrap();
        let value = if interactive {
            snapped(state.settings, value)
        } else {
            value
        };
        if let Some(range) = state.range.as_mut() {
            if upper && value < state.settings.value {
                if range.crossing == ThumbCrossing::Swap {
                    range.upper = state.settings.value;
                    state.settings.value = value;
                    upper = false;
                } else {
                    range.upper = state.settings.value;
                }
            } else if !upper && value > range.upper {
                if range.crossing == ThumbCrossing::Swap {
                    state.settings.value = range.upper;
                    range.upper = value;
                    upper = true;
                } else {
                    state.settings.value = range.upper;
                }
            } else if upper {
                range.upper = value;
            } else {
                state.settings.value = value;
            }
        } else {
            state.settings.value = value;
        }
        self.scene_dirty = true;
        Ok(upper)
    }
    pub(super) fn sync_sliders(&mut self) -> Result<(), ControlError> {
        if self
            .sliders
            .drag
            .is_some_and(|d| !self.is_enabled(d.owner) || self.runtime.captured() != Some(d.owner))
        {
            self.cancel_slider_interaction()?;
        }
        let size = thumb_size(self.theme);
        let defaults: Vec<_> = self.sliders.items.iter().map(|(&id, s)| (id, *s)).collect();
        for (id, state) in defaults {
            let Some(node) = self.runtime.tree().node(id) else {
                continue;
            };
            let desired_width = if state.settings.axis == Axis::Horizontal {
                160.0
            } else {
                self.theme.tokens().control_height
            };
            let desired_height = if state.settings.axis == Axis::Horizontal {
                self.theme.tokens().control_height
            } else {
                160.0
            };
            let width = state
                .owned_width
                .filter(|v| node.props.style.width == Length::Px(*v))
                .map(|_| desired_width);
            let height = state
                .owned_height
                .filter(|v| node.props.style.height == Length::Px(*v))
                .map(|_| desired_height);
            let state = self.sliders.items.get_mut(&id).unwrap();
            state.owned_width = width;
            state.owned_height = height;
            if width.is_some() || height.is_some() {
                self.edit_props(id, |p| {
                    if let Some(v) = width {
                        p.style.width = Length::Px(v);
                    }
                    if let Some(v) = height {
                        p.style.height = Length::Px(v);
                    }
                })?;
            }
        }
        let items: Vec<_> = self
            .sliders
            .items
            .iter()
            .filter_map(|(&id, s)| Some((id, *s, s.range?)))
            .collect();
        for (id, state, range) in items {
            let Some(bounds) = self.runtime.layout_bounds(id) else {
                continue;
            };
            let bounds = Rect::new(0.0, 0.0, bounds.width, bounds.height);
            for (thumb, value) in [
                (range.lower_thumb, state.settings.value),
                (range.upper_thumb, range.upper),
            ] {
                let rect = thumb_rect(bounds, state.settings, value, size);
                self.edit_props(thumb, |p| {
                    p.style.width = Length::Px(rect.width);
                    p.style.height = Length::Px(rect.height);
                    p.style.offset = Point::new(rect.x, rect.y);
                })?;
            }
        }
        Ok(())
    }
    pub(super) fn retain_sliders(&mut self) -> Result<(), ControlError> {
        self.sliders.items.retain(|id, s| {
            self.runtime.tree().contains(*id)
                && s.range.is_none_or(|r| {
                    self.runtime.tree().contains(r.lower_thumb)
                        && self.runtime.tree().contains(r.upper_thumb)
                })
        });
        if self
            .sliders
            .drag
            .is_some_and(|d| !self.sliders.items.contains_key(&d.owner))
        {
            self.sliders.drag = None;
            self.runtime.apply(Mutation::CapturePointer(None))?;
        }
        Ok(())
    }
    pub(super) fn preflight_slider_removal(&self, id: WidgetId) -> Result<(), ControlError> {
        if self.sliders.items.values().any(|s| {
            s.range
                .is_some_and(|r| r.lower_thumb == id || r.upper_thumb == id)
        }) {
            return Err(ControlError::Invalid(
                "Remove the RangeSlider rather than a required thumb".into(),
            ));
        }
        Ok(())
    }
    pub(super) fn cancel_slider_interaction(&mut self) -> Result<(), ControlError> {
        if let Some(drag) = self.sliders.drag.take() {
            if let Some(state) = self.sliders.items.get_mut(&drag.owner) {
                state.settings.value = drag.lower_value;
                if let Some(range) = state.range.as_mut() {
                    range.upper = drag.upper_value;
                }
            }
            self.runtime.apply(Mutation::CapturePointer(None))?;
            self.scene_dirty = true;
        }
        Ok(())
    }
    fn slider_point_value(&self, owner: WidgetId, point: Point) -> Option<f64> {
        if !point.is_finite() {
            return None;
        }
        let state = self.sliders.items.get(&owner)?;
        let bounds = self.runtime.bounds(owner)?;
        let half = thumb_size(self.theme) * 0.5;
        let (position, start, length) = match state.settings.axis {
            Axis::Horizontal => (point.x, bounds.x + half, bounds.width - 2.0 * half),
            Axis::Vertical => (point.y, bounds.y + half, bounds.height - 2.0 * half),
        };
        if length <= 0.0 {
            return None;
        }
        let mut ratio = (f64::from(position - start) / f64::from(length)).clamp(0.0, 1.0);
        if state.settings.axis == Axis::Vertical {
            ratio = 1.0 - ratio;
        }
        if state.settings.reversed {
            ratio = 1.0 - ratio;
        }
        Some(state.settings.minimum + ratio * (state.settings.maximum - state.settings.minimum))
    }
    fn focus_slider_thumb(&mut self, owner: WidgetId, upper: bool) -> Result<(), ControlError> {
        let id = self.sliders.items[&owner]
            .range
            .map_or(owner, |r| if upper { r.upper_thumb } else { r.lower_thumb });
        self.runtime.apply(Mutation::Focus(Some(id)))?;
        Ok(())
    }
    pub(super) fn slider_before_input(&mut self, event: &InputEvent) -> Result<bool, ControlError> {
        if let Some(drag) = self.sliders.drag {
            if matches!(
                event,
                InputEvent::WindowFocused(false)
                    | InputEvent::KeyDown {
                        key: Key::Escape,
                        ..
                    }
            ) {
                self.cancel_slider_interaction()?;
                return Ok(matches!(
                    event,
                    InputEvent::KeyDown {
                        key: Key::Escape,
                        ..
                    }
                ));
            }
            if let InputEvent::PointerMoved { position } = event {
                if let Some(value) = self.slider_point_value(drag.owner, *position) {
                    let upper = self.set_thumb_value(drag.owner, drag.upper, value, true)?;
                    self.sliders.drag.as_mut().unwrap().upper = upper;
                    self.focus_slider_thumb(drag.owner, upper)?;
                }
                return Ok(true);
            }
            if matches!(
                event,
                InputEvent::PointerUp {
                    button: PointerButton::Primary,
                    ..
                }
            ) {
                self.sliders.drag = None;
                self.runtime.apply(Mutation::CapturePointer(None))?;
                self.scene_dirty = true;
                return Ok(true);
            }
        }
        if let InputEvent::PointerDown {
            position,
            button: PointerButton::Primary,
            ..
        } = event
        {
            let Some(hit) = self.runtime.hit_test(*position) else {
                return Ok(false);
            };
            let Some((owner, thumb)) = self.slider_owner(hit) else {
                return Ok(false);
            };
            if !self.is_enabled(owner) {
                return Ok(false);
            }
            let Some(value) = self.slider_point_value(owner, *position) else {
                return Ok(false);
            };
            let state = self.sliders.items[&owner];
            let upper = thumb.unwrap_or_else(|| {
                state
                    .range
                    .is_some_and(|r| (value - r.upper).abs() < (value - state.settings.value).abs())
            });
            self.sliders.drag = Some(SliderDrag {
                owner,
                upper,
                lower_value: state.settings.value,
                upper_value: state.range.map_or(state.settings.value, |r| r.upper),
            });
            self.runtime.apply(Mutation::CapturePointer(Some(owner)))?;
            let upper = self.set_thumb_value(owner, upper, value, true)?;
            self.sliders.drag.as_mut().unwrap().upper = upper;
            self.focus_slider_thumb(owner, upper)?;
            return Ok(true);
        }
        if let InputEvent::KeyDown { key, modifiers, .. } = event {
            let Some(id) = self.runtime.focused() else {
                return Ok(false);
            };
            let Some((owner, Some(upper))) = self.slider_owner(id) else {
                return Ok(false);
            };
            if !self.is_enabled(owner) || modifiers.alt || modifiers.super_key {
                return Ok(false);
            }
            let settings = self.slider_settings(id).unwrap();
            let amount = interaction_step(settings) * if modifiers.control { 10.0 } else { 1.0 };
            let direction = if settings.reversed { -1.0 } else { 1.0 };
            let value = match (settings.axis, key) {
                (_, Key::Home) => settings.minimum,
                (_, Key::End) => settings.maximum,
                (Axis::Horizontal, Key::ArrowLeft) | (Axis::Vertical, Key::ArrowDown) => {
                    settings.value - amount * direction
                }
                (Axis::Horizontal, Key::ArrowRight) | (Axis::Vertical, Key::ArrowUp) => {
                    settings.value + amount * direction
                }
                (_, Key::PageDown) => settings.value - amount * 10.0 * direction,
                (_, Key::PageUp) => settings.value + amount * 10.0 * direction,
                _ => return Ok(false),
            };
            let upper = self.set_thumb_value(
                owner,
                upper,
                value.clamp(settings.minimum, settings.maximum),
                true,
            )?;
            self.focus_slider_thumb(owner, upper)?;
            return Ok(true);
        }
        Ok(false)
    }
    pub(super) fn paint_slider(
        &mut self,
        id: WidgetId,
        bounds: Rect,
        clip: Rect,
    ) -> Result<bool, ControlError> {
        let Some((owner, thumb)) = self.slider_owner(id) else {
            return Ok(false);
        };
        let state = self.sliders.items[&owner];
        let tokens = self.theme.tokens();
        let enabled = self.is_enabled(owner);
        let size = thumb_size(self.theme);
        if id == owner {
            let horizontal = state.settings.axis == Axis::Horizontal;
            let track = if horizontal {
                Rect::new(
                    bounds.x + size * 0.5,
                    bounds.y + bounds.height * 0.5 - 2.0,
                    (bounds.width - size).max(0.0),
                    4.0,
                )
            } else {
                Rect::new(
                    bounds.x + bounds.width * 0.5 - 2.0,
                    bounds.y + size * 0.5,
                    4.0,
                    (bounds.height - size).max(0.0),
                )
            };
            editor_fill(&mut self.scene, track, clip, 2.0, tokens.border);
            let lo = slider_ratio(
                state.settings,
                state
                    .range
                    .map_or(state.settings.minimum, |_| state.settings.value),
            );
            let hi = slider_ratio(
                state.settings,
                state.range.map_or(state.settings.value, |r| r.upper),
            );
            let start = lo.min(hi);
            let end = lo.max(hi);
            let selected = if horizontal {
                Rect::new(
                    track.x + track.width * start,
                    track.y,
                    track.width * (end - start),
                    4.0,
                )
            } else {
                Rect::new(
                    track.x,
                    track.y + track.height * (1.0 - end),
                    4.0,
                    track.height * (end - start),
                )
            };
            editor_fill(
                &mut self.scene,
                selected,
                clip,
                2.0,
                if enabled { tokens.focus } else { tokens.muted },
            );
            if let Some(frequency) = state.settings.tick_frequency {
                let count = ((state.settings.maximum - state.settings.minimum) / frequency).floor();
                let stride = (count / 100.0).ceil().max(1.0);
                for i in 0..=100 {
                    let index = f64::from(i) * stride;
                    if index > count {
                        break;
                    }
                    let value = (state.settings.minimum + index * frequency)
                        .clamp(state.settings.minimum, state.settings.maximum);
                    let ratio = slider_ratio(state.settings, value);
                    let tick = if horizontal {
                        Rect::new(track.x + track.width * ratio, track.y + 8.0, 1.0, 3.0)
                    } else {
                        Rect::new(
                            track.x + 8.0,
                            track.y + track.height * (1.0 - ratio),
                            3.0,
                            1.0,
                        )
                    };
                    fill_clipped(&mut self.scene, tick, clip, tokens.muted);
                }
            }
            if state.range.is_some() {
                return Ok(true);
            }
        }
        let thumb_bounds = if state.range.is_some() {
            bounds
        } else {
            thumb_rect(bounds, state.settings, state.settings.value, size)
        };
        let focused = self.runtime.focused() == Some(id);
        let pressed = self
            .sliders
            .drag
            .is_some_and(|d| d.owner == owner && thumb == Some(d.upper));
        let stroke = if focused || pressed {
            tokens.focus
        } else {
            tokens.border
        };
        editor_fill(&mut self.scene, thumb_bounds, clip, size * 0.5, stroke);
        let inner = Rect::new(
            thumb_bounds.x + 2.0,
            thumb_bounds.y + 2.0,
            (thumb_bounds.width - 4.0).max(0.0),
            (thumb_bounds.height - 4.0).max(0.0),
        );
        editor_fill(
            &mut self.scene,
            inner,
            clip,
            size * 0.5,
            if enabled {
                tokens.surface
            } else {
                tokens.canvas
            },
        );
        let dot = Rect::new(
            thumb_bounds.x + thumb_bounds.width * 0.3,
            thumb_bounds.y + thumb_bounds.height * 0.3,
            thumb_bounds.width * 0.4,
            thumb_bounds.height * 0.4,
        );
        editor_fill(
            &mut self.scene,
            dot,
            clip,
            size * 0.5,
            if enabled { tokens.focus } else { tokens.muted },
        );
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn setup(settings: SliderSettings) -> (Controls, WidgetId) {
        let mut ui = Controls::new(Theme::FluentLight).unwrap();
        let id = ui
            .slider(
                ui.root(),
                settings,
                LayoutStyle {
                    width: Length::Px(220.0),
                    height: Length::Px(36.0),
                    ..Default::default()
                },
            )
            .unwrap();
        ui.update(Size::new(500.0, 300.0)).unwrap();
        (ui, id)
    }
    fn key(ui: &mut Controls, key: Key) {
        ui.dispatch(InputEvent::KeyDown {
            key,
            modifiers: Default::default(),
            repeat: false,
        })
        .unwrap();
    }
    #[test]
    fn scalar_keyboard_snapping_reversal_and_atomic_validation() {
        let (mut ui, id) = setup(SliderSettings {
            minimum: 10.0,
            maximum: 110.0,
            value: 40.0,
            step: 10.0,
            tick_frequency: Some(10.0),
            snap_to_ticks: true,
            reversed: true,
            ..Default::default()
        });
        ui.perform_action(id, SemanticAction::Focus, None).unwrap();
        key(&mut ui, Key::ArrowRight);
        assert_eq!(ui.slider_value(id), Some(30.0));
        key(&mut ui, Key::End);
        assert_eq!(ui.slider_value(id), Some(110.0));
        let old = ui.slider_settings(id).unwrap();
        assert!(
            ui.configure_slider(
                id,
                SliderSettings {
                    maximum: old.minimum,
                    ..old
                }
            )
            .is_err()
        );
        assert_eq!(ui.slider_settings(id), Some(old));
        assert!(ui.set_slider_value(id, f64::NAN).is_err());
        assert_eq!(ui.slider_value(id), Some(110.0));
        ui.set_slider_value(id, 42.0).unwrap();
        assert_eq!(ui.slider_value(id), Some(42.0));
    }
    #[test]
    fn pointer_drag_cancel_and_disabled_capture_preserve_original_value() {
        let (mut ui, id) = setup(SliderSettings {
            value: 25.0,
            ..Default::default()
        });
        let bounds = ui.runtime.bounds(id).unwrap();
        let point = Point::new(bounds.x + 160.0, bounds.y + 18.0);
        let down = InputEvent::PointerDown {
            position: point,
            button: PointerButton::Primary,
            modifiers: Default::default(),
        };
        ui.dispatch(down.clone()).unwrap();
        assert_eq!(ui.slider_value(id), Some(75.0));
        key(&mut ui, Key::Escape);
        assert_eq!(ui.slider_value(id), Some(25.0));
        assert!(ui.runtime.captured().is_none());
        ui.dispatch(down).unwrap();
        ui.set_enabled(id, false).unwrap();
        ui.update(Size::new(500.0, 300.0)).unwrap();
        assert_eq!(ui.slider_value(id), Some(25.0));
        assert!(ui.runtime.captured().is_none());
    }
    #[test]
    fn pointer_and_keyboard_share_step_and_tick_grids() {
        for (snap_to_ticks, expected, next) in [(false, 22.0, 32.0), (true, 27.0, 52.0)] {
            let (mut ui, id) = setup(SliderSettings {
                minimum: 2.0,
                maximum: 102.0,
                value: 2.0,
                step: 10.0,
                tick_frequency: Some(25.0),
                snap_to_ticks,
                ..Default::default()
            });
            let bounds = ui.runtime.bounds(id).unwrap();
            // Track is 200 pixels long; x = 10 + 2 * 23 means raw value 25.
            let point = Point::new(bounds.x + 56.0, bounds.y + 18.0);
            ui.dispatch(InputEvent::PointerDown {
                position: point,
                button: PointerButton::Primary,
                modifiers: Default::default(),
            })
            .unwrap();
            ui.dispatch(InputEvent::PointerUp {
                position: point,
                button: PointerButton::Primary,
                modifiers: Default::default(),
            })
            .unwrap();
            assert_eq!(ui.slider_value(id), Some(expected));
            key(&mut ui, Key::ArrowRight);
            assert_eq!(ui.slider_value(id), Some(next));
            key(&mut ui, Key::ArrowLeft);
            assert_eq!(ui.slider_value(id), Some(expected));
            ui.set_slider_value(id, 24.5).unwrap();
            assert_eq!(ui.slider_value(id), Some(24.5));
            key(&mut ui, Key::End);
            assert_eq!(ui.slider_value(id), Some(102.0));
        }
    }
    #[test]
    fn range_crossing_swaps_focus_and_clamp_never_reorders_values() {
        let mut ui = Controls::new(Theme::Light).unwrap();
        let range = ui
            .range_slider(
                ui.root(),
                RangeSliderSettings {
                    lower: 30.0,
                    upper: 60.0,
                    step: 30.0,
                    crossing: ThumbCrossing::Swap,
                    ..Default::default()
                },
                LayoutStyle::default(),
            )
            .unwrap();
        ui.update(Size::new(500.0, 300.0)).unwrap();
        let (lower, upper) = ui.range_slider_thumbs(range).unwrap();
        ui.perform_action(lower, SemanticAction::Focus, None)
            .unwrap();
        key(&mut ui, Key::ArrowRight);
        assert_eq!(ui.range_slider_values(range), Some((60.0, 60.0)));
        key(&mut ui, Key::ArrowRight);
        assert_eq!(ui.range_slider_values(range), Some((60.0, 90.0)));
        assert_eq!(ui.runtime.focused(), Some(upper));
        let mut settings = ui.range_slider_settings(range).unwrap();
        settings.crossing = ThumbCrossing::Clamp;
        ui.configure_range_slider(range, settings).unwrap();
        ui.perform_action(lower, SemanticAction::Focus, None)
            .unwrap();
        key(&mut ui, Key::End);
        assert_eq!(ui.range_slider_values(range), Some((90.0, 90.0)));
        assert_eq!(ui.runtime.focused(), Some(lower));
        assert!(ui.set_range_slider_values(range, 90.0, 10.0).is_err());
        assert_eq!(ui.range_slider_values(range), Some((90.0, 90.0)));
        assert!(ui.remove(lower).is_err());
    }
    #[test]
    fn vertical_slider_uses_bottom_as_minimum_and_reversed_top_as_minimum() {
        let mut ui = Controls::new(Theme::HighContrastDark).unwrap();
        let id = ui
            .slider(
                ui.root(),
                SliderSettings {
                    axis: Axis::Vertical,
                    ..Default::default()
                },
                LayoutStyle {
                    width: Length::Px(36.0),
                    height: Length::Px(220.0),
                    ..Default::default()
                },
            )
            .unwrap();
        ui.update(Size::new(300.0, 300.0)).unwrap();
        let bounds = ui.runtime.bounds(id).unwrap();
        let low = Point::new(bounds.x + 18.0, bounds.y + 210.0);
        let high = Point::new(bounds.x + 18.0, bounds.y + 10.0);
        assert_eq!(ui.slider_point_value(id, low), Some(0.0));
        assert_eq!(ui.slider_point_value(id, high), Some(100.0));
        let mut settings = ui.slider_settings(id).unwrap();
        settings.reversed = true;
        ui.configure_slider(id, settings).unwrap();
        assert_eq!(ui.slider_point_value(id, low), Some(100.0));
        assert_eq!(ui.slider_point_value(id, high), Some(0.0));
    }
    #[test]
    fn range_cancel_restores_both_values_and_thumb_identity_after_swap() {
        let mut ui = Controls::new(Theme::Light).unwrap();
        let id = ui
            .range_slider(
                ui.root(),
                RangeSliderSettings {
                    crossing: ThumbCrossing::Swap,
                    ..Default::default()
                },
                LayoutStyle {
                    width: Length::Px(220.0),
                    ..Default::default()
                },
            )
            .unwrap();
        ui.update(Size::new(500.0, 300.0)).unwrap();
        let (lower, upper) = ui.range_slider_thumbs(id).unwrap();
        let bounds = ui.runtime.bounds(lower).unwrap();
        let point = Point::new(bounds.x + 10.0, bounds.y + 10.0);
        ui.dispatch(InputEvent::PointerDown {
            position: point,
            button: PointerButton::Primary,
            modifiers: Default::default(),
        })
        .unwrap();
        ui.dispatch(InputEvent::PointerMoved {
            position: Point::new(point.x + 140.0, point.y),
        })
        .unwrap();
        assert_eq!(ui.runtime.focused(), Some(upper));
        ui.cancel_pointer_interaction().unwrap();
        assert_eq!(ui.range_slider_values(id), Some((25.0, 75.0)));
        assert_eq!(ui.range_slider_thumbs(id), Some((lower, upper)));
        assert!(ui.runtime.captured().is_none());
    }
    #[test]
    fn tick_paint_is_bounded_and_semantics_expose_two_independent_values() {
        let mut ui = Controls::new(Theme::Light).unwrap();
        let id = ui
            .range_slider(
                ui.root(),
                RangeSliderSettings {
                    tick_frequency: Some(0.000001),
                    ..Default::default()
                },
                LayoutStyle::default(),
            )
            .unwrap();
        ui.update(Size::new(500.0, 300.0)).unwrap();
        let (lower, upper) = ui.range_slider_thumbs(id).unwrap();
        let nodes = ui.semantics();
        for (id, value) in [(lower, "25"), (upper, "75")] {
            let node = nodes.iter().find(|n| n.id == id).unwrap();
            assert_eq!(node.role, Role::Slider);
            assert_eq!(node.value.as_deref(), Some(value));
            assert!(node.actions.contains(&SemanticAction::SetValue));
        }
        assert!(ui.scene().rectangles.len() + ui.scene().rounded_rectangles.len() < 140);
        ui.perform_action(upper, SemanticAction::SetValue, Some("80"))
            .unwrap();
        assert_eq!(ui.range_slider_values(id), Some((25.0, 80.0)));
        assert!(
            SliderSettings {
                tick_frequency: Some(f64::from_bits(1)),
                ..Default::default()
            }
            .validate()
            .is_err()
        );
    }
}
