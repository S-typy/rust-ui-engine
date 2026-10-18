//! Bounded, transactional task planning over explicit UTC working calendars.
//!
//! Calendars contain finite half-open working intervals. No system calendar,
//! timezone database, resource levelling or background rescheduling is implied.
use crate::{ScheduleInstant, ScheduleUtcInterval};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

pub const MAX_GANTT_TASKS: usize = 4096;
pub const MAX_GANTT_DEPENDENCIES: usize = 16384;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GanttError(pub String);
impl fmt::Display for GanttError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "task plan: {}", self.0)
    }
}
impl std::error::Error for GanttError {}
fn invalid(text: impl Into<String>) -> GanttError {
    GanttError(text.into())
}
fn add(value: ScheduleInstant, seconds: i64) -> Result<ScheduleInstant, GanttError> {
    value
        .checked_add(seconds)
        .map_err(|e| invalid(e.to_string()))
}
fn label(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 4096 && !value.chars().any(char::is_control)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GanttTaskId(pub u64);
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GanttCalendarId(pub u64);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GanttCalendar {
    pub id: GanttCalendarId,
    pub label: String,
    /// Ordered, non-overlapping working intervals; adjacent intervals are allowed.
    pub working: Vec<ScheduleUtcInterval>,
}
impl GanttCalendar {
    pub fn validate(&self) -> Result<(), GanttError> {
        if !label(&self.label) || self.working.is_empty() || self.working.len() > 16384 {
            return Err(invalid(
                "calendar needs a label and 1..16384 working intervals",
            ));
        }
        for (i, interval) in self.working.iter().enumerate() {
            if interval.start >= interval.end
                || interval.end.0.checked_sub(interval.start.0).is_none()
                || (i > 0 && self.working[i - 1].end > interval.start)
            {
                return Err(invalid(
                    "calendar intervals must be finite, ordered and disjoint",
                ));
            }
        }
        Ok(())
    }
    fn next(&self, instant: ScheduleInstant) -> Result<ScheduleInstant, GanttError> {
        self.working
            .iter()
            .find(|slot| slot.end > instant)
            .map(|slot| slot.start.max(instant))
            .ok_or_else(|| invalid("working calendar coverage exhausted"))
    }
    fn finish(&self, start: ScheduleInstant, mut work: i64) -> Result<ScheduleInstant, GanttError> {
        if work <= 0 {
            return Err(invalid("task duration must be positive"));
        }
        for slot in &self.working {
            let from = slot.start.max(start);
            if from >= slot.end {
                continue;
            }
            let available = slot
                .end
                .0
                .checked_sub(from.0)
                .ok_or_else(|| invalid("calendar arithmetic overflow"))?;
            if work <= available {
                return add(from, work);
            }
            work -= available;
        }
        Err(invalid("working calendar cannot fit task duration"))
    }
    fn start_before(
        &self,
        end: ScheduleInstant,
        mut work: i64,
    ) -> Result<ScheduleInstant, GanttError> {
        for slot in self.working.iter().rev() {
            let until = slot.end.min(end);
            if until <= slot.start {
                continue;
            }
            let available = until
                .0
                .checked_sub(slot.start.0)
                .ok_or_else(|| invalid("calendar arithmetic overflow"))?;
            if work <= available {
                return add(until, -work);
            }
            work -= available;
        }
        Err(invalid("working calendar cannot fit task before deadline"))
    }
    fn on_or_before(&self, instant: ScheduleInstant) -> Result<ScheduleInstant, GanttError> {
        self.working
            .iter()
            .rev()
            .find(|s| s.start <= instant)
            .map(|s| instant.min(ScheduleInstant(s.end.0 - 1)))
            .ok_or_else(|| invalid("no working start before constraint"))
    }
    fn working_between(
        &self,
        start: ScheduleInstant,
        end: ScheduleInstant,
    ) -> Result<i64, GanttError> {
        self.working.iter().try_fold(0i64, |sum, slot| {
            let from = start.max(slot.start);
            let to = end.min(slot.end);
            let span = if to > from {
                to.0.checked_sub(from.0)
                    .ok_or_else(|| invalid("slack overflow"))?
            } else {
                0
            };
            sum.checked_add(span)
                .ok_or_else(|| invalid("slack overflow"))
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GanttTask {
    pub id: GanttTaskId,
    pub parent: Option<GanttTaskId>,
    /// Sibling order. Identity, never position, owns selection and editing.
    pub order: i64,
    pub title: String,
    pub calendar: GanttCalendarId,
    pub work_seconds: i64,
    pub start_not_before: ScheduleInstant,
    pub finish_not_after: Option<ScheduleInstant>,
    pub fixed_start: Option<ScheduleInstant>,
    pub progress_percent: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum GanttDependencyKind {
    FinishStart,
    StartStart,
    FinishFinish,
    StartFinish,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct GanttDependency {
    pub predecessor: GanttTaskId,
    pub successor: GanttTaskId,
    pub kind: GanttDependencyKind,
    /// Signed elapsed UTC seconds after the predecessor boundary.
    pub lag_seconds: i64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GanttPlacement {
    pub start: ScheduleInstant,
    pub end: ScheduleInstant,
    pub latest_start: ScheduleInstant,
    pub slack_work_seconds: i64,
    pub critical: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GanttPlan {
    revision: u64,
    calendars: BTreeMap<GanttCalendarId, GanttCalendar>,
    tasks: BTreeMap<GanttTaskId, GanttTask>,
    dependencies: Vec<GanttDependency>,
    placements: BTreeMap<GanttTaskId, GanttPlacement>,
    tree_order: Vec<(GanttTaskId, usize)>,
}
impl GanttPlan {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn task(&self, id: GanttTaskId) -> Option<&GanttTask> {
        self.tasks.get(&id)
    }
    pub fn tasks(&self) -> impl Iterator<Item = &GanttTask> {
        self.tasks.values()
    }
    pub fn calendars(&self) -> impl Iterator<Item = &GanttCalendar> {
        self.calendars.values()
    }
    pub fn dependencies(&self) -> &[GanttDependency] {
        &self.dependencies
    }
    pub fn placement(&self, id: GanttTaskId) -> Option<GanttPlacement> {
        self.placements.get(&id).copied()
    }
    pub fn tree_order(&self) -> &[(GanttTaskId, usize)] {
        &self.tree_order
    }
    /// Validate hierarchy, dependency DAG, calendars and constraints before one publication.
    pub fn replace(
        &mut self,
        expected_revision: u64,
        calendars: Vec<GanttCalendar>,
        tasks: Vec<GanttTask>,
        dependencies: Vec<GanttDependency>,
    ) -> Result<u64, GanttError> {
        if expected_revision != self.revision {
            return Err(invalid("stale plan revision"));
        }
        if tasks.len() > MAX_GANTT_TASKS
            || calendars.len() > 64
            || dependencies.len() > MAX_GANTT_DEPENDENCIES
        {
            return Err(invalid("plan capacity exceeded"));
        }
        let mut staged = Self::new();
        let mut total_intervals = 0usize;
        for calendar in calendars {
            calendar.validate()?;
            total_intervals += calendar.working.len();
            if total_intervals > 65536 || staged.calendars.insert(calendar.id, calendar).is_some() {
                return Err(invalid("duplicate calendar or interval capacity exceeded"));
            }
        }
        for task in tasks {
            if !label(&task.title)
                || task.work_seconds <= 0
                || task.progress_percent > 100
                || !staged.calendars.contains_key(&task.calendar)
            {
                return Err(invalid(
                    "task has invalid label, duration, progress or calendar",
                ));
            }
            if staged.tasks.insert(task.id, task).is_some() {
                return Err(invalid("duplicate task identity"));
            }
        }
        if dependencies.iter().copied().collect::<BTreeSet<_>>().len() != dependencies.len() {
            return Err(invalid("duplicate dependency"));
        }
        for dep in &dependencies {
            if dep.predecessor == dep.successor
                || !staged.tasks.contains_key(&dep.predecessor)
                || !staged.tasks.contains_key(&dep.successor)
                || dep.lag_seconds == i64::MIN
            {
                return Err(invalid(
                    "dependency references an unknown/self task or invalid lag",
                ));
            }
        }
        staged.dependencies = dependencies;
        staged.validate_hierarchy()?;
        staged.schedule()?;
        if self.calendars == staged.calendars
            && self.tasks == staged.tasks
            && self.dependencies == staged.dependencies
        {
            return Ok(self.revision);
        }
        staged.revision = self
            .revision
            .checked_add(1)
            .ok_or_else(|| invalid("plan revision exhausted"))?;
        *self = staged;
        Ok(self.revision)
    }
    pub fn put_task(&mut self, expected_revision: u64, task: GanttTask) -> Result<u64, GanttError> {
        let mut tasks = self.tasks.clone();
        tasks.insert(task.id, task);
        self.replace(
            expected_revision,
            self.calendars.values().cloned().collect(),
            tasks.into_values().collect(),
            self.dependencies.clone(),
        )
    }
    fn validate_hierarchy(&mut self) -> Result<(), GanttError> {
        for task in self.tasks.values() {
            let mut seen = BTreeSet::new();
            let mut current = Some(task.id);
            while let Some(id) = current {
                if !seen.insert(id) || seen.len() > 64 {
                    return Err(invalid("task hierarchy cycle or depth above 64"));
                }
                current = self
                    .tasks
                    .get(&id)
                    .ok_or_else(|| invalid("unknown task parent"))?
                    .parent;
            }
        }
        let mut children = BTreeMap::<Option<GanttTaskId>, Vec<GanttTaskId>>::new();
        for task in self.tasks.values() {
            children.entry(task.parent).or_default().push(task.id);
        }
        for rows in children.values_mut() {
            rows.sort_by_key(|id| (self.tasks[id].order, *id));
        }
        let mut stack = children
            .get(&None)
            .into_iter()
            .flatten()
            .rev()
            .map(|id| (*id, 0))
            .collect::<Vec<_>>();
        while let Some((id, depth)) = stack.pop() {
            self.tree_order.push((id, depth));
            if let Some(rows) = children.get(&Some(id)) {
                stack.extend(rows.iter().rev().map(|id| (*id, depth + 1)));
            }
        }
        Ok(())
    }
    fn schedule(&mut self) -> Result<(), GanttError> {
        let mut incoming = BTreeMap::<GanttTaskId, Vec<GanttDependency>>::new();
        let mut outgoing = BTreeMap::<GanttTaskId, Vec<GanttDependency>>::new();
        let mut degrees = self
            .tasks
            .keys()
            .map(|id| (*id, 0usize))
            .collect::<BTreeMap<_, _>>();
        for dep in &self.dependencies {
            incoming.entry(dep.successor).or_default().push(*dep);
            outgoing.entry(dep.predecessor).or_default().push(*dep);
            *degrees.get_mut(&dep.successor).unwrap() += 1;
        }
        let mut ready = degrees
            .iter()
            .filter_map(|(id, d)| (*d == 0).then_some(*id))
            .collect::<BTreeSet<_>>();
        let mut order = Vec::new();
        while let Some(id) = ready.pop_first() {
            order.push(id);
            let task = &self.tasks[&id];
            let calendar = &self.calendars[&task.calendar];
            let mut start_bound = task.start_not_before;
            let mut end_bound = None::<ScheduleInstant>;
            for dep in incoming.get(&id).into_iter().flatten() {
                let previous = self.placements[&dep.predecessor];
                let boundary = match dep.kind {
                    GanttDependencyKind::FinishStart | GanttDependencyKind::FinishFinish => {
                        previous.end
                    }
                    _ => previous.start,
                };
                let bound = add(boundary, dep.lag_seconds)?;
                if matches!(
                    dep.kind,
                    GanttDependencyKind::FinishStart | GanttDependencyKind::StartStart
                ) {
                    start_bound = start_bound.max(bound);
                } else {
                    end_bound = Some(end_bound.map_or(bound, |old| old.max(bound)));
                }
            }
            let mut start = calendar.next(start_bound)?;
            if let Some(bound) = end_bound
                && calendar.finish(start, task.work_seconds)? < bound
            {
                // Earliest working start whose completion is no earlier than bound.
                // A nonworking boundary maps to the next working instant.
                start = start.max(calendar.start_before(bound, task.work_seconds)?);
                if calendar.finish(start, task.work_seconds)? < bound {
                    start = calendar.next(add(start, 1)?)?;
                }
            }
            if let Some(fixed) = task.fixed_start {
                if fixed < start || calendar.next(fixed)? != fixed {
                    return Err(invalid(
                        "fixed task start conflicts with calendar/dependencies",
                    ));
                }
                start = fixed;
            }
            let end = calendar.finish(start, task.work_seconds)?;
            if task.finish_not_after.is_some_and(|deadline| end > deadline) {
                return Err(invalid("task cannot satisfy its finish deadline"));
            }
            self.placements.insert(
                id,
                GanttPlacement {
                    start,
                    end,
                    latest_start: start,
                    slack_work_seconds: 0,
                    critical: true,
                },
            );
            for dep in outgoing.get(&id).into_iter().flatten() {
                let d = degrees.get_mut(&dep.successor).unwrap();
                *d -= 1;
                if *d == 0 {
                    ready.insert(dep.successor);
                }
            }
        }
        if order.len() != self.tasks.len() {
            return Err(invalid("task dependency cycle"));
        }
        let Some(project_end) = self.placements.values().map(|p| p.end).max() else {
            return Ok(());
        };
        for id in order.into_iter().rev() {
            let task = &self.tasks[&id];
            let calendar = &self.calendars[&task.calendar];
            let mut finish_bound = task
                .finish_not_after
                .map_or(project_end, |d| d.min(project_end));
            let mut start_bound = None::<ScheduleInstant>;
            for dep in outgoing.get(&id).into_iter().flatten() {
                let successor = self.placements[&dep.successor];
                let successor_task = &self.tasks[&dep.successor];
                let successor_end = self.calendars[&successor_task.calendar]
                    .finish(successor.latest_start, successor_task.work_seconds)?;
                let boundary = match dep.kind {
                    GanttDependencyKind::FinishStart | GanttDependencyKind::StartStart => {
                        successor.latest_start
                    }
                    _ => successor_end,
                };
                let bound = add(boundary, -dep.lag_seconds)?;
                if matches!(
                    dep.kind,
                    GanttDependencyKind::FinishStart | GanttDependencyKind::FinishFinish
                ) {
                    finish_bound = finish_bound.min(bound);
                } else {
                    start_bound = Some(start_bound.map_or(bound, |old| old.min(bound)));
                }
            }
            let mut latest = calendar.start_before(finish_bound, task.work_seconds)?;
            if let Some(bound) = start_bound {
                latest = latest.min(calendar.on_or_before(bound)?);
            }
            if let Some(fixed) = task.fixed_start {
                latest = latest.min(fixed);
            }
            let placement = self.placements.get_mut(&id).unwrap();
            if latest < placement.start {
                return Err(invalid("inconsistent latest-start constraints"));
            }
            placement.latest_start = latest;
            placement.slack_work_seconds = calendar.working_between(placement.start, latest)?;
            placement.critical = placement.slack_work_seconds == 0;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
