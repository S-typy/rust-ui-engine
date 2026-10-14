//! Command-reference menus shared by expanded and collapsed Ribbon placements.
use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RibbonMenuEntry {
    Command(CommandId),
    Submenu { header: String, items: Vec<Self> },
    Separator,
}

fn convert(ui: &Controls, entries: &[RibbonMenuEntry]) -> Vec<MenuEntry> {
    entries
        .iter()
        .map(|entry| match entry {
            RibbonMenuEntry::Command(command) => MenuEntry::Item(MenuItem::new(
                ui.commands().label(command).unwrap_or(&command.0),
                command.clone(),
            )),
            RibbonMenuEntry::Submenu { header, items } => MenuEntry::Submenu {
                label: header.clone(),
                items: convert(ui, items),
                enabled: true,
            },
            RibbonMenuEntry::Separator => MenuEntry::Separator,
        })
        .collect()
}

pub(super) fn item_entries(
    ui: &Controls,
    item: &RibbonItem,
) -> Result<Vec<MenuEntry>, ControlError> {
    if let RibbonItemKind::SplitMenu { menu } | RibbonItemKind::DropDown { menu } = &item.kind {
        // Bound recursion before converting application-owned trees. The shared
        // popup preflight subsequently checks generated labels and total bytes.
        let mut pending = vec![(menu.as_slice(), 1)];
        let mut count = 0usize;
        while let Some((entries, depth)) = pending.pop() {
            count = count.saturating_add(entries.len());
            if entries.is_empty() || depth > 8 || count > crate::MAX_MENU_ITEMS {
                return Err(ControlError::Invalid(
                    "Ribbon menus require nonempty levels, at most 8 levels and 256 entries".into(),
                ));
            }
            for entry in entries {
                if let RibbonMenuEntry::Submenu { items, .. } = entry {
                    pending.push((items, depth + 1));
                }
            }
        }
    }
    Ok(match &item.kind {
        RibbonItemKind::Split { menu } => menu
            .iter()
            .map(|command| {
                MenuEntry::Item(MenuItem::new(
                    ui.commands().label(command).unwrap_or(&command.0),
                    command.clone(),
                ))
            })
            .collect(),
        RibbonItemKind::SplitMenu { menu } | RibbonItemKind::DropDown { menu } => convert(ui, menu),
        _ => Vec::new(),
    })
}

pub(super) fn group_entries(
    ui: &Controls,
    group: &RibbonGroup,
) -> Result<Vec<MenuEntry>, ControlError> {
    let mut entries = Vec::new();
    for item in &group.items {
        let label = ui
            .commands()
            .label(&item.command)
            .unwrap_or(&item.command.0);
        if matches!(item.kind, RibbonItemKind::DropDown { .. }) {
            entries.push(MenuEntry::CommandSubmenu {
                command: item.command.clone(),
                items: item_entries(ui, item)?,
            });
            continue;
        }
        entries.push(MenuEntry::Item(MenuItem::new(
            format!("{} / {label}", group.label),
            item.command.clone(),
        )));
        match &item.kind {
            // Keep the legacy flat overflow representation source-compatible.
            RibbonItemKind::Split { menu } => entries.extend(menu.iter().map(|command| {
                MenuEntry::Item(MenuItem::new(
                    format!(
                        "{} / {}",
                        group.label,
                        ui.commands().label(command).unwrap_or(&command.0)
                    ),
                    command.clone(),
                ))
            })),
            RibbonItemKind::SplitMenu { .. } => entries.push(MenuEntry::Submenu {
                label: format!("{label} menu"),
                items: item_entries(ui, item)?,
                enabled: true,
            }),
            _ => {}
        }
    }
    Ok(entries)
}
