//! Renderer-independent text commands in logical pixels.
use crate::{Color, Rect};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TextAlign {
    #[default]
    Start,
    Center,
    End,
}

/// Text constrained by a logical layout box and an explicit ancestor clip.
/// Font lookup and shaping belong to the text engine, not the scene model.
#[derive(Clone, Debug, PartialEq)]
pub struct TextRun {
    pub bounds: Rect,
    pub clip: Rect,
    pub text: String,
    pub font_size: f32,
    pub color: Color,
    pub weight: u16,
    pub align: TextAlign,
    pub wrap: bool,
}

impl TextRun {
    pub fn new(text: impl Into<String>, bounds: Rect, color: Color, font_size: f32) -> Self {
        Self {
            bounds,
            clip: bounds,
            text: text.into(),
            font_size,
            color,
            weight: 400,
            align: TextAlign::Start,
            wrap: false,
        }
    }

    pub fn is_valid(&self) -> bool {
        self.bounds.is_valid()
            && self.clip.is_valid()
            && self.font_size.is_finite()
            && self.font_size > 0.0
            && self.font_size <= 512.0
            && (1..=1000).contains(&self.weight)
            && [self.color.r, self.color.g, self.color.b, self.color.a]
                .into_iter()
                .all(|channel| channel.is_finite() && (0.0..=1.0).contains(&channel))
    }
}

/// References the corresponding typed array in a Scene, in painter's order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DrawCommand {
    Rectangle(usize),
    Text(usize),
}
