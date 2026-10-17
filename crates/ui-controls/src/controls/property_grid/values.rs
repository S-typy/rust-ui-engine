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
pub(super) fn read(ui: &Controls, p: &Property) -> Result<CellValue, String> {
    let value = if p.null.is_some_and(|id| ui.checked(id) == Some(true)) {
        CellValue::Null
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
    p.schema.validate(&value).map_err(|e| e.to_string())?;
    Ok(value)
}
pub(super) fn load(ui: &mut Controls, p: &Property, value: &CellValue) -> Result<(), ControlError> {
    p.schema
        .validate(value)
        .map_err(|e| invalid(e.to_string()))?;
    let text = checked_format(&p.recipe, &p.schema, value)?;
    if matches!(p.recipe, DataFormEditorFactory::CheckBox) {
        ui.set_check_state(
            p.owner,
            match value {
                CellValue::Bool(true) => CheckState::Checked,
                CellValue::Bool(false) => CheckState::Unchecked,
                _ => CheckState::Indeterminate,
            },
        )?;
    } else {
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
    let mixed = !p.editing;
    for (id, visible) in [(p.owner, !mixed), (p.mixed, mixed), (p.edit, mixed)] {
        let props = &ui
            .runtime
            .tree()
            .node(id)
            .ok_or_else(|| invalid("Property presentation was removed"))?
            .props;
        if props.visible != visible || props.collapsed == visible {
            ui.edit_props(id, |n| {
                n.visible = visible;
                n.collapsed = !visible;
            })?;
        }
    }
    if let Some(null) = p.null {
        ui.edit_props(null, |n| {
            n.visible = !mixed;
            n.collapsed = mixed;
        })?;
        ui.set_enabled(null, !p.descriptor.metadata.read_only)?;
        ui.set_enabled(p.owner, ui.checked(null) != Some(true))?;
    }
    ui.set_enabled(p.edit, !p.descriptor.metadata.read_only)?;
    ui.set_enabled(p.apply, !mixed && !p.descriptor.metadata.read_only)?;
    ui.set_enabled(
        p.reset,
        p.descriptor.default.is_some() && !p.descriptor.metadata.read_only,
    )?;
    Ok(())
}
