//! Independent detail views and keyed parent-child projections.
use super::*;
use rust_desktop_ui_data::{DataRow, DataWrite, FieldType, TableSchema};

#[derive(Clone)]
pub struct GridRelation {
    pub name: String,
    pub source: DataSourceHandle,
    pub master_field: String,
    pub foreign_field: String,
    pub settings: GridControlSettings,
}
type ChildLoader = Rc<dyn Fn(i64) -> Result<Vec<DataRow>, ControlError>>;
#[derive(Default)]
pub(super) struct Relations {
    owner: Option<(WidgetId, i64, String)>,
    pub(super) definitions: HashMap<String, GridRelation>,
    pub(super) children: HashMap<(WidgetId, i64, String), WidgetId>,
    pub(super) loader: Option<ChildLoader>,
}
impl SnapshotSource {
    pub(super) fn locked_field(&self, field: &str) -> bool {
        self.relation.as_ref().is_some_and(|(f, _)| f == field)
    }
    pub(super) fn check_relation_row(
        &self,
        schema: &TableSchema,
        row: &DataRow,
    ) -> Result<(), ControlError> {
        if let Some((field, value)) = &self.relation
            && (value == &CellValue::Null
                || schema.field_index(field).and_then(|i| row.values.get(i)) != Some(value))
        {
            return Err(error("Record does not belong to this relation"));
        }
        Ok(())
    }
    pub(super) fn tree_projection(&self) -> Result<Projection, ControlError> {
        let field = self
            .parent_field
            .as_deref()
            .ok_or_else(|| error("Missing parent field"))?;
        if !self.presentation.group_fields.is_empty()
            || self.detail.is_some()
            || !self.presentation.summaries.is_empty()
            || !self.presentation.custom_summaries.is_empty()
        {
            return Err(error(
                "Parent tree requires ungrouped records without summary or inline detail rows",
            ));
        }
        let i = self
            .table
            .schema()
            .field_index(field)
            .ok_or_else(|| error("Missing parent field"))?;
        let schema = &self.table.schema().fields()[i];
        if schema.kind != FieldType::Integer || !schema.nullable {
            return Err(error("Parent field must be a nullable integer key"));
        }
        let mut parents = HashMap::new();
        for row in self.table.rows() {
            if let CellValue::Integer(p) = row.values[i] {
                if self.table.row(p).is_none() {
                    return Err(error("Unknown parent key"));
                }
                parents.insert(row.key, p);
            }
        }
        let mut checked = HashSet::new();
        for key in parents.keys() {
            let mut path = HashSet::new();
            let mut k = *key;
            while !checked.contains(&k) {
                if !path.insert(k) {
                    return Err(error("Cycle in parent keys"));
                }
                let Some(&p) = parents.get(&k) else { break };
                k = p;
            }
            checked.extend(path);
        }
        let mut included = HashSet::new();
        for &key in &self.keys {
            let mut k = key;
            while included.insert(k) {
                let Some(&p) = parents.get(&k) else { break };
                k = p;
            }
        }
        let mut result = Projection::default();
        // Matching siblings keep query order. Add nonmatching ancestors in stable schema order.
        for key in self
            .keys
            .iter()
            .copied()
            .chain(self.table.rows().iter().map(|r| r.key))
        {
            if !included.remove(&key) {
                continue;
            }
            let key_row = row_key(key);
            let siblings = if let Some(&p) = parents.get(&key) {
                result.parents.insert(key_row, row_key(p));
                result.children.entry(row_key(p)).or_default()
            } else {
                &mut result.roots
            };
            result.positions.insert(key_row, siblings.len());
            siblings.push(key_row);
        }
        Ok(result)
    }
}
impl Controls {
    pub(super) fn grid_relation_live(&self, mut id: WidgetId) -> bool {
        while let Some(g) = self.grids.get(&id) {
            let Some((master, key, name)) = &g.relations.owner else {
                return true;
            };
            let Some(parent) = self.grids.get(master) else {
                return false;
            };
            let Some(relation) = parent.relations.definitions.get(name) else {
                return false;
            };
            let Ok(data) = parent.source.handle.try_borrow() else {
                return false;
            };
            if data.table().row(*key).is_none() {
                return false;
            }
            let value = if data
                .table()
                .schema()
                .field(&relation.master_field)
                .is_some()
            {
                data.table().value(*key, &relation.master_field)
            } else {
                parent.source.table.value(*key, &relation.master_field)
            };
            if !g
                .source
                .relation
                .as_ref()
                .is_some_and(|(_, v)| Some(v) == value)
            {
                return false;
            }
            id = *master;
        }
        false
    }
    pub fn set_grid_parent_field(
        &mut self,
        id: WidgetId,
        field: Option<&str>,
    ) -> Result<(), ControlError> {
        let g = self.grids.get(&id).ok_or_else(|| error("Unknown grid"))?;
        let mut next = g.source.clone();
        next.parent_field = field.map(str::to_owned);
        next.lazy_loaded = None;
        self.install_grid_presentation(id, next, g.current_arrangement())?;
        let rel = &mut self.grids.get_mut(&id).unwrap().relations;

        rel.loader = None;
        Ok(())
    }
    pub fn set_grid_child_loader(
        &mut self,
        id: WidgetId,
        loader: impl Fn(i64) -> Result<Vec<DataRow>, ControlError> + 'static,
    ) -> Result<(), ControlError> {
        let g = self
            .grids
            .get_mut(&id)
            .ok_or_else(|| error("Unknown grid"))?;
        if g.source.parent_field.is_none() {
            return Err(error("Configure a parent tree first"));
        }
        g.relations.loader = Some(Rc::new(loader));
        g.source.lazy_loaded = Some(HashSet::new());
        self.scene_dirty = true;
        Ok(())
    }
    /// Fetch and validate a branch once. A failed load can be retried.
    pub fn load_grid_branch(&mut self, id: WidgetId, key: i64) -> Result<(), ControlError> {
        let g = self.grids.get(&id).ok_or_else(|| error("Unknown grid"))?;
        let field = g
            .source
            .parent_field
            .as_deref()
            .ok_or_else(|| error("Missing parent field"))?;
        if !g.source.projection.positions.contains_key(&row_key(key)) {
            return Err(error("Unknown branch"));
        }
        if g.source
            .lazy_loaded
            .as_ref()
            .is_some_and(|keys| !keys.contains(&key))
            && let Some(loader) = g.relations.loader.clone()
        {
            let data = g.source.handle.try_borrow().map_err(error)?;
            let i = data
                .table()
                .schema()
                .field_index(field)
                .ok_or_else(|| error("Unknown parent field"))?;
            drop(data);
            let rows = loader(key)?;
            if rows
                .iter()
                .any(|r| r.values.get(i) != Some(&CellValue::Integer(key)))
            {
                return Err(error("Loader returned a row for a different parent"));
            }
            let writes = rows.into_iter().map(DataWrite::Insert).collect::<Vec<_>>();
            self.apply_grid_batch(id, &writes, None)?;
            self.grids
                .get_mut(&id)
                .unwrap()
                .source
                .lazy_loaded
                .as_mut()
                .unwrap()
                .insert(key);
        }
        self.set_grid_expanded(id, row_key(key), true)
    }
    pub fn register_grid_relation(
        &mut self,
        id: WidgetId,
        relation: GridRelation,
    ) -> Result<(), ControlError> {
        let g = self
            .grids
            .get_mut(&id)
            .ok_or_else(|| error("Unknown grid"))?;
        let m = g
            .source
            .table
            .schema()
            .field(&relation.master_field)
            .ok_or_else(|| error("Unknown master field"))?;
        let data = relation.source.try_borrow().map_err(error)?;
        let f = data
            .table()
            .schema()
            .field(&relation.foreign_field)
            .ok_or_else(|| error("Unknown foreign field"))?;
        if !valid_name(&relation.name)
            || g.relations.definitions.contains_key(&relation.name)
            || m.kind != f.kind
        {
            return Err(error("Duplicate or incompatible relation"));
        }
        drop(data);
        g.relations
            .definitions
            .insert(relation.name.clone(), relation);
        Ok(())
    }
    /// Mount the detail grid in a dedicated host. Revisiting a tab restores its retained view.
    pub fn open_grid_relation(
        &mut self,
        master: WidgetId,
        key: i64,
        name: &str,
        host: WidgetId,
        style: LayoutStyle,
    ) -> Result<WidgetId, ControlError> {
        let g = self
            .grids
            .get(&master)
            .ok_or_else(|| error("Unknown master grid"))?;
        let r = g
            .relations
            .definitions
            .get(name)
            .ok_or_else(|| error("Unknown relation"))?
            .clone();
        if !g.source.positions.contains_key(&key) {
            return Err(error("Record outside master view"));
        }
        let value = g
            .source
            .table
            .value(key, &r.master_field)
            .ok_or_else(|| error("Missing master value"))?
            .clone();
        let tab = (host, key, name.to_owned());
        let siblings = self.relation_views_in(host);
        let child = if let Some(&id) = g
            .relations
            .children
            .get(&tab)
            .filter(|id| self.grids.contains_key(id))
        {
            if self.grids[&id]
                .source
                .relation
                .as_ref()
                .is_some_and(|(_, v)| *v != value)
            {
                return Err(error("Master link changed; close and reopen this view"));
            }
            id
        } else {
            if g.relations.children.len() >= 128 {
                return Err(error("Close an inactive detail before opening more views"));
            }
            let child = self.grid_control(host, r.source, r.settings, style)?;
            self.grids.get_mut(&child).unwrap().relations.owner =
                Some((master, key, name.to_owned()));
            let state = &self.grids[&child];
            let mut src = state.source.clone();
            src.relation = Some((r.foreign_field, value));
            src.data_query = Default::default();
            if let Err(err) =
                self.install_grid_presentation(child, src, state.current_arrangement())
            {
                self.remove(child)?;
                return Err(err);
            }
            self.grids
                .get_mut(&master)
                .unwrap()
                .relations
                .children
                .insert(tab, child);
            child
        };
        for id in siblings.into_iter().filter(|id| *id != child) {
            if self.grids.contains_key(&id) {
                self.edit_props(id, |p| {
                    p.visible = false;
                    p.collapsed = true;
                })?;
            }
        }
        self.edit_props(child, |p| {
            p.visible = true;
            p.collapsed = false;
        })?;
        self.scene_dirty = true;
        Ok(child)
    }
    fn relation_views_in(&self, host: WidgetId) -> Vec<WidgetId> {
        self.grids
            .values()
            .flat_map(|g| g.relations.children.iter())
            .filter_map(|((h, _, _), id)| {
                (*h == host && self.grids.contains_key(id)).then_some(*id)
            })
            .collect()
    }
    pub fn hide_grid_relations(&mut self, host: WidgetId) -> Result<(), ControlError> {
        for id in self.relation_views_in(host) {
            self.edit_props(id, |p| {
                p.visible = false;
                p.collapsed = true;
            })?;
        }
        self.scene_dirty = true;
        Ok(())
    }
    pub fn has_grid_relation(&self, id: WidgetId, name: &str) -> bool {
        self.grids
            .get(&id)
            .is_some_and(|g| g.relations.definitions.contains_key(name))
    }
    pub fn close_grid_relation(
        &mut self,
        master: WidgetId,
        child: WidgetId,
    ) -> Result<(), ControlError> {
        let g = self
            .grids
            .get(&master)
            .ok_or_else(|| error("Unknown master"))?;
        if !g.relations.children.values().any(|id| *id == child) {
            return Err(error("Unknown detail grid"));
        }
        if self.grid_row_draft(child).is_some() || self.grid_editor(child).is_some() {
            return Err(error("Finish the detail edit first"));
        }
        self.remove(child)?;
        self.grids
            .get_mut(&master)
            .unwrap()
            .relations
            .children
            .retain(|_, id| *id != child);
        Ok(())
    }
}
