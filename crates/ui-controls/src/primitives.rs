use crate::ControlError;
use rust_desktop_ui_core::{Axis, Color, Point, Rect, Scene, StrokeSegment};

/// Small built-in symbols defined by this crate's own 16 by 16 line geometry.
/// They require neither an icon font nor an external image resource.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum IconSymbol {
    Check,
    Close,
    Add,
    Remove,
    ChevronDown,
    ChevronRight,
    Save,
    Pin,
    FolderOpen,
    Copy,
    Edit,
    Eraser,
    Sun,
    Moon,
    Lock,
}

/// Standalone icon colors. Unspecified values follow the current theme.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct IconStyle {
    pub foreground: Option<Color>,
    pub disabled_foreground: Option<Color>,
}

impl IconStyle {
    pub fn validate(&self) -> Result<(), ControlError> {
        validate_colors([self.foreground, self.disabled_foreground])
    }
}

/// Icon embedded in a Button or ToggleButton, without a separate
/// semantic node. Its color follows the resolved button foreground.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ButtonIcon {
    pub symbol: IconSymbol,
    /// Logical pixel size in `(0, 512]`.
    pub size: f32,
    /// Gap between the icon and nonempty text in logical pixels, in `[0, 16384]`.
    pub spacing: f32,
}

impl ButtonIcon {
    pub const fn new(symbol: IconSymbol) -> Self {
        Self {
            symbol,
            size: 16.0,
            spacing: 8.0,
        }
    }

    pub fn validate(&self) -> Result<(), ControlError> {
        if !self.size.is_finite() || self.size <= 0.0 || self.size > 512.0 {
            return Err(ControlError::Invalid(
                "Button icon size must be finite and in (0, 512]".into(),
            ));
        }
        if !self.spacing.is_finite() || !(0.0..=16_384.0).contains(&self.spacing) {
            return Err(ControlError::Invalid(
                "Button icon spacing must be finite and in [0, 16384]".into(),
            ));
        }
        Ok(())
    }
}

/// A decorative rule drawn in the center of its layout box. The long axis is
/// provided by layout; its color is independent of enabled state.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SeparatorSettings {
    pub orientation: Axis,
    /// Logical pixels in `(0, 16384]`, clamped to the layout box for painting.
    pub thickness: f32,
    pub foreground: Option<Color>,
}

impl Default for SeparatorSettings {
    fn default() -> Self {
        Self {
            orientation: Axis::Horizontal,
            thickness: 1.0,
            foreground: None,
        }
    }
}

impl SeparatorSettings {
    pub fn validate(&self) -> Result<(), ControlError> {
        if !self.thickness.is_finite() || self.thickness <= 0.0 || self.thickness > 16_384.0 {
            return Err(ControlError::Invalid(
                "Separator thickness must be finite and in (0, 16384]".into(),
            ));
        }
        validate_colors([self.foreground])
    }
}

fn validate_colors<const N: usize>(colors: [Option<Color>; N]) -> Result<(), ControlError> {
    for color in colors.into_iter().flatten() {
        if [color.r, color.g, color.b, color.a]
            .into_iter()
            .any(|value| !value.is_finite() || !(0.0..=1.0).contains(&value))
        {
            return Err(ControlError::Invalid(
                "Primitive color channels must be finite and in [0, 1]".into(),
            ));
        }
    }
    Ok(())
}

const fn segment(x1: f32, y1: f32, x2: f32, y2: f32) -> StrokeSegment {
    StrokeSegment {
        start: Point::new(x1, y1),
        end: Point::new(x2, y2),
    }
}

impl IconSymbol {
    pub(crate) fn segments(self) -> &'static [StrokeSegment] {
        match self {
            Self::Check => const { &[segment(3.0, 8.0, 6.5, 11.5), segment(6.5, 11.5, 13.0, 4.5)] },
            Self::Close => {
                const { &[segment(4.0, 4.0, 12.0, 12.0), segment(12.0, 4.0, 4.0, 12.0)] }
            }
            Self::Add => const { &[segment(3.0, 8.0, 13.0, 8.0), segment(8.0, 3.0, 8.0, 13.0)] },
            Self::Remove => const { &[segment(3.0, 8.0, 13.0, 8.0)] },
            Self::ChevronDown => {
                const { &[segment(3.5, 6.0, 8.0, 10.5), segment(8.0, 10.5, 12.5, 6.0)] }
            }
            Self::ChevronRight => {
                const { &[segment(6.0, 3.5, 10.5, 8.0), segment(10.5, 8.0, 6.0, 12.5)] }
            }
            Self::Save => {
                const {
                    &[
                        segment(2.0, 2.0, 11.0, 2.0),
                        segment(11.0, 2.0, 14.0, 5.0),
                        segment(14.0, 5.0, 14.0, 14.0),
                        segment(14.0, 14.0, 2.0, 14.0),
                        segment(2.0, 14.0, 2.0, 2.0),
                        segment(5.0, 2.0, 5.0, 6.0),
                        segment(5.0, 6.0, 10.0, 6.0),
                        segment(10.0, 6.0, 10.0, 2.0),
                        segment(5.0, 14.0, 5.0, 9.0),
                        segment(5.0, 9.0, 11.0, 9.0),
                        segment(11.0, 9.0, 11.0, 14.0),
                    ]
                }
            }
            Self::Pin => {
                const {
                    &[
                        segment(5.0, 2.0, 11.0, 2.0),
                        segment(6.0, 2.0, 6.0, 7.0),
                        segment(10.0, 2.0, 10.0, 7.0),
                        segment(6.0, 7.0, 3.5, 10.0),
                        segment(3.5, 10.0, 12.5, 10.0),
                        segment(12.5, 10.0, 10.0, 7.0),
                        segment(8.0, 10.0, 8.0, 14.0),
                    ]
                }
            }
            Self::FolderOpen => {
                const {
                    &[
                        segment(2.0, 12.5, 2.0, 3.0),
                        segment(2.0, 3.0, 6.0, 3.0),
                        segment(6.0, 3.0, 8.0, 5.0),
                        segment(8.0, 5.0, 13.0, 5.0),
                        segment(13.0, 5.0, 13.0, 7.0),
                        segment(2.0, 12.5, 5.0, 7.0),
                        segment(5.0, 7.0, 14.0, 7.0),
                        segment(14.0, 7.0, 11.0, 12.5),
                        segment(11.0, 12.5, 2.0, 12.5),
                    ]
                }
            }
            Self::Copy => {
                const {
                    &[
                        segment(5.0, 5.0, 13.0, 5.0),
                        segment(13.0, 5.0, 13.0, 14.0),
                        segment(13.0, 14.0, 5.0, 14.0),
                        segment(5.0, 14.0, 5.0, 5.0),
                        segment(3.0, 11.0, 2.0, 11.0),
                        segment(2.0, 11.0, 2.0, 2.0),
                        segment(2.0, 2.0, 10.0, 2.0),
                        segment(10.0, 2.0, 10.0, 3.0),
                    ]
                }
            }
            Self::Edit => {
                const {
                    &[
                        segment(2.0, 14.0, 3.0, 10.0),
                        segment(3.0, 10.0, 11.0, 2.0),
                        segment(11.0, 2.0, 14.0, 5.0),
                        segment(14.0, 5.0, 6.0, 13.0),
                        segment(6.0, 13.0, 2.0, 14.0),
                        segment(9.0, 4.0, 12.0, 7.0),
                        segment(3.0, 10.0, 6.0, 13.0),
                    ]
                }
            }
            Self::Eraser => {
                const {
                    &[
                        segment(2.0, 10.0, 9.0, 3.0),
                        segment(9.0, 3.0, 14.0, 8.0),
                        segment(14.0, 8.0, 9.0, 13.0),
                        segment(9.0, 13.0, 5.0, 13.0),
                        segment(5.0, 13.0, 2.0, 10.0),
                        segment(5.0, 7.0, 10.0, 12.0),
                        segment(9.0, 13.0, 14.0, 13.0),
                    ]
                }
            }
            Self::Sun => {
                const {
                    &[
                        segment(6.5, 4.5, 9.5, 4.5),
                        segment(9.5, 4.5, 11.5, 6.5),
                        segment(11.5, 6.5, 11.5, 9.5),
                        segment(11.5, 9.5, 9.5, 11.5),
                        segment(9.5, 11.5, 6.5, 11.5),
                        segment(6.5, 11.5, 4.5, 9.5),
                        segment(4.5, 9.5, 4.5, 6.5),
                        segment(4.5, 6.5, 6.5, 4.5),
                        segment(8.0, 1.0, 8.0, 2.0),
                        segment(8.0, 14.0, 8.0, 15.0),
                        segment(1.0, 8.0, 2.0, 8.0),
                        segment(14.0, 8.0, 15.0, 8.0),
                        segment(3.0, 3.0, 3.7, 3.7),
                        segment(12.3, 12.3, 13.0, 13.0),
                        segment(3.0, 13.0, 3.7, 12.3),
                        segment(12.3, 3.7, 13.0, 3.0),
                    ]
                }
            }
            Self::Moon => {
                const {
                    &[
                        segment(9.0, 2.0, 5.0, 3.0),
                        segment(5.0, 3.0, 2.5, 6.0),
                        segment(2.5, 6.0, 2.5, 10.0),
                        segment(2.5, 10.0, 5.0, 13.0),
                        segment(5.0, 13.0, 9.0, 14.0),
                        segment(9.0, 14.0, 12.0, 12.5),
                        segment(12.0, 12.5, 14.0, 9.0),
                        segment(14.0, 9.0, 10.0, 10.0),
                        segment(10.0, 10.0, 7.0, 8.0),
                        segment(7.0, 8.0, 7.0, 5.0),
                        segment(7.0, 5.0, 9.0, 2.0),
                    ]
                }
            }
            Self::Lock => {
                const {
                    &[
                        segment(3.0, 7.0, 13.0, 7.0),
                        segment(13.0, 7.0, 13.0, 14.0),
                        segment(13.0, 14.0, 3.0, 14.0),
                        segment(3.0, 14.0, 3.0, 7.0),
                        segment(5.0, 7.0, 5.0, 4.0),
                        segment(5.0, 4.0, 6.5, 2.0),
                        segment(6.5, 2.0, 9.5, 2.0),
                        segment(9.5, 2.0, 11.0, 4.0),
                        segment(11.0, 4.0, 11.0, 7.0),
                        segment(8.0, 10.0, 8.0, 11.5),
                    ]
                }
            }
        }
    }
}

pub(crate) fn paint_icon(
    scene: &mut Scene,
    symbol: IconSymbol,
    bounds: Rect,
    clip: Rect,
    color: Color,
) {
    let size = bounds.width.min(bounds.height);
    if size <= 0.0 || color.a == 0.0 {
        return;
    }
    let Some(clip) = bounds.intersection(clip) else {
        return;
    };
    let scale = size / 16.0;
    let origin = Point::new(
        bounds.x + (bounds.width - size) * 0.5,
        bounds.y + (bounds.height - size) * 0.5,
    );
    let transform =
        |point: Point| Point::new(origin.x + point.x * scale, origin.y + point.y * scale);
    let segments: Vec<_> = symbol
        .segments()
        .iter()
        .map(|line| StrokeSegment {
            start: transform(line.start),
            end: transform(line.end),
        })
        .collect();
    scene.stroke_path(&segments, clip, 1.5 * scale, color);
}
