use std::{cmp::Ordering, fmt, str::FromStr};

use crate::DataError;

fn invalid(kind: &str) -> DataError {
    DataError::Validation(format!("invalid {kind} value"))
}

/// Exact signed decimal: an i128 coefficient and at most 28 fractional digits.
/// Trailing fractional zeroes are normalized; arithmetic never uses floats.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct DecimalValue {
    coefficient: i128,
    scale: u8,
}

impl DecimalValue {
    pub const MAX_SCALE: u8 = 28;

    pub fn new(mut coefficient: i128, mut scale: u8) -> Result<Self, DataError> {
        if scale > Self::MAX_SCALE {
            return Err(invalid("decimal scale"));
        }
        while scale > 0 && coefficient % 10 == 0 {
            coefficient /= 10;
            scale -= 1;
        }
        Ok(Self { coefficient, scale })
    }
    pub fn coefficient(self) -> i128 {
        self.coefficient
    }
    pub fn scale(self) -> u8 {
        self.scale
    }
}

impl FromStr for DecimalValue {
    type Err = DataError;
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let (negative, digits) = match text.as_bytes().first() {
            Some(b'-') => (true, &text[1..]),
            Some(b'+') => (false, &text[1..]),
            _ => (false, text),
        };
        let (whole, fraction) = match digits.split_once('.') {
            Some((whole, fraction)) if !fraction.is_empty() => (whole, fraction),
            Some(_) => return Err(invalid("decimal")),
            None => (digits, ""),
        };
        if whole.is_empty()
            || !whole
                .bytes()
                .chain(fraction.bytes())
                .all(|b| b.is_ascii_digit())
            || fraction.len() > usize::from(Self::MAX_SCALE)
        {
            return Err(invalid("decimal"));
        }
        // Normalize before accumulation so an exactly representable integer
        // such as i128::MAX followed by ".0" is not spuriously rejected.
        let fraction = fraction.trim_end_matches('0');
        let mut magnitude = 0u128;
        for digit in whole.bytes().chain(fraction.bytes()) {
            magnitude = magnitude
                .checked_mul(10)
                .and_then(|n| n.checked_add(u128::from(digit - b'0')))
                .ok_or_else(|| invalid("decimal coefficient"))?;
        }
        let limit = i128::MAX as u128 + u128::from(negative);
        if magnitude > limit {
            return Err(invalid("decimal coefficient"));
        }
        let coefficient = if negative && magnitude == limit {
            i128::MIN
        } else if negative {
            -(magnitude as i128)
        } else {
            magnitude as i128
        };
        Self::new(coefficient, fraction.len() as u8)
    }
}

impl fmt::Display for DecimalValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.coefficient < 0 {
            f.write_str("-")?;
        }
        let digits = self.coefficient.unsigned_abs().to_string();
        let scale = usize::from(self.scale);
        if scale == 0 {
            f.write_str(&digits)
        } else if digits.len() > scale {
            let point = digits.len() - scale;
            write!(f, "{}.{}", &digits[..point], &digits[point..])
        } else {
            write!(f, "0.{}{}", "0".repeat(scale - digits.len()), digits)
        }
    }
}

impl Ord for DecimalValue {
    fn cmp(&self, other: &Self) -> Ordering {
        match self.coefficient.signum().cmp(&other.coefficient.signum()) {
            Ordering::Equal => {}
            order => return order,
        }
        let left = self.coefficient.unsigned_abs().to_string();
        let right = other.coefficient.unsigned_abs().to_string();
        let left_exponent = left.len() as i32 - i32::from(self.scale);
        let right_exponent = right.len() as i32 - i32::from(other.scale);
        let magnitude = left_exponent.cmp(&right_exponent).then_with(|| {
            // Equal exponents: compare significands with implicit trailing zeroes.
            // No rescaling multiplication can overflow, even at i128::MIN.
            left.bytes()
                .chain(std::iter::repeat(b'0'))
                .zip(right.bytes().chain(std::iter::repeat(b'0')))
                .take(left.len().max(right.len()))
                .map(|(a, b)| a.cmp(&b))
                .find(|order| *order != Ordering::Equal)
                .unwrap_or(Ordering::Equal)
        });
        if self.coefficient < 0 {
            magnitude.reverse()
        } else {
            magnitude
        }
    }
}
impl PartialOrd for DecimalValue {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Proleptic Gregorian calendar date, years 0001 through 9999, without a zone.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DateValue {
    year: u16,
    month: u8,
    day: u8,
}

impl DateValue {
    pub fn new(year: u16, month: u8, day: u8) -> Result<Self, DataError> {
        let leap =
            year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
        let days = match month {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            4 | 6 | 9 | 11 => 30,
            2 if leap => 29,
            2 => 28,
            _ => 0,
        };
        if year == 0 || year > 9999 || day == 0 || day > days {
            return Err(invalid("date"));
        }
        Ok(Self { year, month, day })
    }
    pub fn year(self) -> u16 {
        self.year
    }
    pub fn month(self) -> u8 {
        self.month
    }
    pub fn day(self) -> u8 {
        self.day
    }

    /// Russian editor representation; the storage representation is ISO 8601.
    pub fn display(self) -> String {
        format!("{:02}.{:02}.{:04}", self.day, self.month, self.year)
    }
    pub fn parse_display(text: &str) -> Result<Self, DataError> {
        if !date_shape(text, b'.', 2, 5) {
            return Err(invalid("date"));
        }
        Self::new(
            number(&text[6..10])?,
            number(&text[3..5])?,
            number(&text[0..2])?,
        )
    }
}

fn date_shape(text: &str, separator: u8, first: usize, second: usize) -> bool {
    text.len() == 10
        && text.bytes().enumerate().all(|(i, b)| {
            if i == first || i == second {
                b == separator
            } else {
                b.is_ascii_digit()
            }
        })
}
fn number<T: FromStr>(text: &str) -> Result<T, DataError> {
    text.parse().map_err(|_| invalid("date/time"))
}
impl FromStr for DateValue {
    type Err = DataError;
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        if !date_shape(text, b'-', 4, 7) {
            return Err(invalid("date"));
        }
        Self::new(
            number(&text[0..4])?,
            number(&text[5..7])?,
            number(&text[8..10])?,
        )
    }
}
impl fmt::Display for DateValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }
}

/// Time of day with nanosecond precision, no timezone or leap-second coercion.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TimeValue {
    hour: u8,
    minute: u8,
    second: u8,
    nanosecond: u32,
}

impl TimeValue {
    pub fn new(hour: u8, minute: u8, second: u8, nanosecond: u32) -> Result<Self, DataError> {
        if hour > 23 || minute > 59 || second > 59 || nanosecond >= 1_000_000_000 {
            return Err(invalid("time"));
        }
        Ok(Self {
            hour,
            minute,
            second,
            nanosecond,
        })
    }
    pub fn hour(self) -> u8 {
        self.hour
    }
    pub fn minute(self) -> u8 {
        self.minute
    }
    pub fn second(self) -> u8 {
        self.second
    }
    pub fn nanosecond(self) -> u32 {
        self.nanosecond
    }
    /// HH:MM for whole minutes; includes seconds/fraction whenever needed.
    pub fn display(self) -> String {
        if self.second == 0 && self.nanosecond == 0 {
            format!("{:02}:{:02}", self.hour, self.minute)
        } else {
            self.to_string()
        }
    }
}
impl FromStr for TimeValue {
    type Err = DataError;
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let (clock, fraction) = text
            .split_once('.')
            .map_or((text, None), |(a, b)| (a, Some(b)));
        if !matches!(clock.len(), 5 | 8)
            || !clock.bytes().enumerate().all(|(i, b)| {
                if i == 2 || i == 5 {
                    b == b':'
                } else {
                    b.is_ascii_digit()
                }
            })
            || (fraction.is_some() && clock.len() != 8)
        {
            return Err(invalid("time"));
        }
        let nanos = match fraction {
            Some(digits)
                if !digits.is_empty()
                    && digits.len() <= 9
                    && digits.bytes().all(|b| b.is_ascii_digit()) =>
            {
                number::<u32>(digits)? * 10u32.pow(9 - digits.len() as u32)
            }
            Some(_) => return Err(invalid("time fraction")),
            None => 0,
        };
        Self::new(
            number(&clock[..2])?,
            number(&clock[3..5])?,
            if clock.len() == 8 {
                number(&clock[6..8])?
            } else {
                0
            },
            nanos,
        )
    }
}
impl fmt::Display for TimeValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:02}:{:02}:{:02}", self.hour, self.minute, self.second)?;
        if self.nanosecond != 0 {
            write!(
                f,
                ".{}",
                format!("{:09}", self.nanosecond).trim_end_matches('0')
            )?;
        }
        Ok(())
    }
}
