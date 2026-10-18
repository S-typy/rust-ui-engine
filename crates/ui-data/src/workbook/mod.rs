//! Bounded transactional workbooks with formula evaluation and XLSX interchange.
//!
//! Cell coordinates are zero based; A1 formulas use conventional one-based rows.
//! Formula failures are cell results. Invalid edits leave the workbook unchanged.

mod formula;
#[cfg(test)]
mod tests;
mod xlsx;

use crate::{CellValue, DataError, DataRow, DataTable, TableSchema};
use std::collections::{BTreeMap, BTreeSet};
pub use xlsx::{WorkbookDiagnostic, WorkbookImport};

pub const MAX_SHEETS: usize = 64;
pub const MAX_CELLS: usize = 16_384;
pub const MAX_TEXT_BYTES: usize = 32_768;
pub const MAX_FORMULA_BYTES: usize = 4096;
pub const MAX_ROWS: u32 = 1_048_576;
pub const MAX_COLUMNS: u32 = 16_384;

fn invalid(message: impl Into<String>) -> DataError {
    DataError::Validation(message.into())
}

/// A validated worksheet coordinate.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CellAddress {
    row: u32,
    column: u32,
}
impl CellAddress {
    pub fn new(row: u32, column: u32) -> Result<Self, DataError> {
        if row >= MAX_ROWS || column >= MAX_COLUMNS {
            return Err(invalid("cell outside worksheet limits"));
        }
        Ok(Self { row, column })
    }
    pub fn row(self) -> u32 {
        self.row
    }
    pub fn column(self) -> u32 {
        self.column
    }
    pub fn a1(self) -> String {
        let mut col = self.column + 1;
        let mut letters = Vec::new();
        while col != 0 {
            letters.push((b'A' + ((col - 1) % 26) as u8) as char);
            col = (col - 1) / 26;
        }
        format!(
            "{}{}",
            letters.into_iter().rev().collect::<String>(),
            self.row + 1
        )
    }
    pub fn parse(text: &str) -> Result<Self, DataError> {
        let letters = text.bytes().take_while(u8::is_ascii_alphabetic).count();
        if letters == 0 || letters > 3 || letters == text.len() {
            return Err(invalid("invalid A1 address"));
        }
        let mut column = 0u32;
        for b in text[..letters].bytes() {
            column = column * 26 + u32::from(b.to_ascii_uppercase() - b'A') + 1;
        }
        let digits = &text[letters..];
        if !digits.bytes().all(|b| b.is_ascii_digit()) || digits.starts_with('0') {
            return Err(invalid("invalid A1 row"));
        }
        let row: u32 = digits.parse().map_err(|_| invalid("invalid A1 row"))?;
        Self::new(
            row.checked_sub(1)
                .ok_or_else(|| invalid("invalid A1 row"))?,
            column - 1,
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CellRange {
    start: CellAddress,
    end: CellAddress,
}
impl CellRange {
    pub fn start(self) -> CellAddress {
        self.start
    }
    pub fn end(self) -> CellAddress {
        self.end
    }
    pub fn new(start: CellAddress, end: CellAddress) -> Result<Self, DataError> {
        if start.row > end.row || start.column > end.column {
            return Err(invalid("reversed cell range"));
        }
        let range = Self { start, end };
        if range.len() > MAX_CELLS {
            return Err(invalid("cell range exceeds capacity"));
        }
        Ok(range)
    }
    pub fn len(self) -> usize {
        ((u64::from(self.end.row - self.start.row) + 1)
            * (u64::from(self.end.column - self.start.column) + 1)) as usize
    }
    pub fn is_empty(self) -> bool {
        false
    }
    fn addresses(self) -> impl Iterator<Item = CellAddress> {
        (self.start.row..=self.end.row).flat_map(move |row| {
            (self.start.column..=self.end.column).map(move |column| CellAddress { row, column })
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FormulaError {
    DivisionByZero,
    Value,
    Reference,
    Name,
    Number,
    Cycle,
    Limit,
}
impl FormulaError {
    pub fn display(self) -> &'static str {
        match self {
            Self::DivisionByZero => "#DIV/0!",
            Self::Value => "#VALUE!",
            Self::Reference => "#REF!",
            Self::Name => "#NAME?",
            Self::Number => "#NUM!",
            Self::Cycle => "#CYCLE!",
            Self::Limit => "#LIMIT!",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum WorkbookValue {
    Scalar(CellValue),
    Error(FormulaError),
}
impl WorkbookValue {
    pub fn display(&self) -> String {
        match self {
            Self::Scalar(v) => v.display(),
            Self::Error(e) => e.display().into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum CellInput {
    Value(CellValue),
    Formula(String),
    Error(FormulaError),
}

/// Explicit cell styling. Colors are opaque sRGB RRGGBB values.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CellStyle {
    pub bold: bool,
    pub italic: bool,
    pub foreground: Option<u32>,
    pub background: Option<u32>,
    pub number_format: String,
    pub wrap: bool,
}
#[derive(Clone, Debug, PartialEq)]
pub struct WorkbookCell {
    pub input: CellInput,
    pub style: CellStyle,
}
impl WorkbookCell {
    pub fn value(value: CellValue) -> Self {
        Self {
            input: CellInput::Value(value),
            style: CellStyle::default(),
        }
    }
    pub fn formula(text: impl Into<String>) -> Self {
        Self {
            input: CellInput::Formula(text.into()),
            style: CellStyle::default(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Worksheet {
    name: String,
    cells: BTreeMap<CellAddress, WorkbookCell>,
}
impl Worksheet {
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn cells(&self) -> &BTreeMap<CellAddress, WorkbookCell> {
        &self.cells
    }
    pub fn cell(&self, address: CellAddress) -> Option<&WorkbookCell> {
        self.cells.get(&address)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct WorkbookCellId {
    pub sheet: usize,
    pub address: CellAddress,
}

#[derive(Clone, Debug, PartialEq)]
pub enum WorkbookEdit {
    Set {
        sheet: usize,
        address: CellAddress,
        cell: WorkbookCell,
    },
    Clear {
        sheet: usize,
        range: CellRange,
    },
    /// Snapshot copy; relative references translate and absolute axes stay fixed.
    Copy {
        sheet: usize,
        source: CellRange,
        destination_sheet: usize,
        destination: CellAddress,
    },
    AddSheet {
        name: String,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Workbook {
    sheets: Vec<Worksheet>,
    values: BTreeMap<WorkbookCellId, WorkbookValue>,
    dependencies: BTreeMap<WorkbookCellId, BTreeSet<WorkbookCellId>>,
    revision: u64,
}
impl Workbook {
    pub fn new(name: impl Into<String>) -> Result<Self, DataError> {
        let name = name.into();
        validate_name(&name)?;
        Ok(Self {
            sheets: vec![Worksheet {
                name,
                cells: BTreeMap::new(),
            }],
            values: BTreeMap::new(),
            dependencies: BTreeMap::new(),
            revision: 0,
        })
    }
    pub fn sheets(&self) -> &[Worksheet] {
        &self.sheets
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn value(&self, sheet: usize, address: CellAddress) -> Result<WorkbookValue, DataError> {
        self.sheet(sheet)?;
        Ok(self
            .values
            .get(&WorkbookCellId { sheet, address })
            .cloned()
            .unwrap_or(WorkbookValue::Scalar(CellValue::Null)))
    }
    pub fn dependencies(
        &self,
        sheet: usize,
        address: CellAddress,
    ) -> Option<&BTreeSet<WorkbookCellId>> {
        self.dependencies.get(&WorkbookCellId { sheet, address })
    }
    fn sheet(&self, sheet: usize) -> Result<&Worksheet, DataError> {
        self.sheets
            .get(sheet)
            .ok_or_else(|| invalid("unknown worksheet"))
    }
    /// Apply a bounded batch atomically and recalculate dependencies once.
    pub fn apply(&mut self, edits: &[WorkbookEdit]) -> Result<(), DataError> {
        if edits.is_empty() {
            return Ok(());
        }
        if edits.len() > MAX_CELLS {
            return Err(invalid("edit batch exceeds capacity"));
        }
        let mut next = self.clone();
        for edit in edits {
            match edit {
                WorkbookEdit::Set {
                    sheet,
                    address,
                    cell,
                } => {
                    next.sheet(*sheet)?;
                    validate_cell(cell)?;
                    next.sheets[*sheet].cells.insert(*address, cell.clone());
                }
                WorkbookEdit::Clear { sheet, range } => {
                    next.sheet(*sheet)?;
                    CellRange::new(range.start, range.end)?;
                    next.sheets[*sheet].cells.retain(|a, _| {
                        a.row < range.start.row
                            || a.row > range.end.row
                            || a.column < range.start.column
                            || a.column > range.end.column
                    });
                }
                WorkbookEdit::AddSheet { name } => {
                    validate_name(name)?;
                    if next.sheets.len() == MAX_SHEETS
                        || next
                            .sheets
                            .iter()
                            .any(|s| s.name.to_lowercase() == name.to_lowercase())
                    {
                        return Err(invalid("duplicate sheet or sheet capacity exceeded"));
                    }
                    next.sheets.push(Worksheet {
                        name: name.clone(),
                        cells: BTreeMap::new(),
                    });
                }
                WorkbookEdit::Copy {
                    sheet,
                    source,
                    destination_sheet,
                    destination,
                } => {
                    CellRange::new(source.start, source.end)?;
                    let original = next.sheet(*sheet)?.clone();
                    next.sheet(*destination_sheet)?;
                    let rows = i64::from(destination.row) - i64::from(source.start.row);
                    let cols = i64::from(destination.column) - i64::from(source.start.column);
                    for address in source.addresses() {
                        let target = CellAddress::new(
                            (i64::from(address.row) + rows)
                                .try_into()
                                .map_err(|_| invalid("copy outside sheet"))?,
                            (i64::from(address.column) + cols)
                                .try_into()
                                .map_err(|_| invalid("copy outside sheet"))?,
                        )?;
                        if let Some(cell) = original.cells.get(&address) {
                            let mut cell = cell.clone();
                            if let CellInput::Formula(text) = &mut cell.input {
                                *text = formula::translate(text, rows, cols)?;
                            }
                            next.sheets[*destination_sheet].cells.insert(target, cell);
                        } else {
                            next.sheets[*destination_sheet].cells.remove(&target);
                        }
                    }
                }
            }
            if next.sheets.iter().map(|s| s.cells.len()).sum::<usize>() > MAX_CELLS {
                return Err(invalid("workbook cell capacity exceeded"));
            }
        }
        let payload: usize = next
            .sheets
            .iter()
            .flat_map(|s| s.cells.values())
            .map(|c| {
                c.style.number_format.len()
                    + match &c.input {
                        CellInput::Value(CellValue::Text(t)) | CellInput::Formula(t) => t.len(),
                        _ => 0,
                    }
            })
            .sum();
        if payload > 16 * 1024 * 1024 {
            return Err(invalid("workbook text capacity exceeded"));
        }
        next.recalculate()?;
        next.revision = self
            .revision
            .checked_add(1)
            .ok_or(DataError::RevisionOverflow)?;
        *self = next;
        Ok(())
    }
    fn recalculate(&mut self) -> Result<(), DataError> {
        formula::recalculate(self)
    }
    /// Query bridge: fixed schema is validated against the evaluated range.
    /// Formula errors are reported instead of silently converted to null.
    pub fn to_table(
        &self,
        sheet: usize,
        range: CellRange,
        schema: TableSchema,
    ) -> Result<DataTable, DataError> {
        self.sheet(sheet)?;
        CellRange::new(range.start, range.end)?;
        if schema.fields().len() != (range.end.column - range.start.column + 1) as usize {
            return Err(invalid("range width does not match schema"));
        }
        let mut rows = Vec::new();
        for row in range.start.row..=range.end.row {
            let mut values = Vec::new();
            for column in range.start.column..=range.end.column {
                match self.value(sheet, CellAddress { row, column })? {
                    WorkbookValue::Scalar(v) => values.push(v),
                    WorkbookValue::Error(e) => {
                        return Err(invalid(format!(
                            "{}: {}",
                            CellAddress { row, column }.a1(),
                            e.display()
                        )));
                    }
                }
            }
            rows.push(DataRow::new(i64::from(row), values));
        }
        DataTable::new(schema, rows)
    }
}

fn validate_name(name: &str) -> Result<(), DataError> {
    if name.trim().is_empty()
        || name.chars().count() > 31
        || name.starts_with('\'')
        || name.ends_with('\'')
        || name
            .chars()
            .any(|c| c.is_control() || "[]:*?/\\".contains(c))
    {
        return Err(invalid("invalid worksheet name"));
    }
    Ok(())
}
fn validate_cell(cell: &WorkbookCell) -> Result<(), DataError> {
    if cell.style.foreground.is_some_and(|c| c > 0xffffff)
        || cell.style.background.is_some_and(|c| c > 0xffffff)
        || cell.style.number_format.len() > 256
    {
        return Err(invalid("invalid cell style"));
    }
    match &cell.input {
        CellInput::Value(CellValue::Real(v)) if !v.is_finite() => {
            return Err(invalid("nonfinite cell number"));
        }
        CellInput::Value(CellValue::Text(t)) if t.len() > MAX_TEXT_BYTES || !valid_xml(t) => {
            return Err(invalid("invalid cell text or text capacity exceeded"));
        }
        CellInput::Value(CellValue::Binary(_)) => {
            return Err(invalid("binary cells are not supported"));
        }
        CellInput::Formula(t) => {
            if !valid_xml(t) {
                return Err(invalid("invalid formula text"));
            }
            formula::parse(t)?;
        }
        _ => {}
    }
    if !valid_xml(&cell.style.number_format) {
        return Err(invalid("invalid number format"));
    }
    Ok(())
}
fn valid_xml(text: &str) -> bool {
    text.chars().all(|c| {
        matches!(c, '\t' | '\n' | '\r') || (c >= ' ' && c != '\u{fffe}' && c != '\u{ffff}')
    })
}
