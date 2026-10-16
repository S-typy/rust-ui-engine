//! Typed query semantics shared by local and provider execution.

use crate::{CellValue, DataAdapter, DataError, DataTable, FieldType, SortDirection, TableSchema};

mod compile;
mod evaluate;

use compile::{CompiledCriteria, CompiledExpression};

/// Version of null, type, ordering, expression and aggregate semantics.
/// Providers must match this version before accepting a pushed request.
pub const QUERY_SEMANTICS_VERSION: u32 = 1;
pub const MAX_QUERY_NODES: usize = 512;
pub const MAX_QUERY_DEPTH: usize = 32;
pub const MAX_QUERY_TERMS: usize = 128;
pub const MAX_QUERY_LITERAL_BYTES: usize = 1024 * 1024;

/// Closed expression language. Names are exact schema field names, never SQL.
#[derive(Clone, Debug, PartialEq)]
pub enum QueryExpression {
    Field(String),
    Literal(CellValue),
    Arithmetic {
        left: Box<Self>,
        operator: ArithmeticOperator,
        right: Box<Self>,
    },
    Lowercase(Box<Self>),
    /// Number of Unicode scalar values, not bytes or grapheme clusters.
    TextLength(Box<Self>),
    Coalesce(Box<Self>, Box<Self>),
}

impl QueryExpression {
    pub fn field(name: impl Into<String>) -> Self {
        Self::Field(name.into())
    }
    pub fn literal(value: CellValue) -> Self {
        Self::Literal(value)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArithmeticOperator {
    Add,
    Subtract,
    Multiply,
    /// Integer division truncates toward zero. Decimal division is not supported.
    Divide,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ComparisonOperator {
    Equal,
    NotEqual,
    Less,
    LessOrEqual,
    Greater,
    GreaterOrEqual,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextMatchOperator {
    Contains,
    StartsWith,
    EndsWith,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextComparison {
    Ordinal,
    /// Rust Unicode lowercase followed by ordinal matching; not locale collation.
    UnicodeLowercase,
}

/// Boolean criteria use three-valued logic. Only `True` retains a row.
#[derive(Clone, Debug, PartialEq)]
pub enum Criteria {
    Constant(bool),
    Compare {
        left: QueryExpression,
        operator: ComparisonOperator,
        right: QueryExpression,
    },
    IsNull(QueryExpression),
    Not(Box<Self>),
    All(Vec<Self>),
    Any(Vec<Self>),
    TextMatch {
        text: QueryExpression,
        pattern: QueryExpression,
        operator: TextMatchOperator,
        comparison: TextComparison,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TruthValue {
    False,
    True,
    Unknown,
}

#[derive(Clone, Debug, PartialEq)]
pub struct QueryOrder {
    pub expression: QueryExpression,
    pub direction: SortDirection,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AggregateFunction {
    CountRows,
    CountValues,
    Sum,
    Minimum,
    Maximum,
    /// Integer/decimal mean returns Decimal (28 places, half away from zero).
    /// Real mean returns Real. Null values are excluded from the divisor.
    Average,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Aggregate {
    pub function: AggregateFunction,
    /// Absent only for CountRows; all other functions require an expression.
    pub expression: Option<QueryExpression>,
}

/// Query over a bounded snapshot. Empty ordering uses ascending signed row keys.
/// Equal explicit sort terms also use ascending keys, independently of direction.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct QueryRequest {
    pub filter: Option<Criteria>,
    pub order_by: Vec<QueryOrder>,
    pub group_by: Vec<QueryExpression>,
    pub aggregates: Vec<Aggregate>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExpressionType {
    /// None denotes the untyped null literal.
    pub kind: Option<FieldType>,
    pub nullable: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct QueryGroup {
    /// One value per group_by term, lexicographically ascending, null first.
    pub key: Vec<CellValue>,
    /// Members retain the complete query's row ordering.
    pub row_keys: Vec<i64>,
    /// One value per requested aggregate.
    pub aggregates: Vec<CellValue>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct QueryResult {
    pub row_keys: Vec<i64>,
    /// Global aggregates over all matching rows, before grouping.
    pub aggregates: Vec<CellValue>,
    /// Empty when group_by is empty. An empty input creates no groups.
    pub groups: Vec<QueryGroup>,
}

/// A capability is a promise to preserve *all* operators and types in its
/// category, including errors and deterministic key ordering. Unsupported
/// expressions require complete local fallback, not a partially applied query.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct QueryCapabilities {
    pub semantics_version: u32,
    pub filtering: bool,
    pub expressions: bool,
    pub ordering: bool,
    pub grouping: bool,
    pub aggregates: bool,
}

impl QueryCapabilities {
    pub const fn local_only() -> Self {
        Self {
            semantics_version: 0,
            filtering: false,
            expressions: false,
            ordering: false,
            grouping: false,
            aggregates: false,
        }
    }
    pub const fn complete() -> Self {
        Self {
            semantics_version: QUERY_SEMANTICS_VERSION,
            filtering: true,
            expressions: true,
            ordering: true,
            grouping: true,
            aggregates: true,
        }
    }
    fn supports(self, required: Self) -> bool {
        self.semantics_version == QUERY_SEMANTICS_VERSION
            && (!required.filtering || self.filtering)
            && (!required.expressions || self.expressions)
            && (!required.ordering || self.ordering)
            && (!required.grouping || self.grouping)
            && (!required.aggregates || self.aggregates)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QueryExecution {
    Local,
    Pushdown,
}

/// Immutable schema-checked query, reusable across snapshots of the same schema.
pub struct CompiledQuery {
    schema: TableSchema,
    filter: Option<CompiledCriteria>,
    order_by: Vec<(CompiledExpression, SortDirection)>,
    group_by: Vec<CompiledExpression>,
    aggregates: Vec<(AggregateFunction, Option<CompiledExpression>)>,
    required: QueryCapabilities,
}

impl CompiledQuery {
    pub fn required_capabilities(&self) -> QueryCapabilities {
        self.required
    }
    /// Never splits a request: filtering/grouping/aggregates must see the same
    /// snapshot and semantics. A version mismatch always selects local execution.
    pub fn execution(&self, provider: QueryCapabilities) -> QueryExecution {
        if provider.supports(self.required) {
            QueryExecution::Pushdown
        } else {
            QueryExecution::Local
        }
    }
    pub fn execute(&self, table: &DataTable) -> Result<QueryResult, DataError> {
        if table.schema() != &self.schema {
            return Err(DataError::Schema(
                "compiled query schema changed; compile again".into(),
            ));
        }
        evaluate::execute(self, table)
    }
}

/// Bounded, schema-checked local evaluation with no callbacks, scripts or SQL.
pub struct QueryEngine;

impl QueryEngine {
    pub fn compile(
        schema: &TableSchema,
        request: &QueryRequest,
    ) -> Result<CompiledQuery, DataError> {
        compile::compile(schema, request)
    }
    pub fn execute(table: &DataTable, request: &QueryRequest) -> Result<QueryResult, DataError> {
        Self::compile(table.schema(), request)?.execute(table)
    }
    pub fn expression_type(
        schema: &TableSchema,
        expression: &QueryExpression,
    ) -> Result<ExpressionType, DataError> {
        Ok(compile::compile_expression(schema, expression)?.kind)
    }
    pub fn evaluate_expression(
        table: &DataTable,
        key: i64,
        expression: &QueryExpression,
    ) -> Result<CellValue, DataError> {
        let expression = compile::compile_expression(table.schema(), expression)?;
        let row = table.row(key).ok_or(DataError::UnknownRow(key))?;
        expression.evaluate(row, table.limits())
    }
    pub fn evaluate_criteria(
        table: &DataTable,
        key: i64,
        criteria: &Criteria,
    ) -> Result<TruthValue, DataError> {
        let criteria = compile::compile_criteria(table.schema(), criteria)?;
        let row = table.row(key).ok_or(DataError::UnknownRow(key))?;
        criteria.evaluate(row, table.limits())
    }
    /// Explicit provider read. Pushdown requires a matching schema, full declared
    /// capabilities and an implementation of `DataAdapter::query_pushdown`.
    /// Otherwise a complete snapshot is loaded and evaluated locally. No source
    /// cache is changed and no query fragment is ever passed as SQL text.
    pub fn execute_provider(
        adapter: &mut dyn DataAdapter,
        request: &QueryRequest,
    ) -> Result<QueryResult, DataError> {
        let capabilities = adapter.query_capabilities();
        if capabilities.semantics_version == QUERY_SEMANTICS_VERSION {
            // A provider can expose cheap schema metadata. The default returns
            // None, so existing adapters do not incur an extra snapshot read.
            if let Some(schema) = adapter.query_schema()? {
                let compiled = Self::compile(&schema, request)?;
                if compiled.execution(capabilities) == QueryExecution::Pushdown {
                    return adapter.query_pushdown(request);
                }
            }
        }
        Self::execute(&adapter.load()?, request)
    }
}

fn invalid(message: &str) -> DataError {
    DataError::Validation(format!("query: {message}"))
}

pub(super) fn compare_values(left: &CellValue, right: &CellValue) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    match (left, right) {
        (CellValue::Null, CellValue::Null) => Ordering::Equal,
        (CellValue::Null, _) => Ordering::Less,
        (_, CellValue::Null) => Ordering::Greater,
        (CellValue::Bool(a), CellValue::Bool(b)) => a.cmp(b),
        (CellValue::Integer(a), CellValue::Integer(b)) => a.cmp(b),
        (CellValue::Real(a), CellValue::Real(b)) => {
            a.partial_cmp(b).expect("query reals have been validated")
        }
        (CellValue::Text(a), CellValue::Text(b)) => a.cmp(b),
        (CellValue::Decimal(a), CellValue::Decimal(b)) => a.cmp(b),
        (CellValue::Date(a), CellValue::Date(b)) => a.cmp(b),
        (CellValue::Time(a), CellValue::Time(b)) => a.cmp(b),
        (CellValue::Binary(a), CellValue::Binary(b)) => a.cmp(b),
        _ => unreachable!("query types have been checked before evaluation"),
    }
}
