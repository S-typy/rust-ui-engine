use super::*;
use rust_desktop_ui_core::Modifiers;

impl Controls {
    pub(super) fn check_breadcrumb_parts(&self, owner: WidgetId) -> Result<(), ControlError> {
        let e = self
            .breadcrumbs
            .entries
            .get(&owner)
            .ok_or_else(|| invalid("Not a Breadcrumb"))?;
        let p = e.parts;
        if required(e)
            .iter()
            .any(|id| !self.runtime.tree().contains(*id))
        {
            return Err(invalid("Breadcrumb required content was removed"));
        }
        if [
            (e.bar, owner),
            (p.editor, owner),
            (p.overflow_button, e.bar),
            (p.history_button, e.bar),
            (p.edit_button, e.bar),
            (p.popup.container, self.overlay),
            (p.popup.content, p.popup.container),
            (e.status, p.popup.content),
            (p.list.container, p.popup.content),
            (p.list.content, p.list.viewport),
        ]
        .into_iter()
        .chain(
            e.segments
                .iter()
                .flat_map(|segment| [(segment.button, e.bar), (segment.children, e.bar)]),
        )
        .any(|(child, parent)| self.runtime.tree().parent(child) != Some(parent))
        {
            return Err(invalid("Breadcrumb required content was reparented"));
        }
        let editor = self.editor_control(p.editor)?;
        if editor.editor.max_length != Some(1024)
            || editor.editor.commit_on_focus_lost
            || editor.validation.is_required
            || editor.validator.is_some()
            || self.has_async_editor_validator(p.editor)
            || self.editor_mask_settings(p.editor).is_some()
            || self
                .rich
                .selection_validators
                .contains_key(&p.list.container)
            || self.rich.lists[&p.list.container].settings != SelectionSettings::default()
            || self.has_custom_list_content(p.list.container)
        {
            return Err(invalid(
                "Breadcrumb owns its editor and suggestion-list recipes",
            ));
        }
        Ok(())
    }
    pub(super) fn breadcrumb_active(&self, owner: WidgetId) -> bool {
        self.breadcrumbs.entries.get(&owner).is_some_and(|e| {
            self.window_focused
                && self.is_enabled(owner)
                && !e.session.settings.read_only
                && self.runtime.tree().is_attached(owner)
        })
    }
    pub fn begin_breadcrumb_edit(&mut self, owner: WidgetId) -> Result<(), ControlError> {
        self.check_breadcrumb_parts(owner)?;
        if !self.breadcrumb_active(owner)
            || !self.breadcrumbs.entries[&owner].session.settings.editable
        {
            return Ok(());
        }
        self.close_breadcrumb_popup(owner, false)?;
        let e = self.breadcrumbs.entries.get_mut(&owner).unwrap();
        e.session.cancel();
        e.editing = true;
        let editor = e.parts.editor;
        self.show_breadcrumb_editor(owner, true)?;
        self.runtime.apply(Mutation::Focus(Some(editor)))?;
        self.synchronize_editor_focus()?;
        Ok(())
    }
    pub fn set_breadcrumb_draft(
        &mut self,
        owner: WidgetId,
        text: impl Into<String>,
    ) -> Result<(), ControlError> {
        let text = text.into();
        self.check_breadcrumb_parts(owner)?;
        super::super::breadcrumb_model::validate_text(&text, 1024, 4096, true)?;
        let editor = self.breadcrumbs.entries[&owner].parts.editor;
        self.preflight_editor_text(editor, &text)?;
        if !self.breadcrumb_active(owner)
            || !self.breadcrumbs.entries[&owner].session.settings.editable
        {
            return Err(invalid("Breadcrumb is not editable"));
        }
        if self.document(editor).unwrap().composition().is_some() {
            return Err(invalid(
                "Finish breadcrumb composition before replacing its draft",
            ));
        }
        self.begin_breadcrumb_edit(owner)?;
        self.set_text(editor, &text)?;
        self.breadcrumbs.entries.get_mut(&owner).unwrap().draft = text;
        Ok(())
    }
    pub fn submit_breadcrumb(&mut self, owner: WidgetId) -> Result<(), ControlError> {
        self.check_breadcrumb_parts(owner)?;
        if !self.breadcrumb_active(owner)
            || !self.breadcrumbs.entries[&owner].session.settings.editable
        {
            return Ok(());
        }
        let editor = self.breadcrumbs.entries[&owner].parts.editor;
        if !self.is_text_editable(editor) {
            return Ok(());
        }
        if self.document(editor).unwrap().composition().is_some() {
            return Ok(());
        }
        let text = self.document(editor).unwrap().text().to_owned();
        self.breadcrumbs
            .entries
            .get_mut(&owner)
            .unwrap()
            .session
            .navigate(owner, BreadcrumbNavigationTarget::Text(text.clone()))?;
        self.breadcrumbs.entries.get_mut(&owner).unwrap().draft = text;
        self.controls.get_mut(&editor).unwrap().validation_error = None;
        self.close_breadcrumb_popup(owner, false)?;
        self.scene_dirty = true;
        Ok(())
    }
    pub fn cancel_breadcrumb(&mut self, owner: WidgetId) -> Result<(), ControlError> {
        self.check_breadcrumb_parts(owner)?;
        let p = self.breadcrumbs.entries[&owner].parts;
        if self.document(p.editor).unwrap().composition().is_some() {
            self.cancel_preedit();
        }
        self.close_breadcrumb_popup(owner, false)?;
        let accepted = self.breadcrumbs.entries[&owner].session.path.text.clone();
        self.set_editor_value(p.editor, &accepted)?;
        let e = self.breadcrumbs.entries.get_mut(&owner).unwrap();
        e.session.cancel();
        e.editing = false;
        e.draft = accepted;
        self.show_breadcrumb_editor(owner, false)?;
        self.focus_breadcrumb_end(owner)
    }
    pub fn request_breadcrumb_segment(
        &mut self,
        owner: WidgetId,
        key: &str,
    ) -> Result<(), ControlError> {
        self.check_breadcrumb_parts(owner)?;
        if !self.breadcrumb_active(owner) {
            return Ok(());
        }
        let e = &self.breadcrumbs.entries[&owner];
        let index = e
            .session
            .path
            .segments
            .iter()
            .position(|s| s.key == key)
            .ok_or_else(|| invalid("Unknown breadcrumb segment"))?;
        let keys = e.session.path.segments[..=index]
            .iter()
            .map(|s| s.key.clone())
            .collect();
        self.breadcrumbs
            .entries
            .get_mut(&owner)
            .unwrap()
            .session
            .navigate(owner, BreadcrumbNavigationTarget::Segment(keys))?;
        self.close_breadcrumb_popup(owner, true)
    }
    pub fn request_breadcrumb_children(
        &mut self,
        owner: WidgetId,
        key: &str,
    ) -> Result<(), ControlError> {
        self.check_breadcrumb_parts(owner)?;
        if !self.breadcrumb_active(owner) {
            return Ok(());
        }
        let e = &self.breadcrumbs.entries[&owner];
        if !e
            .session
            .path
            .segments
            .iter()
            .any(|s| s.key == key && s.has_children)
        {
            return Err(invalid("Breadcrumb segment has no declared children"));
        }
        e.session.preflight_request()?;
        self.open_breadcrumb_items(owner, PopupMode::Children, vec![], "Загрузка…")?;
        self.breadcrumbs
            .entries
            .get_mut(&owner)
            .unwrap()
            .session
            .request_children(owner, key)
    }
    pub fn open_breadcrumb_overflow(&mut self, owner: WidgetId) -> Result<(), ControlError> {
        self.check_breadcrumb_parts(owner)?;
        if !self.breadcrumb_active(owner) {
            return Ok(());
        }
        let items = self.breadcrumbs.entries[&owner]
            .session
            .path
            .segments
            .iter()
            .map(|s| SelectionItem::new(&s.key, &s.label))
            .collect();
        self.open_breadcrumb_items(owner, PopupMode::Overflow, items, "Уровни пути")
    }
    pub fn open_breadcrumb_history(&mut self, owner: WidgetId) -> Result<(), ControlError> {
        self.check_breadcrumb_parts(owner)?;
        if !self.breadcrumb_active(owner) {
            return Ok(());
        }
        let items = self.breadcrumbs.entries[&owner]
            .session
            .history
            .iter()
            .enumerate()
            .map(|(i, p)| SelectionItem::new(i.to_string(), &p.text))
            .collect();
        self.open_breadcrumb_items(owner, PopupMode::History, items, "История переходов")
    }
    fn open_breadcrumb_items(
        &mut self,
        owner: WidgetId,
        mode: PopupMode,
        items: Vec<SelectionItem>,
        status: &str,
    ) -> Result<(), ControlError> {
        let e = &self.breadcrumbs.entries[&owner];
        let p = e.parts;
        let label = e.status;
        self.preflight_open_flyout(p.popup)?;
        self.preflight_virtual_list_items(p.list.container, &items)?;
        self.set_selection_items(p.list.container, items)?;
        self.set_selected_keys(p.list.container, &[])?;
        self.set_label(label, status)?;
        self.breadcrumbs
            .entries
            .get_mut(&owner)
            .unwrap()
            .session
            .cancel();
        self.breadcrumbs.entries.get_mut(&owner).unwrap().popup_mode = Some(mode);
        self.open_flyout(p.popup)?;
        self.breadcrumbs.entries.get_mut(&owner).unwrap().was_open = true;
        Ok(())
    }
    pub fn choose_breadcrumb_item(
        &mut self,
        owner: WidgetId,
        key: &str,
    ) -> Result<(), ControlError> {
        self.check_breadcrumb_parts(owner)?;
        if !self.breadcrumb_active(owner) {
            return Ok(());
        }
        let e = &self.breadcrumbs.entries[&owner];
        if !self.flyout_is_open(e.parts.popup) {
            return Err(invalid("Breadcrumb popup is closed"));
        }
        let target = match e.popup_mode {
            Some(PopupMode::Overflow) => return self.request_breadcrumb_segment(owner, key),
            Some(PopupMode::History) => {
                let index = key
                    .parse::<usize>()
                    .ok()
                    .filter(|i| i.to_string() == key)
                    .ok_or_else(|| invalid("Invalid breadcrumb history key"))?;
                BreadcrumbNavigationTarget::History(
                    e.session
                        .history
                        .get(index)
                        .cloned()
                        .ok_or_else(|| invalid("Stale breadcrumb history key"))?,
                )
            }
            Some(PopupMode::Children) => BreadcrumbNavigationTarget::Child {
                parent: e.session.children_parent.clone(),
                key: key.into(),
            },
            None => return Err(invalid("Breadcrumb popup is closed")),
        };
        self.breadcrumbs
            .entries
            .get_mut(&owner)
            .unwrap()
            .session
            .navigate(owner, target)?;
        self.close_breadcrumb_popup(owner, true)
    }
    pub(super) fn close_breadcrumb_popup(
        &mut self,
        owner: WidgetId,
        restore: bool,
    ) -> Result<(), ControlError> {
        let e = self
            .breadcrumbs
            .entries
            .get_mut(&owner)
            .ok_or_else(|| invalid("Not a Breadcrumb"))?;
        e.was_open = false;
        e.popup_mode = None;
        let popup = e.parts.popup;
        if restore {
            self.close_flyout(popup)
        } else {
            self.close_flyout_preserving_focus(popup)
        }
    }
    pub fn set_breadcrumb_settings(
        &mut self,
        owner: WidgetId,
        settings: BreadcrumbSettings,
    ) -> Result<(), ControlError> {
        settings.validate()?;
        self.check_breadcrumb_parts(owner)?;
        let e = &self.breadcrumbs.entries[&owner];
        if settings == e.session.settings {
            return Ok(());
        }
        if settings.source != e.session.settings.source
            && self.breadcrumb_draft(owner) != Some(e.session.path.text.as_str())
        {
            return Err(invalid(
                "Cancel the breadcrumb draft before changing its source",
            ));
        }
        let p = e.parts;
        self.set_flyout_placement(p.popup, settings.popup_size, FlyoutPlacement::Below)?;
        self.close_breadcrumb_popup(owner, true)?;
        let e = self.breadcrumbs.entries.get_mut(&owner).unwrap();
        e.session.cancel();
        if settings.source != e.session.settings.source {
            e.session.history.clear();
            e.session.children.clear();
        }
        e.session.settings = settings;
        e.session.history.truncate(settings.history_limit);
        self.set_read_only(p.editor, settings.read_only || !settings.editable)?;
        self.apply_breadcrumb_availability(owner)
    }
    pub fn set_breadcrumb_read_only(
        &mut self,
        owner: WidgetId,
        read_only: bool,
    ) -> Result<(), ControlError> {
        let mut settings = self
            .breadcrumb_settings(owner)
            .ok_or_else(|| invalid("Not a Breadcrumb"))?;
        settings.read_only = read_only;
        self.set_breadcrumb_settings(owner, settings)
    }
    pub(super) fn focus_breadcrumb_end(&mut self, owner: WidgetId) -> Result<(), ControlError> {
        let e = &self.breadcrumbs.entries[&owner];
        let target = e.segments.last().map_or(e.parts.edit_button, |s| s.button);
        if self.is_enabled(target) {
            self.runtime.apply(Mutation::Focus(Some(target)))?;
            self.synchronize_editor_focus()?;
        }
        Ok(())
    }
    pub(in crate::controls) fn activate_breadcrumb(
        &mut self,
        id: WidgetId,
    ) -> Result<bool, ControlError> {
        let found = self.breadcrumbs.entries.iter().find_map(|(&owner, e)| {
            if id == e.parts.overflow_button {
                Some((owner, 0, String::new()))
            } else if id == e.parts.history_button {
                Some((owner, 1, String::new()))
            } else if id == e.parts.edit_button {
                Some((owner, 2, String::new()))
            } else {
                e.segments.iter().find_map(|s| {
                    if id == s.button {
                        Some((owner, 3, s.key.clone()))
                    } else if id == s.children {
                        Some((owner, 4, s.key.clone()))
                    } else {
                        None
                    }
                })
            }
        });
        if let Some((owner, kind, key)) = found {
            match kind {
                0 => self.open_breadcrumb_overflow(owner)?,
                1 => self.open_breadcrumb_history(owner)?,
                2 => self.begin_breadcrumb_edit(owner)?,
                3 => self.request_breadcrumb_segment(owner, &key)?,
                _ => self.request_breadcrumb_children(owner, &key)?,
            }
            return Ok(true);
        }
        if let Some(selection::RichAction::Select { owner: list, key }) = self.rich.actions.get(&id)
            && let Some(owner) = self
                .breadcrumbs
                .entries
                .iter()
                .find_map(|(&owner, e)| (e.parts.list.container == *list).then_some(owner))
        {
            let key = key.clone();
            self.choose_breadcrumb_item(owner, &key)?;
            return Ok(true);
        }
        Ok(false)
    }
    pub(in crate::controls) fn breadcrumb_before_input(
        &mut self,
        event: &InputEvent,
    ) -> Result<bool, ControlError> {
        if matches!(event, InputEvent::WindowFocused(false)) {
            let owners: Vec<_> = self.breadcrumbs.entries.keys().copied().collect();
            for owner in owners {
                self.breadcrumbs
                    .entries
                    .get_mut(&owner)
                    .unwrap()
                    .session
                    .cancel();
                self.close_breadcrumb_popup(owner, false)?;
            }
            return Ok(false);
        }
        let Some(focused) = self.runtime.focused() else {
            return Ok(false);
        };
        let Some(owner) = self.breadcrumbs.entries.iter().find_map(|(&owner, e)| {
            (self.flyout_descendant(focused, owner)
                || self.flyout_descendant(focused, e.parts.popup.container))
            .then_some(owner)
        }) else {
            return Ok(false);
        };
        let InputEvent::KeyDown {
            key,
            modifiers,
            repeat,
        } = event
        else {
            return Ok(false);
        };
        let p = self.breadcrumbs.entries[&owner].parts;
        if *key == Key::Escape && *modifiers == Modifiers::default() {
            if !repeat {
                if self.document(p.editor).unwrap().composition().is_some() {
                    self.cancel_preedit();
                } else if self.flyout_is_open(p.popup) {
                    self.breadcrumbs
                        .entries
                        .get_mut(&owner)
                        .unwrap()
                        .session
                        .cancel();
                    self.close_breadcrumb_popup(owner, true)?;
                } else {
                    self.cancel_breadcrumb(owner)?;
                }
            }
            return Ok(true);
        }
        if self.document(p.editor).unwrap().composition().is_some() {
            return Ok(false);
        }
        if *key == Key::Tab {
            self.breadcrumbs
                .entries
                .get_mut(&owner)
                .unwrap()
                .session
                .cancel();
            self.close_breadcrumb_popup(owner, false)?;
            return Ok(false);
        }
        if *key == Key::F2 && *modifiers == Modifiers::default() {
            if !repeat {
                self.begin_breadcrumb_edit(owner)?;
            }
            return Ok(true);
        }
        if matches!(key,Key::Character(value) if value.eq_ignore_ascii_case("l"))
            && modifiers.control
            && !modifiers.alt
            && !modifiers.super_key
        {
            if !repeat {
                self.begin_breadcrumb_edit(owner)?;
            }
            return Ok(true);
        }
        if focused != p.editor
            && !self.flyout_descendant(focused, p.popup.container)
            && *modifiers == Modifiers::default()
            && matches!(key, Key::ArrowLeft | Key::ArrowRight | Key::Home | Key::End)
        {
            let e = &self.breadcrumbs.entries[&owner];
            let visible: Vec<_> = e
                .segments
                .iter()
                .filter(|s| !e.hidden.contains(&s.key))
                .collect();
            if !visible.is_empty() {
                let current = visible
                    .iter()
                    .position(|s| s.button == focused || s.children == focused);
                let index = match key {
                    Key::Home => 0,
                    Key::End => visible.len() - 1,
                    Key::ArrowLeft => current.unwrap_or(0).saturating_sub(1),
                    _ => current.map_or(0, |i| (i + 1).min(visible.len() - 1)),
                };
                self.runtime
                    .apply(Mutation::Focus(Some(visible[index].button)))?;
                self.synchronize_editor_focus()?;
            }
            return Ok(true);
        }
        if *key == Key::Enter && focused == p.editor && !modifiers.alt && !modifiers.super_key {
            if !repeat {
                self.submit_breadcrumb(owner)?;
            }
            return Ok(true);
        }
        if *key == Key::ArrowDown && modifiers.alt && !modifiers.control && !modifiers.super_key {
            if !repeat {
                if let Some(key) = self.breadcrumbs.entries[&owner]
                    .session
                    .path
                    .segments
                    .iter()
                    .rev()
                    .find(|s| s.has_children)
                    .map(|s| s.key.clone())
                {
                    self.request_breadcrumb_children(owner, &key)?;
                } else {
                    self.open_breadcrumb_history(owner)?;
                }
            }
            return Ok(true);
        }
        Ok(false)
    }
    pub(in crate::controls) fn preflight_breadcrumb_removal(
        &self,
        id: WidgetId,
    ) -> Result<(), ControlError> {
        if self
            .breadcrumbs
            .entries
            .values()
            .any(|e| required(e).contains(&id))
        {
            Err(invalid(
                "Remove the Breadcrumb owner, not its required parts",
            ))
        } else {
            Ok(())
        }
    }
    pub(in crate::controls) fn prune_breadcrumbs(&mut self) -> Result<(), ControlError> {
        let removed: Vec<_> = self
            .breadcrumbs
            .entries
            .iter()
            .filter_map(|(&owner, e)| {
                (!self.runtime.tree().contains(owner)
                    || required(e)
                        .iter()
                        .any(|id| !self.runtime.tree().contains(*id)))
                .then_some(owner)
            })
            .collect();
        for owner in removed {
            if let Some(e) = self.breadcrumbs.entries.remove(&owner)
                && self.runtime.tree().contains(e.parts.popup.container)
            {
                self.runtime
                    .apply(Mutation::Remove(e.parts.popup.container))?;
            }
        }
        Ok(())
    }
    pub(in crate::controls) fn preflight_breadcrumb_text(
        &self,
        editor: WidgetId,
        text: &str,
    ) -> Result<(), ControlError> {
        if self
            .breadcrumbs
            .entries
            .values()
            .any(|e| e.parts.editor == editor)
        {
            super::super::breadcrumb_model::validate_text(text, 1024, 4096, true)?;
        }
        Ok(())
    }
}
fn required(e: &Entry) -> Vec<WidgetId> {
    let p = e.parts;
    let mut ids = vec![
        e.bar,
        e.status,
        p.editor,
        p.overflow_button,
        p.history_button,
        p.edit_button,
        p.popup.container,
        p.popup.content,
        p.list.container,
        p.list.viewport,
        p.list.content,
    ];
    ids.extend(e.segments.iter().flat_map(|s| [s.button, s.children]));
    ids
}
