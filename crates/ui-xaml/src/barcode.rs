//! Read-only linear barcode definitions.
use crate::*;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BarcodeSymbologyDefinition {
    #[default]
    Ean13,
    Ean8,
    Code128B,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BarcodeDefinition {
    pub symbology: BarcodeSymbologyDefinition,
    pub value: String,
    pub module_size: u16,
    pub bar_height: u32,
    pub show_text: bool,
}
impl Default for BarcodeDefinition {
    fn default() -> Self {
        Self {
            symbology: Default::default(),
            value: String::new(),
            module_size: 2,
            bar_height: 96,
            show_text: true,
        }
    }
}
pub fn validate_barcode(
    value: &BarcodeDefinition,
    common: &CommonProperties,
) -> Result<(), String> {
    if !(1..=16).contains(&value.module_size) || !(16..=1024).contains(&value.bar_height) {
        return Err("Barcode requires ModuleSize 1..16 and BarHeight 16..1024".into());
    }
    for (property, binding) in &common.bindings {
        if !badges::binding_property(property)
            && !matches!(
                property.as_str(),
                "Value" | "IsEnabled" | "Visibility" | "Width" | "Height"
            )
        {
            return Err("Barcode supports Value and common layout bindings".into());
        }
        if property == "Value" {
            if !matches!(binding.mode, BindingMode::OneWay | BindingMode::OneTime) {
                return Err("Barcode Value supports OneWay or OneTime".into());
            }
            if let Some(fallback) = &binding.fallback_value {
                validate_value(value.symbology, fallback)?;
            }
        }
    }
    if !common.bindings.contains_key("Value") {
        validate_value(value.symbology, &value.value)?;
    }
    Ok(())
}
fn validate_value(kind: BarcodeSymbologyDefinition, value: &str) -> Result<(), String> {
    match kind {
        BarcodeSymbologyDefinition::Code128B => {
            if value.is_empty()
                || value.len() > 128
                || !value.bytes().all(|b| (32..=126).contains(&b))
            {
                return Err("Code128B requires 1..128 printable ASCII characters".into());
            }
        }
        _ => {
            let length = if kind == BarcodeSymbologyDefinition::Ean13 {
                13
            } else {
                8
            };
            if ![length - 1, length].contains(&value.len())
                || !value.bytes().all(|b| b.is_ascii_digit())
            {
                return Err("EAN requires 7/8 or 12/13 ASCII digits for its format".into());
            }
            if value.len() == length {
                let sum: u32 = value.as_bytes()[..length - 1]
                    .iter()
                    .rev()
                    .enumerate()
                    .map(|(i, b)| u32::from(b - b'0') * if i % 2 == 0 { 3 } else { 1 })
                    .sum();
                if u32::from(value.as_bytes()[length - 1] - b'0') != (10 - sum % 10) % 10 {
                    return Err("EAN check digit does not match".into());
                }
            }
        }
    }
    Ok(())
}
pub(crate) fn property(
    document: &Document<'_>,
    attr: Attribute<'_, '_>,
    value: &mut BarcodeDefinition,
) -> Result<bool, Diagnostic> {
    match attr.name() {
        "Symbology" => {
            value.symbology = match attr.value() {
                "Ean13" => BarcodeSymbologyDefinition::Ean13,
                "Ean8" => BarcodeSymbologyDefinition::Ean8,
                "Code128B" => BarcodeSymbologyDefinition::Code128B,
                _ => {
                    return Err(at_attribute(
                        document,
                        attr,
                        "Symbology must be Ean13, Ean8 or Code128B",
                    ));
                }
            }
        }
        "Value" => value.value = attr.value().into(),
        "ModuleSize" => {
            value.module_size = attr
                .value()
                .parse()
                .map_err(|_| at_attribute(document, attr, "ModuleSize requires an integer"))?
        }
        "BarHeight" => {
            value.bar_height = attr
                .value()
                .parse()
                .map_err(|_| at_attribute(document, attr, "BarHeight requires an integer"))?
        }
        "ShowText" => value.show_text = boolean(document, attr)?,
        _ => return Ok(false),
    }
    Ok(true)
}
pub(crate) fn code(value: &BarcodeDefinition) -> String {
    format!(
        "rust_desktop_ui_xaml::BarcodeDefinition {{symbology:rust_desktop_ui_xaml::BarcodeSymbologyDefinition::{:?},value:{:?}.into(),module_size:{},bar_height:{},show_text:{}}}",
        value.symbology, value.value, value.module_size, value.bar_height, value.show_text
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    fn window(body: &str) -> Result<WindowDefinition, Diagnostic> {
        parse(&format!(
            "<Window xmlns='urn:rust-ui-engine:ui'>{body}</Window>"
        ))
    }
    #[test]
    fn barcode_literals_binding_defaults_and_codegen() {
        for (kind, data) in [
            ("Ean13", "4006381333931"),
            ("Ean8", "9638507"),
            ("Code128B", "Rust 2026"),
        ] {
            let w = window(&format!(
                "<Barcode Symbology='{kind}' Value='{data}' ModuleSize='3' ShowText='False'/>"
            ))
            .unwrap();
            let ElementKind::Barcode(value) = w.content.kind else {
                panic!()
            };
            assert!(!value.show_text);
            assert!(code(&value).contains("BarcodeDefinition"));
        }
        let w = window("<Barcode Value='{Binding Code}'/>").unwrap();
        assert_eq!(w.content.common.bindings["Value"].mode, BindingMode::OneWay);
    }
    #[test]
    fn barcode_invalid_input_modes_and_children_rejected() {
        for body in [
            "<Barcode/>",
            "<Barcode Value='4006381333932'/>",
            "<Barcode Value='400638133393' ModuleSize='0'/>",
            "<Barcode Value='400638133393' BarHeight='15'/>",
            "<Barcode Value='{Binding Code, Mode=TwoWay}'/>",
            "<Barcode Value='{Binding Code, FallbackValue=wrong}'/>",
            "<Barcode Symbology='Code128B' Value='é'/>",
            "<Barcode Value='400638133393'><Button Content='bad'/></Barcode>",
        ] {
            assert!(window(body).is_err(), "{body}");
        }
    }
}
