//! Shared retained calendar overlays for typed temporal editor sessions.
use super::*;
use rust_desktop_ui_core::Modifiers;

#[derive(Clone)]
struct Entry {
    popup: Flyout,
    calendar: Calendar,
    apply: WidgetId,
    cancel: WidgetId,
    pending: bool,
    explicit_close: bool,
}
#[derive(Default)]
pub(super) struct TemporalPopupState {
    entries: HashMap<WidgetId, Entry>,
}

impl Controls {
    fn temporal_editable(&self, owner: WidgetId) -> bool {
        self.rich_editable(owner)
            && self
                .composite_editor(owner)
                .is_some_and(|editor| self.is_text_editable(editor))
    }
    pub(super) fn temporal_popup_owner(&self, id: WidgetId) -> Option<WidgetId> {
        self.rich_owner(id).filter(|&owner| {
            self.date_time_settings(owner).is_some() || self.date_range_settings(owner).is_some()
        })
    }
    fn temporal_calendar_values(
        &self,
        owner: WidgetId,
    ) -> Result<(CalendarSelection, CalendarSettings), ControlError> {
        let editor = self
            .composite_editor(owner)
            .ok_or_else(|| ControlError::Invalid("Not a temporal editor".into()))?;
        let current = self
            .document(editor)
            .and_then(|d| self.parse_temporal_draft(owner, d.text()).ok().flatten())
            .or_else(|| self.rich.value_baselines.get(&owner).cloned());
        if let Some(settings) = self.date_time_settings(owner) {
            let value = match current {
                Some(EditorValue::DateTime(value)) => value.map(|v| v.date),
                _ => None,
            };
            Ok((
                CalendarSelection::Single(value),
                CalendarSettings {
                    minimum: settings.minimum.map(|v| v.date),
                    maximum: settings.maximum.map(|v| v.date),
                    format: settings.date_format,
                    is_read_only: false,
                },
            ))
        } else if let Some(settings) = self.date_range_settings(owner) {
            let value = match current {
                Some(EditorValue::DateRange(value)) => value,
                _ => None,
            };
            Ok((
                CalendarSelection::Range(value),
                CalendarSettings {
                    minimum: settings.minimum,
                    maximum: settings.maximum,
                    format: settings.format,
                    is_read_only: false,
                },
            ))
        } else {
            Err(ControlError::Invalid(
                "Calendar overlay requires DateTimeEdit or DateRangeEdit".into(),
            ))
        }
    }
    pub(super) fn refresh_temporal_popup(&mut self, owner: WidgetId) -> Result<(), ControlError> {
        if let Some(entry) = self.temporal_popups.entries.get(&owner).cloned() {
            let (selection, settings) = self.temporal_calendar_values(owner)?;
            self.replace_popup_calendar(entry.calendar.container, selection, settings)?;
        }
        Ok(())
    }
    pub fn temporal_calendar(&self, owner: WidgetId) -> Option<&Calendar> {
        self.temporal_popups
            .entries
            .get(&owner)
            .map(|entry| &entry.calendar)
    }
    pub fn temporal_popup_is_open(&self, owner: WidgetId) -> bool {
        self.temporal_popups
            .entries
            .get(&owner)
            .is_some_and(|e| self.flyout_is_open(e.popup))
    }
    pub(super) fn open_temporal_popup(&mut self, owner: WidgetId) -> Result<(), ControlError> {
        if !self.temporal_editable(owner) {
            return Ok(());
        }
        if self.temporal_popup_is_open(owner) {
            return self.close_temporal_popup(owner);
        }
        let (selection, settings) = self.temporal_calendar_values(owner)?;
        if let Some(entry) = self.temporal_popups.entries.get(&owner).cloned() {
            // Validate/reinitialize model atomically, retaining every child WidgetId.
            self.replace_popup_calendar(entry.calendar.container, selection, settings)?;
        } else {
            let popup = self.flyout(owner, Size::new(352.0, 370.0))?;
            let result = (|| {
                self.edit_props(popup.content, |props| {
                    props.style.kind = LayoutKind::Flex(Axis::Vertical)
                })?;
                let calendar = self.calendar(
                    popup.content,
                    "Календарь",
                    selection,
                    settings,
                    LayoutStyle {
                        width: Length::Percent(1.0),
                        height: Length::Px(0.0),
                        flex_grow: 1.0,
                        ..Default::default()
                    },
                )?;
                let footer = self.transparent_panel(
                    popup.content,
                    LayoutStyle {
                        kind: LayoutKind::Flex(Axis::Horizontal),
                        height: Length::Px(36.0),
                        flex_shrink: 0.0,
                        gap: 6.0,
                        ..Default::default()
                    },
                )?;
                let button = LayoutStyle {
                    width: Length::Px(0.0),
                    height: Length::Percent(1.0),
                    flex_grow: 1.0,
                    ..Default::default()
                };
                let apply = self.button(footer, "Применить", None, button.clone())?;
                let cancel = self.button(footer, "Отмена", None, button)?;
                self.temporal_popups.entries.insert(
                    owner,
                    Entry {
                        popup,
                        calendar,
                        apply,
                        cancel,
                        pending: false,
                        explicit_close: false,
                    },
                );
                Ok(())
            })();
            if result.is_err() {
                self.remove(popup.container)?;
                return result;
            }
        }
        let entry = self.temporal_popups.entries[&owner].clone();
        self.open_flyout(entry.popup)?;
        self.focus_calendar_cursor(entry.calendar.container)
    }
    pub fn close_temporal_popup(&mut self, owner: WidgetId) -> Result<(), ControlError> {
        let entry = self
            .temporal_popups
            .entries
            .get(&owner)
            .ok_or_else(|| ControlError::Invalid("Temporal popup has not been created".into()))?
            .clone();
        self.close_flyout(entry.popup)
    }
    pub fn apply_temporal_popup(&mut self, owner: WidgetId) -> Result<CommitOutcome, ControlError> {
        if !self.temporal_popup_is_open(owner) || !self.temporal_editable(owner) {
            return Err(ControlError::Invalid(
                "Temporal popup is not open and editable".into(),
            ));
        }
        let editor = self.composite_editor(owner).unwrap();
        let outcome = self.commit_edit(editor)?;
        self.temporal_popups
            .entries
            .get_mut(&owner)
            .unwrap()
            .pending = outcome == CommitOutcome::Pending;
        if matches!(outcome, CommitOutcome::Committed | CommitOutcome::Unchanged) {
            self.finish_temporal_popup(owner)?;
        }
        Ok(outcome)
    }
    pub fn cancel_temporal_popup(&mut self, owner: WidgetId) -> Result<(), ControlError> {
        let editor = self
            .composite_editor(owner)
            .ok_or_else(|| ControlError::Invalid("Unknown temporal editor".into()))?;
        self.cancel_edit(editor)?;
        self.finish_temporal_popup(owner)
    }
    fn finish_temporal_popup(&mut self, owner: WidgetId) -> Result<(), ControlError> {
        let entry = self
            .temporal_popups
            .entries
            .get_mut(&owner)
            .ok_or_else(|| ControlError::Invalid("Temporal popup has not been created".into()))?;
        entry.explicit_close = true;
        entry.pending = false;
        let popup = entry.popup;
        let result = self.close_flyout(popup);
        if let Some(entry) = self.temporal_popups.entries.get_mut(&owner) {
            entry.explicit_close = false;
        }
        result
    }
    pub(super) fn calendar_popup_changed(
        &mut self,
        calendar: WidgetId,
    ) -> Result<(), ControlError> {
        if let Some(owner) = self
            .rich
            .popup
            .as_ref()
            .filter(|p| {
                p.month.is_some() && self.runtime.tree().parent(calendar) == Some(p.surface)
            })
            .map(|p| p.owner)
        {
            if let Some(CalendarSelection::Single(Some(value))) = self.calendar_selection(calendar)
                && self.user_set_date_value(owner, value)?
            {
                self.close_rich_popup(true)?;
            }
            return Ok(());
        }
        let Some(owner) = self
            .temporal_popups
            .entries
            .iter()
            .find(|(_, e)| e.calendar.container == calendar && self.flyout_is_open(e.popup))
            .map(|(&id, _)| id)
        else {
            return Ok(());
        };
        if !self.temporal_editable(owner) {
            return Ok(());
        }
        let editor = self.composite_editor(owner).unwrap();
        let text = if let Some(settings) = self.date_time_settings(owner) {
            let CalendarSelection::Single(Some(date)) = self.calendar_selection(calendar).unwrap()
            else {
                return Ok(());
            };
            let previous = self
                .document(editor)
                .and_then(|d| self.parse_temporal_draft(owner, d.text()).ok().flatten())
                .or_else(|| self.rich.value_baselines.get(&owner).cloned());
            let time = match previous {
                Some(EditorValue::DateTime(Some(v))) => v.time,
                _ => TimeValue::new(0, 0, 0)?,
            };
            let value = DateTimeValue { date, time };
            value.to_display(settings.date_format, settings.show_seconds)
        } else {
            let settings = self.date_range_settings(owner).unwrap();
            if let Some(start) = self.calendar_range_anchor(calendar) {
                format!("{} – ", start.to_display(settings.format))
            } else {
                match self.calendar_selection(calendar).unwrap() {
                    CalendarSelection::Range(Some(value)) => value.to_display(settings.format),
                    _ => String::new(),
                }
            }
        };
        self.set_text(editor, &text)?;
        Ok(())
    }
    pub(super) fn temporal_focus_editor(&self, id: WidgetId) -> Option<WidgetId> {
        self.temporal_popups
            .entries
            .iter()
            .find(|(owner, e)| {
                self.flyout_is_open(e.popup)
                    && (**owner == id || self.flyout_descendant(id, e.popup.container))
            })
            .and_then(|(&owner, _)| self.composite_editor(owner))
    }
    pub(super) fn temporal_will_close(&mut self, popup: WidgetId) -> Result<(), ControlError> {
        let Some(owner) = self
            .temporal_popups
            .entries
            .iter()
            .find(|(_, e)| e.popup.container == popup)
            .map(|(&id, _)| id)
        else {
            return Ok(());
        };
        let entry = self.temporal_popups.entries.get_mut(&owner).unwrap();
        entry.pending = false;
        if entry.explicit_close {
            return Ok(());
        }
        if let Some(editor) = self.composite_editor(owner) {
            if self.window_focused
                && self.temporal_editable(owner)
                && self
                    .editor_settings(editor)
                    .is_some_and(|s| s.commit_on_focus_lost)
                && self
                    .document(editor)
                    .is_some_and(|d| d.composition().is_none())
            {
                self.commit_edit(editor)?;
            } else if !self.window_focused || !self.temporal_editable(owner) {
                self.invalidate_async_editor(editor);
                if self.observed_editor_focus == Some(editor) {
                    self.observed_editor_focus = None;
                }
            }
        }
        Ok(())
    }
    pub(super) fn activate_temporal_popup(&mut self, id: WidgetId) -> Result<bool, ControlError> {
        let Some((owner, apply)) = self
            .temporal_popups
            .entries
            .iter()
            .find(|(_, e)| e.apply == id || e.cancel == id)
            .map(|(&owner, e)| (owner, e.apply == id))
        else {
            return Ok(false);
        };
        if apply {
            self.apply_temporal_popup(owner)?;
        } else {
            self.cancel_temporal_popup(owner)?;
        }
        Ok(true)
    }
    pub(super) fn temporal_before_input(
        &mut self,
        event: &InputEvent,
    ) -> Result<bool, ControlError> {
        let Some(focus) = self.runtime.focused() else {
            return Ok(false);
        };
        let Some(editor) = self.temporal_focus_editor(focus) else {
            return Ok(false);
        };
        let Some(owner) = self.rich_owner(editor) else {
            return Ok(false);
        };
        match event {
            InputEvent::KeyDown {
                key: Key::Escape,
                modifiers,
                repeat: false,
            } if *modifiers == Modifiers::default()
                && self.popup.is_none()
                && self.rich.popup.is_none() =>
            {
                if self
                    .document(editor)
                    .is_some_and(|d| d.composition().is_some())
                {
                    self.cancel_preedit();
                } else {
                    self.cancel_temporal_popup(owner)?;
                }
                Ok(true)
            }
            InputEvent::KeyDown {
                key: Key::Enter,
                modifiers,
                repeat: false,
            } if modifiers.control
                && !modifiers.alt
                && !modifiers.shift
                && !modifiers.super_key =>
            {
                self.apply_temporal_popup(owner)?;
                Ok(true)
            }
            _ => Ok(false),
        }
    }
    pub(super) fn cancel_unavailable_temporal_validation(&mut self) {
        let unavailable: Vec<_> = self
            .temporal_popups
            .entries
            .iter()
            .filter(|(owner, e)| {
                e.pending && (!self.window_focused || !self.temporal_editable(**owner))
            })
            .map(|(&owner, _)| owner)
            .collect();
        for owner in unavailable {
            if let Some(editor) = self.composite_editor(owner) {
                self.invalidate_async_editor(editor);
            }
            if let Some(entry) = self.temporal_popups.entries.get_mut(&owner) {
                entry.pending = false;
            }
        }
    }
    pub(super) fn sync_temporal_popups(&mut self) -> Result<(), ControlError> {
        self.prune_temporal_popups();
        let entries: Vec<_> = self
            .temporal_popups
            .entries
            .iter()
            .map(|(&owner, e)| (owner, e.clone()))
            .collect();
        for (owner, entry) in entries {
            if !self.flyout_is_open(entry.popup) {
                continue;
            }
            let editor = self.composite_editor(owner).unwrap();
            if !self.temporal_editable(owner) || !self.window_focused {
                self.invalidate_async_editor(editor);
                if self.observed_editor_focus == Some(editor) {
                    self.observed_editor_focus = None;
                }
                self.finish_temporal_popup(owner)?;
                continue;
            }
            self.set_enabled(
                entry.apply,
                self.calendar_range_anchor(entry.calendar.container)
                    .is_none(),
            )?;
            if entry.pending
                && self.editor_validation_status(editor) != crate::EditorValidationStatus::Pending
            {
                self.temporal_popups
                    .entries
                    .get_mut(&owner)
                    .unwrap()
                    .pending = false;
                if self.is_edit_dirty(editor) == Some(false)
                    && self.validation_error(editor).is_none()
                {
                    self.finish_temporal_popup(owner)?;
                }
            }
        }
        Ok(())
    }
    pub(super) fn prune_temporal_popups(&mut self) {
        self.temporal_popups.entries.retain(|owner, e| {
            self.runtime.tree().contains(*owner)
                && [
                    e.popup.container,
                    e.popup.content,
                    e.calendar.container,
                    e.apply,
                    e.cancel,
                ]
                .into_iter()
                .all(|id| self.runtime.tree().contains(id))
        });
    }
    pub(super) fn preflight_temporal_removal(&self, id: WidgetId) -> Result<(), ControlError> {
        if self.temporal_popups.entries.values().any(|e| {
            [e.popup.container, e.calendar.container, e.apply, e.cancel].contains(&id)
                || self.runtime.tree().parent(e.apply) == Some(id)
        }) {
            return Err(ControlError::Invalid(
                "Remove the temporal editor owner, not its required popup parts".into(),
            ));
        }
        Ok(())
    }
}
