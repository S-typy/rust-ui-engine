//! Shape-budget checks before committed text or composition changes.
use super::*;

impl Controls {
    pub(super) fn preflight_new_editor_text(
        &mut self,
        text: &str,
        layout: &LayoutStyle,
        single_line: bool,
    ) -> Result<(), ControlError> {
        let resolved = EditBoxStyle::default().resolve(self.theme, EditBoxVisualState::Normal);
        let width = match layout.width {
            Length::Px(value) if value.is_finite() => value,
            _ => self.viewport.width.max(320.0),
        };
        let bounds = Rect::new(
            0.0,
            0.0,
            (width - resolved.padding.horizontal()).max(1.0),
            100.0,
        );
        self.engine
            .validate_run(
                &TextRun {
                    text: text.to_owned(),
                    bounds,
                    clip: bounds,
                    font_size: resolved.font_size,
                    color: resolved.foreground,
                    weight: 400,
                    align: TextAlign::Start,
                    wrap: !single_line,
                    sensitive: false,
                },
                1.0,
            )
            .map_err(|error| ControlError::Text(error.to_string()))
    }

    pub(super) fn preflight_editor_text(
        &mut self,
        id: WidgetId,
        text: &str,
    ) -> Result<(), ControlError> {
        self.preflight_search_text(id, text)?;
        self.preflight_breadcrumb_text(id, text)?;
        let style = *self
            .edit_box_style(id)
            .ok_or_else(|| ControlError::Invalid("Control is not a TextBox".into()))?;
        self.preflight_editor_style_text(id, text, style)
    }

    pub(super) fn preflight_editor_style_text(
        &mut self,
        id: WidgetId,
        text: &str,
        style: EditBoxStyle,
    ) -> Result<(), ControlError> {
        let control = self.editor_control(id)?;
        let resolved = style.resolve(self.theme, self.edit_box_visual_state(id).unwrap());
        let width = self
            .runtime
            .bounds(id)
            .map(|bounds| bounds.width)
            .filter(|value| *value > 0.0)
            .unwrap_or_else(
                || match self.runtime.tree().node(id).unwrap().props.style.width {
                    Length::Px(value) if value.is_finite() => value,
                    _ => self.viewport.width.max(320.0),
                },
            );
        let bounds = Rect::new(
            0.0,
            0.0,
            (width - resolved.padding.horizontal()).max(1.0),
            100.0,
        );
        let run = TextRun {
            text: text.to_owned(),
            bounds,
            clip: bounds,
            font_size: resolved.font_size,
            color: resolved.foreground,
            weight: 400,
            align: TextAlign::Start,
            wrap: !control.single_line,
            sensitive: false,
        };
        self.engine
            .validate_run(&run, 1.0)
            .map_err(|error| ControlError::Text(error.to_string()))
    }

    /// Compose only the new string, never clone undo history. IME display is
    /// allowed to exceed MaxLength, but not byte or shaping budgets.
    pub(super) fn preflight_editor_insert(
        &mut self,
        id: WidgetId,
        text: &str,
        ime: bool,
        preedit: bool,
    ) -> Result<(), ControlError> {
        let control = self.editor_control(id)?;
        let Kind::TextBox(document) = &control.kind else {
            unreachable!()
        };
        let normalized = if preedit {
            Cow::Borrowed(text)
        } else {
            normalize_editor_text(text, control.single_line).map_err(edit_error)?
        };
        if normalized.is_empty() {
            return Ok(());
        }
        let range = if ime {
            document.composition().map_or_else(
                || document.selection().range(),
                |composition| composition.replacement.clone(),
            )
        } else {
            document.selection().range()
        };
        let size = document.text().len() - range.len() + normalized.len();
        if size > TextLimits::default().max_bytes {
            return Err(edit_error(EditError::LimitExceeded {
                kind: TextLimit::Bytes,
                maximum: TextLimits::default().max_bytes,
            }));
        }
        let mut candidate = String::with_capacity(size);
        candidate.push_str(&document.text()[..range.start]);
        candidate.push_str(&normalized);
        candidate.push_str(&document.text()[range.end..]);
        if !preedit {
            editor_limits(control.editor.max_length)
                .validate(&candidate)
                .map_err(edit_error)?;
        }
        self.preflight_editor_text(id, &candidate)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shaped_budget_rejects_paste_preedit_commit_and_setters_without_losing_draft() {
        let mut ui = Controls::new(Theme::FluentLight).unwrap();
        let id = ui
            .edit_box(ui.root(), "Name", "accepted", LayoutStyle::default())
            .unwrap();
        ui.runtime.apply(Mutation::Focus(Some(id))).unwrap();
        ui.update(Size::new(500.0, 400.0)).unwrap();
        ui.text_input(" draft").unwrap();
        let before = ui.document(id).unwrap().text().to_owned();
        let selection = ui.document(id).unwrap().selection();
        let undo = ui.document(id).unwrap().can_undo();
        ui.drain_text_changes();
        ui.drain_edit_events();
        let oversized = "a".repeat(65_537);
        ui.text_input(&oversized).unwrap();
        ui.ime_preedit("候補", Some((0, 6))).unwrap();
        let composition = ui.document(id).unwrap().composition().cloned();
        ui.ime_preedit(&oversized, None).unwrap();
        assert_eq!(ui.document(id).unwrap().composition(), composition.as_ref());
        ui.ime_commit(&oversized).unwrap();
        assert!(ui.document(id).unwrap().composition().is_none());
        assert!(ui.set_text(id, &oversized).is_err());
        assert!(ui.set_editor_value(id, &oversized).is_err());
        ui.perform_action(id, SemanticAction::SetValue, Some(&oversized))
            .unwrap();
        assert_eq!(ui.document(id).unwrap().text(), before);
        assert_eq!(ui.document(id).unwrap().selection(), selection);
        assert_eq!(ui.document(id).unwrap().can_undo(), undo);
        assert_eq!(ui.accepted_value(id), Some("accepted"));
        assert!(ui.drain_text_changes().is_empty());
        assert!(ui.drain_edit_events().is_empty());
        ui.update(Size::new(500.0, 400.0)).unwrap();
        let children = ui.runtime.tree().children(ui.root()).unwrap().to_vec();
        assert!(
            ui.text_box(ui.root(), "Too long", &oversized, LayoutStyle::default())
                .is_err()
        );
        assert_eq!(ui.runtime.tree().children(ui.root()).unwrap(), children);
    }
}
