//! Retained PDF viewing and print preview over one immutable page snapshot.
use crate::{
    ControlError, Controls, EditorSettings,
    pdf::{PdfDocument, PdfMatch},
};
use rust_desktop_ui_core::{
    pages::{PrintDocument, transform_page},
    *,
};
fn err(e: impl ToString) -> ControlError {
    ControlError::Invalid(e.to_string())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PageLoadToken(u64);
#[derive(Clone, Copy, Debug)]
pub struct PageViewerParts {
    pub root: WidgetId,
    pub canvas: WidgetId,
    pub status: WidgetId,
    pub search: WidgetId,
    pub page_number: WidgetId,
    pub page_text: WidgetId,
    pub field_value: WidgetId,
}
pub struct PageViewer {
    pub parts: PageViewerParts,
    document: PrintDocument,
    pdf: Option<PdfDocument>,
    page: usize,
    zoom: Option<f32>,
    offset: Point,
    actions: Vec<(WidgetId, &'static str)>,
    thumbnails: Vec<WidgetId>,
    matches: Vec<PdfMatch>,
    hit: usize,
    field: usize,
    bookmark: usize,
    epoch: u64,
    pending: Option<PageLoadToken>,
    last_text_page: Option<usize>,
    field_loaded: String,
}
impl PageViewer {
    pub fn pdf(
        ui: &mut Controls,
        parent: WidgetId,
        document: PdfDocument,
        style: LayoutStyle,
    ) -> Result<Self, ControlError> {
        let mut viewer =
            Self::print_preview(ui, parent, document.print_document().map_err(err)?, style)?;
        viewer.pdf = Some(document);
        viewer.last_text_page = None;
        viewer.load_field(ui)?;
        viewer.update_text(ui)?;
        Ok(viewer)
    }
    pub fn print_preview(
        ui: &mut Controls,
        parent: WidgetId,
        document: PrintDocument,
        style: LayoutStyle,
    ) -> Result<Self, ControlError> {
        let root = ui.panel(
            parent,
            LayoutStyle {
                kind: LayoutKind::Grid,
                grid_columns: vec![GridTrack::Fr(1.0)],
                grid_rows: vec![
                    GridTrack::Px(36.0),
                    GridTrack::Fr(1.0),
                    GridTrack::Px(38.0),
                    GridTrack::Px(40.0),
                    GridTrack::Px(30.0),
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
                    grid_columns: vec![
                        GridTrack::Px(50.0),
                        GridTrack::Px(50.0),
                        GridTrack::Px(60.0),
                        GridTrack::Px(52.0),
                        GridTrack::Px(46.0),
                        GridTrack::Px(46.0),
                        GridTrack::Px(48.0),
                        GridTrack::Fr(1.0),
                        GridTrack::Px(64.0),
                        GridTrack::Px(88.0),
                    ],
                    gap: 3.0,
                    ..row(1)
                },
            )?;
            let mut actions = Vec::new();
            for (column, title, action) in [
                (1, "←", "previous"),
                (2, "→", "next"),
                (4, "Go", "goto"),
                (5, "−", "zoom-out"),
                (6, "+", "zoom-in"),
                (7, "Fit", "fit"),
                (9, "Find", "find"),
                (10, "Bookmark", "bookmark"),
            ] {
                let id = ui.button(
                    toolbar,
                    title,
                    None,
                    LayoutStyle {
                        grid_column: GridPlacement {
                            start: Some(column),
                            span: 1,
                        },
                        grid_row: GridPlacement {
                            start: Some(1),
                            span: 1,
                        },
                        ..Default::default()
                    },
                )?;
                actions.push((id, action));
            }
            let page_number = ui.text_box(
                toolbar,
                "Page number",
                "1",
                LayoutStyle {
                    grid_column: GridPlacement {
                        start: Some(3),
                        span: 1,
                    },
                    grid_row: GridPlacement {
                        start: Some(1),
                        span: 1,
                    },
                    ..Default::default()
                },
            )?;
            let search = ui.text_box(
                toolbar,
                "Search document",
                "",
                LayoutStyle {
                    grid_column: GridPlacement {
                        start: Some(8),
                        span: 1,
                    },
                    grid_row: GridPlacement {
                        start: Some(1),
                        span: 1,
                    },
                    ..Default::default()
                },
            )?;
            let body = ui.transparent_panel(
                root,
                LayoutStyle {
                    kind: LayoutKind::Grid,
                    grid_columns: vec![GridTrack::Px(126.0), GridTrack::Fr(1.0)],
                    grid_rows: vec![GridTrack::Fr(1.0)],
                    gap: 8.0,
                    ..row(2)
                },
            )?;
            let sidebar = ui.transparent_panel(
                body,
                LayoutStyle {
                    kind: LayoutKind::Grid,
                    grid_rows: vec![GridTrack::Fr(1.0); 4],
                    grid_columns: vec![GridTrack::Fr(1.0)],
                    grid_column: GridPlacement {
                        start: Some(1),
                        span: 1,
                    },
                    gap: 6.0,
                    ..Default::default()
                },
            )?;
            let mut thumbnails = Vec::new();
            for index in 0..4 {
                thumbnails.push(ui.button(
                    sidebar,
                    "",
                    None,
                    LayoutStyle {
                        grid_row: GridPlacement {
                            start: Some(index + 1),
                            span: 1,
                        },
                        height: Length::Percent(1.0),
                        ..Default::default()
                    },
                )?);
            }
            let canvas = ui.button(
                body,
                "",
                None,
                LayoutStyle {
                    grid_column: GridPlacement {
                        start: Some(2),
                        span: 1,
                    },
                    grid_row: GridPlacement {
                        start: Some(1),
                        span: 1,
                    },
                    width: Length::Percent(1.0),
                    height: Length::Percent(1.0),
                    ..Default::default()
                },
            )?;
            let fields = ui.transparent_panel(
                root,
                LayoutStyle {
                    kind: LayoutKind::Grid,
                    grid_columns: vec![
                        GridTrack::Px(100.0),
                        GridTrack::Fr(1.0),
                        GridTrack::Px(100.0),
                        GridTrack::Px(100.0),
                    ],
                    gap: 4.0,
                    ..row(3)
                },
            )?;
            let field_value = ui.text_box(
                fields,
                "Field value or annotation text",
                "",
                LayoutStyle {
                    grid_column: GridPlacement {
                        start: Some(2),
                        span: 1,
                    },
                    grid_row: GridPlacement {
                        start: Some(1),
                        span: 1,
                    },
                    ..Default::default()
                },
            )?;
            for (column, title, action) in [
                (1, "Next field", "field-next"),
                (3, "Apply field", "field-apply"),
                (4, "Add note", "note"),
            ] {
                let id = ui.button(
                    fields,
                    title,
                    None,
                    LayoutStyle {
                        grid_column: GridPlacement {
                            start: Some(column),
                            span: 1,
                        },
                        grid_row: GridPlacement {
                            start: Some(1),
                            span: 1,
                        },
                        ..Default::default()
                    },
                )?;
                actions.push((id, action));
            }
            let page_text = ui.text_box(root, "Extracted page text", "", row(4))?;
            ui.set_editor_settings(
                page_text,
                EditorSettings {
                    is_read_only: true,
                    ..Default::default()
                },
            )?;
            let status = ui.label(root, "", row(5))?;
            let mut viewer = Self {
                parts: PageViewerParts {
                    root,
                    canvas,
                    status,
                    search,
                    page_number,
                    page_text,
                    field_value,
                },
                document,
                pdf: None,
                page: 0,
                zoom: None,
                offset: Point::ZERO,
                actions,
                thumbnails,
                matches: Vec::new(),
                hit: 0,
                field: 0,
                bookmark: 0,
                epoch: 0,
                pending: None,
                last_text_page: None,
                field_loaded: String::new(),
            };
            viewer.update_text(ui)?;
            Ok(viewer)
        })();
        if result.is_err() {
            let _ = ui.remove(root);
        }
        result
    }
    pub fn document(&self) -> &PrintDocument {
        &self.document
    }
    pub fn pdf_document(&self) -> Option<&PdfDocument> {
        self.pdf.as_ref()
    }
    pub fn page(&self) -> usize {
        self.page
    }
    pub fn begin_load(&mut self) -> Result<PageLoadToken, ControlError> {
        self.bump()?;
        let token = PageLoadToken(self.epoch);
        self.pending = Some(token);
        Ok(token)
    }
    pub fn complete_load(
        &mut self,
        ui: &mut Controls,
        token: PageLoadToken,
        pdf: PdfDocument,
    ) -> Result<bool, ControlError> {
        if self.pending != Some(token) {
            return Ok(false);
        }
        self.guard_field(ui)?;
        let document = pdf.print_document().map_err(err)?;
        self.bump()?;
        self.pdf = Some(pdf);
        self.document = document;
        self.page = 0;
        self.field = 0;
        self.bookmark = 0;
        self.offset = Point::ZERO;
        self.matches.clear();
        self.last_text_page = None;
        self.load_field(ui)?;
        self.update_text(ui)?;
        Ok(true)
    }
    fn bump(&mut self) -> Result<(), ControlError> {
        self.epoch = self
            .epoch
            .checked_add(1)
            .ok_or_else(|| err("Viewer generation exhausted"))?;
        self.pending = None;
        Ok(())
    }
    fn guard_field(&self, ui: &Controls) -> Result<(), ControlError> {
        if ui
            .document(self.parts.field_value)
            .is_some_and(|d| d.composition().is_some() || d.text() != self.field_loaded)
        {
            return Err(err("Apply or cancel the field/note draft first"));
        }
        Ok(())
    }
    fn load_field(&mut self, ui: &mut Controls) -> Result<(), ControlError> {
        self.field_loaded = self
            .pdf
            .as_ref()
            .and_then(|p| p.fields().get(self.field))
            .map_or(String::new(), |f| f.value.clone());
        ui.set_editor_value(self.parts.field_value, &self.field_loaded)?;
        Ok(())
    }
    pub fn set_page(&mut self, ui: &mut Controls, page: usize) -> Result<(), ControlError> {
        if page >= self.document.pages().len() {
            return Err(err("Page outside document"));
        }
        self.page = page;
        self.offset = Point::ZERO;
        ui.set_editor_value(self.parts.page_number, &(page + 1).to_string())?;
        self.update_text(ui)
    }
    fn update_text(&mut self, ui: &mut Controls) -> Result<(), ControlError> {
        if self.last_text_page == Some(self.page) {
            return Ok(());
        }
        let text = self.pdf.as_ref().map_or_else(
            || {
                self.document.pages()[self.page]
                    .scene
                    .texts
                    .iter()
                    .map(|t| t.text.as_str())
                    .collect::<Vec<_>>()
                    .join(" ")
            },
            |p| {
                p.pages()[self.page]
                    .text
                    .iter()
                    .map(|t| t.text.as_str())
                    .collect::<Vec<_>>()
                    .join(" ")
            },
        );
        let mut text = text.replace(['\n', '\r'], " ");
        if text.len() > 1024 * 1024 {
            let mut end = 1024 * 1024 - 100;
            while !text.is_char_boundary(end) {
                end -= 1;
            }
            text.truncate(end);
            text.push_str(
                " […] Extracted-text preview truncated; document and search remain intact.",
            );
        }
        ui.set_editor_value(self.parts.page_text, &text)?;
        ui.set_editor_value(self.parts.page_number, &(self.page + 1).to_string())?;
        self.last_text_page = Some(self.page);
        Ok(())
    }
    pub fn activate(&mut self, ui: &mut Controls, id: WidgetId) -> Result<bool, ControlError> {
        if let Some(index) = self.thumbnails.iter().position(|i| *i == id) {
            let page = self.page / 4 * 4 + index;
            if page < self.document.pages().len() {
                self.set_page(ui, page)?;
            }
            return Ok(true);
        }
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
        match action {
            "previous" => self.set_page(ui, self.page.saturating_sub(1))?,
            "next" => self.set_page(ui, (self.page + 1).min(self.document.pages().len() - 1))?,
            "goto" => {
                let page = ui
                    .document(self.parts.page_number)
                    .unwrap()
                    .text()
                    .parse::<usize>()
                    .map_err(err)?;
                self.set_page(
                    ui,
                    page.checked_sub(1).ok_or_else(|| err("Pages start at 1"))?,
                )?;
            }
            "zoom-in" => self.zoom = Some((self.zoom.unwrap_or(1.0) * 1.25).min(4.0)),
            "zoom-out" => self.zoom = Some((self.zoom.unwrap_or(1.0) / 1.25).max(0.1)),
            "fit" => {
                self.zoom = None;
                self.offset = Point::ZERO;
            }
            "find" => {
                let needle = ui.document(self.parts.search).unwrap().text();
                if let Some(pdf) = &self.pdf {
                    let hits = pdf.search(needle).map_err(err)?;
                    self.hit = if hits.len() == self.matches.len()
                        && !hits.is_empty()
                        && hits.iter().zip(&self.matches).all(|(a, b)| {
                            a.page == b.page && a.item == b.item && a.byte_range == b.byte_range
                        }) {
                        (self.hit + 1) % hits.len()
                    } else {
                        0
                    };
                    self.matches = hits;
                    if let Some(hit) = self.matches.get(self.hit) {
                        self.set_page(ui, hit.page)?;
                    }
                }
            }
            "bookmark" => {
                if let Some(pdf) = &self.pdf
                    && !pdf.bookmarks().is_empty()
                {
                    let page = pdf.bookmarks()[self.bookmark % pdf.bookmarks().len()].page;
                    self.bookmark += 1;
                    self.set_page(ui, page)?;
                }
            }
            "field-next" => {
                self.guard_field(ui)?;
                if let Some(pdf) = &self.pdf
                    && !pdf.fields().is_empty()
                {
                    self.field = (self.field + 1) % pdf.fields().len();
                    let f = pdf.fields()[self.field].clone();
                    self.set_page(ui, f.page)?;
                    ui.set_editor_value(self.parts.field_value, &f.value)?;
                    self.field_loaded = f.value;
                    ui.set_automation_name(
                        self.parts.field_value,
                        format!("PDF field {}", f.name),
                    )?;
                }
            }
            "field-apply" | "note" => {
                if ui
                    .document(self.parts.field_value)
                    .is_some_and(|d| d.composition().is_some())
                {
                    return Err(err("Finish composition first"));
                }
                let value = ui
                    .document(self.parts.field_value)
                    .unwrap()
                    .text()
                    .to_owned();
                let mut next = self
                    .pdf
                    .clone()
                    .ok_or_else(|| err("This preview is not an editable PDF"))?;
                if action == "field-apply" {
                    next.set_field(self.field, &value).map_err(err)?;
                } else {
                    next.add_annotation(self.page, Rect::new(20.0, 20.0, 24.0, 24.0), &value)
                        .map_err(err)?;
                }
                let document = next.print_document().map_err(err)?;
                self.bump()?;
                self.pdf = Some(next);
                self.document = document;
                self.field_loaded = value;
                ui.set_editor_value(self.parts.field_value, &self.field_loaded)?;
                self.last_text_page = None;
            }
            _ => {}
        }
        self.update_text(ui)?;
        Ok(true)
    }
    pub fn input(&mut self, ui: &mut Controls, event: &InputEvent) -> Result<bool, ControlError> {
        let Some(bounds) = ui.runtime.bounds(self.parts.canvas) else {
            return Ok(false);
        };
        match event {
            InputEvent::Wheel {
                position,
                delta,
                modifiers,
            } if bounds.contains(position.x, position.y) => {
                if modifiers.control {
                    let scale = if delta.y > 0.0 { 1.1 } else { 1.0 / 1.1 };
                    self.zoom = Some((self.zoom.unwrap_or(1.0) * scale).clamp(0.1, 4.0));
                } else if modifiers.shift {
                    self.offset.x = (self.offset.x - delta.y).max(0.0);
                } else {
                    self.offset.y = (self.offset.y - delta.y).max(0.0);
                }
                Ok(true)
            }
            InputEvent::KeyDown { key, .. } if ui.runtime.focused() == Some(self.parts.canvas) => {
                match key {
                    Key::PageDown => {
                        self.set_page(ui, (self.page + 1).min(self.document.pages().len() - 1))?
                    }
                    Key::PageUp => self.set_page(ui, self.page.saturating_sub(1))?,
                    Key::Home => self.set_page(ui, 0)?,
                    Key::End => self.set_page(ui, self.document.pages().len() - 1)?,
                    Key::ArrowDown => self.offset.y += 36.0,
                    Key::ArrowUp => self.offset.y = (self.offset.y - 36.0).max(0.0),
                    Key::ArrowRight => self.offset.x += 36.0,
                    Key::ArrowLeft => self.offset.x = (self.offset.x - 36.0).max(0.0),
                    _ => return Ok(false),
                }
                Ok(true)
            }
            _ => Ok(false),
        }
    }
    pub fn refresh(&mut self, ui: &mut Controls) -> Result<(), ControlError> {
        self.update_text(ui)?;
        let tokens = ui.theme().tokens();
        if let Some(bounds) = ui.runtime.bounds(self.parts.canvas) {
            let page = &self.document.pages()[self.page];
            let fit = ((bounds.width - 16.0) / page.size.width)
                .min((bounds.height - 16.0) / page.size.height)
                .clamp(0.01, 4.0);
            let scale = self.zoom.unwrap_or(fit);
            self.offset.x = self
                .offset
                .x
                .min((page.size.width * scale - bounds.width + 16.0).max(0.0));
            self.offset.y = self
                .offset
                .y
                .min((page.size.height * scale - bounds.height + 16.0).max(0.0));
            let origin = Point::new(
                bounds.x + 8.0 + (bounds.width - 16.0 - page.size.width * scale).max(0.0) / 2.0
                    - self.offset.x,
                bounds.y + 8.0 - self.offset.y,
            );
            let mut scene = Scene::default();
            scene.fill(bounds, tokens.canvas);
            scene
                .append_view(
                    &transform_page(&page.scene, scale, origin, bounds).map_err(err)?,
                    Point::ZERO,
                    bounds,
                )
                .map_err(err)?;
            if let Some(hit) = self.matches.get(self.hit).filter(|h| h.page == self.page) {
                let r = hit.bounds;
                scene.rounded_stroke(
                    Rect::new(
                        origin.x + r.x * scale,
                        origin.y + r.y * scale,
                        r.width * scale,
                        r.height * scale,
                    ),
                    bounds,
                    0.0,
                    2.0,
                    tokens.focus,
                );
            }
            if let Some(pdf) = &self.pdf {
                for a in pdf.annotations().iter().filter(|a| a.page == self.page) {
                    scene.rounded_fill(
                        Rect::new(
                            origin.x + a.bounds.x * scale,
                            origin.y + a.bounds.y * scale,
                            24.0 * scale,
                            24.0 * scale,
                        ),
                        bounds,
                        3.0,
                        tokens.selected,
                    );
                }
            }
            ui.set_custom_scene(self.parts.canvas, scene)?;
        }
        for (index, id) in self.thumbnails.iter().enumerate() {
            let number = self.page / 4 * 4 + index;
            ui.set_enabled(*id, number < self.document.pages().len())?;
            if let Some(bounds) = ui.runtime.bounds(*id) {
                let mut scene = Scene::default();
                scene.fill(bounds, tokens.canvas);
                if let Some(page) = self.document.pages().get(number) {
                    let scale = ((bounds.width - 8.0) / page.size.width)
                        .min((bounds.height - 8.0) / page.size.height)
                        .clamp(0.01, 1.0);
                    scene
                        .append_view(
                            &transform_page(
                                &page.scene,
                                scale,
                                Point::new(bounds.x + 4.0, bounds.y + 4.0),
                                bounds,
                            )
                            .map_err(err)?,
                            Point::ZERO,
                            bounds,
                        )
                        .map_err(err)?;
                    if number == self.page {
                        scene.rounded_stroke(bounds, bounds, 0.0, 2.0, tokens.focus);
                    }
                }
                ui.set_custom_scene(*id, scene)?;
                ui.set_automation_name(*id, format!("Page {} thumbnail", number + 1))?;
            }
        }
        let warnings = self.pdf.as_ref().map_or(0, |p| p.diagnostics().len());
        ui.set_label(
            self.parts.status,
            format!(
                "Page {}/{} · {} search matches · {} diagnostics",
                self.page + 1,
                self.document.pages().len(),
                self.matches.len(),
                warnings
            ),
        )?;
        ui.set_automation_name(
            self.parts.canvas,
            format!(
                "Document page {} of {}. Extracted text is available below the page.",
                self.page + 1,
                self.document.pages().len()
            ),
        )?;
        for (id, action) in &self.actions {
            if matches!(*action, "field-next" | "field-apply") {
                ui.set_enabled(
                    *id,
                    self.pdf.as_ref().is_some_and(|p| !p.fields().is_empty()),
                )?;
            }
            if matches!(*action, "note" | "find" | "bookmark") {
                ui.set_enabled(*id, self.pdf.is_some())?;
            }
        }
        Ok(())
    }
}
