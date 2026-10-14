//! Static command references for the compiled Ribbon profile.
use crate::{
    Diagnostic, ElementDefinition, ElementKind, IconSymbol, MAX_ELEMENT_DEPTH, MAX_ELEMENTS,
    MAX_MENU_DEPTH, MAX_TOOLTIP_BYTES, UI_NAMESPACE, at_attribute, at_node, icon_symbol,
    identifier, reject_extension, require_element, require_empty_content,
};
use roxmltree::Node;
use std::collections::HashSet;

/// A single Ribbon in a Window. Command labels, availability and checked state
/// belong to the application's command registry, not to this definition.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RibbonDefinition {
    pub selected_tab: Option<String>,
    /// Initial minimized state; the runtime owns subsequent reveal transitions.
    pub is_minimized: bool,
    pub quick_access: Vec<String>,
    pub tabs: Vec<RibbonTabDefinition>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct RibbonTabDefinition {
    pub id: String,
    pub header: String,
    pub key_tip: String,
    pub context: Option<String>,
    pub groups: Vec<RibbonGroupDefinition>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct RibbonGroupDefinition {
    pub id: String,
    pub header: String,
    pub collapse_priority: u16,
    pub items: Vec<RibbonItemDefinition>,
}

/// A command placement. SplitButton and DropDownButton own nonempty menus.
/// SplitButton's primary command and secondary menu activate independently;
/// DropDownButton uses its command for presentation and opens only its menu.
#[derive(Clone, Debug, PartialEq)]
pub struct RibbonItemDefinition {
    pub command: String,
    pub key_tip: String,
    /// Preferred expanded presentation: Large or Small.
    pub size: RibbonItemSize,
    /// Smallest permitted adaptive presentation, defaulting to IconOnly.
    pub min_size: RibbonItemSize,
    pub kind: RibbonItemKindDefinition,
    pub icon: Option<IconSymbol>,
    pub tool_tip: Option<String>,
    pub menu: Vec<RibbonMenuEntryDefinition>,
}

impl Default for RibbonItemDefinition {
    fn default() -> Self {
        Self {
            command: String::new(),
            key_tip: String::new(),
            size: RibbonItemSize::Large,
            min_size: RibbonItemSize::IconOnly,
            kind: RibbonItemKindDefinition::Button,
            icon: None,
            tool_tip: None,
            menu: Vec::new(),
        }
    }
}

/// Command leaves use the registry's current label and state. A submenu has its
/// own static header and children; it never executes a command itself.
#[derive(Clone, Debug, PartialEq)]
pub enum RibbonMenuEntryDefinition {
    Command(String),
    Submenu {
        header: String,
        items: Vec<RibbonMenuEntryDefinition>,
    },
    Separator,
}

/// Validate a manually constructed Ribbon before mounting it. This checks the
/// static structure and Ribbon-local limits, not command registration, toggle
/// state or presentation conflicts. A host must additionally count the Ribbon's
/// nodes and depth together with the rest of its Window content.
pub fn validate_ribbon(ribbon: &RibbonDefinition) -> Result<(), String> {
    let mut count = 1 + usize::from(!ribbon.quick_access.is_empty());
    let mut text_bytes = 0_usize;
    let mut text = |value: &str| {
        text_bytes = text_bytes.saturating_add(value.len());
        if text_bytes > crate::MAX_SOURCE_BYTES {
            Err("Ribbon text exceeds the 1 MiB UTF-8 limit".to_owned())
        } else {
            Ok(())
        }
    };
    let mut qat = HashSet::new();
    for command in &ribbon.quick_access {
        check_identifier(command, "Command")?;
        text(command)?;
        if !qat.insert(command) {
            return Err("Quick access commands must be unique".into());
        }
        count += 1;
    }
    let mut tab_ids = HashSet::new();
    let mut tab_tips = Vec::new();
    for tab in &ribbon.tabs {
        count += 1;
        check_identifier(&tab.id, "RibbonTab Id")?;
        text(&tab.id)?;
        text(&tab.header)?;
        text(&tab.key_tip)?;
        if !tab_ids.insert(&tab.id) || tab.header.trim().is_empty() {
            return Err("RibbonTab IDs must be unique and headers must be nonempty".into());
        }
        let tip = tab.key_tip.to_ascii_uppercase();
        if !valid_key_tip(&tip, &tab_tips, true) {
            return Err("RibbonTab KeyTips must be ASCII alphanumeric, prefix-free and not start with QAT digits 1-9".into());
        }
        tab_tips.push(tip);
        if let Some(context) = &tab.context {
            check_identifier(context, "Context")?;
            text(context)?;
        }
        if tab.groups.is_empty() {
            return Err("RibbonTab requires a RibbonGroup".into());
        }
        let mut group_ids = HashSet::new();
        let mut item_tips = Vec::new();
        for group in &tab.groups {
            count += 1;
            check_identifier(&group.id, "RibbonGroup Id")?;
            text(&group.id)?;
            text(&group.header)?;
            if !group_ids.insert(&group.id) || group.header.trim().is_empty() {
                return Err(
                    "RibbonGroup IDs must be unique within a tab and headers must be nonempty"
                        .into(),
                );
            }
            if group.items.is_empty() {
                return Err("RibbonGroup requires at least one command item".into());
            }
            for item in &group.items {
                count += 1;
                validate_sizes(item.size, item.min_size)?;
                check_identifier(&item.command, "Command")?;
                text(&item.command)?;
                text(&item.key_tip)?;
                let tip = item.key_tip.to_ascii_uppercase();
                if !valid_key_tip(&tip, &item_tips, false) {
                    return Err("Ribbon item KeyTips must be ASCII alphanumeric and prefix-free within a tab".into());
                }
                item_tips.push(tip);
                if let Some(tool_tip) = &item.tool_tip {
                    if tool_tip.len() > MAX_TOOLTIP_BYTES {
                        return Err("ToolTip exceeds the 4096-byte UTF-8 limit".into());
                    }
                    text(tool_tip)?;
                }
                if matches!(
                    item.kind,
                    RibbonItemKindDefinition::SplitButton
                        | RibbonItemKindDefinition::DropDownButton
                ) {
                    if item.menu.is_empty() {
                        return Err(
                            "Ribbon SplitButton and DropDownButton require a nonempty menu".into(),
                        );
                    }
                } else if !item.menu.is_empty() {
                    return Err("Only Ribbon SplitButton and DropDownButton accept menus".into());
                }
                validate_menu_entries(&item.menu, 1, &mut count, &mut text)?;
                if count > MAX_ELEMENTS {
                    return Err("Ribbon exceeds the element count limit".into());
                }
            }
        }
    }
    if count > MAX_ELEMENTS {
        return Err("Ribbon exceeds the element count limit".into());
    }
    if !ribbon.tabs.iter().any(|tab| tab.context.is_none()) {
        return Err("Ribbon requires at least one ordinary RibbonTab without Context".into());
    }
    if let Some(selected) = &ribbon.selected_tab {
        check_identifier(selected, "SelectedTab")?;
        text(selected)?;
        if !ribbon
            .tabs
            .iter()
            .any(|tab| tab.id == *selected && tab.context.is_none())
        {
            return Err("SelectedTab must identify an ordinary RibbonTab without Context".into());
        }
    }
    Ok(())
}

fn validate_sizes(preferred: RibbonItemSize, minimum: RibbonItemSize) -> Result<(), String> {
    if preferred == RibbonItemSize::IconOnly
        || (preferred == RibbonItemSize::Small && minimum == RibbonItemSize::Large)
    {
        return Err("Ribbon Size must be Large or Small and MinSize must not exceed Size".into());
    }
    Ok(())
}

fn validate_menu_entries(
    entries: &[RibbonMenuEntryDefinition],
    depth: usize,
    count: &mut usize,
    text: &mut impl FnMut(&str) -> Result<(), String>,
) -> Result<(), String> {
    if !entries.is_empty() && depth > MAX_MENU_DEPTH {
        return Err("Ribbon menu exceeds the 8-level menu depth limit".into());
    }
    for entry in entries {
        *count += 1;
        if *count > MAX_ELEMENTS {
            return Err("Ribbon exceeds the element count limit".into());
        }
        match entry {
            RibbonMenuEntryDefinition::Command(command) => {
                check_identifier(command, "Command")?;
                text(command)?;
            }
            RibbonMenuEntryDefinition::Submenu { header, items } => {
                if header.trim().is_empty() || items.is_empty() {
                    return Err("Ribbon submenu requires a nonempty Header and children".into());
                }
                text(header)?;
                validate_menu_entries(items, depth + 1, count, text)?;
            }
            RibbonMenuEntryDefinition::Separator => {}
        }
    }
    Ok(())
}

fn check_identifier(value: &str, property: &str) -> Result<(), String> {
    let mut chars = value.chars();
    if !chars
        .next()
        .is_some_and(|value| value.is_ascii_alphabetic() || value == '_')
        || !chars.all(|value| value.is_ascii_alphanumeric() || value == '_')
    {
        return Err(format!("{property} must match [A-Za-z_][A-Za-z0-9_]*"));
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum RibbonItemSize {
    #[default]
    Large,
    Small,
    /// Smallest adaptive presentation; accepted for MinSize, not preferred Size.
    /// Without an icon the renderer retains a text fallback.
    IconOnly,
}

/// ToggleButton requires a command already registered as a toggle at mount.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum RibbonItemKindDefinition {
    #[default]
    Button,
    ToggleButton,
    SplitButton,
    DropDownButton,
}

pub(super) fn count(element: &ElementDefinition) -> usize {
    match &element.kind {
        ElementKind::Ribbon(_) => 1,
        ElementKind::StackPanel(panel) => panel.children.iter().map(count).sum(),
        _ => 0,
    }
}

fn bounded(node: Node<'_, '_>, depth: usize, count: &mut usize) -> Result<(), Diagnostic> {
    *count += 1;
    if depth > MAX_ELEMENT_DEPTH || *count > MAX_ELEMENTS {
        return Err(at_node(
            node,
            "Ribbon exceeds the element count or depth limit",
        ));
    }
    if node.tag_name().namespace() != Some(UI_NAMESPACE) {
        return Err(at_node(node, "Ribbon children require the UI namespace"));
    }
    Ok(())
}

fn checked_attributes(node: Node<'_, '_>) -> Result<(), Diagnostic> {
    for attribute in node.attributes() {
        reject_extension(node.document(), attribute)?;
        if attribute.namespace().is_some() {
            return Err(at_attribute(
                node.document(),
                attribute,
                "Ribbon model children accept no namespaced properties or x:Name",
            ));
        }
    }
    Ok(())
}

fn nonempty(node: Node<'_, '_>, value: &str, property: &str) -> Result<(), Diagnostic> {
    if value.trim().is_empty() {
        return Err(at_node(
            node,
            format!("{} requires a nonempty {property}", node.tag_name().name()),
        ));
    }
    Ok(())
}

fn key_tip(
    node: Node<'_, '_>,
    candidate: &str,
    existing: &[String],
    tab: bool,
) -> Result<(), Diagnostic> {
    if !valid_key_tip(candidate, existing, tab) {
        return Err(at_node(
            node,
            if tab {
                "RibbonTab KeyTips must be nonempty ASCII alphanumeric, prefix-free and not start with reserved QAT digits 1-9"
            } else {
                "Ribbon item KeyTips must be nonempty ASCII alphanumeric and prefix-free within a tab"
            },
        ));
    }
    Ok(())
}

fn valid_key_tip(candidate: &str, existing: &[String], tab: bool) -> bool {
    !(candidate.is_empty()
        || !candidate.chars().all(|value| value.is_ascii_alphanumeric())
        || existing
            .iter()
            .any(|value| value.starts_with(candidate) || candidate.starts_with(value))
        || (tab && candidate.starts_with(['1', '2', '3', '4', '5', '6', '7', '8', '9'])))
}

pub(super) fn parse_children(
    node: Node<'_, '_>,
    depth: usize,
    count: &mut usize,
    ribbon: &mut RibbonDefinition,
) -> Result<(), Diagnostic> {
    let mut qat_seen = false;
    let mut ids = HashSet::new();
    let mut tips = Vec::new();
    for child in node.children() {
        if !child.is_element() {
            require_empty_content(child)?;
            continue;
        }
        bounded(child, depth + 1, count)?;
        if child.tag_name().name() == "Ribbon.QuickAccessToolbar" {
            if qat_seen {
                return Err(at_node(
                    child,
                    "Ribbon accepts one Ribbon.QuickAccessToolbar",
                ));
            }
            qat_seen = true;
            if child.attributes().len() != 0 {
                return Err(at_node(
                    child,
                    "Ribbon.QuickAccessToolbar accepts no attributes",
                ));
            }
            let mut commands = HashSet::new();
            for reference in child.children() {
                if reference.is_element() {
                    bounded(reference, depth + 2, count)?;
                    let command = command_reference(reference, "Button")?;
                    if !commands.insert(command.clone()) {
                        return Err(at_node(reference, "Quick access commands must be unique"));
                    }
                    ribbon.quick_access.push(command);
                } else {
                    require_empty_content(reference)?;
                }
            }
        } else {
            let tab = parse_tab(child, depth + 1, count)?;
            if !ids.insert(tab.id.clone()) {
                return Err(at_node(
                    child,
                    "RibbonTab Id must be unique within a Ribbon",
                ));
            }
            key_tip(child, &tab.key_tip, &tips, true)?;
            tips.push(tab.key_tip.clone());
            ribbon.tabs.push(tab);
        }
    }
    if !ribbon.tabs.iter().any(|tab| tab.context.is_none()) {
        return Err(at_node(
            node,
            "Ribbon requires at least one ordinary RibbonTab without Context",
        ));
    }
    if let Some(selected) = &ribbon.selected_tab
        && !ribbon
            .tabs
            .iter()
            .any(|tab| tab.id == *selected && tab.context.is_none())
    {
        return Err(at_node(
            node,
            "SelectedTab must identify an ordinary RibbonTab without Context",
        ));
    }
    validate_ribbon(ribbon).map_err(|message| at_node(node, message))
}

fn parse_tab(
    node: Node<'_, '_>,
    depth: usize,
    count: &mut usize,
) -> Result<RibbonTabDefinition, Diagnostic> {
    require_element(node, "RibbonTab")?;
    checked_attributes(node)?;
    let mut tab = RibbonTabDefinition::default();
    for attribute in node.attributes() {
        match attribute.name() {
            "Id" => {
                identifier(node.document(), attribute, "Id")?;
                tab.id = attribute.value().into();
            }
            "Header" => tab.header = attribute.value().into(),
            "KeyTip" => tab.key_tip = attribute.value().to_ascii_uppercase(),
            "Context" => {
                identifier(node.document(), attribute, "Context")?;
                tab.context = Some(attribute.value().into());
            }
            _ => {
                return Err(at_attribute(
                    node.document(),
                    attribute,
                    "Unsupported RibbonTab property",
                ));
            }
        }
    }
    nonempty(node, &tab.id, "Id")?;
    nonempty(node, &tab.header, "Header")?;
    let mut ids = HashSet::new();
    let mut tips = Vec::new();
    for child in node.children() {
        if child.is_element() {
            bounded(child, depth + 1, count)?;
            let group = parse_group(child, depth + 1, count, &mut tips)?;
            if !ids.insert(group.id.clone()) {
                return Err(at_node(child, "RibbonGroup Id must be unique within a tab"));
            }
            tab.groups.push(group);
        } else {
            require_empty_content(child)?;
        }
    }
    if tab.groups.is_empty() {
        return Err(at_node(node, "RibbonTab requires a RibbonGroup"));
    }
    Ok(tab)
}

fn parse_group(
    node: Node<'_, '_>,
    depth: usize,
    count: &mut usize,
    tips: &mut Vec<String>,
) -> Result<RibbonGroupDefinition, Diagnostic> {
    require_element(node, "RibbonGroup")?;
    checked_attributes(node)?;
    let mut group = RibbonGroupDefinition::default();
    for attribute in node.attributes() {
        match attribute.name() {
            "Id" => {
                identifier(node.document(), attribute, "Id")?;
                group.id = attribute.value().into();
            }
            "Header" => group.header = attribute.value().into(),
            "CollapsePriority" => {
                let value = attribute.value();
                if value.is_empty() || !value.bytes().all(|value| value.is_ascii_digit()) {
                    return Err(at_attribute(
                        node.document(),
                        attribute,
                        "CollapsePriority must be an integer in 0..=65535",
                    ));
                }
                group.collapse_priority = value.parse().map_err(|_| {
                    at_attribute(
                        node.document(),
                        attribute,
                        "CollapsePriority must be an integer in 0..=65535",
                    )
                })?;
            }
            _ => {
                return Err(at_attribute(
                    node.document(),
                    attribute,
                    "Unsupported RibbonGroup property",
                ));
            }
        }
    }
    nonempty(node, &group.id, "Id")?;
    nonempty(node, &group.header, "Header")?;
    for child in node.children() {
        if child.is_element() {
            bounded(child, depth + 1, count)?;
            let item = parse_item(child, depth + 1, count)?;
            key_tip(child, &item.key_tip, tips, false)?;
            tips.push(item.key_tip.clone());
            group.items.push(item);
        } else {
            require_empty_content(child)?;
        }
    }
    if group.items.is_empty() {
        return Err(at_node(
            node,
            "RibbonGroup requires at least one command item",
        ));
    }
    Ok(group)
}

fn parse_item(
    node: Node<'_, '_>,
    depth: usize,
    count: &mut usize,
) -> Result<RibbonItemDefinition, Diagnostic> {
    checked_attributes(node)?;
    let kind = match node.tag_name().name() {
        "Button" => RibbonItemKindDefinition::Button,
        "ToggleButton" => RibbonItemKindDefinition::ToggleButton,
        "SplitButton" => RibbonItemKindDefinition::SplitButton,
        "DropDownButton" => RibbonItemKindDefinition::DropDownButton,
        _ => {
            return Err(at_node(
                node,
                "RibbonGroup supports Button, ToggleButton, SplitButton and DropDownButton command items",
            ));
        }
    };
    let mut item = RibbonItemDefinition {
        kind,
        ..Default::default()
    };
    for attribute in node.attributes() {
        match attribute.name() {
            "Command" => {
                identifier(node.document(), attribute, "Command")?;
                item.command = attribute.value().into();
            }
            "KeyTip" => item.key_tip = attribute.value().to_ascii_uppercase(),
            "Size" => {
                item.size = match attribute.value() {
                    "Large" => RibbonItemSize::Large,
                    "Small" => RibbonItemSize::Small,
                    _ => {
                        return Err(at_attribute(
                            node.document(),
                            attribute,
                            "Ribbon Size must be Large or Small",
                        ));
                    }
                };
            }
            "MinSize" => {
                item.min_size = match attribute.value() {
                    "Large" => RibbonItemSize::Large,
                    "Small" => RibbonItemSize::Small,
                    "IconOnly" => RibbonItemSize::IconOnly,
                    _ => {
                        return Err(at_attribute(
                            node.document(),
                            attribute,
                            "Ribbon MinSize must be Large, Small or IconOnly",
                        ));
                    }
                };
            }
            "Icon" => item.icon = Some(icon_symbol(node.document(), attribute)?),
            "ToolTip" => {
                if attribute.value().len() > MAX_TOOLTIP_BYTES {
                    return Err(at_attribute(
                        node.document(),
                        attribute,
                        "ToolTip exceeds the 4096-byte UTF-8 limit",
                    ));
                }
                item.tool_tip = Some(attribute.value().into());
            }
            _ => {
                return Err(at_attribute(
                    node.document(),
                    attribute,
                    "Unsupported Ribbon item property; labels and state belong to CommandRegistry",
                ));
            }
        }
    }
    validate_sizes(item.size, item.min_size).map_err(|message| at_node(node, message))?;
    nonempty(node, &item.command, "Command")?;
    for child in node.children() {
        if child.is_element() {
            if !matches!(
                kind,
                RibbonItemKindDefinition::SplitButton | RibbonItemKindDefinition::DropDownButton
            ) {
                return Err(at_node(
                    child,
                    "Only Ribbon SplitButton and DropDownButton accept menus",
                ));
            }
            item.menu
                .push(parse_menu_entry(child, depth + 1, 1, count)?);
        } else {
            require_empty_content(child)?;
        }
    }
    if matches!(
        kind,
        RibbonItemKindDefinition::SplitButton | RibbonItemKindDefinition::DropDownButton
    ) && item.menu.is_empty()
    {
        return Err(at_node(
            node,
            "Ribbon SplitButton and DropDownButton require a nonempty menu",
        ));
    }
    Ok(item)
}

fn parse_menu_entry(
    node: Node<'_, '_>,
    depth: usize,
    menu_depth: usize,
    count: &mut usize,
) -> Result<RibbonMenuEntryDefinition, Diagnostic> {
    bounded(node, depth, count)?;
    if menu_depth > MAX_MENU_DEPTH {
        return Err(at_node(
            node,
            "Ribbon menu exceeds the 8-level menu depth limit",
        ));
    }
    checked_attributes(node)?;
    if matches!(node.tag_name().name(), "MenuSeparator" | "Separator") {
        if node.attributes().len() != 0 {
            return Err(at_node(
                node,
                "A Ribbon menu separator accepts no attributes",
            ));
        }
        for child in node.children() {
            if child.is_element() {
                return Err(at_node(
                    child,
                    "A Ribbon menu separator accepts no children",
                ));
            }
            require_empty_content(child)?;
        }
        return Ok(RibbonMenuEntryDefinition::Separator);
    }
    require_element(node, "MenuItem")?;
    let mut command = None;
    let mut header = None;
    for attribute in node.attributes() {
        match attribute.name() {
            "Command" => {
                identifier(node.document(), attribute, "Command")?;
                command = Some(attribute.value().to_owned());
            }
            "Header" => header = Some(attribute.value().to_owned()),
            _ => {
                return Err(at_attribute(
                    node.document(),
                    attribute,
                    "Ribbon MenuItem accepts Command for a leaf or Header for a submenu",
                ));
            }
        }
    }
    if let Some(command) = command {
        if header.is_some() {
            return Err(at_node(
                node,
                "Ribbon command leaves accept only Command; labels belong to CommandRegistry",
            ));
        }
        for child in node.children() {
            if child.is_element() {
                return Err(at_node(child, "A Ribbon command leaf accepts no children"));
            }
            require_empty_content(child)?;
        }
        return Ok(RibbonMenuEntryDefinition::Command(command));
    }
    let header = header
        .ok_or_else(|| at_node(node, "Ribbon MenuItem requires Command or a submenu Header"))?;
    nonempty(node, &header, "Header")?;
    let mut items = Vec::new();
    for child in node.children() {
        if child.is_element() {
            items.push(parse_menu_entry(child, depth + 1, menu_depth + 1, count)?);
        } else {
            require_empty_content(child)?;
        }
    }
    if items.is_empty() {
        return Err(at_node(node, "Ribbon submenu requires nonempty children"));
    }
    Ok(RibbonMenuEntryDefinition::Submenu { header, items })
}

fn command_reference(node: Node<'_, '_>, element: &str) -> Result<String, Diagnostic> {
    require_element(node, element)?;
    checked_attributes(node)?;
    let mut command = None;
    for attribute in node.attributes() {
        if attribute.name() != "Command" {
            return Err(at_attribute(
                node.document(),
                attribute,
                "A Ribbon command reference accepts only Command",
            ));
        }
        identifier(node.document(), attribute, "Command")?;
        command = Some(attribute.value().to_owned());
    }
    for child in node.children() {
        if child.is_element() {
            return Err(at_node(
                child,
                "A Ribbon command reference accepts no children",
            ));
        }
        require_empty_content(child)?;
    }
    command.ok_or_else(|| at_node(node, "A Ribbon command reference requires Command"))
}
