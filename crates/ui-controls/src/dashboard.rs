//! One definition for dashboard viewing/editing, with transactional shared filtering.
//! Data adapters publish immutable snapshots; widgets never perform provider I/O.
use crate::definition_codec::*;
use rust_desktop_ui_data::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
mod view;
pub use view::*;
pub type DashboardSources = BTreeMap<String, DataTable>;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", deny_unknown_fields)]
pub enum DashboardWidget {
    Grid,
    Chart {
        x: String,
        y: String,
    },
    Metric {
        aggregate: usize,
    },
    /// Unknown plugins are retained verbatim and shown as a diagnostic tile.
    Missing {
        name: String,
        payload: serde_json::Value,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DashboardPlacement {
    pub row: usize,
    pub column: usize,
    pub row_span: usize,
    pub column_span: usize,
}
#[derive(Clone, Debug)]
pub struct DashboardItem {
    pub id: String,
    pub title: String,
    pub source: String,
    pub widget: DashboardWidget,
    pub placement: DashboardPlacement,
    pub query: QueryRequest,
}
#[derive(Clone, Debug)]
pub struct DashboardParameter {
    pub name: String,
    pub source: String,
    pub field: String,
    pub value: CellValue,
}
#[derive(Clone, Debug)]
pub struct DashboardDefinition {
    pub title: String,
    pub columns: usize,
    pub items: Vec<DashboardItem>,
    pub parameters: Vec<DashboardParameter>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct DashboardFilter {
    pub field: String,
    pub values: Vec<CellValue>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DashboardRevision(u64);
#[derive(Clone, Debug)]
pub struct DashboardResult {
    pub rows: Vec<i64>,
    pub totals: Vec<CellValue>,
    pub table: DataTable,
    pub diagnostic: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DashboardError(pub String);
impl std::fmt::Display for DashboardError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for DashboardError {}
fn fail(s: impl ToString) -> DashboardError {
    DashboardError(s.to_string())
}
fn name(s: &str) -> Result<(), DashboardError> {
    if s.is_empty() || s.len() > 256 || s.chars().any(char::is_control) {
        Err(fail("Invalid dashboard name"))
    } else {
        Ok(())
    }
}
fn combine(target: &mut Option<Criteria>, next: Criteria) {
    *target = Some(match target.take() {
        Some(old) => Criteria::All(vec![old, next]),
        None => next,
    });
}
fn equal(field: &str, value: CellValue) -> Criteria {
    if value == CellValue::Null {
        Criteria::IsNull(QueryExpression::field(field))
    } else {
        Criteria::Compare {
            left: QueryExpression::field(field),
            operator: ComparisonOperator::Equal,
            right: QueryExpression::Literal(value),
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    version: u32,
    title: String,
    columns: usize,
    items: Vec<ItemDto>,
    parameters: Vec<(String, String, String, String)>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ItemDto {
    id: String,
    title: String,
    source: String,
    widget: DashboardWidget,
    placement: DashboardPlacement,
    query: QueryDto,
}
impl DashboardDefinition {
    pub fn to_json(&self) -> Result<String, DashboardError> {
        self.validate_structure()?;
        let f = File {
            version: 1,
            title: self.title.clone(),
            columns: self.columns,
            items: self
                .items
                .iter()
                .map(|i| {
                    Ok(ItemDto {
                        id: i.id.clone(),
                        title: i.title.clone(),
                        source: i.source.clone(),
                        widget: i.widget.clone(),
                        placement: i.placement,
                        query: QueryDto::encode(&i.query).map_err(fail)?,
                    })
                })
                .collect::<Result<_, DashboardError>>()?,
            parameters: self
                .parameters
                .iter()
                .map(|p| {
                    Ok((
                        p.name.clone(),
                        p.source.clone(),
                        p.field.clone(),
                        literal_encode(&p.value).map_err(fail)?,
                    ))
                })
                .collect::<Result<_, DashboardError>>()?,
        };
        let json = serde_json::to_string_pretty(&f).map_err(fail)?;
        if json.len() > 1024 * 1024 {
            return Err(fail("Dashboard exceeds 1 MiB"));
        }
        Ok(json)
    }
    pub fn from_json(json: &str, sources: &DashboardSources) -> Result<Self, DashboardError> {
        if json.len() > 1024 * 1024 {
            return Err(fail("Dashboard exceeds 1 MiB"));
        }
        let f: File = serde_json::from_str(json).map_err(fail)?;
        if f.version != 1 || f.items.len() > 32 || f.parameters.len() > 32 {
            return Err(fail("Unsupported dashboard version/limits"));
        }
        let d = Self {
            title: f.title,
            columns: f.columns,
            items: f
                .items
                .into_iter()
                .map(|i| {
                    let source = sources
                        .get(&i.source)
                        .ok_or_else(|| fail("Missing data source"))?;
                    Ok(DashboardItem {
                        id: i.id,
                        title: i.title,
                        source: i.source,
                        widget: i.widget,
                        placement: i.placement,
                        query: i.query.decode(source.schema()).map_err(fail)?,
                    })
                })
                .collect::<Result<_, DashboardError>>()?,
            parameters: f
                .parameters
                .into_iter()
                .map(|(name, source, field, value)| {
                    let table = sources
                        .get(&source)
                        .ok_or_else(|| fail("Missing parameter source"))?;
                    Ok(DashboardParameter {
                        name,
                        field,
                        value: literal_decode(&value, table.schema()).map_err(fail)?,
                        source,
                    })
                })
                .collect::<Result<_, DashboardError>>()?,
        };
        d.validate_structure()?;
        Ok(d)
    }
    fn validate_structure(&self) -> Result<(), DashboardError> {
        name(&self.title)?;
        if !(1..=12).contains(&self.columns) || self.items.len() > 32 || self.parameters.len() > 32
        {
            return Err(fail("Dashboard capacity exceeded"));
        }
        let mut ids = BTreeSet::new();
        let mut cells = BTreeSet::new();
        for item in &self.items {
            name(&item.id)?;
            name(&item.title)?;
            name(&item.source)?;
            if !ids.insert(&item.id) {
                return Err(fail("Duplicate widget identity"));
            }
            let p = item.placement;
            if p.row >= 64
                || p.column >= self.columns
                || p.row_span == 0
                || p.column_span == 0
                || p.row_span > 64 - p.row
                || p.column_span > self.columns - p.column
            {
                return Err(fail("Invalid dashboard placement"));
            }
            for row in p.row..p.row + p.row_span {
                for col in p.column..p.column + p.column_span {
                    if !cells.insert((row, col)) {
                        return Err(fail("Overlapping dashboard items"));
                    }
                }
            }
            if let DashboardWidget::Missing { name: n, payload } = &item.widget {
                name(n)?;
                if serde_json::to_vec(payload).map_err(fail)?.len() > 65536 {
                    return Err(fail("Plugin payload exceeds 64 KiB"));
                }
            }
        }
        let mut parameters = BTreeSet::new();
        for p in &self.parameters {
            name(&p.name)?;
            name(&p.source)?;
            name(&p.field)?;
            if !parameters.insert(&p.name) {
                return Err(fail("Duplicate parameter"));
            }
        }
        Ok(())
    }
}
pub struct DashboardSession {
    definition: DashboardDefinition,
    sources: DashboardSources,
    filters: BTreeMap<String, DashboardFilter>,
    results: BTreeMap<String, DashboardResult>,
    revision: u64,
    undo: VecDeque<String>,
    redo: VecDeque<String>,
}
impl DashboardSession {
    pub fn new(
        definition: DashboardDefinition,
        sources: DashboardSources,
    ) -> Result<Self, DashboardError> {
        definition.to_json()?;
        let results = evaluate(&definition, &sources, &BTreeMap::new())?;
        Ok(Self {
            definition,
            sources,
            filters: BTreeMap::new(),
            results,
            revision: 0,
            undo: VecDeque::new(),
            redo: VecDeque::new(),
        })
    }
    pub fn definition(&self) -> &DashboardDefinition {
        &self.definition
    }
    pub fn sources(&self) -> &DashboardSources {
        &self.sources
    }
    pub fn results(&self) -> &BTreeMap<String, DashboardResult> {
        &self.results
    }
    pub fn filters(&self) -> &BTreeMap<String, DashboardFilter> {
        &self.filters
    }
    pub fn revision(&self) -> DashboardRevision {
        DashboardRevision(self.revision)
    }
    fn next(&self) -> Result<u64, DashboardError> {
        self.revision
            .checked_add(1)
            .ok_or_else(|| fail("Revision exhausted"))
    }
    /// Only explicit user selections call this method. Repainting does not emit filters.
    /// A widget's own filter is excluded from its query; same-source peers receive it.
    pub fn set_filter(
        &mut self,
        expected: DashboardRevision,
        origin: &str,
        filter: Option<DashboardFilter>,
    ) -> Result<bool, DashboardError> {
        if expected != self.revision() {
            return Err(fail("Stale filter event"));
        }
        if !self.definition.items.iter().any(|i| i.id == origin) {
            return Err(fail("Unknown filter origin"));
        }
        if self.filters.get(origin) == filter.as_ref() {
            return Ok(false);
        }
        let mut filters = self.filters.clone();
        if let Some(f) = filter {
            filters.insert(origin.into(), f);
        } else {
            filters.remove(origin);
        }
        let results = evaluate(&self.definition, &self.sources, &filters)?;
        let revision = self.next()?;
        self.filters = filters;
        self.results = results;
        self.revision = revision;
        Ok(true)
    }
    pub fn clear_filters(&mut self) -> Result<bool, DashboardError> {
        if self.filters.is_empty() {
            return Ok(false);
        }
        let results = evaluate(&self.definition, &self.sources, &BTreeMap::new())?;
        let revision = self.next()?;
        self.filters.clear();
        self.results = results;
        self.revision = revision;
        Ok(true)
    }
    pub fn replace(
        &mut self,
        expected: DashboardRevision,
        definition: DashboardDefinition,
    ) -> Result<(), DashboardError> {
        if expected != self.revision() {
            return Err(fail("Stale dashboard edit"));
        }
        definition.to_json()?;
        // Removing a widget removes its outgoing selection, never another widget's.
        let filters = self
            .filters
            .iter()
            .filter(|(id, _)| definition.items.iter().any(|i| &i.id == *id))
            .map(|(a, b)| (a.clone(), b.clone()))
            .collect();
        let results = evaluate(&definition, &self.sources, &filters)?;
        let revision = self.next()?;
        self.undo.push_back(self.definition.to_json()?);
        trim(&mut self.undo);
        self.redo.clear();
        self.definition = definition;
        self.results = results;
        self.filters = filters;
        self.revision = revision;
        Ok(())
    }
    pub fn set_parameter(&mut self, name: &str, value: CellValue) -> Result<(), DashboardError> {
        let mut d = self.definition.clone();
        d.parameters
            .iter_mut()
            .find(|p| p.name == name)
            .ok_or_else(|| fail("Unknown parameter"))?
            .value = value;
        self.replace(self.revision(), d)
    }
    pub fn refresh_sources(
        &mut self,
        expected: DashboardRevision,
        sources: DashboardSources,
    ) -> Result<(), DashboardError> {
        if expected != self.revision() {
            return Err(fail("Stale data snapshot"));
        }
        let results = evaluate(&self.definition, &sources, &self.filters)?;
        let revision = self.next()?;
        self.sources = sources;
        self.results = results;
        self.revision = revision;
        self.undo.clear();
        self.redo.clear();
        Ok(())
    }
    pub fn save(&self) -> Result<String, DashboardError> {
        self.definition.to_json()
    }
    pub fn restore(
        &mut self,
        expected: DashboardRevision,
        json: &str,
    ) -> Result<(), DashboardError> {
        let definition = DashboardDefinition::from_json(json, &self.sources)?;
        self.replace(expected, definition)
    }
    pub fn undo(&mut self) -> Result<bool, DashboardError> {
        self.history(false)
    }
    pub fn redo(&mut self) -> Result<bool, DashboardError> {
        self.history(true)
    }
    fn history(&mut self, redo: bool) -> Result<bool, DashboardError> {
        let stack = if redo { &self.redo } else { &self.undo };
        let Some(json) = stack.back() else {
            return Ok(false);
        };
        let definition = DashboardDefinition::from_json(json, &self.sources)?;
        // Historical definitions are restored without transient outgoing selections.
        let results = evaluate(&definition, &self.sources, &BTreeMap::new())?;
        let revision = self.next()?;
        let old = self.definition.to_json()?;
        if redo {
            self.redo.pop_back();
            self.undo.push_back(old);
            trim(&mut self.undo)
        } else {
            self.undo.pop_back();
            self.redo.push_back(old);
            trim(&mut self.redo)
        }
        self.definition = definition;
        self.results = results;
        self.filters.clear();
        self.revision = revision;
        Ok(true)
    }
}
fn trim(stack: &mut VecDeque<String>) {
    while stack.len() > 64 || stack.iter().map(String::len).sum::<usize>() > 8 * 1024 * 1024 {
        stack.pop_front();
    }
}
fn evaluate(
    d: &DashboardDefinition,
    sources: &DashboardSources,
    filters: &BTreeMap<String, DashboardFilter>,
) -> Result<BTreeMap<String, DashboardResult>, DashboardError> {
    d.validate_structure()?;
    if sources.len() > 32 || sources.values().any(|s| s.rows().len() > 20_000) {
        return Err(fail("Dashboard source budget exceeded"));
    }
    for p in &d.parameters {
        let source = sources
            .get(&p.source)
            .ok_or_else(|| fail("Unknown parameter source"))?;
        source
            .schema()
            .field(&p.field)
            .ok_or_else(|| fail("Unknown parameter field"))?
            .validate(&p.value)
            .map_err(fail)?;
    }
    for (id, f) in filters {
        let item = d
            .items
            .iter()
            .find(|i| &i.id == id)
            .ok_or_else(|| fail("Unknown filter origin"))?;
        if f.values.len() > 128 {
            return Err(fail("Filter value limit exceeded"));
        }
        let source = sources
            .get(&item.source)
            .ok_or_else(|| fail("Unknown filter source"))?;
        let field = source
            .schema()
            .field(&f.field)
            .ok_or_else(|| fail("Unknown filter field"))?;
        for value in &f.values {
            field.validate(value).map_err(fail)?;
        }
    }
    let mut results = BTreeMap::new();
    for i in &d.items {
        let source = sources
            .get(&i.source)
            .ok_or_else(|| fail("Missing data source"))?;
        let mut query = i.query.clone();
        for p in d.parameters.iter().filter(|p| p.source == i.source) {
            combine(&mut query.filter, equal(&p.field, p.value.clone()));
        }
        for (origin, filter) in filters {
            if origin == &i.id
                || d.items
                    .iter()
                    .find(|v| &v.id == origin)
                    .is_none_or(|v| v.source != i.source)
            {
                continue;
            }
            combine(
                &mut query.filter,
                Criteria::Any(
                    filter
                        .values
                        .iter()
                        .map(|v| equal(&filter.field, v.clone()))
                        .collect(),
                ),
            );
        }
        let result = QueryEngine::execute(source, &query).map_err(fail)?;
        let rows = result
            .row_keys
            .iter()
            .map(|key| source.row(*key).unwrap().clone())
            .collect();
        let table = DataTable::new(source.schema().clone(), rows).map_err(fail)?;
        let diagnostic = match &i.widget {
            DashboardWidget::Chart { x, y } => {
                crate::ChartSeries::from_table(&table, 1, &i.title, x, y).map_err(fail)?;
                None
            }
            DashboardWidget::Metric { aggregate } => {
                if *aggregate >= result.aggregates.len() {
                    return Err(fail("Missing metric aggregate"));
                }
                None
            }
            DashboardWidget::Missing { name, .. } => Some(format!("Widget unavailable: {name}")),
            DashboardWidget::Grid => None,
        };
        results.insert(
            i.id.clone(),
            DashboardResult {
                rows: result.row_keys,
                totals: result.aggregates,
                table,
                diagnostic,
            },
        );
    }
    Ok(results)
}
