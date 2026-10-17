use super::*;
use crate::barcode::BarcodeSymbology;
use crate::{
    BindingContext, BindingProperty, ControlBinding, ControlBindingMode, ControlBindings, DataValue,
};
use std::{cell::RefCell, rc::Rc};
fn fixture() -> (Controls, WidgetId) {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let id = ui
        .barcode(
            ui.root(),
            "400638133393",
            BarcodeSettings::default(),
            LayoutStyle {
                flex_shrink: 0.0,
                ..Default::default()
            },
        )
        .unwrap();
    ui.update(Size::new(800.0, 500.0)).unwrap();
    (ui, id)
}
#[test]
fn barcode_atomic_replacement_auto_geometry_and_read_only_semantics() {
    let (mut ui, id) = fixture();
    assert_eq!(ui.barcode_value(id), Some("4006381333931"));
    assert_eq!(
        ui.barcode_display_status(id),
        Some(BarcodeDisplayStatus::Ready)
    );
    let bounds = ui.runtime.bounds(id);
    let nodes = ui.runtime.tree().len();
    assert!(ui.set_barcode_value(id, "4006381333932").is_err());
    assert!(
        ui.set_barcode(
            id,
            "123",
            BarcodeSettings {
                module_size: 0,
                ..Default::default()
            }
        )
        .is_err()
    );
    assert_eq!(ui.barcode_value(id), Some("4006381333931"));
    assert_eq!(ui.runtime.bounds(id), bounds);
    assert_eq!(ui.runtime.tree().len(), nodes);
    ui.set_barcode(
        id,
        "AB",
        BarcodeSettings {
            symbology: BarcodeSymbology::Code128B,
            ..Default::default()
        },
    )
    .unwrap();
    ui.update(Size::new(800.0, 500.0)).unwrap();
    assert_eq!(ui.runtime.bounds(id).unwrap().width, 154.0);
    let node = ui.semantics().into_iter().find(|s| s.id == id).unwrap();
    assert_eq!(node.role, Role::Image);
    assert!(node.read_only);
    assert!(node.actions.is_empty());
    assert_eq!(node.value.as_deref(), Some("AB"));
    ui.remove(id).unwrap();
    assert!(ui.barcode_value(id).is_none());
}
#[test]
fn barcode_insufficient_space_never_draws_a_partial_symbol() {
    let (mut ui, id) = fixture();
    let expected = ui.barcode_geometry(id).unwrap().bars().len();
    assert!(ui.scene().rectangles.len() >= expected);
    ui.edit_props(id, |p| p.style.width = Length::Px(100.0))
        .unwrap();
    ui.update(Size::new(800.0, 500.0)).unwrap();
    assert_eq!(
        ui.barcode_display_status(id),
        Some(BarcodeDisplayStatus::InsufficientSpace)
    );
    assert!(ui.scene().rectangles.len() < expected);
    ui.set_barcode_value(id, "590123412345").unwrap();
    ui.update(Size::new(800.0, 500.0)).unwrap();
    assert_eq!(ui.runtime.bounds(id).unwrap().width, 100.0);
}
#[test]
fn barcode_oneway_binding_rejects_invalid_updates_without_source_write() {
    let (mut ui, id) = fixture();
    let model = Rc::new(RefCell::new(
        BindingContext::new(DataValue::object([(
            "Code",
            DataValue::String("9638507".into()),
        )]))
        .unwrap(),
    ));
    ui.set_barcode(
        id,
        "9638507",
        BarcodeSettings {
            symbology: BarcodeSymbology::Ean8,
            ..Default::default()
        },
    )
    .unwrap();
    let mut bindings = ControlBindings::new(model.clone());
    bindings
        .add(ControlBinding::new(id, BindingProperty::Value, "Code").unwrap())
        .unwrap();
    bindings.initialize(&mut ui).unwrap();
    assert_eq!(ui.barcode_value(id), Some("96385074"));
    model
        .borrow_mut()
        .set("Code", DataValue::String("bad".into()))
        .unwrap();
    bindings.sync(&mut ui);
    assert_eq!(ui.barcode_value(id), Some("96385074"));
    assert!(!bindings.diagnostics().is_empty());
    let mut invalid = ControlBindings::new(model.clone());
    let mut binding = ControlBinding::new(id, BindingProperty::Value, "Code").unwrap();
    binding.mode = ControlBindingMode::TwoWay;
    invalid.add(binding).unwrap();
    assert!(invalid.initialize(&mut ui).is_err());
    assert_eq!(
        model.borrow().get("Code").unwrap(),
        &DataValue::String("bad".into())
    );
}
