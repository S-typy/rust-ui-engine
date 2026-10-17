use std::{collections::VecDeque, fmt, ops::Range, sync::Arc};
use unicode_segmentation::UnicodeSegmentation;

const HISTORY_LIMIT: usize = 128;

/// Limits for committed text and the transient IME display text.
///
/// `max_graphemes` counts extended grapheme clusters in the final committed
/// document. `Some(0)` permits only empty committed text. Preedit is subject to
/// `max_bytes` only, so input methods can compose a candidate before committing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextLimits {
    pub max_graphemes: Option<usize>,
    pub max_bytes: usize,
}

impl Default for TextLimits {
    fn default() -> Self {
        Self {
            max_graphemes: None,
            max_bytes: 1_048_576,
        }
    }
}

impl TextLimits {
    /// Check committed text without modifying a document or allocating a copy.
    pub fn validate(&self, text: &str) -> Result<(), EditError> {
        self.validate_bytes(text.len())?;
        self.validate_graphemes(text)
    }

    fn validate_bytes(&self, length: usize) -> Result<(), EditError> {
        if length > self.max_bytes {
            return Err(self.exceeded(TextLimit::Bytes));
        }
        Ok(())
    }

    fn validate_graphemes(&self, text: &str) -> Result<(), EditError> {
        if let Some(maximum) = self.max_graphemes
            && text.graphemes(true).nth(maximum).is_some()
        {
            return Err(self.exceeded(TextLimit::Graphemes));
        }
        Ok(())
    }

    fn exceeded(&self, kind: TextLimit) -> EditError {
        EditError::LimitExceeded {
            kind,
            maximum: match kind {
                TextLimit::Bytes => self.max_bytes,
                TextLimit::Graphemes => self.max_graphemes.unwrap_or(usize::MAX),
            },
        }
    }
}

/// The unit of a rejected text limit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextLimit {
    Bytes,
    Graphemes,
}

/// UTF-8 byte offsets at extended grapheme boundaries in committed text.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Selection {
    pub anchor: usize,
    pub focus: usize,
}

impl Selection {
    pub fn range(self) -> Range<usize> {
        self.anchor.min(self.focus)..self.anchor.max(self.focus)
    }
    pub fn is_empty(self) -> bool {
        self.anchor == self.focus
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Composition {
    pub text: String,
    /// UTF-8 boundaries within preedit, as supplied by the input method.
    pub cursor: Option<(usize, usize)>,
    /// Committed bytes replaced when composition is committed.
    pub replacement: Range<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EditError {
    InvalidSelection,
    InvalidPreeditCursor,
    LimitExceeded { kind: TextLimit, maximum: usize },
}

impl fmt::Display for EditError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSelection => {
                f.write_str("selection must use grapheme boundaries within the document")
            }
            Self::InvalidPreeditCursor => {
                f.write_str("preedit cursor must use UTF-8 boundaries within preedit")
            }
            Self::LimitExceeded { kind, maximum } => write!(
                f,
                "text exceeds the limit of {maximum} {}",
                match kind {
                    TextLimit::Bytes => "UTF-8 bytes",
                    TextLimit::Graphemes => "graphemes",
                }
            ),
        }
    }
}
impl std::error::Error for EditError {}

#[derive(Clone, Debug, PartialEq, Eq)]
struct State {
    text: String,
    selection: Selection,
}

/// A plain-text document with grapheme-safe editing and bounded undo history.
///
/// ```
/// use rust_desktop_ui_text::TextDocument;
/// let mut document = TextDocument::new("А👩‍💻e\u{301}");
/// document.backspace(); // Removes the complete e + combining-accent grapheme.
/// assert_eq!(document.text(), "А👩‍💻");
/// assert!(document.undo());
/// assert_eq!(document.text(), "А👩‍💻e\u{301}");
/// ```
#[derive(Debug)]
pub struct TextDocument {
    version: DocumentVersion,
    state: State,
    graphemes: Vec<usize>,
    composition: Option<Composition>,
    undo: VecDeque<State>,
    redo: Vec<State>,
}

/// Opaque identity of a document's text revision. Keeping a version alive
/// prevents identity reuse. Undo, redo and cloning never resurrect an old version.
#[derive(Clone)]
pub struct DocumentVersion(Arc<()>);
impl DocumentVersion {
    fn fresh() -> Self {
        Self(Arc::new(()))
    }
}
impl PartialEq for DocumentVersion {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}
impl Eq for DocumentVersion {}
impl fmt::Debug for DocumentVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("DocumentVersion(..)")
    }
}
impl Clone for TextDocument {
    fn clone(&self) -> Self {
        Self {
            version: DocumentVersion::fresh(),
            state: self.state.clone(),
            graphemes: self.graphemes.clone(),
            composition: self.composition.clone(),
            undo: self.undo.clone(),
            redo: self.redo.clone(),
        }
    }
}

impl Default for TextDocument {
    fn default() -> Self {
        Self::new("")
    }
}

impl TextDocument {
    pub fn new(text: impl Into<String>) -> Self {
        let text = text.into();
        let end = text.len();
        let graphemes = boundaries(&text);
        Self {
            version: DocumentVersion::fresh(),
            state: State {
                text,
                selection: Selection {
                    anchor: end,
                    focus: end,
                },
            },
            graphemes,
            composition: None,
            undo: VecDeque::new(),
            redo: Vec::new(),
        }
    }
    pub fn text(&self) -> &str {
        &self.state.text
    }
    /// Version of committed document text, independent of caret and IME preedit.
    pub fn version(&self) -> DocumentVersion {
        self.version.clone()
    }
    pub fn selection(&self) -> Selection {
        self.state.selection
    }
    pub fn composition(&self) -> Option<&Composition> {
        self.composition.as_ref()
    }
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
    pub fn selected_text(&self) -> &str {
        &self.text()[self.selection().range()]
    }
    pub fn grapheme_boundaries(&self) -> Vec<usize> {
        self.graphemes.clone()
    }

    /// Number of extended graphemes, using the index retained by the document.
    pub fn grapheme_count(&self) -> usize {
        self.graphemes.len() - 1
    }

    /// UTF-8 offset of a grapheme edge. Index zero is the start and
    /// `grapheme_count()` is the end; larger indices return `None`.
    pub fn grapheme_boundary(&self, index: usize) -> Option<usize> {
        self.graphemes.get(index).copied()
    }

    pub fn set_selection(&mut self, anchor: usize, focus: usize) -> Result<(), EditError> {
        if self.graphemes.binary_search(&anchor).is_err()
            || self.graphemes.binary_search(&focus).is_err()
        {
            return Err(EditError::InvalidSelection);
        }
        self.cancel_preedit();
        self.state.selection = Selection { anchor, focus };
        Ok(())
    }

    pub fn select_all(&mut self) {
        self.cancel_preedit();
        self.state.selection = Selection {
            anchor: 0,
            focus: self.text().len(),
        };
    }

    /// Replace the committed document as one undoable operation.
    pub fn set_text(&mut self, text: &str) {
        self.cancel_preedit();
        self.replace(0..self.text().len(), text);
    }

    /// Replace committed text if it fits the limits, as one undoable operation.
    /// On error, text, selection, composition and history remain unchanged.
    pub fn try_set_text(&mut self, text: &str, limits: TextLimits) -> Result<(), EditError> {
        limits.validate(text)?;
        self.set_text(text);
        Ok(())
    }

    /// Paste and typing share the same Unicode-safe replacement operation.
    pub fn insert(&mut self, text: &str) {
        self.cancel_preedit();
        self.replace(self.selection().range(), text);
    }

    /// Replace the selection if the final text fits the limits. Graphemes are
    /// counted after joining the surrounding text, including boundary merges.
    /// On error, text, selection, composition and history remain unchanged.
    pub fn try_insert(&mut self, text: &str, limits: TextLimits) -> Result<(), EditError> {
        self.validate_replacement(self.selection().range(), text, limits)?;
        self.insert(text);
        Ok(())
    }

    /// Replace an explicit grapheme-aligned UTF-8 range as one undo operation.
    /// Rejection preserves text, selection, composition and both history stacks.
    pub fn try_replace_range(
        &mut self,
        range: Range<usize>,
        text: &str,
        limits: TextLimits,
    ) -> Result<(), EditError> {
        if range.start > range.end
            || self.graphemes.binary_search(&range.start).is_err()
            || self.graphemes.binary_search(&range.end).is_err()
        {
            return Err(EditError::InvalidSelection);
        }
        self.validate_replacement(range.clone(), text, limits)?;
        self.cancel_preedit();
        self.replace(range, text);
        Ok(())
    }

    pub fn paste(&mut self, text: &str) {
        self.insert(text);
    }

    pub fn cut(&mut self) -> String {
        let selected = self.selected_text().to_owned();
        self.delete_selection();
        selected
    }

    pub fn delete_selection(&mut self) {
        self.cancel_preedit();
        self.replace(self.selection().range(), "");
    }

    pub fn backspace(&mut self) {
        self.cancel_preedit();
        if !self.selection().is_empty() {
            self.delete_selection();
            return;
        }
        let focus = self.selection().focus;
        let previous = self.graphemes[self
            .graphemes
            .partition_point(|&i| i < focus)
            .saturating_sub(1)];
        self.replace(previous..focus, "");
    }

    pub fn delete_forward(&mut self) {
        self.cancel_preedit();
        if !self.selection().is_empty() {
            self.delete_selection();
            return;
        }
        let focus = self.selection().focus;
        let next = self
            .graphemes
            .get(self.graphemes.partition_point(|&i| i <= focus))
            .copied()
            .unwrap_or(self.text().len());
        self.replace(focus..next, "");
    }

    /// Delete to the previous Unicode word boundary, or delete the selection.
    /// The replacement is one undo step and never splits an extended grapheme.
    pub fn delete_word_backward(&mut self) {
        if !self.selection().is_empty() {
            self.delete_selection();
            return;
        }
        self.cancel_preedit();
        let focus = self.selection().focus;
        let start = self.word_left_boundary();
        self.replace(start..focus, "");
    }

    /// Delete to the next Unicode word boundary, or delete the selection.
    pub fn delete_word_forward(&mut self) {
        if !self.selection().is_empty() {
            self.delete_selection();
            return;
        }
        self.cancel_preedit();
        let focus = self.selection().focus;
        let end = self.word_right_boundary();
        self.replace(focus..end, "");
    }

    pub fn move_left(&mut self, extend: bool) {
        let target = if !extend && !self.selection().is_empty() {
            self.selection().range().start
        } else {
            self.graphemes[self
                .graphemes
                .partition_point(|&i| i < self.selection().focus)
                .saturating_sub(1)]
        };
        self.move_to(target, extend);
    }

    pub fn move_right(&mut self, extend: bool) {
        let target = if !extend && !self.selection().is_empty() {
            self.selection().range().end
        } else {
            self.graphemes
                .get(
                    self.graphemes
                        .partition_point(|&i| i <= self.selection().focus),
                )
                .copied()
                .unwrap_or(self.text().len())
        };
        self.move_to(target, extend);
    }

    pub fn move_word_left(&mut self, extend: bool) {
        self.move_to(self.word_left_boundary(), extend);
    }

    fn word_left_boundary(&self) -> usize {
        let target = self
            .text()
            .unicode_word_indices()
            .map(|(i, _)| i)
            .take_while(|&i| i < self.selection().focus)
            .last()
            .unwrap_or(0);
        self.snap_boundary(target)
    }

    pub fn move_word_right(&mut self, extend: bool) {
        self.move_to(self.word_right_boundary(), extend);
    }

    fn word_right_boundary(&self) -> usize {
        let target = self
            .text()
            .unicode_word_indices()
            .map(|(i, word)| i + word.len())
            .find(|&i| i > self.selection().focus)
            .unwrap_or(self.text().len());
        self.snap_boundary(target)
    }

    /// Move to the beginning of the current hard line, not a visual soft wrap.
    pub fn move_home(&mut self, extend: bool) {
        let target = self.text()[..self.selection().focus]
            .char_indices()
            .rev()
            .find(|&(_, c)| line_break(c))
            .map_or(0, |(i, c)| i + c.len_utf8());
        self.move_to(target, extend);
    }

    pub fn move_end(&mut self, extend: bool) {
        let focus = self.selection().focus;
        let target = self.text()[focus..]
            .char_indices()
            .find(|&(_, c)| line_break(c))
            .map_or(self.text().len(), |(i, _)| focus + i);
        self.move_to(target, extend);
    }

    pub fn move_document_start(&mut self, extend: bool) {
        self.move_to(0, extend);
    }
    pub fn move_document_end(&mut self, extend: bool) {
        self.move_to(self.text().len(), extend);
    }

    /// Store composition separately from committed text and undo history.
    pub fn set_preedit(
        &mut self,
        text: &str,
        cursor: Option<(usize, usize)>,
    ) -> Result<(), EditError> {
        validate_preedit_cursor(text, cursor)?;
        if text.is_empty() {
            self.cancel_preedit();
            return Ok(());
        }
        let replacement = self.composition.as_ref().map_or_else(
            || self.selection().range(),
            |value| value.replacement.clone(),
        );
        self.composition = Some(Composition {
            text: text.into(),
            cursor,
            replacement,
        });
        Ok(())
    }

    /// Update IME composition if the resulting display text fits the byte cap.
    /// The grapheme limit is checked at commit, never by truncating preedit.
    /// On error, the existing composition and all committed state are retained.
    pub fn try_set_preedit(
        &mut self,
        text: &str,
        cursor: Option<(usize, usize)>,
        limits: TextLimits,
    ) -> Result<(), EditError> {
        validate_preedit_cursor(text, cursor)?;
        if text.is_empty() {
            limits.validate_bytes(self.text().len())?;
        } else {
            self.replacement_length(self.composition_range(), text, limits)?;
        }
        self.set_preedit(text, cursor)
    }

    /// Commit composition as exactly one undo step, replacing its original range.
    pub fn commit(&mut self, text: &str) {
        let replacement = self
            .composition
            .take()
            .map_or_else(|| self.selection().range(), |value| value.replacement);
        self.replace(replacement, text);
    }

    /// Commit IME text if the final document fits the limits, as one undo step.
    /// A rejected commit retains the composition for the caller to cancel or
    /// update explicitly, and leaves text, selection and history unchanged.
    pub fn try_commit(&mut self, text: &str, limits: TextLimits) -> Result<(), EditError> {
        self.validate_replacement(self.composition_range(), text, limits)?;
        self.commit(text);
        Ok(())
    }

    pub fn cancel_preedit(&mut self) {
        self.composition = None;
    }

    pub fn display_text(&self) -> String {
        let mut result = self.text().to_owned();
        if let Some(composition) = &self.composition {
            result.replace_range(composition.replacement.clone(), &composition.text);
        }
        result
    }

    /// Selection/caret in display_text. Preedit cursor follows the input method.
    pub fn display_selection(&self) -> Selection {
        match &self.composition {
            Some(composition) => {
                let (anchor, focus) = composition
                    .cursor
                    .unwrap_or((composition.text.len(), composition.text.len()));
                Selection {
                    anchor: composition.replacement.start + anchor,
                    focus: composition.replacement.start + focus,
                }
            }
            None => self.selection(),
        }
    }

    /// Discard undo and redo snapshots without changing text, selection or IME.
    pub fn clear_history(&mut self) {
        self.undo.clear();
        self.redo.clear();
    }

    /// Text restored by the next undo, without changing document or history.
    pub fn undo_text(&self) -> Option<&str> {
        self.undo.back().map(|state| state.text.as_str())
    }

    /// Text restored by the next redo, without changing document or history.
    pub fn redo_text(&self) -> Option<&str> {
        self.redo.last().map(|state| state.text.as_str())
    }

    pub fn undo(&mut self) -> bool {
        self.cancel_preedit();
        if let Some(previous) = self.undo.pop_back() {
            self.version = DocumentVersion::fresh();
            self.redo.push(std::mem::replace(&mut self.state, previous));
            self.graphemes = boundaries(self.text());
            true
        } else {
            false
        }
    }

    /// Undo if the restored text fits the current limits. A rejected undo
    /// retains the composition, current state and both history stacks.
    pub fn try_undo(&mut self, limits: TextLimits) -> Result<bool, EditError> {
        if let Some(previous) = self.undo.back() {
            limits.validate(&previous.text)?;
        }
        Ok(self.undo())
    }

    pub fn redo(&mut self) -> bool {
        self.cancel_preedit();
        if let Some(next) = self.redo.pop() {
            self.version = DocumentVersion::fresh();
            let previous = std::mem::replace(&mut self.state, next);
            self.graphemes = boundaries(self.text());
            self.push_undo(previous);
            true
        } else {
            false
        }
    }

    /// Redo if the restored text fits the current limits. A rejected redo
    /// retains the composition, current state and both history stacks.
    pub fn try_redo(&mut self, limits: TextLimits) -> Result<bool, EditError> {
        if let Some(next) = self.redo.last() {
            limits.validate(&next.text)?;
        }
        Ok(self.redo())
    }

    fn composition_range(&self) -> Range<usize> {
        self.composition.as_ref().map_or_else(
            || self.selection().range(),
            |composition| composition.replacement.clone(),
        )
    }

    fn replacement_length(
        &self,
        range: Range<usize>,
        text: &str,
        limits: TextLimits,
    ) -> Result<usize, EditError> {
        let length = (self.text().len() - range.len())
            .checked_add(text.len())
            .ok_or_else(|| limits.exceeded(TextLimit::Bytes))?;
        limits.validate_bytes(length)?;
        Ok(length)
    }

    fn validate_replacement(
        &self,
        range: Range<usize>,
        text: &str,
        limits: TextLimits,
    ) -> Result<(), EditError> {
        // Check the byte budget before allocating a candidate or undo snapshot.
        let length = self.replacement_length(range.clone(), text, limits)?;
        if limits.max_graphemes.is_some() {
            let mut candidate = String::with_capacity(length);
            candidate.push_str(&self.text()[..range.start]);
            candidate.push_str(text);
            candidate.push_str(&self.text()[range.end..]);
            limits.validate_graphemes(&candidate)?;
        }
        Ok(())
    }

    fn replace(&mut self, range: Range<usize>, text: &str) {
        if range.is_empty() && text.is_empty() {
            return;
        }
        let previous = self.state.clone();
        self.state.text.replace_range(range.clone(), text);
        if self.state.text != previous.text {
            self.version = DocumentVersion::fresh();
        }
        self.graphemes = boundaries(self.text());
        // Insertion/deletion can merge surrounding graphemes (combining marks,
        // regional indicators and ZWJ). Keep the resulting caret on a boundary.
        let intended = range.start + text.len();
        let caret = self.graphemes[self.graphemes.partition_point(|&i| i < intended)];
        self.state.selection = Selection {
            anchor: caret,
            focus: caret,
        };
        if self.state != previous {
            self.push_undo(previous);
            self.redo.clear();
        }
    }

    fn push_undo(&mut self, state: State) {
        if self.undo.len() == HISTORY_LIMIT {
            self.undo.pop_front();
        }
        self.undo.push_back(state);
    }

    fn move_to(&mut self, index: usize, extend: bool) {
        self.cancel_preedit();
        let index = self.snap_boundary(index);
        if !extend {
            self.state.selection.anchor = index;
        }
        self.state.selection.focus = index;
    }

    fn snap_boundary(&self, index: usize) -> usize {
        self.graphemes[self
            .graphemes
            .partition_point(|&i| i <= index)
            .saturating_sub(1)]
    }
}

fn validate_preedit_cursor(text: &str, cursor: Option<(usize, usize)>) -> Result<(), EditError> {
    if let Some((start, end)) = cursor
        && (start > end || !text.is_char_boundary(start) || !text.is_char_boundary(end))
    {
        return Err(EditError::InvalidPreeditCursor);
    }
    Ok(())
}

fn boundaries(text: &str) -> Vec<usize> {
    text.grapheme_indices(true)
        .map(|(index, _)| index)
        .chain(std::iter::once(text.len()))
        .collect()
}
fn line_break(c: char) -> bool {
    matches!(c, '\n' | '\r' | '\u{85}' | '\u{2028}' | '\u{2029}')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn word_deletion_keeps_clusters_selection_and_undo_replacement_atomic() {
        let original = "Привет e\u{301} 👩‍💻 мир";
        let mut document = TextDocument::new(original);
        document.delete_word_backward();
        assert_eq!(document.text(), "Привет e\u{301} 👩‍💻 ");
        assert!(document.undo());
        assert_eq!(document.text(), original);
        document.move_document_start(false);
        document.delete_word_forward();
        assert_eq!(document.text(), " e\u{301} 👩‍💻 мир");
        assert!(document.undo());
        let start = "Привет ".len();
        let end = "Привет e\u{301} 👩‍💻".len();
        document.set_selection(start, end).unwrap();
        document.delete_word_backward();
        assert_eq!(document.text(), "Привет  мир");
        assert!(document.undo());
        assert_eq!(
            document.selection(),
            Selection {
                anchor: start,
                focus: end
            }
        );
        assert_eq!(document.text(), original);
    }

    #[test]
    fn retained_grapheme_index_tracks_merge_composition_history_and_rejected_input() {
        let mut document = TextDocument::new("e");
        document.insert("\u{301}");
        assert_eq!(document.grapheme_count(), 1);
        assert_eq!(document.grapheme_boundary(1), Some(3));
        assert_eq!(document.grapheme_boundary(2), None);
        document.set_preedit("👩‍💻", None).unwrap();
        assert_eq!(document.grapheme_count(), 1);
        let before = document.grapheme_boundaries();
        assert!(document.try_insert("x", grapheme_limits(1)).is_err());
        assert_eq!(document.grapheme_boundaries(), before);
        document.commit("🇷🇺\r\nक्ष");
        assert_eq!(document.grapheme_count(), 4);
        for edge in document.grapheme_boundaries() {
            document.set_selection(edge, edge).unwrap();
        }
        assert!(document.undo());
        assert_eq!(document.grapheme_count(), 1);
        assert_eq!(document.grapheme_boundary(1), Some(3));
        assert!(document.redo());
        assert_eq!(document.grapheme_count(), 4);
        document.move_document_start(false);
        for _ in 0..4 {
            document.move_right(false);
        }
        assert_eq!(document.selection().focus, document.text().len());
    }

    #[test]
    fn grapheme_navigation_and_deletion_keep_emoji_accents_and_crlf_whole() {
        let graphemes = ["А", "👩‍💻", "e\u{301}", "🇷🇺", "\r\n"];
        let mut doc = TextDocument::new(graphemes.concat());
        for end in (0..graphemes.len()).rev() {
            doc.backspace();
            assert_eq!(doc.text(), graphemes[..end].concat());
        }
        assert_eq!(doc.selection(), Selection::default());
        for _ in &graphemes {
            assert!(doc.undo());
        }
        doc.move_document_start(false);
        doc.move_right(true);
        assert_eq!(doc.selected_text(), "А");
        doc.move_right(true);
        assert_eq!(doc.selected_text(), "А👩‍💻");
        doc.delete_selection();
        assert_eq!(doc.text(), "e\u{301}🇷🇺\r\n");
        doc.delete_forward();
        assert_eq!(doc.text(), "🇷🇺\r\n");
    }

    #[test]
    fn invalid_selection_is_atomic_and_combining_insert_snaps_new_boundary() {
        let mut doc = TextDocument::new("e\u{301}💻");
        assert!(doc.set_selection(1, 1).is_err());
        assert!(doc.set_selection(99, 99).is_err());
        doc.set_selection(3, 3).unwrap();
        doc.insert("👩\u{200d}");
        assert_eq!(doc.text(), "e\u{301}👩‍💻");
        assert_eq!(doc.selection().focus, doc.text().len());
        assert!(doc.undo());
        assert_eq!(doc.text(), "e\u{301}💻");
        assert_eq!(doc.selection().focus, 3);
    }

    #[test]
    fn composition_preserves_committed_text_and_has_one_undo_step() {
        let mut doc = TextDocument::new("one ДВА three");
        doc.set_selection(4, 10).unwrap();
        doc.set_preedit("中", Some((3, 3))).unwrap();
        assert_eq!(doc.text(), "one ДВА three");
        assert_eq!(doc.display_text(), "one 中 three");
        assert!(!doc.can_undo());
        assert!(doc.set_preedit("中", Some((1, 1))).is_err());
        assert_eq!(doc.display_text(), "one 中 three");
        doc.set_preedit("中文", Some((6, 6))).unwrap();
        doc.commit("中文");
        assert_eq!(doc.text(), "one 中文 three");
        assert!(doc.composition().is_none());
        assert!(doc.undo());
        assert_eq!(doc.text(), "one ДВА three");
        assert_eq!(doc.selected_text(), "ДВА");
        assert!(!doc.can_undo());
        assert!(doc.redo());
        assert_eq!(doc.text(), "one 中文 three");
    }

    #[test]
    fn cancelled_composition_does_not_change_selection_or_history() {
        let mut doc = TextDocument::new("АБ");
        doc.select_all();
        let selection = doc.selection();
        doc.set_preedit("候補", None).unwrap();
        doc.cancel_preedit();
        assert_eq!(doc.display_text(), "АБ");
        assert_eq!(doc.selection(), selection);
        assert!(!doc.can_undo());
    }

    #[test]
    fn clipboard_word_and_line_navigation_round_trip_unicode() {
        let mut doc = TextDocument::new("hello мир\r\n第二行");
        doc.move_home(false);
        assert_eq!(doc.selection().focus, "hello мир\r\n".len());
        doc.move_document_start(false);
        doc.move_word_right(true);
        assert_eq!(doc.cut(), "hello");
        assert_eq!(doc.text(), " мир\r\n第二行");
        doc.paste("Привет");
        doc.move_end(true);
        assert_eq!(doc.selected_text(), " мир");
        doc.move_word_left(false);
        assert_eq!(doc.selection().focus, "Привет ".len());
        doc.move_document_end(true);
        assert_eq!(doc.selected_text(), "мир\r\n第二行");
    }

    #[test]
    fn editing_after_undo_discards_redo_and_history_is_bounded() {
        let mut doc = TextDocument::default();
        for _ in 0..200 {
            doc.insert("a");
        }
        let mut undos = 0;
        while doc.undo() {
            undos += 1;
        }
        assert_eq!(undos, HISTORY_LIMIT);
        doc.insert("b");
        assert!(!doc.can_redo());
        assert_eq!(doc.text().len(), 73);
    }

    fn assert_document_unchanged(actual: &TextDocument, before: &TextDocument) {
        assert_eq!(actual.state, before.state);
        assert_eq!(actual.composition, before.composition);
        assert_eq!(actual.undo, before.undo);
        assert_eq!(actual.redo, before.redo);
    }

    fn grapheme_limits(maximum: usize) -> TextLimits {
        TextLimits {
            max_graphemes: Some(maximum),
            ..TextLimits::default()
        }
    }

    #[test]
    fn limits_count_graphemes_and_utf8_bytes_independently() {
        let limits = grapheme_limits(2);
        assert_eq!(limits.validate("e\u{301}👩‍💻"), Ok(()));
        assert_eq!(
            limits.validate("e\u{301}👩‍💻x"),
            Err(EditError::LimitExceeded {
                kind: TextLimit::Graphemes,
                maximum: 2,
            })
        );
        assert_eq!(grapheme_limits(0).validate(""), Ok(()));
        assert!(grapheme_limits(0).validate("\u{301}").is_err());

        let limits = TextLimits {
            max_bytes: 4,
            ..TextLimits::default()
        };
        assert_eq!(limits.validate("éé"), Ok(()));
        assert_eq!(
            limits.validate("👩x"),
            Err(EditError::LimitExceeded {
                kind: TextLimit::Bytes,
                maximum: 4,
            })
        );
        let limits = TextLimits::default();
        assert_eq!(limits.max_bytes, 1_048_576);
        assert_eq!(limits.max_graphemes, None);
        assert!(limits.validate(&"a".repeat(1_048_576)).is_ok());
        assert!(limits.validate(&"a".repeat(1_048_577)).is_err());
    }

    #[test]
    fn limited_insert_counts_graphemes_after_merging_both_boundaries() {
        let mut accent = TextDocument::new("e");
        accent.try_insert("\u{301}", grapheme_limits(1)).unwrap();
        assert_eq!(accent.text(), "e\u{301}");
        assert_eq!(accent.selection().focus, accent.text().len());

        let mut emoji = TextDocument::new("👩💻");
        emoji.set_selection("👩".len(), "👩".len()).unwrap();
        emoji.try_insert("\u{200d}", grapheme_limits(1)).unwrap();
        assert_eq!(emoji.text(), "👩‍💻");
        assert_eq!(emoji.selection().focus, emoji.text().len());

        let mut neighboring_emoji = TextDocument::new("e💻");
        neighboring_emoji.set_selection(1, 1).unwrap();
        neighboring_emoji
            .try_insert("👩\u{200d}", grapheme_limits(2))
            .unwrap();
        assert_eq!(neighboring_emoji.text(), "e👩‍💻");
        assert_eq!(
            neighboring_emoji.selection().focus,
            neighboring_emoji.text().len()
        );
    }

    #[test]
    fn limited_replacement_counts_only_final_selected_range_result() {
        let mut doc = TextDocument::new("abcd");
        doc.set_selection(1, 3).unwrap();
        let selection = doc.selection();
        let limits = TextLimits {
            max_graphemes: Some(3),
            max_bytes: 5,
        };
        doc.try_insert("e\u{301}", limits).unwrap();
        assert_eq!(doc.text(), "ae\u{301}d");
        assert_eq!(doc.selection().focus, 4);
        assert!(doc.undo());
        assert_eq!(doc.text(), "abcd");
        assert_eq!(doc.selection(), selection);
        assert!(!doc.can_undo());
    }

    #[test]
    fn failed_limited_edits_preserve_selection_composition_and_both_histories() {
        let mut doc = TextDocument::new("old");
        doc.insert("x");
        doc.insert("y");
        assert!(doc.undo());
        doc.set_selection(1, 3).unwrap();
        doc.set_preedit("中", Some((3, 3))).unwrap();
        let before = doc.clone();
        let limits = grapheme_limits(3);

        assert!(doc.try_insert("abcd", limits).is_err());
        assert_document_unchanged(&doc, &before);
        assert!(doc.try_set_text("abcd", limits).is_err());
        assert_document_unchanged(&doc, &before);
        assert!(doc.try_commit("abcd", limits).is_err());
        assert_document_unchanged(&doc, &before);

        let byte_limits = TextLimits {
            max_bytes: 5,
            ..TextLimits::default()
        };
        assert!(doc.try_insert("中文", byte_limits).is_err());
        assert_document_unchanged(&doc, &before);
        assert!(doc.try_set_text("中文", byte_limits).is_err());
        assert_document_unchanged(&doc, &before);
        assert!(doc.try_commit("中文", byte_limits).is_err());
        assert_document_unchanged(&doc, &before);

        assert!(doc.try_redo(TextLimits::default()).unwrap());
        assert_eq!(doc.text(), "oldxy");
        assert!(doc.composition().is_none());
    }

    #[test]
    fn preedit_ignores_grapheme_limit_but_bounds_final_display_bytes_atomically() {
        let mut doc = TextDocument::new("ax");
        doc.set_selection(1, 2).unwrap();
        let limits = TextLimits {
            max_graphemes: Some(0),
            max_bytes: 7,
        };
        doc.try_set_preedit("日本", Some((6, 6)), limits).unwrap();
        assert_eq!(doc.text(), "ax");
        assert_eq!(doc.display_text(), "a日本");
        assert!(!doc.can_undo());
        let before = doc.clone();

        assert_eq!(
            doc.try_set_preedit("日本語", None, limits),
            Err(EditError::LimitExceeded {
                kind: TextLimit::Bytes,
                maximum: 7,
            })
        );
        assert_document_unchanged(&doc, &before);
        assert_eq!(
            doc.try_set_preedit("日", Some((1, 1)), limits),
            Err(EditError::InvalidPreeditCursor)
        );
        assert_document_unchanged(&doc, &before);
        assert!(doc.try_commit("日", limits).is_err());
        assert_document_unchanged(&doc, &before);

        doc.try_set_preedit("", None, limits).unwrap();
        assert!(doc.composition().is_none());
        assert_eq!(doc.state, before.state);
        assert!(!doc.can_undo());
    }

    #[test]
    fn limited_commit_replaces_composition_once_and_keeps_original_undo_selection() {
        let mut doc = TextDocument::new("a候補b");
        doc.set_selection(1, 7).unwrap();
        let selection = doc.selection();
        let limits = grapheme_limits(3);
        doc.try_set_preedit("未確定", None, limits).unwrap();
        doc.try_commit("中", limits).unwrap();
        assert_eq!(doc.text(), "a中b");
        assert!(doc.composition().is_none());
        assert!(doc.try_undo(TextLimits::default()).unwrap());
        assert_eq!(doc.text(), "a候補b");
        assert_eq!(doc.selection(), selection);
        assert!(!doc.can_undo());
        assert!(doc.try_redo(limits).unwrap());
        assert_eq!(doc.text(), "a中b");
    }

    #[test]
    fn restricted_undo_and_redo_leave_rejected_history_and_composition_available() {
        let mut doc = TextDocument::new("long");
        doc.set_text("x");
        doc.set_preedit("中", None).unwrap();
        let before = doc.clone();
        assert!(doc.try_undo(grapheme_limits(1)).is_err());
        assert_document_unchanged(&doc, &before);
        assert!(doc.try_undo(TextLimits::default()).unwrap());
        assert_eq!(doc.text(), "long");
        assert!(!doc.can_undo());

        doc.set_preedit("中", None).unwrap();
        let before = doc.clone();
        assert!(doc.try_redo(grapheme_limits(0)).is_err());
        assert_document_unchanged(&doc, &before);
        assert!(
            doc.try_redo(TextLimits {
                max_bytes: 0,
                ..TextLimits::default()
            })
            .is_err()
        );
        assert_document_unchanged(&doc, &before);
        assert!(doc.try_redo(grapheme_limits(1)).unwrap());
        assert_eq!(doc.text(), "x");
        assert!(!doc.can_redo());
    }

    #[test]
    fn zero_limit_accepts_clearing_document_and_unrestricted_api_is_compatible() {
        let mut doc = TextDocument::new("unrestricted");
        doc.try_set_text("", grapheme_limits(0)).unwrap();
        assert_eq!(doc.text(), "");
        assert!(doc.try_insert("a", grapheme_limits(0)).is_err());
        doc.insert("legacy API");
        assert_eq!(doc.text(), "legacy API");
        assert!(doc.undo());
        assert_eq!(doc.text(), "");
        assert!(doc.undo());
        assert_eq!(doc.text(), "unrestricted");
        assert!(!doc.try_undo(TextLimits::default()).unwrap());
    }
    #[test]
    fn clear_history_preserves_draft_selection_and_composition() {
        let mut document = TextDocument::new("начало");
        document.insert("1");
        document.insert("2");
        assert!(document.undo());
        document.set_selection(0, "начало".len()).unwrap();
        document.set_preedit("候補", Some((0, 6))).unwrap();
        let text = document.text().to_owned();
        let selection = document.selection();
        let composition = document.composition().cloned();
        document.clear_history();
        assert_eq!(document.text(), text);
        assert_eq!(document.selection(), selection);
        assert_eq!(document.composition(), composition.as_ref());
        document.cancel_preedit();
        assert!(!document.undo());
        assert!(!document.redo());
    }
}
