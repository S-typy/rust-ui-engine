//! Immutable bounded cross-tabulation over the shared typed query engine.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::{
    Aggregate, CellValue, Criteria, DataError, DataTable, QueryEngine, QueryExpression,
    QueryRequest,
};

pub const MAX_PIVOT_AXIS_FIELDS: usize = 2;
pub const MAX_PIVOT_MEASURES: usize = 8;
pub const MAX_PIVOT_PATHS: usize = 10_000;
pub const MAX_PIVOT_INTERSECTIONS: usize = 100_000;
pub const MAX_PIVOT_PATH_BYTES: usize = 4096;
pub const MAX_PIVOT_WORK: usize = 20_000_000;

/// Application-owned identity for one source. Clone it across source revisions;
/// create a new identity when replacing the source, even at the same revision.
#[derive(Clone, Debug)]
pub struct PivotSourceId(Arc<()>);
impl PivotSourceId {
    pub fn new() -> Self {
        Self(Arc::new(()))
    }
}
impl Default for PivotSourceId {
    fn default() -> Self {
        Self::new()
    }
}
impl PartialEq for PivotSourceId {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}
impl Eq for PivotSourceId {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PivotSourceVersion {
    pub source: PivotSourceId,
    pub revision: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PivotMeasure {
    pub title: String,
    pub aggregate: Aggregate,
}

/// Axis fields are unique across both axes. Empty axes represent a grand total.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PivotDefinition {
    pub row_fields: Vec<String>,
    pub column_fields: Vec<String>,
    pub measures: Vec<PivotMeasure>,
    pub filter: Option<Criteria>,
}

/// Stable typed identity, including axis and field names. Ord is an opaque byte
/// ordering, not display ordering; use the snapshot's preorder path slices.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PivotPathId(Arc<[u8]>);

#[derive(Clone, Debug)]
pub struct PivotPath {
    id: PivotPathId,
    parent: Option<PivotPathId>,
    fields: Vec<String>,
    values: Vec<CellValue>,
}
impl PivotPath {
    pub fn id(&self) -> &PivotPathId {
        &self.id
    }
    pub fn parent(&self) -> Option<&PivotPathId> {
        self.parent.as_ref()
    }
    pub fn fields(&self) -> &[String] {
        &self.fields
    }
    pub fn values(&self) -> &[CellValue] {
        &self.values
    }
    pub fn depth(&self) -> usize {
        self.values.len()
    }
}

/// Original signed row keys, in deterministic ascending order. The caller must
/// compare `source` with its live source before consuming rows or publishing UI.
#[derive(Clone, Debug)]
pub struct PivotDrillDown {
    pub source: PivotSourceVersion,
    pub row_keys: Vec<i64>,
}

#[derive(Debug)]
struct Intersection {
    values: Vec<CellValue>,
    row_keys: Vec<i64>,
}
#[derive(Debug)]
struct PivotData {
    source: PivotSourceVersion,
    definition: PivotDefinition,
    rows: Vec<PivotPath>,
    columns: Vec<PivotPath>,
    row_ids: HashSet<PivotPathId>,
    column_ids: HashSet<PivotPathId>,
    cells: HashMap<(PivotPathId, PivotPathId), Intersection>,
    empty: Vec<CellValue>,
    matched: usize,
}

/// Cheaply cloneable accepted result. A failed build never mutates an earlier
/// snapshot. Every subtotal is evaluated from original rows, including Average.
#[derive(Clone, Debug)]
pub struct PivotSnapshot(Arc<PivotData>);
impl PivotSnapshot {
    pub fn build(
        table: &DataTable,
        source: PivotSourceVersion,
        definition: &PivotDefinition,
    ) -> Result<Self, DataError> {
        validate_definition(table, definition)?;
        let mut budget = Budget {
            used: 0,
            max: table.limits().max_bytes,
        };
        let (_, definition_bytes) = definition_size(definition)?;
        budget.add(definition_bytes)?;
        let aggregates: Vec<_> = definition
            .measures
            .iter()
            .map(|m| m.aggregate.clone())
            .collect();
        let empty_table =
            DataTable::with_limits(table.schema().clone(), Vec::new(), table.limits())?;
        let empty = QueryEngine::execute(
            &empty_table,
            &QueryRequest {
                aggregates: aggregates.clone(),
                ..Default::default()
            },
        )?
        .aggregates;
        budget.values(&empty)?;
        let mut rows = HashMap::new();
        let mut columns = HashMap::new();
        let mut row_order = Vec::new();
        let mut column_order = Vec::new();
        let mut cells = HashMap::new();
        let mut matched = 0;
        let max_r = definition.row_fields.len();
        let max_c = definition.column_fields.len();
        // Deepest-only axis queries establish lexicographic/NULL-first preorder.
        // Their prefix paths are interned before any intersections are installed.
        for (axis, fields, paths, order) in [
            (0, &definition.row_fields, &mut rows, &mut row_order),
            (
                1,
                &definition.column_fields,
                &mut columns,
                &mut column_order,
            ),
        ] {
            let root = intern_path(axis, &[], &[], paths, &mut budget)?;
            order.push(root);
            if !fields.is_empty() {
                let result = QueryEngine::execute(
                    table,
                    &QueryRequest {
                        filter: definition.filter.clone(),
                        group_by: fields.iter().map(QueryExpression::field).collect(),
                        ..Default::default()
                    },
                )?;
                let mut seen = HashSet::new();
                for group in result.groups {
                    for depth in 1..=fields.len() {
                        let id = intern_path(
                            axis,
                            &fields[..depth],
                            &group.key[..depth],
                            paths,
                            &mut budget,
                        )?;
                        if seen.insert(id.clone()) {
                            order.push(id);
                        }
                    }
                }
            }
        }
        for r in 0..=max_r {
            for c in 0..=max_c {
                let group_by = definition.row_fields[..r]
                    .iter()
                    .chain(&definition.column_fields[..c])
                    .map(QueryExpression::field)
                    .collect();
                let result = QueryEngine::execute(
                    table,
                    &QueryRequest {
                        filter: definition.filter.clone(),
                        group_by,
                        aggregates: aggregates.clone(),
                        ..Default::default()
                    },
                )?;
                if r == 0 && c == 0 {
                    matched = result.row_keys.len();
                    insert_cell(
                        &mut cells,
                        row_order[0].clone(),
                        column_order[0].clone(),
                        result.aggregates,
                        result.row_keys,
                        &mut budget,
                    )?;
                } else {
                    for group in result.groups {
                        let row = path_id(0, &definition.row_fields[..r], &group.key[..r])?;
                        let col = path_id(1, &definition.column_fields[..c], &group.key[r..])?;
                        let row = rows
                            .get(&row)
                            .ok_or_else(|| invalid("unregistered row path"))?
                            .id
                            .clone();
                        let col = columns
                            .get(&col)
                            .ok_or_else(|| invalid("unregistered column path"))?
                            .id
                            .clone();
                        insert_cell(
                            &mut cells,
                            row,
                            col,
                            group.aggregates,
                            group.row_keys,
                            &mut budget,
                        )?;
                    }
                }
            }
        }
        let row_ids = rows.keys().cloned().collect();
        let column_ids = columns.keys().cloned().collect();
        let rows = row_order
            .into_iter()
            .map(|id| rows.remove(&id).expect("registered path"))
            .collect();
        let columns = column_order
            .into_iter()
            .map(|id| columns.remove(&id).expect("registered path"))
            .collect();
        Ok(Self(Arc::new(PivotData {
            source,
            definition: definition.clone(),
            rows,
            columns,
            row_ids,
            column_ids,
            cells,
            empty,
            matched,
        })))
    }
    pub fn source_version(&self) -> &PivotSourceVersion {
        &self.0.source
    }
    pub fn definition(&self) -> &PivotDefinition {
        &self.0.definition
    }
    /// Preorder: grand total, then typed lexicographic paths with NULL first.
    pub fn row_paths(&self) -> &[PivotPath] {
        &self.0.rows
    }
    pub fn column_paths(&self) -> &[PivotPath] {
        &self.0.columns
    }
    pub fn matched_row_count(&self) -> usize {
        self.0.matched
    }
    pub fn intersection_count(&self) -> usize {
        self.0.cells.len()
    }
    pub fn value(
        &self,
        row: &PivotPathId,
        column: &PivotPathId,
        measure: usize,
    ) -> Result<&CellValue, DataError> {
        self.check_paths(row, column)?;
        let values = self
            .0
            .cells
            .get(&(row.clone(), column.clone()))
            .map_or(&self.0.empty, |cell| &cell.values);
        values
            .get(measure)
            .ok_or_else(|| invalid("unknown measure"))
    }
    pub fn drill_down(
        &self,
        row: &PivotPathId,
        column: &PivotPathId,
        current: &PivotSourceVersion,
    ) -> Result<PivotDrillDown, DataError> {
        if current != &self.0.source {
            return Err(DataError::Conflict(
                "pivot source identity/revision changed".into(),
            ));
        }
        self.check_paths(row, column)?;
        let row_keys = self
            .0
            .cells
            .get(&(row.clone(), column.clone()))
            .map_or_else(Vec::new, |cell| cell.row_keys.clone());
        Ok(PivotDrillDown {
            source: self.0.source.clone(),
            row_keys,
        })
    }
    fn check_paths(&self, row: &PivotPathId, column: &PivotPathId) -> Result<(), DataError> {
        if !self.0.row_ids.contains(row) || !self.0.column_ids.contains(column) {
            return Err(invalid("unknown pivot path or wrong axis"));
        }
        Ok(())
    }
}

fn invalid(message: &str) -> DataError {
    DataError::Validation(format!("pivot: {message}"))
}
fn capacity(message: &str) -> DataError {
    DataError::Capacity(format!("pivot: {message}"))
}
struct Budget {
    used: usize,
    max: usize,
}
impl Budget {
    fn add(&mut self, bytes: usize) -> Result<(), DataError> {
        self.used = self
            .used
            .checked_add(bytes)
            .ok_or_else(|| capacity("byte overflow"))?;
        if self.used > self.max {
            return Err(capacity("snapshot logical byte limit"));
        }
        Ok(())
    }
    fn values(&mut self, values: &[CellValue]) -> Result<(), DataError> {
        for value in values {
            self.add(
                size_of::<CellValue>()
                    + match value {
                        CellValue::Text(s) => s.len(),
                        CellValue::Binary(b) => b.len(),
                        _ => 0,
                    },
            )?;
        }
        Ok(())
    }
}
fn insert_cell(
    cells: &mut HashMap<(PivotPathId, PivotPathId), Intersection>,
    row: PivotPathId,
    col: PivotPathId,
    values: Vec<CellValue>,
    keys: Vec<i64>,
    budget: &mut Budget,
) -> Result<(), DataError> {
    if cells.len() >= MAX_PIVOT_INTERSECTIONS {
        return Err(capacity("intersection limit"));
    }
    budget.add(
        size_of::<Intersection>() + 2 * size_of::<PivotPathId>() + keys.len() * size_of::<i64>(),
    )?;
    budget.values(&values)?;
    if cells
        .insert(
            (row, col),
            Intersection {
                values,
                row_keys: keys,
            },
        )
        .is_some()
    {
        return Err(invalid("duplicate intersection"));
    }
    Ok(())
}
fn intern_path(
    axis: u8,
    fields: &[String],
    values: &[CellValue],
    paths: &mut HashMap<PivotPathId, PivotPath>,
    budget: &mut Budget,
) -> Result<PivotPathId, DataError> {
    let id = path_id(axis, fields, values)?;
    if let Some(path) = paths.get(&id) {
        return Ok(path.id.clone());
    }
    if paths.len() >= MAX_PIVOT_PATHS {
        return Err(capacity("axis path limit"));
    }
    budget.add(
        size_of::<PivotPath>()
            + id.0.len()
            + fields
                .iter()
                .map(|f| f.len() + size_of::<String>())
                .sum::<usize>(),
    )?;
    budget.values(values)?;
    let parent = if values.is_empty() {
        None
    } else {
        let parent = path_id(
            axis,
            &fields[..fields.len() - 1],
            &values[..values.len() - 1],
        )?;
        Some(
            paths
                .get(&parent)
                .ok_or_else(|| invalid("missing parent path"))?
                .id
                .clone(),
        )
    };
    paths.insert(
        id.clone(),
        PivotPath {
            id: id.clone(),
            parent,
            fields: fields.to_vec(),
            values: values.to_vec(),
        },
    );
    Ok(id)
}

fn path_id(axis: u8, fields: &[String], values: &[CellValue]) -> Result<PivotPathId, DataError> {
    let mut bytes = vec![axis];
    fn part(out: &mut Vec<u8>, data: &[u8]) -> Result<(), DataError> {
        if out.len().saturating_add(4).saturating_add(data.len()) > MAX_PIVOT_PATH_BYTES {
            return Err(capacity("typed path byte limit"));
        }
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(data);
        Ok(())
    }
    for (field, value) in fields.iter().zip(values) {
        part(&mut bytes, field.as_bytes())?;
        let (tag, value): (u8, Vec<u8>) = match value {
            CellValue::Null => (0, vec![]),
            CellValue::Bool(v) => (1, vec![u8::from(*v)]),
            CellValue::Integer(v) => (2, v.to_le_bytes().to_vec()),
            CellValue::Real(v) => (
                3,
                (if *v == 0.0 { 0.0 } else { *v })
                    .to_bits()
                    .to_le_bytes()
                    .to_vec(),
            ),
            CellValue::Text(v) => {
                if v.len() > MAX_PIVOT_PATH_BYTES {
                    return Err(capacity("text path byte limit"));
                }
                (4, v.as_bytes().to_vec())
            }
            CellValue::Decimal(v) => (5, v.to_string().into_bytes()),
            CellValue::Date(v) => (6, v.to_string().into_bytes()),
            CellValue::Time(v) => (7, v.to_string().into_bytes()),
            CellValue::Binary(v) => {
                if v.len() > MAX_PIVOT_PATH_BYTES {
                    return Err(capacity("binary path byte limit"));
                }
                (8, v.clone())
            }
        };
        bytes.push(tag);
        part(&mut bytes, &value)?;
    }
    Ok(PivotPathId(bytes.into()))
}

fn validate_definition(table: &DataTable, definition: &PivotDefinition) -> Result<(), DataError> {
    if definition.row_fields.len() > MAX_PIVOT_AXIS_FIELDS
        || definition.column_fields.len() > MAX_PIVOT_AXIS_FIELDS
        || definition.measures.is_empty()
        || definition.measures.len() > MAX_PIVOT_MEASURES
    {
        return Err(capacity("requires 0..2 fields per axis and 1..8 measures"));
    }
    let mut names = HashSet::new();
    for name in definition
        .row_fields
        .iter()
        .chain(&definition.column_fields)
    {
        if name.is_empty() || name.len() > 256 || name.contains('\0') {
            return Err(invalid("axis field name requires 1..256 bytes without NUL"));
        }
        if table.schema().field(name).is_none() {
            return Err(DataError::UnknownField(name.clone()));
        }
        if !names.insert(name) {
            return Err(invalid("axis fields must be disjoint and unique"));
        }
    }
    if definition
        .measures
        .iter()
        .any(|m| m.title.is_empty() || m.title.len() > 256 || m.title.chars().any(char::is_control))
    {
        return Err(invalid(
            "measure title requires 1..256 noncontrol UTF-8 bytes",
        ));
    }
    // Bound the caller's AST by reference before any recursive Clone.
    let (nodes, definition_bytes) = definition_size(definition)?;
    if definition_bytes > table.limits().max_bytes {
        return Err(capacity("definition byte limit"));
    }
    let request = QueryRequest {
        filter: definition.filter.clone(),
        group_by: definition
            .row_fields
            .iter()
            .chain(&definition.column_fields)
            .map(QueryExpression::field)
            .collect(),
        aggregates: definition
            .measures
            .iter()
            .map(|m| m.aggregate.clone())
            .collect(),
        ..Default::default()
    };
    QueryEngine::compile(table.schema(), &request)?;
    let requests = (definition.row_fields.len() + 1) * (definition.column_fields.len() + 1)
        + usize::from(!definition.row_fields.is_empty())
        + usize::from(!definition.column_fields.is_empty());
    let work = table
        .rows()
        .len()
        .checked_mul(nodes + 1)
        .and_then(|n| n.checked_mul(requests))
        .ok_or_else(|| capacity("work overflow"))?;
    if work > MAX_PIVOT_WORK {
        return Err(capacity("query work estimate exceeds limit"));
    }
    Ok(())
}

fn definition_size(definition: &PivotDefinition) -> Result<(usize, usize), DataError> {
    enum Node<'a> {
        Expr(&'a QueryExpression),
        Filter(&'a Criteria),
    }
    let mut pending = Vec::new();
    if let Some(f) = &definition.filter {
        pending.push((Node::Filter(f), 1));
    }
    for m in &definition.measures {
        if let Some(e) = &m.aggregate.expression {
            pending.push((Node::Expr(e), 1));
        }
    }
    let mut count =
        definition.measures.len() + definition.row_fields.len() + definition.column_fields.len();
    let mut bytes = size_of::<PivotDefinition>()
        + definition
            .measures
            .iter()
            .map(|m| size_of::<PivotMeasure>() + m.title.len())
            .sum::<usize>()
        + definition
            .row_fields
            .iter()
            .chain(&definition.column_fields)
            .map(|f| size_of::<String>() + f.len())
            .sum::<usize>();
    let mut literals = 0usize;
    while let Some((node, depth)) = pending.pop() {
        count += 1;
        if count > crate::MAX_QUERY_NODES || depth > crate::MAX_QUERY_DEPTH {
            return Err(capacity("query node/depth limit"));
        }
        bytes = bytes
            .checked_add(size_of::<Criteria>() + size_of::<QueryExpression>())
            .ok_or_else(|| capacity("definition byte overflow"))?;
        match node {
            Node::Expr(
                QueryExpression::Arithmetic { left, right, .. }
                | QueryExpression::Coalesce(left, right),
            ) => {
                pending.push((Node::Expr(left), depth + 1));
                pending.push((Node::Expr(right), depth + 1));
            }
            Node::Expr(QueryExpression::Lowercase(e) | QueryExpression::TextLength(e)) => {
                pending.push((Node::Expr(e), depth + 1))
            }
            Node::Filter(
                Criteria::Compare { left, right, .. }
                | Criteria::TextMatch {
                    text: left,
                    pattern: right,
                    ..
                },
            ) => {
                pending.push((Node::Expr(left), depth + 1));
                pending.push((Node::Expr(right), depth + 1));
            }
            Node::Filter(Criteria::IsNull(e)) => pending.push((Node::Expr(e), depth + 1)),
            Node::Filter(Criteria::Not(f)) => pending.push((Node::Filter(f), depth + 1)),
            Node::Filter(Criteria::All(fs) | Criteria::Any(fs)) => {
                if fs.len() > crate::MAX_QUERY_TERMS {
                    return Err(capacity("query term limit"));
                }
                pending.extend(fs.iter().map(|f| (Node::Filter(f), depth + 1)));
            }
            Node::Expr(QueryExpression::Literal(value)) => {
                let n = match value {
                    CellValue::Text(s) => s.len(),
                    CellValue::Binary(b) => b.len(),
                    _ => 0,
                };
                literals = literals
                    .checked_add(n)
                    .ok_or_else(|| capacity("literal byte overflow"))?;
                if literals > crate::MAX_QUERY_LITERAL_BYTES {
                    return Err(capacity("literal byte limit"));
                }
                bytes = bytes
                    .checked_add(n)
                    .ok_or_else(|| capacity("definition byte overflow"))?;
            }
            Node::Expr(QueryExpression::Field(f)) => {
                bytes = bytes
                    .checked_add(f.len())
                    .ok_or_else(|| capacity("definition byte overflow"))?;
            }
            _ => {}
        }
    }
    Ok((count, bytes))
}

#[cfg(test)]
mod tests;
