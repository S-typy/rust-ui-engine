use crate::{ControlError, Theme};
use rust_desktop_ui_core::{Color, Edges};

/// Optional colors shared by Button, ToggleButton and CheckBox. For CheckBox,
/// background/border describe its indicator; the surrounding row is transparent.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ButtonAppearance {
    pub background: Option<Color>,
    pub foreground: Option<Color>,
    pub border: Option<Color>,
    /// Independent keyboard focus indicator.
    pub accent: Option<Color>,
    pub checkmark: Option<Color>,
}

/// Theme-relative appearance. Normal overrides are followed by checked, then
/// either hovered or pressed (pressed wins), then disabled substitutions.
/// Focus is independent of these palettes. Disabled preserves checked state.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ButtonStyle {
    /// Logical pixels in `(0, 512]`.
    pub font_size: Option<f32>,
    /// Content insets; each logical dimension is in `[0, 16384]`.
    pub padding: Option<Edges>,
    /// Logical pixels in `[0, 16384]`, clamped to the painted shape.
    pub corner_radius: Option<f32>,
    /// Logical pixels in `[0, 16384]`, clamped to the painted shape.
    pub border_width: Option<f32>,
    pub normal: ButtonAppearance,
    pub hovered: ButtonAppearance,
    pub pressed: ButtonAppearance,
    pub checked: ButtonAppearance,
    pub disabled: ButtonAppearance,
}

/// Interaction state independent of appearance. Disabled controls retain their
/// checked value but report no hover, press or focus. A pointer press is visible
/// only while the pointer is over the captured control.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ButtonVisualState {
    pub enabled: bool,
    pub hovered: bool,
    pub pressed: bool,
    pub focused: bool,
    pub checked: bool,
}

impl Default for ButtonVisualState {
    fn default() -> Self {
        Self {
            enabled: true,
            hovered: false,
            pressed: false,
            focused: false,
            checked: false,
        }
    }
}

impl ButtonVisualState {
    pub(crate) fn normalized(mut self) -> Self {
        self.hovered &= self.enabled;
        self.pressed &= self.enabled;
        self.focused &= self.enabled;
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ResolvedButtonStyle {
    pub font_size: f32,
    pub padding: Edges,
    pub corner_radius: f32,
    pub border_width: f32,
    pub background: Color,
    pub foreground: Color,
    pub border: Color,
    pub accent: Color,
    pub checkmark: Color,
    pub focus_visible: bool,
}

impl ButtonStyle {
    /// Validate all overrides before storing any change.
    pub fn validate(&self) -> Result<(), ControlError> {
        if self
            .font_size
            .is_some_and(|value| !value.is_finite() || value <= 0.0 || value > 512.0)
        {
            return Err(ControlError::Invalid(
                "Button font size must be finite and in (0, 512]".into(),
            ));
        }
        let dimension = |value: f32| value.is_finite() && (0.0..=16_384.0).contains(&value);
        if self.corner_radius.is_some_and(|value| !dimension(value))
            || self.border_width.is_some_and(|value| !dimension(value))
            || self.padding.is_some_and(|padding| {
                [padding.left, padding.top, padding.right, padding.bottom]
                    .into_iter()
                    .any(|value| !dimension(value))
            })
        {
            return Err(ControlError::Invalid(
                "Button padding, corner radius and border width must be finite and in [0, 16384]"
                    .into(),
            ));
        }
        for appearance in [
            self.normal,
            self.hovered,
            self.pressed,
            self.checked,
            self.disabled,
        ] {
            for color in [
                appearance.background,
                appearance.foreground,
                appearance.border,
                appearance.accent,
                appearance.checkmark,
            ]
            .into_iter()
            .flatten()
            {
                if [color.r, color.g, color.b, color.a]
                    .into_iter()
                    .any(|value| !value.is_finite() || !(0.0..=1.0).contains(&value))
                {
                    return Err(ControlError::Invalid(
                        "Button color channels must be finite and in [0, 1]".into(),
                    ));
                }
            }
        }
        Ok(())
    }

    pub(crate) fn resolve(
        self,
        theme: Theme,
        state: ButtonVisualState,
        checkbox: bool,
    ) -> ResolvedButtonStyle {
        let state = state.normalized();
        let tokens = theme.tokens();
        let mut result = ResolvedButtonStyle {
            font_size: self.font_size.unwrap_or(tokens.font_size),
            padding: self.padding.unwrap_or(Edges {
                left: 8.0,
                right: 8.0,
                top: 5.0,
                bottom: 5.0,
            }),
            corner_radius: self
                .corner_radius
                .unwrap_or(if theme.is_fluent() { 4.0 } else { 0.0 }),
            border_width: self.border_width.unwrap_or(1.0),
            background: tokens.surface,
            foreground: tokens.text,
            border: tokens.border,
            accent: tokens.focus,
            checkmark: if matches!(theme, Theme::Dark | Theme::FluentDark) {
                tokens.canvas
            } else {
                Color::rgb(255, 255, 255)
            },
            focus_visible: state.focused,
        };
        result.apply(self.normal);
        if state.checked {
            result.background = if checkbox {
                tokens.focus
            } else {
                tokens.selected
            };
            result.apply(self.checked);
        }
        if state.pressed {
            if !(checkbox && state.checked) {
                result.background = tokens.pressed;
            }
            result.apply(self.pressed);
        } else if state.hovered {
            if !(checkbox && state.checked) {
                result.background = tokens.hover;
            }
            result.apply(self.hovered);
        }
        if !state.enabled {
            result.background = tokens.canvas;
            result.foreground = tokens.muted;
            result.border = tokens.border;
            result.checkmark = tokens.muted;
            result.apply(self.disabled);
        }
        result
    }
}

impl ResolvedButtonStyle {
    fn apply(&mut self, appearance: ButtonAppearance) {
        self.background = appearance.background.unwrap_or(self.background);
        self.foreground = appearance.foreground.unwrap_or(self.foreground);
        self.border = appearance.border.unwrap_or(self.border);
        self.accent = appearance.accent.unwrap_or(self.accent);
        self.checkmark = appearance.checkmark.unwrap_or(self.checkmark);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checkbox_checked_hover_preserves_contrast_and_disabled_suppresses_interactions() {
        for theme in [Theme::FluentLight, Theme::FluentDark] {
            let state = ButtonVisualState {
                checked: true,
                hovered: true,
                pressed: true,
                focused: true,
                ..Default::default()
            };
            let resolved = ButtonStyle::default().resolve(theme, state, true);
            assert_eq!(resolved.background, theme.tokens().focus);
            assert!(resolved.focus_visible);
            let disabled = ButtonVisualState {
                enabled: false,
                ..state
            }
            .normalized();
            assert!(disabled.checked);
            assert!(!disabled.hovered && !disabled.pressed && !disabled.focused);
            let resolved = ButtonStyle::default().resolve(theme, disabled, true);
            assert_eq!(resolved.foreground, theme.tokens().muted);
            assert_eq!(resolved.checkmark, theme.tokens().muted);
            assert!(!resolved.focus_visible);
        }
    }

    #[test]
    fn state_overrides_are_sparse_and_pressed_does_not_inherit_hover() {
        let color = Color::rgb(120, 30, 60);
        let style = ButtonStyle {
            normal: ButtonAppearance {
                foreground: Some(color),
                ..Default::default()
            },
            hovered: ButtonAppearance {
                checkmark: Some(color),
                ..Default::default()
            },
            checked: ButtonAppearance {
                border: Some(color),
                ..Default::default()
            },
            ..Default::default()
        };
        let result = style.resolve(
            Theme::FluentLight,
            ButtonVisualState {
                checked: true,
                hovered: true,
                pressed: true,
                ..Default::default()
            },
            false,
        );
        assert_eq!(result.background, Theme::FluentLight.tokens().pressed);
        assert_eq!(result.foreground, color);
        assert_eq!(result.border, color);
        assert_eq!(result.checkmark, Color::rgb(255, 255, 255));
    }
}
