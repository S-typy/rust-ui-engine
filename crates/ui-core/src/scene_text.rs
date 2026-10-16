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
#[derive(Clone, PartialEq)]
pub struct TextRun {
    pub bounds: Rect,
    pub clip: Rect,
    pub text: String,
    pub font_size: f32,
    pub color: Color,
    pub weight: u16,
    pub align: TextAlign,
    pub wrap: bool,
    /// The text is intentionally visible, but must be redacted in diagnostics
    /// and excluded from persistent shaping caches. Renderers still need the
    /// plaintext for the lifetime of this frame; this is not memory encryption.
    pub sensitive: bool,
}

impl std::fmt::Debug for TextRun {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TextRun")
            .field("bounds", &self.bounds)
            .field("clip", &self.clip)
            .field(
                "text",
                &if self.sensitive {
                    "[REDACTED]"
                } else {
                    &self.text
                },
            )
            .field("font_size", &self.font_size)
            .field("color", &self.color)
            .field("weight", &self.weight)
            .field("align", &self.align)
            .field("wrap", &self.wrap)
            .field("sensitive", &self.sensitive)
            .finish()
    }
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
            sensitive: false,
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

#[cfg(test)]
mod privacy_tests {
    use super::*;

    #[test]
    fn sensitive_text_is_redacted_after_scene_composition() {
        let bounds = Rect::new(0.0, 0.0, 100.0, 30.0);
        let mut run = TextRun::new("private-value", bounds, Color::rgb(0, 0, 0), 14.0);
        run.sensitive = true;
        let mut source = crate::Scene::default();
        source.text(run);
        let mut scene = crate::Scene::default();
        scene
            .append_view(&source, crate::Point::ZERO, bounds)
            .unwrap();
        assert!(scene.texts[0].sensitive);
        assert_eq!(scene.texts[0].text, "private-value");
        assert!(!format!("{scene:?}").contains("private-value"));
        assert!(format!("{scene:?}").contains("[REDACTED]"));
    }
}
