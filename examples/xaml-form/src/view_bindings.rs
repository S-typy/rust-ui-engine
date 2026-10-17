//! Resolves XAML names and inherited data paths into reusable control bindings.
use super::*;
use rust_desktop_ui_controls::*;
use rust_desktop_ui_xaml as x;
use std::{cell::RefCell, rc::Rc};

pub(super) struct PendingBindings {
    widget: WidgetId,
    context: String,
    bindings: std::collections::BTreeMap<String, x::BindingDefinition>,
    display: Option<String>,
    key: Option<String>,
    columns: Vec<String>,
    mapping_error: Option<String>,
}

pub(super) fn context_path(parent: &str, child: Option<&str>) -> String {
    match (parent.is_empty(), child) {
        (_, None) => parent.into(),
        (true, Some(child)) => child.into(),
        (false, Some(child)) => format!("{parent}.{child}"),
    }
}
fn property(name: &str) -> Result<BindingProperty, String> {
    Ok(match name {
        "Text" | "Content" => BindingProperty::Text,
        "Query" => BindingProperty::Query,
        "Header" => BindingProperty::Header,
        "IsExpanded" => BindingProperty::IsExpanded,
        "SelectedIndex" => BindingProperty::SelectedIndex,
        "Value" => BindingProperty::Value,
        "LowerValue" => BindingProperty::LowerValue,
        "UpperValue" => BindingProperty::UpperValue,
        "IsIndeterminate" => BindingProperty::IsIndeterminate,
        "SelectedKey" => BindingProperty::SelectedKey,
        "SelectedValue" => BindingProperty::SelectedValue,
        "FilterText" => BindingProperty::FilterText,
        "SelectedValues" => BindingProperty::SelectedValues,
        "ItemsSource" => BindingProperty::ItemsSource,
        "IsChecked" => BindingProperty::IsChecked,
        "IsEnabled" => BindingProperty::IsEnabled,
        "IsReadOnly" => BindingProperty::IsReadOnly,
        "Visibility" => BindingProperty::Visibility,
        "Width" => BindingProperty::Width,
        "Height" => BindingProperty::Height,
        "PlaceholderText" => BindingProperty::PlaceholderText,
        "Badge.Count" => BindingProperty::BadgeCount,
        "Badge.Text" => BindingProperty::BadgeText,
        "Badge.Visibility" => BindingProperty::BadgeVisibility,
        _ => return Err(format!("Unsupported binding property '{name}'")),
    })
}

/// Validate expression syntax and every explicit dependency before creating widgets.
pub(super) fn validate_definitions(element: &ElementDefinition) -> Result<(), String> {
    for (target, definition) in &element.common.bindings {
        let validate = || -> Result<(), String> {
            property(target)?;
            if definition.inputs.is_empty() {
                if definition.expression.is_some() {
                    return Err("An expression requires MultiBinding inputs".into());
                }
                if definition.element_name.is_some() {
                    property(&definition.path)?;
                } else {
                    BindingPath::parse(&definition.path).map_err(err)?;
                }
            } else {
                if definition.inputs.len() > 32
                    || !matches!(
                        definition.mode,
                        x::BindingMode::OneWay | x::BindingMode::OneTime
                    )
                {
                    return Err("MultiBinding requires 1..32 inputs and OneWay or OneTime".into());
                }
                for input in &definition.inputs {
                    if input.element_name.is_some() {
                        property(&input.path)?;
                    } else {
                        BindingPath::parse(&input.path).map_err(err)?;
                    }
                }
                if let Some(source) = &definition.expression {
                    if definition.converter.is_some() {
                        return Err("MultiBinding cannot combine Expression and Converter".into());
                    }
                    let expression = BindingExpression::parse(source).map_err(err)?;
                    if expression.input_count() > definition.inputs.len() {
                        return Err("Expression references an absent MultiBinding input".into());
                    }
                } else if definition.converter.is_none() {
                    return Err("MultiBinding needs Expression or Converter".into());
                }
            }
            Ok(())
        };
        validate().map_err(|error| {
            if definition.source_line > 0 {
                format!(
                    "{}:{}: {error}",
                    definition.source_line, definition.source_column
                )
            } else {
                error
            }
        })?;
    }
    Ok(())
}
fn fallback(
    ui: &Controls,
    widget: WidgetId,
    property: BindingProperty,
    value: &str,
) -> Result<DataValue, String> {
    Ok(match property {
        BindingProperty::IsChecked
            if value == "Null" || value == "{x:Null}" || value == "Indeterminate" =>
        {
            DataValue::Null
        }
        BindingProperty::IsChecked
        | BindingProperty::IsIndeterminate
        | BindingProperty::IsEnabled
        | BindingProperty::IsReadOnly
        | BindingProperty::IsExpanded => match value.to_ascii_lowercase().as_str() {
            "true" => DataValue::Bool(true),
            "false" => DataValue::Bool(false),
            _ => return Err("Boolean FallbackValue must be True or False".into()),
        },
        BindingProperty::Visibility | BindingProperty::BadgeVisibility => {
            match value.to_ascii_lowercase().as_str() {
                "true" => DataValue::Bool(true),
                "false" => DataValue::Bool(false),
                "visible" => DataValue::from("Visible"),
                "hidden" => DataValue::from("Hidden"),
                "collapsed" => DataValue::from("Collapsed"),
                _ => {
                    return Err(
                        "Visibility FallbackValue must be Visible/Hidden/Collapsed or True/False"
                            .into(),
                    );
                }
            }
        }
        BindingProperty::Width | BindingProperty::Height if value == "Auto" => DataValue::Null,
        BindingProperty::SelectedIndex => {
            let index: f64 = value.parse().map_err(err)?;
            if !index.is_finite() || index.fract() != 0.0 || !(0.0..=63.0).contains(&index) {
                return Err("SelectedIndex FallbackValue must be an integer in 0..=63".into());
            }
            DataValue::Number(index)
        }
        BindingProperty::Width
        | BindingProperty::Height
        | BindingProperty::LowerValue
        | BindingProperty::UpperValue => DataValue::Number(value.parse().map_err(err)?),
        BindingProperty::Value
            if ui.spin_settings(widget).is_some()
                || ui.progress_settings(widget).is_some()
                || ui.zoom_settings(widget).is_some() =>
        {
            DataValue::Number(value.parse().map_err(err)?)
        }
        BindingProperty::Value if ui.date_settings(widget).is_some() => {
            if value.is_empty() || value == "Null" {
                DataValue::Null
            } else {
                DateValue::parse_iso(value).map_err(err)?.to_iso().into()
            }
        }
        BindingProperty::Value if ui.numeric_settings(widget).is_some() => {
            if value.is_empty() || value == "Null" || value == "{x:Null}" {
                DataValue::Null
            } else {
                let parsed = ui
                    .numeric_settings(widget)
                    .unwrap()
                    .parse_storage(Some(value))
                    .map_err(err)?;
                parsed.map_or(DataValue::Null, |value| {
                    DataValue::String(value.to_string())
                })
            }
        }
        BindingProperty::Value if ui.time_settings(widget).is_some() => {
            if value.is_empty() || value == "Null" {
                DataValue::Null
            } else {
                TimeValue::parse(value).map_err(err)?.to_iso().into()
            }
        }
        BindingProperty::Value if ui.date_time_settings(widget).is_some() => {
            if value.is_empty() || value == "Null" || value == "{x:Null}" {
                DataValue::Null
            } else {
                DateTimeValue::parse_iso(value)
                    .map_err(err)?
                    .to_iso()
                    .into()
            }
        }
        BindingProperty::Value
            if ui.date_range_settings(widget).is_some()
                || matches!(
                    ui.calendar_selection(widget),
                    Some(CalendarSelection::Range(_))
                ) =>
        {
            if value.is_empty() || value == "Null" || value == "{x:Null}" {
                DataValue::Null
            } else {
                let value = DateRangeValue::parse_iso(value).map_err(err)?;
                DataValue::object([
                    ("start", value.start.to_iso().into()),
                    ("end", value.end.to_iso().into()),
                ])
            }
        }
        BindingProperty::Value if ui.calendar_selection(widget).is_some() => {
            if value.is_empty() || value == "Null" || value == "{x:Null}" {
                DataValue::Null
            } else {
                DateValue::parse_iso(value).map_err(err)?.to_iso().into()
            }
        }
        BindingProperty::Value if ui.rating_settings(widget).is_some() => {
            if value == "null" {
                DataValue::Null
            } else {
                DataValue::Number(value.parse().map_err(err)?)
            }
        }
        BindingProperty::Value if ui.slider_value(widget).is_some() => {
            DataValue::Number(value.parse().map_err(err)?)
        }
        BindingProperty::SelectedValue | BindingProperty::SelectedKey if value == "Null" => {
            DataValue::Null
        }
        _ => DataValue::String(value.into()),
    })
}
impl MountedView {
    pub(super) fn record_bindings(
        &mut self,
        widget: WidgetId,
        element: &ElementDefinition,
        context: String,
    ) {
        if element.common.bindings.is_empty() {
            return;
        }
        let selection = match &element.kind {
            ElementKind::ListBox(selection)
            | ElementKind::ComboBox(selection)
            | ElementKind::RadioGroup(selection) => Some(selection),
            ElementKind::LookupEdit(lookup) => Some(&lookup.selection),
            _ => None,
        };
        let display = selection.and_then(|selection| selection.display_member_path.clone());
        let first_column = match &element.kind {
            ElementKind::LookupEdit(lookup) => {
                lookup.columns.first().map(|column| column.field.clone())
            }
            _ => None,
        };
        let mapping_error = match (&display, &first_column) {
            (Some(display), Some(first)) if display != first => {
                Some("Lookup DisplayMemberPath must match its first column Field".to_owned())
            }
            _ => None,
        };
        self.pending_bindings.push(PendingBindings {
            widget,
            context,
            bindings: element.common.bindings.clone(),
            display: display.or(first_column),
            key: selection.and_then(|s| s.value_member_path.clone()),
            columns: match &element.kind {
                ElementKind::LookupEdit(lookup) => lookup
                    .columns
                    .iter()
                    .skip(1)
                    .map(|c| c.field.clone())
                    .collect(),
                _ => Vec::new(),
            },
            mapping_error,
        });
    }
    pub(super) fn initialize_bindings(
        &mut self,
        ui: &mut Controls,
        context: Rc<RefCell<BindingContext>>,
        converters: Rc<RefCell<BindingConverterRegistry>>,
    ) -> Result<(), String> {
        if self.pending_bindings.is_empty() {
            return Ok(());
        }
        let mut engine = ControlBindings::new(context).with_converters(converters);
        for pending in std::mem::take(&mut self.pending_bindings) {
            if let Some(error) = pending.mapping_error {
                return Err(error);
            }
            for (name, definition) in pending.bindings {
                let target_property = property(&name)?;
                let source = if !definition.inputs.is_empty() {
                    let inputs = definition
                        .inputs
                        .iter()
                        .map(|input| {
                            if let Some(name) = &input.element_name {
                                let widget = self
                                    .find_name(name)
                                    .ok_or_else(|| format!("Unknown ElementName '{name}'"))?;
                                Ok(BindingInput::Element(BindingEndpoint {
                                    widget,
                                    property: property(&input.path)?,
                                }))
                            } else {
                                Ok(BindingInput::DataContext(
                                    BindingPath::parse(&context_path(
                                        &pending.context,
                                        Some(&input.path),
                                    ))
                                    .map_err(err)?,
                                ))
                            }
                        })
                        .collect::<Result<Vec<_>, String>>()?;
                    if let Some(expression) = &definition.expression {
                        BindingSource::Expression {
                            inputs,
                            expression: BindingExpression::parse(expression).map_err(err)?,
                        }
                    } else {
                        BindingSource::Multi(inputs)
                    }
                } else if let Some(name) = &definition.element_name {
                    let widget = self
                        .find_name(name)
                        .ok_or_else(|| format!("Unknown ElementName '{name}'"))?;
                    BindingSource::Element(BindingEndpoint {
                        widget,
                        property: property(&definition.path)?,
                    })
                } else {
                    BindingSource::DataContext(
                        BindingPath::parse(&context_path(&pending.context, Some(&definition.path)))
                            .map_err(err)?,
                    )
                };
                engine
                    .add(ControlBinding {
                        target: BindingEndpoint {
                            widget: pending.widget,
                            property: target_property,
                        },
                        source,
                        mode: match definition.mode {
                            x::BindingMode::OneWay => ControlBindingMode::OneWay,
                            x::BindingMode::TwoWay => ControlBindingMode::TwoWay,
                            x::BindingMode::OneTime => ControlBindingMode::OneTime,
                            x::BindingMode::OneWayToSource => ControlBindingMode::OneWayToSource,
                        },
                        trigger: match definition.update_source_trigger {
                            x::UpdateSourceTrigger::PropertyChanged => {
                                BindingUpdateTrigger::PropertyChanged
                            }
                            x::UpdateSourceTrigger::LostFocus => BindingUpdateTrigger::LostFocus,
                            x::UpdateSourceTrigger::Explicit => BindingUpdateTrigger::Explicit,
                        },
                        converter: definition.converter,
                        fallback: definition
                            .fallback_value
                            .as_deref()
                            .map(|v| fallback(ui, pending.widget, target_property, v))
                            .transpose()?,
                        items: ItemBindingMapping {
                            display_member_path: pending
                                .display
                                .as_deref()
                                .map(BindingPath::parse)
                                .transpose()
                                .map_err(err)?,
                            value_member_path: pending
                                .key
                                .as_deref()
                                .map(BindingPath::parse)
                                .transpose()
                                .map_err(err)?,
                            columns: pending
                                .columns
                                .iter()
                                .map(|v| BindingPath::parse(v))
                                .collect::<Result<_, _>>()
                                .map_err(err)?,
                        },
                    })
                    .map_err(err)?;
            }
        }
        engine.initialize(ui).map_err(err)?;
        self.bindings = Some(engine);
        Ok(())
    }
    pub(crate) fn sync_bindings(&mut self, ui: &mut Controls) -> bool {
        let changed = self.bindings.as_mut().is_some_and(|engine| engine.sync(ui));
        self.sync_template_bindings(ui) || changed
    }
}
