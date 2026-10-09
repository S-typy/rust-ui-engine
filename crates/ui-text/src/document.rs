use std::{collections::VecDeque, fmt, ops::Range};
use unicode_segmentation::UnicodeSegmentation;

const HISTORY_LIMIT: usize = 128;

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
}

impl fmt::Display for EditError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidSelection => "selection must use grapheme boundaries within the document",
            Self::InvalidPreeditCursor => "preedit cursor must use UTF-8 boundaries within preedit",
        })
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
#[derive(Clone, Debug)]
pub struct TextDocument {
    state: State,
    composition: Option<Composition>,
    undo: VecDeque<State>,
    redo: Vec<State>,
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
        Self {
            state: State {
                text,
                selection: Selection {
                    anchor: end,
                    focus: end,
                },
            },
            composition: None,
            undo: VecDeque::new(),
            redo: Vec::new(),
        }
    }
    pub fn text(&self) -> &str {
        &self.state.text
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
        boundaries(self.text())
    }

    pub fn set_selection(&mut self, anchor: usize, focus: usize) -> Result<(), EditError> {
        let boundaries = self.grapheme_boundaries();
        if boundaries.binary_search(&anchor).is_err() || boundaries.binary_search(&focus).is_err() {
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

    /// Paste and typing share the same Unicode-safe replacement operation.
    pub fn insert(&mut self, text: &str) {
        self.cancel_preedit();
        self.replace(self.selection().range(), text);
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
        let previous = self
            .grapheme_boundaries()
            .into_iter()
            .rev()
            .find(|&i| i < focus)
            .unwrap_or(0);
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
            .grapheme_boundaries()
            .into_iter()
            .find(|&i| i > focus)
            .unwrap_or(self.text().len());
        self.replace(focus..next, "");
    }

    pub fn move_left(&mut self, extend: bool) {
        let target = if !extend && !self.selection().is_empty() {
            self.selection().range().start
        } else {
            self.grapheme_boundaries()
                .into_iter()
                .rev()
                .find(|&i| i < self.selection().focus)
                .unwrap_or(0)
        };
        self.move_to(target, extend);
    }

    pub fn move_right(&mut self, extend: bool) {
        let target = if !extend && !self.selection().is_empty() {
            self.selection().range().end
        } else {
            self.grapheme_boundaries()
                .into_iter()
                .find(|&i| i > self.selection().focus)
                .unwrap_or(self.text().len())
        };
        self.move_to(target, extend);
    }

    pub fn move_word_left(&mut self, extend: bool) {
        let target = self
            .text()
            .unicode_word_indices()
            .map(|(i, _)| i)
            .take_while(|&i| i < self.selection().focus)
            .last()
            .unwrap_or(0);
        self.move_to(target, extend);
    }

    pub fn move_word_right(&mut self, extend: bool) {
        let target = self
            .text()
            .unicode_word_indices()
            .map(|(i, word)| i + word.len())
            .find(|&i| i > self.selection().focus)
            .unwrap_or(self.text().len());
        self.move_to(target, extend);
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
        if let Some((start, end)) = cursor
            && (start > end || !text.is_char_boundary(start) || !text.is_char_boundary(end))
        {
            return Err(EditError::InvalidPreeditCursor);
        }
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

    /// Commit composition as exactly one undo step, replacing its original range.
    pub fn commit(&mut self, text: &str) {
        let replacement = self
            .composition
            .take()
            .map_or_else(|| self.selection().range(), |value| value.replacement);
        self.replace(replacement, text);
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

    pub fn undo(&mut self) -> bool {
        self.cancel_preedit();
        if let Some(previous) = self.undo.pop_back() {
            self.redo.push(std::mem::replace(&mut self.state, previous));
            true
        } else {
            false
        }
    }

    pub fn redo(&mut self) -> bool {
        self.cancel_preedit();
        if let Some(next) = self.redo.pop() {
            let previous = std::mem::replace(&mut self.state, next);
            self.push_undo(previous);
            true
        } else {
            false
        }
    }

    fn replace(&mut self, range: Range<usize>, text: &str) {
        if range.is_empty() && text.is_empty() {
            return;
        }
        let previous = self.state.clone();
        self.state.text.replace_range(range.clone(), text);
        // Insertion/deletion can merge surrounding graphemes (combining marks,
        // regional indicators and ZWJ). Keep the resulting caret on a boundary.
        let intended = range.start + text.len();
        let caret = boundaries(self.text())
            .into_iter()
            .find(|&i| i >= intended)
            .unwrap_or(self.text().len());
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
        let index = boundaries(self.text())
            .into_iter()
            .take_while(|&i| i <= index)
            .last()
            .unwrap_or(0);
        if !extend {
            self.state.selection.anchor = index;
        }
        self.state.selection.focus = index;
    }
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
}
