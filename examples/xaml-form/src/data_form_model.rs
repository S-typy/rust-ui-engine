//! An application-owned record, explicit save commands and conflict demonstration.
use crate::view::MountedView;
use rust_desktop_ui_controls::{
    BindingContext, Controls, DataFormCommit, DataSourceHandle, DataValue,
};
use rust_desktop_ui_core::WidgetId;
use rust_desktop_ui_data::{
    CellValue, CollectionAdapter, DataRow, DataSource, DataTable, DateValue, DecimalValue,
    FieldSchema, FieldType, TableSchema, TimeValue,
};
use std::{cell::RefCell, rc::Rc};

type Model = (Rc<RefCell<BindingContext>>, Vec<(String, DataSourceHandle)>);
pub(crate) fn model() -> Result<Model, String> {
    let fields = [
        ("Code", "Код записи", FieldType::Text),
        ("Name", "Наименование", FieldType::Text),
        ("Count", "Количество", FieldType::Integer),
        ("Budget", "Точная сумма", FieldType::Decimal),
        ("Due", "Дата", FieldType::Date),
        ("At", "Время", FieldType::Time),
        ("Active", "Согласовано", FieldType::Bool),
        ("Note", "Примечание", FieldType::Text),
    ]
    .into_iter()
    .map(|(name, title, kind)| {
        let mut field = FieldSchema::new(name, kind);
        field.title = title.into();
        field.read_only = name == "Code";
        field.nullable = matches!(name, "Active" | "Note");
        field
    })
    .collect();
    let schema = TableSchema::new(fields).map_err(err)?;
    let values = vec![
        CellValue::Text("DOC-007".into()),
        CellValue::Text("Поставка оборудования".into()),
        CellValue::Integer(9_007_199_254_740_993),
        CellValue::Decimal(DecimalValue::new(1234567, 2).map_err(err)?),
        CellValue::Date(DateValue::new(2026, 10, 17).map_err(err)?),
        CellValue::Time(TimeValue::new(10, 15, 42, 0).map_err(err)?),
        CellValue::Null,
        CellValue::Null,
    ];
    let table = DataTable::new(schema, vec![DataRow::new(7, values)]).map_err(err)?;
    let source = Rc::new(RefCell::new(
        DataSource::new(CollectionAdapter::new(table)).map_err(err)?,
    ));
    let context = Rc::new(RefCell::new(
        BindingContext::new(DataValue::object([] as [(&str, DataValue); 0])).map_err(err)?,
    ));
    Ok((context, vec![("FormRecords".into(), source)]))
}
pub(crate) fn register(ui: &mut Controls) {
    for (id, label) in [
        ("DataFormSave", "Сохранить"),
        ("DataFormRefresh", "Обновить"),
        ("DataFormRevert", "Отменить изменения"),
        ("DataFormArrange", "Порядок полей"),
        ("DataFormToggleNote", "Примечание"),
        ("DataFormExternal", "Внешнее изменение"),
    ] {
        ui.commands_mut().register(id, label);
    }
}
pub(crate) struct State {
    owner: WidgetId,
    source: DataSourceHandle,
    stored: WidgetId,
    status: WidgetId,
    dirty: WidgetId,
    external: u32,
}
impl State {
    pub(crate) fn install(
        ui: &mut Controls,
        view: &mut MountedView,
    ) -> Result<Option<Self>, String> {
        let Some(owner) = view.find_name("RecordForm") else {
            return Ok(None);
        };
        let form = view
            .data_forms
            .get(&owner)
            .ok_or("DataForm handle missing")?;
        let mut aliases = Vec::new();
        for name in [
            "Code", "Name", "Count", "Budget", "Due", "At", "Active", "Note",
        ] {
            aliases.push((
                format!("Data{name}"),
                form.field(name).ok_or("DataForm field missing")?,
            ));
            if let Some(toggle) = form.field_null_toggle(name) {
                aliases.push((format!("Data{name}Null"), toggle));
            }
        }
        aliases.push(("DataValidation".into(), form.summary));
        for (name, id) in aliases {
            view.names.insert(name, id);
        }
        let required = |name| {
            view.find_name(name)
                .ok_or_else(|| format!("Missing {name}"))
        };
        let mut state = Self {
            owner,
            source: ui.data_source("FormRecords").ok_or("Form source missing")?,
            stored: required("StoredRecord")?,
            status: required("FormStatus")?,
            dirty: required("FormDirty")?,
            external: 0,
        };
        state.refresh(ui, view)?;
        Ok(Some(state))
    }
    pub(crate) fn command(
        &mut self,
        ui: &mut Controls,
        view: &mut MountedView,
        command: &str,
    ) -> Result<bool, String> {
        if !command.starts_with("DataForm") {
            return Ok(false);
        }
        let form = view
            .data_forms
            .get_mut(&self.owner)
            .ok_or("DataForm removed")?;
        let message = match command {
            "DataFormSave" => match form.commit(ui).map_err(err)? {
                DataFormCommit::Saved => "Запись сохранена одной операцией".into(),
                DataFormCommit::Unchanged => "Нет изменений для сохранения".into(),
                DataFormCommit::Invalid(_) => "Исправьте ошибки; источник не изменён".into(),
                DataFormCommit::Pending => {
                    "Завершите ввод или проверку и повторите сохранение".into()
                }
                DataFormCommit::Conflict(message) => format!("Конфликт: {message}"),
            },
            "DataFormRefresh" => {
                form.refresh(ui).map_err(err)?;
                "Данные обновлены; черновики сохранены".into()
            }
            "DataFormRevert" => {
                form.revert(ui).map_err(err)?;
                "Загружена актуальная запись".into()
            }
            "DataFormArrange" => {
                let mut order = form.arrangement();
                order.reverse();
                form.arrange(ui, &order, 150.0).map_err(err)?;
                "Порядок изменён без пересоздания редакторов".into()
            }
            "DataFormToggleNote" => {
                let mut order = form.arrangement();
                for field in &mut order {
                    if field.field == "Note" {
                        field.visible = !field.visible;
                    }
                }
                form.arrange(ui, &order, 150.0).map_err(err)?;
                "Видимость примечания изменена".into()
            }
            "DataFormExternal" => {
                self.external += 1;
                self.source
                    .borrow_mut()
                    .set_cell(
                        7,
                        "Name",
                        CellValue::Text(format!("Изменение источника {}", self.external)),
                    )
                    .map_err(err)?;
                "Имя изменено в источнике; форма ожидает явную команду".into()
            }
            _ => return Ok(false),
        };
        ui.set_label(self.status, message).map_err(err)?;
        self.refresh(ui, view)?;
        Ok(true)
    }
    pub(crate) fn refresh(
        &mut self,
        ui: &mut Controls,
        view: &mut MountedView,
    ) -> Result<(), String> {
        let form = view
            .data_forms
            .get_mut(&self.owner)
            .ok_or("DataForm removed")?;
        form.update(ui).map_err(err)?;
        ui.set_label(
            self.dirty,
            if form.is_dirty(ui) {
                "Есть несохранённые изменения"
            } else {
                "Форма соответствует принятой записи"
            },
        )
        .map_err(err)?;
        let source = self.source.borrow();
        let row = source.row(7).ok_or("Record removed")?;
        let text = source
            .table()
            .schema()
            .fields()
            .iter()
            .zip(&row.values)
            .map(|(field, value)| {
                format!(
                    "{}: {}",
                    field.title,
                    match value {
                        CellValue::Null => "NULL".into(),
                        CellValue::Text(text) if text.is_empty() => "\"\"".into(),
                        _ => value.display(),
                    }
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        ui.set_label(self.stored, text).map_err(err)
    }
}
fn err(value: impl std::fmt::Display) -> String {
    value.to_string()
}
