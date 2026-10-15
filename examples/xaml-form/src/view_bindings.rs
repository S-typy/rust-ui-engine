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
        "Header" => BindingProperty::Header,
        "IsExpanded" => BindingProperty::IsExpanded,
        "SelectedIndex" => BindingProperty::SelectedIndex,
        "Value" => BindingProperty::Value,
        "SelectedValue" => BindingProperty::SelectedValue,
        "SelectedValues" => BindingProperty::SelectedValues,
        "ItemsSource" => BindingProperty::ItemsSource,
        "IsChecked" => BindingProperty::IsChecked,
        "IsEnabled" => BindingProperty::IsEnabled,
        "IsReadOnly" => BindingProperty::IsReadOnly,
        "Visibility" => BindingProperty::Visibility,
        "Width" => BindingProperty::Width,
        "Height" => BindingProperty::Height,
        "PlaceholderText" => BindingProperty::PlaceholderText,
        _ => return Err(format!("Unsupported binding property '{name}'")),
    })
}
fn fallback(
    ui: &Controls,
    widget: WidgetId,
    property: BindingProperty,
    value: &str,
) -> Result<DataValue, String> {
    Ok(match property {
        BindingProperty::IsChecked
        | BindingProperty::IsEnabled
        | BindingProperty::IsReadOnly
        | BindingProperty::IsExpanded => match value.to_ascii_lowercase().as_str() {
            "true" => DataValue::Bool(true),
            "false" => DataValue::Bool(false),
            _ => return Err("Boolean FallbackValue must be True or False".into()),
        },
        BindingProperty::Visibility => match value.to_ascii_lowercase().as_str() {
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
        },
        BindingProperty::Width | BindingProperty::Height if value == "Auto" => DataValue::Null,
        BindingProperty::SelectedIndex => {
            let index: f64 = value.parse().map_err(err)?;
            if !index.is_finite() || index.fract() != 0.0 || !(0.0..=63.0).contains(&index) {
                return Err("SelectedIndex FallbackValue must be an integer in 0..=63".into());
            }
            DataValue::Number(index)
        }
        BindingProperty::Width | BindingProperty::Height => {
            DataValue::Number(value.parse().map_err(err)?)
        }
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
        BindingProperty::Value if ui.time_settings(widget).is_some() => {
            if value.is_empty() || value == "Null" {
                DataValue::Null
            } else {
                TimeValue::parse(value).map_err(err)?.to_iso().into()
            }
        }
        BindingProperty::SelectedValue if value == "Null" => DataValue::Null,
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
            ElementKind::ListBox(selection) | ElementKind::ComboBox(selection) => Some(selection),
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
                let source = if let Some(name) = &definition.element_name {
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
        self.bindings.as_mut().is_some_and(|engine| engine.sync(ui))
    }
}
