//! Exact decimal fraction presentation with bounded denominator search.
use super::*;

pub(super) fn requested(tokens: &[Token]) -> bool {
    tokens.iter().any(|t| code(t) == Some('/'))
}

fn placeholder(t: &Token) -> bool {
    matches!(code(t), Some('0' | '#' | '?'))
}

fn mask(tokens: &[Token]) -> Result<String, &'static str> {
    tokens
        .iter()
        .map(|t| code(t).ok_or("Literal inside fraction mask"))
        .collect()
}

fn valid_mask(text: &str) -> bool {
    if text.is_empty()
        || !text.chars().all(|c| matches!(c, '0' | '#' | '?'))
        || text.trim_start_matches('#').contains('#')
    {
        return false;
    }
    !text.contains('0') || !text.contains('?')
}

/// Halfway candidates prefer the smaller denominator, then the larger value.
fn closest(numerator: u128, denominator: u128, limit: u128) -> (u128, u128) {
    let (mut n, mut d) = (numerator, denominator);
    let (mut p0, mut q0, mut p1, mut q1) = (0, 1, 1, 0);
    while d != 0 {
        let a = n / d;
        if q1 != 0 && a > (limit - q0) / q1 {
            break;
        }
        (p0, p1) = (p1, p0 + a * p1);
        (q0, q1) = (q1, q0 + a * q1);
        (n, d) = (d, n % d);
    }
    if d == 0 {
        return (p1, q1);
    }
    let k = (limit - q0) / q1;
    let (a, b) = (p0 + k * p1, q0 + k * q1);
    let first = (numerator * b).abs_diff(a * denominator) * q1;
    let second = (numerator * q1).abs_diff(p1 * denominator) * b;
    if first < second || (first == second && (b < q1 || (b == q1 && a > p1))) {
        (a, b)
    } else {
        (p1, q1)
    }
}

/// Small multiplication/addition preserves whole integers beyond u128/f64.
fn mul_add(text: &str, factor: u128, add: u128) -> Result<String, &'static str> {
    let mut carry = add;
    let mut out = Vec::with_capacity(text.len() + 8);
    for digit in text.bytes().rev() {
        carry += u128::from(digit - b'0') * factor;
        out.push(b'0' + (carry % 10) as u8);
        carry /= 10;
    }
    while carry != 0 {
        out.push(b'0' + (carry % 10) as u8);
        carry /= 10;
    }
    if out.len() > 1024 {
        return Err("Formatted fraction exceeds display budget");
    }
    out.reverse();
    Ok(String::from_utf8(out).expect("decimal digits"))
}

fn padded(value: &str, mask: &str) -> String {
    let mut digits = if value == "0" && !mask.contains('0') {
        ""
    } else {
        value
    }
    .chars()
    .rev();
    let mut result = Vec::new();
    for ch in mask.chars().rev() {
        if let Some(digit) = digits.next() {
            result.push(digit);
        } else if ch == '0' {
            result.push('0');
        } else if ch == '?' {
            result.push(' ');
        }
    }
    result.extend(digits);
    result.into_iter().rev().collect()
}

pub(super) fn render(number: &Number, tokens: &[Token]) -> Result<String, &'static str> {
    if tokens.iter().filter(|t| code(t) == Some('/')).count() != 1
        || tokens.iter().any(|t| matches!(code(t), Some('%' | ',')))
    {
        return Err("Unsupported fraction scaling or separators");
    }
    let slash = tokens.iter().position(|t| code(t) == Some('/')).unwrap();
    let first = tokens
        .iter()
        .position(placeholder)
        .ok_or("Missing fraction numerator")?;
    let mut numerator_start = slash;
    while numerator_start > 0 && placeholder(&tokens[numerator_start - 1]) {
        numerator_start -= 1;
    }
    if numerator_start == slash || first > numerator_start {
        return Err("Invalid fraction numerator");
    }
    let numerator_mask = mask(&tokens[numerator_start..slash])?;
    if numerator_mask.len() > 6 || !valid_mask(&numerator_mask) {
        return Err("Invalid fraction numerator mask");
    }
    let mixed = first != numerator_start;
    let (whole_mask, separator) = if mixed {
        let text = mask(&tokens[first..numerator_start])?;
        let whole = text.trim_end_matches(' ');
        if whole.len() == text.len() || !valid_mask(whole) {
            return Err("Invalid mixed fraction mask");
        }
        (whole.to_owned(), text[whole.len()..].to_owned())
    } else {
        (String::new(), String::new())
    };
    let mut end = slash + 1;
    while end < tokens.len()
        && code(&tokens[end]).is_some_and(|c| c.is_ascii_digit() || c == '?' || c == '#')
    {
        end += 1;
    }
    let denominator_mask = mask(&tokens[slash + 1..end])?;
    if denominator_mask.is_empty() {
        return Err("Missing fraction denominator");
    }
    let fixed = denominator_mask.bytes().any(|c| (b'1'..=b'9').contains(&c));
    let limit = if fixed {
        if denominator_mask.len() > 6
            || denominator_mask.starts_with('0')
            || !denominator_mask.bytes().all(|c| c.is_ascii_digit())
        {
            return Err("Invalid fixed fraction denominator");
        }
        denominator_mask
            .parse::<u128>()
            .map_err(|_| "Invalid fixed fraction denominator")?
    } else {
        if denominator_mask.len() > 3 || !valid_mask(&denominator_mask) {
            return Err("Invalid variable fraction denominator");
        }
        10u128.pow(denominator_mask.len() as u32) - 1
    };
    let prefix = literal(&tokens[..first], None)?;
    let suffix = literal(&tokens[end..], None)?;
    // DecimalValue has <=28 fractional digits. Real values near a representable
    // denominator boundary need fewer; smaller values round to zero in this profile.
    let (mut whole, fractional) = number.rounded(28)?;
    let n = fractional
        .parse::<u128>()
        .map_err(|_| "Invalid fraction value")?;
    let d = 10u128.pow(28);
    let (mut numerator, denominator) = if fixed {
        ((n * limit + d / 2) / d, limit)
    } else {
        closest(n, d, limit)
    };
    if mixed && numerator == denominator {
        whole = mul_add(&whole, 1, 1)?;
        numerator = 0;
    }
    let numerator_text = if mixed {
        numerator.to_string()
    } else {
        mul_add(&whole, denominator, numerator)?
    };
    let hidden = numerator_text == "0" && !numerator_mask.contains('0');
    let fraction = if hidden {
        let reserved = numerator_mask.contains('?') || denominator_mask.contains('?');
        let width = if reserved {
            padded("0", &numerator_mask).len()
                + 1
                + if fixed {
                    denominator_mask.len()
                } else {
                    padded(&denominator.to_string(), &denominator_mask).len()
                }
        } else {
            0
        };
        " ".repeat(width)
    } else {
        format!(
            "{}/{}",
            padded(&numerator_text, &numerator_mask),
            if fixed {
                denominator_mask
            } else {
                padded(&denominator.to_string(), &denominator_mask)
            }
        )
    };
    Ok(format!(
        "{prefix}{}{separator}{fraction}{suffix}",
        if mixed {
            padded(&whole, &whole_mask)
        } else {
            String::new()
        }
    ))
}
