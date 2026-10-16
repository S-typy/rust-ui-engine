use crate::{ControlError, Theme};
use rust_desktop_ui_core::{Color, Edges};

/// Visual arrangement of a button's content. The full label remains available
/// to commands, accessibility and tooltips in every arrangement.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ButtonContentLayout {
    /// An optional icon followed by a single-line caption.
    #[default]
    Leading,
    /// An icon above a centered caption of at most two lines. Oversized captions
    /// receive a visual ellipsis without changing the semantic label.
    Above,
    /// A centered icon. A missing icon falls back to the full text caption.
    IconOnly,
}

/// A button's surface treatment, independent of its command and checked state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ButtonChrome {
    #[default]
    Standard,
    /// Transparent at rest, with themed hover, press and checked feedback.
    Flat,
    /// Flat tab treatment with an accent underline while checked.
    Tab,
    /// Text-action treatment with an accent foreground and an underline.
    /// Activation still uses the shared command lifecycle; it opens no URL itself.
    Link,
}

/// Optional colors shared by Button, ToggleButton, CheckBox and RadioButton. For indicators,
/// background/border describe the indicator; the surrounding row is transparent.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ButtonAppearance {
    pub background: Option<Color>,
    pub foreground: Option<Color>,
    pub border: Option<Color>,
    /// Independent keyboard focus indicator.
    pub accent: Option<Color>,
    /// CheckBox mark or RadioButton dot.
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
    /// Subtle lower edge for Fluent standard buttons; transparent otherwise.
    pub bottom_border: Color,
    pub accent: Color,
    pub checkmark: Color,
    pub focus_visible: bool,
}

impl ButtonStyle {
    pub(crate) fn resolve_chrome(
        mut self,
        theme: Theme,
        state: ButtonVisualState,
        chrome: ButtonChrome,
    ) -> ResolvedButtonStyle {
        if chrome != ButtonChrome::Standard {
            let colors = theme.chrome();
            let transparent = Color {
                a: 0.0,
                ..theme.tokens().surface
            };
            self.normal.background.get_or_insert(transparent);
            self.disabled.background.get_or_insert(transparent);
            self.border_width.get_or_insert(0.0);
            if theme.is_fluent() {
                self.hovered.background.get_or_insert(
                    if state.checked && chrome == ButtonChrome::Flat {
                        colors.checked_hover
                    } else {
                        colors.subtle_hover
                    },
                );
                self.pressed.background.get_or_insert(
                    if state.checked && chrome == ButtonChrome::Flat {
                        colors.checked_pressed
                    } else {
                        colors.subtle_pressed
                    },
                );
            }
            if chrome == ButtonChrome::Tab {
                self.corner_radius.get_or_insert(0.0);
                self.checked.background.get_or_insert(transparent);
                self.checked.foreground.get_or_insert(if theme.is_fluent() {
                    theme.tokens().text
                } else {
                    theme.tokens().focus
                });
            }
            if chrome == ButtonChrome::Link {
                self.normal.foreground.get_or_insert(theme.tokens().focus);
                self.hovered.foreground.get_or_insert(theme.tokens().focus);
                self.pressed.foreground.get_or_insert(theme.tokens().focus);
                self.checked.foreground.get_or_insert(theme.tokens().focus);
            }
        }
        let mut resolved = self.resolve(theme, state, false);
        if chrome != ButtonChrome::Standard {
            resolved.bottom_border.a = 0.0;
        }
        resolved
    }

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
        let colors = theme.chrome();
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
            border_width: self.border_width.unwrap_or(if theme.is_high_contrast() {
                2.0
            } else {
                1.0
            }),
            background: colors.control_fill,
            foreground: tokens.text,
            border: colors.control_border,
            bottom_border: colors.control_bottom_border,
            accent: tokens.focus,
            checkmark: theme.on_accent(),
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
                result.background = if state.checked {
                    colors.checked_pressed
                } else {
                    colors.control_pressed
                };
            }
            result.apply(self.pressed);
        } else if state.hovered {
            if !(checkbox && state.checked) {
                result.background = if state.checked {
                    colors.checked_hover
                } else {
                    colors.control_hover
                };
            }
            result.apply(self.hovered);
        }
        if !state.enabled {
            result.background = tokens.canvas;
            result.foreground = colors.disabled_text;
            result.border = colors.control_border;
            result.bottom_border = colors.control_border;
            result.checkmark = colors.disabled_text;
            result.apply(self.disabled);
        }
        if !theme.is_fluent() || checkbox {
            result.bottom_border.a = 0.0;
        }
        result
    }
}

impl ResolvedButtonStyle {
    fn apply(&mut self, appearance: ButtonAppearance) {
        self.background = appearance.background.unwrap_or(self.background);
        self.foreground = appearance.foreground.unwrap_or(self.foreground);
        self.border = appearance.border.unwrap_or(self.border);
        self.bottom_border = appearance.border.unwrap_or(self.bottom_border);
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
            assert_eq!(resolved.foreground, theme.chrome().disabled_text);
            assert_eq!(resolved.checkmark, theme.chrome().disabled_text);
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
        assert_eq!(
            result.background,
            Theme::FluentLight.chrome().checked_pressed
        );
        assert_eq!(result.foreground, color);
        assert_eq!(result.border, color);
        assert_eq!(result.bottom_border, color);
        assert_eq!(result.checkmark, Color::rgb(255, 255, 255));
    }

    #[test]
    fn fluent_checked_hover_and_press_keep_selection_visible() {
        for theme in [Theme::FluentLight, Theme::FluentDark] {
            for chrome in [ButtonChrome::Standard, ButtonChrome::Flat] {
                for (hovered, pressed) in [(false, false), (true, false), (true, true)] {
                    let state = ButtonVisualState {
                        hovered,
                        pressed,
                        ..Default::default()
                    };
                    let plain = ButtonStyle::default().resolve_chrome(theme, state, chrome);
                    let checked = ButtonStyle::default().resolve_chrome(
                        theme,
                        ButtonVisualState {
                            checked: true,
                            ..state
                        },
                        chrome,
                    );
                    // A checked command keeps a blue-tinted fill rather than
                    // becoming visually indistinguishable from an ordinary hover.
                    assert_ne!(checked.background, plain.background);
                    assert!(checked.background.b > checked.background.r);
                    assert_eq!(checked.background.a, 1.0);
                    assert_eq!(checked.foreground, theme.tokens().text);
                    assert!(!checked.focus_visible);
                }
            }
        }
    }

    #[test]
    fn fluent_chrome_limits_lower_edge_to_standard_buttons() {
        for theme in [Theme::FluentLight, Theme::FluentDark] {
            let standard = ButtonStyle::default().resolve_chrome(
                theme,
                ButtonVisualState::default(),
                ButtonChrome::Standard,
            );
            assert!(standard.bottom_border.a > 0.0);
            assert_ne!(standard.bottom_border, standard.border);
            for chrome in [ButtonChrome::Flat, ButtonChrome::Tab] {
                let resolved = ButtonStyle::default().resolve_chrome(
                    theme,
                    ButtonVisualState {
                        checked: true,
                        ..Default::default()
                    },
                    chrome,
                );
                assert_eq!(resolved.bottom_border.a, 0.0);
                if chrome == ButtonChrome::Tab {
                    assert_eq!(resolved.background.a, 0.0);
                    assert_eq!(resolved.foreground, theme.tokens().text);
                }
            }
        }
        for theme in [Theme::Light, Theme::Dark, Theme::Compact] {
            let style = ButtonStyle::default().resolve(theme, ButtonVisualState::default(), false);
            assert_eq!(style.bottom_border.a, 0.0);
            assert_eq!(style.background, theme.tokens().surface);
        }
    }

    #[test]
    fn explicit_state_brushes_override_fluent_chrome_and_both_border_edges() {
        let normal = Color::rgb(90, 40, 110);
        let pressed = Color::rgb(40, 100, 80);
        let transparent = Color { a: 0.0, ..normal };
        for theme in [Theme::FluentLight, Theme::FluentDark] {
            let style = ButtonStyle {
                normal: ButtonAppearance {
                    border: Some(normal),
                    ..Default::default()
                },
                checked: ButtonAppearance {
                    background: Some(normal),
                    ..Default::default()
                },
                pressed: ButtonAppearance {
                    background: Some(pressed),
                    border: Some(transparent),
                    ..Default::default()
                },
                ..Default::default()
            };
            let checked = style.resolve_chrome(
                theme,
                ButtonVisualState {
                    checked: true,
                    ..Default::default()
                },
                ButtonChrome::Standard,
            );
            assert_eq!(checked.background, normal);
            assert_eq!(checked.border, normal);
            assert_eq!(checked.bottom_border, normal);
            for chrome in [
                ButtonChrome::Standard,
                ButtonChrome::Flat,
                ButtonChrome::Tab,
            ] {
                let result = style.resolve_chrome(
                    theme,
                    ButtonVisualState {
                        checked: true,
                        hovered: true,
                        pressed: true,
                        ..Default::default()
                    },
                    chrome,
                );
                assert_eq!(result.background, pressed);
                assert_eq!(result.border.a, 0.0);
                assert_eq!(result.bottom_border.a, 0.0);
            }
        }
    }
}
