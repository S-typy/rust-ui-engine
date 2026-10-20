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
                sheet_report(s.session().workbook(), s.session().sheet()).map(|r| r.document)
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
                                let prepared = printing::prepare_for_printer(
                                    &document,
                                    &mut TextEngine::new(),
                                    2.0,
                                    || cancelled.load(Ordering::Acquire),
                                )
                                .map_err(err)?;
                                let result = platform::printing::print(
                                    &prepared,
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
                PlatformEvent::Text(text) => self.form.ui.text_input(&text).map_err(err)?,
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
        if self.busy {
            Some(Duration::from_millis(50))
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
    Ok(book)
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
fn sheet_report(book: &Workbook, index: usize) -> Result<ReportResult, String> {
    let sheet = &book.sheets()[index];
    let rows = sheet.cells().keys().map(|a| a.row() + 1).max().unwrap_or(1);
    let cols = sheet
        .cells()
        .keys()
        .map(|a| a.column() + 1)
        .max()
        .unwrap_or(1);
    if rows > 1000 || cols > 12 {
        return Err("Профиль печати листа: до 1000 строк и 12 столбцов".into());
    }
    if !sheet.merges().is_empty() {
        return Err("Печать объединённых ячеек листа пока не поддерживается".into());
    }
    let schema = TableSchema::new(
        (0..cols)
            .map(|c| FieldSchema::new(format!("c{c}"), FieldType::Text))
            .collect(),
    )
    .map_err(err)?;
    let table = DataTable::new(
        schema,
        (0..rows)
            .map(|r| {
                Ok(DataRow::new(
                    i64::from(r),
                    (0..cols)
                        .map(|c| {
                            book.value(index, CellAddress::new(r, c)?)
                                .map(|v| CellValue::Text(v.display()))
                        })
                        .collect::<Result<Vec<_>, DataError>>()?,
                ))
            })
            .collect::<Result<Vec<_>, DataError>>()
            .map_err(err)?,
    )
    .map_err(err)?;
    let width = 698.0 / cols as f32;
    let d = ReportDefinition {
        title: sheet.name().into(),
        detail: ReportBand {
            height: 26.0,
            items: (0..cols)
                .map(|c| ReportItem::Text {
                    x: c as f32 * width,
                    y: 0.0,
                    width: width - 8.0,
                    font_size: 14.0,
                    bold: false,
                    color: Color::rgb(0, 0, 0),
                    value: ReportValue::Expression(QueryExpression::field(format!("c{c}"))),
                })
                .collect(),
        },
        ..Default::default()
    };
    ReportEngine::build(&d, &table, &BTreeMap::new(), &mut TextEngine::new(), || {
        false
    })
    .map_err(err)
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
}
