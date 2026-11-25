//! Collision-free presentation rows over accepted master/detail snapshots.
use super::*;
use rust_desktop_ui_data::{CellValue, QueryEngine};

const MAX_PROJECTED_ROWS: usize = 1_000_000;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) enum SyntheticIdentity {
    Group(Vec<String>),
    Branch(Vec<String>, Vec<String>),
    Footer(Vec<String>, Vec<String>),
    DetailHeader(i64),
    Detail(i64, i64),
}
#[derive(Clone)]
pub(super) struct SyntheticRow {
    pub identity: SyntheticIdentity,
    pub label: String,
}
#[derive(Clone, Default)]
pub(super) struct Projection {
    pub roots: Vec<RowKey>,
    pub(super) formats: Vec<appearance::FormatScope>,
    pub children: HashMap<RowKey, Vec<RowKey>>,
    pub parents: HashMap<RowKey, RowKey>,
    pub positions: HashMap<RowKey, usize>,
    pub synthetic: HashMap<RowKey, SyntheticRow>,
    pub identities: HashMap<SyntheticIdentity, RowKey>,
    pub groups: Vec<RowKey>,
    pub totals: Vec<CellValue>,
    pub detail_table: Option<DataTable>,
    pub detail_revision: Option<u64>,
}

impl Projection {
    pub fn build(source: &SnapshotSource, previous: &Self) -> Result<Self, ControlError> {
        let mut result = Self::build_rows(source, previous)?;
        result.formats = appearance::compile(source)?;
        Ok(result)
    }
    fn build_rows(source: &SnapshotSource, previous: &Self) -> Result<Self, ControlError> {
        let options = &source.presentation;
        let mut next = Self::default();
        let mut occupied: HashSet<_> = source.table.rows().iter().map(|r| row_key(r.key)).collect();
        // A new group/detail must not steal a still-live previous identity just
        // because query ordering places the new row before its former owner.
        let reserved: HashSet<_> = previous.identities.values().copied().collect();
        let mut cursor = 0_u64;
        let mut allocate =
            |identity: SyntheticIdentity, label: String| -> Result<RowKey, ControlError> {
                if let Some(key) = next.identities.get(&identity) {
                    return Ok(*key);
                }
                let key = if let Some(key) = previous
                    .identities
                    .get(&identity)
                    .filter(|key| !occupied.contains(key))
                {
                    *key
                } else {
                    loop {
                        let candidate = RowKey(cursor);
                        cursor = cursor.checked_add(1).ok_or_else(|| {
                            error("Grid presentation identity capacity exhausted")
                        })?;
                        if !occupied.contains(&candidate) && !reserved.contains(&candidate) {
                            break candidate;
                        }
                    }
                };
                if source.keys.len() + next.synthetic.len() >= MAX_PROJECTED_ROWS {
                    return Err(error(
                        "Grid presentation exceeds one million master/detail/group rows",
                    ));
                }
                occupied.insert(key);
                next.identities.insert(identity.clone(), key);
                next.synthetic.insert(key, SyntheticRow { identity, label });
                Ok(key)
            };
        if source.parent_field.is_some() {
            return source.tree_projection();
        }
        if options.group_fields.is_empty()
            && options.summaries.is_empty()
            && options.custom_summaries.is_empty()
        {
            next.roots = source.keys.iter().copied().map(row_key).collect();
        } else {
            let mut visible = DataTable::with_limits(
                source.table.schema().clone(),
                source
                    .keys
                    .iter()
                    .filter_map(|key| source.table.row(*key).cloned())
                    .collect(),
                source.table.limits(),
            )
            .map_err(error)?;
            let mut query = options.query(&source.data_query);
            let mut group_fields = std::mem::take(&mut query.group_by);
            for (field, rule) in &options.group_expressions {
                let parsed = if rule == "@display" {
                    let Some(index) = options.group_fields.iter().position(|f| f == field) else {
                        continue;
                    };
                    let mut alias = format!("_grid_display_{index}");
                    while visible.schema().field(&alias).is_some() {
                        alias.push('_');
                    }
                    let mut fields = visible.schema().fields().to_vec();
                    let mut schema = rust_desktop_ui_data::FieldSchema::new(
                        &alias,
                        rust_desktop_ui_data::FieldType::Text,
                    );
                    schema.nullable = true;
                    fields.push(schema);
                    let mut rows = visible.rows().to_vec();
                    for row in &mut rows {
                        let value = visible.value(row.key, field).expect("known group field");
                        row.values.push(if *value == CellValue::Null {
                            CellValue::Null
                        } else {
                            CellValue::Text(source.cell_label(field, value))
                        });
                    }
                    visible = DataTable::with_limits(
                        rust_desktop_ui_data::TableSchema::new(fields).map_err(error)?,
                        rows,
                        visible.limits(),
                    )
                    .map_err(error)?;
                    rust_desktop_ui_data::QueryExpression::field(alias)
                } else {
                    rust_desktop_ui_data::parse_expression_text(source.table.schema(), rule)
                        .map_err(error)?
                };
                if let Some(index) = options.group_fields.iter().position(|f| f == field) {
                    group_fields[index] = parsed;
                }
            }
            let levels = group_fields.len();
            let first = if options.hierarchical_groups {
                levels.min(1)
            } else {
                levels
            };
            let mut parents = HashMap::new();
            let mut footers = Vec::new();
            for depth in first..=levels {
                query.group_by = group_fields[..depth].to_vec();
                let mut result = QueryEngine::execute(&visible, &query).map_err(error)?;
                for summary in &options.custom_summaries {
                    result
                        .aggregates
                        .push(summary.evaluate(&visible, &result.row_keys)?);
                    for group in &mut result.groups {
                        group
                            .aggregates
                            .push(summary.evaluate(&visible, &group.row_keys)?);
                    }
                }
                if let Some(index) = options.group_sort_summary {
                    result
                        .sort_groups_by_summary(index, options.group_descending)
                        .map_err(error)?;
                } else if options.group_fields[..depth]
                    .iter()
                    .any(|field| source.group_descending(field))
                {
                    let descending: Vec<_> = options.group_fields[..depth]
                        .iter()
                        .map(|f| source.group_descending(f))
                        .collect();
                    result.groups.sort_by(|a, b| {
                        a.key
                            .iter()
                            .zip(&b.key)
                            .zip(&descending)
                            .map(|((left, right), desc)| {
                                let cmp = rust_desktop_ui_data::compare_values(left, right);
                                if *desc { cmp.reverse() } else { cmp }
                            })
                            .find(|cmp| !cmp.is_eq())
                            .unwrap_or(std::cmp::Ordering::Equal)
                    });
                }
                if depth == first {
                    next.totals = result.aggregates;
                }
                if depth == 0 {
                    next.roots = source.keys.iter().copied().map(row_key).collect();
                    break;
                }
                let mut current = HashMap::new();
                for mut group in result.groups {
                    group.row_keys.sort_by_key(|key| source.positions[key]);
                    if next.groups.len() >= 4096 {
                        return Err(error("Grid presentation supports at most 4096 groups"));
                    }
                    let path: Vec<_> = group.key.iter().map(value_identity).collect();
                    let label_start = if options.hierarchical_groups {
                        depth - 1
                    } else {
                        0
                    };
                    let mut label = options.group_fields[label_start..depth]
                        .iter()
                        .zip(&group.key[label_start..])
                        .map(|(name, value)| {
                            format!(
                                "{}: {}",
                                source
                                    .table
                                    .schema()
                                    .field(name)
                                    .map_or(name.as_str(), |f| f.title.as_str()),
                                if *value == CellValue::Null {
                                    "(NULL)".to_owned()
                                } else if options.group_expressions.iter().any(|(f, _)| f == name) {
                                    value.display()
                                } else {
                                    source.group_key_label(name, value)
                                }
                            )
                        })
                        .collect::<Vec<_>>()
                        .join(" · ");
                    label.push_str(&format!(" ({})", group.row_keys.len()));
                    let summary_text = options
                        .summary_titles()
                        .zip(&group.aggregates)
                        .map(|(title, v)| format!("{title}: {}", v.display()))
                        .collect::<Vec<_>>()
                        .join(" · ");
                    if !options.group_footer && !summary_text.is_empty() {
                        label.push_str(" · ");
                        label.push_str(&summary_text);
                    }
                    if summary_text.len() > 16_384 {
                        return Err(error("Group footer exceeds 16 KiB"));
                    }
                    if label.len() > 16_384 {
                        return Err(error("Group caption exceeds 16 KiB"));
                    }
                    let identity = if options.hierarchical_groups {
                        SyntheticIdentity::Branch(
                            options.group_fields[..depth].to_vec(),
                            path.clone(),
                        )
                    } else {
                        SyntheticIdentity::Group(path.clone())
                    };
                    let key = allocate(identity, label)?;
                    if options.group_footer && !summary_text.is_empty() {
                        let footer = allocate(
                            SyntheticIdentity::Footer(
                                options.group_fields[..depth].to_vec(),
                                path.clone(),
                            ),
                            summary_text,
                        )?;
                        footers.push((key, footer));
                    }
                    if depth == first {
                        next.roots.push(key);
                    } else {
                        let parent = parents
                            .get(&path[..depth - 1])
                            .ok_or_else(|| error("Missing parent group"))?;
                        next.children.entry(*parent).or_default().push(key);
                    }
                    next.groups.push(key);
                    if depth == levels {
                        next.children
                            .insert(key, group.row_keys.into_iter().map(row_key).collect());
                    } else {
                        current.insert(path, key);
                    }
                }
                parents = current;
            }
            for (key, footer) in footers {
                next.children.entry(key).or_default().push(footer);
            }
        }
        if let Some(detail) = &source.detail {
            if Rc::ptr_eq(&source.handle, &detail.source) {
                return Err(error(
                    "Grid detail source must be distinct from the master source",
                ));
            }
            let data = detail
                .source
                .try_borrow()
                .map_err(|_| error("Grid detail source is already borrowed"))?;
            let master = source
                .table
                .schema()
                .field(&detail.master_field)
                .ok_or_else(|| error("Unknown master relation field"))?;
            let foreign = data
                .table()
                .schema()
                .field(&detail.foreign_field)
                .ok_or_else(|| error("Unknown detail relation field"))?;
            if master.kind != foreign.kind {
                return Err(error("Master/detail relation types must match"));
            }
            let mut seen = HashSet::new();
            if detail.columns.is_empty() || detail.columns.len() > source.fields.len() {
                return Err(error(
                    "Detail columns require one to the master column count",
                ));
            }
            for column in &detail.columns {
                if !seen.insert(&column.field)
                    || data.table().schema().field(&column.field).is_none()
                    || column.title.len() > 4096
                    || column.title.chars().any(char::is_control)
                {
                    return Err(error("Invalid or duplicate detail column"));
                }
            }
            let mut index: HashMap<String, Vec<i64>> = HashMap::new();
            for i in 0..data.len() {
                let child = data.row_key(i).expect("accepted visible row");
                let value = data
                    .table()
                    .value(child, &detail.foreign_field)
                    .expect("validated field");
                if *value != CellValue::Null {
                    index.entry(value_identity(value)).or_default().push(child);
                }
            }
            for master in &source.keys {
                let value = source
                    .table
                    .value(*master, &detail.master_field)
                    .expect("validated field");
                if *value == CellValue::Null {
                    continue;
                }
                let Some(children) = index.get(&value_identity(value)) else {
                    continue;
                };
                let header = allocate(SyntheticIdentity::DetailHeader(*master), "Details".into())?;
                let mut rows = vec![header];
                for child in children {
                    rows.push(allocate(
                        SyntheticIdentity::Detail(*master, *child),
                        String::new(),
                    )?);
                }
                next.children.insert(row_key(*master), rows);
            }
            next.detail_revision = Some(data.revision());
            next.detail_table = Some(data.table().clone());
        }
        for (index, key) in next.roots.iter().enumerate() {
            next.positions.insert(*key, index);
        }
        for (parent, children) in &next.children {
            for (index, key) in children.iter().enumerate() {
                next.parents.insert(*key, *parent);
                next.positions.insert(*key, index);
            }
        }
        Ok(next)
    }
}

/// Typed equality identity; query normalization makes equivalent decimals equal.
pub(super) fn value_identity(value: &CellValue) -> String {
    match value {
        CellValue::Real(v) if *v == 0.0 => "Real(0)".into(),
        CellValue::Decimal(v) => format!("Decimal({v})"),
        _ => format!("{value:?}"),
    }
}

impl SnapshotSource {
    pub(super) fn apply_data_query(&mut self, query: DataQuery) -> Result<(), ControlError> {
        if let Some(pending) = &self.page_query {
            self.data_query = query.clone();
            *pending.borrow_mut() = Some(query);
            return Ok(());
        }
        // Evaluate the candidate legacy query and presentation without touching
        // the shared source. Aggregate/type failures therefore cannot publish a
        // half-applied query to sibling views.
        if self.relation.is_some() || !self.formulas.is_empty() || !self.unbound.is_empty() {
            let mut next = self.clone();
            next.data_query = query;
            next.reload()?;
            *self = next;
            return Ok(());
        }
        let data = self
            .handle
            .try_borrow()
            .map_err(|_| error("Grid source is already borrowed"))?;
        let mut preview = DataSource::new(rust_desktop_ui_data::CollectionAdapter::new(
            data.table().clone(),
        ))
        .map_err(error)?;
        preview.set_query(query.clone()).map_err(error)?;
        drop(data);
        let mut next = self.clone();
        next.table = preview.table().clone();
        next.data_query = query.clone();
        next.keys = next.filtered_keys(None)?;
        next.positions = next
            .keys
            .iter()
            .enumerate()
            .map(|(i, key)| (*key, i))
            .collect();
        next.projection = Projection::build(&next, &self.projection)?;
        {
            let mut data = self
                .handle
                .try_borrow_mut()
                .map_err(|_| error("Grid source is already borrowed"))?;
            data.set_query(query).map_err(error)?;
            next.revision = data.revision();
        }
        *self = next;
        Ok(())
    }
    pub(super) fn row_kind(&self, row: RowKey) -> Option<GridRowKind> {
        if let Some(value) = self.projection.synthetic.get(&row) {
            return Some(match &value.identity {
                SyntheticIdentity::Group(key) | SyntheticIdentity::Branch(_, key) => {
                    GridRowKind::Group { key: key.clone() }
                }
                SyntheticIdentity::Footer(_, key) => GridRowKind::GroupFooter { key: key.clone() },
                SyntheticIdentity::DetailHeader(master) => {
                    GridRowKind::DetailHeader { master: *master }
                }
                SyntheticIdentity::Detail(master, key) => GridRowKind::Detail {
                    master: *master,
                    key: *key,
                },
            });
        }
        self.table
            .row(data_key(row))
            .map(|_| GridRowKind::Record(data_key(row)))
    }
    pub(super) fn detail_changed(&self) -> Result<bool, ControlError> {
        self.detail
            .as_ref()
            .map(|detail| {
                detail
                    .source
                    .try_borrow()
                    .map(|s| Some(s.revision()) != self.projection.detail_revision)
                    .map_err(|_| error("Grid detail source is already borrowed"))
            })
            .unwrap_or(Ok(false))
    }
    pub(super) fn projected_cell(&self, row: RowKey, column: ColumnId) -> Option<String> {
        let synthetic = self.projection.synthetic.get(&row)?;
        Some(match &synthetic.identity {
            SyntheticIdentity::Group(_)
            | SyntheticIdentity::Branch(_, _)
            | SyntheticIdentity::Footer(_, _) => {
                if column == ColumnId(0) {
                    synthetic.label.clone()
                } else {
                    String::new()
                }
            }
            SyntheticIdentity::DetailHeader(_) => self
                .detail
                .as_ref()
                .and_then(|d| d.columns.get(column.0 as usize))
                .map(|c| c.title.clone())
                .unwrap_or_default(),
            SyntheticIdentity::Detail(_, child) => self
                .detail
                .as_ref()
                .and_then(|d| d.columns.get(column.0 as usize))
                .and_then(|c| {
                    self.projection
                        .detail_table
                        .as_ref()?
                        .value(*child, &c.field)
                })
                .map(CellValue::display)
                .unwrap_or_default(),
        })
    }
}
