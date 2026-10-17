//! Explicit spelling requests for retained plain-text drafts.
use super::*;
use rust_desktop_ui_text::spellcheck::SpellSession;
pub use rust_desktop_ui_text::spellcheck::{
    LexiconMetadata, LexiconProvider, SpellCompletion, SpellError, SpellIssue, SpellProvider,
    SpellRequest, SpellSettings,
};

pub(super) struct Entry {
    owner: WidgetId,
    session: SpellSession,
}
#[derive(Default)]
pub(super) struct SpellState {
    entries: HashMap<WidgetId, Entry>,
}
fn failure(error: impl ToString) -> ControlError {
    ControlError::Invalid(error.to_string())
}
impl Controls {
    fn spell_editor(&self, owner: WidgetId) -> Result<WidgetId, ControlError> {
        let editor = self.memo_editor(owner).unwrap_or(owner);
        self.editor_control(editor)?;
        if self.is_password_box(editor)
            || self.editor_mask_settings(editor).is_some()
            || self.rich_owner(editor).is_some()
        {
            return Err(failure(
                "Spelling is available only for plain TextBox and Memo drafts",
            ));
        }
        Ok(editor)
    }
    fn spell_available(&self, editor: WidgetId) -> bool {
        self.spellchecks.entries.get(&editor).is_some_and(|entry| {
            self.spell_editor(entry.owner).ok() == Some(editor)
                && self.effective_enabled(entry.owner)
                && self.effective_enabled(editor)
                && self.is_read_only(editor) == Some(false)
        })
    }
    /// Attach or remove an explicit spelling session. No worker or automatic
    /// background check is created. Memo owners resolve to their retained editor.
    pub fn set_spellcheck(
        &mut self,
        owner: WidgetId,
        settings: Option<SpellSettings>,
    ) -> Result<(), ControlError> {
        let editor = self.spell_editor(owner)?;
        if let Some(settings) = settings {
            settings.validate().map_err(failure)?;
            if let Some(entry) = self.spellchecks.entries.get_mut(&editor) {
                entry.session.set_settings(settings).map_err(failure)?;
            } else {
                if self.spellchecks.entries.len() == 128 {
                    return Err(failure("At most 128 spelling editor sessions are attached"));
                }
                self.spellchecks.entries.insert(
                    editor,
                    Entry {
                        owner,
                        session: SpellSession::new(settings).map_err(failure)?,
                    },
                );
            }
        } else {
            self.spellchecks.entries.remove(&editor);
        }
        Ok(())
    }
    pub fn request_spellcheck(&mut self, owner: WidgetId) -> Result<SpellRequest, ControlError> {
        let editor = self.spell_editor(owner)?;
        if !self.spell_available(editor) {
            return Err(failure("Spelling editor is unavailable or read-only"));
        }
        let Kind::TextBox(document) = &self.controls[&editor].kind else {
            unreachable!()
        };
        self.spellchecks
            .entries
            .get_mut(&editor)
            .unwrap()
            .session
            .request(document)
            .map_err(failure)
    }
    /// Ownership, availability, composition and revision are checked before
    /// provider payloads. Stale completions do not publish diagnostics or ranges.
    pub fn complete_spellcheck(
        &mut self,
        owner: WidgetId,
        request: &SpellRequest,
        result: Result<Vec<SpellIssue>, SpellError>,
    ) -> Result<SpellCompletion, ControlError> {
        let Ok(editor) = self.spell_editor(owner) else {
            return Ok(SpellCompletion::Stale);
        };
        if !self.spell_available(editor) {
            self.invalidate_spellcheck(editor);
            return Ok(SpellCompletion::Stale);
        }
        let Kind::TextBox(document) = &self.controls[&editor].kind else {
            unreachable!()
        };
        self.spellchecks
            .entries
            .get_mut(&editor)
            .unwrap()
            .session
            .complete(document, request, result)
            .map_err(failure)
    }
    pub fn spellcheck_issues(&self, owner: WidgetId) -> Option<&[SpellIssue]> {
        let editor = self.spell_editor(owner).ok()?;
        if !self.spell_available(editor) {
            return None;
        }
        self.spellchecks
            .entries
            .get(&editor)?
            .session
            .issues(self.document(editor)?)
    }
    pub fn spellcheck_error(&self, owner: WidgetId) -> Option<&str> {
        let editor = self.spell_editor(owner).ok()?;
        self.spellchecks.entries.get(&editor)?.session.error()
    }
    pub fn cancel_spellcheck(&mut self, owner: WidgetId) -> Result<(), ControlError> {
        let editor = self.spell_editor(owner)?;
        let entry = self
            .spellchecks
            .entries
            .get_mut(&editor)
            .ok_or_else(|| failure("Spelling is not attached"))?;
        entry.session.cancel();
        Ok(())
    }
    pub fn ignore_spelling_once(
        &mut self,
        owner: WidgetId,
        issue: usize,
    ) -> Result<(), ControlError> {
        let editor = self.spell_editor(owner)?;
        if !self.spell_available(editor) {
            return Err(failure("Spelling editor is unavailable"));
        }
        let Kind::TextBox(document) = &self.controls[&editor].kind else {
            unreachable!()
        };
        self.spellchecks
            .entries
            .get_mut(&editor)
            .unwrap()
            .session
            .ignore_once(document, issue)
            .map_err(failure)
    }
    pub fn ignore_spelling_all(&mut self, owner: WidgetId, word: &str) -> Result<(), ControlError> {
        self.spelling_policy_word(owner, word, false)
    }
    pub fn add_spelling_word(&mut self, owner: WidgetId, word: &str) -> Result<(), ControlError> {
        self.spelling_policy_word(owner, word, true)
    }
    fn spelling_policy_word(
        &mut self,
        owner: WidgetId,
        word: &str,
        custom: bool,
    ) -> Result<(), ControlError> {
        let editor = self.spell_editor(owner)?;
        if !self.spell_available(editor)
            || self
                .document(editor)
                .is_some_and(|d| d.composition().is_some())
        {
            return Err(failure(
                "Spelling policy requires an available editor without composition",
            ));
        }
        let session = &mut self.spellchecks.entries.get_mut(&editor).unwrap().session;
        if custom {
            session.add_custom_word(word)
        } else {
            session.ignore_all(word)
        }
        .map_err(failure)
    }
    /// Replace only the verified issue in the exact current draft. This is one
    /// ordinary undoable edit and never directly accepts a business value.
    pub fn apply_spelling_suggestion(
        &mut self,
        owner: WidgetId,
        issue: usize,
        suggestion: usize,
    ) -> Result<(), ControlError> {
        let editor = self.spell_editor(owner)?;
        if !self.spell_available(editor) {
            return Err(failure("Spelling editor is unavailable or read-only"));
        }
        let document = self.document(editor).unwrap();
        let replacement = self.spellchecks.entries[&editor]
            .session
            .suggestion(document, issue, suggestion)
            .map_err(failure)?;
        let candidate = replacement.candidate(document).map_err(failure)?;
        let settings = self.editor_settings(editor).unwrap();
        let normalized = normalize_editor_text(&candidate, self.controls[&editor].single_line)
            .map_err(edit_error)?;
        if normalized != candidate {
            return Err(failure(
                "Spelling replacement cannot change editor text normalization",
            ));
        }
        editor_limits(settings.max_length)
            .validate(&candidate)
            .map_err(edit_error)?;
        self.preflight_editor_text(editor, &candidate)?;
        let control = self.controls.get_mut(&editor).unwrap();
        let Kind::TextBox(document) = &mut control.kind else {
            unreachable!()
        };
        let before = document.text().to_owned();
        document
            .try_replace_range(
                replacement.range(),
                replacement.text(),
                editor_limits(control.editor.max_length),
            )
            .map_err(edit_error)?;
        control.caret = TextCaret {
            index: document.selection().focus,
            ..Default::default()
        };
        self.scene_dirty = true;
        self.record_text_change(editor, &before);
        Ok(())
    }
    pub(super) fn invalidate_spellcheck(&mut self, editor: WidgetId) {
        if let Some(entry) = self.spellchecks.entries.get_mut(&editor) {
            entry.session.invalidate();
        }
    }
    pub(super) fn sync_spellchecks(&mut self) {
        let invalid: Vec<_> = self
            .spellchecks
            .entries
            .keys()
            .copied()
            .filter(|&id| {
                !self.spell_available(id)
                    || self.document(id).is_some_and(|d| d.composition().is_some())
            })
            .collect();
        for id in invalid {
            self.invalidate_spellcheck(id);
        }
        self.spellchecks.entries.retain(|id, e| {
            self.runtime.tree().contains(*id) && self.runtime.tree().contains(e.owner)
        });
        for (&id, entry) in &mut self.spellchecks.entries {
            if let Some(Control {
                kind: Kind::TextBox(document),
                ..
            }) = self.controls.get(&id)
            {
                entry.session.synchronize(document);
            }
        }
    }
}
