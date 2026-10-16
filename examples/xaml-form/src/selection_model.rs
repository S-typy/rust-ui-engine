//! Application-owned collections and commands for the selection example.
use crate::view::MountedView;
use rust_desktop_ui_controls::{
    BindingContext, BindingValidator, Controls, DataValue, SelectionItem,
};
use rust_desktop_ui_core::WidgetId;
use rust_desktop_ui_data::{
    CellValue, DataRow, DataTable, FieldSchema, FieldType, ObservableCollection, TableSchema,
};
use std::{cell::RefCell, rc::Rc};

pub(crate) fn model() -> Result<Rc<RefCell<BindingContext>>, String> {
    let schema = TableSchema::new(vec![
        FieldSchema::new("Name", FieldType::Text),
        FieldSchema::new("Department", FieldType::Text),
    ])
    .map_err(err)?;
    let people = ObservableCollection::new(
        DataTable::new(
            schema,
            [
                (9007199254740993, "Анна Смирнова", "Разработка"),
                (9007199254740994, "Иван Петров", "Дизайн"),
                (9007199254740995, "Мария Орлова", "Поддержка"),
                (9007199254740996, "Алексей Волков", "Разработка"),
            ]
            .into_iter()
            .map(|(key, name, department)| {
                DataRow::new(
                    key,
                    vec![
                        CellValue::Text(name.into()),
                        CellValue::Text(department.into()),
                    ],
                )
            })
            .collect(),
        )
        .map_err(err)?,
    );
    let mut model = BindingContext::new(DataValue::object([
        ("People", DataValue::List(vec![])),
        ("Person", "9007199254740993".into()),
        ("Team", DataValue::List(vec!["9007199254740993".into()])),
        (
            "Roles",
            DataValue::List(
                ["Автор", "Редактор", "Рецензент", "Наблюдатель"]
                    .map(DataValue::from)
                    .into(),
            ),
        ),
        ("SelectedRoles", DataValue::List(vec!["Автор".into()])),
        ("PrimaryRole", "Автор".into()),
        (
            "Cities",
            DataValue::List(
                ["Москва", "Казань", "Санкт-Петербург"]
                    .map(DataValue::from)
                    .into(),
            ),
        ),
        ("City", "Москва".into()),
    ]))
    .map_err(err)?;
    model
        .register_collection("People", people, "Id")
        .map_err(err)?;
    model
        .set_validator(Some(BindingValidator::new(|root| {
            let roles = root
                .as_object()
                .and_then(|object| object.get("SelectedRoles"))
                .and_then(DataValue::as_list)
                .ok_or("Роли не заданы")?;
            if roles.len() > 2 {
                return Err("Выберите не более двух ролей.".into());
            }
            Ok(())
        })))
        .map_err(err)?;
    Ok(Rc::new(RefCell::new(model)))
}

pub(crate) fn register(ui: &mut Controls) {
    for (id, label) in [
        ("SelectionReverse", "Обратный порядок"),
        ("SelectionResize", "Высота текущей строки"),
        ("SelectionRename", "Изменить подпись"),
        ("SelectionPeopleReverse", "Переставить сотрудников"),
        ("SelectionPeopleRename", "Изменить имя"),
    ] {
        ui.commands_mut().register(id, label);
    }
}

pub(crate) struct State {
    list: WidgetId,
    status: WidgetId,
    list_status: WidgetId,
    roles_summary: WidgetId,
    team_summary: WidgetId,
    context: Rc<RefCell<BindingContext>>,
}
impl State {
    pub(crate) fn install(ui: &mut Controls, view: &MountedView) -> Result<Option<Self>, String> {
        let Some(list) = view.find_name("LargeList") else {
            return Ok(None);
        };
        let items = (0..100_000)
            .map(|index| {
                let mut item = SelectionItem::new(
                    format!("document-{index:06}"),
                    format!("Документ {index:06}"),
                );
                item.columns = vec![format!("Группа {:03}", index % 100)];
                item
            })
            .collect();
        ui.set_selection_items(list, items).map_err(err)?;
        for index in [2, 7, 19, 50, 900, 50_000, 99_999] {
            ui.set_list_item_height(list, &format!("document-{index:06}"), Some(56.0))
                .map_err(err)?;
        }
        let required = |name| {
            view.find_name(name)
                .ok_or_else(|| format!("Missing {name}"))
        };
        Ok(Some(Self {
            list,
            status: required("SelectionStatus")?,
            list_status: required("LargeListState")?,
            roles_summary: required("RolesSummary")?,
            team_summary: required("TeamSummary")?,
            context: view
                .bindings
                .as_ref()
                .ok_or("Missing selection bindings")?
                .context(),
        }))
    }
    pub(crate) fn command(&mut self, ui: &mut Controls, command: &str) -> Result<bool, String> {
        if !command.starts_with("Selection") {
            return Ok(false);
        }
        let key = ui
            .selected_keys(self.list)
            .and_then(|keys| keys.first())
            .cloned()
            .or_else(|| {
                ui.list_virtualization_stats(self.list).and_then(|stats| {
                    ui.selection_items(self.list)?
                        .get(stats.range.visible.start)
                        .map(|item| item.key.clone())
                })
            })
            .unwrap_or_else(|| "document-000000".into());
        let result = match command {
            "SelectionReverse" => {
                let mut items = ui
                    .selection_items(self.list)
                    .ok_or("Missing list")?
                    .to_vec();
                items.reverse();
                ui.set_selection_items(self.list, items).map_err(err)?;
                "Порядок изменён, выбранные документы сохранены".to_owned()
            }
            "SelectionResize" => {
                let height = if ui.list_item_height(self.list, &key) == Some(64.0) {
                    32.0
                } else {
                    64.0
                };
                ui.set_list_item_height(self.list, &key, Some(height))
                    .map_err(err)?;
                format!("Высота строки {key}: {height}")
            }
            "SelectionRename" => {
                let mut items = ui
                    .selection_items(self.list)
                    .ok_or("Missing list")?
                    .to_vec();
                let item = items
                    .iter_mut()
                    .find(|item| item.key == key)
                    .ok_or("Unknown selected row")?;
                item.text = "Обновлённый документ".into();
                ui.set_selection_items(self.list, items).map_err(err)?;
                "Подпись обновлена в существующей строке".into()
            }
            "SelectionPeopleReverse" => {
                let people = self
                    .context
                    .borrow()
                    .observable_collection("People")
                    .ok_or("Missing people")?;
                let keys: Vec<_> = people.snapshot().rows().iter().map(|row| row.key).collect();
                for key in keys {
                    people.move_row(key, 0).map_err(err)?;
                }
                "Сотрудники переставлены, выбранные ключи сохранены".into()
            }
            "SelectionPeopleRename" => {
                self.context
                    .borrow()
                    .observable_collection("People")
                    .ok_or("Missing people")?
                    .update_cell(
                        9007199254740993,
                        "Name",
                        CellValue::Text("Анна — обновлено".into()),
                    )
                    .map_err(err)?;
                "Имя сотрудника обновлено во всех полях".into()
            }
            _ => return Ok(false),
        };
        ui.set_label(self.status, result).map_err(err)?;
        Ok(true)
    }
    pub(crate) fn refresh(&self, ui: &mut Controls) -> Result<(), String> {
        if let Some(stats) = ui.list_virtualization_stats(self.list) {
            let count = ui.selected_keys(self.list).map_or(0, |keys| keys.len());
            ui.set_label(
                self.list_status,
                format!(
                    "Строки {}–{} из 100 000 · выбрано {count}",
                    stats.range.visible.start + 1,
                    stats.range.visible.end
                ),
            )
            .map_err(err)?;
        }
        let context = self.context.borrow();
        let summary = |name: &str| -> String {
            context
                .get(name)
                .ok()
                .and_then(DataValue::as_list)
                .unwrap_or_default()
                .iter()
                .filter_map(DataValue::as_str)
                .collect::<Vec<_>>()
                .join(", ")
        };
        ui.set_label(
            self.roles_summary,
            format!(
                "Принятые роли: {}. Не более двух.",
                summary("SelectedRoles")
            ),
        )
        .map_err(err)?;
        ui.set_label(
            self.team_summary,
            format!("Ключи команды: {}", summary("Team")),
        )
        .map_err(err)?;
        Ok(())
    }
}
fn err(error: impl std::fmt::Display) -> String {
    error.to_string()
}
