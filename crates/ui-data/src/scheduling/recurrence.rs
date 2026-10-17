use super::*;
use calendar::{date_from_day, day_number, month_days};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ScheduleWeekday {
    Monday,
    Tuesday,
    Wednesday,
    Thursday,
    Friday,
    Saturday,
    Sunday,
}
impl ScheduleWeekday {
    fn index(self) -> i64 {
        self as i64
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum ScheduleFrequency {
    #[default]
    Once,
    Daily,
    /// Sorted distinct weekdays; weeks start on Monday.
    Weekly(Vec<ScheduleWeekday>),
    /// Uses the anchor day of month.
    Monthly,
    /// Uses the anchor month and day.
    Yearly,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ScheduleInvalidDate {
    #[default]
    Reject,
    Skip,
    LastDay,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ScheduleTermination {
    #[default]
    Never,
    /// Counts valid calendar candidates, including DST skips and cancellations.
    Count(u32),
    /// Inclusive final nominal local start, before timezone resolution.
    Until(ScheduleLocal),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScheduleRecurrence {
    pub frequency: ScheduleFrequency,
    pub interval: u16,
    pub end: ScheduleTermination,
    pub invalid_date: ScheduleInvalidDate,
}
impl Default for ScheduleRecurrence {
    fn default() -> Self {
        Self {
            frequency: ScheduleFrequency::Once,
            interval: 1,
            end: ScheduleTermination::Never,
            invalid_date: ScheduleInvalidDate::Reject,
        }
    }
}
impl ScheduleRecurrence {
    pub fn validate(&self, start: ScheduleLocal) -> Result<(), ScheduleError> {
        if !(1..=366).contains(&self.interval) {
            return Err(invalid("recurrence interval must be 1..366"));
        }
        if matches!(
            self.end,
            ScheduleTermination::Count(0 | 1_000_001..=u32::MAX)
        ) {
            return Err(invalid("recurrence count must be 1..1000000"));
        }
        if matches!(self.end,ScheduleTermination::Until(end) if end<start) {
            return Err(invalid("recurrence Until precedes anchor"));
        }
        if let ScheduleFrequency::Weekly(days) = &self.frequency
            && (days.is_empty() || days.len() > 7 || days.windows(2).any(|p| p[0] >= p[1]))
        {
            return Err(invalid(
                "weekly weekdays must be nonempty, sorted and distinct",
            ));
        }
        if self.frequency == ScheduleFrequency::Once
            && (self.interval != 1 || self.end != ScheduleTermination::Never)
        {
            return Err(invalid(
                "Once does not accept repetition interval or termination",
            ));
        }
        Ok(())
    }
    pub(super) fn visit(
        &self,
        anchor: ScheduleLocal,
        through: ScheduleLocal,
        work: &mut QueryWork,
        mut visitor: impl FnMut(ScheduleLocal) -> Result<bool, ScheduleError>,
    ) -> Result<(), ScheduleError> {
        self.validate(anchor)?;
        if through < anchor {
            return Ok(());
        }
        let through = match self.end {
            ScheduleTermination::Until(end) => through.min(end),
            _ => through,
        };
        let mut step = 0u64;
        let mut emitted = 0u32;
        loop {
            if matches!(self.end,ScheduleTermination::Count(limit) if emitted>=limit) {
                break;
            }
            if self.frequency == ScheduleFrequency::Once && step > 0 {
                break;
            }
            work.candidate()?;
            let Some((year, month, day)) = self.date_parts(anchor.date(), step, through.date())?
            else {
                break;
            };
            step = step
                .checked_add(1)
                .ok_or_else(|| invalid("recurrence counter overflow"))?;
            let last = month_days(year, month);
            let cutoff_day = if self.invalid_date == ScheduleInvalidDate::LastDay {
                day.min(last)
            } else {
                day
            };
            if (year, month, cutoff_day, anchor.time())
                > (
                    through.date().year(),
                    through.date().month(),
                    through.date().day(),
                    through.time(),
                )
            {
                break;
            }
            let day = if day > last {
                match self.invalid_date {
                    ScheduleInvalidDate::Reject => {
                        return Err(invalid("recurrence encounters an invalid calendar day"));
                    }
                    ScheduleInvalidDate::Skip => continue,
                    ScheduleInvalidDate::LastDay => last,
                }
            } else {
                day
            };
            let date =
                DateValue::new(year, month, day).map_err(|_| invalid("invalid recurrence date"))?;
            let local = ScheduleLocal::new(date, anchor.time())?;
            if local < anchor {
                continue;
            }
            if local > through {
                break;
            }
            emitted = emitted
                .checked_add(1)
                .ok_or_else(|| invalid("recurrence count overflow"))?;
            if !visitor(local)? {
                break;
            }
        }
        Ok(())
    }
    fn date_parts(
        &self,
        anchor: DateValue,
        step: u64,
        through: DateValue,
    ) -> Result<Option<(u16, u8, u8)>, ScheduleError> {
        let stride = step
            .checked_mul(u64::from(self.interval))
            .ok_or_else(|| invalid("recurrence stride overflow"))?;
        let date = match &self.frequency {
            ScheduleFrequency::Once => anchor,
            ScheduleFrequency::Daily => {
                let days = i64::try_from(stride)
                    .ok()
                    .and_then(|delta| day_number(anchor).checked_add(delta))
                    .ok_or_else(|| invalid("recurrence day overflow"))?;
                if days > day_number(through) {
                    return Ok(None);
                }
                date_from_day(days)?
            }
            ScheduleFrequency::Weekly(weekdays) => {
                let weeks = (step / weekdays.len() as u64)
                    .checked_mul(u64::from(self.interval))
                    .and_then(|n| n.checked_mul(7))
                    .and_then(|n| i64::try_from(n).ok())
                    .ok_or_else(|| invalid("recurrence week overflow"))?;
                let day = day_number(anchor) - day_number(anchor) % 7
                    + weeks
                    + weekdays[step as usize % weekdays.len()].index();
                if day > day_number(through) {
                    return Ok(None);
                }
                date_from_day(day)?
            }
            ScheduleFrequency::Monthly => {
                let months =
                    u64::from(anchor.year() - 1) * 12 + u64::from(anchor.month() - 1) + stride;
                if months >= 9999 * 12 {
                    return Ok(None);
                }
                let (year, month) = ((months / 12 + 1) as u16, (months % 12 + 1) as u8);
                if (year, month) > (through.year(), through.month()) {
                    return Ok(None);
                }
                return Ok(Some((year, month, anchor.day())));
            }
            ScheduleFrequency::Yearly => {
                let year = u64::from(anchor.year()) + stride;
                if year > u64::from(through.year()) || year > 9999 {
                    return Ok(None);
                }
                if (year as u16, anchor.month()) > (through.year(), through.month()) {
                    return Ok(None);
                }
                return Ok(Some((year as u16, anchor.month(), anchor.day())));
            }
        };
        Ok(Some((date.year(), date.month(), date.day())))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScheduleQueryLimits {
    pub max_candidates: usize,
    pub max_occurrences: usize,
    pub max_zone_checks: usize,
}
impl Default for ScheduleQueryLimits {
    fn default() -> Self {
        Self {
            max_candidates: 100_000,
            max_occurrences: 10_000,
            max_zone_checks: 1_000_000,
        }
    }
}
impl ScheduleQueryLimits {
    pub fn validate(self) -> Result<(), ScheduleError> {
        if self.max_candidates == 0
            || self.max_candidates > 1_000_000
            || self.max_occurrences == 0
            || self.max_occurrences > 100_000
            || self.max_zone_checks == 0
            || self.max_zone_checks > 10_000_000
        {
            return Err(invalid("invalid schedule query work/output limits"));
        }
        Ok(())
    }
}
pub(super) struct QueryWork {
    limits: ScheduleQueryLimits,
    candidates: usize,
    zone_checks: usize,
}
impl QueryWork {
    pub(super) fn new(limits: ScheduleQueryLimits) -> Result<Self, ScheduleError> {
        limits.validate()?;
        Ok(Self {
            limits,
            candidates: 0,
            zone_checks: 0,
        })
    }
    fn candidate(&mut self) -> Result<(), ScheduleError> {
        self.candidates = self
            .candidates
            .checked_add(1)
            .ok_or(ScheduleError::Capacity("candidate counter"))?;
        if self.candidates > self.limits.max_candidates {
            return Err(ScheduleError::Capacity("recurrence candidate work"));
        }
        Ok(())
    }
    pub(super) fn zone(&mut self, checks: usize) -> Result<(), ScheduleError> {
        self.zone_checks = self
            .zone_checks
            .checked_add(checks)
            .ok_or(ScheduleError::Capacity("timezone counter"))?;
        if self.zone_checks > self.limits.max_zone_checks {
            return Err(ScheduleError::Capacity("timezone resolution work"));
        }
        Ok(())
    }
}
