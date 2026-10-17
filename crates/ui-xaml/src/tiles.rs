//! Grouped adaptive tile declarations and readonly item templates.
use crate::*;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TileSizeDefinition {
    #[default]
    Small,
    Medium,
    Wide,
    Large,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TileGroupDefinition {
    pub key: String,
    pub header: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TileDefinition {
    pub key: String,
    pub text: String,
    pub group: String,
    pub size: TileSizeDefinition,
    pub icon: Option<IconSymbol>,
    pub command: Option<String>,
    pub is_enabled: bool,
    pub columns: Vec<String>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct TileCollectionDefinition {
    pub groups: Vec<TileGroupDefinition>,
    pub items: Vec<TileDefinition>,
    pub selected_keys: Vec<String>,
    pub selection_mode: SelectionMode,
    pub is_read_only: bool,
    pub allow_reorder: bool,
    pub cell_width: f32,
    pub cell_height: f32,
    pub gap: f32,
    pub group_header_height: f32,
    pub overscan: f32,
    pub max_realized: usize,
    pub item_template: Option<Box<ElementDefinition>>,
}
impl Default for TileCollectionDefinition {
    fn default() -> Self {
        Self {
            groups: Vec::new(),
            items: Vec::new(),
            selected_keys: Vec::new(),
            selection_mode: SelectionMode::Single,
            is_read_only: false,
            allow_reorder: true,
            cell_width: 144.0,
            cell_height: 88.0,
            gap: 8.0,
            group_header_height: 32.0,
            overscan: 120.0,
            max_realized: 512,
            item_template: None,
        }
    }
}
pub fn validate_tiles(v: &TileCollectionDefinition, c: &CommonProperties) -> Result<(), String> {
    for (value, low, high) in [
        (v.cell_width, 32.0, 1024.0),
        (v.cell_height, 24.0, 1024.0),
        (v.gap, 0.0, 64.0),
        (v.group_header_height, 16.0, 128.0),
        (v.overscan, 0.0, 4096.0),
    ] {
        if !value.is_finite() || !(low..=high).contains(&value) || value.fract() != 0.0 {
            return Err("Tile dimensions require bounded integer logical pixels".into());
        }
    }
    if !(1..=4096).contains(&v.max_realized) || v.groups.len() > 256 || v.items.len() > 100_000 {
        return Err("Tile count or realization limit exceeded".into());
    }
    let mut groups = HashSet::new();
    let mut keys = HashSet::new();
    let mut bytes = 0usize;
    for group in &v.groups {
        if group.key.is_empty() || !groups.insert(group.key.as_str()) {
            return Err("Tile groups require unique nonempty keys".into());
        }
        for text in [&group.key, &group.header] {
            if text.len() > 4096 || text.contains('\0') {
                return Err("Tile group text exceeds4096 bytes or contains NUL".into());
            }
        }
        bytes += group.key.len() + group.header.len();
    }
    for item in &v.items {
        if item.key.is_empty()
            || !keys.insert(item.key.as_str())
            || (!item.group.is_empty() && !groups.contains(item.group.as_str()))
            || item.columns.len() > 31
        {
            return Err("Invalid tile key, group or column count".into());
        }
        for text in std::iter::once(&item.key)
            .chain(std::iter::once(&item.text))
            .chain(item.columns.iter())
            .chain(std::iter::once(&item.group))
            .chain(item.command.iter())
        {
            if text.len() > 4096 || text.contains('\0') {
                return Err("Tile text exceeds4096 bytes or contains NUL".into());
            }
            bytes = bytes.saturating_add(text.len());
        }
    }
    if bytes > 16 * 1024 * 1024 {
        return Err("Tile metadata exceeds16MiB".into());
    }
    if c.bindings.keys().any(|p| {
        !badges::binding_property(p)
            && !matches!(
                p.as_str(),
                "SelectedKey"
                    | "SelectedKeys"
                    | "ItemsSource"
                    | "IsReadOnly"
                    | "IsEnabled"
                    | "Visibility"
                    | "Width"
                    | "Height"
            )
    }) {
        return Err("Unsupported tile binding".into());
    }
    if c.bindings.contains_key("ItemsSource") && !v.items.is_empty() {
        return Err("Tile ItemsSource cannot combine with literal Tile children".into());
    }
    if c.bindings.contains_key("ItemsSource") && !v.selected_keys.is_empty() {
        return Err(
            "Tiles with ItemsSource require a selection binding instead of a literal selected key"
                .into(),
        );
    }
    if c.bindings.contains_key("SelectedKey") && c.bindings.contains_key("SelectedKeys") {
        return Err("Use one tile selection binding".into());
    }
    if v.selection_mode == SelectionMode::Multiple && c.bindings.contains_key("SelectedKey") {
        return Err("Multiple tiles bind SelectedKeys".into());
    }
    if v.selection_mode == SelectionMode::Single && v.selected_keys.len() > 1 {
        return Err("Single tiles accept at most one selected key".into());
    }
    let mut selected = HashSet::new();
    if v.selected_keys.iter().any(|key| {
        !selected.insert(key)
            || (!c.bindings.contains_key("ItemsSource")
                && !v.items.iter().any(|i| i.key == *key && i.is_enabled))
    }) {
        return Err("Selected tile keys must be distinct and enabled".into());
    }
    if let Some(template) = &v.item_template {
        validate_item_template(template)?;
    }
    Ok(())
}
pub(crate) fn property(
    doc: &Document<'_>,
    a: Attribute<'_, '_>,
    v: &mut TileCollectionDefinition,
) -> Result<bool, Diagnostic> {
    match a.name() {
        "SelectedKey" => {
            v.selected_keys = if a.value().is_empty() {
                Vec::new()
            } else {
                vec![a.value().into()]
            }
        }
        "SelectedKeys" => {
            v.selected_keys = a
                .value()
                .split(',')
                .filter(|s| !s.trim().is_empty())
                .map(|s| s.trim().into())
                .collect()
        }
        "SelectionMode" => {
            v.selection_mode = match a.value() {
                "Single" => SelectionMode::Single,
                "Multiple" => SelectionMode::Multiple,
                _ => {
                    return Err(at_attribute(
                        doc,
                        a,
                        "SelectionMode requires Single or Multiple",
                    ));
                }
            }
        }
        "IsReadOnly" => v.is_read_only = boolean(doc, a)?,
        "AllowReorder" => v.allow_reorder = boolean(doc, a)?,
        "MaxRealized" => {
            v.max_realized = a
                .value()
                .parse()
                .map_err(|_| at_attribute(doc, a, "Invalid MaxRealized"))?
        }
        "CellWidth" | "CellHeight" | "Gap" | "GroupHeaderHeight" | "Overscan" => {
            let value = a
                .value()
                .parse()
                .map_err(|_| at_attribute(doc, a, "Invalid tile dimension"))?;
            match a.name() {
                "CellWidth" => v.cell_width = value,
                "CellHeight" => v.cell_height = value,
                "Gap" => v.gap = value,
                "GroupHeaderHeight" => v.group_header_height = value,
                _ => v.overscan = value,
            }
        }
        _ => return Ok(false),
    }
    Ok(true)
}
pub(crate) fn parse_children(
    node: Node<'_, '_>,
    depth: usize,
    count: &mut usize,
    v: &mut TileCollectionDefinition,
) -> Result<(), Diagnostic> {
    if node.attribute("SelectedKey").is_some() && node.attribute("SelectedKeys").is_some() {
        return Err(at_node(
            node,
            "SelectedKey and SelectedKeys are mutually exclusive",
        ));
    }
    for child in node.children() {
        if !child.is_element() {
            require_empty_content(child)?;
            continue;
        }
        if child.tag_name().namespace() != Some(UI_NAMESPACE) {
            return Err(at_node(child, "Unknown tile child namespace"));
        }
        match child.tag_name().name() {
            "Tile" => v.items.push(parse_tile(child, depth + 1, count, "")?),
            "TileGroup" => {
                budget(child, depth + 1, count)?;
                let mut group = TileGroupDefinition {
                    key: String::new(),
                    header: String::new(),
                };
                for a in child.attributes() {
                    metadata(child, a)?;
                    match a.name() {
                        "Key" => group.key = a.value().into(),
                        "Header" => group.header = a.value().into(),
                        _ => return Err(at_node(child, "TileGroup supports Key and Header")),
                    }
                }
                for tile in child.children() {
                    if !tile.is_element() {
                        require_empty_content(tile)?;
                        continue;
                    }
                    if tile.tag_name().namespace() != Some(UI_NAMESPACE)
                        || tile.tag_name().name() != "Tile"
                    {
                        return Err(at_node(tile, "TileGroup accepts Tile children"));
                    }
                    v.items
                        .push(parse_tile(tile, depth + 2, count, &group.key)?);
                }
                v.groups.push(group);
            }
            "TileCollection.ItemTemplate" => {
                if v.item_template.is_some() || child.attributes().len() != 0 {
                    return Err(at_node(child, "Invalid or duplicate Tile ItemTemplate"));
                }
                budget(child, depth + 1, count)?;
                for root in child.children() {
                    if !root.is_element() {
                        require_empty_content(root)?;
                        continue;
                    }
                    if v.item_template.is_some() {
                        return Err(at_node(root, "ItemTemplate requires one readonly root"));
                    }
                    let definition = parse_element(
                        root,
                        depth + 2,
                        *count,
                        count,
                        &mut HashSet::new(),
                        &mut HashSet::new(),
                    )?;
                    validate_item_template(&definition).map_err(|e| at_node(root, e))?;
                    v.item_template = Some(Box::new(definition));
                }
                if v.item_template.is_none() {
                    return Err(at_node(child, "ItemTemplate requires one root"));
                }
            }
            _ => {
                return Err(at_node(
                    child,
                    "TileCollection accepts Tile, TileGroup and ItemTemplate",
                ));
            }
        }
    }
    Ok(())
}
fn parse_tile(
    node: Node<'_, '_>,
    depth: usize,
    count: &mut usize,
    group: &str,
) -> Result<TileDefinition, Diagnostic> {
    budget(node, depth, count)?;
    let mut t = TileDefinition {
        key: String::new(),
        text: String::new(),
        group: group.into(),
        size: TileSizeDefinition::Small,
        icon: None,
        command: None,
        is_enabled: true,
        columns: Vec::new(),
    };
    for a in node.attributes() {
        metadata(node, a)?;
        match a.name() {
            "Key" => t.key = a.value().into(),
            "Text" => t.text = a.value().into(),
            "Command" => t.command = Some(a.value().into()),
            "Icon" => t.icon = Some(icon_symbol(node.document(), a)?),
            "IsEnabled" => t.is_enabled = boolean(node.document(), a)?,
            "Size" => {
                t.size = match a.value() {
                    "Small" => TileSizeDefinition::Small,
                    "Medium" => TileSizeDefinition::Medium,
                    "Wide" => TileSizeDefinition::Wide,
                    "Large" => TileSizeDefinition::Large,
                    _ => return Err(at_node(node, "Unknown tile size")),
                }
            }
            _ => return Err(at_node(node, "Unsupported Tile attribute")),
        }
    }
    for child in node.children() {
        if !child.is_element() {
            require_empty_content(child)?;
            continue;
        }
        if child.tag_name().namespace() != Some(UI_NAMESPACE)
            || child.tag_name().name() != "TileColumn"
            || child.attributes().len() != 1
        {
            return Err(at_node(child, "Tile accepts TileColumn Text metadata"));
        }
        budget(child, depth + 1, count)?;
        let a = child.attributes().next().unwrap();
        metadata(child, a)?;
        if a.name() != "Text" {
            return Err(at_node(child, "TileColumn requires Text"));
        }
        t.columns.push(a.value().into());
        for n in child.children() {
            require_empty_content(n)?;
        }
    }
    Ok(t)
}
fn metadata(node: Node<'_, '_>, a: Attribute<'_, '_>) -> Result<(), Diagnostic> {
    if a.namespace().is_some() {
        return Err(at_node(node, "Tile metadata directives are unsupported"));
    }
    reject_extension(node.document(), a)
}
fn budget(node: Node<'_, '_>, depth: usize, count: &mut usize) -> Result<(), Diagnostic> {
    *count += 1;
    if depth > MAX_ELEMENT_DEPTH || *count > MAX_ELEMENTS {
        return Err(at_node(node, "Tile definition exceeds tree budget"));
    }
    Ok(())
}
pub(crate) fn code(v: &TileCollectionDefinition) -> String {
    let groups = v
        .groups
        .iter()
        .map(|g| {
            format!(
                "rust_desktop_ui_xaml::TileGroupDefinition{{key:{:?}.into(),header:{:?}.into()}}",
                g.key, g.header
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let items=v.items.iter().map(|i|format!("rust_desktop_ui_xaml::TileDefinition{{key:{:?}.into(),text:{:?}.into(),group:{:?}.into(),size:rust_desktop_ui_xaml::TileSizeDefinition::{:?},icon:{},command:{},is_enabled:{},columns:vec![{}]}}",i.key,i.text,i.group,i.size,i.icon.map_or("None".into(),|v|format!("Some(rust_desktop_ui_xaml::IconSymbol::{v:?})")),i.command.as_ref().map_or("None".into(),|v|format!("Some({v:?}.into())")),i.is_enabled,i.columns.iter().map(|v|format!("{v:?}.into()")).collect::<Vec<_>>().join(","))).collect::<Vec<_>>().join(",");
    format!(
        "Box::new(rust_desktop_ui_xaml::TileCollectionDefinition{{groups:vec![{groups}],items:vec![{items}],selected_keys:vec![{}],selection_mode:rust_desktop_ui_xaml::SelectionMode::{:?},is_read_only:{},allow_reorder:{},cell_width:{:?},cell_height:{:?},gap:{:?},group_header_height:{:?},overscan:{:?},max_realized:{},item_template:{}}})",
        v.selected_keys
            .iter()
            .map(|k| format!("{k:?}.into()"))
            .collect::<Vec<_>>()
            .join(","),
        v.selection_mode,
        v.is_read_only,
        v.allow_reorder,
        v.cell_width,
        v.cell_height,
        v.gap,
        v.group_header_height,
        v.overscan,
        v.max_realized,
        v.item_template.as_ref().map_or("None".into(), |t| format!(
            "Some(Box::new({}))",
            codegen::element(t)
        ))
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    fn body(s: &str) -> Result<WindowDefinition, Diagnostic> {
        parse(&format!(
            "<Window xmlns='urn:rust-ui-engine:ui' xmlns:x='http://schemas.microsoft.com/winfx/2006/xaml'>{s}</Window>"
        ))
    }
    #[test]
    fn tiles_compile_groups_bindings_and_private_readonly_template() {
        let w=body("<TileCollection SelectedKey='{Binding Selected}'><TileCollection.ItemTemplate><TextBlock Text='{Binding Text}'/></TileCollection.ItemTemplate><TileGroup Key='docs' Header='Documents'><Tile Key='a' Text='Alpha' Size='Wide' Icon='Edit' Command='Open'><TileColumn Text='Detail'/></Tile></TileGroup></TileCollection>").unwrap();
        let ElementKind::TileCollection(t) = &w.content.kind else {
            panic!()
        };
        assert_eq!(t.items[0].size, TileSizeDefinition::Wide);
        assert_eq!(t.items[0].group, "docs");
        assert!(t.item_template.is_some());
        assert!(codegen::window(&w).contains("TileSizeDefinition::Wide"));
        assert_eq!(
            w.content.common.bindings["SelectedKey"].mode,
            BindingMode::TwoWay
        );
    }
    #[test]
    fn tiles_reject_duplicate_keys_bad_dimensions_interactive_templates_and_conflicting_source() {
        for s in [
            "<TileCollection CellWidth='NaN'/>",
            "<TileCollection><Tile Key='a'/><Tile Key='a'/></TileCollection>",
            "<TileCollection><TileCollection.ItemTemplate><TextBox/></TileCollection.ItemTemplate></TileCollection>",
            "<TileCollection ItemsSource='{Binding Rows}'><Tile Key='a'/></TileCollection>",
            "<TileCollection SelectionMode='Multiple' SelectedKey='{Binding Key}'/>",
            "<TileCollection><TileGroup Key='g' Header='G'/><TileGroup Key='g'/></TileCollection>",
        ] {
            assert!(body(s).is_err(), "{s}");
        }
    }
}
