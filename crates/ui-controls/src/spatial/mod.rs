//! Bounded geographic, projected data, and graph canvases.
//!
//! Models own validated logical geometry. Presenters produce the same ordered
//! GPU scene that is used by hit testing; providers and file I/O remain external.
mod chart3d;
mod diagram;
mod map;

pub use chart3d::*;
pub use diagram::*;
pub use map::*;

use rust_desktop_ui_core::{Color, Point, Rect, Scene, StrokeSegment, TextRun};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SpatialError {
    InvalidCoordinate,
    InvalidViewport,
    InvalidLabel,
    DuplicateIdentity,
    UnknownIdentity,
    InvalidRange,
    Capacity,
    InvalidGraph,
    InvalidDocument,
    StaleRequest,
    InvalidResource,
    ReadOnly,
}
impl std::fmt::Display for SpatialError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Spatial canvas: {self:?}")
    }
}
impl std::error::Error for SpatialError {}

#[derive(Clone, Copy, Debug)]
pub struct SpatialPalette {
    pub background: Color,
    pub surface: Color,
    pub foreground: Color,
    pub muted: Color,
    pub border: Color,
    pub accent: Color,
}
impl SpatialPalette {
    pub fn from_theme(theme: crate::Theme) -> Self {
        let t = theme.tokens();
        Self {
            background: t.canvas,
            surface: t.surface,
            foreground: t.text,
            muted: t.muted,
            border: t.border,
            accent: t.focus,
        }
    }
}
pub(crate) fn valid_label(value: &str) -> Result<(), SpatialError> {
    if value.len() > 512 || value.chars().any(char::is_control) {
        Err(SpatialError::InvalidLabel)
    } else {
        Ok(())
    }
}
pub(crate) fn valid_viewport(b: Rect) -> Result<(), SpatialError> {
    if !b.is_valid() || b.width <= 0.0 || b.height <= 0.0 || b.width > 32768.0 || b.height > 32768.0
    {
        Err(SpatialError::InvalidViewport)
    } else {
        Ok(())
    }
}
pub(crate) fn line(scene: &mut Scene, a: Point, b: Point, clip: Rect, width: f32, color: Color) {
    scene.stroke_path(&[StrokeSegment { start: a, end: b }], clip, width, color);
}
pub(crate) fn label(scene: &mut Scene, value: &str, b: Rect, clip: Rect, color: Color, size: f32) {
    if b.width <= 0.0 || b.height <= 0.0 {
        return;
    }
    let mut run = TextRun::new(value, b, color, size);
    run.clip = b
        .intersection(clip)
        .unwrap_or(Rect::new(b.x, b.y, 0.0, 0.0));
    scene.text(run);
}
pub(crate) fn distance(a: Point, b: Point) -> f32 {
    (a.x - b.x).hypot(a.y - b.y)
}

/// Columns and rows are retained data, including offscreen/unselected records.
/// Applications can bind these to a ListBox/Grid for an alternative to a canvas.
#[derive(Clone, Debug, PartialEq)]
pub struct SpatialTable {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
}
