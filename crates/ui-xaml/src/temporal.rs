//! Typed civil date/time definitions; storage is independent of display format.
use crate::*;
use rust_desktop_ui_data::{DateValue, TimeValue};

#[derive(Clone, Debug, PartialEq)]
pub struct DateTimeEditDefinition {
    pub editor: EditBoxDefinition,
    pub value: Option<String>,
    pub minimum: Option<String>,
    pub maximum: Option<String>,
    pub date_format: String,
    pub show_seconds: bool,
    pub time_step: u32,
    pub allow_empty: bool,
}
impl Default for DateTimeEditDefinition {
    fn default() -> Self {
        Self {
            editor: Default::default(),
            value: None,
            minimum: None,
            maximum: None,
            date_format: "dd.MM.yyyy".into(),
            show_seconds: false,
            time_step: 60,
            allow_empty: true,
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct DateRangeEditDefinition {
    pub editor: EditBoxDefinition,
    pub value: Option<String>,
    pub minimum: Option<String>,
    pub maximum: Option<String>,
    pub format: String,
    pub allow_empty: bool,
}
impl Default for DateRangeEditDefinition {
    fn default() -> Self {
        Self {
            editor: Default::default(),
            value: None,
            minimum: None,
            maximum: None,
            format: "dd.MM.yyyy".into(),
            allow_empty: true,
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CalendarSelectionMode {
    #[default]
    Single,
    Range,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CalendarViewDefinition {
    #[default]
    Days,
    Months,
    Years,
}
#[derive(Clone, Debug, PartialEq)]
pub struct CalendarDefinition {
    pub selection_mode: CalendarSelectionMode,
    pub value: Option<String>,
    pub minimum: Option<String>,
    pub maximum: Option<String>,
    pub display_date: Option<String>,
    pub view: CalendarViewDefinition,
    pub is_read_only: bool,
    pub format: String,
}
impl Default for CalendarDefinition {
    fn default() -> Self {
        Self {
            selection_mode: Default::default(),
            value: None,
            minimum: None,
            maximum: None,
            display_date: None,
            view: Default::default(),
            is_read_only: false,
            format: "dd.MM.yyyy".into(),
        }
    }
}

fn date(value: &str) -> Result<DateValue, String> {
    value
        .parse()
        .map_err(|error: rust_desktop_ui_data::DataError| error.to_string())
}
fn datetime(value: &str) -> Result<(DateValue, TimeValue), String> {
    if value.len() != 19 || !value.is_ascii() || value.as_bytes()[10] != b'T' {
        return Err("DateTimeEdit storage requires YYYY-MM-DDTHH:MM:SS without a timezone".into());
    }
    Ok((
        date(&value[..10])?,
        value[11..]
            .parse()
            .map_err(|error: rust_desktop_ui_data::DataError| error.to_string())?,
    ))
}
fn range(value: &str) -> Result<(DateValue, DateValue), String> {
    let (start, end) = value
        .split_once('/')
        .ok_or("Date range storage requires YYYY-MM-DD/YYYY-MM-DD")?;
    let (start, end) = (date(start)?, date(end)?);
    if start > end {
        return Err("Date range start must not follow its end".into());
    }
    Ok((start, end))
}
fn format(value: &str) -> Result<(), String> {
    if !matches!(value, "dd.MM.yyyy" | "yyyy-MM-dd") {
        return Err("Date format must be dd.MM.yyyy or yyyy-MM-dd".into());
    }
    Ok(())
}
fn bounds<T: Copy + Ord>(
    minimum: Option<T>,
    maximum: Option<T>,
    value: Option<(T, T)>,
) -> Result<(), String> {
    if minimum.zip(maximum).is_some_and(|(a, b)| a > b)
        || value.is_some_and(|(start, end)| {
            minimum.is_some_and(|minimum| start < minimum)
                || maximum.is_some_and(|maximum| end > maximum)
        })
    {
        return Err("Temporal value lies outside ordered Minimum/Maximum".into());
    }
    Ok(())
}
pub fn validate_date_time_edit(
    definition: &DateTimeEditDefinition,
    has_value_binding: bool,
) -> Result<(), String> {
    format(&definition.date_format)?;
    if !(1..=86400).contains(&definition.time_step) {
        return Err("TimeStep must be 1..86400 seconds".into());
    }
    if !definition.allow_empty && definition.value.is_none() && !has_value_binding {
        return Err("DateTimeEdit requires Value when AllowEmpty=False".into());
    }
    let min = definition.minimum.as_deref().map(datetime).transpose()?;
    let max = definition.maximum.as_deref().map(datetime).transpose()?;
    let value = definition.value.as_deref().map(datetime).transpose()?;
    bounds(min, max, value.map(|v| (v, v)))
}
pub fn validate_date_range_edit(
    definition: &DateRangeEditDefinition,
    has_value_binding: bool,
) -> Result<(), String> {
    format(&definition.format)?;
    if !definition.allow_empty && definition.value.is_none() && !has_value_binding {
        return Err("DateRangeEdit requires Value when AllowEmpty=False".into());
    }
    bounds(
        definition.minimum.as_deref().map(date).transpose()?,
        definition.maximum.as_deref().map(date).transpose()?,
        definition.value.as_deref().map(range).transpose()?,
    )
}
pub fn validate_calendar(definition: &CalendarDefinition) -> Result<(), String> {
    format(&definition.format)?;
    if let Some(display) = &definition.display_date {
        date(display)?;
    }
    let value = definition
        .value
        .as_deref()
        .map(|value| match definition.selection_mode {
            CalendarSelectionMode::Single => date(value).map(|value| (value, value)),
            CalendarSelectionMode::Range => range(value),
        })
        .transpose()?;
    bounds(
        definition.minimum.as_deref().map(date).transpose()?,
        definition.maximum.as_deref().map(date).transpose()?,
        value,
    )
}

pub(super) fn property(
    document: &Document<'_>,
    attr: Attribute<'_, '_>,
    kind: &mut ElementKind,
) -> Result<bool, Diagnostic> {
    match kind {
        ElementKind::DateTimeEdit(editor) => match attr.name() {
            "Value" => editor.value = Some(attr.value().into()),
            "Minimum" => editor.minimum = Some(attr.value().into()),
            "Maximum" => editor.maximum = Some(attr.value().into()),
            "DateFormat" => editor.date_format = attr.value().into(),
            "ShowSeconds" => editor.show_seconds = boolean(document, attr)?,
            "TimeStep" => {
                editor.time_step = attr.value().parse().map_err(|_| {
                    at_attribute(document, attr, "TimeStep must be 1..86400 seconds")
                })?
            }
            "AllowEmpty" => editor.allow_empty = boolean(document, attr)?,
            "Text" => return Ok(false),
            _ => return editor_property(document, attr, &mut editor.editor),
        },
        ElementKind::DateRangeEdit(editor) => match attr.name() {
            "Value" => editor.value = Some(attr.value().into()),
            "Minimum" => editor.minimum = Some(attr.value().into()),
            "Maximum" => editor.maximum = Some(attr.value().into()),
            "Format" => editor.format = attr.value().into(),
            "AllowEmpty" => editor.allow_empty = boolean(document, attr)?,
            "Text" => return Ok(false),
            _ => return editor_property(document, attr, &mut editor.editor),
        },
        ElementKind::Calendar(calendar) => match attr.name() {
            "Value" => calendar.value = Some(attr.value().into()),
            "Minimum" => calendar.minimum = Some(attr.value().into()),
            "Maximum" => calendar.maximum = Some(attr.value().into()),
            "DisplayDate" => calendar.display_date = Some(attr.value().into()),
            "Format" => calendar.format = attr.value().into(),
            "IsReadOnly" => calendar.is_read_only = boolean(document, attr)?,
            "SelectionMode" => {
                calendar.selection_mode = match attr.value() {
                    "Single" => CalendarSelectionMode::Single,
                    "Range" => CalendarSelectionMode::Range,
                    _ => {
                        return Err(at_attribute(
                            document,
                            attr,
                            "Calendar SelectionMode must be Single or Range",
                        ));
                    }
                }
            }
            "View" => {
                calendar.view = match attr.value() {
                    "Days" => CalendarViewDefinition::Days,
                    "Months" => CalendarViewDefinition::Months,
                    "Years" => CalendarViewDefinition::Years,
                    _ => {
                        return Err(at_attribute(
                            document,
                            attr,
                            "Calendar View must be Days, Months or Years",
                        ));
                    }
                }
            }
            _ => return Ok(false),
        },
        _ => return Ok(false),
    }
    Ok(true)
}
