//! Registered-image resources for an explicit image editing session.
use crate::*;
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ImageEditorDefinition {
    pub source: Option<String>,
    pub is_read_only: bool,
    pub viewer: bool,
}
pub fn validate_image_editor(
    value: &ImageEditorDefinition,
    common: &CommonProperties,
) -> Result<(), String> {
    if let Some(source) = &value.source {
        crate::validate_image(&ImageDefinition {
            source: source.clone(),
            ..Default::default()
        })?;
    }
    if common.bindings.keys().any(|p| {
        !crate::badges::binding_property(p)
            && !matches!(
                p.as_str(),
                "IsReadOnly" | "IsEnabled" | "Visibility" | "Width" | "Height"
            )
    }) {
        return Err("ImageEditor supports IsReadOnly and common layout bindings; bitmap values use the typed Rust API".into());
    }
    if value.viewer && common.bindings.contains_key("IsReadOnly") {
        return Err("ImageViewer is always read-only".into());
    }
    Ok(())
}
pub(crate) fn property(
    document: &Document<'_>,
    attr: Attribute<'_, '_>,
    value: &mut ImageEditorDefinition,
) -> Result<bool, Diagnostic> {
    match attr.name() {
        "Source" => value.source = Some(attr.value().into()),
        "IsReadOnly" => {
            value.is_read_only = boolean(document, attr)?;
            if value.viewer && !value.is_read_only {
                return Err(at_attribute(
                    document,
                    attr,
                    "ImageViewer is always read-only",
                ));
            }
        }
        _ => return Ok(false),
    }
    Ok(true)
}
pub(crate) fn code(v: &ImageEditorDefinition) -> String {
    format!(
        "rust_desktop_ui_xaml::ImageEditorDefinition{{source:{},is_read_only:{},viewer:{}}}",
        v.source
            .as_ref()
            .map_or_else(|| "None".into(), |s| format!("Some({s:?}.into())")),
        v.is_read_only,
        v.viewer
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn image_editor_sources_and_binding_contract_are_strict() {
        for body in [
            "<ImageEditor Source='Sample' IsReadOnly='{Binding Locked}'/>",
            "<ImageViewer/>",
        ] {
            let w = parse(&format!(
                "<Window xmlns='urn:rust-ui-engine:ui'>{body}</Window>"
            ))
            .unwrap();
            let ElementKind::ImageEditor(v) = w.content.kind else {
                panic!()
            };
            assert!(code(&v).contains("ImageEditorDefinition"));
        }
        for body in [
            "<ImageEditor Source='../file.png'/>",
            "<ImageEditor Value='{Binding Pixels}'/>",
            "<ImageViewer IsReadOnly='False'/>",
            "<ImageEditor><TextBox/></ImageEditor>",
        ] {
            assert!(
                parse(&format!(
                    "<Window xmlns='urn:rust-ui-engine:ui'>{body}</Window>"
                ))
                .is_err(),
                "{body}"
            );
        }
    }
}
