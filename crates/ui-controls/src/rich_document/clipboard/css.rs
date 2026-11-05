//! Deliberately small inline CSS profile. No cascade, external styles or URLs.
use super::super::{DocumentError, RunStyle};

fn invalid() -> DocumentError {
    DocumentError::Invalid("clipboard CSS syntax".into())
}
fn unsupported() -> DocumentError {
    DocumentError::Unsupported("clipboard CSS property or value".into())
}

/// Split declarations without treating punctuation in a quoted family as CSS.
fn declarations(source: &str) -> Result<Vec<&str>, DocumentError> {
    let mut result = Vec::new();
    let mut quote = None;
    let mut escaped = false;
    let mut start = 0;
    for (i, c) in source.char_indices() {
        if escaped {
            escaped = false;
        } else if c == '\\' {
            escaped = true;
        } else if let Some(q) = quote {
            if c == q {
                quote = None;
            }
        } else if c == '\'' || c == '"' {
            quote = Some(c);
        } else if c == ';' {
            result.push(&source[start..i]);
            start = i + 1;
        } else if matches!(c, '/' | '(' | ')' | '{' | '}') {
            return Err(unsupported());
        }
    }
    if quote.is_some() || escaped {
        return Err(invalid());
    }
    result.push(&source[start..]);
    Ok(result)
}

fn family(value: &str) -> Result<String, DocumentError> {
    let Some(q @ ('\'' | '"')) = value.chars().next() else {
        if value.is_empty()
            || !value
                .chars()
                .all(|c| c.is_alphanumeric() || matches!(c, ' ' | '-' | '_'))
        {
            return Err(unsupported());
        }
        return Ok(value.split_whitespace().collect::<Vec<_>>().join(" "));
    };
    let mut chars = value[1..].chars().peekable();
    let mut result = String::new();
    while let Some(c) = chars.next() {
        if c == q {
            return if chars.next().is_none() {
                Ok(result)
            } else {
                Err(unsupported())
            };
        }
        if matches!(c, '\n' | '\r' | '\x0c') {
            return Err(invalid());
        }
        if c != '\\' {
            result.push(c);
            continue;
        }
        let escaped = chars.next().ok_or_else(invalid)?;
        if escaped == '\n' {
            continue;
        }
        if escaped.is_ascii_hexdigit() {
            let mut hex = String::from(escaped);
            while hex.len() < 6 && chars.peek().is_some_and(char::is_ascii_hexdigit) {
                hex.push(chars.next().unwrap());
            }
            if chars.peek().is_some_and(|c| c.is_ascii_whitespace()) {
                chars.next();
            }
            let n = u32::from_str_radix(&hex, 16).map_err(|_| invalid())?;
            let decoded = char::from_u32(n)
                .filter(|c| *c != '\0')
                .ok_or_else(invalid)?;
            result.push(decoded);
        } else {
            result.push(escaped);
        }
    }
    Err(invalid())
}

fn color(value: &str) -> Result<[u8; 3], DocumentError> {
    if let Some(hex) = value.strip_prefix('#') {
        if !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(invalid());
        }
        return match hex.len() {
            3 => Ok([0, 1, 2].map(|i| u8::from_str_radix(&hex[i..i + 1], 16).unwrap() * 17)),
            6 => Ok([0, 2, 4].map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())),
            _ => Err(unsupported()),
        };
    }
    Ok(match value {
        "black" => [0, 0, 0],
        "white" => [255, 255, 255],
        "red" => [255, 0, 0],
        "green" => [0, 128, 0],
        "blue" => [0, 0, 255],
        "yellow" => [255, 255, 0],
        "gray" | "grey" => [128, 128, 128],
        "silver" => [192, 192, 192],
        "maroon" => [128, 0, 0],
        "purple" => [128, 0, 128],
        "fuchsia" => [255, 0, 255],
        "lime" => [0, 255, 0],
        "olive" => [128, 128, 0],
        "navy" => [0, 0, 128],
        "teal" => [0, 128, 128],
        "aqua" => [0, 255, 255],
        _ => return Err(unsupported()),
    })
}

pub(super) fn apply(
    source: &str,
    style: &mut RunStyle,
    preserve: &mut bool,
) -> Result<(), DocumentError> {
    if source.len() > 4096 {
        return Err(DocumentError::Limit("4096 byte clipboard style"));
    }
    for declaration in declarations(source)? {
        let declaration = declaration.trim();
        if declaration.is_empty() {
            continue;
        }
        let (property, value) = declaration.split_once(':').ok_or_else(invalid)?;
        let value = value.trim();
        let lower = value.to_ascii_lowercase();
        match property.trim().to_ascii_lowercase().as_str() {
            "font-family" => style.family = family(value)?,
            "font-size" => {
                let (n, factor) = if let Some(n) = lower.strip_suffix("px") {
                    (n, 1.0)
                } else if let Some(n) = lower.strip_suffix("pt") {
                    (n, 96.0 / 72.0)
                } else {
                    return Err(unsupported());
                };
                style.size = n.parse::<f32>().map_err(|_| invalid())? * factor;
            }
            "font-weight" => {
                style.bold = match lower.as_str() {
                    "normal" | "400" => false,
                    "bold" | "700" => true,
                    _ => return Err(unsupported()),
                }
            }
            "font-style" => {
                style.italic = match lower.as_str() {
                    "normal" => false,
                    "italic" => true,
                    _ => return Err(unsupported()),
                }
            }
            "text-decoration" | "text-decoration-line" => {
                style.underline = match lower.as_str() {
                    "none" => false,
                    "underline" => true,
                    _ => return Err(unsupported()),
                }
            }
            "color" => style.color = color(&lower)?,
            "white-space" => {
                *preserve = match lower.as_str() {
                    "normal" => false,
                    "pre" | "pre-wrap" => true,
                    _ => return Err(unsupported()),
                }
            }
            _ => return Err(unsupported()),
        }
    }
    style.validate()
}
