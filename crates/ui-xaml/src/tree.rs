//! Explicit hierarchy sources and bounded column metadata.
use crate::*;

#[derive(Clone, Debug, PartialEq)]
pub struct TreeDefinition {
    pub source: String,
    pub display_column: u32,
    pub columns: Vec<TreeColumnDefinition>,
    pub selection_mode: SelectionMode,
    pub selected_key: Option<String>,
    pub allow_node_drag: bool,
    pub is_read_only: bool,
}
impl Default for TreeDefinition {
    fn default() -> Self {
        Self {
            source: String::new(),
            display_column: 0,
            columns: Vec::new(),
            selection_mode: SelectionMode::Single,
            selected_key: None,
            allow_node_drag: false,
            is_read_only: true,
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct TreeColumnDefinition {
    pub id: u32,
    pub header: String,
    pub width: f32,
    pub min_width: f32,
    pub max_width: f32,
    pub is_pinned: bool,
}
impl Default for TreeColumnDefinition {
    fn default() -> Self {
        Self {
            id: 0,
            header: String::new(),
            width: 240.0,
            min_width: 32.0,
            max_width: 4096.0,
            is_pinned: false,
        }
    }
}
pub fn validate_tree(
    tree: &TreeDefinition,
    has_columns: bool,
    common: &CommonProperties,
) -> Result<(), String> {
    if tree.source.len() > 128 || !binding::valid_identifier(&tree.source) {
        return Err("Tree Source must name a registered source (1..128 bytes)".into());
    }
    if (has_columns && (tree.columns.is_empty() || tree.columns.len() > 64))
        || (!has_columns && !tree.columns.is_empty())
    {
        return Err("TreeGrid requires 1..64 columns; TreeView uses DisplayColumn".into());
    }
    if has_columns && tree.display_column != 0 {
        return Err("TreeGrid hierarchy column is its first declared column".into());
    }
    if let Some(key) = &tree.selected_key
        && key
            .parse::<u64>()
            .ok()
            .is_none_or(|value| value.to_string() != *key)
    {
        return Err("SelectedKey must be a canonical unsigned 64-bit decimal string".into());
    }
    if tree.selection_mode != SelectionMode::Single
        && (tree.selected_key.is_some() || common.bindings.contains_key("SelectedKey"))
    {
        return Err("SelectedKey requires Single selection".into());
    }
    for property in common.bindings.keys() {
        if !matches!(
            property.as_str(),
            "SelectedKey" | "IsReadOnly" | "IsEnabled" | "Visibility" | "Width" | "Height"
        ) {
            return Err(format!("Tree.{property} does not support bindings"));
        }
    }
    let mut ids = HashSet::new();
    for column in &tree.columns {
        if !ids.insert(column.id)
            || column.header.len() > 4096
            || column.header.contains('\0')
            || !column.width.is_finite()
            || !column.min_width.is_finite()
            || !column.max_width.is_finite()
            || column.min_width < 32.0
            || column.min_width > column.width
            || column.width > column.max_width
            || column.max_width > 4096.0
        {
            return Err("TreeColumn requires unique Id, bounded header and 32 <= MinWidth <= Width <= MaxWidth <= 4096".into());
        }
    }
    Ok(())
}
pub(crate) fn property(
    document: &Document<'_>,
    attr: Attribute<'_, '_>,
    tree: &mut TreeDefinition,
    grid: bool,
) -> Result<bool, Diagnostic> {
    match attr.name() {
        "Source" => tree.source = attr.value().into(),
        "DisplayColumn" if !grid => {
            tree.display_column = attr
                .value()
                .parse()
                .map_err(|_| at_attribute(document, attr, "DisplayColumn requires u32"))?
        }
        "SelectedKey" => tree.selected_key = Some(attr.value().into()),
        "AllowNodeDrag" => tree.allow_node_drag = boolean(document, attr)?,
        "IsReadOnly" => tree.is_read_only = boolean(document, attr)?,
        "SelectionMode" => {
            tree.selection_mode = match attr.value() {
                "Single" => SelectionMode::Single,
                "Multiple" => SelectionMode::Multiple,
                _ => {
                    return Err(at_attribute(
                        document,
                        attr,
                        "SelectionMode must be Single or Multiple",
                    ));
                }
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
    tree: &mut TreeDefinition,
    grid: bool,
) -> Result<(), Diagnostic> {
    let mut wrapper = false;
    let mut direct = false;
    for child in node.children() {
        if !child.is_element() {
            require_empty_content(child)?;
            continue;
        }
        if !grid {
            return Err(at_node(child, "TreeView has no child elements"));
        }
        if child.tag_name().namespace() == Some(UI_NAMESPACE)
            && child.tag_name().name() == "TreeGrid.Columns"
        {
            if wrapper || direct || child.attributes().len() != 0 {
                return Err(at_node(
                    child,
                    "TreeGrid.Columns cannot repeat, have attributes or mix with direct columns",
                ));
            }
            wrapper = true;
            metadata(child, depth + 1, count)?;
            for column in child.children() {
                if !column.is_element() {
                    require_empty_content(column)?;
                    continue;
                }
                metadata(column, depth + 2, count)?;
                tree.columns.push(parse_column(column)?);
            }
        } else {
            if wrapper {
                return Err(at_node(
                    child,
                    "TreeGrid columns cannot mix wrapper and direct syntax",
                ));
            }
            direct = true;
            metadata(child, depth + 1, count)?;
            tree.columns.push(parse_column(child)?);
        }
        if tree.columns.len() > 64 {
            return Err(at_node(child, "TreeGrid supports at most 64 columns"));
        }
    }
    Ok(())
}
fn metadata(node: Node<'_, '_>, depth: usize, count: &mut usize) -> Result<(), Diagnostic> {
    *count += 1;
    if *count > MAX_ELEMENTS || depth > MAX_ELEMENT_DEPTH {
        return Err(at_node(node, "Tree metadata count/depth exceeded"));
    }
    Ok(())
}
fn parse_column(node: Node<'_, '_>) -> Result<TreeColumnDefinition, Diagnostic> {
    if node.tag_name().namespace() != Some(UI_NAMESPACE) || node.tag_name().name() != "TreeColumn" {
        return Err(at_node(node, "TreeGrid accepts TreeColumn metadata"));
    }
    let mut column = TreeColumnDefinition::default();
    if node.attribute("Id").is_none() {
        return Err(at_node(node, "TreeColumn requires Id"));
    }
    for attr in node.attributes() {
        reject_extension(node.document(), attr)?;
        if attr.namespace().is_some() {
            return Err(at_attribute(
                node.document(),
                attr,
                "TreeColumn directives unsupported",
            ));
        }
        match attr.name() {
            "Id" => {
                column.id = attr.value().parse().map_err(|_| {
                    at_attribute(node.document(), attr, "TreeColumn Id requires u32")
                })?
            }
            "Header" => column.header = attr.value().into(),
            "Width" => column.width = editor_metric(node.document(), attr, 4096.0, false)?,
            "MinWidth" => column.min_width = editor_metric(node.document(), attr, 4096.0, false)?,
            "MaxWidth" => column.max_width = editor_metric(node.document(), attr, 4096.0, false)?,
            "IsPinned" => column.is_pinned = boolean(node.document(), attr)?,
            _ => {
                return Err(at_attribute(
                    node.document(),
                    attr,
                    "Unsupported TreeColumn property",
                ));
            }
        }
    }
    for child in node.children() {
        if child.is_element() {
            return Err(at_node(child, "TreeColumn cannot contain elements"));
        }
        require_empty_content(child)?;
    }
    Ok(column)
}
pub(crate) fn code(tree: &TreeDefinition) -> String {
    format!("rust_desktop_ui_xaml::TreeDefinition {{ source:{:?}.into(),display_column:{},selection_mode:rust_desktop_ui_xaml::SelectionMode::{:?},selected_key:{},allow_node_drag:{},is_read_only:{},columns:vec![{}] }}", tree.source,tree.display_column,tree.selection_mode,tree.selected_key.as_ref().map(|key|format!("Some({key:?}.into())")).unwrap_or_else(||"None".into()),tree.allow_node_drag,tree.is_read_only,tree.columns.iter().map(|c|format!("rust_desktop_ui_xaml::TreeColumnDefinition {{id:{},header:{:?}.into(),width:{:?},min_width:{:?},max_width:{:?},is_pinned:{}}}",c.id,c.header,c.width,c.min_width,c.max_width,c.is_pinned)).collect::<Vec<_>>().join(","))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn parse_body(body: &str) -> Result<WindowDefinition, Diagnostic> {
        parse(&format!(
            "<Window xmlns='{UI_NAMESPACE}' xmlns:x='{XAML_NAMESPACE}'>{body}</Window>"
        ))
    }
    #[test]
    fn compiled_hierarchy_and_column_definitions_keep_exact_keys_and_bindings() {
        let view=parse_body("<TreeView Source='Nodes' DisplayColumn='9' SelectedKey='{Binding Selected}' AllowNodeDrag='True' IsReadOnly='False'/>").unwrap();
        let ElementKind::TreeView(tree) = &view.content.kind else {
            panic!()
        };
        assert_eq!(tree.display_column, 9);
        assert!(tree.allow_node_drag);
        assert!(!tree.is_read_only);
        assert_eq!(
            view.content.common.bindings["SelectedKey"].mode,
            BindingMode::TwoWay
        );
        let grid=parse_body("<TreeGrid Source='Nodes' SelectedKey='18446744073709551615'><TreeGrid.Columns><TreeColumn Id='9' Header='Name' Width='240' IsPinned='True'/><TreeColumn Id='2' Header='Value' Width='120'/></TreeGrid.Columns></TreeGrid>").unwrap();
        let ElementKind::TreeGrid(tree) = &grid.content.kind else {
            panic!()
        };
        assert_eq!(
            tree.columns
                .iter()
                .map(|column| column.id)
                .collect::<Vec<_>>(),
            [9, 2]
        );
        assert!(tree.columns[0].is_pinned);
        assert!(code(tree).contains("18446744073709551615"));
    }
    #[test]
    fn malformed_hierarchy_metadata_fails_before_codegen() {
        for body in [
            "<TreeView/>",
            "<TreeView Source='Nodes' SelectedKey='01'/>",
            "<TreeView Source='Nodes' SelectionMode='Multiple' SelectedKey='{Binding Key}'/>",
            "<TreeView Source='Nodes'><TreeColumn Id='0'/></TreeView>",
            "<TreeGrid Source='Nodes'/>",
            "<TreeGrid Source='Nodes'><TreeColumn Id='1'/><TreeColumn Id='1'/></TreeGrid>",
            "<TreeGrid Source='Nodes'><TreeColumn Header='Missing ID'/></TreeGrid>",
            "<TreeGrid Source='Nodes'><TreeColumn Id='1' Width='10'/></TreeGrid>",
            "<TreeView Source='{Binding Provider}'/>",
            "<TreeGrid Source='Nodes' DisplayColumn='1'><TreeColumn Id='0'/></TreeGrid>",
        ] {
            assert!(parse_body(body).is_err(), "{body}");
        }
    }
}
