//! An immutable output snapshot of accepted Grid records and field order.
use super::*;
use rust_desktop_ui_core::pages::{PrintDocument, PrintPage};
use std::fmt::Write as _;
#[derive(Clone, Debug)]
struct OutputRow {
    values: Vec<CellValue>,
    depth: u8,
    record: bool,
}
#[derive(Clone, Debug)]
pub struct GridOutput {
    headers: Vec<String>,
    widths: Vec<f32>,
    bands: Vec<Vec<String>>,
    rows: Vec<OutputRow>,
}
#[derive(Clone, Copy, Debug)]
pub struct GridPrintOptions {
    pub paper: Size,
    pub margin: f32,
    pub row_height: f32,
    /// Inclusive, one-based physical page range, following horizontal then vertical pagination.
    pub range: Option<(usize, usize)>,
}
impl Default for GridPrintOptions {
    fn default() -> Self {
        Self {
            paper: Size::new(794., 1123.),
            margin: 40.,
            row_height: 26.,
            range: None,
        }
    }
}
impl Controls {
    /// Empty fields selects visible columns in their displayed order.
    /// All matching master records are included even when their groups are collapsed.
    pub fn grid_output(
        &self,
        id: WidgetId,
        fields: &[String],
        selection_only: bool,
    ) -> Result<GridOutput, ControlError> {
        let g = self
            .grids
            .get(&id)
            .ok_or_else(|| error("Unknown GridControl"))?;
        let layout = g.current_arrangement();
        let cols: Vec<_> = if fields.is_empty() {
            layout.iter().filter(|c| c.visible).collect()
        } else {
            let mut seen = HashSet::new();
            fields
                .iter()
                .map(|f| {
                    if !seen.insert(f) {
                        return Err(error("Duplicate export field"));
                    }
                    layout
                        .iter()
                        .find(|c| c.field == *f)
                        .ok_or_else(|| error("Unknown export field"))
                })
                .collect::<Result<_, _>>()?
        };
        if cols.is_empty() {
            return Err(error("Select at least one export column"));
        }
        let s = &g.source;
        let p = &s.projection;
        let selected = g.model.selected();
        let permitted = |k: RowKey| !selection_only || selected.contains(&k);
        let bands = std::iter::once(&s.presentation.bands)
            .chain(&s.presentation.band_rows)
            .filter(|tier| !tier.is_empty())
            .map(|tier| {
                cols.iter()
                    .map(|c| {
                        tier.iter()
                            .find(|b| b.fields.contains(&c.field))
                            .map_or_else(String::new, |b| b.title.clone())
                    })
                    .collect()
            })
            .collect();
        let mut output = GridOutput {
            headers: cols
                .iter()
                .map(|c| {
                    g.settings
                        .columns
                        .iter()
                        .find(|v| v.field == c.field)
                        .unwrap()
                        .title
                        .clone()
                })
                .collect(),
            widths: cols.iter().map(|c| c.width).collect(),
            bands,
            rows: vec![],
        };
        let mut pending: Vec<_> = p.roots.iter().rev().map(|k| (*k, 0u8)).collect();
        let mut cells = 0usize;
        let mut bytes = 0usize;
        while let Some((k, depth)) = pending.pop() {
            let synthetic = p.synthetic.get(&k);
            if matches!(
                synthetic.map(|r| &r.identity),
                Some(
                    projection::SyntheticIdentity::DetailHeader(_)
                        | projection::SyntheticIdentity::Detail(..)
                )
            ) {
                continue;
            }
            if let Some(children) = p.children.get(&k) {
                pending.extend(children.iter().rev().map(|k| (*k, depth.saturating_add(1))));
            }
            let row = if let Some(group) = synthetic {
                if selection_only {
                    continue;
                }
                OutputRow {
                    record: false,
                    depth,
                    values: vec![CellValue::Text(group.label.clone())],
                }
            } else {
                if !permitted(k) {
                    continue;
                }
                let values = cols
                    .iter()
                    .map(|c| {
                        s.table
                            .value(data_key(k), &c.field)
                            .cloned()
                            .ok_or_else(|| error("Grid output field unavailable"))
                    })
                    .collect::<Result<_, _>>()?;
                OutputRow {
                    values,
                    depth,
                    record: true,
                }
            };
            cells += row.values.len();
            bytes += row
                .values
                .iter()
                .map(|v| match v {
                    CellValue::Text(t) => t.len(),
                    CellValue::Binary(b) => b.len(),
                    _ => 16,
                })
                .sum::<usize>();
            if cells > 1_000_000 || bytes > 32 * 1024 * 1024 {
                return Err(error("Grid output exceeds one million cells or 32 MiB"));
            }
            output.rows.push(row);
        }
        if !selection_only && !p.totals.is_empty() {
            for (i, (title, val)) in s.presentation.summary_titles().zip(&p.totals).enumerate() {
                let col = s
                    .presentation
                    .summary_columns
                    .iter()
                    .find(|(n, _)| *n == i)
                    .and_then(|(_, f)| cols.iter().position(|c| &c.field == f))
                    .unwrap_or(usize::from(cols.len() > 1));
                let mut values = vec![CellValue::Null; cols.len()];
                if col != 0 {
                    values[0] = CellValue::Text(title.into())
                }
                values[col] = val.clone();
                output.rows.push(OutputRow {
                    values,
                    depth: 0,
                    record: false,
                });
            }
        }
        Ok(output)
    }
}
impl GridOutput {
    pub fn record_count(&self) -> usize {
        self.rows.iter().filter(|r| r.record).count()
    }
    pub fn csv(&self) -> String {
        let mut text = String::new();
        let mut write_row = |cells: Vec<String>| {
            for (i, c) in cells.iter().enumerate() {
                if i > 0 {
                    text.push(',')
                }
                if c.contains([',', '"', '\r', '\n']) {
                    text.push('"');
                    text.push_str(&c.replace('"', "\"\""));
                    text.push('"')
                } else {
                    text.push_str(c)
                }
            }
            text.push_str("\r\n");
        };
        write_row(self.headers.clone());
        for r in self.rows.iter().filter(|r| r.record) {
            write_row(r.values.iter().map(CellValue::display).collect())
        }
        text
    }
    pub fn xlsx(&self) -> Result<Vec<u8>, ControlError> {
        rust_desktop_ui_data::workbook::export_data_sheet(
            "Grid",
            &self.headers,
            self.rows
                .iter()
                .map(|r| (r.depth.min(7), r.values.as_slice())),
        )
        .map_err(error)
    }
    pub fn html(&self) -> String {
        let mut out = String::from("<!doctype html><meta charset=\"utf-8\"><table><thead><tr>");
        for h in &self.headers {
            write!(out, "<th scope=\"col\">{}</th>", transfer::html(h)).unwrap()
        }
        out.push_str("</tr></thead><tbody>");
        for r in &self.rows {
            out.push_str("<tr>");
            for v in &r.values {
                write!(out, "<td>{}</td>", transfer::html(&v.display())).unwrap()
            }
            out.push_str("</tr>")
        }
        out.push_str("</tbody></table>");
        out
    }
    /// Single-sheet Excel 4 BIFF worksheet. Supports ASCII and Russian text,
    /// 16,384 rows, 256 columns and 255 encoded bytes per label.
    pub fn xls(&self) -> Result<Vec<u8>, ControlError> {
        fn record(out: &mut Vec<u8>, kind: u16, body: &[u8]) {
            out.extend(kind.to_le_bytes());
            out.extend((body.len() as u16).to_le_bytes());
            out.extend(body)
        }
        if self.rows.len() >= 16384 {
            return Err(error("BIFF4 row limit exceeded"));
        }
        let mut out = Vec::new();
        record(&mut out, 0x0409, &[0, 4, 0x10, 0, 0, 0]);
        record(&mut out, 0x42, &1251u16.to_le_bytes());
        let head: Vec<_> = self.headers.iter().cloned().map(CellValue::Text).collect();
        for (r, values) in std::iter::once(head.as_slice())
            .chain(self.rows.iter().map(|r| r.values.as_slice()))
            .enumerate()
        {
            for (c, v) in values.iter().enumerate() {
                let mut body = Vec::new();
                body.extend((r as u16).to_le_bytes());
                body.extend((c as u16).to_le_bytes());
                body.extend(0u16.to_le_bytes());
                let numeric = match v {
                    CellValue::Real(n) => Some(*n),
                    CellValue::Integer(n) if n.unsigned_abs() <= 999_999_999_999_999 => {
                        Some(*n as f64)
                    }
                    CellValue::Decimal(d)
                        if d.coefficient().unsigned_abs().to_string().len() <= 15 =>
                    {
                        d.to_string().parse().ok()
                    }
                    _ => None,
                };
                if let Some(n) = numeric {
                    body.extend(n.to_le_bytes());
                    record(&mut out, 0x0203, &body)
                } else if let CellValue::Bool(b) = v {
                    body.extend([u8::from(*b), 0]);
                    record(&mut out, 0x0205, &body)
                } else if v != &CellValue::Null {
                    let mut text = Vec::new();
                    for ch in v.display().chars() {
                        text.push(match ch {
                            '\u{0000}'..='\u{007f}' => ch as u8,
                            'А'..='я' => (ch as u32 - 'А' as u32 + 0xc0) as u8,
                            'Ё' => 0xa8,
                            'ё' => 0xb8,
                            '·' => 0xb7,
                            '№' => 0xb9,
                            '—' => 0x97,
                            '–' => 0x96,
                            _ => {
                                return Err(error(
                                    "Text outside the BIFF4 code-page profile; use XLSX",
                                ));
                            }
                        });
                    }
                    if text.len() > 255 {
                        return Err(error("BIFF4 label exceeds 255 bytes"));
                    }
                    body.extend((text.len() as u16).to_le_bytes());
                    body.extend(text);
                    record(&mut out, 0x0204, &body)
                }
            }
        }
        record(&mut out, 0x000a, &[]);
        Ok(out)
    }
    pub fn rtf(&self) -> String {
        let mut out = String::from("{\\rtf1\\ansi\\uc1\n");
        let mut append = |cells: &[String]| {
            out.push_str("\\trowd");
            for i in 1..=self.headers.len() {
                write!(out, "\\cellx{}", i * 1800).unwrap()
            }
            out.push_str("\\intbl ");
            for i in 0..self.headers.len() {
                if let Some(text) = cells.get(i) {
                    for ch in text.encode_utf16() {
                        match ch {
                            92 | 123 | 125 => {
                                out.push('\\');
                                out.push(char::from_u32(ch.into()).unwrap());
                            }
                            10 => out.push_str("\\line "),
                            13 => out.push_str("\\u13?"),
                            9 => out.push_str("\\tab "),
                            32..=126 => out.push(char::from_u32(ch.into()).unwrap()),
                            _ => write!(out, "\\u{}?", ch as i16).unwrap(),
                        }
                    }
                }
                out.push_str("\\cell ")
            }
            out.push_str("\\row\n");
        };
        append(&self.headers);
        for r in &self.rows {
            append(&r.values.iter().map(CellValue::display).collect::<Vec<_>>())
        }
        out.push('}');
        out
    }
    pub fn print_document(
        &self,
        options: GridPrintOptions,
        mut cancelled: impl FnMut() -> bool,
    ) -> Result<PrintDocument, ControlError> {
        let GridPrintOptions {
            paper,
            margin,
            row_height,
            range,
        } = options;
        let w = paper.width - 2. * margin;
        let h = paper.height - 2. * margin;
        if !paper.width.is_finite()
            || !paper.height.is_finite()
            || !margin.is_finite()
            || margin < 0.
            || !(16.0..=512.).contains(&row_height)
            || w < 32.
            || h < row_height * (3 + self.bands.len()) as f32
            || paper.width > 19200.
            || paper.height > 19200.
        {
            return Err(error("Invalid grid paper or margins"));
        }
        let rows_per_page = (h / row_height).floor() as usize - 2 - self.bands.len();
        let mut panels = vec![];
        let mut start = 0;
        let mut used = 0.;
        for (i, width) in self.widths.iter().enumerate() {
            let cw = width.min(w);
            if i > start && used + cw > w {
                panels.push(start..i);
                start = i;
                used = 0.;
            }
            used += cw;
        }
        panels.push(start..self.widths.len());
        let total = self.rows.len().max(1).div_ceil(rows_per_page) * panels.len();
        let (first, last) = range.unwrap_or((1, total));
        if first == 0 || last < first || last > total || last - first >= 512 {
            return Err(error("Invalid or excessive grid print page range"));
        }
        let mut pages = Vec::new();
        let mut commands = 0;
        for n in first..=last {
            if cancelled() {
                return Err(error("Grid printing cancelled"));
            }
            let cols = panels[(n - 1) % panels.len()].clone();
            let offset = (n - 1) / panels.len() * rows_per_page;
            let mut scene = Scene::default();
            let mut y = margin;
            let line = |values: &[String], heading: bool, y: f32, scene: &mut Scene| {
                let mut x = margin;
                for col in cols.clone() {
                    let width = self.widths[col].min(w);
                    let r = Rect::new(x, y, width, row_height);
                    if heading {
                        scene.fill(r, Color::rgb(226, 232, 239));
                    }
                    if let Some(label) = values.get(col) {
                        views::draw_text(
                            scene,
                            Rect::new(x + 3., y + 1., (width - 6.).max(0.), row_height - 2.),
                            r,
                            label.clone(),
                            Color::rgb(25, 31, 39),
                            if heading { 600 } else { 400 },
                            13.,
                        );
                    }
                    x += width;
                }
            };
            for band in &self.bands {
                let mut start = cols.start;
                let mut x = margin;
                while start < cols.end {
                    let mut end = start + 1;
                    while end < cols.end && band[end] == band[start] {
                        end += 1
                    }
                    let band_width = self.widths[start..end]
                        .iter()
                        .map(|width| width.min(w))
                        .sum();
                    let area = Rect::new(x, y, band_width, row_height);
                    scene.fill(area, Color::rgb(226, 232, 239));
                    views::draw_text(
                        &mut scene,
                        area,
                        area,
                        band[start].clone(),
                        Color::rgb(25, 31, 39),
                        600,
                        13.,
                    );
                    x += band_width;
                    start = end;
                }
                y += row_height;
            }
            line(&self.headers, true, y, &mut scene);
            y += row_height;
            for row in self.rows.iter().skip(offset).take(rows_per_page) {
                let values = row
                    .values
                    .iter()
                    .map(CellValue::display)
                    .collect::<Vec<_>>();
                if !row.record
                    && row.values.len() == 1
                    && matches!(row.values.first(), Some(CellValue::Text(_)))
                {
                    let rect = Rect::new(margin, y, w, row_height);
                    scene.fill(rect, Color::rgb(226, 232, 239));
                    views::draw_text(
                        &mut scene,
                        rect,
                        rect,
                        values[0].clone(),
                        Color::rgb(25, 31, 39),
                        600,
                        13.,
                    );
                } else {
                    line(&values, !row.record, y, &mut scene);
                }
                y += row_height;
            }
            let footer = Rect::new(margin, paper.height - margin - row_height, w, row_height);
            views::draw_text(
                &mut scene,
                footer,
                footer,
                format!("{n} / {total}"),
                Color::rgb(70, 75, 82),
                400,
                12.,
            );
            commands += scene.commands().len();
            if commands > 100_000 {
                return Err(error("Grid print document command limit"));
            }
            pages.push(PrintPage { size: paper, scene });
        }
        PrintDocument::new("Grid", pages).map_err(error)
    }
}
