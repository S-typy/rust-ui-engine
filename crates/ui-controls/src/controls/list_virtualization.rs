//! List materialization consumes the shared sparse-axis and identity contracts.
use super::selection::{RichAction, SelectionItem};
use super::*;
use rust_desktop_ui_core::{
    MAX_MEASURED_EXTENTS, MAX_REALIZED_ITEMS, RecyclingSlots, SparseAxis, StableScrollAnchor,
    VirtualLease, VirtualOverscan, VirtualRange,
};
use std::rc::Rc;

/// Aggregate list extent is below 2^22, where f32 retains quarter-pixel positions.
/// The reusable core axis has its independent f64 range up to 1e12.
pub const MAX_LIST_EXTENT: f64 = 4_194_296.0;
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ListVirtualizationSettings {
    pub default_row_height: f32,
    pub overscan: VirtualOverscan,
    pub max_realized_rows: usize,
}
impl Default for ListVirtualizationSettings {
    fn default() -> Self {
        Self {
            default_row_height: 32.0,
            overscan: VirtualOverscan::default(),
            max_realized_rows: MAX_REALIZED_ITEMS,
        }
    }
}
impl ListVirtualizationSettings {
    fn validate(self) -> Result<(), ControlError> {
        row_height(self.default_row_height)?;
        SparseAxis::new(0, f64::from(self.default_row_height))
            .map_err(error)?
            .range(0.0, 0.0, self.overscan, self.max_realized_rows)
            .map_err(error)?;
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct ListVirtualizationStats {
    pub range: VirtualRange,
    pub realized_rows: usize,
    pub allocated_slots: usize,
    pub measured_rows: usize,
    pub created_rows: u64,
    pub released_rows: u64,
}
struct MountedRow {
    widget: WidgetId,
    lease: VirtualLease,
    labels: Vec<WidgetId>,
}
struct ListGeometry {
    settings: ListVirtualizationSettings,
    axis: SparseAxis,
    heights: HashMap<String, f64>,
    positions: Rc<HashMap<String, usize>>,
}
struct Entry {
    geometry: ListGeometry,
    slots: RecyclingSlots<String>,
    mounted: HashMap<String, MountedRow>,
    created: u64,
    released: u64,
}
#[derive(Default)]
pub(super) struct ListVirtualState {
    entries: HashMap<WidgetId, Entry>,
}
fn error(value: impl fmt::Display) -> ControlError {
    ControlError::Invalid(value.to_string())
}
fn row_height(value: f32) -> Result<(), ControlError> {
    if !value.is_finite() || !(1.0..=4096.0).contains(&value) || (value * 4.0).fract() != 0.0 {
        return Err(error(
            "List row height must be 1..=4096 in quarter-pixel steps",
        ));
    }
    Ok(())
}
fn geometry(
    items: &[SelectionItem],
    settings: ListVirtualizationSettings,
    heights: HashMap<String, f64>,
) -> Result<ListGeometry, ControlError> {
    let positions = Rc::new(
        items
            .iter()
            .enumerate()
            .map(|(i, v)| (v.key.clone(), i))
            .collect(),
    );
    geometry_with_positions(settings, heights, positions)
}
fn geometry_with_positions(
    settings: ListVirtualizationSettings,
    heights: HashMap<String, f64>,
    positions: Rc<HashMap<String, usize>>,
) -> Result<ListGeometry, ControlError> {
    settings.validate()?;
    if heights.len() > MAX_MEASURED_EXTENTS {
        return Err(error("Too many measured list rows"));
    }
    let heights: HashMap<_, _> = heights
        .into_iter()
        .filter(|(key, _)| positions.contains_key(key))
        .collect();
    let mut axis =
        SparseAxis::new(positions.len(), f64::from(settings.default_row_height)).map_err(error)?;
    axis.replace(
        positions.len(),
        f64::from(settings.default_row_height),
        heights.iter().map(|(key, &size)| (positions[key], size)),
    )
    .map_err(error)?;
    if axis.total_extent() > MAX_LIST_EXTENT {
        return Err(error("List extent exceeds its f32-safe geometry budget"));
    }
    Ok(ListGeometry {
        settings,
        axis,
        heights,
        positions,
    })
}
impl Controls {
    pub(super) fn initialize_list_virtualization(
        &mut self,
        id: WidgetId,
    ) -> Result<(), ControlError> {
        let geometry = geometry(
            &self.rich.lists[&id].items,
            ListVirtualizationSettings::default(),
            HashMap::new(),
        )?;
        self.list_virtual.entries.insert(
            id,
            Entry {
                geometry,
                slots: RecyclingSlots::new(MAX_REALIZED_ITEMS).map_err(error)?,
                mounted: HashMap::new(),
                created: 0,
                released: 0,
            },
        );
        Ok(())
    }
    pub fn list_virtualization_settings(&self, id: WidgetId) -> Option<ListVirtualizationSettings> {
        self.list_virtual
            .entries
            .get(&id)
            .map(|e| e.geometry.settings)
    }
    pub fn list_virtualization_stats(&self, id: WidgetId) -> Option<ListVirtualizationStats> {
        let entry = self.list_virtual.entries.get(&id)?;
        Some(ListVirtualizationStats {
            range: self.virtual_list_range(id).ok()?,
            realized_rows: entry.mounted.len(),
            allocated_slots: entry.slots.allocated_slots(),
            measured_rows: entry.geometry.axis.override_count(),
            created_rows: entry.created,
            released_rows: entry.released,
        })
    }
    /// Current visual for a key, or None when outside the materialized range.
    /// A visual released for one key is never reassigned to another key.
    pub(super) fn list_key_position(&self, id: WidgetId, key: &str) -> Option<(usize, usize)> {
        let entry = self.list_virtual.entries.get(&id)?;
        Some((
            *entry.geometry.positions.get(key)?,
            self.rich.lists.get(&id)?.items.len(),
        ))
    }
    pub fn list_row_widget(&self, id: WidgetId, key: &str) -> Option<WidgetId> {
        let row = self.list_virtual.entries.get(&id)?.mounted.get(key)?;
        self.controls
            .contains_key(&row.widget)
            .then_some(row.widget)
    }
    pub fn list_row_lease(&self, id: WidgetId, key: &str) -> Option<VirtualLease> {
        self.list_virtual
            .entries
            .get(&id)?
            .mounted
            .get(key)
            .map(|r| r.lease)
    }
    pub fn list_resolve_lease(&self, id: WidgetId, lease: VirtualLease) -> Option<&str> {
        self.list_virtual
            .entries
            .get(&id)?
            .slots
            .resolve(lease)
            .map(String::as_str)
    }
    pub fn list_item_height(&self, id: WidgetId, key: &str) -> Option<f32> {
        let geometry = &self.list_virtual.entries.get(&id)?.geometry;
        geometry
            .axis
            .extent_at(*geometry.positions.get(key)?)
            .map(|v| v as f32)
    }
    pub fn set_list_virtualization(
        &mut self,
        id: WidgetId,
        settings: ListVirtualizationSettings,
    ) -> Result<(), ControlError> {
        let entry = self
            .list_virtual
            .entries
            .get(&id)
            .ok_or_else(|| error("Not a ListBox"))?;
        let next = geometry_with_positions(
            settings,
            entry.geometry.heights.clone(),
            Rc::clone(&entry.geometry.positions),
        )?;
        self.replace_list_geometry(id, next)
    }
    /// Explicit measured height, keyed independently of source order. None forgets it.
    pub fn set_list_item_height(
        &mut self,
        id: WidgetId,
        key: &str,
        height: Option<f32>,
    ) -> Result<(), ControlError> {
        if let Some(height) = height {
            row_height(height)?;
        }
        let entry = self
            .list_virtual
            .entries
            .get(&id)
            .ok_or_else(|| error("Not a ListBox"))?;
        if !entry.geometry.positions.contains_key(key) {
            return Err(error("Unknown list key"));
        }
        let mut heights = entry.geometry.heights.clone();
        if let Some(height) = height {
            heights.insert(key.into(), f64::from(height));
        } else {
            heights.remove(key);
        }
        let next = geometry_with_positions(
            entry.geometry.settings,
            heights,
            Rc::clone(&entry.geometry.positions),
        )?;
        self.replace_list_geometry(id, next)
    }
    fn list_scroll(&self, id: WidgetId) -> f64 {
        self.runtime
            .tree()
            .node(id)
            .map_or(0.0, |node| f64::from(node.props.scroll.y))
    }
    fn list_anchor(&self, id: WidgetId) -> Option<StableScrollAnchor<String>> {
        let entry = &self.list_virtual.entries[&id];
        StableScrollAnchor::capture(
            &entry.geometry.axis,
            (self.list_scroll(id) - 4.0).max(0.0),
            |i| self.rich.lists[&id].items.get(i).map(|v| v.key.clone()),
        )
    }
    fn restored_list_scroll(&self, id: WidgetId, next: &ListGeometry) -> f64 {
        let old = self.list_scroll(id);
        if old <= 4.0 {
            return old;
        }
        self.list_anchor(id).map_or(0.0, |mut anchor| {
            let index = next.positions.get(&anchor.key).copied().unwrap_or_else(|| {
                anchor
                    .fallback_index
                    .min(next.axis.count().saturating_sub(1))
            });
            if let Some(height) = next.axis.extent_at(index) {
                // Keep the anchor inside the row after f64 -> f32 conversion.
                anchor.within_item = anchor.within_item.min(height - 0.25);
            }
            anchor.restore(&next.axis, |key| next.positions.get(key).copied()) + 4.0
        })
    }
    fn range_for_list(
        &self,
        id: WidgetId,
        geometry: &ListGeometry,
        scroll: f64,
    ) -> Result<VirtualRange, ControlError> {
        let height = f64::from(self.runtime.bounds(id).map_or(320.0, |r| r.height));
        let scroll = scroll.clamp(0.0, (geometry.axis.total_extent() + 8.0 - height).max(0.0));
        let start = (scroll - 4.0).max(0.0);
        let end = (scroll + height - 4.0).clamp(0.0, geometry.axis.total_extent());
        geometry
            .axis
            .range(
                start,
                (end - start).max(0.0),
                geometry.settings.overscan,
                geometry.settings.max_realized_rows,
            )
            .map_err(error)
    }
    pub(super) fn virtual_list_range(&self, id: WidgetId) -> Result<VirtualRange, ControlError> {
        let entry = self
            .list_virtual
            .entries
            .get(&id)
            .ok_or_else(|| error("Missing list geometry"))?;
        self.range_for_list(id, &entry.geometry, self.list_scroll(id))
    }
    fn preflight_list_rows(
        &mut self,
        id: WidgetId,
        items: &[SelectionItem],
        range: &VirtualRange,
    ) -> Result<(), ControlError> {
        self.preflight_list_content_capacity(id, range.materialized.len())?;
        let keys: Vec<_> = items[range.materialized.clone()]
            .iter()
            .map(|i| i.key.clone())
            .collect();
        self.list_virtual.entries[&id]
            .slots
            .can_reconcile(&keys)
            .map_err(error)?;
        let width = self.runtime.bounds(id).map_or(320.0, |r| r.width).max(1.0);
        for item in &items[range.materialized.clone()] {
            self.preflight_list_item_content(id, item, width)?;
            TextLimits::default()
                .validate(&format!("{} {}", item.text, item.columns.join(" ")))
                .map_err(edit_error)?;
            for text in std::iter::once(&item.text).chain(item.columns.iter()) {
                self.preflight_new_editor_text(
                    text,
                    &LayoutStyle {
                        width: Length::Px(width),
                        ..Default::default()
                    },
                    true,
                )?;
            }
        }
        Ok(())
    }
    fn replace_list_geometry(
        &mut self,
        id: WidgetId,
        next: ListGeometry,
    ) -> Result<(), ControlError> {
        let scroll = self.restored_list_scroll(id, &next);
        let range = self.range_for_list(id, &next, scroll)?;
        // Borrow only the materialized slice; avoid cloning a large source on scroll.
        let preview = self.rich.lists[&id].items[range.materialized.clone()].to_vec();
        let local = VirtualRange {
            materialized: 0..preview.len(),
            ..range.clone()
        };
        self.preflight_list_rows(id, &preview, &local)?;
        let extent = next.axis.total_extent() as f32 + 8.0;
        self.list_virtual.entries.get_mut(&id).unwrap().geometry = next;
        self.set_scroll_extent(id, extent)?;
        self.scroll_to(id, scroll as f32)?;
        self.materialize_list_rows(id)
    }
    fn prepare_virtual_list_items(
        &mut self,
        id: WidgetId,
        items: &[SelectionItem],
    ) -> Result<(ListGeometry, f64), ControlError> {
        let entry = &self.list_virtual.entries[&id];
        let next = geometry(
            items,
            entry.geometry.settings,
            entry.geometry.heights.clone(),
        )?;
        let scroll = self.restored_list_scroll(id, &next);
        let range = self.range_for_list(id, &next, scroll)?;
        self.preflight_list_rows(id, items, &range)?;
        Ok((next, scroll))
    }
    /// Validate candidate geometry and currently materialized content without
    /// changing source, leases, focus, scroll, or row subtrees. Source keys and
    /// column limits have already been validated by the selection caller.
    pub(super) fn preflight_virtual_list_items(
        &mut self,
        id: WidgetId,
        items: &[SelectionItem],
    ) -> Result<(), ControlError> {
        self.prepare_virtual_list_items(id, items).map(|_| ())
    }
    pub(super) fn replace_virtual_list_items(
        &mut self,
        id: WidgetId,
        items: Vec<SelectionItem>,
        kept: Vec<String>,
    ) -> Result<(), ControlError> {
        let (next, scroll) = self.prepare_virtual_list_items(id, &items)?;
        let old = &self.rich.lists[&id];
        let cursor = old
            .cursor
            .and_then(|i| old.items.get(i))
            .and_then(|v| next.positions.get(&v.key).copied());
        let anchor = old
            .anchor
            .and_then(|i| old.items.get(i))
            .and_then(|v| next.positions.get(&v.key).copied());
        let extent = next.axis.total_extent() as f32 + 8.0;
        self.list_virtual.entries.get_mut(&id).unwrap().geometry = next;
        let list = self.rich.lists.get_mut(&id).unwrap();
        list.items = items;
        list.keys = kept;
        list.cursor = cursor.or_else(|| list.items.iter().position(|i| list.keys.contains(&i.key)));
        list.anchor = anchor.or(list.cursor);
        self.set_scroll_extent(id, extent)?;
        self.scroll_to(id, scroll as f32)?;
        self.materialize_list_rows(id)
    }
    pub(super) fn virtual_ensure_selection_visible(
        &mut self,
        id: WidgetId,
        index: usize,
    ) -> Result<(), ControlError> {
        let Some(bounds) = self.runtime.bounds(id) else {
            return Ok(());
        };
        let axis = &self.list_virtual.entries[&id].geometry.axis;
        let top = axis
            .offset_of(index)
            .ok_or_else(|| error("Unknown list index"))?
            + 4.0;
        let bottom = top
            + axis
                .extent_at(index)
                .ok_or_else(|| error("Unknown list index"))?;
        let current = self.list_scroll(id);
        if top < current {
            self.scroll_to(id, top as f32)?;
        } else if bottom > current + f64::from(bounds.height) {
            self.scroll_to(
                id,
                (bottom - f64::from(bounds.height)).max(top.min(current)) as f32,
            )?;
        }
        Ok(())
    }
    pub(super) fn virtual_list_page(&self, id: WidgetId, index: usize, forward: bool) -> usize {
        let axis = &self.list_virtual.entries[&id].geometry.axis;
        let height = f64::from(self.runtime.bounds(id).map_or(160.0, |r| r.height));
        let top = axis.offset_of(index).unwrap_or(0.0);
        let offset = if forward {
            top + height
        } else {
            (top - height).max(0.0)
        };
        axis.index_at(offset)
            .unwrap_or(axis.count().saturating_sub(1))
    }
    pub(super) fn prune_virtual_lists(&mut self) {
        self.list_virtual
            .entries
            .retain(|id, _| self.controls.contains_key(id));
        for entry in self.list_virtual.entries.values_mut() {
            entry
                .mounted
                .retain(|_, row| self.controls.contains_key(&row.widget));
        }
    }
    pub(super) fn materialize_list_rows(&mut self, id: WidgetId) -> Result<(), ControlError> {
        let range = self.virtual_list_range(id)?;
        let state = &self.rich.lists[&id];
        let content = state.parts.content;
        let items = state.items[range.materialized.clone()].to_vec();
        let selected: Vec<_> = items
            .iter()
            .map(|item| {
                if self.rich.popup.as_ref().is_some_and(|p| p.list == Some(id))
                    && self.list_item_presentation(id) != Some(ListItemPresentation::CheckBox)
                {
                    state
                        .cursor
                        .and_then(|i| state.items.get(i))
                        .is_some_and(|i| i.key == item.key)
                } else {
                    state.keys.contains(&item.key)
                }
            })
            .collect();
        let widths = state.column_widths.clone();
        let local = VirtualRange {
            materialized: 0..items.len(),
            ..range.clone()
        };
        self.preflight_list_rows(id, &items, &local)?;
        let keys: Vec<_> = items.iter().map(|i| i.key.clone()).collect();
        let mut created = HashMap::new();
        let construct = (|| {
            for item in &items {
                if !self.list_virtual.entries[&id]
                    .mounted
                    .contains_key(&item.key)
                {
                    let row = self.button(content, "", None, LayoutStyle::default())?;
                    created.insert(item.key.clone(), row);
                }
            }
            Ok::<_, ControlError>(())
        })();
        if let Err(error) = construct {
            for row in created.into_values() {
                self.remove(row)?;
            }
            return Err(error);
        }
        let changes = self
            .list_virtual
            .entries
            .get_mut(&id)
            .unwrap()
            .slots
            .reconcile(&keys)
            .map_err(error)?;
        for released in changes.released {
            if let Some(row) = self
                .list_virtual
                .entries
                .get_mut(&id)
                .unwrap()
                .mounted
                .remove(&released.key)
            {
                // The whole old subtree is destroyed. Text documents, IME,
                // command state, accessibility and handlers never cross keys.
                if self
                    .runtime
                    .focused()
                    .is_some_and(|focus| self.flyout_descendant(focus, row.widget))
                {
                    self.runtime.apply(Mutation::Focus(Some(id)))?;
                    self.synchronize_editor_focus()?;
                }
                self.remove(row.widget)?;
                let entry = self.list_virtual.entries.get_mut(&id).unwrap();
                entry.released = entry.released.saturating_add(1);
            }
        }
        let mut rows = Vec::with_capacity(items.len());
        for (local, (item, lease)) in items.iter().zip(changes.active).enumerate() {
            let fresh = created.remove(&item.key);
            if let Some(widget) = fresh {
                let entry = self.list_virtual.entries.get_mut(&id).unwrap();
                entry.mounted.insert(
                    item.key.clone(),
                    MountedRow {
                        widget,
                        lease: lease.lease,
                        labels: Vec::new(),
                    },
                );
                entry.created = entry.created.saturating_add(1);
            }
            let index = range.materialized.start + local;
            let entry = &self.list_virtual.entries[&id];
            let row = entry.mounted[&item.key].widget;
            let top = entry.geometry.axis.offset_of(index).unwrap() as f32;
            let height = entry.geometry.axis.extent_at(index).unwrap() as f32;
            let custom_content = self.has_custom_list_content(id);
            self.edit_props(row, |p| {
                p.style.kind = if custom_content {
                    LayoutKind::Overlay
                } else {
                    LayoutKind::Flex(Axis::Horizontal)
                };
                p.style.width = Length::Percent(1.0);
                p.style.height = Length::Px(height);
                p.style.flex_shrink = 0.0;
                p.style.offset = Point::new(0.0, top);
                p.tab_stop = false;
            })?;
            self.set_label(
                row,
                if item.columns.is_empty() && !self.has_custom_list_content(id) {
                    item.text.as_str()
                } else {
                    ""
                },
            )?;
            self.set_automation_name(row, format!("{} {}", item.text, item.columns.join(" ")))?;
            self.set_role(row, Role::ListItem);
            self.set_enabled(row, item.enabled)?;
            self.controls.get_mut(&row).unwrap().checked = selected[local];
            self.rich.actions.insert(
                row,
                RichAction::Select {
                    owner: id,
                    key: item.key.clone(),
                },
            );
            let texts: Vec<_> = if item.columns.is_empty() || self.has_custom_list_content(id) {
                Vec::new()
            } else {
                std::iter::once(&item.text)
                    .chain(item.columns.iter())
                    .collect()
            };
            let old_labels = self.list_virtual.entries[&id].mounted[&item.key]
                .labels
                .clone();
            if old_labels.len() != texts.len() {
                for label in old_labels {
                    self.remove(label)?;
                }
                let mut labels = Vec::new();
                for _ in &texts {
                    labels.push(self.label(row, "", LayoutStyle::default())?);
                }
                self.list_virtual
                    .entries
                    .get_mut(&id)
                    .unwrap()
                    .mounted
                    .get_mut(&item.key)
                    .unwrap()
                    .labels = labels;
            }
            let labels = self.list_virtual.entries[&id].mounted[&item.key]
                .labels
                .clone();
            for (column, (label, text)) in labels.into_iter().zip(texts).enumerate() {
                self.set_label(label, text)?;
                let width = widths.get(column).copied().flatten();
                self.edit_props(label, |p| {
                    p.style.width = Length::Px(width.unwrap_or(0.0));
                    p.style.flex_grow = if width.is_some() { 0.0 } else { 1.0 };
                    p.style.flex_shrink = 0.0;
                    p.style.height = Length::Px(height);
                })?;
            }
            self.sync_list_item_content(id, row, item)?;
            rows.push(row);
        }
        // Preserve display/semantic order without destroying retained children.
        let mutations = rows
            .iter()
            .map(|&node| Mutation::Reparent {
                node,
                parent: content,
            })
            .collect();
        self.runtime.apply(Mutation::Batch(mutations))?;
        let state = self.rich.lists.get_mut(&id).unwrap();
        state.rows = rows;
        state.row_start = range.materialized.start;
        self.scene_dirty = true;
        Ok(())
    }
}
