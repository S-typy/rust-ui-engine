//! Gregorian dates and time-of-day editors with invariant storage.
use super::selection::{Composite, CompositeKind, EditorControl, RichAction};
use super::*;
use std::fmt::{Display, Formatter};

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
    pub(super) fn weekday(self) -> u8 {
        let offsets = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
        let y = self.year - i32::from(self.month < 3);
        ((y + y / 4 - y / 100 + y / 400 + offsets[self.month as usize - 1] + self.day as i32 + 6)
            % 7) as u8
    }
    /// Add whole months, retaining the day when possible and clamping it in
    /// shorter months. The supported Gregorian year range is never exceeded.
    pub fn checked_add_months(self, delta: i32) -> Option<Self> {
        self.validate().ok()?;
        let index = i64::from(self.year - 1) * 12 + i64::from(self.month - 1) + i64::from(delta);
        if !(0..9999 * 12).contains(&index) {
            return None;
        }
        let mut next = Self {
            year: (index / 12 + 1) as i32,
            month: (index % 12 + 1) as u8,
            day: 1,
        };
        next.day = self.day.min(next.days_in_month());
        Some(next)
    }
    pub fn checked_add_days(self, delta: i64) -> Option<Self> {
        self.validate().ok()?;
        Self::from_ordinal(self.ordinal().checked_add(delta)?)
    }
    pub(super) fn ordinal(self) -> i64 {
        let year = i64::from(self.year - 1);
        let mut day = year * 365 + year / 4 - year / 100 + year / 400 + i64::from(self.day - 1);
        for month in 1..self.month {
            day += i64::from(
                Self {
                    month,
                    day: 1,
                    ..self
                }
                .days_in_month(),
            );
        }
        day
    }
    pub(super) fn from_ordinal(day: i64) -> Option<Self> {
        if !(0..=3_652_058).contains(&day) {
            return None;
        }
        let (mut lo, mut hi) = (1, 10000);
        while lo + 1 < hi {
            let mid = (lo + hi) / 2;
            if (Self {
                year: mid,
                month: 1,
                day: 1,
            })
            .ordinal()
                <= day
            {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        let mut next = Self {
            year: lo,
            month: 1,
            day: 1,
        };
        let mut remaining = day - next.ordinal();
        while remaining >= i64::from(next.days_in_month()) {
            remaining -= i64::from(next.days_in_month());
            next.month += 1;
        }
        next.day = (remaining + 1) as u8;
        Some(next)
    }
    pub(super) fn month_offset(self, delta: i32) -> Self {
        self.checked_add_months(delta).unwrap_or(if delta < 0 {
            Self {
                year: 1,
                month: 1,
                day: 1,
            }
        } else {
            Self {
                year: 9999,
                month: 12,
                day: 31,
            }
        })
    }
    pub(super) fn day_offset(self, delta: i32) -> Self {
        self.checked_add_days(i64::from(delta))
            .unwrap_or(if delta < 0 {
                Self {
                    year: 1,
                    month: 1,
                    day: 1,
                }
            } else {
                Self {
                    year: 9999,
                    month: 12,
                    day: 31,
                }
            })
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
    pub(super) fn check(&self, value: Option<DateValue>) -> Result<(), String> {
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
    pub(super) fn parse(&self, text: &str) -> Result<Option<DateValue>, String> {
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
    pub(super) fn seconds(self) -> i32 {
        self.hour as i32 * 3600 + self.minute as i32 * 60 + self.second as i32
    }
    pub(super) fn from_seconds(value: i32) -> Self {
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

/// Gregorian local date and 24-hour time; it carries no time-zone or offset.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct DateTimeValue {
    pub date: DateValue,
    pub time: TimeValue,
}
impl DateTimeValue {
    pub fn checked_add_seconds(self, delta: i64) -> Option<Self> {
        self.validate().ok()?;
        let total =
            (self.date.ordinal() * 86400 + i64::from(self.time.seconds())).checked_add(delta)?;
        Some(Self {
            date: DateValue::from_ordinal(total.div_euclid(86400))?,
            time: TimeValue::from_seconds(total.rem_euclid(86400) as i32),
        })
    }
    pub fn new(date: DateValue, time: TimeValue) -> Result<Self, ControlError> {
        date.validate()?;
        time.validate()?;
        Ok(Self { date, time })
    }
    pub fn validate(self) -> Result<(), ControlError> {
        self.date.validate()?;
        self.time.validate()
    }
    pub fn parse_iso(text: &str) -> Result<Self, ControlError> {
        if text.len() != 19 || text.as_bytes().get(10) != Some(&b'T') {
            return Err(ControlError::Invalid("Expected YYYY-MM-DDTHH:MM:SS".into()));
        }
        Self::new(
            DateValue::parse_iso(&text[..10])?,
            TimeValue::parse(&text[11..])?,
        )
    }
    pub fn parse_display(text: &str, format: DateFormat) -> Result<Self, ControlError> {
        let Some((date, time)) = text.trim().split_once(' ') else {
            return Err(ControlError::Invalid(
                "Expected date and time separated by a space".into(),
            ));
        };
        Self::new(
            DateValue::parse_display(date, format)?,
            TimeValue::parse(time)?,
        )
    }
    pub fn to_iso(self) -> String {
        format!("{}T{}", self.date.to_iso(), self.time.to_iso())
    }
    pub fn to_display(self, format: DateFormat, seconds: bool) -> String {
        format!(
            "{} {}",
            self.date.to_display(format),
            self.time.to_display(seconds)
        )
    }
}
impl Display for DateTimeValue {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_iso())
    }
}

/// A complete inclusive date interval. An editor's null value is `None`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DateRangeValue {
    pub start: DateValue,
    pub end: DateValue,
}
impl DateRangeValue {
    pub fn new(start: DateValue, end: DateValue) -> Result<Self, ControlError> {
        let value = Self { start, end };
        value.validate()?;
        Ok(value)
    }
    pub fn validate(self) -> Result<(), ControlError> {
        self.start.validate()?;
        self.end.validate()?;
        if self.start > self.end {
            return Err(ControlError::Invalid(
                "Range start must not exceed end".into(),
            ));
        }
        Ok(())
    }
    pub fn parse_iso(text: &str) -> Result<Self, ControlError> {
        let Some((start, end)) = text.split_once('/') else {
            return Err(ControlError::Invalid(
                "Expected YYYY-MM-DD/YYYY-MM-DD".into(),
            ));
        };
        Self::new(DateValue::parse_iso(start)?, DateValue::parse_iso(end)?)
    }
    pub fn parse_display(text: &str, format: DateFormat) -> Result<Self, ControlError> {
        let Some((start, end)) = text.split_once(" – ").or_else(|| text.split_once(" - ")) else {
            return Err(ControlError::Invalid(
                "Введите обе даты диапазона через « – ».".into(),
            ));
        };
        Self::new(
            DateValue::parse_display(start, format)?,
            DateValue::parse_display(end, format)?,
        )
    }
    pub fn to_iso(self) -> String {
        format!("{}/{}", self.start.to_iso(), self.end.to_iso())
    }
    pub fn to_display(self, format: DateFormat) -> String {
        format!(
            "{} – {}",
            self.start.to_display(format),
            self.end.to_display(format)
        )
    }
}
impl Display for DateRangeValue {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_iso())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DateTimeEditSettings {
    pub minimum: Option<DateTimeValue>,
    pub maximum: Option<DateTimeValue>,
    pub allow_empty: bool,
    pub date_format: DateFormat,
    pub show_seconds: bool,
    pub step_seconds: u32,
}
impl Default for DateTimeEditSettings {
    fn default() -> Self {
        Self {
            minimum: None,
            maximum: None,
            allow_empty: true,
            date_format: DateFormat::Russian,
            show_seconds: false,
            step_seconds: 60,
        }
    }
}
impl DateTimeEditSettings {
    pub fn validate(&self) -> Result<(), ControlError> {
        for value in [self.minimum, self.maximum].into_iter().flatten() {
            value.validate()?;
        }
        if self.step_seconds == 0
            || self.step_seconds > 86400
            || matches!((self.minimum,self.maximum),(Some(min),Some(max)) if min>max)
        {
            return Err(ControlError::Invalid(
                "DateTime requires ordered bounds and TimeStep in 1..86400 seconds".into(),
            ));
        }
        Ok(())
    }
    pub(super) fn check(&self, value: Option<DateTimeValue>) -> Result<(), String> {
        if let Some(value) = value {
            value.validate().map_err(|e| e.to_string())?;
            if self.minimum.is_some_and(|v| value < v) || self.maximum.is_some_and(|v| value > v) {
                return Err("Дата и время вне допустимого диапазона.".into());
            }
        } else if !self.allow_empty {
            return Err("Укажите дату и время.".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DateRangeEditSettings {
    pub minimum: Option<DateValue>,
    pub maximum: Option<DateValue>,
    pub allow_empty: bool,
    pub format: DateFormat,
}
impl Default for DateRangeEditSettings {
    fn default() -> Self {
        Self {
            minimum: None,
            maximum: None,
            allow_empty: true,
            format: DateFormat::Russian,
        }
    }
}
impl DateRangeEditSettings {
    pub fn validate(&self) -> Result<(), ControlError> {
        DateEditSettings {
            minimum: self.minimum,
            maximum: self.maximum,
            allow_empty: self.allow_empty,
            format: self.format,
        }
        .validate()
    }
    pub(super) fn check(&self, value: Option<DateRangeValue>) -> Result<(), String> {
        if let Some(value) = value {
            value.validate().map_err(|e| e.to_string())?;
            if self.minimum.is_some_and(|v| value.start < v)
                || self.maximum.is_some_and(|v| value.end > v)
            {
                return Err("Даты вне допустимого диапазона.".into());
            }
        } else if !self.allow_empty {
            return Err("Укажите обе даты диапазона.".into());
        }
        Ok(())
    }
    pub(super) fn parse(&self, text: &str) -> Result<Option<DateRangeValue>, String> {
        let value = if text.trim().is_empty() {
            None
        } else {
            Some(DateRangeValue::parse_display(text, self.format).map_err(|e| e.to_string())?)
        };
        self.check(value)?;
        Ok(value)
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
            || matches!((self.minimum,self.maximum), (Some(min),Some(max)) if min > max)
        {
            return Err(ControlError::Invalid(
                "Time requires ordered bounds and TimeStep in 1..86400 seconds".into(),
            ));
        }
        Ok(())
    }
    pub(super) fn check(&self, value: Option<TimeValue>) -> Result<(), String> {
        if let Some(value) = value {
            value.validate().map_err(|e| e.to_string())?;
            if self.minimum.is_some_and(|min| value < min)
                || self.maximum.is_some_and(|max| value > max)
            {
                return Err("Время вне допустимого диапазона.".into());
            }
        } else if !self.allow_empty {
            return Err("Укажите время.".into());
        }
        Ok(())
    }
    pub(super) fn parse(&self, text: &str) -> Result<Option<TimeValue>, String> {
        let value = if text.trim().is_empty() {
            None
        } else {
            Some(TimeValue::parse(text).map_err(|e| e.to_string())?)
        };
        self.check(value)?;
        Ok(value)
    }
}

impl Controls {
    pub fn set_date_time_settings(
        &mut self,
        id: WidgetId,
        settings: DateTimeEditSettings,
    ) -> Result<(), ControlError> {
        settings.validate()?;
        self.set_temporal_settings(id, CompositeKind::DateTime(settings))
    }
    pub fn set_date_range_settings(
        &mut self,
        id: WidgetId,
        settings: DateRangeEditSettings,
    ) -> Result<(), ControlError> {
        settings.validate()?;
        self.set_temporal_settings(id, CompositeKind::DateRange(settings))
    }
    fn set_temporal_settings(
        &mut self,
        id: WidgetId,
        kind: CompositeKind,
    ) -> Result<(), ControlError> {
        let old = self
            .rich
            .composites
            .get(&id)
            .ok_or_else(|| ControlError::Invalid("Unknown temporal editor".into()))?
            .clone();
        let format_changed = match (&old.kind, &kind) {
            (CompositeKind::Date(a), CompositeKind::Date(b)) => a.format != b.format,
            (CompositeKind::Time(a), CompositeKind::Time(b)) => a.show_seconds != b.show_seconds,
            (CompositeKind::DateTime(a), CompositeKind::DateTime(b)) => {
                a.date_format != b.date_format || a.show_seconds != b.show_seconds
            }
            (CompositeKind::DateRange(a), CompositeKind::DateRange(b)) => a.format != b.format,
            _ => {
                return Err(ControlError::Invalid(
                    "Temporal settings kind mismatch".into(),
                ));
            }
        };
        if format_changed
            && (self.is_edit_dirty(old.parts.editor) == Some(true)
                || self
                    .document(old.parts.editor)
                    .is_some_and(|doc| doc.composition().is_some()))
        {
            return Err(ControlError::Invalid(
                "Commit or cancel the draft before changing temporal display format".into(),
            ));
        }
        let value = self.rich.value_baselines[&id].clone();
        self.rich.composites.get_mut(&id).unwrap().kind = kind;
        self.refresh_temporal_parser(id);
        let parser = self.temporal_value_parser(id).unwrap();
        let result = parser
            .format_value(&value)
            .map_err(ControlError::Invalid)
            .and_then(|text| {
                if format_changed {
                    match self.set_temporal_value(id, value, &text)? {
                        CommitOutcome::Invalid(message) => Err(ControlError::Invalid(message)),
                        _ => Ok(()),
                    }
                } else {
                    Ok(())
                }
            });
        if result.is_err() {
            self.rich.composites.insert(id, old);
            self.refresh_temporal_parser(id);
        } else {
            self.refresh_temporal_popup(id)?;
        }
        result
    }
    pub(super) fn parse_temporal_draft(
        &self,
        owner: WidgetId,
        text: &str,
    ) -> Result<Option<EditorValue>, String> {
        self.temporal_value_parser(owner)
            .map(|parser| parser.parse(text))
            .transpose()
    }
    fn set_temporal_value(
        &mut self,
        owner: WidgetId,
        value: EditorValue,
        text: &str,
    ) -> Result<CommitOutcome, ControlError> {
        let editor = self.rich.composites[&owner].parts.editor;
        let parser = self
            .temporal_value_parser(owner)
            .ok_or_else(|| ControlError::Invalid("Missing temporal parser".into()))?;
        let previous = parser.assignment(Some(value.clone()));
        let result = self.set_editor_value(editor, text);
        parser.assignment(previous);
        let outcome = result?;
        if matches!(outcome, CommitOutcome::Committed | CommitOutcome::Unchanged)
            && self.rich.value_baselines.get(&owner) != Some(&value)
        {
            self.rich.value_baselines.insert(owner, value.clone());
            self.rich.values.push(ValueChanged { id: owner, value });
            self.refresh_temporal_parser(owner);
            self.refresh_temporal_popup(owner)?;
            return Ok(CommitOutcome::Committed);
        }
        if matches!(outcome, CommitOutcome::Committed | CommitOutcome::Unchanged) {
            self.refresh_temporal_popup(owner)?;
        }
        Ok(outcome)
    }

    pub fn date_time_edit(
        &mut self,
        parent: WidgetId,
        label: impl Into<String>,
        value: Option<DateTimeValue>,
        settings: DateTimeEditSettings,
        style: LayoutStyle,
    ) -> Result<EditorControl, ControlError> {
        settings.validate()?;
        settings.check(value).map_err(ControlError::Invalid)?;
        let text = value.map_or(String::new(), |v| {
            v.to_display(settings.date_format, settings.show_seconds)
        });
        let mut parts = self.create_editor_parts(parent, label.into(), &text, style)?;
        let button =
            self.add_editor_button(&parts, "Открыть календарь", Some(IconSymbol::ChevronDown))?;
        self.rich
            .actions
            .insert(button, RichAction::Popup(parts.container));
        parts.buttons.push(button);
        self.add_step_buttons(&mut parts)?;
        self.rich.composites.insert(
            parts.container,
            Composite {
                parts: parts.clone(),
                kind: CompositeKind::DateTime(settings),
                read_only: false,
            },
        );
        self.rich
            .value_baselines
            .insert(parts.container, EditorValue::DateTime(value));
        self.refresh_temporal_parser(parts.container);
        Ok(parts)
    }
    pub fn date_range_edit(
        &mut self,
        parent: WidgetId,
        label: impl Into<String>,
        value: Option<DateRangeValue>,
        settings: DateRangeEditSettings,
        style: LayoutStyle,
    ) -> Result<EditorControl, ControlError> {
        settings.validate()?;
        settings.check(value).map_err(ControlError::Invalid)?;
        let text = value.map_or(String::new(), |v| v.to_display(settings.format));
        let mut parts = self.create_editor_parts(parent, label.into(), &text, style)?;
        let button = self.add_editor_button(
            &parts,
            "Выбрать диапазон дат",
            Some(IconSymbol::ChevronDown),
        )?;
        self.rich
            .actions
            .insert(button, RichAction::Popup(parts.container));
        parts.buttons.push(button);
        self.rich.composites.insert(
            parts.container,
            Composite {
                parts: parts.clone(),
                kind: CompositeKind::DateRange(settings),
                read_only: false,
            },
        );
        self.rich
            .value_baselines
            .insert(parts.container, EditorValue::DateRange(value));
        self.refresh_temporal_parser(parts.container);
        Ok(parts)
    }
    pub fn date_time_value(&self, id: WidgetId) -> Option<Option<DateTimeValue>> {
        match self.rich.value_baselines.get(&id)? {
            EditorValue::DateTime(value) => Some(*value),
            _ => None,
        }
    }
    pub fn date_range_value(&self, id: WidgetId) -> Option<Option<DateRangeValue>> {
        match self.rich.value_baselines.get(&id)? {
            EditorValue::DateRange(value) => Some(*value),
            _ => None,
        }
    }
    pub fn date_time_settings(&self, id: WidgetId) -> Option<&DateTimeEditSettings> {
        match &self.rich.composites.get(&id)?.kind {
            CompositeKind::DateTime(settings) => Some(settings),
            _ => None,
        }
    }
    pub fn date_range_settings(&self, id: WidgetId) -> Option<&DateRangeEditSettings> {
        match &self.rich.composites.get(&id)?.kind {
            CompositeKind::DateRange(settings) => Some(settings),
            _ => None,
        }
    }
    pub fn set_date_time_value(
        &mut self,
        id: WidgetId,
        value: Option<DateTimeValue>,
    ) -> Result<CommitOutcome, ControlError> {
        let settings = self
            .date_time_settings(id)
            .ok_or_else(|| ControlError::Invalid("Not a DateTimeEdit".into()))?;
        if let Err(error) = settings.check(value) {
            return Ok(CommitOutcome::Invalid(error));
        }
        let text = value.map_or(String::new(), |v| {
            v.to_display(settings.date_format, settings.show_seconds)
        });
        self.set_temporal_value(id, EditorValue::DateTime(value), &text)
    }
    pub fn set_date_range_value(
        &mut self,
        id: WidgetId,
        value: Option<DateRangeValue>,
    ) -> Result<CommitOutcome, ControlError> {
        let settings = self
            .date_range_settings(id)
            .ok_or_else(|| ControlError::Invalid("Not a DateRangeEdit".into()))?;
        if let Err(error) = settings.check(value) {
            return Ok(CommitOutcome::Invalid(error));
        }
        let text = value.map_or(String::new(), |v| v.to_display(settings.format));
        self.set_temporal_value(id, EditorValue::DateRange(value), &text)
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
        self.refresh_temporal_parser(parts.container);
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
        self.refresh_temporal_parser(parts.container);
        Ok(parts)
    }
    pub fn date_value(&self, id: WidgetId) -> Option<Option<DateValue>> {
        let c = self.rich.composites.get(&id)?;
        let CompositeKind::Date(settings) = &c.kind else {
            return None;
        };
        settings.parse(self.accepted_value(c.parts.editor)?).ok()
    }
    pub fn time_value(&self, id: WidgetId) -> Option<Option<TimeValue>> {
        match self.rich.value_baselines.get(&id)? {
            EditorValue::Time(value) => Some(*value),
            _ => None,
        }
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
        let result = self.set_temporal_value(id, EditorValue::Date(value), &text)?;
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
        self.set_temporal_value(id, EditorValue::Time(value), &text)
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
    pub fn set_date_settings(
        &mut self,
        id: WidgetId,
        settings: DateEditSettings,
    ) -> Result<(), ControlError> {
        settings.validate()?;
        self.set_temporal_settings(id, CompositeKind::Date(settings))
    }

    pub fn set_time_settings(
        &mut self,
        id: WidgetId,
        settings: TimeEditSettings,
    ) -> Result<(), ControlError> {
        settings.validate()?;
        self.set_temporal_settings(id, CompositeKind::Time(settings))
    }
}
