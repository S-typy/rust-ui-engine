use super::*;
#[derive(Default)]
pub(super) struct Writer {
    pub(super) text: String,
    nodes: usize,
}
impl Writer {
    fn push(&mut self, s: &str) -> Result<(), QueryTextError> {
        if self.text.len().saturating_add(s.len()) > MAX_QUERY_TEXT_BYTES {
            return Err(error(self.text.len(), "Query text exceeds 16 KiB"));
        }
        self.text.push_str(s);
        Ok(())
    }
    fn start(&mut self, name: &str, depth: usize) -> Result<(), QueryTextError> {
        self.nodes += 1;
        if self.nodes > MAX_QUERY_TEXT_NODES || depth > MAX_QUERY_TEXT_DEPTH {
            return Err(error(0, "Query tree exceeds 128 nodes or depth 16"));
        }
        self.push(name)?;
        self.push("(")
    }
    fn string(&mut self, value: &str) -> Result<(), QueryTextError> {
        if value.len() > MAX_QUERY_TEXT_BYTES {
            return Err(error(0, "Literal exceeds text budget"));
        }
        self.push(char::from(34).encode_utf8(&mut [0; 4]))?;
        for c in value.chars() {
            match c as u32 {
                34 | 92 => {
                    self.push(char::from(92).encode_utf8(&mut [0; 4]))?;
                    self.push(c.encode_utf8(&mut [0; 4]))?;
                }
                9 | 10 | 13 => {
                    self.push(char::from(92).encode_utf8(&mut [0; 4]))?;
                    self.push(match c as u32 {
                        9 => "t",
                        10 => "n",
                        _ => "r",
                    })?;
                }
                _ if c.is_control() => {
                    self.push(&format!("{}u{{{:X}}}", char::from(92), c as u32))?
                }
                _ => self.push(c.encode_utf8(&mut [0; 4]))?,
            }
        }
        self.push(char::from(34).encode_utf8(&mut [0; 4]))
    }
    pub(super) fn expression(
        &mut self,
        value: &QueryExpression,
        depth: usize,
    ) -> Result<(), QueryTextError> {
        match value {
            QueryExpression::Field(name) => {
                self.start("field", depth)?;
                self.string(name)?;
            }
            QueryExpression::Literal(v) => {
                let name = match v {
                    CellValue::Null => "null",
                    CellValue::Bool(_) => "bool",
                    CellValue::Integer(_) => "integer",
                    CellValue::Real(_) => "real",
                    CellValue::Decimal(_) => "decimal",
                    CellValue::Text(_) => "text",
                    CellValue::Date(_) => "date",
                    CellValue::Time(_) => "time",
                    CellValue::Binary(_) => "binary",
                };
                self.start(name, depth)?;
                match v {
                    CellValue::Null => {}
                    CellValue::Bool(v) => self.string(if *v { "true" } else { "false" })?,
                    CellValue::Integer(v) => self.string(&v.to_string())?,
                    CellValue::Real(v) => {
                        if !v.is_finite() {
                            return Err(error(0, "Non-finite real"));
                        }
                        self.string(&v.to_string())?;
                    }
                    CellValue::Decimal(v) => self.string(&v.to_string())?,
                    CellValue::Date(v) => self.string(&v.to_string())?,
                    CellValue::Time(v) => self.string(&v.to_string())?,
                    CellValue::Text(v) => self.string(v)?,
                    CellValue::Binary(v) => {
                        if v.len() > MAX_QUERY_TEXT_BYTES / 2 {
                            return Err(error(0, "Binary exceeds text budget"));
                        }
                        self.push("\"")?;
                        for byte in v {
                            self.push(&format!("{byte:02X}"))?;
                        }
                        self.push("\"")?;
                    }
                }
            }
            QueryExpression::Arithmetic {
                left,
                operator,
                right,
            } => {
                self.start(
                    match operator {
                        ArithmeticOperator::Add => "add",
                        ArithmeticOperator::Subtract => "sub",
                        ArithmeticOperator::Multiply => "mul",
                        ArithmeticOperator::Divide => "div",
                    },
                    depth,
                )?;
                self.expression(left, depth + 1)?;
                self.push(",")?;
                self.expression(right, depth + 1)?;
            }
            QueryExpression::Coalesce(left, right) => {
                self.start("coalesce", depth)?;
                self.expression(left, depth + 1)?;
                self.push(",")?;
                self.expression(right, depth + 1)?;
            }
            QueryExpression::Lowercase(v) | QueryExpression::TextLength(v) => {
                self.start(
                    if matches!(value, QueryExpression::Lowercase(_)) {
                        "lower"
                    } else {
                        "length"
                    },
                    depth,
                )?;
                self.expression(v, depth + 1)?;
            }
        }
        self.push(")")
    }
    pub(super) fn criteria(
        &mut self,
        value: &Criteria,
        depth: usize,
    ) -> Result<(), QueryTextError> {
        match value {
            Criteria::Constant(v) => self.start(if *v { "true" } else { "false" }, depth)?,
            Criteria::All(v) | Criteria::Any(v) => {
                self.start(
                    if matches!(value, Criteria::All(_)) {
                        "all"
                    } else {
                        "any"
                    },
                    depth,
                )?;
                for (i, v) in v.iter().enumerate() {
                    if i != 0 {
                        self.push(",")?;
                    }
                    self.criteria(v, depth + 1)?;
                }
            }
            Criteria::Not(v) => {
                self.start("not", depth)?;
                self.criteria(v, depth + 1)?;
            }
            Criteria::IsNull(v) => {
                self.start("is_null", depth)?;
                self.expression(v, depth + 1)?;
            }
            Criteria::Compare {
                left,
                operator,
                right,
            } => {
                self.start(
                    match operator {
                        ComparisonOperator::Equal => "eq",
                        ComparisonOperator::NotEqual => "ne",
                        ComparisonOperator::Less => "lt",
                        ComparisonOperator::LessOrEqual => "le",
                        ComparisonOperator::Greater => "gt",
                        ComparisonOperator::GreaterOrEqual => "ge",
                    },
                    depth,
                )?;
                self.expression(left, depth + 1)?;
                self.push(",")?;
                self.expression(right, depth + 1)?;
            }
            Criteria::TextMatch {
                text,
                pattern,
                operator,
                comparison,
            } => {
                self.start(
                    match (operator, comparison) {
                        (TextMatchOperator::Contains, TextComparison::Ordinal) => "contains",
                        (TextMatchOperator::StartsWith, TextComparison::Ordinal) => "starts_with",
                        (TextMatchOperator::EndsWith, TextComparison::Ordinal) => "ends_with",
                        (TextMatchOperator::Contains, _) => "icontains",
                        (TextMatchOperator::StartsWith, _) => "istarts_with",
                        (TextMatchOperator::EndsWith, _) => "iends_with",
                    },
                    depth,
                )?;
                self.expression(text, depth + 1)?;
                self.push(",")?;
                self.expression(pattern, depth + 1)?;
            }
        }
        self.push(")")
    }
}
