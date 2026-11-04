//! Sparse worksheet dimensions in logical pixels (96 per inch).
use super::*;

/// Bounds dimension metadata independently of populated cells.
pub const MAX_DIMENSIONS: usize = 16_384;
pub const DEFAULT_ROW_HEIGHT: f64 = 28.0;
pub const DEFAULT_COLUMN_WIDTH: f64 = 112.0;

#[derive(Clone, Debug, PartialEq)]
pub struct WorksheetDimensions {
    row_height: f64,
    column_width: f64,
    rows: BTreeMap<u32, f64>,
    columns: BTreeMap<u32, f64>,
}
impl Default for WorksheetDimensions {
    fn default() -> Self {
        Self {
            row_height: DEFAULT_ROW_HEIGHT,
            column_width: DEFAULT_COLUMN_WIDTH,
            rows: BTreeMap::new(),
            columns: BTreeMap::new(),
        }
    }
}
impl WorksheetDimensions {
    pub fn default_row_height(&self) -> f64 {
        self.row_height
    }
    pub fn default_column_width(&self) -> f64 {
        self.column_width
    }
    pub fn rows(&self) -> &BTreeMap<u32, f64> {
        &self.rows
    }
    pub fn columns(&self) -> &BTreeMap<u32, f64> {
        &self.columns
    }
    pub fn row_height(&self, row: u32) -> Option<f64> {
        (row < MAX_ROWS).then(|| self.rows.get(&row).copied().unwrap_or(self.row_height))
    }
    pub fn column_width(&self, column: u32) -> Option<f64> {
        (column < MAX_COLUMNS).then(|| {
            self.columns
                .get(&column)
                .copied()
                .unwrap_or(self.column_width)
        })
    }
    pub(super) fn count(&self) -> usize {
        self.rows.len() + self.columns.len()
    }
    pub(super) fn set_default(&mut self, row: f64, column: f64) -> Result<(), DataError> {
        validate_extent(row)?;
        validate_extent(column)?;
        self.row_height = row;
        self.column_width = column;
        Ok(())
    }
    pub(super) fn set(
        &mut self,
        row: bool,
        index: u32,
        extent: Option<f64>,
    ) -> Result<(), DataError> {
        if index >= if row { MAX_ROWS } else { MAX_COLUMNS } {
            return Err(invalid("dimension outside sheet"));
        }
        if let Some(value) = extent {
            validate_extent(value)?;
        }
        let axis = if row {
            &mut self.rows
        } else {
            &mut self.columns
        };
        if let Some(value) = extent {
            axis.insert(index, value);
        } else {
            axis.remove(&index);
        }
        Ok(())
    }
}
fn validate_extent(value: f64) -> Result<(), DataError> {
    if !value.is_finite() || !(1.0..=4096.0).contains(&value) {
        return Err(invalid(
            "worksheet dimension must be between 1 and 4096 logical pixels",
        ));
    }
    Ok(())
}
