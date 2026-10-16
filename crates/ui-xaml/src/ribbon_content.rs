//! Retained Ribbon slots and Backstage content definitions.
use crate::ribbon::{bounded, checked_attributes, nonempty, valid_key_tip};
use crate::*;

#[derive(Clone, Debug, PartialEq)]
pub struct RibbonContentDefinition {
    pub id: String,
    pub header: String,
    pub key_tip: String,
    pub width: f32,
    pub min_width: f32,
    pub height: f32,
    pub content: Box<ElementDefinition>,
}
impl Default for RibbonContentDefinition {
    fn default() -> Self {
        Self {
            id: String::new(),
            header: String::new(),
            key_tip: String::new(),
            width: 180.0,
            min_width: 120.0,
            height: 84.0,
            content: Box::default(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct RibbonBackstageDefinition {
    pub header: String,
    pub key_tip: String,
    pub items: Vec<RibbonBackstageItemDefinition>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum RibbonBackstageItemDefinition {
    Page {
        id: String,
        header: String,
        key_tip: String,
        content: Box<ElementDefinition>,
    },
    Command {
        command: String,
        key_tip: String,
    },
}

impl RibbonDefinition {
    /// Retained subtrees share the containing Window's names and bindings.
    pub fn content_elements(&self) -> Vec<&ElementDefinition> {
        let mut result: Vec<_> = self
            .tabs
            .iter()
            .flat_map(|tab| &tab.groups)
            .flat_map(|group| &group.contents)
            .map(|slot| slot.content.as_ref())
            .collect();
        if let Some(backstage) = &self.backstage {
            result.extend(backstage.items.iter().filter_map(|item| match item {
                RibbonBackstageItemDefinition::Page { content, .. } => Some(content.as_ref()),
                _ => None,
            }));
        }
        result
    }
    pub fn content_elements_mut(&mut self) -> Vec<&mut ElementDefinition> {
        let mut result: Vec<_> = self
            .tabs
            .iter_mut()
            .flat_map(|tab| &mut tab.groups)
            .flat_map(|group| &mut group.contents)
            .map(|slot| slot.content.as_mut())
            .collect();
        if let Some(backstage) = &mut self.backstage {
            result.extend(backstage.items.iter_mut().filter_map(|item| match item {
                RibbonBackstageItemDefinition::Page { content, .. } => Some(content.as_mut()),
                _ => None,
            }));
        }
        result
    }
}

fn slot_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"_.-".contains(&c))
}

pub(super) fn validate_extensions(
    ribbon: &RibbonDefinition,
    count: &mut usize,
    text: &mut impl FnMut(&str) -> Result<(), String>,
) -> Result<(), String> {
    let mut ids = HashSet::new();
    for tab in &ribbon.tabs {
        let mut tips: Vec<_> = tab
            .groups
            .iter()
            .flat_map(|g| &g.items)
            .map(|item| item.key_tip.to_ascii_uppercase())
            .collect();
        for slot in tab.groups.iter().flat_map(|g| &g.contents) {
            *count += 1;
            if !slot_id(&slot.id) || !ids.insert(slot.id.as_str()) || ids.len() > 64 {
                return Err(
                    "RibbonContent requires unique ASCII IDs of 1..128 bytes and at most 64 slots"
                        .into(),
                );
            }
            if slot.header.trim().is_empty() || slot.header.len() > 4096 || slot.key_tip.len() > 32
            {
                return Err(
                    "RibbonContent requires Header of 1..4096 bytes and KeyTip of 1..32 bytes"
                        .into(),
                );
            }
            if !slot.width.is_finite()
                || !(24.0..=4096.0).contains(&slot.width)
                || !slot.min_width.is_finite()
                || !(24.0..=slot.width).contains(&slot.min_width)
                || !slot.height.is_finite()
                || !(24.0..=512.0).contains(&slot.height)
            {
                return Err(
                    "RibbonContent requires 24 <= MinWidth <= Width <= 4096 and Height in 24..512"
                        .into(),
                );
            }
            let tip = slot.key_tip.to_ascii_uppercase();
            if !valid_key_tip(&tip, &tips, false) {
                return Err("RibbonContent KeyTips must be ASCII alphanumeric and prefix-free within their tab".into());
            }
            tips.push(tip);
            text(&slot.id)?;
            text(&slot.header)?;
            text(&slot.key_tip)?;
        }
    }
    if let Some(backstage) = &ribbon.backstage {
        *count += 2 + backstage.items.len();
        let tab_tips: Vec<_> = ribbon
            .tabs
            .iter()
            .map(|t| t.key_tip.to_ascii_uppercase())
            .collect();
        if backstage.header.trim().is_empty()
            || backstage.header.len() > 4096
            || backstage.key_tip.len() > 32
            || !valid_key_tip(&backstage.key_tip.to_ascii_uppercase(), &tab_tips, true)
            || backstage.items.is_empty()
            || backstage.items.len() > 32
        {
            return Err("Backstage requires Header, a distinct root KeyTip and 1..32 items".into());
        }
        text(&backstage.header)?;
        text(&backstage.key_tip)?;
        let mut pages = HashSet::new();
        let mut tips = Vec::new();
        for item in &backstage.items {
            let key_tip = match item {
                RibbonBackstageItemDefinition::Page {
                    id,
                    header,
                    key_tip,
                    ..
                } => {
                    if !slot_id(id)
                        || !pages.insert(id)
                        || header.trim().is_empty()
                        || header.len() > 4096
                    {
                        return Err(
                            "BackstagePage requires unique ASCII Id and nonempty Header".into()
                        );
                    }
                    text(id)?;
                    text(header)?;
                    key_tip
                }
                RibbonBackstageItemDefinition::Command { command, key_tip } => {
                    crate::ribbon::check_identifier(command, "Command")?;
                    text(command)?;
                    key_tip
                }
            };
            let tip = key_tip.to_ascii_uppercase();
            if tip.len() > 32 || !valid_key_tip(&tip, &tips, false) {
                return Err(
                    "Backstage item KeyTips must be ASCII alphanumeric and prefix-free".into(),
                );
            }
            tips.push(tip);
            text(key_tip)?;
        }
    }
    let mut pending: Vec<_> = ribbon
        .content_elements()
        .into_iter()
        .map(|node| (node, 5))
        .collect();
    let mut total = *count;
    while let Some((node, depth)) = pending.pop() {
        total += 1;
        if total > MAX_ELEMENTS || depth > MAX_ELEMENT_DEPTH {
            return Err("Ribbon retained content exceeds the count or depth limit".into());
        }
        if matches!(
            node.kind,
            ElementKind::Ribbon(_) | ElementKind::DockWorkspace(_)
        ) {
            return Err("Ribbon retained content cannot contain Ribbon or DockWorkspace".into());
        }
        pending.extend(node.children().into_iter().map(|child| (child, depth + 1)));
    }
    Ok(())
}

fn one_content(
    node: Node<'_, '_>,
    depth: usize,
    count: &mut usize,
    names: &mut HashSet<String>,
    radios: &mut HashSet<RadioGroupKey>,
) -> Result<Box<ElementDefinition>, Diagnostic> {
    let parent = *count;
    let mut content = None;
    for child in node.children() {
        if child.is_element() {
            if content.is_some() {
                return Err(at_node(
                    child,
                    "Ribbon retained host requires exactly one content element",
                ));
            }
            content = Some(Box::new(parse_element(
                child,
                depth + 1,
                parent,
                count,
                names,
                radios,
            )?));
        } else {
            require_empty_content(child)?;
        }
    }
    content.ok_or_else(|| {
        at_node(
            node,
            "Ribbon retained host requires exactly one content element",
        )
    })
}

pub(super) fn parse_content(
    node: Node<'_, '_>,
    depth: usize,
    count: &mut usize,
    names: &mut HashSet<String>,
    radios: &mut HashSet<RadioGroupKey>,
) -> Result<RibbonContentDefinition, Diagnostic> {
    checked_attributes(node)?;
    let mut result = RibbonContentDefinition::default();
    for attr in node.attributes() {
        match attr.name() {
            "Id" => result.id = attr.value().into(),
            "Header" => result.header = attr.value().into(),
            "KeyTip" => result.key_tip = attr.value().to_ascii_uppercase(),
            "Width" => result.width = dimension(node.document(), attr)? as f32,
            "MinWidth" => result.min_width = dimension(node.document(), attr)? as f32,
            "Height" => result.height = dimension(node.document(), attr)? as f32,
            _ => {
                return Err(at_attribute(
                    node.document(),
                    attr,
                    "Unsupported RibbonContent property",
                ));
            }
        }
    }
    result.content = one_content(node, depth, count, names, radios)?;
    Ok(result)
}

pub(super) fn parse_backstage(
    property: Node<'_, '_>,
    depth: usize,
    count: &mut usize,
    names: &mut HashSet<String>,
    radios: &mut HashSet<RadioGroupKey>,
) -> Result<RibbonBackstageDefinition, Diagnostic> {
    if property.attributes().len() != 0 {
        return Err(at_node(property, "Ribbon.Backstage accepts no attributes"));
    }
    let mut root = None;
    for node in property.children() {
        if node.is_element() {
            if root.replace(node).is_some() {
                return Err(at_node(
                    node,
                    "Ribbon.Backstage requires exactly one Backstage",
                ));
            }
        } else {
            require_empty_content(node)?;
        }
    }
    let node = root.ok_or_else(|| at_node(property, "Ribbon.Backstage requires Backstage"))?;
    bounded(node, depth + 1, count)?;
    require_element(node, "Backstage")?;
    checked_attributes(node)?;
    let mut result = RibbonBackstageDefinition::default();
    for attr in node.attributes() {
        match attr.name() {
            "Header" => result.header = attr.value().into(),
            "KeyTip" => result.key_tip = attr.value().to_ascii_uppercase(),
            _ => {
                return Err(at_attribute(
                    node.document(),
                    attr,
                    "Unsupported Backstage property",
                ));
            }
        }
    }
    nonempty(node, &result.header, "Header")?;
    for child in node.children() {
        if !child.is_element() {
            require_empty_content(child)?;
            continue;
        }
        bounded(child, depth + 2, count)?;
        checked_attributes(child)?;
        let is_page = child.tag_name().name() == "BackstagePage";
        if !is_page && child.tag_name().name() != "BackstageCommand" {
            return Err(at_node(
                child,
                "Backstage accepts BackstagePage or BackstageCommand",
            ));
        }
        let (mut id, mut header, mut key_tip, mut command) =
            (String::new(), String::new(), String::new(), String::new());
        for attr in child.attributes() {
            match attr.name() {
                "Id" if is_page => id = attr.value().into(),
                "Header" if is_page => header = attr.value().into(),
                "Command" if !is_page => {
                    identifier(child.document(), attr, "Command")?;
                    command = attr.value().into();
                }
                "KeyTip" => key_tip = attr.value().to_ascii_uppercase(),
                _ => {
                    return Err(at_attribute(
                        child.document(),
                        attr,
                        "Unsupported Backstage item property",
                    ));
                }
            }
        }
        result.items.push(if is_page {
            RibbonBackstageItemDefinition::Page {
                id,
                header,
                key_tip,
                content: one_content(child, depth + 2, count, names, radios)?,
            }
        } else {
            for nested in child.children() {
                require_empty_content(nested)?;
            }
            RibbonBackstageItemDefinition::Command { command, key_tip }
        });
    }
    Ok(result)
}
