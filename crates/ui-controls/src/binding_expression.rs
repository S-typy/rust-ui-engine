//! Bounded expressions for multi-source presentation and declarative model rules.
use crate::{BindingError, DataValue};

#[derive(Clone, Debug, PartialEq)]
enum Expr {
    Literal(DataValue),
    Input(usize),
    Unary(char, Box<Expr>),
    Binary(String, Box<Expr>, Box<Expr>),
    Call(String, Vec<Expr>),
}
#[derive(Clone, Debug, PartialEq)]
pub struct BindingExpression {
    source: String,
    expression: Expr,
    input_count: usize,
}
impl BindingExpression {
    /// Inputs are `$0` through `$31`. Operators: arithmetic, comparison, boolean,
    /// null coalescing; functions: if, concat and len. There are no calls into Rust.
    pub fn parse(source: &str) -> Result<Self, BindingError> {
        if source.is_empty() || source.len() > 4096 {
            return Err(error("Expression requires 1..4096 UTF-8 bytes"));
        }
        let mut parser = Parser {
            source,
            offset: 0,
            nodes: 0,
            input_count: 0,
        };
        let expression = parser.parse(0, 0)?;
        parser.space();
        if parser.offset != source.len() {
            return Err(parser.error("Unexpected trailing token"));
        }
        Ok(Self {
            source: source.into(),
            expression,
            input_count: parser.input_count,
        })
    }
    pub fn source(&self) -> &str {
        &self.source
    }
    pub fn input_count(&self) -> usize {
        self.input_count
    }
    pub fn evaluate(&self, inputs: &[DataValue]) -> Result<DataValue, BindingError> {
        if inputs.len() < self.input_count || inputs.len() > 32 {
            return Err(error(
                "Expression input count must cover referenced slots and be <=32",
            ));
        }
        for value in inputs {
            value.validate()?;
        }
        let result = evaluate(&self.expression, inputs)?;
        result.validate()?;
        Ok(result)
    }
}

struct Parser<'a> {
    source: &'a str,
    offset: usize,
    nodes: usize,
    input_count: usize,
}
impl Parser<'_> {
    fn space(&mut self) {
        while self
            .source
            .as_bytes()
            .get(self.offset)
            .is_some_and(u8::is_ascii_whitespace)
        {
            self.offset += 1;
        }
    }
    fn error(&self, text: &str) -> BindingError {
        error(&format!("{text} at byte {}", self.offset))
    }
    fn consume(&mut self, text: &str) -> bool {
        self.space();
        if self.source[self.offset..].starts_with(text) {
            self.offset += text.len();
            true
        } else {
            false
        }
    }
    fn parse(&mut self, minimum: u8, depth: usize) -> Result<Expr, BindingError> {
        if depth > 32 || self.nodes >= 256 {
            return Err(self.error("Expression nesting/node limit"));
        }
        self.nodes += 1;
        self.space();
        let mut left = if self.consume("!") {
            Expr::Unary('!', Box::new(self.parse(8, depth + 1)?))
        } else if self.consume("-") {
            Expr::Unary('-', Box::new(self.parse(8, depth + 1)?))
        } else if self.consume("(") {
            let value = self.parse(0, depth + 1)?;
            if !self.consume(")") {
                return Err(self.error("Expected )"));
            }
            value
        } else if self.consume("$") {
            let start = self.offset;
            while self
                .source
                .as_bytes()
                .get(self.offset)
                .is_some_and(u8::is_ascii_digit)
            {
                self.offset += 1;
            }
            let index = self.source[start..self.offset]
                .parse::<usize>()
                .ok()
                .filter(|i| *i < 32)
                .ok_or_else(|| self.error("Input index must be 0..31"))?;
            self.input_count = self.input_count.max(index + 1);
            Expr::Input(index)
        } else if self
            .source
            .as_bytes()
            .get(self.offset)
            .is_some_and(|b| *b == b'\'' || *b == b'"')
        {
            let quote = self.source.as_bytes()[self.offset];
            self.offset += 1;
            let start = self.offset;
            while self
                .source
                .as_bytes()
                .get(self.offset)
                .is_some_and(|b| *b != quote)
            {
                if self.source.as_bytes()[self.offset] == b'\\' {
                    return Err(self.error("String escapes are not supported"));
                }
                self.offset += 1;
            }
            if self.offset == self.source.len() {
                return Err(self.error("Unterminated string"));
            }
            let value = self.source[start..self.offset].to_owned();
            self.offset += 1;
            Expr::Literal(DataValue::String(value))
        } else if self
            .source
            .as_bytes()
            .get(self.offset)
            .is_some_and(u8::is_ascii_digit)
        {
            let start = self.offset;
            while self
                .source
                .as_bytes()
                .get(self.offset)
                .is_some_and(|b| b.is_ascii_digit() || *b == b'.')
            {
                self.offset += 1;
            }
            let value = self.source[start..self.offset]
                .parse::<f64>()
                .ok()
                .filter(|v| v.is_finite())
                .ok_or_else(|| self.error("Invalid finite numeric literal"))?;
            Expr::Literal(DataValue::Number(value))
        } else {
            let start = self.offset;
            while self
                .source
                .as_bytes()
                .get(self.offset)
                .is_some_and(u8::is_ascii_alphabetic)
            {
                self.offset += 1;
            }
            let name = &self.source[start..self.offset];
            match name {
                "true" => Expr::Literal(DataValue::Bool(true)),
                "false" => Expr::Literal(DataValue::Bool(false)),
                "null" => Expr::Literal(DataValue::Null),
                "if" | "concat" | "len" => {
                    let name = name.to_owned();
                    if !self.consume("(") {
                        return Err(self.error("Expected function arguments"));
                    }
                    let mut args = Vec::new();
                    loop {
                        if args.len() == 32 {
                            return Err(self.error("Too many function arguments"));
                        }
                        args.push(self.parse(0, depth + 1)?);
                        if self.consume(")") {
                            break;
                        }
                        if !self.consume(",") {
                            return Err(self.error("Expected , or )"));
                        }
                    }
                    if (name == "if" && args.len() != 3) || (name == "len" && args.len() != 1) {
                        return Err(self.error("Incorrect function argument count"));
                    }
                    Expr::Call(name, args)
                }
                _ => return Err(self.error("Expected literal, input or supported function")),
            }
        };
        loop {
            self.space();
            let Some((operator, precedence)) = [
                ("??", 1),
                ("||", 2),
                ("&&", 3),
                ("==", 4),
                ("!=", 4),
                ("<=", 5),
                (">=", 5),
                ("<", 5),
                (">", 5),
                ("+", 6),
                ("-", 6),
                ("*", 7),
                ("/", 7),
                ("%", 7),
            ]
            .into_iter()
            .find(|(op, _)| self.source[self.offset..].starts_with(op)) else {
                break;
            };
            if precedence < minimum {
                break;
            }
            self.offset += operator.len();
            self.nodes += 1;
            if self.nodes > 256 {
                return Err(self.error("Expression node limit"));
            }
            let right = self.parse(precedence + 1, depth + 1)?;
            left = Expr::Binary(operator.into(), Box::new(left), Box::new(right));
        }
        Ok(left)
    }
}
fn evaluate(expr: &Expr, inputs: &[DataValue]) -> Result<DataValue, BindingError> {
    use DataValue::*;
    let number = |value: &DataValue| {
        value
            .as_number()
            .ok_or_else(|| error("Expression requires a number"))
    };
    let boolean = |value: &DataValue| {
        value
            .as_bool()
            .ok_or_else(|| error("Expression requires a boolean"))
    };
    let finite = |value: f64| {
        if value.is_finite() {
            Ok(Number(value))
        } else {
            Err(error("Non-finite expression result"))
        }
    };
    match expr {
        Expr::Literal(value) => Ok(value.clone()),
        Expr::Input(index) => inputs
            .get(*index)
            .cloned()
            .ok_or_else(|| error("Missing expression input")),
        Expr::Unary(op, value) => {
            let value = evaluate(value, inputs)?;
            if *op == '!' {
                Ok(Bool(!boolean(&value)?))
            } else {
                finite(-number(&value)?)
            }
        }
        Expr::Binary(op, left, right) => {
            let left = evaluate(left, inputs)?;
            if op == "??" && left != Null {
                return Ok(left);
            }
            if op == "&&" && !boolean(&left)? {
                return Ok(Bool(false));
            }
            if op == "||" && boolean(&left)? {
                return Ok(Bool(true));
            }
            let right = evaluate(right, inputs)?;
            match op.as_str() {
                "??" => Ok(right),
                "&&" | "||" => Ok(Bool(boolean(&right)?)),
                "==" => Ok(Bool(left == right)),
                "!=" => Ok(Bool(left != right)),
                "+" => finite(number(&left)? + number(&right)?),
                "-" => finite(number(&left)? - number(&right)?),
                "*" => finite(number(&left)? * number(&right)?),
                "/" => finite(number(&left)? / number(&right)?),
                "%" => finite(number(&left)? % number(&right)?),
                "<" => Ok(Bool(number(&left)? < number(&right)?)),
                "<=" => Ok(Bool(number(&left)? <= number(&right)?)),
                ">" => Ok(Bool(number(&left)? > number(&right)?)),
                ">=" => Ok(Bool(number(&left)? >= number(&right)?)),
                _ => unreachable!(),
            }
        }
        Expr::Call(name, args) if name == "if" => {
            let condition = evaluate(&args[0], inputs)?;
            evaluate(&args[if boolean(&condition)? { 1 } else { 2 }], inputs)
        }
        Expr::Call(name, args) if name == "len" => {
            let value = evaluate(&args[0], inputs)?;
            let length = match value {
                String(text) => rust_desktop_ui_text::TextDocument::new(text)
                    .grapheme_boundaries()
                    .len()
                    .saturating_sub(1),
                List(values) => values.len(),
                _ => return Err(error("len requires a string or list")),
            };
            Ok(Number(length as f64))
        }
        Expr::Call(_, args) => {
            let mut text = std::string::String::new();
            for arg in args {
                let value = evaluate(arg, inputs)?;
                let part = match value {
                    String(text) => text,
                    Number(n) => n.to_string(),
                    Bool(b) => b.to_string(),
                    Null => std::string::String::new(),
                    _ => return Err(error("concat requires scalar values")),
                };
                if text.len() + part.len() > 65_536 {
                    return Err(error("Expression string exceeds 65536 bytes"));
                }
                text.push_str(&part);
            }
            Ok(String(text))
        }
    }
}
fn error(message: &str) -> BindingError {
    BindingError::InvalidBinding(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn expressions_are_typed_and_short_circuit() {
        assert_eq!(
            BindingExpression::parse("$0 * 2 + $1")
                .unwrap()
                .evaluate(&[3.0.into(), 4.0.into()])
                .unwrap(),
            10.0.into()
        );
        assert_eq!(
            BindingExpression::parse("if($0 > 1, concat('Count: ', $0), 'Empty')")
                .unwrap()
                .evaluate(&[2.0.into()])
                .unwrap(),
            "Count: 2".into()
        );
        assert_eq!(
            BindingExpression::parse("false && 1 / 0 > 1")
                .unwrap()
                .evaluate(&[])
                .unwrap(),
            false.into()
        );
        assert_eq!(
            BindingExpression::parse("null ?? 'fallback'")
                .unwrap()
                .evaluate(&[])
                .unwrap(),
            "fallback".into()
        );
        assert_eq!(
            BindingExpression::parse("len('क्षि')")
                .unwrap()
                .evaluate(&[])
                .unwrap(),
            1.0.into()
        );
        assert!(
            BindingExpression::parse("1 / 0")
                .unwrap()
                .evaluate(&[])
                .is_err()
        );
        assert!(
            BindingExpression::parse("$0 + 1")
                .unwrap()
                .evaluate(&[true.into()])
                .is_err()
        );
    }
    #[test]
    fn syntax_has_bounded_rejected_surface() {
        for text in [
            "process()",
            "$32",
            "1;2",
            "$(shell)",
            "if(true,1)",
            "1e999",
            "a.b",
        ] {
            assert!(BindingExpression::parse(text).is_err(), "{text}");
        }
        assert!(
            BindingExpression::parse(&format!("{}1{}", "(".repeat(40), ")".repeat(40))).is_err()
        );
        assert!(BindingExpression::parse(&"1+".repeat(2049)).is_err());
    }
}
