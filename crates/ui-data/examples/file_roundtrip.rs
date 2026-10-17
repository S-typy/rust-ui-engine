//! Explicit file creation or inspection with a fixed application schema.
use rust_desktop_ui_data::{
    CellValue, CsvOptions, DataFileFormat, DataFileOptions, DataRow, DataSource, DataTable,
    FieldSchema, FieldType, FileAccess, FileDataAdapter, FileTableMapping, TableSchema,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    let [command, path] = arguments.as_slice() else {
        return Err(
            "usage: file_roundtrip <create-json|inspect-json|create-csv|inspect-csv> <path>".into(),
        );
    };
    let command = command.to_str().ok_or("command must be UTF-8")?;
    let (create, format) = match command {
        "create-json" => (true, DataFileFormat::Json),
        "inspect-json" => (false, DataFileFormat::Json),
        "create-csv" => (true, DataFileFormat::Csv(CsvOptions::default())),
        "inspect-csv" => (false, DataFileFormat::Csv(CsvOptions::default())),
        _ => return Err("unknown command".into()),
    };
    let mut caption = FieldSchema::new("caption", FieldType::Text);
    caption.nullable = true;
    let schema = TableSchema::new(vec![
        caption,
        FieldSchema::new("amount", FieldType::Decimal),
    ])?;
    let mapping = FileTableMapping::new(schema.clone(), "key")?;
    let options = DataFileOptions {
        format,
        ..Default::default()
    };
    let adapter = if create {
        let table = DataTable::new(
            schema,
            vec![
                DataRow::new(
                    i64::MAX,
                    vec![
                        CellValue::Text("Документ, é 🙂".into()),
                        CellValue::Decimal("12345.67".parse()?),
                    ],
                ),
                DataRow::new(
                    i64::MIN,
                    vec![
                        CellValue::Null,
                        CellValue::Decimal("0.1234567890123456789012345678".parse()?),
                    ],
                ),
            ],
        )?;
        FileDataAdapter::create(path, table, mapping, options)?
    } else {
        FileDataAdapter::open(path, mapping, options, FileAccess::ReadOnly)?
    };
    let source = DataSource::new(adapter)?;
    println!("{} rows, revision {}", source.len(), source.revision());
    for row in source.table().rows() {
        println!("{}: {:?}", row.key, row.values);
    }
    Ok(())
}
