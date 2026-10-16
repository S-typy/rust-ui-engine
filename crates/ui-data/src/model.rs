use std::{collections::HashMap, error::Error, fmt};

use crate::{DateValue, DecimalValue, TimeValue};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DataError {
    Schema(String),
    Validation(String),
    Capacity(String),
    UnknownRow(i64),
    UnknownField(String),
    ReadOnly(String),
    Conflict(String),
    Adapter(String),
    RevisionOverflow,
}

impl fmt::Display for DataError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Schema(message) => write!(f, "data schema: {message}"),
            Self::Validation(message) => write!(f, "data validation: {message}"),
            Self::Capacity(message) => write!(f, "data capacity: {message}"),
            Self::UnknownRow(key) => write!(f, "unknown data row: {key}"),
            Self::UnknownField(field) => write!(f, "unknown data field: {field}"),
            Self::ReadOnly(field) => write!(f, "data field is read-only: {field}"),
            Self::Conflict(message) => write!(f, "data conflict: {message}"),
            Self::Adapter(message) => write!(f, "data adapter: {message}"),
            Self::RevisionOverflow => f.write_str("data revision exhausted"),
        }
    }
}
impl Error for DataError {}

/// Integer values and row identities are never converted through floating point.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum CellValue {
    #[default]
    Null,
    Bool(bool),
    Integer(i64),
    Real(f64),
    Text(String),
    Decimal(DecimalValue),
    Date(DateValue),
    Time(TimeValue),
    Binary(Vec<u8>),
}

impl CellValue {
    pub fn display(&self) -> String {
        match self {
            Self::Null => String::new(),
            Self::Bool(value) => if *value { "Да" } else { "Нет" }.into(),
            Self::Integer(value) => value.to_string(),
            Self::Real(value) => value.to_string(),
            Self::Text(value) => value.clone(),
            Self::Decimal(value) => value.to_string(),
            Self::Date(value) => value.display(),
            Self::Time(value) => value.display(),
            Self::Binary(value) => {
                use std::fmt::Write;
                let mut text = String::with_capacity(2 + value.len() * 2);
                text.push_str("0x");
                for byte in value {
                    write!(text, "{byte:02X}").expect("writing to String cannot fail");
                }
                text
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldType {
    Bool,
    Integer,
    Real,
    Text,
    Decimal,
    Date,
    Time,
    Binary,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FieldSchema {
    pub name: String,
    pub title: String,
    pub kind: FieldType,
    pub nullable: bool,
    pub read_only: bool,
}

impl FieldSchema {
    pub fn new(name: impl Into<String>, kind: FieldType) -> Self {
        let name = name.into();
        Self {
            title: name.clone(),
            name,
            kind,
            nullable: false,
            read_only: false,
        }
    }

    pub fn validate(&self, value: &CellValue) -> Result<(), DataError> {
        let valid = match (self.kind, value) {
            (_, CellValue::Null) => self.nullable,
            (FieldType::Bool, CellValue::Bool(_))
            | (FieldType::Integer, CellValue::Integer(_))
            | (FieldType::Text, CellValue::Text(_)) => true,
            (FieldType::Decimal, CellValue::Decimal(_))
            | (FieldType::Date, CellValue::Date(_))
            | (FieldType::Time, CellValue::Time(_))
            | (FieldType::Binary, CellValue::Binary(_)) => true,
            (FieldType::Real, CellValue::Real(value)) => value.is_finite(),
            _ => false,
        };
        if valid {
            Ok(())
        } else {
            Err(DataError::Validation(format!(
                "{} requires {}{:?}",
                self.name,
                if self.nullable { "nullable " } else { "" },
                self.kind
            )))
        }
    }

    /// Parse editor text. Empty numeric/boolean text means null only for a
    /// nullable field. Text preserves whitespace and the empty string; assign
    /// `CellValue::Null` explicitly to clear a nullable text field to SQL NULL.
    pub fn parse(&self, text: &str) -> Result<CellValue, DataError> {
        let trimmed = text.trim();
        let value = if self.kind != FieldType::Text && trimmed.is_empty() && self.nullable {
            CellValue::Null
        } else {
            let invalid =
                || DataError::Validation(format!("{}: invalid {:?} value", self.name, self.kind));
            match self.kind {
                FieldType::Text => CellValue::Text(text.into()),
                FieldType::Integer => CellValue::Integer(trimmed.parse().map_err(|_| invalid())?),
                FieldType::Real => CellValue::Real(trimmed.parse().map_err(|_| invalid())?),
                FieldType::Decimal => CellValue::Decimal(trimmed.parse()?),
                FieldType::Date => CellValue::Date(DateValue::parse_display(trimmed)?),
                FieldType::Time => CellValue::Time(trimmed.parse()?),
                FieldType::Binary => {
                    let digits = trimmed
                        .strip_prefix("0x")
                        .or_else(|| trimmed.strip_prefix("0X"))
                        .ok_or_else(invalid)?;
                    if !digits.len().is_multiple_of(2)
                        || !digits.bytes().all(|b| b.is_ascii_hexdigit())
                    {
                        return Err(invalid());
                    }
                    if digits.len() / 2 > DataLimits::default().max_binary_bytes {
                        return Err(DataError::Capacity(
                            "binary cell byte limit exceeded".into(),
                        ));
                    }
                    CellValue::Binary(
                        digits
                            .as_bytes()
                            .chunks_exact(2)
                            .map(|pair| {
                                fn digit(byte: u8) -> u8 {
                                    if byte.is_ascii_digit() {
                                        byte - b'0'
                                    } else {
                                        byte.to_ascii_lowercase() - b'a' + 10
                                    }
                                }
                                digit(pair[0]) * 16 + digit(pair[1])
                            })
                            .collect(),
                    )
                }
                FieldType::Bool => CellValue::Bool(match trimmed.to_lowercase().as_str() {
                    "true" | "1" | "да" => true,
                    "false" | "0" | "нет" => false,
                    _ => return Err(invalid()),
                }),
            }
        };
        self.validate(&value)?;
        Ok(value)
    }
}

/// Limits may be lowered, not raised above the implementation's hard bounds.
/// `max_bytes` counts UTF-8/binary payloads, cells, keys and source row indices. It is
/// not a measurement of allocator overhead or temporary candidate snapshots.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DataLimits {
    pub max_rows: usize,
    pub max_fields: usize,
    pub max_text_bytes: usize,
    pub max_binary_bytes: usize,
    pub max_bytes: usize,
}

impl Default for DataLimits {
    fn default() -> Self {
        Self {
            max_rows: 100_000,
            max_fields: 128,
            max_text_bytes: 1024 * 1024,
            max_binary_bytes: 1024 * 1024,
            max_bytes: 64 * 1024 * 1024,
        }
    }
}

impl DataLimits {
    pub fn validate(self) -> Result<(), DataError> {
        let max = Self::default();
        if self.max_rows > max.max_rows
            || self.max_fields == 0
            || self.max_fields > max.max_fields
            || self.max_text_bytes > max.max_text_bytes
            || self.max_binary_bytes > max.max_binary_bytes
            || self.max_bytes > max.max_bytes
        {
            return Err(DataError::Capacity("limits exceed hard bounds".into()));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TableSchema {
    fields: Vec<FieldSchema>,
    positions: HashMap<String, usize>,
}

impl TableSchema {
    pub fn new(fields: Vec<FieldSchema>) -> Result<Self, DataError> {
        if fields.is_empty() || fields.len() > DataLimits::default().max_fields {
            return Err(DataError::Schema("field count must be 1..=128".into()));
        }
        let mut positions = HashMap::with_capacity(fields.len());
        for (index, field) in fields.iter().enumerate() {
            if field.name.is_empty()
                || field.name.len() > 256
                || field.name.contains('\0')
                || field.title.len() > 4096
            {
                return Err(DataError::Schema(
                    "invalid field name or title length".into(),
                ));
            }
            if positions.insert(field.name.clone(), index).is_some() {
                return Err(DataError::Schema(format!(
                    "duplicate field: {}",
                    field.name
                )));
            }
        }
        Ok(Self { fields, positions })
    }
    pub fn fields(&self) -> &[FieldSchema] {
        &self.fields
    }
    pub fn field(&self, name: &str) -> Option<&FieldSchema> {
        self.field_index(name).map(|index| &self.fields[index])
    }
    pub fn field_index(&self, name: &str) -> Option<usize> {
        self.positions.get(name).copied()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct DataRow {
    pub key: i64,
    pub values: Vec<CellValue>,
}

impl DataRow {
    pub fn new(key: i64, values: Vec<CellValue>) -> Self {
        Self { key, values }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct DataTable {
    schema: TableSchema,
    rows: Vec<DataRow>,
    positions: HashMap<i64, usize>,
    limits: DataLimits,
}

impl DataTable {
    pub fn new(schema: TableSchema, rows: Vec<DataRow>) -> Result<Self, DataError> {
        Self::with_limits(schema, rows, DataLimits::default())
    }
    pub fn with_limits(
        schema: TableSchema,
        rows: Vec<DataRow>,
        limits: DataLimits,
    ) -> Result<Self, DataError> {
        limits.validate()?;
        if rows.len() > limits.max_rows || schema.fields.len() > limits.max_fields {
            return Err(DataError::Capacity("row or field count exceeded".into()));
        }
        let mut bytes = schema
            .fields
            .iter()
            .map(|field| field.name.len() * 2 + field.title.len())
            .sum::<usize>();
        let mut positions = HashMap::with_capacity(rows.len());
        for (index, row) in rows.iter().enumerate() {
            if positions.insert(row.key, index).is_some() {
                return Err(DataError::Schema(format!("duplicate row key: {}", row.key)));
            }
            if row.values.len() != schema.fields.len() {
                return Err(DataError::Schema(format!(
                    "row {} has the wrong field count",
                    row.key
                )));
            }
            bytes = bytes
                .checked_add(std::mem::size_of::<DataRow>() + 4 * std::mem::size_of::<usize>())
                .ok_or_else(|| DataError::Capacity("size overflow".into()))?;
            for (field, value) in schema.fields.iter().zip(&row.values) {
                field.validate(value)?;
                bytes = bytes
                    .checked_add(std::mem::size_of::<CellValue>())
                    .ok_or_else(|| DataError::Capacity("size overflow".into()))?;
                let payload = match value {
                    CellValue::Text(text) => {
                        Some((text.len(), limits.max_text_bytes, "UTF-8 text"))
                    }
                    CellValue::Binary(bytes) => {
                        Some((bytes.len(), limits.max_binary_bytes, "binary"))
                    }
                    _ => None,
                };
                if let Some((size, limit, kind)) = payload {
                    if size > limit {
                        return Err(DataError::Capacity(format!(
                            "{kind} in {} exceeds byte limit",
                            field.name
                        )));
                    }
                    bytes = bytes
                        .checked_add(size)
                        .ok_or_else(|| DataError::Capacity("size overflow".into()))?;
                }
            }
            if bytes > limits.max_bytes {
                return Err(DataError::Capacity("table byte limit exceeded".into()));
            }
        }
        if bytes > limits.max_bytes {
            return Err(DataError::Capacity("schema byte limit exceeded".into()));
        }
        Ok(Self {
            schema,
            rows,
            positions,
            limits,
        })
    }
    pub fn schema(&self) -> &TableSchema {
        &self.schema
    }
    pub fn rows(&self) -> &[DataRow] {
        &self.rows
    }
    pub fn limits(&self) -> DataLimits {
        self.limits
    }
    pub fn row(&self, key: i64) -> Option<&DataRow> {
        self.positions.get(&key).map(|index| &self.rows[*index])
    }
    pub fn value(&self, key: i64, field: &str) -> Option<&CellValue> {
        let index = self.schema.field_index(field)?;
        self.row(key)?.values.get(index)
    }
    pub(crate) fn edited(
        &self,
        key: i64,
        field: &str,
        expected: &CellValue,
        value: &CellValue,
    ) -> Result<Self, DataError> {
        let index = self
            .schema
            .field_index(field)
            .ok_or_else(|| DataError::UnknownField(field.into()))?;
        let schema = &self.schema.fields[index];
        if schema.read_only {
            return Err(DataError::ReadOnly(field.into()));
        }
        schema.validate(value)?;
        let position = *self.positions.get(&key).ok_or(DataError::UnknownRow(key))?;
        if self.rows[position].values[index] != *expected {
            return Err(DataError::Conflict(format!(
                "row {key}, field {field} changed"
            )));
        }
        let mut rows = self.rows.clone();
        rows[position].values[index] = value.clone();
        Self::with_limits(self.schema.clone(), rows, self.limits)
    }
}
