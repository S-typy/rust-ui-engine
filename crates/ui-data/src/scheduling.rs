//! UI-independent schedules with explicit finite timezone tables and recurrence.
use crate::{DateValue, TimeValue};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    fmt,
};

mod calendar;
mod recurrence;
mod store;
mod timezone;
pub use calendar::*;
pub use recurrence::*;
pub use store::*;
pub use timezone::*;
#[cfg(test)]
mod tests;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScheduleError {
    Invalid(String),
    Capacity(&'static str),
    Coverage(String),
    Gap(ScheduleLocal),
    Fold(ScheduleLocal),
    Conflict { expected: u64, actual: u64 },
    RevisionOverflow,
}
impl fmt::Display for ScheduleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(message) => write!(f, "schedule: {message}"),
            Self::Capacity(message) => write!(f, "schedule capacity: {message}"),
            Self::Coverage(zone) => write!(f, "outside timezone coverage: {zone}"),
            Self::Gap(local) => write!(f, "nonexistent local time: {local}"),
            Self::Fold(local) => write!(f, "ambiguous local time: {local}"),
            Self::Conflict { expected, actual } => {
                write!(f, "schedule revision {expected} differs from {actual}")
            }
            Self::RevisionOverflow => f.write_str("schedule revision exhausted"),
        }
    }
}
impl std::error::Error for ScheduleError {}
fn invalid(message: impl Into<String>) -> ScheduleError {
    ScheduleError::Invalid(message.into())
}
fn bounded_label(value: &str, max: usize) -> bool {
    !value.trim().is_empty() && value.len() <= max && !value.chars().any(char::is_control)
}
fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_./+-".contains(&b))
}
