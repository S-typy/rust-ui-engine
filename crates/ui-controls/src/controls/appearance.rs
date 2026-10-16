use super::*;

/// Semantic text color that follows the current theme.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TextTone {
    #[default]
    Primary,
    Secondary,
}

/// Typography for a non-editable label. Explicit colors survive theme changes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextStyle {
    pub font_size: Option<f32>,
    pub font_weight: Option<u16>,
    pub foreground: Option<Color>,
    pub tone: TextTone,
    pub wrap: bool,
}

impl Default for TextStyle {
    fn default() -> Self {
        Self {
            font_size: None,
            font_weight: None,
            foreground: None,
            tone: TextTone::Primary,
            wrap: true,
        }
    }
}

impl TextStyle {
    pub fn validate(self) -> Result<(), ControlError> {
        if self
            .font_size
            .is_some_and(|size| !size.is_finite() || size <= 0.0 || size > 512.0)
            || self
                .font_weight
                .is_some_and(|weight| !(1..=1000).contains(&weight))
        {
            return Err(ControlError::Invalid(
                "Text size must be in (0, 512] and weight in [1, 1000]".into(),
            ));
        }
        if self.foreground.is_some_and(|color| {
            [color.r, color.g, color.b, color.a]
                .into_iter()
                .any(|value| !value.is_finite() || !(0.0..=1.0).contains(&value))
        }) {
            return Err(ControlError::Invalid(
                "Text color channels must be finite and in [0, 1]".into(),
            ));
        }
        Ok(())
    }

    pub(super) fn color(self, theme: Theme, enabled: bool) -> Color {
        if !enabled {
            theme.chrome().disabled_text
        } else {
            self.foreground.unwrap_or(match self.tone {
                TextTone::Primary => theme.tokens().text,
                TextTone::Secondary => theme.tokens().muted,
            })
        }
    }
}

/// Theme-relative decoration of a layout panel; it adds no semantic node.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SurfaceKind {
    #[default]
    Transparent,
    Card,
}

impl Controls {
    fn require_label(&self, id: WidgetId) -> Result<(), ControlError> {
        if self
            .controls
            .get(&id)
            .is_some_and(|control| matches!(control.kind, Kind::Label))
            && self.runtime.tree().node(id).is_some()
        {
            Ok(())
        } else {
            Err(ControlError::Invalid(
                "Text style requires a live label".into(),
            ))
        }
    }

    pub fn set_text_style(&mut self, id: WidgetId, style: TextStyle) -> Result<(), ControlError> {
        self.require_label(id)?;
        style.validate()?;
        let control = self.controls.get_mut(&id).unwrap();
        if control.text_style != style {
            control.text_style = style;
            self.scene_dirty = true;
        }
        Ok(())
    }

    pub fn text_style(&self, id: WidgetId) -> Option<TextStyle> {
        self.require_label(id).ok()?;
        Some(self.controls[&id].text_style)
    }

    /// Opt into height measured from the current label, typography and width.
    /// Explicit heights remain application-owned until this is enabled.
    pub fn set_text_auto_height(
        &mut self,
        id: WidgetId,
        enabled: bool,
    ) -> Result<(), ControlError> {
        self.require_label(id)?;
        self.controls.get_mut(&id).unwrap().text_auto_height = enabled;
        self.scene_dirty = true;
        Ok(())
    }

    pub fn set_surface(&mut self, id: WidgetId, surface: SurfaceKind) -> Result<(), ControlError> {
        if self.runtime.tree().node(id).is_none() {
            return Err(ControlError::Invalid(
                "Surface requires a live panel".into(),
            ));
        }
        let control = self
            .controls
            .get_mut(&id)
            .filter(|control| matches!(control.kind, Kind::Panel | Kind::Scroll { .. }))
            .ok_or_else(|| ControlError::Invalid("Surface requires a panel".into()))?;
        control.surface = Some(surface);
        control.transparent_panel = true;
        self.scene_dirty = true;
        self.refresh_paints()
    }

    pub fn surface(&self, id: WidgetId) -> Option<SurfaceKind> {
        self.runtime.tree().node(id)?;
        self.controls.get(&id)?.surface
    }

    /// Measures the exact font size, weight and wrapping used by label painting.
    /// Width excludes the label's eight-pixel left/right content inset.
    pub fn measure_styled_text(
        &mut self,
        text: &str,
        width: f32,
        style: TextStyle,
    ) -> Result<Size, ControlError> {
        style.validate()?;
        if !width.is_finite() || !(0.0..=16_384.0).contains(&width) {
            return Err(ControlError::Invalid(
                "Text measure width must be finite and in [0, 16384]".into(),
            ));
        }
        if text.len() > 1024 * 1024 {
            return Err(ControlError::Text(TextError::TextTooLong.to_string()));
        }
        let prepared = self
            .engine
            .prepare(
                &TextRun {
                    sensitive: false,
                    bounds: Rect::new(0.0, 0.0, width, 0.0),
                    clip: Rect::new(0.0, 0.0, 0.0, 0.0),
                    text: text.to_owned(),
                    font_size: style.font_size.unwrap_or(self.theme.tokens().font_size),
                    color: style.color(self.theme, true),
                    weight: style.font_weight.unwrap_or(400),
                    align: TextAlign::Start,
                    wrap: style.wrap,
                },
                1.0,
            )
            .map_err(|error| ControlError::Text(error.to_string()))?;
        Ok(Size::new(prepared.width, prepared.height))
    }

    pub(super) fn sync_text_heights(&mut self) -> Result<(), ControlError> {
        let labels: Vec<_> = self
            .controls
            .iter()
            .filter(|(_, control)| control.text_auto_height)
            .filter_map(|(&id, control)| {
                self.runtime.bounds(id).map(|bounds| {
                    (
                        id,
                        control.label.clone(),
                        control.text_style,
                        (bounds.width - 16.0).clamp(0.0, 16_384.0),
                    )
                })
            })
            .collect();
        for (id, _, _, width) in labels {
            let height = self.measure_text_display(id, width)?.height.ceil();
            if self
                .runtime
                .tree()
                .node(id)
                .is_some_and(|node| node.props.style.height != Length::Px(height))
            {
                self.edit_props(id, |props| props.style.height = Length::Px(height))?;
            }
        }
        Ok(())
    }
}

pub(super) fn paint_card(scene: &mut Scene, bounds: Rect, clip: Rect, theme: Theme) {
    let chrome = theme.chrome();
    let radius = if theme.is_fluent() { 8.0 } else { 0.0 };
    editor_fill(scene, bounds, clip, radius, chrome.card_surface);
    shape_border(scene, bounds, clip, radius, 1.0, chrome.surface_border);
}

pub(super) fn paint_flyout(scene: &mut Scene, bounds: Rect, viewport: Rect, theme: Theme) {
    let chrome = theme.chrome();
    let radius = if theme.is_fluent() { 8.0 } else { 0.0 };
    if theme.is_fluent() {
        for (spread, offset, opacity) in [(4.0, 3.0, 0.25), (2.0, 2.0, 0.35), (1.0, 1.0, 0.4)] {
            scene.rounded_fill(
                Rect::new(
                    bounds.x - spread,
                    bounds.y + offset - spread,
                    bounds.width + spread * 2.0,
                    bounds.height + spread * 2.0,
                ),
                viewport,
                radius + spread,
                Color {
                    a: chrome.shadow.a * opacity,
                    ..chrome.shadow
                },
            );
        }
    }
    editor_fill(scene, bounds, viewport, radius, chrome.flyout_surface);
    shape_border(scene, bounds, viewport, radius, 1.0, chrome.flyout_border);
}
