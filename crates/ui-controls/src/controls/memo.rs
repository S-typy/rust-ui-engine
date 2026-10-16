//! A retained multiline editor behind a compact disclosure surface.
use super::*;
use crate::EditorValidationStatus;
use rust_desktop_ui_core::{GridPlacement, GridTrack, Modifiers};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MemoEditSettings {
    pub popup_size: Size,
}

impl Default for MemoEditSettings {
    fn default() -> Self {
        Self {
            popup_size: Size::new(480.0, 300.0),
        }
    }
}
impl MemoEditSettings {
    pub fn validate(self) -> Result<(), ControlError> {
        if !self.popup_size.is_valid()
            || !(160.0..=16_384.0).contains(&self.popup_size.width)
            || !(120.0..=16_384.0).contains(&self.popup_size.height)
        {
            return Err(ControlError::Invalid(
                "Memo popup requires width 160..16384 and height 120..16384".into(),
            ));
        }
        Ok(())
    }
}

/// Permanent parts. Text bindings, editor settings and validation target
/// `editor`; `container` is the compact disclosure and accessible owner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MemoEdit {
    pub container: WidgetId,
    pub editor: WidgetId,
    pub popup: Flyout,
    pub apply_button: WidgetId,
    pub cancel_button: WidgetId,
}

#[derive(Clone)]
struct Entry {
    parts: MemoEdit,
    settings: MemoEditSettings,
    pending_close: bool,
    explicit_close: bool,
}
#[derive(Default)]
pub(super) struct MemoState {
    entries: HashMap<WidgetId, Entry>,
}

impl Controls {
    pub fn memo_edit(
        &mut self,
        parent: WidgetId,
        label: impl Into<String>,
        value: impl Into<String>,
        settings: MemoEditSettings,
        style: LayoutStyle,
    ) -> Result<MemoEdit, ControlError> {
        settings.validate()?;
        let label = label.into();
        let value = value.into();
        TextLimits::default().validate(&value).map_err(edit_error)?;
        TextLimits::default().validate(&label).map_err(edit_error)?;
        let container = self.button(parent, &label, None, style)?;
        let result = (|| {
            self.set_automation_name(container, &label)?;
            let popup = self.flyout(container, settings.popup_size)?;
            let height = self.theme.tokens().control_height;
            self.edit_props(popup.content, |props| {
                props.style.kind = LayoutKind::Grid;
                props.style.grid_columns = vec![GridTrack::Fr(1.0)];
                props.style.grid_rows = vec![GridTrack::Fr(1.0), GridTrack::Px(height)];
            })?;
            let editor = self.text_box(
                popup.content,
                &label,
                &value,
                LayoutStyle {
                    width: Length::Percent(1.0),
                    height: Length::Percent(1.0),
                    grid_row: GridPlacement {
                        start: Some(1),
                        span: 1,
                    },
                    ..Default::default()
                },
            )?;
            let footer = self.transparent_panel(
                popup.content,
                LayoutStyle {
                    kind: LayoutKind::Grid,
                    grid_columns: vec![GridTrack::Fr(1.0), GridTrack::Fr(1.0)],
                    grid_rows: vec![GridTrack::Fr(1.0)],
                    gap: 8.0,
                    grid_row: GridPlacement {
                        start: Some(2),
                        span: 1,
                    },
                    ..Default::default()
                },
            )?;
            let apply_button = self.button(
                footer,
                "Apply",
                None,
                LayoutStyle {
                    width: Length::Percent(1.0),
                    height: Length::Percent(1.0),
                    grid_column: GridPlacement {
                        start: Some(1),
                        span: 1,
                    },
                    ..Default::default()
                },
            )?;
            let cancel_button = self.button(
                footer,
                "Cancel",
                None,
                LayoutStyle {
                    width: Length::Percent(1.0),
                    height: Length::Percent(1.0),
                    grid_column: GridPlacement {
                        start: Some(2),
                        span: 1,
                    },
                    ..Default::default()
                },
            )?;
            self.set_button_flyout(container, popup)?;
            let parts = MemoEdit {
                container,
                editor,
                popup,
                apply_button,
                cancel_button,
            };
            self.memos.entries.insert(
                container,
                Entry {
                    parts,
                    settings,
                    pending_close: false,
                    explicit_close: false,
                },
            );
            Ok(parts)
        })();
        if result.is_err() {
            self.remove(container)?;
        }
        self.sync_memos()?;
        result
    }

    /// Resolve the retained multiline document owned by a compact memo.
    pub fn memo_editor(&self, id: WidgetId) -> Option<WidgetId> {
        let editor = self.memos.entries.get(&id)?.parts.editor;
        (self.runtime.tree().contains(id) && self.document(editor).is_some()).then_some(editor)
    }
    pub fn memo_parts(&self, id: WidgetId) -> Option<MemoEdit> {
        self.memo_editor(id)?;
        Some(self.memos.entries[&id].parts)
    }
    pub fn memo_settings(&self, id: WidgetId) -> Option<MemoEditSettings> {
        self.memo_editor(id)?;
        Some(self.memos.entries[&id].settings)
    }
    pub fn set_memo_settings(
        &mut self,
        id: WidgetId,
        settings: MemoEditSettings,
    ) -> Result<(), ControlError> {
        settings.validate()?;
        let parts = self
            .memo_parts(id)
            .ok_or_else(|| ControlError::Invalid("Unknown MemoEdit".into()))?;
        self.set_flyout_placement(parts.popup, settings.popup_size, FlyoutPlacement::Below)?;
        self.memos.entries.get_mut(&id).unwrap().settings = settings;
        Ok(())
    }
    fn require_memo(&self, parts: MemoEdit) -> Result<(), ControlError> {
        if self.memo_parts(parts.container) == Some(parts) {
            Ok(())
        } else {
            Err(ControlError::Invalid("Unknown MemoEdit parts".into()))
        }
    }
    pub fn memo_is_open(&self, parts: MemoEdit) -> bool {
        self.memo_parts(parts.container) == Some(parts) && self.flyout_is_open(parts.popup)
    }
    pub fn open_memo(&mut self, parts: MemoEdit) -> Result<(), ControlError> {
        self.require_memo(parts)?;
        self.open_flyout(parts.popup)
    }
    /// Close using the editor's normal commit-on-focus-loss policy. Invalid
    /// drafts remain retained; hiding/disable/removal never accepts them.
    pub fn close_memo(&mut self, parts: MemoEdit) -> Result<(), ControlError> {
        self.require_memo(parts)?;
        self.close_flyout(parts.popup)
    }
    /// Accept and close only when valid. Pending asynchronous validation keeps
    /// the surface open until the same draft is accepted; invalid stays open.
    pub fn commit_memo(&mut self, parts: MemoEdit) -> Result<CommitOutcome, ControlError> {
        self.require_memo(parts)?;
        let outcome = self.commit_edit(parts.editor)?;
        self.memos
            .entries
            .get_mut(&parts.container)
            .unwrap()
            .pending_close = outcome == CommitOutcome::Pending;
        if matches!(outcome, CommitOutcome::Committed | CommitOutcome::Unchanged) {
            self.finish_memo_close(parts)?;
        }
        Ok(outcome)
    }
    /// Discard the draft and its undo history, restore accepted text, and close.
    pub fn cancel_memo(&mut self, parts: MemoEdit) -> Result<(), ControlError> {
        self.require_memo(parts)?;
        self.memos
            .entries
            .get_mut(&parts.container)
            .unwrap()
            .pending_close = false;
        self.cancel_edit(parts.editor)?;
        self.finish_memo_close(parts)
    }

    fn finish_memo_close(&mut self, parts: MemoEdit) -> Result<(), ControlError> {
        self.memos
            .entries
            .get_mut(&parts.container)
            .unwrap()
            .explicit_close = true;
        let result = self.close_flyout(parts.popup);
        if let Some(entry) = self.memos.entries.get_mut(&parts.container) {
            entry.explicit_close = false;
        }
        result
    }

    pub(super) fn preflight_memo_removal(&self, id: WidgetId) -> Result<(), ControlError> {
        if self.memos.entries.values().any(|entry| {
            [
                entry.parts.editor,
                entry.parts.popup.container,
                entry.parts.apply_button,
                entry.parts.cancel_button,
            ]
            .contains(&id)
                || self.runtime.tree().parent(entry.parts.apply_button) == Some(id)
        }) {
            Err(ControlError::Invalid(
                "Remove the MemoEdit container, not its required editor or popup parts".into(),
            ))
        } else {
            Ok(())
        }
    }
    pub(super) fn prune_memos(&mut self) {
        self.memos.entries.retain(|owner, entry| {
            [
                *owner,
                entry.parts.editor,
                entry.parts.popup.container,
                entry.parts.popup.content,
                entry.parts.apply_button,
                entry.parts.cancel_button,
            ]
            .into_iter()
            .all(|id| self.runtime.tree().contains(id))
        });
    }
    pub(super) fn cancel_unavailable_memo_validation(&mut self) {
        let unavailable: Vec<_> = self
            .memos
            .entries
            .values()
            .filter(|entry| !self.window_focused || !self.is_enabled(entry.parts.container))
            .map(|entry| entry.parts)
            .collect();
        for parts in unavailable {
            self.invalidate_async_editor(parts.editor);
            if let Some(entry) = self.memos.entries.get_mut(&parts.container) {
                entry.pending_close = false;
            }
        }
    }
    pub(super) fn memo_focus_editor(&self, id: WidgetId) -> Option<WidgetId> {
        self.memos
            .entries
            .values()
            .find(|entry| {
                self.flyout_is_open(entry.parts.popup)
                    && (id == entry.parts.container
                        || self.flyout_descendant(id, entry.parts.popup.container))
            })
            .map(|entry| entry.parts.editor)
    }
    pub(super) fn memo_will_close(&mut self, popup: WidgetId) -> Result<(), ControlError> {
        let Some(parts) = self
            .memos
            .entries
            .values()
            .find(|entry| entry.parts.popup.container == popup)
            .map(|entry| entry.parts)
        else {
            return Ok(());
        };
        self.memos
            .entries
            .get_mut(&parts.container)
            .unwrap()
            .pending_close = false;
        if self.memos.entries[&parts.container].explicit_close {
            return Ok(());
        }
        if self.window_focused
            && self.is_enabled(parts.container)
            && self.is_text_editable(parts.editor)
            && self
                .editor_settings(parts.editor)
                .is_some_and(|settings| settings.commit_on_focus_lost)
        {
            if self
                .document(parts.editor)
                .is_some_and(|doc| doc.composition().is_some())
            {
                self.controls
                    .get_mut(&parts.editor)
                    .and_then(|control| match &mut control.kind {
                        Kind::TextBox(document) => {
                            document.cancel_preedit();
                            Some(())
                        }
                        _ => None,
                    });
            } else {
                self.commit_edit(parts.editor)?;
            }
        }
        Ok(())
    }
    pub(super) fn sync_memos(&mut self) -> Result<(), ControlError> {
        self.prune_memos();
        let entries: Vec<_> = self.memos.entries.values().cloned().collect();
        for entry in entries {
            let parts = entry.parts;
            let document = self.document(parts.editor).unwrap();
            let text = document.text();
            let prefix_end = document.grapheme_boundary(80).unwrap_or(text.len());
            let first_line = text[..prefix_end]
                .char_indices()
                .find(|(_, c)| single_line_separator(*c))
                .map_or(prefix_end, |(i, _)| i);
            let end = first_line;
            let mut preview = text[..end].to_owned();
            if end < text.len() {
                preview.push('…');
            }
            if preview.is_empty() {
                preview.clone_from(&self.controls[&parts.editor].editor.placeholder);
            }
            if self.label_text(parts.container) != Some(preview.as_str()) {
                self.set_label(parts.container, &preview)?;
            }
            let editable = !self.editor_settings(parts.editor).unwrap().is_read_only;
            if self.controls[&parts.apply_button].enabled != editable {
                self.set_enabled(parts.apply_button, editable)?;
            }
            let height = self.theme.tokens().control_height;
            self.edit_props(parts.popup.content, |props| {
                props.style.grid_rows = vec![GridTrack::Fr(1.0), GridTrack::Px(height)];
            })?;
            if entry.pending_close
                && self.editor_validation_status(parts.editor) == EditorValidationStatus::Idle
            {
                self.memos
                    .entries
                    .get_mut(&parts.container)
                    .unwrap()
                    .pending_close = false;
                if self.is_edit_dirty(parts.editor) == Some(false)
                    && self.validation_error(parts.editor).is_none()
                {
                    self.finish_memo_close(parts)?;
                }
            }
        }
        Ok(())
    }
    pub(super) fn activate_memo(&mut self, id: WidgetId) -> Result<bool, ControlError> {
        let Some(parts) = self
            .memos
            .entries
            .values()
            .find(|entry| {
                [
                    entry.parts.container,
                    entry.parts.apply_button,
                    entry.parts.cancel_button,
                ]
                .contains(&id)
            })
            .map(|entry| entry.parts)
        else {
            return Ok(false);
        };
        if id == parts.container {
            if self.memo_is_open(parts) {
                self.close_memo(parts)?;
            } else {
                self.open_memo(parts)?;
            }
        } else if id == parts.cancel_button {
            if self.is_text_editable(parts.editor) {
                self.cancel_memo(parts)?;
            } else {
                self.finish_memo_close(parts)?;
            }
        } else {
            self.commit_memo(parts)?;
        }
        Ok(true)
    }
    pub(super) fn memo_before_input(&mut self, event: &InputEvent) -> Result<bool, ControlError> {
        let Some(focused) = self.runtime.focused() else {
            return Ok(false);
        };
        let Some(editor) = self.memo_focus_editor(focused) else {
            return Ok(false);
        };
        let parts = self
            .memos
            .entries
            .values()
            .find(|entry| entry.parts.editor == editor)
            .unwrap()
            .parts;
        match event {
            InputEvent::KeyDown {
                key: Key::Escape,
                modifiers,
                repeat: false,
            } if *modifiers == Modifiers::default()
                && self.popup.is_none()
                && self.rich.popup.is_none() =>
            {
                if self.document(editor).unwrap().composition().is_some() {
                    self.cancel_preedit();
                } else if !self.is_text_editable(editor) {
                    self.finish_memo_close(parts)?;
                } else {
                    self.cancel_memo(parts)?;
                }
                Ok(true)
            }
            InputEvent::KeyDown {
                key: Key::Enter,
                modifiers,
                repeat: false,
            } if (modifiers.control || modifiers.super_key)
                && !modifiers.alt
                && !modifiers.shift =>
            {
                if self.is_text_editable(editor) {
                    self.commit_memo(parts)?;
                }
                Ok(true)
            }
            _ => Ok(false),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (Controls, MemoEdit) {
        let mut ui = Controls::new(Theme::FluentLight).unwrap();
        let memo = ui
            .memo_edit(
                ui.root(),
                "Notes",
                "accepted",
                MemoEditSettings::default(),
                LayoutStyle {
                    width: Length::Px(220.0),
                    height: Length::Px(36.0),
                    ..Default::default()
                },
            )
            .unwrap();
        frame(&mut ui);
        (ui, memo)
    }
    fn frame(ui: &mut Controls) {
        ui.update(Size::new(640.0, 480.0)).unwrap();
    }
    fn key(ui: &mut Controls, key: Key, control: bool) {
        ui.dispatch(InputEvent::KeyDown {
            key,
            modifiers: Modifiers {
                control,
                ..Default::default()
            },
            repeat: false,
        })
        .unwrap();
    }

    #[test]
    fn memo_retains_document_and_separates_newline_apply_and_cancel() {
        let (mut ui, memo) = setup();
        ui.open_memo(memo).unwrap();
        assert_eq!(ui.runtime.focused(), Some(memo.editor));
        ui.text_input("👩‍💻").unwrap();
        key(&mut ui, Key::Enter, false);
        ui.text_input("second").unwrap();
        assert_eq!(
            ui.document(memo.editor).unwrap().text(),
            "accepted👩‍💻\nsecond"
        );
        assert_eq!(ui.accepted_value(memo.editor), Some("accepted"));
        frame(&mut ui);
        assert_eq!(ui.label_text(memo.container), Some("accepted👩‍💻…"));
        key(&mut ui, Key::Enter, true);
        assert!(!ui.memo_is_open(memo));
        assert_eq!(ui.accepted_value(memo.editor), Some("accepted👩‍💻\nsecond"));
        ui.open_memo(memo).unwrap();
        ui.text_input(" discarded").unwrap();
        ui.runtime
            .apply(Mutation::Focus(Some(memo.cancel_button)))
            .unwrap();
        frame(&mut ui);
        assert_eq!(ui.accepted_value(memo.editor), Some("accepted👩‍💻\nsecond"));
        ui.activate(memo.cancel_button).unwrap();
        assert_eq!(
            ui.document(memo.editor).unwrap().text(),
            "accepted👩‍💻\nsecond"
        );
        ui.open_memo(memo).unwrap();
        key(&mut ui, Key::Character("z".into()), true);
        assert_eq!(
            ui.document(memo.editor).unwrap().text(),
            "accepted👩‍💻\nsecond"
        );
        assert_eq!(ui.memo_editor(memo.container), Some(memo.editor));
    }

    #[test]
    fn memo_invalid_draft_stays_open_on_apply_but_outside_close_retains_it() {
        let (mut ui, memo) = setup();
        ui.set_editor_validation(
            memo.editor,
            EditorValidation {
                is_required: true,
                ..Default::default()
            },
        )
        .unwrap();
        ui.open_memo(memo).unwrap();
        ui.set_text(memo.editor, "").unwrap();
        assert!(matches!(
            ui.commit_memo(memo).unwrap(),
            CommitOutcome::Invalid(_)
        ));
        assert!(ui.memo_is_open(memo));
        ui.close_memo(memo).unwrap();
        assert!(!ui.memo_is_open(memo));
        assert_eq!(ui.accepted_value(memo.editor), Some("accepted"));
        assert_eq!(ui.document(memo.editor).unwrap().text(), "");
        ui.open_memo(memo).unwrap();
        ui.text_input("valid").unwrap();
        ui.close_memo(memo).unwrap();
        assert_eq!(ui.accepted_value(memo.editor), Some("valid"));
        ui.set_editor_settings(
            memo.editor,
            EditorSettings {
                commit_on_focus_lost: false,
                ..Default::default()
            },
        )
        .unwrap();
        ui.open_memo(memo).unwrap();
        ui.text_input(" draft").unwrap();
        ui.close_memo(memo).unwrap();
        assert_eq!(ui.accepted_value(memo.editor), Some("valid"));
        assert_eq!(ui.document(memo.editor).unwrap().text(), "valid draft");
    }

    #[test]
    fn memo_composition_escape_readonly_disable_and_removal_keep_session_rules() {
        let (mut ui, memo) = setup();
        ui.open_memo(memo).unwrap();
        ui.text_input(" draft").unwrap();
        ui.ime_preedit("候補", Some((0, 6))).unwrap();
        key(&mut ui, Key::Escape, false);
        assert!(ui.memo_is_open(memo));
        assert!(ui.document(memo.editor).unwrap().composition().is_none());
        assert_eq!(ui.document(memo.editor).unwrap().text(), "accepted draft");
        ui.set_read_only(memo.editor, true).unwrap();
        frame(&mut ui);
        assert!(!ui.is_enabled(memo.apply_button));
        let semantics = ui.semantics();
        let node = semantics
            .iter()
            .find(|node| node.id == memo.container)
            .unwrap();
        assert!(node.read_only);
        assert_eq!(node.label, "Notes");
        assert_eq!(node.value.as_deref(), Some("accepted draft"));
        assert!(node.actions.contains(&SemanticAction::Collapse));
        key(&mut ui, Key::Escape, false);
        assert!(!ui.memo_is_open(memo));
        assert_eq!(ui.document(memo.editor).unwrap().text(), "accepted draft");
        assert_eq!(ui.accepted_value(memo.editor), Some("accepted"));
        ui.set_read_only(memo.editor, false).unwrap();
        ui.open_memo(memo).unwrap();
        ui.ime_preedit("候補", Some((0, 6))).unwrap();
        ui.set_enabled(memo.container, false).unwrap();
        frame(&mut ui);
        assert!(!ui.memo_is_open(memo));
        assert_eq!(ui.accepted_value(memo.editor), Some("accepted"));
        assert!(ui.document(memo.editor).unwrap().composition().is_none());
        for id in [
            memo.editor,
            memo.popup.container,
            memo.popup.content,
            memo.apply_button,
            memo.cancel_button,
        ] {
            assert!(ui.remove(id).is_err());
        }
        ui.remove(memo.container).unwrap();
        assert!(ui.memo_parts(memo.container).is_none());
        assert!(!ui.runtime.tree().contains(memo.editor));
        assert!(!ui.runtime.tree().contains(memo.popup.container));
    }

    #[test]
    fn memo_settings_rejection_is_atomic_and_preview_never_splits_graphemes() {
        let (mut ui, memo) = setup();
        let value = "e\u{301}".repeat(81);
        ui.set_text(memo.editor, &value).unwrap();
        frame(&mut ui);
        assert_eq!(
            ui.label_text(memo.container),
            Some(format!("{}…", "e\u{301}".repeat(80)).as_str())
        );
        let initial = ui.memo_settings(memo.container).unwrap();
        assert!(
            ui.set_memo_settings(
                memo.container,
                MemoEditSettings {
                    popup_size: Size::new(20.0, 100.0)
                }
            )
            .is_err()
        );
        assert_eq!(ui.memo_settings(memo.container), Some(initial));
        assert_eq!(ui.document(memo.editor).unwrap().text(), value);
        ui.open_memo(memo).unwrap();
        ui.update(Size::new(100.0, 80.0)).unwrap();
        let bounds = ui.runtime.bounds(memo.popup.container).unwrap();
        assert!(
            bounds.x >= 0.0
                && bounds.y >= 0.0
                && bounds.x + bounds.width <= 100.0
                && bounds.y + bounds.height <= 80.0
        );
    }

    #[test]
    fn memo_async_apply_closes_only_after_acceptance_and_disable_discards_late_result() {
        use std::sync::{Arc, Mutex, mpsc};
        use std::time::Duration;
        let (mut ui, memo) = setup();
        let (release, gate) = mpsc::channel();
        let gate = Arc::new(Mutex::new(gate));
        let (wake, completed) = mpsc::channel();
        ui.set_editor_validation_waker(Some(Arc::new(move || {
            let _ = wake.send(());
        })));
        ui.set_async_editor_validator(
            memo.editor,
            Some(crate::AsyncEditorValidator::new(move |_| {
                gate.lock().unwrap().recv().unwrap();
                Ok(())
            })),
        )
        .unwrap();
        ui.open_memo(memo).unwrap();
        ui.text_input(" one").unwrap();
        assert_eq!(ui.commit_memo(memo).unwrap(), CommitOutcome::Pending);
        assert!(ui.memo_is_open(memo));
        release.send(()).unwrap();
        completed.recv_timeout(Duration::from_secs(2)).unwrap();
        frame(&mut ui);
        assert!(!ui.memo_is_open(memo));
        assert_eq!(ui.accepted_value(memo.editor), Some("accepted one"));
        ui.open_memo(memo).unwrap();
        ui.text_input(" two").unwrap();
        assert_eq!(ui.commit_memo(memo).unwrap(), CommitOutcome::Pending);
        ui.set_enabled(memo.container, false).unwrap();
        release.send(()).unwrap();
        completed.recv_timeout(Duration::from_secs(2)).unwrap();
        frame(&mut ui);
        assert!(!ui.memo_is_open(memo));
        assert_eq!(ui.accepted_value(memo.editor), Some("accepted one"));
        assert_eq!(ui.document(memo.editor).unwrap().text(), "accepted one two");
    }
}
