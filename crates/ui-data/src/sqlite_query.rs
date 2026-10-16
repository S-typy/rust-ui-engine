use super::*;
use crate::{
    ComparisonOperator, CompiledQuery, Criteria, QUERY_SEMANTICS_VERSION, QueryCapabilities,
    QueryEngine, QueryExecution, QueryExpression, QueryRequest, QueryResult, SortDirection,
};

/// One explicitly loaded page. Count and rows come from the same read
/// transaction. Separate calls can see different database revisions; offset
/// paging does not promise cross-call snapshot isolation or a persistent cursor.
#[derive(Clone, Debug, PartialEq)]
pub struct SqliteQueryPage {
    pub table: DataTable,
    pub offset: usize,
    pub total_rows: usize,
    pub execution: QueryExecution,
}

impl SqliteAdapter {
    /// Planning only: no I/O and no source mutation. The actual execution also
    /// validates schema/encoding inside its read transaction.
    pub fn query_execution(&self, request: &QueryRequest) -> Result<QueryExecution, DataError> {
        let compiled = QueryEngine::compile(&self.config.schema, request)?;
        Ok(compiled.execution(capabilities(self, request)))
    }

    /// Explicit synchronous page read. Filter/order operate before LIMIT/OFFSET;
    /// equal sort terms use ascending signed keys. SQL-supported requests can
    /// page a table larger than max_rows, while each returned page stays bounded.
    /// Validation scans storage with bounded memory; this is not an indexed-only
    /// server mode. Unsupported expressions use a complete bounded local load.
    /// Grouping/aggregates use the unpaged QueryEngine API instead.
    pub fn query_page(
        &mut self,
        request: &QueryRequest,
        offset: usize,
        limit: usize,
    ) -> Result<SqliteQueryPage, DataError> {
        if limit == 0
            || limit > self.limits.max_rows
            || offset.checked_add(limit).is_none()
            || i64::try_from(offset).is_err()
        {
            return Err(DataError::Capacity(
                "SQLite page range exceeds limits".into(),
            ));
        }
        if !request.group_by.is_empty() || !request.aggregates.is_empty() {
            return Err(DataError::Validation(
                "SQLite pages contain rows; grouping and aggregates require an unpaged query"
                    .into(),
            ));
        }
        let compiled = QueryEngine::compile(&self.config.schema, request)?;
        self.query_schema()?;
        let plan = if self.query_utf8 {
            sql_plan(&self.config, request)
        } else {
            None
        };
        let cookie = self.query_schema_cookie;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .map_err(adapter_error)?;
        check_schema(&transaction, cookie, plan.is_some())?;
        let page = if let Some(plan) = plan {
            let validated_count =
                validate_storage(&transaction, &self.config, self.limits, &compiled)?;
            let count_sql = format!(
                "SELECT count(*) FROM {} AS t WHERE {}",
                quote(&self.config.table),
                plan.filter
            );
            let count: i64 = transaction
                .query_row(
                    &count_sql,
                    rusqlite::params_from_iter(&plan.parameters[..plan.filter_parameter_count]),
                    |row| row.get(0),
                )
                .map_err(adapter_error)?;
            let total_rows = usize::try_from(count)
                .map_err(|_| DataError::Capacity("SQLite count overflow".into()))?;
            if validated_count != total_rows {
                return Err(DataError::Adapter(
                    "SQLite criteria disagreed with typed validation".into(),
                ));
            }
            let mut parameters = plan.parameters;
            let sql = format!(
                "SELECT {} FROM {} AS t WHERE {} ORDER BY {} LIMIT ?{} OFFSET ?{}",
                selection(&self.config),
                quote(&self.config.table),
                plan.filter,
                plan.order,
                parameters.len() + 1,
                parameters.len() + 2
            );
            parameters.push(Value::Integer(limit as i64));
            parameters.push(Value::Integer(offset as i64));
            let table = read_page(&transaction, &self.config, self.limits, &sql, parameters)?;
            SqliteQueryPage {
                table,
                offset,
                total_rows,
                execution: QueryExecution::Pushdown,
            }
        } else {
            let table = read_table(&transaction, &self.config, self.limits)?;
            let result = compiled.execute(&table)?;
            let total_rows = result.row_keys.len();
            let rows = result
                .row_keys
                .into_iter()
                .skip(offset)
                .take(limit)
                .map(|key| table.row(key).expect("local query key").clone())
                .collect();
            let table = DataTable::with_limits(self.config.schema.clone(), rows, self.limits)?;
            SqliteQueryPage {
                table,
                offset,
                total_rows,
                execution: QueryExecution::Local,
            }
        };
        transaction.commit().map_err(adapter_error)?;
        Ok(page)
    }
}

pub(super) fn is_utf8(connection: &Connection) -> Result<bool, DataError> {
    let encoding: String = connection
        .query_row("PRAGMA encoding", [], |row| row.get(0))
        .map_err(adapter_error)?;
    Ok(encoding.eq_ignore_ascii_case("UTF-8"))
}

pub(super) fn schema_cookie(connection: &Connection) -> Result<i64, DataError> {
    connection
        .query_row("PRAGMA schema_version", [], |row| row.get(0))
        .map_err(adapter_error)
}

fn check_schema(
    connection: &Connection,
    expected: Option<i64>,
    utf8: bool,
) -> Result<(), DataError> {
    if let Some(expected) = expected
        && schema_cookie(connection)? != expected
    {
        return Err(DataError::Conflict(
            "SQLite schema changed after query planning".into(),
        ));
    }
    if utf8 && !is_utf8(connection)? {
        return Err(DataError::Conflict(
            "SQLite encoding changed after query planning".into(),
        ));
    }
    Ok(())
}

pub(super) fn capabilities(adapter: &SqliteAdapter, request: &QueryRequest) -> QueryCapabilities {
    // Compile before traversing the AST to enforce its depth/node/payload limits.
    if adapter.query_utf8
        && QueryEngine::compile(&adapter.config.schema, request).is_ok()
        && sql_plan(&adapter.config, request).is_some()
    {
        QueryCapabilities {
            semantics_version: QUERY_SEMANTICS_VERSION,
            filtering: true,
            ordering: true,
            expressions: false,
            grouping: false,
            aggregates: false,
        }
    } else {
        QueryCapabilities::local_only()
    }
}

pub(super) fn execute(
    adapter: &mut SqliteAdapter,
    request: &QueryRequest,
) -> Result<QueryResult, DataError> {
    let compiled = QueryEngine::compile(&adapter.config.schema, request)?;
    let plan = sql_plan(&adapter.config, request)
        .filter(|_| adapter.query_utf8)
        .ok_or_else(|| DataError::Adapter("query requires complete local fallback".into()))?;
    let transaction = adapter
        .connection
        .transaction_with_behavior(TransactionBehavior::Deferred)
        .map_err(adapter_error)?;
    check_schema(&transaction, adapter.query_schema_cookie, true)?;
    // Preserve the bounded snapshot and scalar/error semantics of the generic
    // provider API. SQL owns filtering/order, not unchecked storage coercions.
    let table = read_table(&transaction, &adapter.config, adapter.limits)?;
    compiled.validate_pushdown_snapshot(&table)?;
    let sql = format!(
        "SELECT t.{} FROM {} AS t WHERE {} ORDER BY {}",
        quote(&adapter.config.key_column),
        quote(&adapter.config.table),
        plan.filter,
        plan.order
    );
    let mut statement = transaction.prepare(&sql).map_err(adapter_error)?;
    let row_keys = statement
        .query_map(rusqlite::params_from_iter(plan.parameters), |row| {
            row.get(0)
        })
        .map_err(adapter_error)?
        .collect::<Result<Vec<i64>, _>>()
        .map_err(adapter_error)?;
    drop(statement);
    transaction.commit().map_err(adapter_error)?;
    Ok(QueryResult {
        row_keys,
        aggregates: vec![],
        groups: vec![],
    })
}

struct SqlPlan {
    filter: String,
    order: String,
    parameters: Vec<Value>,
    filter_parameter_count: usize,
}

fn sql_plan(config: &SqliteTableConfig, request: &QueryRequest) -> Option<SqlPlan> {
    if !request.group_by.is_empty() || !request.aggregates.is_empty() {
        return None;
    }
    let mut parameters = Vec::new();
    let filter = match &request.filter {
        Some(criteria) => criteria_sql(config, criteria, &mut parameters)?,
        None => "1".into(),
    };
    let filter_parameter_count = parameters.len();
    let mut order = Vec::new();
    for term in &request.order_by {
        if expression_kind(config, &term.expression) == Some(FieldType::Decimal) {
            return None;
        }
        let expression = expression_sql(&term.expression, &mut parameters)?;
        order.push(format!(
            "{expression} {}",
            if term.direction == SortDirection::Descending {
                "DESC"
            } else {
                "ASC"
            }
        ));
    }
    order.push(format!("t.{} ASC", quote(&config.key_column)));
    Some(SqlPlan {
        filter,
        order: order.join(", "),
        parameters,
        filter_parameter_count,
    })
}

fn expression_kind(config: &SqliteTableConfig, expression: &QueryExpression) -> Option<FieldType> {
    QueryEngine::expression_type(&config.schema, expression)
        .ok()?
        .kind
}

fn expression_sql(expression: &QueryExpression, parameters: &mut Vec<Value>) -> Option<String> {
    Some(match expression {
        // Unary + strips SQLite column affinity without converting its value.
        // Explicit BINARY overrides declared NOCASE/RTRIM column collation.
        QueryExpression::Field(field) => format!("(+t.{} COLLATE BINARY)", quote(field)),
        QueryExpression::Literal(value) => {
            parameters.push(sql_value(value));
            format!("(?{} COLLATE BINARY)", parameters.len())
        }
        _ => return None,
    })
}

fn criteria_sql(
    config: &SqliteTableConfig,
    criteria: &Criteria,
    parameters: &mut Vec<Value>,
) -> Option<String> {
    Some(match criteria {
        Criteria::Constant(value) => if *value { "1" } else { "0" }.into(),
        Criteria::Compare {
            left,
            operator,
            right,
        } => {
            if expression_kind(config, left) == Some(FieldType::Decimal)
                || expression_kind(config, right) == Some(FieldType::Decimal)
            {
                return None;
            }
            let left = expression_sql(left, parameters)?;
            let right = expression_sql(right, parameters)?;
            let operator = match operator {
                ComparisonOperator::Equal => "=",
                ComparisonOperator::NotEqual => "<>",
                ComparisonOperator::Less => "<",
                ComparisonOperator::LessOrEqual => "<=",
                ComparisonOperator::Greater => ">",
                ComparisonOperator::GreaterOrEqual => ">=",
            };
            format!("({left} {operator} {right})")
        }
        Criteria::IsNull(expression) => {
            format!("({} IS NULL)", expression_sql(expression, parameters)?)
        }
        Criteria::Not(value) => format!("(NOT {})", criteria_sql(config, value, parameters)?),
        Criteria::All(values) | Criteria::Any(values) => {
            let all = matches!(criteria, Criteria::All(_));
            if values.is_empty() {
                return Some(if all { "1" } else { "0" }.into());
            }
            let values = values
                .iter()
                .map(|value| criteria_sql(config, value, parameters))
                .collect::<Option<Vec<_>>>()?;
            format!("({})", values.join(if all { " AND " } else { " OR " }))
        }
        Criteria::TextMatch { .. } => return None,
    })
}

fn selection(config: &SqliteTableConfig) -> String {
    std::iter::once(&config.key_column)
        .chain(config.schema.fields().iter().map(|field| &field.name))
        .map(|field| format!("t.{}", quote(field)))
        .collect::<Vec<_>>()
        .join(", ")
}

fn initial_bytes(config: &SqliteTableConfig) -> usize {
    config
        .schema
        .fields()
        .iter()
        .map(|field| field.name.len() * 2 + field.title.len())
        .sum()
}

fn typed_row(
    raw: &rusqlite::Row<'_>,
    config: &SqliteTableConfig,
    limits: DataLimits,
    bytes: &mut usize,
) -> Result<DataRow, DataError> {
    let key = match raw.get_ref(0).map_err(adapter_error)? {
        ValueRef::Integer(key) => key,
        _ => {
            return Err(DataError::Schema(
                "SQLite key must contain signed integers only".into(),
            ));
        }
    };
    *bytes = bytes
        .checked_add(
            size_of::<DataRow>()
                + 4 * size_of::<usize>()
                + config.schema.fields().len() * size_of::<CellValue>(),
        )
        .ok_or_else(|| DataError::Capacity("SQLite row byte count overflow".into()))?;
    if *bytes > limits.max_bytes {
        return Err(DataError::Capacity(
            "SQLite row/page byte limit exceeded".into(),
        ));
    }
    let values = config
        .schema
        .fields()
        .iter()
        .enumerate()
        .map(|(index, field)| {
            read_value(
                raw.get_ref(index + 1).map_err(adapter_error)?,
                field,
                limits,
                bytes,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(DataRow::new(key, values))
}

fn validate_storage(
    connection: &Connection,
    config: &SqliteTableConfig,
    limits: DataLimits,
    query: &CompiledQuery,
) -> Result<usize, DataError> {
    // Validate even rows excluded by SQL, with at most one row in memory. The
    // total database may exceed snapshot bounds; page bounds still apply.
    DataTable::with_limits(config.schema.clone(), vec![], limits)?;
    let sql = format!(
        "SELECT {} FROM {} AS t ORDER BY t.{}",
        selection(config),
        quote(&config.table),
        quote(&config.key_column)
    );
    let mut statement = connection.prepare(&sql).map_err(adapter_error)?;
    let mut rows = statement.query([]).map_err(adapter_error)?;
    let mut previous = None;
    let mut matches = 0usize;
    while let Some(raw) = rows.next().map_err(adapter_error)? {
        let row = typed_row(raw, config, limits, &mut initial_bytes(config))?;
        if previous.is_some_and(|key| key >= row.key) {
            return Err(DataError::Schema(
                "SQLite keys are not unique signed integers".into(),
            ));
        }
        previous = Some(row.key);
        if query.validate_pushdown_row(&row, limits)? {
            matches = matches
                .checked_add(1)
                .ok_or_else(|| DataError::Capacity("SQLite count overflow".into()))?;
        }
    }
    Ok(matches)
}

fn read_page(
    connection: &Connection,
    config: &SqliteTableConfig,
    limits: DataLimits,
    sql: &str,
    parameters: Vec<Value>,
) -> Result<DataTable, DataError> {
    let mut statement = connection.prepare(sql).map_err(adapter_error)?;
    let mut rows = statement
        .query(rusqlite::params_from_iter(parameters))
        .map_err(adapter_error)?;
    let mut output = Vec::new();
    let mut bytes = initial_bytes(config);
    while let Some(raw) = rows.next().map_err(adapter_error)? {
        if output.len() >= limits.max_rows {
            return Err(DataError::Capacity("SQLite page row count exceeded".into()));
        }
        output.push(typed_row(raw, config, limits, &mut bytes)?);
    }
    DataTable::with_limits(config.schema.clone(), output, limits)
}
