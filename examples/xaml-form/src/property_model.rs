//! Application-owned object source with a deliberate transactional setter veto.
use crate::view::MountedView;
use rust_desktop_ui_controls::{Controls, DataSourceHandle, PropertyValue};
use rust_desktop_ui_core::WidgetId;
use rust_desktop_ui_data::{
    CellValue, CollectionAdapter, DataAdapter, DataError, DataRow, DataSource, DataTable,
    DataWrite, FieldSchema, FieldType, TableSchema, WriteCapabilities,
};
use std::{cell::RefCell, rc::Rc};

struct Objects(CollectionAdapter);
impl DataAdapter for Objects {
    fn load(&mut self) -> Result<DataTable, DataError> {
        self.0.load()
    }
    fn write_cell(
        &mut self,
        key: i64,
        field: &str,
        expected: &CellValue,
        value: &CellValue,
    ) -> Result<DataTable, DataError> {
        self.write_batch(&[DataWrite::Update {
            key,
            field: field.into(),
            expected: expected.clone(),
            value: value.clone(),
        }])
    }
    fn write_capabilities(&self) -> WriteCapabilities {
        WriteCapabilities::transactional_crud()
    }
    fn write_batch(&mut self, operations: &[DataWrite]) -> Result<DataTable, DataError> {
        if operations.iter().any(|op|matches!(op,DataWrite::Update{field,value:CellValue::Text(text),..} if field=="name"&&text=="reject")) {return Err(DataError::Validation("Имя reject отклонено проверкой объекта; оба документа сохранены".into()));}
        self.0.write_batch(operations)
    }
}
pub(crate) fn register(ui: &mut Controls) -> Result<(), String> {
    for (id, label) in [
        ("PropertyDemoReset", "Сбросить количество"),
        ("PropertyDemoReject", "Отклонённая запись"),
        ("PropertyDemoExternal", "Изменить источник"),
        ("PropertyDemoRefresh", "Обновить свойства"),
        ("PropertyDemoCancel", "Отменить имя"),
    ] {
        ui.commands_mut().register(id, label);
    }
    let mut name = FieldSchema::new("name", FieldType::Text);
    name.nullable = true;
    let mut enabled = FieldSchema::new("enabled", FieldType::Bool);
    enabled.nullable = true;
    let mut identifier = FieldSchema::new("identifier", FieldType::Text);
    identifier.read_only = true;
    let schema = TableSchema::new(vec![
        name,
        enabled,
        identifier,
        FieldSchema::new("count", FieldType::Integer),
        FieldSchema::new("price", FieldType::Decimal),
        FieldSchema::new("created", FieldType::Date),
    ])
    .map_err(err)?;
    let rows = vec![
        DataRow::new(
            1,
            vec![
                CellValue::Text("Первый документ".into()),
                CellValue::Null,
                CellValue::Text("DOC-01".into()),
                CellValue::Integer(i64::MIN),
                CellValue::Decimal("12345.67".parse().map_err(err)?),
                CellValue::Date("2026-10-17".parse().map_err(err)?),
            ],
        ),
        DataRow::new(
            2,
            vec![
                CellValue::Text("Второй документ".into()),
                CellValue::Bool(false),
                CellValue::Text("DOC-02".into()),
                CellValue::Integer(i64::MAX),
                CellValue::Decimal("12345.67".parse().map_err(err)?),
                CellValue::Date("2026-10-17".parse().map_err(err)?),
            ],
        ),
    ];
    let source = DataSource::new(Objects(CollectionAdapter::new(
        DataTable::new(schema, rows).map_err(err)?,
    )))
    .map_err(err)?;
    ui.register_data_source("InspectionObjects", Rc::new(RefCell::new(source)))
        .map_err(err)
}
pub(crate) struct State {
    owner: WidgetId,
    accepted: WidgetId,
    activity: WidgetId,
    source: DataSourceHandle,
    changes: u64,
    last: String,
}
impl State {
    pub(crate) fn install(
        ui: &mut Controls,
        view: &mut MountedView,
    ) -> Result<Option<Self>, String> {
        let Some(owner) = view.find_name("DocumentProperties") else {
            return Ok(None);
        };
        let grid = view
            .property_grids
            .get(&owner)
            .ok_or("Property inspector not mounted")?;
        for (field, alias) in [
            ("name", "Name"),
            ("count", "Count"),
            ("enabled", "Enabled"),
            ("price", "Price"),
            ("created", "Created"),
            ("identifier", "Identifier"),
        ] {
            for (suffix, id) in [
                ("Editor", grid.property_editor(field).unwrap()),
                ("Owner", grid.property(field).unwrap()),
            ] {
                view.names.insert(format!("Property{alias}{suffix}"), id);
            }
            let (edit, apply, reset) = grid.property_actions(field).unwrap();
            for (suffix, id) in [("Start", edit), ("Apply", apply), ("Default", reset)] {
                view.names.insert(format!("Property{alias}{suffix}"), id);
            }
            if let Some(null) = grid.property_null_toggle(field) {
                view.names.insert(format!("Property{alias}Null"), null);
            }
        }
        if let Some(group) = grid.group(&["Параметры", "Размеры"]) {
            view.names.insert("PropertyDimensions".into(), group.header);
        }
        let mut state = Self {
            owner,
            accepted: view
                .find_name("PropertyAccepted")
                .ok_or("Missing property summary")?,
            activity: view
                .find_name("PropertyActivity")
                .ok_or("Missing property activity")?,
            source: ui
                .data_source("InspectionObjects")
                .ok_or("Missing objects")?,
            changes: 0,
            last: String::new(),
        };
        state.sync(ui, view)?;
        Ok(Some(state))
    }
    pub(crate) fn command(
        &mut self,
        ui: &mut Controls,
        view: &mut MountedView,
        id: &str,
    ) -> Result<bool, String> {
        let grid = view
            .property_grids
            .get_mut(&self.owner)
            .ok_or("Inspector was removed")?;
        self.last = match id {
            "PropertyDemoReset" => format!("Сброс: {:?}", grid.reset(ui, "count").map_err(err)?),
            "PropertyDemoReject" => {
                grid.set_draft(ui, "name", CellValue::Text("reject".into()))
                    .map_err(err)?;
                format!("Запись: {:?}", grid.commit(ui, "name").map_err(err)?)
            }
            "PropertyDemoExternal" => {
                self.source
                    .borrow_mut()
                    .set_cell(1, "name", CellValue::Text("Изменённый источник".into()))
                    .map_err(err)?;
                "Первый объект изменён вне инспектора".into()
            }
            "PropertyDemoRefresh" => {
                grid.refresh(ui).map_err(err)?;
                "Источник синхронизирован, подготовленные значения сохранены".into()
            }
            "PropertyDemoCancel" => {
                grid.cancel(ui, "name").map_err(err)?;
                "Подготовленное имя отменено".into()
            }
            _ => return Ok(false),
        };
        self.sync(ui, view)?;
        Ok(true)
    }
    pub(crate) fn sync(&mut self, ui: &mut Controls, view: &mut MountedView) -> Result<(), String> {
        let grid = view
            .property_grids
            .get_mut(&self.owner)
            .ok_or("Inspector was removed")?;
        self.changes = self
            .changes
            .saturating_add(grid.take_changes().len() as u64);
        let label = |name: &str| match grid.accepted(name).unwrap() {
            PropertyValue::Mixed => "Разные значения".into(),
            PropertyValue::Uniform(CellValue::Null) => "NULL".into(),
            PropertyValue::Uniform(v) => v.display(),
        };
        ui.set_label(
            self.accepted,
            format!(
                "Принято · название: {} · количество: {} · включён: {}",
                label("name"),
                label("count"),
                label("enabled")
            ),
        )
        .map_err(err)?;
        ui.set_label(
            self.activity,
            format!("Изменений: {}. {}", self.changes, self.last),
        )
        .map_err(err)
    }
    pub(crate) fn diagnostics(&self, ui: &Controls, view: &MountedView) -> String {
        use crate::popup_diagnostics::string;
        let grid = &view.property_grids[&self.owner];
        let properties = ["name", "count", "enabled", "price", "created", "identifier"]
            .iter()
            .map(|name| {
                let editor = grid.property_editor(name).unwrap();
                format!(
                    "{{\"field\":{},\"editor\":{},\"accepted\":{},\"draft\":{},\"error\":{}}}",
                    string(name),
                    string(&format!("{editor:?}")),
                    string(&format!("{:?}", grid.accepted(name))),
                    string(&format!("{:?}", grid.draft(ui, name))),
                    grid.error().map_or_else(|| "null".into(), string)
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        let source = self.source.borrow();
        let rows = source
            .table()
            .rows()
            .iter()
            .map(|row| {
                format!(
                    "{{\"key\":{},\"values\":[{}]}}",
                    string(&row.key.to_string()),
                    row.values
                        .iter()
                        .map(|value| string(&format!("{value:?}")))
                        .collect::<Vec<_>>()
                        .join(",")
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "{{\"owner\":{},\"changes\":{},\"properties\":[{}],\"source_revision\":{},\"rows\":[{}]}}",
            string(&format!("{:?}", self.owner)),
            self.changes,
            properties,
            source.revision(),
            rows
        )
    }
}
fn err(value: impl std::fmt::Display) -> String {
    value.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_desktop_ui_controls::Theme;
    use rust_desktop_ui_core::Size;

    #[test]
    fn property_demo_commands_expose_accepted_source_and_retained_mixed_editors() {
        let window = rust_desktop_ui_xaml::parse(include_str!("../PropertyWindow.xaml")).unwrap();
        let mut ui = Controls::new(Theme::FluentLight).unwrap();
        register(&mut ui).unwrap();
        let root = ui.root();
        let mut view = MountedView::mount(&mut ui, root, &window.content).unwrap();
        ui.update(Size::new(1180.0, 880.0)).unwrap();
        let mut state = State::install(&mut ui, &mut view).unwrap().unwrap();
        let editor = view.names["PropertyNameEditor"];
        state
            .command(&mut ui, &mut view, "PropertyDemoReject")
            .unwrap();
        assert_eq!(
            view.property_grids[&state.owner].accepted("name"),
            Some(PropertyValue::Mixed)
        );
        assert_eq!(ui.document(editor).unwrap().text(), "reject");
        assert_eq!(
            state.source.borrow().value(1, "name"),
            Some(&CellValue::Text("Первый документ".into()))
        );
        assert_eq!(state.changes, 0);
        state
            .command(&mut ui, &mut view, "PropertyDemoReset")
            .unwrap();
        assert_eq!(state.changes, 1);
        for key in [1, 2] {
            assert_eq!(
                state.source.borrow().value(key, "count"),
                Some(&CellValue::Integer(10))
            );
        }
        for theme in [Theme::FluentLight, Theme::FluentDark] {
            ui.set_theme(theme).unwrap();
            ui.update(Size::new(1180.0, 880.0)).unwrap();
            assert_eq!(view.names["PropertyNameEditor"], editor);
            assert!(ui.runtime.bounds(editor).unwrap().height >= 20.0);
        }
        let diagnostic = state.diagnostics(&ui, &view);
        assert!(diagnostic.contains("source_revision"));
        assert!(diagnostic.contains("Integer(10)"));
    }
}
