use rust_desktop_ui_core::Color;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Theme {
    #[default]
    Light,
    Dark,
    Compact,
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
    pub control_height: f32,
    pub font_size: f32,
    pub spacing: f32,
}

impl Theme {
    pub fn tokens(self) -> ThemeTokens {
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
                control_height: if compact { 26.0 } else { 34.0 },
                font_size: if compact { 13.0 } else { 15.0 },
                spacing: if compact { 4.0 } else { 8.0 },
            }
        }
    }
}
