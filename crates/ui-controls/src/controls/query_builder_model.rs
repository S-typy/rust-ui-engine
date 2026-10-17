//! Accepted and draft query state over the shared query language.
use super::*;
use rust_desktop_ui_data::{
    Criteria, ExpressionType, QueryEngine, QueryExpression, QueryRequest, TableSchema,
    format_criteria_text, format_expression_text, parse_criteria_text, parse_expression_text,
};
#[derive(Clone, Debug, PartialEq)]
pub enum QueryBuilderValue {
    Criteria(Criteria),
    Expression(QueryExpression),
}
impl QueryBuilderValue {
    pub fn to_text(&self) -> Result<String, ControlError> {
        match self {
            Self::Criteria(v) => format_criteria_text(v),
            Self::Expression(v) => format_expression_text(v),
        }
        .map_err(invalid)
    }
    pub fn parse_like(&self, schema: &TableSchema, text: &str) -> Result<Self, ControlError> {
        match self {
            Self::Criteria(_) => parse_criteria_text(schema, text).map(Self::Criteria),
            Self::Expression(_) => parse_expression_text(schema, text).map(Self::Expression),
        }
        .map_err(invalid)
    }
    pub fn validate(&self, schema: &TableSchema) -> Result<(), ControlError> {
        self.to_text()?;
        match self {
            Self::Criteria(v) => {
                QueryEngine::compile(
                    schema,
                    &QueryRequest {
                        filter: Some(v.clone()),
                        ..Default::default()
                    },
                )
                .map_err(invalid)?;
            }
            Self::Expression(v) => {
                QueryEngine::expression_type(schema, v).map_err(invalid)?;
            }
        }
        Ok(())
    }
    pub fn expression_type(
        &self,
        schema: &TableSchema,
    ) -> Result<Option<ExpressionType>, ControlError> {
        match self {
            Self::Expression(v) => QueryEngine::expression_type(schema, v)
                .map(Some)
                .map_err(invalid),
            _ => Ok(None),
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum QueryBuilderMode {
    #[default]
    Visual,
    Text,
}
#[derive(Clone, Debug, Default)]
pub struct QueryBuilderSettings {
    pub read_only: bool,
    pub mode: QueryBuilderMode,
}
type QueryValidationCallback = dyn Fn(&QueryBuilderValue) -> Result<(), String>;
#[derive(Clone)]
pub struct QueryBuilderValidator(std::rc::Rc<QueryValidationCallback>);
impl QueryBuilderValidator {
    pub fn new(f: impl Fn(&QueryBuilderValue) -> Result<(), String> + 'static) -> Self {
        Self(std::rc::Rc::new(f))
    }
    pub(super) fn check(&self, v: &QueryBuilderValue) -> Result<(), ControlError> {
        (self.0)(v).map_err(invalid)
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct QueryBuilderApplied {
    pub id: WidgetId,
    pub value: QueryBuilderValue,
}
pub(super) fn invalid(e: impl ToString) -> ControlError {
    ControlError::Invalid(e.to_string())
}
pub(super) fn criteria_at<'a>(v: &'a Criteria, path: &[usize]) -> Option<&'a Criteria> {
    if path.is_empty() {
        return Some(v);
    }
    let next = match v {
        Criteria::All(v) | Criteria::Any(v) => v.get(path[0]),
        Criteria::Not(v) if path[0] == 0 => Some(v.as_ref()),
        _ => None,
    }?;
    criteria_at(next, &path[1..])
}
pub(super) fn replace_criteria(v: &mut Criteria, path: &[usize], replacement: Criteria) -> bool {
    if path.is_empty() {
        *v = replacement;
        return true;
    }
    let next = match v {
        Criteria::All(v) | Criteria::Any(v) => v.get_mut(path[0]),
        Criteria::Not(v) if path[0] == 0 => Some(v.as_mut()),
        _ => None,
    };
    next.is_some_and(|v| replace_criteria(v, &path[1..], replacement))
}
pub(super) fn visual_supported(v: &QueryBuilderValue) -> bool {
    fn simple(v: &QueryExpression) -> bool {
        matches!(v, QueryExpression::Field(_) | QueryExpression::Literal(_))
    }
    fn criteria(v: &Criteria) -> bool {
        match v {
            Criteria::Constant(_) => true,
            Criteria::All(v) | Criteria::Any(v) => v.iter().all(criteria),
            Criteria::Not(v) => criteria(v),
            Criteria::Compare { left, right, .. } => {
                matches!(left, QueryExpression::Field(_))
                    && matches!(right, QueryExpression::Literal(_))
            }
            Criteria::IsNull(v) => matches!(v, QueryExpression::Field(_)),
            Criteria::TextMatch { text, pattern, .. } => {
                matches!(text, QueryExpression::Field(_))
                    && matches!(
                        pattern,
                        QueryExpression::Literal(rust_desktop_ui_data::CellValue::Text(_))
                    )
            }
        }
    }
    match v {
        QueryBuilderValue::Criteria(v) => criteria(v),
        QueryBuilderValue::Expression(v) => match v {
            QueryExpression::Field(_) | QueryExpression::Literal(_) => true,
            QueryExpression::Arithmetic { left, right, .. }
            | QueryExpression::Coalesce(left, right) => simple(left) && simple(right),
            QueryExpression::Lowercase(v) | QueryExpression::TextLength(v) => simple(v),
        },
    }
}
