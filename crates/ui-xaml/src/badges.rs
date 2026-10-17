//! Bounded attached badge metadata, independent of visual control types.
use crate::*;
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BadgeSeverityDefinition {
    #[default]
    Neutral,
    Info,
    Success,
    Warning,
    Error,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BadgeAnchorDefinition {
    TopLeft,
    #[default]
    TopRight,
    BottomLeft,
    BottomRight,
    Center,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BadgeClipDefinition {
    Owner,
    #[default]
    Parent,
}
#[derive(Clone, Debug, PartialEq)]
pub struct BadgeDefinition {
    pub count: Option<u64>,
    pub text: Option<String>,
    pub icon: Option<IconSymbol>,
    pub severity: BadgeSeverityDefinition,
    pub anchor: BadgeAnchorDefinition,
    pub offset: [f32; 2],
    pub visible: bool,
    pub clip: BadgeClipDefinition,
    pub max_count: u64,
    pub label: String,
}
impl Default for BadgeDefinition {
    fn default() -> Self {
        Self {
            count: None,
            text: None,
            icon: None,
            severity: Default::default(),
            anchor: Default::default(),
            offset: [0.0; 2],
            visible: true,
            clip: Default::default(),
            max_count: 99,
            label: String::new(),
        }
    }
}
pub(crate) fn binding_property(name: &str) -> bool {
    matches!(name, "Badge.Count" | "Badge.Text" | "Badge.Visibility")
}
fn canonical_count(value: &str) -> Result<u64, String> {
    let parsed = value
        .parse::<u64>()
        .map_err(|_| "Badge count requires a canonical u64 decimal string")?;
    if parsed.to_string() != value {
        return Err("Badge count requires a canonical u64 decimal string".into());
    }
    Ok(parsed)
}
fn text(value: &str, empty: bool) -> Result<(), String> {
    if (!empty && value.is_empty())
        || value.len() > 256
        || value.chars().any(char::is_control)
        || value.graphemes(true).count() > 64
        || value.graphemes(true).any(|g| g.chars().count() > 32)
    {
        return Err("Badge text/label require at most 64 graphemes, 256 bytes, 32 scalars per grapheme and no control characters; text cannot be empty".into());
    }
    Ok(())
}
/// Validates literal and bound attachment profiles before any owner is mounted.
pub fn validate_badge(common: &CommonProperties) -> Result<(), String> {
    let bound = common
        .bindings
        .keys()
        .filter(|name| name.starts_with("Badge."))
        .collect::<Vec<_>>();
    let Some(badge) = &common.badge else {
        if !bound.is_empty() {
            return Err("Badge bindings require Badge metadata".into());
        }
        return Ok(());
    };
    let content = usize::from(badge.count.is_some())
        + usize::from(badge.text.is_some())
        + usize::from(badge.icon.is_some())
        + usize::from(common.bindings.contains_key("Badge.Count"))
        + usize::from(common.bindings.contains_key("Badge.Text"));
    if content != 1 {
        return Err("Specify exactly one Badge.Count, Badge.Text or Badge.Icon".into());
    }
    if badge.max_count == 0
        || badge
            .offset
            .iter()
            .any(|value| !value.is_finite() || value.abs() > 256.0)
    {
        return Err("Badge.MaxCount must be positive and Badge.Offset finite within ±256".into());
    }
    text(&badge.label, true)?;
    if let Some(value) = &badge.text {
        text(value, false)?;
    }
    for name in bound {
        let binding = &common.bindings[name];
        if !binding_property(name)
            || !matches!(binding.mode, BindingMode::OneWay | BindingMode::OneTime)
        {
            return Err(
                "Badge.Count/Text/Visibility support only OneWay or OneTime bindings".into(),
            );
        }
        if let Some(fallback) = &binding.fallback_value {
            match name.as_str() {
                "Badge.Count" => {
                    canonical_count(fallback)?;
                }
                "Badge.Text" => text(fallback, false)?,
                "Badge.Visibility"
                    if !matches!(
                        fallback.as_str(),
                        "True" | "False" | "true" | "false" | "Visible" | "Hidden" | "Collapsed"
                    ) =>
                {
                    return Err(
                        "Badge.Visibility fallback requires boolean or Visible/Hidden/Collapsed"
                            .into(),
                    );
                }
                _ => {}
            }
        }
    }
    Ok(())
}
pub(crate) fn property(
    document: &Document<'_>,
    attr: Attribute<'_, '_>,
    common: &mut CommonProperties,
) -> Result<bool, Diagnostic> {
    if !attr.name().starts_with("Badge.") {
        return Ok(false);
    }
    let badge = common.badge.get_or_insert_with(Default::default);
    let err = |message| at_attribute(document, attr, message);
    match attr.name() {
        "Badge.Count" => badge.count = Some(canonical_count(attr.value()).map_err(err)?),
        "Badge.Text" => badge.text = Some(attr.value().into()),
        "Badge.Icon" => badge.icon = Some(icon_symbol(document, attr)?),
        "Badge.Label" => badge.label = attr.value().into(),
        "Badge.MaxCount" => badge.max_count = canonical_count(attr.value()).map_err(err)?,
        "Badge.Severity" => {
            badge.severity = match attr.value() {
                "Neutral" => BadgeSeverityDefinition::Neutral,
                "Info" => BadgeSeverityDefinition::Info,
                "Success" => BadgeSeverityDefinition::Success,
                "Warning" => BadgeSeverityDefinition::Warning,
                "Error" => BadgeSeverityDefinition::Error,
                _ => {
                    return Err(err(
                        "Badge.Severity must be Neutral, Info, Success, Warning or Error".into(),
                    ));
                }
            }
        }
        "Badge.Anchor" => {
            badge.anchor =
                match attr.value() {
                    "TopLeft" => BadgeAnchorDefinition::TopLeft,
                    "TopRight" => BadgeAnchorDefinition::TopRight,
                    "BottomLeft" => BadgeAnchorDefinition::BottomLeft,
                    "BottomRight" => BadgeAnchorDefinition::BottomRight,
                    "Center" => BadgeAnchorDefinition::Center,
                    _ => return Err(err(
                        "Badge.Anchor must be TopLeft, TopRight, BottomLeft, BottomRight or Center"
                            .into(),
                    )),
                }
        }
        "Badge.Clip" => {
            badge.clip = match attr.value() {
                "Owner" => BadgeClipDefinition::Owner,
                "Parent" => BadgeClipDefinition::Parent,
                _ => return Err(err("Badge.Clip must be Owner or Parent".into())),
            }
        }
        "Badge.Visibility" => {
            badge.visible = match attr.value() {
                "Visible" => true,
                "Hidden" | "Collapsed" => false,
                _ => {
                    return Err(err(
                        "Badge.Visibility must be Visible, Hidden or Collapsed".into()
                    ));
                }
            }
        }
        "Badge.Offset" => {
            let values = attr
                .value()
                .split(',')
                .map(|v| v.trim().parse::<f32>())
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| err("Badge.Offset requires two finite coordinates".into()))?;
            let [x, y] = values.as_slice() else {
                return Err(err("Badge.Offset requires x,y".into()));
            };
            badge.offset = [*x, *y];
        }
        _ => {
            return Err(err(format!(
                "Unsupported attached property {}",
                attr.name()
            )));
        }
    }
    Ok(true)
}
pub(crate) fn code(badge: &Option<Box<BadgeDefinition>>) -> String {
    badge.as_ref().map_or("None".into(),|b| format!("Some(Box::new(rust_desktop_ui_xaml::BadgeDefinition {{ count:{:?},text:{},icon:{},severity:rust_desktop_ui_xaml::BadgeSeverityDefinition::{:?},anchor:rust_desktop_ui_xaml::BadgeAnchorDefinition::{:?},offset:{:?},visible:{},clip:rust_desktop_ui_xaml::BadgeClipDefinition::{:?},max_count:{},label:{:?}.into() }}))",b.count,b.text.as_ref().map_or("None".into(),|v|format!("Some({v:?}.into())")),b.icon.map_or("None".into(),|icon|format!("Some(rust_desktop_ui_xaml::IconSymbol::{icon:?})")),b.severity,b.anchor,b.offset,b.visible,b.clip,b.max_count,b.label))
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
    fn attached_badges_compile_for_owners_and_keep_exact_counts() {
        let w=window("<StackPanel><Button Content='Inbox' Badge.Count='18446744073709551615' Badge.Severity='Info' Badge.Label='Unread'/><TextBox Badge.Text='{Binding Status}' Badge.Anchor='BottomRight' Badge.Offset='-8,3' Badge.Clip='Owner' Badge.Visibility='{Binding Visible}'/><Grid Badge.Icon='Check' Badge.MaxCount='9'/></StackPanel>").unwrap();
        let children = w.content.children();
        assert_eq!(
            children[0].common.badge.as_ref().unwrap().count,
            Some(u64::MAX)
        );
        assert_eq!(
            children[1].common.bindings["Badge.Text"].mode,
            BindingMode::OneWay
        );
        assert!(code(&children[0].common.badge).contains("18446744073709551615"));
        assert!(codegen::element(&w.content).contains("badge: Some("));
        validate_window_docking(&w).unwrap();
    }
    #[test]
    fn attachment_conflicts_and_unbounded_or_writable_metadata_are_rejected() {
        for attributes in [
            "Badge.Count='1' Badge.Text='text'",
            "Badge.Icon='Check' Badge.Count='{Binding Count}'",
            "Badge.Severity='Info'",
            "Badge.Count='-1'",
            "Badge.Count='01'",
            "Badge.Count='18446744073709551616'",
            "Badge.Count='1' Badge.MaxCount='0'",
            "Badge.Count='1' Badge.Offset='NaN,0'",
            "Badge.Count='1' Badge.Offset='0,257'",
            "Badge.Text=''",
            "Badge.Count='{Binding Count,Mode=TwoWay}'",
            "Badge.Text='{Binding Text,Mode=OneWayToSource}'",
            "Badge.Count='{Binding Count,FallbackValue=1.0}'",
            "Badge.Count='1' Badge.Unknown='True'",
            "Badge.Icon='{Binding Icon}'",
        ] {
            assert!(
                window(&format!("<Button {attributes}/>")).is_err(),
                "{attributes}"
            );
        }
        assert!(window(&format!("<Button Badge.Text='{}'/>", "a".repeat(65))).is_err());
        let mut w = window("<Button Badge.Count='{Binding Count}'/>").unwrap();
        w.content.common.badge.as_mut().unwrap().icon = Some(IconSymbol::Check);
        assert!(validate_window_docking(&w).is_err());
    }

    #[test]
    fn attachments_preserve_the_public_nested_content_depth_on_a_default_thread_stack() {
        std::thread::Builder::new()
            .stack_size(2 * 1024 * 1024)
            .spawn(|| {
                let groups = MAX_ELEMENT_DEPTH - 1;
                let source = format!(
                    "{}<Button Badge.Count='18446744073709551615'/>{}",
                    "<Expander Badge.Text='State'>".repeat(groups),
                    "</Expander>".repeat(groups)
                );
                let definition = window(&source).unwrap();
                let mut item = &definition.content;
                for _ in 0..groups {
                    assert_eq!(
                        item.common.badge.as_ref().unwrap().text.as_deref(),
                        Some("State")
                    );
                    item = item.children()[0];
                }
                assert_eq!(item.common.badge.as_ref().unwrap().count, Some(u64::MAX));
            })
            .unwrap()
            .join()
            .unwrap();
    }
}
