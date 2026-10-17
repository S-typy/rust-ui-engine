//! Bounded canonical text for the shared query AST; no evaluator or SQL grammar.
use crate::*;
use std::fmt;
mod format;
mod parse;
#[cfg(test)]
mod tests;
pub const MAX_QUERY_TEXT_BYTES: usize = 16 * 1024;
pub const MAX_QUERY_TEXT_NODES: usize = 128;
pub const MAX_QUERY_TEXT_DEPTH: usize = 16;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueryTextError {
    pub offset: usize,
    pub message: String,
}
impl fmt::Display for QueryTextError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} at byte {}", self.message, self.offset)
    }
}
impl std::error::Error for QueryTextError {}
fn error(offset: usize, message: impl ToString) -> QueryTextError {
    QueryTextError {
        offset,
        message: message.to_string().chars().take(256).collect(),
    }
}
/// Parse and type-check a criterion against exact schema field names.
pub fn parse_criteria_text(schema: &TableSchema, text: &str) -> Result<Criteria, QueryTextError> {
    let mut p = parse::Parser::new(text)?;
    let value = p.criteria(0)?;
    p.finish()?;
    QueryEngine::compile(
        schema,
        &QueryRequest {
            filter: Some(value.clone()),
            ..Default::default()
        },
    )
    .map_err(|e| error(0, e))?;
    Ok(value)
}
/// Parse and type-check an expression. Use QueryEngine::expression_type for its result type.
pub fn parse_expression_text(
    schema: &TableSchema,
    text: &str,
) -> Result<QueryExpression, QueryTextError> {
    let mut p = parse::Parser::new(text)?;
    let value = p.expression(0)?;
    p.finish()?;
    QueryEngine::expression_type(schema, &value).map_err(|e| error(0, e))?;
    Ok(value)
}
pub fn format_criteria_text(value: &Criteria) -> Result<String, QueryTextError> {
    let mut w = format::Writer::default();
    w.criteria(value, 0)?;
    Ok(w.text)
}
pub fn format_expression_text(value: &QueryExpression) -> Result<String, QueryTextError> {
    let mut w = format::Writer::default();
    w.expression(value, 0)?;
    Ok(w.text)
}
/// Deterministic insertion candidates, at most 64. This does not execute text.
pub fn query_text_completions(schema: &TableSchema, prefix: &str) -> Vec<String> {
    if prefix.len() > 256 {
        return vec![];
    }
    let mut result = Vec::new();
    for field in schema.fields() {
        let e = QueryExpression::Field(field.name.clone());
        if let Ok(s) = format_expression_text(&e)
            && (field.name.starts_with(prefix) || s.starts_with(prefix))
        {
            result.push(s);
            if result.len() == 64 {
                return result;
            }
        }
    }
    for name in [
        "all(",
        "any(",
        "not(",
        "eq(",
        "ne(",
        "lt(",
        "le(",
        "gt(",
        "ge(",
        "is_null(",
        "contains(",
        "starts_with(",
        "ends_with(",
        "icontains(",
        "istarts_with(",
        "iends_with(",
        "add(",
        "sub(",
        "mul(",
        "div(",
        "lower(",
        "length(",
        "coalesce(",
        "field(",
        "text(",
        "integer(",
        "real(",
        "decimal(",
        "date(",
        "time(",
        "binary(",
        "bool(",
        "null()",
        "true()",
        "false()",
    ] {
        if name.starts_with(prefix) {
            result.push(name.into());
            if result.len() == 64 {
                break;
            }
        }
    }
    result
}
