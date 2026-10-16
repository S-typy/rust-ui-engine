//! Opt-in command snapshots for developer automation; no application input is logged.
use rust_desktop_ui_controls::{Controls, Ribbon, RibbonPresentation, Role, SemanticNode, Theme};
use rust_desktop_ui_core::{Rect, Size};
use std::fmt::Write;

#[derive(Default)]
pub(crate) struct Diagnostics {
    pub(crate) enabled: bool,
    pub(crate) sequence: u64,
    last: String,
}

impl Diagnostics {
    pub(crate) fn favorite_activated(&mut self) {
        self.sequence += 1;
    }

    pub(crate) fn emit(&mut self, ui: &Controls, ribbon: &Ribbon, viewport: Size) {
        if !self.enabled {
            return;
        }
        let snapshot = self.snapshot(ui, ribbon, viewport);
        if snapshot != self.last {
            println!("command-state {snapshot}");
            self.last = snapshot;
        }
    }

    pub(crate) fn snapshot(&self, ui: &Controls, ribbon: &Ribbon, viewport: Size) -> String {
        let semantics = ui.semantics();
        let visible: Vec<_> = semantics
            .iter()
            .filter(|node| {
                ui.runtime.clip(node.id).is_some_and(|clip| {
                    node.bounds
                        .intersection(clip)
                        .is_some_and(|area| area.width > 0.0 && area.height > 0.0)
                })
            })
            .collect();
        let controls: Vec<_> = visible
            .iter()
            .copied()
            .filter(|node| {
                matches!(
                    node.role,
                    Role::Button | Role::ToggleButton | Role::Tab | Role::MenuItem
                )
            })
            .collect();
        let entries = |nodes: Vec<&SemanticNode>| {
            nodes
                .into_iter()
                .map(|node| entry(ui, node))
                .collect::<Vec<_>>()
                .join(",")
        };
        let placements = entries(
            controls
                .iter()
                .copied()
                .filter(|node| {
                    ui.command_id(node.id)
                        .is_some_and(|command| command.0 == "favorite")
                })
                .collect(),
        );
        let items = entries(
            controls
                .iter()
                .copied()
                .filter(|node| node.role == Role::MenuItem)
                .collect(),
        );
        let menus = visible
            .iter()
            .filter(|node| node.role == Role::Menu)
            .map(|node| rectangle(node.bounds))
            .collect::<Vec<_>>()
            .join(",");
        let presentations = ribbon
            .presentations()
            .iter()
            .map(|presentation| {
                let (kind, groups) = match presentation {
                    RibbonPresentation::Full(group) => ("full", vec![group.as_str()]),
                    RibbonPresentation::Collapsed(group) => ("collapsed", vec![group.as_str()]),
                    RibbonPresentation::Overflow(groups) => {
                        ("overflow", groups.iter().map(String::as_str).collect())
                    }
                };
                format!(
                    "{{\"kind\":{},\"groups\":[{}]}}",
                    json_string(kind),
                    groups
                        .into_iter()
                        .map(json_string)
                        .collect::<Vec<_>>()
                        .join(",")
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        let theme = match ui.theme() {
            Theme::Light => "light",
            Theme::Dark => "dark",
            Theme::Compact => "compact",
            Theme::FluentLight => "fluent-light",
            Theme::FluentDark => "fluent-dark",
            Theme::HighContrastLight => "high-contrast-light",
            Theme::HighContrastDark => "high-contrast-dark",
            Theme::Configured(_) => "configured",
        };
        format!(
            "{{\"sequence\":{},\"checked\":{},\"theme\":{},\"viewport\":[{},{}],\"placements\":[{}],\"popup_open\":{},\"popup_bounds\":[{}],\"items\":[{}],\"ribbon\":[{}],\"controls\":[{}]}}",
            self.sequence,
            optional_bool(ui.commands().checked(&"favorite".into())),
            json_string(theme),
            viewport.width,
            viewport.height,
            placements,
            ui.popup_open(),
            menus,
            items,
            presentations,
            entries(controls)
        )
    }
}

fn entry(ui: &Controls, node: &SemanticNode) -> String {
    let command = ui
        .command_id(node.id)
        .map_or_else(|| "null".into(), |id| json_string(&id.0));
    format!(
        "{{\"command\":{},\"label\":{},\"role\":{},\"checked\":{},\"enabled\":{},\"bounds\":{},\"clip\":{},\"key_tip\":{}}}",
        command,
        json_string(&node.label),
        json_string(&format!("{:?}", node.role)),
        optional_bool(node.checked),
        !node.disabled,
        rectangle(node.bounds),
        ui.runtime
            .clip(node.id)
            .map_or_else(|| "null".into(), rectangle),
        ui.key_tip(node.id)
            .map_or_else(|| "null".into(), json_string)
    )
}

fn rectangle(bounds: Rect) -> String {
    format!(
        "[{},{},{},{}]",
        bounds.x, bounds.y, bounds.width, bounds.height
    )
}

fn optional_bool(value: Option<bool>) -> &'static str {
    match value {
        Some(true) => "true",
        Some(false) => "false",
        None => "null",
    }
}

fn json_string(value: &str) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnostics_escape_labels_as_json_without_losing_unicode() {
        assert_eq!(
            json_string("Книга\n\"\\\t\r\0\u{1f}"),
            "\"Книга\\n\\\"\\\\\\t\\r\\u0000\\u001f\""
        );
    }
}
