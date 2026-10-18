use super::*;

impl Controls {
    /// Restore a grapheme-safe selection without changing text, history, focus,
    /// or a pending composition. Used by composite document editors.
    pub fn set_editor_selection(
        &mut self,
        id: WidgetId,
        anchor: usize,
        focus: usize,
    ) -> Result<(), ControlError> {
        let Some(control) = self.controls.get_mut(&id) else {
            return Err(ControlError::Invalid("Unknown editor".into()));
        };
        let Kind::TextBox(document) = &mut control.kind else {
            return Err(ControlError::Invalid("Control is not a TextBox".into()));
        };
        if document.composition().is_some() {
            return Err(ControlError::Invalid("Composition is active".into()));
        }
        document.set_selection(anchor, focus).map_err(edit_error)?;
        control.caret = TextCaret {
            index: focus,
            ..Default::default()
        };
        control.caret_goal_x = None;
        control.caret_reveal = true;
        self.caret_visible = true;
        self.scene_dirty = true;
        Ok(())
    }
}
