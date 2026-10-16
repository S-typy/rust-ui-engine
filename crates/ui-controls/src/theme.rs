use rust_desktop_ui_core::Color;

mod config;
pub use config::*;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Theme {
    #[default]
    Light,
    Dark,
    Compact,
    FluentLight,
    FluentDark,
    HighContrastLight,
    HighContrastDark,
    Configured(ThemeConfig),
}

#[derive(Clone, Copy, Debug)]
pub struct ThemeTokens {
    pub canvas: Color,
    pub surface: Color,
    pub text: Color,
    pub muted: Color,
    pub border: Color,
    pub hover: Color,
    pub pressed: Color,
    pub selected: Color,
    pub focus: Color,
    /// Validation border and accent, adapted for the theme's surfaces.
    pub error: Color,
    pub control_height: f32,
    pub font_size: f32,
    pub spacing: f32,
}

/// Semantic surface and interaction colors. These distinguish command chrome,
/// ordinary controls, editable content and floating menus without changing
/// their layout or command behavior.
#[derive(Clone, Copy, Debug)]
pub struct ChromeTokens {
    pub ribbon_surface: Color,
    pub card_surface: Color,
    pub surface_border: Color,
    pub divider: Color,
    pub control_fill: Color,
    pub control_hover: Color,
    pub control_pressed: Color,
    pub control_border: Color,
    pub control_bottom_border: Color,
    /// Alpha overlay for borderless command hover; composite over its surface.
    pub subtle_hover: Color,
    /// Alpha overlay for borderless command press; composite over its surface.
    pub subtle_pressed: Color,
    pub checked_hover: Color,
    pub checked_pressed: Color,
    pub disabled_text: Color,
    pub flyout_surface: Color,
    pub flyout_border: Color,
    pub editor_fill: Color,
    pub editor_hover: Color,
    pub editor_read_only: Color,
    pub shadow: Color,
}

impl Theme {
    /// Resolve independent surface and interaction roles. Legacy themes retain
    /// the original token colors; Fluent themes distinguish cards, editors,
    /// ordinary controls and floating menus.
    pub fn chrome(self) -> ChromeTokens {
        if let Some(config) = self.configuration() {
            return config.chrome();
        }
        let tokens = self.tokens();
        let translucent = |color: Color, a| Color { a, ..color };
        if self == Self::FluentLight {
            ChromeTokens {
                ribbon_surface: Color::rgb(252, 252, 252),
                card_surface: Color::rgb(255, 255, 255),
                surface_border: Color::rgb(225, 225, 225),
                divider: Color::rgb(219, 219, 219),
                control_fill: Color::rgb(255, 255, 255),
                control_hover: Color::rgb(248, 248, 248),
                control_pressed: Color::rgb(239, 239, 239),
                control_border: Color::rgb(220, 220, 220),
                control_bottom_border: Color::rgb(188, 188, 188),
                subtle_hover: translucent(Color::rgb(0, 0, 0), 0.045),
                subtle_pressed: translucent(Color::rgb(0, 0, 0), 0.075),
                checked_hover: Color::rgb(183, 217, 241),
                checked_pressed: Color::rgb(168, 207, 234),
                disabled_text: Color::rgb(151, 151, 151),
                flyout_surface: Color::rgb(253, 253, 253),
                flyout_border: Color::rgb(216, 216, 216),
                editor_fill: Color::rgb(255, 255, 255),
                editor_hover: Color::rgb(252, 252, 252),
                editor_read_only: Color::rgb(246, 246, 246),
                shadow: translucent(Color::rgb(0, 0, 0), 0.12),
            }
        } else if self == Self::FluentDark {
            ChromeTokens {
                ribbon_surface: Color::rgb(42, 42, 42),
                card_surface: Color::rgb(46, 46, 46),
                surface_border: Color::rgb(62, 62, 62),
                divider: Color::rgb(66, 66, 66),
                control_fill: Color::rgb(49, 49, 49),
                control_hover: Color::rgb(57, 57, 57),
                control_pressed: Color::rgb(43, 43, 43),
                control_border: Color::rgb(68, 68, 68),
                control_bottom_border: Color::rgb(88, 88, 88),
                subtle_hover: translucent(Color::rgb(255, 255, 255), 0.020),
                subtle_pressed: translucent(Color::rgb(255, 255, 255), 0.035),
                checked_hover: Color::rgb(39, 86, 119),
                checked_pressed: Color::rgb(29, 67, 96),
                disabled_text: Color::rgb(118, 118, 118),
                flyout_surface: Color::rgb(48, 48, 48),
                flyout_border: Color::rgb(72, 72, 72),
                editor_fill: Color::rgb(39, 39, 39),
                editor_hover: Color::rgb(45, 45, 45),
                editor_read_only: Color::rgb(35, 35, 35),
                shadow: translucent(Color::rgb(0, 0, 0), 0.30),
            }
        } else {
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
                checked_hover: tokens.hover,
                checked_pressed: tokens.pressed,
                disabled_text: tokens.muted,
                flyout_surface: tokens.surface,
                flyout_border: tokens.border,
                editor_fill: tokens.surface,
                editor_hover: tokens.hover,
                editor_read_only: tokens.canvas,
                shadow: translucent(tokens.border, 0.0),
            }
        }
    }
    pub fn is_fluent(self) -> bool {
        matches!(
            self,
            Self::FluentLight
                | Self::FluentDark
                | Self::HighContrastLight
                | Self::HighContrastDark
                | Self::Configured(_)
        )
    }
    pub fn is_dark(self) -> bool {
        self.configuration()
            .map_or(matches!(self, Self::Dark | Self::FluentDark), |config| {
                let [r, g, b] = config.palette().canvas;
                let canvas = Color::rgb(r, g, b);
                config::contrast(Color::rgb(255, 255, 255), canvas)
                    > config::contrast(Color::rgb(0, 0, 0), canvas)
            })
    }
    pub fn is_high_contrast(self) -> bool {
        self.configuration()
            .is_some_and(|config| config.mode().is_high_contrast())
    }
    pub fn density(self) -> ThemeDensity {
        self.configuration().map_or(
            if self == Self::Compact {
                ThemeDensity::Compact
            } else {
                ThemeDensity::Comfortable
            },
            |config| config.density(),
        )
    }
    pub fn is_compact(self) -> bool {
        self.density() == ThemeDensity::Compact
    }
    /// Highest-contrast monochrome foreground for the current accent fill.
    pub fn on_accent(self) -> Color {
        let accent = self.tokens().focus;
        let black = Color::rgb(0, 0, 0);
        let white = Color::rgb(255, 255, 255);
        if config::contrast(black, accent) >= config::contrast(white, accent) {
            black
        } else {
            white
        }
    }
    pub fn configuration(self) -> Option<ThemeConfig> {
        match self {
            Self::Configured(config) => Some(config),
            Self::HighContrastLight => Some(ThemeConfig::new(ThemeMode::HighContrastLight)),
            Self::HighContrastDark => Some(ThemeConfig::new(ThemeMode::HighContrastDark)),
            _ => None,
        }
    }
    pub fn tokens(self) -> ThemeTokens {
        if let Some(config) = self.configuration() {
            return config.tokens();
        }
        if self.is_fluent() {
            let dark = self == Self::FluentDark;
            return ThemeTokens {
                canvas: if dark {
                    Color::rgb(32, 32, 32)
                } else {
                    Color::rgb(243, 243, 243)
                },
                surface: if dark {
                    Color::rgb(45, 45, 45)
                } else {
                    Color::rgb(255, 255, 255)
                },
                text: if dark {
                    Color::rgb(245, 245, 245)
                } else {
                    Color::rgb(27, 27, 27)
                },
                muted: if dark {
                    Color::rgb(170, 170, 170)
                } else {
                    Color::rgb(105, 105, 105)
                },
                border: if dark {
                    Color::rgb(85, 85, 85)
                } else {
                    Color::rgb(205, 205, 205)
                },
                hover: if dark {
                    Color::rgb(50, 50, 50)
                } else {
                    Color::rgb(250, 250, 250)
                },
                pressed: if dark {
                    Color::rgb(40, 40, 40)
                } else {
                    Color::rgb(235, 235, 235)
                },
                selected: if dark {
                    Color::rgb(35, 75, 105)
                } else {
                    Color::rgb(196, 225, 245)
                },
                focus: if dark {
                    Color::rgb(96, 205, 255)
                } else {
                    Color::rgb(0, 95, 184)
                },
                error: if dark {
                    Color::rgb(255, 153, 164)
                } else {
                    Color::rgb(196, 43, 28)
                },
                control_height: 36.0,
                font_size: 14.0,
                spacing: 8.0,
            };
        }
        let compact = self == Self::Compact;
        if self == Self::Dark {
            ThemeTokens {
                canvas: Color::rgb(25, 29, 37),
                surface: Color::rgb(40, 46, 57),
                text: Color::rgb(236, 241, 250),
                muted: Color::rgb(137, 149, 167),
                border: Color::rgb(77, 88, 107),
                hover: Color::rgb(56, 75, 98),
                pressed: Color::rgb(34, 86, 134),
                selected: Color::rgb(37, 72, 109),
                focus: Color::rgb(116, 189, 246),
                error: Color::rgb(255, 153, 164),
                control_height: 34.0,
                font_size: 15.0,
                spacing: 8.0,
            }
        } else {
            ThemeTokens {
                canvas: Color::rgb(240, 244, 249),
                surface: Color::rgb(255, 255, 255),
                text: Color::rgb(30, 43, 61),
                muted: Color::rgb(120, 130, 146),
                border: Color::rgb(195, 207, 221),
                hover: Color::rgb(224, 237, 251),
                pressed: Color::rgb(166, 202, 239),
                selected: Color::rgb(199, 222, 246),
                focus: Color::rgb(27, 108, 191),
                error: Color::rgb(196, 43, 28),
                control_height: if compact { 26.0 } else { 34.0 },
                font_size: if compact { 13.0 } else { 15.0 },
                spacing: if compact { 4.0 } else { 8.0 },
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn linear(channel: f32) -> f32 {
        if channel <= 0.04045 {
            channel / 12.92
        } else {
            ((channel + 0.055) / 1.055).powf(2.4)
        }
    }

    fn luminance(color: Color) -> f32 {
        0.2126 * linear(color.r) + 0.7152 * linear(color.g) + 0.0722 * linear(color.b)
    }

    fn contrast(first: Color, second: Color) -> f32 {
        let first = luminance(first);
        let second = luminance(second);
        (first.max(second) + 0.05) / (first.min(second) + 0.05)
    }

    fn composite(overlay: Color, background: Color) -> Color {
        // Match the renderer: straight sRGB colors blend in linear light.
        let blend = |foreground: f32, background: f32| {
            let linear = linear(foreground) * overlay.a + linear(background) * (1.0 - overlay.a);
            if linear <= 0.0031308 {
                linear * 12.92
            } else {
                1.055 * linear.powf(1.0 / 2.4) - 0.055
            }
        };
        Color {
            r: blend(overlay.r, background.r),
            g: blend(overlay.g, background.g),
            b: blend(overlay.b, background.b),
            a: 1.0,
        }
    }

    #[test]
    fn fluent_primary_text_stays_readable_on_all_control_surfaces_and_states() {
        for theme in [Theme::FluentLight, Theme::FluentDark] {
            let tokens = theme.tokens();
            let chrome = theme.chrome();
            for background in [
                tokens.canvas,
                chrome.ribbon_surface,
                chrome.card_surface,
                chrome.control_fill,
                chrome.control_hover,
                chrome.control_pressed,
                tokens.selected,
                chrome.checked_hover,
                chrome.checked_pressed,
                chrome.flyout_surface,
                chrome.editor_fill,
                chrome.editor_hover,
                chrome.editor_read_only,
                composite(chrome.subtle_hover, chrome.ribbon_surface),
                composite(chrome.subtle_pressed, chrome.ribbon_surface),
            ] {
                assert_eq!(background.a, 1.0);
                assert!(
                    contrast(tokens.text, background) >= 4.5,
                    "{theme:?}: primary text is not readable on {background:?}"
                );
            }
            for background in [
                chrome.editor_fill,
                chrome.editor_hover,
                chrome.editor_read_only,
            ] {
                assert!(contrast(tokens.muted, background) >= 4.5);
                assert!(
                    contrast(chrome.disabled_text, background) < contrast(tokens.text, background)
                );
            }
        }
    }

    #[test]
    fn fluent_layers_and_overlay_states_are_distinct_without_changing_legacy_tokens() {
        for theme in [Theme::FluentLight, Theme::FluentDark] {
            let tokens = theme.tokens();
            let chrome = theme.chrome();
            assert_ne!(tokens.canvas, chrome.ribbon_surface);
            assert_ne!(chrome.ribbon_surface, chrome.card_surface);
            assert_ne!(chrome.flyout_border, chrome.flyout_surface);
            assert_ne!(chrome.control_fill, chrome.control_hover);
            assert_ne!(chrome.control_hover, chrome.control_pressed);
            assert!((0.0..1.0).contains(&chrome.subtle_hover.a));
            assert!(chrome.subtle_pressed.a > chrome.subtle_hover.a);
            assert!((0.0..1.0).contains(&chrome.shadow.a));
        }
        for theme in [Theme::Light, Theme::Dark, Theme::Compact] {
            let tokens = theme.tokens();
            let chrome = theme.chrome();
            assert_eq!(chrome.control_fill, tokens.surface);
            assert_eq!(chrome.control_hover, tokens.hover);
            assert_eq!(chrome.control_pressed, tokens.pressed);
            assert_eq!(chrome.editor_fill, tokens.surface);
            assert_eq!(chrome.editor_read_only, tokens.canvas);
            assert_eq!(chrome.shadow.a, 0.0);
        }
    }
}
