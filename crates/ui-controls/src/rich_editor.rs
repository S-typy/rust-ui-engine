//! Retained rich-document editor with a paragraph editing surface and a
//! paginated formatted preview. The code view edits the validated JSON model.
//! Native text input, clipboard and IME are delegated to the existing TextBox.
use crate::rich_document::*;
use crate::{ControlError, Controls};
use rust_desktop_ui_core::{
    Axis, DrawCommand, GridPlacement, GridTrack, InputEvent, Key, LayoutKind, LayoutStyle, Length,
    Rect, Scene, WidgetId,
};
use rust_desktop_ui_text::{Selection, TextDocument, TextEngine};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RichEditorView {
    Document,
    Code,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RichFormat {
    Bold,
    Italic,
    Underline,
}
#[derive(Clone, Copy, Debug)]
pub struct RichEditorParts {
    pub container: WidgetId,
    pub editor: WidgetId,
    pub preview: WidgetId,
    pub status: WidgetId,
    pub previous: WidgetId,
    pub next: WidgetId,
    pub bold: WidgetId,
    pub italic: WidgetId,
    pub underline: WidgetId,
    pub undo: WidgetId,
    pub redo: WidgetId,
    pub code: WidgetId,
    pub apply: WidgetId,
    pub previous_page: WidgetId,
    pub next_page: WidgetId,
}
#[derive(Clone, Debug)]
struct Snapshot {
    document: RichDocument,
    paragraph: usize,
    selection: Selection,
}
#[derive(Clone, Copy, Debug)]
enum Address {
    Body(usize),
    Cell(usize, usize, usize),
}

pub struct RichEditor {
    pub parts: RichEditorParts,
    document: RichDocument,
    paragraph: usize,
    view: RichEditorView,
    last_text: String,
    last_selection: Selection,
    typing_style: RunStyle,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    layout: DocumentLayout,
    preview_page: usize,
    engine: TextEngine,
    revision: u64,
}
impl RichEditor {
    pub fn mount(
        ui: &mut Controls,
        parent: WidgetId,
        document: RichDocument,
        style: LayoutStyle,
    ) -> Result<Self, ControlError> {
        document.validate().map_err(control_error)?;
        if addresses(&document).is_empty() {
            return Err(control_error(DocumentError::Invalid(
                "editor needs an editable paragraph".into(),
            )));
        }
        let container = ui.panel(
            parent,
            LayoutStyle {
                kind: LayoutKind::Grid,
                grid_columns: vec![GridTrack::Fr(1.0)],
                grid_rows: vec![GridTrack::Px(42.0), GridTrack::Fr(1.0), GridTrack::Px(32.0)],
                gap: 8.0,
                ..style
            },
        )?;
        let result = (|| {
            let toolbar = ui.transparent_panel(
                container,
                LayoutStyle {
                    kind: LayoutKind::Stack(Axis::Horizontal),
                    gap: 4.0,
                    grid_row: GridPlacement {
                        start: Some(1),
                        span: 1,
                    },
                    ..Default::default()
                },
            )?;
            let mut button = |label: &str, width: f32| {
                ui.button(
                    toolbar,
                    label,
                    None,
                    LayoutStyle {
                        width: Length::Px(width),
                        height: Length::Px(36.0),
                        ..Default::default()
                    },
                )
            };
            let previous = button("←", 34.0)?;
            let next = button("→", 34.0)?;
            let bold = button("B", 34.0)?;
            let italic = button("I", 34.0)?;
            let underline = button("U", 34.0)?;
            let undo = button("Undo", 64.0)?;
            let redo = button("Redo", 64.0)?;
            let code = button("Code", 64.0)?;
            let apply = button("Apply", 64.0)?;
            let previous_page = button("Page −", 74.0)?;
            let next_page = button("Page +", 74.0)?;
            let body = ui.transparent_panel(
                container,
                LayoutStyle {
                    kind: LayoutKind::Grid,
                    grid_columns: vec![GridTrack::Fr(0.42), GridTrack::Fr(0.58)],
                    grid_rows: vec![GridTrack::Fr(1.0)],
                    gap: 12.0,
                    grid_row: GridPlacement {
                        start: Some(2),
                        span: 1,
                    },
                    ..Default::default()
                },
            )?;
            let editor = ui.text_box(
                body,
                "Document paragraph",
                paragraph_at(&document, 0).unwrap().plain_text(),
                LayoutStyle {
                    width: Length::Percent(1.0),
                    height: Length::Percent(1.0),
                    grid_column: GridPlacement {
                        start: Some(1),
                        span: 1,
                    },
                    ..Default::default()
                },
            )?;
            let preview = ui.panel(
                body,
                LayoutStyle {
                    width: Length::Percent(1.0),
                    height: Length::Percent(1.0),
                    grid_column: GridPlacement {
                        start: Some(2),
                        span: 1,
                    },
                    ..Default::default()
                },
            )?;
            ui.set_automation_name(preview, "Formatted document preview")?;
            let status = ui.label(
                container,
                "",
                LayoutStyle {
                    grid_row: GridPlacement {
                        start: Some(3),
                        span: 1,
                    },
                    ..Default::default()
                },
            )?;
            for (id, name) in [
                (previous, "Previous paragraph"),
                (next, "Next paragraph"),
                (bold, "Bold selection"),
                (italic, "Italic selection"),
                (underline, "Underline selection"),
            ] {
                ui.set_automation_name(id, name)?;
            }
            let parts = RichEditorParts {
                container,
                editor,
                preview,
                status,
                previous,
                next,
                bold,
                italic,
                underline,
                undo,
                redo,
                code,
                apply,
                previous_page,
                next_page,
            };
            let mut engine = TextEngine::new();
            let layout = document.paginate(&mut engine).map_err(control_error)?;
            let last_text = paragraph_at(&document, 0).unwrap().plain_text();
            let last_selection = Selection {
                anchor: last_text.len(),
                focus: last_text.len(),
            };
            let typing_style = paragraph_at(&document, 0)
                .unwrap()
                .style_at(last_selection.focus);
            let mut value = Self {
                parts,
                document,
                paragraph: 0,
                view: RichEditorView::Document,
                last_text,
                last_selection,
                typing_style,
                undo: Vec::new(),
                redo: Vec::new(),
                layout,
                preview_page: 0,
                engine,
                revision: 0,
            };
            value.refresh(ui)?;
            Ok(value)
        })();
        if result.is_err() {
            let _ = ui.remove(container);
        }
        result
    }
    pub fn document(&self) -> &RichDocument {
        &self.document
    }
    pub fn view(&self) -> RichEditorView {
        self.view
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn paragraph_index(&self) -> usize {
        self.paragraph
    }
    pub fn page_count(&self) -> usize {
        self.layout.pages.len()
    }
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
    fn require_committed(&self, ui: &Controls) -> Result<(), ControlError> {
        if ui
            .document(self.parts.editor)
            .is_some_and(|d| d.composition().is_some())
        {
            Err(control_error(DocumentError::CompositionActive))
        } else {
            Ok(())
        }
    }
    fn snapshot(&self) -> Snapshot {
        Snapshot {
            document: self.document.clone(),
            paragraph: self.paragraph,
            selection: self.last_selection,
        }
    }
    fn remember(&mut self, before: Snapshot) {
        self.undo.push(before);
        self.redo.clear();
        trim_history(&mut self.undo, &mut self.redo);
        self.revision = self.revision.wrapping_add(1);
    }
    fn repaginate(&mut self) -> Result<(), ControlError> {
        self.layout = self
            .document
            .paginate(&mut self.engine)
            .map_err(control_error)?;
        self.preview_page = self.preview_page.min(self.layout.pages.len() - 1);
        Ok(())
    }
    /// Observe text committed by the native editor. Preedit never changes the
    /// model. Formatting outside the changed grapheme range remains intact.
    pub fn sync(&mut self, ui: &mut Controls) -> Result<bool, ControlError> {
        let host = ui
            .document(self.parts.editor)
            .ok_or_else(|| ControlError::Invalid("Missing document editor".into()))?;
        if self.view == RichEditorView::Code || host.composition().is_some() {
            return Ok(false);
        }
        let current = host.text().to_owned();
        let selection = host.selection();
        if current == self.last_text {
            if selection != self.last_selection {
                self.typing_style = paragraph_at(&self.document, self.paragraph)
                    .unwrap()
                    .style_at(selection.focus);
            }
            self.last_selection = selection;
            return Ok(false);
        }
        let (range, replacement) = text_difference(&self.last_text, &current);
        let before = self.snapshot();
        let mut candidate = self.document.clone();
        paragraph_mut(&mut candidate, self.paragraph)
            .ok_or_else(|| ControlError::Invalid("Missing paragraph".into()))?
            .replace(range, replacement, self.typing_style.clone())
            .map_err(control_error)?;
        candidate.validate().map_err(control_error)?;
        let layout = candidate
            .paginate(&mut self.engine)
            .map_err(control_error)?;
        self.document = candidate;
        self.layout = layout;
        self.remember(before);
        self.last_text = current;
        self.last_selection = selection;
        self.refresh(ui)?;
        Ok(true)
    }
    pub fn format(&mut self, ui: &mut Controls, format: RichFormat) -> Result<(), ControlError> {
        self.require_committed(ui)?;
        self.sync(ui)?;
        if self.view != RichEditorView::Document {
            return Err(ControlError::Invalid(
                "Formatting requires document view".into(),
            ));
        }
        let selection = ui.document(self.parts.editor).unwrap().selection();
        let p = paragraph_at(&self.document, self.paragraph).unwrap();
        let mut style = p.style_at(selection.range().start);
        match format {
            RichFormat::Bold => style.bold = !style.bold,
            RichFormat::Italic => style.italic = !style.italic,
            RichFormat::Underline => style.underline = !style.underline,
        };
        self.typing_style = style.clone();
        if !selection.is_empty() {
            let before = self.snapshot();
            let mut candidate = self.document.clone();
            paragraph_mut(&mut candidate, self.paragraph)
                .unwrap()
                .map_styles(selection.range(), |current| match format {
                    RichFormat::Bold => current.bold = style.bold,
                    RichFormat::Italic => current.italic = style.italic,
                    RichFormat::Underline => current.underline = style.underline,
                })
                .map_err(control_error)?;
            let layout = candidate
                .paginate(&mut self.engine)
                .map_err(control_error)?;
            self.document = candidate;
            self.layout = layout;
            self.remember(before);
        }
        self.refresh(ui)
    }
    pub fn select_paragraph(
        &mut self,
        ui: &mut Controls,
        index: usize,
    ) -> Result<(), ControlError> {
        self.require_committed(ui)?;
        self.sync(ui)?;
        if self.view != RichEditorView::Document {
            return Err(ControlError::Invalid(
                "Apply or cancel code view first".into(),
            ));
        }
        if paragraph_at(&self.document, index).is_none() {
            return Err(ControlError::Invalid("Paragraph index".into()));
        }
        self.paragraph = index;
        self.last_selection = Selection::default();
        self.load_host(ui)?;
        self.refresh(ui)
    }
    pub fn set_view(
        &mut self,
        ui: &mut Controls,
        view: RichEditorView,
    ) -> Result<(), ControlError> {
        self.require_committed(ui)?;
        self.sync(ui)?;
        if view == self.view {
            return Ok(());
        }
        if view == RichEditorView::Code {
            ui.set_editor_value(
                self.parts.editor,
                &self.document.to_json().map_err(control_error)?,
            )?;
            ui.set_automation_name(self.parts.editor, "Rich document JSON code")?;
        } else {
            self.load_host(ui)?;
        }
        self.view = view;
        self.refresh(ui)
    }
    /// Code changes replace the model only after JSON, references and complete
    /// pagination succeed. On error the draft and accepted document both remain.
    pub fn apply_code(&mut self, ui: &mut Controls) -> Result<(), ControlError> {
        self.require_committed(ui)?;
        if self.view != RichEditorView::Code {
            return Ok(());
        }
        let candidate = RichDocument::from_json(ui.document(self.parts.editor).unwrap().text())
            .map_err(control_error)?;
        if addresses(&candidate).is_empty() {
            return Err(ControlError::Invalid("Editor needs a paragraph".into()));
        }
        let layout = candidate
            .paginate(&mut self.engine)
            .map_err(control_error)?;
        let before = self.snapshot();
        self.document = candidate;
        self.layout = layout;
        self.remember(before);
        self.paragraph = self.paragraph.min(addresses(&self.document).len() - 1);
        self.last_selection = Selection::default();
        self.view = RichEditorView::Document;
        self.load_host(ui)?;
        self.refresh(ui)
    }
    /// Load imported content as one undoable operation; external I/O is app-owned.
    pub fn replace_document(
        &mut self,
        ui: &mut Controls,
        document: RichDocument,
    ) -> Result<(), ControlError> {
        self.require_document_view()?;
        self.require_committed(ui)?;
        self.sync(ui)?;
        document.validate().map_err(control_error)?;
        if addresses(&document).is_empty() {
            return Err(ControlError::Invalid("Editor needs a paragraph".into()));
        }
        let layout = document.paginate(&mut self.engine).map_err(control_error)?;
        let before = self.snapshot();
        self.document = document;
        self.layout = layout;
        self.remember(before);
        self.paragraph = 0;
        self.last_selection = Selection::default();
        self.view = RichEditorView::Document;
        self.load_host(ui)?;
        self.refresh(ui)
    }
    pub fn undo(&mut self, ui: &mut Controls) -> Result<bool, ControlError> {
        self.require_document_view()?;
        self.require_committed(ui)?;
        self.sync(ui)?;
        let Some(previous) = self.undo.pop() else {
            return Ok(false);
        };
        let current = self.snapshot();
        self.restore(ui, previous)?;
        self.redo.push(current);
        trim_history(&mut self.undo, &mut self.redo);
        self.refresh(ui)?;
        Ok(true)
    }
    pub fn redo(&mut self, ui: &mut Controls) -> Result<bool, ControlError> {
        self.require_document_view()?;
        self.require_committed(ui)?;
        self.sync(ui)?;
        let Some(next) = self.redo.pop() else {
            return Ok(false);
        };
        let current = self.snapshot();
        self.restore(ui, next)?;
        self.undo.push(current);
        trim_history(&mut self.undo, &mut self.redo);
        self.refresh(ui)?;
        Ok(true)
    }
    fn restore(&mut self, ui: &mut Controls, snapshot: Snapshot) -> Result<(), ControlError> {
        self.document = snapshot.document;
        self.paragraph = snapshot.paragraph;
        self.last_selection = snapshot.selection;
        self.view = RichEditorView::Document;
        self.repaginate()?;
        self.revision = self.revision.wrapping_add(1);
        // The caller publishes toolbar availability after transferring the
        // snapshot between both history stacks.
        self.load_host(ui)
    }
    fn load_host(&mut self, ui: &mut Controls) -> Result<(), ControlError> {
        self.last_text = paragraph_at(&self.document, self.paragraph)
            .unwrap()
            .plain_text();
        ui.set_editor_value(self.parts.editor, &self.last_text)?;
        let selection = self.last_selection;
        if selection.anchor <= self.last_text.len() && selection.focus <= self.last_text.len() {
            ui.set_editor_selection(self.parts.editor, selection.anchor, selection.focus)?;
        }
        self.typing_style = paragraph_at(&self.document, self.paragraph)
            .unwrap()
            .style_at(selection.focus);
        ui.set_automation_name(
            self.parts.editor,
            format!("Document paragraph {}", self.paragraph + 1),
        )?;
        Ok(())
    }
    /// Call before Controls::dispatch. Other events continue through the shared
    /// editor, including selection, clipboard, composition and accessibility.
    pub fn handle_event(
        &mut self,
        ui: &mut Controls,
        event: &InputEvent,
    ) -> Result<bool, ControlError> {
        if ui.runtime.focused() != Some(self.parts.editor) {
            return Ok(false);
        }
        // Document editing has no business-value rollback on Escape. Only the
        // transient composition is canceled; document undo is explicit.
        if matches!(
            event,
            InputEvent::KeyDown {
                key: Key::Escape,
                ..
            }
        ) {
            ui.cancel_preedit();
            return Ok(true);
        }
        if self.view == RichEditorView::Code {
            return Ok(false);
        }
        if let InputEvent::KeyDown {
            key: Key::Character(key),
            modifiers,
            ..
        } = event
            && (modifiers.control || modifiers.super_key)
            && !modifiers.alt
        {
            match key.to_lowercase().as_str() {
                "z" => {
                    if modifiers.shift {
                        self.redo(ui)?;
                    } else {
                        self.undo(ui)?;
                    }
                }
                "y" => {
                    self.redo(ui)?;
                }
                "b" => self.format(ui, RichFormat::Bold)?,
                "i" => self.format(ui, RichFormat::Italic)?,
                "u" => self.format(ui, RichFormat::Underline)?,
                _ => return Ok(false),
            }
            return Ok(true);
        }
        Ok(false)
    }
    /// Pass each activated widget from the application queue; unrelated IDs are
    /// left to other components. The component never drains shared event queues.
    pub fn activate(&mut self, ui: &mut Controls, id: WidgetId) -> Result<bool, ControlError> {
        let p = self.parts;
        let edits = [
            p.bold,
            p.italic,
            p.underline,
            p.previous,
            p.next,
            p.undo,
            p.redo,
            p.code,
            p.apply,
        ];
        if !edits.contains(&id) && id != p.previous_page && id != p.next_page {
            return Ok(false);
        }
        let action = (|| -> Result<(), ControlError> {
            if id == p.bold {
                self.format(ui, RichFormat::Bold)?;
            } else if id == p.italic {
                self.format(ui, RichFormat::Italic)?;
            } else if id == p.underline {
                self.format(ui, RichFormat::Underline)?;
            } else if id == p.previous {
                self.select_paragraph(ui, self.paragraph.saturating_sub(1))?;
            } else if id == p.next {
                self.select_paragraph(
                    ui,
                    (self.paragraph + 1).min(addresses(&self.document).len() - 1),
                )?;
            } else if id == p.undo {
                self.undo(ui)?;
            } else if id == p.redo {
                self.redo(ui)?;
            } else if id == p.code {
                self.set_view(
                    ui,
                    if self.view == RichEditorView::Code {
                        RichEditorView::Document
                    } else {
                        RichEditorView::Code
                    },
                )?;
            } else if id == p.apply {
                self.apply_code(ui)?;
            } else if id == p.previous_page {
                self.preview_page = self.preview_page.saturating_sub(1);
                self.refresh(ui)?;
            } else if id == p.next_page {
                self.preview_page = (self.preview_page + 1).min(self.layout.pages.len() - 1);
                self.refresh(ui)?;
            }
            Ok(())
        })();
        // A toolbar click transfers focus to its button. Return it to the text
        // surface for editing actions, including a failed Code Apply, so typing
        // continues in the retained selection/draft. Page browsing keeps focus.
        if edits.contains(&id) && ui.is_enabled(p.editor) {
            ui.runtime
                .apply(rust_desktop_ui_core::Mutation::Focus(Some(p.editor)))?;
        }
        action?;
        Ok(true)
    }
    fn require_document_view(&self) -> Result<(), ControlError> {
        if self.view != RichEditorView::Document {
            return Err(ControlError::Invalid(
                "Apply or cancel the Code draft before changing the document".into(),
            ));
        }
        Ok(())
    }
    /// Refresh after the normal layout pass so preview geometry follows resize.
    pub fn refresh(&mut self, ui: &mut Controls) -> Result<(), ControlError> {
        self.preview_page = self.preview_page.min(self.layout.pages.len() - 1);
        ui.set_label(
            self.parts.status,
            format!(
                "Paragraph {}/{} · page {}/{} · {} · {} missing glyphs",
                self.paragraph + 1,
                addresses(&self.document).len(),
                self.preview_page + 1,
                self.layout.pages.len(),
                if self.view == RichEditorView::Code {
                    "JSON draft"
                } else {
                    "Document"
                },
                self.layout.missing_glyphs
            ),
        )?;
        let document_view = self.view == RichEditorView::Document;
        ui.set_label(
            self.parts.code,
            if document_view { "Code" } else { "Cancel" },
        )?;
        ui.set_automation_name(
            self.parts.code,
            if document_view {
                "Open document code view"
            } else {
                "Cancel code draft and return to document"
            },
        )?;
        ui.set_enabled(self.parts.undo, document_view && !self.undo.is_empty())?;
        ui.set_enabled(self.parts.redo, document_view && !self.redo.is_empty())?;
        for id in [
            self.parts.bold,
            self.parts.italic,
            self.parts.underline,
            self.parts.previous,
            self.parts.next,
        ] {
            ui.set_enabled(id, document_view)?;
        }
        ui.set_enabled(self.parts.apply, self.view == RichEditorView::Code)?;
        if let Some(bounds) = ui.runtime.bounds(self.parts.preview) {
            let scale = ((bounds.width - 12.0) / self.document.page.width)
                .min((bounds.height - 12.0) / self.document.page.height)
                .clamp(0.01, 1.0);
            let x = bounds.x + (bounds.width - self.document.page.width * scale) * 0.5;
            let y = bounds.y + 6.0;
            ui.set_custom_scene(
                self.parts.preview,
                transform_scene(&self.layout.pages[self.preview_page].scene, scale, x, y)?,
            )?;
        }
        Ok(())
    }
}
fn control_error(e: DocumentError) -> ControlError {
    ControlError::Invalid(e.to_string())
}
fn trim_history(undo: &mut Vec<Snapshot>, redo: &mut Vec<Snapshot>) {
    while undo.len() + redo.len() > 64
        || undo
            .iter()
            .chain(redo.iter())
            .map(|s| {
                s.document
                    .media
                    .values()
                    .map(|m| m.bytes.len())
                    .sum::<usize>()
                    + s.document
                        .blocks
                        .iter()
                        .map(|block| match block {
                            RichBlock::Paragraph(p) => {
                                p.runs.iter().map(|r| r.text.len() + 256).sum::<usize>()
                            }
                            RichBlock::Table(t) => t
                                .rows
                                .iter()
                                .flatten()
                                .map(|p| p.runs.iter().map(|r| r.text.len() + 256).sum::<usize>())
                                .sum(),
                            RichBlock::Image(i) => i.alt.len(),
                        })
                        .sum::<usize>()
                    + s.document.blocks.len() * 128
            })
            .sum::<usize>()
            > 64 * 1024 * 1024
    {
        if !undo.is_empty() {
            undo.remove(0);
        } else {
            redo.remove(0);
        }
    }
}
fn addresses(document: &RichDocument) -> Vec<Address> {
    let mut result = Vec::new();
    for (i, b) in document.blocks.iter().enumerate() {
        match b {
            RichBlock::Paragraph(_) => result.push(Address::Body(i)),
            RichBlock::Table(t) => {
                for (r, row) in t.rows.iter().enumerate() {
                    for c in 0..row.len() {
                        result.push(Address::Cell(i, r, c));
                    }
                }
            }
            _ => {}
        }
    }
    result
}
fn paragraph_at(document: &RichDocument, index: usize) -> Option<&Paragraph> {
    match *addresses(document).get(index)? {
        Address::Body(i) => match &document.blocks[i] {
            RichBlock::Paragraph(p) => Some(p),
            _ => None,
        },
        Address::Cell(i, r, c) => match &document.blocks[i] {
            RichBlock::Table(t) => Some(&t.rows[r][c]),
            _ => None,
        },
    }
}
fn paragraph_mut(document: &mut RichDocument, index: usize) -> Option<&mut Paragraph> {
    match *addresses(document).get(index)? {
        Address::Body(i) => match &mut document.blocks[i] {
            RichBlock::Paragraph(p) => Some(p),
            _ => None,
        },
        Address::Cell(i, r, c) => match &mut document.blocks[i] {
            RichBlock::Table(t) => Some(&mut t.rows[r][c]),
            _ => None,
        },
    }
}
fn text_difference<'a>(old: &str, new: &'a str) -> (std::ops::Range<usize>, &'a str) {
    let a = TextDocument::new(old).grapheme_boundaries();
    let b = TextDocument::new(new).grapheme_boundaries();
    let mut prefix = 0;
    while prefix + 1 < a.len()
        && prefix + 1 < b.len()
        && old[a[prefix]..a[prefix + 1]] == new[b[prefix]..b[prefix + 1]]
    {
        prefix += 1;
    }
    let (mut ai, mut bi) = (a.len() - 1, b.len() - 1);
    while ai > prefix && bi > prefix && old[a[ai - 1]..a[ai]] == new[b[bi - 1]..b[bi]] {
        ai -= 1;
        bi -= 1;
    }
    (a[prefix]..a[ai], &new[b[prefix]..b[bi]])
}
fn transform_scene(source: &Scene, scale: f32, x: f32, y: f32) -> Result<Scene, ControlError> {
    if !scale.is_finite() || scale <= 0.0 || !x.is_finite() || !y.is_finite() {
        return Err(ControlError::Invalid(
            "Invalid document preview transform".into(),
        ));
    }
    let rect = |r: Rect| {
        Rect::new(
            x + r.x * scale,
            y + r.y * scale,
            r.width * scale,
            r.height * scale,
        )
    };
    let mut out = Scene::default();
    for command in source.commands() {
        match *command {
            DrawCommand::Rectangle(i) => {
                let r = &source.rectangles[i];
                out.fill(rect(r.bounds), r.color);
            }
            DrawCommand::RoundedRectangle(i) => {
                let r = &source.rounded_rectangles[i];
                if let Some(width) = r.stroke_width {
                    out.rounded_stroke(
                        rect(r.bounds),
                        rect(r.clip),
                        r.radius * scale,
                        width * scale,
                        r.color,
                    );
                } else {
                    out.rounded_fill(rect(r.bounds), rect(r.clip), r.radius * scale, r.color);
                }
            }
            DrawCommand::Text(i) => {
                let mut run = source.texts[i].clone();
                run.bounds = rect(run.bounds);
                run.clip = rect(run.clip);
                run.font_size *= scale;
                let mut spans = source.text_spans(i).to_vec();
                for s in &mut spans {
                    if let Some(font) = &mut s.font {
                        font.size *= scale;
                    }
                    if let Some(size) = &mut s.font_size {
                        *size *= scale;
                    }
                }
                out.styled_text(run, spans)
                    .map_err(|error| ControlError::Invalid(error.into()))?;
            }
            DrawCommand::Image(i) => {
                let mut image = source.images[i].clone();
                image.bounds = rect(image.bounds);
                image.clip = rect(image.clip);
                out.image(image);
            }
            DrawCommand::Ellipse(i) => {
                let r = &source.ellipses[i];
                out.ellipse(
                    rect(r.bounds),
                    rect(r.clip),
                    r.stroke_width.map(|w| w * scale),
                    r.color,
                );
            }
            DrawCommand::StrokePath(_) => {}
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests;
