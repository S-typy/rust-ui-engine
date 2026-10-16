//! Renderer-independent text commands in logical pixels.
use crate::{Color, Rect};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TextTrimming {
    #[default]
    None,
    CharacterEllipsis,
    WordEllipsis,
}

/// A nonoverlapping UTF-8 range with optional inline typography. Ranges are
/// checked again against grapheme boundaries by the shaping engine.
#[derive(Clone, Debug, PartialEq)]
pub struct TextSpan {
    pub range: std::ops::Range<usize>,
    pub font_size: Option<f32>,
    pub weight: Option<u16>,
    pub color: Option<Color>,
}

/// Bounded scene validation, without depending on a Unicode or font library.
pub fn valid_text_spans(text: &str, spans: &[TextSpan]) -> bool {
    let mut end = 0;
    spans.len() <= 256
        && spans.iter().all(|span| {
            let valid = span.range.start >= end
                && span.range.end > span.range.start
                && span.range.end <= text.len()
                && text.is_char_boundary(span.range.start)
                && text.is_char_boundary(span.range.end)
                && span
                    .font_size
                    .is_none_or(|size| size.is_finite() && size > 0.0 && size <= 512.0)
                && span
                    .weight
                    .is_none_or(|weight| (1..=1000).contains(&weight))
                && span.color.is_none_or(|color| {
                    [color.r, color.g, color.b, color.a]
                        .into_iter()
                        .all(|c| c.is_finite() && (0.0..=1.0).contains(&c))
                });
            end = span.range.end;
            valid
        })
}

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
    Image(usize),
    Rectangle(usize),
    RoundedRectangle(usize),
    StrokePath(usize),
    Text(usize),
}
