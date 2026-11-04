//! One retained TextBox moves between the formula bar and the active cell.
use super::*;

impl Spreadsheet {
    fn mirror_visibility(&self, ui: &Controls, visible: bool) -> Result<Mutation, ControlError> {
        let mut props = ui
            .runtime
            .tree()
            .node(self.formula_mirror)
            .ok_or_else(|| err("Formula mirror removed"))?
            .props
            .clone();
        props.visible = visible;
        Ok(Mutation::SetProps {
            node: self.formula_mirror,
            props,
        })
    }
    fn cell_editor_style(&self, ui: &Controls) -> Result<LayoutStyle, ControlError> {
        let bounds = ui
            .runtime
            .bounds(self.parts.canvas)
            .ok_or_else(|| err("Worksheet has not been arranged"))?;
        let geometry = self.geometry(bounds)?;
        let active = self.session.active();
        let range = self.session.workbook().sheets()[self.session.sheet()]
            .merges()
            .iter()
            .find(|r| r.contains(active))
            .copied()
            .unwrap_or(CellRange::new(active, active).map_err(err)?);
        let rect = geometry
            .range(range)
            .intersection(geometry.body)
            .ok_or_else(|| err("Active cell is outside the worksheet viewport"))?;
        let mut style = self.formula_home.1.clone();
        style.grid_column = GridPlacement::default();
        style.grid_row = GridPlacement::default();
        style.width = Length::Px(
            rect.width
                .max(96.0)
                .min(geometry.body.x + geometry.body.width - rect.x),
        );
        style.height = Length::Px(
            rect.height
                .max(32.0)
                .min(geometry.body.y + geometry.body.height - rect.y),
        );
        style.offset = Point::new(rect.x - bounds.x, rect.y - bounds.y);
        style.margin = Edges::default();
        style.min_size = Size::ZERO;
        Ok(style)
    }
    /// Focus the active cell without creating a second draft, undo stack or IME target.
    pub fn begin_cell_edit(&mut self, ui: &mut Controls) -> Result<(), ControlError> {
        self.cancel_interaction(ui)?;
        self.guard_composition(ui)?;
        let bounds = ui
            .runtime
            .bounds(self.parts.canvas)
            .ok_or_else(|| err("Worksheet has not been arranged"))?;
        (self.first_row, self.first_column) = self.geometry(bounds)?.reveal(self.session.active());
        let style = self.cell_editor_style(ui)?;
        ui.runtime.apply_batch(&[
            Mutation::Reparent {
                node: self.parts.formula,
                parent: self.viewport,
            },
            Mutation::SetStyle {
                node: self.parts.formula,
                style,
            },
            self.mirror_visibility(ui, true)?,
            Mutation::Focus(Some(self.parts.formula)),
        ])?;
        self.cell_editing = true;
        Ok(())
    }
    /// Return the same draft to the formula bar, preserving its text selection.
    pub fn edit_formula(&mut self, ui: &mut Controls) -> Result<(), ControlError> {
        self.guard_composition(ui)?;
        self.return_editor(ui)?;
        ui.runtime
            .apply(Mutation::Focus(Some(self.parts.formula)))?;
        Ok(())
    }
    pub(super) fn return_editor(&mut self, ui: &mut Controls) -> Result<(), ControlError> {
        if self.cell_editing {
            ui.runtime.apply_batch(&[
                Mutation::Reparent {
                    node: self.parts.formula,
                    parent: self.formula_home.0,
                },
                Mutation::SetStyle {
                    node: self.parts.formula,
                    style: self.formula_home.1.clone(),
                },
                self.mirror_visibility(ui, false)?,
            ])?;
            self.cell_editing = false;
        }
        Ok(())
    }
    pub(super) fn position_editor(&mut self, ui: &mut Controls) -> Result<(), ControlError> {
        if self.cell_editing {
            if let Some(bounds) = ui.runtime.bounds(self.parts.canvas) {
                let geometry = self.geometry(bounds)?;
                if geometry.body.width <= 0.0 || geometry.body.height <= 0.0 {
                    // Retain the draft, selection and IME target when the host
                    // temporarily has no space for an in-cell overlay.
                    return self.return_editor(ui);
                }
                (self.first_row, self.first_column) = geometry.reveal(self.session.active());
            }
            let style = self.cell_editor_style(ui)?;
            if ui
                .runtime
                .tree()
                .node(self.parts.formula)
                .is_some_and(|n| n.props.style != style)
            {
                ui.runtime.apply(Mutation::SetStyle {
                    node: self.parts.formula,
                    style,
                })?;
            }
            let value = ui
                .document(self.parts.formula)
                .map_or("", |doc| doc.text())
                .to_owned();
            ui.set_label(
                self.formula_mirror,
                if value.is_empty() { " ".into() } else { value },
            )?;
        }
        Ok(())
    }
    /// Route committed host text before Controls::text_input. Typing on the
    /// worksheet begins a replacement draft; native IME uses the focused TextBox.
    pub fn text_input(&mut self, ui: &mut Controls, text: &str) -> Result<bool, ControlError> {
        if ui.runtime.focused() != Some(self.parts.canvas) || text.is_empty() {
            return Ok(false);
        }
        self.begin_cell_edit(ui)?;
        ui.set_text(self.parts.formula, "")?;
        ui.text_input(text)?;
        Ok(true)
    }
}
