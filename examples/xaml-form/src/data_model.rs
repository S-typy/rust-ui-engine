//! Application-owned collection and SQLite fixtures for the data form.
use rust_desktop_ui_controls::{BindingContext, DataValue};
use rust_desktop_ui_data::{
    CellValue, CollectionAdapter, DataRow, DataSource, DataTable, FieldSchema, FieldType,
    SqliteAdapter, SqliteConnection, SqliteTableConfig, TableSchema,
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
    let collection = DataSource::new(CollectionAdapter::new(
        DataTable::new(schema.clone(), rows).map_err(err)?,
    ))
    .map_err(err)?;

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
    let context = BindingContext::new(DataValue::object([
        ("Filter", "".into()),
        ("ReadOnly", false.into()),
        ("CollectionKey", FIRST_KEY.to_string().into()),
        ("SqliteKey", FIRST_KEY.to_string().into()),
        (
            "Status",
            "Коллекция: 5 000 строк · SQLite: 1 000 строк".into(),
        ),
    ]))
    .map_err(err)?;
    Ok((
        Rc::new(RefCell::new(context)),
        vec![
            ("Products".into(), Rc::new(RefCell::new(collection))),
            ("SqliteProducts".into(), Rc::new(RefCell::new(sqlite))),
        ],
    ))
}

fn err(error: impl std::fmt::Display) -> String {
    error.to_string()
}
