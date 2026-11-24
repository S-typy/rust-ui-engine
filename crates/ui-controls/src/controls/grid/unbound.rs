//! Auxiliary values supplied by the application, keyed by the same signed row key.
use super::*;
use rust_desktop_ui_data::{QueryEngine, TableSchema};
#[derive(Clone)]
pub struct GridUnboundColumn {
    pub source: DataSourceHandle,
    pub member: String,
}
impl std::fmt::Debug for GridUnboundColumn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GridUnboundColumn")
            .field("member", &self.member)
            .finish_non_exhaustive()
    }
}
impl PartialEq for GridUnboundColumn {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.source, &other.source) && self.member == other.member
    }
}
pub(super) fn revisions(extra: &[(String, GridUnboundColumn)]) -> Result<Vec<u64>, ControlError> {
    extra
        .iter()
        .map(|(_, u)| {
            u.source
                .try_borrow()
                .map(|s| s.revision())
                .map_err(|_| error("Unbound source is busy"))
        })
        .collect()
}
pub(super) fn table(
    master: &DataTable,
    extra: &[(String, GridUnboundColumn)],
    formulas: &[(String, String)],
    replacement: Option<(&DataSourceHandle, &DataTable)>,
) -> Result<DataTable, ControlError> {
    if extra.is_empty() {
        return QueryEngine::calculated_table(master, formulas).map_err(error);
    }
    let mut fields = master.schema().fields().to_vec();
    let mut rows = master.rows().to_vec();
    for (name, column) in extra {
        let borrowed = column
            .source
            .try_borrow()
            .map_err(|_| error("Unbound source is busy"))?;
        let values = replacement
            .filter(|(s, _)| Rc::ptr_eq(s, &column.source))
            .map_or(borrowed.table(), |(_, t)| t);
        let mut field = values
            .schema()
            .field(&column.member)
            .ok_or_else(|| error("Unknown unbound value member"))?
            .clone();
        field.name = name.clone();
        fields.push(field);
        for row in &mut rows {
            let value = values
                .value(row.key, &column.member)
                .ok_or_else(|| error(format!("Missing unbound value for record {}", row.key)))?;
            row.values.push(value.clone());
        }
    }
    let table = DataTable::with_limits(
        TableSchema::new(fields).map_err(error)?,
        rows,
        master.limits(),
    )
    .map_err(error)?;
    QueryEngine::calculated_table(&table, formulas).map_err(error)
}
impl SnapshotSource {
    pub(super) fn write_cell(
        &self,
        row: i64,
        field: &str,
        expected: &CellValue,
        value: CellValue,
    ) -> Result<bool, String> {
        let auxiliary = self
            .unbound
            .iter()
            .find(|(name, _)| name == field)
            .map(|(_, u)| u);
        let (target, member) =
            auxiliary.map_or((&self.handle, field), |u| (&u.source, u.member.as_str()));
        if !self.formulas.is_empty()
            || !self.unbound.is_empty()
            || !self.presentation.summaries.is_empty()
            || !self.presentation.custom_summaries.is_empty()
            || !self.presentation.group_expressions.is_empty()
        {
            let source = target.try_borrow().map_err(|_| "Grid source is busy")?;
            let edited = source
                .table()
                .edited(row, member, expected, &value)
                .map_err(|e| e.to_string())?;
            drop(source);
            let mut candidate = self.clone();
            let base = self
                .handle
                .try_borrow()
                .map_err(|_| "Grid source is busy")?;
            let master = if auxiliary.is_some() {
                base.table()
            } else {
                &edited
            };
            candidate.table = table(
                master,
                &self.unbound,
                &self.formulas,
                Some((target, &edited)),
            )
            .map_err(|e| e.to_string())?;
            candidate.keys = candidate
                .data_query
                .row_keys(&candidate.table)
                .map_err(|e| e.to_string())?;
            candidate.positions = candidate
                .keys
                .iter()
                .enumerate()
                .map(|(i, k)| (*k, i))
                .collect();
            candidate.reload_choices().map_err(|e| e.to_string())?;
            Projection::build(&candidate, &self.projection).map_err(|e| e.to_string())?;
        }
        target
            .try_borrow_mut()
            .map_err(|_| "Grid source is already borrowed".to_owned())?
            .set_cell_expected(row, member, expected, value)
            .map_err(|e| e.to_string())
    }
}
