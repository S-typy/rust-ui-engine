//! Declarative property bindings, independent of a runtime or view model.
use crate::{Diagnostic, at_attribute};
use roxmltree::{Attribute, Document};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum BindingMode {
    #[default]
    OneWay,
    TwoWay,
    OneTime,
    OneWayToSource,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum UpdateSourceTrigger {
    #[default]
    PropertyChanged,
    LostFocus,
    Explicit,
}

/// Named converter implementations are registered by the host. Fallback values
/// are literals converted to the target property's type by that host.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct BindingDefinition {
    pub source_line: u32,
    pub source_column: u32,
    pub inputs: Vec<BindingInputDefinition>,
    pub expression: Option<String>,
    pub path: String,
    pub element_name: Option<String>,
    pub mode: BindingMode,
    pub update_source_trigger: UpdateSourceTrigger,
    pub converter: Option<String>,
    pub fallback_value: Option<String>,
}

/// An explicit dependency in a MultiBinding. Data paths inherit the surrounding context.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct BindingInputDefinition {
    pub path: String,
    pub element_name: Option<String>,
}

pub(crate) fn valid_identifier(value: &str) -> bool {
    let mut chars = value.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

pub(crate) fn valid_path(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 1024
        && value.split('.').all(|part| {
            let property_end = part.find('[').unwrap_or(part.len());
            if !valid_identifier(&part[..property_end]) {
                return false;
            }
            let mut indexes = &part[property_end..];
            while !indexes.is_empty() {
                let Some(body) = indexes.strip_prefix('[') else {
                    return false;
                };
                let Some(end) = body.find(']') else {
                    return false;
                };
                let index = &body[..end];
                if index.is_empty()
                    || !index.bytes().all(|b| b.is_ascii_digit())
                    || index.parse::<usize>().is_err()
                {
                    return false;
                }
                indexes = &body[end + 1..];
            }
            true
        })
}

pub(crate) fn parse_binding(
    document: &Document<'_>,
    attribute: Attribute<'_, '_>,
    editable: bool,
    default_trigger: UpdateSourceTrigger,
) -> Result<BindingDefinition, Diagnostic> {
    let fail = |message: &str| at_attribute(document, attribute, message);
    let source = attribute.value().trim();
    let multiple = source.starts_with("{MultiBinding");
    let body = source
        .strip_prefix(if multiple {
            "{MultiBinding"
        } else {
            "{Binding"
        })
        .and_then(|s| s.strip_suffix('}'))
        .filter(|body| body.starts_with(char::is_whitespace))
        .ok_or_else(|| fail("expected {Binding Path=Name, ...}"))?;
    if body.contains(['{', '}']) {
        return Err(fail(
            "nested markup extensions are not supported in Binding",
        ));
    }
    let location = at_attribute(document, attribute, "");
    let mut result = BindingDefinition {
        source_line: location.line,
        source_column: location.column,
        update_source_trigger: default_trigger,
        ..Default::default()
    };
    if editable && !multiple {
        result.mode = BindingMode::TwoWay;
    }
    let mut seen = std::collections::HashSet::new();
    let mut parts = Vec::new();
    let mut start = 0;
    let mut quote = None;
    for (offset, ch) in body.char_indices() {
        if matches!(ch, '\'' | '"') {
            if quote == Some(ch) {
                quote = None;
            } else if quote.is_none() {
                quote = Some(ch);
            }
        } else if ch == ',' && quote.is_none() {
            parts.push(&body[start..offset]);
            start = offset + 1;
        }
    }
    if quote.is_some() {
        return Err(fail("unterminated quoted Binding value"));
    }
    parts.push(&body[start..]);
    for (index, part) in parts.into_iter().enumerate() {
        let part = part.trim();
        let (key, value) = match part.split_once('=') {
            Some((key, value)) => (key.trim(), value.trim()),
            None if index == 0 && valid_path(part) => ("Path", part),
            _ => return Err(fail("Binding arguments require Name=Value")),
        };
        if !seen.insert(key) {
            return Err(fail("duplicate Binding argument"));
        }
        let value = if value.starts_with(['\'', '"']) {
            let delimiter = value.chars().next().unwrap();
            if !value.ends_with(delimiter) || value.len() < 2 {
                return Err(fail("invalid quoted Binding value"));
            }
            &value[1..value.len() - 1]
        } else {
            value
        };
        match key {
            "Path" if !multiple && valid_path(value) => result.path = value.into(),
            "Paths" if multiple => {
                for path in value.split(';') {
                    let path = path.trim();
                    let (element_name, path) = if let Some(element) = path.strip_prefix('#') {
                        let (name, path) = element
                            .split_once('.')
                            .ok_or_else(|| fail("Element input requires #Name.Property"))?;
                        if !valid_identifier(name) {
                            return Err(fail("Invalid multi-binding ElementName"));
                        }
                        (Some(name.into()), path)
                    } else {
                        (None, path)
                    };
                    if !valid_path(path) || result.inputs.len() >= 32 {
                        return Err(fail(
                            "MultiBinding requires 1..32 valid Paths separated by ;",
                        ));
                    }
                    result.inputs.push(BindingInputDefinition {
                        path: path.into(),
                        element_name,
                    });
                }
            }
            "Expression" if multiple && !value.is_empty() && value.len() <= 4096 => {
                result.expression = Some(value.into())
            }
            "ElementName" if !multiple && valid_identifier(value) => {
                result.element_name = Some(value.into())
            }
            "Mode" => {
                result.mode = match value {
                    "OneWay" => BindingMode::OneWay,
                    "TwoWay" => BindingMode::TwoWay,
                    "OneTime" => BindingMode::OneTime,
                    "OneWayToSource" => BindingMode::OneWayToSource,
                    _ => return Err(fail("unsupported Binding Mode")),
                }
            }
            "UpdateSourceTrigger" => {
                result.update_source_trigger = match value {
                    "PropertyChanged" => UpdateSourceTrigger::PropertyChanged,
                    "LostFocus" => UpdateSourceTrigger::LostFocus,
                    "Explicit" => UpdateSourceTrigger::Explicit,
                    _ => return Err(fail("unsupported UpdateSourceTrigger")),
                }
            }
            "Converter" if valid_identifier(value) => result.converter = Some(value.into()),
            "FallbackValue" if value.len() <= 4096 => result.fallback_value = Some(value.into()),
            _ => return Err(fail("unknown or invalid Binding argument")),
        }
    }
    if (!multiple && result.path.is_empty()) || (multiple && result.inputs.is_empty()) {
        return Err(fail("Binding requires Path; MultiBinding requires Paths"));
    }
    if multiple && !matches!(result.mode, BindingMode::OneWay | BindingMode::OneTime) {
        return Err(fail("MultiBinding supports OneWay and OneTime only"));
    }
    if multiple && result.expression.is_none() && result.converter.is_none() {
        return Err(fail("MultiBinding requires Expression or Converter"));
    }
    if !matches!(
        result.mode,
        BindingMode::TwoWay | BindingMode::OneWayToSource
    ) && seen.contains("UpdateSourceTrigger")
    {
        return Err(fail(
            "UpdateSourceTrigger requires TwoWay or OneWayToSource",
        ));
    }
    Ok(result)
}
