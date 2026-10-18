use super::common::{PlannerCaption, PlannerRule};
use super::{SchedulerMode, SchedulerState};
use rust_desktop_ui_core::{Rect, Size};
use rust_desktop_ui_data::{
    ScheduleError, ScheduleOccurrence, ScheduleOccurrenceId, ScheduleResourceId,
    ScheduleUtcInterval,
};

const CARD_WIDTH: f32 = 176.0;
const CARD_HEIGHT: f32 = 48.0;
const GAP: f32 = 6.0;
const DAY_HEIGHT: f32 = 1008.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct SchedulerPlacementKey {
    pub occurrence: ScheduleOccurrenceId,
    pub resource: Option<ScheduleResourceId>,
    pub segment: u16,
}
#[derive(Clone, Debug, PartialEq)]
pub struct SchedulerAppointment {
    pub key: SchedulerPlacementKey,
    pub bounds: Rect,
    /// Exact projected duration, relative to the readable card. Calendar-list
    /// entries in Month have no proportional time mark.
    pub time_span: Option<Rect>,
    pub caption: String,
    pub description: String,
}
#[derive(Clone, Debug, PartialEq)]
pub struct SchedulerGeometry {
    pub extent: Size,
    pub appointments: Vec<SchedulerAppointment>,
    pub captions: Vec<PlannerCaption>,
    pub rules: Vec<PlannerRule>,
}
impl SchedulerState {
    pub fn geometry(&self, width: f32) -> Result<SchedulerGeometry, ScheduleError> {
        if !width.is_finite() || !(160.0..=65536.0).contains(&width) {
            return Err(ScheduleError::Invalid(
                "Scheduler viewport width must be 160..65536".into(),
            ));
        }
        let mut resources = self
            .model()
            .resources()
            .filter(|r| {
                self.settings().resources.is_empty() || self.settings().resources.contains(&r.id)
            })
            .map(|r| (Some(r.id), r.label.clone()))
            .collect::<Vec<_>>();
        if self.settings().resources.is_empty()
            && (resources.is_empty() || self.occurrences().iter().any(|o| o.resources.is_empty()))
        {
            resources.push((None, "Без ресурса".into()));
        }
        let mut result = SchedulerGeometry {
            extent: Size::new(width, 0.0),
            appointments: Vec::new(),
            captions: Vec::new(),
            rules: Vec::new(),
        };
        let matches = |o: &ScheduleOccurrence, id: Option<ScheduleResourceId>| {
            id.map_or(o.resources.is_empty(), |id| o.resources.contains(&id))
        };
        match self.settings().mode {
            SchedulerMode::Day | SchedulerMode::Week => {
                let available_column =
                    (width - 64.0) / (self.days().len() * resources.len()).max(1) as f32;
                let mut x = 64.0;
                for (di, (date, day)) in self.days().iter().enumerate() {
                    for (resource, label) in &resources {
                        let values = self
                            .occurrences()
                            .iter()
                            .filter(|o| matches(o, *resource) && o.utc.overlaps(*day))
                            .collect::<Vec<_>>();
                        let spans = values.iter().map(|o| {
                            let top = fraction(o.utc.start.0.max(day.start.0), *day) * DAY_HEIGHT;
                            let end = fraction(o.utc.end.0.min(day.end.0), *day) * DAY_HEIGHT;
                            (top, top + (end - top).max(CARD_HEIGHT) + GAP)
                        });
                        let (lanes, count) = display_lanes(spans);
                        let lane_count = count.max(1) as f32;
                        let column = available_column.max(lane_count * (CARD_WIDTH + GAP) + GAP);
                        let lane_width = (column - GAP) / lane_count;
                        result.captions.push(PlannerCaption {
                            bounds: Rect::new(x + 4.0, 4.0, column - 8.0, 24.0),
                            text: format!("{:02}.{:02}", date.day(), date.month()),
                            heading: true,
                        });
                        result.captions.push(PlannerCaption {
                            bounds: Rect::new(x + 4.0, 29.0, column - 8.0, 28.0),
                            text: label.clone(),
                            heading: false,
                        });
                        result.rules.push(PlannerRule {
                            bounds: Rect::new(x, 0.0, 1.0, 1080.0),
                        });
                        for (o, lane) in values.into_iter().zip(lanes) {
                            let top = fraction(o.utc.start.0.max(day.start.0), *day) * DAY_HEIGHT;
                            let end = fraction(o.utc.end.0.min(day.end.0), *day) * DAY_HEIGHT;
                            push(
                                &mut result,
                                o,
                                *resource,
                                di as u16,
                                Rect::new(
                                    x + GAP + lane as f32 * lane_width,
                                    64.0 + top,
                                    lane_width - GAP,
                                    (end - top).max(CARD_HEIGHT),
                                ),
                                Some(Rect::new(0.0, 0.0, 3.0, end - top)),
                                label,
                            )?;
                        }
                        x += column;
                    }
                }
                result.extent = Size::new(x.max(width), 64.0 + DAY_HEIGHT + CARD_HEIGHT + 64.0);
                for tick in 0..=24 {
                    let y = 64.0 + tick as f32 * DAY_HEIGHT / 24.0;
                    result.rules.push(PlannerRule {
                        bounds: Rect::new(60.0, y, result.extent.width - 60.0, 1.0),
                    });
                    result.captions.push(PlannerCaption {
                        bounds: Rect::new(4.0, y, 52.0, 20.0),
                        text: format!("{:02}/24", tick),
                        heading: false,
                    });
                }
                result.captions.push(PlannerCaption {
                    bounds: Rect::new(
                        64.0,
                        64.0 + DAY_HEIGHT + CARD_HEIGHT + GAP,
                        result.extent.width - 64.0,
                        28.0,
                    ),
                    text:
                        "Шкала — доля реальных суток UTC; точное местное время указано во встрече."
                            .into(),
                    heading: false,
                });
            }
            SchedulerMode::Timeline => {
                let left = 160.0;
                let range = ScheduleUtcInterval {
                    start: self.days()[0].1.start,
                    end: self.days().last().unwrap().1.end,
                };
                let usable = (width - left).max(self.days().len() as f32 * (CARD_WIDTH + 16.0));
                result.extent.width = left + usable + CARD_WIDTH + GAP;
                for (date, day) in self.days() {
                    let x = left + fraction(day.start.0, range) * usable;
                    result.captions.push(PlannerCaption {
                        bounds: Rect::new(x + 4.0, 4.0, 96.0, 24.0),
                        text: format!("{:02}.{:02}", date.day(), date.month()),
                        heading: true,
                    });
                }
                let mut y = 40.0;
                for (resource, label) in &resources {
                    let values = self
                        .occurrences()
                        .iter()
                        .filter(|o| matches(o, *resource))
                        .collect::<Vec<_>>();
                    let spans = values.iter().map(|o| {
                        let start = fraction(o.utc.start.0.max(range.start.0), range) * usable;
                        let end = fraction(o.utc.end.0.min(range.end.0), range) * usable;
                        (start, start + (end - start).max(CARD_WIDTH) + GAP)
                    });
                    let (lanes, count) = display_lanes(spans);
                    let height = count.max(1) as f32 * (CARD_HEIGHT + GAP) + GAP * 2.0;
                    result.captions.push(PlannerCaption {
                        bounds: Rect::new(4.0, y + 8.0, left - 8.0, 28.0),
                        text: label.clone(),
                        heading: true,
                    });
                    result.rules.push(PlannerRule {
                        bounds: Rect::new(0.0, y, result.extent.width, 1.0),
                    });
                    for (o, lane) in values.into_iter().zip(lanes) {
                        let x = left + fraction(o.utc.start.0.max(range.start.0), range) * usable;
                        let end = left + fraction(o.utc.end.0.min(range.end.0), range) * usable;
                        push(
                            &mut result,
                            o,
                            *resource,
                            0,
                            Rect::new(
                                x,
                                y + GAP + lane as f32 * (CARD_HEIGHT + GAP),
                                (end - x).max(CARD_WIDTH),
                                CARD_HEIGHT,
                            ),
                            Some(Rect::new(0.0, 0.0, end - x, 3.0)),
                            label,
                        )?;
                    }
                    y += height;
                }
                result.captions.push(PlannerCaption {
                    bounds: Rect::new(left, y + GAP, usable, 28.0),
                    text: "Метка — длительность; карточка сохраняет читаемый размер. Точное время указано внутри.".into(),
                    heading: false,
                });
                result.extent.height = (y + 40.0).max(240.0);
                for (_, day) in self.days() {
                    let x = left + fraction(day.start.0, range) * usable;
                    result.rules.push(PlannerRule {
                        bounds: Rect::new(x, 28.0, 1.0, result.extent.height - 28.0),
                    });
                }
            }
            SchedulerMode::Month => {
                let column = (width / 7.0).max(CARD_WIDTH + GAP * 2.0);
                result.extent.width = column * 7.0;
                let mut section_y = 0.0;
                for (resource, label) in &resources {
                    result.captions.push(PlannerCaption {
                        bounds: Rect::new(4.0, section_y + 4.0, result.extent.width - 8.0, 28.0),
                        text: label.clone(),
                        heading: true,
                    });
                    section_y += 34.0;
                    for week in 0..6 {
                        let max_count = (0..7)
                            .map(|di| {
                                self.occurrences()
                                    .iter()
                                    .filter(|o| {
                                        matches(o, *resource)
                                            && o.utc.overlaps(self.days()[week * 7 + di].1)
                                    })
                                    .count()
                            })
                            .max()
                            .unwrap_or(0);
                        let row_height = (38.0 + max_count as f32 * (CARD_HEIGHT + GAP)).max(108.0);
                        for di in 0..7 {
                            let index = week * 7 + di;
                            let (date, day) = self.days()[index];
                            let x = di as f32 * column;
                            result.rules.push(PlannerRule {
                                bounds: Rect::new(x, section_y, 1.0, row_height),
                            });
                            result.rules.push(PlannerRule {
                                bounds: Rect::new(x, section_y, column, 1.0),
                            });
                            result.captions.push(PlannerCaption {
                                bounds: Rect::new(x + 4.0, section_y + 4.0, column - 8.0, 26.0),
                                text: format!("{:02}.{:02}", date.day(), date.month()),
                                heading: true,
                            });
                            for (n, o) in self
                                .occurrences()
                                .iter()
                                .filter(|o| matches(o, *resource) && o.utc.overlaps(day))
                                .enumerate()
                            {
                                push(
                                    &mut result,
                                    o,
                                    *resource,
                                    index as u16,
                                    Rect::new(
                                        x + GAP,
                                        section_y + 32.0 + n as f32 * (CARD_HEIGHT + GAP),
                                        column - GAP * 2.0,
                                        CARD_HEIGHT,
                                    ),
                                    None,
                                    label,
                                )?;
                            }
                        }
                        section_y += row_height;
                    }
                    section_y += 16.0;
                }
                result.extent.height = section_y.max(240.0);
            }
        }
        if result.appointments.len() > 16384 {
            return Err(ScheduleError::Capacity("Scheduler visible segment budget"));
        }
        Ok(result)
    }
}
fn fraction(value: i64, range: ScheduleUtcInterval) -> f32 {
    ((value - range.start.0) as f64 / (range.end.0 - range.start.0) as f64) as f32
}
fn display_lanes(spans: impl Iterator<Item = (f32, f32)>) -> (Vec<usize>, usize) {
    let mut ends = Vec::new();
    let mut lanes = Vec::new();
    for (start, end) in spans {
        let lane = ends
            .iter()
            .position(|end| *end <= start)
            .unwrap_or(ends.len());
        if lane == ends.len() {
            ends.push(end);
        } else {
            ends[lane] = end;
        }
        lanes.push(lane);
    }
    (lanes, ends.len())
}
fn push(
    result: &mut SchedulerGeometry,
    o: &ScheduleOccurrence,
    resource: Option<ScheduleResourceId>,
    segment: u16,
    bounds: Rect,
    time_span: Option<Rect>,
    label: &str,
) -> Result<(), ScheduleError> {
    if result.appointments.len() >= 16384 {
        return Err(ScheduleError::Capacity("Scheduler visible segment budget"));
    }
    result.appointments.push(SchedulerAppointment {
        key: SchedulerPlacementKey {
            occurrence: o.id,
            resource,
            segment,
        },
        bounds,
        time_span,
        caption: format!(
            "{:02}:{:02}–{:02}:{:02} · {}",
            o.local.start.time().hour(),
            o.local.start.time().minute(),
            o.local.end.time().hour(),
            o.local.end.time().minute(),
            o.title
        ),
        description: format!(
            "{}; {}; {} — {}; UTC {} — {}; {}",
            o.title,
            label,
            o.local.start,
            o.local.end,
            o.utc.start.0,
            o.utc.end.0,
            if o.is_exception {
                "Изменённый экземпляр"
            } else {
                "Встреча"
            }
        ),
    });
    Ok(())
}
