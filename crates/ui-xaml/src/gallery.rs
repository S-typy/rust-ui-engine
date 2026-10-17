//! Grouped visual samples with accepted-key bindings and readonly templates.
use crate::*;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GalleryGroupDefinition {
    pub key: String,
    pub header: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GalleryItemDefinition {
    pub key: String,
    pub text: String,
    pub group: String,
    pub icon: Option<IconSymbol>,
    pub image_resource: Option<String>,
    pub is_enabled: bool,
    pub columns: Vec<String>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct GalleryDefinition {
    pub groups: Vec<GalleryGroupDefinition>,
    pub items: Vec<GalleryItemDefinition>,
    pub selected_key: Option<String>,
    pub is_read_only: bool,
    pub sample_width: f32,
    pub sample_height: f32,
    pub gap: f32,
    pub group_header_height: f32,
    pub item_template: Option<Box<ElementDefinition>>,
}
impl Default for GalleryDefinition {
    fn default() -> Self {
        Self {
            groups: Vec::new(),
            items: Vec::new(),
            selected_key: None,
            is_read_only: false,
            sample_width: 112.0,
            sample_height: 96.0,
            gap: 8.0,
            group_header_height: 28.0,
            item_template: None,
        }
    }
}
fn key(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"_.-/".contains(&c))
}
pub fn validate_gallery(
    value: &GalleryDefinition,
    common: &CommonProperties,
) -> Result<(), String> {
    for (v, min, max) in [
        (value.sample_width, 32.0, 1024.0),
        (value.sample_height, 32.0, 1024.0),
        (value.gap, 0.0, 64.0),
        (value.group_header_height, 16.0, 128.0),
    ] {
        if !v.is_finite() || !(min..=max).contains(&v) {
            return Err("Invalid bounded Gallery dimension".into());
        }
    }
    if value.items.len() > 256 || value.groups.len() > 32 {
        return Err("Gallery supports at most 256 samples and 32 groups".into());
    }
    let mut groups = HashSet::new();
    let mut keys = HashSet::new();
    let mut bytes = 0usize;
    let text = |v: &str| -> Result<(), String> {
        if v.len() > 4096 || v.chars().any(char::is_control) {
            Err("Gallery metadata requires bounded printable text".into())
        } else {
            Ok(())
        }
    };
    for group in &value.groups {
        if !key(&group.key) || !groups.insert(group.key.as_str()) {
            return Err("Gallery group keys must be unique ASCII identifiers".into());
        }
        text(&group.header)?;
        bytes += group.key.len() + group.header.len();
    }
    for item in &value.items {
        if !key(&item.key) || !keys.insert(item.key.as_str()) {
            return Err("Gallery sample keys must be unique ASCII identifiers".into());
        }
        if !item.group.is_empty() && !groups.contains(item.group.as_str()) {
            return Err("Unknown Gallery group".into());
        }
        if item.icon.is_some() && item.image_resource.is_some() {
            return Err("GalleryItem uses either Icon or ImageResource".into());
        }
        if item.image_resource.as_ref().is_some_and(String::is_empty) || item.columns.len() > 31 {
            return Err("Invalid Gallery image key or additional columns".into());
        }
        bytes += item.key.len() + item.group.len();
        for v in std::iter::once(&item.text)
            .chain(item.columns.iter())
            .chain(item.image_resource.iter())
        {
            text(v)?;
            bytes += v.len();
        }
    }
    if bytes > 64 * 1024 {
        return Err("Gallery metadata exceeds 64 KiB".into());
    }
    if let Some(key) = &value.selected_key
        && !value.items.iter().any(|i| i.key == *key && i.is_enabled)
    {
        return Err("SelectedKey requires an enabled GalleryItem".into());
    }
    if common.bindings.keys().any(|p| {
        !badges::binding_property(p)
            && !matches!(
                p.as_str(),
                "SelectedKey" | "IsReadOnly" | "IsEnabled" | "Visibility" | "Width" | "Height"
            )
    }) {
        return Err("Unsupported Gallery binding".into());
    }
    if let Some(template) = &value.item_template {
        validate_item_template(template)?;
    }
    Ok(())
}
pub(crate) fn property(
    doc: &Document<'_>,
    a: Attribute<'_, '_>,
    v: &mut GalleryDefinition,
) -> Result<bool, Diagnostic> {
    match a.name() {
        "SelectedKey" => v.selected_key = (!a.value().is_empty()).then(|| a.value().into()),
        "IsReadOnly" => v.is_read_only = boolean(doc, a)?,
        "SampleWidth" | "SampleHeight" | "Gap" | "GroupHeaderHeight" => {
            let value = a
                .value()
                .parse()
                .map_err(|_| at_attribute(doc, a, "Invalid Gallery dimension"))?;
            match a.name() {
                "SampleWidth" => v.sample_width = value,
                "SampleHeight" => v.sample_height = value,
                "Gap" => v.gap = value,
                _ => v.group_header_height = value,
            }
        }
        _ => return Ok(false),
    }
    Ok(true)
}
fn budget(node: Node<'_, '_>, depth: usize, count: &mut usize) -> Result<(), Diagnostic> {
    *count += 1;
    if depth > MAX_ELEMENT_DEPTH || *count > MAX_ELEMENTS {
        return Err(at_node(node, "Gallery exceeds the compiled tree budget"));
    }
    Ok(())
}
fn metadata(node: Node<'_, '_>, a: Attribute<'_, '_>) -> Result<(), Diagnostic> {
    if a.namespace().is_some() {
        return Err(at_node(node, "Gallery metadata directives are unsupported"));
    }
    reject_extension(node.document(), a)
}
pub(crate) fn parse_children(
    node: Node<'_, '_>,
    depth: usize,
    count: &mut usize,
    v: &mut GalleryDefinition,
) -> Result<(), Diagnostic> {
    for child in node.children() {
        if !child.is_element() {
            require_empty_content(child)?;
            continue;
        }
        if child.tag_name().namespace() != Some(UI_NAMESPACE) {
            return Err(at_node(child, "Unknown Gallery child namespace"));
        }
        match child.tag_name().name() {
            "GalleryItem" => v.items.push(parse_item(child, depth + 1, count, "")?),
            "GalleryGroup" => {
                budget(child, depth + 1, count)?;
                let mut g = GalleryGroupDefinition {
                    key: String::new(),
                    header: String::new(),
                };
                for a in child.attributes() {
                    metadata(child, a)?;
                    match a.name() {
                        "Key" => g.key = a.value().into(),
                        "Header" => g.header = a.value().into(),
                        _ => return Err(at_node(child, "GalleryGroup supports Key and Header")),
                    }
                }
                for item in child.children() {
                    if !item.is_element() {
                        require_empty_content(item)?;
                        continue;
                    }
                    if item.tag_name().namespace() != Some(UI_NAMESPACE)
                        || item.tag_name().name() != "GalleryItem"
                    {
                        return Err(at_node(item, "GalleryGroup accepts GalleryItem children"));
                    }
                    v.items.push(parse_item(item, depth + 2, count, &g.key)?);
                }
                v.groups.push(g);
            }
            "Gallery.ItemTemplate" => {
                if v.item_template.is_some() || child.attributes().len() != 0 {
                    return Err(at_node(child, "Duplicate or invalid Gallery.ItemTemplate"));
                }
                budget(child, depth + 1, count)?;
                for root in child.children() {
                    if !root.is_element() {
                        require_empty_content(root)?;
                        continue;
                    }
                    if v.item_template.is_some() {
                        return Err(at_node(root, "ItemTemplate requires one readonly root"));
                    }
                    let definition = parse_element(
                        root,
                        depth + 2,
                        *count,
                        count,
                        &mut HashSet::new(),
                        &mut HashSet::new(),
                    )?;
                    validate_item_template(&definition).map_err(|e| at_node(root, e))?;
                    v.item_template = Some(Box::new(definition));
                }
                if v.item_template.is_none() {
                    return Err(at_node(child, "ItemTemplate requires one readonly root"));
                }
            }
            _ => {
                return Err(at_node(
                    child,
                    "Gallery accepts GalleryItem, GalleryGroup and ItemTemplate",
                ));
            }
        }
    }
    Ok(())
}
fn parse_item(
    node: Node<'_, '_>,
    depth: usize,
    count: &mut usize,
    group: &str,
) -> Result<GalleryItemDefinition, Diagnostic> {
    budget(node, depth, count)?;
    let mut i = GalleryItemDefinition {
        key: String::new(),
        text: String::new(),
        group: group.into(),
        icon: None,
        image_resource: None,
        is_enabled: true,
        columns: Vec::new(),
    };
    for a in node.attributes() {
        metadata(node, a)?;
        match a.name() {
            "Key" => i.key = a.value().into(),
            "Text" => i.text = a.value().into(),
            "Icon" => i.icon = Some(icon_symbol(node.document(), a)?),
            "ImageResource" => i.image_resource = Some(a.value().into()),
            "IsEnabled" => i.is_enabled = boolean(node.document(), a)?,
            _ => return Err(at_node(node, "Unsupported GalleryItem attribute")),
        }
    }
    for child in node.children() {
        if !child.is_element() {
            require_empty_content(child)?;
            continue;
        }
        if child.tag_name().namespace() != Some(UI_NAMESPACE)
            || child.tag_name().name() != "GalleryColumn"
            || child.attributes().len() != 1
        {
            return Err(at_node(
                child,
                "GalleryItem accepts GalleryColumn Text metadata",
            ));
        }
        budget(child, depth + 1, count)?;
        let a = child.attributes().next().unwrap();
        metadata(child, a)?;
        if a.name() != "Text" {
            return Err(at_node(child, "GalleryColumn requires Text"));
        }
        i.columns.push(a.value().into());
        for n in child.children() {
            require_empty_content(n)?;
        }
    }
    Ok(i)
}
pub(crate) fn code(v: &GalleryDefinition) -> String {
    let groups=v.groups.iter().map(|g|format!("rust_desktop_ui_xaml::GalleryGroupDefinition{{key:{:?}.into(),header:{:?}.into()}}",g.key,g.header)).collect::<Vec<_>>().join(",");
    let items=v.items.iter().map(|i|format!("rust_desktop_ui_xaml::GalleryItemDefinition{{key:{:?}.into(),text:{:?}.into(),group:{:?}.into(),icon:{},image_resource:{},is_enabled:{},columns:vec![{}]}}",i.key,i.text,i.group,i.icon.map_or("None".into(),|v|format!("Some(rust_desktop_ui_xaml::IconSymbol::{v:?})")),i.image_resource.as_ref().map_or("None".into(),|v|format!("Some({v:?}.into())")),i.is_enabled,i.columns.iter().map(|v|format!("{v:?}.into()")).collect::<Vec<_>>().join(","))).collect::<Vec<_>>().join(",");
    format!(
        "Box::new(rust_desktop_ui_xaml::GalleryDefinition{{groups:vec![{groups}],items:vec![{items}],selected_key:{},is_read_only:{},sample_width:{:?},sample_height:{:?},gap:{:?},group_header_height:{:?},item_template:{}}})",
        v.selected_key
            .as_ref()
            .map_or("None".into(), |v| format!("Some({v:?}.into())")),
        v.is_read_only,
        v.sample_width,
        v.sample_height,
        v.gap,
        v.group_header_height,
        v.item_template.as_ref().map_or("None".into(), |t| format!(
            "Some(Box::new({}))",
            codegen::element(t)
        ))
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    fn body(value: &str) -> Result<WindowDefinition, Diagnostic> {
        parse(&format!(
            "<Window xmlns='urn:rust-ui-engine:ui' xmlns:x='http://schemas.microsoft.com/winfx/2006/xaml'>{value}</Window>"
        ))
    }
    #[test]
    fn gallery_compiles_resources_grouped_images_icons_and_accepted_binding() {
        let w=body("<Window.Resources><DataTemplate x:Key='Caption'><TextBlock Text='{Binding Text}'/></DataTemplate></Window.Resources><Gallery SelectedKey='{Binding Chosen}' ItemTemplate='{StaticResource Caption}'><GalleryGroup Key='styles' Header='Styles'><GalleryItem Key='blue' Text='Blue' ImageResource='BlueSample'><GalleryColumn Text='Modern'/></GalleryItem><GalleryItem Key='checked' Text='Checked' Icon='Check'/></GalleryGroup></Gallery>").unwrap();
        let ElementKind::Gallery(v) = &w.content.kind else {
            panic!()
        };
        assert_eq!(v.items[0].image_resource.as_deref(), Some("BlueSample"));
        assert_eq!(v.items[0].group, "styles");
        assert!(v.item_template.is_some());
        assert_eq!(
            w.content.common.bindings["SelectedKey"].mode,
            BindingMode::TwoWay
        );
        assert!(codegen::window(&w).contains("GalleryDefinition"));
    }
    #[test]
    fn gallery_rejects_ambiguous_interactive_or_unbounded_declarations() {
        for s in [
            "<Gallery SampleWidth='NaN'/>",
            "<Gallery SelectedKey='x'/>",
            "<Gallery><GalleryItem Key='a'/><GalleryItem Key='a'/></Gallery>",
            "<Gallery><GalleryItem Key='a' Icon='Check' ImageResource='Image'/></Gallery>",
            "<Gallery><GalleryGroup Key='a'/><GalleryGroup Key='a'/></Gallery>",
            "<Gallery><Gallery.ItemTemplate><Button Content='Action'/></Gallery.ItemTemplate></Gallery>",
            "<Gallery SelectedKey='a'><GalleryItem Key='a' IsEnabled='False'/></Gallery>",
            "<Gallery ItemsSource='{Binding Items}'/>",
        ] {
            assert!(body(s).is_err(), "{s}");
        }
    }
}
