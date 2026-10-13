use crate::*;
use rust_desktop_ui_core::{
    InputEvent, Key, LayoutKind, LayoutStyle, Length, Modifiers, Mutation, Point, PointerButton,
    Size, WidgetId,
};

fn style() -> LayoutStyle {
    LayoutStyle {
        width: Length::Px(140.0),
        height: Length::Px(36.0),
        flex_shrink: 0.0,
        ..Default::default()
    }
}
fn overlay_root(ui: &mut Controls) {
    let mut style = ui
        .runtime
        .tree()
        .node(ui.root())
        .unwrap()
        .props
        .style
        .clone();
    style.kind = LayoutKind::Overlay;
    ui.runtime
        .apply(Mutation::SetStyle {
            node: ui.root(),
            style,
        })
        .unwrap();
}
fn frame(ui: &mut Controls) {
    ui.update(Size::new(640.0, 480.0)).unwrap();
}
fn key(ui: &mut Controls, key: Key) {
    ui.dispatch(InputEvent::KeyDown {
        key,
        modifiers: Modifiers::default(),
        repeat: false,
    })
    .unwrap();
}
fn center(ui: &Controls, id: WidgetId) -> Point {
    let b = ui.runtime.bounds(id).unwrap();
    Point::new(b.x + b.width * 0.5, b.y + b.height * 0.5)
}
fn click(ui: &mut Controls, id: WidgetId) {
    let position = center(ui, id);
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
}
fn entry(label: &str) -> MenuEntry {
    MenuEntry::Item(MenuItem::new(label, "action"))
}
fn nested() -> Vec<MenuEntry> {
    vec![
        MenuEntry::Item(MenuItem::new("Disabled", "action").with_enabled(false)),
        MenuEntry::Separator,
        MenuEntry::Submenu {
            label: "More".into(),
            enabled: true,
            items: vec![entry("Child A"), entry("Child B")],
        },
        entry("Last"),
    ]
}
fn setup() -> (Controls, WidgetId) {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    ui.commands_mut().register("action", "Action");
    let id = ui
        .dropdown_button(ui.root(), "Menu", &nested(), style())
        .unwrap();
    frame(&mut ui);
    (ui, id)
}
fn focused_label(ui: &Controls) -> String {
    ui.semantics()
        .into_iter()
        .find(|n| Some(n.id) == ui.runtime.focused())
        .unwrap()
        .label
}

#[test]
fn hierarchical_keyboard_skips_disabled_and_separators_and_escape_closes_one_level() {
    let (mut ui, anchor) = setup();
    ui.activate(anchor).unwrap();
    assert_eq!(focused_label(&ui), "More");
    assert_eq!(ui.popup_depth(), 1);
    let submenu = ui.runtime.focused().unwrap();
    assert_eq!(ui.popup_expanded(submenu), Some(false));
    key(&mut ui, Key::ArrowRight);
    assert_eq!(ui.popup_depth(), 2);
    assert_eq!(focused_label(&ui), "Child A");
    assert!(ui.drain_activated().is_empty());
    assert!(ui.commands_mut().drain_invocations().is_empty());
    key(&mut ui, Key::ArrowUp);
    assert_eq!(focused_label(&ui), "Child B");
    key(&mut ui, Key::Home);
    assert_eq!(focused_label(&ui), "Child A");
    key(&mut ui, Key::Escape);
    assert_eq!(ui.popup_depth(), 1);
    assert_eq!(ui.runtime.focused(), Some(submenu));
    key(&mut ui, Key::End);
    assert_eq!(focused_label(&ui), "Last");
    key(&mut ui, Key::Enter);
    assert!(!ui.popup_open());
    assert_eq!(ui.runtime.focused(), Some(anchor));
    assert_eq!(ui.commands_mut().drain_invocations().len(), 1);
    assert_eq!(ui.drain_activated().len(), 1);
}

#[test]
fn tab_closes_all_levels_and_traverses_from_anchor_in_both_directions() {
    let (mut ui, anchor) = setup();
    let next = ui.button(ui.root(), "Next", None, style()).unwrap();
    frame(&mut ui);
    for backward in [false, true] {
        ui.activate(anchor).unwrap();
        key(&mut ui, Key::ArrowRight);
        ui.dispatch(InputEvent::KeyDown {
            key: Key::Tab,
            modifiers: Modifiers {
                shift: backward,
                ..Default::default()
            },
            repeat: false,
        })
        .unwrap();
        assert!(!ui.popup_open());
        assert_eq!(ui.runtime.focused(), Some(next));
    }
}

#[test]
fn same_anchor_toggles_and_outside_click_executes_its_command_exactly_once() {
    let (mut ui, anchor) = setup();
    overlay_root(&mut ui);
    let outside = ui
        .button(ui.root(), "Outside", Some("action".into()), style())
        .unwrap();
    // The overlay starts below the anchor, so put the outside target above it.
    let mut layout = style();
    layout.offset = Point::new(350.0, 0.0);
    ui.runtime
        .apply(Mutation::SetStyle {
            node: outside,
            style: layout,
        })
        .unwrap();
    frame(&mut ui);
    frame(&mut ui);
    click(&mut ui, anchor);
    assert!(ui.popup_open());
    click(&mut ui, anchor);
    assert!(!ui.popup_open());
    click(&mut ui, anchor);
    assert!(ui.popup_open());
    click(&mut ui, outside);
    assert!(!ui.popup_open());
    assert_eq!(ui.runtime.focused(), Some(outside));
    assert_eq!(ui.drain_activated(), vec![outside]);
    assert_eq!(ui.commands_mut().drain_invocations().len(), 1);
}

#[test]
fn blur_closes_without_refocusing_inactive_window_and_new_focus_allows_reopening() {
    let (mut ui, anchor) = setup();
    ui.activate(anchor).unwrap();
    key(&mut ui, Key::ArrowRight);
    ui.dispatch(InputEvent::WindowFocused(false)).unwrap();
    assert!(!ui.popup_open());
    assert_eq!(ui.runtime.focused(), None);
    assert!(
        ui.popup_menu_entries(anchor, &[entry("Unavailable window")])
            .is_err()
    );
    ui.dispatch(InputEvent::WindowFocused(true)).unwrap();
    ui.activate(anchor).unwrap();
    assert!(ui.popup_open());
    ui.close_popup().unwrap();
    assert_eq!(ui.runtime.focused(), Some(anchor));
}

#[test]
fn hidden_disabled_and_removed_anchors_close_entire_chain_without_stale_restore() {
    for mode in 0..4 {
        let (mut ui, anchor) = setup();
        ui.activate(anchor).unwrap();
        key(&mut ui, Key::ArrowRight);
        match mode {
            0 => ui.set_enabled(anchor, false).unwrap(),
            1 => {
                let mut props = ui.runtime.tree().node(anchor).unwrap().props.clone();
                props.visible = false;
                ui.runtime
                    .apply(Mutation::SetProps {
                        node: anchor,
                        props,
                    })
                    .unwrap();
            }
            2 => ui.remove(anchor).unwrap(),
            _ => ui.runtime.apply(Mutation::Remove(anchor)).unwrap(),
        }
        frame(&mut ui);
        assert!(!ui.popup_open());
        assert_eq!(ui.runtime.focused(), None);
        assert!(ui.semantics().iter().all(|node| node.role != Role::Menu));
        ui.close_popup().unwrap();
        assert_eq!(ui.runtime.focused(), None);
    }
}

#[test]
fn disabling_submenu_closes_only_child_and_disabling_leaf_keeps_chain() {
    let (mut ui, anchor) = setup();
    ui.activate(anchor).unwrap();
    let submenu = ui.runtime.focused().unwrap();
    key(&mut ui, Key::ArrowRight);
    ui.commands_mut().set_enabled(&"action".into(), false);
    frame(&mut ui);
    assert_eq!(ui.popup_depth(), 2);
    ui.set_enabled(submenu, false).unwrap();
    assert_eq!(ui.popup_depth(), 1);
    key(&mut ui, Key::Escape);
    assert_eq!(ui.runtime.focused(), Some(anchor));
}

#[test]
fn malformed_replacement_preserves_tree_focus_scene_and_expansion() {
    let (mut ui, anchor) = setup();
    ui.activate(anchor).unwrap();
    frame(&mut ui);
    let ids = ui.runtime.tree().preorder();
    let focus = ui.runtime.focused();
    let scene = ui.scene().clone();
    let mut too_deep = vec![entry("Leaf")];
    for _ in 0..MAX_MENU_DEPTH {
        too_deep = vec![MenuEntry::Submenu {
            label: "Deeper".into(),
            items: too_deep,
            enabled: true,
        }];
    }
    let huge = vec![MenuEntry::Separator; MAX_MENU_ITEMS + 1];
    let long = vec![entry(&"x".repeat(1024 * 1024))];
    for bad in [&[][..], &too_deep, &huge, &long] {
        assert!(ui.popup_menu_entries(anchor, bad).is_err());
        assert_eq!(ui.runtime.tree().preorder(), ids);
        assert_eq!(ui.runtime.focused(), focus);
        assert_eq!(ui.scene(), &scene);
        assert_eq!(ui.popup_expanded(anchor), Some(true));
    }
    let panel = ui
        .semantics()
        .iter()
        .find(|n| n.role == Role::Menu)
        .unwrap()
        .id;
    assert!(
        ui.popup_menu_entries(panel, &[entry("Invalid anchor")])
            .is_err()
    );
    assert_eq!(ui.runtime.tree().preorder(), ids);
    assert_eq!(ui.runtime.focused(), focus);
}

#[test]
fn menu_builders_reject_before_inserting_and_limits_include_active_parent_levels() {
    let (mut ui, anchor) = setup();
    let ids = ui.runtime.tree().preorder();
    assert!(ui.dropdown_button(ui.root(), "Bad", &[], style()).is_err());
    let mut invalid = style();
    invalid.gap = f32::NAN;
    assert!(
        ui.split_button(ui.root(), "Bad", None, &[entry("Leaf")], invalid)
            .is_err()
    );
    assert_eq!(ui.runtime.tree().preorder(), ids);
    ui.activate(anchor).unwrap();
    let submenu = ui.runtime.focused().unwrap();
    let mut too_deep = vec![entry("Leaf")];
    for _ in 1..MAX_MENU_DEPTH {
        too_deep = vec![MenuEntry::Submenu {
            label: "D".into(),
            items: too_deep,
            enabled: true,
        }];
    }
    assert!(ui.popup_menu_entries(submenu, &too_deep).is_err());
    assert!(
        ui.popup_menu_entries(submenu, &vec![MenuEntry::Separator; MAX_MENU_ITEMS])
            .is_err()
    );
    assert_eq!(ui.popup_depth(), 1);
    assert_eq!(ui.runtime.focused(), Some(submenu));
}

#[test]
fn split_primary_and_arrow_have_independent_gates_actions_and_accessible_names() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    ui.commands_mut().register("action", "Action");
    ui.commands_mut().register("primary", "Primary");
    let parts = ui
        .split_button(
            ui.root(),
            "Save",
            Some("primary".into()),
            &[entry("Save copy")],
            style(),
        )
        .unwrap();
    ui.commands_mut().set_enabled(&"primary".into(), false);
    frame(&mut ui);
    assert!(!ui.is_enabled(parts.primary));
    assert!(ui.is_enabled(parts.dropdown));
    let nodes = ui.semantics();
    assert_eq!(
        nodes.iter().find(|n| n.id == parts.container).unwrap().role,
        Role::Group
    );
    let arrow = nodes.iter().find(|n| n.id == parts.dropdown).unwrap();
    assert!(!arrow.label.is_empty());
    assert_eq!(arrow.expanded, Some(false));
    assert!(arrow.actions.contains(&SemanticAction::Expand));
    ui.activate(parts.primary).unwrap();
    assert!(!ui.popup_open());
    ui.perform_action(parts.dropdown, SemanticAction::Expand, None)
        .unwrap();
    assert!(ui.popup_open());
    assert!(ui.commands_mut().drain_invocations().is_empty());
    ui.perform_action(parts.dropdown, SemanticAction::Collapse, None)
        .unwrap();
    assert_eq!(ui.runtime.focused(), Some(parts.dropdown));
    ui.commands_mut().set_enabled(&"primary".into(), true);
    ui.activate(parts.primary).unwrap();
    assert!(!ui.popup_open());
    assert_eq!(ui.commands_mut().drain_invocations().len(), 1);
    ui.set_enabled(parts.container, false).unwrap();
    ui.expand_popup(parts.dropdown).unwrap();
    assert!(!ui.popup_open());
}

#[test]
fn placement_flips_clamps_scrolls_long_menus_and_idle_update_does_no_layout() {
    let (mut ui, anchor) = setup();
    overlay_root(&mut ui);
    let mut layout = style();
    layout.offset = Point::new(470.0, 400.0);
    ui.runtime
        .apply(Mutation::SetStyle {
            node: anchor,
            style: layout,
        })
        .unwrap();
    frame(&mut ui);
    let entries = (0..40)
        .map(|i| entry(&format!("Entry {i}")))
        .collect::<Vec<_>>();
    let menu = ui.popup_menu_entries(anchor, &entries).unwrap();
    frame(&mut ui);
    let bounds = ui.runtime.bounds(menu).unwrap();
    assert!(
        bounds.x >= 0.0
            && bounds.y >= 0.0
            && bounds.x + bounds.width <= 640.0
            && bounds.y + bounds.height <= 480.0
    );
    key(&mut ui, Key::End);
    frame(&mut ui);
    assert_eq!(focused_label(&ui), "Entry 39");
    let selected = ui.runtime.bounds(ui.runtime.focused().unwrap()).unwrap();
    assert!(
        selected.y >= bounds.y && selected.y + selected.height <= bounds.y + bounds.height + 0.01
    );
    let delta = ui.update(Size::new(640.0, 480.0)).unwrap();
    assert_eq!(delta.layout_passes, 0);
    assert_eq!(delta.paint_passes, 0);
    ui.update(Size::new(120.0, 90.0)).unwrap();
    // Anchor moved outside the viewport is no longer a valid overlay owner.
    assert!(!ui.popup_open());
}

#[test]
fn submenu_flips_left_at_right_edge_and_scene_chevrons_survive_custom_button_icon() {
    let (mut ui, anchor) = setup();
    overlay_root(&mut ui);
    let mut layout = style();
    layout.offset = Point::new(450.0, 20.0);
    ui.runtime
        .apply(Mutation::SetStyle {
            node: anchor,
            style: layout,
        })
        .unwrap();
    ui.set_button_icon(anchor, Some(ButtonIcon::new(IconSymbol::Add)))
        .unwrap();
    frame(&mut ui);
    assert!(ui.scene().stroke_paths.len() >= 2);
    ui.activate(anchor).unwrap();
    let parent = ui
        .semantics()
        .into_iter()
        .find(|n| n.role == Role::Menu)
        .unwrap();
    key(&mut ui, Key::ArrowRight);
    frame(&mut ui);
    let child = ui
        .semantics()
        .into_iter()
        .find(|n| n.role == Role::Menu && n.id != parent.id)
        .unwrap();
    assert!(child.bounds.x < parent.bounds.x);
    assert!(child.bounds.x >= 0.0 && child.bounds.y >= 0.0);
    assert!(ui.scene().stroke_paths.iter().all(|path| path.is_valid()));
}

#[test]
fn separator_only_menu_keeps_focus_and_recycled_arrow_does_not_open_old_entries() {
    let mut ui = Controls::new(Theme::FluentDark).unwrap();
    let anchor = ui
        .dropdown_button(ui.root(), "Empty actions", &[MenuEntry::Separator], style())
        .unwrap();
    frame(&mut ui);
    ui.activate(anchor).unwrap();
    assert_eq!(ui.popup_depth(), 1);
    assert_eq!(
        ui.semantics()
            .iter()
            .find(|n| Some(n.id) == ui.runtime.focused())
            .unwrap()
            .role,
        Role::Menu
    );
    key(&mut ui, Key::ArrowDown);
    key(&mut ui, Key::Escape);
    assert_eq!(ui.runtime.focused(), Some(anchor));
    ui.remove(anchor).unwrap();
    let replacement = ui.button(ui.root(), "Replacement", None, style()).unwrap();
    frame(&mut ui);
    assert_ne!(replacement, anchor);
    assert_eq!(ui.popup_expanded(anchor), None);
    ui.expand_popup(anchor).unwrap();
    assert!(!ui.popup_open());
    assert_eq!(ui.popup_expanded(replacement), None);
}

#[test]
fn theme_change_reflows_open_rows_preserving_selection_and_idle_geometry() {
    let (mut ui, anchor) = setup();
    ui.activate(anchor).unwrap();
    let selected = ui.runtime.focused().unwrap();
    let old_height = ui.runtime.bounds(selected).unwrap().height;
    ui.set_theme(Theme::Compact).unwrap();
    frame(&mut ui);
    assert_eq!(ui.runtime.focused(), Some(selected));
    assert_eq!(
        ui.runtime.bounds(selected).unwrap().height,
        Theme::Compact.tokens().control_height
    );
    assert_ne!(old_height, ui.runtime.bounds(selected).unwrap().height);
    key(&mut ui, Key::ArrowRight);
    let child = ui.runtime.focused().unwrap();
    assert_eq!(
        ui.runtime.bounds(child).unwrap().height,
        Theme::Compact.tokens().control_height
    );
    ui.set_theme(Theme::FluentDark).unwrap();
    frame(&mut ui);
    assert_eq!(ui.runtime.focused(), Some(child));
    assert_eq!(
        ui.runtime.bounds(child).unwrap().height,
        Theme::FluentDark.tokens().control_height
    );
    let delta = ui.update(Size::new(640.0, 480.0)).unwrap();
    assert_eq!(delta.layout_passes, 0);
    assert_eq!(delta.paint_passes, 0);
}

#[test]
fn flat_menu_exposes_expansion_and_disabled_anchor_rejects_leaf_before_frame() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    ui.commands_mut().register("owner", "Owner");
    ui.commands_mut().register("action", "Action");
    let anchor = ui
        .button(ui.root(), "Anchor", Some("owner".into()), style())
        .unwrap();
    frame(&mut ui);
    ui.popup_menu(anchor, &[MenuItem::new("Action", "action")])
        .unwrap();
    ui.perform_action(anchor, SemanticAction::Collapse, None)
        .unwrap();
    assert!(!ui.popup_open());
    assert_eq!(ui.runtime.focused(), Some(anchor));
    ui.popup_menu(anchor, &[MenuItem::new("Action", "action")])
        .unwrap();
    let leaf = ui.runtime.focused().unwrap();
    let semantic = ui.semantics().into_iter().find(|n| n.id == anchor).unwrap();
    assert_eq!(semantic.expanded, Some(true));
    assert!(semantic.has_popup);
    ui.commands_mut().set_enabled(&"owner".into(), false);
    ui.activate(leaf).unwrap();
    assert!(!ui.popup_open());
    assert_eq!(ui.runtime.focused(), None);
    assert!(ui.commands_mut().drain_invocations().is_empty());
    assert!(ui.drain_activated().is_empty());
    assert_eq!(ui.popup_expanded(anchor), None);
}
