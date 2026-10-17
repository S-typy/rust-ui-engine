//! Bounded, renderer-independent definitions for a docking workspace.
use crate::{
    Diagnostic, ElementDefinition, ElementKind, MAX_DIMENSION, MAX_ELEMENT_DEPTH, MAX_ELEMENTS,
    MenuEntryDefinition, Orientation, RadioGroupKey, RibbonMenuEntryDefinition, UI_NAMESPACE,
    WindowDefinition, at_attribute, at_node, boolean, editor_dimension, identifier, parse_element,
    reject_extension, require_empty_content,
};
use roxmltree::Node;
use std::collections::{HashMap, HashSet};

/// Maximum persistent panes in one workspace, including initially inactive tabs.
pub const MAX_DOCK_PANES: usize = 64;
const MAX_ID_BYTES: usize = 128;
const MAX_HEADER_BYTES: usize = 4096;

/// Initial docked layout. Subsequent moves, floating windows and restored layouts
/// belong to the runtime; they do not recreate the pane's content definition.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DockWorkspaceDefinition {
    pub root: DockNodeDefinition,
}

/// A binary split or a tab group. Group identities are assigned by the runtime.
#[derive(Clone, Debug, PartialEq)]
pub enum DockNodeDefinition {
    Split {
        orientation: Orientation,
        /// Fraction allocated to the first child, in the inclusive range 0.1..=0.9.
        ratio: f32,
        first: Box<DockNodeDefinition>,
        second: Box<DockNodeDefinition>,
    },
    Group {
        /// `None` initially selects the first pane.
        selected_pane: Option<String>,
        panes: Vec<DockPaneDefinition>,
    },
}

impl Default for DockNodeDefinition {
    fn default() -> Self {
        Self::Group {
            selected_pane: None,
            panes: Vec::new(),
        }
    }
}

/// One persistent pane. `id` is independent of the content's global XAML names.
#[derive(Clone, Debug, PartialEq)]
pub struct DockPaneDefinition {
    pub id: String,
    pub header: String,
    pub can_close: bool,
    pub min_width: f32,
    pub min_height: f32,
    pub content: Box<ElementDefinition>,
}

impl DockWorkspaceDefinition {
    /// Pane definitions in initial left-to-right, top-to-bottom tab order.
    /// Validate manually constructed definitions before using their contents.
    pub fn panes(&self) -> Vec<&DockPaneDefinition> {
        let mut pending = vec![&self.root];
        let mut panes = Vec::new();
        while let Some(node) = pending.pop() {
            match node {
                DockNodeDefinition::Split { first, second, .. } => {
                    pending.push(second);
                    pending.push(first);
                }
                DockNodeDefinition::Group { panes: group, .. } => panes.extend(group),
            }
        }
        panes
    }
}

/// Validate workspace structure and all nested content budgets without creating
/// controls. Nested workspaces and Ribbon controls inside panes are unsupported.
/// Command registration and individual control settings remain the host's job.
pub fn validate_docking(workspace: &DockWorkspaceDefinition) -> Result<(), String> {
    validate_entries(vec![(Entry::Dock(&workspace.root), 2, true)], 1, false)?;
    for pane in workspace.panes() {
        crate::validate_status_bars(&pane.content)?;
    }
    Ok(())
}

/// Validate a complete Window's docking structure, shared names and element/depth
/// budgets. At most one workspace is supported; ordinary forms may contain none.
/// This also checks manually constructed definitions before any pane is mounted.
pub fn validate_window_docking(window: &WindowDefinition) -> Result<(), String> {
    validate_entries(vec![(Entry::Element(&window.content), 1, false)], 0, true)?;
    crate::validate_status_bars(&window.content)
}

enum Entry<'a> {
    Element(&'a ElementDefinition),
    Dock(&'a DockNodeDefinition),
    Pane(&'a DockPaneDefinition),
    Menu(&'a MenuEntryDefinition),
    RibbonMenu(&'a RibbonMenuEntryDefinition),
    Count,
}

fn validate_entries(
    mut pending: Vec<(Entry<'_>, usize, bool)>,
    mut count: usize,
    window_scope: bool,
) -> Result<(), String> {
    let mut workspaces = 0;
    let mut pane_ids = HashSet::new();
    let mut names = HashSet::new();
    let mut radio_owners = HashMap::new();
    while let Some((entry, depth, in_pane)) = pending.pop() {
        count += 1;
        if count > MAX_ELEMENTS || depth > MAX_ELEMENT_DEPTH {
            return Err("XAML docking content exceeds the element count or depth limit".into());
        }
        match entry {
            Entry::Element(element) => {
                crate::validate_badge(&element.common)?;
                match &element.kind {
                    ElementKind::GridControl(grid) => {
                        crate::validate_grid_control(grid)?;
                        crate::validate_grid_control_bindings(&element.common)?;
                    }
                    ElementKind::GroupBox(group) => {
                        crate::validate_group_box(group)?;
                        if !group.is_collapsible
                            && element.common.bindings.contains_key("IsExpanded")
                        {
                            return Err(
                                "GroupBox IsExpanded binding requires IsCollapsible=True".into()
                            );
                        }
                    }
                    ElementKind::Expander(expander) => crate::validate_expander(expander)?,
                    ElementKind::TabbedGroup(group) => crate::validate_tabbed_group(group)?,
                    _ => {}
                }
                if let Some(name) = &element.common.name
                    && !names.insert(name)
                {
                    return Err(format!("Duplicate XAML name '{name}' in this Window"));
                }
                match &element.kind {
                    ElementKind::GridControl(grid) => {
                        check_pending(grid.columns.len(), count)?;
                        pending.extend(
                            grid.columns
                                .iter()
                                .map(|_| (Entry::Count, depth + 1, in_pane)),
                        );
                    }
                    ElementKind::TabbedGroup(group) => {
                        check_pending(group.tabs.len(), count)?;
                        for tab in &group.tabs {
                            pending.push((Entry::Count, depth + 1, in_pane));
                            if let Some(child) = &tab.child {
                                pending.push((Entry::Element(child), depth + 2, in_pane));
                            }
                        }
                    }
                    ElementKind::DockWorkspace(workspace) => {
                        if in_pane {
                            return Err("DockPane content cannot contain a DockWorkspace".into());
                        }
                        workspaces += 1;
                        if workspaces > 1 {
                            return Err("Window supports at most one DockWorkspace".into());
                        }
                        pending.push((Entry::Dock(&workspace.root), depth + 1, true));
                    }
                    ElementKind::Ribbon(ribbon) => {
                        if in_pane {
                            return Err("DockPane content cannot contain a Ribbon".into());
                        }
                        // XML counts these internal elements as well as content.
                        crate::validate_ribbon(ribbon)?;
                        if !ribbon.quick_access.is_empty() {
                            pending.push((Entry::Count, depth + 1, false));
                            pending.extend(
                                ribbon
                                    .quick_access
                                    .iter()
                                    .map(|_| (Entry::Count, depth + 2, false)),
                            );
                        }
                        for tab in &ribbon.tabs {
                            pending.push((Entry::Count, depth + 1, false));
                            for group in &tab.groups {
                                pending.push((Entry::Count, depth + 2, false));
                                for item in &group.items {
                                    pending.push((Entry::Count, depth + 3, false));
                                    pending.extend(
                                        item.menu.iter().map(|entry| {
                                            (Entry::RibbonMenu(entry), depth + 4, false)
                                        }),
                                    );
                                }
                            }
                        }
                    }
                    ElementKind::DropDownButton(menu) | ElementKind::SplitButton(menu) => {
                        check_pending(menu.items.len(), count)?;
                        pending.extend(
                            menu.items
                                .iter()
                                .map(|entry| (Entry::Menu(entry), depth + 1, in_pane)),
                        );
                    }
                    _ => {
                        let children = element.children();
                        check_pending(children.len(), count)?;
                        pending.extend(
                            children
                                .into_iter()
                                .map(|child| (Entry::Element(child), depth + 1, in_pane)),
                        );
                    }
                }
            }
            Entry::Dock(DockNodeDefinition::Split {
                ratio,
                first,
                second,
                ..
            }) => {
                if !ratio.is_finite() || !(0.1..=0.9).contains(ratio) {
                    return Err("DockSplit Ratio must be finite and in [0.1, 0.9]".into());
                }
                pending.push((Entry::Dock(second), depth + 1, true));
                pending.push((Entry::Dock(first), depth + 1, true));
            }
            Entry::Dock(DockNodeDefinition::Group {
                selected_pane,
                panes,
            }) => {
                if panes.is_empty() {
                    return Err("DockGroup requires at least one DockPane".into());
                }
                if panes.len() > MAX_DOCK_PANES {
                    return Err("DockWorkspace exceeds the 64-pane limit".into());
                }
                if let Some(selected) = selected_pane
                    && !panes.iter().any(|pane| pane.id == *selected)
                {
                    return Err("SelectedPane must identify a DockPane in its own DockGroup".into());
                }
                pending.extend(
                    panes
                        .iter()
                        .map(|pane| (Entry::Pane(pane), depth + 1, true)),
                );
            }
            Entry::Pane(pane) => {
                validate_id(&pane.id)?;
                if !pane_ids.insert(&pane.id) {
                    return Err(format!("Duplicate DockPane Id '{}'", pane.id));
                }
                if pane_ids.len() > MAX_DOCK_PANES {
                    return Err("DockWorkspace exceeds the 64-pane limit".into());
                }
                if pane.header.trim().is_empty() || pane.header.len() > MAX_HEADER_BYTES {
                    return Err(
                        "DockPane Header must be nonempty and at most 4096 UTF-8 bytes".into(),
                    );
                }
                for value in [pane.min_width, pane.min_height] {
                    if !value.is_finite() || value <= 0.0 || value > MAX_DIMENSION as f32 {
                        return Err(
                            "DockPane minimum dimensions must be finite and in (0, 16384]".into(),
                        );
                    }
                }
                validate_radio_owners(pane, &mut radio_owners)?;
                pending.push((Entry::Element(&pane.content), depth + 1, true));
            }
            Entry::Menu(MenuEntryDefinition::Item(item)) => {
                check_pending(item.items.len(), count)?;
                pending.extend(
                    item.items
                        .iter()
                        .map(|item| (Entry::Menu(item), depth + 1, in_pane)),
                );
            }
            Entry::RibbonMenu(RibbonMenuEntryDefinition::Submenu { items, .. }) => {
                check_pending(items.len(), count)?;
                pending.extend(
                    items
                        .iter()
                        .map(|item| (Entry::RibbonMenu(item), depth + 1, false)),
                );
            }
            Entry::Menu(_) | Entry::RibbonMenu(_) | Entry::Count => {}
        }
        if pending.len() > MAX_ELEMENTS {
            return Err("XAML docking content exceeds the element count limit".into());
        }
    }
    if !window_scope && pane_ids.is_empty() {
        return Err("DockWorkspace requires at least one DockPane".into());
    }
    Ok(())
}

fn check_pending(length: usize, count: usize) -> Result<(), String> {
    if length > MAX_ELEMENTS.saturating_sub(count) {
        Err("XAML docking content exceeds the element count limit".into())
    } else {
        Ok(())
    }
}

fn validate_radio_owners<'a>(
    pane: &'a DockPaneDefinition,
    owners: &mut HashMap<&'a str, &'a str>,
) -> Result<(), String> {
    let mut pending = vec![pane.content.as_ref()];
    let mut count = 0;
    while let Some(element) = pending.pop() {
        count += 1;
        if count > MAX_ELEMENTS {
            return Err("XAML docking content exceeds the element count limit".into());
        }
        match &element.kind {
            ElementKind::RadioButton(radio) => {
                if let Some(group) = &radio.group_name
                    && owners
                        .insert(group, &pane.id)
                        .is_some_and(|owner| owner != pane.id)
                {
                    return Err(format!(
                        "RadioButton GroupName '{group}' cannot span multiple DockPane contents"
                    ));
                }
            }
            _ => {
                let children = element.children();
                check_pending(children.len(), count)?;
                pending.extend(children);
            }
        }
    }
    Ok(())
}

fn validate_id(id: &str) -> Result<(), String> {
    let mut chars = id.chars();
    if id.len() > MAX_ID_BYTES
        || !chars
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        || !chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        return Err("DockPane Id must match [A-Za-z_][A-Za-z0-9_]* and fit 128 UTF-8 bytes".into());
    }
    Ok(())
}

pub(super) fn parse_workspace(
    node: Node<'_, '_>,
    depth: usize,
    count: &mut usize,
    names: &mut HashSet<String>,
    checked_radio_groups: &mut HashSet<RadioGroupKey>,
) -> Result<DockWorkspaceDefinition, Diagnostic> {
    let children = children(node)?;
    if children.len() != 1 {
        return Err(at_node(
            node,
            "DockWorkspace requires exactly one DockSplit or DockGroup",
        ));
    }
    let root = parse_node(children[0], depth + 1, count, names, checked_radio_groups)?;
    let workspace = DockWorkspaceDefinition { root };
    validate_docking(&workspace).map_err(|message| at_node(node, message))?;
    Ok(workspace)
}

fn parse_node(
    node: Node<'_, '_>,
    depth: usize,
    count: &mut usize,
    names: &mut HashSet<String>,
    checked_radio_groups: &mut HashSet<RadioGroupKey>,
) -> Result<DockNodeDefinition, Diagnostic> {
    bounded(node, depth, count)?;
    let document = node.document();
    let children = children(node)?;
    match node.tag_name().name() {
        "DockSplit" => {
            let mut orientation = Orientation::Horizontal;
            let mut ratio = 0.5;
            for attribute in node.attributes() {
                match attribute.name() {
                    "Orientation" => {
                        orientation = match attribute.value() {
                            "Horizontal" => Orientation::Horizontal,
                            "Vertical" => Orientation::Vertical,
                            _ => {
                                return Err(at_attribute(
                                    document,
                                    attribute,
                                    "Orientation must be Horizontal or Vertical",
                                ));
                            }
                        };
                    }
                    "Ratio" => {
                        ratio = attribute.value().parse::<f32>().map_err(|_| {
                            at_attribute(
                                document,
                                attribute,
                                "DockSplit Ratio must be a number in [0.1, 0.9]",
                            )
                        })?;
                    }
                    _ => {
                        return Err(at_attribute(
                            document,
                            attribute,
                            "unsupported DockSplit property",
                        ));
                    }
                }
            }
            if children.len() != 2 {
                return Err(at_node(
                    node,
                    "DockSplit requires exactly two DockSplit or DockGroup children",
                ));
            }
            Ok(DockNodeDefinition::Split {
                orientation,
                ratio,
                first: Box::new(parse_node(
                    children[0],
                    depth + 1,
                    count,
                    names,
                    checked_radio_groups,
                )?),
                second: Box::new(parse_node(
                    children[1],
                    depth + 1,
                    count,
                    names,
                    checked_radio_groups,
                )?),
            })
        }
        "DockGroup" => {
            let mut selected_pane = None;
            for attribute in node.attributes() {
                match attribute.name() {
                    "SelectedPane" => {
                        identifier(document, attribute, "SelectedPane")?;
                        selected_pane = Some(attribute.value().into());
                    }
                    _ => {
                        return Err(at_attribute(
                            document,
                            attribute,
                            "unsupported DockGroup property",
                        ));
                    }
                }
            }
            let mut panes = Vec::new();
            for child in children {
                panes.push(parse_pane(
                    child,
                    depth + 1,
                    count,
                    names,
                    checked_radio_groups,
                )?);
            }
            Ok(DockNodeDefinition::Group {
                selected_pane,
                panes,
            })
        }
        _ => Err(at_node(node, "expected DockSplit or DockGroup")),
    }
}

fn parse_pane(
    node: Node<'_, '_>,
    depth: usize,
    count: &mut usize,
    names: &mut HashSet<String>,
    checked_radio_groups: &mut HashSet<RadioGroupKey>,
) -> Result<DockPaneDefinition, Diagnostic> {
    bounded(node, depth, count)?;
    let pane_serial = *count;
    if node.tag_name().name() != "DockPane" {
        return Err(at_node(node, "DockGroup accepts only DockPane children"));
    }
    let document = node.document();
    let mut id = None;
    let mut header = None;
    let mut can_close = true;
    let mut min_width = 120.0;
    let mut min_height = 80.0;
    for attribute in node.attributes() {
        match attribute.name() {
            "Id" => {
                validate_id(attribute.value())
                    .map_err(|message| at_attribute(document, attribute, message))?;
                id = Some(attribute.value().to_owned());
            }
            "Header" => header = Some(attribute.value().to_owned()),
            "CanClose" => can_close = boolean(document, attribute)?,
            "MinWidth" => min_width = editor_dimension(document, attribute)?,
            "MinHeight" => min_height = editor_dimension(document, attribute)?,
            _ => {
                return Err(at_attribute(
                    document,
                    attribute,
                    "unsupported DockPane property",
                ));
            }
        }
    }
    let id = id.ok_or_else(|| at_node(node, "DockPane requires Id"))?;
    let children = children(node)?;
    if children.len() != 1 {
        return Err(at_node(
            node,
            "DockPane requires exactly one content element",
        ));
    }
    let content = parse_element(
        children[0],
        depth + 1,
        pane_serial,
        count,
        names,
        checked_radio_groups,
    )?;
    Ok(DockPaneDefinition {
        header: header.unwrap_or_else(|| id.clone()),
        id,
        can_close,
        min_width,
        min_height,
        content: Box::new(content),
    })
}

fn bounded(node: Node<'_, '_>, depth: usize, count: &mut usize) -> Result<(), Diagnostic> {
    *count += 1;
    if *count > MAX_ELEMENTS || depth > MAX_ELEMENT_DEPTH {
        return Err(at_node(
            node,
            "XAML docking content exceeds the element count or depth limit",
        ));
    }
    if node.tag_name().namespace() != Some(UI_NAMESPACE) {
        return Err(at_node(node, "Docking children require the UI namespace"));
    }
    for attribute in node.attributes() {
        reject_extension(node.document(), attribute)?;
        if attribute.namespace().is_some() {
            return Err(at_attribute(
                node.document(),
                attribute,
                "Docking model nodes accept no namespaced properties or x:Name",
            ));
        }
    }
    Ok(())
}

fn children<'a, 'input>(node: Node<'a, 'input>) -> Result<Vec<Node<'a, 'input>>, Diagnostic> {
    let mut result = Vec::new();
    for child in node.children() {
        if child.is_element() {
            result.push(child);
        } else {
            require_empty_content(child)?;
        }
    }
    Ok(result)
}
