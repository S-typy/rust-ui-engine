//! Opt-in snapshots of popup state for the native example's input tests.
use rust_desktop_ui_controls::{Controls, Role};
use rust_desktop_ui_core::Rect;
use std::fmt::Write;

pub(crate) fn snapshot(ui: &Controls) -> String {
    let nodes = ui.semantics();
    let menus = nodes
        .iter()
        .filter(|node| node.role == Role::Menu)
        .map(|node| rectangle(node.bounds))
        .collect::<Vec<_>>()
        .join(",");
    let controls = nodes.iter().filter(|node| matches!(node.role,
        Role::Button | Role::ToggleButton | Role::MenuItem | Role::Separator | Role::Tab | Role::TextBox))
        .map(|node| format!(
            "{{\"label\":{},\"role\":{},\"checked\":{},\"enabled\":{},\"bounds\":{},\"clip\":{},\"has_popup\":{},\"expanded\":{},\"focused\":{}}}",
            string(&node.label), string(&format!("{:?}", node.role)), optional_bool(node.checked),
            !node.disabled, rectangle(node.bounds), ui.runtime.clip(node.id).map_or_else(|| "null".into(), rectangle),
            node.has_popup, optional_bool(node.expanded), node.focused))
        .collect::<Vec<_>>().join(",");
    let focused = nodes
        .iter()
        .find(|node| node.focused)
        .map_or("", |node| node.label.as_str());
    let hovered = nodes
        .iter()
        .find(|node| Some(node.id) == ui.runtime.hovered())
        .map_or("", |node| node.label.as_str());
    format!(
        "{{\"popup_open\":{},\"menus\":[{}],\"controls\":[{}],\"focused\":{},\"pin_checked\":{},\"tooltip_bounds\":{},\"tooltip_pending\":{},\"hovered\":{}}}",
        ui.popup_open(),
        menus,
        controls,
        string(focused),
        optional_bool(ui.commands().checked(&"Pin".into())),
        ui.tooltip_bounds().map_or_else(|| "null".into(), rectangle),
        ui.tooltip_deadline().is_some(),
        string(hovered)
    )
}

pub(crate) fn ribbon_snapshot(
    ui: &Controls,
    mounted: &crate::view_ribbon::MountedRibbon,
) -> String {
    format!(
        "{{\"active_tab\":{},\"key_tips\":{},\"bounds\":{},\"presentations\":{}}}",
        string(mounted.ribbon.active_tab()),
        mounted.ribbon.key_tips_visible(),
        ui.runtime
            .bounds(mounted.wrapper)
            .map_or_else(|| "null".into(), rectangle),
        string(&format!("{:?}", mounted.ribbon.presentations()))
    )
}

fn rectangle(rect: Rect) -> String {
    format!("[{},{},{},{}]", rect.x, rect.y, rect.width, rect.height)
}

fn optional_bool(value: Option<bool>) -> &'static str {
    match value {
        Some(true) => "true",
        Some(false) => "false",
        None => "null",
    }
}

fn string(value: &str) -> String {
    let mut result = String::from("\"");
    for ch in value.chars() {
        match ch {
            '"' => result.push_str("\\\""),
            '\\' => result.push_str("\\\\"),
            '\n' => result.push_str("\\n"),
            '\r' => result.push_str("\\r"),
            '\t' => result.push_str("\\t"),
            ch if ch <= '\u{1f}' => {
                write!(&mut result, "\\u{:04x}", ch as u32).expect("String write")
            }
            ch => result.push(ch),
        }
    }
    result.push('"');
    result
}
