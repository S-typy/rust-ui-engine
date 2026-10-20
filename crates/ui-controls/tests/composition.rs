use rust_desktop_ui_controls::{
    Controls, Theme, dashboard::*, media::*, report_designer::*, reports::*,
};
use rust_desktop_ui_core::*;
use rust_desktop_ui_data::*;
use std::{
    collections::BTreeMap,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};
fn table() -> DataTable {
    DataTable::new(
        TableSchema::new(vec![
            FieldSchema::new("id", FieldType::Integer),
            FieldSchema::new("group", FieldType::Text),
            FieldSchema::new("amount", FieldType::Integer),
        ])
        .unwrap(),
        (1..=4)
            .map(|i| {
                DataRow::new(
                    i,
                    vec![
                        CellValue::Integer(i),
                        CellValue::Text(if i % 2 == 0 { "even" } else { "odd" }.into()),
                        CellValue::Integer(i * 10),
                    ],
                )
            })
            .collect(),
    )
    .unwrap()
}
fn report() -> ReportDesignerSession {
    let d = ReportDefinition {
        detail: ReportBand {
            height: 32.0,
            items: vec![ReportItem::Text {
                x: 0.0,
                y: 0.0,
                width: 200.0,
                font_size: 16.0,
                bold: false,
                color: Color::rgb(0, 0, 0),
                value: ReportValue::Expression(QueryExpression::field("amount")),
            }],
        },
        ..Default::default()
    };
    ReportDesignerSession::new(d, table(), BTreeMap::new()).unwrap()
}
fn dashboard() -> DashboardSession {
    let q = QueryRequest {
        aggregates: vec![Aggregate {
            function: AggregateFunction::Sum,
            expression: Some(QueryExpression::field("amount")),
        }],
        ..Default::default()
    };
    DashboardSession::new(
        DashboardDefinition {
            title: "Sales".into(),
            columns: 2,
            parameters: vec![],
            items: vec![
                DashboardItem {
                    id: "grid".into(),
                    title: "Rows".into(),
                    source: "s".into(),
                    widget: DashboardWidget::Grid,
                    placement: DashboardPlacement {
                        row: 0,
                        column: 0,
                        row_span: 1,
                        column_span: 1,
                    },
                    query: q.clone(),
                },
                DashboardItem {
                    id: "chart".into(),
                    title: "Chart".into(),
                    source: "s".into(),
                    widget: DashboardWidget::Chart {
                        x: "id".into(),
                        y: "amount".into(),
                    },
                    placement: DashboardPlacement {
                        row: 0,
                        column: 1,
                        row_span: 1,
                        column_span: 1,
                    },
                    query: q,
                },
            ],
        },
        BTreeMap::from([("s".into(), table())]),
    )
    .unwrap()
}
#[test]
fn report_edit_history_preview_and_codec_share_one_definition() {
    let mut s = report();
    let before = s.save().unwrap();
    let rev = s.revision();
    s.select(Some(ReportSelection {
        band: ReportBandKind::Detail,
        item: 0,
    }))
    .unwrap();
    s.move_selected(13.0, 1.0, Some(8.0)).unwrap();
    assert!(matches!(
        s.definition().detail.items[0],
        ReportItem::Text {
            x: 16.0,
            y: 0.0,
            ..
        }
    ));
    assert_ne!(s.revision(), rev);
    assert_eq!(s.preview().row_keys, vec![1, 2, 3, 4]);
    let json = s.save().unwrap();
    s.undo().unwrap();
    assert_eq!(s.save().unwrap(), before);
    s.redo().unwrap();
    assert_eq!(s.save().unwrap(), json);
    assert!(s.restore(rev, &before).is_err());
    s.restore(s.revision(), &before).unwrap();
    assert_eq!(s.save().unwrap(), before);
    let loaded = ReportDefinition::from_json(&json, table().schema()).unwrap();
    assert_eq!(loaded.to_json().unwrap(), json);
}
#[test]
fn report_bad_binding_unknown_item_and_invalid_geometry_are_atomic() {
    let mut s = report();
    s.select(Some(ReportSelection {
        band: ReportBandKind::Detail,
        item: 0,
    }))
    .unwrap();
    let before = s.save().unwrap();
    let rev = s.revision();
    let rows = s.preview().row_keys.clone();
    let mut d = s.definition().clone();
    if let ReportItem::Text { value, .. } = &mut d.detail.items[0] {
        *value = ReportValue::Expression(QueryExpression::field("missing"));
    }
    assert!(s.replace(rev, d).is_err());
    assert!(s.move_selected(f32::NAN, 0.0, None).is_err());
    assert!(s.move_selected(-200.0, 0.0, None).is_err());
    assert!(
        s.restore(
            rev,
            &before.replace("\"kind\": \"Text\"", "\"kind\": \"Unknown\"")
        )
        .is_err()
    );
    assert!(
        s.restore(rev, &before.replace("\"version\": 1", "\"version\": 9"))
            .is_err()
    );
    assert_eq!(s.save().unwrap(), before);
    assert_eq!(s.revision(), rev);
    assert_eq!(s.preview().row_keys, rows);
    assert!(!s.can_undo());
}
#[test]
fn report_roundtrips_images_query_parameters_and_bands() {
    let mut d = report().definition().clone();
    d.parameters = vec![FieldSchema::new("g", FieldType::Text)];
    d.parameter_filters = vec![ReportParameterFilter {
        parameter: "g".into(),
        field: "group".into(),
        operator: ComparisonOperator::Equal,
    }];
    d.query.group_by = vec![QueryExpression::field("group")];
    d.query.order_by = vec![QueryOrder {
        expression: QueryExpression::field("amount"),
        direction: SortDirection::Descending,
    }];
    d.query.aggregates = vec![Aggregate {
        function: AggregateFunction::Sum,
        expression: Some(QueryExpression::field("amount")),
    }];
    d.report_header = ReportBand {
        height: 24.0,
        items: vec![ReportItem::Image {
            bounds: Rect::new(0.0, 0.0, 20.0, 20.0),
            bitmap: Bitmap::new_rgba8(1, 1, vec![8, 9, 10, 255]).unwrap(),
        }],
    };
    let params = BTreeMap::from([("g".into(), CellValue::Text("even".into()))]);
    let s = ReportDesignerSession::new(d, table(), params.clone()).unwrap();
    let loaded = ReportDefinition::from_json(&s.save().unwrap(), table().schema()).unwrap();
    let restored = ReportDesignerSession::new(loaded, table(), params).unwrap();
    assert_eq!(restored.preview().row_keys, vec![4, 2]);
    assert_eq!(restored.preview().totals, vec![CellValue::Integer(60)]);
    assert_eq!(restored.save().unwrap(), s.save().unwrap());
}
#[test]
fn dashboard_cross_filter_is_typed_idempotent_and_does_not_filter_origin() {
    let mut s = dashboard();
    let revision = s.revision();
    let filter = DashboardFilter {
        field: "group".into(),
        values: vec![CellValue::Text("even".into())],
    };
    assert!(
        s.set_filter(revision, "grid", Some(filter.clone()))
            .unwrap()
    );
    assert_eq!(s.results()["grid"].rows, vec![1, 2, 3, 4]);
    assert_eq!(s.results()["chart"].rows, vec![2, 4]);
    assert_eq!(s.results()["chart"].totals, vec![CellValue::Integer(60)]);
    let current = s.revision();
    assert!(!s.set_filter(current, "grid", Some(filter)).unwrap());
    assert_eq!(s.revision(), current);
    assert!(s.set_filter(revision, "grid", None).is_err());
    assert!(
        s.set_filter(
            current,
            "grid",
            Some(DashboardFilter {
                field: "amount".into(),
                values: vec![CellValue::Text("20".into())]
            })
        )
        .is_err()
    );
    assert_eq!(s.revision(), current);
    assert_eq!(s.results()["chart"].rows, vec![2, 4]);
    s.clear_filters().unwrap();
    assert_eq!(s.results()["chart"].rows, vec![1, 2, 3, 4]);
}
#[test]
fn dashboard_layout_history_missing_widget_and_parameters_roundtrip() {
    let mut s = dashboard();
    let mut d = s.definition().clone();
    d.items[1].widget = DashboardWidget::Missing {
        name: "custom-map".into(),
        payload: serde_json::json!({"center":[10,20]}),
    };
    d.parameters.push(DashboardParameter {
        name: "region".into(),
        source: "s".into(),
        field: "group".into(),
        value: CellValue::Text("odd".into()),
    });
    s.replace(s.revision(), d).unwrap();
    assert_eq!(s.results()["grid"].rows, vec![1, 3]);
    assert!(s.results()["chart"].diagnostic.is_some());
    let json = s.save().unwrap();
    s.undo().unwrap();
    assert!(matches!(
        s.definition().items[1].widget,
        DashboardWidget::Chart { .. }
    ));
    s.redo().unwrap();
    assert_eq!(json, s.save().unwrap());
    s.restore(s.revision(), &json).unwrap();
    assert_eq!(json, s.save().unwrap());
    let mut invalid = s.definition().clone();
    invalid.items[1].placement = invalid.items[0].placement;
    let revision = s.revision();
    assert!(s.replace(revision, invalid).is_err());
    assert_eq!(s.revision(), revision);
    assert!(s.set_parameter("region", CellValue::Integer(3)).is_err());
    assert_eq!(s.save().unwrap(), json);
}
#[test]
fn dashboard_null_empty_filter_and_source_refresh_are_transactional() {
    let mut s = dashboard();
    s.set_filter(
        s.revision(),
        "grid",
        Some(DashboardFilter {
            field: "group".into(),
            values: vec![],
        }),
    )
    .unwrap();
    assert!(s.results()["chart"].rows.is_empty());
    let before = s.revision();
    assert!(s.refresh_sources(before, BTreeMap::new()).is_err());
    assert_eq!(s.revision(), before);
    s.refresh_sources(before, BTreeMap::from([("s".into(), table())]))
        .unwrap();
    assert!(s.refresh_sources(before, BTreeMap::new()).is_err());
}
fn bitmap(v: u8) -> Bitmap {
    Bitmap::new_rgba8(2, 2, vec![v; 16]).unwrap()
}
fn wait_until(mut check: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !check() {
        assert!(Instant::now() < deadline, "worker timed out");
        std::thread::sleep(Duration::from_millis(2));
    }
}
#[test]
fn media_sequence_play_seek_pause_capture_stop_and_replay() {
    let frames = vec![bitmap(10), bitmap(20), bitmap(30)];
    let f: MediaFactory = Arc::new(move |_| {
        Ok(Box::new(ImageSequenceProvider::new(
            "sequence",
            frames.clone(),
            Duration::from_millis(100),
        )?))
    });
    let mut s = MediaSession::default();
    s.open(f).unwrap();
    wait_until(|| {
        s.poll(Instant::now()).unwrap();
        s.frame().is_some()
    });
    assert_eq!(s.state(), MediaState::Paused);
    assert_eq!(s.capture().unwrap().pixels()[0], 10);
    s.seek(Duration::from_millis(200), Instant::now()).unwrap();
    wait_until(|| {
        s.poll(Instant::now()).unwrap();
        s.frame().unwrap().position == Duration::from_millis(200)
    });
    assert_eq!(s.capture().unwrap().pixels()[0], 30);
    let start = Instant::now();
    s.play(start).unwrap();
    s.pause(start + Duration::from_millis(25)).unwrap();
    assert_eq!(s.position(), Duration::from_millis(225));
    assert_eq!(s.state(), MediaState::Paused);
    assert!(s.set_volume(0.5).is_err());
    assert!(s.seek(Duration::from_secs(2), start).is_err());
    s.stop();
    assert!(s.frame().is_none());
    assert_eq!(s.state(), MediaState::Stopped);
    wait_until(|| s.play(Instant::now()).is_ok());
    wait_until(|| {
        s.poll(Instant::now()).unwrap();
        s.frame().is_some()
    });
    assert_eq!(s.state(), MediaState::Playing);
    s.close();
    assert!(s.info().is_none());
    assert_eq!(s.state(), MediaState::Empty);
}
struct Probe {
    drop_count: Arc<AtomicUsize>,
    gate: Arc<AtomicBool>,
    calls: Arc<AtomicUsize>,
    fail: bool,
}
impl Drop for Probe {
    fn drop(&mut self) {
        self.drop_count.fetch_add(1, Ordering::SeqCst);
    }
}
impl MediaProvider for Probe {
    fn info(&self) -> MediaInfo {
        MediaInfo {
            title: "probe".into(),
            duration: Some(Duration::from_secs(5)),
            capabilities: MediaCapabilities {
                seek: true,
                capture: true,
                ..Default::default()
            },
        }
    }
    fn frame_at(
        &mut self,
        p: Duration,
        cancel: &AtomicBool,
    ) -> Result<Option<MediaFrame>, MediaError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        while !self.gate.load(Ordering::SeqCst) && !cancel.load(Ordering::Acquire) {
            std::thread::sleep(Duration::from_millis(1));
        }
        if self.fail {
            return Err(MediaError("decoder failure".into()));
        }
        Ok(Some(MediaFrame {
            position: p,
            bitmap: bitmap(p.as_secs() as u8),
        }))
    }
}
fn probe(
    fail: bool,
) -> (
    MediaFactory,
    Arc<AtomicUsize>,
    Arc<AtomicBool>,
    Arc<AtomicUsize>,
) {
    let drops = Arc::new(AtomicUsize::new(0));
    let gate = Arc::new(AtomicBool::new(false));
    let calls = Arc::new(AtomicUsize::new(0));
    let (d, g, c) = (drops.clone(), gate.clone(), calls.clone());
    (
        Arc::new(move |_| {
            Ok(Box::new(Probe {
                drop_count: d.clone(),
                gate: g.clone(),
                calls: c.clone(),
                fail,
            }))
        }),
        drops,
        gate,
        calls,
    )
}
#[test]
fn media_superseded_frame_never_overwrites_seek_and_ui_does_not_wait() {
    let (f, drops, gate, calls) = probe(false);
    let mut s = MediaSession::default();
    s.open(f).unwrap();
    wait_until(|| {
        s.poll(Instant::now()).unwrap();
        calls.load(Ordering::SeqCst) > 0
    });
    let start = Instant::now();
    for n in 1..=4 {
        s.seek(Duration::from_secs(n), Instant::now()).unwrap();
    }
    assert!(start.elapsed() < Duration::from_millis(100));
    gate.store(true, Ordering::SeqCst);
    wait_until(|| {
        s.poll(Instant::now()).unwrap();
        s.frame().is_some()
    });
    assert_eq!(s.frame().unwrap().position, Duration::from_secs(4));
    assert!(calls.load(Ordering::SeqCst) <= 2);
    s.stop();
    wait_until(|| drops.load(Ordering::SeqCst) == 1);
}
#[test]
fn media_error_stop_and_owner_drop_release_provider_even_after_new_request() {
    for mode in 0..3 {
        let (f, drops, gate, calls) = probe(mode == 0);
        let mut s = MediaSession::default();
        s.open(f).unwrap();
        wait_until(|| {
            s.poll(Instant::now()).unwrap();
            calls.load(Ordering::SeqCst) > 0
        });
        if mode == 0 {
            s.seek(Duration::from_secs(3), Instant::now()).unwrap();
            gate.store(true, Ordering::SeqCst);
            wait_until(|| {
                s.poll(Instant::now()).unwrap();
                s.state() == MediaState::Error
            });
            assert_eq!(s.error().unwrap().0, "decoder failure");
            assert!(s.frame().is_none());
        } else if mode == 1 {
            s.stop();
        } else {
            drop(s);
        }
        wait_until(|| drops.load(Ordering::SeqCst) == 1);
    }
}

#[test]
fn media_volume_confirmation_is_independent_of_superseding_frame_requests() {
    struct VolumeProbe(Probe);
    impl MediaProvider for VolumeProbe {
        fn info(&self) -> MediaInfo {
            let mut i = self.0.info();
            i.capabilities.volume = true;
            i
        }
        fn frame_at(
            &mut self,
            p: Duration,
            c: &AtomicBool,
        ) -> Result<Option<MediaFrame>, MediaError> {
            self.0.frame_at(p, c)
        }
        fn set_volume(&mut self, _v: f32) -> Result<(), MediaError> {
            Ok(())
        }
    }
    let drops = Arc::new(AtomicUsize::new(0));
    let gate = Arc::new(AtomicBool::new(false));
    let calls = Arc::new(AtomicUsize::new(0));
    let (d, g, c) = (drops.clone(), gate.clone(), calls.clone());
    let f: MediaFactory = Arc::new(move |_| {
        Ok(Box::new(VolumeProbe(Probe {
            drop_count: d.clone(),
            gate: g.clone(),
            calls: c.clone(),
            fail: false,
        })))
    });
    let mut s = MediaSession::default();
    s.open(f).unwrap();
    wait_until(|| {
        s.poll(Instant::now()).unwrap();
        calls.load(Ordering::SeqCst) > 0
    });
    s.set_volume(0.25).unwrap();
    assert_eq!(s.volume(), 1.0);
    s.seek(Duration::from_secs(2), Instant::now()).unwrap();
    gate.store(true, Ordering::SeqCst);
    wait_until(|| {
        s.poll(Instant::now()).unwrap();
        s.frame()
            .is_some_and(|f| f.position == Duration::from_secs(2))
            && s.volume() == 0.25
    });
    s.close();
    wait_until(|| drops.load(Ordering::SeqCst) == 1);
}

#[test]
fn report_history_budget_and_data_failure_preserve_preview() {
    let mut s = report();
    for n in 0..70 {
        let mut d = s.definition().clone();
        d.title = format!("Revision {n}");
        s.replace(s.revision(), d).unwrap();
    }
    let mut count = 0;
    while s.undo().unwrap() {
        count += 1;
    }
    assert_eq!(count, 64);
    let before = s.save().unwrap();
    let revision = s.revision();
    let invalid = DataTable::new(
        TableSchema::new(vec![FieldSchema::new("other", FieldType::Text)]).unwrap(),
        vec![],
    )
    .unwrap();
    assert!(s.set_data(invalid, BTreeMap::new()).is_err());
    assert_eq!(s.revision(), revision);
    assert_eq!(s.save().unwrap(), before);
    assert_eq!(s.preview().row_keys, vec![1, 2, 3, 4]);
}
#[test]
fn report_canvas_and_dashboard_retain_editors_across_resize_and_filter() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let root = ui.root();
    ui.runtime
        .apply(Mutation::SetStyle {
            node: root,
            style: LayoutStyle {
                kind: LayoutKind::Grid,
                grid_columns: vec![GridTrack::Fr(1.0)],
                grid_rows: vec![GridTrack::Fr(1.0)],
                ..Default::default()
            },
        })
        .unwrap();
    let mut report = ReportDesigner::mount(
        &mut ui,
        root,
        report(),
        LayoutStyle {
            width: Length::Percent(1.0),
            height: Length::Percent(1.0),
            ..Default::default()
        },
    )
    .unwrap();
    report
        .session_mut()
        .select(Some(ReportSelection {
            band: ReportBandKind::Detail,
            item: 0,
        }))
        .unwrap();
    ui.update(Size::new(1000.0, 700.0)).unwrap();
    report.refresh(&mut ui).unwrap();
    ui.update(Size::new(1000.0, 700.0)).unwrap();
    let editor = report.properties[4];
    ui.set_text(editor, "draft binding").unwrap();
    ui.update(Size::new(850.0, 600.0)).unwrap();
    report.refresh(&mut ui).unwrap();
    assert_eq!(ui.document(editor).unwrap().text(), "draft binding");
    assert!(
        ui.runtime.bounds(report.canvas).unwrap().height > 300.0,
        "canvas={:?}, root={:?}",
        ui.runtime.bounds(report.canvas),
        ui.runtime.bounds(report.root)
    );
    ui.remove(report.root).unwrap();
    let mut dashboard = Dashboard::mount(
        &mut ui,
        root,
        dashboard(),
        LayoutStyle {
            width: Length::Percent(1.0),
            height: Length::Percent(1.0),
            ..Default::default()
        },
    )
    .unwrap();
    ui.update(Size::new(1000.0, 700.0)).unwrap();
    dashboard.refresh(&mut ui).unwrap();
    ui.update(Size::new(1000.0, 700.0)).unwrap();
    let grid = dashboard.widget("grid").unwrap();
    let chart = dashboard.widget("chart").unwrap();
    ui.set_grid_selected_key(grid, Some(2)).unwrap();
    assert!(dashboard.selection_changed(&ui).unwrap());
    dashboard.refresh(&mut ui).unwrap();
    ui.update(Size::new(850.0, 600.0)).unwrap();
    assert!(
        !dashboard.selection_changed(&ui).unwrap(),
        "refresh emitted a filter loop"
    );
    assert_eq!(dashboard.widget("grid"), Some(grid));
    assert_eq!(dashboard.widget("chart"), Some(chart));
    assert_eq!(dashboard.session().results()["chart"].rows, vec![2]);
}
