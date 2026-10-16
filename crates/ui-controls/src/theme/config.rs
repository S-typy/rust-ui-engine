use super::{ChromeTokens, Theme, ThemeTokens};
use crate::ControlError;
use rust_desktop_ui_core::Color;

/// Palette mode is independent of control density.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ThemeMode {
    #[default]
    Light,
    Dark,
    HighContrastLight,
    HighContrastDark,
}

impl ThemeMode {
    pub fn is_dark(self) -> bool {
        matches!(self, Self::Dark | Self::HighContrastDark)
    }
    pub fn is_high_contrast(self) -> bool {
        matches!(self, Self::HighContrastLight | Self::HighContrastDark)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ThemeDensity {
    #[default]
    Comfortable,
    Compact,
    Touch,
}

/// Opaque sRGB roles. Validation occurs when a palette is installed in a
/// `ThemeConfig`; RGB bytes cannot contain NaN, alpha or out-of-range channels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ThemePalette {
    pub canvas: [u8; 3],
    pub surface: [u8; 3],
    pub text: [u8; 3],
    pub muted: [u8; 3],
    pub border: [u8; 3],
    pub accent: [u8; 3],
    pub error: [u8; 3],
}

impl ThemePalette {
    pub const fn for_mode(mode: ThemeMode) -> Self {
        match mode {
            ThemeMode::Light => Self {
                canvas: [243, 243, 243],
                surface: [255, 255, 255],
                text: [27, 27, 27],
                muted: [105, 105, 105],
                border: [205, 205, 205],
                accent: [0, 95, 184],
                error: [196, 43, 28],
            },
            ThemeMode::Dark => Self {
                canvas: [32, 32, 32],
                surface: [45, 45, 45],
                text: [245, 245, 245],
                muted: [170, 170, 170],
                border: [85, 85, 85],
                accent: [96, 205, 255],
                error: [255, 153, 164],
            },
            ThemeMode::HighContrastLight => Self {
                canvas: [255, 255, 255],
                surface: [255, 255, 255],
                text: [0, 0, 0],
                muted: [48, 48, 48],
                border: [0, 0, 0],
                accent: [0, 0, 160],
                error: [160, 0, 0],
            },
            ThemeMode::HighContrastDark => Self {
                canvas: [0, 0, 0],
                surface: [0, 0, 0],
                text: [255, 255, 255],
                muted: [220, 220, 220],
                border: [255, 255, 255],
                accent: [255, 255, 0],
                error: [255, 160, 160],
            },
        }
    }
}

/// An immutable, validated theme configuration. Creating a replacement never
/// changes a mounted control; pass `Theme::Configured(config)` to `set_theme`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ThemeConfig {
    mode: ThemeMode,
    density: ThemeDensity,
    accent: Option<[u8; 3]>,
    palette: Option<ThemePalette>,
}

impl Default for ThemeConfig {
    fn default() -> Self {
        Self::new(ThemeMode::Light)
    }
}

impl ThemeConfig {
    pub const fn new(mode: ThemeMode) -> Self {
        Self {
            mode,
            density: ThemeDensity::Comfortable,
            accent: None,
            palette: None,
        }
    }
    pub const fn mode(self) -> ThemeMode {
        self.mode
    }
    pub const fn density(self) -> ThemeDensity {
        self.density
    }
    pub const fn with_density(mut self, density: ThemeDensity) -> Self {
        self.density = density;
        self
    }
    /// Set an accent whose contrast is at least 3:1 against the configured
    /// surfaces. Invalid contrast returns an error before a theme can be used.
    pub fn with_accent(mut self, accent: [u8; 3]) -> Result<Self, ControlError> {
        self.accent = Some(accent);
        self.validate()?;
        Ok(self)
    }
    pub fn with_palette(mut self, palette: ThemePalette) -> Result<Self, ControlError> {
        self.palette = Some(palette);
        self.validate()?;
        Ok(self)
    }
    pub fn palette(self) -> ThemePalette {
        let mut palette = self
            .palette
            .unwrap_or_else(|| ThemePalette::for_mode(self.mode));
        if let Some(accent) = self.accent {
            palette.accent = accent;
        }
        palette
    }

    fn validate(self) -> Result<(), ControlError> {
        let tokens = self.tokens();
        let chrome = self.chrome();
        let required_text = if self.mode.is_high_contrast() {
            7.0
        } else {
            4.5
        };
        let surfaces = [
            tokens.canvas,
            tokens.surface,
            tokens.hover,
            tokens.pressed,
            tokens.selected,
            chrome.ribbon_surface,
            chrome.card_surface,
            chrome.control_fill,
            chrome.control_hover,
            chrome.control_pressed,
            chrome.checked_hover,
            chrome.checked_pressed,
            chrome.flyout_surface,
            chrome.editor_fill,
            chrome.editor_hover,
            chrome.editor_read_only,
        ];
        let overlay_surfaces = [
            tokens.canvas,
            chrome.ribbon_surface,
            chrome.card_surface,
            chrome.flyout_surface,
        ]
        .into_iter()
        .flat_map(|background| {
            [
                composite(chrome.subtle_hover, background),
                composite(chrome.subtle_pressed, background),
            ]
        });
        for background in surfaces.into_iter().chain(overlay_surfaces) {
            if contrast(tokens.text, background) < required_text {
                return Err(ControlError::Invalid(format!(
                    "Theme primary text contrast must be at least {required_text}:1 on every surface"
                )));
            }
            if contrast(tokens.focus, background) < 3.0 {
                return Err(ControlError::Invalid(
                    "Theme focus contrast must be at least 3:1 on every surface".into(),
                ));
            }
        }
        for background in [
            tokens.canvas,
            tokens.surface,
            chrome.editor_fill,
            chrome.editor_hover,
            chrome.editor_read_only,
        ] {
            if contrast(tokens.muted, background) < 4.5 {
                return Err(ControlError::Invalid(
                    "Theme muted text contrast must be at least 4.5:1".into(),
                ));
            }
            if contrast(tokens.focus, background) < 3.0 || contrast(tokens.error, background) < 3.0
            {
                return Err(ControlError::Invalid(
                    "Theme accent and error contrast must be at least 3:1".into(),
                ));
            }
            if self.mode.is_high_contrast() && contrast(tokens.border, background) < 3.0 {
                return Err(ControlError::Invalid(
                    "High-contrast theme border contrast must be at least 3:1".into(),
                ));
            }
        }
        Ok(())
    }

    pub(super) fn tokens(self) -> ThemeTokens {
        let palette = self.palette();
        let (height, font, spacing) = match self.density {
            ThemeDensity::Comfortable => (36.0, 14.0, 8.0),
            ThemeDensity::Compact => (28.0, 13.0, 4.0),
            ThemeDensity::Touch => (44.0, 16.0, 12.0),
        };
        let surface = color(palette.surface);
        let text = color(palette.text);
        let accent = color(palette.accent);
        let plain = self.palette.is_none() && !self.mode.is_high_contrast();
        let mut tokens = if plain {
            if self.mode.is_dark() {
                Theme::FluentDark.tokens()
            } else {
                Theme::FluentLight.tokens()
            }
        } else {
            ThemeTokens {
                canvas: color(palette.canvas),
                surface,
                text,
                muted: color(palette.muted),
                border: color(palette.border),
                hover: mix(surface, text, 0.04),
                pressed: mix(surface, text, 0.08),
                selected: selection(surface, text, accent, 0.14, self.mode.is_high_contrast()),
                focus: accent,
                error: color(palette.error),
                control_height: height,
                font_size: font,
                spacing,
            }
        };
        tokens.focus = accent;
        if self.accent.is_some() {
            tokens.selected = selection(surface, text, accent, 0.14, self.mode.is_high_contrast());
        }
        tokens.control_height = height;
        tokens.font_size = font;
        tokens.spacing = spacing;
        tokens
    }

    pub(super) fn chrome(self) -> ChromeTokens {
        let tokens = self.tokens();
        if self.palette.is_none() && !self.mode.is_high_contrast() {
            let mut chrome = if self.mode.is_dark() {
                Theme::FluentDark.chrome()
            } else {
                Theme::FluentLight.chrome()
            };
            if self.accent.is_some() {
                chrome.checked_hover =
                    selection(tokens.surface, tokens.text, tokens.focus, 0.20, false);
                chrome.checked_pressed =
                    selection(tokens.surface, tokens.text, tokens.focus, 0.26, false);
            }
            return chrome;
        }
        ChromeTokens {
            ribbon_surface: tokens.surface,
            card_surface: tokens.surface,
            surface_border: tokens.border,
            divider: tokens.border,
            control_fill: tokens.surface,
            control_hover: tokens.hover,
            control_pressed: tokens.pressed,
            control_border: tokens.border,
            control_bottom_border: tokens.border,
            subtle_hover: tokens.hover,
            subtle_pressed: tokens.pressed,
            checked_hover: selection(
                tokens.surface,
                tokens.text,
                tokens.focus,
                0.20,
                self.mode.is_high_contrast(),
            ),
            checked_pressed: selection(
                tokens.surface,
                tokens.text,
                tokens.focus,
                0.26,
                self.mode.is_high_contrast(),
            ),
            disabled_text: tokens.muted,
            flyout_surface: tokens.surface,
            flyout_border: tokens.border,
            editor_fill: tokens.surface,
            editor_hover: tokens.hover,
            editor_read_only: tokens.canvas,
            shadow: Color {
                a: 0.0,
                ..tokens.border
            },
        }
    }
}

fn color(rgb: [u8; 3]) -> Color {
    Color::rgb(rgb[0], rgb[1], rgb[2])
}

fn linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn composite(overlay: Color, background: Color) -> Color {
    let channel = |foreground: f32, background: f32| {
        let value = linear(foreground) * overlay.a + linear(background) * (1.0 - overlay.a);
        if value <= 0.0031308 {
            value * 12.92
        } else {
            1.055 * value.powf(1.0 / 2.4) - 0.055
        }
    };
    Color {
        r: channel(overlay.r, background.r),
        g: channel(overlay.g, background.g),
        b: channel(overlay.b, background.b),
        a: 1.0,
    }
}

fn mix(from: Color, to: Color, amount: f32) -> Color {
    Color {
        r: from.r + (to.r - from.r) * amount,
        g: from.g + (to.g - from.g) * amount,
        b: from.b + (to.b - from.b) * amount,
        a: 1.0,
    }
}

fn selection(
    surface: Color,
    text: Color,
    accent: Color,
    amount: f32,
    high_contrast: bool,
) -> Color {
    // Palette accents can point toward the text luminance. Keep the selected
    // surface readable even for vivid custom accents and high-contrast themes.
    let mut amount = amount;
    for _ in 0..8 {
        let selected = mix(surface, accent, amount);
        if contrast(text, selected) >= if high_contrast { 7.0 } else { 4.5 } {
            return selected;
        }
        amount *= 0.5;
    }
    surface
}

pub(super) fn contrast(a: Color, b: Color) -> f32 {
    let luminance = |c: Color| 0.2126 * linear(c.r) + 0.7152 * linear(c.g) + 0.0722 * linear(c.b);
    let a = luminance(a);
    let b = luminance(b);
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ButtonChrome, ButtonStyle, ButtonVisualState, EditBoxStyle, EditBoxVisualState, IconSymbol,
    };
    use rust_desktop_ui_core::{Rect, Scene};

    #[test]
    fn all_configured_modes_and_densities_keep_readable_states() {
        for mode in [
            ThemeMode::Light,
            ThemeMode::Dark,
            ThemeMode::HighContrastLight,
            ThemeMode::HighContrastDark,
        ] {
            for density in [
                ThemeDensity::Comfortable,
                ThemeDensity::Compact,
                ThemeDensity::Touch,
            ] {
                let config = ThemeConfig::new(mode).with_density(density);
                config.validate().unwrap();
                let theme = Theme::Configured(config);
                assert_eq!(theme.is_dark(), mode.is_dark());
                assert_eq!(theme.is_high_contrast(), mode.is_high_contrast());
                assert_eq!(theme.density(), density);
                let tokens = theme.tokens();
                assert!(contrast(theme.on_accent(), tokens.focus) >= 4.5);
                for checked in [false, true] {
                    for (hovered, pressed) in [(false, false), (true, false), (true, true)] {
                        let state = ButtonVisualState {
                            checked,
                            hovered,
                            pressed,
                            focused: true,
                            ..Default::default()
                        };
                        let style = ButtonStyle::default().resolve(theme, state, false);
                        assert!(
                            contrast(style.foreground, style.background)
                                >= if mode.is_high_contrast() { 7.0 } else { 4.5 }
                        );
                        let indicator = ButtonStyle::default().resolve(theme, state, true);
                        if checked {
                            assert!(contrast(indicator.checkmark, indicator.background) >= 4.5);
                        }
                        assert!(style.focus_visible);
                        for chrome in [ButtonChrome::Flat, ButtonChrome::Tab] {
                            let style = ButtonStyle::default().resolve_chrome(theme, state, chrome);
                            let background =
                                composite(style.background, theme.chrome().ribbon_surface);
                            assert!(
                                contrast(style.foreground, background)
                                    >= if mode.is_high_contrast() { 7.0 } else { 4.5 }
                            );
                            assert!(contrast(style.accent, background) >= 3.0);
                        }
                    }
                }
                for state in [
                    EditBoxVisualState::Normal,
                    EditBoxVisualState::Hovered,
                    EditBoxVisualState::Focused,
                    EditBoxVisualState::ReadOnly,
                    EditBoxVisualState::ReadOnlyFocused,
                    EditBoxVisualState::Disabled,
                ] {
                    let style = EditBoxStyle::default().resolve(theme, state);
                    if state != EditBoxVisualState::Disabled || mode.is_high_contrast() {
                        assert!(contrast(style.foreground, style.background) >= 4.5);
                    }
                    if mode.is_high_contrast() {
                        assert_eq!(style.border_width, 2.0);
                    }
                }
            }
        }
    }

    #[test]
    fn custom_accent_and_palette_are_validated_without_mutating_original() {
        let original = ThemeConfig::default();
        let custom = original.with_accent([112, 52, 180]).unwrap();
        assert_eq!(custom.palette().accent, [112, 52, 180]);
        assert_eq!(original, ThemeConfig::default());
        assert!(original.with_accent([255, 255, 255]).is_err());
        let invalid = ThemePalette {
            text: [255, 255, 255],
            ..ThemePalette::for_mode(ThemeMode::Light)
        };
        assert!(original.with_palette(invalid).is_err());
        let invalid_muted = ThemePalette {
            muted: [190, 190, 190],
            ..ThemePalette::for_mode(ThemeMode::Light)
        };
        assert!(original.with_palette(invalid_muted).is_err());
        let invalid_error = ThemePalette {
            error: [255, 255, 255],
            ..ThemePalette::for_mode(ThemeMode::Light)
        };
        assert!(original.with_palette(invalid_error).is_err());
        let high = ThemeConfig::new(ThemeMode::HighContrastDark);
        assert!(
            high.with_palette(ThemePalette {
                border: [0, 0, 0],
                ..high.palette()
            })
            .is_err()
        );
        let palette = ThemePalette {
            canvas: [246, 242, 233],
            surface: [255, 251, 241],
            text: [40, 27, 18],
            muted: [96, 73, 52],
            border: [130, 97, 60],
            accent: [116, 61, 26],
            error: [170, 20, 20],
        };
        let configured = original.with_palette(palette).unwrap();
        assert_eq!(configured.palette(), palette);
        let theme = Theme::Configured(configured);
        assert_eq!(theme.tokens().canvas, color(palette.canvas));
        assert_eq!(theme.chrome().card_surface, color(palette.surface));
        assert_ne!(theme.tokens().selected, theme.tokens().surface);
        assert_ne!(theme.chrome().control_hover, theme.chrome().control_pressed);
    }

    #[test]
    fn high_contrast_icons_are_monochrome_and_configured_icons_follow_accent() {
        let bounds = Rect::new(0.0, 0.0, 32.0, 32.0);
        for theme in [
            Theme::HighContrastLight,
            Theme::HighContrastDark,
            Theme::Configured(ThemeConfig::new(ThemeMode::HighContrastDark)),
        ] {
            for symbol in [
                IconSymbol::Save,
                IconSymbol::FolderOpen,
                IconSymbol::Check,
                IconSymbol::Sun,
            ] {
                let mut scene = Scene::default();
                crate::primitives::paint_icon_themed(
                    &mut scene,
                    symbol,
                    bounds,
                    bounds,
                    theme.tokens().text,
                    theme,
                );
                assert!(
                    scene
                        .stroke_paths
                        .iter()
                        .all(|path| path.color == theme.tokens().text)
                );
                assert_eq!(scene.stroke_paths.len(), 1);
            }
        }
        let theme = Theme::Configured(ThemeConfig::default().with_accent([112, 52, 180]).unwrap());
        let mut scene = Scene::default();
        crate::primitives::paint_icon_themed(
            &mut scene,
            IconSymbol::Save,
            bounds,
            bounds,
            theme.tokens().text,
            theme,
        );
        assert_eq!(scene.stroke_paths.len(), 2);
        assert!(
            scene
                .stroke_paths
                .iter()
                .any(|path| path.color == theme.tokens().focus)
        );
    }

    #[test]
    fn density_is_independent_of_palette_and_high_contrast_shortcuts_are_equivalent() {
        let base = ThemeConfig::default();
        let compact = Theme::Configured(base.with_density(ThemeDensity::Compact));
        let normal = Theme::Configured(base);
        let touch = Theme::Configured(base.with_density(ThemeDensity::Touch));
        assert!(compact.tokens().control_height < normal.tokens().control_height);
        assert!(normal.tokens().control_height < touch.tokens().control_height);
        assert_eq!(compact.tokens().canvas, touch.tokens().canvas);
        assert_eq!(Theme::Compact.tokens().control_height, 26.0);
        for (theme, mode) in [
            (Theme::HighContrastLight, ThemeMode::HighContrastLight),
            (Theme::HighContrastDark, ThemeMode::HighContrastDark),
        ] {
            let configured = Theme::Configured(ThemeConfig::new(mode));
            assert_eq!(theme.tokens().canvas, configured.tokens().canvas);
            assert_eq!(theme.tokens().focus, configured.tokens().focus);
            assert_eq!(theme.is_fluent(), configured.is_fluent());
        }
    }
}
