//! List presentation shares the existing stable-key selection model.
use super::list_item_templates::MountedTemplate;
use super::*;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ListItemPresentation {
    #[default]
    Plain,
    CheckBox,
    RadioButton,
}
#[derive(Default)]
struct ItemSettings {
    presentation: ListItemPresentation,
    template: Option<ListItemTemplate>,
}
#[derive(Default)]
pub(super) struct ListItemsState {
    settings: HashMap<WidgetId, ItemSettings>,
    mounted: HashMap<WidgetId, MountedTemplate>,
}
impl Controls {
    pub fn list_item_presentation(&self, id: WidgetId) -> Option<ListItemPresentation> {
        self.rich.lists.get(&id)?;
        Some(
            self.list_items
                .settings
                .get(&id)
                .map_or(ListItemPresentation::Plain, |s| s.presentation),
        )
    }
    pub fn set_list_item_presentation(
        &mut self,
        id: WidgetId,
        presentation: ListItemPresentation,
    ) -> Result<(), ControlError> {
        let state = self
            .rich
            .lists
            .get(&id)
            .ok_or_else(|| ControlError::Invalid("Not a ListBox".into()))?;
        validate_mode(presentation, state.settings.mode)?;
        if self.list_item_presentation(id) == Some(presentation) {
            return Ok(());
        }
        let mut mutations = Vec::with_capacity(state.rows.len());
        for &row in &state.rows {
            let mut props = self
                .runtime
                .tree()
                .node(row)
                .ok_or_else(|| ControlError::Invalid("Missing ListBox row".into()))?
                .props
                .clone();
            props.style.padding.left = if presentation == ListItemPresentation::Plain {
                0.0
            } else {
                28.0
            };
            mutations.push(Mutation::SetProps { node: row, props });
        }
        self.runtime.apply(Mutation::Batch(mutations))?;
        self.list_items.settings.entry(id).or_default().presentation = presentation;
        self.scene_dirty = true;
        Ok(())
    }
    pub fn list_item_template(&self, id: WidgetId) -> Option<&ListItemTemplate> {
        self.rich.lists.get(&id)?;
        self.list_items.settings.get(&id)?.template.as_ref()
    }
    /// Replaces readonly presentation atomically for materialized rows. Selection,
    /// cursor, scroll geometry and the stable row identities remain unchanged.
    pub fn set_list_item_template(
        &mut self,
        id: WidgetId,
        template: Option<ListItemTemplate>,
    ) -> Result<(), ControlError> {
        let state = self
            .rich
            .lists
            .get(&id)
            .ok_or_else(|| ControlError::Invalid("Not a ListBox".into()))?;
        let rows: Vec<_> = state
            .rows
            .iter()
            .copied()
            .zip(state.items.iter().skip(state.row_start).cloned())
            .collect();
        if self.list_item_template(id) == template.as_ref() {
            return Ok(());
        }
        self.preflight_button_content_replacement(
            &rows.iter().map(|(row, _)| *row).collect::<Vec<_>>(),
            if template.is_some() { rows.len() } else { 0 },
        )?;
        let width = self.runtime.bounds(id).map_or(320.0, |r| r.width);
        if let Some(template) = &template {
            template.validate()?;
            for (_, item) in &rows {
                self.preflight_item_template(template, item, width)?;
            }
        }
        let mut staged = Vec::new();
        if let Some(template) = &template {
            for (row, item) in &rows {
                match self.create_list_item_template(*row, template, item) {
                    Ok(content) => staged.push((*row, content)),
                    Err(error) => {
                        for (_, content) in staged {
                            self.remove(content.root)?;
                        }
                        return Err(error);
                    }
                }
            }
        }
        // All structural, data and font validation precedes replacing old roots.
        for (row, _) in &rows {
            if let Some(old) = self.list_items.mounted.remove(row) {
                self.set_button_content(*row, None)?;
                self.remove(old.root)?;
            }
        }
        self.list_items.settings.entry(id).or_default().template = template;
        for (row, content) in staged {
            self.set_button_content(row, Some(content.root))?;
            self.list_items.mounted.insert(row, content);
        }
        self.materialize_list_rows(id)?;
        self.scene_dirty = true;
        Ok(())
    }
    /// Readonly content currently mounted for the key. Virtualized-away keys
    /// have no widget; a released subtree is never reused for another key.
    pub fn list_item_content(&self, id: WidgetId, key: &str) -> Option<WidgetId> {
        let row = self.list_row_widget(id, key)?;
        let root = self.list_items.mounted.get(&row)?.root;
        (self.runtime.tree().parent(root) == Some(row)).then_some(root)
    }
    pub(super) fn has_custom_list_content(&self, id: WidgetId) -> bool {
        self.list_item_template(id).is_some()
    }
    pub(super) fn preflight_list_content_capacity(
        &self,
        id: WidgetId,
        next_count: usize,
    ) -> Result<(), ControlError> {
        if self.has_custom_list_content(id) {
            self.preflight_button_content_replacement(&self.rich.lists[&id].rows, next_count)?;
        }
        Ok(())
    }
    pub(super) fn preflight_list_item_content(
        &mut self,
        id: WidgetId,
        item: &SelectionItem,
        width: f32,
    ) -> Result<(), ControlError> {
        if let Some(template) = self.list_item_template(id).cloned() {
            self.preflight_item_template(&template, item, width)?;
        }
        Ok(())
    }
    pub(super) fn sync_list_item_content(
        &mut self,
        id: WidgetId,
        row: WidgetId,
        item: &SelectionItem,
    ) -> Result<(), ControlError> {
        self.sync_list_item_padding(id, row)?;
        let Some(template) = self.list_item_template(id).cloned() else {
            return Ok(());
        };
        if self
            .list_items
            .mounted
            .get(&row)
            .is_some_and(|m| self.runtime.tree().parent(m.root) != Some(row))
        {
            self.list_items.mounted.remove(&row);
        }
        if !self.list_items.mounted.contains_key(&row) {
            let content = self.create_list_item_template(row, &template, item)?;
            self.set_button_content(row, Some(content.root))?;
            self.list_items.mounted.insert(row, content);
        }
        let texts = self.list_items.mounted[&row].texts.clone();
        for text in texts {
            self.set_label(text.widget, text.source.text(item))?;
        }
        Ok(())
    }
    fn sync_list_item_padding(&mut self, id: WidgetId, row: WidgetId) -> Result<(), ControlError> {
        let left = if self.list_item_presentation(id) == Some(ListItemPresentation::Plain) {
            0.0
        } else {
            28.0
        };
        self.edit_props(row, |p| p.style.padding.left = left)
    }
    pub(super) fn prune_list_item_content(&mut self) {
        self.list_items
            .settings
            .retain(|id, _| self.rich.lists.contains_key(id));
        self.list_items.mounted.retain(|row, content| {
            self.runtime.tree().contains(*row) && self.runtime.tree().contains(content.root)
        });
    }
    pub(super) fn validate_list_presentation_mode(
        &self,
        id: WidgetId,
        mode: SelectionMode,
    ) -> Result<(), ControlError> {
        validate_mode(self.list_item_presentation(id).unwrap_or_default(), mode)
    }
    pub(super) fn list_row_presentation(&self, row: WidgetId) -> ListItemPresentation {
        let Some(selection::RichAction::Select { owner, .. }) = self.rich.actions.get(&row) else {
            return ListItemPresentation::Plain;
        };
        self.list_item_presentation(*owner).unwrap_or_default()
    }
    pub(super) fn list_item_checked(&self, row: WidgetId) -> Option<bool> {
        (self.list_row_presentation(row) == ListItemPresentation::CheckBox)
            .then(|| self.controls.get(&row).is_some_and(|c| c.checked))
    }
    pub(super) fn list_item_is_radio(&self, row: WidgetId) -> bool {
        self.list_row_presentation(row) == ListItemPresentation::RadioButton
    }
    pub(super) fn list_item_set_position(&self, row: WidgetId) -> Option<(usize, usize)> {
        let selection::RichAction::Select { owner, key } = self.rich.actions.get(&row)? else {
            return None;
        };
        self.list_key_position(*owner, key)
            .map(|(position, size)| (position + 1, size))
    }
    pub(super) fn paint_list_item_indicator(
        &mut self,
        row: WidgetId,
        bounds: Rect,
        clip: Rect,
    ) -> f32 {
        let presentation = self.list_row_presentation(row);
        if presentation == ListItemPresentation::Plain {
            return 0.0;
        }
        let enabled = self.is_enabled(row);
        let selected = self.controls[&row].checked;
        let chrome = self.theme.chrome();
        let tokens = self.theme.tokens();
        let size = 18.0_f32
            .min(bounds.height)
            .min((bounds.width - 8.0).max(0.0));
        let mark = Rect::new(
            bounds.x + 8.0_f32.min(bounds.width),
            bounds.y + (bounds.height - size) * 0.5,
            size,
            size,
        );
        let foreground = if enabled {
            tokens.focus
        } else {
            chrome.disabled_text
        };
        let radius = if presentation == ListItemPresentation::RadioButton {
            size * 0.5
        } else {
            3.0_f32.min(size * 0.5)
        };
        editor_fill(
            &mut self.scene,
            mark,
            clip,
            radius,
            if selected && presentation == ListItemPresentation::CheckBox {
                foreground
            } else {
                tokens.surface
            },
        );
        shape_border(
            &mut self.scene,
            mark,
            clip,
            radius,
            1.0,
            if selected {
                foreground
            } else {
                chrome.control_border
            },
        );
        if selected {
            if presentation == ListItemPresentation::CheckBox {
                paint_checkmark(&mut self.scene, mark, clip, self.theme.on_accent());
            } else {
                let inset = size * 0.28;
                let dot = Rect::new(
                    mark.x + inset,
                    mark.y + inset,
                    (size - 2.0 * inset).max(0.0),
                    (size - 2.0 * inset).max(0.0),
                );
                self.scene
                    .rounded_fill(dot, clip, dot.width * 0.5, foreground);
            }
        }
        28.0_f32.min(bounds.width)
    }
}
fn validate_mode(
    presentation: ListItemPresentation,
    mode: SelectionMode,
) -> Result<(), ControlError> {
    if matches!(
        (presentation, mode),
        (ListItemPresentation::CheckBox, SelectionMode::Single)
            | (ListItemPresentation::RadioButton, SelectionMode::Multiple)
    ) {
        return Err(ControlError::Invalid(
            "CheckBox list presentation requires Multiple selection; RadioButton requires Single"
                .into(),
        ));
    }
    Ok(())
}
