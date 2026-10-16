//! Disposable application fixtures for transactional sources and protected editors.
use crate::view::MountedView;
use rust_desktop_ui_controls::{BindingContext, Controls, DataValue, PasswordEventKind};
use rust_desktop_ui_core::WidgetId;
use rust_desktop_ui_data::*;
use std::{cell::RefCell, rc::Rc};

type Shared = Rc<RefCell<DataSource>>;
type Model = (Rc<RefCell<BindingContext>>, Vec<(String, Shared)>);

fn schema() -> Result<TableSchema, String> {
    TableSchema::new(vec![
        FieldSchema::new("Name", FieldType::Text),
        FieldSchema::new("Quantity", FieldType::Integer),
    ])
    .map_err(err)
}
fn row(key: i64) -> DataRow {
    DataRow::new(
        key,
        vec![
            CellValue::Text(format!("Документ {key}")),
            CellValue::Integer(key * 2),
        ],
    )
}
fn database(count: i64, limits: DataLimits) -> Result<SqliteAdapter, String> {
    let mut connection = SqliteConnection::open_in_memory().map_err(err)?;
    connection.execute_batch("CREATE TABLE Items (Id INTEGER PRIMARY KEY, Name TEXT NOT NULL, Quantity INTEGER NOT NULL CHECK(Quantity>=0))").map_err(err)?;
    let transaction = connection.transaction().map_err(err)?;
    for key in 1..=count {
        transaction
            .execute(
                "INSERT INTO Items VALUES(?1,?2,?3)",
                (key, format!("Документ {key}"), key * 2),
            )
            .map_err(err)?;
    }
    transaction.commit().map_err(err)?;
    SqliteAdapter::with_limits(
        connection,
        SqliteTableConfig::new("Items", "Id", schema()?).map_err(err)?,
        limits,
    )
    .map_err(err)
}
pub(crate) fn model() -> Result<Model, String> {
    let memory = DataSource::new(CollectionAdapter::new(
        DataTable::new(schema()?, (1..=6).map(row).collect()).map_err(err)?,
    ))
    .map_err(err)?;
    let sqlite = DataSource::new(database(6, DataLimits::default())?).map_err(err)?;
    let context = BindingContext::new(DataValue::object([
        (
            "Notes",
            "Первая строка заметки\nВторая строка: 👩‍💻 и e\u{301}".into(),
        ),
        ("Reference", "DOC-001".into()),
        ("Amount", "1234.56".into()),
        ("OptionalAmount", DataValue::Null),
        ("PreciseAmount", "9007199254740993.01".into()),
        ("DocumentDateTime", "2026-10-16T10:15:42".into()),
        ("Phone", "9123456789".into()),
        ("MaskedCode", "AB1234".into()),
        (
            "Period",
            DataValue::object([("start", "2026-10-16".into()), ("end", "2026-10-20".into())]),
        ),
    ]))
    .map_err(err)?;
    Ok((
        Rc::new(RefCell::new(context)),
        vec![
            ("TransactionMemory".into(), Rc::new(RefCell::new(memory))),
            ("TransactionSqlite".into(), Rc::new(RefCell::new(sqlite))),
        ],
    ))
}
pub(crate) fn register(ui: &mut Controls) {
    for (id, label) in [
        ("DataEditorsMemoryBatch", "Пакет в коллекцию"),
        ("DataEditorsSqliteBatch", "Пакет в SQLite"),
        ("DataEditorsConflict", "Проверить конфликт"),
        ("DataEditorsRollback", "Проверить откат"),
        ("DataEditorsPage", "Следующая страница SQLite"),
        ("DataEditorsReference", "Выбрать номер"),
        ("DataEditorsClearReference", "Очистить номер"),
        ("DataEditorsHelp", "Открыть справку"),
        ("DataEditorsSubmitPassword", "Передать значение"),
        ("DataEditorsButtonFace", "Сменить содержимое кнопки"),
    ] {
        ui.commands_mut().register(id, label);
    }
}
pub(crate) struct State {
    memory: Shared,
    sqlite: Shared,
    first: DataView,
    filtered: DataView,
    pages: SqliteAdapter,
    offset: usize,
    status: WidgetId,
    views: WidgetId,
    page: WidgetId,
    password: WidgetId,
    password_summary: WidgetId,
    reference: WidgetId,
    last_views: String,
    reference_button: WidgetId,
    alternate_face: bool,
}
pub(crate) fn install(ui: &mut Controls, view: &MountedView) -> Result<Option<State>, String> {
    let Some(status) = view.find_name("DataEditorsStatus") else {
        return Ok(None);
    };
    let named = |name| {
        view.find_name(name)
            .ok_or_else(|| format!("Missing data editor {name}"))
    };
    let memory = ui
        .data_source("TransactionMemory")
        .ok_or("Missing collection fixture")?;
    let sqlite = ui
        .data_source("TransactionSqlite")
        .ok_or("Missing SQLite fixture")?;
    let first = DataView::new(&memory.borrow(), DataQuery::default()).map_err(err)?;
    let filtered = DataView::new(
        &memory.borrow(),
        DataQuery {
            filter: "Документ 1".into(),
            sort: vec![],
        },
    )
    .map_err(err)?;
    let reference = named("DecoratedReference")?;
    let reference_button = ui
        .editor_control_parts(reference)
        .ok_or("Missing reference buttons")?
        .buttons[0];
    let reference = ui
        .composite_editor(reference)
        .ok_or("Missing reference editor")?;
    Ok(Some(State {
        memory,
        sqlite,
        first,
        filtered,
        pages: database(
            500,
            DataLimits {
                max_rows: 10,
                ..Default::default()
            },
        )?,
        offset: 0,
        status,
        views: named("IndependentViews")?,
        page: named("PageSummary")?,
        password: named("AccountPassword")?,
        password_summary: named("PasswordSummary")?,
        reference,
        last_views: String::new(),
        reference_button,
        alternate_face: false,
    }))
}
impl State {
    pub(crate) fn refresh(&mut self, ui: &mut Controls) -> Result<(), String> {
        let memory = self.memory.borrow();
        self.first.synchronize(&memory).map_err(err)?;
        self.filtered.synchronize(&memory).map_err(err)?;
        let label = format!(
            "Один источник · все строки: {} · фильтр «Документ 1»: {} · ревизия {}",
            self.first.len(),
            self.filtered.len(),
            memory.revision()
        );
        if label != self.last_views {
            ui.set_label(self.views, &label).map_err(err)?;
            self.last_views = label;
        }
        drop(memory);
        for event in ui.drain_password_events() {
            if event.id == self.password && event.kind == PasswordEventKind::Submitted {
                self.submit_password(ui)?;
            }
        }
        Ok(())
    }
    fn submit_password(&self, ui: &mut Controls) -> Result<(), String> {
        let password = ui.take_password_value(self.password).map_err(err)?;
        let message = if password.is_empty() {
            "Пустое значение не отправлено"
        } else {
            "Значение передано обработчику и удалено из поля"
        };
        ui.set_label(self.password_summary, message).map_err(err)
    }
    pub(crate) fn command(
        &mut self,
        ui: &mut Controls,
        view: &mut MountedView,
        command: &str,
    ) -> Result<bool, String> {
        if !command.starts_with("DataEditors") {
            return Ok(false);
        }
        let result = match command {
            "DataEditorsMemoryBatch" => apply_batch(&self.memory),
            "DataEditorsSqliteBatch" => apply_batch(&self.sqlite),
            "DataEditorsConflict" => reject_batch(&self.memory, true),
            "DataEditorsRollback" => reject_batch(&self.sqlite, false),
            "DataEditorsPage" => {
                let page = self
                    .pages
                    .query_page(&QueryRequest::default(), self.offset, 10)
                    .map_err(err)?;
                let keys = page
                    .table
                    .rows()
                    .iter()
                    .map(|row| row.key.to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                ui.set_label(
                    self.page,
                    format!(
                        "Строки: {keys} · всего {} · {:?}",
                        page.total_rows, page.execution
                    ),
                )
                .map_err(err)?;
                self.offset = if self.offset + 10 >= page.total_rows {
                    0
                } else {
                    self.offset + 10
                };
                Ok("Страница загружена по явной команде".into())
            }
            "DataEditorsReference" => {
                ui.set_editor_value(self.reference, "DOC-042")
                    .map_err(err)?;
                Ok("Номер выбран".into())
            }
            "DataEditorsClearReference" => {
                ui.set_editor_value(self.reference, "").map_err(err)?;
                Ok("Номер очищен".into())
            }
            "DataEditorsHelp" => Ok("Справка доступна при режиме только чтения".into()),
            "DataEditorsSubmitPassword" => {
                self.submit_password(ui)?;
                Ok("Обработчик защищённого поля вызван".into())
            }
            "DataEditorsButtonFace" => {
                let content = if self.alternate_face {
                    "<TextBlock Text='ID' FontWeight='SemiBold'/>"
                } else {
                    "<Icon Symbol='Check' Width='20' Height='20'/>"
                };
                let definition = rust_desktop_ui_xaml::parse(&format!(
                    "<Window xmlns='urn:rust-ui-engine:ui'>{content}</Window>"
                ))
                .map_err(err)?;
                view.replace_editor_button_template(
                    ui,
                    self.reference_button,
                    &definition.content,
                )?;
                self.alternate_face = !self.alternate_face;
                Ok("Содержимое кнопки изменено; редактор и его черновик сохранены".into())
            }
            _ => return Ok(false),
        };
        ui.set_label(
            self.status,
            result.unwrap_or_else(|error| format!("Операция отклонена: {error}")),
        )
        .map_err(err)?;
        self.refresh(ui)?;
        Ok(true)
    }
}
fn apply_batch(source: &Shared) -> Result<String, String> {
    let mut source = source.borrow_mut();
    let before = source.revision();
    let next = source
        .table()
        .rows()
        .iter()
        .map(|row| row.key)
        .max()
        .unwrap_or(0)
        + 1;
    let old = source
        .table()
        .value(1, "Quantity")
        .cloned()
        .ok_or("Missing first row")?;
    let CellValue::Integer(quantity) = old else {
        return Err("Invalid quantity".into());
    };
    source
        .write_batch(&[
            DataWrite::Update {
                key: 1,
                field: "Quantity".into(),
                expected: CellValue::Integer(quantity),
                value: CellValue::Integer(quantity + 1),
            },
            DataWrite::Insert(row(next)),
        ])
        .map_err(err)?;
    Ok(format!(
        "Применены две операции · ревизия {before} → {}",
        source.revision()
    ))
}
fn reject_batch(source: &Shared, conflict: bool) -> Result<String, String> {
    let mut source = source.borrow_mut();
    let before = source.table().clone();
    let revision = source.revision();
    let old = before
        .value(1, "Quantity")
        .cloned()
        .ok_or("Missing first row")?;
    let second = before
        .value(2, "Quantity")
        .cloned()
        .ok_or("Missing second row")?;
    let result = source.write_batch(&[
        DataWrite::Update {
            key: 1,
            field: "Quantity".into(),
            expected: old,
            value: CellValue::Integer(777),
        },
        DataWrite::Update {
            key: 2,
            field: "Quantity".into(),
            expected: if conflict {
                CellValue::Integer(-999)
            } else {
                second
            },
            value: CellValue::Integer(-1),
        },
    ]);
    if result.is_ok() || source.table() != &before || source.revision() != revision {
        return Err("Нарушена атомарность пакета".into());
    }
    source.refresh().map_err(err)?;
    if source.table() != &before {
        return Err("Хранилище изменилось после отклонения пакета".into());
    }
    Ok(if conflict {
        "Конфликт обнаружен: данные и ревизия сохранены"
    } else {
        "SQLite отклонил вторую операцию: весь пакет отменён"
    }
    .into())
}
fn err(error: impl std::fmt::Display) -> String {
    error.to_string()
}
