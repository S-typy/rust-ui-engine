//! Bounded declarative table presentation. Data evaluation remains in the source.
use crate::grid_control::{count_metadata, parse_column, valid_name, validate_columns};
use crate::*;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GridViewDefinition {
    #[default]
    Table,
    Cards,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GridSummaryFunction {
    #[default]
    CountRows,
    Count,
    Sum,
    Average,
    Min,
    Max,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GridSummaryDefinition {
    pub title: String,
    pub function: GridSummaryFunction,
    pub field: Option<String>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GridBandDefinition {
    pub title: String,
    pub fields: Vec<String>,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GridDetailDefinition {
    pub data_source: String,
    pub master_field: String,
    pub foreign_field: String,
    pub columns: Vec<GridColumnDefinition>,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GridCellTemplateDefinition {
    pub field: String,
    pub prefix: String,
    pub suffix: String,
    pub icon: Option<IconSymbol>,
    pub emphasis: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GridEditorDefinition {
    pub field: String,
    pub factory: String,
}

fn field(value: &str) -> bool {
    !value.is_empty() && value.len() <= 256 && !value.contains('\0')
}
fn caption(value: &str) -> bool {
    value.len() <= 256 && !value.chars().any(char::is_control)
}
fn unique_fields(values: &[String], max: usize) -> bool {
    let mut seen = HashSet::new();
    values.len() <= max
        && values
            .iter()
            .all(|value| field(value) && seen.insert(value))
}
pub(super) fn validate(grid: &GridControlDefinition) -> Result<(), String> {
    if !unique_fields(&grid.group_fields, 8) || grid.summaries.len() > 16 || grid.bands.len() > 16 {
        return Err("GridControl supports 8 unique group fields, 16 summaries and 16 bands".into());
    }
    for summary in &grid.summaries {
        if !caption(&summary.title)
            || match summary.function {
                GridSummaryFunction::CountRows => summary.field.is_some(),
                _ => summary.field.as_deref().is_none_or(|value| !field(value)),
            }
        {
            return Err("GridSummary requires a bounded title; CountRows has no field and other functions require FieldName".into());
        }
    }
    let mut band_fields = HashSet::new();
    for band in &grid.bands {
        if band.title.is_empty()
            || !caption(&band.title)
            || band.fields.is_empty()
            || !unique_fields(&band.fields, MAX_GRID_COLUMNS)
            || band.fields.iter().any(|field| !band_fields.insert(field))
        {
            return Err("GridBand requires Header and bounded, unique, disjoint Fields".into());
        }
    }
    if let Some(detail) = &grid.detail {
        if !valid_name(&detail.data_source)
            || !field(&detail.master_field)
            || !field(&detail.foreign_field)
            || detail.columns.is_empty()
        {
            return Err("GridDetail requires a registered DataSource, MasterField, ForeignField and explicit columns".into());
        }
        validate_columns(&detail.columns)?;
        if !grid.columns.is_empty() && detail.columns.len() > grid.columns.len() {
            return Err("GridDetail cannot have more columns than its master grid".into());
        }
    }
    let mut fields = HashSet::new();
    if grid.cell_templates.len() > MAX_GRID_COLUMNS
        || grid.cell_templates.iter().any(|t| {
            !field(&t.field)
                || !fields.insert(&t.field)
                || t.prefix.len() > 256
                || t.suffix.len() > 256
                || t.prefix.contains('\0')
                || t.suffix.contains('\0')
        })
    {
        return Err(
            "GridCellTemplate requires unique fields and prefix/suffix of at most 256 bytes".into(),
        );
    }
    let mut fields = HashSet::new();
    if grid.editors.len() > MAX_GRID_COLUMNS
        || grid
            .editors
            .iter()
            .any(|e| !field(&e.field) || !fields.insert(&e.field) || !valid_name(&e.factory))
    {
        return Err("GridEditor requires unique fields and registered factory identifiers".into());
    }
    Ok(())
}

pub(super) fn view(
    doc: &Document<'_>,
    attr: Attribute<'_, '_>,
) -> Result<GridViewDefinition, Diagnostic> {
    match attr.value() {
        "Table" => Ok(GridViewDefinition::Table),
        "Cards" => Ok(GridViewDefinition::Cards),
        _ => Err(at_attribute(doc, attr, "View must be Table or Cards")),
    }
}
pub(super) fn fields(
    doc: &Document<'_>,
    attr: Attribute<'_, '_>,
    max: usize,
) -> Result<Vec<String>, Diagnostic> {
    let values: Vec<_> = if attr.value().trim().is_empty() {
        vec![]
    } else {
        attr.value()
            .split(',')
            .take(max + 1)
            .map(|v| v.trim().to_owned())
            .collect()
    };
    if !unique_fields(&values, max) {
        return Err(at_attribute(
            doc,
            attr,
            format!("Expected at most {max} unique comma-separated field names"),
        ));
    }
    Ok(values)
}
pub(super) fn is_property(name: &str) -> bool {
    matches!(
        name,
        "GridControl.Summaries"
            | "GridControl.Bands"
            | "GridControl.Detail"
            | "GridControl.CellTemplates"
            | "GridControl.Editors"
    )
}
fn attributes(node: Node<'_, '_>, allowed: &[&str]) -> Result<(), Diagnostic> {
    for attr in node.attributes() {
        if attr.namespace().is_some() || !allowed.contains(&attr.name()) {
            return Err(at_attribute(
                node.document(),
                attr,
                "Unsupported grid metadata property",
            ));
        }
        reject_extension(node.document(), attr)?;
    }
    Ok(())
}
fn empty(node: Node<'_, '_>) -> Result<(), Diagnostic> {
    for child in node.children() {
        if child.is_element() {
            return Err(at_node(child, "This grid metadata cannot contain elements"));
        }
        require_empty_content(child)?;
    }
    Ok(())
}
fn value(node: Node<'_, '_>, name: &str) -> String {
    node.attribute(name).unwrap_or_default().into()
}
fn function(node: Node<'_, '_>) -> Result<GridSummaryFunction, Diagnostic> {
    match node.attribute("Function").unwrap_or("CountRows") {
        "CountRows" => Ok(GridSummaryFunction::CountRows),
        "Count" => Ok(GridSummaryFunction::Count),
        "Sum" => Ok(GridSummaryFunction::Sum),
        "Average" => Ok(GridSummaryFunction::Average),
        "Min" => Ok(GridSummaryFunction::Min),
        "Max" => Ok(GridSummaryFunction::Max),
        _ => Err(at_node(node, "Unsupported GridSummary Function")),
    }
}
pub(super) fn parse_property(
    node: Node<'_, '_>,
    depth: usize,
    count: &mut usize,
    grid: &mut GridControlDefinition,
) -> Result<(), Diagnostic> {
    attributes(node, &[])?;
    count_metadata(node, depth, count)?;
    let (expected, limit) = match node.tag_name().name() {
        "GridControl.Summaries" => ("GridSummary", 16),
        "GridControl.Bands" => ("GridBand", 16),
        "GridControl.Detail" => ("GridDetail", 1),
        "GridControl.CellTemplates" => ("GridCellTemplate", 64),
        "GridControl.Editors" => ("GridEditor", 64),
        _ => unreachable!(),
    };
    let mut entries = 0;
    for item in node.children() {
        if !item.is_element() {
            require_empty_content(item)?;
            continue;
        }
        entries += 1;
        if entries > limit {
            return Err(at_node(
                item,
                format!("{expected} supports at most {limit} entries"),
            ));
        }
        count_metadata(item, depth + 1, count)?;
        if item.tag_name().namespace() != Some(UI_NAMESPACE) || item.tag_name().name() != expected {
            return Err(at_node(item, format!("Expected {expected}")));
        }
        match expected {
            "GridSummary" => {
                attributes(item, &["Title", "Function", "FieldName"])?;
                empty(item)?;
                let function = function(item)?;
                grid.summaries.push(GridSummaryDefinition {
                    title: item
                        .attribute("Title")
                        .map(str::to_owned)
                        .unwrap_or_else(|| format!("{function:?}")),
                    function,
                    field: item.attribute("FieldName").map(str::to_owned),
                });
            }
            "GridBand" => {
                attributes(item, &["Header", "Fields"])?;
                empty(item)?;
                grid.bands.push(GridBandDefinition {
                    title: value(item, "Header"),
                    fields: item
                        .attribute_node("Fields")
                        .map(|attr| fields(item.document(), attr, 64))
                        .transpose()?
                        .unwrap_or_default(),
                });
            }
            "GridCellTemplate" => {
                attributes(item, &["FieldName", "Prefix", "Suffix", "Icon", "Emphasis"])?;
                empty(item)?;
                grid.cell_templates.push(GridCellTemplateDefinition {
                    field: value(item, "FieldName"),
                    prefix: value(item, "Prefix"),
                    suffix: value(item, "Suffix"),
                    icon: item
                        .attribute_node("Icon")
                        .map(|a| icon_symbol(item.document(), a))
                        .transpose()?,
                    emphasis: item
                        .attribute_node("Emphasis")
                        .map(|a| boolean(item.document(), a))
                        .transpose()?
                        .unwrap_or(false),
                });
            }
            "GridEditor" => {
                attributes(item, &["FieldName", "Factory"])?;
                empty(item)?;
                grid.editors.push(GridEditorDefinition {
                    field: value(item, "FieldName"),
                    factory: item.attribute("Factory").unwrap_or("Auto").into(),
                });
            }
            "GridDetail" => {
                attributes(item, &["DataSource", "MasterField", "ForeignField"])?;
                let mut detail = GridDetailDefinition {
                    data_source: value(item, "DataSource"),
                    master_field: value(item, "MasterField"),
                    foreign_field: value(item, "ForeignField"),
                    columns: vec![],
                };
                for column in item.children() {
                    if !column.is_element() {
                        require_empty_content(column)?;
                        continue;
                    }
                    if detail.columns.len() == MAX_GRID_COLUMNS {
                        return Err(at_node(column, "GridDetail supports at most 64 columns"));
                    }
                    count_metadata(column, depth + 2, count)?;
                    detail.columns.push(parse_column(column)?);
                }
                grid.detail = Some(detail);
            }
            _ => unreachable!(),
        }
    }
    if expected == "GridDetail" && entries != 1 {
        return Err(at_node(
            node,
            "GridControl.Detail requires exactly one GridDetail",
        ));
    }
    Ok(())
}

fn strings(values: &[String]) -> String {
    format!(
        "vec![{}]",
        values
            .iter()
            .map(|v| format!("{v:?}.into()"))
            .collect::<Vec<_>>()
            .join(",")
    )
}
fn optional(value: &Option<String>) -> String {
    value
        .as_ref()
        .map(|v| format!("Some({v:?}.into())"))
        .unwrap_or_else(|| "None".into())
}
fn columns(values: &[GridColumnDefinition]) -> String {
    format!("vec![{}]", values.iter().map(|c|format!("rust_desktop_ui_xaml::GridColumnDefinition {{ field: {:?}.into(), header: {:?}.into(), width: {:?}, min_width: {:?}, max_width: {:?}, is_pinned: {}, is_read_only: {} }}",c.field,c.header,c.width,c.min_width,c.max_width,c.is_pinned,c.is_read_only)).collect::<Vec<_>>().join(","))
}
pub(super) fn code(grid: &GridControlDefinition) -> String {
    let summaries = grid.summaries.iter().map(|s|format!("rust_desktop_ui_xaml::GridSummaryDefinition {{ title:{:?}.into(), function:rust_desktop_ui_xaml::GridSummaryFunction::{:?}, field:{} }}",s.title,s.function,optional(&s.field))).collect::<Vec<_>>().join(",");
    let bands = grid
        .bands
        .iter()
        .map(|b| {
            format!(
                "rust_desktop_ui_xaml::GridBandDefinition {{ title:{:?}.into(),fields:{} }}",
                b.title,
                strings(&b.fields)
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let detail = grid.detail.as_ref().map(|d|format!("Some(rust_desktop_ui_xaml::GridDetailDefinition {{ data_source:{:?}.into(),master_field:{:?}.into(),foreign_field:{:?}.into(),columns:{} }})",d.data_source,d.master_field,d.foreign_field,columns(&d.columns))).unwrap_or_else(||"None".into());
    let templates = grid.cell_templates.iter().map(|t|format!("rust_desktop_ui_xaml::GridCellTemplateDefinition {{ field:{:?}.into(),prefix:{:?}.into(),suffix:{:?}.into(),icon:{},emphasis:{} }}",t.field,t.prefix,t.suffix,t.icon.map(|icon|format!("Some(rust_desktop_ui_xaml::IconSymbol::{icon:?})")).unwrap_or_else(||"None".into()),t.emphasis)).collect::<Vec<_>>().join(",");
    let editors = grid.editors.iter().map(|e|format!("rust_desktop_ui_xaml::GridEditorDefinition {{ field:{:?}.into(),factory:{:?}.into() }}",e.field,e.factory)).collect::<Vec<_>>().join(",");
    format!(
        "rust_desktop_ui_xaml::GridControlDefinition {{ data_source: {:?}.into(), columns: {}, is_read_only: {}, filter_text: {:?}.into(), selected_value: {}, view:rust_desktop_ui_xaml::GridViewDefinition::{:?},group_fields:{},summaries:vec![{summaries}],bands:vec![{bands}],detail:{detail},cell_templates:vec![{templates}],editors:vec![{editors}] }}",
        grid.data_source,
        columns(&grid.columns),
        grid.is_read_only,
        grid.filter_text,
        optional(&grid.selected_value),
        grid.view,
        strings(&grid.group_fields)
    )
}
