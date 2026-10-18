use rust_desktop_ui_data::*;
type SchedulerProjection = (
    Vec<(DateValue, ScheduleUtcInterval)>,
    Vec<ScheduleOccurrence>,
);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SchedulerMode {
    Day,
    #[default]
    Week,
    Month,
    Timeline,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SchedulerSettings {
    pub mode: SchedulerMode,
    pub anchor: DateValue,
    pub display_zone: String,
    /// Empty means all model resources plus an unassigned group when needed.
    pub resources: Vec<ScheduleResourceId>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SchedulerEditScope {
    Instance,
    Series,
}
#[derive(Clone, Debug, PartialEq, Eq)]
enum DraftTarget {
    New(ScheduleSeries),
    Instance(ScheduleOccurrenceId),
    Series(ScheduleSeries),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SchedulerDraft {
    pub title: String,
    /// Uses the series timezone, explicitly independent of the display timezone.
    pub interval: ScheduleLocalInterval,
    pub resources: Vec<ScheduleResourceId>,
    target: DraftTarget,
    revision: u64,
    generation: u64,
}
impl SchedulerDraft {
    pub fn is_series(&self) -> bool {
        matches!(self.target, DraftTarget::New(_) | DraftTarget::Series(_))
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
}
#[derive(Clone, Debug)]
pub struct SchedulerState {
    model: SchedulingModel,
    settings: SchedulerSettings,
    occurrences: Vec<ScheduleOccurrence>,
    days: Vec<(DateValue, ScheduleUtcInterval)>,
    selected: Option<ScheduleOccurrenceId>,
    draft: Option<SchedulerDraft>,
    generation: u64,
}
fn invalid(text: impl Into<String>) -> ScheduleError {
    ScheduleError::Invalid(text.into())
}
pub(super) fn midnight(date: DateValue) -> Result<ScheduleLocal, ScheduleError> {
    ScheduleLocal::new(
        date,
        TimeValue::new(0, 0, 0, 0).map_err(|e| invalid(e.to_string()))?,
    )
}
pub(super) fn next_date(date: DateValue, days: i64) -> Result<DateValue, ScheduleError> {
    Ok(midnight(date)?
        .checked_add(
            days.checked_mul(86400)
                .ok_or_else(|| invalid("calendar overflow"))?,
        )?
        .date())
}
pub(super) fn next_month(date: DateValue, delta: i64) -> Result<DateValue, ScheduleError> {
    let index = i64::from(date.year() - 1) * 12 + i64::from(date.month() - 1) + delta;
    if !(0..9999 * 12).contains(&index) {
        return Err(invalid("month navigation leaves supported calendar"));
    }
    let year = (index / 12 + 1) as u16;
    let month = (index % 12 + 1) as u8;
    (1..=date.day())
        .rev()
        .find_map(|day| DateValue::new(year, month, day).ok())
        .ok_or_else(|| invalid("invalid calendar month"))
}
pub(super) fn monday_offset(date: DateValue) -> i64 {
    let y = i64::from(date.year()) - 1;
    let months = [31i64, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let leap = date.year().is_multiple_of(4)
        && (!date.year().is_multiple_of(100) || date.year().is_multiple_of(400));
    let prior = months[..usize::from(date.month() - 1)].iter().sum::<i64>()
        + i64::from(date.month() > 2 && leap);
    (y * 365 + y / 4 - y / 100 + y / 400 + prior + i64::from(date.day()) - 1).rem_euclid(7)
}
fn boundary(
    zone: &ScheduleTimeZone,
    local: ScheduleLocal,
) -> Result<ScheduleInstant, ScheduleError> {
    // A midnight fold selects its first occurrence. A midnight gap is rejected
    // explicitly instead of pretending the day had 24 hours.
    zone.resolve(
        local,
        ScheduleResolvePolicy {
            gap: ScheduleGapPolicy::Reject,
            fold: ScheduleFoldPolicy::Earlier,
        },
    )?
    .ok_or_else(|| invalid("day boundary cannot be skipped"))
}
impl SchedulerState {
    pub fn new(model: SchedulingModel, settings: SchedulerSettings) -> Result<Self, ScheduleError> {
        let (days, occurrences) = Self::project(&model, &settings)?;
        let state = Self {
            model,
            settings,
            occurrences,
            days,
            selected: None,
            draft: None,
            generation: 0,
        };
        state.geometry(160.0)?;
        Ok(state)
    }
    fn project(
        model: &SchedulingModel,
        settings: &SchedulerSettings,
    ) -> Result<SchedulerProjection, ScheduleError> {
        let zone = model
            .zone(&settings.display_zone)
            .ok_or_else(|| invalid("unknown display timezone"))?;
        if model.resources().count() > 16 {
            return Err(ScheduleError::Capacity(
                "Scheduler supports at most 16 resource groups",
            ));
        }
        let (start, count) = match settings.mode {
            SchedulerMode::Day => (settings.anchor, 1),
            SchedulerMode::Week => (
                next_date(settings.anchor, -monday_offset(settings.anchor))?,
                7,
            ),
            SchedulerMode::Timeline => (settings.anchor, 7),
            SchedulerMode::Month => {
                let first = DateValue::new(settings.anchor.year(), settings.anchor.month(), 1)
                    .map_err(|e| invalid(e.to_string()))?;
                (next_date(first, -monday_offset(first))?, 42)
            }
        };
        let mut days = Vec::new();
        for i in 0..count {
            let date = next_date(start, i)?;
            let from = boundary(zone, midnight(date)?)?;
            let until = boundary(zone, midnight(next_date(date, 1)?)?)?;
            days.push((date, ScheduleUtcInterval::new(from, until)?));
        }
        let range = ScheduleUtcInterval::new(days[0].1.start, days.last().unwrap().1.end)?;
        let limits = ScheduleQueryLimits {
            max_occurrences: 1024,
            ..Default::default()
        };
        let occurrences = model.query(range, &settings.resources, limits)?;
        Ok((days, occurrences))
    }
    pub fn model(&self) -> &SchedulingModel {
        &self.model
    }
    pub fn settings(&self) -> &SchedulerSettings {
        &self.settings
    }
    pub fn days(&self) -> &[(DateValue, ScheduleUtcInterval)] {
        &self.days
    }
    pub fn occurrences(&self) -> &[ScheduleOccurrence] {
        &self.occurrences
    }
    pub fn selected(&self) -> Option<ScheduleOccurrenceId> {
        self.selected
    }
    pub fn draft(&self) -> Option<&SchedulerDraft> {
        self.draft.as_ref()
    }
    pub fn draft_mut(&mut self) -> Option<&mut SchedulerDraft> {
        self.draft.as_mut()
    }
    pub fn select(&mut self, id: Option<ScheduleOccurrenceId>) -> Result<(), ScheduleError> {
        if id.is_some_and(|id| !self.occurrences.iter().any(|o| o.id == id)) {
            return Err(invalid("selection is outside current Scheduler projection"));
        }
        self.selected = id;
        Ok(())
    }
    pub fn set_settings(&mut self, settings: SchedulerSettings) -> Result<(), ScheduleError> {
        let (days, occurrences) = Self::project(&self.model, &settings)?;
        let mut staged = self.clone();
        staged.settings = settings;
        staged.days = days;
        staged.occurrences = occurrences;
        staged.geometry(160.0)?;
        *self = staged;
        Ok(())
    }
    pub fn replace_model(&mut self, model: SchedulingModel) -> Result<(), ScheduleError> {
        let (days, occurrences) = Self::project(&model, &self.settings)?;
        let mut staged = self.clone();
        if staged.model != model {
            staged.generation = staged
                .generation
                .checked_add(1)
                .ok_or(ScheduleError::RevisionOverflow)?;
        }
        staged.model = model;
        staged.days = days;
        staged.occurrences = occurrences;
        if staged
            .selected
            .is_some_and(|id| staged.model.series(id.series).is_none())
        {
            staged.selected = None;
        }
        staged.geometry(160.0)?;
        *self = staged;
        Ok(())
    }
    pub fn begin_create(&mut self, series: ScheduleSeries) -> Result<(), ScheduleError> {
        if self.model.series(series.id).is_some() {
            return Err(invalid("new series identity already exists"));
        }
        let mut candidate = self.model.clone();
        candidate.apply(
            candidate.revision(),
            vec![ScheduleChange::PutSeries(series.clone())],
        )?;
        self.draft = Some(SchedulerDraft {
            title: series.title.clone(),
            interval: series.interval,
            resources: series.resources.clone(),
            target: DraftTarget::New(series),
            revision: self.model.revision(),
            generation: self.generation,
        });
        Ok(())
    }
    pub fn begin_edit(&mut self, scope: SchedulerEditScope) -> Result<(), ScheduleError> {
        let id = self
            .selected
            .ok_or_else(|| invalid("select an appointment first"))?;
        let occurrence = self
            .occurrences
            .iter()
            .find(|o| o.id == id)
            .ok_or_else(|| invalid("selected appointment is outside this view"))?;
        let series = self
            .model
            .series(id.series)
            .ok_or_else(|| invalid("selected series disappeared"))?
            .clone();
        self.draft = Some(match scope {
            SchedulerEditScope::Instance => SchedulerDraft {
                title: occurrence.title.clone(),
                interval: occurrence.local,
                resources: occurrence.resources.clone(),
                target: DraftTarget::Instance(id),
                revision: self.model.revision(),
                generation: self.generation,
            },
            SchedulerEditScope::Series => SchedulerDraft {
                title: series.title.clone(),
                interval: series.interval,
                resources: series.resources.clone(),
                target: DraftTarget::Series(series),
                revision: self.model.revision(),
                generation: self.generation,
            },
        });
        Ok(())
    }
    pub fn cancel(&mut self) {
        self.draft = None;
    }
    /// Reject invalid drafts, stale revisions and unrenderable projections without
    /// changing accepted model, selection or the editable draft.
    pub fn commit(&mut self) -> Result<u64, ScheduleError> {
        let draft = self
            .draft
            .as_ref()
            .ok_or_else(|| invalid("no appointment draft"))?;
        if draft.generation != self.generation {
            return Err(invalid("appointment source was replaced during editing"));
        }
        let change = match &draft.target {
            DraftTarget::New(series) | DraftTarget::Series(series) => {
                let mut value = series.clone();
                value.title = draft.title.clone();
                value.interval = draft.interval;
                value.resources = draft.resources.clone();
                ScheduleChange::PutSeries(value)
            }
            DraftTarget::Instance(id) => ScheduleChange::PutException {
                id: *id,
                exception: ScheduleException::Replace {
                    interval: draft.interval,
                    title: Some(draft.title.clone()),
                    resources: Some(draft.resources.clone()),
                },
            },
        };
        let mut candidate = self.model.clone();
        let revision = candidate.apply(draft.revision, vec![change])?;
        self.replace_model(candidate)?;
        self.draft = None;
        Ok(revision)
    }
    pub fn delete_selected(&mut self, scope: SchedulerEditScope) -> Result<u64, ScheduleError> {
        let id = self
            .selected
            .ok_or_else(|| invalid("select an appointment first"))?;
        let mut candidate = self.model.clone();
        let changes = match scope {
            SchedulerEditScope::Instance => vec![ScheduleChange::PutException {
                id,
                exception: ScheduleException::Cancel,
            }],
            SchedulerEditScope::Series => {
                let mut changes = self
                    .model
                    .exceptions()
                    .filter(|(key, _)| key.series == id.series)
                    .map(|(key, _)| ScheduleChange::RemoveException(key))
                    .collect::<Vec<_>>();
                changes.push(ScheduleChange::RemoveSeries(id.series));
                changes
            }
        };
        let revision = candidate.apply(candidate.revision(), changes)?;
        self.replace_model(candidate)?;
        self.selected = None;
        self.draft = None;
        Ok(revision)
    }
    pub fn shift_draft(&mut self, seconds: i64, resize_end: bool) -> Result<(), ScheduleError> {
        let draft = self
            .draft
            .as_mut()
            .ok_or_else(|| invalid("no appointment draft"))?;
        let start = if resize_end {
            draft.interval.start
        } else {
            draft.interval.start.checked_add(seconds)?
        };
        let end = draft.interval.end.checked_add(seconds)?;
        let interval = ScheduleLocalInterval::new(start, end)?;
        draft.interval = interval;
        Ok(())
    }
}
