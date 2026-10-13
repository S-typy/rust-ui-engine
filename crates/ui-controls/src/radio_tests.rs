use crate::*;
use rust_desktop_ui_core::{
    Axis, Color, DrawCommand, Edges, InputEvent, Key, LayoutKind, LayoutStyle, Length, Modifiers,
    Mutation, Point, PointerButton, Size, WidgetId,
};
use std::{cell::Cell, collections::HashSet, rc::Rc};

fn layout(width: f32, height: f32) -> LayoutStyle {
    LayoutStyle {
        width: Length::Px(width),
        height: Length::Px(height),
        flex_shrink: 0.0,
        ..Default::default()
    }
}

fn frame(ui: &mut Controls) {
    ui.update(Size::new(600.0, 480.0)).unwrap();
}

fn radio(
    ui: &mut Controls,
    parent: WidgetId,
    name: &str,
    group: RadioGroupId,
    checked: bool,
) -> WidgetId {
    ui.radio_button(parent, name, group, checked, layout(180.0, 32.0))
        .unwrap()
}

fn focus(ui: &mut Controls, id: WidgetId) {
    ui.runtime.apply(Mutation::Focus(Some(id))).unwrap();
}

fn key(ui: &mut Controls, key: Key, modifiers: Modifiers) {
    ui.dispatch(InputEvent::KeyDown {
        key,
        modifiers,
        repeat: false,
    })
    .unwrap();
}

fn key_up(ui: &mut Controls, key: Key) {
    ui.dispatch(InputEvent::KeyUp {
        key,
        modifiers: Modifiers::default(),
    })
    .unwrap();
}

fn tab(ui: &mut Controls, reverse: bool) {
    key(
        ui,
        Key::Tab,
        Modifiers {
            shift: reverse,
            ..Default::default()
        },
    );
}

fn center(ui: &Controls, id: WidgetId) -> Point {
    let bounds = ui.runtime.bounds(id).unwrap();
    Point::new(
        bounds.x + bounds.width * 0.5,
        bounds.y + bounds.height * 0.5,
    )
}

fn pointer(ui: &mut Controls, position: Point, down: bool) {
    let event = if down {
        InputEvent::PointerDown {
            position,
            button: PointerButton::Primary,
            modifiers: Modifiers::default(),
        }
    } else {
        InputEvent::PointerUp {
            position,
            button: PointerButton::Primary,
            modifiers: Modifiers::default(),
        }
    };
    ui.dispatch(event).unwrap();
}

fn visible(ui: &mut Controls, id: WidgetId, value: bool) {
    let mut props = ui.runtime.tree().node(id).unwrap().props.clone();
    props.visible = value;
    ui.runtime
        .apply(Mutation::SetProps { node: id, props })
        .unwrap();
}

fn assert_checked(ui: &Controls, values: &[(WidgetId, bool)]) {
    for &(id, checked) in values {
        assert_eq!(ui.checked(id), Some(checked));
        assert_eq!(ui.button_visual_state(id).unwrap().checked, checked);
        if let Some(node) = ui.semantics().iter().find(|node| node.id == id) {
            assert_eq!(node.checked, Some(checked));
        }
    }
}

#[test]
fn radio_groups_are_owned_distinct_and_reject_foreign_handles_before_insertion() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let mut other = Controls::new(Theme::FluentLight).unwrap();
    let first = ui.radio_group();
    let second = ui.radio_group();
    let foreign = other.radio_group();
    assert_eq!(HashSet::from([first, second, foreign]).len(), 3);
    let root = ui.root();
    let id = radio(&mut ui, root, "Existing", first, true);
    frame(&mut ui);
    let count = ui.runtime.tree().len();
    let scene = ui.scene().clone();
    assert!(
        ui.radio_button(root, "Foreign", foreign, true, layout(180.0, 32.0))
            .is_err()
    );
    assert_eq!(ui.runtime.tree().len(), count);
    assert_eq!(ui.radio_group_of(id), Some(first));
    assert_eq!(ui.radio_group_of(root), None);
    assert_eq!(ui.scene(), &scene);
    assert_checked(&ui, &[(id, true)]);
    assert!(ui.drain_activated().is_empty());
}

#[test]
fn radio_initial_last_checked_wins_and_group_updates_are_immediately_consistent() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let group = ui.radio_group();
    let separate = ui.radio_group();
    let root = ui.root();
    let first = radio(&mut ui, root, "First", group, true);
    let second = radio(&mut ui, root, "Second", group, true);
    let third = radio(&mut ui, root, "Third", group, false);
    let independent = radio(&mut ui, root, "Independent", separate, true);
    assert_checked(
        &ui,
        &[
            (first, false),
            (second, true),
            (third, false),
            (independent, true),
        ],
    );
    ui.set_checked(first, true).unwrap();
    assert_checked(
        &ui,
        &[
            (first, true),
            (second, false),
            (third, false),
            (independent, true),
        ],
    );
    ui.set_checked(first, false).unwrap();
    assert_checked(
        &ui,
        &[
            (first, false),
            (second, false),
            (third, false),
            (independent, true),
        ],
    );
    assert!(ui.drain_activated().is_empty());
}

#[test]
fn selecting_a_radio_unchecks_hidden_and_disabled_peers_without_requiring_a_frame() {
    let mut ui = Controls::new(Theme::FluentDark).unwrap();
    let group = ui.radio_group();
    let root = ui.root();
    let first = radio(&mut ui, root, "Hidden", group, true);
    let second = radio(&mut ui, root, "Disabled", group, false);
    let third = radio(&mut ui, root, "Available", group, false);
    frame(&mut ui);
    visible(&mut ui, first, false);
    ui.set_enabled(second, false).unwrap();
    ui.set_checked(second, true).unwrap();
    assert_checked(&ui, &[(first, false), (second, true), (third, false)]);
    ui.set_checked(third, true).unwrap();
    assert_checked(&ui, &[(first, false), (second, false), (third, true)]);
    visible(&mut ui, first, true);
    ui.set_enabled(second, true).unwrap();
    frame(&mut ui);
    assert_checked(&ui, &[(first, false), (second, false), (third, true)]);
}

#[test]
fn removed_radio_handles_cannot_change_a_replacement_or_its_group() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let group = ui.radio_group();
    let root = ui.root();
    let removed = radio(&mut ui, root, "Removed", group, true);
    let survivor = radio(&mut ui, root, "Survivor", group, false);
    ui.runtime.apply(Mutation::Remove(removed)).unwrap();
    assert_eq!(ui.checked(removed), None);
    assert_eq!(ui.radio_group_of(removed), None);
    assert_eq!(ui.button_visual_state(removed), None);
    assert!(ui.set_checked(removed, true).is_err());
    assert!(
        ui.set_button_style(removed, ButtonStyle::default())
            .is_err()
    );
    let replacement = radio(&mut ui, root, "Replacement", group, true);
    assert_ne!(replacement, removed);
    assert!(ui.set_checked(removed, false).is_err());
    assert_checked(&ui, &[(survivor, false), (replacement, true)]);
    assert_eq!(ui.radio_group_of(replacement), Some(group));
}

#[test]
fn radio_activation_selects_without_toggling_off_and_exposes_only_selecting_semantics() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let group = ui.radio_group();
    let root = ui.root();
    let first = radio(&mut ui, root, "First", group, true);
    let second = radio(&mut ui, root, "Second", group, false);
    frame(&mut ui);
    let semantics = ui.semantics();
    let node = semantics.iter().find(|node| node.id == second).unwrap();
    assert_eq!(node.role, Role::RadioButton);
    assert!(node.actions.contains(&SemanticAction::Activate));
    assert!(node.actions.contains(&SemanticAction::Focus));
    assert!(!node.actions.contains(&SemanticAction::Toggle));

    ui.perform_action(second, SemanticAction::Focus, None)
        .unwrap();
    assert_checked(&ui, &[(first, true), (second, false)]);
    ui.perform_action(second, SemanticAction::Activate, None)
        .unwrap();
    ui.activate(second).unwrap();
    assert_checked(&ui, &[(first, false), (second, true)]);
    assert_eq!(ui.drain_activated(), vec![second, second]);
    key(&mut ui, Key::Space, Modifiers::default());
    assert!(ui.drain_activated().is_empty());
    key_up(&mut ui, Key::Space);
    key(&mut ui, Key::Enter, Modifiers::default());
    assert_checked(&ui, &[(first, false), (second, true)]);
    assert_eq!(ui.drain_activated(), vec![second, second]);
    let point = center(&ui, first);
    pointer(&mut ui, point, true);
    pointer(&mut ui, point, false);
    assert_checked(&ui, &[(first, true), (second, false)]);
    assert_eq!(ui.drain_activated(), vec![first]);
    assert!(ui.commands_mut().drain_invocations().is_empty());
}

#[test]
fn tab_and_reverse_tab_visit_one_representative_per_group_without_selecting_it() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let group = ui.radio_group();
    let other = ui.radio_group();
    let root = ui.root();
    let before = ui
        .button(root, "Before", None, layout(100.0, 36.0))
        .unwrap();
    let first = radio(&mut ui, root, "First", group, false);
    let selected = radio(&mut ui, root, "Selected", group, true);
    let empty_first = radio(&mut ui, root, "Empty first", other, false);
    let empty_second = radio(&mut ui, root, "Empty second", other, false);
    let after = ui.button(root, "After", None, layout(100.0, 36.0)).unwrap();
    frame(&mut ui);
    focus(&mut ui, before);
    for target in [selected, empty_first, after] {
        tab(&mut ui, false);
        assert_eq!(ui.runtime.focused(), Some(target));
    }
    for target in [empty_first, selected, before] {
        tab(&mut ui, true);
        assert_eq!(ui.runtime.focused(), Some(target));
    }
    for id in [first, selected, empty_first, empty_second] {
        assert!(ui.runtime.tree().node(id).unwrap().props.focusable);
    }
    assert_checked(
        &ui,
        &[
            (first, false),
            (selected, true),
            (empty_first, false),
            (empty_second, false),
        ],
    );
    assert!(ui.drain_activated().is_empty());
}

#[test]
fn tab_leaves_the_group_when_an_unselected_radio_was_focused_directly_or_by_mouse() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let group = ui.radio_group();
    let root = ui.root();
    let before = ui
        .button(root, "Before", None, layout(100.0, 36.0))
        .unwrap();
    let unselected = radio(&mut ui, root, "Unselected", group, false);
    let selected = radio(&mut ui, root, "Selected", group, true);
    let after = ui.button(root, "After", None, layout(100.0, 36.0)).unwrap();
    frame(&mut ui);
    focus(&mut ui, unselected);
    tab(&mut ui, false);
    assert_eq!(ui.runtime.focused(), Some(after));
    focus(&mut ui, unselected);
    tab(&mut ui, true);
    assert_eq!(ui.runtime.focused(), Some(before));

    let point = center(&ui, unselected);
    pointer(&mut ui, point, true);
    assert_eq!(ui.runtime.focused(), Some(unselected));
    pointer(&mut ui, Point::new(570.0, 440.0), false);
    assert_checked(&ui, &[(unselected, false), (selected, true)]);
    tab(&mut ui, false);
    assert_eq!(ui.runtime.focused(), Some(after));
    assert!(ui.drain_activated().is_empty());
}

#[test]
fn prevented_tab_restores_the_group_representative_before_the_next_dispatch() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let group = ui.radio_group();
    let root = ui.root();
    let unselected = radio(&mut ui, root, "Unselected", group, false);
    let selected = radio(&mut ui, root, "Selected", group, true);
    let after = ui.button(root, "After", None, layout(100.0, 36.0)).unwrap();
    let prevent = Rc::new(Cell::new(true));
    for id in [unselected, selected] {
        let handler_prevent = Rc::clone(&prevent);
        ui.runtime
            .on(id, move |event, context| {
                if matches!(event.input, InputEvent::KeyDown { key: Key::Tab, .. })
                    && handler_prevent.get()
                {
                    context.prevent_default();
                }
            })
            .unwrap();
    }
    frame(&mut ui);
    for id in [selected, unselected] {
        focus(&mut ui, id);
        tab(&mut ui, false);
        assert_eq!(ui.runtime.focused(), Some(id));
        assert!(!ui.runtime.tree().node(unselected).unwrap().props.tab_stop);
        assert!(ui.runtime.tree().node(selected).unwrap().props.tab_stop);
        assert_checked(&ui, &[(unselected, false), (selected, true)]);
    }
    prevent.set(false);
    tab(&mut ui, false);
    assert_eq!(ui.runtime.focused(), Some(after));
    assert!(ui.runtime.tree().node(selected).unwrap().props.tab_stop);
    assert!(ui.drain_activated().is_empty());
}

#[test]
fn prevented_radio_arrow_preserves_focus_selection_and_activation_queue() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let group = ui.radio_group();
    let root = ui.root();
    let selected = radio(&mut ui, root, "Selected", group, true);
    let next = radio(&mut ui, root, "Next", group, false);
    let prevent = Rc::new(Cell::new(true));
    let handler_prevent = Rc::clone(&prevent);
    ui.runtime
        .on(selected, move |event, context| {
            if matches!(
                event.input,
                InputEvent::KeyDown {
                    key: Key::ArrowRight,
                    ..
                }
            ) && handler_prevent.get()
            {
                context.prevent_default();
            }
        })
        .unwrap();
    frame(&mut ui);
    focus(&mut ui, selected);
    key(&mut ui, Key::ArrowRight, Modifiers::default());
    assert_eq!(ui.runtime.focused(), Some(selected));
    assert_checked(&ui, &[(selected, true), (next, false)]);
    assert!(ui.drain_activated().is_empty());
    prevent.set(false);
    key(&mut ui, Key::ArrowRight, Modifiers::default());
    assert_eq!(ui.runtime.focused(), Some(next));
    assert_checked(&ui, &[(selected, false), (next, true)]);
    assert_eq!(ui.drain_activated(), vec![next]);
}

#[test]
fn radio_only_forms_cycle_groups_and_keep_the_sole_eligible_member_focused() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let group = ui.radio_group();
    let other = ui.radio_group();
    let root = ui.root();
    let unselected = radio(&mut ui, root, "Unselected", group, false);
    let selected = radio(&mut ui, root, "Selected", group, true);
    let other_first = radio(&mut ui, root, "Other first", other, false);
    let other_selected = radio(&mut ui, root, "Other selected", other, true);
    frame(&mut ui);
    focus(&mut ui, unselected);
    for (reverse, expected) in [
        (false, other_selected),
        (false, selected),
        (true, other_selected),
    ] {
        tab(&mut ui, reverse);
        assert_eq!(ui.runtime.focused(), Some(expected));
    }
    ui.set_enabled(unselected, false).unwrap();
    ui.set_enabled(other_first, false).unwrap();
    ui.set_enabled(other_selected, false).unwrap();
    focus(&mut ui, selected);
    for reverse in [false, true] {
        tab(&mut ui, reverse);
        assert_eq!(ui.runtime.focused(), Some(selected));
        assert!(ui.runtime.tree().node(selected).unwrap().props.tab_stop);
    }
    assert_checked(
        &ui,
        &[
            (unselected, false),
            (selected, true),
            (other_first, false),
            (other_selected, true),
        ],
    );
    assert!(ui.drain_activated().is_empty());
}

#[test]
fn tab_fallback_tracks_disable_hide_and_removal_without_changing_selection() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let group = ui.radio_group();
    let root = ui.root();
    let before = ui
        .button(root, "Before", None, layout(100.0, 36.0))
        .unwrap();
    let fallback = radio(&mut ui, root, "Fallback", group, false);
    let selected = radio(&mut ui, root, "Selected", group, true);
    let after = ui.button(root, "After", None, layout(100.0, 36.0)).unwrap();
    frame(&mut ui);
    ui.set_enabled(selected, false).unwrap();
    focus(&mut ui, before);
    tab(&mut ui, false);
    assert_eq!(ui.runtime.focused(), Some(fallback));
    assert_checked(&ui, &[(fallback, false), (selected, true)]);
    ui.set_enabled(selected, true).unwrap();
    visible(&mut ui, selected, false);
    frame(&mut ui);
    focus(&mut ui, after);
    tab(&mut ui, true);
    assert_eq!(ui.runtime.focused(), Some(fallback));
    assert_checked(&ui, &[(fallback, false), (selected, true)]);
    visible(&mut ui, selected, true);
    frame(&mut ui);
    focus(&mut ui, before);
    tab(&mut ui, false);
    assert_eq!(ui.runtime.focused(), Some(selected));
    ui.runtime.apply(Mutation::Remove(selected)).unwrap();
    frame(&mut ui);
    focus(&mut ui, before);
    tab(&mut ui, false);
    assert_eq!(ui.runtime.focused(), Some(fallback));
    assert_checked(&ui, &[(fallback, false)]);
    assert!(ui.drain_activated().is_empty());
}

#[test]
fn all_four_arrow_keys_select_wrap_and_ignore_command_modifiers() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let group = ui.radio_group();
    let root = ui.root();
    let first = radio(&mut ui, root, "First", group, true);
    let second = radio(&mut ui, root, "Second", group, false);
    let third = radio(&mut ui, root, "Third", group, false);
    frame(&mut ui);
    focus(&mut ui, first);
    for (arrow, expected) in [
        (Key::ArrowRight, second),
        (Key::ArrowDown, third),
        (Key::ArrowRight, first),
        (Key::ArrowLeft, third),
        (Key::ArrowUp, second),
        (Key::ArrowUp, first),
        (Key::ArrowUp, third),
    ] {
        key(&mut ui, arrow, Modifiers::default());
        assert_eq!(ui.runtime.focused(), Some(expected));
        for id in [first, second, third] {
            assert_checked(&ui, &[(id, id == expected)]);
        }
        assert_eq!(ui.drain_activated(), vec![expected]);
    }
    for modifiers in [
        Modifiers {
            control: true,
            ..Default::default()
        },
        Modifiers {
            alt: true,
            ..Default::default()
        },
        Modifiers {
            super_key: true,
            ..Default::default()
        },
    ] {
        key(&mut ui, Key::ArrowRight, modifiers);
        assert_eq!(ui.runtime.focused(), Some(third));
        assert_checked(&ui, &[(third, true)]);
        assert!(ui.drain_activated().is_empty());
    }
    key(
        &mut ui,
        Key::ArrowRight,
        Modifiers {
            shift: true,
            ..Default::default()
        },
    );
    assert_eq!(ui.runtime.focused(), Some(first));
    assert_checked(&ui, &[(first, true), (third, false)]);
}

#[test]
fn arrows_follow_current_tree_order_and_skip_hidden_disabled_and_other_groups() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let group = ui.radio_group();
    let other = ui.radio_group();
    let root = ui.root();
    let stack = LayoutStyle {
        kind: LayoutKind::Stack(Axis::Vertical),
        ..layout(220.0, 160.0)
    };
    let left = ui.transparent_panel(root, stack.clone()).unwrap();
    let right = ui.transparent_panel(root, stack).unwrap();
    let first = radio(&mut ui, left, "First", group, true);
    let hidden = radio(&mut ui, left, "Hidden", group, false);
    let disabled = radio(&mut ui, left, "Disabled", group, false);
    let independent = radio(&mut ui, left, "Other", other, true);
    let second = radio(&mut ui, right, "Second", group, false);
    let third = radio(&mut ui, right, "Third", group, false);
    visible(&mut ui, hidden, false);
    ui.set_enabled(disabled, false).unwrap();
    frame(&mut ui);
    focus(&mut ui, first);
    key(&mut ui, Key::ArrowRight, Modifiers::default());
    assert_eq!(ui.runtime.focused(), Some(second));
    ui.runtime
        .apply(Mutation::Reparent {
            node: first,
            parent: right,
        })
        .unwrap();
    frame(&mut ui);
    // The current tree order is Second, Third, First after reparent appends.
    key(&mut ui, Key::ArrowLeft, Modifiers::default());
    assert_eq!(ui.runtime.focused(), Some(first));
    key(&mut ui, Key::ArrowRight, Modifiers::default());
    assert_eq!(ui.runtime.focused(), Some(second));
    assert_checked(
        &ui,
        &[(independent, true), (hidden, false), (disabled, false)],
    );
    ui.set_enabled(left, false).unwrap();
    ui.set_enabled(disabled, true).unwrap();
    visible(&mut ui, hidden, true);
    focus(&mut ui, third);
    key(&mut ui, Key::ArrowRight, Modifiers::default());
    assert_eq!(ui.runtime.focused(), Some(first));
    assert_checked(&ui, &[(first, true), (second, false), (third, false)]);
}

#[test]
fn disabled_ancestor_blocks_selection_and_cancels_an_active_radio_press() {
    let mut ui = Controls::new(Theme::FluentDark).unwrap();
    let group = ui.radio_group();
    let root = ui.root();
    let selected = radio(&mut ui, root, "Selected", group, true);
    let parent = ui.transparent_panel(root, layout(220.0, 60.0)).unwrap();
    let target = radio(&mut ui, parent, "Target", group, false);
    frame(&mut ui);
    let point = center(&ui, target);
    pointer(&mut ui, point, true);
    assert!(ui.button_visual_state(target).unwrap().pressed);
    ui.set_enabled(parent, false).unwrap();
    pointer(&mut ui, point, false);
    ui.activate(target).unwrap();
    ui.perform_action(target, SemanticAction::Activate, None)
        .unwrap();
    assert_checked(&ui, &[(selected, true), (target, false)]);
    let state = ui.button_visual_state(target).unwrap();
    assert!(!state.enabled && !state.pressed && !state.focused && !state.hovered);
    let node = ui
        .semantics()
        .into_iter()
        .find(|node| node.id == target)
        .unwrap();
    assert!(node.disabled && node.actions.is_empty());
    assert!(ui.drain_activated().is_empty());
    ui.set_checked(target, true).unwrap();
    assert_checked(&ui, &[(selected, false), (target, true)]);
}

#[test]
fn radio_space_press_is_canceled_by_focus_loss_and_escape() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let group = ui.radio_group();
    let root = ui.root();
    let selected = radio(&mut ui, root, "Selected", group, true);
    let target = radio(&mut ui, root, "Target", group, false);
    frame(&mut ui);
    focus(&mut ui, target);
    key(&mut ui, Key::Space, Modifiers::default());
    focus(&mut ui, selected);
    frame(&mut ui);
    focus(&mut ui, target);
    key_up(&mut ui, Key::Space);
    assert_checked(&ui, &[(selected, true), (target, false)]);
    key(&mut ui, Key::Space, Modifiers::default());
    key(&mut ui, Key::Escape, Modifiers::default());
    key_up(&mut ui, Key::Space);
    assert_checked(&ui, &[(selected, true), (target, false)]);
    assert!(ui.drain_activated().is_empty());
}

#[test]
fn radio_style_is_atomic_and_does_not_change_other_button_kinds_or_selection() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let group = ui.radio_group();
    let root = ui.root();
    let id = radio(&mut ui, root, "Radio", group, true);
    let checkbox = ui
        .checkbox(root, "Check", true, layout(180.0, 32.0))
        .unwrap();
    let toggle = ui
        .toggle(root, "Toggle", None, true, layout(180.0, 32.0))
        .unwrap();
    let style = ButtonStyle {
        font_size: Some(19.0),
        padding: Some(Edges::all(3.0)),
        ..Default::default()
    };
    ui.set_button_style(id, style).unwrap();
    frame(&mut ui);
    let scene = ui.scene().clone();
    assert!(
        ui.set_button_style(
            id,
            ButtonStyle {
                border_width: Some(f32::NAN),
                ..style
            }
        )
        .is_err()
    );
    assert_eq!(ui.button_style(id), Some(&style));
    assert_eq!(ui.scene(), &scene);
    assert_checked(&ui, &[(id, true), (checkbox, true), (toggle, true)]);
    ui.activate(checkbox).unwrap();
    ui.activate(toggle).unwrap();
    assert_checked(&ui, &[(id, true), (checkbox, false), (toggle, false)]);
    assert_eq!(ui.radio_group_of(checkbox), None);
    assert_eq!(ui.radio_group_of(toggle), None);
}

#[test]
fn fluent_radio_paints_a_circular_indicator_and_dot_with_a_transparent_row() {
    for theme in [Theme::FluentLight, Theme::FluentDark] {
        let mut ui = Controls::new(theme).unwrap();
        let group = ui.radio_group();
        let root = ui.root();
        let id = radio(&mut ui, root, "Circular", group, true);
        let fill_color = Color {
            r: 0.2,
            g: 0.4,
            b: 0.6,
            a: 0.25,
        };
        let border_color = Color {
            r: 0.7,
            g: 0.3,
            b: 0.1,
            a: 0.5,
        };
        let dot_color = Color::rgb(10, 230, 60);
        ui.set_button_style(
            id,
            ButtonStyle {
                corner_radius: Some(0.0),
                border_width: Some(2.0),
                checked: ButtonAppearance {
                    background: Some(fill_color),
                    border: Some(border_color),
                    checkmark: Some(dot_color),
                    ..Default::default()
                },
                ..Default::default()
            },
        )
        .unwrap();
        frame(&mut ui);
        let bounds = ui.runtime.bounds(id).unwrap();
        let scene = ui.scene();
        assert!(!scene.rectangles.iter().any(|shape| shape.bounds == bounds));
        assert!(
            !scene
                .rounded_rectangles
                .iter()
                .any(|shape| shape.bounds == bounds && shape.stroke_width.is_none())
        );
        let fill_index = scene
            .rounded_rectangles
            .iter()
            .position(|shape| shape.color == fill_color)
            .unwrap();
        let fill = &scene.rounded_rectangles[fill_index];
        assert_eq!(fill.stroke_width, None);
        assert_eq!(fill.bounds.width, fill.bounds.height);
        assert_eq!(fill.radius, fill.bounds.width * 0.5);
        assert!(fill.bounds.width > 0.0 && fill.bounds.width <= 18.0);
        let border_index = scene
            .rounded_rectangles
            .iter()
            .position(|shape| shape.color == border_color)
            .unwrap();
        let border = &scene.rounded_rectangles[border_index];
        assert_eq!(border.bounds, fill.bounds);
        assert_eq!(border.stroke_width, Some(2.0));
        let dot = scene
            .rounded_rectangles
            .iter()
            .find(|shape| shape.color == dot_color)
            .unwrap();
        assert_eq!(dot.stroke_width, None);
        assert_eq!(dot.bounds.width, dot.bounds.height);
        assert_eq!(dot.radius, dot.bounds.width * 0.5);
        assert!(dot.bounds.width > 0.0 && dot.bounds.width < fill.bounds.width);
        assert_eq!(
            dot.bounds.x + dot.bounds.width * 0.5,
            fill.bounds.x + fill.bounds.width * 0.5
        );
        assert_eq!(
            dot.bounds.y + dot.bounds.height * 0.5,
            fill.bounds.y + fill.bounds.height * 0.5
        );
        let commands = scene.commands();
        let fill_order = commands
            .iter()
            .position(|entry| *entry == DrawCommand::RoundedRectangle(fill_index))
            .unwrap();
        let border_order = commands
            .iter()
            .position(|entry| *entry == DrawCommand::RoundedRectangle(border_index))
            .unwrap();
        assert!(fill_order < border_order);
        assert!(
            scene
                .rounded_rectangles
                .iter()
                .all(|shape| shape.is_valid())
        );
        ui.set_checked(id, false).unwrap();
        frame(&mut ui);
        assert!(
            !ui.scene()
                .rounded_rectangles
                .iter()
                .any(|shape| shape.color == dot_color)
        );
    }
}

#[test]
fn radio_circle_keeps_its_geometry_when_clipped_and_handles_tiny_focus_bounds() {
    for tiny in [false, true] {
        let mut ui = Controls::new(Theme::FluentLight).unwrap();
        let group = ui.radio_group();
        let root = ui.root();
        let size = if tiny { 0.5 } else { 10.0 };
        let parent = ui.transparent_panel(root, layout(size, size)).unwrap();
        let id = ui
            .radio_button(
                parent,
                "Clipped",
                group,
                true,
                layout(
                    if tiny { 1.0 } else { 180.0 },
                    if tiny { 1.0 } else { 32.0 },
                ),
            )
            .unwrap();
        ui.set_button_style(
            id,
            ButtonStyle {
                padding: Some(Edges::all(if tiny { 16384.0 } else { 0.0 })),
                border_width: Some(if tiny { 16384.0 } else { 1.0 }),
                ..Default::default()
            },
        )
        .unwrap();
        frame(&mut ui);
        focus(&mut ui, id);
        frame(&mut ui);
        assert!(ui.button_visual_state(id).unwrap().focused);
        for shape in &ui.scene().rounded_rectangles {
            assert!(shape.is_valid());
            assert!(shape.clip.width <= size && shape.clip.height <= size);
        }
        for shape in &ui.scene().rectangles {
            assert!(shape.bounds.is_valid());
        }
        if !tiny {
            let circle = ui
                .scene()
                .rounded_rectangles
                .iter()
                .find(|shape| shape.stroke_width.is_none() && shape.bounds.width == 18.0)
                .unwrap();
            assert_eq!(circle.bounds.height, 18.0);
            assert_eq!(circle.radius, 9.0);
            assert_eq!(circle.clip.width, 10.0);
        }
    }
}
