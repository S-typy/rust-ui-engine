//! Transactional multiple selection and retained tokens for combo editors.
use super::selection::CompositeKind;
use super::*;

/// Bounds both token widgets and one accepted multiple-selection value.
pub const MAX_COMBO_SELECTION: usize = 256;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ComboSelectionPresentation {
    #[default]
    Text,
    Tokens,
}

/// Multiple selection edits a key draft. Editable text is its search query;
/// custom values remain a feature of an editable single-selection combo.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ComboSelectionSettings {
    pub mode: SelectionMode,
    pub presentation: ComboSelectionPresentation,
    pub commit_on_focus_lost: bool,
}

impl Default for ComboSelectionSettings {
    fn default() -> Self {
        Self {
            mode: SelectionMode::Single,
            presentation: ComboSelectionPresentation::Text,
            commit_on_focus_lost: true,
        }
    }
}

#[derive(Clone)]
struct Entry {
    settings: ComboSelectionSettings,
    pending: Option<Vec<String>>,
    revision: u64,
    pending_revision: u64,
    viewer: ScrollViewer,
    tokens: HashMap<String, WidgetId>,
    summary: Option<WidgetId>,
}

#[derive(Clone)]
enum Action {
    Remove { owner: WidgetId, key: String },
    Apply(WidgetId),
    Cancel(WidgetId),
}

#[derive(Default)]
pub(super) struct ComboState {
    entries: HashMap<WidgetId, Entry>,
    actions: HashMap<WidgetId, Action>,
    templates: HashMap<WidgetId, ListItemTemplate>,
}

fn error(message: impl Into<String>) -> ControlError {
    ControlError::Invalid(message.into())
}

fn canonical_keys(items: &[SelectionItem], keys: &[String]) -> Result<Vec<String>, ControlError> {
    if keys.len() > MAX_COMBO_SELECTION {
        return Err(error("Combo selection exceeds 256 keys"));
    }
    let requested: HashSet<_> = keys.iter().collect();
    if requested.len() != keys.len() {
        return Err(error("Combo selection keys must be unique"));
    }
    if keys
        .iter()
        .any(|key| !items.iter().any(|item| item.enabled && &item.key == key))
    {
        return Err(error("Combo selection requires existing enabled keys"));
    }
    Ok(keys.to_vec())
}

impl Controls {
    pub fn combo_selection_settings(&self, owner: WidgetId) -> Option<ComboSelectionSettings> {
        self.combo_settings(owner)?;
        Some(
            self.combos
                .entries
                .get(&owner)
                .map_or_else(ComboSelectionSettings::default, |entry| entry.settings),
        )
    }

    pub(super) fn combo_multiple(&self, owner: WidgetId) -> bool {
        self.combos
            .entries
            .get(&owner)
            .is_some_and(|entry| entry.settings.mode == SelectionMode::Multiple)
    }

    /// A missing draft is represented by the accepted keys, never by popup row
    /// indices. Source replacement rebases the draft onto its live identities.
    pub fn combo_selection_draft(&self, owner: WidgetId) -> Option<&[String]> {
        self.combo_settings(owner)?;
        self.combos
            .entries
            .get(&owner)
            .and_then(|entry| entry.pending.as_deref())
            .or_else(|| self.selected_keys(owner))
    }

    pub fn combo_selection_is_dirty(&self, owner: WidgetId) -> Option<bool> {
        Some(self.combo_selection_draft(owner)? != self.selected_keys(owner)?)
    }

    /// Stable token remove button while that key belongs to the visible draft.
    pub fn combo_token(&self, owner: WidgetId, key: &str) -> Option<WidgetId> {
        self.combos.entries.get(&owner)?.tokens.get(key).copied()
    }

    /// Settings changes require a settled draft and composition. They preserve
    /// the editor, its command button and existing accepted key identities.
    pub fn set_combo_selection_settings(
        &mut self,
        owner: WidgetId,
        settings: ComboSelectionSettings,
    ) -> Result<(), ControlError> {
        let parts = self
            .editor_control_parts(owner)
            .filter(|_| self.combo_settings(owner).is_some())
            .cloned()
            .ok_or_else(|| error("Not a combo editor"))?;
        if settings.mode == SelectionMode::Single
            && (settings.presentation == ComboSelectionPresentation::Tokens
                || self.selected_keys(owner).is_some_and(|keys| keys.len() > 1))
        {
            return Err(error(
                "Tokens and multiple values require Multiple selection",
            ));
        }
        if self.combo_selection_settings(owner) == Some(settings) {
            return Ok(());
        }
        if self.combo_selection_is_dirty(owner) == Some(true)
            || self.is_edit_dirty(parts.editor) == Some(true)
            || self
                .document(parts.editor)
                .is_some_and(|doc| doc.composition().is_some())
        {
            return Err(error(
                "Commit or cancel the combo draft before changing selection settings",
            ));
        }
        canonical_keys(
            self.selection_items(owner).unwrap(),
            self.selected_keys(owner).unwrap(),
        )?;
        let selected: Vec<_> = self
            .selection_items(owner)
            .unwrap()
            .iter()
            .filter(|item| self.selected_keys(owner).unwrap().contains(&item.key))
            .cloned()
            .collect();
        self.preflight_combo_captions(&selected)?;
        self.preflight_combo_presentation(owner, settings, &selected)?;
        if settings.mode == SelectionMode::Multiple
            && selected.is_empty()
            && self
                .accepted_value(parts.editor)
                .is_some_and(|text| !text.is_empty())
        {
            return Err(error(
                "Clear the accepted free text before enabling Multiple selection",
            ));
        }
        let single_text = selected
            .first()
            .map(|item| item.text.clone())
            .unwrap_or_default();
        if settings.mode == SelectionMode::Single {
            self.validate_editor_value(parts.editor, &single_text)
                .map_err(error)?;
        }
        if !self.combos.entries.contains_key(&owner) {
            let viewer = self.scroll_view(
                owner,
                LayoutStyle {
                    width: Length::Px(0.0),
                    height: Length::Percent(1.0),
                    flex_shrink: 0.0,
                    ..Default::default()
                },
                0.0,
            )?;
            self.configure_scroll_view(
                viewer.viewport,
                ScrollSettings {
                    horizontal: ScrollBarVisibility::Hidden,
                    vertical: ScrollBarVisibility::Disabled,
                },
            )?;
            self.edit_props(viewer.content, |props| {
                props.style.kind = LayoutKind::Flex(Axis::Horizontal);
                props.style.padding = Edges::all(3.0);
                props.style.gap = 4.0;
            })?;
            self.combos.entries.insert(
                owner,
                Entry {
                    settings,
                    pending: None,
                    revision: 0,
                    pending_revision: 0,
                    viewer,
                    tokens: HashMap::new(),
                    summary: None,
                },
            );
        } else {
            self.combos.entries.get_mut(&owner).unwrap().settings = settings;
        }
        if settings.mode == SelectionMode::Multiple {
            self.clear_combo_query(owner)?;
        } else {
            self.set_text(parts.editor, &single_text)?;
            self.controls.get_mut(&parts.editor).unwrap().accepted_value = single_text;
        }
        let viewer = self.combos.entries[&owner].viewer;
        self.runtime.apply(Mutation::Batch(vec![
            Mutation::Reparent {
                node: viewer.viewport,
                parent: owner,
            },
            Mutation::Reparent {
                node: parts.editor,
                parent: owner,
            },
        ]))?;
        for button in parts.buttons {
            self.runtime.apply(Mutation::Reparent {
                node: button,
                parent: owner,
            })?;
        }
        self.close_rich_popup(false)?;
        self.sync_combo_presentation(owner)
    }

    fn clear_combo_query(&mut self, owner: WidgetId) -> Result<(), ControlError> {
        let editor = self
            .composite_editor(owner)
            .ok_or_else(|| error("Unknown combo editor"))?;
        self.set_text(editor, "")?;
        let control = self.controls.get_mut(&editor).unwrap();
        control.accepted_value.clear();
        control.validation_error = None;
        Ok(())
    }

    fn validate_combo_keys(
        &self,
        owner: WidgetId,
        keys: &[String],
    ) -> Result<Vec<String>, ControlError> {
        let keys = canonical_keys(
            self.selection_items(owner)
                .ok_or_else(|| error("Unknown combo editor"))?,
            keys,
        )?;
        if let Some(validator) = self.rich.selection_validators.get(&owner) {
            validator.check(&keys).map_err(error)?;
        }
        Ok(keys)
    }

    fn preflight_combo_captions(&mut self, items: &[SelectionItem]) -> Result<(), ControlError> {
        for item in items {
            if item.text.len() > 4096 {
                return Err(error(
                    "Selected combo captions are limited to 4096 UTF-8 bytes",
                ));
            }
            self.measure_text(&item.text, 4096.0, false)?;
        }
        Ok(())
    }

    fn preflight_combo_presentation(
        &mut self,
        owner: WidgetId,
        settings: ComboSelectionSettings,
        items: &[SelectionItem],
    ) -> Result<(), ControlError> {
        let entry = self.combos.entries.get(&owner);
        let old: Vec<_> =
            entry.map_or_else(Vec::new, |entry| entry.tokens.values().copied().collect());
        if let Some(entry) = entry {
            if self.runtime.tree().parent(entry.viewer.viewport) != Some(owner)
                || self.runtime.tree().parent(entry.viewer.content) != Some(entry.viewer.viewport)
                || entry
                    .summary
                    .is_some_and(|id| self.runtime.tree().parent(id) != Some(entry.viewer.content))
            {
                return Err(error(
                    "A required combo presentation part was removed or reparented",
                ));
            }
            for &token in entry.tokens.values() {
                let root = self
                    .button_content(token)
                    .ok_or_else(|| error("A required combo token template was removed"))?;
                if self.runtime.tree().parent(token) != Some(entry.viewer.content)
                    || self
                        .runtime
                        .tree()
                        .children(root)
                        .is_none_or(|children| children.len() != 2)
                {
                    return Err(error(
                        "A required combo token part was removed or reparented",
                    ));
                }
            }
        }
        let count = if settings.mode == SelectionMode::Multiple
            && settings.presentation == ComboSelectionPresentation::Tokens
        {
            items.len()
        } else {
            0
        };
        self.preflight_button_content_replacement(&old, count)?;
        if settings.presentation == ComboSelectionPresentation::Text {
            let text = items
                .iter()
                .take(3)
                .map(|item| item.text.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            self.measure_text(&text, 4096.0, false)?;
        }
        Ok(())
    }

    pub(super) fn set_multiple_combo_keys(
        &mut self,
        owner: WidgetId,
        requested: &[String],
    ) -> Result<bool, ControlError> {
        let keys = self.validate_combo_keys(owner, requested)?;
        // Complete preflight before changing accepted keys or removing tokens.
        let selected_items: Vec<_> = self
            .selection_items(owner)
            .unwrap()
            .iter()
            .filter(|i| keys.contains(&i.key))
            .cloned()
            .collect();
        self.preflight_combo_captions(&selected_items)?;
        self.preflight_combo_presentation(
            owner,
            self.combos.entries[&owner].settings,
            &selected_items,
        )?;
        let changed = self.selected_keys(owner) != Some(keys.as_slice());
        if let CompositeKind::Combo { keys: target, .. } =
            &mut self.rich.composites.get_mut(&owner).unwrap().kind
        {
            target.clone_from(&keys);
        }
        let entry = self.combos.entries.get_mut(&owner).unwrap();
        entry.pending = None;
        if changed {
            self.rich.changes.push(SelectionChanged { id: owner, keys });
        }
        self.clear_combo_query(owner)?;
        self.sync_combo_presentation(owner)?;
        self.sync_combo_popup_selection(owner)?;
        self.scene_dirty = true;
        Ok(changed)
    }

    /// Runs the live selection validators once before publishing accepted keys.
    /// Invalid drafts remain available for correction and never reach bindings.
    pub fn commit_combo_selection(
        &mut self,
        owner: WidgetId,
    ) -> Result<CommitOutcome, ControlError> {
        if !self.combo_multiple(owner) {
            let editor = self
                .composite_editor(owner)
                .ok_or_else(|| error("Not a combo editor"))?;
            return self.commit_edit(editor);
        }
        if !self.rich_editable(owner) {
            return Ok(CommitOutcome::Unchanged);
        }
        let entry = &self.combos.entries[&owner];
        let Some(keys) = entry.pending.clone() else {
            return Ok(CommitOutcome::Unchanged);
        };
        if entry.revision != entry.pending_revision {
            return Ok(CommitOutcome::Invalid(
                "The source changed; reopen the combo draft".into(),
            ));
        }
        match self.set_multiple_combo_keys(owner, &keys) {
            Ok(true) => Ok(CommitOutcome::Committed),
            Ok(false) => Ok(CommitOutcome::Unchanged),
            Err(ControlError::Invalid(message)) => {
                let editor = self.composite_editor(owner).unwrap();
                self.controls.get_mut(&editor).unwrap().validation_error = Some(message.clone());
                self.scene_dirty = true;
                Ok(CommitOutcome::Invalid(message))
            }
            Err(error) => Err(error),
        }
    }

    /// Discards pending key changes and query text. Accepted keys do not change.
    pub fn cancel_combo_selection(&mut self, owner: WidgetId) -> Result<bool, ControlError> {
        if !self.combo_multiple(owner) {
            let editor = self
                .composite_editor(owner)
                .ok_or_else(|| error("Not a combo editor"))?;
            return self.cancel_edit(editor);
        }
        let changed = self.combo_selection_is_dirty(owner).unwrap_or(false);
        self.combos.entries.get_mut(&owner).unwrap().pending = None;
        self.clear_combo_query(owner)?;
        self.sync_combo_presentation(owner)?;
        self.sync_combo_popup_selection(owner)?;
        Ok(changed)
    }

    pub(super) fn combo_widget_owner(&self, id: WidgetId) -> Option<WidgetId> {
        self.combos.actions.get(&id).map(|action| match action {
            Action::Remove { owner, .. } | Action::Apply(owner) | Action::Cancel(owner) => *owner,
        })
    }

    pub(super) fn prune_combos(&mut self) {
        self.combos
            .entries
            .retain(|owner, _| self.rich.composites.contains_key(owner));
        self.combos
            .actions
            .retain(|id, _| self.controls.contains_key(id));
        self.combos
            .templates
            .retain(|owner, _| self.rich.composites.contains_key(owner));
    }

    pub(super) fn preflight_combo_removal(&self, id: WidgetId) -> Result<(), ControlError> {
        if self.combos.entries.values().any(|entry| {
            id == entry.viewer.viewport
                || id == entry.viewer.content
                || entry.summary == Some(id)
                || entry
                    .tokens
                    .values()
                    .any(|token| self.flyout_descendant(id, *token))
        }) {
            return Err(error(
                "Remove the whole combo editor instead of its required token viewport",
            ));
        }
        Ok(())
    }

    pub(super) fn sync_combo_presentation(&mut self, owner: WidgetId) -> Result<(), ControlError> {
        let Some(entry) = self.combos.entries.get(&owner).cloned() else {
            return Ok(());
        };
        let active = entry.settings.mode == SelectionMode::Multiple;
        let keys = self.combo_selection_draft(owner).unwrap().to_vec();
        let items: Vec<_> = keys
            .iter()
            .filter_map(|key| {
                self.selection_items(owner)
                    .unwrap()
                    .iter()
                    .find(|item| &item.key == key)
            })
            .cloned()
            .collect();
        let token_mode =
            active && entry.settings.presentation == ComboSelectionPresentation::Tokens;
        self.preflight_combo_presentation(owner, entry.settings, &items)?;
        let old: Vec<_> = entry
            .tokens
            .iter()
            .filter(|(key, _)| !token_mode || !keys.contains(key))
            .map(|(key, &id)| (key.clone(), id))
            .collect();
        for (key, id) in old {
            if self.runtime.focused() == Some(id) {
                self.runtime
                    .apply(Mutation::Focus(self.composite_editor(owner)))?;
            }
            self.combos
                .entries
                .get_mut(&owner)
                .unwrap()
                .tokens
                .remove(&key);
            self.combos.actions.remove(&id);
            self.remove(id)?;
        }
        if token_mode {
            if let Some(summary) = self.combos.entries.get_mut(&owner).unwrap().summary.take() {
                self.remove(summary)?;
            }
            for item in &items {
                let existing = self.combos.entries[&owner].tokens.get(&item.key).copied();
                // Include the label's 8px paint inset on both sides, content
                // padding (8), icon (12), gap (4), and pixel-rounding reserve.
                let width = (self.measure_text(&item.text, 4096.0, false)?.width.ceil() + 42.0)
                    .clamp(48.0, 196.0);
                if let Some(id) = existing {
                    let root = self.button_content(id).unwrap();
                    let label = self.runtime.tree().children(root).unwrap()[0];
                    self.set_label(label, &item.text)?;
                    self.set_automation_name(id, format!("Удалить: {}", item.text))?;
                    self.edit_props(id, |p| p.style.width = Length::Px(width))?;
                } else {
                    let token = self.button(
                        entry.viewer.content,
                        "",
                        None,
                        LayoutStyle {
                            width: Length::Px(width),
                            height: Length::Percent(1.0),
                            flex_shrink: 0.0,
                            ..Default::default()
                        },
                    )?;
                    self.set_automation_name(token, format!("Удалить: {}", item.text))?;
                    let content = self.transparent_panel(
                        token,
                        LayoutStyle {
                            kind: LayoutKind::Flex(Axis::Horizontal),
                            width: Length::Percent(1.0),
                            height: Length::Percent(1.0),
                            padding: Edges::all(4.0),
                            gap: 4.0,
                            ..Default::default()
                        },
                    )?;
                    let label = self.label(
                        content,
                        &item.text,
                        LayoutStyle {
                            width: Length::Px(0.0),
                            flex_grow: 1.0,
                            height: Length::Px(self.theme.tokens().font_size * 1.25),
                            vertical_alignment: Some(rust_desktop_ui_core::Align::Center),
                            ..Default::default()
                        },
                    )?;
                    self.set_text_style(
                        label,
                        TextStyle {
                            wrap: false,
                            ..Default::default()
                        },
                    )?;
                    self.set_text_display(
                        label,
                        TextDisplaySettings {
                            trimming: TextTrimming::CharacterEllipsis,
                            max_lines: Some(1),
                        },
                    )?;
                    self.icon(
                        content,
                        IconSymbol::Close,
                        LayoutStyle {
                            width: Length::Px(12.0),
                            height: Length::Px(12.0),
                            flex_shrink: 0.0,
                            vertical_alignment: Some(rust_desktop_ui_core::Align::Center),
                            ..Default::default()
                        },
                    )?;
                    self.set_button_content(token, Some(content))?;
                    self.combos.actions.insert(
                        token,
                        Action::Remove {
                            owner,
                            key: item.key.clone(),
                        },
                    );
                    self.combos
                        .entries
                        .get_mut(&owner)
                        .unwrap()
                        .tokens
                        .insert(item.key.clone(), token);
                }
            }
            let ordered: Vec<_> = keys
                .iter()
                .filter_map(|key| self.combos.entries[&owner].tokens.get(key).copied())
                .collect();
            if self.runtime.tree().children(entry.viewer.content) != Some(ordered.as_slice()) {
                self.runtime.apply(Mutation::Batch(
                    ordered
                        .into_iter()
                        .map(|node| Mutation::Reparent {
                            node,
                            parent: entry.viewer.content,
                        })
                        .collect(),
                ))?;
            }
        } else if active {
            let mut text = items
                .iter()
                .take(3)
                .map(|item| item.text.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            if items.len() > 3 {
                text.push_str(&format!(" (+{})", items.len() - 3));
            }
            if let Some(label) = entry.summary {
                self.set_label(label, text)?;
            } else {
                let label = self.label(
                    entry.viewer.content,
                    text,
                    LayoutStyle {
                        height: Length::Percent(1.0),
                        width: Length::Percent(1.0),
                        ..Default::default()
                    },
                )?;
                self.set_text_style(
                    label,
                    TextStyle {
                        wrap: false,
                        ..Default::default()
                    },
                )?;
                self.set_text_display(
                    label,
                    TextDisplaySettings {
                        trimming: TextTrimming::CharacterEllipsis,
                        max_lines: Some(1),
                    },
                )?;
                self.combos.entries.get_mut(&owner).unwrap().summary = Some(label);
            }
        }
        self.edit_props(entry.viewer.viewport, |props| {
            props.focusable = false;
            props.tab_stop = false;
            props.visible = active && !items.is_empty();
            props.collapsed = !props.visible;
        })?;
        self.controls
            .get_mut(&entry.viewer.viewport)
            .unwrap()
            .transparent_panel = true;
        self.controls
            .get_mut(&entry.viewer.content)
            .unwrap()
            .transparent_panel = true;
        self.scene_dirty = true;
        self.sync_combo_geometry(owner)
    }

    fn sync_combo_geometry(&mut self, owner: WidgetId) -> Result<(), ControlError> {
        let entry = self.combos.entries[&owner].clone();
        let width = self
            .runtime
            .bounds(owner)
            .map_or(240.0, |bounds| bounds.width);
        let budget = (width - 54.0).max(0.0) * 0.7;
        let desired = if entry.settings.presentation == ComboSelectionPresentation::Tokens {
            entry
                .tokens
                .values()
                .filter_map(|id| self.runtime.tree().node(*id))
                .map(|node| match node.props.style.width {
                    Length::Px(width) => width + 4.0,
                    _ => 0.0,
                })
                .sum::<f32>()
                + 6.0
        } else {
            budget
        };
        let visible = desired.min(budget);
        let viewport = entry.viewer.viewport;
        let node = self
            .runtime
            .tree()
            .node(viewport)
            .ok_or_else(|| error("The combo token viewport was removed"))?;
        if node.props.style.width != Length::Px(visible) {
            self.edit_props(viewport, |props| props.style.width = Length::Px(visible))?;
        }
        self.set_scroll_extent_size(
            viewport,
            Size::new(desired, self.theme.tokens().control_height),
        )?;
        let editable = self.rich_editable(owner);
        for &id in entry.tokens.values() {
            self.set_enabled(id, editable)?;
        }
        Ok(())
    }

    pub(super) fn sync_combos(&mut self) -> Result<(), ControlError> {
        let ids: Vec<_> = self.combos.entries.keys().copied().collect();
        for id in ids {
            self.sync_combo_geometry(id)?;
        }
        Ok(())
    }

    pub(super) fn settle_combo_blur(&mut self, owner: WidgetId) -> Result<(), ControlError> {
        if self.combo_multiple(owner)
            && self.rich_editable(owner)
            && self.combos.entries[&owner].settings.commit_on_focus_lost
        {
            self.commit_combo_selection(owner)?;
        }
        Ok(())
    }

    fn begin_combo_draft(&mut self, owner: WidgetId) {
        let accepted = self.selected_keys(owner).unwrap().to_vec();
        let entry = self.combos.entries.get_mut(&owner).unwrap();
        if entry.pending.is_none() {
            entry.pending = Some(accepted);
            entry.pending_revision = entry.revision;
        }
    }

    pub(super) fn begin_combo_popup_draft(&mut self, owner: WidgetId) {
        if self.combo_multiple(owner) {
            self.begin_combo_draft(owner);
        }
    }

    pub(super) fn toggle_combo_draft(
        &mut self,
        owner: WidgetId,
        key: &str,
    ) -> Result<(), ControlError> {
        if !self.rich_editable(owner)
            || !self
                .selection_items(owner)
                .unwrap()
                .iter()
                .any(|item| item.key == key && item.enabled)
        {
            return Ok(());
        }
        self.begin_combo_draft(owner);
        let mut keys = self.combos.entries[&owner].pending.clone().unwrap();
        if let Some(index) = keys.iter().position(|candidate| candidate == key) {
            keys.remove(index);
        } else if keys.len() < MAX_COMBO_SELECTION {
            keys.push(key.into());
        }
        let selected: Vec<_> = self
            .selection_items(owner)
            .unwrap()
            .iter()
            .filter(|item| keys.contains(&item.key))
            .cloned()
            .collect();
        self.preflight_combo_captions(&selected)?;
        self.preflight_combo_presentation(owner, self.combos.entries[&owner].settings, &selected)?;
        self.combos.entries.get_mut(&owner).unwrap().pending = Some(keys);
        self.sync_combo_popup_selection(owner)?;
        self.sync_combo_presentation(owner)
    }

    fn sync_combo_popup_selection(&mut self, owner: WidgetId) -> Result<(), ControlError> {
        if let Some(list) = self
            .rich
            .popup
            .as_ref()
            .filter(|p| p.owner == owner)
            .and_then(|p| p.list)
        {
            let cursor = self
                .rich
                .lists
                .get(&list)
                .and_then(|state| state.cursor.and_then(|i| state.items.get(i)))
                .map(|item| item.key.clone());
            let keys: Vec<_> = self
                .combo_selection_draft(owner)
                .unwrap()
                .iter()
                .filter(|key| {
                    self.selection_items(list)
                        .unwrap()
                        .iter()
                        .any(|i| &i.key == *key)
                })
                .cloned()
                .collect();
            self.set_selected_keys(list, &keys)?;
            if let Some(index) = cursor.and_then(|key| {
                self.rich.lists[&list]
                    .items
                    .iter()
                    .position(|item| item.key == key && item.enabled)
            }) {
                self.rich.lists.get_mut(&list).unwrap().cursor = Some(index);
            }
            self.rich.changes.retain(|change| change.id != list);
        }
        Ok(())
    }

    pub(super) fn mount_combo_footer(
        &mut self,
        owner: WidgetId,
        surface: WidgetId,
        list: WidgetId,
    ) -> Result<(), ControlError> {
        if let Some(template) = self.combos.templates.get(&owner).cloned() {
            self.set_list_item_template(list, Some(template))?;
        }
        if !self.combo_multiple(owner) {
            return Ok(());
        }
        self.set_list_item_presentation(list, ListItemPresentation::CheckBox)?;
        let footer = self.transparent_panel(
            surface,
            LayoutStyle {
                kind: LayoutKind::Flex(Axis::Horizontal),
                height: Length::Px(36.0),
                flex_shrink: 0.0,
                gap: 6.0,
                ..Default::default()
            },
        )?;
        for (label, action) in [
            ("Применить", Action::Apply(owner)),
            ("Отмена", Action::Cancel(owner)),
        ] {
            let button = self.button(
                footer,
                label,
                None,
                LayoutStyle {
                    width: Length::Px(0.0),
                    flex_grow: 1.0,
                    height: Length::Percent(1.0),
                    ..Default::default()
                },
            )?;
            self.combos.actions.insert(button, action);
        }
        Ok(())
    }

    pub(super) fn activate_combo(&mut self, id: WidgetId) -> Result<bool, ControlError> {
        let Some(action) = self.combos.actions.get(&id).cloned() else {
            return Ok(false);
        };
        let owner = self.combo_widget_owner(id).unwrap();
        if !self.rich_editable(owner) {
            return Ok(true);
        }
        match action {
            Action::Apply(_) => {
                if !matches!(
                    self.commit_combo_selection(owner)?,
                    CommitOutcome::Invalid(_)
                ) {
                    self.close_rich_popup(true)?;
                }
            }
            Action::Cancel(_) => {
                self.cancel_combo_selection(owner)?;
                self.close_rich_popup(true)?;
            }
            Action::Remove { key, .. } => {
                if self
                    .rich
                    .popup
                    .as_ref()
                    .is_some_and(|popup| popup.owner == owner)
                {
                    self.toggle_combo_draft(owner, &key)?;
                } else {
                    let keys: Vec<_> = self
                        .selected_keys(owner)
                        .unwrap()
                        .iter()
                        .filter(|candidate| **candidate != key)
                        .cloned()
                        .collect();
                    let entry = self.combos.entries.get_mut(&owner).unwrap();
                    entry.pending = Some(keys);
                    entry.pending_revision = entry.revision;
                    self.commit_combo_selection(owner)?;
                    self.sync_combo_presentation(owner)?;
                }
            }
        }
        Ok(true)
    }

    pub fn combo_item_template(&self, owner: WidgetId) -> Option<&ListItemTemplate> {
        self.combos.templates.get(&owner)
    }

    /// The popup uses the same readonly, bounded item template as ListBox.
    /// New rows receive fresh item data; a template owns no selection behavior.
    pub fn set_combo_item_template(
        &mut self,
        owner: WidgetId,
        template: Option<ListItemTemplate>,
    ) -> Result<(), ControlError> {
        self.combo_settings(owner)
            .ok_or_else(|| error("Not a combo editor"))?;
        if let Some(template) = &template {
            template.validate()?;
            let items = self.selection_items(owner).unwrap().to_vec();
            let width = self.runtime.bounds(owner).map_or(240.0, |r| r.width);
            for item in &items {
                self.preflight_item_template(template, item, width)?;
            }
        }
        if let Some(list) = self
            .rich
            .popup
            .as_ref()
            .filter(|p| p.owner == owner)
            .and_then(|p| p.list)
        {
            self.set_list_item_template(list, template.clone())?;
        }
        if let Some(template) = template {
            self.combos.templates.insert(owner, template);
        } else {
            self.combos.templates.remove(&owner);
        }
        Ok(())
    }

    pub(super) fn replace_multiple_combo_items(
        &mut self,
        owner: WidgetId,
        items: Vec<SelectionItem>,
    ) -> Result<(), ControlError> {
        let revision = self.combos.entries[&owner]
            .revision
            .checked_add(1)
            .ok_or_else(|| error("Combo source revision exhausted"))?;
        let previous = self.selected_keys(owner).unwrap().to_vec();
        let kept: Vec<_> = previous
            .iter()
            .filter(|key| items.iter().any(|item| &item.key == *key && item.enabled))
            .cloned()
            .collect();
        if previous != kept
            && let Some(validator) = self.rich.selection_validators.get(&owner)
        {
            validator.check(&kept).map_err(error)?;
        }
        if let Some(template) = self.combos.templates.get(&owner).cloned() {
            let width = self.runtime.bounds(owner).map_or(240.0, |r| r.width);
            for item in &items {
                self.preflight_item_template(&template, item, width)?;
            }
        }
        let pending = self.combo_selection_draft(owner).unwrap().to_vec();
        let selected: Vec<_> = items
            .iter()
            .filter(|item| kept.contains(&item.key) || pending.contains(&item.key))
            .cloned()
            .collect();
        self.preflight_combo_captions(&selected)?;
        let visible: Vec<_> = selected
            .iter()
            .filter(|item| item.enabled && pending.contains(&item.key))
            .cloned()
            .collect();
        self.preflight_combo_presentation(owner, self.combos.entries[&owner].settings, &visible)?;
        if let Some(popup) = self
            .rich
            .popup
            .as_ref()
            .filter(|popup| popup.owner == owner)
        {
            let list = popup.list.unwrap();
            let filtered = self.filter_editor_items(owner, &items, &popup.query);
            self.preflight_virtual_list_items(list, &filtered)?;
            // The existing virtual list validates its full candidate materialized
            // slice before replacing rows. No old popup is destroyed on failure.
            self.set_selection_items(list, filtered)?;
            self.rich.changes.retain(|change| change.id != list);
        }
        let entry = self.combos.entries.get_mut(&owner).unwrap();
        entry.revision = revision;
        if let Some(pending) = &mut entry.pending {
            pending.retain(|key| items.iter().any(|item| &item.key == key && item.enabled));
            entry.pending_revision = entry.revision;
        }
        if let CompositeKind::Combo {
            items: target,
            keys,
            ..
        } = &mut self.rich.composites.get_mut(&owner).unwrap().kind
        {
            *target = items;
            keys.clone_from(&kept);
        }
        if kept != previous {
            self.rich.changes.push(SelectionChanged {
                id: owner,
                keys: kept,
            });
        }
        self.sync_combo_presentation(owner)?;
        self.sync_combo_popup_selection(owner)?;
        Ok(())
    }

    pub(super) fn combo_before_input(&mut self, event: &InputEvent) -> Result<bool, ControlError> {
        let popup_owner = self
            .rich
            .popup
            .as_ref()
            .map(|popup| popup.owner)
            .filter(|owner| self.combo_multiple(*owner));
        if let Some(owner) = popup_owner {
            let editor = self.composite_editor(owner).unwrap();
            let composing = self
                .document(editor)
                .is_some_and(|doc| doc.composition().is_some());
            if matches!(
                event,
                InputEvent::KeyDown {
                    key: Key::Escape,
                    repeat: false,
                    ..
                }
            ) {
                if composing {
                    self.cancel_preedit();
                } else {
                    self.cancel_combo_selection(owner)?;
                    self.close_rich_popup(true)?;
                }
                return Ok(true);
            }
            let outside = match event {
                InputEvent::PointerDown { position, .. } => {
                    let popup = self.rich.popup.as_ref().unwrap();
                    !self
                        .runtime
                        .bounds(popup.surface)
                        .is_some_and(|r| r.contains(position.x, position.y))
                        && !self
                            .runtime
                            .bounds(owner)
                            .is_some_and(|r| r.contains(position.x, position.y))
                }
                InputEvent::WindowFocused(false) => true,
                InputEvent::KeyDown { key: Key::Tab, .. } => {
                    // Internal footer/list focus belongs to this editor session.
                    false
                }
                _ => false,
            };
            if outside {
                if composing {
                    self.cancel_preedit();
                } else if self.rich_editable(owner)
                    && self.combos.entries[&owner].settings.commit_on_focus_lost
                {
                    self.commit_combo_selection(owner)?;
                }
                self.close_rich_popup(false)?;
                return Ok(false);
            }
            if let InputEvent::KeyDown {
                key,
                modifiers,
                repeat: false,
            } = event
            {
                if composing {
                    return Ok(false);
                }
                let focused = self.runtime.focused();
                if *key == Key::Enter {
                    if let Some(id) = focused.filter(|id| self.combos.actions.contains_key(id)) {
                        self.activate_combo(id)?;
                    } else if !matches!(
                        self.commit_combo_selection(owner)?,
                        CommitOutcome::Invalid(_)
                    ) {
                        self.close_rich_popup(true)?;
                    }
                    return Ok(true);
                }
                if *key == Key::Space
                    && (modifiers.control
                        || !self.combo_settings(owner).unwrap().editable
                        || focused != Some(editor))
                {
                    let list = self.rich.popup.as_ref().and_then(|p| p.list).unwrap();
                    let state = &self.rich.lists[&list];
                    if let Some(item) = state.cursor.and_then(|index| state.items.get(index)) {
                        let key = item.key.clone();
                        self.toggle_combo_draft(owner, &key)?;
                    }
                    return Ok(true);
                }
            }
        }
        let InputEvent::KeyDown {
            key,
            modifiers,
            repeat: false,
        } = event
        else {
            return Ok(false);
        };
        if modifiers.alt || modifiers.control || modifiers.super_key {
            return Ok(false);
        }
        let Some(focused) = self.runtime.focused() else {
            return Ok(false);
        };
        let Some(owner) = self
            .rich_owner(focused)
            .filter(|owner| self.combo_multiple(*owner))
        else {
            return Ok(false);
        };
        let editor = self.composite_editor(owner).unwrap();
        if self
            .document(editor)
            .is_some_and(|doc| doc.composition().is_some())
        {
            return Ok(false);
        }
        let entry = &self.combos.entries[&owner];
        let tokens: Vec<_> = self
            .combo_selection_draft(owner)
            .unwrap()
            .iter()
            .filter_map(|key| entry.tokens.get(key).copied())
            .collect();
        let Some(index) = tokens.iter().position(|id| *id == focused) else {
            if focused == editor
                && self
                    .document(editor)
                    .is_some_and(|doc| doc.text().is_empty())
                && matches!(key, Key::ArrowLeft | Key::Backspace)
                && let Some(&token) = tokens.last()
            {
                self.runtime.apply(Mutation::Focus(Some(token)))?;
                self.bring_into_view(token)?;
                return Ok(true);
            }
            return Ok(false);
        };
        let target = match key {
            Key::ArrowLeft => tokens[index.saturating_sub(1)],
            Key::ArrowRight => tokens.get(index + 1).copied().unwrap_or(editor),
            Key::Home => tokens[0],
            Key::End => *tokens.last().unwrap(),
            Key::Delete | Key::Backspace => {
                self.activate_combo(focused)?;
                return Ok(true);
            }
            _ => return Ok(false),
        };
        self.runtime.apply(Mutation::Focus(Some(target)))?;
        self.bring_into_view(target)?;
        Ok(true)
    }
}
