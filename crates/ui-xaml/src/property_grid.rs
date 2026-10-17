//! Explicit property metadata over a registered typed source.
use crate::*;

#[derive(Clone, Debug, PartialEq)]
pub struct PropertyGridDefinition {
    pub data_source: String,
    pub object_keys: Vec<i64>,
    pub properties: Vec<PropertyDefinition>,
    pub label_width: f32,
    pub is_read_only: bool,
}
impl Default for PropertyGridDefinition {
    fn default() -> Self {
        Self {
            data_source: String::new(),
            object_keys: Vec::new(),
            properties: Vec::new(),
            label_width: 180.0,
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
}
impl Default for PropertyDefinition {
    fn default() -> Self {
        Self {
            metadata: DataFormFieldDefinition::default(),
            category: "Свойства".into(),
            groups: Vec::new(),
            default_value: None,
            default_is_null: false,
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
                if inner.is_element() {
                    return Err(at_node(inner, "Property cannot contain children"));
                }
                require_empty_content(inner)?;
            }
            value.properties.push(p);
        }
    }
    Ok(())
}
pub(crate) fn code(v: &PropertyGridDefinition) -> String {
    format!("rust_desktop_ui_xaml::PropertyGridDefinition {{data_source:{:?}.into(),object_keys:vec!{:?},label_width:{:?},is_read_only:{},properties:vec![{}]}}",v.data_source,v.object_keys,v.label_width,v.is_read_only,v.properties.iter().map(|p|format!("rust_desktop_ui_xaml::PropertyDefinition {{ metadata:rust_desktop_ui_xaml::DataFormFieldDefinition {{field:{:?}.into(),label:{},visible:{},is_read_only:{},editor:{:?}.into()}},category:{:?}.into(),groups:vec![{}],default_value:{},default_is_null:{} }}",p.metadata.field,optional_string(&p.metadata.label),p.metadata.visible,p.metadata.is_read_only,p.metadata.editor,p.category,p.groups.iter().map(|g|format!("{g:?}.into()")).collect::<Vec<_>>().join(","),optional_string(&p.default_value),p.default_is_null)).collect::<Vec<_>>().join(","))
}
fn optional_string(v: &Option<String>) -> String {
    v.as_ref()
        .map_or("None".into(), |s| format!("Some({s:?}.into())"))
}

#[cfg(test)]
mod tests {
    use super::*;
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
