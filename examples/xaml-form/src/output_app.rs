use crate::form::Form;
use rust_desktop_ui_controls::{
    page_viewer::{PageLoadToken, PageViewer},
    pdf::PdfDocument,
    printing,
    reports::*,
    spreadsheet::Spreadsheet,
};
use rust_desktop_ui_core::{pages::PrintDocument, *};
use rust_desktop_ui_data::{workbook::*, *};
use rust_desktop_ui_platform_winit::{
    self as platform, DesktopApp, DialogCompletion, DialogOptions, DialogOutcome, DialogRequest,
    EventResponse, FileDialog, FileDialogFilter, FileDialogKind, PlatformEvent, accesskit,
};
use rust_desktop_ui_text::TextEngine;
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, Receiver, SyncSender},
    },
    time::{Duration, Instant},
};

#[path = "accessibility_sheet.rs"]
mod sheet_accessibility;

#[derive(Clone, Copy, Debug)]
pub(crate) enum OutputMode {
    Spreadsheet,
    Pdf,
    Preview,
    Report,
}
enum View {
    Sheet(Box<Spreadsheet>),
    Pages(Box<PageViewer>),
}
enum Task {
    OpenSheet(u64),
    OpenPdf(PageLoadToken),
    SaveSheet(Workbook),
    SavePdf(Box<PdfDocument>),
    Export(PrintDocument),
}
enum Done {
    Sheet(u64, Workbook, Vec<String>),
    Pdf(PageLoadToken, Box<PdfDocument>),
    Message(String),
    Printers(Vec<String>),
}
pub(crate) struct OutputDemo {
    form: Box<Form>,
    view: View,
    actions: Vec<(WidgetId, &'static str)>,
    printer: WidgetId,
    pending: Option<(u64, Task)>,
    dialog: Option<DialogRequest>,
    next: u64,
    tx: SyncSender<Result<Done, String>>,
    rx: Receiver<Result<Done, String>>,
    cancelled: Arc<AtomicBool>,
    busy: bool,
    waker: Option<platform::AppWaker>,
    diagnostics: bool,
    last: String,
    last_snapshot: String,
    sheet_accessible: sheet_accessibility::SheetAccessibility,
}
fn err(e: impl ToString) -> String {
    e.to_string()
}
impl OutputDemo {
    pub(crate) fn new(
        definition: rust_desktop_ui_xaml::WindowDefinition,
        diagnostics: bool,
        mode: OutputMode,
    ) -> Result<Self, String> {
        let mut form = Box::new(Form::new(definition, false)?);
        let host = form.find_name("OutputHost").ok_or("Output host missing")?;
        let style = LayoutStyle {
            width: Length::Percent(1.0),
            height: Length::Percent(1.0),
            ..Default::default()
        };
        let view = match mode {
            OutputMode::Spreadsheet => View::Sheet(Box::new(
                Spreadsheet::mount(&mut form.ui, host, sample_workbook()?, style).map_err(err)?,
            )),
            _ => {
                let document = sample_report()?.document;
                let viewer = if matches!(mode, OutputMode::Pdf) {
                    let bytes = printing::export_pdf(&document, &mut TextEngine::new(), || false)
                        .map_err(err)?;
                    PageViewer::pdf(
                        &mut form.ui,
                        host,
                        PdfDocument::open(&bytes).map_err(err)?,
                        style,
                    )
                } else {
                    PageViewer::print_preview(&mut form.ui, host, document, style)
                };
                View::Pages(Box::new(viewer.map_err(err)?))
            }
        };
        let title_id = form.find_name("OutputTitle").unwrap();
        form.ui
            .set_label(
                title_id,
                match mode {
                    OutputMode::Spreadsheet => "Электронная таблица",
                    OutputMode::Pdf => "PDF — просмотр и поиск",
                    OutputMode::Preview => "Предпросмотр печати",
                    OutputMode::Report => "Отчёт — группы, итоги и страницы",
                },
            )
            .map_err(err)?;
        let parent = form.find_name("OutputActions").unwrap();
        let toolbar = form
            .ui
            .transparent_panel(
                parent,
                LayoutStyle {
                    kind: LayoutKind::Grid,
                    grid_columns: vec![
                        GridTrack::Px(110.0),
                        GridTrack::Px(110.0),
                        GridTrack::Px(110.0),
                        GridTrack::Px(110.0),
                        GridTrack::Fr(1.0),
                        GridTrack::Px(110.0),
                        GridTrack::Px(90.0),
                    ],
                    gap: 5.0,
                    height: Length::Px(36.0),
                    ..Default::default()
                },
            )
            .map_err(err)?;
        let mut actions = Vec::new();
        for (column, title, action) in [
            (1, "Открыть", "open"),
            (2, "Сохранить", "save"),
            (3, "Экспорт PDF", "export"),
            (4, "Принтеры", "printers"),
            (6, "Печатать", "print"),
            (7, "Отмена", "cancel"),
        ] {
            let id = form
                .ui
                .button(
                    toolbar,
                    title,
                    None,
                    LayoutStyle {
                        grid_column: GridPlacement {
                            start: Some(column),
                            span: 1,
                        },
                        ..Default::default()
                    },
                )
                .map_err(err)?;
            actions.push((id, action));
            if matches!(mode, OutputMode::Preview | OutputMode::Report)
                && matches!(action, "open" | "save")
            {
                form.ui.set_enabled(id, false).map_err(err)?;
            }
        }
        let printer = form
            .ui
            .text_box(
                toolbar,
                "Имя принтера",
                "",
                LayoutStyle {
                    grid_column: GridPlacement {
                        start: Some(5),
                        span: 1,
                    },
                    grid_row: GridPlacement {
                        start: Some(1),
                        span: 1,
                    },
                    ..Default::default()
                },
            )
            .map_err(err)?;
        form.ui
            .set_placeholder(printer, "Имя принтера")
            .map_err(err)?;
        match &view {
            View::Sheet(s) => {
                form.view
                    .names
                    .insert("SpreadsheetCanvas".into(), s.parts.canvas);
                form.view.names.insert("FormulaBar".into(), s.parts.formula);
                form.view.names.insert("CellFormat".into(), s.parts.format);
                form.view
                    .names
                    .insert("SheetAutoColumns".into(), s.parts.auto_columns);
                form.view
                    .names
                    .insert("SheetAutoRows".into(), s.parts.auto_rows);
                form.view
                    .names
                    .insert("CellFormatCode".into(), s.parts.format_code);
                form.view
                    .names
                    .insert("CellFormatCodeApply".into(), s.parts.format_code_apply);
                form.view
                    .names
                    .insert("CellFormatApply".into(), s.parts.format_apply);
            }
            View::Pages(v) => {
                form.view.names.insert("PageCanvas".into(), v.parts.canvas);
                form.view.names.insert("PageSearch".into(), v.parts.search);
            }
        }
        let (tx, rx) = mpsc::sync_channel(1);
        Ok(Self {
            form,
            view,
            actions,
            printer,
            pending: None,
            dialog: None,
            next: 0,
            tx,
            rx,
            cancelled: Arc::new(AtomicBool::new(false)),
            busy: false,
            waker: None,
            diagnostics,
            last: String::new(),
            last_snapshot: String::new(),
            sheet_accessible: Default::default(),
        })
    }
    fn message(&mut self, message: impl AsRef<str>) -> Result<(), String> {
        let id = self.form.find_name("OutputResult").unwrap();
        self.form
            .ui
            .set_label(id, message.as_ref().chars().take(1600).collect::<String>())
            .map_err(err)
    }
    fn pages(&mut self) -> Result<PrintDocument, String> {
        match &mut self.view {
            View::Pages(v) => Ok(v.document().clone()),
            View::Sheet(s) => {
                s.commit(&mut self.form.ui).map_err(err)?;
                printing::from_workbook_with_culture(
                    s.session().workbook(),
                    s.session().sheet(),
                    printing::SheetPrintOptions::default(),
                    s.culture(),
                    || false,
                )
                .map_err(err)
            }
        }
    }
    fn queue(&mut self, action: &str) -> Result<(), String> {
        if self.busy || self.pending.is_some() {
            return Err("Дождитесь текущей операции".into());
        }
        let (task, extension) = match (&mut self.view, action) {
            (View::Sheet(s), "open") => {
                s.commit(&mut self.form.ui).map_err(err)?;
                (Task::OpenSheet(s.session().generation()), "xlsx")
            }
            (View::Sheet(s), "save") => {
                s.commit(&mut self.form.ui).map_err(err)?;
                (Task::SaveSheet(s.session().workbook().clone()), "xlsx")
            }
            (View::Pages(v), "open") => (Task::OpenPdf(v.begin_load().map_err(err)?), "pdf"),
            (View::Pages(v), "save") => (
                Task::SavePdf(Box::new(
                    v.pdf_document().ok_or("Нет исходного PDF")?.clone(),
                )),
                "pdf",
            ),
            (_, "export") => (Task::Export(self.pages()?), "pdf"),
            _ => return Err("Неизвестная файловая операция".into()),
        };
        self.next = self.next.checked_add(1).ok_or("Dialog IDs exhausted")?;
        let save = action != "open";
        let mut file = FileDialog::new(if save {
            FileDialogKind::Save
        } else {
            FileDialogKind::Open
        });
        file.title = if save {
            "Сохранить документ"
        } else {
            "Открыть документ"
        }
        .into();
        file.filters = vec![FileDialogFilter {
            label: extension.to_uppercase(),
            extensions: vec![extension.into()],
        }];
        if save {
            file.default_extension = Some(extension.into());
            file.file_name = Some(format!("document.{extension}"));
        }
        self.pending = Some((self.next, task));
        self.dialog = Some(DialogRequest {
            id: self.next,
            options: DialogOptions::File(file),
        });
        Ok(())
    }
    fn spawn(
        &mut self,
        work: impl FnOnce(Arc<AtomicBool>) -> Result<Done, String> + Send + 'static,
    ) -> Result<(), String> {
        if self.busy {
            return Err("Операция уже выполняется".into());
        }
        self.busy = true;
        self.cancelled = Arc::new(AtomicBool::new(false));
        let cancelled = self.cancelled.clone();
        let tx = self.tx.clone();
        let wake = self.waker.clone();
        std::thread::spawn(move || {
            let result = work(cancelled);
            if tx.send(result).is_ok()
                && let Some(w) = wake
            {
                w.wake();
            }
        });
        Ok(())
    }
    fn actions(&mut self) -> Result<(), String> {
        for id in self.form.ui.drain_activated() {
            let operation = (|| {
                if let Some(action) = self
                    .actions
                    .iter()
                    .find_map(|(i, a)| (*i == id).then_some(*a))
                {
                    match action {
                        "open" | "save" | "export" => self.queue(action)?,
                        "cancel" => {
                            self.cancelled.store(true, Ordering::Release);
                            self.message("Запрошена отмена операции")?;
                        }
                        "printers" => self.spawn(|_| {
                            platform::printing::printers()
                                .map(Done::Printers)
                                .map_err(err)
                        })?,
                        "print" => {
                            let printer = self
                                .form
                                .ui
                                .document(self.printer)
                                .unwrap()
                                .text()
                                .to_owned();
                            if printer.is_empty() {
                                return Err(
                                    "Выберите принтер: команда «Принтеры» или точное имя".into()
                                );
                            }
                            let document = self.pages()?;
                            self.spawn(move |cancelled| {
                                let result = platform::printing::print(
                                    &document,
                                    &platform::printing::PrintTicket::new(printer),
                                    &cancelled,
                                )
                                .map_err(err)?;
                                Ok(Done::Message(format!(
                                    "Задание {} передано в очередь, страниц: {}",
                                    result.job_id, result.pages_submitted
                                )))
                            })?;
                        }
                        _ => {}
                    }
                } else {
                    match &mut self.view {
                        View::Sheet(s) => {
                            s.activate(&mut self.form.ui, id).map_err(err)?;
                        }
                        View::Pages(v) => {
                            v.activate(&mut self.form.ui, id).map_err(err)?;
                        }
                    }
                }
                Ok::<_, String>(())
            })();
            if let Err(error) = operation {
                self.message(error)?;
            }
        }
        Ok(())
    }
    fn completed(&mut self) -> Result<(), String> {
        while let Ok(result) = self.rx.try_recv() {
            self.busy = false;
            if self.cancelled.load(Ordering::Acquire) {
                self.message("Операция отменена; текущий документ сохранён")?;
                continue;
            }
            match result {
                Err(e) => self.message(e)?,
                Ok(Done::Message(s)) => self.message(s)?,
                Ok(Done::Printers(names)) => {
                    if let Some(name) = names.first() {
                        self.form
                            .ui
                            .set_editor_value(self.printer, name)
                            .map_err(err)?;
                    }
                    self.message(format!(
                        "Доступные принтеры: {}. Для печати нажмите «Печатать».",
                        names.join("; ")
                    ))?;
                }
                Ok(Done::Sheet(generation, book, diagnostics)) => {
                    if let View::Sheet(s) = &mut self.view {
                        let draft_changed =
                            self.form.ui.document(s.parts.formula).is_some_and(|d| {
                                d.composition().is_some() || d.text() != s.session().entry()
                            });
                        if generation != s.session().generation() || draft_changed {
                            self.message(
                                "Таблица изменилась во время загрузки; результат не применён",
                            )?;
                        } else {
                            s.replace_workbook(&mut self.form.ui, book).map_err(err)?;
                            self.sheet_accessible.clear();
                            self.message(format!("XLSX открыт. {}", diagnostics.join("; ")))?;
                        }
                    }
                }
                Ok(Done::Pdf(token, pdf)) => {
                    let warnings = pdf
                        .diagnostics()
                        .iter()
                        .map(|d| d.message.clone())
                        .collect::<Vec<_>>()
                        .join("; ");
                    if let View::Pages(v) = &mut self.view {
                        if v.complete_load(&mut self.form.ui, token, *pdf)
                            .map_err(err)?
                        {
                            self.message(format!("PDF открыт. {warnings}"))?;
                        } else {
                            self.message("Результат загрузки устарел")?;
                        }
                    }
                }
            }
        }
        Ok(())
    }
}
impl Drop for OutputDemo {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Release);
    }
}
impl DesktopApp for OutputDemo {
    fn set_waker(&mut self, waker: Option<platform::AppWaker>) {
        self.waker = waker.clone();
        self.form.set_waker(waker);
    }
    fn update(&mut self, size: Size, scale: f32) -> Result<(), String> {
        self.completed()?;
        self.form.update(size, scale)?;
        match &mut self.view {
            View::Sheet(s) => s.refresh(&mut self.form.ui).map_err(err)?,
            View::Pages(v) => v.refresh(&mut self.form.ui).map_err(err)?,
        }
        self.form.update(size, scale)?;
        if self.diagnostics {
            if let View::Sheet(sheet) = &self.view {
                use crate::popup_diagnostics::{rectangle, string};
                let cells = sheet
                    .semantics(&self.form.ui)
                    .map_err(err)?
                    .cells
                    .into_iter()
                    .map(|c| {
                        format!(
                            "{{\"address\":{},\"bounds\":{},\"value\":{},\"selected\":{}}}",
                            string(&c.address.a1()),
                            rectangle(c.bounds),
                            string(&c.value),
                            c.selected
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(",");
                let details = format!(
                    "{{\"active\":{},\"revision\":{},\"ime\":{},\"ime_target\":{},\"ime_cursor\":{},\"entry\":{},\"cells\":[{}]}}",
                    string(&sheet.session().active().a1()),
                    sheet.session().workbook().revision(),
                    self.form
                        .ui
                        .document(sheet.parts.formula)
                        .is_some_and(|d| d.composition().is_some()),
                    self.form.ime_target().is_some(),
                    self.form.ime_cursor().map_or("null".into(), rectangle),
                    string(&sheet.session().entry()),
                    cells
                );
                let batch = crate::batch_diagnostics::snapshot(&self.form.ui, &self.form.view);
                let snapshot = format!("{details}\n{batch}");
                if snapshot != self.last_snapshot {
                    println!("worksheet-state {details}");
                    println!("batch-state {batch}");
                    self.last_snapshot = snapshot;
                }
            }
            let signature = match &self.view {
                View::Sheet(s) => format!(
                    "sheet={} selection={}:{} generation={}",
                    s.session().sheet(),
                    s.session().selection().start().a1(),
                    s.session().selection().end().a1(),
                    s.session().generation()
                ),
                View::Pages(v) => format!("page={}/{}", v.page() + 1, v.document().pages().len()),
            };
            if signature != self.last {
                println!("output-state {signature}");
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
        if self.diagnostics {
            println!("output-event {event:?}");
        }

        let action = (|| {
            match event {
                PlatformEvent::Input(input) => {
                    let handled = match &mut self.view {
                        View::Sheet(s) => s.input(&mut self.form.ui, &input),
                        View::Pages(v) => v.input(&mut self.form.ui, &input),
                    }
                    .map_err(err)?;
                    if !handled {
                        self.form.ui.dispatch(input).map_err(err)?;
                    }
                }
                PlatformEvent::Paste(text) => {
                    let handled = match &mut self.view {
                        View::Sheet(s) => s.paste_text(&mut self.form.ui, &text).map_err(err)?,
                        _ => false,
                    };
                    if !handled {
                        self.form.ui.paste_input(&text).map_err(err)?;
                    }
                }
                PlatformEvent::Text(text) => {
                    let handled = if let View::Sheet(sheet) = &mut self.view {
                        sheet.text_input(&mut self.form.ui, &text).map_err(err)?
                    } else {
                        false
                    };
                    if !handled {
                        self.form.ui.text_input(&text).map_err(err)?;
                    }
                }
                PlatformEvent::ImePreedit(text, cursor) => {
                    self.form.ui.ime_preedit(&text, cursor).map_err(err)?
                }
                PlatformEvent::ImeCommit(text) => self.form.ui.ime_commit(&text).map_err(err)?,
                PlatformEvent::ImeCancel => self.form.ui.cancel_preedit(),
                PlatformEvent::Tick => {
                    if let View::Sheet(sheet) = &mut self.view {
                        sheet.interaction_tick(&mut self.form.ui).map_err(err)?;
                    }
                    self.form.ui.tick(Instant::now()).map_err(err)?;
                }
                PlatformEvent::Accessibility(request) => {
                    let handled = if let View::Sheet(sheet) = &mut self.view {
                        self.sheet_accessible
                            .action(sheet, &mut self.form.ui, &request)?
                    } else {
                        false
                    };
                    if !handled {
                        self.form.accessible.action(&mut self.form.ui, request)?;
                    }
                }
            }
            self.actions()?;
            Ok::<_, String>(())
        })();
        if let Err(e) = action {
            self.message(e)?;
        }
        let mut requests = self.form.ui.drain_clipboard_requests();
        if let View::Sheet(s) = &mut self.view {
            requests.extend(s.take_clipboard_requests());
        }
        Ok(EventResponse {
            redraw: true,
            clipboard: requests
                .into_iter()
                .map(|r| match r {
                    rust_desktop_ui_controls::ClipboardRequest::Copy(s) => {
                        platform::ClipboardRequest::Copy(s)
                    }
                    rust_desktop_ui_controls::ClipboardRequest::Paste => {
                        platform::ClipboardRequest::Paste
                    }
                })
                .collect(),
        })
    }
    fn take_dialog_request(&mut self) -> Option<DialogRequest> {
        self.dialog.take()
    }
    fn dialog_completed(&mut self, completion: DialogCompletion) -> Result<EventResponse, String> {
        if self
            .pending
            .as_ref()
            .is_none_or(|(id, _)| *id != completion.id)
        {
            return Ok(EventResponse::default());
        }
        let (_, task) = self.pending.take().unwrap();
        match completion.result {
            Ok(DialogOutcome::Paths(paths)) if paths.len() == 1 => {
                let path = paths[0].clone();
                self.spawn(move |cancelled| {
                    if cancelled.load(Ordering::Acquire) {
                        return Err("Операция отменена".into());
                    }
                    match task {
                        Task::OpenSheet(generation) => {
                            let import =
                                Workbook::from_xlsx(&read_bounded(&path, 64 * 1024 * 1024)?)
                                    .map_err(err)?;
                            Ok(Done::Sheet(
                                generation,
                                import.workbook,
                                import.diagnostics.into_iter().map(|d| d.message).collect(),
                            ))
                        }
                        Task::OpenPdf(token) => {
                            let bytes = read_bounded(&path, 16 * 1024 * 1024)?;
                            let pdf = PdfDocument::open_with_cancel(&bytes, || {
                                cancelled.load(Ordering::Acquire)
                            })
                            .map_err(err)?;
                            Ok(Done::Pdf(token, Box::new(pdf)))
                        }
                        task => {
                            let bytes = match task {
                                Task::SaveSheet(b) => b.to_xlsx().map_err(err)?,
                                Task::SavePdf(p) => p.save().map_err(err)?,
                                Task::Export(p) => {
                                    printing::export_pdf(&p, &mut TextEngine::new(), || {
                                        cancelled.load(Ordering::Acquire)
                                    })
                                    .map_err(err)?
                                }
                                _ => unreachable!(),
                            };
                            atomic_save(&path, &bytes, &cancelled)?;
                            Ok(Done::Message("Снимок документа сохранён".into()))
                        }
                    }
                })?;
            }
            Ok(_) => self.message("Операция отменена")?,
            Err(e) => self.message(e.to_string())?,
        }
        Ok(EventResponse::redraw())
    }
    fn wake_after(&self) -> Option<Duration> {
        if matches!(&self.view,View::Sheet(sheet) if sheet.needs_interaction_tick()) {
            return Some(Duration::from_millis(50));
        }
        if self.busy {
            Some(Duration::from_millis(50))
        } else {
            self.form.wake_after()
        }
    }
    fn accessibility(&mut self, scale: f32) -> accesskit::TreeUpdate {
        let mut tree = self.form.accessibility(scale);
        if let View::Sheet(sheet) = &self.view
            && let Err(error) = self.sheet_accessible.project(
                sheet,
                &self.form.ui,
                &mut self.form.accessible,
                &mut tree,
                scale,
            )
        {
            self.sheet_accessible.clear();
            eprintln!("Worksheet accessibility: {error}");
        }
        tree
    }
    fn pointer_capture_lost(&mut self) -> Result<EventResponse, String> {
        if let View::Sheet(sheet) = &mut self.view {
            sheet.cancel_interaction(&mut self.form.ui).map_err(err)?;
        }
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
pub(crate) fn read_bounded(path: &Path, limit: usize) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(err)?
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(err)?;
    if bytes.len() > limit {
        return Err("Файл превышает допустимый размер".into());
    }
    Ok(bytes)
}
pub(crate) fn atomic_save(path: &Path, bytes: &[u8], cancelled: &AtomicBool) -> Result<(), String> {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let temporary = path.parent().ok_or("No output directory")?.join(format!(
        ".output-{}-{}.tmp",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    // Only clean up a temporary file successfully created by this call.
    // A pre-existing path must survive a create_new failure.
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(err)?;
    let result = (|| {
        f.write_all(bytes).map_err(err)?;
        f.sync_all().map_err(err)?;
        drop(f);
        if cancelled.load(Ordering::Acquire) {
            return Err("Сохранение отменено".into());
        }
        std::fs::rename(&temporary, path).map_err(err)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}
fn sample_workbook() -> Result<Workbook, String> {
    let mut book = Workbook::new("Продажи").map_err(err)?;
    let mut edits = Vec::new();
    for (column, title) in ["Товар", "Количество", "Цена", "Стоимость"]
        .iter()
        .enumerate()
    {
        let mut cell = WorkbookCell::value(CellValue::Text((*title).into()));
        cell.style.bold = true;
        edits.push(WorkbookEdit::Set {
            sheet: 0,
            address: CellAddress::new(0, column as u32).map_err(err)?,
            cell,
        });
    }
    for row in 1..=60 {
        for (column, cell) in [
            WorkbookCell::value(CellValue::Text(format!("Позиция {row}"))),
            WorkbookCell::value(CellValue::Integer(i64::from(row))),
            WorkbookCell::value(CellValue::Integer(125)),
            WorkbookCell::formula(format!("=B{}*C{}", row + 1, row + 1)),
        ]
        .into_iter()
        .enumerate()
        {
            edits.push(WorkbookEdit::Set {
                sheet: 0,
                address: CellAddress::new(row, column as u32).map_err(err)?,
                cell,
            });
        }
    }
    edits.push(WorkbookEdit::AddSheet {
        name: "Примечания".into(),
    });
    book.apply(&edits).map_err(err)?;
    append_format_examples(&mut book)?;
    append_size_examples(&mut book)?;
    Ok(book)
}

fn append_size_examples(book: &mut Workbook) -> Result<(), String> {
    let sheet = book.sheets().len();
    let mut edits = vec![WorkbookEdit::AddSheet {
        name: "Размеры".into(),
    }];
    for (row, column, text, wrap) in [
        (0, 0, "Ширина по содержимому: длинная подпись", false),
        (
            1,
            0,
            "Текст с переносом по ширине столбца. Высота строки подбирается по всем строкам текста.",
            true,
        ),
        (2, 0, "Первая строка\nВторая строка\nТретья строка", false),
        (
            4,
            1,
            "Объединённая ячейка: ширина распределяется между выбранными столбцами",
            false,
        ),
    ] {
        let mut cell = WorkbookCell::value(CellValue::Text(text.into()));
        cell.style.wrap = wrap;
        edits.push(WorkbookEdit::Set {
            sheet,
            address: CellAddress::new(row, column).map_err(err)?,
            cell,
        });
    }
    edits.push(WorkbookEdit::Merge {
        sheet,
        range: CellRange::new(
            CellAddress::new(4, 1).map_err(err)?,
            CellAddress::new(4, 3).map_err(err)?,
        )
        .map_err(err)?,
    });
    book.apply(&edits).map_err(err)
}
pub(crate) fn sample_report() -> Result<ReportResult, String> {
    let schema = TableSchema::new(vec![
        FieldSchema::new("department", FieldType::Text),
        FieldSchema::new("product", FieldType::Text),
        FieldSchema::new("amount", FieldType::Integer),
    ])
    .map_err(err)?;
    let table = DataTable::new(
        schema,
        (0..36)
            .map(|i| {
                DataRow::new(
                    i,
                    vec![
                        CellValue::Text(
                            if i < 18 {
                                "Разработка"
                            } else {
                                "Поддержка"
                            }
                            .into(),
                        ),
                        CellValue::Text(format!("Работа №{}", i + 1)),
                        CellValue::Integer((i + 1) * 125),
                    ],
                )
            })
            .collect(),
    )
    .map_err(err)?;
    let text = |x, width, value, bold| ReportItem::Text {
        x,
        y: 0.0,
        width,
        font_size: 16.0,
        bold,
        color: Color::rgb(30, 35, 42),
        value,
    };
    let d = ReportDefinition {
        title: "Отчёт по подразделениям".into(),
        query: QueryRequest {
            group_by: vec![QueryExpression::field("department")],
            aggregates: vec![Aggregate {
                function: AggregateFunction::Sum,
                expression: Some(QueryExpression::field("amount")),
            }],
            ..Default::default()
        },
        report_header: ReportBand {
            height: 46.0,
            items: vec![text(
                0.0,
                650.0,
                ReportValue::Literal("Отчёт по подразделениям".into()),
                true,
            )],
        },
        group_header: ReportBand {
            height: 34.0,
            items: vec![text(0.0, 650.0, ReportValue::GroupKey(0), true)],
        },
        detail: ReportBand {
            height: 29.0,
            items: vec![
                text(
                    0.0,
                    440.0,
                    ReportValue::Expression(QueryExpression::field("product")),
                    false,
                ),
                text(
                    470.0,
                    180.0,
                    ReportValue::Expression(QueryExpression::field("amount")),
                    false,
                ),
            ],
        },
        group_footer: ReportBand {
            height: 40.0,
            items: vec![
                text(
                    0.0,
                    400.0,
                    ReportValue::Literal("Итого по подразделению".into()),
                    true,
                ),
                text(470.0, 180.0, ReportValue::GroupTotal(0), true),
            ],
        },
        report_footer: ReportBand {
            height: 40.0,
            items: vec![
                text(0.0, 400.0, ReportValue::Literal("Общий итог".into()), true),
                text(470.0, 180.0, ReportValue::Total(0), true),
            ],
        },
        page_footer: ReportBand {
            height: 28.0,
            items: vec![
                text(
                    0.0,
                    400.0,
                    ReportValue::Literal(
                        "Rust UI Engine · сформировано из типизированных данных".into(),
                    ),
                    false,
                ),
                text(600.0, 50.0, ReportValue::PageNumber, false),
            ],
        },
        ..Default::default()
    };
    ReportEngine::build(&d, &table, &BTreeMap::new(), &mut TextEngine::new(), || {
        false
    })
    .map_err(err)
}

fn append_format_examples(book: &mut Workbook) -> Result<(), String> {
    use rust_desktop_ui_data::{DateValue, TimeValue};
    let date = CellValue::Date(DateValue::new(2026, 11, 4).map_err(err)?);
    let time = CellValue::Time(TimeValue::new(16, 5, 8, 123_000_000).map_err(err)?);
    let sheet = book.sheets().len();
    let mut edits = vec![WorkbookEdit::AddSheet {
        name: "Форматы".into(),
    }];
    for (column, (title, width)) in [
        ("Значение", 240.0),
        ("Представление", 400.0),
        ("Формат", 280.0),
    ]
    .into_iter()
    .enumerate()
    {
        edits.push(WorkbookEdit::ColumnWidth {
            sheet,
            column: column as u32,
            width: Some(width),
        });
        let mut cell = WorkbookCell::value(CellValue::Text(title.into()));
        cell.style.bold = true;
        edits.push(WorkbookEdit::Set {
            sheet,
            address: CellAddress::new(0, column as u32).map_err(err)?,
            cell,
        });
    }
    for (index, (label, value, format)) in [
        ("Дата", date.clone(), "dddd, d mmmm yyyy"),
        (
            "Дата · English",
            date.clone(),
            "[$-en-US]dddd, mmmm d, yyyy",
        ),
        ("Месяц", date, "mmmm yyyy"),
        ("Время · 12 часов", time.clone(), "h:mm:ss.000 am/pm"),
        ("Время · 24 часа", time, "hh:mm:ss"),
        (
            "Инженерная запись",
            CellValue::Integer(12_200_000),
            "##0.00E+00",
        ),
        (
            "Десятичный разделитель",
            CellValue::Real(1234.5),
            "[$-en-US]#,##0.00",
        ),
        (
            "Округление порядка",
            CellValue::Decimal("999.995".parse().map_err(err)?),
            "##0.00E+00",
        ),
        ("Длительность · 36 часов", CellValue::Real(1.5), "[h]:mm:ss"),
        ("Длительность · минуты", CellValue::Real(1.5), "[m]:ss"),
        (
            "Длительность · доли секунды",
            CellValue::Decimal("0.0001".parse().map_err(err)?),
            "[ss].000",
        ),
        (
            "Отрицательная длительность",
            CellValue::Real(-1.5),
            "[h]:mm;[Red]([h]:mm)",
        ),
        ("Смешанная дробь", CellValue::Real(5.25), "# ?/?"),
        ("Неправильная дробь", CellValue::Real(5.25), "??/??"),
        ("Фиксированный знаменатель", CellValue::Real(1.875), "# ?/8"),
        (
            "Приближение дробью",
            CellValue::Real(std::f64::consts::PI),
            "# ???/???",
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let row = index as u32 + 1;
        let mut formatted = WorkbookCell::value(value);
        formatted.style.number_format = format.into();
        for (column, cell) in [
            WorkbookCell::value(CellValue::Text(label.into())),
            formatted,
            WorkbookCell::value(CellValue::Text(format.into())),
        ]
        .into_iter()
        .enumerate()
        {
            edits.push(WorkbookEdit::Set {
                sheet,
                address: CellAddress::new(row, column as u32).map_err(err)?,
                cell,
            });
        }
    }
    book.apply(&edits).map_err(err)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn file_save_replaces_atomically_and_cancel_keeps_existing_contents() {
        let directory = std::env::temp_dir().join(format!(
            "rust-ui-output-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&directory).unwrap();
        let path = directory.join("document.pdf");
        atomic_save(&path, b"before", &AtomicBool::new(false)).unwrap();
        assert!(atomic_save(&path, b"cancelled", &AtomicBool::new(true)).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"before");
        atomic_save(&path, b"after", &AtomicBool::new(false)).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"after");
        assert_eq!(std::fs::read_dir(&directory).unwrap().count(), 1);
        std::fs::remove_file(&path).unwrap();
        std::fs::remove_dir(&directory).unwrap();
    }
    #[test]
    fn spreadsheet_resize_preserves_formula_draft_and_cancel_changes_no_cell() {
        let mut app = OutputDemo::new(
            crate::generated_output::window_definition(),
            false,
            OutputMode::Spreadsheet,
        )
        .unwrap();
        let View::Sheet(sheet) = &mut app.view else {
            unreachable!()
        };
        let id = sheet.parts.formula;
        app.form.ui.set_text(id, "=A2*5").unwrap();
        for size in [Size::new(1280.0, 900.0), Size::new(900.0, 650.0)] {
            app.update(size, 1.0).unwrap();
            assert_eq!(app.form.ui.document(id).unwrap().text(), "=A2*5");
        }
        assert!(app.take_dialog_request().is_none());
        assert!(!app.busy);
        app.form
            .ui
            .runtime
            .apply(Mutation::Focus(Some(id)))
            .unwrap();
        app.event(PlatformEvent::Input(InputEvent::KeyDown {
            key: Key::Escape,
            modifiers: Modifiers::default(),
            repeat: false,
        }))
        .unwrap();
        assert_eq!(app.form.ui.document(id).unwrap().text(), "Товар");
        let View::Sheet(sheet) = &app.view else {
            unreachable!()
        };
        assert_eq!(sheet.session().entry(), "Товар");
    }

    #[test]
    fn spreadsheet_accessibility_merged_cells_headers_selection_and_draft_without_window() {
        use accesskit::{Action, ActionRequest, Role, TreeId};
        let mut app = OutputDemo::new(
            crate::generated_output::window_definition(),
            false,
            OutputMode::Spreadsheet,
        )
        .unwrap();
        let address = |s| CellAddress::parse(s).unwrap();
        let mut book = Workbook::new("Accessible sheet").unwrap();
        book.apply(&[WorkbookEdit::Merge {
            sheet: 0,
            range: CellRange::new(address("A1"), address("B2")).unwrap(),
        }])
        .unwrap();
        let View::Sheet(sheet) = &mut app.view else {
            unreachable!()
        };
        sheet.replace_workbook(&mut app.form.ui, book).unwrap();
        let formula = sheet.parts.formula;
        app.update(Size::new(1280.0, 900.0), 1.0).unwrap();
        let tree = app.accessibility(1.0);
        platform::AccessibilitySnapshot::default()
            .update(tree.clone())
            .unwrap();
        let grid = tree
            .nodes
            .iter()
            .find(|(_, n)| n.role() == Role::Grid)
            .unwrap();
        assert_eq!(grid.1.row_count(), Some(MAX_ROWS as usize));
        assert_eq!(grid.1.column_count(), Some(MAX_COLUMNS as usize));
        let find = |tree: &accesskit::TreeUpdate, r, c| {
            tree.nodes
                .iter()
                .find(|(_, n)| {
                    n.role() == Role::Cell
                        && n.row_index() == Some(r)
                        && n.column_index() == Some(c)
                })
                .unwrap()
                .0
        };
        let merged = find(&tree, 0, 0);
        let target = find(&tree, 2, 2);
        let node = &tree.nodes.iter().find(|(id, _)| *id == merged).unwrap().1;
        assert_eq!((node.row_span(), node.column_span()), (Some(2), Some(2)));
        assert!(!tree.nodes.iter().any(|(_, n)| n.role() == Role::Cell
            && n.row_index() == Some(1)
            && n.column_index() == Some(1)));
        assert!(
            tree.nodes
                .iter()
                .any(|(_, n)| n.role() == Role::ColumnHeader && n.label() == Some("C"))
        );
        assert!(
            tree.nodes
                .iter()
                .any(|(_, n)| n.role() == Role::RowHeader && n.label() == Some("3"))
        );
        assert!(tree.nodes.len() < 7000);
        let request = |target| {
            PlatformEvent::Accessibility(ActionRequest {
                action: Action::Focus,
                target_tree: TreeId::ROOT,
                target_node: target,
                data: None,
            })
        };
        app.event(request(target)).unwrap();
        let View::Sheet(sheet) = &app.view else {
            unreachable!()
        };
        assert_eq!(sheet.session().active(), address("C3"));
        assert_eq!(find(&app.accessibility(1.0), 2, 2), target);
        app.form.ui.set_text(formula, "pending formula").unwrap();
        app.event(request(merged)).unwrap();
        let View::Sheet(sheet) = &app.view else {
            unreachable!()
        };
        assert_eq!(sheet.session().active(), address("C3"));
        assert_eq!(
            app.form.ui.document(formula).unwrap().text(),
            "pending formula"
        );
        app.update(Size::new(900.0, 650.0), 1.5).unwrap();
        let smaller = app.accessibility(1.5);
        assert_eq!(find(&smaller, 0, 0), merged);
        platform::AccessibilitySnapshot::default()
            .update(smaller)
            .unwrap();
    }
    #[test]
    fn spreadsheet_virtual_realization_preserves_selection_and_rejects_draft_navigation() {
        use accesskit::{Action, ActionData, ActionRequest, Role, TreeId};
        let mut app = OutputDemo::new(
            crate::generated_output::window_definition(),
            false,
            OutputMode::Spreadsheet,
        )
        .unwrap();
        app.update(Size::new(1000.0, 750.0), 1.0).unwrap();
        let tree = app.accessibility(1.0);
        let focused_cell = tree
            .nodes
            .iter()
            .find(|(_, n)| {
                n.role() == Role::Cell && n.row_index() == Some(0) && n.column_index() == Some(0)
            })
            .unwrap()
            .0;
        app.event(PlatformEvent::Accessibility(ActionRequest {
            action: Action::Focus,
            target_tree: TreeId::ROOT,
            target_node: focused_cell,
            data: None,
        }))
        .unwrap();
        let tree = app.accessibility(1.0);
        assert_eq!(tree.focus, focused_cell);
        let (root, grid) = tree
            .nodes
            .iter()
            .find(|(_, n)| n.role() == Role::Grid)
            .unwrap();
        let root = *root;
        let generation = grid.virtualization_generation().unwrap();
        let request = |row, column| {
            PlatformEvent::Accessibility(ActionRequest {
                action: Action::RealizeCell,
                target_tree: TreeId::ROOT,
                target_node: root,
                data: Some(ActionData::RealizeCell {
                    row,
                    column,
                    generation,
                }),
            })
        };
        let View::Sheet(sheet) = &app.view else {
            unreachable!()
        };
        let active = sheet.session().active();
        let selection = sheet.session().selection();
        let revision = sheet.session().generation();
        let formula = sheet.parts.formula;
        let focus = app.form.ui.runtime.focused();
        app.event(request(100_000, 150)).unwrap();
        app.update(Size::new(1000.0, 750.0), 1.0).unwrap();
        let tree = app.accessibility(1.0);
        assert_eq!(tree.focus, focused_cell);
        assert!(
            tree.nodes
                .iter()
                .find(|(id, _)| *id == focused_cell)
                .unwrap()
                .1
                .bounds()
                .is_none()
        );
        assert!(tree.nodes.iter().any(|(_, n)| n.role() == Role::Cell
            && n.row_index() == Some(100_000)
            && n.column_index() == Some(150)));
        assert!(tree.nodes.len() < 7000);
        assert_eq!(
            tree.nodes
                .iter()
                .find(|(id, _)| *id == root)
                .unwrap()
                .1
                .virtualization_generation(),
            Some(generation)
        );
        let View::Sheet(sheet) = &app.view else {
            unreachable!()
        };
        assert_eq!(
            (
                sheet.session().active(),
                sheet.session().selection(),
                sheet.session().generation()
            ),
            (active, selection, revision)
        );
        assert_eq!(app.form.ui.runtime.focused(), focus);
        app.form
            .ui
            .set_text(formula, "pending =SUM(A1:A3)")
            .unwrap();
        app.event(request(0, 0)).unwrap();
        let tree = app.accessibility(1.0);
        assert!(
            !tree
                .nodes
                .iter()
                .find(|(id, _)| *id == root)
                .unwrap()
                .1
                .supports_action(accesskit::Action::RealizeCell)
        );
        assert!(
            tree.nodes
                .iter()
                .any(|(_, n)| n.role() == Role::Cell && n.row_index() == Some(100_000))
        );
        assert_eq!(
            app.form.ui.document(formula).unwrap().text(),
            "pending =SUM(A1:A3)"
        );
        platform::AccessibilitySnapshot::default()
            .update(tree)
            .unwrap();
    }
    #[test]
    fn viewer_rejects_stale_load_and_navigation_does_not_recreate_text_editor() {
        let mut app = OutputDemo::new(
            crate::generated_output::window_definition(),
            false,
            OutputMode::Pdf,
        )
        .unwrap();
        let View::Pages(view) = &mut app.view else {
            unreachable!()
        };
        let field = view.parts.field_value;
        let old = view.begin_load().unwrap();
        let latest = view.begin_load().unwrap();
        let pdf = view.pdf_document().unwrap().clone();
        assert!(
            !view
                .complete_load(&mut app.form.ui, old, pdf.clone())
                .unwrap()
        );
        assert!(view.complete_load(&mut app.form.ui, latest, pdf).unwrap());
        assert_eq!(view.parts.field_value, field);
        app.update(Size::new(1000.0, 700.0), 1.0).unwrap();
        assert!(app.scene().images.len() > 20);
        let View::Pages(view) = &app.view else {
            unreachable!()
        };
        let bounds = |id| app.form.ui.runtime.bounds(id).unwrap();
        assert!(
            bounds(view.parts.canvas).height > 300.0,
            "{:?}",
            bounds(view.parts.canvas)
        );
        assert!((bounds(view.parts.search).y - bounds(view.parts.page_number).y).abs() < 0.01);
        assert!(
            bounds(view.parts.field_value).y
                > bounds(view.parts.canvas).y + bounds(view.parts.canvas).height
        );
        assert!(bounds(app.printer).height >= 30.0);
        assert!(bounds(app.printer).y < bounds(view.parts.search).y);
        assert!(app.take_dialog_request().is_none());
    }
    #[test]
    fn spreadsheet_pointer_range_fill_resize_cancel_and_undo_share_geometry() {
        fn send(app: &mut OutputDemo, event: InputEvent) {
            app.event(PlatformEvent::Input(event)).unwrap();
            app.update(Size::new(1280.0, 900.0), 1.0).unwrap();
        }
        fn sheet(app: &OutputDemo) -> &Spreadsheet {
            let View::Sheet(sheet) = &app.view else {
                panic!()
            };
            sheet
        }
        fn cell(app: &OutputDemo, address: &str) -> Rect {
            sheet(app)
                .semantics(&app.form.ui)
                .unwrap()
                .cells
                .into_iter()
                .find(|c| c.address == CellAddress::parse(address).unwrap())
                .unwrap()
                .bounds
        }
        fn center(rect: Rect) -> Point {
            Point::new(rect.x + rect.width / 2.0, rect.y + rect.height / 2.0)
        }
        fn down(position: Point) -> InputEvent {
            InputEvent::PointerDown {
                position,
                button: PointerButton::Primary,
                modifiers: Modifiers::default(),
            }
        }
        fn up(position: Point) -> InputEvent {
            InputEvent::PointerUp {
                position,
                button: PointerButton::Primary,
                modifiers: Modifiers::default(),
            }
        }
        let mut app = OutputDemo::new(
            crate::generated_output::window_definition(),
            false,
            OutputMode::Spreadsheet,
        )
        .unwrap();
        let mut book = Workbook::new("Gestures").unwrap();
        book.apply(&[WorkbookEdit::Set {
            sheet: 0,
            address: CellAddress::parse("A1").unwrap(),
            cell: WorkbookCell::value(CellValue::Integer(7)),
        }])
        .unwrap();
        if let View::Sheet(sheet) = &mut app.view {
            sheet.replace_workbook(&mut app.form.ui, book).unwrap();
        }
        app.update(Size::new(1280.0, 900.0), 1.0).unwrap();
        let original = sheet(&app).session().workbook().clone();
        let a1 = cell(&app, "A1");
        let b2 = center(cell(&app, "B2"));
        send(&mut app, down(center(a1)));
        send(&mut app, InputEvent::PointerMoved { position: b2 });
        assert_eq!(sheet(&app).session().selection().end().a1(), "B2");
        send(
            &mut app,
            InputEvent::KeyDown {
                key: Key::Escape,
                modifiers: Modifiers::default(),
                repeat: false,
            },
        );
        assert_eq!(sheet(&app).session().selection().end().a1(), "A1");
        assert!(app.form.ui.runtime.captured().is_none());
        let edge = Point::new(a1.x + a1.width, a1.y - 14.0);
        let moved = Point::new(edge.x + 55.0, edge.y);
        send(&mut app, down(edge));
        send(&mut app, InputEvent::PointerMoved { position: moved });
        assert_eq!(sheet(&app).session().workbook(), &original);
        assert!((cell(&app, "A1").width - 167.0).abs() < 0.01);
        app.pointer_capture_lost().unwrap();
        app.update(Size::new(1280.0, 900.0), 1.0).unwrap();
        assert_eq!(cell(&app, "A1").width, 112.0);
        send(&mut app, down(edge));
        send(&mut app, up(moved));
        assert_eq!(
            sheet(&app).session().workbook().sheets()[0]
                .dimensions()
                .column_width(0),
            Some(167.0)
        );
        send(
            &mut app,
            InputEvent::KeyDown {
                key: Key::Character("z".into()),
                modifiers: Modifiers {
                    control: true,
                    ..Default::default()
                },
                repeat: false,
            },
        );
        assert_eq!(cell(&app, "A1").width, 112.0);
        let handle = Point::new(a1.x + a1.width - 2.0, a1.y + a1.height - 2.0);
        let target = center(cell(&app, "A4"));
        send(&mut app, down(handle));
        send(&mut app, InputEvent::PointerMoved { position: target });
        assert_eq!(sheet(&app).session().workbook(), &original);
        send(&mut app, up(target));
        assert_eq!(
            sheet(&app)
                .session()
                .workbook()
                .value(0, CellAddress::parse("A4").unwrap())
                .unwrap(),
            rust_desktop_ui_data::workbook::WorkbookValue::Scalar(CellValue::Integer(7))
        );
        send(
            &mut app,
            InputEvent::KeyDown {
                key: Key::Character("z".into()),
                modifiers: Modifiers {
                    control: true,
                    ..Default::default()
                },
                repeat: false,
            },
        );
        assert_eq!(sheet(&app).session().workbook(), &original);
    }
    #[test]
    fn spreadsheet_cell_and_formula_bar_share_draft_identity_and_ime_guards() {
        let mut app = OutputDemo::new(
            crate::generated_output::window_definition(),
            false,
            OutputMode::Spreadsheet,
        )
        .unwrap();
        app.update(Size::new(1280.0, 900.0), 1.0).unwrap();
        let View::Sheet(sheet) = &mut app.view else {
            panic!()
        };
        let id = sheet.parts.formula;
        let home = app.form.ui.runtime.bounds(id).unwrap();
        let original = sheet.session().workbook().clone();
        sheet.begin_cell_edit(&mut app.form.ui).unwrap();
        assert!(!sheet.can_realize_accessible_cell(&app.form.ui));
        app.update(Size::new(1280.0, 900.0), 1.0).unwrap();
        let inside = app.form.ui.runtime.bounds(id).unwrap();
        assert!(inside.y > home.y);
        app.form.ui.set_text(id, "Unicode Ω").unwrap();
        let View::Sheet(sheet) = &mut app.view else {
            panic!()
        };
        sheet.edit_formula(&mut app.form.ui).unwrap();
        assert_eq!(app.form.ui.document(id).unwrap().text(), "Unicode Ω");
        assert_eq!(sheet.session().workbook(), &original);
        sheet.begin_cell_edit(&mut app.form.ui).unwrap();
        app.event(PlatformEvent::ImePreedit("にほんご".into(), Some((0, 0))))
            .unwrap();
        let View::Sheet(sheet) = &mut app.view else {
            panic!()
        };
        assert!(sheet.commit(&mut app.form.ui).is_err());
        assert!(sheet.set_number_format(&mut app.form.ui, "0.00").is_err());
        assert!(
            sheet
                .replace_workbook(&mut app.form.ui, Workbook::new("Other").unwrap())
                .is_err()
        );
        assert_eq!(sheet.session().workbook(), &original);
        app.event(PlatformEvent::ImeCancel).unwrap();
        app.update(Size::new(80.0, 120.0), 1.0).unwrap();
        assert_eq!(app.form.ui.document(id).unwrap().text(), "Unicode Ω");
        app.update(Size::new(1280.0, 900.0), 1.0).unwrap();
        app.event(PlatformEvent::Input(InputEvent::KeyDown {
            key: Key::Escape,
            modifiers: Modifiers::default(),
            repeat: false,
        }))
        .unwrap();
        app.update(Size::new(1280.0, 900.0), 1.0).unwrap();
        assert_eq!(app.form.ui.runtime.bounds(id).unwrap(), home);
        assert_eq!(app.form.ui.document(id).unwrap().text(), "Товар");
        app.event(PlatformEvent::Text("Прямой ввод".into()))
            .unwrap();
        app.update(Size::new(1280.0, 900.0), 1.0).unwrap();
        assert_eq!(app.form.ui.document(id).unwrap().text(), "Прямой ввод");
        app.event(PlatformEvent::Input(InputEvent::KeyDown {
            key: Key::Enter,
            modifiers: Modifiers::default(),
            repeat: false,
        }))
        .unwrap();
        app.update(Size::new(1280.0, 900.0), 1.0).unwrap();
        let View::Sheet(sheet) = &app.view else {
            panic!()
        };
        assert_eq!(sheet.parts.formula, id);
        assert_eq!(sheet.session().entry(), "Прямой ввод");
        assert_eq!(app.form.ui.runtime.bounds(id).unwrap(), home);
    }
}
