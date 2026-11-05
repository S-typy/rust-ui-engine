use super::*;
use rust_desktop_ui_text::DocumentVersion;

/// A single-use destination for an asynchronous paste or successful cut write.
/// No text is stored here. A changed document, editor, selection, paragraph,
/// view, or active composition makes completion fail without applying content.
#[derive(Debug)]
pub struct RichClipboardTarget {
    editor: WidgetId,
    revision: u64,
    version: DocumentVersion,
    paragraph: usize,
    selection: Selection,
    style: RunStyle,
}
impl RichEditor {
    pub fn clipboard_target(
        &mut self,
        ui: &mut Controls,
    ) -> Result<RichClipboardTarget, ControlError> {
        self.require_document_view()?;
        self.require_committed(ui)?;
        self.sync(ui)?;
        let host = ui.document(self.parts.editor).ok_or_else(stale)?;
        Ok(RichClipboardTarget {
            editor: self.parts.editor,
            revision: self.revision,
            version: host.version(),
            paragraph: self.paragraph,
            selection: host.selection(),
            style: self.typing_style.clone(),
        })
    }
    pub fn copy_fragment(
        &self,
        ui: &Controls,
        target: &RichClipboardTarget,
    ) -> Result<RichClipboardFragment, ControlError> {
        self.check_clipboard_target(ui, target, false)?;
        RichClipboardFragment::new(
            paragraph_at(&self.document, self.paragraph)
                .unwrap()
                .fragment(target.selection.range())
                .map_err(control_error)?,
        )
        .map_err(control_error)
    }
    pub fn paste_html(
        &mut self,
        ui: &mut Controls,
        target: RichClipboardTarget,
        html: &str,
    ) -> Result<bool, ControlError> {
        self.check_clipboard_target(ui, &target, true)?;
        let fragment =
            RichClipboardFragment::from_html(html, target.style.clone()).map_err(control_error)?;
        if fragment.plain_text().is_empty() {
            return Ok(false);
        }
        self.replace_fragment(ui, target, fragment)
    }
    /// Explicit plain-text insertion. CRLF and isolated CR normalize to LF.
    pub fn paste_text(
        &mut self,
        ui: &mut Controls,
        target: RichClipboardTarget,
        text: &str,
    ) -> Result<bool, ControlError> {
        self.check_clipboard_target(ui, &target, true)?;
        if text.len() > 1024 * 1024 {
            return Err(control_error(DocumentError::Limit("clipboard text")));
        }
        if text.is_empty() {
            return Ok(false);
        }
        let fragment = RichClipboardFragment::new(vec![RichRun {
            text: text.replace("\r\n", "\n").replace('\r', "\n"),
            style: target.style.clone(),
        }])
        .map_err(control_error)?;
        self.replace_fragment(ui, target, fragment)
    }
    /// Call only after the platform confirms writing all requested formats.
    /// Failed writes must discard the target without removing the selection.
    pub fn complete_cut(
        &mut self,
        ui: &mut Controls,
        target: RichClipboardTarget,
    ) -> Result<bool, ControlError> {
        self.replace_fragment(
            ui,
            target,
            RichClipboardFragment::new(Vec::new()).map_err(control_error)?,
        )
    }
    fn check_clipboard_target(
        &self,
        ui: &Controls,
        target: &RichClipboardTarget,
        write: bool,
    ) -> Result<(), ControlError> {
        let host = ui.document(self.parts.editor).ok_or_else(stale)?;
        if self.view != RichEditorView::Document
            || self.parts.editor != target.editor
            || self.revision != target.revision
            || host.version() != target.version
            || self.paragraph != target.paragraph
            || host.selection() != target.selection
            || self.typing_style != target.style
            || host.composition().is_some()
            || !ui.is_enabled(self.parts.editor)
            || (write
                && ui
                    .editor_settings(self.parts.editor)
                    .is_none_or(|s| s.is_read_only))
        {
            return Err(stale());
        }
        Ok(())
    }
    fn replace_fragment(
        &mut self,
        ui: &mut Controls,
        target: RichClipboardTarget,
        fragment: RichClipboardFragment,
    ) -> Result<bool, ControlError> {
        self.check_clipboard_target(ui, &target, true)?;
        let mut candidate = self.document.clone();
        let range = target.selection.range();
        let inserted = fragment.plain_text().len();
        let paragraph = paragraph_mut(&mut candidate, self.paragraph).unwrap();
        fragment
            .replace_in(paragraph, range.clone())
            .map_err(control_error)?;
        let text = paragraph.plain_text();
        if candidate == self.document {
            return Ok(false);
        }
        candidate.validate().map_err(control_error)?;
        let layout = candidate
            .paginate(&mut self.engine)
            .map_err(control_error)?;
        let caret = TextDocument::new(&text)
            .grapheme_boundaries()
            .into_iter()
            .find(|i| *i >= range.start + inserted)
            .ok_or_else(stale)?;
        // The retained editor validates limits/business policy before either
        // model or history is published. Invalid is an outcome, not an error.
        if let crate::CommitOutcome::Invalid(message) =
            ui.set_editor_value(self.parts.editor, &text)?
        {
            return Err(ControlError::Invalid(message));
        }
        ui.set_editor_selection(self.parts.editor, caret, caret)?;
        let before = self.snapshot();
        self.document = candidate;
        self.layout = layout;
        self.last_text = text;
        self.last_selection = Selection {
            anchor: caret,
            focus: caret,
        };
        self.typing_style = paragraph_at(&self.document, self.paragraph)
            .unwrap()
            .style_at(caret);
        self.remember(before);
        self.refresh(ui)?;
        Ok(true)
    }
}
fn stale() -> ControlError {
    ControlError::Invalid("Clipboard destination changed or is not editable".into())
}
