//! Numeric and calendar values use invariant storage and explicit display formats.
use super::selection::{
    Composite, CompositeKind, EditorControl, RichAction, RichPopup, SelectionChanged,
};
use super::*;
use std::fmt::{Display, Formatter};

#[derive(Clone, Debug, PartialEq)]
pub struct SpinBoxSettings {
    pub minimum: f64,
    pub maximum: f64,
    pub step: f64,
}
impl Default for SpinBoxSettings {
    fn default() -> Self {
        Self {
            minimum: -1.0e15,
            maximum: 1.0e15,
            step: 1.0,
        }
    }
}
impl SpinBoxSettings {
    pub fn validate(&self) -> Result<(), ControlError> {
        if !self.minimum.is_finite()
            || !self.maximum.is_finite()
            || !self.step.is_finite()
            || self.minimum > self.maximum
            || self.step <= 0.0
        {
            return Err(ControlError::Invalid(
                "SpinBox requires finite minimum <= maximum and step > 0".into(),
            ));
        }
        Ok(())
    }
    fn parse(&self, text: &str) -> Result<f64, String> {
        let value = text
            .trim()
            .parse::<f64>()
            .map_err(|_| "Введите число.".to_owned())?;
        if !value.is_finite() || value < self.minimum || value > self.maximum {
            return Err(format!(
                "Значение должно быть от {} до {}.",
                self.minimum, self.maximum
            ));
        }
        Ok(value)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct DateValue {
    pub year: i32,
    pub month: u8,
    pub day: u8,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DateFormat {
    Iso,
    #[default]
    Russian,
}
impl DateValue {
    /// Current UTC date, used only as the initial view of an empty calendar.
    /// This does not assign an editor value or introduce a time-zone dependency.
    pub fn today_utc() -> Self {
        let mut days = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |duration| duration.as_secs() / 86_400);
        let mut year = 1970;
        while year < 9999 {
            let count = if (Self {
                year,
                month: 2,
                day: 1,
            })
            .days_in_month()
                == 29
            {
                366
            } else {
                365
            };
            if days < count {
                break;
            }
            days -= count;
            year += 1;
        }
        if days > 365 {
            return Self {
                year: 9999,
                month: 12,
                day: 31,
            };
        }
        let mut date = Self {
            year,
            month: 1,
            day: 1,
        };
        while date.month < 12 && days >= date.days_in_month() as u64 {
            days -= date.days_in_month() as u64;
            date.month += 1;
        }
        date.day = (days + 1).min(date.days_in_month() as u64) as u8;
        date
    }
    pub fn new(year: i32, month: u8, day: u8) -> Result<Self, ControlError> {
        let value = Self { year, month, day };
        value.validate()?;
        Ok(value)
    }
    pub fn validate(self) -> Result<(), ControlError> {
        if !(1..=9999).contains(&self.year)
            || !(1..=12).contains(&self.month)
            || self.day == 0
            || self.day > self.days_in_month()
        {
            return Err(ControlError::Invalid(
                "Invalid Gregorian date (supported years 1..9999)".into(),
            ));
        }
        Ok(())
    }
    pub fn days_in_month(self) -> u8 {
        match self.month {
            2 if self.year % 4 == 0 && (self.year % 100 != 0 || self.year % 400 == 0) => 29,
            2 => 28,
            4 | 6 | 9 | 11 => 30,
            _ => 31,
        }
    }
    pub fn parse_iso(text: &str) -> Result<Self, ControlError> {
        Self::parse_display(text, DateFormat::Iso)
    }
    pub fn parse_display(text: &str, format: DateFormat) -> Result<Self, ControlError> {
        let text = text.trim();
        let separator = if format == DateFormat::Iso { '-' } else { '.' };
        let fields = text.split(separator).collect::<Vec<_>>();
        let invalid = || {
            ControlError::Invalid(
                "Expected YYYY-MM-DD or DD.MM.YYYY according to DateFormat".into(),
            )
        };
        if fields.len() != 3
            || fields
                .iter()
                .any(|f| f.is_empty() || !f.bytes().all(|c| c.is_ascii_digit()))
        {
            return Err(invalid());
        }
        let (year, month, day) = if format == DateFormat::Iso {
            (fields[0], fields[1], fields[2])
        } else {
            (fields[2], fields[1], fields[0])
        };
        if year.len() != 4 || month.len() != 2 || day.len() != 2 {
            return Err(invalid());
        }
        Self::new(
            year.parse().map_err(|_| invalid())?,
            month.parse().map_err(|_| invalid())?,
            day.parse().map_err(|_| invalid())?,
        )
    }
    pub fn to_iso(self) -> String {
        self.to_display(DateFormat::Iso)
    }
    pub fn to_display(self, format: DateFormat) -> String {
        match format {
            DateFormat::Iso => format!("{:04}-{:02}-{:02}", self.year, self.month, self.day),
            DateFormat::Russian => format!("{:02}.{:02}.{:04}", self.day, self.month, self.year),
        }
    }
    /// Monday is zero. The arithmetic covers the proleptic Gregorian calendar.
    fn weekday(self) -> u8 {
        let offsets = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
        let y = self.year - i32::from(self.month < 3);
        ((y + y / 4 - y / 100 + y / 400 + offsets[self.month as usize - 1] + self.day as i32 + 6)
            % 7) as u8
    }
    fn month_offset(self, delta: i32) -> Self {
        let index = ((self.year - 1) * 12 + self.month as i32 - 1 + delta).clamp(0, 9999 * 12 - 1);
        Self {
            year: index / 12 + 1,
            month: (index % 12 + 1) as u8,
            day: 1,
        }
    }
    fn day_offset(mut self, delta: i32) -> Self {
        for _ in 0..delta.unsigned_abs() {
            if delta > 0 {
                if self.day < self.days_in_month() {
                    self.day += 1;
                } else {
                    let next = self.month_offset(1);
                    if next.year == self.year && next.month == self.month {
                        break;
                    }
                    self = next;
                }
            } else if self.day > 1 {
                self.day -= 1;
            } else {
                let mut previous = self.month_offset(-1);
                if previous.year == self.year && previous.month == self.month {
                    break;
                }
                previous.day = previous.days_in_month();
                self = previous;
            }
        }
        self
    }
}
impl Display for DateValue {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_iso())
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DateEditSettings {
    pub minimum: Option<DateValue>,
    pub maximum: Option<DateValue>,
    pub allow_empty: bool,
    pub format: DateFormat,
}
impl Default for DateEditSettings {
    fn default() -> Self {
        Self {
            minimum: None,
            maximum: None,
            allow_empty: true,
            format: DateFormat::Russian,
        }
    }
}
impl DateEditSettings {
    pub fn validate(&self) -> Result<(), ControlError> {
        if let Some(value) = self.minimum {
            value.validate()?;
        }
        if let Some(value) = self.maximum {
            value.validate()?;
        }
        if let (Some(min), Some(max)) = (self.minimum, self.maximum)
            && min > max
        {
            return Err(ControlError::Invalid(
                "Date minimum must not exceed maximum".into(),
            ));
        }
        Ok(())
    }
    fn check(&self, value: Option<DateValue>) -> Result<(), String> {
        if let Some(value) = value {
            value.validate().map_err(|e| e.to_string())?;
            if self.minimum.is_some_and(|min| value < min)
                || self.maximum.is_some_and(|max| value > max)
            {
                return Err("Дата вне допустимого диапазона.".into());
            }
        } else if !self.allow_empty {
            return Err("Укажите дату.".into());
        }
        Ok(())
    }
    fn parse(&self, text: &str) -> Result<Option<DateValue>, String> {
        let value = if text.trim().is_empty() {
            None
        } else {
            Some(DateValue::parse_display(text, self.format).map_err(|e| e.to_string())?)
        };
        self.check(value)?;
        Ok(value)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct TimeValue {
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
}
impl TimeValue {
    pub fn new(hour: u8, minute: u8, second: u8) -> Result<Self, ControlError> {
        let value = Self {
            hour,
            minute,
            second,
        };
        value.validate()?;
        Ok(value)
    }
    pub fn validate(self) -> Result<(), ControlError> {
        if self.hour > 23 || self.minute > 59 || self.second > 59 {
            Err(ControlError::Invalid(
                "Time must be HH:mm[:ss] in the 24-hour clock".into(),
            ))
        } else {
            Ok(())
        }
    }
    pub fn parse(text: &str) -> Result<Self, ControlError> {
        let fields = text.trim().split(':').collect::<Vec<_>>();
        let invalid =
            || ControlError::Invalid("Time must be HH:mm[:ss] in the 24-hour clock".into());
        if !(2..=3).contains(&fields.len())
            || fields
                .iter()
                .any(|f| f.len() != 2 || !f.bytes().all(|c| c.is_ascii_digit()))
        {
            return Err(invalid());
        }
        Self::new(
            fields[0].parse().map_err(|_| invalid())?,
            fields[1].parse().map_err(|_| invalid())?,
            if fields.len() == 3 {
                fields[2].parse().map_err(|_| invalid())?
            } else {
                0
            },
        )
    }
    pub fn to_iso(self) -> String {
        format!("{:02}:{:02}:{:02}", self.hour, self.minute, self.second)
    }
    pub fn to_display(self, seconds: bool) -> String {
        if seconds {
            self.to_iso()
        } else {
            format!("{:02}:{:02}", self.hour, self.minute)
        }
    }
    fn seconds(self) -> i32 {
        self.hour as i32 * 3600 + self.minute as i32 * 60 + self.second as i32
    }
    fn from_seconds(value: i32) -> Self {
        Self {
            hour: (value / 3600) as u8,
            minute: (value % 3600 / 60) as u8,
            second: (value % 60) as u8,
        }
    }
}
impl Display for TimeValue {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_iso())
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TimeEditSettings {
    pub minimum: Option<TimeValue>,
    pub maximum: Option<TimeValue>,
    pub allow_empty: bool,
    pub show_seconds: bool,
    pub step_seconds: u32,
}
impl Default for TimeEditSettings {
    fn default() -> Self {
        Self {
            minimum: None,
            maximum: None,
            allow_empty: true,
            show_seconds: false,
            step_seconds: 60,
        }
    }
}
impl TimeEditSettings {
    pub fn validate(&self) -> Result<(), ControlError> {
        if let Some(value) = self.minimum {
            value.validate()?;
        }
        if let Some(value) = self.maximum {
            value.validate()?;
        }
        if self.step_seconds == 0
            || self.step_seconds > 86400
            || (!self.show_seconds && !self.step_seconds.is_multiple_of(60))
            || matches!((self.minimum,self.maximum), (Some(min),Some(max)) if min > max)
        {
            return Err(ControlError::Invalid(
                "Time bounds and step must match the display precision".into(),
            ));
        }
        if !self.show_seconds
            && [self.minimum, self.maximum]
                .into_iter()
                .flatten()
                .any(|t| t.second != 0)
        {
            return Err(ControlError::Invalid(
                "Second precision requires ShowSeconds".into(),
            ));
        }
        Ok(())
    }
    fn check(&self, value: Option<TimeValue>) -> Result<(), String> {
        if let Some(value) = value {
            value.validate().map_err(|e| e.to_string())?;
            if (!self.show_seconds && value.second != 0)
                || self.minimum.is_some_and(|min| value < min)
                || self.maximum.is_some_and(|max| value > max)
            {
                return Err("Время вне допустимого диапазона или точности.".into());
            }
        } else if !self.allow_empty {
            return Err("Укажите время.".into());
        }
        Ok(())
    }
    fn parse(&self, text: &str) -> Result<Option<TimeValue>, String> {
        let value = if text.trim().is_empty() {
            None
        } else {
            Some(TimeValue::parse(text).map_err(|e| e.to_string())?)
        };
        self.check(value)?;
        Ok(value)
    }
}
#[derive(Clone, Debug, PartialEq)]
pub enum EditorValue {
    Number(f64),
    Date(Option<DateValue>),
    Time(Option<TimeValue>),
}
#[derive(Clone, Debug, PartialEq)]
pub struct ValueChanged {
    pub id: WidgetId,
    pub value: EditorValue,
}

impl Controls {
    pub fn editor_validator(&self, id: WidgetId) -> Option<&EditorValidator> {
        self.controls.get(&id)?.validator.as_ref()
    }
    pub fn label_text(&self, id: WidgetId) -> Option<&str> {
        self.controls.get(&id).map(|c| c.label.as_str())
    }
    /// Preflight a draft without changing the document, accepted value or diagnostics.
    pub fn validate_edit(&self, id: WidgetId, text: &str) -> Result<(), String> {
        self.editor_control(id).map_err(|e| e.to_string())?;
        self.validate_editor_value(id, text)
    }
    pub fn drain_value_changes(&mut self) -> Vec<ValueChanged> {
        std::mem::take(&mut self.rich.values)
    }
    pub fn composite_read_only(&self, id: WidgetId) -> Option<bool> {
        self.rich.composites.get(&id).map(|c| c.read_only)
    }
    /// Typed validation remains independent from this application validator chain.
    pub fn chain_editor_validator(
        &mut self,
        id: WidgetId,
        validator: EditorValidator,
    ) -> Result<(), ControlError> {
        self.editor_control(id)?;
        let previous = self.controls[&id].validator.clone();
        self.set_editor_validator(
            id,
            Some(EditorValidator::new(move |text| {
                if let Some(previous) = &previous {
                    previous.check(text)?;
                }
                validator.check(text)
            })),
        )
    }
    pub fn spin_box(
        &mut self,
        parent: WidgetId,
        label: impl Into<String>,
        value: f64,
        settings: SpinBoxSettings,
        style: LayoutStyle,
    ) -> Result<EditorControl, ControlError> {
        settings.validate()?;
        settings
            .parse(&value.to_string())
            .map_err(ControlError::Invalid)?;
        let mut parts =
            self.create_editor_parts(parent, label.into(), &value.to_string(), style)?;
        self.add_step_buttons(&mut parts)?;
        self.set_role(parts.editor, Role::SpinButton);
        self.rich.composites.insert(
            parts.container,
            Composite {
                parts: parts.clone(),
                kind: CompositeKind::Spin(settings),
                read_only: false,
            },
        );
        self.rich
            .value_baselines
            .insert(parts.container, EditorValue::Number(value));
        Ok(parts)
    }
    pub fn date_edit(
        &mut self,
        parent: WidgetId,
        label: impl Into<String>,
        value: Option<DateValue>,
        settings: DateEditSettings,
        style: LayoutStyle,
    ) -> Result<EditorControl, ControlError> {
        settings.validate()?;
        settings.check(value).map_err(ControlError::Invalid)?;
        let mut parts = self.create_editor_parts(
            parent,
            label.into(),
            &value.map_or(String::new(), |v| v.to_display(settings.format)),
            style,
        )?;
        let button =
            self.add_editor_button(&parts, "Открыть календарь", Some(IconSymbol::ChevronDown))?;
        self.rich
            .actions
            .insert(button, RichAction::Popup(parts.container));
        parts.buttons.push(button);
        self.rich.composites.insert(
            parts.container,
            Composite {
                parts: parts.clone(),
                kind: CompositeKind::Date(settings),
                read_only: false,
            },
        );
        self.rich
            .value_baselines
            .insert(parts.container, EditorValue::Date(value));
        Ok(parts)
    }
    pub fn time_edit(
        &mut self,
        parent: WidgetId,
        label: impl Into<String>,
        value: Option<TimeValue>,
        settings: TimeEditSettings,
        style: LayoutStyle,
    ) -> Result<EditorControl, ControlError> {
        settings.validate()?;
        settings.check(value).map_err(ControlError::Invalid)?;
        let mut parts = self.create_editor_parts(
            parent,
            label.into(),
            &value.map_or(String::new(), |v| v.to_display(settings.show_seconds)),
            style,
        )?;
        self.add_step_buttons(&mut parts)?;
        self.rich.composites.insert(
            parts.container,
            Composite {
                parts: parts.clone(),
                kind: CompositeKind::Time(settings),
                read_only: false,
            },
        );
        self.rich
            .value_baselines
            .insert(parts.container, EditorValue::Time(value));
        Ok(parts)
    }
    fn add_step_buttons(&mut self, parts: &mut EditorControl) -> Result<(), ControlError> {
        for (label, icon, direction) in [
            ("Уменьшить", IconSymbol::Remove, -1),
            ("Увеличить", IconSymbol::Add, 1),
        ] {
            let id = self.add_editor_button(parts, label, Some(icon))?;
            self.rich.actions.insert(
                id,
                RichAction::Step {
                    owner: parts.container,
                    direction,
                },
            );
            parts.buttons.push(id);
        }
        Ok(())
    }
    pub fn spin_value(&self, id: WidgetId) -> Option<f64> {
        let c = self.rich.composites.get(&id)?;
        let CompositeKind::Spin(settings) = &c.kind else {
            return None;
        };
        settings.parse(self.accepted_value(c.parts.editor)?).ok()
    }
    pub fn date_value(&self, id: WidgetId) -> Option<Option<DateValue>> {
        let c = self.rich.composites.get(&id)?;
        let CompositeKind::Date(settings) = &c.kind else {
            return None;
        };
        settings.parse(self.accepted_value(c.parts.editor)?).ok()
    }
    pub fn time_value(&self, id: WidgetId) -> Option<Option<TimeValue>> {
        let c = self.rich.composites.get(&id)?;
        let CompositeKind::Time(settings) = &c.kind else {
            return None;
        };
        settings.parse(self.accepted_value(c.parts.editor)?).ok()
    }
    pub fn set_spin_value(
        &mut self,
        id: WidgetId,
        value: f64,
    ) -> Result<CommitOutcome, ControlError> {
        let c = self
            .rich
            .composites
            .get(&id)
            .ok_or_else(|| ControlError::Invalid("Not a SpinBox".into()))?;
        if !matches!(c.kind, CompositeKind::Spin(_)) {
            return Err(ControlError::Invalid("Not a SpinBox".into()));
        }
        self.set_editor_value(c.parts.editor, &value.to_string())
    }
    pub fn set_date_value(
        &mut self,
        id: WidgetId,
        value: Option<DateValue>,
    ) -> Result<CommitOutcome, ControlError> {
        let c = self
            .rich
            .composites
            .get(&id)
            .ok_or_else(|| ControlError::Invalid("Not a DateEdit".into()))?;
        let CompositeKind::Date(settings) = &c.kind else {
            return Err(ControlError::Invalid("Not a DateEdit".into()));
        };
        if let Err(message) = settings.check(value) {
            return Ok(CommitOutcome::Invalid(message));
        }
        let text = value.map_or(String::new(), |v| v.to_display(settings.format));
        let result = self.set_editor_value(c.parts.editor, &text)?;
        if !matches!(result, CommitOutcome::Invalid(_))
            && self
                .rich
                .popup
                .as_ref()
                .is_some_and(|popup| popup.owner == id && popup.month.is_some())
        {
            self.open_editor_popup(id)?;
        }
        Ok(result)
    }
    pub fn set_time_value(
        &mut self,
        id: WidgetId,
        value: Option<TimeValue>,
    ) -> Result<CommitOutcome, ControlError> {
        let c = self
            .rich
            .composites
            .get(&id)
            .ok_or_else(|| ControlError::Invalid("Not a TimeEdit".into()))?;
        let CompositeKind::Time(settings) = &c.kind else {
            return Err(ControlError::Invalid("Not a TimeEdit".into()));
        };
        if let Err(message) = settings.check(value) {
            return Ok(CommitOutcome::Invalid(message));
        }
        let text = value.map_or(String::new(), |v| v.to_display(settings.show_seconds));
        self.set_editor_value(c.parts.editor, &text)
    }
    pub fn spin_settings(&self, id: WidgetId) -> Option<&SpinBoxSettings> {
        match &self.rich.composites.get(&id)?.kind {
            CompositeKind::Spin(s) => Some(s),
            _ => None,
        }
    }
    pub fn date_settings(&self, id: WidgetId) -> Option<&DateEditSettings> {
        match &self.rich.composites.get(&id)?.kind {
            CompositeKind::Date(s) => Some(s),
            _ => None,
        }
    }
    pub fn time_settings(&self, id: WidgetId) -> Option<&TimeEditSettings> {
        match &self.rich.composites.get(&id)?.kind {
            CompositeKind::Time(s) => Some(s),
            _ => None,
        }
    }
    pub fn set_spin_settings(
        &mut self,
        id: WidgetId,
        settings: SpinBoxSettings,
    ) -> Result<(), ControlError> {
        settings.validate()?;
        let value = self
            .spin_value(id)
            .ok_or_else(|| ControlError::Invalid("Not a SpinBox".into()))?;
        settings
            .parse(&value.to_string())
            .map_err(ControlError::Invalid)?;
        self.rich.composites.get_mut(&id).unwrap().kind = CompositeKind::Spin(settings);
        Ok(())
    }
    pub fn set_date_settings(
        &mut self,
        id: WidgetId,
        settings: DateEditSettings,
    ) -> Result<(), ControlError> {
        settings.validate()?;
        let value = self
            .date_value(id)
            .ok_or_else(|| ControlError::Invalid("Not a DateEdit".into()))?;
        settings.check(value).map_err(ControlError::Invalid)?;
        let c = self.rich.composites[&id].clone();
        if self.is_edit_dirty(c.parts.editor) == Some(true)
            && self.date_settings(id).unwrap().format != settings.format
        {
            return Err(ControlError::Invalid(
                "Commit or cancel the draft before changing DateFormat".into(),
            ));
        }
        let format_changed = self.date_settings(id).unwrap().format != settings.format;
        let text = value.map_or(String::new(), |v| v.to_display(settings.format));
        self.rich.composites.get_mut(&id).unwrap().kind = CompositeKind::Date(settings);
        if format_changed {
            match self.set_editor_value(c.parts.editor, &text) {
                Err(error) => {
                    self.rich.composites.insert(id, c);
                    return Err(error);
                }
                Ok(CommitOutcome::Invalid(message)) => {
                    self.rich.composites.insert(id, c);
                    return Err(ControlError::Invalid(message));
                }
                _ => {}
            }
        }
        Ok(())
    }
    pub fn set_time_settings(
        &mut self,
        id: WidgetId,
        settings: TimeEditSettings,
    ) -> Result<(), ControlError> {
        settings.validate()?;
        let value = self
            .time_value(id)
            .ok_or_else(|| ControlError::Invalid("Not a TimeEdit".into()))?;
        settings.check(value).map_err(ControlError::Invalid)?;
        let c = self.rich.composites[&id].clone();
        if self.is_edit_dirty(c.parts.editor) == Some(true)
            && self.time_settings(id).unwrap().show_seconds != settings.show_seconds
        {
            return Err(ControlError::Invalid(
                "Commit or cancel the draft before changing time precision".into(),
            ));
        }
        let format_changed = self.time_settings(id).unwrap().show_seconds != settings.show_seconds;
        let text = value.map_or(String::new(), |v| v.to_display(settings.show_seconds));
        self.rich.composites.get_mut(&id).unwrap().kind = CompositeKind::Time(settings);
        if format_changed {
            match self.set_editor_value(c.parts.editor, &text) {
                Err(error) => {
                    self.rich.composites.insert(id, c);
                    return Err(error);
                }
                Ok(CommitOutcome::Invalid(message)) => {
                    self.rich.composites.insert(id, c);
                    return Err(ControlError::Invalid(message));
                }
                _ => {}
            }
        }
        Ok(())
    }
    pub(super) fn validate_rich_value(&self, id: WidgetId, text: &str) -> Result<(), String> {
        let Some(owner) = self.rich_owner(id) else {
            return Ok(());
        };
        let c = &self.rich.composites[&owner];
        if c.parts.editor != id {
            return Ok(());
        }
        match &c.kind {
            CompositeKind::Spin(s) => {
                s.parse(text)?;
            }
            CompositeKind::Date(s) => {
                s.parse(text)?;
            }
            CompositeKind::Time(s) => {
                s.parse(text)?;
            }
            CompositeKind::Combo {
                items,
                editable,
                lookup,
                keys,
                ..
            } => {
                if (!editable || *lookup)
                    && !text.is_empty()
                    && !items.iter().any(|i| {
                        i.text == text
                            && (i.enabled || self.rich.suppress_combo_commit == Some(owner))
                    })
                {
                    return Err("Выберите значение из списка.".into());
                }
                if self.rich.suppress_combo_commit != Some(owner) {
                    let next = items
                        .iter()
                        .find(|i| i.text == text && i.enabled && keys.contains(&i.key))
                        .or_else(|| items.iter().find(|i| i.text == text && i.enabled))
                        .map(|i| vec![i.key.clone()])
                        .unwrap_or_default();
                    if &next != keys
                        && let Some(validator) = self.rich.selection_validators.get(&owner)
                    {
                        validator.check(&next)?;
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }
    pub(super) fn rich_value_committed(&mut self, editor: WidgetId) -> Result<(), ControlError> {
        let Some(owner) = self.rich_owner(editor) else {
            return Ok(());
        };
        let c = self.rich.composites[&owner].clone();
        let text = self.accepted_value(editor).unwrap_or("").to_owned();
        let value = match c.kind {
            CompositeKind::Spin(s) => Some(EditorValue::Number(
                s.parse(&text).map_err(ControlError::Invalid)?,
            )),
            CompositeKind::Date(s) => Some(EditorValue::Date(
                s.parse(&text).map_err(ControlError::Invalid)?,
            )),
            CompositeKind::Time(s) => Some(EditorValue::Time(
                s.parse(&text).map_err(ControlError::Invalid)?,
            )),
            CompositeKind::Combo { items, keys, .. }
                if self.rich.suppress_combo_commit != Some(owner) =>
            {
                let next = items
                    .iter()
                    .find(|i| i.text == text && i.enabled && keys.contains(&i.key))
                    .or_else(|| items.iter().find(|i| i.text == text && i.enabled))
                    .map(|i| vec![i.key.clone()])
                    .unwrap_or_default();
                if keys != next {
                    if let CompositeKind::Combo { keys, .. } =
                        &mut self.rich.composites.get_mut(&owner).unwrap().kind
                    {
                        keys.clone_from(&next)
                    }
                    self.rich.changes.push(SelectionChanged {
                        id: owner,
                        keys: next,
                    });
                }
                None
            }
            _ => None,
        };
        if let Some(value) = value
            && self.rich.value_baselines.get(&owner) != Some(&value)
        {
            self.rich.value_baselines.insert(owner, value.clone());
            self.rich.values.push(ValueChanged { id: owner, value });
        }
        Ok(())
    }
    pub(super) fn step_value(
        &mut self,
        owner: WidgetId,
        direction: i32,
    ) -> Result<(), ControlError> {
        if !self.rich_editable(owner) {
            return Ok(());
        }
        let c = self.rich.composites[&owner].clone();
        if self
            .document(c.parts.editor)
            .is_some_and(|d| d.composition().is_some())
        {
            return Ok(());
        }
        let draft = self.document(c.parts.editor).unwrap().text().to_owned();
        match c.kind {
            CompositeKind::Spin(s) => {
                let Ok(current) = s.parse(&draft) else {
                    self.commit_edit(c.parts.editor)?;
                    return Ok(());
                };
                let next = (current + s.step * direction as f64).clamp(s.minimum, s.maximum);
                self.set_spin_value(owner, next)?;
            }
            CompositeKind::Time(s) => {
                let Ok(current) = s.parse(&draft) else {
                    self.commit_edit(c.parts.editor)?;
                    return Ok(());
                };
                let minimum = s.minimum.map_or(0, TimeValue::seconds);
                let maximum = s.maximum.map_or(
                    if s.show_seconds { 86399 } else { 86340 },
                    TimeValue::seconds,
                );
                let current = current.map_or(minimum, TimeValue::seconds);
                let next = (current + s.step_seconds as i32 * direction).clamp(minimum, maximum);
                self.set_time_value(owner, Some(TimeValue::from_seconds(next)))?;
            }
            _ => {}
        }
        Ok(())
    }
    pub(super) fn build_calendar_popup(
        &mut self,
        owner: WidgetId,
        month: DateValue,
    ) -> Result<(), ControlError> {
        let settings = self
            .date_settings(owner)
            .cloned()
            .ok_or_else(|| ControlError::Invalid("Not a DateEdit".into()))?;
        let desired = Size::new(294.0, 286.0);
        let surface = self.panel(
            self.overlay,
            LayoutStyle {
                width: Length::Px(desired.width),
                height: Length::Px(desired.height),
                padding: Edges::all(6.0),
                gap: 2.0,
                ..Default::default()
            },
        )?;
        self.controls.get_mut(&surface).unwrap().popup_surface = true;
        let title = self.transparent_panel(
            surface,
            LayoutStyle {
                kind: LayoutKind::Flex(Axis::Horizontal),
                height: Length::Px(34.0),
                flex_shrink: 0.0,
                ..Default::default()
            },
        )?;
        for direction in [-1, 1] {
            if direction == 1 {
                self.label(
                    title,
                    format!("{:02}.{:04}", month.month, month.year),
                    LayoutStyle {
                        width: Length::Px(0.0),
                        flex_grow: 1.0,
                        height: Length::Px(34.0),
                        ..Default::default()
                    },
                )?;
            }
            let button = self.button(
                title,
                if direction < 0 { "‹" } else { "›" },
                None,
                LayoutStyle {
                    width: Length::Px(32.0),
                    height: Length::Px(34.0),
                    flex_shrink: 0.0,
                    ..Default::default()
                },
            )?;
            self.set_button_chrome(button, ButtonChrome::Flat)?;
            self.rich
                .actions
                .insert(button, RichAction::Month { owner, direction });
            let next = month.month_offset(direction);
            self.set_enabled(button, next.year != month.year || next.month != month.month)?;
        }
        let headings = self.transparent_panel(
            surface,
            LayoutStyle {
                kind: LayoutKind::Flex(Axis::Horizontal),
                height: Length::Px(22.0),
                flex_shrink: 0.0,
                ..Default::default()
            },
        )?;
        for day in ["Пн", "Вт", "Ср", "Чт", "Пт", "Сб", "Вс"] {
            self.label(
                headings,
                day,
                LayoutStyle {
                    width: Length::Px(40.0),
                    height: Length::Px(22.0),
                    ..Default::default()
                },
            )?;
        }
        let first = DateValue { day: 1, ..month }.weekday() as i32;
        let days = month.days_in_month() as i32;
        for week in 0..6 {
            let row = self.transparent_panel(
                surface,
                LayoutStyle {
                    kind: LayoutKind::Flex(Axis::Horizontal),
                    height: Length::Px(32.0),
                    gap: 0.0,
                    flex_shrink: 0.0,
                    ..Default::default()
                },
            )?;
            for column in 0..7 {
                let day = week * 7 + column - first + 1;
                if day < 1 || day > days {
                    self.label(
                        row,
                        "",
                        LayoutStyle {
                            width: Length::Px(40.0),
                            height: Length::Px(32.0),
                            ..Default::default()
                        },
                    )?;
                    continue;
                }
                let value = DateValue {
                    day: day as u8,
                    ..month
                };
                let button = self.button(
                    row,
                    day.to_string(),
                    None,
                    LayoutStyle {
                        width: Length::Px(40.0),
                        height: Length::Px(32.0),
                        flex_shrink: 0.0,
                        ..Default::default()
                    },
                )?;
                self.set_automation_name(button, value.to_display(settings.format))?;
                self.set_button_chrome(button, ButtonChrome::Flat)?;
                self.controls.get_mut(&button).unwrap().checked = month == value;
                self.set_enabled(button, settings.check(Some(value)).is_ok())?;
                self.edit_props(button, |p| p.tab_stop = false)?;
                self.rich
                    .actions
                    .insert(button, RichAction::Date { owner, value });
            }
        }
        self.rich.popup = Some(RichPopup {
            owner,
            surface,
            list: None,
            desired,
            query: String::new(),
            month: Some(month),
        });
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
