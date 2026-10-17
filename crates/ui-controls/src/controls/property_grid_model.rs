//! Property-inspector metadata extends the shared typed form recipes.
use super::*;
use rust_desktop_ui_data::CellValue;

/// Business values of all inspected objects. Mixed is distinct from NULL.
#[derive(Clone, Debug, PartialEq)]
pub enum PropertyValue {
    Uniform(CellValue),
    Mixed,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PropertyDescriptor {
    pub metadata: DataFormField,
    pub category: String,
    /// Static expandable path within a category, at most eight levels.
    pub groups: Vec<String>,
    pub default: Option<CellValue>,
}
impl PropertyDescriptor {
    pub fn new(field: impl Into<String>) -> Self {
        Self {
            metadata: DataFormField::new(field),
            category: "Свойства".into(),
            groups: Vec::new(),
            default: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PropertyGridSettings {
    /// Empty generates schema fields in one category.
    pub properties: Vec<PropertyDescriptor>,
    pub label_width: f32,
    pub read_only: bool,
}
impl Default for PropertyGridSettings {
    fn default() -> Self {
        Self {
            properties: Vec::new(),
            label_width: 180.0,
            read_only: false,
        }
    }
}
impl PropertyGridSettings {
    pub(super) fn validate(&self) -> Result<(), ControlError> {
        DataFormSettings {
            fields: self.properties.iter().map(|v| v.metadata.clone()).collect(),
            label_width: self.label_width,
            read_only: self.read_only,
            ..Default::default()
        }
        .validate()?;
        let mut bytes = 0usize;
        for property in &self.properties {
            if property.groups.len() > 8
                || std::iter::once(&property.category)
                    .chain(&property.groups)
                    .any(|v| v.is_empty() || v.len() > 256 || v.chars().any(char::is_control))
            {
                return Err(invalid(
                    "Property categories and group paths require 1..256 UTF-8 bytes and at most 8 groups",
                ));
            }
            bytes += property.category.len()
                + property.groups.iter().map(String::len).sum::<usize>()
                + property.metadata.field.len()
                + property.metadata.label.as_ref().map_or(0, String::len);
        }
        if bytes > 32768 {
            return Err(invalid("Property metadata exceeds 32 KiB"));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum PropertyCommit {
    Saved,
    Unchanged,
    Pending,
    Rejected(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct PropertyChanged {
    pub field: String,
    pub value: CellValue,
    pub objects: Vec<i64>,
}

pub(super) fn invalid(message: impl Into<String>) -> ControlError {
    ControlError::Invalid(message.into())
}
pub(super) fn aggregate(values: &[CellValue]) -> PropertyValue {
    if values.iter().skip(1).all(|v| v == &values[0]) {
        PropertyValue::Uniform(values[0].clone())
    } else {
        PropertyValue::Mixed
    }
}
