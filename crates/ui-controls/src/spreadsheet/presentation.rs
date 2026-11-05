//! Bounded cell presentation, independent of input/formula storage.
mod elapsed;
mod fraction;
mod temporal;
use super::*;
use crate::UiCulture;
use rust_desktop_ui_data::CellValue;
use temporal::date_time;

#[derive(Clone, Debug, PartialEq)]
pub struct CellDisplay {
    pub text: String,
    pub foreground: Option<u32>,
    /// Unsupported syntax falls back to the raw value with an explicit diagnostic.
    pub diagnostic: Option<String>,
}

pub fn format_cell(
    book: &Workbook,
    sheet: usize,
    address: CellAddress,
    culture: UiCulture,
) -> Result<CellDisplay, ControlError> {
    let value = book.value(sheet, address).map_err(err)?;
    let format = book.sheets()[sheet]
        .cell(address)
        .map_or("", |c| c.style.number_format.as_str());
    Ok(format_value(&value, format, culture))
}

pub fn format_value(value: &WorkbookValue, format: &str, culture: UiCulture) -> CellDisplay {
    let fallback = || CellDisplay {
        text: value.display(),
        foreground: None,
        diagnostic: None,
    };
    if format.is_empty() || format.eq_ignore_ascii_case("General") {
        let mut result = fallback();
        match value {
            WorkbookValue::Scalar(
                CellValue::Integer(_) | CellValue::Real(_) | CellValue::Decimal(_),
            ) => {
                result.text = result.text.replace('.', decimal(culture).1);
            }
            WorkbookValue::Scalar(CellValue::Date(date)) => {
                result.text = match culture {
                    UiCulture::Russian => date.display(),
                    UiCulture::English => {
                        format!("{:02}/{:02}/{:04}", date.month(), date.day(), date.year())
                    }
                    UiCulture::Arabic => {
                        format!("{:02}/{:02}/{:04}", date.day(), date.month(), date.year())
                    }
                }
            }
            WorkbookValue::Scalar(CellValue::Time(time)) => result.text = time.display(),
            _ => {}
        }
        return result;
    }
    match formatted(value, format, culture) {
        Ok((text, foreground)) => CellDisplay {
            text,
            foreground,
            diagnostic: None,
        },
        Err(message) => CellDisplay {
            diagnostic: Some(message.into()),
            ..fallback()
        },
    }
}

#[derive(Clone, Debug)]
enum Token {
    Code(char),
    Literal(char),
}
fn sections(format: &str) -> Result<Vec<Vec<Token>>, &'static str> {
    if format.len() > 256 {
        return Err("Cell format exceeds 256 bytes");
    }
    let mut result = vec![Vec::new()];
    let mut quoted = false;
    let mut input = format.chars();
    while let Some(ch) = input.next() {
        match ch {
            '"' => quoted = !quoted,
            '\\' if !quoted => result.last_mut().unwrap().push(Token::Literal(
                input.next().ok_or("Incomplete format escape")?,
            )),
            ';' if !quoted => {
                if result.len() == 4 {
                    return Err("More than four format sections");
                }
                result.push(Vec::new());
            }
            _ => result.last_mut().unwrap().push(if quoted {
                Token::Literal(ch)
            } else {
                Token::Code(ch)
            }),
        }
    }
    if quoted {
        return Err("Unterminated quoted format literal");
    }
    Ok(result)
}
fn code(t: &Token) -> Option<char> {
    match t {
        Token::Code(c) => Some(*c),
        _ => None,
    }
}
fn color(tokens: &mut Vec<Token>) -> Result<Option<u32>, &'static str> {
    if tokens.first().and_then(code) != Some('[') {
        return Ok(None);
    }
    let end = tokens
        .iter()
        .position(|t| code(t) == Some(']'))
        .ok_or("Unclosed format modifier")?;
    let name = tokens[1..end]
        .iter()
        .map(|t| code(t).ok_or("Invalid format modifier"))
        .collect::<Result<String, _>>()?;
    let rgb = match name.to_ascii_lowercase().as_str() {
        "black" => 0,
        "white" => 0xffffff,
        "red" => 0xff0000,
        "green" => 0x008000,
        "blue" => 0x0000ff,
        "yellow" => 0xffff00,
        "magenta" => 0xff00ff,
        "cyan" => 0x00ffff,
        _ => {
            return Err("Unsupported color or condition modifier");
        }
    };
    tokens.drain(..=end);
    Ok(Some(rgb))
}
fn directives(
    tokens: &mut Vec<Token>,
    mut culture: UiCulture,
) -> Result<(Option<u32>, UiCulture), &'static str> {
    let mut rgb = None;
    let mut localized = false;
    while tokens.first().and_then(code) == Some('[') {
        if elapsed::total_marker(tokens).is_some() {
            break;
        }
        let end = tokens
            .iter()
            .position(|t| code(t) == Some(']'))
            .ok_or("Unclosed format modifier")?;
        let name = tokens[1..end]
            .iter()
            .map(|t| code(t).ok_or("Invalid format modifier"))
            .collect::<Result<String, _>>()?;
        if let Some(tag) = name.strip_prefix("$-") {
            if localized {
                return Err("Duplicate locale modifier");
            }
            culture = match tag.to_ascii_lowercase().as_str() {
                "409" | "0409" | "en-us" => UiCulture::English,
                "419" | "0419" | "ru-ru" => UiCulture::Russian,
                "401" | "0401" | "ar-sa" => UiCulture::Arabic,
                _ => return Err("Unsupported format locale"),
            };
            localized = true;
            tokens.drain(..=end);
        } else {
            if rgb.is_some() {
                return Err("Duplicate color modifier");
            }
            rgb = color(tokens)?;
        }
    }
    Ok((rgb, culture))
}

fn literal(tokens: &[Token], text: Option<&str>) -> Result<String, &'static str> {
    let mut result = String::new();
    for t in tokens {
        match t {
            Token::Literal(c) => result.push(*c),
            Token::Code('@') if text.is_some() => result.push_str(text.unwrap()),
            Token::Code(c) if " +-()$€₽£¥%:/.,".contains(*c) => result.push(*c),
            _ => return Err("Unsupported format literal or placeholder"),
        }
    }
    Ok(result)
}
fn formatted(
    value: &WorkbookValue,
    format: &str,
    culture: UiCulture,
) -> Result<(String, Option<u32>), &'static str> {
    let mut parts = sections(format)?;
    let WorkbookValue::Scalar(value) = value else {
        return Ok((value.display(), None));
    };
    if matches!(value, CellValue::Null | CellValue::Bool(_)) {
        return Ok((value.display(), None));
    }
    if let CellValue::Text(text) = value {
        if parts.len() < 4 && format != "@" {
            return Ok((text.clone(), None));
        }
        let mut part = parts.pop().unwrap();
        let (rgb, _) = directives(&mut part, culture)?;
        return Ok((literal(&part, Some(text))?, rgb));
    }
    if matches!(value, CellValue::Date(_) | CellValue::Time(_)) {
        let mut part = parts.remove(0);
        let (rgb, culture) = directives(&mut part, culture)?;
        return Ok((
            if elapsed::requested(&part) {
                elapsed::render(value, &part)?
            } else {
                date_time(value, &part, culture)?
            },
            rgb,
        ));
    }
    let mut number = Number::new(&value.display())?;
    let negative = number.negative && number.digits != "0";
    let index = if number.digits == "0" && parts.len() >= 3 {
        2
    } else if negative && parts.len() >= 2 {
        1
    } else {
        0
    };
    let mut part = parts[index].clone();
    let (rgb, culture) = directives(&mut part, culture)?;
    if part.is_empty() {
        return Ok((String::new(), rgb));
    }
    if elapsed::requested(&part) {
        let text = elapsed::render(value, &part)?;
        return Ok((
            format!("{}{}", if negative && index == 0 { "-" } else { "" }, text),
            rgb,
        ));
    }
    if fraction::requested(&part) {
        let text = fraction::render(&number, &part)?;
        return Ok((
            format!("{}{}", if negative && index == 0 { "-" } else { "" }, text),
            rgb,
        ));
    }
    let Some(first) = part.iter().position(|t| matches!(code(t), Some('0' | '#'))) else {
        return Ok((literal(&part, None)?, rgb));
    };
    let last = part
        .iter()
        .rposition(|t| matches!(code(t), Some('0' | '#')))
        .unwrap();
    let mask = part[first..=last]
        .iter()
        .map(|t| code(t).ok_or("Literal inside numeric placeholders"))
        .collect::<Result<String, _>>()?;
    let prefix = literal(&part[..first], None)?;
    let mut suffix_tokens = &part[last + 1..];
    while suffix_tokens.first().and_then(code) == Some(',') {
        number.scale += 3;
        suffix_tokens = &suffix_tokens[1..];
    }
    let suffix = literal(suffix_tokens, None)?;
    let percents = part.iter().filter(|t| code(t) == Some('%')).count();
    if percents > 3 {
        return Err("Excess percent scaling");
    }
    number.scale -= percents as i32 * 2;
    let rendered = numeric(number, &mask, culture)?;
    Ok((
        format!(
            "{}{}{}{}",
            if negative && index == 0 { "-" } else { "" },
            prefix,
            rendered,
            suffix
        ),
        rgb,
    ))
}

struct Number {
    negative: bool,
    digits: String,
    scale: i32,
}
impl Number {
    fn new(text: &str) -> Result<Self, &'static str> {
        let negative = text.starts_with('-');
        let text = text.trim_start_matches(['-', '+']);
        let (text, exponent) = if let Some((base, exp)) = text.split_once(['e', 'E']) {
            (
                base,
                exp.parse::<i32>().map_err(|_| "Invalid numeric exponent")?,
            )
        } else {
            (text, 0)
        };
        let (whole, fraction) = text.split_once('.').unwrap_or((text, ""));
        if whole.len() + fraction.len() > 1024
            || !whole
                .bytes()
                .chain(fraction.bytes())
                .all(|b| b.is_ascii_digit())
        {
            return Err("Unsupported numeric value");
        }
        let digits = format!("{whole}{fraction}")
            .trim_start_matches('0')
            .to_owned();
        Ok(Self {
            negative,
            digits: if digits.is_empty() {
                "0".into()
            } else {
                digits
            },
            scale: fraction.len() as i32 - exponent,
        })
    }
    /// Decimal-string rounding avoids conversion of integers/DecimalValue to f64.
    fn rounded(&self, places: usize) -> Result<(String, String), &'static str> {
        let keep = self.digits.len() as i32 - self.scale + places as i32;
        if keep > 1024 || places > 28 {
            return Err("Formatted number exceeds display budget");
        }
        let mut digits = if keep >= self.digits.len() as i32 {
            format!(
                "{}{}",
                self.digits,
                "0".repeat((keep - self.digits.len() as i32) as usize)
            )
        } else {
            let mut value = if keep > 0 {
                self.digits.as_bytes()[..keep as usize].to_vec()
            } else {
                vec![b'0']
            };
            if keep >= 0 && self.digits.as_bytes()[keep as usize] >= b'5' {
                let mut carry = true;
                for digit in value.iter_mut().rev() {
                    if *digit == b'9' {
                        *digit = b'0';
                    } else {
                        *digit += 1;
                        carry = false;
                        break;
                    }
                }
                if carry {
                    value.insert(0, b'1');
                }
            }
            String::from_utf8(value).expect("decimal digits")
        };
        if digits.len() <= places {
            digits = format!("{}{}", "0".repeat(places + 1 - digits.len()), digits);
        }
        let fraction = digits.split_off(digits.len() - places);
        let whole = digits.trim_start_matches('0');
        Ok((
            if whole.is_empty() {
                "0".into()
            } else {
                whole.into()
            },
            fraction,
        ))
    }
}
fn decimal(culture: UiCulture) -> (&'static str, &'static str) {
    match culture {
        UiCulture::Russian => ("\u{a0}", ","),
        UiCulture::English => (",", "."),
        UiCulture::Arabic => ("\u{66c}", "\u{66b}"),
    }
}
fn numeric(mut number: Number, mask: &str, culture: UiCulture) -> Result<String, &'static str> {
    let exponent_character = if mask.contains('e') { 'e' } else { 'E' };
    let (mask, exponent_format) = mask
        .split_once(['e', 'E'])
        .map_or((mask, None), |(a, b)| (a, Some(b)));
    let mut exponent = 0;
    let (integer, fraction) = mask.split_once('.').unwrap_or((mask, ""));
    if integer.is_empty()
        || !integer.chars().all(|c| matches!(c, '0' | '#' | ','))
        || !fraction.chars().all(|c| matches!(c, '0' | '#'))
        || fraction.len() > 28
        || fraction.contains("#0")
    {
        return Err("Unsupported numeric placeholder pattern");
    }
    let exponent_step = integer.len() as i32;
    if exponent_format.is_some() {
        if integer.len() > 6
            || integer.contains(',')
            || integer.contains("0#")
            || !integer.contains('0')
        {
            return Err("Scientific format requires one to six ordered #/0 integer placeholders");
        }
        if number.digits != "0" {
            exponent = (number.digits.len() as i32 - number.scale - 1).div_euclid(exponent_step)
                * exponent_step;
            number.scale += exponent;
        }
    }
    let places = fraction.len();
    let required = fraction.bytes().filter(|b| *b == b'0').count();
    let (mut whole, mut frac) = number.rounded(places)?;
    if exponent_format.is_some() && whole.len() > integer.len() {
        exponent += exponent_step;
        number = Number {
            negative: false,
            digits: format!("{whole}{frac}"),
            scale: frac.len() as i32 + exponent_step,
        };
        (whole, frac) = number.rounded(places)?;
    }
    while frac.len() > required && frac.ends_with('0') {
        frac.pop();
    }
    let minimum = integer.bytes().filter(|b| *b == b'0').count();
    if whole == "0" && minimum == 0 {
        whole.clear();
    }
    if whole.len() < minimum {
        whole = format!("{}{}", "0".repeat(minimum - whole.len()), whole);
    }
    let (group, point) = decimal(culture);
    let mut result = String::new();
    for (i, ch) in whole.chars().enumerate() {
        if integer.contains(',') && i > 0 && (whole.len() - i).is_multiple_of(3) {
            result.push_str(group);
        }
        result.push(ch);
    }
    if !frac.is_empty() {
        result.push_str(point);
        result.push_str(&frac);
    }
    if let Some(pattern) = exponent_format {
        let digits = pattern.strip_prefix(['+', '-']).unwrap_or(pattern);
        if digits.is_empty() || digits.len() > 6 || !digits.bytes().all(|b| b == b'0') {
            return Err("Invalid exponent format");
        }
        result.push(exponent_character);
        if exponent < 0 {
            result.push('-');
        } else if pattern.starts_with('+') {
            result.push('+');
        }
        result.push_str(&format!(
            "{:0width$}",
            exponent.unsigned_abs(),
            width = digits.len()
        ));
    }
    Ok(result)
}
