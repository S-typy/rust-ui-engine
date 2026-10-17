//! Bounded accepted locations; application code resolves navigation requests.
use crate::*;
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BreadcrumbSegmentDefinition {
    pub key: String,
    pub label: String,
    pub has_children: bool,
}
#[derive(Clone, Debug, PartialEq)]
pub struct BreadcrumbDefinition {
    pub path: String,
    pub segments: Vec<BreadcrumbSegmentDefinition>,
    pub is_editable: bool,
    pub is_read_only: bool,
    pub history_limit: usize,
    pub popup_width: f32,
    pub popup_height: f32,
}
impl Default for BreadcrumbDefinition {
    fn default() -> Self {
        Self {
            path: String::new(),
            segments: vec![],
            is_editable: true,
            is_read_only: false,
            history_limit: 20,
            popup_width: 360.0,
            popup_height: 280.0,
        }
    }
}
pub fn validate_breadcrumb(
    value: &BreadcrumbDefinition,
    common: &CommonProperties,
) -> Result<(), String> {
    let line = |s: &str, g: usize, b: usize| {
        s.len() <= b
            && s.graphemes(true).count() <= g
            && !s.chars().any(char::is_control)
            && !s.contains(['\u{2028}', '\u{2029}'])
    };
    if !line(&value.path, 1024, 4096)
        || value.segments.len() > 64
        || value.history_limit > 100
        || !value.popup_width.is_finite()
        || !(160.0..=4096.0).contains(&value.popup_width)
        || !value.popup_height.is_finite()
        || !(80.0..=4096.0).contains(&value.popup_height)
    {
        return Err("Breadcrumb requires a bounded single-line path, at most 64 segments, history 0..100 and popup 160..4096 × 80..4096".into());
    }
    let mut keys = std::collections::HashSet::new();
    let mut bytes = 0;
    for segment in &value.segments {
        if segment.key.is_empty()
            || segment.key.len() > 128
            || segment.key.chars().any(char::is_control)
            || !keys.insert(&segment.key)
            || segment.label.trim().is_empty()
            || !line(&segment.label, 128, 512)
        {
            return Err(
                "Breadcrumb segments require unique keys and bounded nonempty labels".into(),
            );
        }
        bytes += segment.key.len() + segment.label.len();
    }
    if bytes > 16384 {
        return Err("Breadcrumb segment metadata exceeds 16 KiB".into());
    }
    if common.bindings.keys().any(|property| {
        !badges::binding_property(property)
            && !matches!(
                property.as_str(),
                "Value" | "IsReadOnly" | "IsEnabled" | "Visibility" | "Width" | "Height"
            )
    }) {
        return Err("Breadcrumb supports accepted Value, IsReadOnly and common bindings".into());
    }
    if common
        .bindings
        .get("Value")
        .is_some_and(|binding| binding.fallback_value.is_some())
    {
        return Err(
            "Breadcrumb Value is a typed object; string FallbackValue is unsupported".into(),
        );
    }
    Ok(())
}
pub(crate) fn property(
    document: &Document<'_>,
    attr: Attribute<'_, '_>,
    value: &mut BreadcrumbDefinition,
) -> Result<bool, Diagnostic> {
    match attr.name() {
        "Path" => value.path = attr.value().into(),
        "IsEditable" => value.is_editable = boolean(document, attr)?,
        "IsReadOnly" => value.is_read_only = boolean(document, attr)?,
        "HistoryLimit" => {
            value.history_limit = attr
                .value()
                .parse()
                .map_err(|_| at_attribute(document, attr, "HistoryLimit requires an integer"))?
        }
        "PopupWidth" => value.popup_width = editor_metric(document, attr, 4096.0, false)?,
        "PopupHeight" => value.popup_height = editor_metric(document, attr, 4096.0, false)?,
        _ => return Ok(false),
    }
    Ok(true)
}
pub(crate) fn parse_children(
    node: Node<'_, '_>,
    depth: usize,
    count: &mut usize,
    value: &mut BreadcrumbDefinition,
) -> Result<(), Diagnostic> {
    for child in node.children() {
        if !child.is_element() {
            require_empty_content(child)?;
            continue;
        }
        if child.tag_name().namespace() != Some(UI_NAMESPACE)
            || child.tag_name().name() != "BreadcrumbSegment"
        {
            return Err(at_node(
                child,
                "Breadcrumb accepts BreadcrumbSegment metadata",
            ));
        }
        *count += 1;
        if *count > MAX_ELEMENTS || depth + 1 > MAX_ELEMENT_DEPTH || value.segments.len() >= 64 {
            return Err(at_node(child, "Breadcrumb metadata count/depth exceeded"));
        }
        let mut segment = BreadcrumbSegmentDefinition::default();
        for attr in child.attributes() {
            if attr.namespace().is_some() {
                return Err(at_attribute(
                    node.document(),
                    attr,
                    "BreadcrumbSegment directives are unsupported",
                ));
            }
            match attr.name() {
                "Key" => segment.key = attr.value().into(),
                "Label" => segment.label = attr.value().into(),
                "HasChildren" => segment.has_children = boolean(node.document(), attr)?,
                _ => {
                    return Err(at_attribute(
                        node.document(),
                        attr,
                        "Unsupported BreadcrumbSegment attribute",
                    ));
                }
            }
        }
        for nested in child.children() {
            require_empty_content(nested)?;
        }
        value.segments.push(segment);
    }
    Ok(())
}
pub(crate) fn code(value: &BreadcrumbDefinition) -> String {
    let segments=value.segments.iter().map(|s|format!("rust_desktop_ui_xaml::BreadcrumbSegmentDefinition{{key:{:?}.into(),label:{:?}.into(),has_children:{}}}",s.key,s.label,s.has_children)).collect::<Vec<_>>().join(",");
    format!(
        "rust_desktop_ui_xaml::BreadcrumbDefinition{{path:{:?}.into(),segments:vec![{}],is_editable:{},is_read_only:{},history_limit:{},popup_width:{:?},popup_height:{:?}}}",
        value.path,
        segments,
        value.is_editable,
        value.is_read_only,
        value.history_limit,
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
    fn breadcrumb_metadata_binding_and_codegen_are_typed() {
        let window=window("<Breadcrumb Path='project:source' Value='{Binding Location}'><BreadcrumbSegment Key='root' Label='Проект' HasChildren='True'/><BreadcrumbSegment Key='src' Label='Исходники'/></Breadcrumb>").unwrap();
        let ElementKind::Breadcrumb(value) = &window.content.kind else {
            panic!()
        };
        assert_eq!(value.segments.len(), 2);
        assert!(value.segments[0].has_children);
        assert_eq!(
            window.content.common.bindings["Value"].mode,
            BindingMode::TwoWay
        );
        assert!(code(value).contains("BreadcrumbSegmentDefinition"));
    }
    #[test]
    fn breadcrumb_rejects_ambiguous_or_unbounded_metadata() {
        for body in [
            "<Breadcrumb HistoryLimit='101'/>",
            "<Breadcrumb PopupWidth='40'/>",
            "<Breadcrumb Value='root'/>",
            "<Breadcrumb Text='draft'/>",
            "<Breadcrumb Value='{Binding Location, FallbackValue=root}'/>",
            "<Breadcrumb><Button Content='x'/></Breadcrumb>",
            "<Breadcrumb><BreadcrumbSegment Key='x' Label='One'/><BreadcrumbSegment Key='x' Label='Two'/></Breadcrumb>",
            "<Breadcrumb><BreadcrumbSegment Key='x' Label=''/></Breadcrumb>",
        ] {
            assert!(window(body).is_err(), "{body}");
        }
    }
}
