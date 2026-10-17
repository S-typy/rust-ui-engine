//! Exact nullable colors and bounded solid/linear brush definitions.
use crate::*;
pub use rust_desktop_ui_core::{BrushValue, GradientStop, LinearGradient, Srgba8};

#[derive(Clone, Debug, PartialEq)]
pub struct ColorEditorDefinition {
    pub value: Option<BrushValue>,
    pub allow_empty: bool,
    pub is_read_only: bool,
    pub palette: Option<Vec<Srgba8>>,
    pub recent_limit: usize,
    pub popup_width: f32,
    pub popup_height: f32,
}
impl Default for ColorEditorDefinition {
    fn default() -> Self {
        Self {
            value: None,
            allow_empty: true,
            is_read_only: false,
            palette: None,
            recent_limit: 16,
            popup_width: 520.0,
            popup_height: 640.0,
        }
    }
}
pub fn validate_color_editor(
    value: &ColorEditorDefinition,
    brush: bool,
    common: &CommonProperties,
) -> Result<(), String> {
    if !brush && matches!(value.value, Some(BrushValue::Linear(_))) {
        return Err("ColorEdit accepts only a solid color".into());
    }
    if value.value.is_none() && !value.allow_empty && !common.bindings.contains_key("Value") {
        return Err("Empty color requires AllowEmpty".into());
    }
    if value.palette.as_ref().is_some_and(|p| p.len() > 128)
        || value.recent_limit > 32
        || !value.popup_width.is_finite()
        || !(320.0..=1024.0).contains(&value.popup_width)
        || !value.popup_height.is_finite()
        || !(240.0..=1024.0).contains(&value.popup_height)
    {
        return Err("Color editor requires Palette <=128, RecentLimit <=32, PopupWidth 320..1024 and PopupHeight 240..1024".into());
    }
    if common.bindings.keys().any(|p| {
        !badges::binding_property(p)
            && !matches!(
                p.as_str(),
                "Value" | "IsReadOnly" | "IsEnabled" | "Visibility" | "Width" | "Height"
            )
    }) {
        return Err(
            "ColorEdit/BrushEdit supports Value, IsReadOnly and common layout bindings".into(),
        );
    }
    if let Some(fallback) = common
        .bindings
        .get("Value")
        .and_then(|b| b.fallback_value.as_ref())
    {
        let parsed = parse_color_literal(fallback)?;
        if parsed.is_none() && !value.allow_empty {
            return Err("Empty fallback requires AllowEmpty".into());
        }
    }
    Ok(())
}
pub fn parse_color_literal(text: &str) -> Result<Option<Srgba8>, String> {
    if matches!(text, "" | "null") {
        Ok(None)
    } else {
        text.parse()
            .map(Some)
            .map_err(|e: rust_desktop_ui_core::ColorValueError| e.to_string())
    }
}
pub(crate) fn property(
    document: &Document<'_>,
    attr: Attribute<'_, '_>,
    value: &mut ColorEditorDefinition,
) -> Result<bool, Diagnostic> {
    match attr.name() {
        "Value" => {
            value.value = parse_color_literal(attr.value())
                .map_err(|e| at_attribute(document, attr, e))?
                .map(BrushValue::Solid)
        }
        "AllowEmpty" => value.allow_empty = boolean(document, attr)?,
        "IsReadOnly" => value.is_read_only = boolean(document, attr)?,
        "RecentLimit" => {
            value.recent_limit = attr
                .value()
                .parse()
                .map_err(|_| at_attribute(document, attr, "RecentLimit requires 0..32"))?
        }
        "PopupWidth" => {
            value.popup_width = attr
                .value()
                .parse()
                .map_err(|_| at_attribute(document, attr, "Invalid PopupWidth"))?
        }
        "PopupHeight" => {
            value.popup_height = attr
                .value()
                .parse()
                .map_err(|_| at_attribute(document, attr, "Invalid PopupHeight"))?
        }
        "Palette" => {
            let mut colors = Vec::new();
            if !attr.value().is_empty() {
                for text in attr.value().split(';') {
                    if colors.len() >= 128 {
                        return Err(at_attribute(document, attr, "Palette exceeds 128 colors"));
                    }
                    colors.push(
                        text.parse::<Srgba8>()
                            .map_err(|e| at_attribute(document, attr, e.to_string()))?,
                    );
                }
            }
            value.palette = Some(colors);
        }
        _ => return Ok(false),
    }
    Ok(true)
}
pub(crate) fn children(
    node: Node<'_, '_>,
    definition: &mut ColorEditorDefinition,
    brush: bool,
    depth: usize,
    count: &mut usize,
) -> Result<(), Diagnostic> {
    let mut seen = false;
    for property in node.children() {
        if !property.is_element() {
            require_empty_content(property)?;
            continue;
        }
        if !brush
            || property.tag_name().namespace() != Some(UI_NAMESPACE)
            || property.tag_name().name() != "BrushEdit.Value"
            || property.attributes().len() != 0
            || seen
            || node.attribute("Value").is_some()
        {
            return Err(at_node(
                property,
                "BrushEdit.Value requires one unique brush and cannot combine with Value attribute/binding",
            ));
        }
        seen = true;
        let mut parsed = None;
        for child in property.children() {
            if !child.is_element() {
                require_empty_content(child)?;
                continue;
            }
            if parsed.is_some() || child.tag_name().namespace() != Some(UI_NAMESPACE) {
                return Err(at_node(child, "BrushEdit.Value requires exactly one brush"));
            }
            *count += 1;
            if *count > MAX_ELEMENTS || depth + 3 > MAX_ELEMENT_DEPTH {
                return Err(at_node(child, "Brush definition exceeds tree budget"));
            }
            parsed = Some(match child.tag_name().name() {
                "SolidColorBrush" => {
                    if child.attributes().len() != 1 {
                        return Err(at_node(child, "SolidColorBrush requires Color"));
                    }
                    let attr = child.attributes().next().unwrap();
                    if attr.name() != "Color" || attr.namespace().is_some() {
                        return Err(at_node(child, "SolidColorBrush supports only Color"));
                    }
                    let color = attr
                        .value()
                        .parse::<Srgba8>()
                        .map_err(|e| at_node(child, e.to_string()))?;
                    for n in child.children() {
                        require_empty_content(n)?;
                    }
                    BrushValue::Solid(color)
                }
                "LinearGradientBrush" => {
                    let mut angle = 0.0;
                    for attr in child.attributes() {
                        if attr.name() != "Angle" || attr.namespace().is_some() {
                            return Err(at_node(child, "LinearGradientBrush supports only Angle"));
                        }
                        angle = attr
                            .value()
                            .parse::<f64>()
                            .map_err(|_| at_node(child, "Invalid gradient angle"))?;
                    }
                    let mut stops = Vec::new();
                    for stop in child.children() {
                        if !stop.is_element() {
                            require_empty_content(stop)?;
                            continue;
                        }
                        if stop.tag_name().namespace() != Some(UI_NAMESPACE)
                            || stop.tag_name().name() != "GradientStop"
                            || stop.attributes().len() != 2
                            || stops.len() >= 32
                        {
                            return Err(at_node(
                                stop,
                                "LinearGradientBrush requires 2..32 GradientStop children",
                            ));
                        }
                        let mut offset = None;
                        let mut color = None;
                        for attr in stop.attributes() {
                            if attr.namespace().is_some() {
                                return Err(at_node(
                                    stop,
                                    "Unsupported GradientStop attribute namespace",
                                ));
                            }
                            match attr.name() {
                                "Offset" => {
                                    offset = Some(
                                        attr.value()
                                            .parse::<f64>()
                                            .map_err(|_| at_node(stop, "Invalid offset"))?,
                                    )
                                }
                                "Color" => {
                                    color = Some(
                                        attr.value()
                                            .parse::<Srgba8>()
                                            .map_err(|e| at_node(stop, e.to_string()))?,
                                    )
                                }
                                _ => {
                                    return Err(at_node(
                                        stop,
                                        "GradientStop supports Offset and Color",
                                    ));
                                }
                            }
                        }
                        for n in stop.children() {
                            require_empty_content(n)?;
                        }
                        *count += 1;
                        if *count > MAX_ELEMENTS {
                            return Err(at_node(stop, "Brush definition exceeds tree budget"));
                        }
                        stops.push(GradientStop::new(
                            offset.ok_or_else(|| at_node(stop, "Missing Offset"))?,
                            color.ok_or_else(|| at_node(stop, "Missing Color"))?,
                        ));
                    }
                    BrushValue::Linear(
                        LinearGradient::new(angle, stops)
                            .map_err(|e| at_node(child, e.to_string()))?,
                    )
                }
                _ => {
                    return Err(at_node(
                        child,
                        "Expected SolidColorBrush or LinearGradientBrush",
                    ));
                }
            });
        }
        definition.value =
            Some(parsed.ok_or_else(|| at_node(property, "BrushEdit.Value requires one brush"))?);
    }
    Ok(())
}
fn color_code(c: Srgba8) -> String {
    format!(
        "rust_desktop_ui_xaml::Srgba8::new({},{},{},{})",
        c.r, c.g, c.b, c.a
    )
}
pub(crate) fn code(value: &ColorEditorDefinition) -> String {
    let brush = match &value.value {
        None => "None".into(),
        Some(BrushValue::Solid(c)) => format!(
            "Some(rust_desktop_ui_xaml::BrushValue::Solid({}))",
            color_code(*c)
        ),
        Some(BrushValue::Linear(g)) => format!(
            "Some(rust_desktop_ui_xaml::BrushValue::Linear(rust_desktop_ui_xaml::LinearGradient::new({:?},vec![{}]).expect(\"validated gradient\")))",
            g.angle(),
            g.stops()
                .iter()
                .map(|s| format!(
                    "rust_desktop_ui_xaml::GradientStop::new({:?},{})",
                    s.offset,
                    color_code(s.color)
                ))
                .collect::<Vec<_>>()
                .join(",")
        ),
    };
    let palette = value.palette.as_ref().map_or("None".into(), |colors| {
        format!(
            "Some(vec![{}])",
            colors
                .iter()
                .map(|c| color_code(*c))
                .collect::<Vec<_>>()
                .join(",")
        )
    });
    format!(
        "rust_desktop_ui_xaml::ColorEditorDefinition{{value:{brush},allow_empty:{},is_read_only:{},palette:{palette},recent_limit:{},popup_width:{:?},popup_height:{:?}}}",
        value.allow_empty,
        value.is_read_only,
        value.recent_limit,
        value.popup_width,
        value.popup_height
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
    fn color_null_alpha_hex_palette_and_binding_use_exact_values() {
        let w =
            window("<ColorEdit Value='#00123456' Palette='#FFFFFF;#AA010203' RecentLimit='0'/>")
                .unwrap();
        let ElementKind::ColorEdit(value) = w.content.kind else {
            panic!()
        };
        assert_eq!(
            value.value,
            Some(BrushValue::Solid(Srgba8::new(18, 52, 86, 0)))
        );
        assert_eq!(value.palette.unwrap().len(), 2);
        let w = window(
            "<ColorEdit Value='{Binding Color, FallbackValue=#00123456}' AllowEmpty='False'/>",
        )
        .unwrap();
        assert_eq!(w.content.common.bindings["Value"].mode, BindingMode::TwoWay);
        assert_eq!(
            w.content.common.bindings["Value"].update_source_trigger,
            UpdateSourceTrigger::PropertyChanged
        );
        for attrs in [
            "Value='red'",
            "Value='#GG0000'",
            "Value='#12345'",
            "AllowEmpty='False'",
            "RecentLimit='33'",
            "PopupWidth='NaN'",
            "PopupHeight='239'",
            "Text='bad'",
            "Value='{Binding X, FallbackValue=bad}'",
        ] {
            assert!(window(&format!("<ColorEdit {attrs}/>")).is_err(), "{attrs}");
        }
    }
    #[test]
    fn gradient_children_preserve_hard_edges_and_reject_unsorted_or_invalid_tree() {
        let body = "<BrushEdit><BrushEdit.Value><LinearGradientBrush Angle='23.123456789'><GradientStop Offset='0' Color='#00112233'/><GradientStop Offset='0.5' Color='#000000'/><GradientStop Offset='0.5' Color='#FFFFFF'/><GradientStop Offset='1' Color='#FFFFFFFF'/></LinearGradientBrush></BrushEdit.Value></BrushEdit>";
        let w = window(body).unwrap();
        let ElementKind::BrushEdit(value) = w.content.kind else {
            panic!()
        };
        let Some(BrushValue::Linear(g)) = &value.value else {
            panic!()
        };
        assert_eq!(g.stops()[0].color.a, 0);
        assert_eq!(g.sample(0.5).unwrap(), Srgba8::WHITE);
        assert!(code(&value).contains("23.123456789"));
        for invalid in [
            body.replace("Angle='23.123456789'", "Angle='NaN'"),
            body.replacen("Offset='0.5'", "Offset='0.8'", 1),
            body.replace("<BrushEdit>", "<BrushEdit Value='{Binding B}'>"),
            body.replace(
                "GradientStop Offset='1'",
                "GradientStop Extra='x' Offset='1'",
            ),
            "<ColorEdit><SolidColorBrush Color='#FFFFFF'/></ColorEdit>".into(),
            "<BrushEdit><BrushEdit.Value/></BrushEdit>".into(),
        ] {
            assert!(window(&invalid).is_err(), "{invalid}");
        }
    }
}
