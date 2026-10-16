//! Calendar navigation and retained Gregorian selection.
use super::selection::RichPopup;
use super::*;

impl Controls {
    pub(super) fn build_calendar_popup(
        &mut self,
        owner: WidgetId,
        month: DateValue,
    ) -> Result<(), ControlError> {
        let settings = self
            .date_settings(owner)
            .cloned()
            .ok_or_else(|| ControlError::Invalid("Not a DateEdit".into()))?;
        let desired = Size::new(320.0, 300.0);
        let surface = self.panel(
            self.overlay,
            LayoutStyle {
                width: Length::Px(desired.width),
                height: Length::Px(desired.height),
                ..Default::default()
            },
        )?;
        self.controls.get_mut(&surface).unwrap().popup_surface = true;
        self.rich.popup = Some(RichPopup {
            owner,
            surface,
            list: None,
            desired,
            query: String::new(),
            month: Some(month),
        });
        let result = self.calendar(
            surface,
            "Календарь",
            CalendarSelection::Single(Some(month)),
            CalendarSettings {
                minimum: settings.minimum,
                maximum: settings.maximum,
                format: settings.format,
                is_read_only: false,
            },
            LayoutStyle {
                width: Length::Percent(1.0),
                height: Length::Percent(1.0),
                ..Default::default()
            },
        );
        if let Err(error) = result {
            self.close_rich_popup(false)?;
            return Err(error);
        }
        Ok(())
    }
    pub(super) fn change_calendar_month(
        &mut self,
        owner: WidgetId,
        direction: i32,
    ) -> Result<(), ControlError> {
        if !self.rich_editable(owner) {
            return Ok(());
        }
        let Some(month) = self
            .rich
            .popup
            .as_ref()
            .filter(|p| p.owner == owner)
            .and_then(|p| p.month)
        else {
            return Ok(());
        };
        self.close_rich_popup(false)?;
        self.build_calendar_popup(owner, month.month_offset(direction))?;
        if let Some(editor) = self.composite_editor(owner) {
            self.runtime.apply(Mutation::Focus(Some(editor)))?;
        }
        Ok(())
    }
    pub(super) fn calendar_key(&mut self, key: &Key) -> Result<bool, ControlError> {
        let Some(popup) = &self.rich.popup else {
            return Ok(false);
        };
        let Some(cursor) = popup.month else {
            return Ok(false);
        };
        let owner = popup.owner;
        if *key == Key::Enter {
            if self.user_set_date_value(owner, cursor)? {
                self.close_rich_popup(true)?;
            }
            return Ok(true);
        }
        let next = match key {
            Key::ArrowLeft => cursor.day_offset(-1),
            Key::ArrowRight => cursor.day_offset(1),
            Key::ArrowUp => cursor.day_offset(-7),
            Key::ArrowDown => cursor.day_offset(7),
            Key::Home => DateValue { day: 1, ..cursor },
            Key::End => DateValue {
                day: cursor.days_in_month(),
                ..cursor
            },
            Key::PageUp => cursor.month_offset(-1),
            Key::PageDown => cursor.month_offset(1),
            _ => return Ok(false),
        };
        let settings = self.date_settings(owner).unwrap();
        let next = next
            .max(settings.minimum.unwrap_or(DateValue {
                year: 1,
                month: 1,
                day: 1,
            }))
            .min(settings.maximum.unwrap_or(DateValue {
                year: 9999,
                month: 12,
                day: 31,
            }));
        self.close_rich_popup(false)?;
        self.build_calendar_popup(owner, next)?;
        if let Some(editor) = self.composite_editor(owner) {
            self.runtime.apply(Mutation::Focus(Some(editor)))?;
        }
        Ok(true)
    }
    pub(super) fn user_set_date_value(
        &mut self,
        owner: WidgetId,
        value: DateValue,
    ) -> Result<bool, ControlError> {
        match self.set_date_value(owner, Some(value))? {
            CommitOutcome::Invalid(message) => {
                let editor = self.rich.composites[&owner].parts.editor;
                self.controls.get_mut(&editor).unwrap().validation_error = Some(message.clone());
                self.edit_events.push(EditEvent::ValidationFailed {
                    id: editor,
                    message,
                });
                self.scene_dirty = true;
                Ok(false)
            }
            _ => Ok(true),
        }
    }
}
