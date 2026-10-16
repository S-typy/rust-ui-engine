use super::*;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CalendarView {
    #[default]
    Days,
    Months,
    Years,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CalendarSelection {
    Single(Option<DateValue>),
    Range(Option<DateRangeValue>),
}
impl Default for CalendarSelection {
    fn default() -> Self {
        Self::Single(None)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CalendarSettings {
    pub minimum: Option<DateValue>,
    pub maximum: Option<DateValue>,
    pub format: DateFormat,
    pub is_read_only: bool,
}
impl Default for CalendarSettings {
    fn default() -> Self {
        Self {
            minimum: None,
            maximum: None,
            format: DateFormat::Russian,
            is_read_only: false,
        }
    }
}
impl CalendarSettings {
    pub fn validate(&self) -> Result<(), ControlError> {
        DateEditSettings {
            minimum: self.minimum,
            maximum: self.maximum,
            allow_empty: true,
            format: self.format,
        }
        .validate()
    }
    pub(super) fn minimum(&self) -> DateValue {
        self.minimum.unwrap_or(DateValue {
            year: 1,
            month: 1,
            day: 1,
        })
    }
    pub(super) fn maximum(&self) -> DateValue {
        self.maximum.unwrap_or(DateValue {
            year: 9999,
            month: 12,
            day: 31,
        })
    }
    pub(super) fn contains(&self, value: DateValue) -> bool {
        value >= self.minimum() && value <= self.maximum()
    }
    pub(super) fn clamp(&self, value: DateValue) -> DateValue {
        value.max(self.minimum()).min(self.maximum())
    }
    pub(super) fn check(&self, selection: CalendarSelection) -> Result<(), ControlError> {
        match selection {
            CalendarSelection::Single(value) => {
                if let Some(value) = value {
                    value.validate()?;
                    if !self.contains(value) {
                        return Err(ControlError::Invalid(
                            "Calendar date is outside bounds".into(),
                        ));
                    }
                }
            }
            CalendarSelection::Range(value) => {
                if let Some(value) = value {
                    value.validate()?;
                    if !self.contains(value.start) || !self.contains(value.end) {
                        return Err(ControlError::Invalid(
                            "Calendar range is outside bounds".into(),
                        ));
                    }
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone)]
pub(super) struct CalendarModel {
    pub settings: CalendarSettings,
    pub selection: CalendarSelection,
    pub cursor: DateValue,
    pub view: CalendarView,
    pub range_anchor: Option<DateValue>,
}
impl CalendarModel {
    pub fn new(
        selection: CalendarSelection,
        settings: CalendarSettings,
    ) -> Result<Self, ControlError> {
        settings.validate()?;
        settings.check(selection)?;
        let cursor = settings.clamp(
            match selection {
                CalendarSelection::Single(value) => value,
                CalendarSelection::Range(value) => value.map(|v| v.start),
            }
            .unwrap_or_else(DateValue::today_utc),
        );
        Ok(Self {
            settings,
            selection,
            cursor,
            view: CalendarView::Days,
            range_anchor: None,
        })
    }
    pub fn set_selection(&mut self, selection: CalendarSelection) -> Result<bool, ControlError> {
        self.settings.check(selection)?;
        let changed = self.selection != selection;
        self.selection = selection;
        self.range_anchor = None;
        if let Some(date) = match selection {
            CalendarSelection::Single(value) => value,
            CalendarSelection::Range(value) => value.map(|v| v.start),
        } {
            self.cursor = date;
        }
        Ok(changed)
    }
    pub fn activate(&mut self, date: DateValue) -> Result<bool, ControlError> {
        date.validate()?;
        if self.settings.is_read_only || !self.settings.contains(date) {
            return Ok(false);
        }
        self.cursor = date;
        match self.selection {
            CalendarSelection::Single(_) => {
                self.set_selection(CalendarSelection::Single(Some(date)))
            }
            CalendarSelection::Range(_) => {
                if let Some(start) = self.range_anchor.take() {
                    let value = DateRangeValue::new(start.min(date), start.max(date))?;
                    self.set_selection(CalendarSelection::Range(Some(value)))
                } else {
                    self.range_anchor = Some(date);
                    Ok(false)
                }
            }
        }
    }
    pub fn page(&mut self, direction: i32) {
        let months = match self.view {
            CalendarView::Days => 1,
            CalendarView::Months => 12,
            CalendarView::Years => 144,
        };
        self.cursor = self
            .settings
            .clamp(self.cursor.month_offset(direction.signum() * months));
    }
    pub fn can_page(&self, direction: i32) -> bool {
        let mut next = self.clone();
        next.page(direction);
        match self.view {
            CalendarView::Days => {
                (next.cursor.year, next.cursor.month) != (self.cursor.year, self.cursor.month)
            }
            CalendarView::Months => next.cursor.year != self.cursor.year,
            CalendarView::Years => (next.cursor.year - 1) / 12 != (self.cursor.year - 1) / 12,
        }
    }
    pub fn days(&self) -> [Option<DateValue>; 42] {
        let first = DateValue {
            day: 1,
            ..self.cursor
        };
        std::array::from_fn(|i| first.checked_add_days(i as i64 - i64::from(first.weekday())))
    }
    pub fn interval_enabled(&self, start: DateValue, end: DateValue) -> bool {
        end >= self.settings.minimum() && start <= self.settings.maximum()
    }
    pub fn selected(&self, date: DateValue) -> bool {
        if let Some(start) = self.range_anchor {
            return date >= start.min(self.cursor) && date <= start.max(self.cursor);
        }
        match self.selection {
            CalendarSelection::Single(value) => value == Some(date),
            CalendarSelection::Range(Some(value)) => date >= value.start && date <= value.end,
            CalendarSelection::Range(None) => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn d(y: i32, m: u8, day: u8) -> DateValue {
        DateValue::new(y, m, day).unwrap()
    }
    #[test]
    fn gregorian_arithmetic_handles_centuries_extremes_and_preserves_month_day() {
        assert!(DateValue::new(1900, 2, 29).is_err());
        assert!(DateValue::new(2000, 2, 29).is_ok());
        assert_eq!(d(2024, 1, 31).checked_add_months(1), Some(d(2024, 2, 29)));
        assert_eq!(d(2023, 1, 31).checked_add_months(1), Some(d(2023, 2, 28)));
        assert_eq!(d(2000, 2, 28).checked_add_days(2), Some(d(2000, 3, 1)));
        assert_eq!(d(1900, 2, 28).checked_add_days(1), Some(d(1900, 3, 1)));
        assert_eq!(d(1, 1, 1).checked_add_days(-1), None);
        assert_eq!(d(9999, 12, 31).checked_add_days(1), None);
        assert_eq!(
            d(1, 1, 1).checked_add_days(3_652_058),
            Some(d(9999, 12, 31))
        );
        assert_eq!(
            d(9999, 12, 31).checked_add_days(-3_652_058),
            Some(d(1, 1, 1))
        );
        assert_eq!(d(2024, 1, 1).checked_add_days(i64::MAX), None);
    }
    #[test]
    fn calendar_range_draft_is_ordered_only_after_second_choice_and_bounds_are_atomic() {
        let mut model = CalendarModel::new(
            CalendarSelection::Range(None),
            CalendarSettings {
                minimum: Some(d(2024, 2, 20)),
                maximum: Some(d(2024, 3, 5)),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(!model.activate(d(2024, 3, 4)).unwrap());
        assert_eq!(model.selection, CalendarSelection::Range(None));
        assert!(model.activate(d(2024, 2, 29)).unwrap());
        let accepted = CalendarSelection::Range(Some(
            DateRangeValue::new(d(2024, 2, 29), d(2024, 3, 4)).unwrap(),
        ));
        assert_eq!(model.selection, accepted);
        assert!(!model.activate(d(2024, 3, 6)).unwrap());
        assert_eq!(model.selection, accepted);
        assert!(
            model
                .set_selection(CalendarSelection::Single(Some(d(2024, 2, 19))))
                .is_err()
        );
        assert_eq!(model.selection, accepted);
    }
    #[test]
    fn calendar_pages_respect_bounds_in_days_months_years_and_calendar_edges() {
        let mut model = CalendarModel::new(
            CalendarSelection::Single(Some(d(2024, 1, 31))),
            CalendarSettings {
                minimum: Some(d(2023, 12, 15)),
                maximum: Some(d(2024, 2, 29)),
                ..Default::default()
            },
        )
        .unwrap();
        model.page(1);
        assert_eq!(model.cursor, d(2024, 2, 29));
        assert!(!model.can_page(1));
        model.page(-1);
        assert_eq!(model.cursor, d(2024, 1, 29));
        model.view = CalendarView::Months;
        model.page(-1);
        assert_eq!(model.cursor, d(2023, 12, 15));
        assert!(!model.can_page(-1));
        model.view = CalendarView::Years;
        model.page(1);
        assert_eq!(model.cursor, d(2024, 2, 29));
        assert!(!model.can_page(1));
        let edge = CalendarModel::new(
            CalendarSelection::Single(Some(d(1, 1, 1))),
            CalendarSettings::default(),
        )
        .unwrap();
        assert_eq!(edge.days()[0], Some(d(1, 1, 1)));
        let edge = CalendarModel::new(
            CalendarSelection::Single(Some(d(9999, 12, 31))),
            CalendarSettings::default(),
        )
        .unwrap();
        assert!(edge.days().contains(&None));
    }
}
