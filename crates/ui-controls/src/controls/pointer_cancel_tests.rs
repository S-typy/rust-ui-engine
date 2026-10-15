use super::*;
use rust_desktop_ui_core::Modifiers;

fn bounds() -> LayoutStyle {
    LayoutStyle {
        width: Length::Px(280.0),
        height: Length::Px(40.0),
        ..Default::default()
    }
}

fn frame(ui: &mut Controls) {
    ui.update(Size::new(600.0, 480.0)).unwrap();
}

fn point(ui: &Controls, id: WidgetId, offset: f32) -> Point {
    let rect = ui.runtime.bounds(id).unwrap();
    Point::new(rect.x + offset, rect.y + rect.height / 2.0)
}

fn pointer(position: Point, down: bool) -> InputEvent {
    if down {
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
    }
}

#[test]
fn cancelled_editor_selection_preserves_draft_accepted_focus_history_and_preedit() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let editor = ui
        .edit_box(ui.root(), "Editor", "Accepted", bounds())
        .unwrap();
    let mut settings = ui.editor_settings(editor).unwrap().clone();
    settings.commit_on_focus_lost = true;
    ui.set_editor_settings(editor, settings).unwrap();
    frame(&mut ui);
    ui.runtime.apply(Mutation::Focus(Some(editor))).unwrap();
    ui.text_input(" draft").unwrap();
    frame(&mut ui);
    let start = point(&ui, editor, 10.0);
    let end = point(&ui, editor, 75.0);
    ui.dispatch(pointer(start, true)).unwrap();
    ui.dispatch(InputEvent::PointerMoved { position: end })
        .unwrap();
    assert_eq!(ui.runtime.captured(), Some(editor));
    assert!(!ui.document(editor).unwrap().selection().is_empty());
    ui.ime_preedit("候補", Some((0, "候補".len()))).unwrap();
    let document = ui.document(editor).unwrap();
    let text = document.text().to_owned();
    let selection = document.selection();
    let composition = document.composition().cloned();
    assert!(composition.is_some());
    assert!(document.can_undo());
    ui.drain_edit_events();
    ui.drain_text_changes();
    ui.cancel_pointer_interaction().unwrap();
    frame(&mut ui);
    assert_eq!(ui.runtime.captured(), None);
    assert_eq!(ui.runtime.hovered(), None);
    assert_eq!(ui.runtime.focused(), Some(editor));
    ui.dispatch(InputEvent::PointerMoved {
        position: point(&ui, editor, 230.0),
    })
    .unwrap();
    ui.dispatch(pointer(end, false)).unwrap();
    let document = ui.document(editor).unwrap();
    assert_eq!(document.text(), text);
    assert_eq!(document.selection(), selection);
    assert_eq!(document.composition(), composition.as_ref());
    assert!(document.can_undo());
    assert_eq!(ui.accepted_value(editor), Some("Accepted"));
    assert!(ui.drain_edit_events().is_empty());
    assert!(ui.drain_text_changes().is_empty());
}

#[test]
fn cancelled_button_press_does_not_activate_on_late_up_or_leave_hover_or_tooltip() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let command = CommandId::from("Run");
    ui.commands_mut()
        .register_toggle(command.clone(), "Run", false);
    let button = ui
        .button(ui.root(), "Run", Some(command.clone()), bounds())
        .unwrap();
    ui.set_tooltip(button, "Run command").unwrap();
    frame(&mut ui);
    let center = point(&ui, button, 100.0);
    ui.dispatch(InputEvent::PointerMoved { position: center })
        .unwrap();
    assert!(ui.tooltip_deadline().is_some());
    ui.dispatch(pointer(center, true)).unwrap();
    assert_eq!(ui.runtime.captured(), Some(button));
    assert!(ui.button_visual_state(button).unwrap().pressed);
    ui.cancel_pointer_interaction().unwrap();
    frame(&mut ui);
    let state = ui.button_visual_state(button).unwrap();
    assert!(!state.pressed && !state.hovered);
    assert!(state.focused);
    assert_eq!(ui.tooltip_deadline(), None);
    assert_eq!(ui.runtime.captured(), None);
    ui.dispatch(pointer(center, false)).unwrap();
    assert!(ui.drain_activated().is_empty());
    assert!(ui.commands_mut().drain_invocations().is_empty());
    assert_eq!(ui.commands().checked(&command), Some(false));
}

#[test]
fn pointer_cancel_preserves_a_keyboard_space_press() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let button = ui.button(ui.root(), "Run", None, bounds()).unwrap();
    frame(&mut ui);
    ui.runtime.apply(Mutation::Focus(Some(button))).unwrap();
    ui.dispatch(InputEvent::KeyDown {
        key: Key::Space,
        modifiers: Modifiers::default(),
        repeat: false,
    })
    .unwrap();
    ui.cancel_pointer_interaction().unwrap();
    assert!(ui.button_visual_state(button).unwrap().pressed);
    assert!(ui.drain_activated().is_empty());
    ui.dispatch(InputEvent::KeyUp {
        key: Key::Space,
        modifiers: Modifiers::default(),
    })
    .unwrap();
    assert_eq!(ui.drain_activated(), [button]);
}

#[test]
fn cancelled_popup_item_press_keeps_menu_and_focus_without_activating() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let command = CommandId::from("Run");
    ui.commands_mut().register(command.clone(), "Run");
    let anchor = ui.button(ui.root(), "Menu", None, bounds()).unwrap();
    frame(&mut ui);
    ui.popup_menu(anchor, &[MenuItem::new("Run", command)])
        .unwrap();
    frame(&mut ui);
    let item = ui
        .semantics()
        .into_iter()
        .find(|node| node.role == Role::MenuItem && node.label == "Run")
        .unwrap()
        .id;
    let center = point(&ui, item, 20.0);
    ui.dispatch(pointer(center, true)).unwrap();
    assert_eq!(ui.runtime.focused(), Some(item));
    assert_eq!(ui.runtime.captured(), Some(item));
    let depth = ui.popup_depth();
    ui.cancel_pointer_interaction().unwrap();
    ui.dispatch(pointer(center, false)).unwrap();
    assert_eq!(ui.runtime.focused(), Some(item));
    assert_eq!(ui.popup_depth(), depth);
    assert!(ui.popup_open());
    assert!(ui.drain_activated().is_empty());
    assert!(ui.commands_mut().drain_invocations().is_empty());
}
