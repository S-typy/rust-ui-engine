//! Bounded rich documents, atomic operations, fragments and paginated layout.
//! The model has no window, filesystem or GPU dependency.

mod docx;
mod layout;
pub use docx::*;
pub use layout::*;

use rust_desktop_ui_text::TextDocument;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fmt, ops::Range};

pub const MAX_DOCUMENT_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_DOCUMENT_BLOCKS: usize = 4096;
pub const MAX_DOCUMENT_IMAGES: usize = 64;
pub const MAX_DOCUMENT_MEDIA_BYTES: usize = 32 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DocumentError {
    Invalid(String),
    Limit(&'static str),
    Unsupported(String),
    Codec(String),
    CompositionActive,
}
impl fmt::Display for DocumentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(s) => write!(f, "Invalid document: {s}"),
            Self::Limit(s) => write!(f, "Document limit: {s}"),
            Self::Unsupported(s) => write!(f, "Unsupported document feature: {s}"),
            Self::Codec(s) => write!(f, "Document codec: {s}"),
            Self::CompositionActive => {
                f.write_str("Finish composition before formatting or changing views")
            }
        }
    }
}
impl std::error::Error for DocumentError {}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunStyle {
    pub family: String,
    pub size: f32,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub color: [u8; 3],
}
impl Default for RunStyle {
    fn default() -> Self {
        Self {
            family: "Segoe UI".into(),
            size: 16.0,
            bold: false,
            italic: false,
            underline: false,
            color: [24, 24, 24],
        }
    }
}
impl RunStyle {
    pub fn validate(&self) -> Result<(), DocumentError> {
        if self.family.trim().is_empty()
            || self.family.len() > 256
            || self.family.chars().any(char::is_control)
            || !self.size.is_finite()
            || !(4.0..=192.0).contains(&self.size)
        {
            return Err(DocumentError::Invalid("run font".into()));
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RichRun {
    pub text: String,
    pub style: RunStyle,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ParagraphAlignment {
    #[default]
    Start,
    Center,
    End,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Paragraph {
    pub runs: Vec<RichRun>,
    pub alignment: ParagraphAlignment,
    pub space_after: f32,
    pub page_break_before: bool,
    pub style: Option<String>,
}
impl Default for Paragraph {
    fn default() -> Self {
        Self {
            runs: Vec::new(),
            alignment: ParagraphAlignment::Start,
            space_after: 8.0,
            page_break_before: false,
            style: None,
        }
    }
}
impl Paragraph {
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            runs: vec![RichRun {
                text: text.into(),
                style: RunStyle::default(),
            }],
            ..Default::default()
        }
    }
    pub fn plain_text(&self) -> String {
        self.runs.iter().map(|r| r.text.as_str()).collect()
    }
    pub fn validate(&self) -> Result<(), DocumentError> {
        if self.runs.len() > 256 {
            return Err(DocumentError::Limit("256 runs per paragraph"));
        }
        if !self.space_after.is_finite() || !(0.0..=512.0).contains(&self.space_after) {
            return Err(DocumentError::Invalid("paragraph spacing".into()));
        }
        let text = self.plain_text();
        if text.len() > 1024 * 1024 {
            return Err(DocumentError::Limit("paragraph text"));
        }
        if text.contains(['\r', '\0']) {
            return Err(DocumentError::Invalid(
                "paragraph contains CR or NUL".into(),
            ));
        }
        let boundaries = TextDocument::new(&text).grapheme_boundaries();
        let mut end = 0;
        for run in &self.runs {
            run.style.validate()?;
            end += run.text.len();
            if boundaries.binary_search(&end).is_err() {
                return Err(DocumentError::Invalid("style splits a grapheme".into()));
            }
        }
        Ok(())
    }
    pub fn replace(
        &mut self,
        range: Range<usize>,
        replacement: &str,
        style: RunStyle,
    ) -> Result<(), DocumentError> {
        self.check_range(&range)?;
        style.validate()?;
        let mut runs = self.slice(0..range.start);
        if !replacement.is_empty() {
            runs.push(RichRun {
                text: replacement.into(),
                style,
            });
        }
        runs.extend(self.slice(range.end..self.plain_text().len()));
        let mut candidate = self.clone();
        candidate.runs = merge_runs(runs);
        candidate.validate()?;
        *self = candidate;
        Ok(())
    }
    pub fn format(&mut self, range: Range<usize>, style: RunStyle) -> Result<(), DocumentError> {
        style.validate()?;
        self.map_styles(range, |current| *current = style.clone())
    }
    /// Change selected attributes while preserving the other run properties.
    pub fn map_styles(
        &mut self,
        range: Range<usize>,
        mut map: impl FnMut(&mut RunStyle),
    ) -> Result<(), DocumentError> {
        self.check_range(&range)?;
        if range.is_empty() {
            return Ok(());
        }
        let mut runs = self.slice(0..range.start);
        for mut selected in self.slice(range.clone()) {
            map(&mut selected.style);
            runs.push(selected);
        }
        runs.extend(self.slice(range.end..self.plain_text().len()));
        let mut candidate = self.clone();
        candidate.runs = merge_runs(runs);
        candidate.validate()?;
        *self = candidate;
        Ok(())
    }
    pub fn style_at(&self, byte: usize) -> RunStyle {
        let mut offset = 0;
        for run in &self.runs {
            offset += run.text.len();
            if byte < offset {
                return run.style.clone();
            }
        }
        self.runs
            .last()
            .map(|r| r.style.clone())
            .unwrap_or_default()
    }
    pub fn fragment(&self, range: Range<usize>) -> Result<Vec<RichRun>, DocumentError> {
        self.check_range(&range)?;
        Ok(self.slice(range))
    }
    fn check_range(&self, range: &Range<usize>) -> Result<(), DocumentError> {
        let boundaries = TextDocument::new(self.plain_text()).grapheme_boundaries();
        if range.start > range.end
            || boundaries.binary_search(&range.start).is_err()
            || boundaries.binary_search(&range.end).is_err()
        {
            return Err(DocumentError::Invalid(
                "range must use grapheme boundaries".into(),
            ));
        }
        Ok(())
    }
    fn slice(&self, range: Range<usize>) -> Vec<RichRun> {
        let mut offset = 0;
        let mut result = Vec::new();
        for run in &self.runs {
            let end = offset + run.text.len();
            let a = range.start.max(offset);
            let b = range.end.min(end);
            if a < b {
                result.push(RichRun {
                    text: run.text[a - offset..b - offset].into(),
                    style: run.style.clone(),
                });
            }
            offset = end;
        }
        result
    }
}
fn merge_runs(runs: Vec<RichRun>) -> Vec<RichRun> {
    let mut result: Vec<RichRun> = Vec::new();
    for run in runs {
        if run.text.is_empty() {
            continue;
        }
        if let Some(last) = result.last_mut()
            && last.style == run.style
        {
            last.text.push_str(&run.text);
        } else {
            result.push(run);
        }
    }
    result
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RichTable {
    pub columns: Vec<f32>,
    pub rows: Vec<Vec<Paragraph>>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RichImage {
    pub media: String,
    pub width: f32,
    pub height: f32,
    pub alt: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", deny_unknown_fields)]
pub enum RichBlock {
    Paragraph(Paragraph),
    Table(RichTable),
    Image(RichImage),
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DocumentMedia {
    pub content_type: String,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PageSettings {
    pub width: f32,
    pub height: f32,
    pub margin: f32,
    /// Optional top/right/bottom/left margins; `margin` is the uniform fallback.
    #[serde(default)]
    pub margins: Option<[f32; 4]>,
}
impl Default for PageSettings {
    fn default() -> Self {
        Self {
            width: 794.0,
            height: 1123.0,
            margin: 64.0,
            margins: None,
        }
    }
}
impl PageSettings {
    pub fn margins(self) -> [f32; 4] {
        self.margins.unwrap_or([self.margin; 4])
    }
    pub fn content_width(self) -> f32 {
        let m = self.margins();
        self.width - m[1] - m[3]
    }
    pub fn content_height(self) -> f32 {
        let m = self.margins();
        self.height - m[0] - m[2]
    }
    pub fn validate(self) -> Result<(), DocumentError> {
        if ![self.width, self.height, self.margin]
            .iter()
            .all(|v| v.is_finite())
            || !(100.0..=4096.0).contains(&self.width)
            || !(100.0..=8192.0).contains(&self.height)
            || self.margin < 0.0
            || self.margins().iter().any(|v| !v.is_finite() || *v < 0.0)
            || self.content_width() <= 32.0
            || self.content_height() <= 32.0
        {
            return Err(DocumentError::Invalid("page geometry".into()));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RichDocument {
    pub version: u32,
    pub page: PageSettings,
    pub styles: BTreeMap<String, RunStyle>,
    pub media: BTreeMap<String, DocumentMedia>,
    pub blocks: Vec<RichBlock>,
}
impl Default for RichDocument {
    fn default() -> Self {
        Self {
            version: 1,
            page: PageSettings::default(),
            styles: BTreeMap::new(),
            media: BTreeMap::new(),
            blocks: vec![RichBlock::Paragraph(Paragraph::default())],
        }
    }
}
impl RichDocument {
    pub fn from_text(text: &str) -> Result<Self, DocumentError> {
        let value = Self {
            blocks: text
                .replace("\r\n", "\n")
                .split('\n')
                .map(|line| RichBlock::Paragraph(Paragraph::text(line)))
                .collect(),
            ..Default::default()
        };
        value.validate()?;
        Ok(value)
    }
    pub fn plain_text(&self) -> String {
        self.blocks
            .iter()
            .map(|b| match b {
                RichBlock::Paragraph(p) => p.plain_text(),
                RichBlock::Table(t) => t
                    .rows
                    .iter()
                    .map(|row| {
                        row.iter()
                            .map(Paragraph::plain_text)
                            .collect::<Vec<_>>()
                            .join("\t")
                    })
                    .collect::<Vec<_>>()
                    .join("\n"),
                RichBlock::Image(i) => format!("[{}]", i.alt),
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
    pub fn validate(&self) -> Result<(), DocumentError> {
        if self.version != 1 {
            return Err(DocumentError::Unsupported(format!(
                "version {}",
                self.version
            )));
        }
        self.page.validate()?;
        if self.blocks.is_empty() || self.blocks.len() > MAX_DOCUMENT_BLOCKS {
            return Err(DocumentError::Limit("1..4096 blocks"));
        }
        if self.styles.len() > 128 || self.media.len() > MAX_DOCUMENT_IMAGES {
            return Err(DocumentError::Limit("styles/media"));
        }
        for (key, style) in &self.styles {
            valid_key(key)?;
            style.validate()?;
        }
        let mut media_bytes = 0usize;
        for (key, media) in &self.media {
            valid_key(key)?;
            if !matches!(media.content_type.as_str(), "image/png" | "image/jpeg") {
                return Err(DocumentError::Unsupported("media type".into()));
            }
            media_bytes = media_bytes.saturating_add(media.bytes.len());
            if media.bytes.len() > 16 * 1024 * 1024
                || media.bytes.is_empty()
                || media_bytes > MAX_DOCUMENT_MEDIA_BYTES
            {
                return Err(DocumentError::Limit("media bytes"));
            }
        }
        let mut text_bytes = 0usize;
        let mut paragraphs = 0usize;
        for block in &self.blocks {
            match block {
                RichBlock::Paragraph(p) => {
                    self.validate_paragraph(p, &mut text_bytes, &mut paragraphs)?
                }
                RichBlock::Table(t) => {
                    if t.columns.is_empty()
                        || t.columns.len() > 32
                        || t.rows.is_empty()
                        || t.rows.len() > 1024
                        || t.columns.iter().any(|v| !v.is_finite() || *v < 16.0)
                        || t.columns.iter().sum::<f32>() > self.page.content_width() + 0.1
                    {
                        return Err(DocumentError::Invalid("table geometry".into()));
                    }
                    for row in &t.rows {
                        if row.len() != t.columns.len() {
                            return Err(DocumentError::Invalid("ragged table".into()));
                        }
                        for p in row {
                            self.validate_paragraph(p, &mut text_bytes, &mut paragraphs)?;
                        }
                    }
                }
                RichBlock::Image(i) => {
                    if !self.media.contains_key(&i.media)
                        || !i.width.is_finite()
                        || !i.height.is_finite()
                        || i.width <= 0.0
                        || i.height <= 0.0
                        || i.width > self.page.content_width()
                        || i.height > self.page.content_height()
                        || i.alt.len() > 4096
                    {
                        return Err(DocumentError::Invalid("image reference or geometry".into()));
                    }
                }
            }
        }
        Ok(())
    }
    fn validate_paragraph(
        &self,
        p: &Paragraph,
        bytes: &mut usize,
        paragraphs: &mut usize,
    ) -> Result<(), DocumentError> {
        *bytes = bytes.saturating_add(p.runs.iter().map(|r| r.text.len()).sum::<usize>());
        *paragraphs += 1;
        if *bytes > MAX_DOCUMENT_BYTES || *paragraphs > 16384 {
            return Err(DocumentError::Limit("document text/paragraphs"));
        }
        p.validate()?;
        if p.style
            .as_ref()
            .is_some_and(|s| !self.styles.contains_key(s))
        {
            return Err(DocumentError::Invalid("unknown paragraph style".into()));
        }
        Ok(())
    }
    /// Candidate publication is atomic, including cross references and limits.
    pub fn transact(
        &mut self,
        operation: impl FnOnce(&mut Self) -> Result<(), DocumentError>,
    ) -> Result<bool, DocumentError> {
        let mut next = self.clone();
        operation(&mut next)?;
        next.validate()?;
        if *self == next {
            return Ok(false);
        }
        *self = next;
        Ok(true)
    }
    pub fn to_json(&self) -> Result<String, DocumentError> {
        self.validate()?;
        serde_json::to_string_pretty(self).map_err(|e| DocumentError::Codec(e.to_string()))
    }
    pub fn from_json(text: &str) -> Result<Self, DocumentError> {
        if text.len() > 192 * 1024 * 1024 {
            return Err(DocumentError::Limit("JSON input"));
        }
        let value: Self =
            serde_json::from_str(text).map_err(|e| DocumentError::Codec(e.to_string()))?;
        value.validate()?;
        Ok(value)
    }
    pub fn fragment(&self, range: Range<usize>) -> Result<Self, DocumentError> {
        if range.start >= range.end || range.end > self.blocks.len() {
            return Err(DocumentError::Invalid("block range".into()));
        }
        let mut next = self.clone();
        next.blocks = self.blocks[range].to_vec();
        next.media.retain(|key, _| {
            next.blocks
                .iter()
                .any(|b| matches!(b, RichBlock::Image(i) if &i.media==key))
        });
        next.validate()?;
        Ok(next)
    }
    pub fn insert_fragment(
        &mut self,
        index: usize,
        fragment: &Self,
    ) -> Result<bool, DocumentError> {
        fragment.validate()?;
        self.transact(|next| {
            if index > next.blocks.len() {
                return Err(DocumentError::Invalid("block index".into()));
            }
            for (k, v) in &fragment.styles {
                if next.styles.get(k).is_some_and(|old| old != v) {
                    return Err(DocumentError::Invalid("fragment style conflict".into()));
                }
                next.styles.insert(k.clone(), v.clone());
            }
            for (k, v) in &fragment.media {
                if next.media.get(k).is_some_and(|old| old != v) {
                    return Err(DocumentError::Invalid("fragment media conflict".into()));
                }
                next.media.insert(k.clone(), v.clone());
            }
            next.blocks.splice(index..index, fragment.blocks.clone());
            Ok(())
        })
    }
    pub fn apply_paragraph_style(
        &mut self,
        index: usize,
        key: &str,
    ) -> Result<bool, DocumentError> {
        self.transact(|next| {
            let style = next
                .styles
                .get(key)
                .cloned()
                .ok_or_else(|| DocumentError::Invalid("unknown style".into()))?;
            let Some(RichBlock::Paragraph(p)) = next.blocks.get_mut(index) else {
                return Err(DocumentError::Invalid("paragraph index".into()));
            };
            p.style = Some(key.into());
            for run in &mut p.runs {
                run.style = style.clone();
            }
            Ok(())
        })
    }
}
fn valid_key(key: &str) -> Result<(), DocumentError> {
    if key.is_empty()
        || key.len() > 128
        || !key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    {
        return Err(DocumentError::Invalid("resource key".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
