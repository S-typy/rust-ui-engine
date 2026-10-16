use std::{path::Path, time::Duration};

use rusqlite::{
    Connection, OpenFlags, TransactionBehavior, params,
    types::{Value, ValueRef},
};

use crate::{
    CellValue, DataAdapter, DataError, DataLimits, DataRow, DataTable, DataWrite, FieldSchema,
    FieldType, TableSchema, WriteCapabilities,
};

#[path = "sqlite_query.rs"]
mod query;
#[path = "sqlite_write.rs"]
mod write;
pub use query::SqliteQueryPage;

/// SQLite connection type for application-owned schema creation and fixtures.
pub use rusqlite::Connection as SqliteConnection;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SqliteTableConfig {
    pub table: String,
    pub key_column: String,
    pub schema: TableSchema,
}

impl SqliteTableConfig {
    pub fn new(
        table: impl Into<String>,
        key_column: impl Into<String>,
        schema: TableSchema,
    ) -> Result<Self, DataError> {
        let config = Self {
            table: table.into(),
            key_column: key_column.into(),
            schema,
        };
        config.validate()?;
        Ok(config)
    }
    fn validate(&self) -> Result<(), DataError> {
        for name in [&self.table, &self.key_column] {
            if name.is_empty() || name.len() > 256 || name.contains('\0') {
                return Err(DataError::Schema("invalid SQLite identifier".into()));
            }
        }
        let mut names = std::collections::HashSet::new();
        for field in self.schema.fields() {
            if !names.insert(field.name.to_ascii_lowercase()) {
                return Err(DataError::Schema(
                    "SQLite field names must be distinct ignoring ASCII case".into(),
                ));
            }
            if field.name.eq_ignore_ascii_case(&self.key_column)
                && (!field.read_only || field.kind != FieldType::Integer || field.nullable)
            {
                return Err(DataError::Schema(
                    "a visible SQLite key must be a non-null read-only integer".into(),
                ));
            }
        }
        Ok(())
    }
}

/// Explicit synchronous bounded SQLite table access. `open` never creates a
/// missing file. The adapter does not execute arbitrary SQL supplied by UI
/// markup; table/field identifiers are quoted and values are parameters.
///
/// A write uses an immediate transaction and an expected-value predicate.
/// Constraints, changed/deleted rows, invalid trigger results or snapshot
/// capacity errors roll back the transaction before an error is returned.
pub struct SqliteAdapter {
    connection: Connection,
    config: SqliteTableConfig,
    limits: DataLimits,
    query_schema_cookie: Option<i64>,
    query_utf8: bool,
}

impl SqliteAdapter {
    pub fn open(path: impl AsRef<Path>, config: SqliteTableConfig) -> Result<Self, DataError> {
        let path = path.as_ref();
        if !path.is_file() {
            return Err(DataError::Adapter(
                "SQLite path must name an existing database file".into(),
            ));
        }
        let connection = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(adapter_error)?;
        Self::from_connection(connection, config)
    }
    /// Construct from an application-owned connection, including an in-memory
    /// database. Ownership transfers to the adapter; connections are not shared
    /// with widgets. Schema creation remains the application's responsibility.
    pub fn from_connection(
        connection: Connection,
        config: SqliteTableConfig,
    ) -> Result<Self, DataError> {
        Self::with_limits(connection, config, DataLimits::default())
    }
    pub fn with_limits(
        connection: Connection,
        config: SqliteTableConfig,
        limits: DataLimits,
    ) -> Result<Self, DataError> {
        config.validate()?;
        limits.validate()?;
        connection
            .busy_timeout(Duration::from_millis(250))
            .map_err(adapter_error)?;
        let query_utf8 = query::is_utf8(&connection)?;
        Ok(Self {
            connection,
            config,
            limits,
            query_schema_cookie: None,
            query_utf8,
        })
    }
}

impl DataAdapter for SqliteAdapter {
    fn load(&mut self) -> Result<DataTable, DataError> {
        read_table(&self.connection, &self.config, self.limits)
    }
    fn write_capabilities(&self) -> WriteCapabilities {
        WriteCapabilities::transactional_crud()
    }
    fn write_batch(&mut self, operations: &[DataWrite]) -> Result<DataTable, DataError> {
        write::write_batch(self, operations)
    }
    fn query_capabilities_for(&self, request: &crate::QueryRequest) -> crate::QueryCapabilities {
        query::capabilities(self, request)
    }
    fn query_schema(&mut self) -> Result<Option<TableSchema>, DataError> {
        self.query_schema_cookie = Some(query::schema_cookie(&self.connection)?);
        self.query_utf8 = query::is_utf8(&self.connection)?;
        Ok(Some(self.config.schema.clone()))
    }
    fn query_pushdown(
        &mut self,
        request: &crate::QueryRequest,
    ) -> Result<crate::QueryResult, DataError> {
        query::execute(self, request)
    }
    fn write_cell(
        &mut self,
        key: i64,
        field: &str,
        expected: &CellValue,
        value: &CellValue,
    ) -> Result<DataTable, DataError> {
        let schema = self
            .config
            .schema
            .field(field)
            .ok_or_else(|| DataError::UnknownField(field.into()))?;
        if schema.read_only || field.eq_ignore_ascii_case(&self.config.key_column) {
            return Err(DataError::ReadOnly(field.into()));
        }
        schema.validate(expected)?;
        schema.validate(value)?;
        if let CellValue::Text(text) = value
            && text.len() > self.limits.max_text_bytes
        {
            return Err(DataError::Capacity("cell text limit exceeded".into()));
        }
        if let CellValue::Binary(bytes) = value
            && bytes.len() > self.limits.max_binary_bytes
        {
            return Err(DataError::Capacity("cell binary limit exceeded".into()));
        }
        let sql = format!(
            "UPDATE {} SET {} = ?1 WHERE {} = ?2 AND {} IS ?3",
            quote(&self.config.table),
            quote(field),
            quote(&self.config.key_column),
            quote(field)
        );
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(adapter_error)?;
        // SQLite comparison follows column collation and can equate unlike
        // storage classes. Check the precise typed value under the same write
        // lock before UPDATE, including case-only changes in NOCASE columns.
        check_expected(
            &transaction,
            &self.config,
            self.limits,
            key,
            schema,
            expected,
        )?;
        let changed = transaction
            .execute(&sql, params![sql_value(value), key, sql_value(expected)])
            .map_err(adapter_error)?;
        if changed != 1 {
            return Err(DataError::Conflict(format!(
                "row {key}, field {field} changed or was deleted"
            )));
        }
        let next = read_table(&transaction, &self.config, self.limits)?;
        if next.value(key, field) != Some(value) {
            return Err(DataError::Conflict(
                "SQLite affinity or trigger changed the requested value".into(),
            ));
        }
        transaction.commit().map_err(adapter_error)?;
        Ok(next)
    }
}

fn quote(identifier: &str) -> String {
    format!("\"{}\"", identifier.replace('"', "\"\""))
}

fn adapter_error(error: rusqlite::Error) -> DataError {
    DataError::Adapter(error.to_string())
}

fn sql_value(value: &CellValue) -> Value {
    match value {
        CellValue::Null => Value::Null,
        CellValue::Bool(value) => Value::Integer(i64::from(*value)),
        CellValue::Integer(value) => Value::Integer(*value),
        CellValue::Real(value) => Value::Real(*value),
        CellValue::Text(value) => Value::Text(value.clone()),
        CellValue::Decimal(value) => Value::Text(value.to_string()),
        CellValue::Date(value) => Value::Text(value.to_string()),
        CellValue::Time(value) => Value::Text(value.to_string()),
        CellValue::Binary(value) => Value::Blob(value.clone()),
    }
}

fn check_expected(
    connection: &Connection,
    config: &SqliteTableConfig,
    limits: DataLimits,
    key: i64,
    field: &FieldSchema,
    expected: &CellValue,
) -> Result<(), DataError> {
    let sql = format!(
        "SELECT t.{}, t.{} FROM {} AS t WHERE t.{} = ?1 LIMIT 2",
        quote(&config.key_column),
        quote(&field.name),
        quote(&config.table),
        quote(&config.key_column),
    );
    let mut statement = connection.prepare(&sql).map_err(adapter_error)?;
    let mut rows = statement.query([key]).map_err(adapter_error)?;
    let conflict = || {
        DataError::Conflict(format!(
            "row {key}, field {} changed or was deleted",
            field.name
        ))
    };
    let row = rows.next().map_err(adapter_error)?.ok_or_else(conflict)?;
    if row.get_ref(0).map_err(adapter_error)? != ValueRef::Integer(key) {
        return Err(conflict());
    }
    let mut bytes = 0;
    let current = read_value(
        row.get_ref(1).map_err(adapter_error)?,
        field,
        limits,
        &mut bytes,
    )?;
    if current != *expected || rows.next().map_err(adapter_error)?.is_some() {
        return Err(conflict());
    }
    Ok(())
}

fn read_value(
    raw: ValueRef<'_>,
    field: &FieldSchema,
    limits: DataLimits,
    bytes: &mut usize,
) -> Result<CellValue, DataError> {
    let invalid = || {
        DataError::Validation(format!(
            "SQLite field {} has a storage type incompatible with {:?}",
            field.name, field.kind
        ))
    };
    Ok(match (field.kind, raw) {
        (_, ValueRef::Null) if field.nullable => CellValue::Null,
        (FieldType::Bool, ValueRef::Integer(value @ (0 | 1))) => CellValue::Bool(value != 0),
        (FieldType::Integer, ValueRef::Integer(value)) => CellValue::Integer(value),
        (FieldType::Real, ValueRef::Real(value)) if value.is_finite() => CellValue::Real(value),
        (FieldType::Binary, ValueRef::Blob(value)) => {
            charge_payload(
                bytes,
                value.len(),
                limits.max_binary_bytes,
                limits.max_bytes,
            )?;
            CellValue::Binary(value.to_vec())
        }
        (FieldType::Decimal | FieldType::Date | FieldType::Time, ValueRef::Text(value)) => {
            // Canonical ASCII storage avoids both affinity rounding and ambiguity
            // in the SQL expected-value predicate (e.g. "1.00" versus "1").
            // All supported scalar encodings fit in 70 bytes.
            if value.len() > 70 {
                return Err(invalid());
            }
            let text = std::str::from_utf8(value).map_err(|_| invalid())?;
            let (value, canonical) = match field.kind {
                FieldType::Decimal => {
                    let value: crate::DecimalValue = text.parse()?;
                    (CellValue::Decimal(value), value.to_string())
                }
                FieldType::Date => {
                    let value: crate::DateValue = text.parse()?;
                    (CellValue::Date(value), value.to_string())
                }
                FieldType::Time => {
                    let value: crate::TimeValue = text.parse()?;
                    (CellValue::Time(value), value.to_string())
                }
                _ => unreachable!(),
            };
            if canonical != text {
                return Err(DataError::Validation(format!(
                    "SQLite field {} requires canonical scalar text",
                    field.name
                )));
            }
            value
        }
        (FieldType::Text, ValueRef::Text(value)) => {
            charge_payload(bytes, value.len(), limits.max_text_bytes, limits.max_bytes)?;
            CellValue::Text(std::str::from_utf8(value).map_err(|_| invalid())?.into())
        }
        _ => return Err(invalid()),
    })
}

fn charge_payload(
    bytes: &mut usize,
    size: usize,
    cell_limit: usize,
    table_limit: usize,
) -> Result<(), DataError> {
    if size > cell_limit {
        return Err(DataError::Capacity(
            "SQLite cell byte limit exceeded".into(),
        ));
    }
    *bytes = bytes
        .checked_add(size)
        .ok_or_else(|| DataError::Capacity("size overflow".into()))?;
    if *bytes > table_limit {
        return Err(DataError::Capacity(
            "SQLite snapshot byte limit exceeded".into(),
        ));
    }
    Ok(())
}

fn read_table(
    connection: &Connection,
    config: &SqliteTableConfig,
    limits: DataLimits,
) -> Result<DataTable, DataError> {
    if config.schema.fields().len() > limits.max_fields {
        return Err(DataError::Capacity("field count exceeded".into()));
    }
    // Qualification prevents SQLite's legacy double-quoted-string fallback
    // from treating an unknown column identifier as a string constant.
    let fields = config
        .schema
        .fields()
        .iter()
        .map(|field| format!("t.{}", quote(&field.name)))
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "SELECT t.{}, {} FROM {} AS t ORDER BY t.{} LIMIT ?1",
        quote(&config.key_column),
        fields,
        quote(&config.table),
        quote(&config.key_column)
    );
    let mut statement = connection.prepare(&sql).map_err(adapter_error)?;
    let mut rows = statement
        .query([limits.max_rows as i64 + 1])
        .map_err(adapter_error)?;
    let mut output = Vec::new();
    let mut bytes = config
        .schema
        .fields()
        .iter()
        .map(|field| field.name.len() * 2 + field.title.len())
        .sum::<usize>();
    while let Some(row) = rows.next().map_err(adapter_error)? {
        if output.len() == limits.max_rows {
            return Err(DataError::Capacity("SQLite row count exceeded".into()));
        }
        let key = match row.get_ref(0).map_err(adapter_error)? {
            ValueRef::Integer(key) => key,
            _ => {
                return Err(DataError::Schema(
                    "SQLite key must contain signed integers only".into(),
                ));
            }
        };
        bytes = bytes
            .checked_add(
                std::mem::size_of::<DataRow>()
                    + 4 * std::mem::size_of::<usize>()
                    + config.schema.fields().len() * std::mem::size_of::<CellValue>(),
            )
            .ok_or_else(|| DataError::Capacity("size overflow".into()))?;
        if bytes > limits.max_bytes {
            return Err(DataError::Capacity(
                "SQLite snapshot byte limit exceeded".into(),
            ));
        }
        let mut values = Vec::with_capacity(config.schema.fields().len());
        for (index, field) in config.schema.fields().iter().enumerate() {
            let raw = row.get_ref(index + 1).map_err(adapter_error)?;
            values.push(read_value(raw, field, limits, &mut bytes)?);
        }
        output.push(DataRow::new(key, values));
    }
    DataTable::with_limits(config.schema.clone(), output, limits)
}
