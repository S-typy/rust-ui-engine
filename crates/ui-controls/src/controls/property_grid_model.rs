//! Property-inspector metadata extends the shared typed form recipes.
use super::*;
use rust_desktop_ui_data::CellValue;

/// Business values of all inspected objects. Mixed is distinct from NULL.
#[derive(Clone, Debug, PartialEq)]
pub enum PropertyValue {
    Uniform(CellValue),
    Mixed,
}

/// Appearance of an accepted value before the editor is opened.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PropertyPreview {
    #[default]
    Auto,
    /// A boolean switch. The editable control uses the same presentation.
    Switch,
    /// A text field containing #RRGGBB or #AARRGGBB. Other text stays readable.
    Color,
    /// An integer percentage. Values outside 0..=100 retain their numeric display.
    Progress,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PropertyDescriptor {
    pub metadata: DataFormField,
    pub category: String,
    pub description: String,
    /// Static expandable path within a category, at most eight levels.
    pub groups: Vec<String>,
    pub default: Option<CellValue>,
    /// Finite choices for a text property, in popup order.
    pub choices: Vec<String>,
    /// Stable keys corresponding to choice labels. Empty uses the labels as keys.
    pub choice_keys: Vec<String>,
    pub mask: Option<String>,
    pub uri_format: Option<String>,
    pub currency_symbol: Option<String>,
    pub minimum: Option<i64>,
    pub maximum: Option<i64>,
    pub step: Option<i64>,
    pub preview: PropertyPreview,
    /// Let the value occupy the name column as well.
    pub full_width: bool,
}
impl PropertyDescriptor {
    pub fn new(field: impl Into<String>) -> Self {
        Self {
            metadata: DataFormField::new(field),
            category: "Свойства".into(),
            description: String::new(),
            groups: Vec::new(),
            default: None,
            choices: Vec::new(),
            choice_keys: Vec::new(),
            mask: None,
            uri_format: None,
            currency_symbol: None,
            minimum: None,
            maximum: None,
            step: None,
            preview: PropertyPreview::Auto,
            full_width: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PropertyGridSettings {
    /// Empty generates schema fields in one category.
    pub properties: Vec<PropertyDescriptor>,
    pub label_width: f32,
    /// Optional fraction of the entire row reserved for property names.
    pub label_fraction: Option<f32>,
    /// Keep the toolbar visible while the property rows scroll in a bounded host.
    pub scrollable: bool,
    pub read_only: bool,
}
impl Default for PropertyGridSettings {
    fn default() -> Self {
        Self {
            properties: Vec::new(),
            label_width: 180.0,
            label_fraction: None,
            scrollable: false,
            read_only: false,
        }
    }
}
impl PropertyGridSettings {
    pub(super) fn validate(&self) -> Result<(), ControlError> {
        if self.label_fraction.is_some_and(|v| !(v > 0. && v < 1.)) {
            return Err(invalid(
                "PropertyGrid label fraction must be between zero and one",
            ));
        }
        DataFormSettings {
            fields: self.properties.iter().map(|v| v.metadata.clone()).collect(),
            label_width: self.label_width,
            read_only: self.read_only,
            ..Default::default()
        }
        .validate()?;
        let mut bytes = 0usize;
        for property in &self.properties {
            if let Some(symbol) = &property.currency_symbol {
                if !matches!(property.metadata.editor.as_str(), "Spin" | "Calculator")
                    || symbol.is_empty()
                    || symbol.len() > 12
                    || symbol.chars().any(|c| c.is_control() || c.is_whitespace())
                {
                    return Err(invalid("Invalid property currency symbol"));
                }
                bytes += symbol.len();
            }
            if let Some(format) = &property.uri_format {
                if property.metadata.editor != "Hyperlink"
                    || format.len() > 2048
                    || format.chars().any(char::is_control)
                {
                    return Err(invalid("Invalid property hyperlink format"));
                }
                bytes += format.len();
            }
            if !property.choice_keys.is_empty()
                && (property.choice_keys.len() != property.choices.len()
                    || property.choice_keys.iter().collect::<HashSet<_>>().len()
                        != property.choice_keys.len()
                    || property
                        .choice_keys
                        .iter()
                        .any(|k| k.is_empty() || k.len() > 256 || k.chars().any(char::is_control)))
            {
                return Err(invalid("ChoiceKeys requires one unique key per choice"));
            }
            if property
                .mask
                .as_ref()
                .is_some_and(|m| m.is_empty() || m.len() > 1024)
                || property
                    .minimum
                    .zip(property.maximum)
                    .is_some_and(|(lo, hi)| lo >= hi)
                || property.step.is_some_and(|s| s <= 0)
            {
                return Err(invalid("Invalid property editor mask or numeric bounds"));
            }
            bytes += property.choice_keys.iter().map(String::len).sum::<usize>()
                + property.mask.as_ref().map_or(0, String::len);
            if property.description.len() > 2048
                || property.description.chars().any(char::is_control)
            {
                return Err(invalid("Invalid property description"));
            }
            bytes += property.description.len();
            if property.groups.len() > 8
                || std::iter::once(&property.category)
                    .chain(&property.groups)
                    .any(|v| v.is_empty() || v.len() > 256 || v.chars().any(char::is_control))
            {
                return Err(invalid(
                    "Property categories and group paths require 1..256 UTF-8 bytes and at most 8 groups",
                ));
            }
            if property.choices.len() > 128
                || property
                    .choices
                    .iter()
                    .any(|v| v.is_empty() || v.len() > 256 || v.chars().any(char::is_control))
                || (property.choice_keys.is_empty()
                    && property.choices.iter().collect::<HashSet<_>>().len()
                        != property.choices.len())
            {
                return Err(invalid(
                    "Property choices require up to 128 nonempty labels with unique identities",
                ));
            }
            bytes += property.choices.iter().map(String::len).sum::<usize>();
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
