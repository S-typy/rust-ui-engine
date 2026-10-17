use super::*;

const EPOCH_DAYS: i64 = 719162;
const LAST_DAY: i64 = 3652058;
fn days_before_year(year: u16) -> i64 {
    let year = i64::from(year) - 1;
    year * 365 + year / 4 - year / 100 + year / 400
}
pub(super) fn month_days(year: u16, month: u8) -> u8 {
    (28..=31)
        .rev()
        .find(|day| DateValue::new(year, month, *day).is_ok())
        .unwrap_or(0)
}
pub(super) fn day_number(date: DateValue) -> i64 {
    days_before_year(date.year())
        + (1..date.month())
            .map(|m| i64::from(month_days(date.year(), m)))
            .sum::<i64>()
        + i64::from(date.day())
        - 1
}
pub(super) fn date_from_day(day: i64) -> Result<DateValue, ScheduleError> {
    if !(0..=LAST_DAY).contains(&day) {
        return Err(invalid("calendar year must be in 0001..9999"));
    }
    let (mut low, mut high) = (1u16, 10000u16);
    while low + 1 < high {
        let middle = low + (high - low) / 2;
        if days_before_year(middle) <= day {
            low = middle;
        } else {
            high = middle;
        }
    }
    let mut remaining = day - days_before_year(low);
    for month in 1..=12 {
        let length = i64::from(month_days(low, month));
        if remaining < length {
            return DateValue::new(low, month, remaining as u8 + 1)
                .map_err(|_| invalid("invalid calendar day"));
        }
        remaining -= length;
    }
    Err(invalid("invalid calendar day"))
}

/// Unix seconds without leap seconds. Arithmetic is checked; conversion to a
/// local civil date is restricted to the DateValue range 0001..9999.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ScheduleInstant(pub i64);
impl ScheduleInstant {
    pub fn checked_add(self, seconds: i64) -> Result<Self, ScheduleError> {
        self.0
            .checked_add(seconds)
            .map(Self)
            .ok_or_else(|| invalid("UTC seconds overflow"))
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ScheduleLocal {
    date: DateValue,
    time: TimeValue,
}
impl ScheduleLocal {
    pub fn new(date: DateValue, time: TimeValue) -> Result<Self, ScheduleError> {
        if time.nanosecond() != 0 {
            return Err(invalid(
                "schedules require whole seconds; fractional time is unsupported",
            ));
        }
        Ok(Self { date, time })
    }
    pub fn date(self) -> DateValue {
        self.date
    }
    pub fn time(self) -> TimeValue {
        self.time
    }
    pub fn checked_add(self, seconds: i64) -> Result<Self, ScheduleError> {
        Self::from_seconds(
            self.seconds()
                .checked_add(seconds)
                .ok_or_else(|| invalid("local seconds overflow"))?,
        )
    }
    pub(super) fn seconds(self) -> i64 {
        (day_number(self.date) - EPOCH_DAYS) * 86400
            + i64::from(self.time.hour()) * 3600
            + i64::from(self.time.minute()) * 60
            + i64::from(self.time.second())
    }
    pub(super) fn from_seconds(seconds: i64) -> Result<Self, ScheduleError> {
        let day = seconds
            .div_euclid(86400)
            .checked_add(EPOCH_DAYS)
            .ok_or_else(|| invalid("calendar overflow"))?;
        let clock = seconds.rem_euclid(86400);
        Self::new(
            date_from_day(day)?,
            TimeValue::new(
                (clock / 3600) as u8,
                ((clock % 3600) / 60) as u8,
                (clock % 60) as u8,
                0,
            )
            .map_err(|_| invalid("invalid clock"))?,
        )
    }
}
impl std::str::FromStr for ScheduleLocal {
    type Err = ScheduleError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let (date, time) = value
            .split_once('T')
            .ok_or_else(|| invalid("local time requires YYYY-MM-DDTHH:MM:SS"))?;
        let local = Self::new(
            date.parse().map_err(|_| invalid("invalid local date"))?,
            time.parse().map_err(|_| invalid("invalid local clock"))?,
        )?;
        if local.to_string() != value {
            return Err(invalid(
                "local time requires canonical whole-second spelling",
            ));
        }
        Ok(local)
    }
}
impl fmt::Display for ScheduleLocal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}T{}", self.date, self.time)
    }
}

/// Half-open UTC interval: start is included and end is excluded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScheduleUtcInterval {
    pub start: ScheduleInstant,
    pub end: ScheduleInstant,
}
impl ScheduleUtcInterval {
    pub fn new(start: ScheduleInstant, end: ScheduleInstant) -> Result<Self, ScheduleError> {
        if start >= end {
            return Err(invalid("UTC interval requires start < end"));
        }
        Ok(Self { start, end })
    }
    pub fn overlaps(self, other: Self) -> bool {
        self.start < other.end && other.start < self.end
    }
}
/// Half-open wall-clock interval. Resolved UTC duration can differ across DST.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScheduleLocalInterval {
    pub start: ScheduleLocal,
    pub end: ScheduleLocal,
}
impl ScheduleLocalInterval {
    pub fn new(start: ScheduleLocal, end: ScheduleLocal) -> Result<Self, ScheduleError> {
        if start >= end {
            return Err(invalid("local interval requires start < end"));
        }
        Ok(Self { start, end })
    }
    pub fn wall_seconds(self) -> i64 {
        self.end.seconds() - self.start.seconds()
    }
}
