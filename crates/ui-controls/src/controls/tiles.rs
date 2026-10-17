//! Virtual adaptive tiles with separate selection and invocation semantics.
use super::list_item_templates::MountedTemplate;
use super::tiles_layout::*;
use super::tiles_model::*;
use super::*;
use rust_desktop_ui_core::{RecyclingSlots, VirtualLease};
use std::cell::RefCell;
use std::rc::Rc;

mod input;
mod materialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TileCollection {
    pub container: WidgetId,
    pub viewport: WidgetId,
    pub content: WidgetId,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TileEventKind {
    SelectionChanged(Vec<String>),
    Activated {
        key: String,
        command: Option<CommandId>,
    },
    Reordered {
        key: String,
        group: String,
        before: Option<String>,
    },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TileEvent {
    pub owner: WidgetId,
    pub kind: TileEventKind,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TileStats {
    pub total: usize,
    pub realized: usize,
    pub allocated_slots: usize,
    pub created: u64,
    pub released: u64,
}
type ModelGate = dyn Fn(&TileModel) -> Result<(), String>;
#[derive(Clone)]
pub struct TileModelValidator(Rc<ModelGate>);
impl TileModelValidator {
    pub fn new(gate: impl Fn(&TileModel) -> Result<(), String> + 'static) -> Self {
        Self(Rc::new(gate))
    }
}
struct MountedTile {
    row: WidgetId,
    button: WidgetId,
    content: MountedTemplate,
    recipe: ListItemTemplate,
    lease: VirtualLease,
    parents: Vec<(WidgetId, WidgetId)>,
}
struct SourcePolicy {
    alive: bool,
    model: Rc<TileModel>,
    settings: TileSettings,
    validator: Option<SelectionValidator>,
}
struct Header {
    widget: WidgetId,
    caption: String,
}
struct Entry {
    parts: TileCollection,
    model: Rc<TileModel>,
    settings: TileSettings,
    geometry: TileGeometry,
    template: Option<ListItemTemplate>,
    keys: Vec<String>,
    cursor: Option<String>,
    slots: RecyclingSlots<String>,
    mounted: HashMap<String, MountedTile>,
    headers: HashMap<String, Header>,
    validator: Option<SelectionValidator>,
    source_validators: Vec<SelectionValidator>,
    model_validator: Option<TileModelValidator>,
    events: Vec<TileEventKind>,
    error: Option<String>,
    created: u64,
    released: u64,
    source: Rc<RefCell<SourcePolicy>>,
}
#[derive(Default)]
pub(super) struct Tiles {
    entries: HashMap<WidgetId, Entry>,
    actions: HashMap<WidgetId, (WidgetId, String, bool)>,
}

impl Controls {
    pub fn tile_collection(
        &mut self,
        parent: WidgetId,
        label: impl Into<String>,
        model: TileModel,
        settings: TileSettings,
        style: LayoutStyle,
    ) -> Result<TileCollection, ControlError> {
        if self.tiles.entries.len() >= 64 {
            return Err(tile_error("At most 64 tile collections are supported"));
        }
        let label = label.into();
        self.measure_text(&label, 4096.0, false)?;
        self.preflight_tile_model(&model, &settings)?;
        let width = match style.width {
            Length::Px(w) => w,
            _ => 640.0,
        };
        let geometry = TileGeometry::new(&model, &settings, width)?;
        let view = self.scroll_view(parent, style, geometry.extent)?;
        let parts = TileCollection {
            container: view.viewport,
            viewport: view.viewport,
            content: view.content,
        };
        self.set_label(parts.container, label)?;
        self.set_role(parts.container, Role::ListBox);
        self.controls
            .get_mut(&parts.content)
            .unwrap()
            .transparent_panel = true;
        self.edit_props(parts.content, |p| {
            p.style.kind = LayoutKind::Overlay;
            p.style.padding = Edges::all(0.0);
            p.style.gap = 0.0;
        })?;
        let model = Rc::new(model);
        let source = Rc::new(RefCell::new(SourcePolicy {
            alive: true,
            model: model.clone(),
            settings: settings.clone(),
            validator: None,
        }));
        self.tiles.entries.insert(
            parts.container,
            Entry {
                parts,
                model,
                settings,
                geometry,
                template: None,
                keys: Vec::new(),
                cursor: None,
                source,
                slots: RecyclingSlots::new(4096).map_err(|e| tile_error(e.to_string()))?,
                mounted: HashMap::new(),
                headers: HashMap::new(),
                validator: None,
                source_validators: Vec::new(),
                model_validator: None,
                events: Vec::new(),
                error: None,
                created: 0,
                released: 0,
            },
        );
        if let Err(error) = self.realize_tiles(parts.container) {
            self.remove(parts.container)?;
            return Err(error);
        }
        Ok(parts)
    }
    pub fn tile_parts(&self, id: WidgetId) -> Option<TileCollection> {
        self.tiles.entries.get(&id).map(|e| e.parts)
    }
    pub fn tile_model(&self, id: WidgetId) -> Option<&TileModel> {
        self.tiles.entries.get(&id).map(|e| e.model.as_ref())
    }
    pub fn tile_settings(&self, id: WidgetId) -> Option<&TileSettings> {
        self.tiles.entries.get(&id).map(|e| &e.settings)
    }
    pub fn tile_selected_keys(&self, id: WidgetId) -> Option<&[String]> {
        self.tiles.entries.get(&id).map(|e| e.keys.as_slice())
    }
    pub fn tile_cursor(&self, id: WidgetId) -> Option<&str> {
        self.tiles.entries.get(&id)?.cursor.as_deref()
    }
    pub fn tile_error(&self, id: WidgetId) -> Option<&str> {
        self.tiles.entries.get(&id)?.error.as_deref()
    }
    pub fn tile_item_widget(&self, id: WidgetId, key: &str) -> Option<WidgetId> {
        Some(self.tiles.entries.get(&id)?.mounted.get(key)?.row)
    }
    pub fn tile_invoke_widget(&self, id: WidgetId, key: &str) -> Option<WidgetId> {
        Some(self.tiles.entries.get(&id)?.mounted.get(key)?.button)
    }
    pub fn tile_item_lease(&self, id: WidgetId, key: &str) -> Option<VirtualLease> {
        Some(self.tiles.entries.get(&id)?.mounted.get(key)?.lease)
    }
    pub fn resolve_tile_lease(&self, id: WidgetId, lease: VirtualLease) -> Option<&str> {
        self.tiles
            .entries
            .get(&id)?
            .slots
            .resolve(lease)
            .map(String::as_str)
    }
    pub fn tile_item_bounds(&self, id: WidgetId, key: &str) -> Option<Rect> {
        let e = self.tiles.entries.get(&id)?;
        Some(e.geometry.rects[*e.geometry.order.get(*e.geometry.positions.get(key)?)?])
    }
    pub fn tile_stats(&self, id: WidgetId) -> Option<TileStats> {
        let e = self.tiles.entries.get(&id)?;
        Some(TileStats {
            total: e.model.items().len(),
            realized: e.mounted.len(),
            allocated_slots: e.slots.allocated_slots(),
            created: e.created,
            released: e.released,
        })
    }
    pub fn drain_tile_events(&mut self) -> Vec<TileEvent> {
        self.tiles
            .entries
            .iter_mut()
            .flat_map(|(&owner, e)| {
                std::mem::take(&mut e.events)
                    .into_iter()
                    .map(move |kind| TileEvent { owner, kind })
            })
            .collect()
    }
    pub fn set_tile_selection_validator(
        &mut self,
        id: WidgetId,
        validator: Option<SelectionValidator>,
    ) -> Result<(), ControlError> {
        self.check_tile_parts(id)?;
        let e = self.tiles.entries.get_mut(&id).unwrap();
        e.source.borrow_mut().validator = validator.clone();
        e.validator = validator;
        Ok(())
    }
    pub(crate) fn tile_source_validator(&self, id: WidgetId) -> Option<SelectionValidator> {
        let policy = self.tiles.entries.get(&id)?.source.clone();
        Some(SelectionValidator::new(move |keys| {
            let p = policy.borrow();
            if !p.alive {
                return Err("Tile source was removed".into());
            }
            validate_keys(&p.model, &p.settings, keys).map_err(|e| e.to_string())?;
            if let Some(v) = &p.validator {
                v.check(keys)?;
            }
            Ok(())
        }))
    }
    pub fn set_tile_model_validator(
        &mut self,
        id: WidgetId,
        validator: Option<TileModelValidator>,
    ) -> Result<(), ControlError> {
        self.check_tile_parts(id)?;
        self.tiles.entries.get_mut(&id).unwrap().model_validator = validator;
        Ok(())
    }
    pub(crate) fn add_tile_source_validator(
        &mut self,
        id: WidgetId,
        validator: SelectionValidator,
    ) -> Result<(), ControlError> {
        self.check_tile_parts(id)?;
        self.tiles
            .entries
            .get_mut(&id)
            .unwrap()
            .source_validators
            .push(validator);
        Ok(())
    }
    pub fn set_tile_selected_keys(
        &mut self,
        id: WidgetId,
        keys: Vec<String>,
    ) -> Result<(), ControlError> {
        self.select_tiles(id, keys, false)
    }
    fn validate_tile_selection(
        &self,
        id: WidgetId,
        model: &TileModel,
        settings: &TileSettings,
        keys: &[String],
    ) -> Result<(), ControlError> {
        validate_keys(model, settings, keys)?;
        let e = &self.tiles.entries[&id];
        for validator in e.validator.iter().chain(e.source_validators.iter()) {
            validator.check(keys).map_err(tile_error)?;
        }
        Ok(())
    }
    fn select_tiles(
        &mut self,
        id: WidgetId,
        keys: Vec<String>,
        notify: bool,
    ) -> Result<(), ControlError> {
        self.check_tile_parts(id)?;
        let e = &self.tiles.entries[&id];
        self.validate_tile_selection(id, &e.model, &e.settings, &keys)?;
        if e.keys == keys {
            return Ok(());
        }
        if notify && e.events.len() >= 256 {
            return Err(tile_error("Drain tile events before changing selection"));
        }
        let e = self.tiles.entries.get_mut(&id).unwrap();
        e.keys = keys.clone();
        e.error = None;
        self.controls.get_mut(&id).unwrap().validation_error = None;
        if notify {
            e.events.push(TileEventKind::SelectionChanged(keys));
        }
        self.refresh_tile_selection(id)?;
        Ok(())
    }
    pub fn set_tile_model(&mut self, id: WidgetId, model: TileModel) -> Result<(), ControlError> {
        self.check_tile_parts(id)?;
        let e = &self.tiles.entries[&id];
        let settings = e.settings.clone();
        let template = e.template.clone();
        self.replace_tiles(id, Rc::new(model), settings, template)
    }
    pub fn set_tile_settings(
        &mut self,
        id: WidgetId,
        settings: TileSettings,
    ) -> Result<(), ControlError> {
        self.check_tile_parts(id)?;
        let e = &self.tiles.entries[&id];
        self.replace_tiles(id, e.model.clone(), settings, e.template.clone())
    }
    pub fn set_tile_item_template(
        &mut self,
        id: WidgetId,
        template: Option<ListItemTemplate>,
    ) -> Result<(), ControlError> {
        self.check_tile_parts(id)?;
        let e = &self.tiles.entries[&id];
        self.replace_tiles(id, e.model.clone(), e.settings.clone(), template)
    }
    pub fn move_tile(
        &mut self,
        id: WidgetId,
        key: &str,
        group: &str,
        before: Option<&str>,
    ) -> Result<(), ControlError> {
        self.check_tile_parts(id)?;
        let e = &self.tiles.entries[&id];
        if !self.tile_available(id) || !e.settings.allow_reorder {
            return Err(tile_error("Tile rearrangement is unavailable"));
        }
        let tile = e
            .model
            .items()
            .iter()
            .find(|t| t.item.key == key)
            .ok_or_else(|| tile_error("Unknown tile key"))?;
        if !tile.item.enabled {
            return Err(tile_error("Disabled tiles cannot be rearranged"));
        }
        let model = e.model.moved(key, group, before)?;
        if model == *e.model {
            return Ok(());
        }
        if e.events.len() >= 256 {
            return Err(tile_error("Drain tile events before rearranging"));
        }
        let settings = e.settings.clone();
        let template = e.template.clone();
        self.replace_tiles(id, Rc::new(model), settings, template)?;
        self.tiles
            .entries
            .get_mut(&id)
            .unwrap()
            .events
            .push(TileEventKind::Reordered {
                key: key.into(),
                group: group.into(),
                before: before.map(str::to_owned),
            });
        Ok(())
    }
    fn preflight_tile_model(
        &mut self,
        model: &TileModel,
        settings: &TileSettings,
    ) -> Result<(), ControlError> {
        model.validate()?;
        settings.validate()?;
        // A source accepted at a wide size must also remain bounded when narrow.
        TileGeometry::new(model, settings, 1.0)?;
        for item in model.items() {
            if let Some(command) = &item.command
                && !self.commands.contains(command)
            {
                return Err(tile_error("Tile references an unknown command"));
            }
        }
        for group in model.groups() {
            self.measure_text(&group.header, 4096.0, true)?;
        }
        Ok(())
    }
    fn replace_tiles(
        &mut self,
        id: WidgetId,
        model: Rc<TileModel>,
        settings: TileSettings,
        template: Option<ListItemTemplate>,
    ) -> Result<(), ControlError> {
        self.check_tile_parts(id)?;
        self.preflight_tile_model(&model, &settings)?;
        if let Some(template) = &template {
            template.validate()?;
        }
        let e = &self.tiles.entries[&id];
        if model != e.model
            && let Some(validator) = &e.model_validator
        {
            (validator.0)(&model).map_err(tile_error)?;
        }
        let allowed: HashSet<_> = model
            .items()
            .iter()
            .filter(|t| t.item.enabled)
            .map(|t| t.item.key.as_str())
            .collect();
        let keys = e
            .keys
            .iter()
            .filter(|key| allowed.contains(key.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        if keys != e.keys || settings.selection != e.settings.selection {
            self.validate_tile_selection(id, &model, &settings, &keys)?;
        }
        let cursor = e
            .cursor
            .clone()
            .filter(|key| allowed.contains(key.as_str()));
        let offset = self.scroll_offset(id).map_or(0.0, |p| p.y);
        let anchor = e.geometry.anchor(&e.model, offset);
        let width = self
            .runtime
            .bounds(id)
            .map_or(e.geometry.width, |r| r.width);
        let geometry = TileGeometry::new(&model, &settings, width)?;
        let offset = geometry.restore(anchor.as_ref());
        self.install_tile_candidate(
            id, model, settings, template, geometry, keys, cursor, offset,
        )
    }
    fn check_tile_parts(&self, id: WidgetId) -> Result<(), ControlError> {
        let e = self
            .tiles
            .entries
            .get(&id)
            .ok_or_else(|| tile_error("Unknown tile collection"))?;
        if self.runtime.tree().parent(e.parts.content) != Some(id) {
            return Err(tile_error("Tile content must remain in its owned viewport"));
        }
        for row in e.mounted.values() {
            if self.runtime.tree().parent(row.row) != Some(e.parts.content)
                || self.runtime.tree().parent(row.button) != Some(row.row)
                || self.runtime.tree().parent(row.content.root) != Some(row.button)
            {
                return Err(tile_error(
                    "Tile required presentation parts were removed or reparented",
                ));
            }
            let owned: HashSet<_> = row.parents.iter().map(|(node, _)| *node).collect();
            if self.runtime.tree().children(row.row) != Some(&[row.button])
                || self.runtime.tree().children(row.button) != Some(&[row.content.root])
                || row.parents.iter().any(|(node, parent)| {
                    self.runtime.tree().parent(*node) != Some(*parent)
                        || self
                            .runtime
                            .tree()
                            .node(*node)
                            .is_none_or(|node| node.props.focusable || node.props.hit_test)
                        || self.runtime.tree().children(*node).is_none_or(|children| {
                            children.iter().any(|child| !owned.contains(child))
                        })
                })
            {
                return Err(tile_error(
                    "Tile templates must retain their readonly owned nodes",
                ));
            }
        }
        if e.headers
            .values()
            .any(|h| self.runtime.tree().parent(h.widget) != Some(e.parts.content))
        {
            return Err(tile_error("Tile header was removed or reparented"));
        }
        Ok(())
    }
    fn tile_available(&self, id: WidgetId) -> bool {
        self.tiles
            .entries
            .get(&id)
            .is_some_and(|e| !e.settings.is_read_only)
            && self.effective_enabled(id)
            && self.runtime.tree().is_attached(id)
    }
    pub(super) fn tile_read_only(&self, id: WidgetId) -> bool {
        let owner = self.tiles.actions.get(&id).map_or(id, |v| v.0);
        self.tiles
            .entries
            .get(&owner)
            .is_some_and(|e| e.settings.is_read_only)
    }
    pub(super) fn tile_set_position(&self, id: WidgetId) -> Option<(usize, usize)> {
        let (owner, key, invoke) = self.tiles.actions.get(&id)?;
        if *invoke {
            return None;
        }
        let e = self.tiles.entries.get(owner)?;
        Some((*e.geometry.positions.get(key)? + 1, e.model.items().len()))
    }
    pub(super) fn prune_tiles(&mut self) {
        self.tiles.entries.retain(|id, e| {
            let alive = self.runtime.tree().contains(*id);
            if !alive {
                e.source.borrow_mut().alive = false;
            }
            alive
        });
        self.tiles.actions.retain(|id, (owner, _, _)| {
            self.runtime.tree().contains(*id) && self.tiles.entries.contains_key(owner)
        });
    }
    pub(super) fn preflight_tile_removal(&self, id: WidgetId) -> Result<(), ControlError> {
        if self.tiles.entries.values().any(|e| e.parts.content == id) {
            return Err(tile_error(
                "Remove the tile collection owner instead of its required content",
            ));
        }
        Ok(())
    }
    pub(super) fn sync_tiles(&mut self) -> Result<(), ControlError> {
        self.prune_tiles();
        for id in self.tiles.entries.keys().copied().collect::<Vec<_>>() {
            self.check_tile_parts(id)?;
            if self.runtime.bounds(id).is_none() {
                continue;
            }
            let e = &self.tiles.entries[&id];
            let width = self.runtime.bounds(id).unwrap().width.floor().max(1.0);
            if width != e.geometry.width {
                let model = e.model.clone();
                let settings = e.settings.clone();
                let template = e.template.clone();
                let keys = e.keys.clone();
                let cursor = e.cursor.clone();
                let anchor = e
                    .geometry
                    .anchor(&e.model, self.scroll_offset(id).map_or(0.0, |p| p.y));
                let geometry = TileGeometry::new(&model, &settings, width)?;
                let offset = geometry.restore(anchor.as_ref());
                self.install_tile_candidate(
                    id, model, settings, template, geometry, keys, cursor, offset,
                )?;
            } else {
                self.realize_tiles(id)?;
            }
        }
        Ok(())
    }
}
fn validate_keys(
    model: &TileModel,
    settings: &TileSettings,
    keys: &[String],
) -> Result<(), ControlError> {
    if settings.selection == SelectionMode::Single && keys.len() > 1 {
        return Err(tile_error("Single tile selection accepts at most one key"));
    }
    let mut unique = HashSet::new();
    let available: HashMap<_, _> = model
        .items()
        .iter()
        .map(|t| (t.item.key.as_str(), t.item.enabled))
        .collect();
    if keys
        .iter()
        .any(|key| !unique.insert(key) || available.get(key.as_str()) != Some(&true))
    {
        return Err(tile_error(
            "Selected tile keys must be distinct, known and enabled",
        ));
    }
    Ok(())
}
