//! Shared, versioned accessibility contract for plain text editors.
use super::*;
use rust_desktop_ui_text::{AccessibleTextRun, DocumentVersion, Selection};

#[derive(Clone, Debug)]
pub struct EditorTextSnapshot {
    pub version: DocumentVersion,
    pub text: String,
    pub selection: Selection,
    pub runs: Vec<AccessibleTextRun>,
    pub clip: Rect,
    pub multiline: bool,
}

impl Controls {
    /// Reveal a versioned text position without moving focus or selection.
    /// Vertical alignment applies to multiline text; single-line text reveals
    /// the requested caret using the smallest horizontal scroll.
    pub fn scroll_editor_text_into_view(
        &mut self,
        id: WidgetId,
        version: &DocumentVersion,
        byte: usize,
        align_to_top: bool,
    ) -> Result<bool, ControlError> {
        if !self.is_enabled(id) || !self.semantics().iter().any(|node| node.id == id) {
            return Ok(false);
        }
        let Some(control) = self.controls.get(&id) else {
            return Ok(false);
        };
        let Kind::TextBox(document) = &control.kind else {
            return Ok(false);
        };
        if document.version() != *version
            || document.composition().is_some()
            || document.grapheme_boundaries().binary_search(&byte).is_err()
        {
            return Ok(false);
        }
        let Some(layout) = self.text_layouts.get(&id) else {
            return Ok(false);
        };
        // Reject stale shaping as well as a stale semantic snapshot.
        if !document.text().is_empty() {
            layout
                .accessible_runs(document.text())
                .map_err(|error| ControlError::Text(error.to_string()))?;
        }
        let clip = layout.text_clip();
        let caret = layout.caret_rect(byte);
        let offset = if control.single_line {
            let delta = if caret.x < clip.x {
                caret.x - clip.x
            } else if caret.x + caret.width > clip.x + clip.width {
                caret.x + caret.width - clip.x - clip.width
            } else {
                0.0
            };
            Point::new(control.text_scroll_x + delta, 0.0)
        } else {
            let delta = if align_to_top {
                caret.y - clip.y
            } else {
                caret.y + caret.height - clip.y - clip.height
            };
            Point::new(0.0, control.text_scroll + delta)
        };
        self.scroll_editor_to(id, offset)?;
        Ok(true)
    }

    /// Native adapters use the exact shaped text, graphemes and geometry.
    /// Passwords and active preedit are never exposed through this contract.
    /// A stale layout or exhausted semantic budget is an explicit error.
    pub fn editor_text_snapshot(
        &self,
        id: WidgetId,
    ) -> Result<Option<EditorTextSnapshot>, ControlError> {
        let Some(control) = self.controls.get(&id) else {
            return Ok(None);
        };
        let Kind::TextBox(document) = &control.kind else {
            return Ok(None);
        };
        if document.composition().is_some() {
            return Ok(None);
        }
        let Some(layout) = self.text_layouts.get(&id) else {
            return Ok(None);
        };
        let runs = if document.text().is_empty() {
            vec![AccessibleTextRun {
                range: 0..0,
                line: 0,
                bounds: layout.caret_rect(0),
                rtl: false,
                character_lengths: Vec::new(),
                character_positions: Vec::new(),
                character_widths: Vec::new(),
                word_starts: Vec::new(),
            }]
        } else {
            layout
                .accessible_runs(document.text())
                .map_err(|e| ControlError::Text(e.to_string()))?
        };
        Ok(Some(EditorTextSnapshot {
            version: document.version(),
            text: document.text().to_owned(),
            selection: document.selection(),
            runs,
            clip: layout.text_clip(),
            multiline: !control.single_line,
        }))
    }

    /// Change selection without editing, accepting a business value or changing
    /// focus. Read-only permits selection; disabled/stale/preedit targets reject
    /// the request without changing state. Offsets must be UTF-8 grapheme edges.
    pub fn select_editor_text(
        &mut self,
        id: WidgetId,
        version: &DocumentVersion,
        anchor: usize,
        focus: usize,
    ) -> Result<bool, ControlError> {
        if !self.is_enabled(id) || !self.semantics().iter().any(|n| n.id == id) {
            return Ok(false);
        }
        let Some(Control {
            kind: Kind::TextBox(document),
            caret,
            caret_goal_x,
            caret_reveal,
            ..
        }) = self.controls.get_mut(&id)
        else {
            return Ok(false);
        };
        if document.version() != *version || document.composition().is_some() {
            return Ok(false);
        }
        if document.set_selection(anchor, focus).is_err() {
            return Ok(false);
        }
        *caret = TextCaret {
            index: focus,
            ..Default::default()
        };
        *caret_goal_x = None;
        *caret_reveal = true;
        self.caret_visible = true;
        self.scene_dirty = true;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn text_range_scroll_reveals_the_endpoint_without_editing_or_moving_selection() {
        for multiline in [false, true] {
            let mut ui = Controls::new(Theme::FluentLight).unwrap();
            let value = if multiline {
                "one two three four\n".repeat(30)
            } else {
                "long text ".repeat(50)
            };
            let style = LayoutStyle {
                width: Length::Px(180.0),
                height: Length::Px(72.0),
                ..Default::default()
            };
            let id = if multiline {
                ui.text_box(ui.root(), "Text", &value, style).unwrap()
            } else {
                ui.edit_box(ui.root(), "Text", &value, style).unwrap()
            };
            ui.update(Size::new(400.0, 300.0)).unwrap();
            let snapshot = ui.editor_text_snapshot(id).unwrap().unwrap();
            let selection = ui.document(id).unwrap().selection();
            let focus = ui.runtime.focused();
            assert!(
                ui.scroll_editor_text_into_view(id, &snapshot.version, value.len(), false)
                    .unwrap()
            );
            ui.update(Size::new(400.0, 300.0)).unwrap();
            let offset = ui.editor_scroll_offset(id).unwrap();
            assert!(if multiline {
                offset.y > 100.0
            } else {
                offset.x > 100.0
            });
            let caret = ui.text_layouts[&id].caret_rect(value.len());
            let clip = ui.text_layouts[&id].text_clip();
            assert!(
                caret.y >= clip.y - 1.0 && caret.y + caret.height <= clip.y + clip.height + 1.0
            );
            assert!(
                caret.x >= clip.x - 1.0 && caret.x <= clip.x + clip.width + 1.0,
                "multiline={multiline} offset={offset:?} caret={caret:?} clip={clip:?}"
            );
            assert!(
                ui.scroll_editor_text_into_view(id, &snapshot.version, 0, true)
                    .unwrap()
            );
            ui.update(Size::new(400.0, 300.0)).unwrap();
            assert_eq!(ui.editor_scroll_offset(id), Some(Point::new(0.0, 0.0)));
            assert_eq!(ui.document(id).unwrap().selection(), selection);
            assert_eq!(ui.runtime.focused(), focus);
            assert_eq!(ui.accepted_value(id), Some(value.as_str()));
            assert!(!ui.document(id).unwrap().can_undo());
            ui.set_text(id, "replacement").unwrap();
            assert!(
                !ui.scroll_editor_text_into_view(id, &snapshot.version, 0, true)
                    .unwrap()
            );
        }
    }
    #[test]
    fn text_selection_is_versioned_readonly_safe_and_never_exposes_passwords() {
        let mut ui = Controls::new(Theme::FluentLight).unwrap();
        let id = ui
            .text_box(
                ui.root(),
                "Editor",
                "A e\u{301} 😀",
                LayoutStyle {
                    width: Length::Px(200.0),
                    height: Length::Px(36.0),
                    ..Default::default()
                },
            )
            .unwrap();
        ui.runtime.apply(Mutation::Focus(Some(id))).unwrap();
        ui.update(Size::new(400.0, 200.0)).unwrap();
        let snapshot = ui.editor_text_snapshot(id).unwrap().unwrap();
        let accepted = ui.accepted_value(id).unwrap().to_owned();
        ui.drain_text_changes();
        ui.drain_edit_events();
        assert!(!ui.select_editor_text(id, &snapshot.version, 3, 4).unwrap());
        assert!(ui.select_editor_text(id, &snapshot.version, 2, 5).unwrap());
        assert_eq!(ui.document(id).unwrap().selected_text(), "e\u{301}");
        ui.set_read_only(id, true).unwrap();
        assert!(ui.select_editor_text(id, &snapshot.version, 6, 10).unwrap());
        assert_eq!(ui.document(id).unwrap().selected_text(), "😀");
        assert_eq!(ui.accepted_value(id), Some(accepted.as_str()));
        assert!(!ui.document(id).unwrap().can_undo());
        assert!(ui.drain_text_changes().is_empty() && ui.drain_edit_events().is_empty());
        ui.set_text(id, "New text").unwrap();
        assert!(!ui.select_editor_text(id, &snapshot.version, 0, 1).unwrap());
        ui.set_read_only(id, false).unwrap();
        ui.update(Size::new(400.0, 200.0)).unwrap();
        let version = ui.document(id).unwrap().version();
        ui.ime_preedit("に", Some((0, 0))).unwrap();
        assert!(ui.editor_text_snapshot(id).unwrap().is_none());
        assert!(!ui.select_editor_text(id, &version, 0, 1).unwrap());
        ui.cancel_preedit();
        ui.set_enabled(id, false).unwrap();
        assert!(!ui.select_editor_text(id, &version, 0, 1).unwrap());
        let password = ui
            .password_box(
                ui.root(),
                "Protected",
                Default::default(),
                LayoutStyle::default(),
            )
            .unwrap();
        ui.set_password_value(password, "secret").unwrap();
        ui.update(Size::new(400.0, 200.0)).unwrap();
        assert!(ui.editor_text_snapshot(password).unwrap().is_none());
        assert!(!ui.select_editor_text(password, &version, 0, 1).unwrap());
    }
}
