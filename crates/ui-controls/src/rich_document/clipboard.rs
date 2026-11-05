//! Validated inline fragments and bounded HTML clipboard serialization.
//! Platform clipboard ownership and asynchronous completions belong to the host.
use super::{DocumentError, Paragraph, RichRun};
mod css;
mod html;

const MAX_HTML_BYTES: usize = 4 * 1024 * 1024;

/// Styled text from one paragraph or table cell. Paragraph/page metadata and
/// embedded block objects belong to the document, not to this inline fragment.
#[derive(Clone, PartialEq)]
pub struct RichClipboardFragment {
    runs: Vec<RichRun>,
}

impl std::fmt::Debug for RichClipboardFragment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RichClipboardFragment")
            .field("runs", &self.runs.len())
            .field(
                "text_bytes",
                &self.runs.iter().map(|r| r.text.len()).sum::<usize>(),
            )
            .finish()
    }
}

impl RichClipboardFragment {
    /// Parse a bounded inline HTML fragment. Unsupported markup or CSS is an
    /// error; callers may explicitly offer plain-text paste instead.
    pub fn from_html(source: &str, default_style: super::RunStyle) -> Result<Self, DocumentError> {
        html::parse(source, default_style)
    }

    pub fn new(runs: Vec<RichRun>) -> Result<Self, DocumentError> {
        Paragraph {
            runs: runs.clone(),
            ..Default::default()
        }
        .validate()?;
        Ok(Self { runs })
    }

    pub fn runs(&self) -> &[RichRun] {
        &self.runs
    }

    pub fn plain_text(&self) -> String {
        self.runs.iter().map(|run| run.text.as_str()).collect()
    }

    /// Candidate construction preserves paragraph metadata and publishes only
    /// after validating the resulting grapheme/style boundaries and limits.
    pub fn replace_in(
        &self,
        paragraph: &mut Paragraph,
        range: std::ops::Range<usize>,
    ) -> Result<(), DocumentError> {
        paragraph.validate()?;
        paragraph.fragment(range.clone())?;
        let mut runs = paragraph.fragment(0..range.start)?;
        runs.extend(self.runs.iter().cloned());
        runs.extend(paragraph.fragment(range.end..paragraph.plain_text().len())?);
        let mut merged: Vec<RichRun> = Vec::new();
        for run in runs {
            if run.text.is_empty() {
                continue;
            }
            if let Some(last) = merged.last_mut()
                && last.style == run.style
            {
                last.text.push_str(&run.text);
            } else {
                merged.push(run);
            }
        }
        let mut candidate = paragraph.clone();
        candidate.runs = merged;
        candidate.validate()?;
        *paragraph = candidate;
        Ok(())
    }

    /// HTML uses inline CSS with explicit attributes, independent of the
    /// destination document's default font. Whitespace is preserved intentionally.
    pub fn to_html(&self) -> Result<String, DocumentError> {
        let mut output = String::from("<div style=\"white-space:pre-wrap\">");
        for run in &self.runs {
            if run.text.is_empty() {
                continue;
            }
            let mut family = String::new();
            for ch in run.style.family.chars() {
                if matches!(ch, '\\' | '"') {
                    family.push('\\');
                }
                family.push(ch);
            }
            let style = &run.style;
            let css = format!(
                "font-family:\"{family}\";font-size:{}px;font-weight:{};font-style:{};text-decoration:{};color:#{:02x}{:02x}{:02x}",
                style.size,
                if style.bold { 700 } else { 400 },
                if style.italic { "italic" } else { "normal" },
                if style.underline { "underline" } else { "none" },
                style.color[0],
                style.color[1],
                style.color[2]
            );
            push(&mut output, "<span style=\"")?;
            escape(&mut output, &css)?;
            push(&mut output, "\">")?;
            escape(&mut output, &run.text)?;
            push(&mut output, "</span>")?;
        }
        push(&mut output, "</div>")?;
        Ok(output)
    }
}

fn push(output: &mut String, text: &str) -> Result<(), DocumentError> {
    if text.len() > MAX_HTML_BYTES - output.len() {
        return Err(DocumentError::Limit("4 MiB clipboard HTML"));
    }
    output.push_str(text);
    Ok(())
}

fn escape(output: &mut String, text: &str) -> Result<(), DocumentError> {
    for ch in text.chars() {
        match ch {
            '&' => push(output, "&amp;")?,
            '<' => push(output, "&lt;")?,
            '>' => push(output, "&gt;")?,
            '"' => push(output, "&quot;")?,
            '\'' => push(output, "&#39;")?,
            _ => {
                let mut bytes = [0; 4];
                push(output, ch.encode_utf8(&mut bytes))?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rich_document::RunStyle;

    #[test]
    fn html_roundtrip_preserves_every_supported_style_and_whitespace() {
        for family in ["Segoe UI", "A\\B\";color:red;<fake>", "Кириллица", "A'B"] {
            let fragment = RichClipboardFragment::new(vec![
                RichRun {
                    text: "  α\n\n e\u{301} 👩‍💻\t ".into(),
                    style: RunStyle {
                        family: family.into(),
                        size: 18.5,
                        bold: true,
                        italic: true,
                        underline: true,
                        color: [18, 52, 86],
                    },
                },
                RichRun {
                    text: "normal\n ".into(),
                    ..Default::default()
                },
            ])
            .unwrap();
            let parsed =
                RichClipboardFragment::from_html(&fragment.to_html().unwrap(), RunStyle::default())
                    .unwrap();
            assert_eq!(parsed, fragment);
        }
    }
    #[test]
    fn external_inline_html_decodes_entities_nesting_and_css_overrides() {
        let fragment = RichClipboardFragment::from_html(
            "<P>one <B>bold <I>both</I></B> &amp; &#x1f600;<br>line</P><p><span style='font-family: &quot;A\\22 B&quot;;font-size:12pt;color:#abc; font-weight:700'>next</span></p>",
            RunStyle::default()).unwrap();
        assert_eq!(fragment.plain_text(), "one bold both & 😀\nline\nnext");
        assert!(
            fragment
                .runs()
                .iter()
                .any(|r| r.text == "both" && r.style.bold && r.style.italic)
        );
        let last = fragment.runs().last().unwrap();
        assert!(last.text.ends_with("next"));
        assert_eq!(last.style.family, "A\"B");
        assert_eq!(last.style.size, 16.0);
        assert_eq!(last.style.color, [170, 187, 204]);
        let override_style = RichClipboardFragment::from_html(
            "<b><span style='font-weight:normal'>x</span></b>",
            RunStyle::default(),
        )
        .unwrap();
        assert!(!override_style.runs()[0].style.bold);
    }
    #[test]
    fn clipboard_html_rejects_lossy_features_and_malformed_fragments() {
        for source in [
            "<script>alert(1)</script>",
            "<img src='file:///x'>",
            "<a href='https://example.org'>x</a>",
            "<span onclick='x'>x</span>",
            "<span class='red'>x</span>",
            "<span style='background:red'>x</span>",
            "<span style='font-size:NaNpx'>x</span>",
            "<span style='font-size:999px'>x</span>",
            "<span style='color:rgba(1,2,3,0.2)'>x</span>",
            "<span style='color:red!important'>x</span>",
            "<span style='font-family:&quot;unterminated'>x</span>",
            "<b>x</i>",
            "<b>x",
            "<b/>",
            "<span style='' style=''>x</span>",
            "<!DOCTYPE html>x",
            "&#0;",
            "x\0y",
            "e<b>\u{301}</b>",
        ] {
            assert!(
                RichClipboardFragment::from_html(source, RunStyle::default()).is_err(),
                "{source}"
            );
        }
    }
    #[test]
    fn clipboard_parser_enforces_depth_text_run_and_source_budgets() {
        for source in [
            format!("{}x{}", "<b>".repeat(65), "</b>".repeat(65)),
            "x".repeat(1024 * 1024 + 1),
            "<!--x-->".repeat(600_000),
            "<b>x</b><i>y</i>".repeat(129),
            format!("<span style='{}'>x</span>", " ".repeat(4097)),
            "<!--x-->".repeat(16385),
        ] {
            assert!(RichClipboardFragment::from_html(&source, RunStyle::default()).is_err());
        }
        let text = "x".repeat(1024 * 1024);
        assert_eq!(
            RichClipboardFragment::from_html(&text, RunStyle::default())
                .unwrap()
                .plain_text(),
            text
        );
    }

    #[test]
    fn insertion_preserves_neighbor_styles_and_paragraph_metadata() {
        let bold = RunStyle {
            bold: true,
            ..Default::default()
        };
        let italic = RunStyle {
            italic: true,
            ..Default::default()
        };
        let mut paragraph = Paragraph {
            runs: vec![
                RichRun {
                    text: "Left ".into(),
                    style: bold.clone(),
                },
                RichRun {
                    text: "e\u{301} 👩‍💻".into(),
                    ..Default::default()
                },
                RichRun {
                    text: " Right".into(),
                    style: bold.clone(),
                },
            ],
            space_after: 17.0,
            page_break_before: true,
            ..Default::default()
        };
        let fragment = RichClipboardFragment::new(vec![RichRun {
            text: "новое".into(),
            style: italic.clone(),
        }])
        .unwrap();
        let end = "Left e\u{301} 👩‍💻".len();
        fragment.replace_in(&mut paragraph, 5..end).unwrap();
        assert_eq!(paragraph.plain_text(), "Left новое Right");
        assert_eq!(paragraph.runs[0].style, bold);
        assert_eq!(paragraph.runs[1].style, italic);
        assert_eq!(paragraph.runs[2].style, bold);
        assert_eq!(paragraph.space_after, 17.0);
        assert!(paragraph.page_break_before);
    }

    #[test]
    fn malformed_selection_or_join_does_not_change_the_paragraph() {
        let mut paragraph = Paragraph::text("e\u{301} Z");
        let before = paragraph.clone();
        let fragment = RichClipboardFragment::new(vec![RichRun {
            text: "x".into(),
            ..Default::default()
        }])
        .unwrap();
        assert!(fragment.replace_in(&mut paragraph, 1..3).is_err());
        assert_eq!(paragraph, before);
        let fragment = RichClipboardFragment::new(vec![RichRun {
            text: "\u{301}".into(),
            style: RunStyle {
                bold: true,
                ..Default::default()
            },
        }])
        .unwrap();
        let end = paragraph.plain_text().len();
        assert!(fragment.replace_in(&mut paragraph, end..end).is_err());
        assert_eq!(paragraph, before);
    }

    #[test]
    fn serialization_keeps_unicode_whitespace_and_explicit_style() {
        let runs = vec![RichRun {
            text: "A<&\"'\n e\u{301} 👩‍💻  B".into(),
            style: RunStyle {
                family: "Arial".into(),
                size: 18.5,
                bold: true,
                italic: true,
                underline: true,
                color: [18, 52, 86],
            },
        }];
        let fragment = RichClipboardFragment::new(runs.clone()).unwrap();
        let html = fragment.to_html().unwrap();
        assert_eq!(fragment.runs(), runs);
        assert_eq!(fragment.plain_text(), runs[0].text);
        assert!(!format!("{fragment:?}").contains("👩‍💻"));
        assert!(html.contains("white-space:pre-wrap"));
        assert!(html.contains("font-family:&quot;Arial&quot;;font-size:18.5px;font-weight:700;font-style:italic;text-decoration:underline;color:#123456"));
        assert!(html.contains("A&lt;&amp;&quot;&#39;\n e\u{301} 👩‍💻  B"));
    }

    #[test]
    fn font_quotes_and_backslashes_cannot_leave_the_css_string() {
        let family = "A\\B\";color:red;<fake>";
        let fragment = RichClipboardFragment::new(vec![RichRun {
            text: "safe".into(),
            style: RunStyle {
                family: family.into(),
                ..Default::default()
            },
        }])
        .unwrap();
        let html = fragment.to_html().unwrap();
        assert!(html.contains("font-family:&quot;A\\\\B\\&quot;;color:red;&lt;fake&gt;&quot;"));
        assert!(!html.contains("<fake>"));
        assert_eq!(fragment.runs()[0].style.family, family);
    }

    #[test]
    fn malformed_or_oversized_input_is_rejected_without_publication() {
        assert!(
            RichClipboardFragment::new(vec![RichRun {
                text: "bad\0text".into(),
                ..Default::default()
            }])
            .is_err()
        );
        assert!(
            RichClipboardFragment::new(vec![
                RichRun {
                    text: "e".into(),
                    ..Default::default()
                },
                RichRun {
                    text: "\u{301}".into(),
                    style: RunStyle {
                        bold: true,
                        ..Default::default()
                    }
                }
            ])
            .is_err()
        );
        let fragment = RichClipboardFragment::new(vec![RichRun {
            text: "\"".repeat(1024 * 1024),
            ..Default::default()
        }])
        .unwrap();
        assert!(matches!(fragment.to_html(), Err(DocumentError::Limit(_))));
        assert_eq!(fragment.plain_text().len(), 1024 * 1024);
    }
}
