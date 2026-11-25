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
    pub display_column: Option<String>,
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
pub struct GridFormatDefinition {
    pub field: String,
    pub kind: String,
    pub criteria: String,
    pub count: usize,
    pub whole_row: bool,
    pub enabled: bool,
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
    pub choice: Option<(String, String, Vec<String>)>,
    pub mask: Option<String>,
    pub command: Option<String>,
    pub when: Option<(String, String)>,
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
    if grid
        .group_sort_summary
        .is_some_and(|i| i >= grid.summaries.len())
    {
        return Err("Unknown GroupSortSummary".into());
    }
    for summary in &grid.summaries {
        if summary.display_column.as_ref().is_some_and(|f| !field(f))
            || !caption(&summary.title)
            || match summary.function {
                GridSummaryFunction::CountRows => summary.field.is_some(),
                _ => summary.field.as_deref().is_none_or(|value| !field(value)),
            }
        {
            return Err("GridSummary requires a bounded title; CountRows has no field and other functions require FieldName".into());
        }
    }
    let mut parent: Option<&[GridBandDefinition]> = None;
    if grid.band_rows.len() >= 8
        || grid.bands.len() + grid.band_rows.iter().map(Vec::len).sum::<usize>() > 64
    {
        return Err("Invalid nested grid band layout".into());
    }
    for row in std::iter::once(&grid.bands).chain(&grid.band_rows) {
        if parent.is_some() && row.is_empty() {
            return Err("Invalid nested grid band layout".into());
        }
        let mut band_fields = HashSet::new();
        for band in row {
            if let Some(parents) = parent
                && !parents
                    .iter()
                    .any(|p| band.fields.iter().all(|f| p.fields.contains(f)))
            {
                return Err("Invalid nested grid band layout".into());
            }

            if band.title.is_empty()
                || !caption(&band.title)
                || band.fields.is_empty()
                || !unique_fields(&band.fields, MAX_GRID_COLUMNS)
                || band.fields.iter().any(|field| !band_fields.insert(field))
            {
                return Err("GridBand requires Header and bounded, unique, disjoint Fields".into());
            }
        }
        parent = Some(row);
    }
    let mut names = std::collections::HashSet::new();
    if grid.relations.len() > 32
        || grid
            .relations
            .iter()
            .any(|(n, _)| !valid_name(n) || !names.insert(n))
    {
        return Err("Relations require unique names and at most 32 entries".into());
    }
    for (detail, inline) in grid
        .detail
        .iter()
        .map(|d| (d, true))
        .chain(grid.relations.iter().map(|(_, d)| (d, false)))
    {
        if !valid_name(&detail.data_source)
            || !field(&detail.master_field)
            || !field(&detail.foreign_field)
            || (inline && detail.columns.is_empty())
        {
            return Err("GridDetail requires a registered DataSource, MasterField, ForeignField and explicit columns".into());
        }
        validate_columns(&detail.columns)?;
        if detail
            .columns
            .iter()
            .any(|c| c.expression.is_some() || c.value_source.is_some())
        {
            return Err("Calculated fields require a master GridColumn".into());
        }
        if inline && !grid.columns.is_empty() && detail.columns.len() > grid.columns.len() {
            return Err("GridDetail cannot have more columns than its master grid".into());
        }
    }
    let mut fields = HashSet::new();
    if grid.text_templates.iter().any(|(k, (a, b))| {
        !matches!(k.as_str(), "Header" | "Indicator" | "Summary") || a.len() > 256 || b.len() > 256
    }) {
        return Err("Invalid grid text template".into());
    }
    if grid.format_rules.len() > 32
        || grid.format_rules.iter().any(|r| {
            !field(&r.field)
                || !grid.columns.is_empty() && !grid.columns.iter().any(|c| c.field == r.field)
                || !matches!(
                    r.kind.as_str(),
                    "Expression"
                        | "Top"
                        | "Duplicates"
                        | "AboveAverage"
                        | "DataBar"
                        | "Icons"
                        | "ColorScale"
                )
                || (r.kind == "Expression") == r.criteria.trim().is_empty()
                || r.criteria.len() > 16384
                || r.count == 0
        })
    {
        return Err("Invalid GridFormatRule field, kind or arguments".into());
    }
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
        || grid.editors.iter().any(|e| {
            !field(&e.field)
                || !fields.insert((&e.field, e.when.is_some()))
                || !valid_name(&e.factory)
                || e.mask
                    .as_ref()
                    .is_some_and(|s| s.is_empty() || s.len() > 4096)
                || (e.factory == "Masked") != e.mask.is_some()
                || e.command
                    .as_ref()
                    .is_some_and(|s| !valid_name(s) || e.factory != "Button")
                || e.when.as_ref().is_some_and(|(f, v)| {
                    !field(f)
                        || v.len() > 4096
                        || !matches!(
                            e.factory.as_str(),
                            "ComboBox" | "Lookup" | "Masked" | "Memo" | "Button"
                        )
                })
                || match &e.choice {
                    Some((source, key, fields)) => {
                        !matches!(e.factory.as_str(), "ComboBox" | "Lookup")
                            || !valid_name(source)
                            || !field(key)
                            || fields.is_empty()
                            || !unique_fields(fields, 32)
                    }
                    None => matches!(e.factory.as_str(), "ComboBox" | "Lookup"),
                }
        })
    {
        return Err("GridEditor requires at most one base and one conditional recipe per field and registered factory identifiers".into());
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
            | "GridControl.Relations"
            | "GridControl.Detail"
            | "GridControl.TextTemplates"
            | "GridControl.FormatRules"
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
        "GridControl.Relations" => ("GridRelation", 32),
        "GridControl.TextTemplates" => ("GridTextTemplate", 3),
        "GridControl.FormatRules" => ("GridFormatRule", 32),
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
                attributes(item, &["Title", "Function", "FieldName", "DisplayColumn"])?;
                empty(item)?;
                let function = function(item)?;
                grid.summaries.push(GridSummaryDefinition {
                    title: item
                        .attribute("Title")
                        .map(str::to_owned)
                        .unwrap_or_else(|| format!("{function:?}")),
                    function,
                    display_column: item.attribute("DisplayColumn").map(str::to_owned),
                    field: item.attribute("FieldName").map(str::to_owned),
                });
            }
            "GridBand" => {
                parse_band(item, depth + 1, count, 0, grid)?;
            }
            "GridTextTemplate" => {
                attributes(item, &["Target", "Prefix", "Suffix"])?;
                empty(item)?;
                if grid
                    .text_templates
                    .insert(
                        value(item, "Target"),
                        (value(item, "Prefix"), value(item, "Suffix")),
                    )
                    .is_some()
                {
                    return Err(at_node(item, "Repeated template target"));
                }
            }
            "GridFormatRule" => {
                attributes(
                    item,
                    &[
                        "FieldName",
                        "Kind",
                        "Criteria",
                        "Count",
                        "WholeRow",
                        "IsEnabled",
                    ],
                )?;
                empty(item)?;
                let flag = |name, default| {
                    item.attribute_node(name)
                        .map(|a| boolean(item.document(), a))
                        .transpose()
                        .map(|v| v.unwrap_or(default))
                };
                let count = item
                    .attribute("Count")
                    .unwrap_or("1")
                    .parse()
                    .map_err(|_| at_node(item, "Invalid format Count"))?;
                let kind = value(item, "Kind");
                if item.attribute("Count").is_some() && kind != "Top" {
                    return Err(at_node(item, "Count requires Top"));
                }
                grid.format_rules.push(GridFormatDefinition {
                    field: value(item, "FieldName"),
                    kind,
                    criteria: value(item, "Criteria"),
                    count,
                    whole_row: flag("WholeRow", false)?,
                    enabled: flag("IsEnabled", true)?,
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
                attributes(
                    item,
                    &[
                        "FieldName",
                        "Factory",
                        "ItemsSource",
                        "ValueField",
                        "DisplayFields",
                        "Mask",
                        "Command",
                        "WhenField",
                        "WhenValue",
                    ],
                )?;
                empty(item)?;
                if item.attribute("WhenField").is_some() != item.attribute("WhenValue").is_some() {
                    return Err(at_node(
                        item,
                        "Conditional editor requires WhenField and WhenValue",
                    ));
                }
                grid.editors.push(GridEditorDefinition {
                    mask: item.attribute("Mask").map(str::to_owned),
                    command: item.attribute("Command").map(str::to_owned),
                    when: item
                        .attribute("WhenField")
                        .zip(item.attribute("WhenValue"))
                        .map(|(f, v)| (f.into(), v.into())),
                    field: value(item, "FieldName"),
                    choice: if item.attribute("ItemsSource").is_some()
                        || item.attribute("ValueField").is_some()
                        || item.attribute("DisplayFields").is_some()
                    {
                        Some((
                            value(item, "ItemsSource"),
                            value(item, "ValueField"),
                            item.attribute_node("DisplayFields")
                                .map(|a| fields(item.document(), a, 32))
                                .transpose()?
                                .unwrap_or_default(),
                        ))
                    } else {
                        None
                    },
                    factory: item.attribute("Factory").unwrap_or("Auto").into(),
                });
            }
            "GridDetail" | "GridRelation" => {
                if expected == "GridRelation" {
                    attributes(item, &["Name", "DataSource", "MasterField", "ForeignField"])?;
                } else {
                    attributes(item, &["DataSource", "MasterField", "ForeignField"])?;
                }
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
                if expected == "GridRelation" {
                    grid.relations.push((value(item, "Name"), detail));
                } else {
                    grid.detail = Some(detail);
                }
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

fn parse_band(
    node: Node<'_, '_>,
    depth: usize,
    count: &mut usize,
    level: usize,
    grid: &mut GridControlDefinition,
) -> Result<Vec<String>, Diagnostic> {
    if level >= 8 {
        return Err(at_node(node, "At most eight band tiers are supported"));
    }
    attributes(node, &["Header", "Fields"])?;
    let declared = node.attribute_node("Fields");
    let mut members = declared
        .map(|a| fields(node.document(), a, MAX_GRID_COLUMNS))
        .transpose()?
        .unwrap_or_default();
    for child in node.children() {
        if !child.is_element() {
            require_empty_content(child)?;
            continue;
        }
        if declared.is_some()
            || child.tag_name().namespace() != Some(UI_NAMESPACE)
            || child.tag_name().name() != "GridBand"
        {
            return Err(at_node(
                child,
                "A band contains either Fields or child GridBand elements",
            ));
        }
        count_metadata(child, depth + 1, count)?;
        members.extend(parse_band(child, depth + 1, count, level + 1, grid)?);
        if members.len() > MAX_GRID_COLUMNS {
            return Err(at_node(node, "Too many fields in band"));
        }
    }
    let band = GridBandDefinition {
        title: value(node, "Header"),
        fields: members.clone(),
    };
    if level == 0 {
        grid.bands.push(band);
    } else {
        if grid.band_rows.len() < level {
            grid.band_rows.resize_with(level, Vec::new);
        }
        grid.band_rows[level - 1].push(band);
    }
    Ok(members)
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
    format!("vec![{}]", values.iter().map(|c|format!("rust_desktop_ui_xaml::GridColumnDefinition {{ field: {:?}.into(), header: {:?}.into(), width: {:?}, min_width: {:?}, max_width: {:?}, is_pinned: {}, is_read_only: {}, expression: {},group_expression:{},value_source:{},value_member:{} }}",c.field,c.header,c.width,c.min_width,c.max_width,c.is_pinned,c.is_read_only,optional(&c.expression),optional(&c.group_expression),optional(&c.value_source),optional(&c.value_member))).collect::<Vec<_>>().join(","))
}
pub(super) fn code(grid: &GridControlDefinition) -> String {
    let summaries = grid.summaries.iter().map(|s|format!("rust_desktop_ui_xaml::GridSummaryDefinition {{ title:{:?}.into(), function:rust_desktop_ui_xaml::GridSummaryFunction::{:?}, field:{},display_column:{} }}",s.title,s.function,optional(&s.field),optional(&s.display_column))).collect::<Vec<_>>().join(",");
    let stacks = grid
        .stacked_fields
        .iter()
        .map(|(f, r)| format!("({f:?}.into(),{r:?}.into())"))
        .collect::<Vec<_>>()
        .join(",");
    let wrap = strings(&grid.wrap_fields);
    let merge = strings(&grid.merge_fields);
    let right = strings(&grid.right_pinned_fields);
    let band_code = |row: &[GridBandDefinition]| {
        row.iter()
            .map(|b| {
                format!(
                    "rust_desktop_ui_xaml::GridBandDefinition {{ title:{:?}.into(),fields:{} }}",
                    b.title,
                    strings(&b.fields)
                )
            })
            .collect::<Vec<_>>()
            .join(",")
    };
    let bands = band_code(&grid.bands);
    let band_rows = grid
        .band_rows
        .iter()
        .map(|r| format!("vec![{}]", band_code(r)))
        .collect::<Vec<_>>()
        .join(",");
    let emit_detail = |d: &GridDetailDefinition| {
        format!(
            "rust_desktop_ui_xaml::GridDetailDefinition {{ data_source:{:?}.into(),master_field:{:?}.into(),foreign_field:{:?}.into(),columns:{} }}",
            d.data_source,
            d.master_field,
            d.foreign_field,
            columns(&d.columns)
        )
    };
    let detail = grid
        .detail
        .as_ref()
        .map_or_else(|| "None".into(), |d| format!("Some({})", emit_detail(d)));
    let relations = grid
        .relations
        .iter()
        .map(|(n, d)| format!("({n:?}.into(),{})", emit_detail(d)))
        .collect::<Vec<_>>()
        .join(",");
    let roles = grid
        .text_templates
        .iter()
        .map(|(k, (a, b))| format!("({k:?}.into(),({a:?}.into(),{b:?}.into()))"))
        .collect::<Vec<_>>()
        .join(",");
    let formats=grid.format_rules.iter().map(|r|format!("rust_desktop_ui_xaml::GridFormatDefinition{{field:{:?}.into(),kind:{:?}.into(),criteria:{:?}.into(),count:{},whole_row:{},enabled:{}}}",r.field,r.kind,r.criteria,r.count,r.whole_row,r.enabled)).collect::<Vec<_>>().join(",");
    let templates = grid.cell_templates.iter().map(|t|format!("rust_desktop_ui_xaml::GridCellTemplateDefinition {{ field:{:?}.into(),prefix:{:?}.into(),suffix:{:?}.into(),icon:{},emphasis:{} }}",t.field,t.prefix,t.suffix,t.icon.map(|icon|format!("Some(rust_desktop_ui_xaml::IconSymbol::{icon:?})")).unwrap_or_else(||"None".into()),t.emphasis)).collect::<Vec<_>>().join(",");
    let editors = grid.editors.iter().map(|e|format!("rust_desktop_ui_xaml::GridEditorDefinition {{ field:{:?}.into(),factory:{:?}.into(),mask:{},command:{},when:{},choice:{} }}",e.field,e.factory,optional(&e.mask),optional(&e.command),e.when.as_ref().map(|(f,v)|format!("Some(({f:?}.into(),{v:?}.into()))")).unwrap_or_else(||"None".into()),e.choice.as_ref().map(|(s,k,d)|format!("Some(({s:?}.into(),{k:?}.into(),{}))",strings(d))).unwrap_or_else(||"None".into()))).collect::<Vec<_>>().join(",");
    format!(
        "rust_desktop_ui_xaml::GridControlDefinition {{ data_source: {:?}.into(), columns: {}, is_read_only: {}, filter_text: {:?}.into(), criteria:{:?}.into(),criteria_disabled:{}, show_auto_filter_row:{},show_row_selector:{},parent_field:{}, selected_value: {}, view:rust_desktop_ui_xaml::GridViewDefinition::{:?},hierarchical_groups:{},group_sort_summary:{:?},group_descending:{},group_footer:{},show_group_panel:{},stacked_fields:vec![{stacks}],wrap_fields:{wrap},merge_fields:{merge},right_pinned_fields:{right},group_fields:{},summaries:vec![{summaries}],bands:vec![{bands}],band_rows:vec![{band_rows}],detail:{detail},relations:vec![{relations}],text_templates:[{roles}].into(),format_rules:vec![{formats}],cell_templates:vec![{templates}],editors:vec![{editors}] }}",
        grid.data_source,
        columns(&grid.columns),
        grid.is_read_only,
        grid.filter_text,
        grid.criteria,
        grid.criteria_disabled,
        grid.show_auto_filter_row,
        grid.show_row_selector,
        grid.parent_field
            .as_ref()
            .map_or_else(|| "None".to_string(), |f| format!("Some({f:?}.into())")),
        optional(&grid.selected_value),
        grid.view,
        grid.hierarchical_groups,
        grid.group_sort_summary,
        grid.group_descending,
        grid.group_footer,
        grid.show_group_panel,
        strings(&grid.group_fields)
    )
}
