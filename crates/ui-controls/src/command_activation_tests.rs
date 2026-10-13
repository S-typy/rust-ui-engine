use crate::*;
use rust_desktop_ui_core::{
    Color, InputEvent, Key, LayoutStyle, Length, Modifiers, Mutation, Point, PointerButton, Size,
    WidgetId,
};
use std::{cell::Cell, rc::Rc};

fn style() -> LayoutStyle {
    LayoutStyle {
        width: Length::Px(160.0),
        height: Length::Px(36.0),
        flex_shrink: 0.0,
        ..Default::default()
    }
}
fn frame(ui: &mut Controls) {
    ui.update(Size::new(600.0, 480.0)).unwrap();
}
fn semantic(ui: &Controls, id: WidgetId) -> SemanticNode {
    ui.semantics()
        .into_iter()
        .find(|node| node.id == id)
        .unwrap()
}
fn body(ui: &Controls, id: WidgetId) -> Color {
    let bounds = ui.runtime.bounds(id).unwrap();
    ui.scene()
        .rounded_rectangles
        .iter()
        .find(|rect| rect.bounds == bounds && rect.stroke_width.is_none())
        .map(|rect| rect.color)
        .or_else(|| {
            ui.scene()
                .rectangles
                .iter()
                .find(|rect| rect.bounds == bounds)
                .map(|rect| rect.color)
        })
        .unwrap()
}

#[test]
fn registry_activation_toggles_once_but_raw_execution_preserves_state() {
    let mut commands = CommandRegistry::default();
    let id = CommandId::from("pin");
    commands.register_toggle(id.clone(), "Pin", false);
    let calls = Rc::new(Cell::new(0));
    let observed = calls.clone();
    commands.on_execute(&id, move || observed.set(observed.get() + 1));
    let gates = Rc::new(Cell::new(0));
    let observed = gates.clone();
    commands.set_can_execute(&id, move || {
        observed.set(observed.get() + 1);
        true
    });
    assert!(commands.activate(&id));
    assert_eq!(commands.checked(&id), Some(true));
    assert!(commands.execute(&id));
    assert_eq!(commands.checked(&id), Some(true));
    assert!(commands.activate(&id));
    assert_eq!(commands.checked(&id), Some(false));
    assert_eq!((gates.get(), calls.get()), (3, 3));
    assert_eq!(
        commands
            .drain_invocations()
            .iter()
            .map(|event| event.checked)
            .collect::<Vec<_>>(),
        [Some(true), Some(true), Some(false)]
    );
    commands.set_checked(&id, None);
    assert!(commands.activate(&id));
    assert_eq!(commands.checked(&id), None);
    assert_eq!(commands.drain_invocations()[0].checked, None);
}

#[test]
fn registry_rejection_and_programmatic_state_changes_emit_nothing() {
    let mut commands = CommandRegistry::default();
    let id = CommandId::from("pin");
    commands.register_toggle(id.clone(), "Pin", true);
    let calls = Rc::new(Cell::new(0));
    let observed = calls.clone();
    commands.on_execute(&id, move || observed.set(observed.get() + 1));
    commands.set_can_execute(&id, || false);
    assert!(!commands.activate(&id));
    assert!(!commands.execute(&id));
    assert_eq!(commands.checked(&id), Some(true));
    commands.set_can_execute(&id, || {
        panic!("disabled commands must not evaluate predicate")
    });
    commands.set_enabled(&id, false);
    assert!(!commands.activate(&id));
    assert!(!commands.execute(&id));
    assert!(!commands.activate(&"missing".into()));
    assert!(!commands.set_checked(&"missing".into(), Some(true)));
    commands.set_checked(&id, Some(false));
    commands.set_checked(&id, None);
    assert!(commands.drain_invocations().is_empty());
    assert_eq!(calls.get(), 0);
}

#[test]
fn bound_button_toggle_and_menu_share_live_state_without_an_intermediate_frame() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let id = CommandId::from("pin");
    ui.commands_mut().register_toggle(id.clone(), "Pin", false);
    let button = ui
        .button(ui.root(), "Button", Some(id.clone()), style())
        .unwrap();
    let toggle = ui
        .toggle(ui.root(), "Toggle", Some(id.clone()), true, style())
        .unwrap();
    frame(&mut ui);
    ui.popup_menu(button, &[MenuItem::new("Menu Pin", id.clone())])
        .unwrap();
    let menu = ui
        .semantics()
        .into_iter()
        .find(|node| node.label == "Menu Pin")
        .unwrap()
        .id;
    assert_eq!(ui.checked(toggle), Some(false));
    for (placement, expected) in [(button, true), (toggle, false)] {
        ui.activate(placement).unwrap();
        for peer in [button, toggle, menu] {
            assert_eq!(ui.checked(peer), Some(expected));
            assert_eq!(semantic(&ui, peer).checked, Some(expected));
            assert!(
                semantic(&ui, peer)
                    .actions
                    .contains(&SemanticAction::Toggle)
            );
        }
        assert_eq!(ui.button_visual_state(button).unwrap().checked, expected);
        assert_eq!(ui.button_visual_state(toggle).unwrap().checked, expected);
    }
    assert_eq!(semantic(&ui, button).role, Role::ToggleButton);
    assert_eq!(semantic(&ui, menu).role, Role::MenuItem);
    ui.perform_action(menu, SemanticAction::Activate, None)
        .unwrap();
    assert_eq!(ui.commands().checked(&id), Some(true));
    assert!(!ui.popup_open());
    assert_eq!(ui.drain_activated(), [button, toggle, menu]);
    assert_eq!(
        ui.commands_mut()
            .drain_invocations()
            .iter()
            .map(|event| event.checked)
            .collect::<Vec<_>>(),
        [Some(true), Some(false), Some(true)]
    );
}

#[test]
fn removing_command_toggle_mode_clears_live_semantics_and_every_theme_palette() {
    for theme in [
        Theme::Light,
        Theme::Dark,
        Theme::Compact,
        Theme::FluentLight,
        Theme::FluentDark,
    ] {
        let mut ui = Controls::new(theme).unwrap();
        let command = CommandId::from("pin");
        ui.commands_mut()
            .register_toggle(command.clone(), "Pin", true);
        let button = ui
            .button(ui.root(), "Button", Some(command.clone()), style())
            .unwrap();
        let toggle = ui
            .toggle(ui.root(), "Toggle", Some(command.clone()), false, style())
            .unwrap();
        let ordinary = ui.button(ui.root(), "Ordinary", None, style()).unwrap();
        frame(&mut ui);
        assert_ne!(body(&ui, button), body(&ui, ordinary));
        ui.commands_mut().set_checked(&command, None);
        for id in [button, toggle] {
            assert_eq!(ui.checked(id), None);
            assert!(!ui.button_visual_state(id).unwrap().checked);
            let node = semantic(&ui, id);
            assert_eq!(node.role, Role::Button);
            assert_eq!(node.checked, None);
            assert!(!node.actions.contains(&SemanticAction::Toggle));
        }
        frame(&mut ui);
        assert_eq!(body(&ui, button), body(&ui, ordinary));
        assert_eq!(body(&ui, toggle), body(&ui, ordinary));
        ui.activate(toggle).unwrap();
        assert_eq!(ui.commands().checked(&command), None);
        assert_eq!(ui.commands_mut().drain_invocations()[0].checked, None);
        ui.commands_mut().set_checked(&command, Some(false));
        assert_eq!(semantic(&ui, button).role, Role::ToggleButton);
        ui.activate(button).unwrap();
        assert_eq!(ui.checked(toggle), Some(true));
        frame(&mut ui);
        assert_ne!(body(&ui, button), body(&ui, ordinary));
    }
}

#[test]
fn legacy_toggle_builders_promote_once_and_preserve_existing_shared_value() {
    let mut ui = Controls::new(Theme::Light).unwrap();
    let command = CommandId::from("legacy");
    ui.commands_mut().register(command.clone(), "Legacy");
    let first = ui
        .toggle(ui.root(), "First", Some(command.clone()), true, style())
        .unwrap();
    ui.toggle(ui.root(), "Second", Some(command.clone()), false, style())
        .unwrap();
    assert_eq!(ui.commands().checked(&command), Some(true));
    frame(&mut ui);
    ui.popup_menu(
        first,
        &[MenuItem::new("Legacy item", command.clone()).toggling()],
    )
    .unwrap();
    assert_eq!(ui.commands().checked(&command), Some(true));
    let menu = ui
        .semantics()
        .into_iter()
        .find(|node| node.label == "Legacy item")
        .unwrap()
        .id;
    let checked_x = ui
        .scene()
        .texts
        .iter()
        .find(|run| run.text == "Legacy item")
        .unwrap()
        .bounds
        .x;
    ui.commands_mut().set_checked(&command, None);
    frame(&mut ui);
    assert_eq!(ui.checked(first), None);
    assert_eq!(semantic(&ui, menu).checked, None);
    let ordinary_x = ui
        .scene()
        .texts
        .iter()
        .find(|run| run.text == "Legacy item")
        .unwrap()
        .bounds
        .x;
    assert_eq!(checked_x - ordinary_x, 24.0);
    ui.activate(menu).unwrap();
    assert_eq!(ui.commands().checked(&command), None);
    let fresh = CommandId::from("fresh-menu");
    ui.commands_mut().register(fresh.clone(), "Fresh");
    ui.popup_menu(
        first,
        &[MenuItem::new("Fresh item", fresh.clone()).toggling()],
    )
    .unwrap();
    assert_eq!(ui.commands().checked(&fresh), Some(false));
}

#[test]
fn activation_uses_one_final_gate_and_rejection_keeps_menu_focus_and_events() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let command = CommandId::from("pin");
    ui.commands_mut()
        .register_toggle(command.clone(), "Pin", false);
    let button = ui
        .button(ui.root(), "Anchor", Some(command.clone()), style())
        .unwrap();
    let gates = Rc::new(Cell::new(0));
    let allowed = Rc::new(Cell::new(false));
    let gate_count = gates.clone();
    let permission = allowed.clone();
    ui.commands_mut().set_can_execute(&command, move || {
        let count = gate_count.get();
        gate_count.set(count + 1);
        permission.get() && count == 0
    });
    let calls = Rc::new(Cell::new(0));
    let count = calls.clone();
    ui.commands_mut()
        .on_execute(&command, move || count.set(count.get() + 1));
    frame(&mut ui);
    gates.set(0);
    allowed.set(true);
    // Cached props are still disabled: direct activation must use current policy.
    ui.activate(button).unwrap();
    assert_eq!((gates.get(), calls.get()), (1, 1));
    assert_eq!(ui.checked(button), Some(true));
    ui.drain_activated();
    ui.commands_mut().drain_invocations();
    ui.commands_mut().set_can_execute(&command, || true);
    // Keep the anchor independent: disabling an anchor now closes its overlay.
    let anchor = ui.button(ui.root(), "Menu anchor", None, style()).unwrap();
    frame(&mut ui);
    ui.popup_menu(anchor, &[MenuItem::new("Menu Pin", command.clone())])
        .unwrap();
    let menu = ui.runtime.focused().unwrap();
    let gates = Rc::new(Cell::new(0));
    let count = gates.clone();
    ui.commands_mut().set_can_execute(&command, move || {
        count.set(count.get() + 1);
        false
    });
    ui.perform_action(menu, SemanticAction::Toggle, None)
        .unwrap();
    assert_eq!(gates.get(), 1);
    assert!(ui.popup_open());
    assert_eq!(ui.runtime.focused(), Some(menu));
    assert_eq!(ui.commands().checked(&command), Some(true));
    assert_eq!(calls.get(), 1);
    assert!(ui.drain_activated().is_empty());
    assert!(ui.commands_mut().drain_invocations().is_empty());
}

#[test]
fn placement_ancestors_visibility_and_removed_ids_gate_before_command_execution() {
    let mut ui = Controls::new(Theme::Light).unwrap();
    let command = CommandId::from("pin");
    ui.commands_mut()
        .register_toggle(command.clone(), "Pin", false);
    let parent = ui.panel(ui.root(), LayoutStyle::default()).unwrap();
    let button = ui
        .button(parent, "Button", Some(command.clone()), style())
        .unwrap();
    frame(&mut ui);
    ui.set_enabled(parent, false).unwrap();
    assert!(!ui.is_enabled(button));
    ui.activate(button).unwrap();
    ui.set_enabled(parent, true).unwrap();
    let mut props = ui.runtime.tree().node(parent).unwrap().props.clone();
    props.visible = false;
    ui.runtime
        .apply(Mutation::SetProps {
            node: parent,
            props: props.clone(),
        })
        .unwrap();
    assert!(!ui.is_enabled(button));
    ui.activate(button).unwrap();
    props.visible = true;
    ui.runtime
        .apply(Mutation::SetProps {
            node: parent,
            props,
        })
        .unwrap();
    assert!(ui.is_enabled(button));
    ui.commands_mut().set_enabled(&command, false);
    assert!(!ui.is_enabled(button));
    ui.activate(button).unwrap();
    ui.commands_mut().set_enabled(&command, true);
    assert!(ui.is_enabled(button));
    ui.runtime.apply(Mutation::Remove(button)).unwrap();
    assert!(!ui.is_enabled(button));
    assert_eq!(ui.command_id(button), None);
    assert_eq!(ui.checked(button), None);
    assert!(ui.set_checked(button, true).is_err());
    ui.activate(button).unwrap();
    assert!(!ui.commands().checked(&command).unwrap());
    assert!(ui.drain_activated().is_empty());
    assert!(ui.commands_mut().drain_invocations().is_empty());
    let replacement = ui
        .button(parent, "Replacement", Some(command.clone()), style())
        .unwrap();
    assert_ne!(button, replacement);
    assert_eq!(ui.command_id(replacement), Some(&command));
    assert_eq!(ui.command_id(parent), None);
    ui.activate(button).unwrap();
    ui.activate(replacement).unwrap();
    assert_eq!(ui.drain_activated(), [replacement]);
    assert_eq!(ui.commands_mut().drain_invocations().len(), 1);
    assert_eq!(ui.checked(replacement), Some(true));
}

#[test]
fn unbound_selection_and_explicit_roles_survive_shared_command_projection() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let toggle = ui.toggle(ui.root(), "Tab", None, false, style()).unwrap();
    ui.set_role(toggle, Role::Tab);
    let check = ui.checkbox(ui.root(), "Check", true, style()).unwrap();
    let command = CommandId::from("custom");
    ui.commands_mut()
        .register_toggle(command.clone(), "Custom", false);
    let custom = ui
        .button(ui.root(), "Custom", Some(command.clone()), style())
        .unwrap();
    ui.set_role(custom, Role::CheckBox);
    frame(&mut ui);
    ui.activate(toggle).unwrap();
    ui.activate(check).unwrap();
    ui.activate(custom).unwrap();
    assert_eq!(ui.checked(toggle), Some(true));
    assert_eq!(ui.checked(check), Some(false));
    assert_eq!(semantic(&ui, toggle).role, Role::Tab);
    assert!(semantic(&ui, toggle).selected);
    assert_eq!(semantic(&ui, custom).role, Role::CheckBox);
    ui.commands_mut().set_checked(&command, None);
    assert_eq!(semantic(&ui, custom).role, Role::CheckBox);
    assert_eq!(semantic(&ui, custom).checked, None);
}

#[test]
fn pointer_keyboard_and_semantic_routes_each_invert_a_bound_button_once() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let command = CommandId::from("pin");
    ui.commands_mut()
        .register_toggle(command.clone(), "Pin", false);
    let button = ui
        .button(ui.root(), "Pin", Some(command.clone()), style())
        .unwrap();
    frame(&mut ui);
    let bounds = ui.runtime.bounds(button).unwrap();
    let position = Point::new(
        bounds.x + bounds.width / 2.0,
        bounds.y + bounds.height / 2.0,
    );
    ui.dispatch(InputEvent::PointerMoved { position }).unwrap();
    ui.dispatch(InputEvent::PointerDown {
        position,
        button: PointerButton::Primary,
        modifiers: Modifiers::default(),
    })
    .unwrap();
    assert_eq!(ui.checked(button), Some(false));
    ui.dispatch(InputEvent::PointerUp {
        position,
        button: PointerButton::Primary,
        modifiers: Modifiers::default(),
    })
    .unwrap();
    assert_eq!(ui.checked(button), Some(true));
    for (key, repeat) in [
        (Key::Enter, false),
        (Key::Enter, true),
        (Key::Space, false),
        (Key::Space, true),
    ] {
        ui.dispatch(InputEvent::KeyDown {
            key,
            repeat,
            modifiers: Modifiers::default(),
        })
        .unwrap();
    }
    assert_eq!(ui.checked(button), Some(false));
    ui.dispatch(InputEvent::KeyUp {
        key: Key::Space,
        modifiers: Modifiers::default(),
    })
    .unwrap();
    assert_eq!(ui.checked(button), Some(true));
    ui.perform_action(button, SemanticAction::Toggle, None)
        .unwrap();
    assert_eq!(ui.checked(button), Some(false));
    assert_eq!(ui.drain_activated(), [button; 4]);
    assert_eq!(
        ui.commands_mut()
            .drain_invocations()
            .iter()
            .map(|event| event.checked)
            .collect::<Vec<_>>(),
        [Some(true), Some(false), Some(true), Some(false)]
    );
}
