//! Neutral text geometry for native accessibility adapters.
use super::*;

/// A logical sequence of at most 256 extended graphemes on one visual line.
/// Coordinates are physical pixels, like the other PreparedText geometry APIs.
/// No text is exposed: ranges refer to the caller's supplied text.
#[derive(Clone, Debug, PartialEq)]
pub struct AccessibleTextRun {
    pub range: Range<usize>,
    pub line: usize,
    pub bounds: Rect,
    pub rtl: bool,
    pub character_lengths: Vec<u8>,
    pub character_positions: Vec<f32>,
    pub character_widths: Vec<f32>,
    pub word_starts: Vec<u8>,
}

struct ClusterBox {
    range: Range<usize>,
    line: usize,
    bounds: Rect,
    rtl: bool,
}

fn union(a: Rect, b: Rect) -> Rect {
    let x = a.x.min(b.x);
    let y = a.y.min(b.y);
    Rect::new(
        x,
        y,
        (a.x + a.width).max(b.x + b.width) - x,
        (a.y + a.height).max(b.y + b.height) - y,
    )
}

impl PreparedText {
    /// Visible text area in the same physical coordinate space as the runs.
    pub fn text_clip(&self) -> Rect {
        self.clip
    }
    /// Project the exact supplied text using shaped clusters and grapheme edges.
    /// Reject mismatched/stale layouts; callers must never expose protected text.
    /// The independent 4096-run budget bounds a native accessibility tree.
    pub fn accessible_runs(&self, text: &str) -> Result<Vec<AccessibleTextRun>, TextError> {
        if text != self.text.as_ref() {
            return Err(TextError::AccessibilityLimit);
        }
        let mut clusters = Vec::new();
        for (line_index, line) in self.layout.lines().enumerate() {
            let metrics = line.metrics();
            let mut x = self.origin.x + metrics.offset;
            for run in line.runs() {
                for cluster in run.visual_clusters() {
                    let advance = cluster.advance();
                    clusters.push(ClusterBox {
                        range: cluster.text_range(),
                        line: line_index,
                        rtl: run.is_rtl(),
                        bounds: Rect::new(
                            x,
                            self.origin.y + metrics.block_min_coord,
                            advance.max(0.0),
                            metrics.block_max_coord - metrics.block_min_coord,
                        ),
                    });
                    x += advance;
                }
            }
        }
        clusters.sort_by_key(|cluster| (cluster.range.start, cluster.range.end));
        let word_starts: HashSet<_> = text.unicode_word_indices().map(|(i, _)| i).collect();
        let mut result: Vec<AccessibleTextRun> = Vec::new();
        let mut cursor = 0;
        for (start, grapheme) in text.grapheme_indices(true) {
            let end = start + grapheme.len();
            while cursor < clusters.len() && clusters[cursor].range.end <= start {
                cursor += 1;
            }
            let first = clusters
                .get(cursor)
                .filter(|c| c.range.start < end)
                .ok_or(TextError::AccessibilityLimit)?;
            let mut bounds = first.bounds;
            for cluster in clusters[cursor + 1..]
                .iter()
                .take_while(|c| c.range.start < end)
            {
                if cluster.line != first.line {
                    return Err(TextError::AccessibilityLimit);
                }
                bounds = union(bounds, cluster.bounds);
            }
            if result.last().is_none_or(|run| {
                run.line != first.line
                    || run.rtl != first.rtl
                    || run.range.end != start
                    || run.character_lengths.len() == 256
            }) {
                if result.len() == 4096 {
                    return Err(TextError::AccessibilityLimit);
                }
                result.push(AccessibleTextRun {
                    range: start..start,
                    line: first.line,
                    bounds,
                    rtl: first.rtl,
                    character_lengths: Vec::new(),
                    character_positions: Vec::new(),
                    character_widths: Vec::new(),
                    word_starts: Vec::new(),
                });
            }
            let run = result.last_mut().unwrap();
            if word_starts.contains(&start) || start == 0 || text[..start].ends_with(['\n', '\r']) {
                run.word_starts.push(run.character_lengths.len() as u8);
            }
            run.range.end = end;
            run.bounds = union(run.bounds, bounds);
            run.character_lengths
                .push(u8::try_from(grapheme.len()).map_err(|_| TextError::AccessibilityLimit)?);
            // Temporarily store absolute left edges; normalize after each run
            // has its complete bounds (especially for RTL visual order).
            run.character_positions.push(bounds.x);
            run.character_widths.push(bounds.width);
        }
        if text.is_empty() || text.ends_with(['\n', '\r']) {
            if result.len() == 4096 {
                return Err(TextError::AccessibilityLimit);
            }
            let bounds = self.caret_rect(text.len());
            result.push(AccessibleTextRun {
                range: text.len()..text.len(),
                line: self.line_count().saturating_sub(1),
                bounds,
                rtl: false,
                character_lengths: Vec::new(),
                character_positions: Vec::new(),
                character_widths: Vec::new(),
                word_starts: Vec::new(),
            });
        }
        for run in &mut result {
            for (x, width) in run
                .character_positions
                .iter_mut()
                .zip(&run.character_widths)
            {
                *x = if run.rtl {
                    run.bounds.x + run.bounds.width - *x - width
                } else {
                    *x - run.bounds.x
                };
                *x = x.max(0.0);
            }
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn logical_text_and_graphemes_survive_bidi_wrap_and_trailing_empty_line() {
        let mut engine = TextEngine::new();
        for value in [
            "",
            "office fi e\u{301} 😀",
            "English אבג العربية tail",
            "one\ntwo\n",
        ] {
            let text = TextRun::new(
                value,
                Rect::new(12.0, 20.0, 90.0, 400.0),
                Color::rgb(0, 0, 0),
                16.0,
            );
            let prepared = engine.prepare(&text, 1.0).unwrap();
            let runs = prepared.accessible_runs(value).unwrap();
            assert_eq!(
                runs.iter()
                    .map(|r| &value[r.range.clone()])
                    .collect::<String>(),
                value
            );
            assert_eq!(
                runs.iter()
                    .map(|r| r.character_lengths.len())
                    .sum::<usize>(),
                value.graphemes(true).count()
            );
            for run in &runs {
                assert!(run.bounds.is_valid());
                assert_eq!(
                    run.character_lengths
                        .iter()
                        .map(|v| usize::from(*v))
                        .sum::<usize>(),
                    run.range.len()
                );
                assert_eq!(run.character_positions.len(), run.character_lengths.len());
                assert!(
                    run.character_positions
                        .iter()
                        .all(|x| x.is_finite() && *x >= 0.0)
                );
                let mut byte = run.range.start;
                for (index, length) in run.character_lengths.iter().enumerate() {
                    let end = byte + usize::from(*length);
                    if !value[byte..end].contains(['\n', '\r']) {
                        let expected = prepared.selection_rects(byte..end);
                        if expected.len() == 1 {
                            let x = if run.rtl {
                                run.bounds.x + run.bounds.width
                                    - run.character_positions[index]
                                    - run.character_widths[index]
                            } else {
                                run.bounds.x + run.character_positions[index]
                            };
                            assert!(
                                (expected[0].x - x).abs() < 0.75,
                                "{value:?} {byte}: {:?} versus {x}",
                                expected[0]
                            );
                            assert!((expected[0].width - run.character_widths[index]).abs() < 0.75);
                        }
                    }
                    byte = end;
                }
            }
            if value.ends_with('\n') {
                assert!(runs.last().unwrap().range.is_empty());
            }
            assert!(prepared.accessible_runs("stale").is_err());
        }
    }
    #[test]
    fn long_lines_chunk_without_splitting_graphemes_and_line_budget_is_explicit() {
        let mut engine = TextEngine::new();
        let value = "e\u{301}".repeat(600);
        let text = TextRun::new(
            &value,
            Rect::new(0.0, 0.0, 20000.0, 100.0),
            Color::rgb(0, 0, 0),
            16.0,
        );
        let layout = engine.prepare(&text, 1.0).unwrap();
        let runs = layout.accessible_runs(&value).unwrap();
        assert!(runs.len() >= 3);
        assert!(runs.iter().all(
            |r| r.character_lengths.len() <= 256 && r.character_lengths.iter().all(|v| *v == 3)
        ));
        let value = "\n".repeat(4096);
        let text = TextRun::new(
            &value,
            Rect::new(0.0, 0.0, 100.0, 100.0),
            Color::rgb(0, 0, 0),
            16.0,
        );
        assert_eq!(
            engine
                .prepare(&text, 1.0)
                .unwrap()
                .accessible_runs(&value)
                .unwrap_err(),
            TextError::AccessibilityLimit
        );
    }
}
