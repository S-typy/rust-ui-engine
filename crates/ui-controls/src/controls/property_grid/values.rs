use super::*;
use rust_desktop_ui_data::{DateValue as DataDate, FieldType, TimeValue as DataTime};

pub(super) fn neutral_value(
    recipe: &DataFormEditorFactory,
    field: &FieldSchema,
    default: Option<&CellValue>,
) -> Result<CellValue, ControlError> {
    if let Some(value) = default {
        return Ok(value.clone());
    }
    let value = if field.nullable {
        CellValue::Null
    } else {
        match recipe {
            DataFormEditorFactory::Numeric(s) => {
                let zero = DecimalValue::new(0, 0).unwrap();
                let value = zero.max(s.minimum).min(s.maximum);
                if field.kind == FieldType::Integer {
                    let divisor = 10_i128.pow(u32::from(value.scale()));
                    let floor = value.coefficient().div_euclid(divisor);
                    let integer = if value > zero && value.coefficient().rem_euclid(divisor) != 0 {
                        floor + 1
                    } else {
                        floor
                    };
                    let integer = i64::try_from(integer).map_err(|_| {
                        invalid("Integer recipe has no representable initial value")
                    })?;
                    s.validate_value(Some(DecimalValue::new(i128::from(integer), 0).unwrap()))
                        .map_err(invalid)?;
                    CellValue::Integer(integer)
                } else {
                    CellValue::Decimal(value)
                }
            }
            DataFormEditorFactory::Date(s) => CellValue::Date(
                DateValue::new(2000, 1, 1)?
                    .max(s.minimum.unwrap_or(DateValue::new(1, 1, 1)?))
                    .min(s.maximum.unwrap_or(DateValue::new(9999, 12, 31)?))
                    .to_iso()
                    .parse()
                    .map_err(|e: rust_desktop_ui_data::DataError| invalid(e.to_string()))?,
            ),
            DataFormEditorFactory::Time(s) => {
                let mut value = s.minimum.unwrap_or(TimeValue::new(0, 0, 0)?);
                if !s.show_seconds && value.second != 0 {
                    let seconds = (value.seconds() / 60 + 1) * 60;
                    if seconds >= 86400 {
                        return Err(invalid("Time recipe has no representable initial minute"));
                    }
                    value = TimeValue::from_seconds(seconds);
                }
                CellValue::Time(
                    value
                        .to_iso()
                        .parse()
                        .map_err(|e: rust_desktop_ui_data::DataError| invalid(e.to_string()))?,
                )
            }
            _ => match field.kind {
                FieldType::Bool => CellValue::Bool(false),
                FieldType::Text => CellValue::Text(String::new()),
                FieldType::Integer => CellValue::Integer(0),
                FieldType::Real => CellValue::Real(0.0),
                FieldType::Binary => CellValue::Binary(Vec::new()),
                FieldType::Decimal => CellValue::Decimal(DecimalValue::new(0, 0).unwrap()),
                FieldType::Date => CellValue::Date("2000-01-01".parse::<DataDate>().unwrap()),
                FieldType::Time => CellValue::Time("00:00:00".parse::<DataTime>().unwrap()),
            },
        }
    };
    field.validate(&value).map_err(|e| invalid(e.to_string()))?;
    checked_format(recipe, field, &value)?;
    Ok(value)
}
pub(super) fn check_recipe(ui: &Controls, p: &Property) -> Result<(), ControlError> {
    if p.specialized.is_some() {
        return if ui.controls.contains_key(&p.owner) && ui.controls.contains_key(&p.editor) {
            Ok(())
        } else {
            Err(invalid("Property editor was removed"))
        };
    }
    let changed = match &p.recipe {
        DataFormEditorFactory::Numeric(s) => ui.numeric_settings(p.owner) != Some(s),
        DataFormEditorFactory::Date(s) => ui.date_settings(p.owner) != Some(s),
        DataFormEditorFactory::Time(s) => ui.time_settings(p.owner) != Some(s),
        DataFormEditorFactory::CheckBox => ui.check_state(p.owner).is_none(),
        DataFormEditorFactory::Text(_) => ui.document(p.editor).is_none(),
    };
    if changed || ui.editor_mask_settings(p.editor).is_some() {
        return Err(invalid("Property editor recipe was replaced"));
    }
    Ok(())
}
pub(super) fn validate_choice(choices: &[String], value: &CellValue) -> Result<(), ControlError> {
    if !choices.is_empty()
        && !matches!(value, CellValue::Null)
        && !matches!(value, CellValue::Text(v) if choices.contains(v))
    {
        return Err(invalid("Property value is outside its choices"));
    }
    Ok(())
}
pub(super) fn read(ui: &Controls, p: &Property) -> Result<CellValue, String> {
    let value = if p.null.is_some_and(|id| ui.checked(id) == Some(true)) {
        CellValue::Null
    } else if let Some(editor) = &p.specialized {
        editor.read(ui)?
    } else if !p.descriptor.choices.is_empty() {
        ui.selected_keys(p.owner)
            .and_then(|v| v.first())
            .cloned()
            .map(CellValue::Text)
            .unwrap_or(CellValue::Null)
    } else if matches!(p.recipe, DataFormEditorFactory::CheckBox) {
        match ui
            .check_state(p.owner)
            .ok_or("Property checkbox was removed")?
        {
            CheckState::Checked => CellValue::Bool(true),
            CheckState::Unchecked => CellValue::Bool(false),
            CheckState::Indeterminate => CellValue::Null,
        }
    } else {
        parse_value(
            &p.recipe,
            &p.schema,
            ui.document(p.editor)
                .ok_or("Property editor was removed")?
                .text(),
        )?
    };
    if p.specialized.is_none() {
        validate_choice(&p.descriptor.choices, &value).map_err(|e| e.to_string())?;
    }
    p.schema.validate(&value).map_err(|e| e.to_string())?;
    Ok(value)
}
pub(super) fn load(ui: &mut Controls, p: &Property, value: &CellValue) -> Result<(), ControlError> {
    p.schema
        .validate(value)
        .map_err(|e| invalid(e.to_string()))?;
    if let Some(editor) = &p.specialized {
        let family = p.family.as_ref().expect("mounted editor family");
        family.validate(&p.descriptor, &p.schema, value)?;
        editor.load(ui, value)?;
    } else if !p.descriptor.choices.is_empty() {
        validate_choice(&p.descriptor.choices, value)?;
        let keys = if let CellValue::Text(v) = value {
            std::slice::from_ref(v)
        } else {
            &[]
        };
        ui.set_selected_keys(p.owner, keys)?;
    } else if matches!(p.recipe, DataFormEditorFactory::CheckBox) {
        ui.set_check_state(
            p.owner,
            match value {
                CellValue::Bool(true) => CheckState::Checked,
                CellValue::Bool(false) => CheckState::Unchecked,
                _ => CheckState::Indeterminate,
            },
        )?;
    } else {
        let text = checked_format(&p.recipe, &p.schema, value)?;
        match ui.set_editor_value(p.editor, &text)? {
            CommitOutcome::Committed | CommitOutcome::Unchanged => {}
            CommitOutcome::Invalid(message) => return Err(invalid(message)),
            _ => {
                return Err(invalid(
                    "Property editor is awaiting validation or composition",
                ));
            }
        }
    }
    if let Some(null) = p.null {
        ui.set_checked(null, *value == CellValue::Null)?;
    }
    Ok(())
}
pub(super) fn dirty(ui: &Controls, p: &Property) -> bool {
    if !p.editing {
        return false;
    }
    if matches!(aggregate(&p.baseline), PropertyValue::Mixed) {
        return true;
    }
    read(ui, p).map_or(true, |v| {
        aggregate(&p.baseline) != PropertyValue::Uniform(v)
    }) || ui.is_edit_dirty(p.editor) == Some(true)
        || ui
            .document(p.editor)
            .is_some_and(|d| d.composition().is_some())
}
pub(super) fn present(ui: &mut Controls, p: &mut Property) -> Result<(), ControlError> {
    let writable = !p.descriptor.metadata.read_only;
    let label = p
        .descriptor
        .metadata
        .label
        .as_ref()
        .unwrap_or(&p.schema.title);
    let accepted = aggregate(&p.baseline);
    let mark = match (&accepted, p.descriptor.preview) {
        (PropertyValue::Uniform(CellValue::Text(raw)), PropertyPreview::Color) => raw
            .parse::<Srgba8>()
            .map_or(ValueMark::Plain, ValueMark::Swatch),
        (PropertyValue::Uniform(CellValue::Integer(v)), PropertyPreview::Progress)
            if (0..=100).contains(v) =>
        {
            ValueMark::Percent(*v as u8)
        }
        (_, kind) if matches!(p.recipe, DataFormEditorFactory::CheckBox) => {
            let state = match accepted {
                PropertyValue::Uniform(CellValue::Bool(true)) => CheckState::Checked,
                PropertyValue::Uniform(CellValue::Bool(false)) => CheckState::Unchecked,
                _ => CheckState::Indeterminate,
            };
            if kind == PropertyPreview::Switch {
                ValueMark::Switch(state)
            } else {
                ValueMark::Check(state)
            }
        }
        _ if !p.descriptor.choices.is_empty() => ValueMark::Choice,
        _ => ValueMark::Plain,
    };
    ui.controls.get_mut(&p.edit).unwrap().inspector_part = Some(InspectorPart::Value(mark));
    let preview = match accepted {
        PropertyValue::Uniform(ref amount @ (CellValue::Integer(_) | CellValue::Decimal(_)))
            if p.descriptor.currency_symbol.is_some() =>
        {
            let digits = amount.display();
            let precision = digits
                .split_once('.')
                .map_or(2, |(_, fraction)| fraction.len().max(2) as u8);
            let culture = ui
                .numeric_settings(p.owner)
                .map_or(crate::UiCulture::Russian, |settings| settings.culture);
            let formatted = super::super::numeric_model::format_decimal(
                &digits,
                culture,
                true,
                Some(precision),
            );
            format!(
                "{formatted} {}",
                p.descriptor.currency_symbol.as_deref().unwrap()
            )
        }
        PropertyValue::Uniform(CellValue::Integer(v)) if matches!(mark, ValueMark::Percent(_)) => {
            format!("{v}%")
        }
        PropertyValue::Mixed => "Разные значения".to_owned(),
        PropertyValue::Uniform(CellValue::Null) => "Не задано".to_owned(),
        PropertyValue::Uniform(CellValue::Bool(v)) => if v { "Да" } else { "Нет" }.to_owned(),
        PropertyValue::Uniform(v) => {
            if let Some(editor) = &p.specialized {
                editor.rest_label(&v).unwrap_or_else(|| {
                    if matches!(v, CellValue::Binary(_)) {
                        "Изображение".into()
                    } else {
                        v.display()
                    }
                })
            } else {
                checked_format(&p.recipe, &p.schema, &v)?
            }
        }
    };
    if let Some(label) = p.value_label
        && ui.controls.get(&label).is_some_and(|c| c.label != preview)
        && ui
            .button_content(p.edit)
            .is_some_and(|root| ui.flyout_descendant(label, root))
    {
        if p.measure_value_width {
            let width = ui
                .measure_styled_text(&preview, 16_384., ui.text_style(label).unwrap())?
                .width
                .ceil()
                + 16.;
            ui.edit_props(label, |props| props.style.width = Length::Px(width))?;
        }
        ui.set_label(label, &preview)?;
    }
    if ui.controls[&p.edit].label != preview || ui.controls[&p.edit].automation_name.is_none() {
        ui.set_automation_name(p.edit, format!("{label}: {preview}"))?;
        ui.set_label(p.edit, preview)?;
    }
    let inline = p.specialized.as_ref().is_some_and(|e| e.inline())
        && matches!(aggregate(&p.baseline), PropertyValue::Uniform(ref value) if *value != CellValue::Null);
    if let Some(editor) = &p.specialized {
        editor.set_active(ui, p.active)?;
    }
    show(ui, p.owner, p.active || inline)?;
    show(ui, p.edit, !p.active && !inline)?;
    show(ui, p.apply, p.active && writable)?;
    if let Some(unit) = p.currency {
        show(ui, unit, p.active)?;
    }
    show(ui, p.reset, p.descriptor.default.is_some() && writable)?;
    if let Some(null) = p.null {
        show(ui, null, p.active)?;
        enabled(ui, null, writable)?;
        let usable = writable
            || matches!(
                p.descriptor.metadata.editor.as_str(),
                "Hyperlink" | "ProgressBar"
            );
        enabled(ui, p.owner, usable && ui.checked(null) != Some(true))?;
    }
    enabled(ui, p.edit, writable || !p.descriptor.description.is_empty())?;
    enabled(ui, p.apply, p.editing && writable)?;
    enabled(ui, p.reset, p.descriptor.default.is_some() && writable)?;
    Ok(())
}

fn enabled(ui: &mut Controls, id: WidgetId, value: bool) -> Result<(), ControlError> {
    if ui.controls[&id].enabled != value {
        ui.set_enabled(id, value)?;
    }
    Ok(())
}
