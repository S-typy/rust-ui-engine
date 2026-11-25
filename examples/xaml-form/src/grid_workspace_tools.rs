//! Commands for the grid's user layout, provider window and printable snapshot.
use super::grid_workspace_model as demo;
use crate::view::MountedView;
use rust_desktop_ui_controls::{
    Controls, GridFormatKind, GridFormatRule, GridPageView, GridPrintOptions, Theme,
    page_viewer::PageViewer, printing,
};
use rust_desktop_ui_core::{LayoutStyle, Length, Size, WidgetId};
use rust_desktop_ui_data::*;
use std::{
    cell::RefCell,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
};
fn err(e: impl ToString) -> String {
    e.to_string()
}
fn home() -> Result<PathBuf, String> {
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("rust-ui-engine")
        .join("grid");
    std::fs::create_dir_all(&base).map_err(err)?;
    Ok(base)
}
fn layout_path(name: &str) -> Result<PathBuf, String> {
    if name.is_empty()
        || name.len() > 100
        || !name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    {
        return Err(
            "Имя раскладки должно состоять из букв, цифр, дефисов или подчёркиваний".into(),
        );
    }
    Ok(home()?.join(format!("layout-{name}.json")))
}
fn export_dir() -> Result<PathBuf, String> {
    let base = home()?;
    for n in 1..10000 {
        let dir = base.join(format!("export-{n}"));
        match std::fs::create_dir(&dir) {
            Ok(()) => return Ok(dir),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(err(e)),
        }
    }
    Err("Нет свободного имени каталога".into())
}
struct PrintWorker {
    cancel: Arc<AtomicBool>,
    result: mpsc::Receiver<Result<String, String>>,
}
impl Drop for PrintWorker {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Release);
    }
}
pub(crate) struct Tools {
    job: RefCell<Option<PrintWorker>>,
    job_status: WidgetId,
    job_waker: RefCell<Option<rust_desktop_ui_platform_winit::AppWaker>>,
    range: WidgetId,
    selected: WidgetId,
    paper: WidgetId,
    margin: WidgetId,
    printer: WidgetId,
    grid: WidgetId,
    host: WidgetId,
    status: WidgetId,
    criteria: WidgetId,
    rule_index: WidgetId,
    rule_list: WidgetId,
    name: WidgetId,
    branches: RefCell<rust_desktop_ui_controls::GridPageBranches>,
    pages: RefCell<GridPageView>,
    preview: Option<PageViewer>,
}
impl Tools {
    pub(crate) fn new(
        ui: &mut Controls,
        view: &MountedView,
        grid: WidgetId,
    ) -> Result<Self, String> {
        let find = |name| {
            view.find_name(name)
                .ok_or_else(|| format!("Missing {name}"))
        };
        let schema = virtual_schema()?;
        let pages = PagedDataSource::new(
            schema,
            Default::default(),
            PageCacheSettings {
                page_size: 64,
                max_rows: 256,
                max_pages: 4,
                ..Default::default()
            },
            QueryCapabilities {
                grouping: false,
                aggregates: false,
                ..QueryCapabilities::complete()
            },
        )
        .map_err(err)?;
        let pages = GridPageView::mount(
            ui,
            find("WorkspaceProviderHost")?,
            pages,
            LayoutStyle {
                height: Length::Px(180.),
                ..Default::default()
            },
        )
        .map_err(err)?;
        let menu = |label: &str, cmd: &str| {
            rust_desktop_ui_controls::MenuItem::new(label, format!("GridWorkspace{cmd}"))
        };
        ui.set_grid_context_menus(
            grid,
            vec![
                menu("Подобрать ширину", "FitColumn"),
                menu("Выбрать колонки", "Chooser"),
            ],
            vec![menu("Свернуть группу", "CollapseContext")],
            vec![
                menu("Закрепить запись", "PinRecord"),
                menu("Изменить запись", "ContextEdit"),
            ],
        )
        .map_err(err)?;
        Ok(Self {
            job: Default::default(),
            job_waker: Default::default(),
            job_status: find("WorkspacePrintStatus")?,
            range: find("WorkspacePrintRange")?,
            selected: find("WorkspacePrintSelected")?,
            paper: find("WorkspacePaper")?,
            margin: find("WorkspaceMargin")?,
            printer: find("WorkspacePrinter")?,
            grid,
            host: find("WorkspacePreviewHost")?,
            status: find("WorkspaceProviderStatus")?,
            criteria: find("WorkspaceStyleCriteria")?,
            rule_index: find("WorkspaceRuleIndex")?,
            rule_list: find("WorkspaceRuleList")?,
            name: find("WorkspaceLayoutName")?,
            branches: RefCell::new(rust_desktop_ui_controls::GridPageBranches::new(
                find("WorkspaceBranchesHost")?,
                pages.source(),
            )),
            pages: RefCell::new(pages),
            preview: None,
        })
    }
    pub(crate) fn wake(
        &self,
        waker: Option<rust_desktop_ui_platform_winit::AppWaker>,
    ) -> Result<(), String> {
        self.job_waker.replace(waker.clone());
        let waker = waker.map(|w| {
            Arc::new(move || {
                w.wake();
            }) as Arc<dyn Fn() + Send + Sync>
        });
        self.branches
            .borrow_mut()
            .set_waker(waker.clone())
            .map_err(err)?;
        self.pages.borrow_mut().set_waker(waker).map_err(err)
    }
    pub(crate) fn refresh(&self, ui: &mut Controls) -> Result<(), String> {
        let result = self
            .job
            .borrow()
            .as_ref()
            .and_then(|job| match job.result.try_recv() {
                Ok(result) => Some(result),
                Err(mpsc::TryRecvError::Disconnected) => {
                    Some(Err("Поток печати завершился".into()))
                }
                Err(mpsc::TryRecvError::Empty) => None,
            });
        if let Some(result) = result {
            self.job.replace(None);
            ui.set_label(
                self.job_status,
                result.unwrap_or_else(|e| format!("Печать: {e}")),
            )
            .map_err(err)?;
        }
        for (req, sender) in self.branches.borrow_mut().poll(ui).map_err(err)? {
            launch(req, sender);
        }
        let rules = &ui
            .grid_presentation(self.grid)
            .ok_or("Missing grid")?
            .format_rules;
        let labels = rules
            .iter()
            .enumerate()
            .map(|(i, r)| {
                format!(
                    "{}. {} · {} · {}",
                    i + 1,
                    r.field,
                    match &r.kind {
                        GridFormatKind::Expression(s) => s.clone(),
                        other => format!("{other:?}"),
                    },
                    if r.enabled {
                        "включено"
                    } else {
                        "отключено"
                    }
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        ui.set_label(self.rule_list, labels).map_err(err)?;
        let mut page = self.pages.borrow_mut();
        let requests = page.take_input(ui).map_err(err)?;
        for req in requests {
            launch(req, page.source().completion_sender());
        }
        page.poll(ui).map_err(err)?;
        let count = match page.source().total() {
            PageTotal::Known(n) => n.to_string(),
            PageTotal::Unknown => "—".into(),
        };
        let state = match page.status() {
            PageStatus::Ready { rows, .. } => format!("Строк на странице: {rows}"),
            PageStatus::Failed(reason) => format!("Ошибка: {reason}"),
            PageStatus::Pending(_) => "Загрузка…".into(),
            PageStatus::Missing => "Откройте страницу".into(),
        };
        ui.set_label(
            self.status,
            format!("{state} · смещение {} · всего {count}", page.offset()),
        )
        .map_err(err)
    }
    pub(crate) fn activate(&mut self, ui: &mut Controls, id: WidgetId) -> Result<(), String> {
        if let Some(v) = &mut self.preview {
            v.activate(ui, id).map_err(err)?;
        }
        Ok(())
    }
    fn print_options(&self, ui: &Controls) -> Result<GridPrintOptions, String> {
        let paper = paper_size(ui.document(self.paper).unwrap().text())?;
        Ok(GridPrintOptions {
            paper,
            margin: ui
                .document(self.margin)
                .unwrap()
                .text()
                .trim()
                .parse()
                .map_err(err)?,
            range: parse_range(ui.document(self.range).unwrap().text())?,
            ..Default::default()
        })
    }
    pub(crate) fn command(
        &mut self,
        ui: &mut Controls,
        cmd: &str,
    ) -> Result<Option<String>, String> {
        let grid = self.grid;
        let message = match cmd {
            "AddFormat" | "RaiseFormat" | "LowerFormat" | "ToggleFormat" | "RemoveFormat" => {
                let mut rules = ui.grid_presentation(grid).unwrap().format_rules.clone();
                let active = if cmd == "AddFormat" {
                    rules.push(GridFormatRule {
                        field: "Title".into(),
                        kind: GridFormatKind::Expression(
                            ui.document(self.criteria).unwrap().text().to_owned(),
                        ),
                        whole_row: true,
                        enabled: true,
                    });
                    rules.len() - 1
                } else {
                    let i = ui
                        .document(self.rule_index)
                        .unwrap()
                        .text()
                        .trim()
                        .parse::<usize>()
                        .ok()
                        .and_then(|i| i.checked_sub(1))
                        .filter(|i| *i < rules.len())
                        .ok_or("Укажите номер правила из списка")?;
                    match cmd {
                        "RaiseFormat" if i > 0 => {
                            rules.swap(i, i - 1);
                            i - 1
                        }
                        "LowerFormat" if i + 1 < rules.len() => {
                            rules.swap(i, i + 1);
                            i + 1
                        }
                        "ToggleFormat" => {
                            rules[i].enabled = !rules[i].enabled;
                            i
                        }
                        "RemoveFormat" => {
                            rules.remove(i);
                            i.min(rules.len().saturating_sub(1))
                        }
                        _ => i,
                    }
                };
                let count = rules.len();
                ui.set_grid_format_rules(grid, rules).map_err(err)?;
                ui.set_editor_value(self.rule_index, &(active + 1).to_string())
                    .map_err(err)?;
                format!("Правил оформления: {count}")
            }
            "SaveLayout" | "RestoreLayout" => {
                let name = ui.document(self.name).unwrap().text().to_owned();
                let path = layout_path(&name)?;
                if cmd == "SaveLayout" {
                    let text = ui.save_grid_layout(grid, &name).map_err(err)?;
                    crate::output_app::atomic_save(&path, text.as_bytes(), &false.into())?;
                } else {
                    ui.restore_grid_layout(
                        grid,
                        &name,
                        &String::from_utf8(crate::output_app::read_bounded(&path, 1024 * 1024)?)
                            .map_err(err)?,
                    )
                    .map_err(err)?;
                }
                path.display().to_string()
            }
            "FitColumn" => {
                let field = match ui.grid_context_target(grid) {
                    Some(rust_desktop_ui_controls::GridContextTarget::Column(f)) => f.clone(),
                    _ => "Title".into(),
                };
                format!(
                    "Ширина {field}: {:.0}",
                    ui.auto_fit_grid_column(grid, &field).map_err(err)?
                )
            }
            "CollapseContext" => {
                if let Some(rust_desktop_ui_controls::GridContextTarget::Group(r)) =
                    ui.grid_context_target(grid)
                {
                    ui.set_grid_expanded(grid, *r, false).map_err(err)?;
                }
                "Группа свёрнута".into()
            }
            "PinRecord" => {
                let key = match ui.grid_context_target(grid) {
                    Some(rust_desktop_ui_controls::GridContextTarget::Record(key)) => Some(*key),
                    _ => ui.grid_selected_key(grid),
                }
                .ok_or("Выберите запись")?;
                ui.set_grid_navigation_rows(grid, vec![key], vec![key])
                    .map_err(err)?;
                format!("Закреплена запись {key}")
            }
            "CancelPrint" => {
                if let Some(job) = self.job.borrow().as_ref() {
                    job.cancel.store(true, Ordering::Release);
                }
                "Запрошена отмена печати".into()
            }
            "Preview" | "Print" => {
                let options = self.print_options(ui)?;
                let selected = ui.checked(self.selected) == Some(true);
                let output = ui.grid_output(grid, &[], selected).map_err(err)?;
                if cmd == "Print" {
                    if self.job.borrow().is_some() {
                        return Err("Дождитесь окончания задания печати".into());
                    }
                    let printer = ui.document(self.printer).unwrap().text();
                    if printer.trim().is_empty() {
                        return Err("Укажите имя принтера".into());
                    }
                    let ticket =
                        rust_desktop_ui_platform_winit::printing::PrintTicket::new(printer);
                    let flag = Arc::new(AtomicBool::new(false));
                    let cancel = flag.clone();
                    let (tx, result) = mpsc::channel();
                    let waker = self.job_waker.borrow().clone();
                    std::thread::Builder::new()
                        .name("grid-print".into())
                        .spawn(move || {
                            let result = output
                                .print_document(options, || cancel.load(Ordering::Acquire))
                                .map_err(err)
                                .and_then(|doc| print_grid_document(&doc, ticket, &cancel))
                                .map(|receipt| format!("Задание передано принтеру: {receipt:?}"));
                            let _ = tx.send(result);
                            if let Some(waker) = waker {
                                waker.wake();
                            }
                        })
                        .map_err(err)?;
                    self.job.replace(Some(PrintWorker {
                        cancel: flag,
                        result,
                    }));
                    ui.set_label(self.job_status, "Подготовка задания…")
                        .map_err(err)?;
                    return Ok(Some("Печать запущена".into()));
                }
                let doc = output.print_document(options, || false).map_err(err)?;
                let next = PageViewer::print_preview(
                    ui,
                    self.host,
                    doc,
                    LayoutStyle {
                        height: Length::Px(240.),
                        ..Default::default()
                    },
                )
                .map_err(err)?;
                if let Some(old) = self.preview.replace(next) {
                    ui.remove(old.parts.root).map_err(err)?
                }
                "Предпросмотр выбранных страниц".into()
            }
            "Export" => {
                let dir = export_dir()?;
                save_output(
                    ui,
                    grid,
                    &dir,
                    ui.checked(self.selected) == Some(true),
                    self.print_options(ui)?,
                )?;
                dir.display().to_string()
            }
            "RegionZero" | "RegionOne" | "CollapseBranches" => {
                let mut branches = self.branches.borrow_mut();
                if cmd == "CollapseBranches" {
                    branches.collapse(ui, &[]).map_err(err)?;
                } else {
                    let value = i64::from(cmd == "RegionOne");
                    let rows = branches
                        .expand(ui, vec![("Region".into(), CellValue::Integer(value))])
                        .map_err(err)?;
                    for (req, sender) in rows {
                        launch(req, sender);
                    }
                }
                "Обновлены раскрытые группы".into()
            }
            "FirstPage" | "NextPage" | "LastPage" | "RetryPage" | "Branch" | "RootPages" => {
                let mut view = self.pages.borrow_mut();
                let size = view.source().settings().page_size as u64;
                let req = match cmd {
                    "Branch" => view.open_path(ui, vec![("Region".into(), CellValue::Integer(1))]),
                    "RootPages" => view.open_path(ui, vec![]),
                    "RetryPage" => view.retry().map(|r| vec![r]),
                    _ => {
                        let offset = match cmd {
                            "NextPage" => view.offset() + size,
                            "LastPage" => match view.source().total() {
                                PageTotal::Known(n) => n.saturating_sub(1),
                                PageTotal::Unknown => 999_999,
                            },
                            _ => 0,
                        };
                        view.navigate(ui, offset)
                    }
                }
                .map_err(err)?;
                let sender = view.source().completion_sender();
                for request in req {
                    launch(request, sender.clone());
                }
                "Запрошена страница".into()
            }
            _ => return Ok(None),
        };
        Ok(Some(message))
    }
}
fn virtual_schema() -> Result<TableSchema, String> {
    let mut fields = vec![
        FieldSchema::new("Number", FieldType::Integer),
        FieldSchema::new("Region", FieldType::Integer),
    ];
    fields.extend((1..=30).map(|i| FieldSchema::new(format!("Value{i}"), FieldType::Integer)));
    TableSchema::new(fields).map_err(err)
}
fn launch(request: PageRequest, sender: PageCompletionSender) {
    std::thread::spawn(move || {
        if request.cancellation.is_cancelled() {
            return;
        }
        let result = (|| {
            let region = match &request.query.filter {
                None => None,
                Some(Criteria::Compare {
                    left: QueryExpression::Field(f),
                    operator: ComparisonOperator::Equal,
                    right: QueryExpression::Literal(CellValue::Integer(n)),
                }) if f == "Region" && (0..4).contains(n) => Some(*n),
                _ => return Err("Unsupported generated query".into()),
            };
            let direction = request.query.order_by.first().map(|s| s.direction);
            if request.query.order_by.len() > 1
                || request.query.order_by.first().is_some_and(
                    |o| !matches!(&o.expression,QueryExpression::Field(f) if f!="Region"),
                )
            {
                return Err(
                    "Этот демонстрационный источник сортирует по одному числовому полю".into(),
                );
            }
            let total = if region.is_some() { 250_000 } else { 1_000_000 };
            let end = (request.offset + request.limit as u64).min(total);
            let rows = (request.offset..end)
                .map(|index| {
                    let index = if direction == Some(SortDirection::Descending) {
                        total - 1 - index
                    } else {
                        index
                    };
                    let n = if let Some(region) = region {
                        index as i64 * 4 + region
                    } else {
                        index as i64
                    };
                    let mut cells = vec![CellValue::Integer(n), CellValue::Integer(n % 4)];
                    cells.extend((1..=30).map(|i| CellValue::Integer(n + i)));
                    DataRow::new(n, cells)
                })
                .collect();
            Ok(PageResponse {
                table: DataTable::new(virtual_schema()?, rows).map_err(err)?,
                total: PageTotal::Known(total),
                has_more: end < total,
                snapshot_revision: 1,
            })
        })();
        let mut payload = result;
        loop {
            match sender.try_complete(&request, payload) {
                Ok(()) => break,
                Err(e) if matches!(e.error, PageSendError::Full | PageSendError::Busy) => {
                    payload = e.result;
                    std::thread::sleep(std::time::Duration::from_millis(2));
                }
                Err(_) => break,
            }
        }
    });
}
fn print_grid_document(
    document: &rust_desktop_ui_core::pages::PrintDocument,
    mut ticket: rust_desktop_ui_platform_winit::printing::PrintTicket,
    cancel: &AtomicBool,
) -> Result<rust_desktop_ui_platform_winit::printing::PrintReceipt, String> {
    use rust_desktop_ui_platform_winit::printing::PrintMedia;
    let paper = document.pages()[0].size;
    ticket.media = Some(
        match (paper.width.round() as u32, paper.height.round() as u32) {
            (794, 1123) => PrintMedia::A4,
            (1123, 794) => PrintMedia::A4Landscape,
            (559, 794) => PrintMedia::A5,
            (816, 1056) => PrintMedia::Letter,
            _ => return Err("Неизвестный формат бумаги".into()),
        },
    );
    let prepared = printing::prepare_for_printer(
        document,
        &mut rust_desktop_ui_text::TextEngine::new(),
        2.,
        || cancel.load(Ordering::Acquire),
    )
    .map_err(err)?;
    rust_desktop_ui_platform_winit::printing::print(&prepared, &ticket, cancel).map_err(err)
}
fn paper_size(text: &str) -> Result<Size, String> {
    match text.trim().to_ascii_uppercase().as_str() {
        "A4" => Ok(Size::new(794., 1123.)),
        "A4-L" => Ok(Size::new(1123., 794.)),
        "A5" => Ok(Size::new(559., 794.)),
        "LETTER" => Ok(Size::new(816., 1056.)),
        _ => Err("Поддерживаются A4, A4-L, A5, Letter".into()),
    }
}
fn parse_range(text: &str) -> Result<Option<(usize, usize)>, String> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(None);
    }
    let (a, b) = text.split_once('-').unwrap_or((text, text));
    let a = a.trim().parse::<usize>().map_err(err)?;
    let b = b.trim().parse::<usize>().map_err(err)?;
    if a == 0 || b < a {
        return Err("Неверный диапазон страниц".into());
    }
    Ok(Some((a, b)))
}
fn save_output(
    ui: &Controls,
    grid: WidgetId,
    dir: &Path,
    selected: bool,
    options: GridPrintOptions,
) -> Result<(), String> {
    let out = ui.grid_output(grid, &[], selected).map_err(err)?;
    let doc = out.print_document(options, || false).map_err(err)?;
    let pdf = printing::export_pdf(&doc, &mut rust_desktop_ui_text::TextEngine::new(), || false)
        .map_err(err)?;
    let files = [
        ("grid.csv", out.csv().into_bytes()),
        ("grid.xlsx", out.xlsx().map_err(err)?),
        ("grid.html", out.html().into_bytes()),
        ("grid.rtf", out.rtf().into_bytes()),
        ("grid.xls", out.xls().map_err(err)?),
        ("preview.pdf", pdf),
    ];
    for (name, data) in files {
        std::fs::write(dir.join(name), data).map_err(err)?
    }
    Ok(())
}
/// Headless entry point for the same compiled workspace source and presentation.
pub(crate) fn cli(command: &str, args: Vec<String>) -> Result<(), String> {
    let mut ui = Controls::new(Theme::FluentLight).map_err(err)?;
    ui.commands_mut().register("Light", "Light");
    ui.commands_mut().register("Dark", "Dark");
    let sources = demo::register(&mut ui)?;
    if command == "--grid-design" && args == ["--properties"] {
        for field in sources.master.borrow().table().schema().fields() {
            println!(
                "{}\t{:?}\tnullable={}\tread_only={}",
                field.name, field.kind, field.nullable, field.read_only
            )
        }
        println!(
            "GridColumn: FieldName Header Width MinWidth MaxWidth IsPinned IsReadOnly Expression GroupExpression ValueSource ValueMember"
        );
        return Ok(());
    }
    if command == "--grid-design" {
        println!(
            "{}",
            rust_desktop_ui_xaml::design_grid_form(
                "WorkspaceRecords",
                sources.master.borrow().table().schema(),
                &args
            )?
        );
        return Ok(());
    }
    let definition = crate::generated_grid_workspace::window_definition();
    let root = ui.root();
    let view = MountedView::mount_with_context(&mut ui, root, &definition.content, demo::model()?)?;
    let grid = view
        .find_name("WorkspaceGrid")
        .ok_or("Missing workspace grid")?;
    ui.update(Size::new(1280., 800.)).map_err(err)?;
    if command == "--grid-print-file" {
        if !(3..=5).contains(&args.len()) {
            return Err(
                "Укажите принтер, новый PDF-файл, фильтр и при необходимости бумагу и диапазон"
                    .into(),
            );
        }
        let path = std::path::absolute(&args[1]).map_err(err)?;
        if path.exists() {
            return Err("Файл назначения уже существует".into());
        }
        ui.set_grid_filter(grid, &args[2]).map_err(err)?;
        let doc = ui
            .grid_output(grid, &[], false)
            .map_err(err)?
            .print_document(
                GridPrintOptions {
                    paper: paper_size(args.get(3).map_or("A4", String::as_str))?,
                    range: parse_range(args.get(4).map_or("", String::as_str))?,
                    ..Default::default()
                },
                || false,
            )
            .map_err(err)?;
        let mut ticket = rust_desktop_ui_platform_winit::printing::PrintTicket::new(&args[0]);
        ticket.output = Some(path);
        print_grid_document(&doc, ticket, &false.into())?;
        return Ok(());
    }
    if args.is_empty() || args.len() > 2 {
        return Err("Укажите каталог вывода или принтер и необязательный текст фильтра".into());
    }
    if let Some(filter) = args.get(1) {
        ui.set_grid_filter(grid, filter).map_err(err)?;
    }
    if command == "--grid-print" {
        let pages = ui
            .grid_output(grid, &[], false)
            .map_err(err)?
            .print_document(GridPrintOptions::default(), || false)
            .map_err(err)?;
        let ticket = rust_desktop_ui_platform_winit::printing::PrintTicket::new(&args[0]);
        print_grid_document(&pages, ticket, &false.into())?;
        return Ok(());
    }
    let dir = Path::new(&args[0]);
    std::fs::create_dir(dir).map_err(err)?;
    save_output(
        &ui,
        grid,
        dir,
        false,
        GridPrintOptions {
            range: if args.len() == 2 { None } else { Some((1, 1)) },
            ..Default::default()
        },
    )
}
