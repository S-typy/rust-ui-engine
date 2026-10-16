//! Typed metadata and bounded editor recipes for retained data forms.
use super::*;
use rust_desktop_ui_data::{CellValue, FieldSchema, FieldType};

/// A named recipe chooses an existing editor and its typed value adapter.
/// It cannot install arbitrary callbacks during a storage transaction.
#[derive(Clone, Debug)]
pub enum DataFormEditorFactory {
    Text(EditorSettings),
    Numeric(NumericEditSettings),
    Date(DateEditSettings),
    Time(TimeEditSettings),
    CheckBox,
}

#[derive(Clone, Debug, Default)]
pub struct DataFormFactories {
    pub(super) entries: HashMap<String, DataFormEditorFactory>,
}
impl DataFormFactories {
    pub fn register(
        &mut self,
        name: &str,
        factory: DataFormEditorFactory,
    ) -> Result<(), ControlError> {
        if !valid_factory_name(name)
            || builtin_name(name)
            || self.entries.contains_key(name)
            || self.entries.len() >= 64
        {
            return Err(ControlError::Invalid("Factory name must be unique, nonreserved and at most 64 ASCII characters; at most 64 recipes".into()));
        }
        match &factory {
            DataFormEditorFactory::Numeric(v) => v.validate()?,
            DataFormEditorFactory::Date(v) => v.validate()?,
            DataFormEditorFactory::Time(v) => v.validate()?,
            DataFormEditorFactory::Text(v) => {
                TextLimits::default()
                    .validate(&v.placeholder)
                    .map_err(edit_error)?;
            }
            DataFormEditorFactory::CheckBox => {}
        }
        self.entries.insert(name.into(), factory);
        Ok(())
    }
    pub(super) fn resolve(
        &self,
        name: &str,
        field: &FieldSchema,
    ) -> Result<DataFormEditorFactory, ControlError> {
        let automatic = match field.kind {
            FieldType::Bool => "CheckBox",
            FieldType::Integer | FieldType::Decimal => "Numeric",
            FieldType::Date => "Date",
            FieldType::Time => "Time",
            _ => "Text",
        };
        let automatic_recipe = name == "Auto";
        let name = if automatic_recipe { automatic } else { name };
        let value =
            match name {
                "Text" => DataFormEditorFactory::Text(EditorSettings::default()),
                "CheckBox" => DataFormEditorFactory::CheckBox,
                "Numeric" => DataFormEditorFactory::Numeric(NumericEditSettings {
                    minimum: DecimalValue::new(
                        if field.kind == FieldType::Integer {
                            i64::MIN as i128
                        } else {
                            i128::MIN
                        },
                        0,
                    )
                    .unwrap(),
                    maximum: DecimalValue::new(
                        if field.kind == FieldType::Integer {
                            i64::MAX as i128
                        } else {
                            i128::MAX
                        },
                        0,
                    )
                    .unwrap(),
                    precision: if field.kind == FieldType::Integer {
                        0
                    } else {
                        28
                    },
                    allow_empty: field.nullable,
                    trim_trailing_zeros: automatic_recipe && field.kind == FieldType::Decimal,
                    ..Default::default()
                }),
                "Date" => DataFormEditorFactory::Date(DateEditSettings {
                    allow_empty: field.nullable,
                    ..Default::default()
                }),
                "Time" => DataFormEditorFactory::Time(TimeEditSettings {
                    allow_empty: field.nullable,
                    show_seconds: true,
                    ..Default::default()
                }),
                _ => self.entries.get(name).cloned().ok_or_else(|| {
                    ControlError::Invalid(format!("Unknown editor factory '{name}'"))
                })?,
            };
        let valid = match value {
            DataFormEditorFactory::Text(_) => true,
            DataFormEditorFactory::CheckBox => field.kind == FieldType::Bool,
            DataFormEditorFactory::Numeric(_) => {
                matches!(field.kind, FieldType::Integer | FieldType::Decimal)
            }
            DataFormEditorFactory::Date(_) => field.kind == FieldType::Date,
            DataFormEditorFactory::Time(_) => field.kind == FieldType::Time,
        };
        if !valid {
            return Err(ControlError::Invalid(format!(
                "Factory '{name}' cannot edit {:?}",
                field.kind
            )));
        }
        if field.nullable
            && match &value {
                DataFormEditorFactory::Numeric(v) => !v.allow_empty,
                DataFormEditorFactory::Date(v) => !v.allow_empty,
                DataFormEditorFactory::Time(v) => !v.allow_empty,
                _ => false,
            }
        {
            return Err(ControlError::Invalid(
                "A nullable field requires an editor recipe that accepts empty values".into(),
            ));
        }
        Ok(value)
    }
}
fn valid_factory_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
}
fn builtin_name(name: &str) -> bool {
    matches!(
        name,
        "Auto" | "Text" | "Numeric" | "Date" | "Time" | "CheckBox"
    )
}

#[derive(Clone, Debug, PartialEq)]
pub struct DataFormField {
    pub field: String,
    /// None uses FieldSchema.title; empty labels are allowed.
    pub label: Option<String>,
    pub visible: bool,
    pub read_only: bool,
    pub editor: String,
}
impl DataFormField {
    pub fn new(field: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            label: None,
            visible: true,
            read_only: false,
            editor: "Auto".into(),
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct DataFormSettings {
    /// Empty generates all fields in schema order. Otherwise this is an explicit subset.
    pub fields: Vec<DataFormField>,
    pub label_width: f32,
    pub row_gap: f32,
    pub read_only: bool,
}
impl Default for DataFormSettings {
    fn default() -> Self {
        Self {
            fields: Vec::new(),
            label_width: 160.0,
            row_gap: 8.0,
            read_only: false,
        }
    }
}
impl DataFormSettings {
    pub(super) fn validate(&self) -> Result<(), ControlError> {
        if !self.label_width.is_finite()
            || !(0.0..=4096.0).contains(&self.label_width)
            || !self.row_gap.is_finite()
            || !(0.0..=256.0).contains(&self.row_gap)
            || self.fields.len() > 128
        {
            return Err(ControlError::Invalid(
                "DataForm supports 128 fields, LabelWidth 0..4096 and RowGap 0..256".into(),
            ));
        }
        let mut names = HashSet::new();
        for field in &self.fields {
            if field.field.is_empty()
                || field.field.len() > 256
                || !names.insert(&field.field)
                || !valid_factory_name(&field.editor)
                || field.label.as_ref().is_some_and(|v| v.len() > 4096)
            {
                return Err(ControlError::Invalid(
                    "Invalid or duplicate DataForm field metadata".into(),
                ));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DataFormIssue {
    pub field: String,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DataFormCommit {
    Saved,
    Unchanged,
    Invalid(Vec<DataFormIssue>),
    Pending,
    Conflict(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DataFormArrangement {
    pub field: String,
    pub visible: bool,
}

pub(super) fn decimal_cell(
    field: &FieldSchema,
    value: Option<DecimalValue>,
) -> Result<CellValue, String> {
    match value {
        None => Ok(CellValue::Null),
        Some(value) if field.kind == FieldType::Integer => {
            if value.scale() != 0 {
                return Err("Integer field cannot contain a fraction".into());
            }
            i64::try_from(value.coefficient())
                .map(CellValue::Integer)
                .map_err(|_| "Integer is outside i64 range".into())
        }
        Some(value) => Ok(CellValue::Decimal(value)),
    }
}
