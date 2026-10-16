//! Bounded mask syntax and reversible display/storage formatting.
use super::ControlError;
use crate::UiCulture;
use rust_desktop_ui_text::TextDocument;
use std::ops::Range;

pub const MAX_MASK_SLOTS: usize = 256;
pub const MAX_MASK_PATTERN_BYTES: usize = 4096;
pub const MAX_MASK_VALUE_BYTES: usize = 16 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MaskClass {
    Digit,
    Letter,
    AlphaNumeric,
    Grapheme,
}
impl MaskClass {
    pub(super) fn accepts(self, value: &str, placeholder: char) -> bool {
        if value.is_empty() || value.contains(placeholder) || forbidden(value) {
            return false;
        }
        match self {
            Self::Digit => value.len() == 1 && value.as_bytes()[0].is_ascii_digit(),
            Self::Letter => value.chars().next().is_some_and(char::is_alphabetic),
            Self::AlphaNumeric => value.chars().next().is_some_and(char::is_alphanumeric),
            Self::Grapheme => true,
        }
    }
    fn sample(self, placeholder: char) -> String {
        let primary = if self == Self::Digit { '0' } else { 'A' };
        if primary != placeholder {
            primary.to_string()
        } else if primary == '0' {
            "1".into()
        } else {
            "B".into()
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MaskSeparator {
    Decimal,
    Group,
    Date,
    Time,
}
impl MaskSeparator {
    pub(super) fn text(self, culture: UiCulture) -> &'static str {
        match (self, culture) {
            (Self::Decimal, UiCulture::Russian) => ",",
            (Self::Decimal, UiCulture::English) => ".",
            (Self::Decimal, UiCulture::Arabic) => "٫",
            (Self::Group, UiCulture::Russian) => "\u{a0}",
            (Self::Group, UiCulture::English) => ",",
            (Self::Group, UiCulture::Arabic) => "٬",
            (Self::Date, UiCulture::Russian) => ".",
            (Self::Date, _) => "/",
            (Self::Time, _) => ":",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MaskSegment {
    Literal(String),
    Separator(MaskSeparator),
    Field {
        class: MaskClass,
        minimum: u16,
        maximum: u16,
    },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Atom {
    Literal(String),
    Separator(MaskSeparator),
    Slot { class: MaskClass, required: bool },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MaskPattern {
    pub(super) atoms: Vec<Atom>,
    slots: usize,
}
impl MaskPattern {
    /// 0/9 digits, L/l letters, A/a alphanumeric, X/x graphemes. Lowercase
    /// variants and 9 are optional. Backslash escapes one grapheme; . , / :
    /// are culture separators. Other graphemes are literals.
    pub fn compile(pattern: &str) -> Result<Self, ControlError> {
        if pattern.len() > MAX_MASK_PATTERN_BYTES {
            return Err(invalid("Mask pattern exceeds 4096 bytes"));
        }
        let mut segments = Vec::new();
        let mut glyphs = graphemes(pattern).into_iter();
        while let Some(glyph) = glyphs.next() {
            let segment = match glyph {
                "\\" => MaskSegment::Literal(
                    glyphs
                        .next()
                        .ok_or_else(|| invalid("Dangling mask escape"))?
                        .into(),
                ),
                "0" | "9" | "L" | "l" | "A" | "a" | "X" | "x" => {
                    let class = match glyph {
                        "0" | "9" => MaskClass::Digit,
                        "L" | "l" => MaskClass::Letter,
                        "A" | "a" => MaskClass::AlphaNumeric,
                        _ => MaskClass::Grapheme,
                    };
                    MaskSegment::Field {
                        class,
                        minimum: u16::from(matches!(glyph, "0" | "L" | "A" | "X")),
                        maximum: 1,
                    }
                }
                "." => MaskSegment::Separator(MaskSeparator::Decimal),
                "," => MaskSegment::Separator(MaskSeparator::Group),
                "/" => MaskSegment::Separator(MaskSeparator::Date),
                ":" => MaskSegment::Separator(MaskSeparator::Time),
                _ => MaskSegment::Literal(glyph.into()),
            };
            segments.push(segment);
        }
        Self::from_segments(segments)
    }
    pub fn from_segments(segments: Vec<MaskSegment>) -> Result<Self, ControlError> {
        if segments.len() > MAX_MASK_PATTERN_BYTES {
            return Err(invalid("Too many mask segments"));
        }
        let mut atoms = Vec::new();
        let mut slots = 0usize;
        let mut cost = 0usize;
        for segment in segments {
            match segment {
                MaskSegment::Literal(text) => {
                    if forbidden(&text) {
                        return Err(invalid("Mask literals must be single-line printable text"));
                    }
                    cost = cost.saturating_add(text.len());
                    if !text.is_empty() {
                        atoms.push(Atom::Literal(text));
                    }
                }
                MaskSegment::Separator(separator) => {
                    cost += 1;
                    atoms.push(Atom::Separator(separator));
                }
                MaskSegment::Field {
                    class,
                    minimum,
                    maximum,
                } => {
                    if minimum > maximum || maximum == 0 {
                        return Err(invalid(
                            "Mask fields require 0 <= minimum <= maximum and maximum > 0",
                        ));
                    }
                    slots += usize::from(maximum);
                    cost += usize::from(maximum);
                    if slots > MAX_MASK_SLOTS {
                        return Err(invalid("Mask supports at most 256 slots"));
                    }
                    for i in 0..maximum {
                        atoms.push(Atom::Slot {
                            class,
                            required: i < minimum,
                        });
                    }
                }
            }
            if cost > MAX_MASK_PATTERN_BYTES {
                return Err(invalid("Mask segments exceed 4096 bytes"));
            }
        }
        if slots == 0 {
            return Err(invalid("Mask requires at least one input slot"));
        }
        Ok(Self { atoms, slots })
    }
    pub fn slot_count(&self) -> usize {
        self.slots
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MaskSettings {
    pub pattern: MaskPattern,
    pub culture: UiCulture,
    pub placeholder: char,
    pub allow_empty: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MaskParse {
    pub storage: String,
    pub complete: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct MaskLayout {
    pub display: String,
    pub slots: Vec<Range<usize>>,
}
impl MaskSettings {
    pub fn new(pattern: &str) -> Result<Self, ControlError> {
        let settings = Self {
            pattern: MaskPattern::compile(pattern)?,
            culture: UiCulture::Russian,
            placeholder: '_',
            allow_empty: true,
        };
        settings.validate()?;
        Ok(settings)
    }
    pub fn validate(&self) -> Result<(), ControlError> {
        let prompt = self.placeholder.to_string();
        if forbidden(&prompt)
            || graphemes(&format!("A{prompt}A")).len() != 3
            || graphemes(&format!("{prompt}{prompt}")).len() != 2
        {
            return Err(invalid(
                "Mask placeholder must form a stable printable grapheme",
            ));
        }
        self.layout(&vec![None; self.pattern.slots])
            .map(|_| ())
            .map_err(invalid)
    }
    pub fn validate_storage(&self, storage: &str) -> Result<(), ControlError> {
        self.format(storage).map(|_| ())
    }
    pub fn format(&self, storage: &str) -> Result<String, ControlError> {
        self.validate()?;
        let cells = self.storage_cells(storage).map_err(invalid)?;
        self.layout(&cells).map(|v| v.display).map_err(invalid)
    }
    pub fn sample_value(&self) -> Result<String, ControlError> {
        self.validate()?;
        let mut value = String::new();
        for atom in &self.pattern.atoms {
            if let Atom::Slot {
                class,
                required: true,
            } = atom
            {
                value.push_str(&class.sample(self.placeholder));
            }
        }
        if value.is_empty()
            && !self.allow_empty
            && let Some(Atom::Slot { class, .. }) = self
                .pattern
                .atoms
                .iter()
                .find(|a| matches!(a, Atom::Slot { .. }))
        {
            value = class.sample(self.placeholder);
        }
        self.validate_storage(&value)?;
        Ok(value)
    }
    pub fn parse_display(&self, display: &str) -> Result<MaskParse, String> {
        self.validate().map_err(|e| e.to_string())?;
        let cells = self.display_cells(display)?;
        Ok(self.parsed(&cells))
    }
    pub(super) fn parsed(&self, cells: &[Option<String>]) -> MaskParse {
        let mut storage = String::new();
        let mut complete = true;
        let mut index = 0;
        for atom in &self.pattern.atoms {
            if let Atom::Slot { required, .. } = atom {
                if let Some(value) = &cells[index] {
                    storage.push_str(value);
                } else if *required {
                    complete = false;
                }
                index += 1;
            }
        }
        if storage.is_empty() {
            complete = self.allow_empty;
        }
        MaskParse { storage, complete }
    }
    pub(super) fn storage_cells(&self, storage: &str) -> Result<Vec<Option<String>>, String> {
        if storage.len() > MAX_MASK_VALUE_BYTES {
            return Err("Mask value exceeds 16 KiB".into());
        }
        let input = graphemes(storage);
        let count = input.len();
        if count > self.pattern.slots {
            return Err("Value has more graphemes than mask slots".into());
        }
        if count == 0 && self.allow_empty {
            return Ok(vec![None; self.pattern.slots]);
        }
        if count == 0 {
            return Err("Masked value cannot be empty".into());
        }
        let slots: Vec<_> = self
            .pattern
            .atoms
            .iter()
            .filter_map(|a| {
                if let Atom::Slot { class, required } = a {
                    Some((*class, *required))
                } else {
                    None
                }
            })
            .collect();
        // At most 257*257 cells. No recursive or exponential pattern matching.
        let width = count + 1;
        let mut possible = vec![false; (slots.len() + 1) * width];
        possible[slots.len() * width + count] = true;
        for i in (0..slots.len()).rev() {
            for j in (0..=count).rev() {
                possible[i * width + j] = (!slots[i].1 && possible[(i + 1) * width + j])
                    || (j < count
                        && slots[i].0.accepts(input[j], self.placeholder)
                        && possible[(i + 1) * width + j + 1]);
            }
        }
        if !possible[0] {
            return Err(
                "Storage does not satisfy required mask slots or contains the reserved placeholder"
                    .into(),
            );
        }
        let mut cells = Vec::with_capacity(slots.len());
        let mut j = 0;
        for (i, (class, _)) in slots.iter().enumerate() {
            if j < count
                && class.accepts(input[j], self.placeholder)
                && possible[(i + 1) * width + j + 1]
            {
                cells.push(Some(input[j].to_owned()));
                j += 1;
            } else {
                cells.push(None);
            }
        }
        Ok(cells)
    }
    pub(super) fn display_cells(&self, display: &str) -> Result<Vec<Option<String>>, String> {
        if display.len() > MAX_MASK_VALUE_BYTES + MAX_MASK_PATTERN_BYTES * 4 {
            return Err("Masked display exceeds its text budget".into());
        }
        let boundaries = TextDocument::new(display).grapheme_boundaries();
        let mut offset = 0;
        let mut cells = Vec::with_capacity(self.pattern.slots);
        for atom in &self.pattern.atoms {
            match atom {
                Atom::Literal(text) => {
                    if !display[offset..].starts_with(text) {
                        return Err("Mask literal does not match".into());
                    }
                    offset += text.len();
                }
                Atom::Separator(separator) => {
                    let text = separator.text(self.culture);
                    if !display[offset..].starts_with(text) {
                        return Err("Mask culture separator does not match".into());
                    }
                    offset += text.len();
                }
                Atom::Slot { class, .. } => {
                    let index = boundaries
                        .binary_search(&offset)
                        .map_err(|_| "Mask boundary splits a grapheme")?;
                    let end = *boundaries.get(index + 1).ok_or("Missing mask slot")?;
                    let glyph = &display[offset..end];
                    if glyph == self.placeholder.to_string() {
                        cells.push(None);
                    } else if class.accepts(glyph, self.placeholder) {
                        cells.push(Some(glyph.into()));
                    } else {
                        return Err("Character does not satisfy mask slot".into());
                    }
                    offset = end;
                }
            }
            if boundaries.binary_search(&offset).is_err() {
                return Err("Mask literal or slot boundary joins a grapheme".into());
            }
        }
        if offset != display.len() {
            return Err("Unexpected text after mask".into());
        }
        // Removing literals must not merge neighboring data graphemes (for
        // example regional indicators or Hangul Jamo separated by a dash).
        let values: Vec<_> = cells.iter().filter_map(|value| value.as_deref()).collect();
        let storage: String = values.concat();
        if storage.len() > MAX_MASK_VALUE_BYTES {
            return Err("Mask value exceeds 16 KiB".into());
        }
        if graphemes(&storage) != values {
            return Err("Mask data boundaries join when literals are removed".into());
        }
        Ok(cells)
    }
    pub(super) fn layout(&self, cells: &[Option<String>]) -> Result<MaskLayout, String> {
        if cells.len() != self.pattern.slots {
            return Err("Mask slot count mismatch".into());
        }
        let mut display = String::new();
        let mut ranges = Vec::with_capacity(cells.len());
        let mut index = 0;
        for atom in &self.pattern.atoms {
            match atom {
                Atom::Literal(text) => display.push_str(text),
                Atom::Separator(separator) => display.push_str(separator.text(self.culture)),
                Atom::Slot { class, .. } => {
                    let start = display.len();
                    if let Some(value) = &cells[index] {
                        if graphemes(value).len() != 1 || !class.accepts(value, self.placeholder) {
                            return Err("Invalid mask slot value".into());
                        }
                        display.push_str(value);
                    } else {
                        display.push(self.placeholder);
                    }
                    ranges.push(start..display.len());
                    index += 1;
                }
            }
        }
        // Validate boundaries after concatenation: combining/joining input must
        // not silently merge a data slot with a literal or adjacent placeholder.
        if self.display_cells(&display)? != cells {
            return Err("Mask display is not reversible".into());
        }
        Ok(MaskLayout {
            display,
            slots: ranges,
        })
    }
}
pub(super) fn graphemes(text: &str) -> Vec<&str> {
    TextDocument::new(text)
        .grapheme_boundaries()
        .windows(2)
        .map(|r| &text[r[0]..r[1]])
        .collect()
}
fn forbidden(text: &str) -> bool {
    text.chars()
        .any(|c| c.is_control() || matches!(c, '\u{2028}' | '\u{2029}'))
}
fn invalid(message: impl Into<String>) -> ControlError {
    ControlError::Invalid(message.into())
}
