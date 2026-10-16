use super::*;
use crate::{SemanticAction, Theme};
use rust_desktop_ui_core::Size;

fn pane(value: &str) -> DockPaneId {
    DockPaneId::new(value).unwrap()
}

fn fixture() -> (Controls, DockLayout, DockView) {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let root = ui.root();
    ui.runtime
        .apply(Mutation::SetStyle {
            node: root,
            style: LayoutStyle {
                kind: LayoutKind::Overlay,
                ..Default::default()
            },
        })
        .unwrap();
    let catalog = ["Document", "Search", "Properties"]
        .map(|name| DockPane {
            id: pane(name),
            title: name.into(),
            closable: name != "Document",
            min_size: Size::new(100.0, 70.0),
        })
        .to_vec();
    let layout = DockLayout::new(
        catalog,
        DockNode::Split {
            id: DockSplitId(1),
            axis: Axis::Horizontal,
            ratio: 0.65,
            first: Box::new(DockNode::Tabs {
                id: DockGroupId(1),
                panes: vec![pane("Document"), pane("Search")],
                active: pane("Document"),
            }),
            second: Box::new(DockNode::Tabs {
                id: DockGroupId(2),
                panes: vec![pane("Properties")],
                active: pane("Properties"),
            }),
        },
    )
    .unwrap();
    let mut view = DockView::new(&mut ui, root, DockWindowId::MAIN).unwrap();
    refresh(&mut ui, &layout, &mut view);
    (ui, layout, view)
}

fn refresh(ui: &mut Controls, layout: &DockLayout, view: &mut DockView) {
    view.update(ui, layout, Rect::new(8.0, 90.0, 784.0, 500.0))
        .unwrap();
    ui.update(Size::new(800.0, 600.0)).unwrap();
}

fn center(bounds: Rect) -> Point {
    Point::new(
        bounds.x + bounds.width / 2.0,
        bounds.y + bounds.height / 2.0,
    )
}
fn down(position: Point) -> InputEvent {
    InputEvent::PointerDown {
        position,
        button: PointerButton::Primary,
        modifiers: Modifiers::default(),
    }
}
fn up(position: Point) -> InputEvent {
    InputEvent::PointerUp {
        position,
        button: PointerButton::Primary,
        modifiers: Modifiers::default(),
    }
}
fn key(key: Key) -> InputEvent {
    InputEvent::KeyDown {
        key,
        modifiers: Modifiers::default(),
        repeat: false,
    }
}

fn apply(layout: &mut DockLayout, requests: Vec<DockRequest>) {
    for request in requests {
        match request {
            DockRequest::Activate(pane) => {
                layout.activate(&pane).unwrap();
            }
            DockRequest::Close(pane) => {
                layout.close(&pane).unwrap();
            }
            DockRequest::Float(pane) => {
                layout
                    .float_pane(&pane, Rect::new(50.0, 50.0, 400.0, 300.0))
                    .unwrap();
            }
            DockRequest::RedockWindow(window) => {
                layout.redock_window(window).unwrap();
            }
            DockRequest::Move { pane, placement } => {
                layout.move_pane(&pane, placement).unwrap();
            }
            DockRequest::Resize { split, ratio } => {
                layout.resize(split, ratio).unwrap();
            }
            DockRequest::SetAutoHide { pane, settings } => {
                layout.set_auto_hide(&pane, settings).unwrap();
            }
            DockRequest::DismissAutoHide { window, .. } => {
                layout.dismiss_auto_hide(window).unwrap();
            }
        }
    }
}

#[test]
fn global_geometry_selected_semantics_and_focus_survive_updates() {
    let (mut ui, layout, mut view) = fixture();
    let widget = view.tab_widget(&pane("Document")).unwrap();
    let expected = view.tab_bounds[&pane("Document")];
    assert_eq!(ui.runtime.bounds(widget), Some(expected));
    ui.runtime.apply(Mutation::Focus(Some(widget))).unwrap();
    refresh(&mut ui, &layout, &mut view);
    assert_eq!(view.tab_widget(&pane("Document")), Some(widget));
    assert_eq!(ui.runtime.focused(), Some(widget));
    let semantics = ui.semantics();
    let tab = semantics.iter().find(|node| node.id == widget).unwrap();
    assert_eq!(tab.role, Role::Tab);
    assert!(tab.selected);
    assert_eq!(tab.label, "Document");
    assert!(tab.actions.contains(&SemanticAction::Activate));
    assert!(
        semantics
            .iter()
            .any(|node| node.role == Role::Separator
                && node.actions.contains(&SemanticAction::Focus))
    );
    assert_eq!(view.geometry().unwrap().groups[0].content.y, 122.0);
}

#[test]
fn selected_tab_paints_an_accent_and_semibold_label_in_both_fluent_themes() {
    for theme in [Theme::FluentLight, Theme::FluentDark] {
        let (mut ui, mut layout, mut view) = fixture();
        ui.set_theme(theme).unwrap();
        let document = pane("Document");
        let search = pane("Search");
        let document_widget = view.tab_widget(&document).unwrap();
        let search_widget = view.tab_widget(&search).unwrap();
        for selected in [&document, &search, &search] {
            let widget = view.tab_widget(selected).unwrap();
            ui.activate(widget).unwrap();
            let requests = view.process_activated(&ui.drain_activated(), &layout);
            apply(&mut layout, requests);
            refresh(&mut ui, &layout, &mut view);
            for pane in [&document, &search] {
                let bounds = view.tab_bounds[pane];
                let underline = Rect::new(
                    bounds.x + 8.0,
                    bounds.y + bounds.height - 3.0,
                    bounds.width - 16.0,
                    3.0,
                );
                let marks = ui
                    .base_scene()
                    .rounded_rectangles
                    .iter()
                    .filter(|mark| {
                        mark.bounds == underline
                            && mark.color == theme.tokens().focus
                            && mark.stroke_width.is_none()
                    })
                    .count();
                assert_eq!(marks, usize::from(pane == selected), "{theme:?} {pane:?}");
                let label = ui
                    .base_scene()
                    .texts
                    .iter()
                    .find(|run| run.text == pane.as_str())
                    .unwrap();
                assert_eq!(label.weight, if pane == selected { 600 } else { 400 });
                let semantic = ui
                    .semantics()
                    .into_iter()
                    .find(|node| node.id == view.tab_widget(pane).unwrap())
                    .unwrap();
                assert_eq!(semantic.selected, pane == selected);
            }
            assert_eq!(view.tab_widget(&document), Some(document_widget));
            assert_eq!(view.tab_widget(&search), Some(search_widget));
        }
    }
}

#[test]
fn narrow_selected_title_fits_the_painted_semibold_font_without_clipping_a_glyph() {
    let (mut ui, layout, mut view) = fixture();
    let title = "Обозреватель";
    let regular = measure_title(&mut ui, title, false).unwrap();
    let selected = measure_title(&mut ui, title, true).unwrap();
    assert!(selected > regular);
    let width = (regular + selected) / 2.0;
    assert_eq!(fit_title(&mut ui, title, width, false).unwrap(), title);
    let fitted = fit_title(&mut ui, title, width, true).unwrap();
    assert_ne!(fitted, title);
    assert!(fitted.ends_with('…'));
    assert!(measure_title(&mut ui, &fitted, true).unwrap() <= width);

    let mut panes = layout.panes().to_vec();
    panes
        .iter_mut()
        .find(|p| p.id == pane("Document"))
        .unwrap()
        .title = title.into();
    let layout = DockLayout::from_snapshot(panes, layout.snapshot()).unwrap();
    let mut saw_ellipsis = false;
    for width in [170.0, 230.0, 320.0] {
        view.update(&mut ui, &layout, Rect::new(8.0, 90.0, width, 500.0))
            .unwrap();
        ui.update(Size::new(800.0, 600.0)).unwrap();
        let tab = view.tab_bounds[&pane("Document")];
        let run = ui
            .base_scene()
            .texts
            .iter()
            .find(|run| tab.contains(run.bounds.x, run.bounds.y) && run.weight == 600)
            .unwrap()
            .clone();
        saw_ellipsis |= run.text.ends_with('…');
        assert!(measure_title(&mut ui, &run.text, true).unwrap() <= run.clip.width + 0.001);
    }
    assert!(saw_ellipsis);
}

#[test]
fn content_pointer_input_is_not_intercepted() {
    let (mut ui, layout, mut view) = fixture();
    let point = center(view.geometry().unwrap().groups[0].content);
    assert_eq!(
        view.handle_input(&mut ui, &down(point), &layout).unwrap(),
        (false, Vec::new())
    );
    assert!(!view.is_interacting());
}

#[test]
fn tab_drag_preview_is_nonmutating_until_drop_and_capture_survives_relayout() {
    let (mut ui, mut layout, mut view) = fixture();
    let source = pane("Search");
    let start = center(view.tab_bounds[&source]);
    let (_, requests) = view.handle_input(&mut ui, &down(start), &layout).unwrap();
    apply(&mut layout, requests);
    refresh(&mut ui, &layout, &mut view);
    assert_eq!(ui.runtime.captured(), view.tab_widget(&source));
    let before = layout.snapshot();
    let target = center(view.geometry().unwrap().groups[1].content);
    let (consumed, requests) = view
        .handle_input(
            &mut ui,
            &InputEvent::PointerMoved { position: target },
            &layout,
        )
        .unwrap();
    assert!(consumed && requests.is_empty());
    assert!(!view.overlay_scene().commands().is_empty());
    assert_eq!(layout.snapshot(), before);
    let (_, requests) = view.handle_input(&mut ui, &up(target), &layout).unwrap();
    assert!(
        matches!(&requests[..], [DockRequest::Move { pane, placement: DockPlacement::Tab { group: DockGroupId(2), .. } }] if pane == &source)
    );
    apply(&mut layout, requests);
    refresh(&mut ui, &layout, &mut view);
    assert_eq!(layout.group_for(&source), Some(DockGroupId(2)));
    assert!(view.overlay_scene().commands().is_empty());
    assert_eq!(ui.runtime.captured(), None);
}

#[test]
fn drag_escape_and_blur_cancel_without_a_move_or_float_request() {
    let (mut ui, layout, mut view) = fixture();
    let start = center(view.tab_bounds[&pane("Search")]);
    for cancel in [key(Key::Escape), InputEvent::WindowFocused(false)] {
        view.handle_input(&mut ui, &down(start), &layout).unwrap();
        view.handle_input(
            &mut ui,
            &InputEvent::PointerMoved {
                position: Point::new(-100.0, 0.0),
            },
            &layout,
        )
        .unwrap();
        let (_, requests) = view.handle_input(&mut ui, &cancel, &layout).unwrap();
        assert!(requests.is_empty());
        assert!(!view.is_interacting());
        assert_eq!(ui.runtime.captured(), None);
        assert!(view.drop_preview().is_none());
    }
}

#[test]
fn release_outside_the_workspace_detaches_only_after_drag_threshold() {
    let (mut ui, layout, mut view) = fixture();
    let start = center(view.tab_bounds[&pane("Search")]);
    view.handle_input(&mut ui, &down(start), &layout).unwrap();
    assert!(
        view.handle_input(&mut ui, &up(Point::new(-100.0, 0.0)), &layout)
            .unwrap()
            .1
            .is_empty()
    );
    view.handle_input(&mut ui, &down(start), &layout).unwrap();
    let outside = Point::new(850.0, 350.0);
    view.handle_input(
        &mut ui,
        &InputEvent::PointerMoved { position: outside },
        &layout,
    )
    .unwrap();
    assert_eq!(
        view.handle_input(&mut ui, &up(outside), &layout).unwrap().1,
        vec![DockRequest::Float(pane("Search"))]
    );
}

#[test]
fn splitter_drag_tracks_ratio_and_escape_or_blur_restores_original() {
    let (mut ui, mut layout, mut view) = fixture();
    for cancel in [key(Key::Escape), InputEvent::WindowFocused(false)] {
        let split = view.geometry().unwrap().splitters[0].clone();
        let start = center(split.bounds);
        view.handle_input(&mut ui, &down(start), &layout).unwrap();
        let (_, requests) = view
            .handle_input(
                &mut ui,
                &InputEvent::PointerMoved {
                    position: Point::new(start.x + 70.0, start.y),
                },
                &layout,
            )
            .unwrap();
        apply(&mut layout, requests);
        refresh(&mut ui, &layout, &mut view);
        assert!(view.geometry().unwrap().splitters[0].ratio > split.ratio);
        let (_, requests) = view.handle_input(&mut ui, &cancel, &layout).unwrap();
        assert_eq!(
            requests,
            vec![DockRequest::Resize {
                split: split.id,
                ratio: split.ratio
            }]
        );
        apply(&mut layout, requests);
        refresh(&mut ui, &layout, &mut view);
        assert_eq!(view.geometry().unwrap().splitters[0].ratio, split.ratio);
    }
}

#[test]
fn keyboard_navigation_activates_and_focuses_the_next_tab() {
    let (mut ui, mut layout, mut view) = fixture();
    ui.runtime
        .apply(Mutation::Focus(view.tab_widget(&pane("Document"))))
        .unwrap();
    for expected in ["Search", "Document"] {
        let (consumed, requests) = view
            .handle_input(&mut ui, &key(Key::ArrowRight), &layout)
            .unwrap();
        assert!(consumed);
        assert_eq!(requests, vec![DockRequest::Activate(pane(expected))]);
        apply(&mut layout, requests);
        refresh(&mut ui, &layout, &mut view);
        assert_eq!(ui.runtime.focused(), view.tab_widget(&pane(expected)));
    }
    ui.runtime
        .apply(Mutation::Focus(view.splitter_widget(DockSplitId(1))))
        .unwrap();
    let (_, requests) = view
        .handle_input(&mut ui, &key(Key::ArrowLeft), &layout)
        .unwrap();
    assert_eq!(
        requests,
        vec![DockRequest::Resize {
            split: DockSplitId(1),
            ratio: 0.65 - 0.02
        }]
    );
}

#[test]
fn button_accessibility_activations_and_hidden_tab_overflow_remain_reachable() {
    let (mut ui, mut layout, mut view) = fixture();
    let close = view.groups[&DockGroupId(2)].close;
    ui.perform_action(close, SemanticAction::Activate, None)
        .unwrap();
    assert_eq!(
        view.process_activated(&ui.drain_activated(), &layout),
        vec![DockRequest::Close(pane("Properties"))]
    );
    layout.resize(DockSplitId(1), 0.2).unwrap();
    refresh(&mut ui, &layout, &mut view);
    let next = view.groups[&DockGroupId(1)].next;
    assert!(ui.runtime.tree().node(next).unwrap().props.visible);
    let requests = view.process_activated(&[next], &layout);
    assert_eq!(requests, vec![DockRequest::Activate(pane("Search"))]);
    apply(&mut layout, requests);
    refresh(&mut ui, &layout, &mut view);
    assert!(view.tab_bounds.contains_key(&pane("Search")));
    let selected = ui
        .semantics()
        .into_iter()
        .find(|node| node.id == view.tab_widget(&pane("Search")).unwrap())
        .unwrap();
    assert!(selected.selected);
}

#[test]
fn moving_tabs_preserves_chrome_ids_and_closed_tabs_are_not_interactive() {
    let (mut ui, mut layout, mut view) = fixture();
    let widget = view.tab_widget(&pane("Search")).unwrap();
    layout
        .move_pane(
            &pane("Search"),
            DockPlacement::Tab {
                group: DockGroupId(2),
                index: 1,
            },
        )
        .unwrap();
    refresh(&mut ui, &layout, &mut view);
    assert_eq!(view.tab_widget(&pane("Search")), Some(widget));
    layout.close(&pane("Search")).unwrap();
    refresh(&mut ui, &layout, &mut view);
    assert!(ui.runtime.tree().contains(widget));
    assert!(ui.runtime.bounds(widget).is_none());
    assert!(view.process_activated(&[widget], &layout).is_empty());
}

#[test]
fn top_header_drop_reorders_tabs_instead_of_creating_an_outer_split() {
    let (mut ui, mut layout, mut view) = fixture();
    let search = pane("Search");
    let start = center(view.tab_bounds[&search]);
    let target_rect = view.tab_bounds[&pane("Document")];
    let target = Point::new(target_rect.x + 8.0, target_rect.y + 16.0);
    let (_, requests) = view.handle_input(&mut ui, &down(start), &layout).unwrap();
    apply(&mut layout, requests);
    view.handle_input(
        &mut ui,
        &InputEvent::PointerMoved { position: target },
        &layout,
    )
    .unwrap();
    assert!(matches!(
        view.drop_preview(),
        Some(DockDropPreview {
            placement: DockPlacement::Tab {
                group: DockGroupId(1),
                index: 0
            },
            ..
        })
    ));
    let (_, requests) = view.handle_input(&mut ui, &up(target), &layout).unwrap();
    apply(&mut layout, requests);
    refresh(&mut ui, &layout, &mut view);
    assert_eq!(view.geometry().unwrap().splitters.len(), 1);
    assert_eq!(
        view.geometry().unwrap().groups[0].panes,
        vec![search, pane("Document")]
    );
}

#[test]
fn late_pointer_down_after_native_blur_does_not_fail_or_capture() {
    let (mut ui, layout, mut view) = fixture();
    let points = [
        center(view.tab_bounds[&pane("Search")]),
        center(view.geometry().unwrap().splitters[0].bounds),
    ];
    view.handle_input(&mut ui, &InputEvent::WindowFocused(false), &layout)
        .unwrap();
    ui.dispatch(InputEvent::WindowFocused(false)).unwrap();
    for point in points {
        assert_eq!(
            view.handle_input(&mut ui, &down(point), &layout).unwrap(),
            (true, Vec::new())
        );
        assert!(!view.is_interacting());
        assert_eq!(ui.runtime.captured(), None);
        assert_eq!(ui.runtime.focused(), None);
    }
}

fn other_window(layout: &DockLayout, window: DockWindowId) -> (Controls, DockView) {
    let mut ui = Controls::new(Theme::FluentDark).unwrap();
    let root = ui.root();
    ui.runtime
        .apply(Mutation::SetStyle {
            node: root,
            style: LayoutStyle {
                kind: LayoutKind::Overlay,
                ..Default::default()
            },
        })
        .unwrap();
    let mut view = DockView::new(&mut ui, root, window).unwrap();
    view.update(&mut ui, layout, Rect::new(8.0, 8.0, 584.0, 384.0))
        .unwrap();
    ui.update(Size::new(600.0, 400.0)).unwrap();
    (ui, view)
}

#[test]
fn cross_window_projection_previews_without_moving_or_transferring_capture() {
    let (mut ui, mut layout, mut source) = fixture();
    let floating = layout
        .float_pane(&pane("Properties"), Rect::new(300.0, 300.0, 600.0, 400.0))
        .unwrap();
    refresh(&mut ui, &layout, &mut source);
    let (mut target_ui, mut target) = other_window(&layout, floating);
    let id = pane("Search");
    let widget = source.tab_widget(&id).unwrap();
    let target_widget = target.tab_widget(&id).unwrap();
    let start = center(source.tab_bounds[&id]);
    let (_, requests) = source.handle_input(&mut ui, &down(start), &layout).unwrap();
    apply(&mut layout, requests);
    let before = layout.snapshot();
    assert_eq!(
        source.tab_drag(),
        Some(DockTabDrag {
            pane: id.clone(),
            widget,
            dragging: false
        })
    );
    source
        .handle_input(
            &mut ui,
            &InputEvent::PointerMoved {
                position: Point::new(start.x + 4.0, start.y),
            },
            &layout,
        )
        .unwrap();
    assert!(!source.tab_drag().unwrap().dragging);
    source
        .handle_input(
            &mut ui,
            &InputEvent::PointerMoved {
                position: Point::new(-50.0, 250.0),
            },
            &layout,
        )
        .unwrap();
    assert!(source.tab_drag().unwrap().dragging);
    let projection = target.drop_targets().unwrap().clone();
    assert_eq!(projection.window(), floating);
    assert_eq!(projection.bounds(), target.geometry().unwrap().bounds);
    let point = center(target.geometry().unwrap().groups[0].content);
    let preview = projection.target_at(point).unwrap();
    assert_eq!(Some(preview), target.drop_target_at(point));
    target.set_external_preview(Some(preview)).unwrap();
    source.set_external_preview(None).unwrap();
    assert!(
        target
            .overlay_scene()
            .rounded_rectangles
            .iter()
            .any(|item| item.color == Theme::FluentDark.tokens().focus)
    );
    assert_eq!(ui.runtime.captured(), Some(widget));
    assert_eq!(target_ui.runtime.captured(), None);
    assert_eq!(target_ui.runtime.focused(), None);
    assert_eq!(target.tab_drag(), None);
    assert_eq!(layout.snapshot(), before);
    let moving = source.finish_tab_drag(&mut ui).unwrap().unwrap();
    assert_eq!(moving, id);
    assert_eq!(ui.runtime.captured(), None);
    assert_eq!(source.tab_drag(), None);
    assert!(source.overlay_scene().commands().is_empty());
    assert_eq!(
        source
            .handle_input(&mut ui, &up(Point::new(-50.0, 250.0)), &layout)
            .unwrap(),
        (false, Vec::new())
    );
    assert_eq!(layout.snapshot(), before);
    layout.move_pane(&moving, preview.placement).unwrap();
    target.set_external_preview(None).unwrap();
    refresh(&mut ui, &layout, &mut source);
    target
        .update(&mut target_ui, &layout, projection.bounds())
        .unwrap();
    target_ui.update(Size::new(600.0, 400.0)).unwrap();
    assert_eq!(layout.window_for(&id), Some(floating));
    assert_eq!(source.tab_widget(&id), Some(widget));
    assert_eq!(target.tab_widget(&id), Some(target_widget));
    assert!(!source.tab_bounds.contains_key(&id));
    assert!(target.tab_bounds.contains_key(&id));
}

#[test]
fn finishing_pending_or_lost_tab_capture_never_commits_or_releases_another_capture() {
    let (mut ui, layout, mut view) = fixture();
    let start = center(view.tab_bounds[&pane("Search")]);
    let before = layout.snapshot();
    view.handle_input(&mut ui, &down(start), &layout).unwrap();
    assert_eq!(view.finish_tab_drag(&mut ui).unwrap(), None);
    assert_eq!(ui.runtime.captured(), None);
    assert!(!view.is_interacting());
    view.handle_input(&mut ui, &down(start), &layout).unwrap();
    view.handle_input(
        &mut ui,
        &InputEvent::PointerMoved {
            position: Point::new(start.x + 5.0, start.y),
        },
        &layout,
    )
    .unwrap();
    let other = view.tab_widget(&pane("Document")).unwrap();
    ui.runtime
        .apply(Mutation::CapturePointer(Some(other)))
        .unwrap();
    assert_eq!(view.finish_tab_drag(&mut ui).unwrap(), None);
    assert_eq!(ui.runtime.captured(), Some(other));
    assert!(!view.is_interacting());
    assert!(view.overlay_scene().commands().is_empty());
    ui.runtime.apply(Mutation::CapturePointer(None)).unwrap();
    let split = view.geometry().unwrap().splitters[0].clone();
    view.handle_input(&mut ui, &down(center(split.bounds)), &layout)
        .unwrap();
    let captured = ui.runtime.captured();
    assert!(captured.is_some());
    assert_eq!(view.tab_drag(), None);
    assert_eq!(view.finish_tab_drag(&mut ui).unwrap(), None);
    assert!(view.is_interacting());
    assert_eq!(ui.runtime.captured(), captured);
    assert_eq!(layout.snapshot(), before);
}

#[test]
fn external_preview_rejects_stale_geometry_and_clears_on_cancel_theme_and_layout_changes() {
    let (mut ui, layout, mut view) = fixture();
    let before = layout.snapshot();
    let point = center(view.geometry().unwrap().groups[0].content);
    let projection = view.drop_targets().unwrap();
    let preview = projection.target_at(point).unwrap();
    view.set_external_preview(Some(preview)).unwrap();
    let old_scene = view.overlay_scene().clone();
    let invalid = [
        DockDropPreview {
            bounds: Rect::new(f32::NAN, 0.0, 50.0, 50.0),
            ..preview
        },
        DockDropPreview {
            placement: DockPlacement::Tab {
                group: DockGroupId(999),
                index: 0,
            },
            ..preview
        },
        DockDropPreview {
            placement: DockPlacement::Tab {
                group: DockGroupId(1),
                index: 999,
            },
            ..preview
        },
        DockDropPreview {
            placement: DockPlacement::Root {
                window: DockWindowId(999),
                edge: DockEdge::Left,
            },
            ..preview
        },
    ];
    for invalid in invalid {
        assert!(view.set_external_preview(Some(invalid)).is_err());
        assert_eq!(view.drop_preview(), Some(preview));
        assert_eq!(view.overlay_scene(), &old_scene);
    }
    ui.set_theme(Theme::FluentDark).unwrap();
    refresh(&mut ui, &layout, &mut view);
    assert!(
        view.overlay_scene()
            .rounded_rectangles
            .iter()
            .any(|item| item.color == Theme::FluentDark.tokens().focus)
    );
    for event in [key(Key::Escape), InputEvent::WindowFocused(false)] {
        view.set_external_preview(Some(preview)).unwrap();
        let (_, requests) = view.handle_input(&mut ui, &event, &layout).unwrap();
        assert!(requests.is_empty());
        assert_eq!(view.drop_preview(), None);
        assert!(view.overlay_scene().commands().is_empty());
        assert_eq!(ui.runtime.captured(), None);
    }
    view.set_external_preview(Some(preview)).unwrap();
    view.update(&mut ui, &layout, Rect::new(8.0, 90.0, 500.0, 300.0))
        .unwrap();
    assert_eq!(view.drop_preview(), None);
    assert_eq!(projection.target_at(point), Some(preview));
    assert!(view.set_external_preview(Some(preview)).is_err());
    let start = center(view.tab_bounds[&pane("Search")]);
    view.handle_input(&mut ui, &down(start), &layout).unwrap();
    let moved = InputEvent::PointerMoved {
        position: Point::new(start.x + 10.0, start.y),
    };
    view.handle_input(&mut ui, &moved, &layout).unwrap();
    assert!(view.tab_drag().unwrap().dragging);
    ui.runtime.apply(Mutation::CapturePointer(None)).unwrap();
    assert!(
        view.handle_input(&mut ui, &moved, &layout)
            .unwrap()
            .1
            .is_empty()
    );
    assert_eq!(view.tab_drag(), None);
    assert!(view.overlay_scene().commands().is_empty());
    assert_eq!(layout.snapshot(), before);
}

#[test]
fn empty_main_accepts_a_foreign_pane_anywhere_and_invalid_points_have_no_target() {
    let (mut ui, mut layout, mut view) = fixture();
    for id in ["Document", "Search", "Properties"] {
        layout
            .float_pane(&pane(id), Rect::new(200.0, 200.0, 600.0, 400.0))
            .unwrap();
    }
    refresh(&mut ui, &layout, &mut view);
    let bounds = view.geometry().unwrap().bounds;
    let projection = view.drop_targets().unwrap();
    let expected = DockDropPreview {
        placement: DockPlacement::Root {
            window: DockWindowId::MAIN,
            edge: DockEdge::Right,
        },
        bounds,
    };
    for point in [
        center(bounds),
        Point::new(bounds.x + 1.0, bounds.y + 1.0),
        Point::new(
            bounds.x + bounds.width - 1.0,
            bounds.y + bounds.height - 1.0,
        ),
    ] {
        assert_eq!(projection.target_at(point), Some(expected));
        assert_eq!(view.drop_target_at(point), Some(expected));
    }
    for point in [
        Point::new(f32::NAN, 10.0),
        Point::new(10.0, f32::INFINITY),
        Point::new(bounds.x - 1.0, bounds.y),
    ] {
        assert_eq!(projection.target_at(point), None);
    }
    view.set_external_preview(Some(expected)).unwrap();
    layout
        .move_pane(&pane("Search"), expected.placement)
        .unwrap();
    refresh(&mut ui, &layout, &mut view);
    assert_eq!(layout.window_for(&pane("Search")), Some(DockWindowId::MAIN));
    assert_eq!(view.drop_preview(), None);
    view.update(&mut ui, &layout, Rect::new(0.0, 0.0, 0.0, 0.0))
        .unwrap();
    assert_eq!(view.drop_target_at(Point::ZERO), None);
}

#[test]
fn auto_hide_rail_retains_tabs_and_pin_restores_original_split() {
    let (mut ui, mut layout, mut view) = fixture();
    let id = pane("Properties");
    let widget = view.tab_widget(&id).unwrap();
    let original = layout.snapshot().root;
    layout
        .set_auto_hide(
            &id,
            Some(DockAutoHideSettings {
                edge: DockEdge::Right,
                extent: 280.0,
            }),
        )
        .unwrap();
    refresh(&mut ui, &layout, &mut view);
    assert_eq!(view.tab_widget(&id), Some(widget));
    assert_eq!(view.geometry().unwrap().groups.len(), 1);
    assert!(view.geometry().unwrap().auto_hide_panel.is_none());
    let point = center(ui.runtime.bounds(widget).unwrap());
    let (consumed, requests) = view.handle_input(&mut ui, &down(point), &layout).unwrap();
    assert!(consumed);
    apply(&mut layout, requests);
    view.handle_input(&mut ui, &up(point), &layout).unwrap();
    refresh(&mut ui, &layout, &mut view);
    let panel = view.geometry().unwrap().auto_hide_panel.as_ref().unwrap();
    assert_eq!(panel.pane, id);
    assert_eq!(view.tab_widget(&id), Some(widget));
    assert!(!view.auto_hide_background_scene().commands().is_empty());
    let pin = *view
        .actions
        .iter()
        .find(|(_, action)| {
            matches!(
                action,
                Action::Request(DockRequest::SetAutoHide { settings: None, .. })
            )
        })
        .unwrap()
        .0;
    let requests = view.process_activated(&[pin], &layout);
    apply(&mut layout, requests);
    refresh(&mut ui, &layout, &mut view);
    assert_eq!(layout.snapshot().root, original);
    assert_eq!(view.tab_widget(&id), Some(widget));
    assert!(view.geometry().unwrap().auto_hide_panel.is_none());
}

#[test]
fn auto_hide_caption_and_rail_drag_do_not_target_occluded_panes() {
    let (mut ui, mut layout, mut view) = fixture();
    let id = pane("Search");
    layout
        .set_auto_hide(
            &id,
            Some(DockAutoHideSettings {
                edge: DockEdge::Left,
                extent: 300.0,
            }),
        )
        .unwrap();
    layout.reveal_auto_hide(&id).unwrap();
    refresh(&mut ui, &layout, &mut view);
    let geometry = view.geometry().unwrap().clone();
    let panel = geometry.auto_hide_panel.as_ref().unwrap();
    assert!(view.drop_target_at(center(panel.content)).is_none());
    assert!(
        view.drop_targets()
            .unwrap()
            .target_at(center(geometry.auto_hide_rails[0].bounds))
            .is_none()
    );
    let caption = view.auto_hide_caption_widget().unwrap();
    let point = center(ui.runtime.bounds(caption).unwrap());
    let (consumed, _) = view.handle_input(&mut ui, &down(point), &layout).unwrap();
    assert!(consumed);
    assert_eq!(view.tab_drag().unwrap().widget, caption);
    view.handle_input(
        &mut ui,
        &InputEvent::PointerMoved {
            position: Point::new(point.x + 7.0, point.y),
        },
        &layout,
    )
    .unwrap();
    assert!(view.tab_drag().unwrap().dragging);
    assert_eq!(view.finish_tab_drag(&mut ui).unwrap(), Some(id.clone()));
    let rail = view.tab_widget(&id).unwrap();
    let point = center(ui.runtime.bounds(rail).unwrap());
    view.handle_input(&mut ui, &down(point), &layout).unwrap();
    assert_eq!(view.tab_drag().unwrap().widget, rail);
    refresh(&mut ui, &layout, &mut view);
    assert_eq!(ui.runtime.captured(), Some(rail));
    assert_eq!(ui.runtime.focused(), Some(rail));
    assert_eq!(view.tab_widget(&id), Some(rail));
}

#[test]
fn auto_hide_escape_cancels_composition_before_dismiss_and_outside_preserves_focus_policy() {
    let (mut ui, mut layout, mut view) = fixture();
    let id = pane("Properties");
    layout
        .set_auto_hide(&id, Some(DockAutoHideSettings::default()))
        .unwrap();
    layout.reveal_auto_hide(&id).unwrap();
    refresh(&mut ui, &layout, &mut view);
    let editor = ui
        .text_box(
            ui.root(),
            "Editor",
            "draft",
            LayoutStyle {
                width: Length::Px(150.0),
                height: Length::Px(32.0),
                ..Default::default()
            },
        )
        .unwrap();
    ui.update(Size::new(800.0, 600.0)).unwrap();
    ui.runtime.apply(Mutation::Focus(Some(editor))).unwrap();
    ui.ime_preedit("composition", None).unwrap();
    let (consumed, requests) = view
        .handle_input(&mut ui, &key(Key::Escape), &layout)
        .unwrap();
    assert!(consumed && requests.is_empty());
    assert!(ui.document(editor).unwrap().composition().is_none());
    assert_eq!(ui.document(editor).unwrap().text(), "draft");
    let (_, requests) = view
        .handle_input(&mut ui, &key(Key::Escape), &layout)
        .unwrap();
    assert_eq!(
        requests,
        vec![DockRequest::DismissAutoHide {
            window: DockWindowId::MAIN,
            restore_focus: true
        }]
    );
    let outside = Point::new(790.0, 580.0);
    let (_, requests) = view.handle_input(&mut ui, &down(outside), &layout).unwrap();
    assert!(requests.contains(&DockRequest::DismissAutoHide {
        window: DockWindowId::MAIN,
        restore_focus: false
    }));
    let (_, requests) = view
        .handle_input(&mut ui, &InputEvent::WindowFocused(false), &layout)
        .unwrap();
    assert!(requests.contains(&DockRequest::DismissAutoHide {
        window: DockWindowId::MAIN,
        restore_focus: false
    }));
}

#[test]
fn auto_hide_overflow_keyboard_reveals_last_tab_with_same_identity() {
    let (mut ui, mut layout, mut view) = fixture();
    for id in ["Document", "Search", "Properties"] {
        layout
            .set_auto_hide(
                &pane(id),
                Some(DockAutoHideSettings {
                    edge: DockEdge::Bottom,
                    extent: 180.0,
                }),
            )
            .unwrap();
    }
    view.update(&mut ui, &layout, Rect::new(0.0, 0.0, 150.0, 260.0))
        .unwrap();
    ui.update(Size::new(150.0, 260.0)).unwrap();
    let first = view.tab_widget(&pane("Document")).unwrap();
    let last = view.tab_widget(&pane("Properties")).unwrap();
    ui.runtime.apply(Mutation::Focus(Some(first))).unwrap();
    let (_, requests) = view.handle_input(&mut ui, &key(Key::End), &layout).unwrap();
    apply(&mut layout, requests);
    view.update(&mut ui, &layout, Rect::new(0.0, 0.0, 150.0, 260.0))
        .unwrap();
    ui.update(Size::new(150.0, 260.0)).unwrap();
    assert_eq!(
        layout.revealed_auto_hide(DockWindowId::MAIN),
        Some(&pane("Properties"))
    );
    assert_eq!(ui.runtime.focused(), Some(last));
    assert!(view.tab_bounds.contains_key(&pane("Properties")));
    assert!(!view.tab_bounds.contains_key(&pane("Document")));
    assert_eq!(view.tab_widget(&pane("Document")), Some(first));
}

#[test]
fn drop_insertion_indices_include_auto_hidden_tabs_in_original_model_order() {
    let (mut ui, mut layout, mut view) = fixture();
    layout
        .set_auto_hide(&pane("Document"), Some(DockAutoHideSettings::default()))
        .unwrap();
    refresh(&mut ui, &layout, &mut view);
    let group = view
        .geometry()
        .unwrap()
        .groups
        .iter()
        .find(|group| group.id == DockGroupId(1))
        .unwrap();
    assert_eq!(group.panes, vec![pane("Search")]);
    assert_eq!(group.insertion_slots, vec![1, 2]);
    let tab = view.tab_bounds[&pane("Search")];
    let target = view
        .drop_target_at(Point::new(tab.x + 1.0, tab.y + tab.height / 2.0))
        .unwrap();
    assert_eq!(
        target.placement,
        DockPlacement::Tab {
            group: DockGroupId(1),
            index: 1
        }
    );
    layout
        .move_pane(&pane("Properties"), target.placement)
        .unwrap();
    layout.set_auto_hide(&pane("Document"), None).unwrap();
    let geometry = layout
        .arrange(DockWindowId::MAIN, Rect::new(0.0, 0.0, 900.0, 600.0))
        .unwrap();
    assert_eq!(
        geometry.groups[0].panes,
        vec![pane("Document"), pane("Properties"), pane("Search")]
    );
}

#[test]
fn revealed_panel_content_and_header_occlude_pinned_splitter_input() {
    let (mut ui, mut layout, mut view) = fixture();
    layout
        .set_auto_hide(
            &pane("Search"),
            Some(DockAutoHideSettings {
                edge: DockEdge::Left,
                extent: 650.0,
            }),
        )
        .unwrap();
    layout.reveal_auto_hide(&pane("Search")).unwrap();
    refresh(&mut ui, &layout, &mut view);
    let geometry = view.geometry().unwrap().clone();
    let panel = geometry.auto_hide_panel.as_ref().unwrap();
    let split = &geometry.splitters[0];
    let position = Point::new(
        split.bounds.x + split.bounds.width / 2.0,
        panel.content.y + 20.0,
    );
    assert!(panel.content.contains(position.x, position.y));
    assert!(split.bounds.contains(position.x, position.y));
    let (consumed, requests) = view
        .handle_input(&mut ui, &down(position), &layout)
        .unwrap();
    assert!(!consumed);
    assert!(requests.is_empty());
    assert!(!view.is_interacting());
    assert!(ui.runtime.captured().is_none());
}
