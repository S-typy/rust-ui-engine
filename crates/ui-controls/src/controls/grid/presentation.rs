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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GridBand {
    pub title: String,
    /// Disjoint fields. Reordering, hiding or pinning can split a band into fragments.
    pub fields: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct GridPresentationSettings {
    pub view: GridViewMode,
    /// A composite grouping key. At most eight fields; NULL uses QueryEngine semantics.
    pub group_fields: Vec<String>,
    pub summaries: Vec<GridSummary>,
    pub bands: Vec<GridBand>,
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
    DetailHeader { master: i64 },
    Detail { master: i64, key: i64 },
}

impl GridPresentationSettings {
    /// Validate metadata and aggregate/relation results against accepted snapshots.
    /// The supplied columns must be resolved (not an empty auto-column request).
    /// This performs no storage I/O or source/query mutation.
    pub fn validate_source(
        &self,
        source: &DataSourceHandle,
        columns: &[GridControlColumn],
        detail: Option<&GridDetailSettings>,
    ) -> Result<(), ControlError> {
        self.validate(columns)?;
        let mut snapshot = SnapshotSource::new(
            source.clone(),
            columns.iter().map(|c| c.field.clone()).collect(),
        )?;
        let mut names = HashSet::new();
        if columns.is_empty()
            || columns.len() > 64
            || columns.iter().any(|c| {
                !names.insert(&c.field) || snapshot.table.schema().field(&c.field).is_none()
            })
        {
            return Err(error("Grid columns require 1..64 unique known fields"));
        }
        snapshot.presentation = self.clone();
        snapshot.detail = detail.cloned();
        Projection::build(&snapshot, &Projection::default())?;
        Ok(())
    }

    pub(super) fn validate(&self, columns: &[GridControlColumn]) -> Result<(), ControlError> {
        if self.group_fields.len() > 8 || self.summaries.len() > 16 || self.bands.len() > 16 {
            return Err(error(
                "Grid presentation supports 8 group fields, 16 summaries and 16 bands",
            ));
        }
        let known: HashSet<_> = columns.iter().map(|c| c.field.as_str()).collect();
        let mut seen = HashSet::new();
        for field in &self.group_fields {
            if !known.contains(field.as_str()) || !seen.insert(field) {
                return Err(error("Grouping requires unique grid fields"));
            }
        }
        for summary in &self.summaries {
            if summary.title.len() > 256 || summary.title.chars().any(char::is_control) {
                return Err(error(
                    "Summary title exceeds 256 bytes or contains control characters",
                ));
            }
        }
        let mut band_fields = HashSet::new();
        for band in &self.bands {
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
