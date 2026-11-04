//! Bounded, platform-neutral accessibility snapshot of the visible worksheet.
use super::*;

#[derive(Clone, Debug)]
pub struct SpreadsheetCell {
    pub address: CellAddress,
    pub range: CellRange,
    pub bounds: Rect,
    pub value: String,
    pub format_diagnostic: Option<String>,
    pub selected: bool,
}

#[derive(Clone, Debug)]
pub struct SpreadsheetHeader {
    pub index: u32,
    pub label: String,
    pub bounds: Rect,
}

#[derive(Clone, Debug, Default)]
pub struct SpreadsheetSemantics {
    /// Logical limits; only materialized cells are included in `cells`.
    pub row_count: u32,
    pub column_count: u32,
    pub rows: Vec<SpreadsheetHeader>,
    pub columns: Vec<SpreadsheetHeader>,
    pub cells: Vec<SpreadsheetCell>,
}

impl Spreadsheet {
    /// Whether accessibility can reveal cells without replacing a formula draft.
    pub fn can_realize_accessible_cell(&self, ui: &Controls) -> bool {
        self.drag.is_none()
            && !self.cell_editing
            && ui.is_enabled(self.parts.canvas)
            && ui.semantics().iter().any(|n| n.id == self.parts.canvas)
            && ui
                .document(self.parts.formula)
                .is_some_and(|doc| doc.composition().is_none() && doc.text() == self.loaded)
    }

    /// Reveal a logical cell while preserving selection, focus and the formula
    /// draft. Draft text and IME composition prevent viewport movement.
    pub fn realize_accessible_cell(
        &mut self,
        ui: &mut Controls,
        address: CellAddress,
    ) -> Result<bool, ControlError> {
        if !self.can_realize_accessible_cell(ui) {
            return Ok(false);
        }
        let address = self.session.workbook().sheets()[self.session.sheet()].anchor(address);
        self.first_row = address.row();
        self.first_column = address.column();
        self.refresh(ui)?;
        Ok(true)
    }

    /// Produce the same visible merged geometry used by the renderer. This does
    /// not focus, scroll, commit the formula draft, or allocate an entire sheet.
    pub fn semantics(&self, ui: &Controls) -> Result<SpreadsheetSemantics, ControlError> {
        let mut result = SpreadsheetSemantics {
            row_count: MAX_ROWS,
            column_count: MAX_COLUMNS,
            ..Default::default()
        };
        let Some(bounds) = ui.runtime.bounds(self.parts.canvas).filter(|_| {
            ui.semantics()
                .iter()
                .any(|node| node.id == self.parts.canvas)
        }) else {
            return Ok(result);
        };
        let geometry = self.geometry(bounds)?;
        let body = geometry.body;
        for index in geometry.visible_columns.clone() {
            let rect = geometry.column_header(index);
            if let Some(bounds) = rect.intersection(bounds) {
                result.columns.push(SpreadsheetHeader {
                    index,
                    label: CellAddress::new(0, index)
                        .map_err(err)?
                        .a1()
                        .trim_end_matches('1')
                        .into(),
                    bounds,
                });
            }
        }
        let sheet = &self.session.workbook().sheets()[self.session.sheet()];
        let mut seen = BTreeSet::new();
        for index in geometry.visible_rows.clone() {
            let rect = geometry.row_header(index);
            if let Some(bounds) = rect.intersection(bounds) {
                result.rows.push(SpreadsheetHeader {
                    index,
                    label: (index + 1).to_string(),
                    bounds,
                });
            }
            for column in geometry.visible_columns.clone() {
                let address = CellAddress::new(index, column).map_err(err)?;
                let anchor = sheet.anchor(address);
                if !seen.insert(anchor) {
                    continue;
                }
                let range = sheet
                    .merges()
                    .iter()
                    .find(|r| r.contains(address))
                    .copied()
                    .unwrap_or(CellRange::new(address, address).map_err(err)?);
                let rect = geometry.range(range);
                if let Some(bounds) = rect.intersection(body) {
                    let display = format_cell(
                        self.session.workbook(),
                        self.session.sheet(),
                        anchor,
                        self.culture(),
                    )?;
                    result.cells.push(SpreadsheetCell {
                        address: anchor,
                        range,
                        bounds,
                        value: display.text,
                        format_diagnostic: display.diagnostic,
                        selected: self.session.selection().intersects(range),
                    });
                }
            }
        }
        Ok(result)
    }

    /// Select a materialized accessible cell. A pending formula draft or IME
    /// composition is preserved; accessibility navigation cannot commit it.
    pub fn select_accessible_cell(
        &mut self,
        ui: &mut Controls,
        address: CellAddress,
    ) -> Result<bool, ControlError> {
        if self.drag.is_some()
            || !ui.is_enabled(self.parts.canvas)
            || !ui
                .semantics()
                .iter()
                .any(|node| node.id == self.parts.canvas)
        {
            return Ok(false);
        }
        let Some(document) = ui.document(self.parts.formula) else {
            return Ok(false);
        };
        if document.composition().is_some() || document.text() != self.loaded {
            return Ok(false);
        }
        if !self
            .semantics(ui)?
            .cells
            .iter()
            .any(|cell| cell.address == address)
        {
            return Ok(false);
        }
        self.session.select(address, address).map_err(err)?;
        self.anchor = address;
        self.load_entry(ui)?;
        ui.runtime.apply(Mutation::Focus(Some(self.parts.canvas)))?;
        Ok(true)
    }
}
