//! View-local criteria, header facets, ordering and incremental search.
use super::*;
use rust_desktop_ui_data::{
    ComparisonOperator as Op, Criteria, FieldSchema, FieldType, QueryEngine,
    QueryExpression as Expr, QueryRequest, TextComparison, TextMatchOperator, compare_values,
};
use std::collections::BTreeMap;
#[derive(Clone, Default)]
pub(super) struct GridFilters {
    pub(super) columns: BTreeMap<String, Criteria>,
    pub(super) facets: BTreeMap<String, Vec<CellValue>>,
    pub(super) inputs: BTreeMap<String, String>,
    pub(super) rule: Option<Criteria>,
    pub(super) disabled: bool,
    pub(super) priorities: BTreeMap<String, Vec<CellValue>>,
}
impl GridFilters {
    pub(super) fn is_empty(&self) -> bool {
        self.columns.is_empty()
            && self.facets.is_empty()
            && self.rule.is_none()
            && self.priorities.is_empty()
    }
}
fn equals(field: &str, value: CellValue) -> Criteria {
    if value == CellValue::Null {
        Criteria::IsNull(Expr::field(field))
    } else {
        Criteria::Compare {
            left: Expr::field(field),
            operator: Op::Equal,
            right: Expr::Literal(value),
        }
    }
}
pub(super) fn auto_rule(field: &FieldSchema, input: &str) -> Result<Criteria, ControlError> {
    let name = &field.name;
    let input = input.trim();
    if input.eq_ignore_ascii_case("NULL") {
        return Ok(Criteria::IsNull(Expr::field(name)));
    }
    let parse = |v: &str| field.parse(v.trim()).map_err(error);
    let compare = |op, v| Criteria::Compare {
        left: Expr::field(name),
        operator: op,
        right: Expr::Literal(v),
    };
    if field.kind != FieldType::Text
        && let Some((a, b)) = input.split_once("..")
    {
        let lo = parse(a)?;
        let hi = parse(b)?;
        if lo == CellValue::Null || hi == CellValue::Null || compare_values(&lo, &hi).is_gt() {
            return Err(error("Invalid inclusive range"));
        }
        return Ok(Criteria::All(vec![
            compare(Op::GreaterOrEqual, lo),
            compare(Op::LessOrEqual, hi),
        ]));
    }
    for (prefix, op) in [
        (">=", Op::GreaterOrEqual),
        ("<=", Op::LessOrEqual),
        ("!=", Op::NotEqual),
        ("=", Op::Equal),
        (">", Op::Greater),
        ("<", Op::Less),
    ] {
        if let Some(v) = input.strip_prefix(prefix) {
            return Ok(compare(op, parse(v)?));
        }
    }
    if field.kind == FieldType::Text {
        Ok(Criteria::TextMatch {
            text: Expr::field(name),
            pattern: Expr::Literal(CellValue::Text(input.into())),
            operator: TextMatchOperator::Contains,
            comparison: TextComparison::UnicodeLowercase,
        })
    } else {
        Ok(equals(name, parse(input)?))
    }
}
impl SnapshotSource {
    pub(super) fn filtered_keys(&self, except: Option<&str>) -> Result<Vec<i64>, ControlError> {
        let mut keys = self.data_query.row_keys(&self.table).map_err(error)?;
        if let Some((field, value)) = &self.relation {
            keys.retain(|k| {
                *value != CellValue::Null && self.table.value(*k, field) == Some(value)
            });
        }
        let f = &self.filters;
        let mut terms: Vec<_> = f
            .columns
            .iter()
            .filter(|(name, _)| Some(name.as_str()) != except)
            .map(|(_, v)| v.clone())
            .collect();
        if !f.disabled
            && let Some(rule) = &f.rule
        {
            terms.push(rule.clone());
        }
        if !terms.is_empty() {
            let filter = if terms.len() == 1 {
                terms.pop().unwrap()
            } else {
                Criteria::All(terms)
            };
            let accepted: HashSet<_> = QueryEngine::execute(
                &self.table,
                &QueryRequest {
                    filter: Some(filter),
                    ..Default::default()
                },
            )
            .map_err(error)?
            .row_keys
            .into_iter()
            .collect();
            keys.retain(|k| accepted.contains(k));
        }
        for (field, values) in &f.facets {
            if except == Some(field.as_str()) {
                continue;
            }
            let accepted: HashSet<_> = values.iter().map(projection::value_identity).collect();
            keys.retain(|k| {
                self.table
                    .value(*k, field)
                    .is_some_and(|v| accepted.contains(&projection::value_identity(v)))
            });
        }
        if !f.priorities.is_empty() && !self.data_query.sort.is_empty() {
            let ranks: HashMap<_, HashMap<_, _>> = f
                .priorities
                .iter()
                .map(|(f, values)| {
                    (
                        f,
                        values
                            .iter()
                            .enumerate()
                            .map(|(i, v)| (projection::value_identity(v), i))
                            .collect(),
                    )
                })
                .collect();
            keys.sort_by(|a, b| {
                for sort in &self.data_query.sort {
                    let av = self.table.value(*a, &sort.field).unwrap();
                    let bv = self.table.value(*b, &sort.field).unwrap();
                    let ranked = ranks
                        .get(&sort.field)
                        .map(|r| {
                            let rank = |v: &CellValue| {
                                *r.get(&projection::value_identity(v)).unwrap_or(&usize::MAX)
                            };
                            rank(av).cmp(&rank(bv))
                        })
                        .unwrap_or(std::cmp::Ordering::Equal);
                    let cmp = ranked.then_with(|| compare_values(av, bv));
                    if !cmp.is_eq() {
                        return if sort.direction == rust_desktop_ui_data::SortDirection::Descending
                        {
                            cmp.reverse()
                        } else {
                            cmp
                        };
                    }
                }
                a.cmp(b)
            });
        }
        Ok(keys)
    }
}
impl Controls {
    fn change_grid_filters(
        &mut self,
        id: WidgetId,
        edit: impl FnOnce(&mut SnapshotSource) -> Result<(), ControlError>,
    ) -> Result<(), ControlError> {
        let g = self.grids.get(&id).ok_or_else(|| error("Unknown grid"))?;
        let mut src = g.source.clone();
        edit(&mut src)?;
        self.install_grid_presentation(id, src, g.current_arrangement())
    }
    pub fn set_grid_auto_filter(
        &mut self,
        id: WidgetId,
        field: &str,
        text: &str,
    ) -> Result<(), ControlError> {
        self.change_grid_filters(id, |s| {
            if text.len() > 4096 {
                return Err(error("Column filter exceeds 4096 bytes"));
            }
            let schema = s
                .table
                .schema()
                .field(field)
                .ok_or_else(|| error("Unknown filter field"))?;
            if text.trim().is_empty() {
                s.filters.inputs.remove(field);
                s.filters.columns.remove(field);
            } else {
                s.filters
                    .columns
                    .insert(field.into(), auto_rule(schema, text)?);
                s.filters.inputs.insert(field.into(), text.into());
            }
            Ok(())
        })
    }
    /// None clears this facet. An empty selection matches zero rows.
    pub fn set_grid_facet(
        &mut self,
        id: WidgetId,
        field: &str,
        values: Option<Vec<CellValue>>,
    ) -> Result<(), ControlError> {
        self.change_grid_filters(id, |s| {
            let schema = s
                .table
                .schema()
                .field(field)
                .ok_or_else(|| error("Unknown facet field"))?;
            if let Some(values) = values {
                if values.len() > s.table.limits().max_rows {
                    return Err(error("Too many facet values"));
                }
                for v in &values {
                    schema.validate(v).map_err(error)?;
                }
                s.filters.facets.insert(field.into(), values);
            } else {
                s.filters.facets.remove(field);
            }
            Ok(())
        })
    }
    pub fn grid_facet_values(
        &self,
        id: WidgetId,
        field: &str,
    ) -> Result<Vec<CellValue>, ControlError> {
        let s = &self
            .grids
            .get(&id)
            .ok_or_else(|| error("Unknown grid"))?
            .source;
        if s.table.schema().field(field).is_none() {
            return Err(error("Unknown facet field"));
        }
        let mut vals: Vec<_> = s
            .filtered_keys(Some(field))?
            .into_iter()
            .map(|k| s.table.value(k, field).unwrap().clone())
            .collect();
        vals.sort_by(compare_values);
        vals.dedup();
        Ok(vals)
    }
    pub fn set_grid_sort_priority(
        &mut self,
        id: WidgetId,
        field: &str,
        values: Vec<CellValue>,
    ) -> Result<(), ControlError> {
        self.change_grid_filters(id, |s| {
            let schema = s
                .table
                .schema()
                .field(field)
                .ok_or_else(|| error("Unknown priority field"))?;
            if values.len() > 1024 {
                return Err(error("Too many custom ranks"));
            }
            let mut seen = HashSet::new();
            for v in &values {
                schema.validate(v).map_err(error)?;
                if !seen.insert(projection::value_identity(v)) {
                    return Err(error("Duplicate custom rank"));
                }
            }
            if values.is_empty() {
                s.filters.priorities.remove(field);
            } else {
                s.filters.priorities.insert(field.into(), values);
            }
            Ok(())
        })
    }
    pub fn set_grid_criteria(&mut self, id: WidgetId, text: &str) -> Result<(), ControlError> {
        if self.grid_criteria(id).as_deref() == Some(text) {
            return Ok(());
        }
        if let Some(v) = self
            .grids
            .get(&id)
            .and_then(|g| g.criteria_validator.as_ref())
        {
            v.check(text).map_err(error)?;
        }
        self.change_grid_filters(id, |s| {
            s.filters.rule = if text.trim().is_empty() {
                None
            } else {
                Some(
                    rust_desktop_ui_data::parse_criteria_text(s.table.schema(), text)
                        .map_err(error)?,
                )
            };
            Ok(())
        })
    }
    pub fn grid_criteria(&self, id: WidgetId) -> Option<String> {
        let g = self.grids.get(&id)?;
        Some(
            g.source
                .filters
                .rule
                .as_ref()
                .and_then(|r| rust_desktop_ui_data::format_criteria_text(r).ok())
                .unwrap_or_default(),
        )
    }
    pub fn grid_criteria_enabled(&self, id: WidgetId) -> Option<bool> {
        Some(!self.grids.get(&id)?.source.filters.disabled)
    }
    pub fn set_grid_criteria_enabled(
        &mut self,
        id: WidgetId,
        enabled: bool,
    ) -> Result<(), ControlError> {
        if self.grid_criteria_enabled(id) == Some(enabled) {
            return Ok(());
        }
        self.change_grid_filters(id, |s| {
            s.filters.disabled = !enabled;
            Ok(())
        })
    }
    pub fn set_grid_search(&mut self, id: WidgetId, text: &str) -> Result<(), ControlError> {
        if text.len() > 4096 {
            return Err(error("Search exceeds 4096 bytes"));
        }
        self.grids
            .get_mut(&id)
            .ok_or_else(|| error("Unknown grid"))?
            .search = text.to_lowercase();
        self.scene_dirty = true;
        Ok(())
    }
    /// Move to the next matching displayed value in visual row/column order, with wraparound.
    pub fn find_grid_match(
        &mut self,
        id: WidgetId,
        forward: bool,
        current_column: bool,
    ) -> Result<bool, ControlError> {
        let g = self.grids.get(&id).ok_or_else(|| error("Unknown grid"))?;
        if g.session.is_some() || g.search.is_empty() {
            return Ok(false);
        }
        let mut cols = g.model.display_column_order();
        if current_column {
            let current = g.model.focused_column().or_else(|| cols.first().copied());
            cols.retain(|c| Some(*c) == current);
        }
        let keys = g.visual_keys()?;
        let n = keys.len() * cols.len();
        if n == 0 {
            return Ok(false);
        }
        let start = g
            .selected_key
            .and_then(|key| keys.iter().position(|k| *k == key))
            .and_then(|i| {
                cols.iter()
                    .position(|c| Some(*c) == g.model.focused_column())
                    .map(|col| i * cols.len() + col)
            })
            .unwrap_or(if forward { n - 1 } else { 0 });
        for step in 1..=n {
            let pos = if forward {
                (start + step) % n
            } else {
                (start + n - step) % n
            };
            let key = keys[pos / cols.len()];
            let col = cols[pos % cols.len()];
            if g.source
                .cell(row_key(key), col)
                .to_lowercase()
                .contains(&g.search)
            {
                let field = g.source.fields[col.0 as usize].clone();
                self.grid_focus_cell(id, key, Some(&field))?;
                return Ok(true);
            }
        }
        Ok(false)
    }
}
