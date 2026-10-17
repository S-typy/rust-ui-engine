//! Schema-referenced visual/text query editing declarations.
use crate::*;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum QueryBuilderModeDefinition {
    #[default]
    Visual,
    Text,
}
#[derive(Clone, Debug, PartialEq)]
pub struct QueryBuilderDefinition {
    pub data_source: String,
    pub value: String,
    pub mode: QueryBuilderModeDefinition,
    pub is_read_only: bool,
}
impl Default for QueryBuilderDefinition {
    fn default() -> Self {
        Self {
            data_source: String::new(),
            value: "true()".into(),
            mode: Default::default(),
            is_read_only: false,
        }
    }
}
pub fn validate_query_builder(
    value: &QueryBuilderDefinition,
    common: &CommonProperties,
) -> Result<(), String> {
    if value.data_source.is_empty()
        || value.data_source.len() > 128
        || !value
            .data_source
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"_.-".contains(&c))
    {
        return Err("Query builder requires a registered DataSource name".into());
    }
    if value.value.len() > rust_desktop_ui_data::MAX_QUERY_TEXT_BYTES {
        return Err("Query value exceeds 16 KiB".into());
    }
    if common.bindings.keys().any(|p| {
        !badges::binding_property(p)
            && !matches!(
                p.as_str(),
                "Value" | "IsReadOnly" | "IsEnabled" | "Visibility" | "Width" | "Height"
            )
    }) {
        return Err("Query builders bind Value, IsReadOnly and common properties".into());
    }
    Ok(())
}
pub(crate) fn attribute(
    doc: &Document<'_>,
    a: Attribute<'_, '_>,
    v: &mut QueryBuilderDefinition,
) -> Result<bool, Diagnostic> {
    match a.name() {
        "DataSource" => v.data_source = a.value().into(),
        "Value" => v.value = a.value().into(),
        "IsReadOnly" => v.is_read_only = boolean(doc, a)?,
        "Mode" => {
            v.mode = match a.value() {
                "Visual" => QueryBuilderModeDefinition::Visual,
                "Text" => QueryBuilderModeDefinition::Text,
                _ => return Err(at_attribute(doc, a, "Mode must be Visual or Text")),
            }
        }
        _ => return Ok(false),
    }
    Ok(true)
}
pub(crate) fn code(v: &QueryBuilderDefinition) -> String {
    format!(
        "Box::new(rust_desktop_ui_xaml::QueryBuilderDefinition{{data_source:{:?}.into(),value:{:?}.into(),mode:rust_desktop_ui_xaml::QueryBuilderModeDefinition::{:?},is_read_only:{}}})",
        v.data_source, v.value, v.mode, v.is_read_only
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn compiled_builders_preserve_modes_bindings_and_limits() {
        let w=parse("<Window xmlns='urn:rust-ui-engine:ui'><StackPanel><FilterBuilder DataSource='Orders' Value='{Binding Filter}'/><ExpressionBuilder DataSource='Orders' Value='integer(&quot;2&quot;)' Mode='Text'/></StackPanel></Window>").unwrap();
        assert!(crate::codegen::window(&w).contains("QueryBuilderDefinition"));
        for attr in [
            "DataSource='Bad source'",
            "DataSource='Orders' Mode='Other'",
            "DataSource='Orders' Text='true()'",
            "DataSource='Orders' SelectedValue='{Binding X}'",
        ] {
            assert!(
                parse(&format!(
                    "<Window xmlns='urn:rust-ui-engine:ui'><FilterBuilder {attr}/></Window>"
                ))
                .is_err()
            );
        }
    }
}
