//! Exact decimal editing and bounded calculator arithmetic.
use super::ControlError;
use crate::UiCulture;
pub use rust_desktop_ui_data::DecimalValue;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NumericEditSettings {
    pub minimum: DecimalValue,
    pub maximum: DecimalValue,
    pub step: DecimalValue,
    pub precision: u8,
    pub allow_empty: bool,
    pub culture: UiCulture,
    pub grouping: bool,
    /// Omit insignificant fractional zeros without reducing accepted precision.
    pub trim_trailing_zeros: bool,
    pub calculator: bool,
}
impl Default for NumericEditSettings {
    fn default() -> Self {
        Self {
            minimum: DecimalValue::new(-1_000_000_000_000_000, 0).unwrap(),
            maximum: DecimalValue::new(1_000_000_000_000_000, 0).unwrap(),
            step: DecimalValue::new(1, 0).unwrap(),
            precision: 2,
            allow_empty: true,
            culture: UiCulture::Russian,
            grouping: false,
            trim_trailing_zeros: false,
            calculator: true,
        }
    }
}
impl NumericEditSettings {
    pub fn validate(&self) -> Result<(), ControlError> {
        if self.precision > 28
            || self.minimum > self.maximum
            || self.step.coefficient() <= 0
            || [self.minimum, self.maximum, self.step]
                .iter()
                .any(|v| v.scale() > self.precision)
        {
            return Err(ControlError::Invalid("NumericEdit requires minimum <= maximum, positive step and decimal precision 0..28 covering bounds and step".into()));
        }
        Ok(())
    }
    pub fn validate_value(&self, value: Option<DecimalValue>) -> Result<(), String> {
        match value {
            None if !self.allow_empty => Err("Введите число.".into()),
            Some(v) if v < self.minimum || v > self.maximum => Err(format!(
                "Значение должно быть от {} до {}.",
                self.minimum, self.maximum
            )),
            Some(v) if v.scale() > self.precision => Err(format!(
                "Допускается не более {} знаков после разделителя.",
                self.precision
            )),
            _ => Ok(()),
        }
    }
    /// Parse display text without converting through a binary floating point value.
    pub fn parse(&self, text: &str) -> Result<Option<DecimalValue>, String> {
        let value = parse_display(text, self.culture)?;
        self.validate_value(value)?;
        Ok(value)
    }
    /// Canonical storage strings use a dot and no grouping; null stays null.
    pub fn parse_storage(&self, text: Option<&str>) -> Result<Option<DecimalValue>, String> {
        let value = text
            .map(|s| -> Result<DecimalValue, String> {
                let v: DecimalValue = s
                    .parse()
                    .map_err(|e: rust_desktop_ui_data::DataError| e.to_string())?;
                if v.to_string() != s {
                    return Err("Expected canonical decimal storage".into());
                }
                Ok(v)
            })
            .transpose()?;
        self.validate_value(value)?;
        Ok(value)
    }
    pub fn format(&self, value: Option<DecimalValue>) -> Result<String, ControlError> {
        self.validate_value(value).map_err(ControlError::Invalid)?;
        Ok(value.map_or(String::new(), |v| {
            format_decimal(
                v,
                self.culture,
                self.grouping,
                (!self.trim_trailing_zeros).then_some(self.precision),
            )
        }))
    }
    /// Empty starts at zero clamped to the range. A step saturates at either bound.
    pub fn stepped(
        &self,
        value: Option<DecimalValue>,
        increase: bool,
    ) -> Result<DecimalValue, String> {
        self.validate_value(value)?;
        let zero = DecimalValue::new(0, 0).unwrap();
        let Some(value) = value else {
            return Ok(zero.clamp(self.minimum, self.maximum));
        };
        if (increase && value == self.maximum) || (!increase && value == self.minimum) {
            return Ok(value);
        }
        // Compare the unbounded result before conversion, so saturation also works at i128 extrema.
        let result = arithmetic_parts(
            value,
            self.step,
            if increase {
                NumericOperation::Add
            } else {
                NumericOperation::Subtract
            },
            self.precision,
        )?;
        let min = Parts::from(self.minimum);
        let max = Parts::from(self.maximum);
        if result.compare(&min).is_lt() {
            return Ok(self.minimum);
        }
        if result.compare(&max).is_gt() {
            return Ok(self.maximum);
        }
        result.decimal()
    }
}

fn separators(culture: UiCulture) -> (char, char) {
    match culture {
        UiCulture::Russian => (',', '\u{a0}'),
        UiCulture::English => ('.', ','),
        UiCulture::Arabic => ('\u{66b}', '\u{66c}'),
    }
}
fn parse_display(text: &str, culture: UiCulture) -> Result<Option<DecimalValue>, String> {
    if text.len() > 512 {
        return Err("Numeric text exceeds 512 bytes".into());
    }
    let text = text.trim();
    if text.is_empty() {
        return Ok(None);
    }
    let (decimal, group) = separators(culture);
    let (sign, body) = if let Some(s) = text.strip_prefix('-') {
        ("-", s)
    } else {
        ("", text.strip_prefix('+').unwrap_or(text))
    };
    let mut pieces = body.split(decimal);
    let integer = pieces.next().unwrap();
    let fraction = pieces.next();
    if pieces.next().is_some()
        || integer.is_empty()
        || fraction.is_some_and(|s| s.is_empty() || !s.bytes().all(|c| c.is_ascii_digit()))
    {
        return Err("Введите число в выбранном формате.".into());
    }
    let groups: Vec<_> = integer.split(group).collect();
    if groups
        .iter()
        .any(|g| g.is_empty() || !g.bytes().all(|c| c.is_ascii_digit()))
        || (groups.len() > 1 && (groups[0].len() > 3 || groups[1..].iter().any(|g| g.len() != 3)))
    {
        return Err("Неверная группировка цифр.".into());
    }
    let canonical = format!(
        "{sign}{}{}",
        groups.concat(),
        fraction.map_or(String::new(), |f| format!(".{f}"))
    );
    canonical
        .parse()
        .map(Some)
        .map_err(|e: rust_desktop_ui_data::DataError| e.to_string())
}
fn format_decimal(
    value: DecimalValue,
    culture: UiCulture,
    grouping: bool,
    precision: Option<u8>,
) -> String {
    let text = value.to_string();
    let (negative, body) = text
        .strip_prefix('-')
        .map_or((false, text.as_str()), |s| (true, s));
    let (integer, fraction) = body.split_once('.').unwrap_or((body, ""));
    let (decimal, group) = separators(culture);
    let mut result = String::new();
    if negative {
        result.push('-');
    }
    for (i, ch) in integer.chars().enumerate() {
        if grouping && i != 0 && (integer.len() - i).is_multiple_of(3) {
            result.push(group);
        }
        result.push(ch);
    }
    let digits = precision
        .map_or(fraction.len(), usize::from)
        .max(fraction.len());
    if digits > 0 {
        result.push(decimal);
        result.push_str(fraction);
        result.extend(std::iter::repeat_n('0', digits - fraction.len()));
    }
    result
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NumericOperation {
    Add,
    Subtract,
    Multiply,
    Divide,
}
/// Midpoints round away from zero. Applied only to calculator results, never parsed edits.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NumericRounding {
    #[default]
    HalfAwayFromZero,
}
pub fn calculate_decimal(
    left: DecimalValue,
    right: DecimalValue,
    operation: NumericOperation,
    precision: u8,
    _rounding: NumericRounding,
) -> Result<DecimalValue, String> {
    if precision > 28 {
        return Err("Precision must be 0..28".into());
    }
    arithmetic_parts(left, right, operation, precision)?.decimal()
}

// Little-endian decimal digits; operands cap at 39 digits, intermediates at 96.
// This avoids spurious i128 rescaling overflow, including exact cancellation.
#[derive(Clone, Debug)]
struct Parts {
    negative: bool,
    digits: Vec<u8>,
    scale: usize,
}
impl From<DecimalValue> for Parts {
    fn from(value: DecimalValue) -> Self {
        Self {
            negative: value.coefficient() < 0,
            digits: value
                .coefficient()
                .unsigned_abs()
                .to_string()
                .bytes()
                .rev()
                .map(|c| c - b'0')
                .collect(),
            scale: value.scale() as usize,
        }
    }
}
fn trim(v: &mut Vec<u8>) {
    while v.len() > 1 && v.last() == Some(&0) {
        v.pop();
    }
}
fn cmp(a: &[u8], b: &[u8]) -> std::cmp::Ordering {
    a.len()
        .cmp(&b.len())
        .then_with(|| a.iter().rev().cmp(b.iter().rev()))
}
fn shift(v: &mut Vec<u8>, count: usize) {
    if v != &[0] {
        v.splice(0..0, std::iter::repeat_n(0, count));
    }
}
fn add(a: &[u8], b: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut carry = 0;
    for i in 0..a.len().max(b.len()) {
        let n = a.get(i).copied().unwrap_or(0) + b.get(i).copied().unwrap_or(0) + carry;
        out.push(n % 10);
        carry = n / 10;
    }
    if carry != 0 {
        out.push(carry);
    }
    out
}
fn sub(a: &[u8], b: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut borrow = 0i16;
    for (i, d) in a.iter().enumerate() {
        let n = *d as i16 - b.get(i).copied().unwrap_or(0) as i16 - borrow;
        borrow = i16::from(n < 0);
        out.push(n.rem_euclid(10) as u8);
    }
    trim(&mut out);
    out
}
fn div(a: &[u8], b: &[u8]) -> (Vec<u8>, Vec<u8>) {
    let mut remainder = vec![0];
    let mut quotient = Vec::new();
    for d in a.iter().rev() {
        remainder.insert(0, *d);
        trim(&mut remainder);
        let mut n = 0;
        while !cmp(&remainder, b).is_lt() {
            remainder = sub(&remainder, b);
            n += 1;
        }
        quotient.push(n);
    }
    quotient.reverse();
    trim(&mut quotient);
    (quotient, remainder)
}
impl Parts {
    fn align(&mut self, scale: usize) {
        shift(&mut self.digits, scale - self.scale);
        self.scale = scale;
    }
    fn compare(&self, other: &Self) -> std::cmp::Ordering {
        let mut a = self.clone();
        let mut b = other.clone();
        let scale = a.scale.max(b.scale);
        a.align(scale);
        b.align(scale);
        if a.digits == [0] {
            a.negative = false;
        }
        if b.digits == [0] {
            b.negative = false;
        }
        if a.negative != b.negative {
            return b.negative.cmp(&a.negative);
        }
        let order = cmp(&a.digits, &b.digits);
        if a.negative { order.reverse() } else { order }
    }
    fn round(&mut self, precision: usize) {
        if self.scale <= precision {
            return;
        }
        let remove = self.scale - precision;
        let up = self.digits.get(remove - 1).is_some_and(|d| *d >= 5);
        self.digits = if remove >= self.digits.len() {
            vec![0]
        } else {
            self.digits[remove..].to_vec()
        };
        if up {
            self.digits = add(&self.digits, &[1]);
        }
        self.scale = precision;
    }
    fn decimal(mut self) -> Result<DecimalValue, String> {
        while self.scale > 0 && self.digits.first() == Some(&0) {
            self.digits.remove(0);
            self.scale -= 1;
        }
        if self.digits.is_empty() {
            self.digits.push(0);
        }
        let mut text: String = self
            .digits
            .iter()
            .rev()
            .map(|d| char::from(b'0' + d))
            .collect();
        while text.len() <= self.scale {
            text.insert(0, '0');
        }
        if self.scale > 0 {
            text.insert(text.len() - self.scale, '.');
        }
        if self.negative {
            text.insert(0, '-');
        }
        text.parse().map_err(|_| "Decimal overflow".into())
    }
}
fn arithmetic_parts(
    left: DecimalValue,
    right: DecimalValue,
    operation: NumericOperation,
    precision: u8,
) -> Result<Parts, String> {
    let mut a = Parts::from(left);
    let mut b = Parts::from(right);
    let mut out = match operation {
        NumericOperation::Add | NumericOperation::Subtract => {
            if operation == NumericOperation::Subtract {
                b.negative = !b.negative;
            }
            let scale = a.scale.max(b.scale);
            a.align(scale);
            b.align(scale);
            if a.negative == b.negative {
                Parts {
                    negative: a.negative,
                    digits: add(&a.digits, &b.digits),
                    scale,
                }
            } else if cmp(&a.digits, &b.digits).is_lt() {
                Parts {
                    negative: b.negative,
                    digits: sub(&b.digits, &a.digits),
                    scale,
                }
            } else {
                Parts {
                    negative: a.negative,
                    digits: sub(&a.digits, &b.digits),
                    scale,
                }
            }
        }
        NumericOperation::Multiply => {
            let mut digits = vec![0];
            for (i, d) in b.digits.iter().enumerate() {
                let mut row = vec![0];
                for _ in 0..*d {
                    row = add(&row, &a.digits);
                }
                shift(&mut row, i);
                digits = add(&digits, &row);
            }
            Parts {
                negative: a.negative != b.negative,
                digits,
                scale: a.scale + b.scale,
            }
        }
        NumericOperation::Divide => {
            if b.digits == [0] {
                return Err("Деление на ноль.".into());
            }
            let exponent = i32::from(precision) + b.scale as i32 - a.scale as i32;
            if exponent >= 0 {
                shift(&mut a.digits, exponent as usize);
            } else {
                shift(&mut b.digits, (-exponent) as usize);
            }
            let (mut digits, remainder) = div(&a.digits, &b.digits);
            if !cmp(&add(&remainder, &remainder), &b.digits).is_lt() {
                digits = add(&digits, &[1]);
            }
            Parts {
                negative: a.negative != b.negative,
                digits,
                scale: precision as usize,
            }
        }
    };
    out.round(precision as usize);
    Ok(out)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CalculatorKey {
    Digit(u8),
    DecimalSeparator,
    Backspace,
    Clear,
    Negate,
    Operation(NumericOperation),
    Equals,
}
/// Immediate, left-to-right calculator. Errors leave every field unchanged.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NumericCalculator {
    entry: String,
    accumulator: Option<DecimalValue>,
    operation: Option<NumericOperation>,
    replace: bool,
    precision: u8,
}
impl NumericCalculator {
    pub fn new(value: Option<DecimalValue>, precision: u8) -> Result<Self, String> {
        if precision > 28 {
            return Err("Precision must be 0..28".into());
        }
        Ok(Self {
            entry: value.map_or("0".into(), |v| v.to_string()),
            accumulator: None,
            operation: None,
            replace: true,
            precision,
        })
    }
    pub fn value(&self) -> Result<DecimalValue, String> {
        self.entry
            .trim_end_matches('.')
            .parse()
            .map_err(|_| "Decimal overflow".into())
    }
    pub fn display(&self, culture: UiCulture) -> String {
        self.entry.replace('.', &separators(culture).0.to_string())
    }
    pub fn press(&mut self, key: CalculatorKey) -> Result<(), String> {
        let mut next = self.clone();
        next.press_inner(key)?;
        *self = next;
        Ok(())
    }
    fn press_inner(&mut self, key: CalculatorKey) -> Result<(), String> {
        match key {
            CalculatorKey::Clear => {
                self.entry = "0".into();
                self.accumulator = None;
                self.operation = None;
                self.replace = true;
            }
            CalculatorKey::Digit(d) => {
                if d > 9 {
                    return Err("Calculator digit must be 0..9".into());
                }
                if self.replace || self.entry == "0" {
                    self.entry.clear();
                    self.replace = false;
                }
                if self.entry.len() >= 80 {
                    return Err("Calculator entry exceeds 80 bytes".into());
                }
                self.entry.push(char::from(b'0' + d));
                self.value()?;
            }
            CalculatorKey::DecimalSeparator => {
                if self.replace {
                    self.entry = "0".into();
                    self.replace = false;
                }
                if !self.entry.contains('.') {
                    self.entry.push('.');
                }
            }
            CalculatorKey::Backspace => {
                if self.replace {
                    self.entry = "0".into();
                    self.replace = false;
                } else {
                    self.entry.pop();
                    if self.entry.is_empty() || self.entry == "-" {
                        self.entry = "0".into();
                    }
                }
            }
            CalculatorKey::Negate => {
                if self.entry.starts_with('-') {
                    self.entry.remove(0);
                } else if self.value()?.coefficient() != 0 {
                    self.entry.insert(0, '-');
                }
                self.value()?;
            }
            CalculatorKey::Operation(op) => {
                if !self.replace {
                    self.finish()?;
                }
                self.accumulator = Some(self.value()?);
                self.operation = Some(op);
                self.replace = true;
            }
            CalculatorKey::Equals => {
                self.finish()?;
                self.replace = true;
            }
        }
        Ok(())
    }
    fn finish(&mut self) -> Result<(), String> {
        if let (Some(left), Some(op)) = (self.accumulator, self.operation) {
            self.entry = calculate_decimal(
                left,
                self.value()?,
                op,
                self.precision,
                NumericRounding::HalfAwayFromZero,
            )?
            .to_string();
        }
        self.accumulator = None;
        self.operation = None;
        Ok(())
    }
}
