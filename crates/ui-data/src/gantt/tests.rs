use super::*;
fn instant(value: i64) -> ScheduleInstant {
    ScheduleInstant(value)
}
fn calendar() -> GanttCalendar {
    GanttCalendar {
        id: GanttCalendarId(1),
        label: "Work".into(),
        working: vec![
            ScheduleUtcInterval::new(instant(0), instant(8)).unwrap(),
            ScheduleUtcInterval::new(instant(24), instant(32)).unwrap(),
            ScheduleUtcInterval::new(instant(48), instant(80)).unwrap(),
        ],
    }
}
fn task(id: u64, duration: i64) -> GanttTask {
    GanttTask {
        id: GanttTaskId(id),
        parent: None,
        order: id as i64,
        title: format!("Task {id}"),
        calendar: GanttCalendarId(1),
        work_seconds: duration,
        start_not_before: instant(0),
        finish_not_after: None,
        fixed_start: None,
        progress_percent: 0,
    }
}
fn dependency(a: u64, b: u64) -> GanttDependency {
    GanttDependency {
        predecessor: GanttTaskId(a),
        successor: GanttTaskId(b),
        kind: GanttDependencyKind::FinishStart,
        lag_seconds: 0,
    }
}
#[test]
fn calendars_skip_nonworking_time_and_critical_path_has_real_slack() {
    let mut plan = GanttPlan::new();
    plan.replace(
        0,
        vec![calendar()],
        vec![task(1, 6), task(2, 6), task(3, 2)],
        vec![dependency(1, 2)],
    )
    .unwrap();
    assert_eq!(plan.placement(GanttTaskId(1)).unwrap().end, instant(6));
    assert_eq!(plan.placement(GanttTaskId(2)).unwrap().end, instant(28));
    assert!(plan.placement(GanttTaskId(1)).unwrap().critical);
    assert!(plan.placement(GanttTaskId(2)).unwrap().critical);
    let free = plan.placement(GanttTaskId(3)).unwrap();
    assert_eq!(free.latest_start, instant(26));
    assert_eq!(free.slack_work_seconds, 10);
    assert!(!free.critical);
}
#[test]
fn graph_cycles_deadlines_and_missing_references_are_atomic() {
    let mut plan = GanttPlan::new();
    plan.replace(
        0,
        vec![calendar()],
        vec![task(1, 6), task(2, 6)],
        vec![dependency(1, 2)],
    )
    .unwrap();
    let original = plan.clone();
    assert!(
        plan.replace(
            1,
            vec![calendar()],
            vec![task(1, 6), task(2, 6)],
            vec![dependency(1, 2), dependency(2, 1)]
        )
        .is_err()
    );
    assert_eq!(plan, original);
    let mut impossible = task(2, 6);
    impossible.finish_not_after = Some(instant(8));
    assert!(plan.put_task(1, impossible).is_err());
    assert_eq!(plan, original);
    let mut unknown = task(2, 6);
    unknown.parent = Some(GanttTaskId(9));
    assert!(plan.put_task(1, unknown).is_err());
    assert_eq!(plan, original);
    let mut cycle = task(1, 6);
    cycle.parent = Some(GanttTaskId(1));
    assert!(plan.put_task(1, cycle).is_err());
    assert_eq!(plan, original);
    assert!(plan.put_task(0, task(1, 2)).is_err());
    assert_eq!(plan, original);
}
#[test]
fn hierarchy_order_and_progress_do_not_change_identity_or_dependency_plan() {
    let mut a = task(1, 2);
    a.order = 20;
    let mut b = task(2, 2);
    b.order = 10;
    let mut c = task(3, 2);
    c.parent = Some(a.id);
    let mut plan = GanttPlan::new();
    plan.replace(0, vec![calendar()], vec![a.clone(), b, c], vec![])
        .unwrap();
    assert_eq!(
        plan.tree_order(),
        &[
            (GanttTaskId(2), 0),
            (GanttTaskId(1), 0),
            (GanttTaskId(3), 1)
        ]
    );
    a.order = 0;
    a.progress_percent = 80;
    plan.put_task(1, a).unwrap();
    assert_eq!(
        plan.tree_order(),
        &[
            (GanttTaskId(1), 0),
            (GanttTaskId(3), 1),
            (GanttTaskId(2), 0)
        ]
    );
}
#[test]
fn all_four_dependency_kinds_and_signed_lags_constrain_correct_boundaries() {
    for kind in [
        GanttDependencyKind::FinishStart,
        GanttDependencyKind::StartStart,
        GanttDependencyKind::FinishFinish,
        GanttDependencyKind::StartFinish,
    ] {
        let mut a = task(1, 4);
        a.start_not_before = instant(24);
        let mut b = task(2, 2);
        b.start_not_before = instant(0);
        let mut dep = dependency(1, 2);
        dep.kind = kind;
        dep.lag_seconds = 1;
        let mut plan = GanttPlan::new();
        plan.replace(0, vec![calendar()], vec![a, b], vec![dep])
            .unwrap();
        let a = plan.placement(GanttTaskId(1)).unwrap();
        let b = plan.placement(GanttTaskId(2)).unwrap();
        match kind {
            GanttDependencyKind::FinishStart => assert!(b.start.0 > a.end.0),
            GanttDependencyKind::StartStart => assert!(b.start.0 > a.start.0),
            GanttDependencyKind::FinishFinish => assert!(b.end.0 > a.end.0),
            GanttDependencyKind::StartFinish => assert!(b.end.0 > a.start.0),
        }
    }
}
#[test]
fn finish_constraint_at_calendar_endpoint_is_allowed() {
    let mut a = task(1, 8);
    a.start_not_before = instant(72);
    let mut dep = dependency(1, 2);
    dep.kind = GanttDependencyKind::FinishFinish;
    let mut plan = GanttPlan::new();
    plan.replace(0, vec![calendar()], vec![a, task(2, 4)], vec![dep])
        .unwrap();
    assert_eq!(plan.placement(GanttTaskId(2)).unwrap().end, instant(80));
}
#[test]
fn fixed_start_and_overflow_are_validated_before_publication() {
    let mut fixed = task(1, 2);
    fixed.fixed_start = Some(instant(10));
    let mut plan = GanttPlan::new();
    assert!(
        plan.replace(0, vec![calendar()], vec![fixed], vec![])
            .is_err()
    );
    assert_eq!(plan.revision(), 0);
    let mut dep = dependency(1, 2);
    dep.lag_seconds = i64::MAX;
    assert!(
        plan.replace(0, vec![calendar()], vec![task(1, 2), task(2, 2)], vec![dep])
            .is_err()
    );
    assert_eq!(plan.revision(), 0);
}
