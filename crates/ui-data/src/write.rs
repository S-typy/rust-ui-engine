use crate::{CellValue, DataError, DataLimits, DataRow, DataTable, TableSchema};

/// Maximum operations in one adapter transaction.
pub const MAX_DATA_WRITE_OPERATIONS: usize = 1024;
/// Incoming values, expectations and operation metadata in one transaction.
pub const MAX_DATA_WRITE_BYTES: usize = 64 * 1024 * 1024;

/// Sequential changes to a private transactional candidate. Expectations refer
/// to the candidate after preceding operations, not only the initial snapshot.
/// Inserts provide a unique stable key and all fields, including initial values
/// of read-only fields. Updates cannot change read-only fields. Deletes compare
/// the complete row so an unrelated field change also causes a conflict.
#[derive(Clone, Debug, PartialEq)]
pub enum DataWrite {
    Insert(DataRow),
    Update {
        key: i64,
        field: String,
        expected: CellValue,
        value: CellValue,
    },
    Delete {
        expected: DataRow,
    },
}

/// Explicit opt-in to atomic adapter writes. A provider never emulates an
/// unsupported batch by issuing individual writes. `write_cell` is the separate
/// legacy contract and does not imply any batch capability.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WriteCapabilities {
    pub insert: bool,
    pub update: bool,
    pub delete: bool,
    pub atomic_batch: bool,
}

impl WriteCapabilities {
    pub const fn transactional_crud() -> Self {
        Self {
            insert: true,
            update: true,
            delete: true,
            atomic_batch: true,
        }
    }

    pub fn validate(self, operations: &[DataWrite]) -> Result<(), DataError> {
        if operations.is_empty() {
            return Ok(());
        }
        if !self.atomic_batch
            || operations.iter().any(|operation| match operation {
                DataWrite::Insert(_) => !self.insert,
                DataWrite::Update { .. } => !self.update,
                DataWrite::Delete { .. } => !self.delete,
            })
        {
            return Err(DataError::Adapter(
                "adapter does not support the requested atomic write operations".into(),
            ));
        }
        Ok(())
    }
}

/// Validate incoming values before cloning the table or accessing storage.
pub(crate) fn validate_write_payload(
    schema: &TableSchema,
    limits: DataLimits,
    operations: &[DataWrite],
) -> Result<(), DataError> {
    if operations.len() > MAX_DATA_WRITE_OPERATIONS {
        return Err(DataError::Capacity("write operation count exceeded".into()));
    }
    let mut bytes = operations.len().saturating_mul(size_of::<DataWrite>());
    for operation in operations {
        match operation {
            DataWrite::Insert(row) | DataWrite::Delete { expected: row } => {
                if row.values.len() != schema.fields().len() {
                    return Err(DataError::Validation(
                        "write row width differs from schema".into(),
                    ));
                }
                for (field, value) in schema.fields().iter().zip(&row.values) {
                    field.validate(value)?;
                    charge_value(&mut bytes, limits, value)?;
                }
            }
            DataWrite::Update {
                field,
                expected,
                value,
                ..
            } => {
                if field.len() > 256 {
                    return Err(DataError::Capacity("write field name exceeded".into()));
                }
                let field_schema = schema
                    .field(field)
                    .ok_or_else(|| DataError::UnknownField(field.clone()))?;
                if field_schema.read_only {
                    return Err(DataError::ReadOnly(field.clone()));
                }
                field_schema.validate(expected)?;
                field_schema.validate(value)?;
                bytes = bytes.saturating_add(field.len());
                charge_value(&mut bytes, limits, expected)?;
                charge_value(&mut bytes, limits, value)?;
            }
        }
    }
    if bytes > MAX_DATA_WRITE_BYTES {
        return Err(DataError::Capacity("write payload exceeded".into()));
    }
    Ok(())
}

fn charge_value(bytes: &mut usize, limits: DataLimits, value: &CellValue) -> Result<(), DataError> {
    let payload = match value {
        CellValue::Text(text) => {
            if text.len() > limits.max_text_bytes {
                return Err(DataError::Capacity("write cell text exceeded".into()));
            }
            text.len()
        }
        CellValue::Binary(value) => {
            if value.len() > limits.max_binary_bytes {
                return Err(DataError::Capacity("write cell binary exceeded".into()));
            }
            value.len()
        }
        _ => 0,
    };
    *bytes = bytes
        .saturating_add(size_of::<CellValue>())
        .saturating_add(payload);
    if *bytes > MAX_DATA_WRITE_BYTES {
        return Err(DataError::Capacity("write payload exceeded".into()));
    }
    Ok(())
}

/// Shared transactional semantics used for source preflight and memory writes.
pub(crate) fn apply_writes(
    table: &DataTable,
    operations: &[DataWrite],
) -> Result<DataTable, DataError> {
    validate_write_payload(table.schema(), table.limits(), operations)?;
    let mut rows = table.rows().to_vec();
    for operation in operations {
        match operation {
            DataWrite::Insert(row) => {
                if rows.iter().any(|current| current.key == row.key) {
                    return Err(DataError::Conflict(format!(
                        "row {} already exists",
                        row.key
                    )));
                }
                if rows.len() >= table.limits().max_rows {
                    return Err(DataError::Capacity("write row count exceeded".into()));
                }
                rows.push(row.clone());
            }
            DataWrite::Update {
                key,
                field,
                expected,
                value,
            } => {
                let row = rows
                    .iter_mut()
                    .find(|row| row.key == *key)
                    .ok_or_else(|| DataError::Conflict(format!("row {key} was deleted")))?;
                let column = table.schema().field_index(field).expect("validated field");
                if row.values[column] != *expected {
                    return Err(DataError::Conflict(format!(
                        "row {key}, field {field} changed"
                    )));
                }
                row.values[column] = value.clone();
            }
            DataWrite::Delete { expected } => {
                let index = rows
                    .iter()
                    .position(|row| row.key == expected.key)
                    .ok_or_else(|| {
                        DataError::Conflict(format!("row {} was deleted", expected.key))
                    })?;
                if rows[index] != *expected {
                    return Err(DataError::Conflict(format!("row {} changed", expected.key)));
                }
                rows.remove(index);
            }
        }
    }
    DataTable::with_limits(table.schema().clone(), rows, table.limits())
}
