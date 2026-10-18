use super::common::*;
use super::*;
use crate::{Controls, Theme};
use rust_desktop_ui_core::{
    InputEvent, Key, LayoutStyle, Length, Mutation, Point, PointerButton, Size,
};
use rust_desktop_ui_data::*;
fn local(value: &str) -> ScheduleLocal {
    value.parse().unwrap()
}
fn instant(value: &str) -> ScheduleInstant {
    utc_zone()
        .unwrap()
        .resolve(local(value), Default::default())
        .unwrap()
        .unwrap()
}
fn schedule() -> SchedulingModel {
    let mut model = SchedulingModel::new();
    let zone = ScheduleTimeZone::new(
        "Europe/Test",
        ScheduleUtcInterval::new(
            instant("2026-01-01T00:00:00"),
            instant("2027-01-01T00:00:00"),
        )
        .unwrap(),
        3600,
        vec![
            ScheduleTransition {
                at: instant("2026-03-29T01:00:00"),
                offset_after: 7200,
            },
            ScheduleTransition {
                at: instant("2026-10-25T01:00:00"),
                offset_after: 3600,
            },
        ],
    )
    .unwrap();
    model
        .apply(
            0,
            vec![
                ScheduleChange::PutZone(zone),
                ScheduleChange::PutResource(ScheduleResource {
                    id: ScheduleResourceId(1),
                    label: "Команда".into(),
                }),
                ScheduleChange::PutResource(ScheduleResource {
                    id: ScheduleResourceId(2),
                    label: "Переговорная".into(),
                }),
                ScheduleChange::PutSeries(ScheduleSeries {
                    id: ScheduleSeriesId(1),
                    title: "Планирование".into(),
                    zone: "Europe/Test".into(),
                    interval: ScheduleLocalInterval::new(
                        local("2026-03-29T01:30:00"),
                        local("2026-03-29T03:30:00"),
                    )
                    .unwrap(),
                    resources: vec![ScheduleResourceId(1)],
                    recurrence: Default::default(),
                    policy: Default::default(),
                }),
                ScheduleChange::PutSeries(ScheduleSeries {
                    id: ScheduleSeriesId(2),
                    title: "Пересечение".into(),
                    zone: "Europe/Test".into(),
                    interval: ScheduleLocalInterval::new(
                        local("2026-03-29T03:00:00"),
                        local("2026-03-29T04:00:00"),
                    )
                    .unwrap(),
                    resources: vec![ScheduleResourceId(1), ScheduleResourceId(2)],
                    recurrence: ScheduleRecurrence {
                        frequency: ScheduleFrequency::Daily,
                        end: ScheduleTermination::Count(3),
                        ..Default::default()
                    },
                    policy: Default::default(),
                }),
            ],
        )
        .unwrap();
    model
}
fn state(mode: SchedulerMode) -> SchedulerState {
    SchedulerState::new(
        schedule(),
        SchedulerSettings {
            mode,
            anchor: DateValue::new(2026, 3, 29).unwrap(),
            display_zone: "Europe/Test".into(),
            resources: vec![],
        },
    )
    .unwrap()
}
#[test]
fn scheduler_all_four_views_share_stable_occurrences_and_dst_duration() {
    let mut state = state(SchedulerMode::Day);
    let first = state.occurrences()[0].clone();
    assert_eq!(first.local.wall_seconds(), 7200);
    assert_eq!(first.utc.end.0 - first.utc.start.0, 3600);
    assert_eq!(
        state.days()[0].1.end.0 - state.days()[0].1.start.0,
        23 * 3600
    );
    for mode in [
        SchedulerMode::Day,
        SchedulerMode::Week,
        SchedulerMode::Month,
        SchedulerMode::Timeline,
    ] {
        let mut settings = state.settings().clone();
        settings.mode = mode;
        state.set_settings(settings).unwrap();
        assert_eq!(
            state.occurrences().iter().find(|o| o.id == first.id),
            Some(&first)
        );
        let geometry = state.geometry(900.0).unwrap();
        assert!(geometry.appointments.iter().all(|p| p.bounds.is_valid()));
        assert!(
            geometry
                .appointments
                .iter()
                .any(|p| p.key.occurrence == first.id)
        );
        assert!(!geometry.captions.is_empty());
    }
}
#[test]
fn scheduler_overlap_lanes_and_resource_groups_do_not_share_same_rectangle() {
    let state = state(SchedulerMode::Day);
    let g = state.geometry(900.0).unwrap();
    let a = g
        .appointments
        .iter()
        .find(|p| p.key.occurrence.series == ScheduleSeriesId(1))
        .unwrap();
    let b = g
        .appointments
        .iter()
        .find(|p| {
            p.key.occurrence.series == ScheduleSeriesId(2)
                && p.key.resource == Some(ScheduleResourceId(1))
        })
        .unwrap();
    assert!(a.bounds.x + a.bounds.width <= b.bounds.x || b.bounds.x + b.bounds.width <= a.bounds.x);
    assert_eq!(
        g.appointments
            .iter()
            .filter(|p| p.key.occurrence.series == ScheduleSeriesId(2))
            .count(),
        2
    );
}
#[test]
fn scheduler_readable_cards_pack_visual_collisions_without_changing_time_spans() {
    let mut model = schedule();
    let template = model.series(ScheduleSeriesId(1)).unwrap().clone();
    let changes = (0..6)
        .map(|index| {
            let start = local("2026-03-29T10:00:00")
                .checked_add(index * 30)
                .unwrap();
            let mut series = template.clone();
            series.id = ScheduleSeriesId(10 + index as u64);
            series.title = format!("Короткая встреча {index}");
            series.interval =
                ScheduleLocalInterval::new(start, start.checked_add(5).unwrap()).unwrap();
            ScheduleChange::PutSeries(series)
        })
        .collect();
    model.apply(model.revision(), changes).unwrap();
    for mode in [
        SchedulerMode::Day,
        SchedulerMode::Week,
        SchedulerMode::Month,
        SchedulerMode::Timeline,
    ] {
        let state = SchedulerState::new(
            model.clone(),
            SchedulerSettings {
                mode,
                anchor: DateValue::new(2026, 3, 29).unwrap(),
                display_zone: "Europe/Test".into(),
                resources: vec![],
            },
        )
        .unwrap();
        let geometry = state.geometry(640.0).unwrap();
        for (index, card) in geometry.appointments.iter().enumerate() {
            assert!(
                card.bounds.width >= 175.99 && card.bounds.height >= 47.99,
                "{mode:?}: {:?}",
                card.bounds
            );
            assert!(card.bounds.x >= 0.0 && card.bounds.y >= 0.0);
            assert!(card.bounds.x + card.bounds.width <= geometry.extent.width + 0.01);
            assert!(card.bounds.y + card.bounds.height <= geometry.extent.height + 0.01);
            assert!(card.caption.contains('–'));
            for other in &geometry.appointments[index + 1..] {
                assert!(
                    card.bounds.intersection(other.bounds).is_none(),
                    "{mode:?}: overlapping hit rectangles {:?} {:?}",
                    card.key,
                    other.key
                );
            }
            assert_eq!(card.time_span.is_none(), mode == SchedulerMode::Month);
        }
        let short = geometry
            .appointments
            .iter()
            .find(|card| card.key.occurrence.series == ScheduleSeriesId(10))
            .unwrap();
        if let Some(mark) = short.time_span {
            assert!(mark.width <= short.bounds.width && mark.height <= short.bounds.height);
            if mode == SchedulerMode::Timeline {
                assert!(mark.width < 1.0);
            } else {
                assert!(mark.height < 1.0);
            }
        }
    }
}
#[test]
fn scheduler_instance_cancel_validation_stale_revision_and_series_deletion_are_atomic() {
    let mut s = state(SchedulerMode::Day);
    let id = s.occurrences()[0].id;
    s.select(Some(id)).unwrap();
    s.begin_edit(SchedulerEditScope::Instance).unwrap();
    let original = s.model().clone();
    s.draft_mut().unwrap().title = String::new();
    assert!(s.commit().is_err());
    assert_eq!(s.model(), &original);
    assert!(s.draft().is_some());
    s.cancel();
    assert_eq!(s.model(), &original);
    s.begin_edit(SchedulerEditScope::Instance).unwrap();
    s.draft_mut().unwrap().title = "Изменено".into();
    s.commit().unwrap();
    assert_eq!(
        s.occurrences().iter().find(|o| o.id == id).unwrap().title,
        "Изменено"
    );
    assert_eq!(s.model().series(id.series).unwrap().title, "Планирование");
    s.begin_edit(SchedulerEditScope::Instance).unwrap();
    let mut replacement = s.model().clone();
    replacement
        .apply(
            replacement.revision(),
            vec![ScheduleChange::PutResource(ScheduleResource {
                id: ScheduleResourceId(1),
                label: "Новое имя".into(),
            })],
        )
        .unwrap();
    s.replace_model(replacement).unwrap();
    assert!(s.commit().is_err());
    assert!(s.draft().is_some());
    s.cancel();
    s.delete_selected(SchedulerEditScope::Series).unwrap();
    assert!(s.model().series(id.series).is_none());
    assert_eq!(s.model().exceptions().count(), 0);
}
#[test]
fn scheduler_new_and_shift_resize_commit_only_when_accepted() {
    let mut s = state(SchedulerMode::Day);
    let id = s.occurrences()[1].id;
    s.select(Some(id)).unwrap();
    s.begin_edit(SchedulerEditScope::Instance).unwrap();
    let before = s.model().clone();
    s.shift_draft(900, false).unwrap();
    s.shift_draft(900, true).unwrap();
    assert_eq!(s.model(), &before);
    let expected = s.draft().unwrap().interval;
    s.commit().unwrap();
    assert_eq!(
        s.occurrences().iter().find(|o| o.id == id).unwrap().local,
        expected
    );
    let mut series = s.model().series(id.series).unwrap().clone();
    series.id = ScheduleSeriesId(10);
    series.recurrence = Default::default();
    s.begin_create(series).unwrap();
    assert!(s.model().series(ScheduleSeriesId(10)).is_none());
    s.commit().unwrap();
    assert!(s.model().series(ScheduleSeriesId(10)).is_some());
}
fn settle_scheduler(ui: &mut Controls, view: &mut SchedulerControl) {
    settle_scheduler_size(ui, view, Size::new(1100.0, 850.0));
}
fn settle_scheduler_size(ui: &mut Controls, view: &mut SchedulerControl, size: Size) {
    for _ in 0..4 {
        ui.update(size).unwrap();
        view.refresh(ui).unwrap();
    }
    ui.update(size).unwrap();
}
#[test]
fn scheduler_initial_and_resize_reveal_clear_scrollbars_without_refresh_scroll_reset() {
    let mut model = schedule();
    let changes = model
        .series_iter()
        .cloned()
        .enumerate()
        .map(|(index, mut series)| {
            let start = local("2026-03-29T09:00:00")
                .checked_add(index as i64 * 3600)
                .unwrap();
            series.interval =
                ScheduleLocalInterval::new(start, start.checked_add(5400).unwrap()).unwrap();
            ScheduleChange::PutSeries(series)
        })
        .collect();
    model.apply(model.revision(), changes).unwrap();
    for mode in [
        SchedulerMode::Day,
        SchedulerMode::Week,
        SchedulerMode::Month,
    ] {
        let state = SchedulerState::new(
            model.clone(),
            SchedulerSettings {
                mode,
                anchor: DateValue::new(2026, 3, 29).unwrap(),
                display_zone: "Europe/Test".into(),
                resources: vec![],
            },
        )
        .unwrap();
        let mut ui = Controls::new(Theme::FluentLight).unwrap();
        let root = ui.root();
        let mut view = SchedulerControl::mount(
            &mut ui,
            root,
            state,
            LayoutStyle {
                width: Length::Percent(1.0),
                height: Length::Percent(1.0),
                ..Default::default()
            },
        )
        .unwrap();
        let large = Size::new(1180.0, 920.0);
        settle_scheduler_size(&mut ui, &mut view, large);
        let first = view.geometry().unwrap().appointments[0].key;
        let check = |ui: &Controls, view: &SchedulerControl, key, stage: &str| {
            let bounds = ui
                .runtime
                .bounds(view.appointment_widget(key).unwrap())
                .unwrap();
            let viewport = ui.runtime.bounds(view.viewport).unwrap();
            assert!(
                bounds.x >= viewport.x - 0.01 && bounds.y >= viewport.y + 23.99,
                "{mode:?} {stage}: {bounds:?}, {viewport:?}; scroll={:?}",
                ui.scroll_offset(view.viewport)
            );
            assert!(
                bounds.x + bounds.width <= viewport.x + viewport.width - 23.99,
                "{mode:?} {stage}: {bounds:?}, {viewport:?}; scroll={:?}",
                ui.scroll_offset(view.viewport)
            );
            assert!(
                bounds.y + bounds.height <= viewport.y + viewport.height - 23.99,
                "{mode:?} {stage}: {bounds:?}, {viewport:?}; scroll={:?}",
                ui.scroll_offset(view.viewport)
            );
            bounds
        };
        let initial = check(&ui, &view, first, "initial");
        let viewport = ui.runtime.bounds(view.viewport).unwrap();
        assert!(
            (initial.y - viewport.y - 48.0).abs() < 0.01,
            "initial time context: {mode:?}"
        );
        let focus = ui.runtime.focused();
        ui.scroll_to_point(view.viewport, Point::ZERO).unwrap();
        settle_scheduler_size(&mut ui, &mut view, large);
        assert_eq!(ui.scroll_offset(view.viewport), Some(Point::ZERO));
        let resize_viewport = |ui: &mut Controls, view: &SchedulerControl, size: Size| {
            let mut style = ui
                .runtime
                .tree()
                .node(view.viewport)
                .unwrap()
                .props
                .style
                .clone();
            style.width = Length::Px(size.width);
            style.height = Length::Px(size.height);
            ui.runtime
                .apply(Mutation::SetStyle {
                    node: view.viewport,
                    style,
                })
                .unwrap();
        };
        let small = Size::new(884.0, 600.0);
        resize_viewport(&mut ui, &view, Size::new(840.0, 320.0));
        settle_scheduler_size(&mut ui, &mut view, small);
        let resized = ui.runtime.bounds(view.viewport).unwrap();
        assert_eq!((resized.width, resized.height), (840.0, 320.0));
        check(&ui, &view, first, "shrink");
        let selected = view
            .geometry()
            .unwrap()
            .appointments
            .last()
            .unwrap()
            .key
            .occurrence;
        let key = view
            .geometry()
            .unwrap()
            .appointments
            .iter()
            .find(|p| p.key.occurrence == selected)
            .unwrap()
            .key;
        ui.activate(view.appointment_widget(key).unwrap()).unwrap();
        view.process(&mut ui).unwrap();
        assert_eq!(view.state().selected(), Some(selected));
        ui.scroll_to_point(view.viewport, Point::ZERO).unwrap();
        settle_scheduler_size(&mut ui, &mut view, small);
        assert_eq!(ui.scroll_offset(view.viewport), Some(Point::ZERO));
        resize_viewport(&mut ui, &view, Size::new(840.0, 240.0));
        settle_scheduler_size(&mut ui, &mut view, small);
        assert_eq!(ui.runtime.bounds(view.viewport).unwrap().height, 240.0);
        check(&ui, &view, key, "height");
        resize_viewport(&mut ui, &view, Size::new(1140.0, 420.0));
        settle_scheduler_size(&mut ui, &mut view, large);
        let resized = ui.runtime.bounds(view.viewport).unwrap();
        assert_eq!((resized.width, resized.height), (1140.0, 420.0));
        check(&ui, &view, key, "grow");
        assert_eq!(ui.runtime.focused(), focus);
    }
}
#[test]
fn scheduler_retained_scene_pointer_activation_editor_cancel_and_names() {
    for theme in [Theme::FluentLight, Theme::FluentDark] {
        let mut ui = Controls::new(theme).unwrap();
        let root = ui.root();
        let mut view = SchedulerControl::mount(
            &mut ui,
            root,
            state(SchedulerMode::Timeline),
            LayoutStyle {
                width: Length::Percent(1.0),
                height: Length::Percent(1.0),
                ..Default::default()
            },
        )
        .unwrap();
        settle_scheduler(&mut ui, &mut view);
        let p = view.geometry().unwrap().appointments[0].clone();
        let widget = view.appointment_widget(p.key).unwrap();
        let b = ui.runtime.bounds(widget).unwrap();
        let viewport = ui.runtime.bounds(view.viewport).unwrap();
        let visible = b
            .intersection(viewport)
            .expect("initial appointment is visible");
        for (visible_value, card_value) in [
            (visible.x, b.x),
            (visible.y, b.y),
            (visible.width, b.width),
            (visible.height, b.height),
        ] {
            assert!(
                (visible_value - card_value).abs() <= 0.01,
                "initial appointment is fully visible: card={b:?}, visible={visible:?}"
            );
        }
        assert!(b.width >= 175.99 && b.height >= 47.99);
        assert!(
            ui.scene()
                .texts
                .iter()
                .any(|text| text.text.contains("01:30–03:30")
                    && text.text.contains("Планирование")
                    && text.bounds.width > 100.0
                    && text.clip.width > 100.0)
        );
        let position = Point::new(b.x + b.width * 0.5, b.y + b.height * 0.5);
        for event in [
            InputEvent::PointerDown {
                position,
                button: PointerButton::Primary,
                modifiers: Default::default(),
            },
            InputEvent::PointerUp {
                position,
                button: PointerButton::Primary,
                modifiers: Default::default(),
            },
        ] {
            ui.dispatch(event).unwrap();
            view.process(&mut ui).unwrap();
        }
        assert_eq!(view.state().selected(), Some(p.key.occurrence));
        view.begin_edit(&mut ui, SchedulerEditScope::Instance)
            .unwrap();
        settle_scheduler(&mut ui, &mut view);
        ui.set_text(view.title_editor, "Черновик").unwrap();
        ui.runtime
            .apply(Mutation::Focus(Some(view.title_editor)))
            .unwrap();
        assert!(
            view.input(
                &mut ui,
                &InputEvent::KeyDown {
                    key: Key::Escape,
                    modifiers: Default::default(),
                    repeat: false
                }
            )
            .unwrap()
        );
        assert!(view.state().draft().is_none());
        assert!(
            !view
                .state()
                .model()
                .series_iter()
                .any(|s| s.title == "Черновик")
        );
        settle_scheduler(&mut ui, &mut view);
        assert_eq!(view.appointment_widget(p.key), Some(widget));
        let scroll = ui.scroll_offset(view.viewport);
        settle_scheduler(&mut ui, &mut view);
        assert_eq!(ui.scroll_offset(view.viewport), scroll);
        assert!(
            ui.semantics()
                .iter()
                .any(|n| n.id == widget && n.label.contains("UTC"))
        );
        view.begin_edit(&mut ui, SchedulerEditScope::Instance)
            .unwrap();
        settle_scheduler(&mut ui, &mut view);
        ui.set_text(view.title_editor, "Принято клавиатурой")
            .unwrap();
        ui.runtime
            .apply(Mutation::Focus(Some(view.title_editor)))
            .unwrap();
        assert!(
            view.input(
                &mut ui,
                &InputEvent::KeyDown {
                    key: Key::Enter,
                    modifiers: rust_desktop_ui_core::Modifiers {
                        control: true,
                        ..Default::default()
                    },
                    repeat: false
                }
            )
            .unwrap()
        );
        assert_eq!(
            view.state()
                .occurrences()
                .iter()
                .find(|o| o.id == p.key.occurrence)
                .unwrap()
                .title,
            "Принято клавиатурой"
        );
        settle_scheduler(&mut ui, &mut view);
        assert_eq!(view.appointment_widget(p.key), Some(widget));
        view.unmount(&mut ui).unwrap();
    }
}
fn plan() -> GanttPlan {
    let start = instant("2026-10-19T09:00:00");
    let working = (0..10)
        .map(|d| {
            ScheduleUtcInterval::new(
                start.checked_add(d * 86400).unwrap(),
                start.checked_add(d * 86400 + 8 * 3600).unwrap(),
            )
            .unwrap()
        })
        .collect();
    let tasks = (1..=40)
        .map(|i| GanttTask {
            id: GanttTaskId(i),
            parent: if i > 1 { Some(GanttTaskId(1)) } else { None },
            order: i as i64,
            title: format!("Задача {i}"),
            calendar: GanttCalendarId(1),
            work_seconds: 3600 * (1 + i as i64 % 3),
            start_not_before: start,
            finish_not_after: None,
            fixed_start: None,
            progress_percent: ((i % 21) * 5) as u8,
        })
        .collect();
    let mut plan = GanttPlan::new();
    plan.replace(
        0,
        vec![GanttCalendar {
            id: GanttCalendarId(1),
            label: "Рабочий день".into(),
            working,
        }],
        tasks,
        vec![GanttDependency {
            predecessor: GanttTaskId(2),
            successor: GanttTaskId(3),
            kind: GanttDependencyKind::FinishStart,
            lag_seconds: 0,
        }],
    )
    .unwrap();
    plan
}
fn settle_gantt(ui: &mut Controls, view: &mut GanttControl) {
    for _ in 0..4 {
        ui.update(Size::new(1100.0, 850.0)).unwrap();
        view.refresh(ui).unwrap();
    }
    ui.update(Size::new(1100.0, 850.0)).unwrap();
}
#[test]
fn gantt_real_tree_timeline_rows_share_selection_and_survive_reorder() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let root = ui.root();
    let mut view = GanttControl::mount(
        &mut ui,
        root,
        plan(),
        LayoutStyle {
            width: Length::Percent(1.0),
            height: Length::Percent(1.0),
            ..Default::default()
        },
    )
    .unwrap();
    settle_gantt(&mut ui, &mut view);
    let task = GanttTaskId(3);
    view.select(&mut ui, Some(task)).unwrap();
    settle_gantt(&mut ui, &mut view);
    let widget = view.bar_widget(task).unwrap();
    let oldtree = view.tree;
    let mut replacement = view.plan().clone();
    let mut changed = replacement.task(task).unwrap().clone();
    changed.order = -10;
    changed.title = "Переставленная задача".into();
    replacement
        .put_task(replacement.revision(), changed)
        .unwrap();
    view.replace_plan(&mut ui, replacement).unwrap();
    settle_gantt(&mut ui, &mut view);
    assert_eq!(view.tree, oldtree);
    assert_eq!(view.bar_widget(task), Some(widget));
    assert_eq!(view.selected(), Some(task));
    assert_eq!(
        ui.tree_selected_keys(view.tree).unwrap(),
        vec![rust_desktop_ui_treegrid::RowKey(3)]
    );
    for bar in view.geometry() {
        let row = ui
            .tree_frame(view.tree)
            .unwrap()
            .semantics
            .rows
            .iter()
            .find(|r| r.key.0 == bar.task.0)
            .unwrap();
        let b = ui
            .runtime
            .bounds(view.bar_widget(bar.task).unwrap())
            .unwrap();
        assert!((b.y - row.bounds.y - 3.0).abs() < 0.1);
    }
    view.begin_edit(&mut ui).unwrap();
    ui.set_text(view.duration_editor, "-1").unwrap();
    let revision = view.plan().revision();
    assert!(view.apply(&mut ui).is_err());
    assert_eq!(view.plan().revision(), revision);
    view.cancel(&mut ui).unwrap();
    assert!(view.draft().is_none());
}
#[test]
fn gantt_scroll_anchor_and_selection_survive_source_update() {
    let mut ui = Controls::new(Theme::FluentDark).unwrap();
    let root = ui.root();
    let mut view = GanttControl::mount(
        &mut ui,
        root,
        plan(),
        LayoutStyle {
            width: Length::Percent(1.0),
            height: Length::Percent(1.0),
            ..Default::default()
        },
    )
    .unwrap();
    settle_gantt(&mut ui, &mut view);
    ui.tree_scroll_to(view.tree, 0.0, 96.0).unwrap();
    settle_gantt(&mut ui, &mut view);
    let top = view.scroll_anchor(&ui).unwrap();
    let mut next = view.plan().clone();
    let mut t = next.task(GanttTaskId(16)).unwrap().clone();
    t.order = -100;
    next.put_task(next.revision(), t).unwrap();
    view.replace_plan(&mut ui, next).unwrap();
    settle_gantt(&mut ui, &mut view);
    let after = view.scroll_anchor(&ui).unwrap();
    assert_eq!(after.key, top.key);
    assert_eq!(after.within_row, top.within_row);
}
