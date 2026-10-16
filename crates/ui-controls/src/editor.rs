use crate::{ControlError, Theme};
use rust_desktop_ui_core::{Color, Edges, WidgetId};
use std::{fmt, rc::Rc};

/// Maximum UTF-8 bytes retained in a validation diagnostic.
pub const MAX_VALIDATION_MESSAGE_BYTES: usize = 4096;

/// Reusable editor behavior and placeholder. Applying settings does not replace
/// the document, its selection, or its edit history.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditorSettings {
    /// Allow selection and copy while preventing user text changes.
    pub is_read_only: bool,
    /// Maximum extended graphemes in committed text. `Some(0)` permits only
    /// empty text; `None` leaves the independent byte limit in effect.
    pub max_length: Option<usize>,
    /// Presentation only; this is neither the value nor the accessible name.
    pub placeholder: String,
    /// Attempt to accept the draft when keyboard focus leaves this editor.
    /// On by default; applications may opt into explicit acceptance only.
    pub commit_on_focus_lost: bool,
}

impl Default for EditorSettings {
    fn default() -> Self {
        Self {
            is_read_only: false,
            max_length: None,
            placeholder: String::new(),
            commit_on_focus_lost: true,
        }
    }
}

/// Built-in validation, applied to the editable text when accepting a value.
/// Input is not trimmed or otherwise changed by validation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditorValidation {
    /// Require at least one character that Rust's Unicode `trim` retains.
    pub is_required: bool,
    /// Required-value diagnostic. Empty or whitespace-only uses the default.
    pub message: String,
}

impl Default for EditorValidation {
    fn default() -> Self {
        Self {
            is_required: false,
            message: "A value is required.".into(),
        }
    }
}

impl EditorValidation {
    /// Reject oversized diagnostics before applying a validation policy.
    pub fn validate(&self) -> Result<(), ControlError> {
        if self.message.len() > MAX_VALIDATION_MESSAGE_BYTES {
            return Err(ControlError::Invalid(format!(
                "Validation message exceeds {MAX_VALIDATION_MESSAGE_BYTES} UTF-8 bytes"
            )));
        }
        Ok(())
    }

    pub(crate) fn check(&self, text: &str) -> Result<(), String> {
        if self.is_required && text.trim().is_empty() {
            let message = if self.message.trim().is_empty() {
                "A value is required."
            } else {
                &self.message
            };
            Err(message[..message.floor_char_boundary(MAX_VALIDATION_MESSAGE_BYTES)].to_owned())
        } else {
            Ok(())
        }
    }
}

type ValidatorCallback = dyn Fn(&str) -> Result<(), String>;

/// A reusable synchronous value validator. Runs only when requested by the
/// editor's validation/acceptance API, not for transient IME composition.
///
/// The callback should be short and free of side effects. Clones share the
/// callback; this UI-thread value does not require `Send` or `Sync`.
#[derive(Clone)]
pub struct EditorValidator(Rc<ValidatorCallback>);

impl EditorValidator {
    pub fn new(validator: impl Fn(&str) -> Result<(), String> + 'static) -> Self {
        Self(Rc::new(validator))
    }

    pub(crate) fn check(&self, text: &str) -> Result<(), String> {
        (self.0)(text).map_err(|mut message| {
            let oversized = message.len() > MAX_VALIDATION_MESSAGE_BYTES;
            message.truncate(message.floor_char_boundary(MAX_VALIDATION_MESSAGE_BYTES));
            if message.trim().is_empty() {
                return "The value is invalid.".to_owned();
            }
            if oversized {
                message.shrink_to_fit();
            }
            message
        })
    }
}

impl fmt::Debug for EditorValidator {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("EditorValidator(..)")
    }
}

/// Acceptance is distinct from changing the draft or completing IME input.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CommitOutcome {
    Committed,
    Unchanged,
    Invalid(String),
    CompositionActive,
}

/// Value-transaction events. `TextChanged` separately reports draft mutations.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EditEvent {
    Committed { id: WidgetId, value: String },
    Canceled { id: WidgetId },
    ValidationFailed { id: WidgetId, message: String },
}

/// Optional colors for an editor state. Unspecified colors inherit the normal
/// appearance after the theme's state-specific substitutions.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct EditBoxAppearance {
    pub background: Option<Color>,
    pub foreground: Option<Color>,
    pub placeholder: Option<Color>,
    pub border: Option<Color>,
    /// Focus indicator, caret and composition underline.
    pub accent: Option<Color>,
    pub selection: Option<Color>,
}

/// Theme-relative editor appearance. Defaults follow the current theme;
/// explicit overrides persist when the theme changes.
///
/// Colors resolve from theme defaults, then `normal` overrides, then the
/// theme's state substitutions and the selected state's overrides. Hover
/// substitutes the background, read-only substitutes the background, and
/// disabled substitutes the background, foreground and placeholder. Focused
/// read-only editors use `read_only` colors with a visible focus indicator.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct EditBoxStyle {
    /// Logical pixel size in `(0, 512]`.
    pub font_size: Option<f32>,
    /// Content insets in logical pixels, each in `[0, 16384]`.
    pub padding: Option<Edges>,
    /// Logical pixels in `[0, 16384]`, clamped to the painted rectangle.
    pub corner_radius: Option<f32>,
    /// Logical pixels in `[0, 16384]`, clamped to the painted rectangle.
    pub border_width: Option<f32>,
    pub normal: EditBoxAppearance,
    pub hovered: EditBoxAppearance,
    pub focused: EditBoxAppearance,
    pub read_only: EditBoxAppearance,
    pub disabled: EditBoxAppearance,
    /// Applied over the resolved enabled state when validation fails. The
    /// default changes border and accent to the theme's error color.
    pub invalid: EditBoxAppearance,
}

/// Resolved interaction state. Disabled takes precedence over focus and hover;
/// read-only preserves its appearance while still allowing focus.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EditBoxVisualState {
    #[default]
    Normal,
    Hovered,
    Focused,
    ReadOnly,
    ReadOnlyFocused,
    Disabled,
}

impl EditBoxVisualState {
    pub(crate) fn from_flags(enabled: bool, read_only: bool, focused: bool, hovered: bool) -> Self {
        match (enabled, read_only, focused, hovered) {
            (false, _, _, _) => Self::Disabled,
            (true, true, true, _) => Self::ReadOnlyFocused,
            (true, true, false, _) => Self::ReadOnly,
            (true, false, true, _) => Self::Focused,
            (true, false, false, true) => Self::Hovered,
            (true, false, false, false) => Self::Normal,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ResolvedEditBoxStyle {
    pub font_size: f32,
    pub padding: Edges,
    pub corner_radius: f32,
    pub border_width: f32,
    pub background: Color,
    pub foreground: Color,
    pub placeholder: Color,
    pub border: Color,
    pub accent: Color,
    pub selection: Color,
    pub focus_visible: bool,
}

impl EditBoxStyle {
    /// Validate every override before a style is stored or rendered.
    pub fn validate(&self) -> Result<(), ControlError> {
        if self
            .font_size
            .is_some_and(|size| !size.is_finite() || size <= 0.0 || size > 512.0)
        {
            return Err(ControlError::Invalid(
                "Editor font size must be finite and in (0, 512]".into(),
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
                "Editor padding, corner radius and border width must be finite and in [0, 16384]"
                    .into(),
            ));
        }
        for appearance in [
            self.normal,
            self.hovered,
            self.focused,
            self.read_only,
            self.disabled,
            self.invalid,
        ] {
            for color in [
                appearance.background,
                appearance.foreground,
                appearance.placeholder,
                appearance.border,
                appearance.accent,
                appearance.selection,
            ]
            .into_iter()
            .flatten()
            {
                if [color.r, color.g, color.b, color.a]
                    .into_iter()
                    .any(|channel| !channel.is_finite() || !(0.0..=1.0).contains(&channel))
                {
                    return Err(ControlError::Invalid(
                        "Editor color channels must be finite and in [0, 1]".into(),
                    ));
                }
            }
        }
        Ok(())
    }

    pub(crate) fn resolve(self, theme: Theme, state: EditBoxVisualState) -> ResolvedEditBoxStyle {
        let tokens = theme.tokens();
        let colors = theme.chrome();
        let horizontal_padding = if theme.is_fluent() { 12.0 } else { 8.0 };
        let mut resolved = ResolvedEditBoxStyle {
            font_size: self.font_size.unwrap_or(tokens.font_size),
            padding: self.padding.unwrap_or(Edges {
                left: horizontal_padding,
                right: horizontal_padding,
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
            background: colors.editor_fill,
            foreground: tokens.text,
            placeholder: tokens.muted,
            border: colors.control_border,
            accent: tokens.focus,
            selection: tokens.selected,
            focus_visible: matches!(
                state,
                EditBoxVisualState::Focused | EditBoxVisualState::ReadOnlyFocused
            ),
        };
        resolved.apply(self.normal);
        let appearance = match state {
            EditBoxVisualState::Normal => EditBoxAppearance::default(),
            EditBoxVisualState::Hovered => {
                resolved.background = colors.editor_hover;
                self.hovered
            }
            EditBoxVisualState::Focused => self.focused,
            EditBoxVisualState::ReadOnly | EditBoxVisualState::ReadOnlyFocused => {
                resolved.background = colors.editor_read_only;
                self.read_only
            }
            EditBoxVisualState::Disabled => {
                resolved.background = colors.editor_read_only;
                resolved.foreground = colors.disabled_text;
                resolved.placeholder = colors.disabled_text;
                self.disabled
            }
        };
        resolved.apply(appearance);
        resolved
    }
}

impl ResolvedEditBoxStyle {
    pub(crate) fn apply_invalid(&mut self, theme: Theme, appearance: EditBoxAppearance) {
        self.border = theme.tokens().error;
        self.accent = theme.tokens().error;
        self.apply(appearance);
    }

    fn apply(&mut self, appearance: EditBoxAppearance) {
        self.background = appearance.background.unwrap_or(self.background);
        self.foreground = appearance.foreground.unwrap_or(self.foreground);
        self.placeholder = appearance.placeholder.unwrap_or(self.placeholder);
        self.border = appearance.border.unwrap_or(self.border);
        self.accent = appearance.accent.unwrap_or(self.accent);
        self.selection = appearance.selection.unwrap_or(self.selection);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn required_validation_checks_unicode_whitespace_without_altering_values() {
        assert!(EditorSettings::default().commit_on_focus_lost);
        assert!(EditorValidation::default().check("").is_ok());
        let validation = EditorValidation {
            is_required: true,
            message: "Введите имя".into(),
        };
        for text in ["", " \t\n", "\u{00a0}\u{2003}\u{3000}"] {
            assert_eq!(validation.check(text), Err("Введите имя".into()));
        }
        for text in ["  Ada  ", "\u{00a0}Ж\u{3000}", "👩‍💻", "\u{200b}"] {
            assert!(validation.check(text).is_ok());
        }
        let validation = EditorValidation {
            is_required: true,
            message: String::new(),
        };
        validation.validate().unwrap();
        assert_eq!(validation.check(""), Err("A value is required.".into()));
        let validation = EditorValidation {
            message: " \t\u{3000}".into(),
            ..validation
        };
        assert_eq!(validation.check(""), Err("A value is required.".into()));
    }

    #[test]
    fn validation_configuration_uses_a_byte_cap_and_rejects_oversized_messages() {
        let mut validation = EditorValidation {
            is_required: true,
            message: "😀".repeat(MAX_VALIDATION_MESSAGE_BYTES / 4),
        };
        validation.validate().unwrap();
        assert_eq!(
            validation.check("").unwrap_err().len(),
            MAX_VALIDATION_MESSAGE_BYTES
        );
        validation.message.push('a');
        assert!(validation.validate().is_err());
        // The internal check is also defensive against an unvalidated policy.
        assert_eq!(
            validation.check("").unwrap_err().len(),
            MAX_VALIDATION_MESSAGE_BYTES
        );
    }

    #[test]
    fn callbacks_share_configuration_and_bound_errors_at_utf8_boundaries() {
        let expected_prefix = "a".repeat(MAX_VALIDATION_MESSAGE_BYTES - 1);
        let callback_message = format!("{expected_prefix}😀suffix");
        let validator = EditorValidator::new(move |text| match text {
            "valid" => Ok(()),
            "empty" => Err(String::new()),
            "whitespace" => Err(" \u{2003}\n".into()),
            _ => Err(callback_message.clone()),
        });
        let cloned = validator.clone();
        assert!(cloned.check("valid").is_ok());
        assert_eq!(cloned.check("empty"), Err("The value is invalid.".into()));
        assert_eq!(
            cloned.check("whitespace"),
            Err("The value is invalid.".into())
        );
        assert_eq!(cloned.check("other"), Err(expected_prefix.clone()));
        assert_eq!(validator.check("other"), Err(expected_prefix));
        // Equality with the cap is retained without truncating the final scalar.
        let validator = EditorValidator::new(|_| Err("😀".repeat(1024)));
        assert_eq!(
            validator.check("").unwrap_err().len(),
            MAX_VALIDATION_MESSAGE_BYTES
        );
        let validator = EditorValidator::new(|_| {
            Err(format!(
                "{}late diagnostic",
                " ".repeat(MAX_VALIDATION_MESSAGE_BYTES)
            ))
        });
        assert_eq!(validator.check(""), Err("The value is invalid.".into()));
    }

    #[test]
    fn invalid_overlay_preserves_focus_metrics_and_accepts_explicit_colors() {
        for theme in [
            Theme::Light,
            Theme::Dark,
            Theme::Compact,
            Theme::FluentLight,
            Theme::FluentDark,
        ] {
            for state in [
                EditBoxVisualState::Normal,
                EditBoxVisualState::Focused,
                EditBoxVisualState::ReadOnlyFocused,
            ] {
                let style = EditBoxStyle {
                    font_size: Some(18.0),
                    padding: Some(Edges::all(7.0)),
                    ..Default::default()
                };
                let before = style.resolve(theme, state);
                let mut invalid = before;
                invalid.apply_invalid(theme, style.invalid);
                assert_eq!(invalid.border, theme.tokens().error);
                assert_eq!(invalid.accent, theme.tokens().error);
                assert_eq!(invalid.background, before.background);
                assert_eq!(invalid.foreground, before.foreground);
                assert_eq!(invalid.focus_visible, before.focus_visible);
                assert_eq!(invalid.font_size, before.font_size);
                assert_eq!(invalid.padding, before.padding);
                let color = Color::rgb(80, 40, 120);
                invalid.apply_invalid(
                    theme,
                    EditBoxAppearance {
                        background: Some(color),
                        border: Some(color),
                        accent: Some(color),
                        ..Default::default()
                    },
                );
                assert_eq!(invalid.background, color);
                assert_eq!(invalid.border, color);
                assert_eq!(invalid.accent, color);
                assert_eq!(invalid.foreground, before.foreground);
            }
        }
    }

    #[test]
    fn visual_state_prioritizes_disabled_and_keeps_readonly_focus() {
        for read_only in [false, true] {
            for focused in [false, true] {
                for hovered in [false, true] {
                    assert_eq!(
                        EditBoxVisualState::from_flags(false, read_only, focused, hovered),
                        EditBoxVisualState::Disabled
                    );
                }
            }
        }
        assert_eq!(
            EditBoxVisualState::from_flags(true, true, true, true),
            EditBoxVisualState::ReadOnlyFocused
        );
        assert_eq!(
            EditBoxVisualState::from_flags(true, true, false, true),
            EditBoxVisualState::ReadOnly
        );
        assert_eq!(
            EditBoxVisualState::from_flags(true, false, true, true),
            EditBoxVisualState::Focused
        );
        assert_eq!(
            EditBoxVisualState::from_flags(true, false, false, true),
            EditBoxVisualState::Hovered
        );
        assert_eq!(
            EditBoxVisualState::from_flags(true, false, false, false),
            EditBoxVisualState::Normal
        );
    }

    #[test]
    fn default_style_tracks_theme_and_explicit_metrics_remain() {
        let style = EditBoxStyle::default();
        for theme in [
            Theme::Light,
            Theme::Dark,
            Theme::Compact,
            Theme::FluentLight,
            Theme::FluentDark,
        ] {
            let normal = style.resolve(theme, EditBoxVisualState::Normal);
            assert_eq!(normal.background, theme.chrome().editor_fill);
            assert_eq!(normal.font_size, theme.tokens().font_size);
            assert_eq!(normal.foreground, theme.tokens().text);
            assert_eq!(normal.accent, theme.tokens().focus);
            assert_eq!(
                normal.corner_radius,
                if theme.is_fluent() { 4.0 } else { 0.0 }
            );
            assert!(!normal.focus_visible);
        }
        let style = EditBoxStyle {
            font_size: Some(19.0),
            padding: Some(Edges::all(9.0)),
            corner_radius: Some(8.0),
            border_width: Some(3.0),
            ..Default::default()
        };
        for theme in [Theme::Light, Theme::FluentDark] {
            let resolved = style.resolve(theme, EditBoxVisualState::Focused);
            assert_eq!(resolved.font_size, 19.0);
            assert_eq!(resolved.padding, Edges::all(9.0));
            assert_eq!(resolved.corner_radius, 8.0);
            assert_eq!(resolved.border_width, 3.0);
            assert!(resolved.focus_visible);
        }
    }

    #[test]
    fn state_colors_inherit_normal_with_explicit_theme_substitutions() {
        let foreground = Color::rgb(60, 70, 80);
        let accent = Color::rgb(20, 100, 200);
        let background = Color::rgb(240, 220, 180);
        let hover_border = Color::rgb(100, 50, 70);
        let focused_background = Color::rgb(220, 240, 250);
        let style = EditBoxStyle {
            normal: EditBoxAppearance {
                foreground: Some(foreground),
                background: Some(background),
                accent: Some(accent),
                ..Default::default()
            },
            hovered: EditBoxAppearance {
                border: Some(hover_border),
                ..Default::default()
            },
            focused: EditBoxAppearance {
                background: Some(focused_background),
                ..Default::default()
            },
            ..Default::default()
        };
        let theme = Theme::FluentLight;
        let normal = style.resolve(theme, EditBoxVisualState::Normal);
        assert_eq!(normal.background, background);
        let hovered = style.resolve(theme, EditBoxVisualState::Hovered);
        assert_eq!(hovered.background, theme.chrome().editor_hover);
        assert_eq!(hovered.border, hover_border);
        assert_eq!(hovered.foreground, foreground);
        let focused = style.resolve(theme, EditBoxVisualState::Focused);
        assert_eq!(focused.background, focused_background);
        assert_eq!(focused.border, theme.chrome().control_border);
        assert_eq!(focused.accent, accent);
        let read_only = style.resolve(theme, EditBoxVisualState::ReadOnly);
        let read_only_focused = style.resolve(theme, EditBoxVisualState::ReadOnlyFocused);
        assert_eq!(read_only.background, theme.chrome().editor_read_only);
        assert_eq!(read_only.foreground, foreground);
        assert_eq!(read_only_focused.background, read_only.background);
        assert_eq!(read_only_focused.accent, accent);
        assert!(read_only_focused.focus_visible);
        assert!(!read_only.focus_visible);
        let disabled = style.resolve(theme, EditBoxVisualState::Disabled);
        assert_eq!(disabled.background, theme.chrome().editor_read_only);
        assert_eq!(disabled.foreground, theme.chrome().disabled_text);
        assert_eq!(disabled.placeholder, theme.chrome().disabled_text);
        assert!(!disabled.focus_visible);
    }

    #[test]
    fn state_overrides_apply_after_disabled_and_readonly_substitutions() {
        let background = Color::rgb(70, 60, 50);
        let foreground = Color::rgb(190, 180, 170);
        let style = EditBoxStyle {
            read_only: EditBoxAppearance {
                background: Some(background),
                ..Default::default()
            },
            disabled: EditBoxAppearance {
                foreground: Some(foreground),
                ..Default::default()
            },
            ..Default::default()
        };
        for state in [
            EditBoxVisualState::ReadOnly,
            EditBoxVisualState::ReadOnlyFocused,
        ] {
            assert_eq!(
                style.resolve(Theme::FluentDark, state).background,
                background
            );
        }
        assert_eq!(
            style
                .resolve(Theme::FluentDark, EditBoxVisualState::Disabled)
                .foreground,
            foreground
        );
    }

    #[test]
    fn style_validation_accepts_bounds_and_rejects_invalid_metrics() {
        for size in [f32::MIN_POSITIVE, 512.0] {
            EditBoxStyle {
                font_size: Some(size),
                ..Default::default()
            }
            .validate()
            .unwrap();
        }
        for size in [0.0, -1.0, 512.1, f32::INFINITY, f32::NAN] {
            assert!(
                EditBoxStyle {
                    font_size: Some(size),
                    ..Default::default()
                }
                .validate()
                .is_err()
            );
        }
        for metric in [0.0, 16_384.0] {
            EditBoxStyle {
                padding: Some(Edges::all(metric)),
                corner_radius: Some(metric),
                border_width: Some(metric),
                ..Default::default()
            }
            .validate()
            .unwrap();
        }
        for metric in [-0.1, 16_385.0, f32::INFINITY, f32::NAN] {
            for style in [
                EditBoxStyle {
                    padding: Some(Edges {
                        bottom: metric,
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                EditBoxStyle {
                    corner_radius: Some(metric),
                    ..Default::default()
                },
                EditBoxStyle {
                    border_width: Some(metric),
                    ..Default::default()
                },
            ] {
                assert!(style.validate().is_err());
            }
        }
    }

    #[test]
    fn style_validation_checks_all_state_colors_and_channels() {
        for bad_channel in [-0.01, 1.01, f32::INFINITY, f32::NAN] {
            for channel in 0..4 {
                let mut components = [0.5; 4];
                components[channel] = bad_channel;
                let color = Color {
                    r: components[0],
                    g: components[1],
                    b: components[2],
                    a: components[3],
                };
                for field in 0..6 {
                    let mut appearance = EditBoxAppearance::default();
                    *match field {
                        0 => &mut appearance.background,
                        1 => &mut appearance.foreground,
                        2 => &mut appearance.placeholder,
                        3 => &mut appearance.border,
                        4 => &mut appearance.accent,
                        _ => &mut appearance.selection,
                    } = Some(color);
                    for state in 0..6 {
                        let mut style = EditBoxStyle::default();
                        *match state {
                            0 => &mut style.normal,
                            1 => &mut style.hovered,
                            2 => &mut style.focused,
                            3 => &mut style.read_only,
                            4 => &mut style.disabled,
                            _ => &mut style.invalid,
                        } = appearance;
                        assert!(style.validate().is_err());
                    }
                }
            }
        }
        EditBoxStyle {
            normal: EditBoxAppearance {
                background: Some(Color {
                    r: 0.0,
                    g: 1.0,
                    b: 0.5,
                    a: 0.0,
                }),
                ..Default::default()
            },
            ..Default::default()
        }
        .validate()
        .unwrap();
    }
}
