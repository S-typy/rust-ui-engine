use super::*;

fn id(value: &str) -> DockPaneId {
    DockPaneId::new(value).unwrap()
}

fn catalog() -> Vec<DockPane> {
    ["document", "properties", "output", "search"]
        .into_iter()
        .map(|name| DockPane {
            id: id(name),
            title: name.into(),
            closable: name != "document",
            min_size: Size::new(100.0, 70.0),
        })
        .collect()
}

fn tabs(group: u64, names: &[&str]) -> DockNode {
    DockNode::Tabs {
        id: DockGroupId(group),
        panes: names.iter().map(|name| id(name)).collect(),
        active: id(names[0]),
    }
}

fn sample() -> DockLayout {
    DockLayout::new(
        catalog(),
        DockNode::Split {
            id: DockSplitId(1),
            axis: Axis::Horizontal,
            ratio: 0.7,
            first: Box::new(tabs(1, &["document", "search"])),
            second: Box::new(DockNode::Split {
                id: DockSplitId(2),
                axis: Axis::Vertical,
                ratio: 0.6,
                first: Box::new(tabs(2, &["properties"])),
                second: Box::new(tabs(3, &["output"])),
            }),
        },
    )
    .unwrap()
}

fn floating_bounds() -> Rect {
    Rect::new(-400.0, 120.0, 640.0, 480.0)
}

#[test]
fn initialization_validates_ids_catalog_and_places_omitted_panes_closed() {
    assert!(DockPaneId::new("").is_err());
    assert!(DockPaneId::new("line\nbreak").is_err());
    assert!(DockPaneId::new("ы".repeat(65)).is_err());
    assert_eq!(DockPaneId::new("Свойства").unwrap().as_str(), "Свойства");
    let layout = DockLayout::new(catalog(), tabs(8, &["document"])).unwrap();
    assert!(layout.is_closed(&id("output")));
    assert_eq!(
        layout.pane_location(&id("document")),
        Some((DockWindowId::MAIN, DockGroupId(8)))
    );
    assert_eq!(layout.panes().len(), 4);
    let mut duplicate = catalog();
    duplicate.push(duplicate[0].clone());
    assert!(DockLayout::new(duplicate, None).is_err());
    let mut invalid_minimum = catalog();
    invalid_minimum[0].min_size.width = f32::INFINITY;
    assert!(DockLayout::new(invalid_minimum, None).is_err());
}

#[test]
fn snapshots_round_trip_without_content_or_widgets() {
    let mut layout = sample();
    layout.float_pane(&id("output"), floating_bounds()).unwrap();
    layout.close(&id("search")).unwrap();
    let snapshot = layout.snapshot();
    assert_eq!(
        DockLayout::from_snapshot(catalog(), snapshot.clone())
            .unwrap()
            .snapshot(),
        snapshot
    );
    assert!(!layout.restore_snapshot(snapshot).unwrap());
}

#[test]
fn malformed_snapshot_rejection_is_atomic() {
    let mut layout = sample();
    let before = layout.snapshot();
    let mut invalids = Vec::new();
    let mut unknown_version = before.clone();
    unknown_version.version = DOCK_SNAPSHOT_VERSION + 1;
    invalids.push(unknown_version);
    let mut duplicate = before.clone();
    duplicate.closed.push(id("document"));
    invalids.push(duplicate);
    let mut missing = before.clone();
    missing.root = None;
    invalids.push(missing);
    let mut wrong_active = before.clone();
    wrong_active.root = Some(DockNode::Tabs {
        id: DockGroupId(1),
        panes: vec![id("document")],
        active: id("output"),
    });
    invalids.push(wrong_active);
    let mut unknown = before.clone();
    unknown.closed.push(id("unknown"));
    invalids.push(unknown);
    let mut invalid_ratio = before.clone();
    if let Some(DockNode::Split { ratio, .. }) = &mut invalid_ratio.root {
        *ratio = f32::NAN;
    }
    invalids.push(invalid_ratio);
    let mut reserved_window = before.clone();
    reserved_window.floating.push(DockFloating {
        id: DockWindowId::MAIN,
        root: tabs(12, &["output"]),
        bounds: floating_bounds(),
    });
    invalids.push(reserved_window);
    for invalid in invalids {
        assert!(layout.restore_snapshot(invalid).is_err());
        assert_eq!(layout.snapshot(), before);
    }
}

#[test]
fn split_and_group_ids_are_unique_across_native_windows() {
    let mut layout = sample();
    let window = layout.float_pane(&id("search"), floating_bounds()).unwrap();
    let before = layout.snapshot();
    let mut invalid = before.clone();
    invalid.floating[0].root = DockNode::Tabs {
        id: DockGroupId(1),
        panes: vec![id("search")],
        active: id("search"),
    };
    assert!(layout.restore_snapshot(invalid).is_err());
    assert_eq!(layout.snapshot(), before);
    assert_ne!(layout.group_for(&id("search")), Some(DockGroupId(1)));
    assert_eq!(layout.window_for(&id("search")), Some(window));
}

#[test]
fn tab_reordering_uses_original_insertion_slots_and_selects_moved_pane() {
    let mut layout = sample();
    assert!(
        layout
            .move_pane(
                &id("document"),
                DockPlacement::Tab {
                    group: DockGroupId(1),
                    index: 2
                }
            )
            .unwrap()
    );
    assert!(
        matches!(layout.root(DockWindowId::MAIN), Some(DockNode::Split { first, .. })
        if matches!(first.as_ref(), DockNode::Tabs { panes, active, .. } if panes == &vec![id("search"), id("document")] && active == &id("document")))
    );
    assert!(
        !layout
            .move_pane(
                &id("document"),
                DockPlacement::Tab {
                    group: DockGroupId(1),
                    index: 2
                }
            )
            .unwrap()
    );
    assert!(
        layout
            .move_pane(
                &id("document"),
                DockPlacement::Tab {
                    group: DockGroupId(1),
                    index: 0
                }
            )
            .unwrap()
    );
    let before = layout.snapshot();
    assert!(
        layout
            .move_pane(
                &id("document"),
                DockPlacement::Tab {
                    group: DockGroupId(1),
                    index: 8
                }
            )
            .is_err()
    );
    assert_eq!(layout.snapshot(), before);
}

#[test]
fn source_group_is_normalized_and_active_neighbor_is_preserved() {
    let mut layout = sample();
    layout.activate(&id("search")).unwrap();
    layout
        .move_pane(
            &id("search"),
            DockPlacement::Tab {
                group: DockGroupId(3),
                index: 1,
            },
        )
        .unwrap();
    assert!(
        matches!(layout.group(DockGroupId(1)), Some(DockNode::Tabs { active, .. }) if active == &id("document"))
    );
    layout
        .move_pane(
            &id("properties"),
            DockPlacement::Tab {
                group: DockGroupId(3),
                index: 0,
            },
        )
        .unwrap();
    assert!(layout.group(DockGroupId(2)).is_none());
    let geometry = layout
        .arrange(DockWindowId::MAIN, Rect::new(0.0, 0.0, 800.0, 600.0))
        .unwrap();
    assert_eq!(geometry.splitters.len(), 1);
    assert_eq!(
        layout.window_panes(DockWindowId::MAIN).unwrap(),
        vec![id("document"), id("properties"), id("output"), id("search")]
    );
}

#[test]
fn float_move_between_windows_and_redock_remove_empty_native_containers() {
    let mut layout = sample();
    let first = layout
        .float_pane(&id("properties"), floating_bounds())
        .unwrap();
    let group = layout.group_for(&id("properties")).unwrap();
    layout
        .move_pane(&id("output"), DockPlacement::Tab { group, index: 1 })
        .unwrap();
    assert_eq!(
        layout.window_panes(first).unwrap(),
        vec![id("properties"), id("output")]
    );
    let second = layout
        .float_pane(&id("output"), Rect::new(800.0, 100.0, 500.0, 400.0))
        .unwrap();
    assert_ne!(first, second);
    assert_eq!(layout.floating().len(), 2);
    layout
        .move_pane(&id("output"), DockPlacement::Tab { group, index: 0 })
        .unwrap();
    assert_eq!(layout.floating().len(), 1);
    assert!(
        layout
            .arrange(second, Rect::new(0.0, 0.0, 1.0, 1.0))
            .is_err()
    );
    layout.redock_window_at(first, DockEdge::Right).unwrap();
    assert!(layout.floating().is_empty());
    assert_eq!(layout.group_for(&id("properties")), Some(group));
    assert_eq!(layout.window_for(&id("output")), Some(DockWindowId::MAIN));
}

#[test]
fn floating_only_layout_and_native_window_close_preserve_full_split_tree() {
    let mut layout = DockLayout::new(catalog(), tabs(1, &["document"])).unwrap();
    let window = layout
        .float_pane(&id("document"), floating_bounds())
        .unwrap();
    assert!(layout.root(DockWindowId::MAIN).is_none());
    assert!(
        layout
            .arrange(DockWindowId::MAIN, Rect::new(0.0, 0.0, 400.0, 300.0))
            .unwrap()
            .groups
            .is_empty()
    );
    layout
        .restore(
            &id("output"),
            DockPlacement::Root {
                window,
                edge: DockEdge::Bottom,
            },
        )
        .unwrap();
    let before_tree = layout.root(window).cloned();
    layout.redock_window_at(window, DockEdge::Left).unwrap();
    assert_eq!(layout.root(DockWindowId::MAIN), before_tree.as_ref());
    assert_eq!(layout.window_for(&id("document")), Some(DockWindowId::MAIN));
    assert!(
        layout
            .redock_window_at(DockWindowId::MAIN, DockEdge::Left)
            .is_err()
    );
}

#[test]
fn close_last_floating_pane_restore_and_nonclosable_policy() {
    let mut layout = sample();
    let window = layout.float_pane(&id("output"), floating_bounds()).unwrap();
    assert!(layout.close(&id("output")).unwrap());
    assert!(!layout.close(&id("output")).unwrap());
    assert!(layout.floating().is_empty());
    assert!(layout.window_panes(window).is_err());
    let before = layout.snapshot();
    assert!(layout.close(&id("document")).is_err());
    assert_eq!(layout.snapshot(), before);
    layout
        .restore(
            &id("output"),
            DockPlacement::Root {
                window: DockWindowId::MAIN,
                edge: DockEdge::Bottom,
            },
        )
        .unwrap();
    assert!(!layout.is_closed(&id("output")));
    assert!(
        layout
            .restore(
                &id("output"),
                DockPlacement::Tab {
                    group: DockGroupId(1),
                    index: 0
                }
            )
            .is_err()
    );
}

#[test]
fn isolated_float_updates_geometry_without_changing_window_or_group_identity() {
    let mut layout = sample();
    let window = layout.float_pane(&id("output"), floating_bounds()).unwrap();
    let group = layout.group_for(&id("output"));
    let moved = Rect::new(-1900.0, -300.0, 900.0, 700.0);
    assert_eq!(layout.float_pane(&id("output"), moved).unwrap(), window);
    assert_eq!(layout.group_for(&id("output")), group);
    assert_eq!(layout.floating()[0].bounds, moved);
    let before = layout.snapshot();
    for bad in [
        Rect::new(0.0, 0.0, 0.0, 50.0),
        Rect::new(f32::NAN, 0.0, 100.0, 100.0),
    ] {
        assert!(layout.update_floating_bounds(window, bad).is_err());
        assert_eq!(layout.snapshot(), before);
    }
}

#[test]
fn fresh_identifiers_are_not_recycled_after_close_and_snapshot_restore() {
    let mut layout = sample();
    let baseline = layout.snapshot();
    let first = layout.float_pane(&id("output"), floating_bounds()).unwrap();
    let first_group = layout.group_for(&id("output")).unwrap();
    layout.restore_snapshot(baseline).unwrap();
    let second = layout.float_pane(&id("output"), floating_bounds()).unwrap();
    assert!(second.0 > first.0);
    assert!(layout.group_for(&id("output")).unwrap().0 > first_group.0);
}

#[test]
fn unavailable_targets_and_bad_ratios_leave_live_layout_untouched() {
    let mut layout = sample();
    let before = layout.snapshot();
    for placement in [
        DockPlacement::Tab {
            group: DockGroupId(99),
            index: 0,
        },
        DockPlacement::Split {
            group: DockGroupId(99),
            edge: DockEdge::Left,
        },
        DockPlacement::Root {
            window: DockWindowId(99),
            edge: DockEdge::Top,
        },
    ] {
        assert!(layout.move_pane(&id("output"), placement).is_err());
        assert_eq!(layout.snapshot(), before);
    }
    for ratio in [0.0, 1.0, -0.1, f32::NAN, f32::INFINITY] {
        assert!(layout.resize(DockSplitId(1), ratio).is_err());
        assert_eq!(layout.snapshot(), before);
    }
    assert!(layout.resize(DockSplitId(99), 0.5).is_err());
    assert_eq!(layout.snapshot(), before);
    assert!(layout.resize(DockSplitId(1), 0.6).unwrap());
    assert!(!layout.resize(DockSplitId(1), 0.6).unwrap());
}

#[test]
fn splitting_within_own_tab_group_and_empty_main_restore_work() {
    let mut layout = sample();
    layout
        .move_pane(
            &id("search"),
            DockPlacement::Split {
                group: DockGroupId(1),
                edge: DockEdge::Bottom,
            },
        )
        .unwrap();
    assert_ne!(layout.group_for(&id("search")), Some(DockGroupId(1)));
    assert!(
        !layout
            .move_pane(
                &id("document"),
                DockPlacement::Split {
                    group: DockGroupId(1),
                    edge: DockEdge::Left
                }
            )
            .unwrap()
    );
    let mut empty = DockLayout::new(catalog(), None).unwrap();
    empty
        .restore(
            &id("document"),
            DockPlacement::Root {
                window: DockWindowId::MAIN,
                edge: DockEdge::Left,
            },
        )
        .unwrap();
    assert!(
        !empty
            .move_pane(
                &id("document"),
                DockPlacement::Root {
                    window: DockWindowId::MAIN,
                    edge: DockEdge::Bottom
                }
            )
            .unwrap()
    );
    assert_eq!(
        empty.window_panes(DockWindowId::MAIN).unwrap(),
        vec![id("document")]
    );
}

#[test]
fn bounded_geometry_respects_minimums_and_does_not_rewrite_preferences() {
    let layout = sample();
    let saved = layout.snapshot();
    for (width, height) in [(900.0, 600.0), (240.0, 200.0), (1.0, 1.0), (0.0, 0.0)] {
        let area = Rect::new(10.0, 15.0, width, height);
        let geometry = layout.arrange(DockWindowId::MAIN, area).unwrap();
        assert_eq!(geometry.minimum_size, Size::new(206.0, 210.0));
        assert_eq!(geometry.groups.len(), 3);
        for rect in geometry
            .groups
            .iter()
            .flat_map(|group| [group.bounds, group.tab_strip, group.content])
            .chain(geometry.splitters.iter().map(|split| split.bounds))
        {
            assert!(rect.is_valid(), "{rect:?}");
            assert!(rect.x >= area.x && rect.y >= area.y);
            assert!(rect.x + rect.width <= area.x + area.width + 0.001);
            assert!(rect.y + rect.height <= area.y + area.height + 0.001);
        }
        if width == 900.0 {
            assert!(
                geometry
                    .groups
                    .iter()
                    .all(|group| group.content.width >= 100.0 && group.content.height >= 70.0)
            );
            assert_eq!(geometry.splitters[0].effective_ratio, 0.7);
        }
        assert_eq!(layout.snapshot(), saved);
    }
}

#[test]
fn empty_catalog_and_absent_main_workspace_are_valid() {
    let layout = DockLayout::new(Vec::new(), None).unwrap();
    assert!(layout.panes().is_empty());
    assert!(layout.window_panes(DockWindowId::MAIN).unwrap().is_empty());
    assert!(
        layout
            .arrange(DockWindowId::MAIN, Rect::new(0.0, 0.0, 0.0, 0.0))
            .unwrap()
            .groups
            .is_empty()
    );
}

#[test]
fn floating_limit_and_catalog_limit_are_enforced_atomically() {
    let panes: Vec<_> = (0..17)
        .map(|index| DockPane {
            id: id(&format!("pane{index}")),
            title: String::new(),
            closable: true,
            min_size: Size::ZERO,
        })
        .collect();
    let mut layout = DockLayout::new(panes.clone(), None).unwrap();
    for pane in &panes[..16] {
        layout.float_pane(&pane.id, floating_bounds()).unwrap();
    }
    let before = layout.snapshot();
    assert!(layout.float_pane(&panes[16].id, floating_bounds()).is_err());
    assert_eq!(layout.snapshot(), before);
    let too_many = (0..65)
        .map(|index| DockPane {
            id: id(&format!("pane{index}")),
            title: String::new(),
            closable: true,
            min_size: Size::ZERO,
        })
        .collect();
    assert!(DockLayout::new(too_many, None).is_err());
}

#[test]
fn snapshot_depth_limit_and_id_exhaustion_are_atomic() {
    let mut layout = sample();
    let before = layout.snapshot();
    let mut deep = tabs(100, &["document"]);
    for index in 0..33 {
        deep = DockNode::Split {
            id: DockSplitId(index + 100),
            axis: Axis::Horizontal,
            ratio: 0.5,
            first: Box::new(tabs(index + 101, &["output"])),
            second: Box::new(deep),
        };
    }
    let mut invalid = before.clone();
    invalid.root = Some(deep);
    assert!(layout.restore_snapshot(invalid).is_err());
    assert_eq!(layout.snapshot(), before);
    let mut exhausted = DockLayout::new(catalog(), tabs(u64::MAX, &["document"])).unwrap();
    let before = exhausted.snapshot();
    assert!(
        exhausted
            .float_pane(&id("output"), floating_bounds())
            .is_err()
    );
    assert_eq!(exhausted.snapshot(), before);
}

#[test]
fn fractional_minimum_rounding_cannot_invert_split_clamp_bounds() {
    let mut panes = catalog();
    panes[0].min_size.width = 2136.977;
    panes[1].min_size.width = 27144.72;
    let layout = DockLayout::new(
        panes,
        DockNode::Split {
            id: DockSplitId(1),
            axis: Axis::Horizontal,
            ratio: 0.5,
            first: Box::new(tabs(1, &["document"])),
            second: Box::new(tabs(2, &["properties"])),
        },
    )
    .unwrap();
    let geometry = layout
        .arrange(DockWindowId::MAIN, Rect::new(0.0, 0.0, 29287.697, 200.0))
        .unwrap();
    assert!(geometry.groups.iter().all(|group| group.bounds.is_valid()));
    assert_eq!(geometry.groups[0].bounds.width, 2136.977);
    assert!(geometry.groups[1].bounds.x + geometry.groups[1].bounds.width <= 29287.697);
}
