//! Bridge from the bounded compiled Ribbon profile to retained command controls.
use rust_desktop_ui_controls::{
    ButtonIcon, Controls, Ribbon, RibbonBackstageItem, RibbonBackstageSpec,
    RibbonCommandAppearance, RibbonContentSpec, RibbonGroup, RibbonItem, RibbonItemKind,
    RibbonItemSize as RuntimeSize, RibbonMenuEntry, RibbonModel, RibbonTab,
};
use rust_desktop_ui_core::WidgetId;
use rust_desktop_ui_xaml::{
    RibbonBackstageItemDefinition, RibbonDefinition, RibbonItemKindDefinition, RibbonItemSize,
    RibbonMenuEntryDefinition, validate_ribbon,
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
                    RibbonItemKind::SplitMenu {
                        menu: menu_entries(&item.menu),
                    }
                } else if item.kind == RibbonItemKindDefinition::DropDownButton {
                    RibbonItemKind::DropDown {
                        menu: menu_entries(&item.menu),
                    }
                } else {
                    // Toggle behavior belongs to the registry, never a presentation hint.
                    match item.size {
                        RibbonItemSize::Large => RibbonItemKind::Large,
                        RibbonItemSize::Small => RibbonItemKind::Small,
                        RibbonItemSize::IconOnly => unreachable!("validated preferred size"),
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
    for tab in &definition.tabs {
        for item in tab.groups.iter().flat_map(|group| &group.items) {
            ribbon
                .set_item_size_limits(&tab.id, &item.key_tip, size(item.size), size(item.min_size))
                .map_err(err)?;
        }
        for group in &tab.groups {
            for slot in &group.contents {
                ribbon
                    .add_content(RibbonContentSpec {
                        id: slot.id.clone(),
                        tab_id: tab.id.clone(),
                        group_id: group.id.clone(),
                        label: slot.header.clone(),
                        key_tip: slot.key_tip.clone(),
                        preferred_width: slot.width,
                        minimum_width: slot.min_width,
                        height: slot.height,
                    })
                    .map_err(err)?;
            }
        }
    }
    ribbon
        .set_backstage(definition.backstage.as_ref().map(|backstage| {
            RibbonBackstageSpec {
                label: backstage.header.clone(),
                key_tip: backstage.key_tip.clone(),
                items: backstage
                    .items
                    .iter()
                    .map(|item| match item {
                        RibbonBackstageItemDefinition::Page {
                            id,
                            header,
                            key_tip,
                            ..
                        } => RibbonBackstageItem::Page {
                            id: id.clone(),
                            label: header.clone(),
                            key_tip: key_tip.clone(),
                        },
                        RibbonBackstageItemDefinition::Command { command, key_tip } => {
                            RibbonBackstageItem::Command {
                                command: command.as_str().into(),
                                key_tip: key_tip.clone(),
                            }
                        }
                    })
                    .collect(),
            }
        }))
        .map_err(err)?;
    ribbon.set_minimized(definition.is_minimized);
    Ok(ribbon)
}

pub(crate) fn configure_customization(
    ui: &mut Controls,
    ribbon: &mut Ribbon,
    definition: &RibbonDefinition,
) -> Result<(), String> {
    if definition.customize_quick_access {
        let mut seen = std::collections::HashSet::new();
        let catalog = commands(definition)
            .into_iter()
            .filter(|id| seen.insert(id.clone()))
            .map(Into::into)
            .collect();
        ribbon
            .set_quick_access_catalog(ui, Some(catalog))
            .map_err(err)?;
    }
    Ok(())
}

fn size(size: RibbonItemSize) -> RuntimeSize {
    match size {
        RibbonItemSize::Large => RuntimeSize::Large,
        RibbonItemSize::Small => RuntimeSize::Small,
        RibbonItemSize::IconOnly => RuntimeSize::IconOnly,
    }
}

fn menu_entries(entries: &[RibbonMenuEntryDefinition]) -> Vec<RibbonMenuEntry> {
    entries
        .iter()
        .map(|entry| match entry {
            RibbonMenuEntryDefinition::Command(command) => {
                RibbonMenuEntry::Command(command.as_str().into())
            }
            RibbonMenuEntryDefinition::Submenu { header, items } => RibbonMenuEntry::Submenu {
                header: header.clone(),
                items: menu_entries(items),
            },
            RibbonMenuEntryDefinition::Separator => RibbonMenuEntry::Separator,
        })
        .collect()
}

pub(crate) fn commands(definition: &RibbonDefinition) -> Vec<String> {
    fn collect(entries: &[RibbonMenuEntryDefinition], commands: &mut Vec<String>) {
        for entry in entries {
            match entry {
                RibbonMenuEntryDefinition::Command(command) => commands.push(command.clone()),
                RibbonMenuEntryDefinition::Submenu { items, .. } => collect(items, commands),
                RibbonMenuEntryDefinition::Separator => {}
            }
        }
    }
    let mut commands = definition.quick_access.clone();
    for item in definition
        .tabs
        .iter()
        .flat_map(|tab| &tab.groups)
        .flat_map(|group| &group.items)
    {
        commands.push(item.command.clone());
        collect(&item.menu, &mut commands);
    }
    if let Some(backstage) = &definition.backstage {
        commands.extend(backstage.items.iter().filter_map(|item| match item {
            RibbonBackstageItemDefinition::Command { command, .. } => Some(command.clone()),
            _ => None,
        }));
    }
    commands
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
            count += group.contents.len();
            if !group.contents.is_empty() {
                depth = depth.max(4);
            }
            for item in &group.items {
                count += 1;
                depth = depth.max(4);
                count_menu(&item.menu, 5, &mut count, &mut depth);
            }
        }
    }
    if let Some(backstage) = &definition.backstage {
        count += 2 + backstage.items.len();
        depth = depth.max(4);
    }
    (count, depth)
}

fn count_menu(
    entries: &[RibbonMenuEntryDefinition],
    level: usize,
    count: &mut usize,
    depth: &mut usize,
) {
    for entry in entries {
        *count += 1;
        *depth = (*depth).max(level);
        if let RibbonMenuEntryDefinition::Submenu { items, .. } = entry {
            count_menu(items, level + 1, count, depth);
        }
    }
}

fn err(error: impl std::fmt::Display) -> String {
    error.to_string()
}
