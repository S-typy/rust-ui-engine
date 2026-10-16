//! Bounded projections of a shared typed collection into binding paths.
use super::*;
use rust_desktop_ui_data::{CellValue, DataChange, DataTable, ObservableCollection};
use std::collections::BTreeSet;

#[derive(Clone, Debug)]
pub(super) struct RegisteredCollection {
    source: ObservableCollection,
    key_member: String,
    revision: u64,
}

impl BindingContext {
    /// Register a live collection at an existing list path, publishing its
    /// initial contents atomically. Row keys and integer fields become exact
    /// decimal strings; bool, real, text and null retain their value kinds.
    ///
    /// `key_member` names an extra object member and cannot collide with schema
    /// fields. Mutate the shared collection rather than writing this path or
    /// its ancestors/descendants with `set`; use `unregister_collection` to
    /// return the accepted list to ordinary context ownership.
    pub fn register_collection(
        &mut self,
        path: &str,
        source: ObservableCollection,
        key_member: &str,
    ) -> Result<(), BindingError> {
        let path = BindingPath::parse(path)?;
        if self.collections.len() >= MAX_BINDING_UPDATES {
            return Err(invalid("Too many registered collections"));
        }
        if path.segments().is_empty() {
            return Err(invalid("A collection requires a named list path"));
        }
        self.validate_collection_write(&path)?;
        if !matches!(self.get_path(&path)?, DataValue::List(_)) {
            return Err(invalid("A collection requires an existing List value"));
        }
        let key = BindingPath::parse(key_member)?;
        if !matches!(key.segments(), [BindingPathSegment::Field(_)]) || key_member.len() > 256 {
            return Err(invalid(
                "Collection key member must be one field name of at most 256 bytes",
            ));
        }
        let revision = source.revision();
        let projected = project(&source.snapshot(), key_member)?;
        self.set_many_impl([(path.as_str(), projected)], true)?;
        self.collections.insert(
            path,
            RegisteredCollection {
                source,
                key_member: key_member.into(),
                revision,
            },
        );
        Ok(())
    }

    /// Clone the live handle registered at exactly this canonical path.
    pub fn observable_collection(&self, path: &str) -> Option<ObservableCollection> {
        self.collections
            .get(&BindingPath::parse(path).ok()?)
            .map(|collection| collection.source.clone())
    }

    /// Detach a registration, preserving its last accepted projected value.
    pub fn unregister_collection(&mut self, path: &str) -> Result<bool, BindingError> {
        Ok(self
            .collections
            .remove(&BindingPath::parse(path)?)
            .is_some())
    }

    /// Publish pending collection revisions once. ControlBindings calls this
    /// automatically before resolving links; applications without a host may
    /// call it explicitly. Failed projection/schema/model validation preserves
    /// every accepted value and cursor, allowing a later retry. Cell-only
    /// changes publish precise member paths. Structural changes, expired
    /// journals and unaddressable member names publish the complete list.
    pub fn synchronize_collections(&mut self) -> Result<bool, BindingError> {
        let mut updates = Vec::new();
        let mut revisions = Vec::new();
        let mut total_size = ProjectionSize::default();
        let mut full_lists = false;
        for (path, collection) in &self.collections {
            let revision = collection.source.revision();
            if revision == collection.revision {
                continue;
            }
            let changes = collection.source.changes_since(collection.revision);
            let snapshot = collection.source.snapshot();
            // Registered paths never overlap. Their combined projections
            // must fit the final model even before accounting for other data.
            // Bound retained update payloads before allocating row maps or
            // copying cell strings; keep only one source snapshot at a time.
            total_size.include(projection_size(&snapshot, &collection.key_member)?)?;
            if !full_lists {
                if let Some(cells) = cell_updates(path, &snapshot, &changes.changes) {
                    updates.extend(cells);
                } else {
                    updates.push((
                        path.clone(),
                        project_values(&snapshot, &collection.key_member),
                    ));
                }
                if updates.len() > MAX_BINDING_UPDATES {
                    updates.clear();
                    full_lists = true;
                }
            }
            revisions.push((path.clone(), revision));
        }
        if revisions.is_empty() {
            return Ok(false);
        }
        if full_lists {
            for (path, _) in &revisions {
                let collection = &self.collections[path];
                updates.push((
                    path.clone(),
                    project(&collection.source.snapshot(), &collection.key_member)?,
                ));
            }
        }
        let changed = self.set_many_impl(updates, true)?;
        for (path, revision) in revisions {
            self.collections.get_mut(&path).unwrap().revision = revision;
        }
        Ok(changed)
    }

    pub(crate) fn has_collection_path(&self, path: &BindingPath) -> bool {
        self.collections
            .keys()
            .any(|registered| registered.overlaps(path))
    }

    pub(super) fn validate_collection_write(&self, path: &BindingPath) -> Result<(), BindingError> {
        if self.has_collection_path(path) {
            return Err(invalid(
                "Registered collection paths and their ancestors/descendants are read-only; mutate the collection or unregister it",
            ));
        }
        Ok(())
    }
}

fn project(table: &DataTable, key_member: &str) -> Result<DataValue, BindingError> {
    projection_size(table, key_member)?;
    Ok(project_values(table, key_member))
}

#[derive(Default)]
struct ProjectionSize {
    nodes: usize,
    bytes: usize,
}

impl ProjectionSize {
    fn include(&mut self, next: Self) -> Result<(), BindingError> {
        self.nodes = self.nodes.saturating_add(next.nodes);
        self.bytes = self.bytes.saturating_add(next.bytes);
        if self.nodes > MAX_BINDING_NODES || self.bytes > MAX_BINDING_BYTES {
            return Err(invalid(
                "Combined collection projections exceed the binding model limits",
            ));
        }
        Ok(())
    }
}

fn projection_size(table: &DataTable, key_member: &str) -> Result<ProjectionSize, BindingError> {
    if table.schema().field(key_member).is_some() {
        return Err(invalid(
            "Collection key member collides with a schema field",
        ));
    }
    // Reject obviously oversized projections before allocating per-row maps.
    let per_row = table.schema().fields().len().saturating_add(2);
    let nodes = table.rows().len().saturating_mul(per_row).saturating_add(1);
    if nodes > MAX_BINDING_NODES {
        return Err(invalid(
            "Collection projection exceeds the binding node limit",
        ));
    }
    let names = table
        .schema()
        .fields()
        .iter()
        .map(|field| field.name.len())
        .sum::<usize>()
        + key_member.len();
    let mut bytes = names.saturating_mul(table.rows().len());
    for row in table.rows() {
        bytes = bytes.saturating_add(row.key.to_string().len());
        for value in &row.values {
            bytes = bytes.saturating_add(match value {
                CellValue::Integer(value) => value.to_string().len(),
                CellValue::Text(value) => value.len(),
                _ => 0,
            });
        }
        if bytes > MAX_BINDING_BYTES {
            return Err(invalid(
                "Collection projection exceeds the binding byte limit",
            ));
        }
    }
    Ok(ProjectionSize { nodes, bytes })
}

fn project_values(table: &DataTable, key_member: &str) -> DataValue {
    let rows = table
        .rows()
        .iter()
        .map(|row| {
            let mut fields = BTreeMap::from([(
                key_member.to_owned(),
                DataValue::String(row.key.to_string()),
            )]);
            for (field, value) in table.schema().fields().iter().zip(&row.values) {
                fields.insert(field.name.clone(), project_cell(value));
            }
            DataValue::Object(fields)
        })
        .collect();
    DataValue::List(rows)
}

fn project_cell(value: &CellValue) -> DataValue {
    match value {
        CellValue::Null => DataValue::Null,
        CellValue::Bool(value) => DataValue::Bool(*value),
        CellValue::Integer(value) => DataValue::String(value.to_string()),
        CellValue::Real(value) => DataValue::Number(*value),
        CellValue::Text(value) => DataValue::String(value.clone()),
    }
}

fn cell_updates(
    path: &BindingPath,
    table: &DataTable,
    changes: &[DataChange],
) -> Option<Vec<(BindingPath, DataValue)>> {
    if changes.is_empty() {
        return None;
    }
    let mut fields = BTreeSet::new();
    for change in changes {
        let DataChange::Cell { key, field } = change else {
            return None;
        };
        let member = BindingPath::parse(field).ok()?;
        if !matches!(member.segments(), [BindingPathSegment::Field(name)] if name == field) {
            return None;
        }
        fields.insert((*key, field));
        if fields.len() > MAX_BINDING_UPDATES {
            return None;
        }
    }
    let rows: BTreeMap<_, _> = table
        .rows()
        .iter()
        .enumerate()
        .map(|(index, row)| (row.key, (index, row)))
        .collect();
    fields
        .into_iter()
        .map(|(key, field)| {
            let (index, row) = rows.get(&key)?;
            let position = table
                .schema()
                .fields()
                .iter()
                .position(|schema| &schema.name == field)?;
            Some((
                BindingPath::parse(&format!("{path}[{index}].{field}")).ok()?,
                project_cell(&row.values[position]),
            ))
        })
        .collect()
}

fn invalid(message: &str) -> BindingError {
    BindingError::InvalidValue(message.into())
}

#[cfg(test)]
#[path = "binding_collection_tests.rs"]
mod tests;
