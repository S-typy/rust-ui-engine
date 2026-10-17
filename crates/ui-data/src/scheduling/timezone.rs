use super::*;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ScheduleGapPolicy {
    #[default]
    Reject,
    Skip,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ScheduleFoldPolicy {
    #[default]
    Reject,
    Earlier,
    Later,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ScheduleResolvePolicy {
    pub gap: ScheduleGapPolicy,
    pub fold: ScheduleFoldPolicy,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScheduleTransition {
    pub at: ScheduleInstant,
    pub offset_after: i32,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Period {
    start: i64,
    end: i64,
    offset: i32,
    local_start: i64,
    local_end: i64,
}
/// Application-supplied offset history with explicit finite UTC coverage.
/// This type does not consult an operating system or timezone database.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScheduleTimeZone {
    id: String,
    coverage: ScheduleUtcInterval,
    initial_offset: i32,
    transitions: Vec<ScheduleTransition>,
    periods: Vec<Period>,
    local_min: i64,
    local_max: i64,
    min_offset: i32,
    max_offset: i32,
}
impl ScheduleTimeZone {
    pub fn new(
        id: impl Into<String>,
        coverage: ScheduleUtcInterval,
        initial_offset: i32,
        transitions: Vec<ScheduleTransition>,
    ) -> Result<Self, ScheduleError> {
        let id = id.into();
        ScheduleUtcInterval::new(coverage.start, coverage.end)?;
        if !identifier(&id) || initial_offset.unsigned_abs() > 86399 || transitions.len() > 4096 {
            return Err(invalid(
                "invalid timezone identifier, offset or transition budget",
            ));
        }
        let mut periods = Vec::with_capacity(transitions.len() + 1);
        let (mut start, mut offset) = (coverage.start.0, initial_offset);
        for transition in &transitions {
            if transition.at.0 <= start
                || transition.at >= coverage.end
                || transition.offset_after.unsigned_abs() > 86399
                || transition.offset_after == offset
            {
                return Err(invalid(
                    "timezone transitions must be ordered, inside coverage and change the bounded offset",
                ));
            }
            periods.push(period(start, transition.at.0, offset)?);
            start = transition.at.0;
            offset = transition.offset_after;
        }
        periods.push(period(start, coverage.end.0, offset)?);
        let mut edges = BTreeMap::<i64, i32>::new();
        for p in &periods {
            *edges.entry(p.local_start).or_default() += 1;
            *edges.entry(p.local_end).or_default() -= 1;
        }
        let mut overlap = 0;
        for delta in edges.values() {
            overlap += delta;
            if overlap > 2 {
                return Err(invalid(
                    "timezone has more than two mappings for a local time",
                ));
            }
        }
        let local_min = periods.iter().map(|p| p.local_start).min().unwrap();
        let local_max = periods.iter().map(|p| p.local_end).max().unwrap();
        let min_offset = periods.iter().map(|p| p.offset).min().unwrap();
        let max_offset = periods.iter().map(|p| p.offset).max().unwrap();
        Ok(Self {
            id,
            coverage,
            initial_offset,
            transitions,
            periods,
            local_min,
            local_max,
            min_offset,
            max_offset,
        })
    }
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn coverage(&self) -> ScheduleUtcInterval {
        self.coverage
    }
    pub fn initial_offset(&self) -> i32 {
        self.initial_offset
    }
    pub fn transitions(&self) -> &[ScheduleTransition] {
        &self.transitions
    }
    pub fn to_local(&self, utc: ScheduleInstant) -> Result<ScheduleLocal, ScheduleError> {
        if utc < self.coverage.start || utc >= self.coverage.end {
            return Err(ScheduleError::Coverage(self.id.clone()));
        }
        let index = self.periods.partition_point(|p| p.start <= utc.0) - 1;
        ScheduleLocal::from_seconds(
            utc.0
                .checked_add(i64::from(self.periods[index].offset))
                .ok_or_else(|| invalid("timezone arithmetic overflow"))?,
        )
    }
    pub fn resolve(
        &self,
        local: ScheduleLocal,
        policy: ScheduleResolvePolicy,
    ) -> Result<Option<ScheduleInstant>, ScheduleError> {
        self.resolve_endpoint(local, policy, false)
    }
    fn resolve_endpoint(
        &self,
        local: ScheduleLocal,
        policy: ScheduleResolvePolicy,
        endpoint: bool,
    ) -> Result<Option<ScheduleInstant>, ScheduleError> {
        let value = local.seconds();
        let mut candidates = Vec::with_capacity(2);
        for p in &self.periods {
            if value >= p.local_start && value < p.local_end {
                candidates.push(ScheduleInstant(value - i64::from(p.offset)));
            }
        }
        if endpoint
            && let Some(last) = self.periods.last()
            && value == last.local_end
        {
            candidates.push(self.coverage.end);
        }
        candidates.sort_unstable();
        candidates.dedup();
        match candidates.as_slice() {
            [one] => Ok(Some(*one)),
            [] if value < self.local_min || value >= self.local_max => {
                Err(ScheduleError::Coverage(self.id.clone()))
            }
            [] => match policy.gap {
                ScheduleGapPolicy::Reject => Err(ScheduleError::Gap(local)),
                ScheduleGapPolicy::Skip => Ok(None),
            },
            values => match policy.fold {
                ScheduleFoldPolicy::Reject => Err(ScheduleError::Fold(local)),
                ScheduleFoldPolicy::Earlier => Ok(values.first().copied()),
                ScheduleFoldPolicy::Later => Ok(values.last().copied()),
            },
        }
    }
    pub fn resolve_interval(
        &self,
        local: ScheduleLocalInterval,
        policy: ScheduleResolvePolicy,
    ) -> Result<Option<ScheduleUtcInterval>, ScheduleError> {
        ScheduleLocalInterval::new(local.start, local.end)?;
        // Resolve both endpoints even when the first is skipped: a coverage
        // failure must not masquerade as an ordinary DST skip.
        let start = self.resolve_endpoint(local.start, policy, false)?;
        let end = self.resolve_endpoint(local.end, policy, true)?;
        match (start, end) {
            (Some(start), Some(end)) => Ok(Some(ScheduleUtcInterval::new(start, end)?)),
            _ => Ok(None),
        }
    }
    pub(super) fn checks(&self) -> usize {
        self.periods.len() * 4
    }
    pub(super) fn local_bounds(&self) -> (i64, i64) {
        (self.local_min, self.local_max)
    }
    pub(super) fn offset_bounds(&self) -> (i32, i32) {
        (self.min_offset, self.max_offset)
    }
    pub(super) fn validate_query_range(
        &self,
        range: ScheduleUtcInterval,
    ) -> Result<(), ScheduleError> {
        if range.start < self.coverage.start || range.end > self.coverage.end {
            return Err(ScheduleError::Coverage(self.id.clone()));
        }
        for p in &self.periods {
            let start = p.start.max(range.start.0);
            let end = p.end.min(range.end.0);
            if start < end {
                // The query is half-open: the final included second must be
                // representable, while its exclusive endpoint may be year 10000.
                ScheduleLocal::from_seconds(start + i64::from(p.offset))?;
                ScheduleLocal::from_seconds(end - 1 + i64::from(p.offset))?;
            }
        }
        Ok(())
    }
    pub(super) fn may_overlap(
        &self,
        local: ScheduleLocalInterval,
        range: ScheduleUtcInterval,
    ) -> bool {
        let (a, b) = (local.start.seconds(), local.end.seconds());
        for p in &self.periods {
            let start = p.start.max(range.start.0);
            let end = p.end.min(range.end.0);
            if start < end && a < end + i64::from(p.offset) && start + i64::from(p.offset) < b {
                return true;
            }
        }
        // A wholly nonexistent interval belongs to the instant of the forward
        // transition for query rejection/skipping, never to the preceding day.
        self.periods.windows(2).any(|pair| {
            let at = pair[1].start;
            range.start.0 <= at
                && at < range.end.0
                && pair[0].local_end < pair[1].local_start
                && a < pair[1].local_start
                && pair[0].local_end < b
        })
    }
}
fn period(start: i64, end: i64, offset: i32) -> Result<Period, ScheduleError> {
    Ok(Period {
        start,
        end,
        offset,
        local_start: start
            .checked_add(i64::from(offset))
            .ok_or_else(|| invalid("timezone arithmetic overflow"))?,
        local_end: end
            .checked_add(i64::from(offset))
            .ok_or_else(|| invalid("timezone arithmetic overflow"))?,
    })
}
