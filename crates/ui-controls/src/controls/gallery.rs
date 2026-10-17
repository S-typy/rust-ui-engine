//! Retained grouped samples with transient preview and accepted selection.
use super::gallery_model::{invalid, validate_selection};
use super::*;
use std::{cell::RefCell, rc::Rc};
mod host;
mod input;
mod layout;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Gallery {
    pub container: WidgetId,
    pub content: WidgetId,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GallerySelectionChanged {
    pub owner: WidgetId,
    pub key: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GalleryPreviewChanged {
    pub owner: WidgetId,
    pub key: Option<String>,
}
struct Sample {
    widget: WidgetId,
    visual: WidgetId,
    parents: Vec<(WidgetId, WidgetId)>,
}
struct Policy {
    alive: bool,
    model: GalleryModel,
    validator: Option<SelectionValidator>,
}
struct Popup {
    handle: Flyout,
    anchor: WidgetId,
    home: WidgetId,
    style: LayoutStyle,
    open: bool,
}
struct Entry {
    parts: Gallery,
    model: GalleryModel,
    settings: GallerySettings,
    template: Option<ListItemTemplate>,
    samples: HashMap<String, Sample>,
    headers: Vec<WidgetId>,
    selected: Option<String>,
    preview: Option<String>,
    cursor: Option<String>,
    preview_event: Option<Option<String>>,
    events: Vec<Option<String>>,
    epoch: u64,
    validators: Vec<SelectionValidator>,
    policy: Rc<RefCell<Policy>>,
    popup: Option<Popup>,
    parent: WidgetId,
    width: f32,
    reveal: bool,
    columns: usize,
    error: Option<String>,
}
#[derive(Default)]
pub(super) struct Galleries {
    entries: HashMap<WidgetId, Entry>,
    actions: HashMap<WidgetId, (WidgetId, String)>,
    pointer: Option<(WidgetId, String, u64)>,
}
impl Controls {
    pub fn gallery(
        &mut self,
        parent: WidgetId,
        label: impl Into<String>,
        model: GalleryModel,
        settings: GallerySettings,
        style: LayoutStyle,
    ) -> Result<Gallery, ControlError> {
        if self.galleries.entries.len() >= 32 {
            return Err(invalid("At most 32 galleries are supported"));
        }
        let label = label.into();
        self.measure_text(&label, 4096.0, false)?;
        self.preflight_gallery(&model, &settings, None)?;
        let viewer = self.scroll_view(parent, style, 0.0)?;
        let parts = Gallery {
            container: viewer.viewport,
            content: viewer.content,
        };
        let result = (|| {
            self.set_label(parts.container, label)?;
            self.set_role(parts.container, Role::ListBox);
            self.controls
                .get_mut(&parts.content)
                .unwrap()
                .transparent_panel = true;
            self.edit_props(parts.content, |p| {
                p.style.kind = LayoutKind::Overlay;
                p.style.padding = Edges::default();
                p.style.gap = 0.0;
            })?;
            let policy = Rc::new(RefCell::new(Policy {
                alive: true,
                model: model.clone(),
                validator: None,
            }));
            self.galleries.entries.insert(
                parts.container,
                Entry {
                    parts,
                    model: model.clone(),
                    settings: settings.clone(),
                    template: None,
                    samples: HashMap::new(),
                    headers: Vec::new(),
                    selected: None,
                    preview: None,
                    cursor: None,
                    preview_event: None,
                    events: Vec::new(),
                    epoch: 0,
                    validators: Vec::new(),
                    policy,
                    popup: None,
                    parent,
                    width: -1.0,
                    reveal: true,
                    columns: 1,
                    error: None,
                },
            );
            self.install_gallery(parts.container, model, settings, None)?;
            Ok::<_, ControlError>(parts)
        })();
        if result.is_err() {
            self.galleries.entries.remove(&parts.container);
            self.remove(parts.container)?;
        }
        result
    }
    pub fn gallery_parts(&self, id: WidgetId) -> Option<Gallery> {
        self.galleries.entries.get(&id).map(|e| e.parts)
    }
    pub fn gallery_model(&self, id: WidgetId) -> Option<&GalleryModel> {
        self.galleries.entries.get(&id).map(|e| &e.model)
    }
    pub fn gallery_settings(&self, id: WidgetId) -> Option<&GallerySettings> {
        self.galleries.entries.get(&id).map(|e| &e.settings)
    }
    pub fn gallery_selected_key(&self, id: WidgetId) -> Option<&str> {
        self.galleries.entries.get(&id)?.selected.as_deref()
    }
    pub fn gallery_preview_key(&self, id: WidgetId) -> Option<&str> {
        self.galleries.entries.get(&id)?.preview.as_deref()
    }
    pub fn gallery_error(&self, id: WidgetId) -> Option<&str> {
        self.galleries.entries.get(&id)?.error.as_deref()
    }
    pub fn gallery_item_widget(&self, id: WidgetId, key: &str) -> Option<WidgetId> {
        Some(self.galleries.entries.get(&id)?.samples.get(key)?.widget)
    }
    pub fn drain_gallery_selection_changes(&mut self) -> Vec<GallerySelectionChanged> {
        self.galleries
            .entries
            .iter_mut()
            .flat_map(|(&owner, e)| {
                std::mem::take(&mut e.events)
                    .into_iter()
                    .map(move |key| GallerySelectionChanged { owner, key })
            })
            .collect()
    }
    /// Hover changes are coalesced to the latest preview per gallery.
    pub fn drain_gallery_previews(&mut self) -> Vec<GalleryPreviewChanged> {
        self.galleries
            .entries
            .iter_mut()
            .filter_map(|(&owner, e)| {
                e.preview_event
                    .take()
                    .map(|key| GalleryPreviewChanged { owner, key })
            })
            .collect()
    }
    pub fn set_gallery_selection_validator(
        &mut self,
        id: WidgetId,
        validator: Option<SelectionValidator>,
    ) -> Result<(), ControlError> {
        self.check_gallery(id)?;
        self.galleries.entries[&id].policy.borrow_mut().validator = validator;
        Ok(())
    }
    pub(crate) fn gallery_source_validator(&self, id: WidgetId) -> Option<SelectionValidator> {
        let policy = self.galleries.entries.get(&id)?.policy.clone();
        Some(SelectionValidator::new(move |keys| {
            let p = policy.borrow();
            if !p.alive {
                return Err("Gallery was removed".into());
            }
            if keys.len() > 1 {
                return Err("Gallery accepts at most one key".into());
            }
            validate_selection(&p.model, keys.first().map(String::as_str))
                .map_err(|e| e.to_string())?;
            if let Some(v) = &p.validator {
                v.check(keys)?;
            }
            Ok(())
        }))
    }
    pub(crate) fn add_gallery_source_validator(
        &mut self,
        id: WidgetId,
        validator: SelectionValidator,
    ) -> Result<(), ControlError> {
        self.check_gallery(id)?;
        let e = self.galleries.entries.get_mut(&id).unwrap();
        if e.validators.len() >= 64 {
            return Err(invalid("Gallery binding validator limit exceeded"));
        }
        e.validators.push(validator);
        Ok(())
    }
    fn validate_gallery_key(
        &self,
        id: WidgetId,
        model: &GalleryModel,
        key: Option<&str>,
    ) -> Result<(), ControlError> {
        validate_selection(model, key)?;
        let keys = key.into_iter().map(str::to_owned).collect::<Vec<_>>();
        let e = &self.galleries.entries[&id];
        let p = e.policy.borrow();
        for validator in p.validator.iter().chain(e.validators.iter()) {
            validator.check(&keys).map_err(invalid)?;
        }
        Ok(())
    }
    pub fn set_gallery_selected_key(
        &mut self,
        id: WidgetId,
        key: Option<String>,
    ) -> Result<(), ControlError> {
        self.accept_gallery(id, key, false)
    }
    pub fn accept_gallery_preview(&mut self, id: WidgetId) -> Result<(), ControlError> {
        self.check_gallery(id)?;
        if !self.gallery_available(id) {
            return Err(invalid("Gallery selection is unavailable"));
        }
        let key = self.galleries.entries[&id]
            .preview
            .clone()
            .ok_or_else(|| invalid("Gallery has no preview"))?;
        self.accept_gallery(id, Some(key), true)
    }
    fn accept_gallery(
        &mut self,
        id: WidgetId,
        key: Option<String>,
        notify: bool,
    ) -> Result<(), ControlError> {
        self.check_gallery(id)?;
        let e = &self.galleries.entries[&id];
        self.validate_gallery_key(id, &e.model, key.as_deref())?;
        if notify && e.selected != key && e.events.len() >= 256 {
            return Err(invalid(
                "Drain gallery selection events before accepting another sample",
            ));
        }
        let e = self.galleries.entries.get_mut(&id).unwrap();
        if e.selected != key {
            e.selected = key.clone();
            if notify {
                e.events.push(key.clone());
            }
        }
        e.cursor = key;
        e.error = None;
        self.controls.get_mut(&id).unwrap().validation_error = None;
        self.cancel_gallery_preview(id)?;
        self.refresh_gallery_selection(id)?;
        self.reveal_gallery_selection(id)
    }
    pub fn preview_gallery_item(&mut self, id: WidgetId, key: &str) -> Result<(), ControlError> {
        self.check_gallery(id)?;
        if !self.is_enabled(id) {
            return Err(invalid("Gallery is unavailable"));
        }
        validate_selection(&self.galleries.entries[&id].model, Some(key))?;
        let e = self.galleries.entries.get_mut(&id).unwrap();
        if e.preview.as_deref() != Some(key) {
            e.preview = Some(key.into());
            e.preview_event = Some(e.preview.clone());
        }
        e.cursor = Some(key.into());
        self.scene_dirty = true;
        Ok(())
    }
    pub fn cancel_gallery_preview(&mut self, id: WidgetId) -> Result<(), ControlError> {
        let e = self
            .galleries
            .entries
            .get_mut(&id)
            .ok_or_else(|| invalid("Unknown gallery"))?;
        if e.preview.take().is_some() {
            e.preview_event = Some(None);
        }
        self.scene_dirty = true;
        Ok(())
    }
    pub fn set_gallery_model(
        &mut self,
        id: WidgetId,
        model: GalleryModel,
    ) -> Result<(), ControlError> {
        self.check_gallery(id)?;
        let e = &self.galleries.entries[&id];
        self.install_gallery(id, model, e.settings.clone(), e.template.clone())
    }
    pub fn set_gallery_settings(
        &mut self,
        id: WidgetId,
        settings: GallerySettings,
    ) -> Result<(), ControlError> {
        self.check_gallery(id)?;
        let e = &self.galleries.entries[&id];
        self.install_gallery(id, e.model.clone(), settings, e.template.clone())
    }
    pub fn set_gallery_item_template(
        &mut self,
        id: WidgetId,
        template: Option<ListItemTemplate>,
    ) -> Result<(), ControlError> {
        self.check_gallery(id)?;
        let e = &self.galleries.entries[&id];
        self.install_gallery(id, e.model.clone(), e.settings.clone(), template)
    }
    fn check_gallery(&self, id: WidgetId) -> Result<(), ControlError> {
        let e = self
            .galleries
            .entries
            .get(&id)
            .ok_or_else(|| invalid("Unknown gallery"))?;
        if self.runtime.tree().parent(id) != Some(e.parent)
            || self.runtime.tree().parent(e.parts.content) != Some(id)
        {
            return Err(invalid("Gallery owned host was reparented or removed"));
        }
        for s in e.samples.values() {
            if self.runtime.tree().parent(s.widget) != Some(e.parts.content)
                || self.runtime.tree().children(s.widget) != Some(&[s.visual])
            {
                return Err(invalid("Gallery sample parts were removed or changed"));
            }
            let owned: HashSet<_> = s.parents.iter().map(|(n, _)| *n).collect();
            if s.parents.iter().any(|(node, parent)| {
                self.runtime.tree().parent(*node) != Some(*parent)
                    || self
                        .runtime
                        .tree()
                        .node(*node)
                        .is_none_or(|n| n.props.focusable || n.props.hit_test)
                    || self
                        .runtime
                        .tree()
                        .children(*node)
                        .is_none_or(|c| c.iter().any(|n| !owned.contains(n)))
            }) {
                return Err(invalid(
                    "Gallery presentation must retain its readonly owned nodes",
                ));
            }
        }
        if e.headers
            .iter()
            .any(|h| self.runtime.tree().parent(*h) != Some(e.parts.content))
        {
            return Err(invalid("Gallery group headers were removed or reparented"));
        }
        Ok(())
    }
    fn gallery_available(&self, id: WidgetId) -> bool {
        self.galleries
            .entries
            .get(&id)
            .is_some_and(|e| !e.settings.is_read_only)
            && self.activation_available(id)
    }
    pub(super) fn gallery_read_only(&self, id: WidgetId) -> bool {
        let owner = self.galleries.actions.get(&id).map_or(id, |v| v.0);
        self.galleries
            .entries
            .get(&owner)
            .is_some_and(|e| e.settings.is_read_only)
    }
    pub(super) fn gallery_set_position(&self, id: WidgetId) -> Option<(usize, usize)> {
        let (owner, key) = self.galleries.actions.get(&id)?;
        let e = self.galleries.entries.get(owner)?;
        Some((
            e.model
                .order()
                .iter()
                .position(|&i| e.model.items()[i].item.key == *key)?
                + 1,
            e.model.items().len(),
        ))
    }
    pub(super) fn preflight_gallery_removal(&self, id: WidgetId) -> Result<(), ControlError> {
        if self.galleries.entries.values().any(|e| {
            e.parts.content == id
                || e.samples
                    .values()
                    .any(|s| s.widget == id || s.parents.iter().any(|(n, _)| *n == id))
                || e.headers.contains(&id)
        }) {
            return Err(invalid(
                "Remove a gallery owner rather than its required presentation part",
            ));
        }
        Ok(())
    }
    fn refresh_gallery_selection(&mut self, id: WidgetId) -> Result<(), ControlError> {
        let e = &self.galleries.entries[&id];
        for (key, sample) in &e.samples {
            self.controls.get_mut(&sample.widget).unwrap().checked =
                e.selected.as_ref() == Some(key);
        }
        self.scene_dirty = true;
        Ok(())
    }
    pub fn reveal_gallery_selection(&mut self, id: WidgetId) -> Result<(), ControlError> {
        self.check_gallery(id)?;
        self.galleries.entries.get_mut(&id).unwrap().reveal = true;
        self.reveal_gallery_now(id)
    }
    fn reveal_gallery_now(&mut self, id: WidgetId) -> Result<(), ControlError> {
        self.check_gallery(id)?;
        if !self.is_enabled(id) || self.runtime.bounds(id).is_none() {
            return Ok(());
        }
        let e = &self.galleries.entries[&id];
        if let Some(widget) = e
            .cursor
            .as_ref()
            .or(e.selected.as_ref())
            .and_then(|key| e.samples.get(key))
            .map(|s| s.widget)
        {
            self.bring_into_view(widget)?;
        }
        Ok(())
    }
}
