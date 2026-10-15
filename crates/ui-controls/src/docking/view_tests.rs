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
