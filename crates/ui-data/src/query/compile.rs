use super::*;

pub(super) struct CompiledExpression {
    pub(super) kind: ExpressionType,
    pub(super) node: ExpressionNode,
}

pub(super) enum ExpressionNode {
    Field(usize),
    Literal(CellValue),
    Arithmetic(
        Box<CompiledExpression>,
        ArithmeticOperator,
        Box<CompiledExpression>,
    ),
    Lowercase(Box<CompiledExpression>),
    TextLength(Box<CompiledExpression>),
    Coalesce(Box<CompiledExpression>, Box<CompiledExpression>),
}

pub(super) enum CompiledCriteria {
    Constant(bool),
    Compare(CompiledExpression, ComparisonOperator, CompiledExpression),
    IsNull(CompiledExpression),
    Not(Box<Self>),
    All(Vec<Self>),
    Any(Vec<Self>),
    TextMatch(
        CompiledExpression,
        CompiledExpression,
        TextMatchOperator,
        TextComparison,
    ),
}

struct Compiler<'a> {
    schema: &'a TableSchema,
    nodes: usize,
    bytes: usize,
    expressions: bool,
}

impl<'a> Compiler<'a> {
    fn new(schema: &'a TableSchema) -> Self {
        Self {
            schema,
            nodes: 0,
            bytes: 0,
            expressions: false,
        }
    }
    fn enter(&mut self, depth: usize) -> Result<(), DataError> {
        self.nodes += 1;
        if self.nodes > MAX_QUERY_NODES || depth > MAX_QUERY_DEPTH {
            return Err(DataError::Capacity("query AST limit exceeded".into()));
        }
        Ok(())
    }
    fn payload(&mut self, bytes: usize) -> Result<(), DataError> {
        self.bytes = self
            .bytes
            .checked_add(bytes)
            .ok_or_else(|| DataError::Capacity("query literal size overflow".into()))?;
        if self.bytes > MAX_QUERY_LITERAL_BYTES {
            return Err(DataError::Capacity("query literal limit exceeded".into()));
        }
        Ok(())
    }
    fn expression(
        &mut self,
        expression: &QueryExpression,
        depth: usize,
    ) -> Result<CompiledExpression, DataError> {
        self.enter(depth)?;
        let (kind, node) = match expression {
            QueryExpression::Field(name) => {
                self.payload(name.len())?;
                let index = self
                    .schema
                    .field_index(name)
                    .ok_or_else(|| DataError::UnknownField(name.clone()))?;
                let field = &self.schema.fields()[index];
                (
                    ExpressionType {
                        kind: Some(field.kind),
                        nullable: field.nullable,
                    },
                    ExpressionNode::Field(index),
                )
            }
            QueryExpression::Literal(value) => {
                self.payload(
                    std::mem::size_of::<CellValue>()
                        + match value {
                            CellValue::Text(text) => text.len(),
                            CellValue::Binary(bytes) => bytes.len(),
                            _ => 0,
                        },
                )?;
                let kind = match value {
                    CellValue::Null => None,
                    CellValue::Bool(_) => Some(FieldType::Bool),
                    CellValue::Integer(_) => Some(FieldType::Integer),
                    CellValue::Real(value) if value.is_finite() => Some(FieldType::Real),
                    CellValue::Real(_) => return Err(invalid("non-finite real literal")),
                    CellValue::Text(_) => Some(FieldType::Text),
                    CellValue::Decimal(_) => Some(FieldType::Decimal),
                    CellValue::Date(_) => Some(FieldType::Date),
                    CellValue::Time(_) => Some(FieldType::Time),
                    CellValue::Binary(_) => Some(FieldType::Binary),
                };
                (
                    ExpressionType {
                        kind,
                        nullable: kind.is_none(),
                    },
                    ExpressionNode::Literal(value.clone()),
                )
            }
            QueryExpression::Arithmetic {
                left,
                operator,
                right,
            } => {
                self.expressions = true;
                let left = self.expression(left, depth + 1)?;
                let right = self.expression(right, depth + 1)?;
                let kind = unify(left.kind, right.kind)?;
                if !matches!(
                    kind.kind,
                    Some(FieldType::Integer | FieldType::Real | FieldType::Decimal)
                ) {
                    return Err(invalid("arithmetic requires matching numeric types"));
                }
                if kind.kind == Some(FieldType::Decimal) && *operator == ArithmeticOperator::Divide
                {
                    return Err(invalid("decimal division is not supported"));
                }
                (
                    kind,
                    ExpressionNode::Arithmetic(Box::new(left), *operator, Box::new(right)),
                )
            }
            QueryExpression::Lowercase(value) | QueryExpression::TextLength(value) => {
                self.expressions = true;
                let value = self.expression(value, depth + 1)?;
                require_text(value.kind)?;
                let length = matches!(expression, QueryExpression::TextLength(_));
                let kind = ExpressionType {
                    kind: Some(if length {
                        FieldType::Integer
                    } else {
                        FieldType::Text
                    }),
                    nullable: value.kind.nullable,
                };
                (
                    kind,
                    if length {
                        ExpressionNode::TextLength(Box::new(value))
                    } else {
                        ExpressionNode::Lowercase(Box::new(value))
                    },
                )
            }
            QueryExpression::Coalesce(left, right) => {
                self.expressions = true;
                let left = self.expression(left, depth + 1)?;
                let right = self.expression(right, depth + 1)?;
                let mut kind = unify(left.kind, right.kind)?;
                kind.nullable = left.kind.nullable && right.kind.nullable;
                (
                    kind,
                    ExpressionNode::Coalesce(Box::new(left), Box::new(right)),
                )
            }
        };
        Ok(CompiledExpression { kind, node })
    }
    fn criteria(
        &mut self,
        criteria: &Criteria,
        depth: usize,
    ) -> Result<CompiledCriteria, DataError> {
        self.enter(depth)?;
        Ok(match criteria {
            Criteria::Constant(value) => CompiledCriteria::Constant(*value),
            Criteria::Compare {
                left,
                operator,
                right,
            } => {
                let left = self.expression(left, depth + 1)?;
                let right = self.expression(right, depth + 1)?;
                unify(left.kind, right.kind)?;
                CompiledCriteria::Compare(left, *operator, right)
            }
            Criteria::IsNull(value) => CompiledCriteria::IsNull(self.expression(value, depth + 1)?),
            Criteria::Not(value) => {
                CompiledCriteria::Not(Box::new(self.criteria(value, depth + 1)?))
            }
            Criteria::All(values) | Criteria::Any(values) => {
                if values.len() > MAX_QUERY_NODES {
                    return Err(DataError::Capacity("query criteria count exceeded".into()));
                }
                let values = values
                    .iter()
                    .map(|value| self.criteria(value, depth + 1))
                    .collect::<Result<_, _>>()?;
                if matches!(criteria, Criteria::All(_)) {
                    CompiledCriteria::All(values)
                } else {
                    CompiledCriteria::Any(values)
                }
            }
            Criteria::TextMatch {
                text,
                pattern,
                operator,
                comparison,
            } => {
                let text = self.expression(text, depth + 1)?;
                let pattern = self.expression(pattern, depth + 1)?;
                require_text(text.kind)?;
                require_text(pattern.kind)?;
                CompiledCriteria::TextMatch(text, pattern, *operator, *comparison)
            }
        })
    }
}

fn unify(left: ExpressionType, right: ExpressionType) -> Result<ExpressionType, DataError> {
    if left.kind.is_some() && right.kind.is_some() && left.kind != right.kind {
        return Err(invalid(
            "expression types differ; implicit conversion is forbidden",
        ));
    }
    Ok(ExpressionType {
        kind: left.kind.or(right.kind),
        nullable: left.nullable || right.nullable,
    })
}
fn require_text(kind: ExpressionType) -> Result<(), DataError> {
    if kind.kind.is_some() && kind.kind != Some(FieldType::Text) {
        return Err(invalid("text expression required"));
    }
    Ok(())
}

pub(super) fn compile_expression(
    schema: &TableSchema,
    expression: &QueryExpression,
) -> Result<CompiledExpression, DataError> {
    Compiler::new(schema).expression(expression, 1)
}
pub(super) fn compile_criteria(
    schema: &TableSchema,
    criteria: &Criteria,
) -> Result<CompiledCriteria, DataError> {
    Compiler::new(schema).criteria(criteria, 1)
}

pub(super) fn compile(
    schema: &TableSchema,
    request: &QueryRequest,
) -> Result<CompiledQuery, DataError> {
    if request.order_by.len() > MAX_QUERY_TERMS
        || request.group_by.len() > MAX_QUERY_TERMS
        || request.aggregates.len() > MAX_QUERY_TERMS
    {
        return Err(DataError::Capacity("query term count exceeded".into()));
    }
    let mut compiler = Compiler::new(schema);
    let filter = request
        .filter
        .as_ref()
        .map(|filter| compiler.criteria(filter, 1))
        .transpose()?;
    let order_by = request
        .order_by
        .iter()
        .map(|order| Ok((compiler.expression(&order.expression, 1)?, order.direction)))
        .collect::<Result<_, DataError>>()?;
    let group_by = request
        .group_by
        .iter()
        .map(|expression| compiler.expression(expression, 1))
        .collect::<Result<_, _>>()?;
    let aggregates = request
        .aggregates
        .iter()
        .map(|aggregate| {
            compiler.enter(1)?;
            let expression = aggregate
                .expression
                .as_ref()
                .map(|expression| compiler.expression(expression, 2))
                .transpose()?;
            if (aggregate.function == AggregateFunction::CountRows) != expression.is_none() {
                return Err(invalid(
                    "only CountRows requires an absent aggregate expression",
                ));
            }
            if matches!(
                aggregate.function,
                AggregateFunction::Sum | AggregateFunction::Average
            ) && !matches!(
                expression.as_ref().and_then(|value| value.kind.kind),
                Some(FieldType::Integer | FieldType::Real | FieldType::Decimal)
            ) {
                return Err(invalid("sum and average require a numeric expression"));
            }
            Ok((aggregate.function, expression))
        })
        .collect::<Result<_, DataError>>()?;
    let required = QueryCapabilities {
        semantics_version: QUERY_SEMANTICS_VERSION,
        filtering: request.filter.is_some(),
        expressions: compiler.expressions,
        // Key ordering is required even without explicit sort terms.
        ordering: true,
        grouping: !request.group_by.is_empty(),
        aggregates: !request.aggregates.is_empty(),
    };
    Ok(CompiledQuery {
        schema: schema.clone(),
        filter,
        order_by,
        group_by,
        aggregates,
        required,
    })
}
