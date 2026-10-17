use super::*;
pub(super) struct Parser<'a> {
    text: &'a str,
    pos: usize,
    nodes: usize,
}
impl<'a> Parser<'a> {
    pub(super) fn new(text: &'a str) -> Result<Self, QueryTextError> {
        if text.len() > MAX_QUERY_TEXT_BYTES {
            return Err(error(0, "Query text exceeds 16 KiB"));
        }
        Ok(Self {
            text,
            pos: 0,
            nodes: 0,
        })
    }
    fn fail(&self, message: impl ToString) -> QueryTextError {
        error(self.pos, message)
    }
    fn ws(&mut self) {
        while self
            .text
            .as_bytes()
            .get(self.pos)
            .is_some_and(u8::is_ascii_whitespace)
        {
            self.pos += 1;
        }
    }
    fn eat(&mut self, ch: u8) -> bool {
        self.ws();
        if self.text.as_bytes().get(self.pos) == Some(&ch) {
            self.pos += 1;
            true
        } else {
            false
        }
    }
    fn expect(&mut self, ch: u8) -> Result<(), QueryTextError> {
        if self.eat(ch) {
            Ok(())
        } else {
            Err(self.fail(format!("Expected '{}'", ch as char)))
        }
    }
    fn enter(&mut self, depth: usize) -> Result<String, QueryTextError> {
        self.nodes += 1;
        if depth > MAX_QUERY_TEXT_DEPTH || self.nodes > MAX_QUERY_TEXT_NODES {
            return Err(self.fail("Query tree exceeds 128 nodes or depth 16"));
        }
        self.ws();
        let start = self.pos;
        while self
            .text
            .as_bytes()
            .get(self.pos)
            .is_some_and(|c| c.is_ascii_lowercase() || *c == b'_')
        {
            self.pos += 1;
        }
        if start == self.pos {
            return Err(self.fail("Expected a function name"));
        }
        let name = self.text[start..self.pos].to_owned();
        self.expect(b'(')?;
        Ok(name)
    }
    pub(super) fn finish(&mut self) -> Result<(), QueryTextError> {
        self.ws();
        if self.pos == self.text.len() {
            Ok(())
        } else {
            Err(self.fail("Unexpected trailing input"))
        }
    }
    fn string(&mut self) -> Result<String, QueryTextError> {
        self.expect(b'"')?;
        let mut out = String::new();
        loop {
            let ch = self.text[self.pos..]
                .chars()
                .next()
                .ok_or_else(|| self.fail("Unterminated string"))?;
            self.pos += ch.len_utf8();
            match ch {
                '"' => return Ok(out),
                '\\' => {
                    let esc = self.text[self.pos..]
                        .chars()
                        .next()
                        .ok_or_else(|| self.fail("Incomplete escape"))?;
                    self.pos += esc.len_utf8();
                    out.push(match esc {
                        '"' => '"',
                        '\\' => '\\',
                        'n' => '\n',
                        'r' => '\r',
                        't' => '\t',
                        'u' => {
                            self.expect(b'{')?;
                            let start = self.pos;
                            while self
                                .text
                                .as_bytes()
                                .get(self.pos)
                                .is_some_and(u8::is_ascii_hexdigit)
                            {
                                self.pos += 1;
                                if self.pos - start > 6 {
                                    return Err(self.fail("Unicode escape exceeds six hex digits"));
                                }
                            }
                            let value = u32::from_str_radix(&self.text[start..self.pos], 16)
                                .ok()
                                .and_then(char::from_u32)
                                .ok_or_else(|| self.fail("Invalid Unicode scalar escape"))?;
                            self.expect(b'}')?;
                            value
                        }
                        _ => return Err(self.fail("Unsupported string escape")),
                    });
                }
                c if c.is_control() => return Err(self.fail("Control characters must be escaped")),
                c => out.push(c),
            }
        }
    }
    pub(super) fn expression(&mut self, depth: usize) -> Result<QueryExpression, QueryTextError> {
        let name = self.enter(depth)?;
        let result = match name.as_str() {
            "null" => QueryExpression::Literal(CellValue::Null),
            "field" => QueryExpression::Field(self.string()?),
            "text" => QueryExpression::Literal(CellValue::Text(self.string()?)),
            "integer" | "real" | "decimal" | "date" | "time" | "bool" | "binary" => {
                let s = self.string()?;
                let invalid = || self.fail(format!("Invalid {name} literal"));
                let v = match name.as_str() {
                    "integer" => CellValue::Integer(s.parse().map_err(|_| invalid())?),
                    "real" => {
                        let value: f64 = s.parse().map_err(|_| invalid())?;
                        if !value.is_finite() {
                            return Err(invalid());
                        }
                        CellValue::Real(value)
                    }
                    "decimal" => CellValue::Decimal(s.parse().map_err(|_| invalid())?),
                    "date" => CellValue::Date(s.parse().map_err(|_| invalid())?),
                    "time" => CellValue::Time(s.parse().map_err(|_| invalid())?),
                    "bool" => CellValue::Bool(match s.as_str() {
                        "true" => true,
                        "false" => false,
                        _ => return Err(invalid()),
                    }),
                    "binary" => {
                        if s.len() % 2 != 0 || !s.bytes().all(|c| c.is_ascii_hexdigit()) {
                            return Err(invalid());
                        }
                        CellValue::Binary(
                            (0..s.len())
                                .step_by(2)
                                .map(|i| {
                                    u8::from_str_radix(&s[i..i + 2], 16).expect("validated hex")
                                })
                                .collect(),
                        )
                    }
                    _ => unreachable!(),
                };
                QueryExpression::Literal(v)
            }
            "lower" | "length" => {
                let v = Box::new(self.expression(depth + 1)?);
                if name == "lower" {
                    QueryExpression::Lowercase(v)
                } else {
                    QueryExpression::TextLength(v)
                }
            }
            "coalesce" | "add" | "sub" | "mul" | "div" => {
                let left = Box::new(self.expression(depth + 1)?);
                self.expect(b',')?;
                let right = Box::new(self.expression(depth + 1)?);
                if name == "coalesce" {
                    QueryExpression::Coalesce(left, right)
                } else {
                    QueryExpression::Arithmetic {
                        left,
                        right,
                        operator: match name.as_str() {
                            "add" => ArithmeticOperator::Add,
                            "sub" => ArithmeticOperator::Subtract,
                            "mul" => ArithmeticOperator::Multiply,
                            _ => ArithmeticOperator::Divide,
                        },
                    }
                }
            }
            _ => return Err(self.fail(format!("Unsupported expression function '{name}'"))),
        };
        self.expect(b')')?;
        Ok(result)
    }
    pub(super) fn criteria(&mut self, depth: usize) -> Result<Criteria, QueryTextError> {
        let name = self.enter(depth)?;
        let result = match name.as_str() {
            "true" => Criteria::Constant(true),
            "false" => Criteria::Constant(false),
            "all" | "any" => {
                let mut nodes = Vec::new();
                if self.eat(b')') {
                    return Ok(if name == "all" {
                        Criteria::All(nodes)
                    } else {
                        Criteria::Any(nodes)
                    });
                }
                loop {
                    nodes.push(self.criteria(depth + 1)?);
                    if !self.eat(b',') {
                        break;
                    }
                }
                if name == "all" {
                    Criteria::All(nodes)
                } else {
                    Criteria::Any(nodes)
                }
            }
            "not" => Criteria::Not(Box::new(self.criteria(depth + 1)?)),
            "is_null" => Criteria::IsNull(self.expression(depth + 1)?),
            "eq" | "ne" | "lt" | "le" | "gt" | "ge" => {
                let left = self.expression(depth + 1)?;
                self.expect(b',')?;
                let right = self.expression(depth + 1)?;
                Criteria::Compare {
                    left,
                    right,
                    operator: match name.as_str() {
                        "eq" => ComparisonOperator::Equal,
                        "ne" => ComparisonOperator::NotEqual,
                        "lt" => ComparisonOperator::Less,
                        "le" => ComparisonOperator::LessOrEqual,
                        "gt" => ComparisonOperator::Greater,
                        _ => ComparisonOperator::GreaterOrEqual,
                    },
                }
            }
            "contains" | "starts_with" | "ends_with" | "icontains" | "istarts_with"
            | "iends_with" => {
                let text = self.expression(depth + 1)?;
                self.expect(b',')?;
                let pattern = self.expression(depth + 1)?;
                Criteria::TextMatch {
                    text,
                    pattern,
                    operator: if name.ends_with("contains") {
                        TextMatchOperator::Contains
                    } else if name.ends_with("starts_with") {
                        TextMatchOperator::StartsWith
                    } else {
                        TextMatchOperator::EndsWith
                    },
                    comparison: if name.starts_with('i') {
                        TextComparison::UnicodeLowercase
                    } else {
                        TextComparison::Ordinal
                    },
                }
            }
            _ => return Err(self.fail(format!("Unsupported criteria function '{name}'"))),
        };
        self.expect(b')')?;
        Ok(result)
    }
}
