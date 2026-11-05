//! Bounded duration display. Numeric values count days; typed clocks retain time.
use super::*;

const SECOND: u128 = 1_000_000_000;

fn unit(ch: char) -> Option<u128> {
    match ch.to_ascii_lowercase() {
        'h' => Some(3600 * SECOND),
        'm' => Some(60 * SECOND),
        's' => Some(SECOND),
        _ => None,
    }
}

pub(super) fn total_marker(tokens: &[Token]) -> Option<(usize, char, usize)> {
    if tokens.first().and_then(code) != Some('[') {
        return None;
    }
    let field = tokens.get(1).and_then(code)?.to_ascii_lowercase();
    unit(field)?;
    let count = tokens[1..]
        .iter()
        .take_while(|t| code(t).is_some_and(|c| c.eq_ignore_ascii_case(&field)))
        .count();
    ((1..=2).contains(&count) && tokens.get(count + 1).and_then(code) == Some(']')).then_some((
        count + 2,
        field,
        count,
    ))
}

pub(super) fn requested(tokens: &[Token]) -> bool {
    tokens.iter().any(|t| code(t) == Some('['))
}

// Multiplication stays decimal, including integer values beyond f64 precision.
fn numeric_ticks(value: &CellValue) -> Result<u128, &'static str> {
    let mut number = Number::new(&value.display())?;
    let mut carry = 0u32;
    let mut product = Vec::with_capacity(number.digits.len() + 5);
    for digit in number.digits.bytes().rev() {
        carry += u32::from(digit - b'0') * 86400;
        product.push(b'0' + (carry % 10) as u8);
        carry /= 10;
    }
    while carry != 0 {
        product.push(b'0' + (carry % 10) as u8);
        carry /= 10;
    }
    product.reverse();
    number.digits = String::from_utf8(product).expect("decimal duration digits");
    if number.digits == "0" {
        return Ok(0);
    }
    let keep = number.digits.len() as i32 - number.scale + 9;
    if keep <= 0 {
        return Ok(0);
    }
    if keep > 39 {
        return Err("Duration exceeds nanosecond display budget");
    }
    // Omitted units and sub-nanosecond precision truncate toward zero.
    number.digits.truncate(keep as usize);
    if number.digits.len() < keep as usize {
        number
            .digits
            .push_str(&"0".repeat(keep as usize - number.digits.len()));
    }
    number
        .digits
        .parse()
        .map_err(|_| "Duration exceeds nanosecond display budget")
}

pub(super) fn render(value: &CellValue, tokens: &[Token]) -> Result<String, &'static str> {
    let ticks = match value {
        CellValue::Time(t) => {
            (u128::from(t.hour()) * 3600 + u128::from(t.minute()) * 60 + u128::from(t.second()))
                * SECOND
                + u128::from(t.nanosecond())
        }
        CellValue::Integer(_) | CellValue::Real(_) | CellValue::Decimal(_) => numeric_ticks(value)?,
        _ => return Err("Duration format requires a numeric day count or a time value"),
    };
    let mut output = String::new();
    let mut total = false;
    let mut previous = u128::MAX;
    let mut previous_seconds = false;
    let mut fraction = false;
    let mut i = 0;
    while i < tokens.len() {
        if let Some((len, field, count)) = total_marker(&tokens[i..]) {
            if total || previous != u128::MAX {
                return Err("Duration requires one leading total field");
            }
            let divisor = unit(field).expect("validated total field");
            output.push_str(&format!("{:0width$}", ticks / divisor, width = count));
            previous = divisor;
            previous_seconds = field == 's';
            total = true;
            i += len;
            continue;
        }
        if let Some(field) = code(&tokens[i]).filter(|c| c.is_ascii_alphabetic()) {
            let divisor = unit(field).ok_or("Unsupported duration field")?;
            if !total || divisor >= previous || fraction {
                return Err("Duration remainder fields must decrease in order");
            }
            let start = i;
            while i < tokens.len()
                && code(&tokens[i]).is_some_and(|c| c.eq_ignore_ascii_case(&field))
            {
                i += 1;
            }
            let count = i - start;
            if count > 2 {
                return Err("Duration remainder field exceeds two characters");
            }
            output.push_str(&format!("{:0width$}", ticks / divisor % 60, width = count));
            previous = divisor;
            previous_seconds = divisor == SECOND;
            continue;
        }
        if code(&tokens[i]) == Some('.') && tokens.get(i + 1).and_then(code) == Some('0') {
            if !previous_seconds || fraction {
                return Err("Fractional duration requires a seconds field");
            }
            let start = i + 1;
            i = start;
            while i < tokens.len() && code(&tokens[i]) == Some('0') {
                i += 1;
            }
            let count = i - start;
            if count > 9 {
                return Err("Duration fractional seconds exceed nanosecond precision");
            }
            output.push('.');
            let digits = format!("{:09}", ticks % SECOND);
            output.push_str(&digits[..count]);
            fraction = true;
            previous_seconds = false;
            continue;
        }
        output.push_str(&literal(&tokens[i..i + 1], None)?);
        previous_seconds = false;
        i += 1;
    }
    if !total {
        return Err("Missing duration total field");
    }
    Ok(output)
}
