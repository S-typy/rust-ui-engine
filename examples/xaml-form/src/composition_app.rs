use crate::{
    form::Form,
    output_app::{atomic_save, read_bounded},
};
use rust_desktop_ui_controls::{dashboard::*, media::*, report_designer::*, reports::*};
use rust_desktop_ui_core::*;
use rust_desktop_ui_data::*;
use rust_desktop_ui_platform_winit::{self as platform, accesskit, *};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

#[derive(Clone, Copy)]
pub(crate) enum CompositionMode {
    ReportDesigner,
    Dashboard,
    Media,
}
enum View {
    Report(Box<ReportDesigner>),
    Dashboard(Box<Dashboard>),
    Media(Box<MediaView>),
}
#[derive(Clone, Copy)]
enum Revision {
    Report(ReportRevision),
    Dashboard(DashboardRevision),
}
enum Task {
    Open(Revision),
    Save(Vec<u8>),
    Video,
}
enum Done {
    Open(Revision, String),
    Saved,
}
pub(crate) struct CompositionDemo {
    form: Box<Form>,
    view: View,
    actions: Vec<(WidgetId, &'static str)>,
    dialog: Option<DialogRequest>,
    pending: Option<(u64, Task)>,
    next: u64,
    tx: mpsc::SyncSender<Result<Done, String>>,
    rx: mpsc::Receiver<Result<Done, String>>,
    busy: bool,
    cancelled: Arc<AtomicBool>,
    waker: Option<AppWaker>,
    playlist: Vec<PathBuf>,
    playlist_index: usize,
    playlist_label: WidgetId,
    diagnostics: bool,
    last: String,
    export: Option<pages::PrintDocument>,
    capture: Option<Bitmap>,
}
fn err(e: impl ToString) -> String {
    e.to_string()
}
impl CompositionDemo {
    pub(crate) fn new(
        mut definition: rust_desktop_ui_xaml::WindowDefinition,
        diagnostics: bool,
        mode: CompositionMode,
        preview: bool,
        video: Option<PathBuf>,
    ) -> Result<Self, String> {
        definition.title = match mode {
            CompositionMode::ReportDesigner => "Дизайнер отчётов",
            CompositionMode::Dashboard => "Dashboard",
            CompositionMode::Media => "Медиа",
        }
        .into();
        let mut form = Box::new(Form::new(definition, false)?);
        let host = form
            .find_name("OutputHost")
            .ok_or("Missing composition host")?;
        let style = LayoutStyle {
            width: Length::Percent(1.0),
            height: Length::Percent(1.0),
            ..Default::default()
        };
        let view = match mode {
            CompositionMode::ReportDesigner => {
                let (d, t) = report_sample()?;
                let session = ReportDesignerSession::new(d, t, BTreeMap::new()).map_err(err)?;
                let mut v =
                    ReportDesigner::mount(&mut form.ui, host, session, style).map_err(err)?;
                v.set_preview(preview);
                View::Report(Box::new(v))
            }
            CompositionMode::Dashboard => {
                let mut v = Dashboard::mount(&mut form.ui, host, dashboard_sample()?, style)
                    .map_err(err)?;
                v.set_designer(preview);
                View::Dashboard(Box::new(v))
            }
            CompositionMode::Media => {
                let mut v = MediaView::mount(&mut form.ui, host, style).map_err(err)?;
                if let Some(path) = video.clone() {
                    v.session_mut()
                        .open(Arc::new(move |c| {
                            platform::media::open_video_file(&path, c)
                        }))
                        .map_err(err)?;
                } else {
                    v.session_mut().open(sequence()?).map_err(err)?;
                }
                if preview {
                    v.session_mut().play(Instant::now()).map_err(err)?;
                }
                View::Media(Box::new(v))
            }
        };
        let title = form.find_name("OutputTitle").unwrap();
        form.ui
            .set_label(
                title,
                match mode {
                    CompositionMode::ReportDesigner => "Дизайнер отчётов",
                    CompositionMode::Dashboard => "Dashboard · связанные данные",
                    CompositionMode::Media => "Медиа · воспроизведение и кадры",
                },
            )
            .map_err(err)?;
        let parent = form.find_name("OutputActions").unwrap();
        let toolbar = form
            .ui
            .transparent_panel(
                parent,
                LayoutStyle {
                    kind: LayoutKind::Wrap(Axis::Horizontal),
                    gap: 6.0,
                    ..Default::default()
                },
            )
            .map_err(err)?;
        let mut actions = Vec::new();
        for (label, action) in [
            ("Открыть", "open"),
            ("Сохранить", "save"),
            ("Экспорт PDF", "pdf"),
            ("Предыдущий файл", "previous"),
            ("Следующий файл", "next"),
            ("Отмена операции", "cancel"),
        ] {
            let measured = form
                .ui
                .measure_button_content(
                    label,
                    &rust_desktop_ui_controls::ButtonStyle::default(),
                    None,
                    rust_desktop_ui_controls::ButtonContentLayout::Leading,
                    2048.0,
                )
                .map_err(err)?;
            let id = form
                .ui
                .button(
                    toolbar,
                    label,
                    None,
                    LayoutStyle {
                        width: Length::Px(measured.width.ceil() + 4.0),
                        height: Length::Px(measured.height.ceil().max(36.0)),
                        ..Default::default()
                    },
                )
                .map_err(err)?;
            let media = matches!(mode, CompositionMode::Media);
            form.ui
                .set_enabled(
                    id,
                    match action {
                        "save" => !media,
                        "pdf" => matches!(mode, CompositionMode::ReportDesigner),
                        "previous" | "next" => media,
                        _ => true,
                    },
                )
                .map_err(err)?;
            actions.push((id, action));
        }
        let playlist_label = form
            .ui
            .label(
                toolbar,
                "",
                LayoutStyle {
                    height: Length::Px(30.0),
                    ..Default::default()
                },
            )
            .map_err(err)?;
        let (tx, rx) = mpsc::sync_channel(1);
        let mut result = Self {
            form,
            view,
            actions,
            dialog: None,
            pending: None,
            next: 0,
            tx,
            rx,
            busy: false,
            cancelled: Arc::new(AtomicBool::new(false)),
            waker: None,
            playlist: video.into_iter().collect(),
            playlist_index: 0,
            playlist_label,
            diagnostics,
            last: String::new(),
            export: None,
            capture: None,
        };
        result.message(
            "Изменения принимаются после проверки. Файлы сохраняются только по команде.",
        )?;
        Ok(result)
    }
    fn message(&mut self, s: impl AsRef<str>) -> Result<(), String> {
        let id = self.form.find_name("OutputResult").unwrap();
        self.form
            .ui
            .set_label(id, s.as_ref().chars().take(1600).collect::<String>())
            .map_err(err)
    }
    fn queue(&mut self, action: &str) -> Result<(), String> {
        if self.busy || self.pending.is_some() {
            return Err("Дождитесь текущей операции".into());
        }
        let (task, extension) = match (&self.view, action) {
            (View::Report(v), "open") => {
                (Task::Open(Revision::Report(v.session().revision())), "json")
            }
            (View::Dashboard(v), "open") => (
                Task::Open(Revision::Dashboard(v.session().revision())),
                "json",
            ),
            (View::Media(_), "open") => (Task::Video, "mp4"),
            (View::Report(v), "save") => (
                Task::Save(v.session().save().map_err(err)?.into_bytes()),
                "json",
            ),
            (View::Dashboard(v), "save") => (
                Task::Save(v.session().save().map_err(err)?.into_bytes()),
                "json",
            ),
            (View::Report(v), "pdf") => {
                // Snapshot is immutable; export is evaluated by the file worker below.
                let document = v.session().preview().document.clone();
                self.next += 1;
                let id = self.next;
                let mut file = FileDialog::new(FileDialogKind::Save);
                file.default_extension = Some("pdf".into());
                file.file_name = Some("report.pdf".into());
                self.export = Some(document);
                self.pending = Some((id, Task::Save(Vec::new())));
                self.dialog = Some(DialogRequest {
                    id,
                    options: DialogOptions::File(file),
                });
                return Ok(());
            }
            _ => return Ok(()),
        };
        self.next = self.next.checked_add(1).ok_or("Request id exhausted")?;
        let save = matches!(task, Task::Save(_));
        let multiple = matches!(task, Task::Video);
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
        file.multiple = multiple;
        file.filters = vec![FileDialogFilter {
            label: if multiple {
                "Видео"
            } else {
                "Документ"
            }
            .into(),
            extensions: if multiple {
                vec!["mp4".into(), "wmv".into(), "avi".into()]
            } else {
                vec![extension.into()]
            },
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
    fn actions(&mut self) -> Result<(), String> {
        for id in self.form.ui.drain_activated() {
            let operation = (|| {
                if let Some(action) = self
                    .actions
                    .iter()
                    .find_map(|(i, a)| (*i == id).then_some(*a))
                {
                    match action {
                        "cancel" => {
                            self.cancelled.store(true, Ordering::Release);
                            self.message("Запрошена отмена")?;
                        }
                        "next" | "previous" if !self.playlist.is_empty() => {
                            let index = if action == "next" {
                                (self.playlist_index + 1) % self.playlist.len()
                            } else {
                                (self.playlist_index + self.playlist.len() - 1)
                                    % self.playlist.len()
                            };
                            self.open_playlist(self.playlist.clone(), index)?;
                        }
                        _ => self.queue(action)?,
                    }
                } else {
                    match &mut self.view {
                        View::Report(v) => {
                            v.activate(&mut self.form.ui, id).map_err(err)?;
                        }
                        View::Dashboard(v) => {
                            v.activate(&mut self.form.ui, id).map_err(err)?;
                        }
                        View::Media(v) => {
                            v.activate(&self.form.ui, id).map_err(err)?;
                        }
                    }
                }
                if let View::Media(v) = &mut self.view
                    && let Some(bitmap) = v.take_capture()
                {
                    if self.busy || self.pending.is_some() {
                        return Err("Дождитесь операции с файлом".into());
                    }
                    self.next += 1;
                    let mut file = FileDialog::new(FileDialogKind::Save);
                    file.file_name = Some("frame.png".into());
                    file.default_extension = Some("png".into());
                    // PNG encoding also belongs to the worker.
                    self.capture = Some(bitmap);
                    self.pending = Some((self.next, Task::Save(Vec::new())));
                    self.dialog = Some(DialogRequest {
                        id: self.next,
                        options: DialogOptions::File(file),
                    });
                }
                Ok::<_, String>(())
            })();
            if let Err(e) = operation {
                self.message(e)?;
            }
        }
        Ok(())
    }
    fn open_playlist(&mut self, playlist: Vec<PathBuf>, index: usize) -> Result<(), String> {
        let path = playlist.get(index).ok_or("Invalid playlist index")?.clone();
        if let View::Media(v) = &mut self.view {
            v.session_mut()
                .open(Arc::new(move |c| {
                    platform::media::open_video_file(&path, c)
                }))
                .map_err(err)?;
        }
        self.playlist = playlist;
        self.playlist_index = index;
        self.form
            .ui
            .set_label(
                self.playlist_label,
                format!("Файл {} / {}", self.playlist_index + 1, self.playlist.len()),
            )
            .map_err(err)?;
        Ok(())
    }
    fn completed(&mut self) -> Result<(), String> {
        let Ok(result) = self.rx.try_recv() else {
            return Ok(());
        };
        self.busy = false;
        if self.cancelled.load(Ordering::Acquire) && !matches!(result, Ok(Done::Saved)) {
            return self.message("Операция отменена; принятый документ сохранён");
        }
        let result = result.and_then(|done| {
            match (done, &mut self.view) {
                (Done::Open(Revision::Report(revision), json), View::Report(v)) => {
                    v.session_mut().restore(revision, &json).map_err(err)?
                }
                (Done::Open(Revision::Dashboard(revision), json), View::Dashboard(v)) => {
                    v.session_mut().restore(revision, &json).map_err(err)?
                }
                (Done::Saved, _) => {}
                _ => return Err("Устаревший ответ".into()),
            }
            Ok(())
        });
        self.message(result.map_or_else(|e| e, |()| "Операция выполнена".into()))
    }
}
impl Drop for CompositionDemo {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Release);
    }
}
impl DesktopApp for CompositionDemo {
    fn set_waker(&mut self, waker: Option<AppWaker>) {
        self.waker = waker.clone();
        self.form.set_waker(waker);
    }
    fn update(&mut self, size: Size, scale: f32) -> Result<(), String> {
        self.completed()?;
        self.form.update(size, scale)?;
        match &mut self.view {
            View::Report(v) => v.refresh(&mut self.form.ui).map_err(err)?,
            View::Dashboard(v) => v.refresh(&mut self.form.ui).map_err(err)?,
            View::Media(v) => v.refresh(&mut self.form.ui, Instant::now()).map_err(err)?,
        }
        self.form.update(size, scale)?;
        if self.diagnostics {
            let signature = match &self.view {
                View::Report(v) => format!(
                    "report {:?} pages={} preview={}",
                    v.session().revision(),
                    v.session().preview().document.pages().len(),
                    v.is_preview()
                ),
                View::Dashboard(v) => format!(
                    "dashboard {:?} widgets={} filters={}",
                    v.session().revision(),
                    v.session().results().len(),
                    v.session().filters().len()
                ),
                View::Media(v) => format!(
                    "media {:?} frame={:?}",
                    v.session().state(),
                    v.session().frame().map(|f| f.position)
                ),
            };
            if signature != self.last {
                println!("composition-state {signature}");
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
        let operation = (|| {
            match event {
                PlatformEvent::Input(input) => {
                    let handled = match &mut self.view {
                        View::Report(v) => v.input(&mut self.form.ui, &input).map_err(err)?,
                        View::Media(v) => v.input(&self.form.ui, &input).map_err(err)?,
                        _ => false,
                    };
                    if !handled {
                        self.form.ui.dispatch(input).map_err(err)?;
                    }
                }
                PlatformEvent::Text(s) => self.form.ui.text_input(&s).map_err(err)?,
                PlatformEvent::Paste(s) => self.form.ui.paste_input(&s).map_err(err)?,
                PlatformEvent::ImePreedit(s, c) => self.form.ui.ime_preedit(&s, c).map_err(err)?,
                PlatformEvent::ImeCommit(s) => self.form.ui.ime_commit(&s).map_err(err)?,
                PlatformEvent::ImeCancel => self.form.ui.cancel_preedit(),
                PlatformEvent::Tick => {
                    self.form.ui.tick(Instant::now()).map_err(err)?;
                }
                PlatformEvent::Accessibility(request) => {
                    self.form.accessible.action(&mut self.form.ui, request)?
                }
            }
            if let View::Dashboard(v) = &mut self.view {
                v.selection_changed(&self.form.ui).map_err(err)?;
            }
            self.actions()
        })();
        if let Err(e) = operation {
            self.message(e)?;
        }
        Ok(EventResponse {
            redraw: true,
            clipboard: self
                .form
                .ui
                .drain_clipboard_requests()
                .into_iter()
                .map(|r| match r {
                    rust_desktop_ui_controls::ClipboardRequest::Copy(s) => {
                        ClipboardRequest::Copy(s)
                    }
                    rust_desktop_ui_controls::ClipboardRequest::Paste => ClipboardRequest::Paste,
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
        let export = self.export.take();
        let capture = self.capture.take();
        match completion.result {
            Ok(DialogOutcome::Paths(paths)) if !paths.is_empty() => {
                if matches!(task, Task::Video) {
                    if paths.len() > 128 {
                        self.message("Не более 128 файлов")?;
                    } else {
                        if let Err(e) = self.open_playlist(paths, 0) {
                            self.message(e)?;
                        }
                    }
                } else if paths.len() == 1 {
                    self.busy = true;
                    self.cancelled = Arc::new(AtomicBool::new(false));
                    let c = self.cancelled.clone();
                    let tx = self.tx.clone();
                    let w = self.waker.clone();
                    let path = paths[0].clone();
                    std::thread::spawn(move || {
                        let work = || -> Result<Done, String> {
                            if c.load(Ordering::Acquire) {
                                return Err("Отменено".into());
                            }
                            match task {
                                Task::Open(revision) => Ok(Done::Open(
                                    revision,
                                    String::from_utf8(read_bounded(&path, 4 * 1024 * 1024)?)
                                        .map_err(err)?,
                                )),
                                Task::Save(mut bytes) => {
                                    if let Some(document) = export {
                                        bytes = rust_desktop_ui_controls::printing::export_pdf(
                                            &document,
                                            &mut rust_desktop_ui_text::TextEngine::new(),
                                            || c.load(Ordering::Acquire),
                                        )
                                        .map_err(err)?;
                                    }
                                    if let Some(bitmap) = capture {
                                        bytes = rust_desktop_ui_images::encode_bitmap_png(&bitmap)
                                            .map_err(err)?;
                                    }
                                    atomic_save(&path, &bytes, &c)?;
                                    Ok(Done::Saved)
                                }
                                Task::Video => unreachable!(),
                            }
                        };
                        let result = work();
                        if tx.send(result).is_ok()
                            && let Some(w) = w
                        {
                            w.wake();
                        }
                    });
                }
            }
            Err(e) => self.message(e.to_string())?,
            _ => self.message("Операция отменена")?,
        }
        Ok(EventResponse::redraw())
    }
    fn wake_after(&self) -> Option<Duration> {
        if self.busy || matches!(self.view, View::Media(_)) {
            Some(Duration::from_millis(33))
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

fn data() -> Result<DataTable, String> {
    DataTable::new(
        TableSchema::new(vec![
            FieldSchema::new("month", FieldType::Integer),
            FieldSchema::new("region", FieldType::Text),
            FieldSchema::new("amount", FieldType::Integer),
        ])
        .map_err(err)?,
        (1..=12)
            .map(|i| {
                DataRow::new(
                    i,
                    vec![
                        CellValue::Integer(i),
                        CellValue::Text(if i % 2 == 0 { "Север" } else { "Юг" }.into()),
                        CellValue::Integer(500 + i * 125 + (i % 3) * 180),
                    ],
                )
            })
            .collect(),
    )
    .map_err(err)
}
fn report_sample() -> Result<(ReportDefinition, DataTable), String> {
    let item = |x, width, value, bold| ReportItem::Text {
        x,
        y: 0.0,
        width,
        font_size: 16.0,
        bold,
        color: Color::rgb(30, 35, 45),
        value,
    };
    Ok((
        ReportDefinition {
            title: "Продажи по месяцам".into(),
            report_header: ReportBand {
                height: 48.0,
                items: vec![item(
                    0.0,
                    650.0,
                    ReportValue::Literal("Продажи по месяцам".into()),
                    true,
                )],
            },
            page_header: ReportBand {
                height: 32.0,
                items: vec![
                    item(0.0, 150.0, ReportValue::Literal("Месяц".into()), true),
                    item(180.0, 220.0, ReportValue::Literal("Регион".into()), true),
                    item(430.0, 200.0, ReportValue::Literal("Сумма".into()), true),
                ],
            },
            detail: ReportBand {
                height: 30.0,
                items: vec![
                    item(
                        0.0,
                        150.0,
                        ReportValue::Expression(QueryExpression::field("month")),
                        false,
                    ),
                    item(
                        180.0,
                        220.0,
                        ReportValue::Expression(QueryExpression::field("region")),
                        false,
                    ),
                    item(
                        430.0,
                        200.0,
                        ReportValue::Expression(QueryExpression::field("amount")),
                        false,
                    ),
                ],
            },
            report_footer: ReportBand {
                height: 36.0,
                items: vec![
                    item(0.0, 400.0, ReportValue::Literal("Итого".into()), true),
                    item(430.0, 200.0, ReportValue::Total(0), true),
                ],
            },
            query: QueryRequest {
                aggregates: vec![Aggregate {
                    function: AggregateFunction::Sum,
                    expression: Some(QueryExpression::field("amount")),
                }],
                ..Default::default()
            },
            ..Default::default()
        },
        data()?,
    ))
}
fn dashboard_sample() -> Result<DashboardSession, String> {
    let query = QueryRequest {
        aggregates: vec![Aggregate {
            function: AggregateFunction::Sum,
            expression: Some(QueryExpression::field("amount")),
        }],
        ..Default::default()
    };
    let widget = |id: &str, title: &str, kind, row, column, span| DashboardItem {
        id: id.into(),
        title: title.into(),
        source: "sales".into(),
        widget: kind,
        placement: DashboardPlacement {
            row,
            column,
            row_span: 1,
            column_span: span,
        },
        query: query.clone(),
    };
    DashboardSession::new(
        DashboardDefinition {
            title: "Продажи · обзор".into(),
            columns: 3,
            parameters: vec![],
            items: vec![
                widget(
                    "chart",
                    "Продажи по месяцам",
                    DashboardWidget::Chart {
                        x: "month".into(),
                        y: "amount".into(),
                    },
                    0,
                    0,
                    2,
                ),
                widget(
                    "total",
                    "Сумма выбранных данных",
                    DashboardWidget::Metric { aggregate: 0 },
                    0,
                    2,
                    1,
                ),
                widget(
                    "table",
                    "Детализация · выберите строку",
                    DashboardWidget::Grid,
                    1,
                    0,
                    3,
                ),
            ],
        },
        BTreeMap::from([("sales".into(), data()?)]),
    )
    .map_err(err)
}
fn sequence() -> Result<MediaFactory, String> {
    let mut frames = Vec::new();
    for n in 0..60 {
        let (w, h) = (480, 270);
        let mut pixels = vec![0; w * h * 4];
        for y in 0..h {
            for x in 0..w {
                let p = (y * w + x) * 4;
                let moving = (x as i32 - n * 7).rem_euclid(w as i32) < 65;
                pixels[p..p + 4].copy_from_slice(if moving {
                    &[30, 150, 245, 255]
                } else if y < h / 2 {
                    &[25, 40, 62, 255]
                } else {
                    &[42, 65, 90, 255]
                });
            }
        }
        frames.push(Bitmap::new_rgba8(w as u32, h as u32, pixels).map_err(err)?);
    }
    Ok(Arc::new(move |_| {
        Ok(Box::new(ImageSequenceProvider::new(
            "Демонстрационная последовательность · 6 секунд",
            frames.clone(),
            Duration::from_millis(100),
        )?))
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn delayed_report_restore_does_not_overwrite_newer_edit() {
        let mut app = CompositionDemo::new(
            crate::generated_output::window_definition(),
            false,
            CompositionMode::ReportDesigner,
            false,
            None,
        )
        .unwrap();
        let View::Report(v) = &mut app.view else {
            unreachable!()
        };
        let old = v.session().revision();
        let json = v.session().save().unwrap();
        let mut candidate = v.session().definition().clone();
        candidate.title = "Newer edit".into();
        v.session_mut().replace(old, candidate).unwrap();
        app.tx
            .send(Ok(Done::Open(Revision::Report(old), json)))
            .unwrap();
        app.completed().unwrap();
        let View::Report(v) = &app.view else {
            unreachable!()
        };
        assert_eq!(v.session().definition().title, "Newer edit");
    }
    #[test]
    fn preview_and_designer_geometry_survives_form_resize() {
        for mode in [
            CompositionMode::ReportDesigner,
            CompositionMode::Dashboard,
            CompositionMode::Media,
        ] {
            let mut app = CompositionDemo::new(
                crate::generated_output::window_definition(),
                false,
                mode,
                true,
                None,
            )
            .unwrap();
            app.update(Size::new(1280.0, 900.0), 1.5).unwrap();
            app.update(Size::new(885.0, 596.0), 1.5).unwrap();
            for (id, _) in &app.actions {
                let b = app.form.ui.runtime.bounds(*id).unwrap();
                assert!(b.width >= 50.0 && b.height >= 34.0, "{b:?}");
            }
            match &app.view {
                View::Report(v) => {
                    assert!(app.form.ui.runtime.bounds(v.canvas).unwrap().height > 200.0)
                }
                View::Media(v) => {
                    assert!(app.form.ui.runtime.bounds(v.canvas).unwrap().height > 200.0)
                }
                View::Dashboard(v) => assert_eq!(
                    app.form.ui.chart_display_status(v.widget("chart").unwrap()),
                    Some(rust_desktop_ui_controls::ChartDisplayStatus::Full),
                    "the sample chart must still render points after resize"
                ),
            }
        }
    }
}
