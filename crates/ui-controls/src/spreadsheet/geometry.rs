//! One sparse geometry contract for worksheet painting, input and accessibility.
use super::*;
use std::ops::Range;

pub(super) struct SheetGeometry {
    pub bounds: Rect,
    pub body: Rect,
    rows: SparseAxis,
    columns: SparseAxis,
    first_row: u32,
    first_column: u32,
    pub visible_rows: Range<u32>,
    pub visible_columns: Range<u32>,
}

impl SheetGeometry {
    pub fn new(
        bounds: Rect,
        first_row: u32,
        first_column: u32,
        dimensions: &WorksheetDimensions,
    ) -> Result<Self, ControlError> {
        Self::with_override(bounds, first_row, first_column, dimensions, None)
    }
    pub fn with_override(
        bounds: Rect,
        first_row: u32,
        first_column: u32,
        dimensions: &WorksheetDimensions,
        resize: Option<(bool, u32, f64)>,
    ) -> Result<Self, ControlError> {
        let mut rows =
            SparseAxis::new(MAX_ROWS as usize, dimensions.default_row_height()).map_err(err)?;
        rows.replace(
            MAX_ROWS as usize,
            dimensions.default_row_height(),
            dimensions.rows().iter().map(|(i, v)| (*i as usize, *v)),
        )
        .map_err(err)?;
        let mut columns = SparseAxis::new(MAX_COLUMNS as usize, dimensions.default_column_width())
            .map_err(err)?;
        columns
            .replace(
                MAX_COLUMNS as usize,
                dimensions.default_column_width(),
                dimensions.columns().iter().map(|(i, v)| (*i as usize, *v)),
            )
            .map_err(err)?;
        if let Some((row, index, extent)) = resize {
            (if row { &mut rows } else { &mut columns })
                .set_extent(index as usize, Some(extent))
                .map_err(err)?;
        }
        if rows.count() != MAX_ROWS as usize
            || columns.count() != MAX_COLUMNS as usize
            || first_row >= MAX_ROWS
            || first_column >= MAX_COLUMNS
        {
            return Err(err("Invalid worksheet axes"));
        }
        let body = Rect::new(
            bounds.x + ROW_HEADER,
            bounds.y + HEADER,
            (bounds.width - ROW_HEADER).max(0.0),
            (bounds.height - HEADER).max(0.0),
        );
        let visible = |axis: &SparseAxis, first: u32, extent: f32| {
            if extent <= 0.0 {
                return first..first;
            }
            let start = axis.offset_of(first as usize).unwrap();
            let end = (start + f64::from(extent)).min(axis.total_extent());
            let last = axis
                .index_at((end - 0.000001).max(start))
                .unwrap_or(axis.count() - 1);
            first..last as u32 + 1
        };
        let visible_rows = visible(&rows, first_row, body.height);
        let visible_columns = visible(&columns, first_column, body.width);
        if u64::from(visible_rows.end - visible_rows.start)
            * u64::from(visible_columns.end - visible_columns.start)
            > MAX_CELLS as u64
        {
            return Err(err(
                "Worksheet viewport exceeds the materialized-cell budget",
            ));
        }
        Ok(Self {
            bounds,
            body,
            rows,
            columns,
            first_row,
            first_column,
            visible_rows,
            visible_columns,
        })
    }
    pub fn x(&self, column: u32) -> f32 {
        self.body.x
            + (self.columns.offset_of(column as usize).unwrap()
                - self.columns.offset_of(self.first_column as usize).unwrap()) as f32
    }
    pub fn y(&self, row: u32) -> f32 {
        self.body.y
            + (self.rows.offset_of(row as usize).unwrap()
                - self.rows.offset_of(self.first_row as usize).unwrap()) as f32
    }
    pub fn range(&self, range: CellRange) -> Rect {
        let x = self.x(range.start().column());
        let y = self.y(range.start().row());
        Rect::new(
            x,
            y,
            (self
                .columns
                .offset_of(range.end().column() as usize + 1)
                .unwrap()
                - self
                    .columns
                    .offset_of(range.start().column() as usize)
                    .unwrap()) as f32,
            (self.rows.offset_of(range.end().row() as usize + 1).unwrap()
                - self.rows.offset_of(range.start().row() as usize).unwrap()) as f32,
        )
    }
    pub fn column_header(&self, column: u32) -> Rect {
        Rect::new(
            self.x(column),
            self.bounds.y,
            self.columns.extent_at(column as usize).unwrap() as f32,
            HEADER,
        )
    }
    pub fn row_header(&self, row: u32) -> Rect {
        Rect::new(
            self.bounds.x,
            self.y(row),
            ROW_HEADER,
            self.rows.extent_at(row as usize).unwrap() as f32,
        )
    }
    pub fn hit(&self, point: Point) -> Option<CellAddress> {
        if !self.body.contains(point.x, point.y) {
            return None;
        }
        let column = self.columns.index_at(
            self.columns.offset_of(self.first_column as usize)? + f64::from(point.x - self.body.x),
        )?;
        let row = self.rows.index_at(
            self.rows.offset_of(self.first_row as usize)? + f64::from(point.y - self.body.y),
        )?;
        CellAddress::new(row as u32, column as u32).ok()
    }
    pub fn edge(&self, point: Point) -> Option<(bool, u32)> {
        if point.y >= self.bounds.y
            && point.y < self.body.y
            && point.x >= self.body.x
            && point.x < self.bounds.x + self.bounds.width
        {
            return self
                .visible_columns
                .clone()
                .find(|col| (point.x - self.x(col + 1)).abs() <= 4.0)
                .map(|col| (false, col));
        }
        if point.x >= self.bounds.x
            && point.x < self.body.x
            && point.y >= self.body.y
            && point.y < self.bounds.y + self.bounds.height
        {
            return self
                .visible_rows
                .clone()
                .find(|row| (point.y - self.y(row + 1)).abs() <= 4.0)
                .map(|row| (true, row));
        }
        None
    }
    pub fn fill_handle(&self, range: CellRange) -> Option<Rect> {
        let rect = self.range(range);
        let x = rect.x + rect.width;
        let y = rect.y + rect.height;
        self.body
            .contains(x - 0.5, y - 0.5)
            .then(|| Rect::new(x - 4.0, y - 4.0, 8.0, 8.0))
    }
    pub fn reveal(&self, address: CellAddress) -> (u32, u32) {
        fn first(axis: &SparseAxis, current: u32, target: u32, extent: f32) -> u32 {
            if target < current {
                return target;
            }
            let start = axis.offset_of(current as usize).unwrap();
            let end = axis.offset_of(target as usize + 1).unwrap();
            if end - start <= f64::from(extent) {
                return current;
            }
            let goal = (end - f64::from(extent)).max(0.0);
            let mut first = axis.index_at(goal).unwrap_or(target as usize);
            if axis.offset_of(first).unwrap() < goal {
                first += 1;
            }
            (first as u32).min(target)
        }
        (
            first(&self.rows, self.first_row, address.row(), self.body.height),
            first(
                &self.columns,
                self.first_column,
                address.column(),
                self.body.width,
            ),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sparse_sizes_share_hit_bounds_headers_and_last_cell_precision() {
        let mut book = Workbook::new("Geometry").unwrap();
        book.apply(&[
            WorkbookEdit::ColumnWidth {
                sheet: 0,
                column: 0,
                width: Some(180.0),
            },
            WorkbookEdit::RowHeight {
                sheet: 0,
                row: 0,
                height: Some(42.0),
            },
            WorkbookEdit::ColumnWidth {
                sheet: 0,
                column: MAX_COLUMNS - 1,
                width: Some(211.25),
            },
            WorkbookEdit::RowHeight {
                sheet: 0,
                row: MAX_ROWS - 1,
                height: Some(56.5),
            },
        ])
        .unwrap();
        let dimensions = book.sheets()[0].dimensions();
        let bounds = Rect::new(10.0, 20.0, 700.0, 400.0);
        let geometry = SheetGeometry::new(bounds, 0, 0, dimensions).unwrap();
        assert_eq!(geometry.column_header(1).x, 250.0);
        assert_eq!(geometry.row_header(1).y, 90.0);
        assert_eq!(
            geometry.hit(Point::new(251.0, 91.0)),
            Some(CellAddress::new(1, 1).unwrap())
        );
        assert_eq!(geometry.hit(Point::new(69.0, 91.0)), None);
        let range = CellRange::new(
            CellAddress::new(0, 0).unwrap(),
            CellAddress::new(1, 1).unwrap(),
        )
        .unwrap();
        assert_eq!(geometry.range(range), Rect::new(70.0, 48.0, 292.0, 70.0));
        let last = CellAddress::new(MAX_ROWS - 1, MAX_COLUMNS - 1).unwrap();
        let (row, column) = geometry.reveal(last);
        let geometry = SheetGeometry::new(bounds, row, column, dimensions).unwrap();
        let rect = geometry.range(CellRange::new(last, last).unwrap());
        assert_eq!(rect.width, 211.25);
        assert_eq!(rect.height, 56.5);
        assert_eq!(
            geometry.hit(Point::new(rect.x + 1.0, rect.y + 1.0)),
            Some(last)
        );
        assert!(rect.x + rect.width <= bounds.x + bounds.width);
        assert!(rect.y + rect.height <= bounds.y + bounds.height);
    }
}
