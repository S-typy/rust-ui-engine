//! Finite, UI-independent scheduling example with an application-supplied zone.
use rust_desktop_ui_data::*;

fn local(value: &str) -> ScheduleLocal {
    value.parse().unwrap()
}
fn utc(value: &str) -> ScheduleInstant {
    let zone = ScheduleTimeZone::new(
        "Fixture/UTC",
        ScheduleUtcInterval::new(ScheduleInstant(-62135596800), ScheduleInstant(253402300799))
            .unwrap(),
        0,
        vec![],
    )
    .unwrap();
    zone.resolve(local(value), ScheduleResolvePolicy::default())
        .unwrap()
        .unwrap()
}
fn interval(start: &str, end: &str) -> ScheduleLocalInterval {
    ScheduleLocalInterval::new(local(start), local(end)).unwrap()
}
fn range(start: &str, end: &str) -> ScheduleUtcInterval {
    ScheduleUtcInterval::new(utc(start), utc(end)).unwrap()
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    // These are synthetic transitions for this example, not a timezone database.
    let zone = ScheduleTimeZone::new(
        "Fixture/Office",
        range("2026-01-01T00:00:00", "2027-01-01T00:00:00"),
        3600,
        vec![
            ScheduleTransition {
                at: utc("2026-03-29T01:00:00"),
                offset_after: 7200,
            },
            ScheduleTransition {
                at: utc("2026-10-25T01:00:00"),
                offset_after: 3600,
            },
        ],
    )?;
    let series = ScheduleSeries {
        id: ScheduleSeriesId(7),
        title: "Planning".into(),
        zone: zone.id().into(),
        interval: interval("2026-03-28T02:30:00", "2026-03-28T03:30:00"),
        resources: vec![ScheduleResourceId(1)],
        recurrence: ScheduleRecurrence {
            frequency: ScheduleFrequency::Daily,
            interval: 1,
            end: ScheduleTermination::Count(3),
            invalid_date: ScheduleInvalidDate::Reject,
        },
        policy: ScheduleResolvePolicy {
            gap: ScheduleGapPolicy::Skip,
            fold: ScheduleFoldPolicy::Later,
        },
    };
    let mut model = SchedulingModel::new();
    model.apply(
        0,
        vec![
            ScheduleChange::PutZone(zone),
            ScheduleChange::PutResource(ScheduleResource {
                id: ScheduleResourceId(1),
                label: "Office".into(),
            }),
            ScheduleChange::PutSeries(series),
        ],
    )?;
    let march = model.query(
        range("2026-03-27T00:00:00", "2026-04-01T00:00:00"),
        &[],
        ScheduleQueryLimits::default(),
    )?;
    assert_eq!(march.len(), 2); // Count consumes the skipped DST candidate too.
    let moved = march[0].id;
    model.apply(
        1,
        vec![ScheduleChange::PutException {
            id: moved,
            exception: ScheduleException::Replace {
                interval: interval("2026-04-02T10:00:00", "2026-04-02T11:00:00"),
                title: Some("Rescheduled planning".into()),
                resources: None,
            },
        }],
    )?;
    let april = model.query(
        range("2026-04-01T00:00:00", "2026-04-03T00:00:00"),
        &[ScheduleResourceId(1)],
        ScheduleQueryLimits::default(),
    )?;
    assert_eq!(april.len(), 1);
    assert_eq!(april[0].id, moved);
    let before = model.clone();
    assert!(
        model
            .apply(
                2,
                vec![ScheduleChange::RemoveResource(ScheduleResourceId(1))]
            )
            .is_err()
    );
    assert_eq!(model, before);
    assert_eq!(model.changes_since(1).batches.len(), 1);
    for item in april {
        println!(
            "series={} original={} display={} utc=[{}, {}) title={}",
            item.id.series.0,
            item.id.original,
            item.local.start,
            item.utc.start.0,
            item.utc.end.0,
            item.title
        );
    }
    println!(
        "scheduling-cli PASS: DST skip/count, moved identity/query, resource rollback, revision batch"
    );
    Ok(())
}
