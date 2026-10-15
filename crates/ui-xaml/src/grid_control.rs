//! Declarative grid columns and references to host-registered data sources.
use crate::{
    CommonProperties, Diagnostic, MAX_ELEMENT_DEPTH, MAX_ELEMENTS, UI_NAMESPACE, at_attribute,
    at_node, binding, boolean, editor_metric, reject_extension, require_empty_content,
};
use roxmltree::Node;
use std::collections::HashSet;

/// Maximum explicitly declared columns in one GridControl.
pub const MAX_GRID_COLUMNS: usize = 64;
/// Maximum UTF-8 length of a grid filter or column header.
pub const MAX_GRID_TEXT_BYTES: usize = 4096;

/// A virtualized table whose source is registered by the application.
///
/// `data_source` is an identifier, never SQL, a file path or a CLR type name.
/// An empty column list asks the host to generate columns from the source schema.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GridControlDefinition {
    pub data_source: String,
    pub columns: Vec<GridColumnDefinition>,
    pub is_read_only: bool,
    pub filter_text: String,
    /// A signed 64-bit row key represented as a decimal string, preserving all bits.
    pub selected_value: Option<String>,
}

/// Explicit presentation of one field in the registered source schema.
#[derive(Clone, Debug, PartialEq)]
pub struct GridColumnDefinition {
    /// Exact schema field name, not a property path or reflection expression.
    pub field: String,
    pub header: String,
    pub width: f32,
    pub min_width: f32,
    pub max_width: f32,
    pub is_pinned: bool,
    pub is_read_only: bool,
}

impl Default for GridColumnDefinition {
    fn default() -> Self {
        Self {
            field: String::new(),
            header: String::new(),
            width: 140.0,
            min_width: 32.0,
            max_width: 4096.0,
            is_pinned: false,
            is_read_only: false,
        }
    }
}

fn valid_name(value: &str) -> bool {
    value.len() <= 128 && binding::valid_identifier(value)
}

fn valid_text(value: &str) -> bool {
    value.len() <= MAX_GRID_TEXT_BYTES && !value.contains('\0')
}

fn valid_key(value: &str) -> bool {
    let digits = value.strip_prefix('-').unwrap_or(value);
    value.len() <= 20
        && !digits.is_empty()
        && digits.bytes().all(|byte| byte.is_ascii_digit())
        && value.parse::<i64>().is_ok()
}

/// Validate a manually constructed grid definition before mounting it.
///
/// This checks bounded identifiers, text, keys, column uniqueness and dimensions.
/// Source registration and schema field existence are checked by the host.
pub fn validate_grid_control(grid: &GridControlDefinition) -> Result<(), String> {
    if !valid_name(&grid.data_source) {
        return Err(
            "GridControl DataSource must be a registered identifier of at most 128 bytes".into(),
        );
    }
    if !valid_text(&grid.filter_text) {
        return Err("GridControl FilterText exceeds 4096 UTF-8 bytes or contains NUL".into());
    }
    if grid
        .selected_value
        .as_deref()
        .is_some_and(|key| !valid_key(key))
    {
        return Err("GridControl SelectedValue must be a signed 64-bit decimal row key".into());
    }
    if grid.columns.len() > MAX_GRID_COLUMNS {
        return Err("GridControl supports at most 64 columns".into());
    }
    let mut fields = HashSet::new();
    for column in &grid.columns {
        if column.field.is_empty()
            || column.field.len() > 256
            || column.field.contains('\0')
            || !fields.insert(&column.field)
        {
            return Err(
                "GridColumn FieldName must be a unique nonempty schema name of at most 256 UTF-8 bytes without NUL".into(),
            );
        }
        if !valid_text(&column.header) {
            return Err("GridColumn Header exceeds 4096 UTF-8 bytes or contains NUL".into());
        }
        if !column.width.is_finite()
            || !column.min_width.is_finite()
            || !column.max_width.is_finite()
            || column.min_width < 32.0
            || column.max_width > 4096.0
            || column.min_width > column.width
            || column.width > column.max_width
        {
            return Err(
                "GridColumn requires finite 32 <= MinWidth <= Width <= MaxWidth <= 4096".into(),
            );
        }
    }
    Ok(())
}

/// Validate binding targets on a manually constructed GridControl before mount.
/// Values, converters and registered source paths are resolved by the host.
pub fn validate_grid_control_bindings(common: &CommonProperties) -> Result<(), String> {
    for property in common.bindings.keys() {
        if !matches!(
            property.as_str(),
            "IsEnabled"
                | "Visibility"
                | "Width"
                | "Height"
                | "IsReadOnly"
                | "FilterText"
                | "SelectedValue"
        ) {
            return Err(format!("GridControl.{property} does not support a binding"));
        }
    }
    Ok(())
}

pub(crate) fn parse_children(
    node: Node<'_, '_>,
    depth: usize,
    count: &mut usize,
    grid: &mut GridControlDefinition,
) -> Result<(), Diagnostic> {
    let mut wrapper = false;
    let mut direct = false;
    for child in node.children() {
        if !child.is_element() {
            require_empty_content(child)?;
            continue;
        }
        if child.tag_name().namespace() != Some(UI_NAMESPACE) {
            return Err(at_node(child, "unsupported grid child namespace"));
        }
        if child.tag_name().name() == "GridControl.Columns" {
            if wrapper || direct {
                return Err(at_node(
                    child,
                    "GridControl.Columns and direct columns are mutually exclusive and cannot repeat",
                ));
            }
            wrapper = true;
            if child.attributes().len() != 0 {
                return Err(at_node(child, "GridControl.Columns cannot have attributes"));
            }
            count_metadata(child, depth + 1, count)?;
            for column in child.children() {
                if !column.is_element() {
                    require_empty_content(column)?;
                    continue;
                }
                count_metadata(column, depth + 2, count)?;
                grid.columns.push(parse_column(column)?);
                check_columns(child, grid)?;
            }
        } else {
            if wrapper {
                return Err(at_node(
                    child,
                    "GridControl.Columns and direct columns are mutually exclusive",
                ));
            }
            direct = true;
            count_metadata(child, depth + 1, count)?;
            grid.columns.push(parse_column(child)?);
            check_columns(child, grid)?;
        }
    }
    Ok(())
}

fn count_metadata(node: Node<'_, '_>, depth: usize, count: &mut usize) -> Result<(), Diagnostic> {
    *count += 1;
    if *count > MAX_ELEMENTS || depth > MAX_ELEMENT_DEPTH {
        return Err(at_node(
            node,
            "grid content exceeds element count or depth limit",
        ));
    }
    Ok(())
}

fn check_columns(node: Node<'_, '_>, grid: &GridControlDefinition) -> Result<(), Diagnostic> {
    if grid.columns.len() > MAX_GRID_COLUMNS {
        return Err(at_node(node, "GridControl supports at most 64 columns"));
    }
    Ok(())
}

fn parse_column(node: Node<'_, '_>) -> Result<GridColumnDefinition, Diagnostic> {
    if node.tag_name().namespace() != Some(UI_NAMESPACE) || node.tag_name().name() != "GridColumn" {
        return Err(at_node(
            node,
            "GridControl accepts only GridColumn metadata",
        ));
    }
    let mut column = GridColumnDefinition::default();
    for attr in node.attributes() {
        if attr.namespace().is_some() {
            return Err(at_attribute(
                node.document(),
                attr,
                "GridColumn does not support directives",
            ));
        }
        reject_extension(node.document(), attr)?;
        match attr.name() {
            "FieldName" => column.field = attr.value().into(),
            "Header" => column.header = attr.value().into(),
            "Width" => column.width = editor_metric(node.document(), attr, 4096.0, false)?,
            "MinWidth" => column.min_width = editor_metric(node.document(), attr, 4096.0, false)?,
            "MaxWidth" => column.max_width = editor_metric(node.document(), attr, 4096.0, false)?,
            "IsPinned" => column.is_pinned = boolean(node.document(), attr)?,
            "IsReadOnly" => column.is_read_only = boolean(node.document(), attr)?,
            _ => {
                return Err(at_attribute(
                    node.document(),
                    attr,
                    "unsupported GridColumn property",
                ));
            }
        }
    }
    if node.attribute("Header").is_none() {
        column.header.clone_from(&column.field);
    }
    for child in node.children() {
        if child.is_element() {
            return Err(at_node(child, "GridColumn cannot contain child elements"));
        }
        require_empty_content(child)?;
    }
    Ok(column)
}
