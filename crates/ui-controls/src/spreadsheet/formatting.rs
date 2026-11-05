//! Explicit preset selection over the same workbook style and undo history.
use super::*;

pub(super) fn presets() -> Vec<crate::SelectionItem> {
    [
        ("General", "Общий"),
        ("0", "Целое"),
        ("#,##0.00", "Число · два знака"),
        ("0.00%", "Проценты"),
        ("0.00E+00", "Научная запись"),
        ("##0.00E+00", "Инженерная запись"),
        ("dd.mm.yyyy", "Дата"),
        ("dddd, d mmmm yyyy", "Дата · полностью"),
        ("hh:mm", "Время"),
        ("hh:mm:ss", "Время · с секундами"),
        ("h:mm AM/PM", "Время · 12 часов"),
        ("[h]:mm:ss", "Длительность · часы"),
        ("[m]:ss", "Длительность · минуты"),
        ("#,##0.00 \"₽\"", "Сумма · рубли"),
    ]
    .into_iter()
    .map(|(format, title)| crate::SelectionItem::new(format, title))
    .collect()
}

impl Spreadsheet {
    pub(super) fn load_format(&self, ui: &mut Controls) -> Result<(), ControlError> {
        Self::load_session_format(ui, self.parts.format, self.parts.format_code, &self.session)
    }
    fn load_session_format(
        ui: &mut Controls,
        id: WidgetId,
        code: WidgetId,
        session: &SpreadsheetSession,
    ) -> Result<(), ControlError> {
        let current = session.workbook().sheets()[session.sheet()]
            .cell(session.active())
            .map_or("", |cell| cell.style.number_format.as_str());
        let current = if current.is_empty() || current.eq_ignore_ascii_case("General") {
            "General"
        } else {
            current
        };
        let mut items = presets();
        if !items.iter().any(|item| item.key == current) {
            let caption: String = current
                .chars()
                .map(|c| if c.is_control() { ' ' } else { c })
                .collect();
            items.push(crate::SelectionItem::new(
                current,
                format!("Пользовательский: {caption}"),
            ));
        }
        ui.set_selection_items(id, items)?;
        ui.set_selected_keys(id, &[current.into()])?;
        ui.set_editor_value(code, current)?;
        Ok(())
    }

    /// Apply the format-code draft explicitly. Enter is equivalent to the code
    /// Apply button; neither typing nor leaving this editor changes the workbook.
    pub fn apply_format_code(&mut self, ui: &mut Controls) -> Result<(), ControlError> {
        self.guard_composition(ui)?;
        let code = ui
            .document(self.parts.format_code)
            .ok_or_else(|| err("Format code editor removed"))?
            .text()
            .to_owned();
        self.set_number_format(ui, &code)
    }

    /// Commit a valid current draft, then apply display metadata to the selected
    /// range. A rejected candidate preserves the live draft and workbook/history.
    /// The draft and formatting remain separate undo steps when both changed.
    pub fn set_number_format(
        &mut self,
        ui: &mut Controls,
        format: &str,
    ) -> Result<(), ControlError> {
        self.guard_composition(ui)?;
        let text = ui
            .document(self.parts.formula)
            .ok_or_else(|| err("Formula editor removed"))?
            .text()
            .to_owned();
        let mut next = self.session.clone();
        if text != self.loaded {
            next.enter(&text).map_err(err)?;
        }
        next.set_number_format(format).map_err(err)?;
        self.cancel_interaction(ui)?;
        self.return_editor(ui)?;
        Self::load_session_format(ui, self.parts.format, self.parts.format_code, &next)?;
        self.session = next;
        self.loaded = text;
        self.refresh(ui)
    }
}
