//! Search composition with a retained suggestion list and UI-thread completions.
use super::search_model::SearchSession;
use super::*;
use rust_desktop_ui_core::{GridPlacement, GridTrack, Modifiers};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SearchBox {
    pub container: WidgetId,
    pub editor: WidgetId,
    pub clear_button: WidgetId,
    pub search_button: WidgetId,
    pub popup: Flyout,
    pub list: ListBox,
    pub status_label: WidgetId,
}
struct Entry {
    parts: SearchBox,
    session: SearchSession,
    initial_request: bool,
    wants_open: bool,
    was_open: bool,
    history_mode: bool,
    clear_width: f32,
    search_width: f32,
}
#[derive(Default)]
pub(super) struct SearchState {
    entries: HashMap<WidgetId, Entry>,
}

impl Controls {
    pub fn search_box(
        &mut self,
        parent: WidgetId,
        label: impl Into<String>,
        query: impl Into<String>,
        settings: SearchBoxSettings,
        mut style: LayoutStyle,
    ) -> Result<SearchBox, ControlError> {
        if self.searches.entries.len() >= 64 {
            return Err(search_error("At most 64 SearchBox controls are supported"));
        }
        let label = label.into();
        let query = query.into();
        let session = SearchSession::new(settings, &query)?;
        TextLimits::default().validate(&label).map_err(edit_error)?;
        self.preflight_new_editor_text(&query, &style, true)?;
        let height = self.theme.tokens().control_height;
        let search_width = self
            .measure_button_content(
                "Search",
                &ButtonStyle::default(),
                None,
                ButtonContentLayout::Leading,
                4096.0,
            )?
            .width
            .ceil()
            .max(height)
            + 2.0;
        let automatic_height = style.height == Length::Auto;
        style.kind = LayoutKind::Grid;
        style.grid_columns = vec![
            GridTrack::Fr(1.0),
            GridTrack::Px(height),
            GridTrack::Px(search_width),
        ];
        style.grid_rows = vec![GridTrack::Fr(1.0)];
        style.gap = 4.0;
        if style.height == Length::Auto {
            style.height = Length::Px(height);
        }
        let container = self.transparent_panel(parent, style)?;
        self.controls.get_mut(&container).unwrap().default_height = automatic_height;
        let result = (|| {
            self.set_role(container, Role::Group);
            self.set_automation_name(container, &label)?;
            let editor = self.edit_box(container, &label, &query, cell(1))?;
            self.set_editor_settings(
                editor,
                EditorSettings {
                    max_length: Some(settings.max_query_length),
                    commit_on_focus_lost: false,
                    ..Default::default()
                },
            )?;
            let clear_button = self.button(container, "Clear query", None, cell(2))?;
            self.set_button_icon(clear_button, Some(ButtonIcon::new(IconSymbol::Close)))?;
            self.set_button_content_layout(clear_button, ButtonContentLayout::IconOnly)?;
            let search_button = self.button(container, "Search", None, cell(3))?;
            let popup = self.flyout(editor, settings.popup_size)?;
            self.edit_props(popup.content, |p| {
                p.style.kind = LayoutKind::Grid;
                p.style.grid_columns = vec![GridTrack::Fr(1.0)];
                p.style.grid_rows = vec![GridTrack::Auto, GridTrack::Fr(1.0)];
            })?;
            let status_label = self.label(
                popup.content,
                "",
                LayoutStyle {
                    grid_row: GridPlacement {
                        start: Some(1),
                        span: 1,
                    },
                    ..Default::default()
                },
            )?;
            let list = self.list_box(
                popup.content,
                format!("{label} suggestions"),
                Vec::new(),
                SelectionSettings::default(),
                LayoutStyle {
                    width: Length::Percent(1.0),
                    height: Length::Percent(1.0),
                    grid_row: GridPlacement {
                        start: Some(2),
                        span: 1,
                    },
                    ..Default::default()
                },
            )?;
            let parts = SearchBox {
                container,
                editor,
                clear_button,
                search_button,
                popup,
                list,
                status_label,
            };
            self.searches.entries.insert(
                container,
                Entry {
                    parts,
                    session,
                    initial_request: true,
                    wants_open: false,
                    was_open: false,
                    history_mode: false,
                    clear_width: height,
                    search_width,
                },
            );
            Ok(parts)
        })();
        if result.is_err() {
            self.remove(container)?;
        }
        result
    }
    pub fn search_parts(&self, owner: WidgetId) -> Option<SearchBox> {
        Some(self.searches.entries.get(&owner)?.parts)
    }
    pub fn search_settings(&self, owner: WidgetId) -> Option<SearchBoxSettings> {
        Some(self.searches.entries.get(&owner)?.session.settings)
    }
    pub fn search_query(&self, owner: WidgetId) -> Option<&str> {
        self.document(self.search_parts(owner)?.editor)
            .map(TextDocument::text)
    }
    pub fn search_status(&self, owner: WidgetId) -> Option<&SearchStatus> {
        Some(&self.searches.entries.get(&owner)?.session.status)
    }
    pub fn search_history(&self, owner: WidgetId) -> Option<&[String]> {
        Some(&self.searches.entries.get(&owner)?.session.history)
    }
    pub fn search_suggestions(&self, owner: WidgetId) -> Option<&[SearchSuggestion]> {
        Some(&self.searches.entries.get(&owner)?.session.suggestions)
    }
    pub fn search_popup_is_open(&self, owner: WidgetId) -> bool {
        self.search_parts(owner)
            .is_some_and(|p| self.flyout_is_open(p.popup))
    }
    pub fn set_search_query(&mut self, owner: WidgetId, query: &str) -> Result<(), ControlError> {
        let parts = self
            .search_parts(owner)
            .ok_or_else(|| search_error("Not a SearchBox"))?;
        self.searches.entries[&owner]
            .session
            .settings
            .validate_query(query)?;
        self.set_text(parts.editor, query)?;
        self.sync_searches()
    }
    /// Drain at most one latest request per owner. The application executes its
    /// provider and later calls complete_search_suggestions on the UI thread.
    pub fn drain_search_suggestion_requests(&mut self) -> Vec<SearchSuggestionRequest> {
        self.searches
            .entries
            .values_mut()
            .filter_map(|entry| entry.session.take_request())
            .collect()
    }
    pub fn drain_search_events(&mut self) -> Vec<SearchEvent> {
        self.searches
            .entries
            .iter_mut()
            .flat_map(|(&owner, entry)| {
                std::mem::take(&mut entry.session.events)
                    .into_iter()
                    .map(move |kind| SearchEvent { owner, kind })
            })
            .collect()
    }
    pub fn request_search_suggestions(&mut self, owner: WidgetId) -> Result<(), ControlError> {
        self.sync_searches()?;
        if !self.search_active(owner) {
            return Ok(());
        }
        let query = self.search_query(owner).unwrap().to_owned();
        self.close_search_suggestions(owner)?;
        let entry = self.searches.entries.get_mut(&owner).unwrap();
        entry.initial_request = false;
        entry.history_mode = false;
        entry.session.start(owner, &query)
    }
    pub fn complete_search_suggestions(
        &mut self,
        request: &SearchSuggestionRequest,
        mut result: Result<Vec<SearchSuggestion>, String>,
    ) -> Result<SearchCompletion, ControlError> {
        self.sync_searches()?;
        let owner = request.owner();
        let Some(entry) = self.searches.entries.get(&owner) else {
            return Ok(SearchCompletion::Stale);
        };
        if !entry.session.matches(request) || !self.search_active(owner) {
            return Ok(SearchCompletion::Stale);
        }
        let parts = entry.parts;
        let result_validation = result
            .as_ref()
            .ok()
            .map(|values| entry.session.validate_results(values))
            .transpose();
        let validation = (|| {
            result_validation?;
            if let Ok(values) = &result {
                for value in values {
                    for text in std::iter::once(&value.text).chain(value.description.iter()) {
                        self.preflight_new_editor_text(
                            text,
                            &LayoutStyle {
                                width: Length::Px(4096.0),
                                ..Default::default()
                            },
                            true,
                        )?;
                    }
                    self.preflight_editor_text(parts.editor, &value.query)?;
                    editor_limits(self.controls[&parts.editor].editor.max_length)
                        .validate(&value.query)
                        .map_err(edit_error)?;
                }
            }
            Ok::<_, ControlError>(())
        })();
        if let Err(error) = validation {
            result = Err(error.to_string());
        }
        if let Err(message) = &result
            && (message.len() > 4096
                || message.contains('\0')
                || self
                    .preflight_new_editor_text(message, &LayoutStyle::default(), true)
                    .is_err())
        {
            result = Err("The suggestion provider returned an invalid error message".into());
        }
        // Candidate ListBox source preflight occurs before publishing provider state.
        if let Ok(values) = &result {
            let items = suggestion_items(values);
            if let Err(error) = self.preflight_virtual_list_items(parts.list.container, &items) {
                result = Err(error.to_string());
            }
        }
        if let Ok(values) = &result {
            // Reject application validators before either source or session changes.
            if let Some(validator) = self.rich.selection_validators.get(&parts.list.container) {
                validator.check(&[]).map_err(ControlError::Invalid)?;
            }
            self.set_selection_items(parts.list.container, suggestion_items(values))?;
            self.set_selected_keys(parts.list.container, &[])?;
        }
        let completion = self
            .searches
            .entries
            .get_mut(&owner)
            .unwrap()
            .session
            .complete(request, result)?;
        if completion != SearchCompletion::Stale {
            self.render_search_results(owner)?;
            self.searches.entries.get_mut(&owner).unwrap().wants_open = true;
            self.open_search_if_ready(owner)?;
        }
        Ok(completion)
    }
    pub fn choose_search_suggestion(
        &mut self,
        owner: WidgetId,
        key: &str,
    ) -> Result<(), ControlError> {
        self.sync_searches()?;
        if !self.search_active(owner) {
            return Ok(());
        }
        let entry = self
            .searches
            .entries
            .get(&owner)
            .ok_or_else(|| search_error("Not a SearchBox"))?;
        let candidate = entry.session.candidate(key)?;
        let parts = entry.parts;
        self.preflight_editor_text(parts.editor, &candidate.query)?;
        self.set_text(parts.editor, &candidate.query)?;
        self.searches
            .entries
            .get_mut(&owner)
            .unwrap()
            .session
            .choose(candidate);
        self.close_search_suggestions(owner)?;
        self.runtime.apply(Mutation::Focus(Some(parts.editor)))?;
        self.synchronize_editor_focus()
    }
    pub fn submit_search(&mut self, owner: WidgetId) -> Result<(), ControlError> {
        if !self.search_editable(owner) {
            return Ok(());
        }
        let query = self
            .search_query(owner)
            .ok_or_else(|| search_error("Not a SearchBox"))?
            .to_owned();
        self.searches
            .entries
            .get_mut(&owner)
            .unwrap()
            .session
            .search(&query)?;
        self.close_search_suggestions(owner)
    }
    pub fn clear_search_query(&mut self, owner: WidgetId) -> Result<(), ControlError> {
        if !self.search_editable(owner) {
            return Ok(());
        }
        self.set_search_query(owner, "")
    }
    pub fn set_search_history(
        &mut self,
        owner: WidgetId,
        values: Vec<String>,
    ) -> Result<(), ControlError> {
        let editor = self
            .search_parts(owner)
            .ok_or_else(|| search_error("Not a SearchBox"))?
            .editor;
        let settings = self.searches.entries[&owner].session.settings;
        if values.len() > settings.history_limit {
            return Err(search_error("Search history exceeds its limit"));
        }
        for value in &values {
            settings.validate_query(value)?;
            self.preflight_editor_text(editor, value)?;
        }
        self.searches
            .entries
            .get_mut(&owner)
            .ok_or_else(|| search_error("Not a SearchBox"))?
            .session
            .set_history(values)
    }
    pub fn open_search_history(&mut self, owner: WidgetId) -> Result<(), ControlError> {
        if !self.search_active(owner) {
            return Ok(());
        }
        let entry = &self.searches.entries[&owner];
        let query = self
            .document(entry.parts.editor)
            .expect("live SearchBox editor")
            .text()
            .to_owned();
        entry.session.settings.validate_query(&query)?;
        let values: Vec<_> = entry
            .session
            .history
            .iter()
            .take(entry.session.settings.max_suggestions)
            .enumerate()
            .map(|(index, query)| SearchSuggestion::new(format!("history-{index}"), query))
            .collect();
        let list = entry.parts.list.container;
        self.preflight_virtual_list_items(list, &suggestion_items(&values))?;
        if let Some(validator) = self.rich.selection_validators.get(&list) {
            validator.check(&[]).map_err(ControlError::Invalid)?;
        }
        self.close_search_suggestions(owner)?;
        let entry = self
            .searches
            .entries
            .get_mut(&owner)
            .ok_or_else(|| search_error("Not a SearchBox"))?;
        entry.session.suggestions = values;
        // An explicit history request consumes focus-entry initialization.
        // Otherwise the next frame starts the normal query and closes history.
        entry.session.query = query;
        entry.initial_request = false;
        entry.session.status = SearchStatus::Ready;
        entry.history_mode = true;
        entry.wants_open = true;
        self.render_search_results(owner)?;
        self.open_search_if_ready(owner)
    }
    pub fn close_search_suggestions(&mut self, owner: WidgetId) -> Result<(), ControlError> {
        let parts = self
            .search_parts(owner)
            .ok_or_else(|| search_error("Not a SearchBox"))?;
        let entry = self.searches.entries.get_mut(&owner).unwrap();
        entry.session.cancel();
        entry.wants_open = false;
        entry.was_open = false;
        if self
            .runtime
            .focused()
            .is_some_and(|id| self.flyout_descendant(id, parts.popup.container))
        {
            self.close_flyout(parts.popup)
        } else {
            self.close_flyout_preserving_focus(parts.popup)
        }
    }
    fn search_editable(&self, owner: WidgetId) -> bool {
        self.search_parts(owner).is_some_and(|parts| {
            self.window_focused
                && self.is_enabled(owner)
                && self.is_text_editable(parts.editor)
                && self.runtime.tree().is_attached(owner)
        })
    }
    pub(super) fn search_button_blocked(&self, id: WidgetId) -> bool {
        self.searches
            .entries
            .values()
            .find(|entry| id == entry.parts.clear_button || id == entry.parts.search_button)
            .is_some_and(|entry| !self.is_text_editable(entry.parts.editor))
    }
    fn search_active(&self, owner: WidgetId) -> bool {
        self.search_editable(owner)
            && self.search_parts(owner).is_some_and(|parts| {
                self.runtime.focused().is_some_and(|id| {
                    self.flyout_descendant(id, owner)
                        || self.flyout_descendant(id, parts.popup.container)
                })
            })
    }
    fn render_search_results(&mut self, owner: WidgetId) -> Result<(), ControlError> {
        let entry = &self.searches.entries[&owner];
        let parts = entry.parts;
        let (items, message) = match &entry.session.status {
            SearchStatus::Ready => (
                suggestion_items(&entry.session.suggestions),
                if entry.session.suggestions.is_empty() {
                    "No suggestions".into()
                } else if entry.history_mode {
                    "Recent searches".into()
                } else {
                    String::new()
                },
            ),
            SearchStatus::Failed(message) => (Vec::new(), message.clone()),
            SearchStatus::Pending => (Vec::new(), "Searching…".into()),
            SearchStatus::Idle => (Vec::new(), String::new()),
        };
        let ready = entry.session.status == SearchStatus::Ready;
        if ready {
            self.set_selection_items(parts.list.container, items)?;
            self.set_selected_keys(parts.list.container, &[])?;
        }
        self.edit_props(parts.list.container, |props| {
            props.visible = ready;
            props.collapsed = !ready;
        })?;
        self.rich
            .lists
            .get_mut(&parts.list.container)
            .unwrap()
            .cursor = None;
        self.set_label(parts.status_label, message)?;
        self.scene_dirty = true;
        Ok(())
    }
    fn open_search_if_ready(&mut self, owner: WidgetId) -> Result<(), ControlError> {
        let parts = self.searches.entries[&owner].parts;
        if !self.searches.entries[&owner].wants_open
            || !self.search_active(owner)
            || self
                .document(parts.editor)
                .is_some_and(|d| d.composition().is_some())
            || self
                .runtime
                .bounds(parts.editor)
                .zip(self.runtime.clip(parts.editor))
                .and_then(|(bounds, clip)| bounds.intersection(clip))
                .is_none()
        {
            return Ok(());
        }
        self.preflight_open_flyout(parts.popup)?;
        self.searches.entries.get_mut(&owner).unwrap().wants_open = false;
        self.open_flyout_preserving_focus(parts.popup)?;
        self.searches.entries.get_mut(&owner).unwrap().was_open = true;
        Ok(())
    }
    pub(super) fn sync_searches(&mut self) -> Result<(), ControlError> {
        self.prune_searches()?;
        let owners: Vec<_> = self.searches.entries.keys().copied().collect();
        for owner in owners {
            let parts = self.searches.entries[&owner].parts;
            let height = self.theme.tokens().control_height;
            let old_width = self.searches.entries[&owner].clear_width;
            let old_search_width = self.searches.entries[&owner].search_width;
            let label = self.label_text(parts.search_button).unwrap().to_owned();
            let button_style = *self.button_style(parts.search_button).unwrap();
            let search_width = self
                .measure_button_content(
                    &label,
                    &button_style,
                    self.button_icon(parts.search_button).copied(),
                    self.button_content_layout(parts.search_button).unwrap(),
                    4096.0,
                )?
                .width
                .ceil()
                .max(height)
                + 2.0;
            if height != old_width || search_width != old_search_width {
                self.edit_props(owner, |props| {
                    if props.style.grid_columns.get(1) == Some(&GridTrack::Px(old_width)) {
                        props.style.grid_columns[1] = GridTrack::Px(height);
                    }
                    if props.style.grid_columns.get(2) == Some(&GridTrack::Px(old_search_width)) {
                        props.style.grid_columns[2] = GridTrack::Px(search_width);
                    }
                })?;
                self.searches.entries.get_mut(&owner).unwrap().clear_width = height;
                self.searches.entries.get_mut(&owner).unwrap().search_width = search_width;
            }
            let query = self.document(parts.editor).unwrap().text().to_owned();
            let composing = self.document(parts.editor).unwrap().composition().is_some();
            if !self.search_active(owner) {
                self.close_search_suggestions(owner)?;
                self.searches
                    .entries
                    .get_mut(&owner)
                    .unwrap()
                    .initial_request = true;
                continue;
            }
            let changed = self.searches.entries[&owner].session.query != query;
            let initial = self.searches.entries[&owner].initial_request;
            if !composing && (changed || initial) {
                self.close_search_suggestions(owner)?;
                let entry = self.searches.entries.get_mut(&owner).unwrap();
                entry.initial_request = false;
                entry.history_mode = false;
                entry.session.start(owner, &query)?;
            }
            if self.searches.entries[&owner].was_open && !self.flyout_is_open(parts.popup) {
                self.search_will_close(parts.popup.container);
            }
            self.open_search_if_ready(owner)?;
        }
        Ok(())
    }
    pub(super) fn preflight_search_text(
        &self,
        editor: WidgetId,
        text: &str,
    ) -> Result<(), ControlError> {
        if (text.len() > MAX_SEARCH_QUERY_BYTES || text.contains('\0'))
            && self
                .searches
                .entries
                .values()
                .any(|entry| entry.parts.editor == editor)
        {
            return Err(search_error(
                "Search text exceeds 65536 UTF-8 bytes or contains NUL",
            ));
        }
        Ok(())
    }
    pub(super) fn preflight_search_settings(
        &self,
        editor: WidgetId,
        settings: &EditorSettings,
    ) -> Result<(), ControlError> {
        if let Some(entry) = self
            .searches
            .entries
            .values()
            .find(|entry| entry.parts.editor == editor)
            && (settings
                .max_length
                .is_none_or(|maximum| maximum > entry.session.settings.max_query_length)
                || settings.commit_on_focus_lost)
        {
            return Err(search_error(
                "SearchBox keeps a bounded Query draft and does not commit on focus loss",
            ));
        }
        Ok(())
    }
    pub(super) fn search_will_close(&mut self, popup: WidgetId) {
        for entry in self
            .searches
            .entries
            .values_mut()
            .filter(|entry| entry.parts.popup.container == popup)
        {
            entry.session.cancel();
            entry.wants_open = false;
            entry.was_open = false;
        }
    }
    pub(super) fn preflight_search_removal(&self, id: WidgetId) -> Result<(), ControlError> {
        if self.searches.entries.values().any(|entry| {
            let p = entry.parts;
            [
                p.editor,
                p.clear_button,
                p.search_button,
                p.popup.container,
                p.popup.content,
                p.list.container,
                p.list.content,
                p.status_label,
            ]
            .contains(&id)
        }) {
            return Err(search_error(
                "Remove the SearchBox owner, not its required parts",
            ));
        }
        Ok(())
    }
    pub(super) fn prune_searches(&mut self) -> Result<(), ControlError> {
        let invalid: Vec<_> = self
            .searches
            .entries
            .iter()
            .filter_map(|(&owner, entry)| {
                let p = entry.parts;
                [
                    p.container,
                    p.editor,
                    p.clear_button,
                    p.search_button,
                    p.popup.container,
                    p.popup.content,
                    p.list.container,
                    p.list.content,
                    p.status_label,
                ]
                .iter()
                .any(|id| !self.runtime.tree().contains(*id))
                .then_some(owner)
            })
            .collect();
        for owner in invalid {
            if let Some(entry) = self.searches.entries.remove(&owner)
                && self.runtime.tree().contains(entry.parts.popup.container)
            {
                self.runtime
                    .apply(Mutation::Remove(entry.parts.popup.container))?;
            }
        }
        Ok(())
    }
    pub(super) fn activate_search(&mut self, id: WidgetId) -> Result<bool, ControlError> {
        if let Some((owner, clear)) = self.searches.entries.iter().find_map(|(&owner, e)| {
            if e.parts.clear_button == id {
                Some((owner, true))
            } else if e.parts.search_button == id {
                Some((owner, false))
            } else {
                None
            }
        }) {
            if clear {
                self.clear_search_query(owner)?;
            } else {
                self.submit_search(owner)?;
            }
            if self.search_editable(owner) {
                self.runtime.apply(Mutation::Focus(Some(
                    self.searches.entries[&owner].parts.editor,
                )))?;
                self.synchronize_editor_focus()?;
            }
            return Ok(true);
        }
        let Some(selection::RichAction::Select { owner: list, key }) = self.rich.actions.get(&id)
        else {
            return Ok(false);
        };
        let key = key.clone();
        let owner = self
            .searches
            .entries
            .iter()
            .find_map(|(&owner, e)| (e.parts.list.container == *list).then_some(owner));
        if let Some(owner) = owner {
            self.choose_search_suggestion(owner, &key)?;
            return Ok(true);
        }
        Ok(false)
    }
    pub(super) fn search_before_input(&mut self, event: &InputEvent) -> Result<bool, ControlError> {
        if matches!(event, InputEvent::WindowFocused(false)) {
            let owners: Vec<_> = self.searches.entries.keys().copied().collect();
            for owner in owners {
                self.close_search_suggestions(owner)?;
            }
            return Ok(false);
        }
        let Some(focused) = self.runtime.focused() else {
            return Ok(false);
        };
        let Some(owner) = self.searches.entries.iter().find_map(|(&owner, entry)| {
            (self.flyout_descendant(focused, owner)
                || self.flyout_descendant(focused, entry.parts.popup.container))
            .then_some(owner)
        }) else {
            return Ok(false);
        };
        let parts = self.searches.entries[&owner].parts;
        let InputEvent::KeyDown {
            key,
            modifiers,
            repeat,
        } = event
        else {
            return Ok(false);
        };
        if *key == Key::Escape && *modifiers == Modifiers::default() {
            if !repeat {
                if self
                    .document(parts.editor)
                    .is_some_and(|d| d.composition().is_some())
                {
                    self.cancel_preedit();
                } else {
                    self.close_search_suggestions(owner)?;
                }
            }
            return Ok(true);
        }
        if focused == parts.clear_button || focused == parts.search_button {
            return Ok(false);
        }
        if self
            .document(parts.editor)
            .is_some_and(|d| d.composition().is_some())
        {
            return Ok(false);
        }
        if *key == Key::Tab {
            self.close_search_suggestions(owner)?;
            return Ok(false);
        }
        if *key == Key::Enter && !modifiers.alt && !modifiers.super_key {
            if !repeat {
                let selected = self
                    .selected_keys(parts.list.container)
                    .and_then(|keys| keys.first())
                    .cloned();
                if !modifiers.control
                    && self.search_popup_is_open(owner)
                    && let Some(key) = selected
                {
                    self.choose_search_suggestion(owner, &key)?;
                } else {
                    self.submit_search(owner)?;
                }
            }
            return Ok(true);
        }
        if matches!(key, Key::ArrowDown | Key::ArrowUp)
            && !modifiers.control
            && !modifiers.super_key
        {
            if !self.search_popup_is_open(owner) {
                if modifiers.alt || self.search_query(owner).is_some_and(str::is_empty) {
                    self.open_search_history(owner)?;
                } else {
                    self.request_search_suggestions(owner)?;
                }
                return Ok(true);
            }
            let state = self.rich.lists[&parts.list.container].clone();
            let enabled: Vec<_> = state
                .items
                .iter()
                .enumerate()
                .filter_map(|(i, item)| item.enabled.then_some(i))
                .collect();
            if enabled.is_empty() {
                return Ok(true);
            }
            let current = state
                .cursor
                .and_then(|i| enabled.iter().position(|n| *n == i));
            let index = if *key == Key::ArrowDown {
                enabled[current.map_or(0, |i| (i + 1).min(enabled.len() - 1))]
            } else {
                enabled[current.map_or(enabled.len() - 1, |i| i.saturating_sub(1))]
            };
            self.set_selected_keys(parts.list.container, &[state.items[index].key.clone()])?;
            self.rich
                .lists
                .get_mut(&parts.list.container)
                .unwrap()
                .cursor = Some(index);
            self.virtual_ensure_selection_visible(parts.list.container, index)?;
            self.materialize_list_rows(parts.list.container)?;
            self.scene_dirty = true;
            return Ok(true);
        }
        Ok(false)
    }
}
fn suggestion_items(values: &[SearchSuggestion]) -> Vec<SelectionItem> {
    values
        .iter()
        .map(|value| SelectionItem {
            key: value.key.clone(),
            text: value.text.clone(),
            columns: value.description.iter().cloned().collect(),
            enabled: value.enabled,
        })
        .collect()
}
fn cell(column: u16) -> LayoutStyle {
    LayoutStyle {
        width: Length::Percent(1.0),
        height: Length::Percent(1.0),
        grid_column: GridPlacement {
            start: Some(column),
            span: 1,
        },
        ..Default::default()
    }
}
fn search_error(message: &str) -> ControlError {
    ControlError::Invalid(message.into())
}
