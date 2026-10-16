use std::cmp::Ordering;

use crate::{DataLimits, DataRow, DecimalValue};

use super::{compile::ExpressionNode, *};

impl CompiledExpression {
    pub(super) fn evaluate(
        &self,
        row: &DataRow,
        limits: DataLimits,
    ) -> Result<CellValue, DataError> {
        let value = match &self.node {
            ExpressionNode::Field(index) => row.values[*index].clone(),
            ExpressionNode::Literal(value) => value.clone(),
            ExpressionNode::Arithmetic(left, operator, right) => arithmetic(
                &left.evaluate(row, limits)?,
                *operator,
                &right.evaluate(row, limits)?,
            )?,
            ExpressionNode::Coalesce(left, right) => match left.evaluate(row, limits)? {
                CellValue::Null => right.evaluate(row, limits)?,
                value => value,
            },
            ExpressionNode::Lowercase(value) => match value.evaluate(row, limits)? {
                CellValue::Text(text) => CellValue::Text(lowercase(&text, limits)?),
                CellValue::Null => CellValue::Null,
                _ => unreachable!("compiled text expression"),
            },
            ExpressionNode::TextLength(value) => match value.evaluate(row, limits)? {
                CellValue::Text(text) => CellValue::Integer(text.chars().count() as i64),
                CellValue::Null => CellValue::Null,
                _ => unreachable!("compiled text expression"),
            },
        };
        validate_scalar(&value, limits)?;
        Ok(value)
    }
}

impl CompiledCriteria {
    pub(super) fn evaluate(
        &self,
        row: &DataRow,
        limits: DataLimits,
    ) -> Result<TruthValue, DataError> {
        use TruthValue::{False, True, Unknown};
        let truth = |value| if value { True } else { False };
        Ok(match self {
            Self::Constant(value) => truth(*value),
            Self::Compare(left, operator, right) => {
                let left = left.evaluate(row, limits)?;
                let right = right.evaluate(row, limits)?;
                if left == CellValue::Null || right == CellValue::Null {
                    Unknown
                } else {
                    let order = compare_values(&left, &right);
                    truth(match operator {
                        ComparisonOperator::Equal => order == Ordering::Equal,
                        ComparisonOperator::NotEqual => order != Ordering::Equal,
                        ComparisonOperator::Less => order == Ordering::Less,
                        ComparisonOperator::LessOrEqual => order != Ordering::Greater,
                        ComparisonOperator::Greater => order == Ordering::Greater,
                        ComparisonOperator::GreaterOrEqual => order != Ordering::Less,
                    })
                }
            }
            Self::IsNull(expression) => truth(expression.evaluate(row, limits)? == CellValue::Null),
            Self::Not(value) => match value.evaluate(row, limits)? {
                False => True,
                True => False,
                Unknown => Unknown,
            },
            Self::All(values) => {
                let mut result = True;
                for value in values {
                    match value.evaluate(row, limits)? {
                        False => return Ok(False),
                        Unknown => result = Unknown,
                        True => {}
                    }
                }
                result
            }
            Self::Any(values) => {
                let mut result = False;
                for value in values {
                    match value.evaluate(row, limits)? {
                        True => return Ok(True),
                        Unknown => result = Unknown,
                        False => {}
                    }
                }
                result
            }
            Self::TextMatch(text, pattern, operator, comparison) => {
                match (text.evaluate(row, limits)?, pattern.evaluate(row, limits)?) {
                    (CellValue::Null, _) | (_, CellValue::Null) => Unknown,
                    (CellValue::Text(mut text), CellValue::Text(mut pattern)) => {
                        if *comparison == TextComparison::UnicodeLowercase {
                            text = lowercase(&text, limits)?;
                            pattern = lowercase(&pattern, limits)?;
                        }
                        truth(match operator {
                            TextMatchOperator::Contains => text.contains(&pattern),
                            TextMatchOperator::StartsWith => text.starts_with(&pattern),
                            TextMatchOperator::EndsWith => text.ends_with(&pattern),
                        })
                    }
                    _ => unreachable!("compiled text criteria"),
                }
            }
        })
    }
}

fn validate_scalar(value: &CellValue, limits: DataLimits) -> Result<(), DataError> {
    let payload = match value {
        CellValue::Text(text) => {
            if text.len() > limits.max_text_bytes {
                return Err(DataError::Capacity(
                    "query text result limit exceeded".into(),
                ));
            }
            text.len()
        }
        CellValue::Binary(bytes) => {
            if bytes.len() > limits.max_binary_bytes {
                return Err(DataError::Capacity(
                    "query binary result limit exceeded".into(),
                ));
            }
            bytes.len()
        }
        _ => 0,
    };
    if std::mem::size_of::<CellValue>() + payload > limits.max_bytes {
        return Err(DataError::Capacity(
            "query scalar byte limit exceeded".into(),
        ));
    }
    Ok(())
}

fn lowercase(text: &str, limits: DataLimits) -> Result<String, DataError> {
    let text = text.to_lowercase();
    if text.len() > limits.max_text_bytes
        || std::mem::size_of::<CellValue>() + text.len() > limits.max_bytes
    {
        return Err(DataError::Capacity(
            "query lowercase result limit exceeded".into(),
        ));
    }
    Ok(text)
}

fn arithmetic(
    left: &CellValue,
    operator: ArithmeticOperator,
    right: &CellValue,
) -> Result<CellValue, DataError> {
    use ArithmeticOperator::{Add, Divide, Multiply, Subtract};
    if *left == CellValue::Null || *right == CellValue::Null {
        return Ok(CellValue::Null);
    }
    match (left, right) {
        (CellValue::Integer(left), CellValue::Integer(right)) => {
            let result = match operator {
                Add => left.checked_add(*right),
                Subtract => left.checked_sub(*right),
                Multiply => left.checked_mul(*right),
                Divide => left.checked_div(*right),
            };
            result
                .map(CellValue::Integer)
                .ok_or_else(|| invalid("integer overflow or division by zero"))
        }
        (CellValue::Real(left), CellValue::Real(right)) => {
            let result = match operator {
                Add => left + right,
                Subtract => left - right,
                Multiply => left * right,
                Divide => left / right,
            };
            finite_real(result)
        }
        (CellValue::Decimal(left), CellValue::Decimal(right)) => Ok(CellValue::Decimal(
            decimal_arithmetic(*left, operator, *right)?,
        )),
        _ => unreachable!("compiled numeric expression"),
    }
}

fn finite_real(value: f64) -> Result<CellValue, DataError> {
    if value.is_finite() {
        Ok(CellValue::Real(value))
    } else {
        Err(invalid("non-finite real result"))
    }
}

fn decimal_arithmetic(
    left: DecimalValue,
    operator: ArithmeticOperator,
    right: DecimalValue,
) -> Result<DecimalValue, DataError> {
    let overflow = || invalid("decimal coefficient or scale overflow");
    let (coefficient, scale) = match operator {
        ArithmeticOperator::Add | ArithmeticOperator::Subtract => {
            let scale = left.scale().max(right.scale());
            let left = left
                .coefficient()
                .checked_mul(10i128.pow(u32::from(scale - left.scale())))
                .ok_or_else(overflow)?;
            let right = right
                .coefficient()
                .checked_mul(10i128.pow(u32::from(scale - right.scale())))
                .ok_or_else(overflow)?;
            let coefficient = if operator == ArithmeticOperator::Add {
                left.checked_add(right)
            } else {
                left.checked_sub(right)
            }
            .ok_or_else(overflow)?;
            (coefficient, scale)
        }
        ArithmeticOperator::Multiply => {
            let mut coefficient = left
                .coefficient()
                .checked_mul(right.coefficient())
                .ok_or_else(overflow)?;
            let mut scale = left.scale() + right.scale();
            while scale > 0 && coefficient % 10 == 0 {
                coefficient /= 10;
                scale -= 1;
            }
            (coefficient, scale)
        }
        ArithmeticOperator::Divide => return Err(invalid("decimal division is not supported")),
    };
    DecimalValue::new(coefficient, scale).map_err(|_| overflow())
}

/// Division by a bounded row count, retaining exact decimal digits and rounding
/// the last retained digit half away from zero. No floating conversion occurs.
fn decimal_average(value: DecimalValue, count: i64) -> Result<DecimalValue, DataError> {
    let count = i128::from(count);
    let mut coefficient = value.coefficient() / count;
    let mut remainder = value.coefficient() % count;
    let mut scale = value.scale();
    while remainder != 0 && scale < DecimalValue::MAX_SCALE {
        remainder *= 10; // abs(remainder) < count <= the bounded row count.
        coefficient = coefficient
            .checked_mul(10)
            .and_then(|value| value.checked_add(remainder / count))
            .ok_or_else(|| invalid("decimal average coefficient overflow"))?;
        remainder %= count;
        scale += 1;
    }
    if remainder.abs() * 2 >= count {
        coefficient = coefficient
            .checked_add(remainder.signum())
            .ok_or_else(|| invalid("decimal average rounding overflow"))?;
    }
    DecimalValue::new(coefficient, scale)
}

struct EvaluatedRow {
    index: usize,
    key: i64,
    sort: Vec<CellValue>,
    group: Vec<CellValue>,
}

struct Budget {
    used: usize,
    max: usize,
}
impl Budget {
    fn add(&mut self, bytes: usize) -> Result<(), DataError> {
        self.used = self
            .used
            .checked_add(bytes)
            .ok_or_else(|| DataError::Capacity("query byte count overflow".into()))?;
        if self.used > self.max {
            return Err(DataError::Capacity(
                "query working/result byte limit exceeded".into(),
            ));
        }
        Ok(())
    }
    fn values(&mut self, values: &[CellValue]) -> Result<(), DataError> {
        for value in values {
            self.add(
                std::mem::size_of::<CellValue>()
                    + match value {
                        CellValue::Text(text) => text.len(),
                        CellValue::Binary(bytes) => bytes.len(),
                        _ => 0,
                    },
            )?;
        }
        Ok(())
    }
}

pub(super) fn execute(query: &CompiledQuery, table: &DataTable) -> Result<QueryResult, DataError> {
    let limits = table.limits();
    let mut budget = Budget {
        used: 0,
        max: table.limits().max_bytes,
    };
    let mut rows = Vec::new();
    // Canonical evaluation order makes errors and floating aggregates independent
    // of collection insertion order and provider storage order.
    let mut indices: Vec<_> = (0..table.rows().len()).collect();
    budget.add(indices.len() * std::mem::size_of::<usize>())?;
    indices.sort_by_key(|index| table.rows()[*index].key);
    for index in indices {
        let row = &table.rows()[index];
        if let Some(filter) = &query.filter
            && filter.evaluate(row, limits)? != TruthValue::True
        {
            continue;
        }
        budget.add(std::mem::size_of::<EvaluatedRow>())?;
        let mut sort = Vec::with_capacity(query.order_by.len());
        for (expression, _) in &query.order_by {
            let value = expression.evaluate(row, limits)?;
            budget.values(std::slice::from_ref(&value))?;
            sort.push(value);
        }
        let mut group = Vec::with_capacity(query.group_by.len());
        for expression in &query.group_by {
            let value = expression.evaluate(row, limits)?;
            budget.values(std::slice::from_ref(&value))?;
            group.push(value);
        }
        rows.push(EvaluatedRow {
            index,
            key: row.key,
            sort,
            group,
        });
    }
    rows.sort_by(|left, right| {
        for ((left, right), (_, direction)) in
            left.sort.iter().zip(&right.sort).zip(&query.order_by)
        {
            let order = compare_values(left, right);
            let order = if *direction == SortDirection::Descending {
                order.reverse()
            } else {
                order
            };
            if order != Ordering::Equal {
                return order;
            }
        }
        left.key.cmp(&right.key)
    });
    budget.add(rows.len() * std::mem::size_of::<i64>())?;
    let row_keys = rows.iter().map(|row| row.key).collect();
    let aggregates = aggregate_rows(query, table, rows.iter().map(|row| row.index), &mut budget)?;
    let mut groups: Vec<QueryGroup> = Vec::new();
    if !query.group_by.is_empty() {
        budget.add(rows.len() * std::mem::size_of::<usize>())?;
        let mut grouped: Vec<_> = (0..rows.len()).collect();
        grouped.sort_by(|left, right| {
            compare_vectors(&rows[*left].group, &rows[*right].group).then_with(|| left.cmp(right))
        });
        let mut start = 0;
        while start < grouped.len() {
            let key = &rows[grouped[start]].group;
            let mut end = start + 1;
            while end < grouped.len()
                && compare_vectors(key, &rows[grouped[end]].group) == Ordering::Equal
            {
                end += 1;
            }
            budget.add(
                std::mem::size_of::<QueryGroup>() + (end - start) * std::mem::size_of::<i64>(),
            )?;
            budget.values(key)?;
            let aggregates = aggregate_rows(
                query,
                table,
                grouped[start..end].iter().map(|index| rows[*index].index),
                &mut budget,
            )?;
            groups.push(QueryGroup {
                key: key.clone(),
                row_keys: grouped[start..end]
                    .iter()
                    .map(|index| rows[*index].key)
                    .collect(),
                aggregates,
            });
            start = end;
        }
    }
    Ok(QueryResult {
        row_keys,
        aggregates,
        groups,
    })
}

fn compare_vectors(left: &[CellValue], right: &[CellValue]) -> Ordering {
    left.iter()
        .zip(right)
        .map(|(left, right)| compare_values(left, right))
        .find(|order| *order != Ordering::Equal)
        .unwrap_or(Ordering::Equal)
}

fn aggregate_rows(
    query: &CompiledQuery,
    table: &DataTable,
    indices: impl Iterator<Item = usize>,
    budget: &mut Budget,
) -> Result<Vec<CellValue>, DataError> {
    if query.aggregates.is_empty() {
        return Ok(Vec::new());
    }
    let mut indices: Vec<_> = indices.collect();
    let scratch_bytes = indices.len() * std::mem::size_of::<usize>();
    budget.add(scratch_bytes)?;
    indices.sort_by_key(|index| table.rows()[*index].key);
    let values = query.aggregates.iter().map(|(function, expression)| {
        if *function == AggregateFunction::CountRows {
            return Ok(CellValue::Integer(indices.len() as i64));
        }
        let expression = expression.as_ref().expect("compiled aggregate expression");
        let mut count = 0i64;
        let mut value = CellValue::Null;
        for &index in &indices {
            let next = expression.evaluate(&table.rows()[index], table.limits())?;
            if next == CellValue::Null {
                continue;
            }
            count += 1;
            if *function == AggregateFunction::CountValues {
                continue;
            }
            let next = if *function == AggregateFunction::Average {
                match next {
                    CellValue::Integer(value) => {
                        CellValue::Decimal(DecimalValue::new(i128::from(value), 0)?)
                    }
                    next => next,
                }
            } else {
                next
            };
            value = match function {
                AggregateFunction::Minimum
                    if value == CellValue::Null
                        || compare_values(&next, &value) == Ordering::Less =>
                {
                    next
                }
                AggregateFunction::Maximum
                    if value == CellValue::Null
                        || compare_values(&next, &value) == Ordering::Greater =>
                {
                    next
                }
                AggregateFunction::Minimum | AggregateFunction::Maximum => value,
                AggregateFunction::Sum | AggregateFunction::Average if value == CellValue::Null => {
                    next
                }
                AggregateFunction::Sum | AggregateFunction::Average => {
                    arithmetic(&value, ArithmeticOperator::Add, &next)?
                }
                _ => unreachable!("counts have been handled"),
            };
        }
        if *function == AggregateFunction::CountValues {
            return Ok(CellValue::Integer(count));
        }
        if *function == AggregateFunction::Average && count > 0 {
            value = match value {
                CellValue::Decimal(value) => CellValue::Decimal(decimal_average(value, count)?),
                CellValue::Real(value) => finite_real(value / count as f64)?,
                _ => unreachable!("compiled numeric average"),
            };
        }
        Ok(value)
    });
    let mut result = Vec::with_capacity(query.aggregates.len());
    for value in values {
        let value = value?;
        budget.values(std::slice::from_ref(&value))?;
        result.push(value);
    }
    budget.used -= scratch_bytes;
    Ok(result)
}
