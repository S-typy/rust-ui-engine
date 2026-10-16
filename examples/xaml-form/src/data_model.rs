//! Application-owned collection and SQLite fixtures for the data form.
use rust_desktop_ui_controls::{BindingContext, DataValue};
use rust_desktop_ui_data::{
    CellValue, CollectionAdapter, CollectionMutation, DataRow, DataSource, DataTable, FieldSchema,
    FieldType, SqliteAdapter, SqliteConnection, SqliteTableConfig, TableSchema,
};
use std::{cell::RefCell, rc::Rc};

pub(crate) type SharedSource = Rc<RefCell<DataSource>>;
type FormData = (Rc<RefCell<BindingContext>>, Vec<(String, SharedSource)>);
pub(crate) const FIRST_KEY: i64 = 9_007_199_254_740_993;

fn schema() -> Result<TableSchema, String> {
    let fields = [
        ("Code", "Артикул", FieldType::Text),
        ("Product", "Наименование", FieldType::Text),
        ("Quantity", "Кол-во", FieldType::Integer),
        ("Price", "Цена", FieldType::Real),
        ("Available", "В наличии", FieldType::Bool),
        ("Note", "Примечание", FieldType::Text),
    ]
    .into_iter()
    .map(|(name, title, kind)| {
        let mut field = FieldSchema::new(name, kind);
        field.title = title.into();
        field.nullable = name == "Note";
        field.read_only = name == "Code";
        field
    })
    .collect();
    TableSchema::new(fields).map_err(err)
}

fn product(index: usize) -> String {
    let names = [
        "Монитор",
        "Клавиатура",
        "Мышь",
        "Док-станция",
        "Наушники",
        "Камера",
    ];
    format!("{} {}", names[index % names.len()], index + 1)
}

pub(crate) fn model() -> Result<FormData, String> {
    let schema = schema()?;
    let rows = (0..5000)
        .map(|index| {
            DataRow::new(
                FIRST_KEY + index as i64,
                vec![
                    CellValue::Text(format!("R-{:05}", index + 1)),
                    CellValue::Text(product(index)),
                    CellValue::Integer((index % 100 + 1) as i64),
                    CellValue::Real(1490.0 + (index % 80) as f64 * 100.0),
                    CellValue::Bool(index % 7 != 0),
                    if index % 3 == 0 {
                        CellValue::Text("Основной склад".into())
                    } else {
                        CellValue::Null
                    },
                ],
            )
        })
        .collect();
    let adapter = CollectionAdapter::new(DataTable::new(schema.clone(), rows).map_err(err)?);
    let products = adapter.handle();
    let collection = DataSource::new(adapter).map_err(err)?;

    // A real SQLite database, deliberately in memory for this disposable demo.
    // Production applications can pass an existing file to SqliteAdapter::open.
    let mut database = SqliteConnection::open_in_memory().map_err(err)?;
    database
        .execute_batch(
            "CREATE TABLE Products (
        Id INTEGER PRIMARY KEY, Code TEXT NOT NULL UNIQUE, Product TEXT NOT NULL,
        Quantity INTEGER NOT NULL CHECK (Quantity >= 0),
        Price REAL NOT NULL CHECK (Price >= 0),
        Available INTEGER NOT NULL CHECK (Available IN (0, 1)), Note TEXT
    )",
        )
        .map_err(err)?;
    let transaction = database.transaction().map_err(err)?;
    for index in 0..1000 {
        transaction
            .execute(
                "INSERT INTO Products VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                (
                    FIRST_KEY + index as i64,
                    format!("S-{:05}", index + 1),
                    product(index),
                    (index % 100 + 1) as i64,
                    2490.0 + (index % 80) as f64 * 100.0,
                    index % 5 != 0,
                    if index % 3 == 0 {
                        Some("Резерв")
                    } else {
                        None
                    },
                ),
            )
            .map_err(err)?;
    }
    transaction.commit().map_err(err)?;
    let sqlite = DataSource::new(
        SqliteAdapter::from_connection(
            database,
            SqliteTableConfig::new("Products", "Id", schema).map_err(err)?,
        )
        .map_err(err)?,
    )
    .map_err(err)?;
    let mut context = BindingContext::new(DataValue::object([
        ("Filter", "".into()),
        ("ReadOnly", false.into()),
        ("CollectionKey", FIRST_KEY.to_string().into()),
        ("SqliteKey", FIRST_KEY.to_string().into()),
        ("Products", DataValue::List(Vec::new())),
        ("NextProductKey", (FIRST_KEY + 5000).to_string().into()),
        (
            "Status",
            "Коллекция: 5 000 строк · SQLite: 1 000 строк".into(),
        ),
    ]))
    .map_err(err)?;
    context
        .register_collection("Products", products, "Id")
        .map_err(err)?;
    Ok((
        Rc::new(RefCell::new(context)),
        vec![
            ("Products".into(), Rc::new(RefCell::new(collection))),
            ("SqliteProducts".into(), Rc::new(RefCell::new(sqlite))),
        ],
    ))
}

/// Demo commands mutate the collection itself. Observers adopt changes during
/// their normal update; no explicit source refresh or list replacement is needed.
pub(crate) fn apply_command(context: &mut BindingContext, command: &str) -> Result<(), String> {
    let result = change_product(context, command);
    let message = result.unwrap_or_else(|error| format!("Изменение отклонено: {error}"));
    context.set("Status", message.into()).map_err(err)?;
    Ok(())
}

fn change_product(context: &mut BindingContext, command: &str) -> Result<String, String> {
    if context.get("ReadOnly").map_err(err)?.as_bool() == Some(true) {
        return Err("включён режим только чтения".into());
    }
    let collection = context
        .observable_collection("Products")
        .ok_or("Коллекция недоступна")?;
    if command == "AddProduct" {
        let key = context
            .get("NextProductKey")
            .map_err(err)?
            .as_str()
            .ok_or("Некорректный ключ")?
            .parse::<i64>()
            .map_err(err)?;
        let next = key.checked_add(1).ok_or("Ключи исчерпаны")?;
        let serial = key - FIRST_KEY + 1;
        let row = DataRow::new(
            key,
            vec![
                CellValue::Text(format!("R-{serial:05}")),
                CellValue::Text(format!("Новый товар {serial}")),
                CellValue::Integer(1),
                CellValue::Real(1000.0),
                CellValue::Bool(true),
                CellValue::Null,
            ],
        );
        let table = collection.snapshot();
        let selected = context
            .get("CollectionKey")
            .map_err(err)?
            .as_str()
            .and_then(|value| value.parse::<i64>().ok());
        let index = table
            .rows()
            .iter()
            .position(|row| Some(row.key) == selected)
            .map_or(0, |index| index + 1);
        collection.insert(index, row).map_err(err)?;
        context
            .set("NextProductKey", next.to_string().into())
            .map_err(err)?;
        return Ok(format!(
            "Добавлен R-{serial:05} · строк: {}",
            table.rows().len() + 1
        ));
    }
    let key = context
        .get("CollectionKey")
        .map_err(err)?
        .as_str()
        .and_then(|value| value.parse::<i64>().ok())
        .ok_or("Выберите товар")?;
    match command {
        "RemoveProduct" => {
            collection.remove(key).map_err(err)?;
            Ok("Товар удалён из общей коллекции".into())
        }
        "MoveProduct" => {
            collection.move_row(key, 0).map_err(err)?;
            Ok("Товар перенесён в начало коллекции; сортировка таблицы сохраняется".into())
        }
        "UpdateProduct" => {
            let table = collection.snapshot();
            let quantity = match table.value(key, "Quantity") {
                Some(CellValue::Integer(value)) => {
                    value.checked_add(1).ok_or("Количество слишком велико")?
                }
                _ => return Err("Товар недоступен".into()),
            };
            collection
                .batch(vec![
                    CollectionMutation::UpdateCell {
                        key,
                        field: "Product".into(),
                        value: CellValue::Text(format!("Товар · обновление {quantity}")),
                    },
                    CollectionMutation::UpdateCell {
                        key,
                        field: "Quantity".into(),
                        value: CellValue::Integer(quantity),
                    },
                ])
                .map_err(err)?;
            Ok("Наименование и количество изменены одним пакетом".into())
        }
        _ => Err("Неизвестная команда коллекции".into()),
    }
}

fn err(error: impl std::fmt::Display) -> String {
    error.to_string()
}
