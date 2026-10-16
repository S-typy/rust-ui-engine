//! Application-owned records and commands for table/card presentation.
use crate::view::MountedView;
use rust_desktop_ui_controls::{
    BindingContext, Controls, DataSourceHandle, DataValue, GridViewMode,
};
use rust_desktop_ui_core::WidgetId;
use rust_desktop_ui_data::{
    CellValue, CollectionAdapter, DataRow, DataSource, DataTable, DateValue, DecimalValue,
    FieldSchema, FieldType, TableSchema,
};
use rust_desktop_ui_treegrid::RowKey;
use std::{cell::RefCell, rc::Rc};

#[derive(Clone)]
pub(crate) struct Sources {
    pub master: DataSourceHandle,
    pub detail: DataSourceHandle,
}
pub(crate) fn model() -> Result<Rc<RefCell<BindingContext>>, String> {
    Ok(Rc::new(RefCell::new(
        BindingContext::new(DataValue::object([
            ("SelectedKey", i64::MIN.to_string().into()),
            ("Filter", "".into()),
            ("ReadOnly", false.into()),
        ]))
        .map_err(err)?,
    )))
}
pub(crate) fn register(ui: &mut Controls) -> Result<Sources, String> {
    let fields = [
        ("RecordId", "Ключ", FieldType::Integer),
        ("Code", "Код", FieldType::Text),
        ("Category", "Категория", FieldType::Text),
        ("Title", "Наименование", FieldType::Text),
        ("Quantity", "Количество", FieldType::Integer),
        ("Price", "Сумма", FieldType::Decimal),
        ("Due", "Дата", FieldType::Date),
        ("Active", "Согласовано", FieldType::Bool),
    ]
    .into_iter()
    .map(|(name, title, kind)| {
        let mut field = FieldSchema::new(name, kind);
        field.title = title.into();
        field.read_only = matches!(name, "RecordId" | "Code");
        field.nullable = name == "Active";
        field
    })
    .collect();
    let mut records = Vec::new();
    let mut details = Vec::new();
    for index in 0..10_000_i64 {
        let key = match index {
            0 => i64::MIN,
            1 => i64::MAX,
            _ => index,
        };
        records.push(DataRow::new(
            key,
            vec![
                CellValue::Integer(key),
                CellValue::Text(format!("DOC-{index:05}")),
                CellValue::Text(["Разработка", "Поставка", "Поддержка"][index as usize % 3].into()),
                CellValue::Text(format!("Рабочая запись {index:05}")),
                CellValue::Integer(if index == 0 {
                    9_007_199_254_740_993
                } else {
                    index % 100 + 1
                }),
                CellValue::Decimal(
                    DecimalValue::new(125_025 + i128::from(index) * 175, 2).map_err(err)?,
                ),
                CellValue::Date(DateValue::new(2026, 10, (index % 28 + 1) as u8).map_err(err)?),
                if index % 5 == 0 {
                    CellValue::Null
                } else {
                    CellValue::Bool(index % 2 == 0)
                },
            ],
        ));
        if index < 20 {
            for item in 0..3 {
                details.push(DataRow::new(
                    index * 10 + item,
                    vec![
                        CellValue::Integer(key),
                        CellValue::Text(format!("Позиция {}", item + 1)),
                        CellValue::Integer(item + 1),
                        CellValue::Decimal(
                            DecimalValue::new(i128::from((item + 1) * 25075), 2).map_err(err)?,
                        ),
                    ],
                ));
            }
        }
    }
    let source = |schema, rows| -> Result<DataSourceHandle, String> {
        Ok(Rc::new(RefCell::new(
            DataSource::new(CollectionAdapter::new(
                DataTable::new(schema, rows).map_err(err)?,
            ))
            .map_err(err)?,
        )))
    };
    let master = source(TableSchema::new(fields).map_err(err)?, records)?;
    let detail = source(
        TableSchema::new(vec![
            FieldSchema::new("OwnerId", FieldType::Integer),
            FieldSchema::new("Caption", FieldType::Text),
            FieldSchema::new("Quantity", FieldType::Integer),
            FieldSchema::new("Price", FieldType::Decimal),
        ])
        .map_err(err)?,
        details,
    )?;
    ui.register_data_source("WorkspaceRecords", master.clone())
        .map_err(err)?;
    ui.register_data_source("WorkspaceDetails", detail.clone())
        .map_err(err)?;
    for (id, label) in [
        ("Table", "Таблица"),
        ("Cards", "Карточки"),
        ("Group", "Группировка"),
        ("Details", "Подробности"),
        ("Reorder", "Переставить колонки"),
        ("Pin", "Закрепить категорию"),
        ("Date", "Показать дату"),
        ("Quantity", "Количество"),
        ("Price", "Сумма"),
        ("Due", "Дата"),
        ("Active", "Согласовано"),
        ("Commit", "Подтвердить"),
        ("Cancel", "Отменить"),
        ("Conflict", "Конфликт записи"),
    ] {
        ui.commands_mut()
            .register(format!("GridWorkspace{id}"), label);
    }
    Ok(Sources { master, detail })
}
pub(crate) struct State {
    grid: WidgetId,
    summary: WidgetId,
    stored: WidgetId,
    status: WidgetId,
    sources: Sources,
    external: i64,
}
impl State {
    pub(crate) fn install(
        ui: &mut Controls,
        view: &MountedView,
        sources: Sources,
    ) -> Result<Option<Self>, String> {
        let Some(grid) = view.find_name("WorkspaceGrid") else {
            return Ok(None);
        };
        let required = |name| {
            view.find_name(name)
                .ok_or_else(|| format!("Missing {name}"))
        };
        let state = Self {
            grid,
            summary: required("WorkspaceSummary")?,
            stored: required("WorkspaceStored")?,
            status: required("WorkspaceStatus")?,
            sources,
            external: 0,
        };
        state.refresh(ui)?;
        Ok(Some(state))
    }
    pub(crate) fn command(&mut self, ui: &mut Controls, command: &str) -> Result<bool, String> {
        let Some(command) = command.strip_prefix("GridWorkspace") else {
            return Ok(false);
        };
        let result = self.run(ui, command);
        ui.set_label(
            self.status,
            result.unwrap_or_else(|message| format!("Операция отклонена: {message}")),
        )
        .map_err(err)?;
        self.refresh(ui)?;
        Ok(true)
    }
    fn run(&mut self, ui: &mut Controls, command: &str) -> Result<String, String> {
        match command {
            "Table" | "Cards" | "Group" => {
                let mut settings = ui
                    .grid_presentation(self.grid)
                    .ok_or("Grid removed")?
                    .clone();
                match command {
                    "Table" => settings.view = GridViewMode::Table,
                    "Cards" => settings.view = GridViewMode::Cards,
                    _ => {
                        if settings.group_fields.is_empty() {
                            settings.group_fields.push("Category".into())
                        } else {
                            settings.group_fields.clear()
                        }
                    }
                }
                ui.set_grid_presentation(self.grid, settings).map_err(err)?;
                Ok("Представление изменено; ключ выбранной записи сохранён.".into())
            }
            "Details" => {
                let key = ui.grid_selected_key(self.grid).ok_or("Выберите запись")?;
                let key = RowKey(u64::from_ne_bytes(key.to_ne_bytes()));
                let expanded = ui
                    .grid_frame(self.grid)
                    .and_then(|frame| frame.semantics.rows.iter().find(|r| r.key == key))
                    .and_then(|r| r.expanded)
                    .unwrap_or(false);
                ui.set_grid_expanded(self.grid, key, !expanded)
                    .map_err(err)?;
                Ok("Подробности выбранной записи переключены; строки подробностей доступны только для чтения.".into())
            }
            "Reorder" | "Pin" | "Date" => {
                let mut columns = ui.grid_arrangement(self.grid).ok_or("Grid removed")?;
                match command {
                    "Reorder" => {
                        let index = columns.iter().position(|c| c.field == "Title").unwrap();
                        let title = columns.remove(index);
                        columns.insert(if index == 1 { 2 } else { 1 }, title);
                    }
                    "Pin" => {
                        let column = columns.iter_mut().find(|c| c.field == "Category").unwrap();
                        column.pinned = !column.pinned;
                    }
                    _ => {
                        let column = columns.iter_mut().find(|c| c.field == "Due").unwrap();
                        column.visible = !column.visible;
                    }
                }
                ui.arrange_grid_columns(self.grid, columns).map_err(err)?;
                Ok("Колонки обновлены; данные не изменены.".into())
            }
            "Quantity" | "Price" | "Due" | "Active" => {
                let key = ui.grid_selected_key(self.grid).ok_or("Выберите запись")?;
                ui.begin_grid_edit(self.grid, key, command).map_err(err)?;
                Ok("Редактор открыт: Enter подтверждает, Escape отменяет.".into())
            }
            "Commit" => {
                if ui.grid_editor(self.grid).is_none() {
                    return Ok("Нет активного редактора.".into());
                }
                Ok(if ui.commit_grid_edit(self.grid).map_err(err)? {
                    "Значение записано.".into()
                } else {
                    ui.grid_error(self.grid)
                        .unwrap_or("Ожидается проверка значения")
                        .into()
                })
            }
            "Cancel" => {
                ui.cancel_grid_edit(self.grid).map_err(err)?;
                Ok("Ввод отменён; значение источника сохранено.".into())
            }
            "Conflict" => {
                let key = ui.grid_selected_key(self.grid).ok_or("Выберите запись")?;
                if ui.grid_editor(self.grid).is_some() {
                    return Err("Завершите текущий ввод перед проверкой конфликта".into());
                }
                ui.begin_grid_edit(self.grid, key, "Quantity")
                    .map_err(err)?;
                ui.set_grid_edit_value(self.grid, CellValue::Integer(42))
                    .map_err(err)?;
                self.external += 1;
                self.sources
                    .master
                    .borrow_mut()
                    .set_cell(key, "Quantity", CellValue::Integer(100 + self.external))
                    .map_err(err)?;
                Ok("Черновик: 42, источник изменён извне. Enter проверит конфликт; Escape сохранит внешнее значение.".into())
            }
            _ => Err("Неизвестная команда таблицы".into()),
        }
    }
    pub(crate) fn refresh(&self, ui: &mut Controls) -> Result<(), String> {
        let source = self.sources.master.borrow();
        let selected = ui.grid_selected_key(self.grid);
        let frame = ui.grid_frame(self.grid);
        let rows = frame.map_or(0, |f| f.stats.logical_rows);
        let painted = frame.map_or(0, |f| f.semantics.rows.len());
        ui.set_label(self.summary,format!("Ключ: {} · записей: {} · строк представления: {rows} · строк в кадре: {painted} · версия: {} · подробностей: {}",selected.map_or_else(||"—".into(),|k|k.to_string()),source.len(),source.revision(),self.sources.detail.borrow().len())).map_err(err)?;
        let text = selected
            .and_then(|key| {
                source.value(key, "Quantity").map(|value| {
                    format!(
                        "Сохранённое количество: {} · активный редактор: {}",
                        value.display(),
                        if ui.grid_editor(self.grid).is_some() {
                            "да"
                        } else {
                            "нет"
                        }
                    )
                })
            })
            .unwrap_or_default();
        ui.set_label(self.stored, text).map_err(err)
    }
}
fn err(error: impl std::fmt::Display) -> String {
    error.to_string()
}
