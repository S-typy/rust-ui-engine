//! Application data for the editors and groups examples.
use rust_desktop_ui_controls::{BindingContext, BindingValidator, DataValue};
use std::{cell::RefCell, rc::Rc};

pub(crate) fn model() -> Result<Rc<RefCell<BindingContext>>, String> {
    let person = |id: &str, name: &str, department: &str| {
        DataValue::object([
            ("Id", id.into()),
            ("Name", name.into()),
            ("Department", department.into()),
        ])
    };
    let mut model = BindingContext::new(DataValue::object([
        (
            "People",
            DataValue::List(vec![
                person("anna", "Анна Смирнова", "Разработка"),
                person("ivan", "Иван Петров", "Дизайн"),
                person("maria", "Мария Орлова", "Поддержка"),
                person("alex", "Алексей Волков", "Разработка"),
            ]),
        ),
        (
            "Tags",
            DataValue::List(
                ["Продукт", "Дизайн", "Тестирование"]
                    .map(DataValue::from)
                    .into(),
            ),
        ),
        (
            "Form",
            DataValue::object([
                ("Person", "anna".into()),
                ("Tags", DataValue::List(vec!["Продукт".into()])),
                ("Title", "Новая форма".into()),
                ("Reference", "DOC-001".into()),
                ("Quantity", 12.0.into()),
                ("Date", "2026-10-15".into()),
                ("Time", "09:30:00".into()),
                ("ReadOnly", false.into()),
                ("Note", "Заметка к документу".into()),
                ("GroupHeader", "Основные сведения".into()),
                ("DetailsExpanded", true.into()),
            ]),
        ),
        ("Status", "Данные загружены".into()),
    ]))
    .map_err(|e| e.to_string())?;
    model
        .set_validator(Some(BindingValidator::new(|root| {
            let form = root
                .as_object()
                .and_then(|r| r.get("Form"))
                .and_then(DataValue::as_object)
                .ok_or("Нет данных формы")?;
            if form
                .get("Title")
                .and_then(DataValue::as_str)
                .is_none_or(|s| s.trim().is_empty())
            {
                return Err("Название обязательно.".into());
            }
            if form
                .get("Quantity")
                .and_then(DataValue::as_number)
                .is_none_or(|v| !(1.0..=1000.0).contains(&v))
            {
                return Err("Количество должно быть от 1 до 1000.".into());
            }
            Ok(())
        })))
        .map_err(|e| e.to_string())?;
    Ok(Rc::new(RefCell::new(model)))
}
