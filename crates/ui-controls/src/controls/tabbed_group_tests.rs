use super::*;
use rust_desktop_ui_core::Modifiers;

fn setup() -> (Controls, TabbedGroup) {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let mut disabled = GroupTab::new("Disabled");
    disabled.enabled = false;
    let group = ui
        .tabbed_group(
            ui.root(),
            vec![GroupTab::new("First"), disabled, GroupTab::new("Last")],
            0,
            LayoutStyle {
                width: Length::Px(420.0),
                ..Default::default()
            },
        )
        .unwrap();
    (ui, group)
}
fn frame(ui: &mut Controls) {
    ui.update(Size::new(800.0, 700.0)).unwrap();
}
fn key(ui: &mut Controls, key: Key) {
    ui.dispatch(InputEvent::KeyDown {
        key,
        modifiers: Modifiers::default(),
        repeat: false,
    })
    .unwrap();
}

#[test]
fn selection_collapses_layout_and_preserves_retained_ids() {
    let (mut ui, group) = setup();
    ui.label(
        group.contents[0],
        "Short",
        LayoutStyle {
            height: Length::Px(30.0),
            ..Default::default()
        },
    )
    .unwrap();
    ui.label(
        group.contents[2],
        "Tall",
        LayoutStyle {
            height: Length::Px(160.0),
            ..Default::default()
        },
    )
    .unwrap();
    frame(&mut ui);
    let ids = ui.runtime.tree().preorder();
    assert_eq!(ui.runtime.bounds(group.headers[0]).unwrap().height, 48.0);
    assert_eq!(ui.runtime.bounds(group.container).unwrap().height, 110.0);
    assert!(ui.runtime.bounds(group.contents[2]).is_none());
    ui.set_selected_tab(group.headers[0], 2).unwrap();
    frame(&mut ui);
    assert_eq!(ui.runtime.tree().preorder(), ids);
    assert_eq!(ui.runtime.bounds(group.container).unwrap().height, 240.0);
    assert!(ui.runtime.bounds(group.contents[0]).is_none());
    assert_eq!(ui.selected_tab(group.headers[2]), Some(2));
    assert!(
        ui.semantics()
            .iter()
            .any(|n| n.id == group.headers[2] && n.role == Role::Tab && n.selected)
    );
}

#[test]
fn selection_preserves_draft_accepted_selection_and_undo_but_cancels_preedit() {
    let (mut ui, group) = setup();
    let editor = ui
        .edit_box(group.contents[0], "Name", "Anna", LayoutStyle::default())
        .unwrap();
    frame(&mut ui);
    ui.runtime.apply(Mutation::Focus(Some(editor))).unwrap();
    ui.text_input(" draft").unwrap();
    ui.ime_preedit("候補", Some((0, "候補".len()))).unwrap();
    let draft = ui.document(editor).unwrap().text().to_owned();
    let selection = ui.document(editor).unwrap().selection();
    ui.drain_edit_events();
    ui.set_selected_tab(group.container, 2).unwrap();
    assert_eq!(ui.runtime.focused(), Some(group.headers[2]));
    assert_eq!(ui.accepted_value(editor), Some("Anna"));
    assert!(ui.drain_edit_events().is_empty());
    ui.set_selected_tab(group.container, 0).unwrap();
    frame(&mut ui);
    assert_eq!(ui.document(editor).unwrap().text(), draft);
    assert_eq!(ui.document(editor).unwrap().selection(), selection);
    assert!(ui.document(editor).unwrap().composition().is_none());
    ui.runtime.apply(Mutation::Focus(Some(editor))).unwrap();
    ui.dispatch(InputEvent::KeyDown {
        key: Key::Character("z".into()),
        modifiers: Modifiers {
            control: true,
            ..Default::default()
        },
        repeat: false,
    })
    .unwrap();
    assert_eq!(ui.document(editor).unwrap().text(), "Anna");
}

#[test]
fn headers_have_one_roving_stop_and_navigation_skips_disabled_pages() {
    let (mut ui, group) = setup();
    frame(&mut ui);
    key(&mut ui, Key::Tab);
    assert_eq!(ui.runtime.focused(), Some(group.headers[0]));
    key(&mut ui, Key::ArrowRight);
    assert_eq!(ui.selected_tab(group.container), Some(2));
    assert_eq!(ui.runtime.focused(), Some(group.headers[2]));
    assert_eq!(
        group
            .headers
            .iter()
            .filter(|&&id| ui.runtime.tree().node(id).unwrap().props.tab_stop)
            .count(),
        1
    );
    key(&mut ui, Key::ArrowRight);
    assert_eq!(ui.selected_tab(group.container), Some(0));
    key(&mut ui, Key::End);
    assert_eq!(ui.selected_tab(group.container), Some(2));
    key(&mut ui, Key::Home);
    assert_eq!(ui.selected_tab(group.container), Some(0));
    ui.dispatch(InputEvent::KeyDown {
        key: Key::ArrowRight,
        modifiers: Modifiers {
            control: true,
            ..Default::default()
        },
        repeat: false,
    })
    .unwrap();
    assert_eq!(ui.selected_tab(group.container), Some(0));
}

#[test]
fn invalid_construction_and_selection_are_atomic_and_noop_keeps_frame_idle() {
    let (mut ui, group) = setup();
    let ids = ui.runtime.tree().preorder();
    assert!(
        ui.tabbed_group(ui.root(), Vec::new(), 0, LayoutStyle::default())
            .is_err()
    );
    for header in ["", "  \t\n"] {
        assert!(
            ui.tabbed_group(
                ui.root(),
                vec![GroupTab::new(header)],
                0,
                LayoutStyle::default()
            )
            .is_err()
        );
    }
    assert!(
        ui.tabbed_group(
            ui.root(),
            vec![GroupTab::new("x".repeat(4097))],
            0,
            LayoutStyle::default()
        )
        .is_err()
    );
    assert!(
        ui.tabbed_group(
            ui.root(),
            vec![GroupTab::new("x"); 65],
            0,
            LayoutStyle::default()
        )
        .is_err()
    );
    assert_eq!(ui.runtime.tree().preorder(), ids);
    for index in [1, 3, usize::MAX] {
        assert!(ui.set_selected_tab(group.container, index).is_err());
    }
    assert_eq!(ui.selected_tab(group.container), Some(0));
    frame(&mut ui);
    ui.set_selected_tab(group.container, 0).unwrap();
    let delta = ui.update(Size::new(800.0, 700.0)).unwrap();
    assert_eq!(delta.layout_passes, 0);
    assert_eq!(delta.paint_passes, 0);
}

#[test]
fn required_parts_cannot_be_removed_and_owner_removal_cleans_state() {
    let (mut ui, group) = setup();
    let ids = ui.runtime.tree().preorder();
    let state = &ui.tabs[&group.container];
    let required: Vec<_> = [state.header, state.content]
        .into_iter()
        .chain(group.headers.clone())
        .chain(group.contents.clone())
        .collect();
    for id in required {
        assert!(ui.remove(id).is_err());
        assert_eq!(ui.runtime.tree().preorder(), ids);
    }
    ui.remove(group.container).unwrap();
    assert_eq!(ui.selected_tab(group.container), None);
    assert_eq!(ui.selected_tab(group.headers[0]), None);
    assert!(ui.tabs.is_empty());
}

#[test]
fn programmatic_selection_preserves_external_focus_and_closes_owned_menu() {
    let (mut ui, group) = setup();
    let outside = ui
        .button(ui.root(), "Outside", None, LayoutStyle::default())
        .unwrap();
    ui.commands_mut().register("action", "Action");
    let dropdown = ui
        .dropdown_button(
            group.contents[0],
            "Menu",
            &[MenuEntry::Item(MenuItem::new("Action", "action"))],
            LayoutStyle::default(),
        )
        .unwrap();
    frame(&mut ui);
    ui.expand_popup(dropdown).unwrap();
    assert_eq!(ui.popup_expanded(dropdown), Some(true));
    ui.set_selected_tab(group.container, 2).unwrap();
    assert_eq!(ui.popup_expanded(dropdown), Some(false));
    assert_eq!(ui.runtime.focused(), Some(group.headers[2]));
    ui.runtime.apply(Mutation::Focus(Some(outside))).unwrap();
    ui.set_selected_tab(group.container, 0).unwrap();
    assert_eq!(ui.runtime.focused(), Some(outside));
}

#[test]
fn pointer_switch_commits_valid_draft_once_and_exposes_selected_semantics() {
    let (mut ui, group) = setup();
    let editor = ui
        .edit_box(group.contents[0], "Name", "Anna", LayoutStyle::default())
        .unwrap();
    frame(&mut ui);
    ui.runtime.apply(Mutation::Focus(Some(editor))).unwrap();
    ui.text_input(" draft").unwrap();
    let draft = ui.document(editor).unwrap().text().to_owned();
    ui.drain_edit_events();
    let bounds = ui.runtime.bounds(group.headers[2]).unwrap();
    let position = Point::new(bounds.x + 12.0, bounds.y + 12.0);
    ui.dispatch(InputEvent::PointerDown {
        position,
        button: PointerButton::Primary,
        modifiers: Modifiers::default(),
    })
    .unwrap();
    ui.dispatch(InputEvent::PointerUp {
        position,
        button: PointerButton::Primary,
        modifiers: Modifiers::default(),
    })
    .unwrap();
    frame(&mut ui);
    assert_eq!(ui.accepted_value(editor), Some(draft.as_str()));
    assert_eq!(ui.drain_edit_events().len(), 1);
    assert_eq!(ui.drain_activated(), vec![group.headers[2]]);
    assert_eq!(ui.selected_tab(group.container), Some(2));
    assert!(!ui.semantics().iter().any(|node| node.id == editor));
}

#[test]
fn changing_page_releases_old_selection_capture_and_pressed_state() {
    let (mut ui, group) = setup();
    let editor = ui
        .edit_box(group.contents[0], "Name", "Anna", LayoutStyle::default())
        .unwrap();
    frame(&mut ui);
    let bounds = ui.runtime.bounds(editor).unwrap();
    let position = Point::new(bounds.x + 12.0, bounds.y + 12.0);
    ui.dispatch(InputEvent::PointerDown {
        position,
        button: PointerButton::Primary,
        modifiers: Modifiers::default(),
    })
    .unwrap();
    assert_eq!(ui.runtime.captured(), Some(editor));
    ui.set_selected_tab(group.container, 2).unwrap();
    assert_eq!(ui.runtime.captured(), None);
    assert_eq!(ui.selecting, None);
    assert_eq!(ui.pressed, None);
    ui.dispatch(InputEvent::PointerMoved {
        position: Point::new(790.0, 690.0),
    })
    .unwrap();
    assert_eq!(ui.runtime.focused(), Some(group.headers[2]));
}

#[test]
fn narrow_tab_icons_are_clipped_to_their_own_header_in_both_themes() {
    for theme in [Theme::FluentLight, Theme::FluentDark] {
        let mut ui = Controls::new(theme).unwrap();
        let tabs = ui
            .tabbed_group(
                ui.root(),
                vec![
                    GroupTab {
                        header: "One".into(),
                        icon: Some(IconSymbol::Edit),
                        enabled: true,
                    },
                    GroupTab::new("Two"),
                ],
                0,
                LayoutStyle {
                    width: Length::Px(40.0),
                    ..Default::default()
                },
            )
            .unwrap();
        frame(&mut ui);
        let bounds = ui.runtime.bounds(tabs.headers[0]).unwrap();
        assert!(!ui.scene().stroke_paths.is_empty());
        let overflow = ui
            .runtime
            .bounds(ui.tab_overflow_button(tabs.container).unwrap())
            .unwrap();
        for path in &ui.scene().stroke_paths {
            let bounds = if path.clip.x >= overflow.x {
                overflow
            } else {
                bounds
            };
            assert!(path.clip.x >= bounds.x);
            assert!(path.clip.x + path.clip.width <= bounds.x + bounds.width);
            assert!(path.clip.y >= bounds.y);
            assert!(path.clip.y + path.clip.height <= bounds.y + bounds.height);
        }
    }
}

#[test]
fn ordinary_tab_leaves_headers_even_when_semantics_focused_an_unselected_tab() {
    let (mut ui, group) = setup();
    let outside = ui
        .button(ui.root(), "Outside", None, LayoutStyle::default())
        .unwrap();
    frame(&mut ui);
    ui.perform_action(group.headers[2], SemanticAction::Focus, None)
        .unwrap();
    assert_eq!(ui.selected_tab(group.container), Some(0));
    ui.dispatch(InputEvent::KeyDown {
        key: Key::Tab,
        modifiers: Modifiers {
            shift: true,
            ..Default::default()
        },
        repeat: false,
    })
    .unwrap();
    assert_eq!(ui.runtime.focused(), Some(outside));
    assert!(
        ui.runtime
            .tree()
            .node(group.headers[0])
            .unwrap()
            .props
            .tab_stop
    );
    assert!(
        !ui.runtime
            .tree()
            .node(group.headers[2])
            .unwrap()
            .props
            .tab_stop
    );
}

#[test]
fn model_selection_cancels_an_old_header_press_before_pointer_release() {
    let (mut ui, group) = setup();
    frame(&mut ui);
    let bounds = ui.runtime.bounds(group.headers[0]).unwrap();
    let position = Point::new(bounds.x + 12.0, bounds.y + 12.0);
    ui.dispatch(InputEvent::PointerDown {
        position,
        button: PointerButton::Primary,
        modifiers: Modifiers::default(),
    })
    .unwrap();
    assert_eq!(ui.runtime.captured(), Some(group.headers[0]));
    ui.set_selected_tab(group.container, 2).unwrap();
    assert_eq!(ui.runtime.captured(), None);
    assert_eq!(ui.pressed, None);
    ui.dispatch(InputEvent::PointerUp {
        position,
        button: PointerButton::Primary,
        modifiers: Modifiers::default(),
    })
    .unwrap();
    assert_eq!(ui.selected_tab(group.container), Some(2));
    assert!(ui.drain_activated().is_empty());
}

#[test]
fn legacy_tab_control_keeps_overlay_viewport_when_caller_replaces_page_layout() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let tabs = ui
        .tab_control(
            ui.root(),
            &["Grid", "Other"],
            LayoutStyle {
                width: Length::Px(420.0),
                height: Length::Px(300.0),
                padding: Edges::all(10.0),
                gap: 9.0,
                ..Default::default()
            },
        )
        .unwrap();
    // Legacy callers replace the whole page style without supplying flex_grow:
    // the page belongs to an Overlay viewport and must still fill it.
    ui.runtime
        .apply(Mutation::SetStyle {
            node: tabs.pages[0],
            style: LayoutStyle {
                kind: LayoutKind::Flex(Axis::Vertical),
                padding: Edges::all(4.0),
                gap: 8.0,
                ..Default::default()
            },
        })
        .unwrap();
    ui.label(
        tabs.pages[0],
        "Toolbar",
        LayoutStyle {
            height: Length::Px(34.0),
            flex_shrink: 0.0,
            ..Default::default()
        },
    )
    .unwrap();
    let viewport = ui
        .panel(
            tabs.pages[0],
            LayoutStyle {
                kind: LayoutKind::Overlay,
                height: Length::Px(0.0),
                flex_grow: 1.0,
                ..Default::default()
            },
        )
        .unwrap();
    ui.label(
        tabs.pages[0],
        "Status",
        LayoutStyle {
            height: Length::Px(26.0),
            flex_shrink: 0.0,
            ..Default::default()
        },
    )
    .unwrap();
    frame(&mut ui);
    let container_style = &ui.runtime.tree().node(tabs.container).unwrap().props.style;
    assert_eq!(container_style.padding, Edges::all(10.0));
    assert_eq!(container_style.gap, 9.0);
    assert_eq!(
        ui.runtime.bounds(tabs.tabs[0]).unwrap().height,
        ui.theme.tokens().control_height
    );
    let page_bounds = ui.runtime.bounds(tabs.pages[0]).unwrap();
    let host = ui.runtime.tree().parent(tabs.pages[0]).unwrap();
    assert_eq!(page_bounds, ui.runtime.bounds(host).unwrap());
    let grid_height = ui.runtime.bounds(viewport).unwrap().height;
    assert!(grid_height > 100.0);
    ui.select_tab(tabs.container, 1).unwrap();
    frame(&mut ui);
    assert!(ui.runtime.bounds(viewport).is_none());
    ui.select_tab(tabs.container, 0).unwrap();
    frame(&mut ui);
    assert_eq!(ui.runtime.bounds(viewport).unwrap().height, grid_height);
}
