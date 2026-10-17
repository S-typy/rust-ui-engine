//! Bounded descriptor literals and Value bindings for FontPicker.
use crate::*;
#[derive(Clone, Debug, PartialEq)]
pub struct FontPickerDefinition {
    pub family: String,
    pub size: f32,
    pub weight: u16,
    pub width: f32,
    pub style: String,
    pub angle: f32,
    pub preview_text: String,
    pub is_read_only: bool,
}
impl Default for FontPickerDefinition {
    fn default() -> Self {
        Self {
            family: "Segoe UI".into(),
            size: 14.0,
            weight: 400,
            width: 100.0,
            style: "Normal".into(),
            angle: 0.0,
            preview_text: "Aa Бб 123 · Текст / Preview".into(),
            is_read_only: false,
        }
    }
}
pub fn validate_font_picker(
    font: &FontPickerDefinition,
    common: &CommonProperties,
) -> Result<(), String> {
    if font.family.trim().is_empty()
        || font.family.len() > 256
        || font.family.chars().any(char::is_control)
        || !font.size.is_finite()
        || !(1.0..=512.0).contains(&font.size)
        || !(1..=1000).contains(&font.weight)
        || !font.width.is_finite()
        || !(25.0..=400.0).contains(&font.width)
        || !font.angle.is_finite()
        || !(-45.0..=45.0).contains(&font.angle)
        || !matches!(font.style.as_str(), "Normal" | "Italic" | "Oblique")
        || font.style != "Oblique" && font.angle != 0.0
        || font.preview_text.is_empty()
        || font.preview_text.len() > 4096
        || font.preview_text.contains('\0')
    {
        return Err("FontPicker requires a bounded family, size 1..512, weight 1..1000, width 25..400, Normal/Italic or Oblique angle -45..45 and preview text 1..4096 bytes".into());
    }
    if common.bindings.keys().any(|property| {
        !crate::badges::binding_property(property)
            && !matches!(
                property.as_str(),
                "Value" | "IsReadOnly" | "IsEnabled" | "Visibility" | "Width" | "Height"
            )
    }) {
        return Err(
            "FontPicker supports Value, IsReadOnly and common geometry/visibility bindings".into(),
        );
    }
    if common
        .bindings
        .get("Value")
        .is_some_and(|b| b.fallback_value.is_some())
    {
        return Err(
            "FontPicker Value has a typed object; string FallbackValue is unsupported".into(),
        );
    }
    Ok(())
}
pub(crate) fn property(
    document: &Document<'_>,
    attr: Attribute<'_, '_>,
    font: &mut FontPickerDefinition,
) -> Result<bool, Diagnostic> {
    match attr.name() {
        "FontFamily" => font.family = attr.value().into(),
        "FontSize" => font.size = editor_metric(document, attr, 512.0, false)?,
        "FontWeight" => {
            font.weight = attr
                .value()
                .parse()
                .map_err(|_| at_attribute(document, attr, "FontWeight requires u16"))?
        }
        "FontWidth" => font.width = editor_metric(document, attr, 400.0, false)?,
        "FontStyle" => font.style = attr.value().into(),
        "FontAngle" => {
            font.angle = attr
                .value()
                .parse()
                .map_err(|_| at_attribute(document, attr, "FontAngle requires a finite angle"))?
        }
        "PreviewText" => font.preview_text = attr.value().into(),
        "IsReadOnly" => font.is_read_only = boolean(document, attr)?,
        _ => return Ok(false),
    }
    Ok(true)
}
pub(crate) fn code(font: &FontPickerDefinition) -> String {
    format!(
        "rust_desktop_ui_xaml::FontPickerDefinition {{ family:{:?}.into(),size:{:?},weight:{},width:{:?},style:{:?}.into(),angle:{:?},preview_text:{:?}.into(),is_read_only:{} }}",
        font.family,
        font.size,
        font.weight,
        font.width,
        font.style,
        font.angle,
        font.preview_text,
        font.is_read_only
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
    fn font_literals_and_binding_profile_are_bounded() {
        let parsed=window("<FontPicker FontFamily='Arial' FontSize='18.5' FontStyle='Oblique' FontAngle='12' Value='{Binding Font}'/>").unwrap();
        let ElementKind::FontPicker(font) = &parsed.content.kind else {
            panic!()
        };
        assert_eq!(font.size, 18.5);
        assert!(code(font).contains("FontPickerDefinition"));
        assert_eq!(
            parsed.content.common.bindings["Value"].mode,
            BindingMode::TwoWay
        );
        for attr in [
            "FontSize='NaN'",
            "FontWidth='Infinity'",
            "FontWeight='0'",
            "FontStyle='Bold'",
            "FontAngle='12'",
            "PreviewText=''",
            "Text='secret'",
            "Value='Arial'",
            "Value='{Binding Font, FallbackValue=Arial}'",
        ] {
            assert!(window(&format!("<FontPicker {attr}/>")).is_err(), "{attr}");
        }
        assert!(window("<FontPicker><TextBlock Text='x'/></FontPicker>").is_err());
    }
}
