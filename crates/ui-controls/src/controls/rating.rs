//! Nullable star ratings with separate preview and accepted value.
use super::*;
use rust_desktop_ui_core::Modifiers;
use std::{cell::RefCell, collections::VecDeque, rc::Rc};
mod glyph;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RatingSettings {
    pub value: Option<f64>,
    pub maximum: u8,
    pub step: f64,
    pub allow_empty: bool,
    pub read_only: bool,
    pub glyph_size: f32,
    pub spacing: f32,
}
impl Default for RatingSettings {
    fn default() -> Self {
        Self {
            value: None,
            maximum: 5,
            step: 1.0,
            allow_empty: true,
            read_only: false,
            glyph_size: 24.0,
            spacing: 4.0,
        }
    }
}
impl RatingSettings {
    pub fn validate(self) -> Result<(), ControlError> {
        if !(1..=20).contains(&self.maximum)
            || !self.step.is_finite()
            || !(0.01..=1.0).contains(&self.step)
            || !self.glyph_size.is_finite()
            || !(8.0..=64.0).contains(&self.glyph_size)
            || !self.spacing.is_finite()
            || !(0.0..=32.0).contains(&self.spacing)
        {
            return Err(error(
                "Rating requires maximum 1..20, step 0.01..1, glyph size 8..64 and spacing 0..32",
            ));
        }
        self.validate_value(self.value)
    }
    pub fn validate_value(self, value: Option<f64>) -> Result<(), ControlError> {
        if value.is_none() && !self.allow_empty
            || value.is_some_and(|value| {
                !value.is_finite() || !(0.0..=f64::from(self.maximum)).contains(&value)
            })
        {
            return Err(error(
                "Rating value must be empty when allowed or finite within 0..maximum",
            ));
        }
        Ok(())
    }
    fn width(self) -> f32 {
        f32::from(self.maximum) * self.glyph_size
            + f32::from(self.maximum - 1) * self.spacing
            + 64.0
    }
    fn height(self) -> f32 {
        self.glyph_size + 8.0
    }
}
#[derive(Clone)]
pub struct RatingValidator(Rc<RatingValidation>);
type RatingValidation = dyn Fn(Option<f64>) -> Result<(), String>;
impl RatingValidator {
    pub fn new(check: impl Fn(Option<f64>) -> Result<(), String> + 'static) -> Self {
        Self(Rc::new(check))
    }
    pub fn check(&self, value: Option<f64>) -> Result<(), String> {
        (self.0)(value)
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RatingChanged {
    pub id: WidgetId,
    pub value: Option<f64>,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RatingPreview {
    None,
    Value(Option<f64>),
}
#[derive(Clone, Copy, PartialEq)]
enum PreviewOwner {
    Pointer,
    Keyboard,
}
struct State {
    settings: RatingSettings,
    preview: RatingPreview,
    preview_owner: Option<PreviewOwner>,
    validator: Rc<RefCell<Option<RatingValidator>>>,
    settings_shadow: Rc<RefCell<RatingSettings>>,
    owned_width: Option<f32>,
    owned_height: Option<f32>,
}
#[derive(Default)]
pub(super) struct Ratings {
    items: HashMap<WidgetId, State>,
    drag: Option<WidgetId>,
    events: VecDeque<RatingChanged>,
}
fn error(text: &str) -> ControlError {
    ControlError::Invalid(text.into())
}
fn score(value: Option<f64>) -> String {
    value.map_or_else(|| "Нет оценки".into(), |value| value.to_string())
}
fn input_score(value: f64, maximum: u8) -> f64 {
    ((value * 1e9).round() / 1e9).clamp(0.0, f64::from(maximum))
}
impl Controls {
    pub fn rating(
        &mut self,
        parent: WidgetId,
        label: impl Into<String>,
        settings: RatingSettings,
        mut style: LayoutStyle,
    ) -> Result<WidgetId, ControlError> {
        settings.validate()?;
        let label = label.into();
        self.preflight_new_editor_text(&label, &style, false)?;
        let owned_width = (style.width == Length::Auto).then_some(settings.width());
        let owned_height = (style.height == Length::Auto).then_some(settings.height());
        if let Some(width) = owned_width {
            style.width = Length::Px(width);
        }
        if let Some(height) = owned_height {
            style.height = Length::Px(height);
        }
        style.kind = LayoutKind::Overlay;
        style.padding = Edges::all(0.0);
        let id = self.transparent_panel(parent, style)?;
        self.set_role(id, Role::Rating);
        self.set_automation_name(id, label)?;
        self.edit_props(id, |p| {
            p.focusable = true;
            p.clip = true;
        })?;
        self.ratings.items.insert(
            id,
            State {
                settings,
                preview: RatingPreview::None,
                preview_owner: None,
                validator: Rc::new(RefCell::new(None)),
                settings_shadow: Rc::new(RefCell::new(settings)),
                owned_width,
                owned_height,
            },
        );
        Ok(id)
    }
    pub fn rating_settings(&self, id: WidgetId) -> Option<RatingSettings> {
        self.ratings.items.get(&id).map(|state| state.settings)
    }
    pub fn rating_value(&self, id: WidgetId) -> Option<Option<f64>> {
        self.rating_settings(id).map(|s| s.value)
    }
    pub fn rating_preview(&self, id: WidgetId) -> Option<RatingPreview> {
        self.ratings.items.get(&id).map(|s| s.preview)
    }
    pub fn rating_error(&self, id: WidgetId) -> Option<&str> {
        self.rating_settings(id)?;
        self.controls.get(&id)?.validation_error.as_deref()
    }
    pub fn rating_validator(&self, id: WidgetId) -> Option<RatingValidator> {
        self.ratings.items.get(&id)?.validator.borrow().clone()
    }
    pub fn rating_source_validator(&self, id: WidgetId) -> Option<RatingValidator> {
        let state = self.ratings.items.get(&id)?;
        let settings = Rc::clone(&state.settings_shadow);
        let validator = Rc::clone(&state.validator);
        Some(RatingValidator::new(move |value| {
            settings
                .borrow()
                .validate_value(value)
                .map_err(|e| e.to_string())?;
            let validator = validator.borrow().clone();
            if let Some(validator) = validator {
                validator.check(value)?;
            }
            Ok(())
        }))
    }
    pub fn set_rating_validator(
        &mut self,
        id: WidgetId,
        validator: Option<RatingValidator>,
    ) -> Result<(), ControlError> {
        *self
            .ratings
            .items
            .get(&id)
            .ok_or_else(|| error("Not a Rating"))?
            .validator
            .borrow_mut() = validator;
        Ok(())
    }
    pub fn chain_rating_validator(
        &mut self,
        id: WidgetId,
        validator: RatingValidator,
    ) -> Result<(), ControlError> {
        let old = self.rating_validator(id);
        self.set_rating_validator(
            id,
            Some(RatingValidator::new(move |value| {
                if let Some(old) = &old {
                    old.check(value)?;
                }
                validator.check(value)
            })),
        )
    }
    pub fn set_rating_settings(
        &mut self,
        id: WidgetId,
        settings: RatingSettings,
    ) -> Result<(), ControlError> {
        settings.validate()?;
        let old = self
            .ratings
            .items
            .get(&id)
            .ok_or_else(|| error("Not a Rating"))?;
        if settings.value != old.settings.value
            && let Some(validator) = old.validator.borrow().clone()
        {
            validator
                .check(settings.value)
                .map_err(ControlError::Invalid)?;
        }
        let (width, height) = (old.owned_width, old.owned_height);
        self.edit_props(id, |props| {
            if width.is_some_and(|old| props.style.width == Length::Px(old)) {
                props.style.width = Length::Px(settings.width());
            }
            if height.is_some_and(|old| props.style.height == Length::Px(old)) {
                props.style.height = Length::Px(settings.height());
            }
        })?;
        self.cancel_rating_preview(id)?;
        let state = self.ratings.items.get_mut(&id).unwrap();
        state.settings = settings;
        *state.settings_shadow.borrow_mut() = settings;
        self.controls.get_mut(&id).unwrap().validation_error = None;
        state.owned_width = width.map(|_| settings.width());
        state.owned_height = height.map(|_| settings.height());
        self.scene_dirty = true;
        Ok(())
    }
    pub fn set_rating_value(
        &mut self,
        id: WidgetId,
        value: Option<f64>,
    ) -> Result<(), ControlError> {
        let mut settings = self
            .rating_settings(id)
            .ok_or_else(|| error("Not a Rating"))?;
        settings.value = value;
        self.set_rating_settings(id, settings)
    }
    pub fn set_rating_read_only(
        &mut self,
        id: WidgetId,
        read_only: bool,
    ) -> Result<(), ControlError> {
        let mut settings = self
            .rating_settings(id)
            .ok_or_else(|| error("Not a Rating"))?;
        settings.read_only = read_only;
        self.set_rating_settings(id, settings)
    }
    pub fn drain_rating_changed(&mut self) -> Vec<RatingChanged> {
        self.ratings.events.drain(..).collect()
    }
    fn rating_editable(&self, id: WidgetId) -> bool {
        self.ratings
            .items
            .get(&id)
            .is_some_and(|s| !s.settings.read_only)
            && self.window_focused
            && self.is_enabled(id)
            && self.runtime.tree().is_attached(id)
    }
    fn set_rating_preview(&mut self, id: WidgetId, value: Option<f64>, owner: PreviewOwner) {
        let state = self.ratings.items.get_mut(&id).unwrap();
        state.preview = RatingPreview::Value(value);
        state.preview_owner = Some(owner);
        self.scene_dirty = true;
    }
    fn cancel_rating_preview(&mut self, id: WidgetId) -> Result<(), ControlError> {
        if self.ratings.drag == Some(id) {
            self.ratings.drag = None;
            if self.runtime.captured() == Some(id) {
                self.runtime.apply(Mutation::CapturePointer(None))?;
            }
        }
        if let Some(state) = self.ratings.items.get_mut(&id)
            && state.preview != RatingPreview::None
        {
            state.preview = RatingPreview::None;
            state.preview_owner = None;
            self.scene_dirty = true;
        }
        Ok(())
    }
    fn accept_rating(&mut self, id: WidgetId, value: Option<f64>) -> Result<(), ControlError> {
        if !self.rating_editable(id) {
            return Ok(());
        }
        let state = &self.ratings.items[&id];
        let changed = state.settings.value != value;
        let validation = state
            .settings
            .validate_value(value)
            .and_then(|_| {
                state
                    .validator
                    .borrow()
                    .clone()
                    .map_or(Ok(()), |validator| {
                        validator.check(value).map_err(ControlError::Invalid)
                    })
            })
            .and_then(|_| {
                if changed && self.ratings.events.len() >= 256 {
                    Err(error("Drain rating events before further commits"))
                } else {
                    Ok(())
                }
            });
        self.cancel_rating_preview(id)?;
        if let Err(error) = validation {
            self.controls.get_mut(&id).unwrap().validation_error = Some(error.to_string());
            self.scene_dirty = true;
            return Ok(());
        }
        self.controls.get_mut(&id).unwrap().validation_error = None;
        if changed {
            let state = self.ratings.items.get_mut(&id).unwrap();
            state.settings.value = value;
            state.settings_shadow.borrow_mut().value = value;
            self.ratings.events.push_back(RatingChanged { id, value });
        }
        self.scene_dirty = true;
        Ok(())
    }
    pub(super) fn sync_ratings(&mut self) -> Result<(), ControlError> {
        let ids: Vec<_> = self.ratings.items.keys().copied().collect();
        for id in ids {
            let state = &self.ratings.items[&id];
            let cancel = !self.rating_editable(id)
                || self.ratings.drag == Some(id) && self.runtime.captured() != Some(id)
                || state.preview_owner == Some(PreviewOwner::Keyboard)
                    && self.runtime.focused() != Some(id);
            if cancel {
                self.cancel_rating_preview(id)?;
            }
        }
        Ok(())
    }
    pub(super) fn retain_ratings(&mut self) -> Result<(), ControlError> {
        if let Some(id) = self.ratings.drag
            && !self.runtime.tree().contains(id)
        {
            self.cancel_rating_preview(id)?;
        }
        self.ratings
            .items
            .retain(|id, _| self.runtime.tree().contains(*id));
        self.ratings
            .events
            .retain(|event| self.ratings.items.contains_key(&event.id));
        Ok(())
    }
    pub(super) fn cancel_rating_interaction(&mut self) -> Result<(), ControlError> {
        for id in self.ratings.items.keys().copied().collect::<Vec<_>>() {
            self.cancel_rating_preview(id)?;
        }
        Ok(())
    }
    fn rating_point(&self, id: WidgetId, point: Point) -> Option<f64> {
        if !point.is_finite() {
            return None;
        }
        let settings = self.rating_settings(id)?;
        let bounds = self.runtime.bounds(id)?;
        let (area, size, gap) = glyph::geometry(bounds, settings);
        if size <= 0.0 {
            return None;
        }
        let unit = size + gap;
        let position = (point.x - area.x).clamp(0.0, area.width);
        let index = (position / unit).floor();
        let fraction = ((position - index * unit) / size).clamp(0.0, 1.0);
        Some(input_score(
            (((f64::from(index + fraction) / settings.step) - 1e-9).ceil() * settings.step)
                .clamp(0.0, f64::from(settings.maximum)),
            settings.maximum,
        ))
    }
    fn rating_inside(&self, id: WidgetId, point: Point) -> bool {
        self.runtime
            .bounds(id)
            .zip(self.runtime.clip(id))
            .and_then(|(b, c)| b.intersection(c))
            .is_some_and(|bounds| bounds.contains(point.x, point.y))
    }
    pub(super) fn rating_before_input(&mut self, event: &InputEvent) -> Result<bool, ControlError> {
        self.sync_ratings()?;
        if matches!(event, InputEvent::WindowFocused(false)) {
            self.cancel_rating_interaction()?;
            return Ok(false);
        }
        if let Some(id) = self.ratings.drag {
            match event {
                InputEvent::PointerMoved { position } => {
                    if let Some(value) = self.rating_point(id, *position) {
                        self.set_rating_preview(id, Some(value), PreviewOwner::Pointer);
                    }
                    return Ok(true);
                }
                InputEvent::PointerUp {
                    position,
                    button: PointerButton::Primary,
                    ..
                } => {
                    let value = self.rating_point(id, *position);
                    let inside = self.rating_inside(id, *position);
                    self.cancel_rating_preview(id)?;
                    if inside && let Some(value) = value {
                        self.accept_rating(id, Some(value))?;
                    }
                    return Ok(true);
                }
                InputEvent::KeyDown {
                    key: Key::Escape, ..
                } => {
                    self.cancel_rating_preview(id)?;
                    return Ok(true);
                }
                _ => {}
            }
        }
        match event {
            InputEvent::PointerDown {
                position,
                button: PointerButton::Primary,
                modifiers,
            } if *modifiers == Modifiers::default() => {
                let Some(id) = self
                    .runtime
                    .hit_test(*position)
                    .filter(|id| self.ratings.items.contains_key(id))
                else {
                    return Ok(false);
                };
                if !self.rating_editable(id) {
                    return Ok(false);
                }
                let Some(value) = self.rating_point(id, *position) else {
                    return Ok(false);
                };
                self.runtime.apply(Mutation::Focus(Some(id)))?;
                self.runtime.apply(Mutation::CapturePointer(Some(id)))?;
                self.ratings.drag = Some(id);
                self.set_rating_preview(id, Some(value), PreviewOwner::Pointer);
                return Ok(true);
            }
            InputEvent::PointerMoved { position } => {
                let hit = self
                    .runtime
                    .hit_test(*position)
                    .filter(|id| self.rating_editable(*id));
                for id in self.ratings.items.keys().copied().collect::<Vec<_>>() {
                    if Some(id) == hit {
                        if let Some(value) = self.rating_point(id, *position) {
                            self.set_rating_preview(id, Some(value), PreviewOwner::Pointer);
                        }
                    } else if self.ratings.items[&id].preview_owner == Some(PreviewOwner::Pointer) {
                        self.cancel_rating_preview(id)?;
                    }
                }
            }
            InputEvent::PointerLeft => {
                for id in self.ratings.items.keys().copied().collect::<Vec<_>>() {
                    if self.ratings.items[&id].preview_owner == Some(PreviewOwner::Pointer) {
                        self.cancel_rating_preview(id)?;
                    }
                }
            }
            InputEvent::KeyDown {
                key,
                modifiers,
                repeat,
            } if *modifiers == Modifiers::default() => {
                let Some(id) = self
                    .runtime
                    .focused()
                    .filter(|id| self.rating_editable(*id))
                else {
                    return Ok(false);
                };
                let state = &self.ratings.items[&id];
                let settings = state.settings;
                let current = match state.preview {
                    RatingPreview::Value(value) => value,
                    RatingPreview::None => settings.value,
                };
                match key {
                    Key::Escape if state.preview != RatingPreview::None => {
                        self.cancel_rating_preview(id)?;
                        return Ok(true);
                    }
                    Key::Enter | Key::Space => {
                        if !repeat {
                            self.accept_rating(id, current)?;
                        }
                        return Ok(true);
                    }
                    Key::Delete if settings.allow_empty => {
                        if !repeat {
                            self.accept_rating(id, None)?;
                        }
                        return Ok(true);
                    }
                    _ => {}
                }
                let value = match key {
                    Key::Home => 0.0,
                    Key::End => f64::from(settings.maximum),
                    Key::ArrowLeft | Key::ArrowDown => {
                        (current.unwrap_or(0.0) - settings.step).max(0.0)
                    }
                    Key::ArrowRight | Key::ArrowUp => {
                        (current.unwrap_or(0.0) + settings.step).min(f64::from(settings.maximum))
                    }
                    _ => return Ok(false),
                };
                let value = input_score(value, settings.maximum);
                self.set_rating_preview(id, Some(value), PreviewOwner::Keyboard);
                return Ok(true);
            }
            _ => {}
        }
        Ok(false)
    }
    pub(super) fn rating_semantic_value(&self, id: WidgetId) -> Option<String> {
        let settings = self.rating_settings(id)?;
        Some(format!("{} из {}", score(settings.value), settings.maximum))
    }
    pub(super) fn rating_accessible_action(
        &mut self,
        id: WidgetId,
        action: SemanticAction,
        value: Option<&str>,
    ) -> Result<bool, ControlError> {
        let Some(settings) = self.rating_settings(id) else {
            return Ok(false);
        };
        let candidate = match action {
            SemanticAction::Increment => Some(Some(input_score(
                settings.value.unwrap_or(0.0) + settings.step,
                settings.maximum,
            ))),
            SemanticAction::Decrement => Some(Some(input_score(
                settings.value.unwrap_or(0.0) - settings.step,
                settings.maximum,
            ))),
            SemanticAction::SetValue => match value {
                Some("" | "null") => Some(None),
                Some(value) => value.parse::<f64>().ok().map(Some),
                None => None,
            },
            _ => return Ok(false),
        };
        if let Some(value) = candidate {
            self.accept_rating(id, value)?;
        }
        Ok(true)
    }
    pub(super) fn paint_rating(
        &mut self,
        id: WidgetId,
        bounds: Rect,
        clip: Rect,
    ) -> Result<bool, ControlError> {
        let Some(state) = self.ratings.items.get(&id) else {
            return Ok(false);
        };
        let settings = state.settings;
        let preview = state.preview;
        let selected = match preview {
            RatingPreview::Value(value) => value,
            RatingPreview::None => settings.value,
        };
        let (area, size, gap) = glyph::geometry(bounds, settings);
        let tokens = self.theme.tokens();
        let enabled = self.is_enabled(id);
        for index in 0..settings.maximum {
            let rect = Rect::new(area.x + f32::from(index) * (size + gap), area.y, size, size);
            glyph::paint(
                &mut self.scene,
                rect,
                clip,
                (selected.unwrap_or(0.0) - f64::from(index)).clamp(0.0, 1.0) as f32,
                if enabled { tokens.focus } else { tokens.muted },
                tokens.border,
            );
        }
        let label = selected.map_or_else(|| "—".into(), |value| value.to_string());
        let label = if label.len() > 7 {
            format!("{}…", &label[..6])
        } else {
            label
        };
        let mut run = TextRun::new(
            label,
            Rect::new(
                area.x + area.width + 8.0,
                bounds.y,
                (bounds.width - area.width - 12.0).max(0.0),
                bounds.height,
            ),
            if enabled { tokens.text } else { tokens.muted },
            14.0,
        );
        run.clip = clip;
        run.font_size = 14.0;
        run.wrap = false;
        let height = self
            .engine
            .prepare(&run, 1.0)
            .map_err(|e| ControlError::Text(e.to_string()))?
            .height;
        run.bounds.y = bounds.y + (bounds.height - height).max(0.0) * 0.5;
        run.bounds.height = height;
        self.scene.text(run);
        if self.rating_error(id).is_some() {
            self.scene
                .rounded_stroke(bounds, clip, 4.0, 1.5, tokens.error);
        } else if self.runtime.focused() == Some(id) {
            self.scene
                .rounded_stroke(bounds, clip, 4.0, 1.5, tokens.focus);
        }
        Ok(true)
    }
}
