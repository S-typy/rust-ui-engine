//! Immutable worksheet pages preserving static cell styling and merged ranges.
use super::*;
use rust_desktop_ui_data::workbook::{CellAddress, CellRange, Workbook};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SheetPrintOptions {
    pub page_size: Size,
    pub margin: f32,
    pub column_width: f32,
    pub row_height: f32,
    pub fit_width: bool,
}
impl Default for SheetPrintOptions {
    fn default() -> Self {
        Self {
            page_size: Size::new(794.0, 1123.0),
            margin: 48.0,
            column_width: 112.0,
            row_height: 28.0,
            fit_width: true,
        }
    }
}
impl SheetPrintOptions {
    fn body(&self) -> Result<Rect, PdfError> {
        if !self.page_size.width.is_finite()
            || !self.page_size.height.is_finite()
            || !(1.0..=19200.0).contains(&self.page_size.width)
            || !(1.0..=19200.0).contains(&self.page_size.height)
            || !self.margin.is_finite()
            || self.margin < 0.0
            || !self.column_width.is_finite()
            || !(24.0..=2048.0).contains(&self.column_width)
            || !self.row_height.is_finite()
            || !(14.0..=2048.0).contains(&self.row_height)
        {
            return Err(invalid("worksheet print geometry"));
        }
        let body = Rect::new(
            self.margin,
            self.margin,
            self.page_size.width - self.margin * 2.0,
            self.page_size.height - self.margin * 2.0,
        );
        if !body.is_valid() || body.width < 24.0 || body.height < 14.0 {
            return Err(invalid("worksheet print margins leave no content area"));
        }
        Ok(body)
    }
}

/// Snapshot a worksheet as immutable pages with static cell styling and merges.
/// The bounded profile accepts at most 1000 used rows and 12 used columns.
/// A merged block that cannot fit one page is rejected instead of clipped.
pub fn from_workbook(
    book: &Workbook,
    sheet_index: usize,
    options: SheetPrintOptions,
    mut cancelled: impl FnMut() -> bool,
) -> Result<PrintDocument, PdfError> {
    if cancelled() {
        return Err(PdfError::Cancelled);
    }
    let body = options.body()?;
    let sheet = book
        .sheets()
        .get(sheet_index)
        .ok_or_else(|| invalid("unknown worksheet"))?;
    let extent = sheet
        .cells()
        .keys()
        .copied()
        .chain(sheet.merges().iter().map(|r| r.end()));
    let (rows, columns) = extent.fold((1, 1), |(rows, columns), a| {
        (rows.max(a.row() + 1), columns.max(a.column() + 1))
    });
    if rows > 1000 || columns > 12 {
        return Err(PdfError::Limit(
            "worksheet print profile: 1000 rows / 12 columns",
        ));
    }
    let width = columns as f32 * options.column_width;
    let scale = if options.fit_width {
        (body.width / width).min(1.0)
    } else {
        1.0
    };
    if width * scale > body.width + 0.001 || scale < 0.05 {
        return Err(invalid("worksheet columns do not fit the page"));
    }
    let row_height = options.row_height * scale;
    let column_width = options.column_width * scale;
    let capacity = (body.height / row_height).floor() as u32;
    if capacity == 0 {
        return Err(invalid("worksheet row is taller than printable area"));
    }
    let mut first = 0;
    let mut pages = Vec::new();
    let mut commands = 0usize;
    let mut text_bytes = 0usize;
    while first < rows {
        if cancelled() {
            return Err(PdfError::Cancelled);
        }
        let mut end = first.saturating_add(capacity).min(rows);
        // Every moved break can intersect a merge in another column; repeat
        // until all ranges on the page are whole. Progress is strictly decreasing.
        loop {
            let before = end;
            for range in sheet.merges() {
                if range.start().row() < end && range.end().row() >= end {
                    end = end.min(range.start().row());
                }
            }
            if end <= first {
                return Err(invalid("merged worksheet block exceeds one page"));
            }
            if end == before {
                break;
            }
        }
        let mut scene = Scene::default();
        scene.fill(
            Rect::new(0.0, 0.0, options.page_size.width, options.page_size.height),
            Color::rgb(255, 255, 255),
        );
        for row in first..end {
            for column in 0..columns {
                if cancelled() {
                    return Err(PdfError::Cancelled);
                }
                let address = CellAddress::new(row, column).map_err(invalid)?;
                if sheet.anchor(address) != address {
                    continue;
                }
                let range = sheet
                    .merges()
                    .iter()
                    .find(|r| r.start() == address)
                    .copied()
                    .unwrap_or(CellRange::new(address, address).map_err(invalid)?);
                let rect = Rect::new(
                    body.x + column as f32 * column_width,
                    body.y + (row - first) as f32 * row_height,
                    (range.end().column() - column + 1) as f32 * column_width,
                    (range.end().row() - row + 1) as f32 * row_height,
                );
                let cell = sheet.cell(address);
                let rgb = |v: u32| Color::rgb((v >> 16) as u8, (v >> 8) as u8, v as u8);
                if let Some(color) = cell.and_then(|c| c.style.background) {
                    scene.fill(rect, rgb(color));
                }
                scene.rounded_stroke(rect, body, 0.0, 0.5 * scale, Color::rgb(210, 210, 210));
                let value = book.value(sheet_index, address).map_err(invalid)?.display();
                text_bytes = text_bytes
                    .checked_add(value.len())
                    .ok_or(PdfError::Limit("worksheet text"))?;
                if text_bytes > 16 * 1024 * 1024 {
                    return Err(PdfError::Limit("worksheet print text"));
                }
                let clip = rect
                    .intersection(body)
                    .ok_or_else(|| invalid("worksheet cell clip"))?;
                let run = crate::spreadsheet::cell_text_run(
                    value,
                    rect,
                    clip,
                    Color::rgb(0, 0, 0),
                    scale,
                );
                crate::spreadsheet::paint_cell_text(&mut scene, run, cell.map(|c| &c.style))
                    .map_err(invalid)?;
            }
        }
        commands += scene.commands().len();
        if commands > 100_000 || pages.len() == 512 {
            return Err(PdfError::Limit("worksheet print pages/commands"));
        }
        pages.push(PrintPage {
            size: options.page_size,
            scene,
        });
        first = end;
    }
    if cancelled() {
        return Err(PdfError::Cancelled);
    }
    PrintDocument::new(sheet.name(), pages).map_err(invalid)
}
