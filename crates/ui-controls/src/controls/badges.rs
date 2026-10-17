//! Attached, noninteractive indicators painted after their owner's content.
use super::*;

pub const MAX_BADGES: usize = 1024;
const HEIGHT: f32 = 22.0;
const FONT_SIZE: f32 = 12.0;
const MAX_WIDTH: f32 = 160.0;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BadgeContent {
    Count(u64),
    Text(String),
    Icon(IconSymbol),
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BadgeSeverity {
    #[default]
    Neutral,
    Info,
    Success,
    Warning,
    Error,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BadgeAnchor {
    TopLeft,
    #[default]
    TopRight,
    BottomLeft,
    BottomRight,
    Center,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BadgeClip {
    /// Restrict paint to the owner's border box and its ancestor clip.
    Owner,
    /// Allow paint outside the owner, within the ancestor clip and viewport.
    #[default]
    Parent,
}
#[derive(Clone, Debug, PartialEq)]
pub struct BadgeSettings {
    pub content: BadgeContent,
    pub severity: BadgeSeverity,
    pub anchor: BadgeAnchor,
    pub offset: Point,
    pub visible: bool,
    pub clip: BadgeClip,
    /// Visual count cap; the accessible description always contains the full count.
    pub max_count: u64,
    /// Optional accessible caption. It does not occupy space in the badge.
    pub label: String,
}
impl Default for BadgeSettings {
    fn default() -> Self {
        Self {
            content: BadgeContent::Count(0),
            severity: BadgeSeverity::Neutral,
            anchor: BadgeAnchor::TopRight,
            offset: Point::new(0.0, 0.0),
            visible: true,
            clip: BadgeClip::Parent,
            max_count: 99,
            label: String::new(),
        }
    }
}
impl BadgeSettings {
    pub fn validate(&self) -> Result<(), ControlError> {
        if !self.offset.x.is_finite()
            || !self.offset.y.is_finite()
            || self.offset.x.abs() > 256.0
            || self.offset.y.abs() > 256.0
            || self.max_count == 0
        {
            return Err(invalid(
                "Badge offsets must be finite within ±256 and MaxCount positive",
            ));
        }
        validate_caption(&self.label, true)?;
        if let BadgeContent::Text(text) = &self.content {
            validate_caption(text, false)?;
        }
        Ok(())
    }
    fn display_text(&self) -> String {
        match &self.content {
            BadgeContent::Count(count) if *count > self.max_count => format!("{}+", self.max_count),
            BadgeContent::Count(count) => count.to_string(),
            BadgeContent::Text(text) => text.clone(),
            BadgeContent::Icon(_) => String::new(),
        }
    }
    fn description(&self) -> String {
        let content = match &self.content {
            BadgeContent::Count(count) => count.to_string(),
            BadgeContent::Text(text) => text.clone(),
            BadgeContent::Icon(symbol) => format!("{symbol:?}"),
        };
        let caption = if self.label.is_empty() {
            "Индикатор"
        } else {
            &self.label
        };
        let severity = match self.severity {
            BadgeSeverity::Neutral => "",
            BadgeSeverity::Info => "; информация",
            BadgeSeverity::Success => "; успешно",
            BadgeSeverity::Warning => "; предупреждение",
            BadgeSeverity::Error => "; ошибка",
        };
        format!("{caption}: {content}{severity}")
    }
}
fn invalid(message: &str) -> ControlError {
    ControlError::Invalid(message.into())
}
fn validate_caption(text: &str, allow_empty: bool) -> Result<(), ControlError> {
    if (!allow_empty && text.is_empty()) || text.chars().any(char::is_control) {
        return Err(invalid(
            "Badge text must be nonempty and contain no control characters",
        ));
    }
    TextLimits {
        max_graphemes: Some(64),
        max_bytes: 256,
    }
    .validate(text)
    .map_err(ControlError::Edit)?;
    Ok(())
}
pub(super) struct BadgeState {
    settings: BadgeSettings,
    text: String,
    description: String,
    width: f32,
    text_height: f32,
}

impl Controls {
    /// Atomically attach or replace an indicator without creating a layout,
    /// focus, hit-test or accessibility node. `None` removes the attachment.
    pub fn set_badge(
        &mut self,
        owner: WidgetId,
        settings: Option<BadgeSettings>,
    ) -> Result<(), ControlError> {
        if !self.controls.contains_key(&owner) || !self.runtime.tree().contains(owner) {
            return Err(invalid("Badge owner must be a live control"));
        }
        let Some(settings) = settings else {
            if self.badges.remove(&owner).is_some() {
                self.scene_dirty = true;
            }
            return Ok(());
        };
        settings.validate()?;
        if !self.badges.contains_key(&owner) && self.badges.len() >= MAX_BADGES {
            return Err(invalid("At most 1024 badge attachments are supported"));
        }
        let description = settings.description();
        // Check the final concatenation, including joins at caption and suffix
        // boundaries, before changing any published state.
        let run = |text| {
            let mut run = TextRun::new(
                text,
                Rect::new(0.0, 0.0, 16384.0, 64.0),
                Color::rgb(0, 0, 0),
                FONT_SIZE,
            );
            run.wrap = false;
            run
        };
        self.engine
            .validate_run(&run(description.clone()), 1.0)
            .map_err(|error| ControlError::Text(error.to_string()))?;
        let mut display = run(settings.display_text());
        display.bounds.width = MAX_WIDTH - 12.0;
        display.bounds.height = HEIGHT;
        display.clip = display.bounds;
        display.weight = 600;
        let fitted = self
            .engine
            .fit_text(
                &display,
                &[],
                rust_desktop_ui_core::TextTrimming::CharacterEllipsis,
                Some(1),
            )
            .map_err(|error| ControlError::Text(error.to_string()))?;
        let width = if matches!(settings.content, BadgeContent::Icon(_)) {
            HEIGHT
        } else {
            (fitted.measured.width + 12.0).clamp(HEIGHT, MAX_WIDTH)
        };
        self.badges.insert(
            owner,
            BadgeState {
                settings,
                text: fitted.run.text,
                description,
                width,
                text_height: fitted.measured.height.min(HEIGHT),
            },
        );
        self.scene_dirty = true;
        Ok(())
    }
    pub fn badge(&self, owner: WidgetId) -> Option<&BadgeSettings> {
        self.runtime.tree().node(owner)?;
        Some(&self.badges.get(&owner)?.settings)
    }
    /// Unclipped logical badge rectangle from the last layout, or `None` when
    /// hidden, detached or wholly clipped. This is never a hit-test rectangle.
    pub fn badge_bounds(&self, owner: WidgetId) -> Option<Rect> {
        self.badge_geometry(owner).map(|(bounds, _)| bounds)
    }
    fn badge_geometry(&self, owner: WidgetId) -> Option<(Rect, Rect)> {
        let state = self.badges.get(&owner)?;
        if !state.settings.visible || !self.runtime.tree().is_attached(owner) {
            return None;
        }
        let mut ancestor = Some(owner);
        while let Some(id) = ancestor {
            let props = &self.runtime.tree().node(id)?.props;
            if !props.visible || props.collapsed {
                return None;
            }
            ancestor = self.runtime.tree().parent(id);
        }
        let owner_bounds = self.runtime.bounds(owner)?;
        if owner_bounds.width <= 0.0 || owner_bounds.height <= 0.0 {
            return None;
        }
        let mut clip = self.runtime.clip(owner)?;
        if state.settings.clip == BadgeClip::Owner {
            clip = clip.intersection(owner_bounds)?;
        }
        let (x, y) = match state.settings.anchor {
            BadgeAnchor::TopLeft => (owner_bounds.x, owner_bounds.y),
            BadgeAnchor::TopRight => (owner_bounds.x + owner_bounds.width, owner_bounds.y),
            BadgeAnchor::BottomLeft => (owner_bounds.x, owner_bounds.y + owner_bounds.height),
            BadgeAnchor::BottomRight => (
                owner_bounds.x + owner_bounds.width,
                owner_bounds.y + owner_bounds.height,
            ),
            BadgeAnchor::Center => (
                owner_bounds.x + owner_bounds.width * 0.5,
                owner_bounds.y + owner_bounds.height * 0.5,
            ),
        };
        let bounds = Rect::new(
            x - state.width * 0.5 + state.settings.offset.x,
            y - HEIGHT * 0.5 + state.settings.offset.y,
            state.width,
            HEIGHT,
        );
        bounds.intersection(clip)?;
        Some((bounds, clip))
    }
    pub(super) fn badge_description(&self, owner: WidgetId) -> Option<String> {
        self.badge_geometry(owner)?;
        Some(self.badges.get(&owner)?.description.clone())
    }
    pub(super) fn paint_badge(&mut self, owner: WidgetId) {
        let Some((bounds, clip)) = self.badge_geometry(owner) else {
            return;
        };
        let state = &self.badges[&owner];
        let tokens = self.theme.tokens();
        let (fill, foreground) = if !self.effective_enabled(owner) {
            (tokens.border, self.theme.chrome().disabled_text)
        } else if self.theme.is_high_contrast() {
            (tokens.text, tokens.surface)
        } else {
            match state.settings.severity {
                BadgeSeverity::Neutral => (tokens.text, tokens.surface),
                BadgeSeverity::Info => (Color::rgb(0, 83, 153), Color::rgb(255, 255, 255)),
                BadgeSeverity::Success => (Color::rgb(20, 108, 46), Color::rgb(255, 255, 255)),
                BadgeSeverity::Warning => (Color::rgb(255, 211, 82), Color::rgb(34, 34, 34)),
                BadgeSeverity::Error => (Color::rgb(178, 26, 38), Color::rgb(255, 255, 255)),
            }
        };
        self.scene.rounded_fill(bounds, clip, HEIGHT * 0.5, fill);
        if let BadgeContent::Icon(icon) = state.settings.content {
            crate::primitives::paint_icon(
                &mut self.scene,
                icon,
                Rect::new(bounds.x + 4.0, bounds.y + 4.0, 14.0, 14.0),
                clip,
                foreground,
            );
        } else {
            let mut run = TextRun::new(
                state.text.clone(),
                Rect::new(
                    bounds.x + 6.0,
                    bounds.y + (HEIGHT - state.text_height) * 0.5,
                    (bounds.width - 12.0).max(0.0),
                    state.text_height,
                ),
                foreground,
                FONT_SIZE,
            );
            run.clip = clip;
            run.wrap = false;
            run.weight = 600;
            run.align = TextAlign::Center;
            self.scene.text(run);
        }
    }
}
