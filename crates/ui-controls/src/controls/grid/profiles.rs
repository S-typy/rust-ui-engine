//! Named, versioned layouts and schema-driven column sizing.
use super::*;
use rust_desktop_ui_data::{
    QueryExpression, format_criteria_text, format_expression_text, parse_criteria_text,
    parse_expression_text,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
type Values = BTreeMap<String, Vec<String>>;
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Profile {
    version: u16,
    name: String,
    cards: bool,
    columns: Vec<GridColumnArrangement>,
    bands: Vec<GridBand>,
    tiers: Vec<Vec<GridBand>>,
    wrap: Vec<String>,
    merge: Vec<String>,
    text_templates: BTreeMap<String, (String, String)>,
    right: Vec<String>,
    stacked: Vec<(String, String)>,
    groups: Vec<String>,
    group_rules: Vec<(String, String)>,
    hierarchy: bool,
    group_footer: bool,
    group_panel: bool,
    group_summary: Option<usize>,
    group_descending: bool,
    sort: Vec<(String, bool)>,
    filter: String,
    inputs: BTreeMap<String, String>,
    facets: Values,
    ranks: Values,
    criteria: Option<String>,
    disabled: bool,
    formats: Vec<GridFormatRule>,
}
const MAX_PROFILE: usize = 1024 * 1024;
fn name_ok(name: &str) -> bool {
    !name.trim().is_empty() && name.len() <= 128 && !name.chars().any(char::is_control)
}
fn pack(fields: &BTreeMap<String, Vec<CellValue>>) -> Result<Values, ControlError> {
    fields
        .iter()
        .map(|(f, v)| {
            Ok((
                f.clone(),
                v.iter()
                    .map(|v| {
                        format_expression_text(&QueryExpression::Literal(v.clone())).map_err(error)
                    })
                    .collect::<Result<_, _>>()?,
            ))
        })
        .collect()
}
fn unpack(
    table: &DataTable,
    fields: Values,
) -> Result<BTreeMap<String, Vec<CellValue>>, ControlError> {
    fields
        .into_iter()
        .map(|(field, vs)| {
            let schema = table
                .schema()
                .field(&field)
                .ok_or_else(|| error("Unknown saved field"))?;
            let mut values = Vec::new();
            let mut seen = HashSet::new();
            for s in vs {
                let QueryExpression::Literal(v) =
                    parse_expression_text(table.schema(), &s).map_err(error)?
                else {
                    return Err(error("Expected literal"));
                };
                schema.validate(&v).map_err(error)?;
                if !seen.insert(projection::value_identity(&v)) {
                    return Err(error("Repeated saved value"));
                }
                values.push(v);
            }
            Ok((field, values))
        })
        .collect()
}
impl Controls {
    /// Serialize explicit user settings. The caller chooses durable storage.
    pub fn save_grid_layout(&self, id: WidgetId, name: &str) -> Result<String, ControlError> {
        if !name_ok(name) {
            return Err(error("Invalid grid layout name"));
        }
        let grid = self.grids.get(&id).ok_or_else(|| error("Unknown grid"))?;
        let s = &grid.source;
        let p = &s.presentation;
        let f = &s.filters;
        let profile = Profile {
            version: 1,
            name: name.into(),
            cards: p.view == GridViewMode::Cards,
            columns: grid.current_arrangement(),
            bands: p.bands.clone(),
            tiers: p.band_rows.clone(),
            wrap: p.wrap_fields.clone(),
            merge: p.merge_fields.clone(),
            text_templates: p.text_templates.clone(),
            right: p.right_pinned_fields.clone(),
            stacked: p.stacked_fields.clone(),
            groups: p.group_fields.clone(),
            group_rules: p.group_expressions.clone(),
            hierarchy: p.hierarchical_groups,
            group_footer: p.group_footer,
            group_panel: p.show_group_panel,
            group_summary: p.group_sort_summary,
            group_descending: p.group_descending,
            filter: s.data_query.filter.clone(),
            sort: s
                .data_query
                .sort
                .iter()
                .map(|s| {
                    (
                        s.field.clone(),
                        s.direction == rust_desktop_ui_data::SortDirection::Descending,
                    )
                })
                .collect(),
            inputs: f.inputs.clone(),
            facets: pack(&f.facets)?,
            ranks: pack(&f.priorities)?,
            criteria: f
                .rule
                .as_ref()
                .map(format_criteria_text)
                .transpose()
                .map_err(error)?,
            disabled: f.disabled,
            formats: p.format_rules.clone(),
        };
        let text = serde_json::to_string_pretty(&profile).map_err(error)?;
        if text.len() > MAX_PROFILE {
            return Err(error("Grid layout exceeds 1 MiB"));
        }
        Ok(text)
    }
    pub fn restore_grid_layout(
        &mut self,
        id: WidgetId,
        name: &str,
        json: &str,
    ) -> Result<(), ControlError> {
        if json.len() > MAX_PROFILE {
            return Err(error("Grid layout exceeds 1 MiB"));
        }
        let p: Profile = serde_json::from_str(json).map_err(error)?;
        if p.version != 1 || !name_ok(name) || p.name != name || p.filter.len() > 4096 {
            return Err(error("Invalid grid layout version, name or filter"));
        }
        let g = self.grids.get(&id).ok_or_else(|| error("Unknown grid"))?;
        if let Some(v) = &g.filter_validator {
            v.check(&p.filter).map_err(error)?
        }
        if let Some(v) = &g.criteria_validator {
            v.check(p.criteria.as_deref().unwrap_or(""))
                .map_err(error)?
        }
        let mut src = g.source.clone();
        let mut columns = p.columns;
        if columns.is_empty() || columns.len() > 64 || !columns.iter().any(|c| c.visible) {
            return Err(error("Invalid saved columns"));
        }
        for col in g.current_arrangement() {
            if !columns.iter().any(|c| c.field == col.field) {
                columns.push(col)
            }
        }
        let opts = &mut src.presentation;
        opts.view = if p.cards {
            GridViewMode::Cards
        } else {
            GridViewMode::Table
        };
        opts.wrap_fields = p.wrap;
        opts.merge_fields = p.merge;
        opts.text_templates = p.text_templates;
        opts.bands = p.bands;
        opts.band_rows = p.tiers;
        opts.group_fields = p.groups;
        opts.group_expressions = p.group_rules;
        opts.group_footer = p.group_footer;
        opts.show_group_panel = p.group_panel;
        opts.group_sort_summary = p.group_summary;
        opts.group_descending = p.group_descending;
        opts.hierarchical_groups = p.hierarchy;
        opts.right_pinned_fields = p.right;
        opts.stacked_fields = p.stacked;
        opts.format_rules = p.formats;
        opts.validate(&g.settings.columns)?;
        let schema = src.table.schema();
        let filters = &mut src.filters;
        filters.rule = p
            .criteria
            .map(|s| parse_criteria_text(schema, &s))
            .transpose()
            .map_err(error)?;
        filters.disabled = p.disabled;
        filters.inputs = p.inputs;
        filters.columns = filters
            .inputs
            .iter()
            .map(|(f, s)| {
                if s.len() > 4096 {
                    return Err(error("Invalid saved filter"));
                }
                Ok((
                    f.clone(),
                    filtering::auto_rule(
                        schema
                            .field(f)
                            .ok_or_else(|| error("Unknown filter field"))?,
                        s,
                    )?,
                ))
            })
            .collect::<Result<_, _>>()?;
        filters.facets = unpack(&src.table, p.facets)?;
        filters.priorities = unpack(&src.table, p.ranks)?;
        if filters.priorities.values().any(|v| v.len() > 1024) {
            return Err(error("Too many custom ranks"));
        }
        let mut seen = HashSet::new();
        let sort = p
            .sort
            .into_iter()
            .map(|(field, desc)| {
                if !src.fields.contains(&field) || !seen.insert(field.clone()) {
                    return Err(error("Invalid saved sort"));
                }
                Ok(SortDescriptor {
                    field,
                    direction: if desc {
                        rust_desktop_ui_data::SortDirection::Descending
                    } else {
                        rust_desktop_ui_data::SortDirection::Ascending
                    },
                })
            })
            .collect::<Result<_, _>>()?;
        self.install_grid_profile(
            id,
            src,
            columns,
            Some(DataQuery {
                filter: p.filter,
                sort,
            }),
        )
    }
    /// Shape the header and unique displayed values before changing the width.
    pub fn auto_fit_grid_column(&mut self, id: WidgetId, field: &str) -> Result<f32, ControlError> {
        let grid = self.grids.get(&id).ok_or_else(|| error("Unknown grid"))?;
        let index = grid
            .source
            .fields
            .iter()
            .position(|f| f == field)
            .ok_or_else(|| error("Unknown grid field"))?;
        let c = &grid.settings.columns[index];
        let (min, max) = (c.min_width, c.max_width);
        let template = grid.cell_templates.get(&ColumnId(index as u64));
        let padding = if template.is_some_and(|t| t.icon.is_some()) {
            46.
        } else {
            24.
        };
        let mut labels = HashSet::from([c.title.clone()]);
        for key in &grid.source.keys {
            let label = grid.source.cell(row_key(*key), ColumnId(index as u64));
            labels.insert(template.map_or_else(
                || label.clone(),
                |t| format!("{}{}{}", t.prefix, label, t.suffix),
            ));
        }
        let mut arrangement = grid.current_arrangement();
        let mut width = min;
        for label in labels {
            width = width
                .max(self.measure_text(&label, max, false)?.width + padding)
                .min(max);
            if width >= max {
                break;
            }
        }
        arrangement
            .iter_mut()
            .find(|c| c.field == field)
            .unwrap()
            .width = width;
        self.arrange_grid_columns(id, arrangement)?;
        Ok(width)
    }
}
