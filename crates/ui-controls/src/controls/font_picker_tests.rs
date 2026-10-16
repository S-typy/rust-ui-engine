use super::*;
#[test]
fn font_picker_installed_and_missing_requested_fonts() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let family = ui
        .font_families()
        .unwrap()
        .iter()
        .find(|name| name.as_str() == "Arial")
        .unwrap()
        .clone();
    let face = ui.font_faces(&family).unwrap()[0].clone();
    let value = face.descriptor(&family, 18.0);
    let p = ui
        .font_picker(
            ui.root(),
            "Font",
            value.clone(),
            FontPickerSettings::default(),
            LayoutStyle {
                width: Length::Px(620.0),
                ..Default::default()
            },
        )
        .unwrap();
    ui.update(Size::new(800.0, 600.0)).unwrap();
    assert_eq!(ui.font_value(p.container), Some(&value));
    assert!(ui.font_resolution(p.container).unwrap().family_available);
    let missing = FontDescriptor {
        family: "Missing Font 73945".into(),
        ..value
    };
    ui.set_font_value(p.container, missing.clone()).unwrap();
    ui.update(Size::new(800.0, 600.0)).unwrap();
    assert_eq!(ui.font_value(p.container), Some(&missing));
    assert!(!ui.font_resolution(p.container).unwrap().family_available);
    assert_eq!(
        ui.editor_settings(p.family.editor).unwrap().placeholder,
        missing.family
    );
    assert!(
        !ui.font_resolution(p.container)
            .unwrap()
            .actual_fonts
            .is_empty()
    );
    assert!(
        !ui.selection_items(p.family.container)
            .unwrap()
            .iter()
            .any(|item| item.key == missing.family)
    );
}

fn setup() -> (Controls, FontPicker) {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let p = ui
        .font_picker(
            ui.root(),
            "Font",
            FontDescriptor::default(),
            FontPickerSettings::default(),
            LayoutStyle {
                width: Length::Px(620.0),
                ..Default::default()
            },
        )
        .unwrap();
    ui.update(Size::new(800.0, 600.0)).unwrap();
    (ui, p)
}
#[test]
fn invalid_descriptor_and_rejected_selection_preserve_value_preview_and_event_count() {
    let (mut ui, p) = setup();
    let before = ui.font_value(p.container).unwrap().clone();
    let status = ui.label_text(p.status).unwrap().to_owned();
    ui.set_font_validator(
        p.container,
        Some(FontValidator::new(|font| {
            if font.size > 24.0 {
                Err("Maximum 24".into())
            } else {
                Ok(())
            }
        })),
    )
    .unwrap();
    for value in [
        FontDescriptor {
            size: f32::NAN,
            ..before.clone()
        },
        FontDescriptor {
            width: f32::INFINITY,
            ..before.clone()
        },
        FontDescriptor {
            size: 32.0,
            ..before.clone()
        },
    ] {
        assert!(ui.set_font_value(p.container, value).is_err());
        assert_eq!(ui.font_value(p.container), Some(&before));
        assert_eq!(ui.label_text(p.status), Some(status.as_str()));
    }
    assert!(ui.take_font_changes().is_empty());
    ui.set_spin_value(p.size.container, 32.0).unwrap();
    ui.update(Size::new(800.0, 600.0)).unwrap();
    assert_eq!(ui.font_value(p.container), Some(&before));
    assert_eq!(
        ui.spin_value(p.size.container),
        Some(f64::from(before.size))
    );
    assert!(ui.take_font_changes().is_empty());
    assert!(ui.font_error(p.container).is_some());
    ui.set_spin_value(p.size.container, 22.0).unwrap();
    ui.update(Size::new(800.0, 600.0)).unwrap();
    assert_eq!(ui.font_value(p.container).unwrap().size, 22.0);
    assert_eq!(ui.take_font_changes().len(), 1);
    ui.update(Size::new(800.0, 600.0)).unwrap();
    assert!(ui.take_font_changes().is_empty());
}
#[test]
fn readonly_required_parts_and_invalid_size_draft_keep_owned_state() {
    let (mut ui, p) = setup();
    ui.set_font_read_only(p.container, true).unwrap();
    for part in [&p.family, &p.face, &p.size] {
        assert_eq!(ui.composite_read_only(part.container), Some(true));
        assert_eq!(ui.is_read_only(part.editor), Some(true));
    }
    let before = ui.runtime.tree().live_ids();
    for id in [
        p.fields,
        p.family.container,
        p.face.container,
        p.size.buttons[0],
        p.preview,
    ] {
        assert!(ui.remove(id).is_err());
        assert_eq!(ui.runtime.tree().live_ids(), before);
    }
    ui.set_font_read_only(p.container, false).unwrap();
    let original = ui.font_value(p.container).unwrap().clone();
    ui.set_text(p.size.editor, "invalid").unwrap();
    assert!(matches!(
        ui.commit_edit(p.size.editor).unwrap(),
        CommitOutcome::Invalid(_)
    ));
    ui.update(Size::new(800.0, 600.0)).unwrap();
    assert_eq!(ui.document(p.size.editor).unwrap().text(), "invalid");
    assert_eq!(ui.font_value(p.container), Some(&original));
    ui.remove(p.container).unwrap();
    ui.update(Size::new(800.0, 600.0)).unwrap();
    assert!(ui.font_value(p.container).is_none());
    assert!(ui.take_font_changes().is_empty());
}
#[test]
fn family_face_and_size_are_real_retained_preview_inputs() {
    let (mut ui, p) = setup();
    let family = ui
        .font_families()
        .unwrap()
        .iter()
        .find(|f| f.as_str() == "Arial")
        .unwrap()
        .clone();
    ui.set_selected_keys(p.family.container, std::slice::from_ref(&family))
        .unwrap();
    ui.update(Size::new(800.0, 600.0)).unwrap();
    assert_eq!(ui.font_value(p.container).unwrap().family, family);
    let faces = ui.font_faces(&family).unwrap();
    let i = faces
        .iter()
        .position(|f| f.slant == FontSlant::Italic)
        .unwrap();
    ui.set_selected_keys(p.face.container, &[i.to_string()])
        .unwrap();
    ui.update(Size::new(800.0, 600.0)).unwrap();
    assert_eq!(ui.font_value(p.container).unwrap().slant, FontSlant::Italic);
    assert_eq!(ui.font_parts(p.container).unwrap(), &p);
    assert!(
        ui.font_resolution(p.container)
            .unwrap()
            .actual_fonts
            .iter()
            .any(|name| name.to_lowercase().contains("italic"))
    );
    let value = ui.font_value(p.container).unwrap().clone();
    let data = font_descriptor_to_value(&value);
    assert_eq!(font_descriptor_from_value(&data).unwrap(), value);
    let crate::DataValue::Object(mut fields) = data else {
        panic!()
    };
    fields.insert("width".into(), crate::DataValue::Number(f64::NAN));
    assert!(font_descriptor_from_value(&crate::DataValue::Object(fields)).is_err());
}
#[test]
fn descriptor_binding_rejects_before_preview_or_model_mutation() {
    use crate::{
        BindingContext, BindingProperty, BindingValidator, ControlBinding, ControlBindingMode,
        ControlBindings, DataValue,
    };
    use std::{cell::RefCell, rc::Rc};
    let (mut ui, p) = setup();
    let initial = ui.font_value(p.container).unwrap().clone();
    let model = Rc::new(RefCell::new(
        BindingContext::new(DataValue::object([(
            "Font",
            font_descriptor_to_value(&initial),
        )]))
        .unwrap(),
    ));
    model
        .borrow_mut()
        .set_field_validator(
            "Font",
            Some(BindingValidator::new(|v| {
                if font_descriptor_from_value(v)?.size > 24.0 {
                    Err("Too large".into())
                } else {
                    Ok(())
                }
            })),
        )
        .unwrap();
    let mut binding = ControlBinding::new(p.container, BindingProperty::Value, "Font").unwrap();
    binding.mode = ControlBindingMode::TwoWay;
    let mut bindings = ControlBindings::new(Rc::clone(&model));
    bindings.add(binding).unwrap();
    bindings.initialize(&mut ui).unwrap();
    assert!(
        ui.set_font_value(
            p.container,
            FontDescriptor {
                size: 32.0,
                ..initial.clone()
            }
        )
        .is_err()
    );
    assert_eq!(ui.font_value(p.container), Some(&initial));
    assert_eq!(
        model.borrow().get("Font").unwrap(),
        &font_descriptor_to_value(&initial)
    );
    let next = FontDescriptor {
        size: 23.0,
        ..initial
    };
    ui.set_font_value(p.container, next.clone()).unwrap();
    bindings.sync(&mut ui);
    assert_eq!(
        model.borrow().get("Font").unwrap(),
        &font_descriptor_to_value(&next)
    );
}

#[test]
fn owned_size_recipe_and_binding_angle_cannot_silently_change_accepted_descriptor() {
    let (mut ui, p) = setup();
    let value = ui.font_value(p.container).unwrap().clone();
    let label = ui.label_text(p.status).unwrap().to_owned();
    ui.set_spin_settings(
        p.size.container,
        SpinBoxSettings {
            maximum: 20.0,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(
        ui.set_font_value(
            p.container,
            FontDescriptor {
                size: 24.0,
                ..value.clone()
            }
        )
        .is_err()
    );
    assert_eq!(ui.font_value(p.container), Some(&value));
    assert_eq!(ui.label_text(p.status), Some(label.as_str()));
    assert!(ui.take_font_changes().is_empty());
    let crate::DataValue::Object(mut data) = font_descriptor_to_value(&value) else {
        panic!()
    };
    data.insert("angle".into(), crate::DataValue::Number(1.0e-50));
    assert!(font_descriptor_from_value(&crate::DataValue::Object(data.clone())).is_err());
    data.insert("angle".into(), 0.0.into());
    data.insert("size".into(), crate::DataValue::Number(512.00000001));
    assert!(font_descriptor_from_value(&crate::DataValue::Object(data)).is_err());
}

#[test]
fn descriptor_update_during_composition_is_atomic_and_keeps_draft() {
    let (mut ui, p) = setup();
    let value = ui.font_value(p.container).unwrap().clone();
    ui.runtime
        .apply(Mutation::Focus(Some(p.size.editor)))
        .unwrap();
    ui.ime_preedit("23", Some((0, 2))).unwrap();
    let before = ui.document(p.size.editor).unwrap().composition().cloned();
    assert!(
        ui.set_font_value(
            p.container,
            FontDescriptor {
                size: 24.0,
                ..value.clone()
            }
        )
        .is_err()
    );
    ui.update(Size::new(800.0, 600.0)).unwrap();
    assert_eq!(
        ui.document(p.size.editor).unwrap().composition(),
        before.as_ref()
    );
    assert_eq!(ui.font_value(p.container), Some(&value));
    assert!(ui.take_font_changes().is_empty());
}
