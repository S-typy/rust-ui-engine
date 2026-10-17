use super::*;
use rust_desktop_ui_controls::{BarcodeSettings, barcode::BarcodeSymbology};
use rust_desktop_ui_xaml::{BarcodeDefinition, BarcodeSymbologyDefinition};
pub(super) fn settings(value: &BarcodeDefinition) -> BarcodeSettings {
    BarcodeSettings {
        symbology: match value.symbology {
            BarcodeSymbologyDefinition::Ean13 => BarcodeSymbology::Ean13,
            BarcodeSymbologyDefinition::Ean8 => BarcodeSymbology::Ean8,
            BarcodeSymbologyDefinition::Code128B => BarcodeSymbology::Code128B,
        },
        module_size: value.module_size,
        bar_height: value.bar_height,
        show_text: value.show_text,
    }
}
pub(super) fn initial(value: &BarcodeDefinition, bound: bool) -> &str {
    if bound {
        match value.symbology {
            BarcodeSymbologyDefinition::Ean13 => "000000000000",
            BarcodeSymbologyDefinition::Ean8 => "0000000",
            BarcodeSymbologyDefinition::Code128B => " ",
        }
    } else {
        &value.value
    }
}
pub(super) fn width(value: &BarcodeDefinition, bound: bool) -> Result<f32, String> {
    let settings = settings(value);
    let symbol = settings
        .validate_value(initial(value, bound))
        .map_err(err)?;
    Ok(f32::from(symbol.total_modules()) * f32::from(settings.module_size))
}
#[cfg(test)]
use crate::barcode_model as demo;
#[cfg(test)]
mod tests {
    use super::*;
    use rust_desktop_ui_controls::{BarcodeDisplayStatus, BindingContext, DataValue, Theme};
    use rust_desktop_ui_core::Size;
    use std::{cell::RefCell, rc::Rc};
    fn content(body: &str) -> ElementDefinition {
        rust_desktop_ui_xaml::parse(&format!("<Window xmlns='urn:rust-ui-engine:ui' xmlns:x='http://schemas.microsoft.com/winfx/2006/xaml'>{body}</Window>")).unwrap().content
    }
    #[test]
    fn barcode_compiled_binding_fallback_and_invalid_initial_are_atomic() {
        let mut ui = Controls::new(Theme::FluentDark).unwrap();
        let root = ui.root();
        let model = Rc::new(RefCell::new(
            BindingContext::new(DataValue::object([(
                "Code",
                DataValue::from("400638133393"),
            )]))
            .unwrap(),
        ));
        let definition = content("<Barcode x:Name='Code' Value='{Binding Code}'/>");
        let mut view =
            MountedView::mount_with_context(&mut ui, root, &definition, model.clone()).unwrap();
        let id = view.names["Code"];
        ui.update(Size::new(800.0, 600.0)).unwrap();
        assert_eq!(ui.barcode_value(id), Some("4006381333931"));
        assert_eq!(
            ui.barcode_display_status(id),
            Some(BarcodeDisplayStatus::Ready)
        );
        model
            .borrow_mut()
            .set("Code", DataValue::from("bad"))
            .unwrap();
        view.sync_bindings(&mut ui);
        assert_eq!(ui.barcode_value(id), Some("4006381333931"));
        let ids = ui.runtime.tree().live_ids();
        assert!(MountedView::mount_with_context(&mut ui, root, &definition, model).is_err());
        assert_eq!(ui.runtime.tree().live_ids(), ids);
        let fallback =
            content("<Barcode Symbology='Ean8' Value='{Binding Missing, FallbackValue=9638507}'/>");
        assert!(MountedView::mount(&mut ui, root, &fallback).is_ok());
    }
    #[test]
    fn barcode_demo_has_complete_symbol_geometry_in_both_themes() {
        let definition =
            rust_desktop_ui_xaml::parse(include_str!("../BarcodeWindow.xaml")).unwrap();
        for theme in [Theme::FluentLight, Theme::FluentDark] {
            let mut ui = Controls::new(theme).unwrap();
            demo::register(&mut ui);
            let root = ui.root();
            let view = MountedView::mount(&mut ui, root, &definition.content).unwrap();
            ui.update(Size::new(1000.0, 800.0)).unwrap();
            for name in ["BarcodeEan13", "BarcodeEan8", "BarcodeAscii"] {
                assert_eq!(
                    ui.barcode_display_status(view.names[name]),
                    Some(BarcodeDisplayStatus::Ready),
                    "{name}"
                );
            }
            let mut state = demo::State::install(&mut ui, &view).unwrap().unwrap();
            state.command(&mut ui, "BarcodeChange").unwrap();
            assert_eq!(
                ui.barcode_value(view.names["BarcodeEan13"]),
                Some("5901234123457")
            );
            state.command(&mut ui, "BarcodeReject").unwrap();
            assert_eq!(
                ui.barcode_value(view.names["BarcodeEan13"]),
                Some("5901234123457")
            );
        }
    }
}
