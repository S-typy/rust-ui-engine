//! Virtual spreadsheet viewport and retained formula editor over Workbook.
use crate::{ClipboardRequest, ControlError, Controls};
use rust_desktop_ui_core::*;
use rust_desktop_ui_data::{
    spreadsheet::{SheetClipboard, SpreadsheetSession},
    workbook::*,
};
use std::collections::BTreeSet;

mod editing;
mod formatting;
mod presentation;
pub use presentation::{CellDisplay, format_cell, format_value};
mod geometry;
mod gestures;
use gestures::{Drag, DragKind};
mod semantics;
use geometry::SheetGeometry;
pub use semantics::{SpreadsheetCell, SpreadsheetHeader, SpreadsheetSemantics};

const HEADER: f32 = 28.0;
const ROW_HEADER: f32 = 60.0;
fn err(e: impl ToString) -> ControlError {
    ControlError::Invalid(e.to_string())
}

pub(crate) fn cell_text_run(
    value: String,
    rect: Rect,
    clip: Rect,
    color: Color,
    scale: f32,
) -> TextRun {
    let mut run = TextRun::new(
        value,
        Rect::new(
            rect.x + 6.0 * scale,
            rect.y + 5.0 * scale,
            (rect.width - 12.0 * scale).max(1.0),
            (rect.height - 7.0 * scale).max(1.0),
        ),
        color,
        14.0 * scale,
    );
    run.clip = clip;
    run
}

/// Static typography shared by the viewport and immutable printed pages.
pub(crate) fn paint_cell_text(
    scene: &mut Scene,
    mut run: TextRun,
    style: Option<&CellStyle>,
) -> Result<(), ControlError> {
    if let Some(style) = style {
        run.weight = if style.bold { 700 } else { 400 };
        run.wrap = style.wrap;
        if let Some(color) = style.foreground {
            run.color = Color::rgb((color >> 16) as u8, (color >> 8) as u8, color as u8);
        }
        if style.italic && !run.text.is_empty() {
            let span = TextSpan {
                range: 0..run.text.len(),
                font: Some(FontDescriptor {
                    size: run.font_size,
                    weight: run.weight,
                    slant: FontSlant::Italic,
                    ..Default::default()
                }),
                font_size: None,
                weight: None,
                color: None,
            };
            return scene.styled_text(run, vec![span]).map_err(err);
        }
    }
    scene.text(run);
    Ok(())
}
#[derive(Clone, Copy, Debug)]
pub struct SpreadsheetParts {
    pub root: WidgetId,
    pub formula: WidgetId,
    pub address: WidgetId,
    pub canvas: WidgetId,
    pub status: WidgetId,
    pub format: WidgetId,
    pub format_apply: WidgetId,
    pub format_code: WidgetId,
    pub format_code_apply: WidgetId,
}
pub struct Spreadsheet {
    pub parts: SpreadsheetParts,
    session: SpreadsheetSession,
    actions: Vec<(WidgetId, &'static str)>,
    clipboard: Option<SheetClipboard>,
    requests: Vec<ClipboardRequest>,
    pending_paste: Option<(u64, usize, CellAddress)>,
    first_row: u32,
    first_column: u32,
    anchor: CellAddress,
    loaded: String,
    drag: Option<Drag>,
    viewport: WidgetId,
    formula_home: (WidgetId, LayoutStyle),
    formula_mirror: WidgetId,
    cell_editing: bool,
    culture: crate::UiCulture,
    last_click: Option<(CellAddress, std::time::Instant)>,
}
impl Spreadsheet {
    pub fn mount(
        ui: &mut Controls,
        parent: WidgetId,
        workbook: Workbook,
        style: LayoutStyle,
    ) -> Result<Self, ControlError> {
        let root = ui.panel(
            parent,
            LayoutStyle {
                kind: LayoutKind::Grid,
                grid_columns: vec![GridTrack::Fr(1.0)],
                grid_rows: vec![
                    GridTrack::Px(36.0),
                    GridTrack::Px(36.0),
                    GridTrack::Px(78.0),
                    GridTrack::Fr(1.0),
                    GridTrack::Px(28.0),
                ],
                gap: 4.0,
                ..style
            },
        )?;
        let result = (|| {
            let row = |n| LayoutStyle {
                grid_row: GridPlacement {
                    start: Some(n),
                    span: 1,
                },
                ..Default::default()
            };
            let toolbar = ui.transparent_panel(
                root,
                LayoutStyle {
                    kind: LayoutKind::Grid,
                    grid_columns: vec![GridTrack::Fr(1.0); 10],
                    gap: 3.0,
                    ..row(1)
                },
            )?;
            let mut actions = Vec::new();
            for (n, (title, action)) in [
                ("Undo", "undo"),
                ("Redo", "redo"),
                ("Merge", "merge"),
                ("Unmerge", "unmerge"),
                ("Fill ↓", "down"),
                ("Fill →", "right"),
                ("Copy", "copy"),
                ("Paste", "paste"),
                ("Sheet −", "previous"),
                ("Sheet +", "next"),
            ]
            .into_iter()
            .enumerate()
            {
                let id = ui.button(
                    toolbar,
                    title,
                    None,
                    LayoutStyle {
                        grid_column: GridPlacement {
                            start: Some(n as u16 + 1),
                            span: 1,
                        },
                        height: Length::Px(32.0),
                        ..Default::default()
                    },
                )?;
                actions.push((id, action));
            }
            let entry = ui.transparent_panel(
                root,
                LayoutStyle {
                    kind: LayoutKind::Grid,
                    grid_columns: vec![
                        GridTrack::Px(100.0),
                        GridTrack::Fr(1.0),
                        GridTrack::Px(76.0),
                    ],
                    gap: 6.0,
                    ..row(2)
                },
            )?;
            let address = ui.label(
                entry,
                "A1",
                LayoutStyle {
                    grid_column: GridPlacement {
                        start: Some(1),
                        span: 1,
                    },
                    ..Default::default()
                },
            )?;
            let formula_host = ui.transparent_panel(
                entry,
                LayoutStyle {
                    kind: LayoutKind::Overlay,
                    grid_column: GridPlacement {
                        start: Some(2),
                        span: 1,
                    },
                    ..Default::default()
                },
            )?;
            let fill = LayoutStyle {
                width: Length::Percent(1.0),
                height: Length::Percent(1.0),
                ..Default::default()
            };
            let formula = ui.text_box(formula_host, "Cell value or formula", "", fill.clone())?;
            let formula_home = (
                formula_host,
                ui.runtime
                    .tree()
                    .node(formula)
                    .expect("mounted editor")
                    .props
                    .style
                    .clone(),
            );
            let formula_mirror = ui.button(formula_host, " ", None, fill)?;
            let mut props = ui
                .runtime
                .tree()
                .node(formula_mirror)
                .expect("mounted mirror")
                .props
                .clone();
            props.visible = false;
            ui.runtime.apply(Mutation::SetProps {
                node: formula_mirror,
                props,
            })?;
            ui.set_automation_name(formula_mirror, "Formula preview")?;
            actions.push((formula_mirror, "formula"));
            let apply = ui.button(
                entry,
                "Apply",
                None,
                LayoutStyle {
                    grid_column: GridPlacement {
                        start: Some(3),
                        span: 1,
                    },
                    ..Default::default()
                },
            )?;
            actions.push((apply, "apply"));
            let formats = ui.transparent_panel(
                root,
                LayoutStyle {
                    kind: LayoutKind::Grid,
                    grid_columns: vec![
                        GridTrack::Px(100.0),
                        GridTrack::Fr(1.0),
                        GridTrack::Px(100.0),
                    ],
                    grid_rows: vec![GridTrack::Px(36.0); 2],
                    gap: 6.0,
                    ..row(3)
                },
            )?;
            ui.label(formats, "Формат", LayoutStyle::default())?;
            let format = ui
                .combo_box(
                    formats,
                    "Формат ячеек",
                    formatting::presets(),
                    crate::ComboBoxSettings::default(),
                    LayoutStyle {
                        width: Length::Percent(1.0),
                        ..Default::default()
                    },
                )?
                .container;
            let format_apply = ui.button(formats, "Применить", None, LayoutStyle::default())?;
            actions.push((format_apply, "format"));
            ui.label(formats, "Код", LayoutStyle::default())?;
            let format_code = ui.text_box(
                formats,
                "Код числового формата",
                "General",
                LayoutStyle {
                    width: Length::Percent(1.0),
                    ..Default::default()
                },
            )?;
            let format_code_apply =
                ui.button(formats, "Применить", None, LayoutStyle::default())?;
            ui.set_automation_name(format_code_apply, "Применить код формата")?;
            actions.push((format_code_apply, "format-code"));
            let viewport = ui.transparent_panel(
                root,
                LayoutStyle {
                    kind: LayoutKind::Overlay,
                    ..row(4)
                },
            )?;
            let canvas = ui.button(
                viewport,
                "",
                None,
                LayoutStyle {
                    width: Length::Percent(1.0),
                    height: Length::Percent(1.0),
                    ..Default::default()
                },
            )?;
            let status = ui.label(root, "", row(5))?;
            let session = SpreadsheetSession::new(workbook);
            let mut value = Self {
                parts: SpreadsheetParts {
                    root,
                    formula,
                    address,
                    canvas,
                    status,
                    format,
                    format_apply,
                    format_code,
                    format_code_apply,
                },
                anchor: session.active(),
                session,
                actions,
                clipboard: None,
                requests: Vec::new(),
                pending_paste: None,
                first_row: 0,
                first_column: 0,
                loaded: String::new(),
                drag: None,
                viewport,
                formula_home,
                formula_mirror,
                cell_editing: false,
                culture: crate::UiCulture::Russian,
                last_click: None,
            };
            value.load_entry(ui)?;
            Ok(value)
        })();
        if result.is_err() {
            let _ = ui.remove(root);
        }
        result
    }
    fn geometry(&self, bounds: Rect) -> Result<SheetGeometry, ControlError> {
        SheetGeometry::with_override(
            bounds,
            self.first_row,
            self.first_column,
            self.session.workbook().sheets()[self.session.sheet()].dimensions(),
            self.drag.and_then(|d| match d.kind {
                DragKind::Resize {
                    row, index, extent, ..
                } => Some((row, index, extent)),
                _ => None,
            }),
        )
    }
    fn apply_dimensions(
        &mut self,
        ui: &mut Controls,
        edit: WorkbookEdit,
    ) -> Result<(), ControlError> {
        self.cancel_interaction(ui)?;
        let mut next = self.session.workbook().clone();
        next.apply(std::slice::from_ref(&edit)).map_err(err)?;
        if let Some(bounds) = ui.runtime.bounds(self.parts.canvas) {
            SheetGeometry::new(
                bounds,
                self.first_row,
                self.first_column,
                next.sheets()[self.session.sheet()].dimensions(),
            )?;
        }
        self.commit(ui)?;
        self.session.apply(&[edit]).map_err(err)?;
        self.refresh(ui)
    }
    /// Change one column width through the same undo history as cell edits.
    pub fn set_column_width(
        &mut self,
        ui: &mut Controls,
        column: u32,
        width: Option<f64>,
    ) -> Result<(), ControlError> {
        self.apply_dimensions(
            ui,
            WorkbookEdit::ColumnWidth {
                sheet: self.session.sheet(),
                column,
                width,
            },
        )
    }
    /// Change one row height; None restores the worksheet default.
    pub fn set_row_height(
        &mut self,
        ui: &mut Controls,
        row: u32,
        height: Option<f64>,
    ) -> Result<(), ControlError> {
        self.apply_dimensions(
            ui,
            WorkbookEdit::RowHeight {
                sheet: self.session.sheet(),
                row,
                height,
            },
        )
    }
    pub fn culture(&self) -> crate::UiCulture {
        self.culture
    }
    pub fn set_culture(
        &mut self,
        ui: &mut Controls,
        culture: crate::UiCulture,
    ) -> Result<(), ControlError> {
        self.culture = culture;
        self.refresh(ui)
    }
    pub fn session(&self) -> &SpreadsheetSession {
        &self.session
    }
    pub fn replace_workbook(
        &mut self,
        ui: &mut Controls,
        workbook: Workbook,
    ) -> Result<(), ControlError> {
        self.guard_composition(ui)?;
        self.cancel_interaction(ui)?;
        self.return_editor(ui)?;
        self.session = SpreadsheetSession::new(workbook);
        self.pending_paste = None;
        self.first_row = 0;
        self.first_column = 0;
        self.anchor = self.session.active();
        self.load_entry(ui)
    }
    pub fn take_clipboard_requests(&mut self) -> Vec<ClipboardRequest> {
        std::mem::take(&mut self.requests)
    }
    pub fn paste_text(&mut self, ui: &mut Controls, text: &str) -> Result<bool, ControlError> {
        let Some((generation, sheet, address)) = self.pending_paste.take() else {
            return Ok(false);
        };
        if (generation, sheet, address)
            != (
                self.session.generation(),
                self.session.sheet(),
                self.session.active(),
            )
        {
            return Err(err("Clipboard target changed before paste completed"));
        }
        self.guard_composition(ui)?;
        if ui
            .document(self.parts.formula)
            .is_some_and(|d| d.text() != self.loaded)
        {
            return Err(err("Clipboard target acquired an uncommitted editor draft"));
        }
        self.session.paste_text(text, false).map_err(err)?;
        self.load_entry(ui)?;
        Ok(true)
    }
    fn guard_composition(&self, ui: &Controls) -> Result<(), ControlError> {
        if [self.parts.formula, self.parts.format_code]
            .into_iter()
            .any(|id| ui.document(id).is_some_and(|d| d.composition().is_some()))
        {
            return Err(err("Finish or cancel text composition first"));
        }
        Ok(())
    }
    pub fn commit(&mut self, ui: &mut Controls) -> Result<(), ControlError> {
        self.cancel_interaction(ui)?;
        self.guard_composition(ui)?;
        let text = ui
            .document(self.parts.formula)
            .ok_or_else(|| err("Formula editor removed"))?
            .text()
            .to_owned();
        if text != self.loaded {
            self.session.enter(&text).map_err(err)?;
            self.loaded = text;
        }
        self.return_editor(ui)?;
        Ok(())
    }
    fn load_entry(&mut self, ui: &mut Controls) -> Result<(), ControlError> {
        self.loaded = self.session.entry();
        ui.set_editor_value(self.parts.formula, &self.loaded)?;
        ui.set_label(self.parts.address, self.session.active().a1())?;
        self.load_format(ui)?;
        Ok(())
    }
    pub fn activate(&mut self, ui: &mut Controls, id: WidgetId) -> Result<bool, ControlError> {
        if id == self.parts.canvas {
            return Ok(true);
        }
        let Some(action) = self
            .actions
            .iter()
            .find_map(|(i, a)| (*i == id).then_some(*a))
        else {
            return Ok(false);
        };
        self.command(ui, action)?;
        Ok(true)
    }
    fn command(&mut self, ui: &mut Controls, action: &str) -> Result<(), ControlError> {
        if action == "format-code" {
            return self.apply_format_code(ui);
        }
        if action == "format" {
            let format = ui
                .selected_keys(self.parts.format)
                .and_then(|keys| keys.first())
                .cloned()
                .ok_or_else(|| err("Выберите формат ячеек"))?;
            return self.set_number_format(ui, &format);
        }
        if action == "formula" {
            return self.edit_formula(ui);
        }
        self.cancel_interaction(ui)?;
        self.commit(ui)?;
        match action {
            "apply" => {}
            "undo" => {
                self.session.undo().map_err(err)?;
            }
            "redo" => {
                self.session.redo().map_err(err)?;
            }
            "merge" => self.session.merge().map_err(err)?,
            "unmerge" => self.session.unmerge().map_err(err)?,
            "down" => self.session.fill(true).map_err(err)?,
            "right" => self.session.fill(false).map_err(err)?,
            "delete" => self.session.clear().map_err(err)?,
            "copy" => {
                self.clipboard = Some(self.session.copy());
                self.requests.push(ClipboardRequest::Copy(
                    self.session.copy_text().map_err(err)?,
                ));
            }
            "paste" => {
                self.pending_paste = Some((
                    self.session.generation(),
                    self.session.sheet(),
                    self.session.active(),
                ));
                self.requests.push(ClipboardRequest::Paste);
            }
            "paste-typed" => {
                if let Some(payload) = &self.clipboard {
                    self.session
                        .paste(payload, self.session.active())
                        .map_err(err)?;
                }
            }
            "previous" | "next" => {
                let count = self.session.workbook().sheets().len();
                let index = if action == "next" {
                    (self.session.sheet() + 1) % count
                } else {
                    (self.session.sheet() + count - 1) % count
                };
                self.session.select_sheet(index).map_err(err)?;
                self.first_row = 0;
                self.first_column = 0;
            }
            _ => return Ok(()),
        }
        self.anchor = self.session.active();
        self.load_entry(ui)
    }
    /// Intercept before Controls::dispatch. Native text and IME remain with the
    /// formula editor; canvas navigation changes only logical selection.
    pub fn input(&mut self, ui: &mut Controls, event: &InputEvent) -> Result<bool, ControlError> {
        if ui.runtime.focused() == Some(self.parts.format_code)
            && ui.is_enabled(self.parts.format_code)
            && let InputEvent::KeyDown { key, .. } = event
            && ui
                .document(self.parts.format_code)
                .is_none_or(|d| d.composition().is_none())
        {
            match key {
                Key::Enter => {
                    self.apply_format_code(ui)?;
                    return Ok(true);
                }
                Key::Escape => {
                    self.load_format(ui)?;
                    return Ok(true);
                }
                _ => {}
            }
        }
        if self.gesture_input(ui, event)? {
            return Ok(true);
        }
        if !ui.is_enabled(self.parts.canvas) {
            return Ok(false);
        }
        let Some(bounds) = ui.runtime.bounds(self.parts.canvas) else {
            return Ok(false);
        };
        match event {
            InputEvent::Wheel {
                position,
                delta,
                modifiers,
            } if bounds.contains(position.x, position.y) => {
                if !delta.is_finite() {
                    return Ok(false);
                }
                let steps = |value: f64| {
                    if value > 0.0 {
                        3
                    } else if value < 0.0 {
                        -3
                    } else {
                        0
                    }
                };
                let (columns, rows) = if modifiers.shift {
                    (steps(f64::from(delta.x) + f64::from(delta.y)), 0)
                } else {
                    (steps(f64::from(delta.x)), steps(f64::from(delta.y)))
                };
                if columns == 0 && rows == 0 {
                    return Ok(false);
                }
                self.commit(ui)?;
                self.first_column = self
                    .first_column
                    .saturating_add_signed(columns)
                    .min(MAX_COLUMNS - 1);
                self.first_row = self.first_row.saturating_add_signed(rows).min(MAX_ROWS - 1);
                Ok(true)
            }
            InputEvent::KeyDown { key, modifiers, .. }
                if ui.runtime.focused() == Some(self.parts.formula) =>
            {
                if ui
                    .document(self.parts.formula)
                    .is_some_and(|d| d.composition().is_some())
                {
                    return Ok(false);
                }
                match key {
                    Key::Enter => {
                        self.commit(ui)?;
                        ui.runtime.apply(Mutation::Focus(Some(self.parts.canvas)))?;
                        Ok(true)
                    }
                    Key::Escape => {
                        self.load_entry(ui)?;
                        self.return_editor(ui)?;
                        ui.runtime.apply(Mutation::Focus(Some(self.parts.canvas)))?;
                        Ok(true)
                    }
                    _ => {
                        let _ = modifiers;
                        Ok(false)
                    }
                }
            }
            InputEvent::KeyDown { key, modifiers, .. }
                if ui.runtime.focused() == Some(self.parts.canvas) =>
            {
                if let Key::Character(c) = key
                    && (modifiers.control || modifiers.super_key)
                {
                    let action = match c.to_ascii_lowercase().as_str() {
                        "c" => "copy",
                        "v" if modifiers.shift => "paste-typed",
                        "v" => "paste",
                        "z" if modifiers.shift => "redo",
                        "z" => "undo",
                        "y" => "redo",
                        "d" => "down",
                        "r" => "right",
                        _ => return Ok(false),
                    };
                    self.command(ui, action)?;
                    return Ok(true);
                }
                let current = self.session.active();
                let (dr, dc) = match key {
                    Key::ArrowUp => (-1, 0),
                    Key::ArrowDown => (1, 0),
                    Key::ArrowLeft => (0, -1),
                    Key::ArrowRight => (0, 1),
                    Key::PageUp => (-20, 0),
                    Key::PageDown => (20, 0),
                    Key::Home => {
                        self.first_column = 0;
                        (0, -(current.column() as i32))
                    }
                    Key::Delete => {
                        self.command(ui, "delete")?;
                        return Ok(true);
                    }
                    Key::F2 | Key::Enter => {
                        self.begin_cell_edit(ui)?;
                        return Ok(true);
                    }
                    _ => return Ok(false),
                };
                let merge = self.session.workbook().sheets()[self.session.sheet()]
                    .merges()
                    .iter()
                    .find(|r| r.contains(current));
                let base = merge.map_or(current, |r| {
                    CellAddress::new(
                        if dr > 0 { r.end().row() } else { current.row() },
                        if dc > 0 {
                            r.end().column()
                        } else {
                            current.column()
                        },
                    )
                    .expect("existing merge coordinates")
                });
                let next = CellAddress::new(
                    base.row().saturating_add_signed(dr).min(MAX_ROWS - 1),
                    base.column().saturating_add_signed(dc).min(MAX_COLUMNS - 1),
                )
                .map_err(err)?;
                if !modifiers.shift {
                    self.anchor = next;
                }
                self.session.select(self.anchor, next).map_err(err)?;
                (self.first_row, self.first_column) = self.geometry(bounds)?.reveal(next);
                self.load_entry(ui)?;
                Ok(true)
            }
            _ => Ok(false),
        }
    }
    pub fn refresh(&mut self, ui: &mut Controls) -> Result<(), ControlError> {
        self.validate_interaction(ui)?;
        self.position_editor(ui)?;
        let Some(bounds) = ui.runtime.bounds(self.parts.canvas) else {
            return Ok(());
        };
        let tokens = ui.theme().tokens();
        let mut scene = Scene::default();
        scene.fill(bounds, tokens.surface);
        let geometry = self.geometry(bounds)?;
        let body = geometry.body;
        let text = |scene: &mut Scene, label: String, rect: Rect, color: Color| {
            if let Some(clip) = rect.intersection(bounds) {
                let mut run = TextRun::new(
                    label,
                    Rect::new(
                        rect.x + 6.0,
                        rect.y + 5.0,
                        (rect.width - 12.0).max(1.0),
                        22.0,
                    ),
                    color,
                    14.0,
                );
                run.clip = clip;
                scene.text(run);
            }
        };
        for col in geometry.visible_columns.clone() {
            let a = CellAddress::new(0, col).map_err(err)?.a1();
            text(
                &mut scene,
                a.trim_end_matches('1').into(),
                geometry.column_header(col),
                tokens.muted,
            );
        }
        let sheet = &self.session.workbook().sheets()[self.session.sheet()];
        let mut painted = BTreeSet::new();
        let mut diagnostics = BTreeSet::new();
        for row in geometry.visible_rows.clone() {
            text(
                &mut scene,
                (row + 1).to_string(),
                geometry.row_header(row),
                tokens.muted,
            );
            for col in geometry.visible_columns.clone() {
                let a = CellAddress::new(row, col).map_err(err)?;
                let anchor = sheet.anchor(a);
                if !painted.insert(anchor) {
                    continue;
                }
                let range = sheet
                    .merges()
                    .iter()
                    .find(|r| r.contains(a))
                    .copied()
                    .unwrap_or(CellRange::new(a, a).map_err(err)?);
                let rect = geometry.range(range);
                let Some(clip) = rect.intersection(body) else {
                    continue;
                };
                if self.session.selection().intersects(range) {
                    scene.fill(clip, tokens.selected);
                } else if let Some(color) = sheet.cell(anchor).and_then(|c| c.style.background) {
                    scene.fill(
                        clip,
                        Color::rgb((color >> 16) as u8, (color >> 8) as u8, color as u8),
                    );
                }
                scene.rounded_stroke(
                    rect,
                    body,
                    0.0,
                    if anchor == self.session.active() {
                        2.0
                    } else {
                        0.5
                    },
                    if anchor == self.session.active() {
                        tokens.focus
                    } else {
                        tokens.border
                    },
                );
                let display = format_cell(
                    self.session.workbook(),
                    self.session.sheet(),
                    anchor,
                    self.culture(),
                )?;
                if let Some(message) = display.diagnostic {
                    diagnostics.insert(message);
                }
                let run = cell_text_run(display.text, rect, clip, tokens.text, 1.0);
                let mut style = sheet
                    .cell(anchor)
                    .map(|c| c.style.clone())
                    .unwrap_or_default();
                if let Some(color) = display.foreground {
                    style.foreground = Some(color);
                }
                paint_cell_text(&mut scene, run, Some(&style))?;
            }
        }
        if let Some(Drag {
            kind: DragKind::Fill { target, .. },
            ..
        }) = self.drag
        {
            scene.rounded_stroke(geometry.range(target), body, 0.0, 2.0, tokens.focus);
        }
        if let Some(handle) = geometry.fill_handle(self.session.selection())
            && let Some(clip) = handle.intersection(body)
        {
            scene.fill(clip, tokens.focus);
        }
        let range = self.session.selection();
        ui.set_label(
            self.parts.status,
            format!(
                "{} · {}:{} · {} cells · Enter/F2: edit · Shift+arrows: range",
                sheet.name(),
                range.start().a1(),
                range.end().a1(),
                range.len()
            ),
        )?;
        if let Some(message) = diagnostics.iter().next() {
            ui.set_label(
                self.parts.status,
                format!("{} · {}", self.session.active().a1(), message),
            )?;
        }
        ui.set_automation_name(
            self.parts.canvas,
            format!(
                "Worksheet {}. Selected {}. Value {}. Range {}:{}",
                sheet.name(),
                self.session.active().a1(),
                format_cell(
                    self.session.workbook(),
                    self.session.sheet(),
                    self.session.active(),
                    self.culture()
                )?
                .text,
                range.start().a1(),
                range.end().a1()
            ),
        )?;
        ui.set_custom_scene(self.parts.canvas, scene)?;
        for (id, action) in &self.actions {
            if *action == "undo" {
                ui.set_enabled(*id, self.session.can_undo())?;
            }
            if *action == "redo" {
                ui.set_enabled(*id, self.session.can_redo())?;
            }
        }
        Ok(())
    }
}
