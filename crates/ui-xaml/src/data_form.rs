//! Bounded metadata for a form over a registered typed data source.
use crate::*;

#[derive(Clone, Debug, PartialEq)]
pub struct DataFormDefinition {
    pub data_source: String,
    pub row_key: i64,
    pub fields: Vec<DataFormFieldDefinition>,
    pub label_width: f32,
    pub row_gap: f32,
    pub is_read_only: bool,
}
impl Default for DataFormDefinition {
    fn default() -> Self {
        Self {
            data_source: String::new(),
            row_key: 0,
            fields: Vec::new(),
            label_width: 160.0,
            row_gap: 8.0,
            is_read_only: false,
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct DataFormFieldDefinition {
    pub field: String,
    pub label: Option<String>,
    pub visible: bool,
    pub is_read_only: bool,
    /// Built-in typed recipe: Auto, Text, Numeric, Date, Time or CheckBox.
    pub editor: String,
}
impl Default for DataFormFieldDefinition {
    fn default() -> Self {
        Self {
            field: String::new(),
            label: None,
            visible: true,
            is_read_only: false,
            editor: "Auto".into(),
        }
    }
}
pub fn validate_data_form(form: &DataFormDefinition) -> Result<(), String> {
    if form.data_source.len() > 128 || !binding::valid_identifier(&form.data_source) {
        return Err("DataForm DataSource must be a registered identifier".into());
    }
    if form.fields.len() > 128
        || !form.label_width.is_finite()
        || !(0.0..=4096.0).contains(&form.label_width)
        || !form.row_gap.is_finite()
        || !(0.0..=256.0).contains(&form.row_gap)
    {
        return Err("DataForm supports 128 fields, LabelWidth 0..4096 and RowGap 0..256".into());
    }
    let mut fields = HashSet::new();
    for field in &form.fields {
        if field.field.is_empty()
            || field.field.len() > 256
            || field.field.contains('\0')
            || !fields.insert(&field.field)
            || field
                .label
                .as_ref()
                .is_some_and(|s| s.len() > 4096 || s.contains('\0'))
            || !matches!(
                field.editor.as_str(),
                "Auto" | "Text" | "Numeric" | "Date" | "Time" | "CheckBox"
            )
        {
            return Err("Invalid or duplicate DataFormField metadata or editor recipe".into());
        }
    }
    Ok(())
}
pub(crate) fn parse_children(
    node: Node<'_, '_>,
    depth: usize,
    count: &mut usize,
    form: &mut DataFormDefinition,
) -> Result<(), Diagnostic> {
    let mut wrapper = false;
    for child in node.children() {
        if !child.is_element() {
            require_empty_content(child)?;
            continue;
        }
        if child.tag_name().namespace() != Some(UI_NAMESPACE)
            || child.tag_name().name() != "DataForm.Fields"
            || wrapper
            || child.attributes().len() != 0
        {
            return Err(at_node(
                child,
                "DataForm accepts one DataForm.Fields property without attributes",
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
            if *count > MAX_ELEMENTS || depth + 2 > MAX_ELEMENT_DEPTH || form.fields.len() >= 128 {
                return Err(at_node(item, "DataForm field count or depth exceeded"));
            }
            if item.tag_name().namespace() != Some(UI_NAMESPACE)
                || item.tag_name().name() != "DataFormField"
            {
                return Err(at_node(
                    item,
                    "DataForm.Fields accepts DataFormField metadata",
                ));
            }
            let mut field = DataFormFieldDefinition::default();
            for attr in item.attributes() {
                reject_extension(item.document(), attr)?;
                if attr.namespace().is_some() {
                    return Err(at_attribute(
                        item.document(),
                        attr,
                        "DataFormField directives are unsupported",
                    ));
                }
                match attr.name() {
                    "FieldName" => field.field = attr.value().into(),
                    "Label" => field.label = Some(attr.value().into()),
                    "IsVisible" => field.visible = boolean(item.document(), attr)?,
                    "IsReadOnly" => field.is_read_only = boolean(item.document(), attr)?,
                    "Editor" => field.editor = attr.value().into(),
                    _ => {
                        return Err(at_attribute(
                            item.document(),
                            attr,
                            "Unsupported DataFormField property",
                        ));
                    }
                }
            }
            for inner in item.children() {
                if inner.is_element() {
                    return Err(at_node(inner, "DataFormField cannot contain children"));
                }
                require_empty_content(inner)?;
            }
            form.fields.push(field);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn metadata_generates_typed_bounded_fields_and_exact_row_keys() {
        let source = r#"<Window xmlns="urn:rust-ui-engine:ui"><DataForm DataSource="People" RowKey="-9223372036854775808" LabelWidth="180"><DataForm.Fields><DataFormField FieldName="name" Label="Имя"/><DataFormField FieldName="count" Editor="Numeric" IsReadOnly="True"/></DataForm.Fields></DataForm></Window>"#;
        let window = parse(source).unwrap();
        let ElementKind::DataForm(form) = &window.content.kind else {
            panic!()
        };
        assert_eq!(form.row_key, i64::MIN);
        assert_eq!(form.fields.len(), 2);
        assert!(form.fields[1].is_read_only);
        assert!(crate::codegen::window(&window).contains("DataFormDefinition"));
        for malformed in [
            source.replace("-9223372036854775808", "9223372036854775808"),
            source.replace("FieldName=\"count\"", "FieldName=\"name\""),
            source.replace("Editor=\"Numeric\"", "Editor=\"Unknown\""),
            source.replace("LabelWidth=\"180\"", "LabelWidth=\"5000\""),
        ] {
            assert!(parse(&malformed).is_err());
        }
    }
}
