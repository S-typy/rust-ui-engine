//! Notification ownership and retained wait-content declarations.
use crate::*;
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NotificationCenterDefinition {
    pub card_width: f32,
    pub max_visible: usize,
}
impl Default for NotificationCenterDefinition {
    fn default() -> Self {
        Self {
            card_width: 360.0,
            max_visible: 3,
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct LoadingOverlayDefinition {
    pub label: String,
    pub child: Option<Box<ElementDefinition>>,
}
impl Default for LoadingOverlayDefinition {
    fn default() -> Self {
        Self {
            label: "Loading".into(),
            child: None,
        }
    }
}
pub fn validate_notifications(kind: &ElementKind, common: &CommonProperties) -> Result<(), String> {
    if common.bindings.keys().any(|p| {
        !badges::binding_property(p)
            && !matches!(p.as_str(), "IsEnabled" | "Visibility" | "Width" | "Height")
    }) {
        return Err("Notifications support common layout/availability bindings; operation state belongs to the application".into());
    }
    match kind {
        ElementKind::NotificationCenter(value) => {
            if !value.card_width.is_finite()
                || !(240.0..=640.0).contains(&value.card_width)
                || !(1..=3).contains(&value.max_visible)
            {
                return Err(
                    "NotificationCenter requires CardWidth240..640 and MaxVisible1..3".into(),
                );
            }
        }
        ElementKind::LoadingOverlay(value) => {
            if value.label.len() > 256 || value.child.is_none() {
                return Err("LoadingOverlay requires one content child and Label<=256bytes".into());
            }
        }
        _ => return Err("Expected notification or loading control".into()),
    }
    Ok(())
}
pub(crate) fn property(
    doc: &Document<'_>,
    a: Attribute<'_, '_>,
    kind: &mut ElementKind,
) -> Result<bool, Diagnostic> {
    match kind {
        ElementKind::NotificationCenter(value) => match a.name() {
            "CardWidth" => {
                value.card_width = a
                    .value()
                    .parse()
                    .map_err(|_| at_attribute(doc, a, "Invalid CardWidth"))?
            }
            "MaxVisible" => {
                value.max_visible = a
                    .value()
                    .parse()
                    .map_err(|_| at_attribute(doc, a, "Invalid MaxVisible"))?
            }
            _ => return Ok(false),
        },
        ElementKind::LoadingOverlay(value) => match a.name() {
            "Label" => value.label = a.value().into(),
            _ => return Ok(false),
        },
        _ => return Ok(false),
    }
    Ok(true)
}
pub(crate) fn parse_loading(
    node: Node<'_, '_>,
    depth: usize,
    parent: usize,
    count: &mut usize,
    names: &mut HashSet<String>,
    radio: &mut HashSet<RadioGroupKey>,
    value: &mut LoadingOverlayDefinition,
) -> Result<(), Diagnostic> {
    for child in node.children() {
        if child.is_element() {
            if value.child.is_some() {
                return Err(at_node(
                    child,
                    "LoadingOverlay accepts exactly one content child",
                ));
            }
            value.child = Some(Box::new(parse_element(
                child,
                depth + 1,
                parent,
                count,
                names,
                radio,
            )?));
        } else {
            require_empty_content(child)?;
        }
    }
    Ok(())
}
pub(crate) fn code(kind: &ElementKind) -> String {
    match kind {
        ElementKind::NotificationCenter(v) => format!(
            "rust_desktop_ui_xaml::NotificationCenterDefinition {{card_width:{:?},max_visible:{}}}",
            v.card_width, v.max_visible
        ),
        ElementKind::LoadingOverlay(v) => format!(
            "Box::new(rust_desktop_ui_xaml::LoadingOverlayDefinition {{label:{:?}.into(),child:{}}})",
            v.label,
            v.child.as_deref().map_or_else(
                || "None".into(),
                |child| format!("Some(Box::new({}))", crate::codegen::element(child))
            )
        ),
        _ => unreachable!(),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn notification_limits_and_retained_child_names_are_compiled() {
        let text = "<Window xmlns='urn:rust-ui-engine:ui' xmlns:x='http://schemas.microsoft.com/winfx/2006/xaml'><StackPanel><NotificationCenter x:Name='Toasts' CardWidth='400' MaxVisible='2'/><LoadingOverlay x:Name='Wait' Label='Wait'><TextBox x:Name='Draft' Text='{Binding Draft}'/></LoadingOverlay></StackPanel></Window>";
        let window = parse(text).unwrap();
        assert_eq!(
            window.content.children()[1].children()[0]
                .common
                .name
                .as_deref(),
            Some("Draft")
        );
        let code = crate::codegen::window(&window);
        assert!(code.contains("LoadingOverlayDefinition"));
        for attr in [
            "CardWidth='239'",
            "CardWidth='NaN'",
            "MaxVisible='4'",
            "Message='{Binding X}'",
        ] {
            assert!(
                parse(&format!(
                    "<Window xmlns='urn:rust-ui-engine:ui'><NotificationCenter {attr}/></Window>"
                ))
                .is_err()
            );
        }
    }
    #[test]
    fn loading_requires_single_content_and_uses_ordinary_namescope_limits() {
        for content in [
            "",
            "<TextBox/><TextBox/>",
            "<StackPanel><TextBox x:Name='Same'/><TextBox x:Name='Same'/></StackPanel>",
        ] {
            assert!(parse(&format!("<Window xmlns='urn:rust-ui-engine:ui' xmlns:x='http://schemas.microsoft.com/winfx/2006/xaml'><LoadingOverlay>{content}</LoadingOverlay></Window>")).is_err());
        }
    }
}
