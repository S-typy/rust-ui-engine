use super::*;

pub(super) fn write_batch(
    adapter: &mut SqliteAdapter,
    operations: &[DataWrite],
) -> Result<DataTable, DataError> {
    crate::write::validate_write_payload(&adapter.config.schema, adapter.limits, operations)?;
    if operations.is_empty() {
        return adapter.load();
    }
    for operation in operations {
        if let DataWrite::Insert(row) = operation {
            for (index, field) in adapter.config.schema.fields().iter().enumerate() {
                if field.name.eq_ignore_ascii_case(&adapter.config.key_column)
                    && row.values[index] != CellValue::Integer(row.key)
                {
                    return Err(DataError::Validation(
                        "inserted visible key differs from row identity".into(),
                    ));
                }
            }
        }
    }
    let transaction = adapter
        .connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(adapter_error)?;
    let before = read_table(&transaction, &adapter.config, adapter.limits)?;
    let candidate = crate::write::apply_writes(&before, operations)?;
    for operation in operations {
        match operation {
            DataWrite::Insert(row) => {
                let mut columns = vec![quote(&adapter.config.key_column)];
                let mut parameters = vec![Value::Integer(row.key)];
                for (field, value) in adapter.config.schema.fields().iter().zip(&row.values) {
                    if !field.name.eq_ignore_ascii_case(&adapter.config.key_column) {
                        columns.push(quote(&field.name));
                        parameters.push(sql_value(value));
                    }
                }
                let placeholders = (1..=parameters.len())
                    .map(|index| format!("?{index}"))
                    .collect::<Vec<_>>();
                let sql = format!(
                    "INSERT INTO {} ({}) VALUES ({})",
                    quote(&adapter.config.table),
                    columns.join(", "),
                    placeholders.join(", ")
                );
                let count = transaction
                    .execute(&sql, rusqlite::params_from_iter(parameters))
                    .map_err(adapter_error)?;
                if count != 1 {
                    return Err(DataError::Conflict(
                        "SQLite insert was ignored or replaced".into(),
                    ));
                }
            }
            DataWrite::Update {
                key,
                field,
                expected,
                value,
            } => {
                let schema = adapter.config.schema.field(field).expect("validated field");
                check_expected(
                    &transaction,
                    &adapter.config,
                    adapter.limits,
                    *key,
                    schema,
                    expected,
                )?;
                // The precise comparison above is under the same write lock;
                // SQL equality is an additional guard, never a typed comparator.
                let sql = format!(
                    "UPDATE {} SET {} = ?1 WHERE {} = ?2 AND {} IS ?3",
                    quote(&adapter.config.table),
                    quote(field),
                    quote(&adapter.config.key_column),
                    quote(field)
                );
                let count = transaction
                    .execute(&sql, params![sql_value(value), key, sql_value(expected)])
                    .map_err(adapter_error)?;
                if count != 1 {
                    return Err(DataError::Conflict(format!(
                        "row {key} update was ignored or changed"
                    )));
                }
            }
            DataWrite::Delete { expected } => {
                for (field, value) in adapter.config.schema.fields().iter().zip(&expected.values) {
                    check_expected(
                        &transaction,
                        &adapter.config,
                        adapter.limits,
                        expected.key,
                        field,
                        value,
                    )?;
                }
                let sql = format!(
                    "DELETE FROM {} WHERE {} = ?1",
                    quote(&adapter.config.table),
                    quote(&adapter.config.key_column)
                );
                if transaction
                    .execute(&sql, [expected.key])
                    .map_err(adapter_error)?
                    != 1
                {
                    return Err(DataError::Conflict(format!(
                        "row {} delete was ignored or changed",
                        expected.key
                    )));
                }
            }
        }
    }
    let next = read_table(&transaction, &adapter.config, adapter.limits)?;
    // A trigger or affinity must not silently substitute values, remove another
    // configured row or introduce another key. Other tables remain application
    // owned and participate in the same SQLite transaction normally.
    if candidate.rows().len() != next.rows().len()
        || candidate
            .rows()
            .iter()
            .any(|row| next.row(row.key) != Some(row))
    {
        return Err(DataError::Conflict(
            "SQLite affinity or trigger changed the requested batch result".into(),
        ));
    }
    transaction.commit().map_err(adapter_error)?;
    Ok(next)
}
