//! Explicit DOCX I/O around the retained rich-document editor.
use crate::form::Form;
use rust_desktop_ui_controls::rich_document::{
    DocumentMedia, DocxImport, Paragraph, RichBlock, RichDocument, RichImage, RichTable,
};
use rust_desktop_ui_controls::rich_editor::RichEditor;
use rust_desktop_ui_core::{LayoutStyle, Length, Rect, Scene, Size, WidgetId};
use rust_desktop_ui_platform_winit::{
    self as platform, DesktopApp, DialogCompletion, DialogOptions, DialogOutcome, DialogRequest,
    EventResponse, FileDialog, FileDialogFilter, FileDialogKind, PlatformEvent, accesskit,
};
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, Receiver, SyncSender},
    },
    time::{Duration, Instant},
};

enum Pending {
    Open { revision: u64 },
    Save { document: RichDocument },
}
enum Completion {
    Open {
        revision: u64,
        result: Result<DocxImport, String>,
    },
    Save(Result<(), String>),
}
pub(crate) struct RichDocumentDemo {
    pub(crate) form: Form,
    editor: RichEditor,
    open: WidgetId,
    save: WidgetId,
    paragraph: WidgetId,
    table: WidgetId,
    image: WidgetId,
    pending: Option<(u64, Pending)>,
    request: Option<DialogRequest>,
    next: u64,
    tx: SyncSender<Completion>,
    rx: Receiver<Completion>,
    busy: bool,
    alive: Arc<AtomicBool>,
    waker: Option<platform::AppWaker>,
    diagnostics: bool,
    last: String,
}
fn err(e: impl ToString) -> String {
    e.to_string()
}
impl RichDocumentDemo {
    pub(crate) fn new(
        definition: rust_desktop_ui_xaml::WindowDefinition,
        diagnostics: bool,
    ) -> Result<Self, String> {
        let mut form = Form::new(definition, false)?;
        let host = form
            .find_name("RichDocumentHost")
            .ok_or("Rich document host missing")?;
        let editor = RichEditor::mount(
            &mut form.ui,
            host,
            crate::rich_document_model::sample_document().map_err(err)?,
            LayoutStyle {
                width: Length::Percent(1.0),
                height: Length::Percent(1.0),
                ..Default::default()
            },
        )
        .map_err(err)?;
        let actions = form
            .find_name("RichFileActions")
            .ok_or("Rich action host missing")?;
        let mut button = |label: &str| {
            form.ui
                .button(
                    actions,
                    label,
                    None,
                    LayoutStyle {
                        width: Length::Px(160.0),
                        height: Length::Px(36.0),
                        ..Default::default()
                    },
                )
                .map_err(err)
        };
        let open = button("Открыть DOCX")?;
        let save = button("Сохранить DOCX")?;
        let paragraph = button("Добавить абзац")?;
        let table = button("Добавить таблицу")?;
        let image = button("Пример изображения")?;
        for (name, id) in [
            ("RichOpen", open),
            ("RichSave", save),
            ("RichAddParagraph", paragraph),
            ("RichAddTable", table),
            ("RichAddImage", image),
            ("RichTextEditor", editor.parts.editor),
            ("RichPreview", editor.parts.preview),
            ("RichBold", editor.parts.bold),
            ("RichItalic", editor.parts.italic),
            ("RichUnderline", editor.parts.underline),
            ("RichUndo", editor.parts.undo),
            ("RichRedo", editor.parts.redo),
            ("RichCode", editor.parts.code),
            ("RichApply", editor.parts.apply),
            ("RichPrevious", editor.parts.previous),
            ("RichNext", editor.parts.next),
        ] {
            form.view.names.insert(name.into(), id);
        }
        let (tx, rx) = mpsc::sync_channel(1);
        Ok(Self {
            form,
            editor,
            open,
            save,
            paragraph,
            table,
            image,
            pending: None,
            request: None,
            next: 0,
            tx,
            rx,
            busy: false,
            alive: Arc::new(AtomicBool::new(true)),
            waker: None,
            diagnostics,
            last: String::new(),
        })
    }
    fn result(&mut self, message: &str) -> Result<(), String> {
        let text: String = message.chars().take(1500).collect();
        let id = self.form.find_name("RichDocumentResult").unwrap();
        self.form.ui.set_label(id, text).map_err(err)
    }
    fn refresh(&mut self) -> Result<(), String> {
        if let Err(error) = self.editor.sync(&mut self.form.ui) {
            self.result(&error.to_string())?;
        }
        while let Ok(completion) = self.rx.try_recv() {
            self.busy = false;
            match completion {
                Completion::Open { revision, result } => match result {
                    Ok(imported) if self.editor.revision() == revision && self.editor.view()==rust_desktop_ui_controls::rich_editor::RichEditorView::Document => {
                        match self
                            .editor
                            .replace_document(&mut self.form.ui, imported.document)
                        {
                            Ok(()) => {
                                let message = if imported.diagnostics.is_empty() {
                                    "DOCX открыт".into()
                                } else {
                                    format!(
                                        "DOCX открыт. {}",
                                        imported
                                            .diagnostics
                                            .iter()
                                            .map(|d| d.message.as_str())
                                            .collect::<Vec<_>>()
                                            .join("; ")
                                    )
                                };
                                self.result(&message)?;
                            }
                            Err(error) => {
                                self.result(&format!("Открытие не выполнено: {error}"))?
                            }
                        }
                    }
                    Ok(_) => self.result(
                        "Документ изменён во время загрузки; результат не заменил вашу работу",
                    )?,
                    Err(error) => self.result(&format!("Открытие не выполнено: {error}"))?,
                },
                Completion::Save(result) => self.result(&match result {
                    Ok(()) => "Снимок документа сохранён в DOCX".into(),
                    Err(error) => format!("Сохранение не выполнено: {error}"),
                })?,
            }
        }
        self.form
            .ui
            .set_enabled(self.open, !self.busy && self.pending.is_none())
            .map_err(err)?;
        self.form
            .ui
            .set_enabled(self.save, !self.busy && self.pending.is_none())
            .map_err(err)?;
        let document_view =
            self.editor.view() == rust_desktop_ui_controls::rich_editor::RichEditorView::Document;
        for id in [self.paragraph, self.table, self.image] {
            self.form.ui.set_enabled(id, document_view).map_err(err)?;
        }
        Ok(())
    }
    fn actions(&mut self) -> Result<(), String> {
        for id in self.form.ui.drain_activated() {
            let action = (|| -> Result<(), String> {
                if id == self.open {
                    self.queue(false)?;
                } else if id == self.save {
                    self.queue(true)?;
                } else if id == self.paragraph || id == self.table || id == self.image {
                    self.editor.sync(&mut self.form.ui).map_err(err)?;
                    let mut document = self.editor.document().clone();
                    if id == self.paragraph {
                        document
                            .blocks
                            .push(RichBlock::Paragraph(Paragraph::text("Новый абзац")));
                    } else if id == self.table {
                        let width = document.page.content_width() / 2.0;
                        document.blocks.push(RichBlock::Table(RichTable {
                            columns: vec![width, width],
                            rows: vec![vec![
                                Paragraph::text("Ячейка 1"),
                                Paragraph::text("Ячейка 2"),
                            ]],
                        }));
                    } else {
                        add_sample_image(&mut document)?;
                    }
                    self.editor
                        .replace_document(&mut self.form.ui, document)
                        .map_err(err)?;
                } else {
                    self.editor.activate(&mut self.form.ui, id).map_err(err)?;
                }
                Ok(())
            })();
            if let Err(error) = action {
                self.result(&error)?;
            }
        }
        Ok(())
    }
    fn queue(&mut self, save: bool) -> Result<(), String> {
        if self.busy || self.pending.is_some() {
            return Err("Дождитесь завершения операции".into());
        }
        self.editor.sync(&mut self.form.ui).map_err(err)?;
        if self
            .form
            .ui
            .document(self.editor.parts.editor)
            .is_some_and(|d| d.composition().is_some())
        {
            return Err("Завершите ввод перед открытием или сохранением".into());
        }
        if self.editor.view() == rust_desktop_ui_controls::rich_editor::RichEditorView::Code {
            return Err("Примените или отмените изменения Code перед работой с файлом".into());
        }
        self.next = self
            .next
            .checked_add(1)
            .ok_or("Dialog identity exhausted")?;
        let mut options = FileDialog::new(if save {
            FileDialogKind::Save
        } else {
            FileDialogKind::Open
        });
        options.title = if save {
            "Сохранить документ DOCX"
        } else {
            "Открыть документ DOCX"
        }
        .into();
        options.filters = vec![FileDialogFilter {
            label: "Документ DOCX".into(),
            extensions: vec!["docx".into()],
        }];
        if save {
            options.default_extension = Some("docx".into());
            options.file_name = Some("document.docx".into());
        }
        self.request = Some(DialogRequest {
            id: self.next,
            options: DialogOptions::File(options),
        });
        self.pending = Some((
            self.next,
            if save {
                Pending::Save {
                    document: self.editor.document().clone(),
                }
            } else {
                Pending::Open {
                    revision: self.editor.revision(),
                }
            },
        ));
        Ok(())
    }
    fn start(&mut self, pending: Pending, path: PathBuf) {
        self.busy = true;
        let tx = self.tx.clone();
        let alive = self.alive.clone();
        let wake = self.waker.clone();
        std::thread::spawn(move || {
            let completion = match pending {
                Pending::Open { revision } => Completion::Open {
                    revision,
                    result: (|| {
                        let file = std::fs::File::open(&path).map_err(err)?;
                        let mut bytes = Vec::new();
                        file.take(64 * 1024 * 1024 + 1)
                            .read_to_end(&mut bytes)
                            .map_err(err)?;
                        RichDocument::from_docx(&bytes).map_err(err)
                    })(),
                },
                Pending::Save { document } => Completion::Save((|| {
                    let export = document.to_docx().map_err(err)?;
                    atomic_save(&path, &export.bytes, &alive)
                })()),
            };
            if alive.load(Ordering::Relaxed)
                && tx.send(completion).is_ok()
                && let Some(wake) = wake
            {
                wake.wake();
            }
        });
    }
}
impl Drop for RichDocumentDemo {
    fn drop(&mut self) {
        self.alive.store(false, Ordering::Relaxed);
    }
}
impl DesktopApp for RichDocumentDemo {
    fn set_waker(&mut self, waker: Option<platform::AppWaker>) {
        self.waker = waker.clone();
        self.form.set_waker(waker);
    }
    fn update(&mut self, size: Size, scale: f32) -> Result<(), String> {
        self.refresh()?;
        self.form.update(size, scale)?;
        self.editor.refresh(&mut self.form.ui).map_err(err)?;
        self.form.update(size, scale)?;
        if self.diagnostics {
            let signature = format!(
                "revision={} blocks={} pages={} view={:?} busy={}",
                self.editor.revision(),
                self.editor.document().blocks.len(),
                self.editor.page_count(),
                self.editor.view(),
                self.busy
            );
            if signature != self.last {
                println!("rich-document-state {signature}");
                println!(
                    "batch-state {}",
                    crate::batch_diagnostics::snapshot(&self.form.ui, &self.form.view)
                );
                self.last = signature;
            }
        }
        Ok(())
    }
    fn scene(&self) -> &Scene {
        self.form.scene()
    }
    fn title(&self) -> String {
        self.form.title()
    }
    fn event(&mut self, event: PlatformEvent) -> Result<EventResponse, String> {
        let action = (|| -> Result<(), String> {
            match event {
                PlatformEvent::Input(input) => {
                    if !self
                        .editor
                        .handle_event(&mut self.form.ui, &input)
                        .map_err(err)?
                    {
                        self.form.ui.dispatch(input).map_err(err)?;
                    }
                }
                PlatformEvent::Text(text) => self.form.ui.text_input(&text).map_err(err)?,
                PlatformEvent::Paste(text) => self.form.ui.paste_input(&text).map_err(err)?,
                PlatformEvent::ImePreedit(text, cursor) => {
                    self.form.ui.ime_preedit(&text, cursor).map_err(err)?
                }
                PlatformEvent::ImeCommit(text) => self.form.ui.ime_commit(&text).map_err(err)?,
                PlatformEvent::ImeCancel => self.form.ui.cancel_preedit(),
                PlatformEvent::Tick => {
                    self.form.ui.tick(Instant::now()).map_err(err)?;
                }
                PlatformEvent::Accessibility(request) => {
                    self.form.accessible.action(&mut self.form.ui, request)?
                }
            }
            self.actions()?;
            Ok(())
        })();
        if let Err(error) = action {
            self.result(&error)?;
        }
        self.refresh()?;
        let clipboard = self
            .form
            .ui
            .drain_clipboard_requests()
            .into_iter()
            .map(|r| match r {
                rust_desktop_ui_controls::ClipboardRequest::Copy(t) => {
                    platform::ClipboardRequest::Copy(t)
                }
                rust_desktop_ui_controls::ClipboardRequest::Paste => {
                    platform::ClipboardRequest::Paste
                }
            })
            .collect();
        Ok(EventResponse {
            redraw: true,
            clipboard,
        })
    }
    fn take_dialog_request(&mut self) -> Option<DialogRequest> {
        self.request.take()
    }
    fn dialog_completed(&mut self, completion: DialogCompletion) -> Result<EventResponse, String> {
        if self
            .pending
            .as_ref()
            .is_none_or(|(id, _)| *id != completion.id)
        {
            return Ok(EventResponse::default());
        }
        let (_, pending) = self.pending.take().unwrap();
        match completion.result {
            Ok(DialogOutcome::Paths(paths)) if paths.len() == 1 => {
                self.start(pending, paths[0].clone())
            }
            Ok(_) => self.result("Операция отменена; документ сохранён")?,
            Err(error) => self.result(&error.to_string())?,
        }
        Ok(EventResponse::redraw())
    }
    fn wake_after(&self) -> Option<Duration> {
        if self.busy {
            Some(Duration::from_millis(40))
        } else {
            self.form.wake_after()
        }
    }
    fn accessibility(&mut self, scale: f32) -> accesskit::TreeUpdate {
        self.form.accessibility(scale)
    }
    fn pointer_capture_lost(&mut self) -> Result<EventResponse, String> {
        self.form.pointer_capture_lost()
    }
    fn ime_cursor(&self) -> Option<Rect> {
        self.form.ime_cursor()
    }
    fn ime_target(&self) -> Option<WidgetId> {
        self.form.ime_target()
    }
    fn ime_composition_active(&self) -> Option<bool> {
        self.form.ime_composition_active()
    }
}
fn atomic_save(path: &Path, bytes: &[u8], alive: &AtomicBool) -> Result<(), String> {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let token = NEXT.fetch_add(1, Ordering::Relaxed);
    let parent = path.parent().ok_or("Target directory missing")?;
    let temporary = parent.join(format!(".rich-document-{}-{token}.tmp", std::process::id()));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(err)?;
    let result = (|| {
        file.write_all(bytes).map_err(err)?;
        file.sync_all().map_err(err)?;
        drop(file);
        if !alive.load(Ordering::Relaxed) {
            return Err("Сохранение отменено".into());
        }
        std::fs::rename(&temporary, path).map_err(err)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}
fn add_sample_image(document: &mut RichDocument) -> Result<(), String> {
    // Original 2x2 checker from the image example; encoded PNG is immutable.
    const CHECKER: &[u8] = &[
        137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 2, 0, 0, 0, 2, 8, 6,
        0, 0, 0, 114, 182, 13, 36, 0, 0, 0, 22, 73, 68, 65, 84, 120, 156, 99, 96, 136, 223, 241,
        255, 255, 49, 159, 255, 12, 96, 2, 200, 1, 0, 89, 226, 10, 77, 76, 54, 151, 197, 0, 0, 0,
        0, 73, 69, 78, 68, 174, 66, 96, 130,
    ];
    let key = (1..=65)
        .map(|i| format!("checker{i}"))
        .find(|key| !document.media.contains_key(key))
        .ok_or("Image identifiers exhausted")?;
    document
        .transact(|d| {
            d.media.insert(
                key.clone(),
                DocumentMedia {
                    content_type: "image/png".into(),
                    bytes: CHECKER.to_vec(),
                },
            );
            d.blocks.push(RichBlock::Image(RichImage {
                media: key,
                width: 120.0,
                height: 120.0,
                alt: "Собственная цветная сетка 2×2".into(),
            }));
            Ok(())
        })
        .map_err(err)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn compiled_document_host_retains_paragraph_and_draws_preview_after_resize() {
        let mut app =
            RichDocumentDemo::new(crate::generated_rich_document::window_definition(), false)
                .unwrap();
        let editor = app.editor.parts.editor;
        for size in [Size::new(1240.0, 840.0), Size::new(1000.0, 700.0)] {
            app.update(size, 1.0).unwrap();
            assert_eq!(app.editor.parts.editor, editor);
            let bounds = app
                .form
                .ui
                .runtime
                .bounds(app.editor.parts.preview)
                .unwrap();
            assert!(bounds.width > 400.0 && bounds.height > 400.0);
            assert!(
                app.form
                    .scene()
                    .texts
                    .iter()
                    .any(|r| r.text == "Документ и форматирование")
            );
        }
        assert!(app.take_dialog_request().is_none());
    }
    #[test]
    fn sample_image_is_exported_and_parsed_by_real_docx_codec() {
        let mut d = crate::rich_document_model::sample_document().unwrap();
        add_sample_image(&mut d).unwrap();
        let imported = RichDocument::from_docx(&d.to_docx().unwrap().bytes).unwrap();
        assert_eq!(d.plain_text(), imported.document.plain_text());
        assert_eq!(d.media, imported.document.media);
    }
    #[test]
    fn queued_add_actions_do_not_discard_an_invalid_code_draft() {
        let mut app =
            RichDocumentDemo::new(crate::generated_rich_document::window_definition(), false)
                .unwrap();
        app.update(Size::new(1240.0, 840.0), 1.0).unwrap();
        let accepted = app.editor.document().clone();
        let id = app.editor.parts.editor;
        app.editor
            .set_view(
                &mut app.form.ui,
                rust_desktop_ui_controls::rich_editor::RichEditorView::Code,
            )
            .unwrap();
        app.form.ui.set_text(id, "{unfinished JSON draft").unwrap();
        // Model a previously enabled action queued immediately before the Code
        // view refresh. The mutation gate must still reject it independently.
        for button in [app.paragraph, app.table, app.image] {
            app.form
                .ui
                .perform_action(
                    button,
                    rust_desktop_ui_controls::SemanticAction::Activate,
                    None,
                )
                .unwrap();
            app.actions().unwrap();
            assert_eq!(app.editor.document(), &accepted);
            assert_eq!(
                app.form.ui.document(id).unwrap().text(),
                "{unfinished JSON draft"
            );
        }
        app.refresh().unwrap();
        for button in [app.paragraph, app.table, app.image] {
            assert!(!app.form.ui.is_enabled(button));
        }
    }
}
