//! Collision-free presentation rows over accepted master/detail snapshots.
use super::*;
use rust_desktop_ui_data::{CellValue, QueryEngine};

const MAX_PROJECTED_ROWS: usize = 1_000_000;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) enum SyntheticIdentity {
    Group(Vec<String>),
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
        if options.group_fields.is_empty() && options.summaries.is_empty() {
            next.roots = source.keys.iter().copied().map(row_key).collect();
        } else {
            let visible = DataTable::with_limits(
                source.table.schema().clone(),
                source
                    .keys
                    .iter()
                    .filter_map(|key| source.table.row(*key).cloned())
                    .collect(),
                source.table.limits(),
            )
            .map_err(error)?;
            let result = QueryEngine::execute(&visible, &options.query(&source.data_query))
                .map_err(error)?;
            next.totals = result.aggregates;
            if options.group_fields.is_empty() {
                next.roots = result.row_keys.into_iter().map(row_key).collect();
            } else {
                for group in result.groups {
                    if next.groups.len() >= 4096 {
                        return Err(error("Grid presentation supports at most 4096 groups"));
                    }
                    let identity =
                        SyntheticIdentity::Group(group.key.iter().map(value_identity).collect());
                    let mut label = options
                        .group_fields
                        .iter()
                        .zip(&group.key)
                        .map(|(name, value)| {
                            format!(
                                "{}: {}",
                                source
                                    .table
                                    .schema()
                                    .field(name)
                                    .map_or(name.as_str(), |field| field.title.as_str()),
                                value.display()
                            )
                        })
                        .collect::<Vec<_>>()
                        .join(" · ");
                    label.push_str(&format!(" ({})", group.row_keys.len()));
                    for (summary, value) in options.summaries.iter().zip(&group.aggregates) {
                        label.push_str(&format!(" · {}: {}", summary.title, value.display()));
                    }
                    if label.len() > 16_384 {
                        return Err(error("Group caption exceeds 16 KiB"));
                    }
                    let key = allocate(identity, label)?;
                    next.roots.push(key);
                    next.groups.push(key);
                    next.children
                        .insert(key, group.row_keys.into_iter().map(row_key).collect());
                }
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
fn value_identity(value: &CellValue) -> String {
    match value {
        CellValue::Real(v) if *v == 0.0 => "Real(0)".into(),
        CellValue::Decimal(v) => format!("Decimal({v})"),
        _ => format!("{value:?}"),
    }
}

impl SnapshotSource {
    pub(super) fn apply_data_query(&mut self, query: DataQuery) -> Result<(), ControlError> {
        // Evaluate the candidate legacy query and presentation without touching
        // the shared source. Aggregate/type failures therefore cannot publish a
        // half-applied query to sibling views.
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
        next.keys = (0..preview.len())
            .filter_map(|i| preview.row_key(i))
            .collect();
        next.positions = next
            .keys
            .iter()
            .enumerate()
            .map(|(i, key)| (*key, i))
            .collect();
        next.data_query = query.clone();
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
                SyntheticIdentity::Group(key) => GridRowKind::Group { key: key.clone() },
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
            SyntheticIdentity::Group(_) => {
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
