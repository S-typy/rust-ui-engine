use super::*;

fn local(value: &str) -> ScheduleLocal {
    value.parse().unwrap()
}
fn utc(value: &str) -> ScheduleInstant {
    ScheduleInstant(local(value).seconds())
}
fn range(start: &str, end: &str) -> ScheduleUtcInterval {
    ScheduleUtcInterval::new(utc(start), utc(end)).unwrap()
}
fn interval(start: &str, end: &str) -> ScheduleLocalInterval {
    ScheduleLocalInterval::new(local(start), local(end)).unwrap()
}
fn synthetic() -> ScheduleTimeZone {
    ScheduleTimeZone::new(
        "Fixture/Seasonal",
        range("2024-01-01T00:00:00", "2030-01-01T00:00:00"),
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
    )
    .unwrap()
}
fn fixed() -> ScheduleTimeZone {
    ScheduleTimeZone::new(
        "Fixture/UTC",
        range("0001-01-01T00:00:00", "9999-12-31T23:59:59"),
        0,
        vec![],
    )
    .unwrap()
}
fn daily() -> ScheduleSeries {
    ScheduleSeries {
        id: ScheduleSeriesId(1),
        title: "Daily meeting".into(),
        zone: "Fixture/Seasonal".into(),
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
            fold: ScheduleFoldPolicy::Earlier,
        },
    }
}
fn fixture(series: ScheduleSeries) -> SchedulingModel {
    let mut model = SchedulingModel::new();
    model
        .apply(
            0,
            vec![
                ScheduleChange::PutZone(synthetic()),
                ScheduleChange::PutZone(fixed()),
                ScheduleChange::PutResource(ScheduleResource {
                    id: ScheduleResourceId(1),
                    label: "Room one".into(),
                }),
                ScheduleChange::PutSeries(series),
            ],
        )
        .unwrap();
    model
}

#[test]
fn scheduling_local_calendar_roundtrip_and_checked_precision() {
    for text in [
        "0001-01-01T00:00:00",
        "1600-02-29T23:59:59",
        "1900-03-01T00:00:00",
        "1970-01-01T00:00:00",
        "2000-02-29T12:34:56",
        "9999-12-31T23:59:59",
    ] {
        let value = local(text);
        assert_eq!(ScheduleLocal::from_seconds(value.seconds()).unwrap(), value);
        assert_eq!(value.to_string(), text);
    }
    assert_eq!(utc("1970-01-01T00:00:00"), ScheduleInstant(0));
    for value in [
        "2026-02-29T12:00:00",
        "2026-01-01T12:00",
        "2026-01-01T12:00:00.1",
        "2026-01-01T23:59:60",
    ] {
        assert!(value.parse::<ScheduleLocal>().is_err());
    }
    assert!(ScheduleInstant(i64::MAX).checked_add(1).is_err());
    assert!(local("9999-12-31T23:59:59").checked_add(1).is_err());
}
#[test]
fn scheduling_dst_gap_fold_and_finite_coverage_are_explicit() {
    let zone = synthetic();
    let reject = ScheduleResolvePolicy::default();
    let gap = local("2026-03-29T02:30:00");
    assert_eq!(zone.resolve(gap, reject), Err(ScheduleError::Gap(gap)));
    assert_eq!(
        zone.resolve(
            gap,
            ScheduleResolvePolicy {
                gap: ScheduleGapPolicy::Skip,
                ..reject
            }
        )
        .unwrap(),
        None
    );
    let fold = local("2026-10-25T02:30:00");
    assert_eq!(zone.resolve(fold, reject), Err(ScheduleError::Fold(fold)));
    assert_eq!(
        zone.resolve(
            fold,
            ScheduleResolvePolicy {
                fold: ScheduleFoldPolicy::Earlier,
                ..reject
            }
        )
        .unwrap(),
        Some(utc("2026-10-25T00:30:00"))
    );
    assert_eq!(
        zone.resolve(
            fold,
            ScheduleResolvePolicy {
                fold: ScheduleFoldPolicy::Later,
                ..reject
            }
        )
        .unwrap(),
        Some(utc("2026-10-25T01:30:00"))
    );
    assert!(matches!(
        zone.resolve(local("2030-01-01T01:00:00"), reject),
        Err(ScheduleError::Coverage(_))
    ));
    assert!(zone.to_local(zone.coverage().end).is_err());
    let ending = interval("2030-01-01T00:00:00", "2030-01-01T01:00:00");
    assert_eq!(
        zone.resolve_interval(ending, reject).unwrap().unwrap().end,
        zone.coverage().end
    );
    assert!(
        zone.resolve_interval(
            interval("2026-03-29T02:30:00", "2031-01-01T00:00:00"),
            ScheduleResolvePolicy {
                gap: ScheduleGapPolicy::Skip,
                ..reject
            }
        )
        .is_err()
    );
}
#[test]
fn scheduling_count_counts_calendar_candidates_even_when_dst_skips() {
    let model = fixture(daily());
    let values = model
        .query(
            range("2026-03-27T00:00:00", "2026-04-01T00:00:00"),
            &[],
            ScheduleQueryLimits::default(),
        )
        .unwrap();
    assert_eq!(values.len(), 2);
    assert_eq!(values[0].id.original, local("2026-03-28T02:30:00"));
    assert_eq!(values[1].id.original, local("2026-03-30T02:30:00"));
    assert_eq!(values[0].utc.start, utc("2026-03-28T01:30:00"));
    assert_eq!(values[1].utc.start, utc("2026-03-30T00:30:00"));
}

fn calendar_series(
    start: &str,
    frequency: ScheduleFrequency,
    end: ScheduleTermination,
    policy: ScheduleInvalidDate,
) -> ScheduleSeries {
    let start = local(start);
    ScheduleSeries {
        id: ScheduleSeriesId(1),
        title: "Calendar".into(),
        zone: "Fixture/UTC".into(),
        interval: ScheduleLocalInterval::new(start, start.checked_add(3600).unwrap()).unwrap(),
        resources: vec![ScheduleResourceId(1)],
        recurrence: ScheduleRecurrence {
            frequency,
            interval: 1,
            end,
            invalid_date: policy,
        },
        policy: ScheduleResolvePolicy::default(),
    }
}
#[test]
fn scheduling_month_end_and_leap_year_policies_have_stable_candidates() {
    for (policy, expected) in [
        (
            ScheduleInvalidDate::Skip,
            vec![
                "2024-01-31T10:00:00",
                "2024-03-31T10:00:00",
                "2024-05-31T10:00:00",
            ],
        ),
        (
            ScheduleInvalidDate::LastDay,
            vec![
                "2024-01-31T10:00:00",
                "2024-02-29T10:00:00",
                "2024-03-31T10:00:00",
            ],
        ),
    ] {
        let model = fixture(calendar_series(
            "2024-01-31T10:00:00",
            ScheduleFrequency::Monthly,
            ScheduleTermination::Count(3),
            policy,
        ));
        let rows = model
            .query(
                range("2024-01-01T00:00:00", "2024-06-01T00:00:00"),
                &[],
                ScheduleQueryLimits::default(),
            )
            .unwrap();
        assert_eq!(
            rows.iter()
                .map(|o| o.id.original.to_string())
                .collect::<Vec<_>>(),
            expected
        );
    }
    let model = fixture(calendar_series(
        "2024-01-31T10:00:00",
        ScheduleFrequency::Monthly,
        ScheduleTermination::Count(3),
        ScheduleInvalidDate::Reject,
    ));
    assert!(
        model
            .query(
                range("2024-01-01T00:00:00", "2024-06-01T00:00:00"),
                &[],
                ScheduleQueryLimits::default()
            )
            .is_err()
    );
    let model = fixture(calendar_series(
        "2024-02-29T10:00:00",
        ScheduleFrequency::Yearly,
        ScheduleTermination::Count(2),
        ScheduleInvalidDate::Skip,
    ));
    let rows = model
        .query(
            range("2024-01-01T00:00:00", "2029-01-01T00:00:00"),
            &[],
            ScheduleQueryLimits::default(),
        )
        .unwrap();
    assert_eq!(
        rows.iter()
            .map(|o| o.id.original.date().year())
            .collect::<Vec<_>>(),
        [2024, 2028]
    );
}
#[test]
fn scheduling_weekly_interval_until_and_half_open_query() {
    let mut series = calendar_series(
        "2026-01-06T10:00:00",
        ScheduleFrequency::Weekly(vec![ScheduleWeekday::Monday, ScheduleWeekday::Wednesday]),
        ScheduleTermination::Until(local("2026-02-02T10:00:00")),
        ScheduleInvalidDate::Reject,
    );
    series.recurrence.interval = 2;
    let model = fixture(series);
    let rows = model
        .query(
            range("2026-01-01T00:00:00", "2026-02-10T00:00:00"),
            &[],
            ScheduleQueryLimits::default(),
        )
        .unwrap();
    assert_eq!(
        rows.iter()
            .map(|o| o.id.original.to_string())
            .collect::<Vec<_>>(),
        [
            "2026-01-07T10:00:00",
            "2026-01-19T10:00:00",
            "2026-01-21T10:00:00",
            "2026-02-02T10:00:00"
        ]
    );
    assert!(
        model
            .query(
                range("2026-01-07T11:00:00", "2026-01-19T10:00:00"),
                &[],
                ScheduleQueryLimits::default()
            )
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        model
            .query(
                range("2026-01-07T10:59:59", "2026-01-07T11:00:00"),
                &[],
                ScheduleQueryLimits::default()
            )
            .unwrap()
            .len(),
        1
    );
}
#[test]
fn scheduling_moved_and_cancelled_exceptions_preserve_original_identity_and_resources() {
    let series = calendar_series(
        "2026-01-01T10:00:00",
        ScheduleFrequency::Daily,
        ScheduleTermination::Count(5),
        ScheduleInvalidDate::Reject,
    );
    let mut model = fixture(series);
    let moved = ScheduleOccurrenceId {
        series: ScheduleSeriesId(1),
        original: local("2026-01-01T10:00:00"),
    };
    let cancelled = ScheduleOccurrenceId {
        series: ScheduleSeriesId(1),
        original: local("2026-01-02T10:00:00"),
    };
    model
        .apply(
            1,
            vec![
                ScheduleChange::PutResource(ScheduleResource {
                    id: ScheduleResourceId(2),
                    label: "Room two".into(),
                }),
                ScheduleChange::PutException {
                    id: moved,
                    exception: ScheduleException::Replace {
                        interval: interval("2026-02-01T10:00:00", "2026-02-01T12:00:00"),
                        title: Some("Moved".into()),
                        resources: Some(vec![ScheduleResourceId(2)]),
                    },
                },
                ScheduleChange::PutException {
                    id: cancelled,
                    exception: ScheduleException::Cancel,
                },
            ],
        )
        .unwrap();
    let rows = model
        .query(
            range("2026-02-01T00:00:00", "2026-02-02T00:00:00"),
            &[ScheduleResourceId(2)],
            ScheduleQueryLimits::default(),
        )
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, moved);
    assert_eq!(rows[0].title, "Moved");
    assert!(rows[0].is_exception);
    assert_eq!(
        model
            .query(
                range("2026-01-01T00:00:00", "2026-01-10T00:00:00"),
                &[],
                ScheduleQueryLimits::default()
            )
            .unwrap()
            .len(),
        3
    );
    assert!(
        model
            .query(
                range("2026-02-01T00:00:00", "2026-02-02T00:00:00"),
                &[ScheduleResourceId(1)],
                ScheduleQueryLimits::default()
            )
            .unwrap()
            .is_empty()
    );
    model
        .apply(2, vec![ScheduleChange::RemoveException(moved)])
        .unwrap();
    let rows = model
        .query(
            range("2026-01-01T00:00:00", "2026-01-02T00:00:00"),
            &[],
            ScheduleQueryLimits::default(),
        )
        .unwrap();
    assert_eq!(rows[0].id, moved);
    assert!(!rows[0].is_exception);
}
#[test]
fn scheduling_transaction_resource_series_and_zone_failures_preserve_entire_model() {
    let mut model = fixture(daily());
    let original = model.clone();
    assert!(
        model
            .apply(
                1,
                vec![ScheduleChange::RemoveResource(ScheduleResourceId(1))]
            )
            .is_err()
    );
    assert_eq!(model, original);
    assert!(
        model
            .apply(
                1,
                vec![ScheduleChange::RemoveZone("Fixture/Seasonal".into())]
            )
            .is_err()
    );
    assert_eq!(model, original);
    let mut series = daily();
    series.resources = vec![ScheduleResourceId(2)];
    model
        .apply(
            1,
            vec![
                ScheduleChange::RemoveResource(ScheduleResourceId(1)),
                ScheduleChange::PutSeries(series),
                ScheduleChange::PutResource(ScheduleResource {
                    id: ScheduleResourceId(2),
                    label: "New room".into(),
                }),
            ],
        )
        .unwrap();
    assert_eq!(model.revision(), 2);
    assert_eq!(model.changes_since(1).batches.len(), 1);
    let before = model.clone();
    assert!(matches!(
        model.apply(1, vec![]),
        Err(ScheduleError::Conflict { .. })
    ));
    assert_eq!(model, before);
    assert!(
        model
            .apply(
                2,
                vec![
                    ScheduleChange::PutResource(ScheduleResource {
                        id: ScheduleResourceId(3),
                        label: "Good".into()
                    }),
                    ScheduleChange::PutResource(ScheduleResource {
                        id: ScheduleResourceId(4),
                        label: String::new()
                    })
                ]
            )
            .is_err()
    );
    assert_eq!(model, before);
}
#[test]
fn scheduling_series_edit_cannot_strand_exception_and_removal_is_explicit() {
    let mut model = fixture(daily());
    let id = ScheduleOccurrenceId {
        series: ScheduleSeriesId(1),
        original: local("2026-03-30T02:30:00"),
    };
    model
        .apply(
            1,
            vec![ScheduleChange::PutException {
                id,
                exception: ScheduleException::Cancel,
            }],
        )
        .unwrap();
    let before = model.clone();
    let mut series = daily();
    series.recurrence.end = ScheduleTermination::Count(2);
    assert!(
        model
            .apply(2, vec![ScheduleChange::PutSeries(series.clone())])
            .is_err()
    );
    assert_eq!(model, before);
    model
        .apply(
            2,
            vec![
                ScheduleChange::PutSeries(series),
                ScheduleChange::RemoveException(id),
            ],
        )
        .unwrap();
    model
        .apply(
            3,
            vec![
                ScheduleChange::RemoveSeries(ScheduleSeriesId(1)),
                ScheduleChange::RemoveResource(ScheduleResourceId(1)),
                ScheduleChange::RemoveZone("Fixture/Seasonal".into()),
            ],
        )
        .unwrap();
    assert!(model.series_iter().next().is_none());
}
#[test]
fn scheduling_query_budgets_and_coverage_never_return_partial_results() {
    let model = fixture(daily());
    let r = range("2026-03-27T00:00:00", "2026-04-01T00:00:00");
    for limits in [
        ScheduleQueryLimits {
            max_candidates: 1,
            ..Default::default()
        },
        ScheduleQueryLimits {
            max_occurrences: 1,
            ..Default::default()
        },
        ScheduleQueryLimits {
            max_zone_checks: 1,
            ..Default::default()
        },
    ] {
        assert!(matches!(
            model.query(r, &[], limits),
            Err(ScheduleError::Capacity(_))
        ));
    }
    assert!(matches!(
        model.query(
            range("2029-12-31T00:00:00", "2030-01-02T00:00:00"),
            &[],
            ScheduleQueryLimits::default()
        ),
        Err(ScheduleError::Coverage(_))
    ));
    assert_eq!(model.revision(), 1);
    assert_eq!(model.changes_since(0).batches.len(), 1);
    let mut series = daily();
    series.recurrence = ScheduleRecurrence::default();
    series.interval = interval("2029-12-31T23:00:00", "2030-01-01T01:00:00");
    let model = fixture(series);
    assert_eq!(
        model
            .query(
                range("2029-12-31T00:00:00", "2030-01-01T00:00:00"),
                &[],
                ScheduleQueryLimits::default()
            )
            .unwrap()
            .len(),
        1
    );
}
#[test]
fn scheduling_transition_validation_and_wall_duration_across_dst() {
    let coverage = range("2026-01-01T00:00:00", "2027-01-01T00:00:00");
    assert!(ScheduleTimeZone::new("Bad", coverage, 86400, vec![]).is_err());
    assert!(
        ScheduleTimeZone::new(
            "Bad",
            coverage,
            0,
            vec![ScheduleTransition {
                at: coverage.start,
                offset_after: 3600
            }]
        )
        .is_err()
    );
    assert!(
        ScheduleTimeZone::new(
            "Bad",
            coverage,
            0,
            vec![ScheduleTransition {
                at: utc("2026-05-01T00:00:00"),
                offset_after: 0
            }]
        )
        .is_err()
    );
    let z = synthetic();
    let p = ScheduleResolvePolicy {
        fold: ScheduleFoldPolicy::Earlier,
        ..Default::default()
    };
    let spring = z
        .resolve_interval(interval("2026-03-29T00:00:00", "2026-03-30T00:00:00"), p)
        .unwrap()
        .unwrap();
    assert_eq!(spring.end.0 - spring.start.0, 23 * 3600);
    let fall = z
        .resolve_interval(interval("2026-10-25T00:00:00", "2026-10-26T00:00:00"), p)
        .unwrap()
        .unwrap();
    assert_eq!(fall.end.0 - fall.start.0, 25 * 3600);
}

#[test]
fn scheduling_table_overflow_multiple_folds_and_reversed_intervals_reject() {
    assert!(
        ScheduleTimeZone::new(
            "Overflow",
            ScheduleUtcInterval::new(ScheduleInstant(i64::MAX - 10), ScheduleInstant(i64::MAX))
                .unwrap(),
            1,
            vec![]
        )
        .is_err()
    );
    assert!(
        ScheduleTimeZone::new(
            "Triple",
            ScheduleUtcInterval::new(ScheduleInstant(0), ScheduleInstant(300)).unwrap(),
            0,
            vec![
                ScheduleTransition {
                    at: ScheduleInstant(100),
                    offset_after: -100
                },
                ScheduleTransition {
                    at: ScheduleInstant(200),
                    offset_after: -200
                }
            ]
        )
        .is_err()
    );
    let zone = fixed();
    let local = ScheduleLocalInterval {
        start: local("2026-01-02T00:00:00"),
        end: local("2026-01-01T00:00:00"),
    };
    assert!(
        zone.resolve_interval(local, ScheduleResolvePolicy::default())
            .is_err()
    );
    assert!(ScheduleUtcInterval::new(ScheduleInstant(2), ScheduleInstant(1)).is_err());
    assert!(ScheduleLocalInterval::new(local.start, local.start).is_err());
}
#[test]
fn scheduling_atomic_duplicate_unknown_exception_and_gap_replacement() {
    let mut model = fixture(daily());
    let before = model.clone();
    let invalid_id = ScheduleOccurrenceId {
        series: ScheduleSeriesId(1),
        original: local("2026-03-31T02:30:00"),
    };
    assert!(
        model
            .apply(
                1,
                vec![ScheduleChange::PutException {
                    id: invalid_id,
                    exception: ScheduleException::Cancel
                }]
            )
            .is_err()
    );
    assert_eq!(model, before);
    let id = ScheduleOccurrenceId {
        series: ScheduleSeriesId(1),
        original: local("2026-03-28T02:30:00"),
    };
    assert!(
        model
            .apply(
                1,
                vec![ScheduleChange::PutException {
                    id,
                    exception: ScheduleException::Replace {
                        interval: interval("2026-03-29T02:15:00", "2026-03-29T03:15:00"),
                        title: None,
                        resources: None
                    }
                }]
            )
            .is_err()
    );
    assert_eq!(model, before);
    assert!(
        model
            .apply(
                1,
                vec![
                    ScheduleChange::PutSeries(daily()),
                    ScheduleChange::PutSeries(daily())
                ]
            )
            .is_err()
    );
    assert_eq!(model, before);
    assert_eq!(
        model
            .apply(1, vec![ScheduleChange::PutSeries(daily())])
            .unwrap(),
        1
    );
    assert_eq!(model, before);
}
#[test]
fn scheduling_journal_reset_and_metadata_updates_keep_generated_ids() {
    let mut model = fixture(daily());
    let query = range("2026-03-27T00:00:00", "2026-04-01T00:00:00");
    let before = model
        .query(query, &[], ScheduleQueryLimits::default())
        .unwrap();
    for n in 0..65 {
        let mut series = daily();
        series.title = format!("Revision {n}");
        series.interval.end = series.interval.end.checked_add(60).unwrap();
        model
            .apply(model.revision(), vec![ScheduleChange::PutSeries(series)])
            .unwrap();
    }
    let after = model
        .query(query, &[], ScheduleQueryLimits::default())
        .unwrap();
    assert_eq!(
        before.iter().map(|o| o.id).collect::<Vec<_>>(),
        after.iter().map(|o| o.id).collect::<Vec<_>>()
    );
    assert!(model.changes_since(0).reset_required);
    assert!(model.changes_since(model.revision() + 1).reset_required);
    let delta = model.changes_since(model.revision() - 1);
    assert!(!delta.reset_required);
    assert_eq!(delta.batches.len(), 1);
    assert_eq!(delta.batches[0].series, [ScheduleSeriesId(1)]);
}
#[test]
fn scheduling_fall_fold_daily_identity_is_independent_of_resolution_policy() {
    let mut series = daily();
    series.interval = interval("2026-10-24T02:15:00", "2026-10-24T02:45:00");
    series.policy.fold = ScheduleFoldPolicy::Earlier;
    let mut model = fixture(series.clone());
    let query = range("2026-10-24T00:00:00", "2026-10-27T00:00:00");
    let early = model
        .query(query, &[], ScheduleQueryLimits::default())
        .unwrap();
    series.policy.fold = ScheduleFoldPolicy::Later;
    model
        .apply(1, vec![ScheduleChange::PutSeries(series)])
        .unwrap();
    let late = model
        .query(query, &[], ScheduleQueryLimits::default())
        .unwrap();
    assert_eq!(early[1].id, late[1].id);
    assert_eq!(late[1].utc.start.0 - early[1].utc.start.0, 3600);
    assert_eq!(early[1].utc.end.0 - early[1].utc.start.0, 1800);
    assert_eq!(late[1].utc.end.0 - late[1].utc.start.0, 1800);
}

#[test]
fn scheduling_range_culling_does_not_reject_future_calendar_gap_or_coverage() {
    let model = fixture(calendar_series(
        "2026-01-31T10:00:00",
        ScheduleFrequency::Monthly,
        ScheduleTermination::Never,
        ScheduleInvalidDate::Reject,
    ));
    assert_eq!(
        model
            .query(
                range("2026-01-31T00:00:00", "2026-02-01T00:00:00"),
                &[],
                ScheduleQueryLimits::default()
            )
            .unwrap()
            .len(),
        1
    );
    let zone = ScheduleTimeZone::new(
        "Short",
        range("2026-01-01T00:00:00", "2026-01-10T00:00:00"),
        0,
        vec![],
    )
    .unwrap();
    let mut model = SchedulingModel::new();
    let mut series = calendar_series(
        "2026-01-01T23:30:00",
        ScheduleFrequency::Daily,
        ScheduleTermination::Never,
        ScheduleInvalidDate::Reject,
    );
    series.zone = "Short".into();
    series.resources.clear();
    model
        .apply(
            0,
            vec![
                ScheduleChange::PutZone(zone),
                ScheduleChange::PutSeries(series),
            ],
        )
        .unwrap();
    assert_eq!(
        model
            .query(
                range("2026-01-09T00:00:00", "2026-01-09T10:00:00"),
                &[],
                ScheduleQueryLimits::default()
            )
            .unwrap()
            .len(),
        1
    );
    assert!(matches!(
        model.query(
            range("2026-01-09T23:00:00", "2026-01-10T00:00:00"),
            &[],
            ScheduleQueryLimits::default()
        ),
        Err(ScheduleError::Coverage(_))
    ));
    let mut series = daily();
    series.policy.gap = ScheduleGapPolicy::Reject;
    let model = fixture(series);
    assert!(
        model
            .query(
                range("2026-03-29T00:00:00", "2026-03-29T00:45:00"),
                &[],
                ScheduleQueryLimits::default()
            )
            .unwrap()
            .is_empty()
    );
    assert!(matches!(
        model.query(
            range("2026-03-29T01:00:00", "2026-03-29T02:00:00"),
            &[],
            ScheduleQueryLimits::default()
        ),
        Err(ScheduleError::Gap(_))
    ));
}

#[test]
fn scheduling_query_civil_boundary_and_future_duration_are_checked_in_order() {
    let mut series = calendar_series(
        "9999-12-30T23:30:00",
        ScheduleFrequency::Daily,
        ScheduleTermination::Never,
        ScheduleInvalidDate::Reject,
    );
    series.interval.end = local("9999-12-31T00:30:00");
    let model = fixture(series);
    assert_eq!(
        model
            .query(
                range("9999-12-31T00:00:00", "9999-12-31T10:00:00"),
                &[],
                ScheduleQueryLimits::default(),
            )
            .unwrap()
            .len(),
        1
    );

    let last = utc("9999-12-31T23:59:59").0;
    let mut model = SchedulingModel::new();
    let mut series = calendar_series(
        "9999-12-30T10:00:00",
        ScheduleFrequency::Once,
        ScheduleTermination::Never,
        ScheduleInvalidDate::Reject,
    );
    series.zone = "Wide".into();
    series.resources.clear();
    model
        .apply(
            0,
            vec![
                ScheduleChange::PutZone(
                    ScheduleTimeZone::new(
                        "Wide",
                        ScheduleUtcInterval::new(
                            ScheduleInstant(-62135596801),
                            ScheduleInstant(last + 86400),
                        )
                        .unwrap(),
                        0,
                        vec![],
                    )
                    .unwrap(),
                ),
                ScheduleChange::PutSeries(series),
            ],
        )
        .unwrap();
    assert!(
        model
            .query(
                ScheduleUtcInterval::new(ScheduleInstant(last), ScheduleInstant(last + 1)).unwrap(),
                &[],
                ScheduleQueryLimits::default()
            )
            .unwrap()
            .is_empty()
    );
    assert!(matches!(
        model.query(
            ScheduleUtcInterval::new(ScheduleInstant(last + 1), ScheduleInstant(last + 2)).unwrap(),
            &[],
            ScheduleQueryLimits::default()
        ),
        Err(ScheduleError::Invalid(_))
    ));
    assert!(matches!(
        model.query(
            ScheduleUtcInterval::new(ScheduleInstant(-62135596801), ScheduleInstant(-62135596800))
                .unwrap(),
            &[],
            ScheduleQueryLimits::default()
        ),
        Err(ScheduleError::Invalid(_))
    ));
}
