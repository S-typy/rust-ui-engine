//! Explicit, bounded schema-based JSON/CSV storage.
use std::collections::HashSet;

use crate::{CellValue, DataError, DataLimits, DataRow, DataTable, FieldType, TableSchema};

#[path = "file_csv.rs"]
mod csv;
#[path = "file_encoding.rs"]
mod encoding;
#[path = "file_json.rs"]
mod json;
#[path = "file_store.rs"]
mod store;
pub use store::{FileAccess, FileDataAdapter};

/// Serialized file size, including encoding and escaping. May only be lowered.
pub const MAX_DATA_FILE_BYTES: usize = 128 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FileEncoding {
    #[default]
    Utf8,
    Utf8Bom,
    Utf16Le,
    Utf16Be,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CsvOptions {
    pub delimiter: u8,
}
impl Default for CsvOptions {
    fn default() -> Self {
        Self { delimiter: b',' }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DataFileFormat {
    #[default]
    Json,
    Csv(CsvOptions),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DataFileOptions {
    pub format: DataFileFormat,
    pub encoding: FileEncoding,
    pub limits: DataLimits,
    pub max_file_bytes: usize,
}
impl Default for DataFileOptions {
    fn default() -> Self {
        Self {
            format: DataFileFormat::Json,
            encoding: FileEncoding::Utf8,
            limits: DataLimits::default(),
            max_file_bytes: MAX_DATA_FILE_BYTES,
        }
    }
}
impl DataFileOptions {
    pub fn validate(self) -> Result<(), DataError> {
        self.limits.validate()?;
        if self.max_file_bytes == 0 || self.max_file_bytes > MAX_DATA_FILE_BYTES {
            return Err(DataError::Capacity(
                "file byte budget must be 1..=128 MiB".into(),
            ));
        }
        if let DataFileFormat::Csv(options) = self.format
            && (!options.delimiter.is_ascii()
                || options.delimiter.is_ascii_control()
                || matches!(options.delimiter, b'"' | b'\\' | b'N'))
        {
            return Err(DataError::Schema(
                "CSV delimiter must be printable ASCII other than quote, backslash or the null marker's N".into(),
            ));
        }
        Ok(())
    }
}

/// Column names are explicit and case-sensitive. The key is a separate column;
/// it is never guessed from a row position or converted through floating point.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileTableMapping {
    schema: TableSchema,
    key_column: String,
    columns: Vec<String>,
}
impl FileTableMapping {
    pub fn new(schema: TableSchema, key_column: impl Into<String>) -> Result<Self, DataError> {
        let columns = schema.fields().iter().map(|f| f.name.clone()).collect();
        Self::with_columns(schema, key_column, columns)
    }
    pub fn with_columns(
        schema: TableSchema,
        key_column: impl Into<String>,
        columns: Vec<String>,
    ) -> Result<Self, DataError> {
        let result = Self {
            schema,
            key_column: key_column.into(),
            columns,
        };
        result.validate()?;
        Ok(result)
    }
    pub fn schema(&self) -> &TableSchema {
        &self.schema
    }
    pub fn key_column(&self) -> &str {
        &self.key_column
    }
    pub fn columns(&self) -> &[String] {
        &self.columns
    }
    fn validate(&self) -> Result<(), DataError> {
        if self.columns.len() != self.schema.fields().len() {
            return Err(DataError::Schema(
                "file mapping width differs from schema".into(),
            ));
        }
        let mut seen = HashSet::new();
        for column in std::iter::once(&self.key_column).chain(&self.columns) {
            if column.is_empty()
                || column.len() > 256
                || column.chars().any(char::is_control)
                || !seen.insert(column)
            {
                return Err(DataError::Schema(
                    "file column names must be distinct printable names of 1..=256 bytes".into(),
                ));
            }
        }
        Ok(())
    }
    fn column(&self, index: usize) -> &str {
        if index == 0 {
            &self.key_column
        } else {
            &self.columns[index - 1]
        }
    }
    fn position(&self, name: &str) -> Option<usize> {
        if name == self.key_column {
            Some(0)
        } else {
            self.columns.iter().position(|c| c == name).map(|i| i + 1)
        }
    }
}

/// Decode only after checking the serialized byte budget. Failure returns no
/// partial table. Schema, types, nullability and stable keys are never inferred.
pub fn decode_data_file(
    bytes: &[u8],
    mapping: &FileTableMapping,
    options: DataFileOptions,
) -> Result<DataTable, DataError> {
    mapping.validate()?;
    options.validate()?;
    if mapping.schema.fields().len() > options.limits.max_fields {
        return Err(DataError::Capacity(
            "file schema exceeds field limit".into(),
        ));
    }
    let text = encoding::decode(bytes, options)?;
    match options.format {
        DataFileFormat::Json => json::decode(&text, mapping, options),
        DataFileFormat::Csv(csv) => csv::decode(&text, mapping, options, csv),
    }
}

/// Encode a complete compatible table. Integer/decimal/key values are exact;
/// nullable text and the empty string have distinct representations.
pub fn encode_data_file(
    table: &DataTable,
    mapping: &FileTableMapping,
    options: DataFileOptions,
) -> Result<Vec<u8>, DataError> {
    mapping.validate()?;
    options.validate()?;
    if table.schema() != &mapping.schema {
        return Err(DataError::Schema(
            "file mapping schema differs from table".into(),
        ));
    }
    let mut budget = TableBuilder::new(mapping, options);
    if mapping.schema.fields().len() > options.limits.max_fields
        || budget.bytes > options.limits.max_bytes
    {
        return Err(DataError::Capacity(
            "file schema exceeds table budget".into(),
        ));
    }
    for row in table.rows() {
        budget.charge(row)?;
    }
    let text = match options.format {
        DataFileFormat::Json => json::encode(table, mapping, options)?,
        DataFileFormat::Csv(csv) => csv::encode(table, mapping, options, csv)?,
    };
    encoding::encode(&text, options)
}

fn diagnostic(record: usize, column: &str, message: &str) -> DataError {
    // Column identifiers are validated at 256 bytes. Never include cell data.
    DataError::Adapter(format!("record {record}, column {column:?}: {message}"))
}
fn parse_key(text: &str) -> Option<i64> {
    let key = text.parse::<i64>().ok()?;
    (key.to_string() == text).then_some(key)
}
fn scalar_text(value: &CellValue) -> String {
    match value {
        CellValue::Null => "\\N".into(),
        CellValue::Bool(v) => v.to_string(),
        CellValue::Integer(v) => v.to_string(),
        CellValue::Real(v) => v.to_string(),
        CellValue::Text(v) => v.clone(),
        CellValue::Decimal(v) => v.to_string(),
        CellValue::Date(v) => v.to_string(),
        CellValue::Time(v) => v.to_string(),
        CellValue::Binary(v) => {
            use std::fmt::Write;
            let mut text = String::with_capacity(2 + v.len() * 2);
            text.push_str("0x");
            for byte in v {
                write!(text, "{byte:02x}").expect("String write");
            }
            text
        }
    }
}
fn parse_scalar(text: &str, kind: FieldType) -> Option<CellValue> {
    Some(match kind {
        FieldType::Text => CellValue::Text(text.into()),
        FieldType::Integer => CellValue::Integer(parse_key(text)?),
        FieldType::Real => {
            let v: f64 = text.parse().ok()?;
            if !v.is_finite() || text.trim() != text {
                return None;
            }
            CellValue::Real(v)
        }
        FieldType::Bool => CellValue::Bool(match text {
            "true" => true,
            "false" => false,
            _ => return None,
        }),
        FieldType::Decimal => {
            let v: crate::DecimalValue = text.parse().ok()?;
            if v.to_string() != text {
                return None;
            }
            CellValue::Decimal(v)
        }
        FieldType::Date => CellValue::Date(text.parse().ok()?),
        FieldType::Time => {
            let v: crate::TimeValue = text.parse().ok()?;
            if v.to_string() != text {
                return None;
            }
            CellValue::Time(v)
        }
        FieldType::Binary => {
            let hex = text.strip_prefix("0x")?;
            if !hex.len().is_multiple_of(2) {
                return None;
            }
            let mut bytes = Vec::with_capacity(hex.len() / 2);
            for pair in hex.as_bytes().chunks_exact(2) {
                let digit = |b: u8| match b {
                    b'0'..=b'9' => Some(b - b'0'),
                    b'a'..=b'f' => Some(b - b'a' + 10),
                    _ => None,
                };
                bytes.push(digit(pair[0])? * 16 + digit(pair[1])?);
            }
            CellValue::Binary(bytes)
        }
    })
}

struct TableBuilder<'a> {
    mapping: &'a FileTableMapping,
    options: DataFileOptions,
    rows: Vec<DataRow>,
    keys: HashSet<i64>,
    bytes: usize,
    count: usize,
}
impl<'a> TableBuilder<'a> {
    fn new(mapping: &'a FileTableMapping, options: DataFileOptions) -> Self {
        Self {
            mapping,
            options,
            rows: Vec::new(),
            keys: HashSet::new(),
            bytes: mapping
                .schema
                .fields()
                .iter()
                .map(|f| f.name.len() * 2 + f.title.len())
                .sum(),
            count: 0,
        }
    }
    fn push(&mut self, row: DataRow) -> Result<(), DataError> {
        self.charge(&row)?;
        self.rows.push(row);
        Ok(())
    }
    fn charge(&mut self, row: &DataRow) -> Result<(), DataError> {
        if self.count >= self.options.limits.max_rows {
            return Err(DataError::Capacity("file row limit exceeded".into()));
        }
        if row.values.len() != self.mapping.schema.fields().len() {
            return Err(DataError::Schema(
                "file row width differs from mapping".into(),
            ));
        }
        if !self.keys.insert(row.key) {
            return Err(diagnostic(
                self.count + 1,
                self.mapping.key_column(),
                "duplicate key",
            ));
        }
        self.bytes = self
            .bytes
            .saturating_add(size_of::<DataRow>() + 4 * size_of::<usize>());
        for (field, value) in self.mapping.schema.fields().iter().zip(&row.values) {
            field.validate(value).map_err(|_| {
                diagnostic(self.count + 1, &field.name, "wrong type or forbidden null")
            })?;
            let payload = match value {
                CellValue::Text(v) if v.len() <= self.options.limits.max_text_bytes => v.len(),
                CellValue::Binary(v) if v.len() <= self.options.limits.max_binary_bytes => v.len(),
                CellValue::Text(_) | CellValue::Binary(_) => {
                    return Err(diagnostic(
                        self.count + 1,
                        &field.name,
                        "cell byte budget exceeded",
                    ));
                }
                _ => 0,
            };
            self.bytes = self
                .bytes
                .saturating_add(size_of::<CellValue>())
                .saturating_add(payload);
        }
        if self.bytes > self.options.limits.max_bytes {
            return Err(DataError::Capacity(
                "decoded table byte budget exceeded".into(),
            ));
        }
        self.count += 1;
        Ok(())
    }
    fn finish(self) -> Result<DataTable, DataError> {
        DataTable::with_limits(self.mapping.schema.clone(), self.rows, self.options.limits)
    }
}

struct TextWriter {
    text: String,
    limit: usize,
}
impl TextWriter {
    fn new(options: DataFileOptions) -> Self {
        Self {
            text: String::new(),
            limit: options.max_file_bytes,
        }
    }
    fn push(&mut self, text: &str) -> Result<(), DataError> {
        if self
            .text
            .len()
            .checked_add(text.len())
            .is_none_or(|n| n > self.limit)
        {
            return Err(DataError::Capacity(
                "encoded file byte budget exceeded".into(),
            ));
        }
        self.text.push_str(text);
        Ok(())
    }
}

#[cfg(test)]
#[path = "file_tests.rs"]
mod tests;
