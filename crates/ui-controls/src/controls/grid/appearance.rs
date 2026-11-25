//! Ordered, view-local conditional formatting over the accepted filter scope.
use super::*;
use rust_desktop_ui_data::{
    FieldType, QueryEngine, QueryRequest, compare_values, parse_criteria_text,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum GridFormatKind {
    Expression(String),
    Top(usize),
    AboveAverage,
    Duplicates,
    DataBar,
    ColorScale,
    Icons,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GridFormatRule {
    pub field: String,
    pub kind: GridFormatKind,
    pub whole_row: bool,
    pub enabled: bool,
}
#[derive(Clone, Default)]
pub(super) struct FormatScope {
    matches: HashSet<i64>,
    extent: Option<(CellValue, CellValue)>,
}
#[cfg(test)]
impl FormatScope {
    pub(super) fn contains(&self, key: i64) -> bool {
        self.matches.contains(&key)
    }
}
fn number(value: &CellValue) -> Option<f64> {
    match value {
        CellValue::Integer(i) => Some(*i as f64),
        CellValue::Real(n) => Some(*n),
        CellValue::Decimal(d) => Some(d.coefficient() as f64 / 10f64.powi(i32::from(d.scale()))),
        _ => None,
    }
}
pub(super) fn compile(source: &SnapshotSource) -> Result<Vec<FormatScope>, ControlError> {
    use GridFormatKind::*;
    let rules = &source.presentation.format_rules;
    if rules.len() > 32 {
        return Err(error("Grid supports up to 32 format rules"));
    }
    rules
        .iter()
        .map(|rule| {
            let schema = source
                .table
                .schema()
                .field(&rule.field)
                .filter(|_| source.fields.contains(&rule.field))
                .ok_or_else(|| error("Unknown format field"))?;
            let mut scope = FormatScope::default();
            if !matches!(rule.kind, Expression(_) | Duplicates)
                && !matches!(
                    schema.kind,
                    FieldType::Integer | FieldType::Decimal | FieldType::Real
                )
            {
                return Err(error("Numeric format requires a numeric field"));
            }
            if let Expression(text) = &rule.kind {
                let filter = parse_criteria_text(source.table.schema(), text).map_err(error)?;
                if rule.enabled {
                    scope.matches = QueryEngine::execute(
                        &source.table,
                        &QueryRequest {
                            filter: Some(filter),
                            ..Default::default()
                        },
                    )
                    .map_err(error)?
                    .row_keys
                    .into_iter()
                    .collect();
                }
                return Ok(scope);
            }
            if matches!(rule.kind, Top(0)) {
                return Err(error("Top count must be positive"));
            }
            if !rule.enabled {
                return Ok(scope);
            }
            let values = source.keys.iter().filter_map(|&key| {
                source
                    .table
                    .value(key, &rule.field)
                    .filter(|v| **v != CellValue::Null)
                    .map(|v| (key, v))
            });
            match &rule.kind {
                Duplicates => {
                    let mut first = HashMap::new();
                    for (key, value) in values {
                        if let Some(other) = first.insert(projection::value_identity(value), key) {
                            scope.matches.extend([other, key]);
                        }
                    }
                }
                Top(n) => {
                    let mut ranked: Vec<_> = values.collect();
                    ranked.sort_by(|(a, x), (b, y)| compare_values(y, x).then_with(|| a.cmp(b)));
                    scope
                        .matches
                        .extend(ranked.into_iter().take(*n).map(|(k, _)| k));
                }
                AboveAverage => {
                    use rust_desktop_ui_data::{
                        Aggregate, AggregateFunction, DecimalValue, QueryExpression,
                    };
                    let filtered = DataTable::with_limits(
                        source.table.schema().clone(),
                        source
                            .keys
                            .iter()
                            .filter_map(|k| source.table.row(*k).cloned())
                            .collect(),
                        source.table.limits(),
                    )
                    .map_err(error)?;
                    let avg = &QueryEngine::execute(
                        &filtered,
                        &QueryRequest {
                            aggregates: vec![Aggregate {
                                function: AggregateFunction::Average,
                                expression: Some(QueryExpression::field(&rule.field)),
                            }],
                            ..Default::default()
                        },
                    )
                    .map_err(error)?
                    .aggregates[0];
                    for (key, value) in values {
                        let above = match (value, avg) {
                            (CellValue::Integer(i), CellValue::Decimal(avg)) => {
                                DecimalValue::new(i128::from(*i), 0).map_err(error)? > *avg
                            }
                            _ => compare_values(value, avg).is_gt(),
                        };
                        if above {
                            scope.matches.insert(key);
                        }
                    }
                }
                DataBar | ColorScale | Icons => {
                    for (_, v) in values {
                        match &mut scope.extent {
                            None => scope.extent = Some((v.clone(), v.clone())),
                            Some((lo, hi)) => {
                                if compare_values(v, lo).is_lt() {
                                    *lo = v.clone();
                                }
                                if compare_values(v, hi).is_gt() {
                                    *hi = v.clone();
                                }
                            }
                        }
                    }
                }
                Expression(_) => unreachable!(),
            }
            Ok(scope)
        })
        .collect()
}
fn fraction(n: f64, lo: f64, hi: f64) -> f32 {
    if lo == hi {
        return 0.5;
    }
    let scale = lo.abs().max(hi.abs());
    let f = if (hi - lo).is_finite() {
        (n - lo) / (hi - lo)
    } else {
        (n / scale - lo / scale) / (hi / scale - lo / scale)
    };
    f.clamp(0., 1.) as f32
}
fn scaled_fraction(v: &CellValue, lo: &CellValue, hi: &CellValue) -> f32 {
    if lo == hi {
        return 0.5;
    }
    if v == lo {
        return 0.;
    }
    if v == hi {
        return 1.;
    }
    match (v, lo, hi) {
        (CellValue::Integer(n), CellValue::Integer(a), CellValue::Integer(b)) => {
            (n.abs_diff(*a) as f64 / b.abs_diff(*a) as f64) as f32
        }
        (CellValue::Decimal(n), CellValue::Decimal(a), CellValue::Decimal(b)) => {
            fn difference(
                x: rust_desktop_ui_data::DecimalValue,
                y: rust_desktop_ui_data::DecimalValue,
            ) -> f64 {
                let (coarse, fine) = if x.scale() <= y.scale() {
                    (x, y)
                } else {
                    (y, x)
                };
                let factor = 10i128.pow(u32::from(fine.scale() - coarse.scale()));
                if let Some(common) = coarse.coefficient().checked_mul(factor) {
                    return common.abs_diff(fine.coefficient()) as f64
                        / 10f64.powi(i32::from(fine.scale()));
                }
                let quotient = fine.coefficient().div_euclid(factor);
                let remainder = fine.coefficient().rem_euclid(factor) as f64 / factor as f64;
                let distance = coarse.coefficient().abs_diff(quotient) as f64;
                (if coarse.coefficient() > quotient {
                    distance - remainder
                } else {
                    distance + remainder
                }) / 10f64.powi(i32::from(coarse.scale()))
            }
            (difference(*n, *a) / difference(*b, *a)).clamp(0., 1.) as f32
        }
        _ => fraction(number(v).unwrap(), number(lo).unwrap(), number(hi).unwrap()),
    }
}
pub(super) fn paint(source: &SnapshotSource, frame: &mut GridFrame) {
    use GridFormatKind::*;
    if source.presentation.format_rules.is_empty() {
        return;
    }
    for row in &frame.semantics.rows {
        if source.projection.synthetic.contains_key(&row.key) {
            continue;
        }
        let key = data_key(row.key);
        for cell in &row.cells {
            let r = cell.bounds;
            if r.width < 4. || r.height < 5. {
                continue;
            }
            for (rule, scope) in source
                .presentation
                .format_rules
                .iter()
                .zip(&source.projection.formats)
            {
                if !rule.enabled
                    || (!rule.whole_row && source.fields[cell.column.0 as usize] != rule.field)
                {
                    continue;
                }
                let mut mark = Rect::new(r.x + 1., r.y + r.height - 4., r.width - 2., 3.);
                let mut color = Color::rgb(34, 125, 189);
                if let Some((lo, hi)) = &scope.extent {
                    let Some(value) = source
                        .table
                        .value(key, &rule.field)
                        .filter(|v| **v != CellValue::Null)
                    else {
                        continue;
                    };
                    let t = scaled_fraction(value, lo, hi);
                    match &rule.kind {
                        DataBar => {
                            let n = number(value).unwrap();
                            let a = number(lo).unwrap().min(0.);
                            let b = number(hi).unwrap().max(0.);
                            let zero = fraction(0., a, b);
                            let end = fraction(n, a, b);
                            mark.x += mark.width * zero.min(end);
                            mark.width = (mark.width * (end - zero).abs()).max(1.);
                            if n < 0. {
                                color = Color::rgb(206, 86, 71)
                            }
                        }
                        ColorScale => {
                            color = Color::rgb(
                                (210. - 175. * t) as u8,
                                (98. + 65. * t) as u8,
                                (72. + 70. * t) as u8,
                            );
                        }
                        Icons => {
                            let icon = if t < 0.333 {
                                IconSymbol::ChevronDown
                            } else if t > 0.667 {
                                IconSymbol::Check
                            } else {
                                IconSymbol::Remove
                            };
                            let size = 12f32.min(r.height).min(r.width);
                            crate::primitives::paint_icon(
                                &mut frame.scene,
                                icon,
                                Rect::new(r.x + r.width - size, r.y, size, size),
                                r,
                                color,
                            );
                            break;
                        }
                        _ => continue,
                    }
                } else if !scope.matches.contains(&key) {
                    continue;
                }
                frame.scene.fill(mark, color);
                break;
            }
        }
    }
}
impl Controls {
    pub fn set_grid_format_rules(
        &mut self,
        id: WidgetId,
        rules: Vec<GridFormatRule>,
    ) -> Result<(), ControlError> {
        let mut p = self
            .grid_presentation(id)
            .ok_or_else(|| error("Unknown GridControl"))?
            .clone();
        p.format_rules = rules;
        self.set_grid_presentation(id, p)
    }
}
