//! One shared fitting plan for measuring and painting bounded display text.
use crate::{PreparedText, TextEngine, TextError};
use rust_desktop_ui_core::{TextRun, TextSpan, TextTrimming};
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone, Debug)]
pub struct PreparedDisplayText {
    pub run: TextRun,
    pub spans: Vec<TextSpan>,
    pub measured: PreparedText,
    pub trimmed: bool,
}

impl TextEngine {
    /// Fits complete graphemes and shapes every candidate as a single paragraph.
    /// `max_lines` is in 1..=256. The logical bounds limit visible height too;
    /// use a generous height when measuring an automatically sized label.
    pub fn fit_text(
        &mut self,
        run: &TextRun,
        spans: &[TextSpan],
        trimming: TextTrimming,
        max_lines: Option<usize>,
    ) -> Result<PreparedDisplayText, TextError> {
        if max_lines.is_some_and(|count| !(1..=256).contains(&count)) {
            return Err(TextError::InvalidRun);
        }
        let measured = self.prepare_styled(run, spans, 1.0)?;
        let fits = |prepared: &PreparedText| {
            max_lines.is_none_or(|count| prepared.line_count() <= count)
                && prepared.height <= run.bounds.height + 0.01
                && prepared.width <= run.bounds.width + 0.01
                && prepared
                    .line_metrics()
                    .iter()
                    .all(|line| line.bounds.width <= run.bounds.width + 0.01)
        };
        if (trimming == TextTrimming::None && max_lines.is_none()) || fits(&measured) {
            return Ok(PreparedDisplayText {
                run: run.clone(),
                spans: spans.to_vec(),
                measured,
                trimmed: false,
            });
        }
        let boundaries: Vec<_> = run
            .text
            .grapheme_indices(true)
            .map(|(index, _)| index)
            .chain(std::iter::once(run.text.len()))
            .collect();
        let words: Vec<_> = run
            .text
            .unicode_word_indices()
            .map(|(index, text)| index + text.len())
            .filter(|end| boundaries.binary_search(end).is_ok())
            .collect();
        let candidate = |end: usize| {
            let mut fitted = run.clone();
            fitted.text.truncate(end);
            if trimming != TextTrimming::None {
                fitted.text.push('…');
            }
            let fitted_spans: Vec<_> = spans
                .iter()
                .filter(|span| span.range.start < end)
                .map(|span| {
                    let mut span = span.clone();
                    span.range.end = span.range.end.min(end);
                    if span.range.end == end {
                        span.range.end = fitted.text.len();
                    }
                    span
                })
                .collect();
            (fitted, fitted_spans)
        };
        let mut low = 0;
        let mut high = boundaries.len();
        while low < high {
            let middle = low + (high - low) / 2;
            let (fitted, fitted_spans) = candidate(boundaries[middle]);
            let prepared = self.prepare_styled(&fitted, &fitted_spans, 1.0)?;
            if fits(&prepared) {
                low = middle + 1;
            } else {
                high = middle;
            }
        }
        let mut end = boundaries[low.saturating_sub(1)];
        if trimming == TextTrimming::WordEllipsis {
            end = words
                .into_iter()
                .take_while(|word| *word <= end)
                .last()
                .unwrap_or(end);
        }
        let (mut fitted, mut fitted_spans) = candidate(end);
        let mut measured = self.prepare_styled(&fitted, &fitted_spans, 1.0)?;
        // A zero-sized box or an ellipsis wider than the box paints no glyphs.
        if !fits(&measured) {
            fitted.text.clear();
            fitted_spans.clear();
            measured = self.prepare(&fitted, 1.0)?;
        }
        Ok(PreparedDisplayText {
            run: fitted,
            spans: fitted_spans,
            measured,
            trimmed: true,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_desktop_ui_core::{Color, Rect};

    #[test]
    fn fitting_retains_complete_graphemes_and_inline_style() {
        let mut engine = TextEngine::new();
        for text in [
            "A👩‍👩‍👧‍👦B e\u{301} long text",
            "Русский длинный заголовок",
            "مرحبا بالعالم שלום עולם",
            "क्\u{200d}ष test text",
        ] {
            let run = TextRun::new(
                text,
                Rect::new(0.0, 0.0, 40.0, 100.0),
                Color::rgb(0, 0, 0),
                16.0,
            );
            let spans = [TextSpan {
                font: None,
                range: 0..text.len(),
                weight: Some(600),
                font_size: None,
                color: Some(Color::rgb(200, 0, 0)),
            }];
            let fitted = engine
                .fit_text(&run, &spans, TextTrimming::CharacterEllipsis, Some(1))
                .unwrap();
            assert!(fitted.trimmed);
            assert!(fitted.measured.width <= run.bounds.width + 0.01);
            let prefix = fitted
                .run
                .text
                .strip_suffix('…')
                .unwrap_or(&fitted.run.text);
            assert!(text.starts_with(prefix));
            assert!(
                text.grapheme_indices(true)
                    .any(|(index, _)| index == prefix.len())
                    || prefix.len() == text.len()
            );
            assert_eq!(fitted.spans[0].range.end, fitted.run.text.len());
        }
    }

    #[test]
    fn styled_cache_excludes_color_but_includes_typography_and_rejects_split_graphemes() {
        let mut engine = TextEngine::new();
        let run = TextRun::new(
            "one e\u{301} two",
            Rect::new(0.0, 0.0, 240.0, 100.0),
            Color::rgb(0, 0, 0),
            16.0,
        );
        let mut spans = [TextSpan {
            font: None,
            range: 0..3,
            font_size: Some(24.0),
            weight: Some(700),
            color: Some(Color::rgb(200, 0, 0)),
        }];
        let first = engine.prepare_styled(&run, &spans, 1.0).unwrap();
        spans[0].color = Some(Color::rgb(0, 100, 200));
        let second = engine.prepare_styled(&run, &spans, 1.0).unwrap();
        assert_eq!(first.width, second.width);
        assert_eq!(engine.stats().layouts, 1);
        assert!(
            second
                .glyphs
                .iter()
                .any(|glyph| glyph.tint == Color::rgb(0, 100, 200))
        );
        spans[0].range = 4..5;
        assert!(engine.prepare_styled(&run, &spans, 1.0).is_err());
    }

    #[test]
    fn color_only_inline_boundaries_preserve_arabic_joining_and_bidi_measurement() {
        let mut engine = TextEngine::new();
        let text = "مرحبا بالعالم שלום 123";
        let run = TextRun::new(
            text,
            Rect::new(0.0, 0.0, 500.0, 100.0),
            Color::rgb(0, 0, 0),
            20.0,
        );
        let plain = engine.prepare(&run, 1.0).unwrap();
        // The first boundary is inside an Arabic word, at a complete grapheme.
        let spans = [
            TextSpan {
                font: None,
                range: 0..4,
                font_size: None,
                weight: None,
                color: Some(Color::rgb(200, 0, 0)),
            },
            TextSpan {
                font: None,
                range: 4..text.len(),
                font_size: None,
                weight: None,
                color: Some(Color::rgb(0, 80, 200)),
            },
        ];
        let styled = engine.prepare_styled(&run, &spans, 1.0).unwrap();
        assert!((plain.width - styled.width).abs() < 0.01);
        assert!((plain.height - styled.height).abs() < 0.01);
        assert_eq!(plain.glyphs.len(), styled.glyphs.len());
        for (plain, styled) in plain.glyphs.iter().zip(&styled.glyphs) {
            assert_eq!(plain.key, styled.key);
            assert_eq!(plain.position, styled.position);
        }
    }

    #[test]
    fn max_lines_and_empty_fit_keep_bounds_and_do_not_split_unicode() {
        let mut engine = TextEngine::new();
        let mut run = TextRun::new(
            "One two three\nSecond 👩‍👩‍👧‍👦 line\nThird line",
            Rect::new(0.0, 0.0, 150.0, 100.0),
            Color::rgb(0, 0, 0),
            16.0,
        );
        run.wrap = true;
        let fitted = engine
            .fit_text(&run, &[], TextTrimming::WordEllipsis, Some(2))
            .unwrap();
        assert!(fitted.trimmed);
        assert!(fitted.measured.line_count() <= 2);
        assert!(fitted.measured.width <= 150.01);
        assert!(fitted.run.text.ends_with('…'));
        run.bounds.width = 0.1;
        let fitted = engine
            .fit_text(&run, &[], TextTrimming::CharacterEllipsis, Some(1))
            .unwrap();
        assert!(fitted.run.text.is_empty());
        assert!(fitted.spans.is_empty());
        assert!(fitted.measured.glyphs.is_empty());
        assert!(
            engine
                .fit_text(&run, &[], TextTrimming::None, Some(0))
                .is_err()
        );
        assert!(
            engine
                .fit_text(&run, &[], TextTrimming::None, Some(257))
                .is_err()
        );
    }
}
