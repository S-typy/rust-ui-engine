//! Versioned native text ranges for the shared retained TextBox.
use super::*;
use rust_desktop_ui_controls::EditorTextSnapshot;

struct Editor {
    root: NodeId,
    clip: NodeId,
    runs: Vec<NodeId>,
    snapshot: EditorTextSnapshot,
    present: bool,
}

#[derive(Default)]
pub(super) struct TextAccessibility {
    editors: HashMap<WidgetId, Editor>,
}
fn allocate(next: &mut u64) -> NodeId {
    let id = NodeId(*next);
    *next = next
        .checked_add(1)
        .expect("text accessibility identity exhausted");
    id
}
impl Editor {
    fn position(&self, byte: usize) -> Option<accesskit::TextPosition> {
        for (index, run) in self.snapshot.runs.iter().enumerate() {
            if byte < run.range.start || byte > run.range.end {
                continue;
            }
            if byte == run.range.end
                && self
                    .snapshot
                    .runs
                    .get(index + 1)
                    .is_some_and(|next| next.range.start == byte)
            {
                continue;
            }
            let mut at = run.range.start;
            for character_index in 0..=run.character_lengths.len() {
                if at == byte {
                    return Some(accesskit::TextPosition {
                        node: self.runs[index],
                        character_index,
                    });
                }
                at += usize::from(*run.character_lengths.get(character_index)?);
            }
        }
        None
    }
    fn byte(&self, position: accesskit::TextPosition) -> Option<usize> {
        let index = self.runs.iter().position(|id| *id == position.node)?;
        let run = &self.snapshot.runs[index];
        if position.character_index > run.character_lengths.len() {
            return None;
        }
        Some(
            run.range.start
                + run.character_lengths[..position.character_index]
                    .iter()
                    .map(|value| usize::from(*value))
                    .sum::<usize>(),
        )
    }
}
impl TextAccessibility {
    pub(super) fn begin(&mut self) {
        for editor in self.editors.values_mut() {
            editor.present = false;
        }
    }
    pub(super) fn finish(&mut self) {
        self.editors.retain(|_, editor| editor.present);
    }
    pub(super) fn project(
        &mut self,
        ui: &Controls,
        (widget, root): (WidgetId, NodeId),
        scale: f32,
        next: &mut u64,
        parent: &mut Node,
        nodes: &mut Vec<(NodeId, Node)>,
    ) -> Result<(), String> {
        let Some(snapshot) = ui.editor_text_snapshot(widget).map_err(|e| e.to_string())? else {
            return Ok(());
        };
        let same = self.editors.get(&widget).is_some_and(|old| {
            old.snapshot.version == snapshot.version
                && old.snapshot.runs.len() == snapshot.runs.len()
                && old
                    .snapshot
                    .runs
                    .iter()
                    .zip(&snapshot.runs)
                    .all(|(a, b)| a.range == b.range && a.line == b.line && a.rtl == b.rtl)
        });
        if same {
            let old = self.editors.get_mut(&widget).unwrap();
            old.snapshot = snapshot;
            old.present = true;
        } else {
            let clip = allocate(next);
            let runs = snapshot.runs.iter().map(|_| allocate(next)).collect();
            self.editors.insert(
                widget,
                Editor {
                    root,
                    clip,
                    runs,
                    snapshot,
                    present: true,
                },
            );
        }
        let editor = &self.editors[&widget];
        if editor.snapshot.multiline {
            parent.set_role(Role::MultilineTextInput);
        }
        let mut clip = Node::new(Role::GenericContainer);
        clip.set_bounds(bounds(editor.snapshot.clip, scale));
        clip.set_clips_children();
        clip.set_children(editor.runs.clone());
        parent.push_child(editor.clip);
        nodes.push((editor.clip, clip));
        for (index, run) in editor.snapshot.runs.iter().enumerate() {
            let mut node = Node::new(Role::TextRun);
            node.set_value(editor.snapshot.text[run.range.clone()].to_owned());
            node.set_bounds(bounds(run.bounds, scale));
            node.set_text_direction(if run.rtl {
                accesskit::TextDirection::RightToLeft
            } else {
                accesskit::TextDirection::LeftToRight
            });
            node.set_character_lengths(run.character_lengths.clone());
            node.set_character_positions(
                run.character_positions
                    .iter()
                    .map(|v| v * scale)
                    .collect::<Vec<_>>(),
            );
            node.set_character_widths(
                run.character_widths
                    .iter()
                    .map(|v| v * scale)
                    .collect::<Vec<_>>(),
            );
            node.set_word_starts(run.word_starts.clone());
            if index > 0 && editor.snapshot.runs[index - 1].line == run.line {
                node.set_previous_on_line(editor.runs[index - 1]);
            }
            if editor
                .snapshot
                .runs
                .get(index + 1)
                .is_some_and(|next| next.line == run.line)
            {
                node.set_next_on_line(editor.runs[index + 1]);
            }
            nodes.push((editor.runs[index], node));
        }
        if let (Some(anchor), Some(focus)) = (
            editor.position(editor.snapshot.selection.anchor),
            editor.position(editor.snapshot.selection.focus),
        ) {
            parent.set_text_selection(accesskit::TextSelection { anchor, focus });
            if ui.is_enabled(widget) {
                parent.add_action(Action::SetTextSelection);
                parent.add_action(Action::ScrollIntoView);
            }
        }
        Ok(())
    }
    pub(super) fn action(
        &self,
        ui: &mut Controls,
        request: &accesskit::ActionRequest,
    ) -> Result<bool, String> {
        if !matches!(
            request.action,
            Action::SetTextSelection | Action::ScrollIntoView
        ) {
            return Ok(false);
        }
        let Some((widget, editor)) = self
            .editors
            .iter()
            .find(|(_, e)| e.root == request.target_node)
        else {
            return Ok(false);
        };
        if request.action == Action::ScrollIntoView {
            if let Some(accesskit::ActionData::ScrollTextIntoView {
                position,
                align_to_top,
            }) = &request.data
                && let Some(byte) = editor.byte(*position)
            {
                ui.scroll_editor_text_into_view(
                    *widget,
                    &editor.snapshot.version,
                    byte,
                    *align_to_top,
                )
                .map_err(|e| e.to_string())?;
            }
            return Ok(true);
        }
        if let Some(accesskit::ActionData::SetTextSelection(selection)) = &request.data
            && let (Some(anchor), Some(focus)) =
                (editor.byte(selection.anchor), editor.byte(selection.focus))
        {
            ui.select_editor_text(*widget, &editor.snapshot.version, anchor, focus)
                .map_err(|e| e.to_string())?;
        }
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_desktop_ui_controls::Theme;
    use rust_desktop_ui_core::{LayoutStyle, Length, Mutation, Size};
    #[test]
    fn readonly_selects_disabled_is_passive_and_password_has_no_text_ranges() {
        let mut ui = Controls::new(Theme::FluentLight).unwrap();
        let widget = ui
            .edit_box(
                ui.root(),
                "Read only",
                "value",
                LayoutStyle {
                    width: Length::Px(220.0),
                    height: Length::Px(36.0),
                    ..Default::default()
                },
            )
            .unwrap();
        let password = ui
            .password_box(
                ui.root(),
                "Secret",
                Default::default(),
                LayoutStyle {
                    width: Length::Px(220.0),
                    height: Length::Px(36.0),
                    ..Default::default()
                },
            )
            .unwrap();
        ui.set_password_value(password, "protected fixture")
            .unwrap();
        ui.set_read_only(widget, true).unwrap();
        ui.update(Size::new(400.0, 300.0)).unwrap();
        let mut native = TextAccessibility::default();
        let mut next = 100;
        for enabled in [true, false] {
            ui.set_enabled(widget, enabled).unwrap();
            ui.update(Size::new(400.0, 300.0)).unwrap();
            let mut parent = Node::new(Role::TextInput);
            let mut nodes = Vec::new();
            native.begin();
            native
                .project(
                    &ui,
                    (widget, NodeId(2)),
                    1.0,
                    &mut next,
                    &mut parent,
                    &mut nodes,
                )
                .unwrap();
            native.finish();
            assert!(!nodes.is_empty());
            assert_eq!(parent.supports_action(Action::SetTextSelection), enabled);
            assert_eq!(parent.supports_action(Action::ScrollIntoView), enabled);
        }
        let mut parent = Node::new(Role::PasswordInput);
        let mut nodes = Vec::new();
        native.begin();
        native
            .project(
                &ui,
                (password, NodeId(3)),
                1.0,
                &mut next,
                &mut parent,
                &mut nodes,
            )
            .unwrap();
        native.finish();
        assert!(nodes.is_empty() && parent.children().is_empty());
        assert!(native.editors.is_empty());
    }
    #[test]
    fn native_selection_uses_current_grapheme_positions_and_rejects_old_ranges() {
        let mut ui = Controls::new(Theme::FluentLight).unwrap();
        let widget = ui
            .text_box(
                ui.root(),
                "Text",
                "A e\u{301} 😀",
                LayoutStyle {
                    width: Length::Px(220.0),
                    height: Length::Px(36.0),
                    ..Default::default()
                },
            )
            .unwrap();
        ui.runtime.apply(Mutation::Focus(Some(widget))).unwrap();
        ui.update(Size::new(400.0, 200.0)).unwrap();
        let mut native = TextAccessibility::default();
        let mut next = 100;
        let mut parent = Node::new(Role::TextInput);
        let mut nodes = Vec::new();
        native.begin();
        native
            .project(
                &ui,
                (widget, NodeId(2)),
                1.0,
                &mut next,
                &mut parent,
                &mut nodes,
            )
            .unwrap();
        native.finish();
        let editor = &native.editors[&widget];
        let selection = accesskit::TextSelection {
            anchor: editor.position(2).unwrap(),
            focus: editor.position(5).unwrap(),
        };
        let request = accesskit::ActionRequest {
            action: Action::SetTextSelection,
            target_tree: accesskit::TreeId::ROOT,
            target_node: NodeId(2),
            data: Some(accesskit::ActionData::SetTextSelection(selection)),
        };
        assert!(native.action(&mut ui, &request).unwrap());
        assert_eq!(ui.document(widget).unwrap().selected_text(), "e\u{301}");
        assert_eq!(ui.accepted_value(widget), Some("A e\u{301} 😀"));
        assert!(!ui.document(widget).unwrap().can_undo());
        let ids = native.editors[&widget].runs.clone();
        ui.update(Size::new(400.0, 200.0)).unwrap();
        native.begin();
        native
            .project(
                &ui,
                (widget, NodeId(2)),
                1.5,
                &mut next,
                &mut Node::new(Role::TextInput),
                &mut Vec::new(),
            )
            .unwrap();
        native.finish();
        assert_eq!(native.editors[&widget].runs, ids);
        ui.set_text(widget, "Changed").unwrap();
        let before = ui.document(widget).unwrap().selection();
        assert!(native.action(&mut ui, &request).unwrap());
        assert_eq!(ui.document(widget).unwrap().selection(), before);
        ui.update(Size::new(400.0, 200.0)).unwrap();
        native.begin();
        native
            .project(
                &ui,
                (widget, NodeId(2)),
                1.0,
                &mut next,
                &mut Node::new(Role::TextInput),
                &mut Vec::new(),
            )
            .unwrap();
        native.finish();
        assert_ne!(native.editors[&widget].runs, ids);
        assert!(native.action(&mut ui, &request).unwrap());
        assert_eq!(ui.document(widget).unwrap().selection(), before);
    }
}
