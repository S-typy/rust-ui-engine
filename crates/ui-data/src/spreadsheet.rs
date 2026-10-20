//! Transactional spreadsheet interaction, independent of windowing and clipboard I/O.
use crate::workbook::*;
use crate::{CellValue, DataError};

fn invalid(s: &str) -> DataError {
    DataError::Validation(s.into())
}

#[derive(Clone, Debug)]
struct Snapshot {
    workbook: Workbook,
    sheet: usize,
    selection: CellRange,
    active: CellAddress,
}

/// An immutable typed range payload. External text never acquires formula semantics
/// unless the application explicitly asks to interpret formulas on paste.
#[derive(Clone, Debug)]
pub struct SheetClipboard {
    origin: CellAddress,
    rows: u32,
    columns: u32,
    cells: Vec<(CellAddress, WorkbookCell)>,
    merges: Vec<CellRange>,
}

#[derive(Clone, Debug)]
pub struct SpreadsheetSession {
    current: Snapshot,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    generation: u64,
}
impl SpreadsheetSession {
    pub fn new(workbook: Workbook) -> Self {
        let origin = CellAddress::new(0, 0).expect("valid origin");
        Self {
            current: Snapshot {
                workbook,
                sheet: 0,
                active: origin,
                selection: CellRange::new(origin, origin).expect("valid selection"),
            },
            undo: Vec::new(),
            redo: Vec::new(),
            generation: 0,
        }
    }
    pub fn workbook(&self) -> &Workbook {
        &self.current.workbook
    }
    pub fn sheet(&self) -> usize {
        self.current.sheet
    }
    pub fn active(&self) -> CellAddress {
        self.current.active
    }
    pub fn selection(&self) -> CellRange {
        self.current.selection
    }
    /// Monotonic across edits, undo, redo and sheet changes; protects async results.
    pub fn generation(&self) -> u64 {
        self.generation
    }
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
    pub fn select_sheet(&mut self, sheet: usize) -> Result<(), DataError> {
        if sheet >= self.workbook().sheets().len() {
            return Err(invalid("unknown worksheet"));
        }
        let previous = self.current.sheet;
        self.current.sheet = sheet;
        let result = self.select(CellAddress::new(0, 0)?, CellAddress::new(0, 0)?);
        if result.is_err() {
            self.current.sheet = previous;
        }
        result
    }
    /// Expand the selection to include every merged cell it intersects.
    pub fn select(&mut self, anchor: CellAddress, focus: CellAddress) -> Result<(), DataError> {
        let sheet = &self.workbook().sheets()[self.sheet()];
        let mut range = CellRange::new(
            CellAddress::new(
                anchor.row().min(focus.row()),
                anchor.column().min(focus.column()),
            )?,
            CellAddress::new(
                anchor.row().max(focus.row()),
                anchor.column().max(focus.column()),
            )?,
        )?;
        loop {
            let previous = range;
            for merge in sheet.merges().iter().filter(|r| r.intersects(previous)) {
                range = CellRange::new(
                    CellAddress::new(
                        range.start().row().min(merge.start().row()),
                        range.start().column().min(merge.start().column()),
                    )?,
                    CellAddress::new(
                        range.end().row().max(merge.end().row()),
                        range.end().column().max(merge.end().column()),
                    )?,
                )?;
            }
            if previous == range {
                break;
            }
        }
        let active = sheet.anchor(focus);
        self.bump()?;
        self.current.active = active;
        self.current.selection = range;
        Ok(())
    }
    pub fn entry(&self) -> String {
        self.workbook().sheets()[self.sheet()]
            .cell(self.active())
            .map_or(String::new(), |c| match &c.input {
                CellInput::Formula(s) => s.clone(),
                CellInput::Value(v) => v.display(),
                CellInput::Error(e) => e.display().into(),
            })
    }
    /// Parse an explicit cell/formula edit; leading apostrophe forces text.
    pub fn enter(&mut self, text: &str) -> Result<(), DataError> {
        let mut cell = self.workbook().sheets()[self.sheet()]
            .cell(self.active())
            .cloned()
            .unwrap_or_else(|| WorkbookCell::value(CellValue::Null));
        cell.input = parse_entry(text, true)?;
        self.apply(&[WorkbookEdit::Set {
            sheet: self.sheet(),
            address: self.active(),
            cell,
        }])
    }
    pub fn apply(&mut self, edits: &[WorkbookEdit]) -> Result<(), DataError> {
        if edits.is_empty() {
            return Ok(());
        }
        let mut next = self.current.clone();
        next.workbook.apply(edits)?;
        self.bump()?;
        self.undo.push(std::mem::replace(&mut self.current, next));
        self.redo.clear();
        self.trim_history();
        Ok(())
    }
    pub fn clear(&mut self) -> Result<(), DataError> {
        self.apply(&[WorkbookEdit::Clear {
            sheet: self.sheet(),
            range: self.selection(),
        }])
    }
    pub fn merge(&mut self) -> Result<(), DataError> {
        self.apply(&[WorkbookEdit::Merge {
            sheet: self.sheet(),
            range: self.selection(),
        }])?;
        self.current.active = self.selection().start();
        Ok(())
    }
    pub fn unmerge(&mut self) -> Result<(), DataError> {
        self.apply(&[WorkbookEdit::Unmerge {
            sheet: self.sheet(),
            range: self.selection(),
        }])
    }
    pub fn undo(&mut self) -> Result<bool, DataError> {
        if self.undo.is_empty() {
            return Ok(false);
        }
        self.bump()?;
        let old = self.undo.pop().expect("history checked");
        self.redo.push(std::mem::replace(&mut self.current, old));
        Ok(true)
    }
    pub fn redo(&mut self) -> Result<bool, DataError> {
        if self.redo.is_empty() {
            return Ok(false);
        }
        self.bump()?;
        let next = self.redo.pop().expect("history checked");
        self.undo.push(std::mem::replace(&mut self.current, next));
        Ok(true)
    }
    fn bump(&mut self) -> Result<(), DataError> {
        self.generation = self
            .generation
            .checked_add(1)
            .ok_or(DataError::RevisionOverflow)?;
        Ok(())
    }
    fn trim_history(&mut self) {
        while self.undo.len() > 64
            || self
                .undo
                .iter()
                .map(|s| s.workbook.estimated_bytes())
                .sum::<usize>()
                > 64 * 1024 * 1024
        {
            self.undo.remove(0);
        }
    }
    pub fn copy(&self) -> SheetClipboard {
        let range = self.selection();
        let sheet = &self.workbook().sheets()[self.sheet()];
        SheetClipboard {
            origin: range.start(),
            rows: range.end().row() - range.start().row() + 1,
            columns: range.end().column() - range.start().column() + 1,
            cells: sheet
                .cells()
                .iter()
                .filter(|(a, _)| range.contains(**a))
                .map(|(a, c)| (*a, c.clone()))
                .collect(),
            merges: sheet
                .merges()
                .iter()
                .copied()
                .filter(|r| range.contains_range(*r))
                .collect(),
        }
    }
    pub fn paste(
        &mut self,
        payload: &SheetClipboard,
        destination: CellAddress,
    ) -> Result<(), DataError> {
        let end = CellAddress::new(
            destination.row() + payload.rows - 1,
            destination.column() + payload.columns - 1,
        )?;
        let range = CellRange::new(destination, end)?;
        let row = i64::from(destination.row()) - i64::from(payload.origin.row());
        let col = i64::from(destination.column()) - i64::from(payload.origin.column());
        let translate = |a: CellAddress| {
            CellAddress::new(
                (i64::from(a.row()) + row) as u32,
                (i64::from(a.column()) + col) as u32,
            )
        };
        let sheet = self.sheet();
        let mut edits = vec![
            WorkbookEdit::Unmerge { sheet, range },
            WorkbookEdit::Clear { sheet, range },
        ];
        for (a, c) in &payload.cells {
            edits.push(WorkbookEdit::Set {
                sheet,
                address: translate(*a)?,
                cell: c.translated(row, col)?,
            });
        }
        for r in &payload.merges {
            edits.push(WorkbookEdit::Merge {
                sheet,
                range: CellRange::new(translate(r.start())?, translate(r.end())?)?,
            });
        }
        self.apply(&edits)?;
        self.current.active = destination;
        self.current.selection = range;
        Ok(())
    }
    /// Repeat the top row down or left column right, translating formula references.
    /// Merged ranges must be unmerged first.
    pub fn fill(&mut self, down: bool) -> Result<(), DataError> {
        let range = self.selection();
        if self.workbook().sheets()[self.sheet()]
            .merges()
            .iter()
            .any(|r| r.intersects(range))
        {
            return Err(invalid("unmerge the fill range first"));
        }
        let source = CellRange::new(
            range.start(),
            CellAddress::new(
                if down {
                    range.start().row()
                } else {
                    range.end().row()
                },
                if down {
                    range.end().column()
                } else {
                    range.start().column()
                },
            )?,
        )?;
        let count = if down {
            range.end().row() - range.start().row()
        } else {
            range.end().column() - range.start().column()
        };
        let edits = (1..=count)
            .map(|n| {
                Ok(WorkbookEdit::Copy {
                    sheet: self.sheet(),
                    source,
                    destination_sheet: self.sheet(),
                    destination: CellAddress::new(
                        range.start().row() + if down { n } else { 0 },
                        range.start().column() + if down { 0 } else { n },
                    )?,
                })
            })
            .collect::<Result<Vec<_>, DataError>>()?;
        self.apply(&edits)
    }
    /// Bounded quoted TSV values, suitable for a host-owned clipboard.
    pub fn copy_text(&self) -> Result<String, DataError> {
        let mut result = String::new();
        let range = self.selection();
        for row in range.start().row()..=range.end().row() {
            if row != range.start().row() {
                result.push('\n');
            }
            for col in range.start().column()..=range.end().column() {
                if col != range.start().column() {
                    result.push('\t');
                }
                let value = self
                    .workbook()
                    .value(self.sheet(), CellAddress::new(row, col)?)?
                    .display();
                if value.contains(['\t', '\n', '\r', '"']) {
                    result.push('"');
                    result.push_str(&value.replace('"', "\"\""));
                    result.push('"');
                } else {
                    result.push_str(&value);
                }
                if result.len() > 4 * 1024 * 1024 {
                    return Err(invalid("clipboard payload limit exceeded"));
                }
            }
        }
        Ok(result)
    }
    pub fn paste_text(&mut self, text: &str, interpret_formulas: bool) -> Result<(), DataError> {
        let rows = parse_tsv(text)?;
        let origin = self.active();
        let columns = rows[0].len();
        if rows.iter().any(|r| r.len() != columns) {
            return Err(invalid("clipboard rows have different widths"));
        }
        let range = CellRange::new(
            origin,
            CellAddress::new(
                origin.row() + rows.len() as u32 - 1,
                origin.column() + columns as u32 - 1,
            )?,
        )?;
        let mut edits = vec![
            WorkbookEdit::Unmerge {
                sheet: self.sheet(),
                range,
            },
            WorkbookEdit::Clear {
                sheet: self.sheet(),
                range,
            },
        ];
        for (r, row) in rows.iter().enumerate() {
            for (c, text) in row.iter().enumerate() {
                edits.push(WorkbookEdit::Set {
                    sheet: self.sheet(),
                    address: CellAddress::new(origin.row() + r as u32, origin.column() + c as u32)?,
                    cell: WorkbookCell {
                        input: parse_entry(text, interpret_formulas)?,
                        style: CellStyle::default(),
                    },
                });
            }
        }
        self.apply(&edits)?;
        self.current.selection = range;
        Ok(())
    }
}
fn parse_entry(text: &str, formulas: bool) -> Result<CellInput, DataError> {
    if text.len() > MAX_TEXT_BYTES {
        return Err(invalid("cell entry limit exceeded"));
    }
    if formulas && text.starts_with('=') {
        return Ok(CellInput::Formula(text.into()));
    }
    let value = if let Some(text) = text.strip_prefix('\'').filter(|_| formulas) {
        CellValue::Text(text.into())
    } else if text.is_empty() {
        CellValue::Null
    } else if let Ok(n) = text.parse::<i64>() {
        CellValue::Integer(n)
    } else if let Ok(n) = text.parse::<f64>() {
        if !n.is_finite() {
            return Err(invalid("nonfinite cell value"));
        }
        CellValue::Real(n)
    } else if text.eq_ignore_ascii_case("true") || text.eq_ignore_ascii_case("false") {
        CellValue::Bool(text.eq_ignore_ascii_case("true"))
    } else {
        CellValue::Text(text.into())
    };
    Ok(CellInput::Value(value))
}
fn parse_tsv(text: &str) -> Result<Vec<Vec<String>>, DataError> {
    if text.len() > 4 * 1024 * 1024 {
        return Err(invalid("clipboard payload limit exceeded"));
    }
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut cell = String::new();
    let mut chars = text.chars().peekable();
    let mut quoted = false;
    let mut closed = false;
    let mut count = 0;
    while let Some(c) = chars.next() {
        if quoted {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    cell.push('"');
                } else {
                    quoted = false;
                    closed = true;
                }
            } else {
                cell.push(c);
            }
        } else if c == '"' && cell.is_empty() && !closed {
            quoted = true;
        } else if c == '\t' || c == '\n' || c == '\r' {
            row.push(std::mem::take(&mut cell));
            closed = false;
            count += 1;
            if count > MAX_CELLS {
                return Err(invalid("clipboard cell limit exceeded"));
            }
            if c != '\t' {
                if c == '\r' && chars.peek() == Some(&'\n') {
                    chars.next();
                }
                rows.push(std::mem::take(&mut row));
            }
        } else {
            if closed {
                return Err(invalid("characters after quoted clipboard value"));
            }
            cell.push(c);
        }
    }
    if quoted {
        return Err(invalid("unterminated clipboard quote"));
    }
    if !row.is_empty() || !cell.is_empty() || closed || rows.is_empty() {
        row.push(cell);
        count += 1;
        rows.push(row);
    }
    if count > MAX_CELLS {
        return Err(invalid("clipboard cell limit exceeded"));
    }
    Ok(rows)
}
