//! Explicit property metadata over a registered typed source.
use crate::*;

#[derive(Clone, Debug, PartialEq)]
pub struct PropertyGridDefinition {
    pub data_source: String,
    pub object_keys: Vec<i64>,
    pub properties: Vec<PropertyDefinition>,
    pub label_width: f32,
    /// Keep the toolbar visible while the property rows scroll in a bounded host.
    pub scrollable: bool,
    pub is_read_only: bool,
}
impl Default for PropertyGridDefinition {
    fn default() -> Self {
        Self {
            data_source: String::new(),
            object_keys: Vec::new(),
            properties: Vec::new(),
            label_width: 180.0,
            scrollable: false,
            is_read_only: false,
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct PropertyDefinition {
    pub metadata: DataFormFieldDefinition,
    pub category: String,
    pub groups: Vec<String>,
    /// Typed storage text interpreted against the registered FieldSchema.
    pub default_value: Option<String>,
    pub default_is_null: bool,
    pub choices: Vec<String>,
    pub full_width: bool,
    pub display_template: Option<Box<ElementDefinition>>,
}
impl PropertyDefinition {
    /// Optional TextBlock that presents the accepted value in the compiled template.
    pub fn value_part(&self) -> Option<&ElementDefinition> {
        let mut nodes = vec![self.display_template.as_deref()?];
        while let Some(node) = nodes.pop() {
            if node.common.name.as_deref().is_some_and(is_value_part) {
                return Some(node);
            }
            nodes.extend(node.children());
        }
        None
    }
}

fn is_value_part(name: &str) -> bool {
    name == "PART_Value"
        || name
            .strip_prefix("__template_")
            .and_then(|rest| rest.split_once('_'))
            .is_some_and(|(number, name)| number.parse::<usize>().is_ok() && name == "PART_Value")
}
impl Default for PropertyDefinition {
    fn default() -> Self {
        Self {
            metadata: DataFormFieldDefinition::default(),
            category: "Свойства".into(),
            groups: Vec::new(),
            default_value: None,
            default_is_null: false,
            choices: Vec::new(),
            full_width: false,
            display_template: None,
        }
    }
}
pub fn validate_property_grid(
    value: &PropertyGridDefinition,
    common: &CommonProperties,
) -> Result<(), String> {
    let form = DataFormDefinition {
        data_source: value.data_source.clone(),
        row_key: 0,
        fields: value
            .properties
            .iter()
            .map(|p| p.metadata.clone())
            .collect(),
        label_width: value.label_width,
        row_gap: 0.0,
        is_read_only: value.is_read_only,
    };
    validate_data_form(&form)?;
    if value.object_keys.is_empty()
        || value.object_keys.len() > 64
        || value.object_keys.iter().collect::<HashSet<_>>().len() != value.object_keys.len()
    {
        return Err("PropertyGrid requires 1..64 unique ObjectKeys".into());
    }
    if common
        .bindings
        .keys()
        .any(|p| !matches!(p.as_str(), "IsEnabled" | "Visibility" | "Width" | "Height"))
    {
        return Err("PropertyGrid binds a registered typed source; only common geometry/availability bindings are supported".into());
    }
    let mut bytes = 0;
    for p in &value.properties {
        if let Some(template) = &p.display_template {
            validate_header_content(template)?;
            let mut nodes = vec![template.as_ref()];
            while let Some(node) = nodes.pop() {
                if node.common.name.as_deref().is_some_and(is_value_part)
                    && (!matches!(node.kind, ElementKind::TextBlock(_))
                        || node.common.bindings.contains_key("Text"))
                {
                    return Err("PART_Value requires a TextBlock with source-owned text".into());
                }
                nodes.extend(node.children());
            }
        }
        if p.choices.len() > 128
            || p.choices.iter().collect::<HashSet<_>>().len() != p.choices.len()
            || p.choices
                .iter()
                .any(|v| v.is_empty() || v.len() > 256 || v.chars().any(char::is_control))
        {
            return Err("Invalid property Choices".into());
        }
        bytes += p.choices.iter().map(String::len).sum::<usize>();
        if p.groups.len() > 8
            || std::iter::once(&p.category)
                .chain(&p.groups)
                .any(|s| s.is_empty() || s.len() > 256 || s.chars().any(char::is_control))
            || p.default_is_null && p.default_value.is_some()
            || p.default_value
                .as_ref()
                .is_some_and(|s| s.len() > 16384 || s.contains('\0'))
        {
            return Err("Invalid Property category, group path or default".into());
        }
        bytes += p.category.len()
            + p.groups.iter().map(String::len).sum::<usize>()
            + p.metadata.field.len()
            + p.metadata.label.as_ref().map_or(0, String::len);
    }
    if bytes > 32768 {
        return Err("Property metadata exceeds 32 KiB".into());
    }
    Ok(())
}
pub(crate) fn attribute(
    document: &roxmltree::Document<'_>,
    attr: roxmltree::Attribute<'_, '_>,
    value: &mut PropertyGridDefinition,
) -> Result<bool, Diagnostic> {
    match attr.name() {
        "DataSource" => value.data_source = attr.value().into(),
        "ObjectKeys" => {
            value.object_keys = attr
                .value()
                .split(',')
                .map(|s| {
                    s.trim().parse::<i64>().map_err(|_| {
                        at_attribute(
                            document,
                            attr,
                            "ObjectKeys contains an invalid signed 64-bit key",
                        )
                    })
                })
                .collect::<Result<_, _>>()?;
        }
        "Scrollable" => value.scrollable = boolean(document, attr)?,
        "LabelWidth" => value.label_width = editor_metric(document, attr, 4096.0, true)?,
        "IsReadOnly" => value.is_read_only = boolean(document, attr)?,
        _ => return Ok(false),
    }
    Ok(true)
}
pub(crate) fn parse_children(
    node: Node<'_, '_>,
    depth: usize,
    count: &mut usize,
    value: &mut PropertyGridDefinition,
) -> Result<(), Diagnostic> {
    let mut wrapper = false;
    for child in node.children() {
        if !child.is_element() {
            require_empty_content(child)?;
            continue;
        }
        if wrapper
            || child.tag_name().namespace() != Some(UI_NAMESPACE)
            || child.tag_name().name() != "PropertyGrid.Properties"
            || child.attributes().len() != 0
        {
            return Err(at_node(
                child,
                "PropertyGrid accepts one PropertyGrid.Properties property",
            ));
        }
        wrapper = true;
        *count += 1;
        for item in child.children() {
            if !item.is_element() {
                require_empty_content(item)?;
                continue;
            }
            *count += 1;
            if *count > MAX_ELEMENTS
                || depth + 2 > MAX_ELEMENT_DEPTH
                || value.properties.len() >= 128
            {
                return Err(at_node(item, "Property metadata count or depth exceeded"));
            }
            if item.tag_name().namespace() != Some(UI_NAMESPACE)
                || item.tag_name().name() != "Property"
            {
                return Err(at_node(
                    item,
                    "PropertyGrid.Properties accepts Property metadata",
                ));
            }
            let mut p = PropertyDefinition::default();
            for attr in item.attributes() {
                reject_extension(item.document(), attr)?;
                if attr.namespace().is_some() {
                    return Err(at_attribute(
                        item.document(),
                        attr,
                        "Property directives are unsupported",
                    ));
                }
                match attr.name() {
                    "FieldName" => p.metadata.field = attr.value().into(),
                    "Label" => p.metadata.label = Some(attr.value().into()),
                    "Editor" => p.metadata.editor = attr.value().into(),
                    "IsReadOnly" => p.metadata.is_read_only = boolean(item.document(), attr)?,
                    "IsVisible" => p.metadata.visible = boolean(item.document(), attr)?,
                    "Category" => p.category = attr.value().into(),
                    "FullWidth" => p.full_width = boolean(item.document(), attr)?,
                    "Choices" => {
                        p.choices = attr
                            .value()
                            .split('|')
                            .map(|s| s.trim().to_owned())
                            .collect()
                    }
                    "Group" => p.groups = attr.value().split('/').map(str::to_string).collect(),
                    "DefaultValue" => p.default_value = Some(attr.value().into()),
                    "DefaultIsNull" => p.default_is_null = boolean(item.document(), attr)?,
                    _ => {
                        return Err(at_attribute(
                            item.document(),
                            attr,
                            "Unsupported Property metadata",
                        ));
                    }
                }
            }
            for inner in item.children() {
                if !inner.is_element() {
                    require_empty_content(inner)?;
                    continue;
                }
                require_element(inner, "Property.DisplayTemplate")?;
                if inner.attributes().len() != 0 || p.display_template.is_some() {
                    return Err(at_node(
                        inner,
                        "Invalid or duplicate Property.DisplayTemplate",
                    ));
                }
                for visual in inner.children() {
                    if !visual.is_element() {
                        require_empty_content(visual)?;
                        continue;
                    }
                    if p.display_template.is_some() {
                        return Err(at_node(visual, "DisplayTemplate requires one visual root"));
                    }
                    let definition = parse_element(
                        visual,
                        depth + 4,
                        *count,
                        count,
                        &mut HashSet::new(),
                        &mut HashSet::new(),
                    )?;
                    validate_header_content(&definition).map_err(|e| at_node(visual, e))?;
                    p.display_template = Some(Box::new(definition));
                }
                if p.display_template.is_none() {
                    return Err(at_node(inner, "Empty DisplayTemplate"));
                }
            }
            value.properties.push(p);
        }
    }
    Ok(())
}
pub(crate) fn code(v: &PropertyGridDefinition) -> String {
    let strings = |items: &[String]| {
        items
            .iter()
            .map(|s| format!("{s:?}.into()"))
            .collect::<Vec<_>>()
            .join(",")
    };
    let properties = v.properties.iter().map(|p| {
        let m = &p.metadata;
        let metadata = format!("rust_desktop_ui_xaml::DataFormFieldDefinition {{ field:{:?}.into(), label:{}, visible:{}, is_read_only:{}, editor:{:?}.into() }}",
            m.field, optional_string(&m.label), m.visible, m.is_read_only, m.editor);
        format!("rust_desktop_ui_xaml::PropertyDefinition {{ metadata:{metadata}, category:{:?}.into(), groups:vec![{}], default_value:{}, default_is_null:{}, choices:vec![{}], full_width:{}, display_template:{} }}",
            p.category, strings(&p.groups), optional_string(&p.default_value), p.default_is_null, strings(&p.choices), p.full_width, codegen::optional_child(p.display_template.as_deref()))
    }).collect::<Vec<_>>().join(",");
    format!(
        "rust_desktop_ui_xaml::PropertyGridDefinition {{ data_source:{:?}.into(), object_keys:vec!{:?}, label_width:{:?}, is_read_only:{}, scrollable:{}, properties:vec![{properties}] }}",
        v.data_source, v.object_keys, v.label_width, v.is_read_only, v.scrollable
    )
}
fn optional_string(v: &Option<String>) -> String {
    v.as_ref()
        .map_or("None".into(), |s| format!("Some({s:?}.into())"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn property_templates_scope_value_parts_and_reject_invalid_choices_or_controls() {
        let source = r#"<Window xmlns="urn:rust-ui-engine:ui" xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"><PropertyGrid DataSource="Objects" ObjectKeys="1"><PropertyGrid.Properties><Property FieldName="a" Choices="One|Two"><Property.DisplayTemplate><TextBlock x:Name="PART_Value"/></Property.DisplayTemplate></Property><Property FieldName="b" FullWidth="True"><Property.DisplayTemplate><TextBlock x:Name="PART_Value"/></Property.DisplayTemplate></Property></PropertyGrid.Properties></PropertyGrid></Window>"#;
        let parsed = parse(source).unwrap();
        let ElementKind::PropertyGrid(grid) = parsed.content.kind else {
            panic!()
        };
        assert_ne!(
            grid.properties[0].value_part().unwrap().common.name,
            grid.properties[1].value_part().unwrap().common.name
        );
        for invalid in [
            source.replace("One|Two", "One|One"),
            source.replace("One|Two", "One|"),
            source.replace("<TextBlock", "<Button"),
        ] {
            assert!(parse(&invalid).is_err());
        }
    }
    #[test]
    fn inspector_metadata_codegen_preserves_exact_keys_mixed_defaults_and_paths() {
        let source = r#"<Window xmlns="urn:rust-ui-engine:ui"><PropertyGrid DataSource="Objects" ObjectKeys="-9223372036854775808,9223372036854775807"><PropertyGrid.Properties><Property FieldName="count" Category="Layout" Group="Size/Bounds" DefaultValue="9223372036854775807"/><Property FieldName="name" DefaultIsNull="True" IsReadOnly="True"/></PropertyGrid.Properties></PropertyGrid></Window>"#;
        let parsed = parse(source).unwrap();
        let ElementKind::PropertyGrid(value) = &parsed.content.kind else {
            panic!()
        };
        assert_eq!(value.object_keys, [i64::MIN, i64::MAX]);
        assert_eq!(value.properties[0].groups, ["Size", "Bounds"]);
        assert!(codegen::window(&parsed).contains("PropertyGridDefinition"));
        for invalid in [
            source.replace("9223372036854775807\"", "9223372036854775808\""),
            source.replace("FieldName=\"name\"", "FieldName=\"count\""),
            source.replace("Group=\"Size/Bounds\"", "Group=\"Size//Bounds\""),
            source.replace(
                "DefaultIsNull=\"True\"",
                "DefaultIsNull=\"True\" DefaultValue=\"x\"",
            ),
            source.replace(
                "DefaultValue=\"9223372036854775807\"",
                "DefaultValue=\"{Binding Secret}\"",
            ),
            source.replace(
                "ObjectKeys=\"-9223372036854775808,9223372036854775807\"",
                "ObjectKeys=\"1,1\"",
            ),
        ] {
            assert!(parse(&invalid).is_err(), "{invalid}");
        }
    }
}
