//! Bridge from the bounded compiled Ribbon profile to retained command controls.
use rust_desktop_ui_controls::{
    ButtonIcon, Controls, Ribbon, RibbonCommandAppearance, RibbonGroup, RibbonItem, RibbonItemKind,
    RibbonModel, RibbonTab,
};
use rust_desktop_ui_core::WidgetId;
use rust_desktop_ui_xaml::{
    RibbonDefinition, RibbonItemKindDefinition, RibbonItemSize, validate_ribbon,
};
use std::collections::HashMap;

pub(crate) struct MountedRibbon {
    pub(crate) wrapper: WidgetId,
    pub(crate) ribbon: Ribbon,
}

impl MountedRibbon {
    pub(crate) fn update(&mut self, ui: &mut Controls) -> Result<(), String> {
        let width = ui
            .runtime
            .bounds(self.wrapper)
            .map_or(0.0, |rect| rect.width);
        self.ribbon.update(ui, width).map_err(err)
    }
}

/// Validate references and appearance conflicts before creating any retained node.
pub(crate) fn build_ribbon(ui: &Controls, definition: &RibbonDefinition) -> Result<Ribbon, String> {
    validate_ribbon(definition)?;
    let mut appearances: HashMap<String, RibbonCommandAppearance> = HashMap::new();
    for command in commands(definition) {
        if !ui.commands().contains(&command.as_str().into()) {
            return Err(format!("Unregistered XAML Ribbon command '{command}'"));
        }
    }
    let mut tabs = Vec::new();
    for tab in &definition.tabs {
        let mut groups = Vec::new();
        for group in &tab.groups {
            let mut items = Vec::new();
            for item in &group.items {
                if item.kind == RibbonItemKindDefinition::ToggleButton
                    && ui
                        .commands()
                        .checked(&item.command.as_str().into())
                        .is_none()
                {
                    return Err(format!(
                        "Ribbon ToggleButton command '{}' requires a registered checked state",
                        item.command
                    ));
                }
                let appearance = appearances.entry(item.command.clone()).or_default();
                if let Some(symbol) = item.icon {
                    let icon = ButtonIcon::new(crate::view::icon_symbol(symbol));
                    if appearance.icon.as_ref().is_some_and(|old| *old != icon) {
                        return Err(format!(
                            "Conflicting Ribbon Icon for command '{}'",
                            item.command
                        ));
                    }
                    appearance.icon = Some(icon);
                }
                if let Some(tip) = &item.tool_tip {
                    if appearance.tool_tip.as_ref().is_some_and(|old| old != tip) {
                        return Err(format!(
                            "Conflicting Ribbon ToolTip for command '{}'",
                            item.command
                        ));
                    }
                    appearance.tool_tip = Some(tip.clone());
                }
                let kind = if item.kind == RibbonItemKindDefinition::SplitButton {
                    RibbonItemKind::Split {
                        menu: item.menu.iter().map(|id| id.as_str().into()).collect(),
                    }
                } else {
                    // Toggle behavior belongs to the registry, never a presentation hint.
                    match item.size {
                        RibbonItemSize::Large => RibbonItemKind::Large,
                        RibbonItemSize::Small => RibbonItemKind::Small,
                    }
                };
                items.push(RibbonItem::new(item.command.as_str(), kind, &item.key_tip));
            }
            groups.push(RibbonGroup {
                id: group.id.clone(),
                label: group.header.clone(),
                collapse_priority: group.collapse_priority,
                items,
            });
        }
        tabs.push(RibbonTab {
            id: tab.id.clone(),
            label: tab.header.clone(),
            key_tip: tab.key_tip.clone(),
            context: tab.context.clone(),
            groups,
        });
    }
    let mut ribbon = Ribbon::new(RibbonModel {
        tabs,
        quick_access: definition
            .quick_access
            .iter()
            .map(|id| id.as_str().into())
            .collect(),
    })
    .map_err(err)?;
    if let Some(active) = &definition.selected_tab
        && !ribbon.set_active_tab(active)
    {
        return Err(format!("SelectedTab '{active}' is not initially visible"));
    }
    for (command, appearance) in appearances {
        ribbon
            .set_command_appearance(command, appearance)
            .map_err(err)?;
    }
    Ok(ribbon)
}

pub(crate) fn commands(definition: &RibbonDefinition) -> Vec<String> {
    definition
        .quick_access
        .iter()
        .cloned()
        .chain(
            definition
                .tabs
                .iter()
                .flat_map(|tab| &tab.groups)
                .flat_map(|group| &group.items)
                .flat_map(|item| {
                    std::iter::once(item.command.clone()).chain(item.menu.iter().cloned())
                }),
        )
        .collect()
}

/// Nested model nodes count towards the same budget as ordinary XAML elements.
pub(crate) fn nested_size(definition: &RibbonDefinition) -> (usize, usize) {
    let mut count =
        definition.quick_access.len() + usize::from(!definition.quick_access.is_empty());
    let mut depth = 1;
    for tab in &definition.tabs {
        count += 1;
        for group in &tab.groups {
            count += 1;
            for item in &group.items {
                count += 1 + item.menu.len();
                depth = depth.max(if item.menu.is_empty() { 4 } else { 5 });
            }
        }
    }
    (count, depth)
}

fn err(error: impl std::fmt::Display) -> String {
    error.to_string()
}
