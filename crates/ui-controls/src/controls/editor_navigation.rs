//! Selection, visual navigation and scrolling of plain text editors.
use super::*;

#[derive(Clone, Copy)]
pub(super) struct EditorScrollDrag {
    id: WidgetId,
    grab: f32,
}

#[derive(Clone, Copy)]
struct EditorScrollBar {
    track: Rect,
    thumb: Rect,
    maximum: f32,
}

impl Controls {
    pub(super) fn place_caret(
        &mut self,
        id: WidgetId,
        point: Point,
        extend: bool,
    ) -> Result<(), ControlError> {
        if self
            .document(id)
            .is_some_and(|document| document.composition().is_some())
        {
            self.cancel_preedit();
            self.rebuild_scene()?;
        }
        let Some(layout) = self.text_layouts.get(&id) else {
            return Ok(());
        };
        let next_caret = layout.hit_test_caret(point);
        let index = next_caret.index;
        if let Some(Control {
            kind: Kind::TextBox(document),
            caret,
            caret_goal_x,
            caret_reveal,
            ..
        }) = self.controls.get_mut(&id)
        {
            let anchor = if extend {
                document.selection().anchor
            } else {
                index
            };
            document
                .set_selection(anchor, index)
                .map_err(|e| ControlError::Text(e.to_string()))?;
            *caret = next_caret;
            *caret_goal_x = None;
            *caret_reveal = true;
            document.cancel_preedit();
            self.scene_dirty = true;
        }
        Ok(())
    }
    pub(super) fn edit_key(
        &mut self,
        id: WidgetId,
        key: &Key,
        modifiers: rust_desktop_ui_core::Modifiers,
        repeat: bool,
    ) -> Result<(), ControlError> {
        let shortcut = (modifiers.control && !modifiers.alt) || modifiers.super_key;
        let confirm_key = *key == Key::Enter
            && !repeat
            && !modifiers.shift
            && !modifiers.alt
            && self.controls.get(&id).is_some_and(|control| {
                if control.single_line {
                    !modifiers.control && !modifiers.super_key
                } else {
                    shortcut
                }
            });
        if confirm_key && self.is_text_editable(id) {
            self.commit_edit(id)?;
            return Ok(());
        }
        if self.mask_edit_key(id, key, modifiers, repeat)? {
            return Ok(());
        }
        if self.is_text_editable(id)
            && self
                .document(id)
                .is_some_and(|document| document.composition().is_none())
        {
            if *key == Key::Enter
                && !self.controls[&id].single_line
                && !modifiers.control
                && !modifiers.super_key
                && !modifiers.alt
                && self
                    .preflight_editor_insert(id, "\n", false, false)
                    .is_err()
            {
                return Ok(());
            }
            if shortcut
                && !repeat
                && let Key::Character(value) = key
            {
                let document = self.document(id).unwrap();
                let previous = match value.to_lowercase().as_str() {
                    "z" if modifiers.shift => document.redo_text(),
                    "z" => document.undo_text(),
                    "y" => document.redo_text(),
                    _ => None,
                }
                .map(str::to_owned);
                if let Some(text) = previous
                    && self.preflight_editor_text(id, &text).is_err()
                {
                    return Ok(());
                }
            }
        }
        let Some(Control {
            kind: Kind::TextBox(document),
            caret,
            caret_goal_x,
            caret_reveal,
            single_line,
            editor:
                EditorSettings {
                    is_read_only: read_only,
                    max_length,
                    ..
                },
            ..
        }) = self.controls.get_mut(&id)
        else {
            return Ok(());
        };
        if document.composition().is_some() {
            return Ok(());
        }
        *caret_reveal = true;
        // Ctrl+Alt is commonly AltGr, whose printable text arrives separately.
        if matches!(
            key,
            Key::ArrowUp | Key::ArrowDown | Key::PageUp | Key::PageDown
        ) && let Some(layout) = self.text_layouts.get(&id)
        {
            if caret.index != document.selection().focus {
                *caret = TextCaret {
                    index: document.selection().focus,
                    ..Default::default()
                };
            }
            let geometry = layout.caret_geometry(*caret);
            let origin_x = self.runtime.bounds(id).map_or(0.0, |bounds| bounds.x);
            let desired_x = origin_x + *caret_goal_x.get_or_insert(geometry.x - origin_x);
            let direction = if matches!(key, Key::ArrowUp | Key::PageUp) {
                -1.0
            } else {
                1.0
            };
            let distance = if matches!(key, Key::PageUp | Key::PageDown) {
                self.runtime
                    .bounds(id)
                    .map_or(geometry.height, |r| r.height)
            } else {
                geometry.height
            };
            *caret = layout.hit_test_caret(Point::new(
                desired_x,
                geometry.y + geometry.height * 0.5 + direction * distance,
            ));
            let anchor = if modifiers.shift {
                document.selection().anchor
            } else {
                caret.index
            };
            document
                .set_selection(anchor, caret.index)
                .map_err(|e| ControlError::Text(e.to_string()))?;
            self.scene_dirty = true;
            return Ok(());
        }
        *caret_goal_x = None;
        // Home/End follow the displayed wrapped line; Ctrl+Home/End retain
        // their document-wide meaning. The shaped geometry preserves bidi affinity.
        if !*single_line
            && !shortcut
            && matches!(key, Key::Home | Key::End)
            && let Some(layout) = self.text_layouts.get(&id)
        {
            if caret.index != document.selection().focus {
                *caret = TextCaret {
                    index: document.selection().focus,
                    ..Default::default()
                };
            }
            let geometry = layout.caret_geometry(*caret);
            let y = geometry.y + geometry.height * 0.5;
            if let Some(line) = layout
                .line_metrics()
                .into_iter()
                .find(|line| y >= line.bounds.y && y < line.bounds.y + line.bounds.height)
            {
                let x = if *key == Key::Home {
                    line.bounds.x - 1.0
                } else {
                    line.bounds.x + line.bounds.width + 1.0
                };
                *caret = layout.hit_test_caret(Point::new(x, y));
                let anchor = if modifiers.shift {
                    document.selection().anchor
                } else {
                    caret.index
                };
                document
                    .set_selection(anchor, caret.index)
                    .map_err(edit_error)?;
                self.scene_dirty = true;
                return Ok(());
            }
        }
        if matches!(key, Key::ArrowLeft | Key::ArrowRight)
            && !shortcut
            && (modifiers.shift || document.selection().anchor == document.selection().focus)
            && let Some(layout) = self.text_layouts.get(&id)
        {
            if caret.index != document.selection().focus {
                *caret = TextCaret {
                    index: document.selection().focus,
                    ..Default::default()
                };
            }
            *caret = layout.move_caret_visual(*caret, *key == Key::ArrowRight);
            let anchor = if modifiers.shift {
                document.selection().anchor
            } else {
                caret.index
            };
            document
                .set_selection(anchor, caret.index)
                .map_err(|e| ControlError::Text(e.to_string()))?;
            self.scene_dirty = true;
            return Ok(());
        }
        let before = document.text().to_owned();
        let selection_before = document.selection();
        let limits = editor_limits(*max_length);
        match key {
            Key::ArrowLeft if shortcut => document.move_word_left(modifiers.shift),
            Key::ArrowRight if shortcut => document.move_word_right(modifiers.shift),
            Key::ArrowLeft => document.move_left(modifiers.shift),
            Key::ArrowRight => document.move_right(modifiers.shift),
            Key::Home if shortcut => document.move_document_start(modifiers.shift),
            Key::End if shortcut => document.move_document_end(modifiers.shift),
            Key::Home => document.move_home(modifiers.shift),
            Key::End => document.move_end(modifiers.shift),
            Key::Backspace if shortcut && !*read_only => document.delete_word_backward(),
            Key::Delete if shortcut && !*read_only => document.delete_word_forward(),
            Key::Insert if modifiers.control && !modifiers.alt && !repeat => {
                self.clipboard
                    .push(ClipboardRequest::Copy(document.selected_text().to_owned()));
            }
            Key::Insert if modifiers.shift && !modifiers.alt && !repeat && !*read_only => {
                self.clipboard.push(ClipboardRequest::Paste);
            }
            Key::Delete if modifiers.shift && !modifiers.alt && !repeat && !*read_only => {
                self.clipboard
                    .push(ClipboardRequest::Copy(document.selected_text().to_owned()));
                document.delete_selection();
            }
            Key::Backspace if !*read_only => {
                document.backspace();
            }
            Key::Delete if !*read_only => {
                document.delete_forward();
            }
            Key::Enter
                if !*single_line
                    && !*read_only
                    && !modifiers.control
                    && !modifiers.super_key
                    && !modifiers.alt =>
            {
                if let Err(error) = document.try_insert("\n", limits) {
                    return user_edit(Err(error));
                }
            }
            Key::Character(value) if shortcut => match value.to_lowercase().as_str() {
                "a" => document.select_all(),
                "c" if !repeat => self
                    .clipboard
                    .push(ClipboardRequest::Copy(document.selected_text().to_owned())),
                "x" if !repeat && !*read_only => {
                    self.clipboard
                        .push(ClipboardRequest::Copy(document.selected_text().to_owned()));
                    document.insert("");
                }
                "v" if !repeat && !*read_only => self.clipboard.push(ClipboardRequest::Paste),
                "z" if !repeat && modifiers.shift && !*read_only => {
                    if let Err(error) = document.try_redo(limits) {
                        return user_edit(Err(error));
                    }
                }
                "z" if !repeat && !*read_only => {
                    if let Err(error) = document.try_undo(limits) {
                        return user_edit(Err(error));
                    }
                }
                "y" if !repeat && !*read_only => {
                    if let Err(error) = document.try_redo(limits) {
                        return user_edit(Err(error));
                    }
                }
                _ => {}
            },
            _ => {}
        }
        if document.selection() != selection_before || document.text() != before {
            *caret = TextCaret {
                index: document.selection().focus,
                ..Default::default()
            };
        }
        self.scene_dirty = true;
        self.record_text_change(id, &before);
        Ok(())
    }

    /// Current logical text offset. A multiline editor scrolls vertically;
    /// a single-line editor scrolls horizontally. This does not move selection.
    pub fn editor_scroll_offset(&self, id: WidgetId) -> Option<Point> {
        self.document(id)?;
        let control = self.controls.get(&id)?;
        Some(Point::new(control.text_scroll_x, control.text_scroll))
    }

    /// Scroll an arranged editor without moving its caret or changing its draft.
    /// A later editing/navigation action reveals the caret again. Offsets are
    /// clamped to the currently shaped document and unsupported axes stay zero.
    pub fn scroll_editor_to(&mut self, id: WidgetId, offset: Point) -> Result<(), ControlError> {
        if !offset.is_finite() {
            return Err(ControlError::Invalid("Editor scroll must be finite".into()));
        }
        self.editor_control(id)?;
        let viewport = self.editor_text_viewport(id).ok_or_else(|| {
            ControlError::Invalid("Editor must be visible and arranged before scrolling".into())
        })?;
        let layout = self
            .text_layouts
            .get(&id)
            .ok_or_else(|| ControlError::Invalid("Editor text has not been arranged".into()))?;
        let control = self.controls.get_mut(&id).unwrap();
        let (x, y) = if control.single_line {
            (
                offset
                    .x
                    .clamp(0.0, (layout.width - viewport.width).max(0.0)),
                0.0,
            )
        } else {
            (
                0.0,
                offset
                    .y
                    .clamp(0.0, (layout.height - viewport.height).max(0.0)),
            )
        };
        if control.text_scroll_x != x || control.text_scroll != y {
            control.text_scroll_x = x;
            control.text_scroll = y;
            control.caret_reveal = false;
            self.scene_dirty = true;
        }
        Ok(())
    }

    fn editor_text_viewport(&self, id: WidgetId) -> Option<Rect> {
        let control = self.controls.get(&id)?;
        self.document(id)?;
        let bounds = self.runtime.bounds(id)?;
        let style = control
            .edit_box_style
            .resolve(self.theme, self.edit_box_visual_state(id)?);
        Some(Rect::new(
            bounds.x + style.padding.left.min(bounds.width),
            bounds.y + style.padding.top.min(bounds.height),
            (bounds.width - style.padding.horizontal()).max(0.0),
            (bounds.height - style.padding.vertical()).max(0.0),
        ))
    }

    fn editor_scroll_bar(&self, id: WidgetId) -> Option<EditorScrollBar> {
        let control = self.controls.get(&id)?;
        if control.single_line {
            return None;
        }
        let viewport = self.editor_text_viewport(id)?;
        let total = self.text_layouts.get(&id)?.height;
        let maximum = (total - viewport.height).max(0.0);
        if maximum <= 0.0 || viewport.height < 12.0 {
            return None;
        }
        let bounds = self.runtime.bounds(id)?;
        let track = Rect::new(
            bounds.x + (bounds.width - 8.0).max(0.0),
            viewport.y,
            6.0_f32.min(bounds.width),
            viewport.height,
        );
        let height = (viewport.height * viewport.height / total).clamp(12.0, viewport.height);
        let y = track.y + (track.height - height) * (control.text_scroll / maximum).clamp(0.0, 1.0);
        Some(EditorScrollBar {
            track,
            thumb: Rect::new(track.x, y, track.width, height),
            maximum,
        })
    }

    pub(super) fn paint_editor_scroll_bar(&mut self, id: WidgetId) {
        let Some(bar) = self.editor_scroll_bar(id) else {
            return;
        };
        let Some(clip) = self.runtime.clip(id) else {
            return;
        };
        editor_fill(
            &mut self.scene,
            bar.track,
            clip,
            3.0,
            self.theme.chrome().subtle_hover,
        );
        editor_fill(
            &mut self.scene,
            bar.thumb,
            clip,
            3.0,
            self.theme.tokens().muted,
        );
    }

    pub(super) fn cancel_editor_scroll(&mut self) -> Result<(), ControlError> {
        if self.editor_scroll_drag.take().is_some() {
            self.runtime.apply(Mutation::CapturePointer(None))?;
            self.scene_dirty = true;
        }
        Ok(())
    }

    fn route_editor_scroll(&mut self, event: &InputEvent) -> Result<bool, ControlError> {
        let report = self.runtime.dispatch(event.clone());
        self.prune_removed_controls()?;
        if let Some(error) = report.errors.into_iter().next() {
            return Err(error.into());
        }
        self.synchronize_editor_focus()?;
        self.scene_dirty = true;
        Ok(!report.default_prevented)
    }

    fn drag_editor_scroll(
        &mut self,
        drag: EditorScrollDrag,
        position: Point,
    ) -> Result<(), ControlError> {
        let Some(bar) = self.editor_scroll_bar(drag.id) else {
            return self.cancel_editor_scroll();
        };
        let travel = bar.track.height - bar.thumb.height;
        let offset = if travel > 0.0 {
            (position.y - bar.track.y - drag.grab) / travel * bar.maximum
        } else {
            0.0
        };
        self.scroll_editor_to(drag.id, Point::new(0.0, offset))
    }

    pub(super) fn editor_scroll_before_input(
        &mut self,
        event: &InputEvent,
    ) -> Result<bool, ControlError> {
        if let Some(drag) = self.editor_scroll_drag {
            if !self.is_enabled(drag.id) || self.runtime.captured() != Some(drag.id) {
                self.cancel_editor_scroll()?;
            } else {
                match event {
                    InputEvent::PointerMoved { position } => {
                        if self.route_editor_scroll(event)? && self.is_enabled(drag.id) {
                            self.drag_editor_scroll(drag, *position)?;
                        }
                        return Ok(true);
                    }
                    InputEvent::PointerUp {
                        button: PointerButton::Primary,
                        ..
                    } => {
                        self.route_editor_scroll(event)?;
                        self.cancel_editor_scroll()?;
                        return Ok(true);
                    }
                    InputEvent::KeyDown {
                        key: Key::Escape, ..
                    }
                    | InputEvent::WindowFocused(false) => {
                        self.cancel_editor_scroll()?;
                    }
                    _ => {}
                }
            }
        }
        match event {
            InputEvent::Wheel {
                position, delta, ..
            } if delta.is_finite() => {
                let Some(id) = self
                    .runtime
                    .hit_test(*position)
                    .filter(|&id| self.is_enabled(id) && self.document(id).is_some())
                else {
                    return Ok(false);
                };
                let Some(bar) = self.editor_scroll_bar(id) else {
                    return Ok(false);
                };
                let previous = self.controls[&id].text_scroll;
                let next = (f64::from(previous) + f64::from(delta.y))
                    .clamp(0.0, f64::from(bar.maximum)) as f32;
                if next == previous {
                    return Ok(false);
                }
                if self.route_editor_scroll(event)? && self.is_enabled(id) {
                    self.scroll_editor_to(id, Point::new(0.0, next))?;
                }
                Ok(true)
            }
            InputEvent::PointerDown {
                position,
                button: PointerButton::Primary,
                ..
            } => {
                let Some(id) = self
                    .runtime
                    .hit_test(*position)
                    .filter(|&id| self.is_enabled(id) && self.document(id).is_some())
                else {
                    return Ok(false);
                };
                let Some(bar) = self
                    .editor_scroll_bar(id)
                    .filter(|bar| bar.track.contains(position.x, position.y))
                else {
                    return Ok(false);
                };
                if self.route_editor_scroll(event)? && self.is_enabled(id) {
                    let grab = if bar.thumb.contains(position.x, position.y) {
                        position.y - bar.thumb.y
                    } else {
                        bar.thumb.height / 2.0
                    };
                    let drag = EditorScrollDrag { id, grab };
                    self.selecting = None;
                    self.runtime.apply(Mutation::CapturePointer(Some(id)))?;
                    self.editor_scroll_drag = Some(drag);
                    self.drag_editor_scroll(drag, *position)?;
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
    use rust_desktop_ui_core::Modifiers;
    fn setup(text: &str, width: f32, height: f32) -> (Controls, WidgetId) {
        let mut ui = Controls::new(Theme::FluentLight).unwrap();
        let id = ui
            .text_box(
                ui.root(),
                "Document",
                text,
                LayoutStyle {
                    width: Length::Px(width),
                    height: Length::Px(height),
                    flex_shrink: 0.0,
                    ..Default::default()
                },
            )
            .unwrap();
        ui.runtime.apply(Mutation::Focus(Some(id))).unwrap();
        frame(&mut ui);
        (ui, id)
    }
    fn frame(ui: &mut Controls) {
        ui.update(Size::new(500.0, 400.0)).unwrap();
    }
    fn key(ui: &mut Controls, key: Key, modifiers: Modifiers) {
        ui.dispatch(InputEvent::KeyDown {
            key,
            modifiers,
            repeat: false,
        })
        .unwrap();
        frame(ui);
    }
    fn select(ui: &mut Controls, id: WidgetId, a: usize, f: usize) {
        if let Kind::TextBox(document) = &mut ui.controls.get_mut(&id).unwrap().kind {
            document.set_selection(a, f).unwrap();
        }
        ui.controls.get_mut(&id).unwrap().caret_reveal = true;
        ui.scene_dirty = true;
        frame(ui);
    }

    #[test]
    fn vertical_navigation_retains_desired_column_across_short_lines_and_extends_selection() {
        let (mut ui, id) = setup("abcdefghij\nx\nabcdefghij", 300.0, 130.0);
        select(&mut ui, id, 8, 8);
        let initial_x = ui.caret_bounds().unwrap().x;
        key(&mut ui, Key::ArrowDown, Modifiers::default());
        assert_eq!(ui.document(id).unwrap().selection().focus, 12);
        ui.edit_props(id, |props| props.style.margin.left = 40.0)
            .unwrap();
        frame(&mut ui);
        key(
            &mut ui,
            Key::ArrowDown,
            Modifiers {
                shift: true,
                ..Default::default()
            },
        );
        assert_eq!(ui.document(id).unwrap().selection().anchor, 12);
        assert_eq!(ui.document(id).unwrap().selection().focus, 21);
        assert!((ui.caret_bounds().unwrap().x - initial_x - 40.0).abs() < 0.1);
        key(&mut ui, Key::ArrowLeft, Modifiers::default());
        assert!(ui.controls[&id].caret_goal_x.is_none());
    }

    #[test]
    fn home_end_follow_wrapped_lines_while_control_moves_to_document_edges() {
        let (mut ui, id) = setup(
            "one two three four five six seven eight nine ten",
            130.0,
            200.0,
        );
        let lines = ui.text_layouts[&id].line_metrics();
        assert!(lines.len() > 2);
        let line = lines[1].clone();
        let middle = ui.text_layouts[&id].hit_test(Point::new(
            line.bounds.x + line.bounds.width / 2.0,
            line.bounds.y + line.bounds.height / 2.0,
        ));
        select(&mut ui, id, middle, middle);
        key(&mut ui, Key::Home, Modifiers::default());
        assert_eq!(
            ui.document(id).unwrap().selection().focus,
            line.text_range.start
        );
        key(&mut ui, Key::End, Modifiers::default());
        assert!(ui.document(id).unwrap().selection().focus >= middle);
        assert!(ui.document(id).unwrap().selection().focus <= line.text_range.end);
        key(
            &mut ui,
            Key::End,
            Modifiers {
                control: true,
                ..Default::default()
            },
        );
        assert_eq!(
            ui.document(id).unwrap().selection().focus,
            ui.document(id).unwrap().text().len()
        );
        key(
            &mut ui,
            Key::Home,
            Modifiers {
                control: true,
                ..Default::default()
            },
        );
        assert_eq!(ui.document(id).unwrap().selection().focus, 0);
    }

    #[test]
    fn manual_wheel_and_scrollbar_preserve_caret_draft_and_survive_repaints_until_navigation() {
        let text = (0..40).map(|i| format!("Line {i}\n")).collect::<String>();
        let (mut ui, id) = setup(&text, 250.0, 100.0);
        select(&mut ui, id, 0, 0);
        let selection = ui.document(id).unwrap().selection();
        ui.dispatch(InputEvent::Wheel {
            position: Point::new(50.0, 40.0),
            delta: Point::new(0.0, 120.0),
            modifiers: Default::default(),
        })
        .unwrap();
        frame(&mut ui);
        assert_eq!(ui.editor_scroll_offset(id).unwrap().y, 120.0);
        ui.scene_dirty = true;
        frame(&mut ui);
        assert_eq!(ui.editor_scroll_offset(id).unwrap().y, 120.0);
        assert_eq!(ui.document(id).unwrap().selection(), selection);
        assert_eq!(ui.accepted_value(id), Some(text.as_str()));
        let bar = ui.editor_scroll_bar(id).unwrap();
        let down = Point::new(bar.thumb.x + 2.0, bar.thumb.y + bar.thumb.height / 2.0);
        ui.dispatch(InputEvent::PointerDown {
            position: down,
            button: PointerButton::Primary,
            modifiers: Default::default(),
        })
        .unwrap();
        assert_eq!(ui.runtime.captured(), Some(id));
        ui.dispatch(InputEvent::PointerMoved {
            position: Point::new(down.x, down.y + 20.0),
        })
        .unwrap();
        frame(&mut ui);
        assert!(ui.editor_scroll_offset(id).unwrap().y > 120.0);
        assert_eq!(ui.document(id).unwrap().selection(), selection);
        key(&mut ui, Key::Escape, Modifiers::default());
        assert!(ui.runtime.captured().is_none());
        key(
            &mut ui,
            Key::Home,
            Modifiers {
                control: true,
                ..Default::default()
            },
        );
        assert_eq!(ui.editor_scroll_offset(id).unwrap().y, 0.0);
        assert!(ui.scroll_editor_to(id, Point::new(0.0, f32::NAN)).is_err());
    }

    #[test]
    fn alternate_clipboard_shortcuts_and_word_delete_respect_readonly_and_graphemes() {
        let (mut ui, id) = setup("e\u{301} one 👩‍💻", 300.0, 100.0);
        select(&mut ui, id, 0, 3);
        key(
            &mut ui,
            Key::Insert,
            Modifiers {
                control: true,
                ..Default::default()
            },
        );
        assert_eq!(
            ui.drain_clipboard_requests(),
            vec![ClipboardRequest::Copy("e\u{301}".into())]
        );
        key(
            &mut ui,
            Key::Delete,
            Modifiers {
                shift: true,
                ..Default::default()
            },
        );
        assert_eq!(
            ui.drain_clipboard_requests(),
            vec![ClipboardRequest::Copy("e\u{301}".into())]
        );
        assert_eq!(ui.document(id).unwrap().text(), " one 👩‍💻");
        key(
            &mut ui,
            Key::Insert,
            Modifiers {
                shift: true,
                ..Default::default()
            },
        );
        assert_eq!(ui.drain_clipboard_requests(), vec![ClipboardRequest::Paste]);
        key(
            &mut ui,
            Key::Character("z".into()),
            Modifiers {
                control: true,
                ..Default::default()
            },
        );
        assert_eq!(ui.document(id).unwrap().selected_text(), "e\u{301}");
        ui.set_read_only(id, true).unwrap();
        key(
            &mut ui,
            Key::Delete,
            Modifiers {
                shift: true,
                ..Default::default()
            },
        );
        key(
            &mut ui,
            Key::Backspace,
            Modifiers {
                control: true,
                ..Default::default()
            },
        );
        key(
            &mut ui,
            Key::Insert,
            Modifiers {
                shift: true,
                ..Default::default()
            },
        );
        assert!(ui.drain_clipboard_requests().is_empty());
        assert_eq!(ui.document(id).unwrap().text(), "e\u{301} one 👩‍💻");
    }
}
