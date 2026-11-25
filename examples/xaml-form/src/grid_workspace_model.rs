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
            ("Criteria", "true()".into()),
            ("RuleEnabled", true.into()),
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
    let mut payments = Vec::new();
    let mut notes = Vec::new();
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
                CellValue::Text(["DEV", "SUP", "SVC"][index as usize % 3].into()),
                CellValue::Text(if index == 7 {
                    "Заказ, \"Север\"\r\nВторая строка".into()
                } else {
                    format!("Рабочая запись {index:05}")
                }),
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
            payments.push(DataRow::new(
                index,
                vec![
                    CellValue::Integer(key),
                    CellValue::Text(format!("Платёж №{}", index + 1)),
                ],
            ));
            for item in 0..3 {
                let line = index * 10 + item;
                notes.push(DataRow::new(
                    line,
                    vec![
                        CellValue::Integer(line),
                        CellValue::Text(format!("Примечание к позиции {}", item + 1)),
                    ],
                ));
                details.push(DataRow::new(
                    index * 10 + item,
                    vec![
                        CellValue::Integer(key),
                        CellValue::Text(format!("Позиция {}", item + 1)),
                        CellValue::Integer(item + 1),
                        CellValue::Decimal(
                            DecimalValue::new(i128::from((item + 1) * 25075), 2).map_err(err)?,
                        ),
                        CellValue::Integer(item * 1024),
                        CellValue::Integer(line),
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
            FieldSchema::new("Position", FieldType::Integer),
            FieldSchema {
                read_only: true,
                ..FieldSchema::new("LineId", FieldType::Integer)
            },
        ])
        .map_err(err)?,
        details,
    )?;
    let categories = source(
        TableSchema::new(vec![
            FieldSchema::new("Code", FieldType::Text),
            FieldSchema::new("Name", FieldType::Text),
            FieldSchema::new("Area", FieldType::Text),
        ])
        .map_err(err)?,
        [
            ("DEV", "Разработка", "Проекты"),
            ("SUP", "Поставка", "Логистика"),
            ("SVC", "Поддержка", "Сервис"),
        ]
        .into_iter()
        .enumerate()
        .map(|(i, (key, name, area))| {
            DataRow::new(
                i as i64,
                vec![
                    CellValue::Text(key.into()),
                    CellValue::Text(name.into()),
                    CellValue::Text(area.into()),
                ],
            )
        })
        .collect(),
    )?;
    let topics = source(
        TableSchema::new(vec![
            FieldSchema::new("Title", FieldType::Text),
            FieldSchema {
                nullable: true,
                ..FieldSchema::new("ParentId", FieldType::Integer)
            },
        ])
        .map_err(err)?,
        vec![
            DataRow::new(1, vec![CellValue::Text("Заказы".into()), CellValue::Null]),
            DataRow::new(2, vec![CellValue::Text("Склад".into()), CellValue::Null]),
        ],
    )?;
    ui.register_data_source("WorkspaceTopics", topics)
        .map_err(err)?;
    ui.register_data_source("WorkspaceCategories", categories)
        .map_err(err)?;
    ui.register_data_source("WorkspaceRecords", master.clone())
        .map_err(err)?;
    for (name, foreign, rows) in [
        ("WorkspacePayments", "OwnerId", payments),
        ("WorkspaceNotes", "LineId", notes),
    ] {
        let child = source(
            TableSchema::new(vec![
                FieldSchema::new(foreign, FieldType::Integer),
                FieldSchema::new("Caption", FieldType::Text),
            ])
            .map_err(err)?,
            rows,
        )?;
        ui.register_data_source(name, child).map_err(err)?;
    }
    ui.register_data_source("WorkspaceDetails", detail.clone())
        .map_err(err)?;
    for (id, label) in [
        ("SaveLayout", "Сохранить раскладку"),
        ("RestoreLayout", "Восстановить раскладку"),
        ("FitColumn", "Подобрать ширину"),
        ("CollapseContext", "Свернуть группу"),
        ("ContextEdit", "Редактировать"),
        ("PinRecord", "Закрепить запись"),
        ("AddFormat", "Добавить правило"),
        ("LowerFormat", "Ниже"),
        ("RaiseFormat", "Поднять приоритет"),
        ("ToggleFormat", "Включить или выключить"),
        ("RemoveFormat", "Удалить правило"),
        ("Preview", "Предпросмотр"),
        ("Print", "Печать"),
        ("CancelPrint", "Отменить печать"),
        ("Export", "Экспорт"),
        ("RegionZero", "Открыть регион 0"),
        ("RegionOne", "Открыть регион 1"),
        ("CollapseBranches", "Свернуть группы"),
        ("FirstPage", "В начало"),
        ("NextPage", "Вперёд"),
        ("LastPage", "В конец"),
        ("RetryPage", "Повторить"),
        ("Branch", "Регион 1"),
        ("RootPages", "Все регионы"),
        ("NewRow", "Добавить"),
        ("EditRow", "Изменить"),
        ("DeleteRow", "Удалить"),
        ("SaveRow", "Записать"),
        ("CancelRow", "Отменить"),
        ("OpenItems", "Открыть позиции"),
        ("Payments", "Платежи"),
        ("Notes", "Примечания к позиции"),
        ("Table", "Таблица"),
        ("Cards", "Карточки"),
        ("Group", "Группировка"),
        ("SortGroups", "Порядок групп"),
        ("Footer", "Итоги под группой"),
        ("Details", "Подробности"),
        ("Formula", "Применить формулу"),
        ("Chooser", "Выбрать колонки"),
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
    pub(crate) tools: crate::grid_workspace_tools::Tools,
    grid: WidgetId,
    formula: WidgetId,
    row_title: WidgetId,
    row_quantity: WidgetId,
    items_host: WidgetId,
    notes_host: WidgetId,
    active_items: Option<WidgetId>,
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
            tools: crate::grid_workspace_tools::Tools::new(ui, view, grid)?,
            grid,
            row_title: required("WorkspaceRowTitle")?,
            row_quantity: required("WorkspaceRowQuantity")?,
            items_host: required("WorkspaceItemsHost")?,
            notes_host: required("WorkspaceNotesHost")?,
            active_items: None,
            formula: required("WorkspaceFormula")?,
            summary: required("WorkspaceSummary")?,
            stored: required("WorkspaceStored")?,
            status: required("WorkspaceStatus")?,
            sources,
            external: 0,
        };
        ui.set_grid_row_validator(grid,|schema,row|{
            if row.values[schema.field_index("Title").unwrap()].display().trim().is_empty() || matches!(row.values[schema.field_index("Quantity").unwrap()],CellValue::Integer(v) if v<1){
                let field=if row.values[schema.field_index("Title").unwrap()].display().trim().is_empty(){"Title"}else{"Quantity"};
                return Err(rust_desktop_ui_controls::GridRowIssue{field:field.into(),message:"Введите наименование и положительное количество".into()});
            }Ok(())
        }).map_err(err)?;
        let topics = required("WorkspaceTopicsGrid")?;
        ui.set_grid_child_loader(topics, |parent| {
            let labels: &[&str] = match parent {
                1 => &["В обработке", "Выполненные"],
                2 => &["Резерв", "Отгрузка"],
                _ => &[],
            };
            Ok(labels
                .iter()
                .enumerate()
                .map(|(i, t)| {
                    DataRow::new(
                        parent * 10 + i as i64,
                        vec![CellValue::Text((*t).into()), CellValue::Integer(parent)],
                    )
                })
                .collect())
        })
        .map_err(err)?;
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
        if let Some(message) = self.tools.command(ui, command)? {
            return Ok(message);
        }
        if command == "ContextEdit"
            && let Some(rust_desktop_ui_controls::GridContextTarget::Record(key)) =
                ui.grid_context_target(self.grid)
        {
            ui.set_grid_selected_key(self.grid, Some(*key))
                .map_err(err)?;
            return self.run(ui, "EditRow");
        }
        match command {
            "NewRow" | "EditRow" => {
                let source = self.sources.master.borrow();
                let key = ui.grid_selected_key(self.grid);
                if command == "NewRow" {
                    let mut key = -1;
                    while source.table().row(key).is_some() {
                        key -= 1;
                    }
                    let row = DataRow::new(
                        key,
                        vec![
                            CellValue::Integer(key),
                            CellValue::Text(format!("DOC-{key}")),
                            CellValue::Text("DEV".into()),
                            CellValue::Text(String::new()),
                            CellValue::Integer(1),
                            CellValue::Decimal(DecimalValue::new(0, 2).map_err(err)?),
                            CellValue::Date(DateValue::new(2026, 1, 1).map_err(err)?),
                            CellValue::Null,
                        ],
                    );
                    drop(source);
                    ui.begin_grid_insert(self.grid, row).map_err(err)?;
                } else {
                    drop(source);
                    ui.begin_grid_row_edit(self.grid, key.ok_or("Выберите запись")?)
                        .map_err(err)?;
                }
                let d = ui.grid_row_draft(self.grid).unwrap();
                let title = d.row.values[3].display();
                let qty = d.row.values[4].display();
                ui.set_editor_value(self.row_title, &title).map_err(err)?;
                ui.set_editor_value(self.row_quantity, &qty).map_err(err)?;
                Ok("Открыт черновик строки".into())
            }
            "SaveRow" => {
                let title = ui.document(self.row_title).unwrap().text().to_owned();
                let qty = ui
                    .document(self.row_quantity)
                    .unwrap()
                    .text()
                    .parse::<i64>()
                    .map_err(err)?;
                ui.set_grid_draft_cell(self.grid, "Title", CellValue::Text(title))
                    .map_err(err)?;
                ui.set_grid_draft_cell(self.grid, "Quantity", CellValue::Integer(qty))
                    .map_err(err)?;
                ui.commit_grid_record(self.grid).map_err(err)?;
                Ok("Строка записана".into())
            }
            "CancelRow" => {
                ui.cancel_grid_record(self.grid).map_err(err)?;
                Ok("Черновик отменён".into())
            }
            "DeleteRow" => {
                ui.delete_grid_record(
                    self.grid,
                    ui.grid_selected_key(self.grid).ok_or("Выберите запись")?,
                )
                .map_err(err)?;
                Ok("Строка удалена".into())
            }
            "Notes" => {
                let items = self.active_items.ok_or("Откройте позиции заказа")?;
                let key = ui.grid_selected_key(items).ok_or("Выберите позицию")?;
                ui.open_grid_relation(
                    items,
                    key,
                    "Notes",
                    self.notes_host,
                    rust_desktop_ui_core::LayoutStyle {
                        height: rust_desktop_ui_core::Length::Px(90.),
                        ..Default::default()
                    },
                )
                .map_err(err)?;
                Ok("Примечания к позиции открыты".into())
            }
            "Payments" | "OpenItems" => {
                let child = ui
                    .open_grid_relation(
                        self.grid,
                        ui.grid_selected_key(self.grid).ok_or("Выберите запись")?,
                        if command == "Payments" {
                            "Payments"
                        } else {
                            "Items"
                        },
                        self.items_host,
                        rust_desktop_ui_core::LayoutStyle {
                            height: rust_desktop_ui_core::Length::Px(140.),
                            ..Default::default()
                        },
                    )
                    .map_err(err)?;
                ui.hide_grid_relations(self.notes_host).map_err(err)?;
                self.active_items = (command == "OpenItems").then_some(child);
                if command == "Payments" {
                    return Ok("Открыты платежи заказа".into());
                }
                if !ui.has_grid_relation(child, "Notes") {
                    ui.register_grid_relation(
                        child,
                        rust_desktop_ui_controls::GridRelation {
                            name: "Notes".into(),
                            source: ui
                                .data_source("WorkspaceNotes")
                                .ok_or("Источник примечаний недоступен")?,
                            master_field: "LineId".into(),
                            foreign_field: "LineId".into(),
                            settings: Default::default(),
                        },
                    )
                    .map_err(err)?;
                }
                ui.configure_grid_row_drop(child, Some("Position"), true)
                    .map_err(err)?;
                ui.allow_grid_external_move(child, true).map_err(err)?;
                Ok("Открыты позиции. Для перестановки отсортируйте по Position и тяните строку с Alt.".into())
            }
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
                            settings.group_fields = vec!["Category".into(), "Active".into()]
                        } else {
                            settings.group_fields.clear()
                        }
                    }
                }
                ui.set_grid_presentation(self.grid, settings).map_err(err)?;
                Ok("Представление изменено; ключ выбранной записи сохранён.".into())
            }
            "SortGroups" | "Footer" => {
                let mut settings = ui
                    .grid_presentation(self.grid)
                    .ok_or("Grid removed")?
                    .clone();
                let label = if command == "Footer" {
                    settings.group_footer = !settings.group_footer;
                    if settings.group_footer {
                        "Итоги показаны под группой"
                    } else {
                        "Итоги показаны в заголовке группы"
                    }
                } else {
                    match (settings.group_sort_summary, settings.group_descending) {
                        (None, false) => {
                            settings.group_sort_summary = Some(1);
                            settings.group_descending = true;
                            "Группы по сумме, по убыванию"
                        }
                        (Some(_), true) => {
                            settings.group_descending = false;
                            "Группы по сумме, по возрастанию"
                        }
                        _ => {
                            settings.group_sort_summary = None;
                            settings.group_descending = false;
                            "Группы по ключу"
                        }
                    }
                };
                ui.set_grid_presentation(self.grid, settings).map_err(err)?;
                Ok(label.into())
            }
            "Formula" => {
                let formula = ui
                    .document(self.formula)
                    .ok_or("Поле формулы недоступно")?
                    .text()
                    .to_owned();
                ui.set_grid_calculated_expression(self.grid, "LineTotal", &formula)
                    .map_err(err)?;
                Ok("Сумма строки и итоги пересчитаны.".into())
            }
            "Chooser" => {
                ui.set_grid_column_chooser_open(self.grid, true)
                    .map_err(err)?;
                Ok("Выбор колонок и полос".into())
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
        self.tools.refresh(ui)?;
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
