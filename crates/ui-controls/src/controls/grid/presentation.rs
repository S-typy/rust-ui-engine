//! Bounded table presentation metadata; query evaluation remains in ui-data.
use super::*;
use rust_desktop_ui_data::{Aggregate, QueryExpression};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GridViewMode {
    #[default]
    Table,
    /// One virtual vertical card per record; fields retain their column identity.
    Cards,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GridSummary {
    pub title: String,
    pub aggregate: Aggregate,
}

type SummaryCallback = dyn Fn(&DataTable, &[i64]) -> Result<CellValue, String>;
/// Pure aggregate over the accepted snapshot and the filtered record keys.
#[derive(Clone)]
pub struct GridCustomSummary {
    pub title: String,
    callback: Rc<SummaryCallback>,
}
impl GridCustomSummary {
    pub fn new(
        title: impl Into<String>,
        callback: impl Fn(&DataTable, &[i64]) -> Result<CellValue, String> + 'static,
    ) -> Self {
        Self {
            title: title.into(),
            callback: Rc::new(callback),
        }
    }
    pub(super) fn evaluate(
        &self,
        table: &DataTable,
        keys: &[i64],
    ) -> Result<CellValue, ControlError> {
        let value = (self.callback)(table, keys).map_err(error)?;
        if matches!(&value, CellValue::Real(v) if !v.is_finite()) || value.display().len() > 16_384
        {
            return Err(error("Invalid custom summary result"));
        }
        Ok(value)
    }
}
impl std::fmt::Debug for GridCustomSummary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GridCustomSummary")
            .field("title", &self.title)
            .finish_non_exhaustive()
    }
}
impl PartialEq for GridCustomSummary {
    fn eq(&self, other: &Self) -> bool {
        self.title == other.title && Rc::ptr_eq(&self.callback, &other.callback)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GridBand {
    pub title: String,
    /// Disjoint fields. Reordering, hiding or pinning can split a band into fragments.
    pub fields: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct GridPresentationSettings {
    pub view: GridViewMode,
    /// Build one branch per prefix of the group fields. Defaults to composite groups.
    pub hierarchical_groups: bool,
    /// Optional zero-based group summary to sort by, otherwise order by grouping key.
    pub group_sort_summary: Option<usize>,
    pub group_descending: bool,
    pub group_footer: bool,
    pub show_group_panel: bool,
    /// (field, track root) for multi-row cells within a band.
    pub stacked_fields: Vec<(String, String)>,
    pub right_pinned_fields: Vec<String>,
    /// An ordered grouping key. At most eight fields; NULL uses QueryEngine semantics.
    pub group_fields: Vec<String>,
    /// Override grouping by value with a typed expression, or @display for the cell caption.
    pub group_expressions: Vec<(String, String)>,
    pub summaries: Vec<GridSummary>,
    pub custom_summaries: Vec<GridCustomSummary>,
    /// Anchor summary indices to visible field columns in the total footer.
    pub summary_columns: Vec<(usize, String)>,
    pub bands: Vec<GridBand>,
    /// Additional nested header tiers, in top-to-bottom order.
    pub band_rows: Vec<Vec<GridBand>>,
}

#[derive(Clone)]
pub struct GridDetailSettings {
    pub source: DataSourceHandle,
    pub master_field: String,
    pub foreign_field: String,
    /// Detail cells use the corresponding master column geometry. At most one
    /// level; details are read-only and do not become master selection keys.
    pub columns: Vec<GridControlColumn>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GridColumnArrangement {
    pub field: String,
    pub visible: bool,
    pub width: f32,
    pub pinned: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GridRowKind {
    Record(i64),
    Group { key: Vec<String> },
    GroupFooter { key: Vec<String> },
    DetailHeader { master: i64 },
    Detail { master: i64, key: i64 },
}

impl GridPresentationSettings {
    pub(super) fn footer_lines(&self) -> usize {
        let count = self.summaries.len() + self.custom_summaries.len();
        usize::from(count > 0)
            + usize::from(!self.summary_columns.is_empty() && self.summary_columns.len() < count)
    }
    pub fn summary_titles(&self) -> impl Iterator<Item = &str> {
        self.summaries
            .iter()
            .map(|s| s.title.as_str())
            .chain(self.custom_summaries.iter().map(|s| s.title.as_str()))
    }

    /// Validate metadata and aggregate/relation results against accepted snapshots.
    /// The supplied columns must be resolved (not an empty auto-column request).
    /// This performs no storage I/O or source/query mutation.
    pub fn validate_source(
        &self,
        source: &DataSourceHandle,
        columns: &[GridControlColumn],
        detail: Option<&GridDetailSettings>,
    ) -> Result<(), ControlError> {
        self.preview(source, columns, detail, None).map(|_| ())
    }
    /// Return the accepted projected table for declarative source and editor validation.
    pub fn preview(
        &self,
        source: &DataSourceHandle,
        columns: &[GridControlColumn],
        detail: Option<&GridDetailSettings>,
        filter: Option<&str>,
    ) -> Result<DataTable, ControlError> {
        self.validate(columns)?;
        let mut snapshot = SnapshotSource::new(source.clone(), columns)?;
        let mut names = HashSet::new();
        if columns.is_empty()
            || columns.len() > 64
            || columns.iter().any(|c| {
                !names.insert(&c.field) || snapshot.table.schema().field(&c.field).is_none()
            })
        {
            return Err(error("Grid columns require 1..64 unique known fields"));
        }
        if let Some(filter) = filter {
            snapshot.data_query.filter = filter.to_owned();
            snapshot.keys = snapshot
                .data_query
                .row_keys(&snapshot.table)
                .map_err(error)?;
            snapshot.positions = snapshot
                .keys
                .iter()
                .enumerate()
                .map(|(i, k)| (*k, i))
                .collect();
        }
        snapshot.presentation = self.clone();
        snapshot.detail = detail.cloned();
        Projection::build(&snapshot, &Projection::default())?;
        Ok(snapshot.table)
    }

    pub(super) fn validate(&self, columns: &[GridControlColumn]) -> Result<(), ControlError> {
        if self.group_fields.len() > 8
            || self.summaries.len() + self.custom_summaries.len() > 16
            || self.bands.len() > 16
        {
            return Err(error(
                "Grid presentation supports 8 group fields, 16 summaries and 16 bands",
            ));
        }
        if self
            .group_sort_summary
            .is_some_and(|i| i >= self.summaries.len() + self.custom_summaries.len())
        {
            return Err(error("Group sort summary index is outside summaries"));
        }
        let known: HashSet<_> = columns.iter().map(|c| c.field.as_str()).collect();
        let mut group_rules = HashSet::new();
        if self.group_expressions.iter().any(|(f, e)| {
            !known.contains(f.as_str())
                || !group_rules.insert(f)
                || e.is_empty()
                || e.len() > rust_desktop_ui_data::MAX_QUERY_TEXT_BYTES
        }) {
            return Err(error("Invalid group expression"));
        }
        let mut indices = HashSet::new();
        if self.summary_columns.iter().any(|(i, f)| {
            !indices.insert(*i)
                || *i >= self.summaries.len() + self.custom_summaries.len()
                || !known.contains(f.as_str())
        }) {
            return Err(error("Invalid summary column"));
        }
        let stacked: HashSet<_> = self.stacked_fields.iter().map(|(field, _)| field).collect();
        if stacked.len() != self.stacked_fields.len()
            || self.stacked_fields.iter().any(|(f, r)| {
                !known.contains(f.as_str()) || !known.contains(r.as_str()) || stacked.contains(r)
            })
            || self
                .right_pinned_fields
                .iter()
                .any(|f| !known.contains(f.as_str()))
            || self
                .right_pinned_fields
                .iter()
                .collect::<HashSet<_>>()
                .len()
                != self.right_pinned_fields.len()
        {
            return Err(error("Unknown or ambiguous column tracks"));
        }
        let mut seen = HashSet::new();
        for field in &self.group_fields {
            if !known.contains(field.as_str()) || !seen.insert(field) {
                return Err(error("Grouping requires unique grid fields"));
            }
        }
        for title in self.summary_titles() {
            if title.len() > 256 || title.chars().any(char::is_control) {
                return Err(error(
                    "Summary title exceeds 256 bytes or contains control characters",
                ));
            }
        }
        for (field, root) in &self.stacked_fields {
            if std::iter::once(&self.bands)
                .chain(&self.band_rows)
                .flatten()
                .any(|band| band.fields.contains(field) != band.fields.contains(root))
            {
                return Err(error("Stacked fields must belong to the same band path"));
            }
        }
        let mut parent: Option<&[GridBand]> = None;
        if self.band_rows.len() >= 8
            || self.bands.len() + self.band_rows.iter().map(Vec::len).sum::<usize>() > 64
        {
            return Err(error("Invalid nested grid band layout"));
        }
        for row in std::iter::once(&self.bands).chain(&self.band_rows) {
            if parent.is_some() && row.is_empty() {
                return Err(error("Invalid nested grid band layout"));
            }
            let mut band_fields = HashSet::new();
            for band in row {
                if let Some(parents) = parent
                    && !parents
                        .iter()
                        .any(|p| band.fields.iter().all(|f| p.fields.contains(f)))
                {
                    return Err(error("Invalid nested grid band layout"));
                }

                if band.title.is_empty()
                    || band.title.len() > 256
                    || band.title.chars().any(char::is_control)
                    || band.fields.is_empty()
                    || band.fields.len() > columns.len()
                {
                    return Err(error("Invalid grid band caption or fields"));
                }
                for field in &band.fields {
                    if !known.contains(field.as_str()) || !band_fields.insert(field) {
                        return Err(error("Band fields must be known and disjoint"));
                    }
                }
            }
            parent = Some(row);
        }
        Ok(())
    }
    pub(super) fn query(&self, query: &DataQuery) -> rust_desktop_ui_data::QueryRequest {
        rust_desktop_ui_data::QueryRequest {
            order_by: query
                .sort
                .iter()
                .map(|sort| rust_desktop_ui_data::QueryOrder {
                    expression: QueryExpression::field(&sort.field),
                    direction: sort.direction,
                })
                .collect(),
            group_by: self
                .group_fields
                .iter()
                .map(QueryExpression::field)
                .collect(),
            aggregates: self.summaries.iter().map(|s| s.aggregate.clone()).collect(),
            ..Default::default()
        }
    }
}
