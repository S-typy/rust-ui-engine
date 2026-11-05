//! Text-based sparse sizing over the shared worksheet formatter and typography.
use super::*;
use rust_desktop_ui_text::TextEngine;
use std::{collections::BTreeMap, ops::RangeInclusive};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AutoFitAxis {
    Columns,
    Rows,
}

/// Plan one atomic sizing batch without modifying the workbook.
/// Columns measure unwrapped text; rows use the current combined column width.
/// Merged-cell deficits are shared among selected axes only. Outside axes stay
/// fixed. Unrepresentable sizes and unsupported formats return no partial plan.
pub fn auto_fit_edits(
    workbook: &Workbook,
    sheet_index: usize,
    axis: AutoFitAxis,
    selected: RangeInclusive<u32>,
    culture: crate::UiCulture,
    engine: &mut TextEngine,
) -> Result<Vec<WorkbookEdit>, ControlError> {
    let sheet = workbook
        .sheets()
        .get(sheet_index)
        .ok_or_else(|| err("Unknown worksheet"))?;
    let (first, last) = (*selected.start(), *selected.end());
    let limit = if axis == AutoFitAxis::Rows {
        MAX_ROWS
    } else {
        MAX_COLUMNS
    };
    if first > last || last >= limit || u64::from(last - first) + 1 > MAX_DIMENSIONS as u64 {
        return Err(err("Auto-fit selection exceeds worksheet dimension budget"));
    }
    let dimensions = sheet.dimensions();
    let current = |index| {
        if axis == AutoFitAxis::Rows {
            dimensions.row_height(index).expect("validated row")
        } else {
            dimensions.column_width(index).expect("validated column")
        }
    };
    let minimum = if axis == AutoFitAxis::Rows {
        28.0
    } else {
        32.0
    };
    let mut sizes: BTreeMap<u32, f64> = selected.clone().map(|i| (i, minimum)).collect();
    if axis == AutoFitAxis::Columns {
        for (index, size) in &mut sizes {
            let label = CellAddress::new(0, *index).map_err(err)?.a1();
            let run = TextRun::new(
                label.trim_end_matches('1'),
                Rect::new(0.0, 0.0, 4096.0, 28.0),
                Color::rgb(0, 0, 0),
                14.0,
            );
            let extent = engine.prepare(&run, 1.0).map_err(err)?.horizontal_extent();
            *size = size.max(f64::from((extent.end - extent.start).ceil()) + 12.0);
        }
    }
    let mut populated = BTreeSet::new();
    let mut constraints = Vec::new();
    for (address, cell) in sheet.cells() {
        if sheet.anchor(*address) != *address {
            continue;
        }
        let merge = sheet.merges().iter().find(|m| m.start() == *address);
        let end = merge.map_or(*address, |m| m.end());
        let (start, end_axis) = if axis == AutoFitAxis::Rows {
            (address.row(), end.row())
        } else {
            (address.column(), end.column())
        };
        let low = start.max(first);
        let high = end_axis.min(last);
        if low > high {
            continue;
        }
        let display = format_cell(workbook, sheet_index, *address, culture)?;
        if let Some(diagnostic) = display.diagnostic {
            return Err(err(diagnostic));
        }
        if display.text.is_empty() {
            continue;
        }
        populated.extend(low..=high);
        let width = if axis == AutoFitAxis::Rows {
            (address.column()..=end.column())
                .map(|i| dimensions.column_width(i).unwrap())
                .sum::<f64>() as f32
        } else {
            4108.0
        };
        let mut scene = Scene::default();
        let mut style = cell.style.clone();
        if axis == AutoFitAxis::Columns {
            style.wrap = false;
        }
        paint_cell_text(
            &mut scene,
            cell_text_run(
                display.text,
                Rect::new(0.0, 0.0, width, 4096.0),
                Rect::new(0.0, 0.0, width, 4096.0),
                Color::rgb(0, 0, 0),
                1.0,
            ),
            Some(&style),
        )?;
        let run = &scene.texts[0];
        let prepared = engine
            .prepare_styled(run, scene.text_spans(0), 1.0)
            .map_err(err)?;
        let required = if axis == AutoFitAxis::Rows {
            prepared.height + 7.0
        } else {
            let extent = prepared.horizontal_extent();
            extent.end - extent.start + 12.0
        };
        if !required.is_finite() {
            return Err(err("Nonfinite auto-fit measurement"));
        }
        let required = f64::from(required.ceil()).max(minimum);
        if start == end_axis {
            let value = sizes.get_mut(&start).expect("selected extent");
            *value = value.max(required);
        } else {
            constraints.push((start, end_axis, required));
        }
    }
    // Empty axes use the sheet default as their minimum. Nonempty axes can shrink.
    for (index, value) in &mut sizes {
        if !populated.contains(index) {
            let default = if axis == AutoFitAxis::Rows {
                dimensions.default_row_height()
            } else {
                dimensions.default_column_width()
            };
            *value = value.max(default);
        }
    }
    for (start, end, required) in constraints {
        // Sum sparse overrides instead of visiting every unselected axis for
        // each merged-cell constraint.
        let (default, overrides) = if axis == AutoFitAxis::Rows {
            (dimensions.default_row_height(), dimensions.rows())
        } else {
            (dimensions.default_column_width(), dimensions.columns())
        };
        let total = f64::from(end - start + 1) * default
            + overrides
                .range(start..=end)
                .map(|(_, v)| v - default)
                .sum::<f64>()
            + sizes
                .range(start..=end)
                .map(|(i, v)| v - current(*i))
                .sum::<f64>();
        if required > total {
            let members = start.max(first)..=end.min(last);
            let count = u64::from(*members.end() - *members.start()) + 1;
            let extra = ((required - total) / count as f64).ceil();
            for i in members {
                *sizes.get_mut(&i).unwrap() += extra;
            }
        }
    }
    if sizes.values().any(|v| !(1.0..=4096.0).contains(v)) {
        return Err(err(
            "Auto-fit cannot fit content within 4096 logical pixels per axis",
        ));
    }
    Ok(sizes
        .into_iter()
        .filter(|(i, value)| current(*i) != *value)
        .map(|(index, extent)| {
            if axis == AutoFitAxis::Rows {
                WorkbookEdit::RowHeight {
                    sheet: sheet_index,
                    row: index,
                    height: Some(extent),
                }
            } else {
                WorkbookEdit::ColumnWidth {
                    sheet: sheet_index,
                    column: index,
                    width: Some(extent),
                }
            }
        })
        .collect())
}

impl Spreadsheet {
    /// Fit the requested axes in one sizing undo step, after committing a valid
    /// cell draft in its own undo step. Both steps are prepared before publication.
    /// A failed measurement leaves the live workbook, draft and history intact.
    pub fn auto_fit(
        &mut self,
        ui: &mut Controls,
        axis: AutoFitAxis,
        selected: RangeInclusive<u32>,
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
        let edits = auto_fit_edits(
            next.workbook(),
            next.sheet(),
            axis,
            selected,
            self.culture,
            &mut TextEngine::new(),
        )?;
        if !edits.is_empty() {
            next.apply(&edits).map_err(err)?;
        }
        if let Some(bounds) = ui.runtime.bounds(self.parts.canvas) {
            SheetGeometry::new(
                bounds,
                self.first_row,
                self.first_column,
                next.workbook().sheets()[next.sheet()].dimensions(),
            )?;
        }
        self.cancel_interaction(ui)?;
        self.return_editor(ui)?;
        self.session = next;
        self.loaded = text;
        self.refresh(ui)
    }
}
