use super::*;
use recurrence::QueryWork;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ScheduleSeriesId(pub u64);
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ScheduleResourceId(pub u64);
/// Identity uses the original nominal local start, not an exception's new time.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ScheduleOccurrenceId {
    pub series: ScheduleSeriesId,
    pub original: ScheduleLocal,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScheduleResource {
    pub id: ScheduleResourceId,
    pub label: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScheduleSeries {
    pub id: ScheduleSeriesId,
    pub title: String,
    pub zone: String,
    pub interval: ScheduleLocalInterval,
    pub resources: Vec<ScheduleResourceId>,
    pub recurrence: ScheduleRecurrence,
    pub policy: ScheduleResolvePolicy,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScheduleException {
    Cancel,
    Replace {
        interval: ScheduleLocalInterval,
        title: Option<String>,
        resources: Option<Vec<ScheduleResourceId>>,
    },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScheduleOccurrence {
    pub id: ScheduleOccurrenceId,
    pub title: String,
    pub resources: Vec<ScheduleResourceId>,
    pub local: ScheduleLocalInterval,
    pub utc: ScheduleUtcInterval,
    pub is_exception: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScheduleChange {
    PutZone(ScheduleTimeZone),
    RemoveZone(String),
    PutResource(ScheduleResource),
    RemoveResource(ScheduleResourceId),
    PutSeries(ScheduleSeries),
    RemoveSeries(ScheduleSeriesId),
    PutException {
        id: ScheduleOccurrenceId,
        exception: ScheduleException,
    },
    RemoveException(ScheduleOccurrenceId),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScheduleChangeBatch {
    pub revision: u64,
    pub zones: Vec<String>,
    pub resources: Vec<ScheduleResourceId>,
    pub series: Vec<ScheduleSeriesId>,
    pub occurrences: Vec<ScheduleOccurrenceId>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScheduleChanges {
    pub revision: u64,
    pub reset_required: bool,
    pub batches: Vec<ScheduleChangeBatch>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct State {
    zones: BTreeMap<String, ScheduleTimeZone>,
    resources: BTreeMap<ScheduleResourceId, ScheduleResource>,
    series: BTreeMap<ScheduleSeriesId, ScheduleSeries>,
    exceptions: BTreeMap<ScheduleOccurrenceId, ScheduleException>,
}
/// Owned immutable descriptors, optimistic atomic writes, and bounded queries.
/// No callbacks, interior shared state, timezone service, transport or UI are used.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SchedulingModel {
    state: State,
    revision: u64,
    journal: VecDeque<ScheduleChangeBatch>,
}
impl SchedulingModel {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn zone(&self, id: &str) -> Option<&ScheduleTimeZone> {
        self.state.zones.get(id)
    }
    pub fn resource(&self, id: ScheduleResourceId) -> Option<&ScheduleResource> {
        self.state.resources.get(&id)
    }
    pub fn series(&self, id: ScheduleSeriesId) -> Option<&ScheduleSeries> {
        self.state.series.get(&id)
    }
    pub fn exception(&self, id: ScheduleOccurrenceId) -> Option<&ScheduleException> {
        self.state.exceptions.get(&id)
    }
    pub fn resources(&self) -> impl Iterator<Item = &ScheduleResource> {
        self.state.resources.values()
    }
    pub fn series_iter(&self) -> impl Iterator<Item = &ScheduleSeries> {
        self.state.series.values()
    }
    pub fn changes_since(&self, cursor: u64) -> ScheduleChanges {
        let reset_required = cursor > self.revision
            || self
                .journal
                .front()
                .is_some_and(|batch| cursor < batch.revision.saturating_sub(1));
        ScheduleChanges {
            revision: self.revision,
            reset_required,
            batches: if reset_required {
                Vec::new()
            } else {
                self.journal
                    .iter()
                    .filter(|b| b.revision > cursor)
                    .cloned()
                    .collect()
            },
        }
    }
    /// Every submitted descriptor is copied into a staged candidate. Referenced
    /// removals require dependent edits in this same batch; order is immaterial.
    pub fn apply(
        &mut self,
        expected_revision: u64,
        changes: Vec<ScheduleChange>,
    ) -> Result<u64, ScheduleError> {
        if expected_revision != self.revision {
            return Err(ScheduleError::Conflict {
                expected: expected_revision,
                actual: self.revision,
            });
        }
        if changes.is_empty() {
            return Ok(self.revision);
        }
        if changes.len() > 256 {
            return Err(ScheduleError::Capacity("change batch exceeds 256 entries"));
        }
        let revision = self
            .revision
            .checked_add(1)
            .ok_or(ScheduleError::RevisionOverflow)?;
        let mut staged = self.state.clone();
        let (mut zones, mut resources, mut series, mut occurrences) = (
            BTreeSet::new(),
            BTreeSet::new(),
            BTreeSet::new(),
            BTreeSet::new(),
        );
        for change in changes {
            match change {
                ScheduleChange::PutZone(zone) => {
                    if !zones.insert(zone.id().to_owned()) {
                        return Err(invalid("duplicate zone change"));
                    }
                    staged.zones.insert(zone.id().to_owned(), zone);
                }
                ScheduleChange::RemoveZone(id) => {
                    if !zones.insert(id.clone()) || staged.zones.remove(&id).is_none() {
                        return Err(invalid("duplicate or unknown zone removal"));
                    }
                }
                ScheduleChange::PutResource(resource) => {
                    if !resources.insert(resource.id) {
                        return Err(invalid("duplicate resource change"));
                    }
                    staged.resources.insert(resource.id, resource);
                }
                ScheduleChange::RemoveResource(id) => {
                    if !resources.insert(id) || staged.resources.remove(&id).is_none() {
                        return Err(invalid("duplicate or unknown resource removal"));
                    }
                }
                ScheduleChange::PutSeries(value) => {
                    if !series.insert(value.id) {
                        return Err(invalid("duplicate series change"));
                    }
                    staged.series.insert(value.id, value);
                }
                ScheduleChange::RemoveSeries(id) => {
                    if !series.insert(id) || staged.series.remove(&id).is_none() {
                        return Err(invalid("duplicate or unknown series removal"));
                    }
                }
                ScheduleChange::PutException { id, exception } => {
                    if !occurrences.insert(id) {
                        return Err(invalid("duplicate occurrence change"));
                    }
                    staged.exceptions.insert(id, exception);
                }
                ScheduleChange::RemoveException(id) => {
                    if !occurrences.insert(id) || staged.exceptions.remove(&id).is_none() {
                        return Err(invalid("duplicate or unknown exception removal"));
                    }
                }
            }
        }
        validate_state(&staged)?;
        if staged == self.state {
            return Ok(self.revision);
        }
        let batch = ScheduleChangeBatch {
            revision,
            zones: zones.into_iter().collect(),
            resources: resources.into_iter().collect(),
            series: series.into_iter().collect(),
            occurrences: occurrences.into_iter().collect(),
        };
        self.state = staged;
        self.revision = revision;
        if self.journal.len() == 64 {
            self.journal.pop_front();
        }
        self.journal.push_back(batch);
        Ok(revision)
    }
    /// Returns all overlapping occurrences, sorted by UTC start/end and identity.
    /// Resource filtering is any-of; an empty filter includes unassigned events.
    /// Capacity, coverage and DST rejection return Err with no partial result.
    pub fn query(
        &self,
        range: ScheduleUtcInterval,
        resources: &[ScheduleResourceId],
        limits: ScheduleQueryLimits,
    ) -> Result<Vec<ScheduleOccurrence>, ScheduleError> {
        ScheduleUtcInterval::new(range.start, range.end)?;
        limits.validate()?;
        if resources.len() > 32
            || resources.iter().collect::<BTreeSet<_>>().len() != resources.len()
            || resources
                .iter()
                .any(|id| !self.state.resources.contains_key(id))
        {
            return Err(invalid(
                "resource filter requires at most 32 distinct known identities",
            ));
        }
        let mut result = Vec::new();
        let mut work = QueryWork::new(limits)?;
        let mut zone_work = QueryWork::new(limits)?;
        let mut validated_zones = BTreeSet::new();
        for series in self.state.series.values() {
            let zone = &self.state.zones[&series.zone];
            let exceptions = self
                .state
                .exceptions
                .range(
                    ScheduleOccurrenceId {
                        series: series.id,
                        original: first_local(),
                    }..=ScheduleOccurrenceId {
                        series: series.id,
                        original: last_local(),
                    },
                )
                .collect::<Vec<_>>();
            let base_matches = matches_resources(&series.resources, resources);
            let moved_matches=exceptions.iter().any(|(_,e)|matches!(e,ScheduleException::Replace{resources:Some(ids),..} if matches_resources(ids,resources)));
            if !base_matches && !moved_matches {
                continue;
            }
            if validated_zones.insert(zone.id()) {
                zone_work.zone(zone.checks())?;
                zone.validate_query_range(range)?;
            }
            // Actual table offsets bound the nominal starts which can overlap;
            // moved exceptions are evaluated separately from their original date.
            let through = local_ceiling(
                range
                    .end
                    .0
                    .saturating_add(i64::from(zone.offset_bounds().1))
                    .min(zone.local_bounds().1.saturating_sub(1)),
            );
            if base_matches {
                series
                    .recurrence
                    .visit(series.interval.start, through, &mut work, |original| {
                        let id = ScheduleOccurrenceId {
                            series: series.id,
                            original,
                        };
                        if self.state.exceptions.contains_key(&id)
                            || original.seconds() - i64::from(zone.offset_bounds().1) >= range.end.0
                        {
                            return Ok(true);
                        }
                        let local = ScheduleLocalInterval::new(
                            original,
                            original.checked_add(series.interval.wall_seconds())?,
                        )?;
                        if local.end.seconds() <= zone.local_bounds().0
                            || local.start.seconds() - i64::from(zone.offset_bounds().1)
                                >= range.end.0
                            || local.end.seconds() - i64::from(zone.offset_bounds().0)
                                <= range.start.0
                        {
                            return Ok(true);
                        }
                        zone_work.zone(zone.checks())?;
                        if !zone.may_overlap(local, range) {
                            return Ok(true);
                        }
                        work_zone_occurrence(
                            zone,
                            series,
                            id,
                            local,
                            None,
                            None,
                            false,
                            range,
                            resources,
                            &mut result,
                            limits,
                        )?;
                        Ok(true)
                    })?;
            }
            for (id, exception) in exceptions {
                if let ScheduleException::Replace {
                    interval,
                    title,
                    resources: replacement_resources,
                } = exception
                {
                    zone_work.zone(zone.checks())?;
                    work_zone_occurrence(
                        zone,
                        series,
                        *id,
                        *interval,
                        title.as_deref(),
                        replacement_resources.as_deref(),
                        true,
                        range,
                        resources,
                        &mut result,
                        limits,
                    )?;
                }
            }
        }
        result.sort_by_key(|o| (o.utc.start, o.utc.end, o.id));
        Ok(result)
    }
}

fn matches_resources(assigned: &[ScheduleResourceId], requested: &[ScheduleResourceId]) -> bool {
    requested.is_empty() || requested.iter().any(|r| assigned.contains(r))
}
#[allow(clippy::too_many_arguments)]
fn work_zone_occurrence(
    zone: &ScheduleTimeZone,
    series: &ScheduleSeries,
    id: ScheduleOccurrenceId,
    local: ScheduleLocalInterval,
    title: Option<&str>,
    replacement_resources: Option<&[ScheduleResourceId]>,
    is_exception: bool,
    range: ScheduleUtcInterval,
    requested: &[ScheduleResourceId],
    result: &mut Vec<ScheduleOccurrence>,
    limits: ScheduleQueryLimits,
) -> Result<(), ScheduleError> {
    let resources = replacement_resources.unwrap_or(&series.resources);
    if !matches_resources(resources, requested) {
        return Ok(());
    }
    let Some(utc) = zone.resolve_interval(local, series.policy)? else {
        return Ok(());
    };
    if !utc.overlaps(range) {
        return Ok(());
    }
    if result.len() == limits.max_occurrences {
        return Err(ScheduleError::Capacity("occurrence output"));
    }
    result.push(ScheduleOccurrence {
        id,
        title: title.unwrap_or(&series.title).into(),
        resources: resources.to_vec(),
        local,
        utc,
        is_exception,
    });
    Ok(())
}
fn validate_resources(ids: &[ScheduleResourceId], state: &State) -> Result<(), ScheduleError> {
    if ids.len() > 32
        || ids.iter().collect::<BTreeSet<_>>().len() != ids.len()
        || ids.iter().any(|id| !state.resources.contains_key(id))
    {
        return Err(invalid(
            "appointment resources require at most 32 distinct existing identities",
        ));
    }
    Ok(())
}
fn validate_state(state: &State) -> Result<(), ScheduleError> {
    if state.zones.len() > 64
        || state.resources.len() > 4096
        || state.series.len() > 4096
        || state.exceptions.len() > 65536
    {
        return Err(ScheduleError::Capacity("schedule model descriptor count"));
    }
    let mut bytes = state
        .zones
        .values()
        .map(|z| z.id().len() + z.transitions().len() * 80 + 80)
        .sum::<usize>();
    for resource in state.resources.values() {
        if !bounded_label(&resource.label, 1024) {
            return Err(invalid("resource requires a nonempty bounded label"));
        }
        bytes += resource.label.len() + 16;
    }
    let mut work = QueryWork::new(ScheduleQueryLimits {
        max_candidates: 1_000_000,
        max_occurrences: 100_000,
        max_zone_checks: 10_000_000,
    })?;
    for series in state.series.values() {
        if !bounded_label(&series.title, 4096) {
            return Err(invalid("series requires a nonempty bounded title"));
        }
        ScheduleLocalInterval::new(series.interval.start, series.interval.end)?;
        series.recurrence.validate(series.interval.start)?;
        validate_resources(&series.resources, state)?;
        let zone = state
            .zones
            .get(&series.zone)
            .ok_or_else(|| invalid("series references an unknown timezone"))?;
        work.zone(zone.checks())?;
        zone.resolve_interval(series.interval, series.policy)?;
        bytes += series.title.len() + series.zone.len() + series.resources.len() * 8 + 128;
    }
    for (id, exception) in &state.exceptions {
        let series = state
            .series
            .get(&id.series)
            .ok_or_else(|| invalid("exception references an unknown series"))?;
        let mut found = false;
        series
            .recurrence
            .visit(series.interval.start, id.original, &mut work, |candidate| {
                if candidate == id.original {
                    found = true;
                    return Ok(false);
                }
                Ok(true)
            })?;
        if !found {
            return Err(invalid(
                "exception identity is not a candidate of its series",
            ));
        }
        bytes += 64;
        if let ScheduleException::Replace {
            interval,
            title,
            resources,
        } = exception
        {
            if title
                .as_ref()
                .is_some_and(|text| !bounded_label(text, 4096))
            {
                return Err(invalid("replacement title is invalid"));
            }
            if let Some(ids) = resources {
                validate_resources(ids, state)?;
                bytes += ids.len() * 8;
            }
            if let Some(title) = title {
                bytes += title.len();
            }
            let zone = &state.zones[&series.zone];
            work.zone(zone.checks())?;
            if zone.resolve_interval(*interval, series.policy)?.is_none() {
                return Err(invalid(
                    "replacement interval cannot be silently skipped in a DST gap",
                ));
            }
        }
        if bytes > 16 * 1024 * 1024 {
            return Err(ScheduleError::Capacity(
                "schedule logical payload exceeds 16 MiB",
            ));
        }
    }
    if bytes > 16 * 1024 * 1024 {
        return Err(ScheduleError::Capacity(
            "schedule logical payload exceeds 16 MiB",
        ));
    }
    Ok(())
}
fn first_local() -> ScheduleLocal {
    ScheduleLocal::new(
        DateValue::new(1, 1, 1).unwrap(),
        TimeValue::new(0, 0, 0, 0).unwrap(),
    )
    .unwrap()
}
fn last_local() -> ScheduleLocal {
    ScheduleLocal::new(
        DateValue::new(9999, 12, 31).unwrap(),
        TimeValue::new(23, 59, 59, 0).unwrap(),
    )
    .unwrap()
}
fn local_ceiling(seconds: i64) -> ScheduleLocal {
    if seconds <= first_local().seconds() {
        first_local()
    } else if seconds >= last_local().seconds() {
        last_local()
    } else {
        ScheduleLocal::from_seconds(seconds).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scheduling_revision_exhaustion_precedes_any_publication() {
        let mut model = SchedulingModel {
            revision: u64::MAX,
            ..Default::default()
        };
        let before = model.clone();
        assert_eq!(
            model.apply(
                u64::MAX,
                vec![ScheduleChange::PutResource(ScheduleResource {
                    id: ScheduleResourceId(2),
                    label: "Room".into()
                })]
            ),
            Err(ScheduleError::RevisionOverflow)
        );
        assert_eq!(model, before);
    }
}
