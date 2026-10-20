//! Virtual spreadsheet viewport and retained formula editor over Workbook.
use crate::{ClipboardRequest, ControlError, Controls};
use rust_desktop_ui_core::*;
use rust_desktop_ui_data::{
    spreadsheet::{SheetClipboard, SpreadsheetSession},
    workbook::*,
};
use std::collections::BTreeSet;

const HEADER: f32 = 28.0;
const ROW: f32 = 28.0;
const COLUMN: f32 = 112.0;
const ROW_HEADER: f32 = 60.0;
fn err(e: impl ToString) -> ControlError {
    ControlError::Invalid(e.to_string())
}
#[derive(Clone, Copy, Debug)]
pub struct SpreadsheetParts {
    pub root: WidgetId,
    pub formula: WidgetId,
    pub address: WidgetId,
    pub canvas: WidgetId,
    pub status: WidgetId,
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
            let formula = ui.text_box(
                entry,
                "Cell value or formula",
                "",
                LayoutStyle {
                    grid_column: GridPlacement {
                        start: Some(2),
                        span: 1,
                    },
                    ..Default::default()
                },
            )?;
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
            let canvas = ui.button(
                root,
                "",
                None,
                LayoutStyle {
                    width: Length::Percent(1.0),
                    height: Length::Percent(1.0),
                    ..row(3)
                },
            )?;
            let status = ui.label(root, "", row(4))?;
            let session = SpreadsheetSession::new(workbook);
            let mut value = Self {
                parts: SpreadsheetParts {
                    root,
                    formula,
                    address,
                    canvas,
                    status,
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
            };
            value.load_entry(ui)?;
            Ok(value)
        })();
        if result.is_err() {
            let _ = ui.remove(root);
        }
        result
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
        self.session.paste_text(text, false).map_err(err)?;
        self.load_entry(ui)?;
        Ok(true)
    }
    fn guard_composition(&self, ui: &Controls) -> Result<(), ControlError> {
        if ui
            .document(self.parts.formula)
            .is_some_and(|d| d.composition().is_some())
        {
            return Err(err("Finish or cancel text composition first"));
        }
        Ok(())
    }
    pub fn commit(&mut self, ui: &mut Controls) -> Result<(), ControlError> {
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
        Ok(())
    }
    fn load_entry(&mut self, ui: &mut Controls) -> Result<(), ControlError> {
        self.loaded = self.session.entry();
        ui.set_editor_value(self.parts.formula, &self.loaded)?;
        ui.set_label(self.parts.address, self.session.active().a1())?;
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
        let Some(bounds) = ui.runtime.bounds(self.parts.canvas) else {
            return Ok(false);
        };
        match event {
            InputEvent::PointerDown {
                position,
                button: PointerButton::Primary,
                modifiers,
            } if bounds.contains(position.x, position.y) => {
                let x = position.x - bounds.x - ROW_HEADER;
                let y = position.y - bounds.y - HEADER;
                if x >= 0.0 && y >= 0.0 {
                    self.commit(ui)?;
                    let address = CellAddress::new(
                        (self.first_row + (y / ROW) as u32).min(MAX_ROWS - 1),
                        (self.first_column + (x / COLUMN) as u32).min(MAX_COLUMNS - 1),
                    )
                    .map_err(err)?;
                    if !modifiers.shift {
                        self.anchor = address;
                    }
                    self.session.select(self.anchor, address).map_err(err)?;
                    ui.runtime.apply(Mutation::Focus(Some(self.parts.canvas)))?;
                    self.load_entry(ui)?;
                }
                Ok(true)
            }
            InputEvent::Wheel {
                position,
                delta,
                modifiers,
            } if bounds.contains(position.x, position.y) => {
                let step = if delta.y < 0.0 { 3 } else { -3 };
                if modifiers.shift {
                    self.first_column = self
                        .first_column
                        .saturating_add_signed(step)
                        .min(MAX_COLUMNS - 1);
                } else {
                    self.first_row = self.first_row.saturating_add_signed(step).min(MAX_ROWS - 1);
                }
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
                        ui.runtime
                            .apply(Mutation::Focus(Some(self.parts.formula)))?;
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
                let rows = ((bounds.height - HEADER) / ROW).floor().max(1.0) as u32;
                let cols = ((bounds.width - ROW_HEADER) / COLUMN).floor().max(1.0) as u32;
                self.first_row = self
                    .first_row
                    .min(next.row())
                    .max(next.row().saturating_sub(rows - 1));
                self.first_column = self
                    .first_column
                    .min(next.column())
                    .max(next.column().saturating_sub(cols - 1));
                self.load_entry(ui)?;
                Ok(true)
            }
            _ => Ok(false),
        }
    }
    pub fn refresh(&mut self, ui: &mut Controls) -> Result<(), ControlError> {
        let Some(bounds) = ui.runtime.bounds(self.parts.canvas) else {
            return Ok(());
        };
        let tokens = ui.theme().tokens();
        let mut scene = Scene::default();
        scene.fill(bounds, tokens.surface);
        let body = Rect::new(
            bounds.x + ROW_HEADER,
            bounds.y + HEADER,
            (bounds.width - ROW_HEADER).max(0.0),
            (bounds.height - HEADER).max(0.0),
        );
        let rows = (body.height / ROW).ceil().clamp(0.0, 100.0) as u32;
        let cols = (body.width / COLUMN).ceil().clamp(0.0, 64.0) as u32;
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
        for col in 0..cols {
            if self.first_column + col >= MAX_COLUMNS {
                break;
            }
            let a = CellAddress::new(0, self.first_column + col)
                .map_err(err)?
                .a1();
            text(
                &mut scene,
                a.trim_end_matches('1').into(),
                Rect::new(body.x + col as f32 * COLUMN, bounds.y, COLUMN, HEADER),
                tokens.muted,
            );
        }
        let sheet = &self.session.workbook().sheets()[self.session.sheet()];
        let mut painted = BTreeSet::new();
        for row in 0..rows {
            if self.first_row + row >= MAX_ROWS {
                break;
            }
            text(
                &mut scene,
                (self.first_row + row + 1).to_string(),
                Rect::new(bounds.x, body.y + row as f32 * ROW, ROW_HEADER, ROW),
                tokens.muted,
            );
            for col in 0..cols {
                if self.first_column + col >= MAX_COLUMNS {
                    break;
                }
                let a =
                    CellAddress::new(self.first_row + row, self.first_column + col).map_err(err)?;
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
                let rect = Rect::new(
                    body.x
                        + (range.start().column() as f64 - self.first_column as f64) as f32
                            * COLUMN,
                    body.y + (range.start().row() as f64 - self.first_row as f64) as f32 * ROW,
                    (range.end().column() - range.start().column() + 1) as f32 * COLUMN,
                    (range.end().row() - range.start().row() + 1) as f32 * ROW,
                );
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
                let value = self
                    .session
                    .workbook()
                    .value(self.session.sheet(), anchor)
                    .map_err(err)?
                    .display();
                let mut run = TextRun::new(
                    value,
                    Rect::new(
                        rect.x + 6.0,
                        rect.y + 5.0,
                        (rect.width - 12.0).max(1.0),
                        (rect.height - 7.0).max(1.0),
                    ),
                    tokens.text,
                    14.0,
                );
                run.clip = clip;
                if let Some(cell) = sheet.cell(anchor) {
                    run.weight = if cell.style.bold { 700 } else { 400 };
                    run.wrap = cell.style.wrap;
                    if let Some(color) = cell.style.foreground {
                        run.color =
                            Color::rgb((color >> 16) as u8, (color >> 8) as u8, color as u8);
                    }
                    if cell.style.italic && !run.text.is_empty() {
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
                        scene.styled_text(run, vec![span]).map_err(err)?;
                        continue;
                    }
                }
                scene.text(run);
            }
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
        ui.set_automation_name(
            self.parts.canvas,
            format!(
                "Worksheet {}. Selected {}. Value {}. Range {}:{}",
                sheet.name(),
                self.session.active().a1(),
                self.session
                    .workbook()
                    .value(self.session.sheet(), self.session.active())
                    .map_err(err)?
                    .display(),
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
