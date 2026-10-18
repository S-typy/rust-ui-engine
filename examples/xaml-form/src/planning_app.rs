use crate::form::Form;
use rust_desktop_ui_controls::{
    GanttControl, SchedulerControl, SchedulerMode, SchedulerSettings, SchedulerState,
};
use rust_desktop_ui_core::{LayoutStyle, Length, Size, WidgetId};
use rust_desktop_ui_data::*;
use rust_desktop_ui_platform_winit::{DesktopApp, EventResponse, PlatformEvent, accesskit};
use rust_desktop_ui_xaml::WindowDefinition;
fn error(e: impl std::fmt::Display) -> String {
    e.to_string()
}
fn local(text: &str) -> Result<ScheduleLocal, String> {
    text.parse().map_err(error)
}
fn zone() -> Result<ScheduleTimeZone, String> {
    ScheduleTimeZone::new(
        "Europe/Moscow",
        ScheduleUtcInterval::new(ScheduleInstant(1767225600), ScheduleInstant(1830297600))
            .map_err(error)?,
        10800,
        vec![],
    )
    .map_err(error)
}
fn schedule() -> Result<SchedulingModel, String> {
    let mut model = SchedulingModel::new();
    let mut changes = vec![
        ScheduleChange::PutZone(zone()?),
        ScheduleChange::PutResource(ScheduleResource {
            id: ScheduleResourceId(1),
            label: "Разработка".into(),
        }),
        ScheduleChange::PutResource(ScheduleResource {
            id: ScheduleResourceId(2),
            label: "Дизайн".into(),
        }),
    ];
    for (id, title, from, to, resources, repeat) in [
        (
            1,
            "Планирование недели",
            "2026-10-19T09:00:00",
            "2026-10-19T10:30:00",
            vec![ScheduleResourceId(1), ScheduleResourceId(2)],
            false,
        ),
        (
            2,
            "Работа над формами",
            "2026-10-19T10:00:00",
            "2026-10-19T12:00:00",
            vec![ScheduleResourceId(1)],
            false,
        ),
        (
            3,
            "Обзор интерфейса",
            "2026-10-20T11:00:00",
            "2026-10-20T12:00:00",
            vec![ScheduleResourceId(2)],
            true,
        ),
        (
            4,
            "Выпуск сборки",
            "2026-10-22T14:00:00",
            "2026-10-22T16:00:00",
            vec![ScheduleResourceId(1)],
            false,
        ),
    ] {
        changes.push(ScheduleChange::PutSeries(ScheduleSeries {
            id: ScheduleSeriesId(id),
            title: title.into(),
            zone: "Europe/Moscow".into(),
            interval: ScheduleLocalInterval::new(local(from)?, local(to)?).map_err(error)?,
            resources,
            recurrence: if repeat {
                ScheduleRecurrence {
                    frequency: ScheduleFrequency::Daily,
                    end: ScheduleTermination::Count(3),
                    ..Default::default()
                }
            } else {
                Default::default()
            },
            policy: Default::default(),
        }));
    }
    model.apply(0, changes).map_err(error)?;
    Ok(model)
}
fn plan() -> Result<GanttPlan, String> {
    let zone = zone()?;
    let first = local("2026-10-19T09:00:00")?;
    let start = zone
        .resolve(first, Default::default())
        .map_err(error)?
        .ok_or("missing start")?;
    let mut working = Vec::new();
    for day in 0..19 {
        if day % 7 < 5 {
            working.push(
                ScheduleUtcInterval::new(
                    start.checked_add(day * 86400).map_err(error)?,
                    start.checked_add(day * 86400 + 8 * 3600).map_err(error)?,
                )
                .map_err(error)?,
            );
        }
    }
    let tasks = [
        (1, None, "Подготовка", 8, 100),
        (2, Some(1), "Модель данных", 16, 75),
        (3, Some(1), "Макет", 12, 100),
        (4, None, "Реализация", 24, 35),
        (5, Some(4), "Редакторы", 16, 20),
        (6, Some(4), "Доступность", 8, 0),
        (7, None, "Проверка и выпуск", 16, 0),
    ]
    .into_iter()
    .map(|(id, parent, title, hours, progress)| GanttTask {
        id: GanttTaskId(id),
        parent: parent.map(GanttTaskId),
        order: id as i64,
        title: title.into(),
        calendar: GanttCalendarId(1),
        work_seconds: hours * 3600,
        start_not_before: start,
        finish_not_after: None,
        fixed_start: None,
        progress_percent: progress,
    })
    .collect();
    let deps = [
        (1, 2),
        (1, 3),
        (2, 4),
        (3, 4),
        (4, 5),
        (4, 6),
        (5, 7),
        (6, 7),
    ]
    .into_iter()
    .map(|(a, b)| GanttDependency {
        predecessor: GanttTaskId(a),
        successor: GanttTaskId(b),
        kind: GanttDependencyKind::FinishStart,
        lag_seconds: 0,
    })
    .collect();
    let mut plan = GanttPlan::new();
    plan.replace(
        0,
        vec![GanttCalendar {
            id: GanttCalendarId(1),
            label: "Пн–Пт, 09:00–17:00 Москва".into(),
            working,
        }],
        tasks,
        deps,
    )
    .map_err(error)?;
    Ok(plan)
}
enum View {
    Scheduler(Box<SchedulerControl>),
    Gantt(Box<GanttControl>),
}
pub struct PlanningDemo {
    form: Form,
    view: View,
    diagnostics: bool,
    last: String,
}
impl PlanningDemo {
    pub fn new(
        definition: WindowDefinition,
        gantt: bool,
        diagnostics: bool,
    ) -> Result<Self, String> {
        let mut form = Form::new(definition, false)?;
        let host = form
            .find_name("PlanningHost")
            .ok_or("missing PlanningHost")?;
        let style = LayoutStyle {
            width: Length::Percent(1.0),
            height: Length::Percent(1.0),
            ..Default::default()
        };
        let view = if gantt {
            form.ui
                .set_label(
                    form.find_name("PlanningTitle").unwrap(),
                    "План задач и зависимости",
                )
                .map_err(error)?;
            form.ui.set_label(form.find_name("PlanningHint").unwrap(),"Дерево и временная шкала используют одни ключи. Красная рамка отмечает критический путь; изменения применяются атомарно.").map_err(error)?;
            View::Gantt(Box::new(
                GanttControl::mount(&mut form.ui, host, plan()?, style).map_err(error)?,
            ))
        } else {
            View::Scheduler(Box::new(
                SchedulerControl::mount(
                    &mut form.ui,
                    host,
                    SchedulerState::new(
                        schedule()?,
                        SchedulerSettings {
                            mode: SchedulerMode::Timeline,
                            anchor: DateValue::new(2026, 10, 19).map_err(error)?,
                            display_zone: "Europe/Moscow".into(),
                            resources: vec![],
                        },
                    )
                    .map_err(error)?,
                    style,
                )
                .map_err(error)?,
            ))
        };
        Ok(Self {
            form,
            view,
            diagnostics,
            last: String::new(),
        })
    }
    pub fn preview_mode(&mut self, index: u32) -> Result<(), String> {
        let mode = match index {
            0 => SchedulerMode::Day,
            1 => SchedulerMode::Week,
            2 => SchedulerMode::Month,
            3 => SchedulerMode::Timeline,
            _ => {
                return Err(
                    "planning preview must be 0 (Day), 1 (Week), 2 (Month), or 3 (Timeline)".into(),
                );
            }
        };
        match &mut self.view {
            View::Scheduler(view) => view.set_mode(&mut self.form.ui, mode).map_err(error),
            View::Gantt(_) => Err("planning preview is only available for the scheduler".into()),
        }
    }
    fn process(&mut self) -> Result<(), String> {
        match &mut self.view {
            View::Scheduler(v) => v.process(&mut self.form.ui),
            View::Gantt(v) => v.process(&mut self.form.ui),
        }
        .map_err(error)
    }
    fn diagnose(&mut self) {
        if !self.diagnostics {
            return;
        }
        let value = match &self.view {
            View::Scheduler(v) => format!(
                "planning-state scheduler view={:?} revision={} occurrences={} placements={} selected={:?} draft={}",
                v.state().settings().mode,
                v.state().model().revision(),
                v.state().occurrences().len(),
                v.geometry().map_or(0, |g| g.appointments.len()),
                v.state().selected(),
                v.state().draft().is_some()
            ),
            View::Gantt(v) => format!(
                "planning-state gantt revision={} tasks={} bars={} selected={:?} draft={}",
                v.plan().revision(),
                v.plan().tasks().count(),
                v.geometry().len(),
                v.selected(),
                v.draft().is_some()
            ),
        };
        if value != self.last {
            println!("{value}");
            self.last = value;
        }
    }
}
impl DesktopApp for PlanningDemo {
    fn title(&self) -> String {
        self.form.title()
    }
    fn scene(&self) -> &rust_desktop_ui_core::Scene {
        self.form.scene()
    }
    fn update(&mut self, size: Size, scale: f32) -> Result<(), String> {
        self.process()?;
        for _ in 0..3 {
            self.form.update(size, scale)?;
            let changed = match &mut self.view {
                View::Scheduler(v) => v.refresh(&mut self.form.ui),
                View::Gantt(v) => v.refresh(&mut self.form.ui),
            }
            .map_err(error)?;
            if !changed {
                break;
            }
        }
        self.form.update(size, scale)?;
        self.diagnose();
        Ok(())
    }
    fn event(&mut self, event: PlatformEvent) -> Result<EventResponse, String> {
        if let PlatformEvent::Input(input) = &event {
            let handled = match &mut self.view {
                View::Scheduler(v) => v.input(&mut self.form.ui, input),
                View::Gantt(v) => v.input(&mut self.form.ui, input),
            }
            .map_err(error)?;
            if handled {
                return Ok(EventResponse {
                    redraw: true,
                    ..Default::default()
                });
            }
        }
        let mut response = self.form.event(event)?;
        self.process()?;
        response.redraw = true;
        Ok(response)
    }
    fn set_waker(&mut self, w: Option<rust_desktop_ui_platform_winit::AppWaker>) {
        self.form.set_waker(w)
    }
    fn pointer_capture_lost(&mut self) -> Result<EventResponse, String> {
        self.form.pointer_capture_lost()
    }
    fn wake_after(&self) -> Option<std::time::Duration> {
        self.form.wake_after()
    }
    fn ime_cursor(&self) -> Option<rust_desktop_ui_core::Rect> {
        self.form.ime_cursor()
    }
    fn ime_target(&self) -> Option<WidgetId> {
        self.form.ime_target()
    }
    fn ime_composition_active(&self) -> Option<bool> {
        self.form.ime_composition_active()
    }
    fn accessibility(&mut self, scale: f32) -> accesskit::TreeUpdate {
        self.form.accessibility(scale)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sample_schedule_and_plan_validate_with_real_overlap_and_critical_path() {
        let model = schedule().unwrap();
        assert_eq!(model.series_iter().count(), 4);
        let p = plan().unwrap();
        assert_eq!(p.tasks().count(), 7);
        assert!(p.tasks().any(|t| p.placement(t.id).unwrap().critical));
        assert!(p.tasks().any(|t| !p.placement(t.id).unwrap().critical));
    }
    #[test]
    fn passive_preview_selects_each_view_before_layout_and_rejects_gantt() {
        let definition = crate::generated_planning::window_definition();
        let mut demo = PlanningDemo::new(definition, false, false).unwrap();
        for (index, mode) in [
            SchedulerMode::Day,
            SchedulerMode::Week,
            SchedulerMode::Month,
            SchedulerMode::Timeline,
        ]
        .into_iter()
        .enumerate()
        {
            demo.preview_mode(index as u32).unwrap();
            let View::Scheduler(view) = &demo.view else {
                unreachable!()
            };
            assert_eq!(view.state().settings().mode, mode);
        }
        assert!(demo.preview_mode(4).is_err());
        let View::Scheduler(view) = &demo.view else {
            unreachable!()
        };
        assert_eq!(view.state().settings().mode, SchedulerMode::Timeline);
        let mut gantt =
            PlanningDemo::new(crate::generated_planning::window_definition(), true, false).unwrap();
        assert!(gantt.preview_mode(0).is_err());
    }
}
