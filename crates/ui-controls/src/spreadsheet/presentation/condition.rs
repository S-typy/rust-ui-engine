//! Exact decimal predicates and ordered numeric section selection.
use super::{Number, Token, code};
use std::cmp::Ordering;

#[derive(Clone, Copy)]
enum Operator {
    Less,
    LessEqual,
    Equal,
    NotEqual,
    GreaterEqual,
    Greater,
}
impl Operator {
    fn inverse(self) -> Self {
        match self {
            Self::Less => Self::GreaterEqual,
            Self::LessEqual => Self::Greater,
            Self::Equal => Self::NotEqual,
            Self::NotEqual => Self::Equal,
            Self::GreaterEqual => Self::Less,
            Self::Greater => Self::LessEqual,
        }
    }
}

pub(super) struct Condition {
    operator: Operator,
    value: Number,
}

fn compare(left: &Number, right: &Number) -> Ordering {
    let sign = |n: &Number| {
        if n.digits == "0" {
            0
        } else if n.negative {
            -1
        } else {
            1
        }
    };
    let (a, b) = (sign(left), sign(right));
    if a != b {
        return a.cmp(&b);
    }
    if a == 0 {
        return Ordering::Equal;
    }
    let magnitude = (left.digits.len() as i64 - i64::from(left.scale))
        .cmp(&(right.digits.len() as i64 - i64::from(right.scale)))
        .then_with(|| {
            let length = left.digits.len().max(right.digits.len());
            left.digits
                .bytes()
                .chain(std::iter::repeat(b'0'))
                .take(length)
                .cmp(
                    right
                        .digits
                        .bytes()
                        .chain(std::iter::repeat(b'0'))
                        .take(length),
                )
        });
    if a < 0 {
        magnitude.reverse()
    } else {
        magnitude
    }
}

impl Condition {
    fn parse(text: &str) -> Result<Self, &'static str> {
        let (operator, number) = [
            ("<=", Operator::LessEqual),
            (">=", Operator::GreaterEqual),
            ("<>", Operator::NotEqual),
            ("<", Operator::Less),
            (">", Operator::Greater),
            ("=", Operator::Equal),
        ]
        .into_iter()
        .find_map(|(prefix, op)| text.strip_prefix(prefix).map(|n| (op, n)))
        .ok_or("Invalid numeric condition operator")?;
        let unsigned = number.strip_prefix(['-', '+']).unwrap_or(number);
        let (base, exponent) = unsigned.split_once(['e', 'E']).unwrap_or((unsigned, "0"));
        let exponent = exponent
            .parse::<i32>()
            .map_err(|_| "Invalid condition exponent")?;
        if !(-1024..=1024).contains(&exponent) {
            return Err("Condition exponent exceeds budget");
        }
        let mut dots = 0;
        let mut digits = 0;
        for b in base.bytes() {
            match b {
                b'0'..=b'9' => digits += 1,
                b'.' => dots += 1,
                _ => return Err("Invalid condition number"),
            }
        }
        if digits == 0 || dots > 1 {
            return Err("Invalid condition number");
        }
        Ok(Self {
            operator,
            value: Number::new(number)?,
        })
    }
    fn matches(&self, number: &Number) -> bool {
        let ordering = compare(number, &self.value);
        match self.operator {
            Operator::Less => ordering.is_lt(),
            Operator::LessEqual => !ordering.is_gt(),
            Operator::Equal => ordering.is_eq(),
            Operator::NotEqual => !ordering.is_eq(),
            Operator::GreaterEqual => !ordering.is_lt(),
            Operator::Greater => ordering.is_gt(),
        }
    }
    fn negative_only(&self, complement: bool) -> bool {
        let negative = self.value.negative && self.value.digits != "0";
        let nonpositive = negative || self.value.digits == "0";
        match if complement {
            self.operator.inverse()
        } else {
            self.operator
        } {
            Operator::Less => nonpositive,
            Operator::LessEqual | Operator::Equal => negative,
            _ => false,
        }
    }
}

/// Remove only conditions; color, locale and duration markers remain for their
/// existing parsers. Quoted/escaped brackets never become directives.
pub(super) fn extract(parts: &mut [Vec<Token>]) -> Result<Vec<Option<Condition>>, &'static str> {
    let mut result = Vec::new();
    for (index, part) in parts.iter_mut().enumerate() {
        let mut condition = None;
        let mut cursor = 0;
        while part.get(cursor).and_then(code) == Some('[') {
            let end = (cursor + 1..part.len())
                .find(|i| code(&part[*i]) == Some(']'))
                .ok_or("Unclosed format modifier")?;
            let name = part[cursor + 1..end]
                .iter()
                .map(|t| code(t).ok_or("Invalid format modifier"))
                .collect::<Result<String, _>>()?;
            if name.starts_with(['<', '>', '=']) {
                if index > 1 {
                    return Err("Conditions are limited to the first two numeric sections");
                }
                if condition.is_some() {
                    return Err("Duplicate numeric condition");
                }
                condition = Some(Condition::parse(&name)?);
                part.drain(cursor..=end);
            } else {
                cursor = end + 1;
            }
        }
        result.push(condition);
    }
    Ok(result)
}

pub(super) enum Selection {
    /// Section index and whether to add the original negative sign.
    Section(usize, bool),
    General,
}

pub(super) fn select(
    conditions: &[Option<Condition>],
    number: &Number,
) -> Result<Selection, &'static str> {
    let count = conditions.len().min(3);
    let negative = number.negative && number.digits != "0";
    if !conditions.iter().any(Option::is_some) {
        let index = if number.digits == "0" && count >= 3 {
            2
        } else if negative && count >= 2 {
            1
        } else {
            0
        };
        return Ok(Selection::Section(index, negative && index == 0));
    }
    if let Some(first) = &conditions[0] {
        if first.matches(number) {
            return Ok(Selection::Section(
                0,
                negative && !first.negative_only(false),
            ));
        }
    } else if !negative {
        return Ok(Selection::Section(0, false));
    }
    if count == 1 {
        return Ok(Selection::General);
    }
    if let Some(second) = &conditions[1] {
        if second.matches(number) {
            return Ok(Selection::Section(
                1,
                negative && !second.negative_only(false),
            ));
        }
    } else if count == 2 {
        let negative_only = conditions[0]
            .as_ref()
            .is_some_and(|c| c.negative_only(true));
        return Ok(Selection::Section(1, negative && !negative_only));
    } else if negative {
        return Ok(Selection::Section(1, false));
    }
    if count == 3 {
        return Ok(Selection::Section(2, negative));
    }
    Err("No conditional numeric section matches")
}
