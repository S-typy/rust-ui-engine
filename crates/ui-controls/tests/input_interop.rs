use rust_desktop_ui_controls::{Controls, Theme};
use rust_desktop_ui_core::{
    DragPayload, DropEffects, InputEvent, Key, LayoutStyle, Length, Modifiers, Mutation, Point,
    Size,
};

fn editor() -> (Controls, rust_desktop_ui_core::WidgetId) {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let id = ui
        .edit_box(
            ui.root(),
            "Name",
            "draft",
            LayoutStyle {
                width: Length::Px(200.0),
                height: Length::Px(32.0),
                ..Default::default()
            },
        )
        .unwrap();
    ui.update(Size::new(400.0, 200.0)).unwrap();
    ui.runtime.apply(Mutation::Focus(Some(id))).unwrap();
    (ui, id)
}

#[test]
fn routed_prevent_default_consumes_the_companion_committed_text() {
    let (mut ui, id) = editor();
    ui.runtime.on(id, |event, context| {
        if matches!(&event.input, InputEvent::KeyDown { key: Key::Character(text), .. } if text == "x") {
            context.prevent_default();
        }
    }).unwrap();
    ui.dispatch(InputEvent::KeyDown {
        key: Key::Character("x".into()),
        modifiers: Modifiers::default(),
        repeat: false,
    })
    .unwrap();
    ui.text_input("x").unwrap();
    assert_eq!(ui.document(id).unwrap().text(), "draft");
    ui.text_input("y").unwrap();
    assert!(ui.document(id).unwrap().text().contains('y'));
}

#[test]
fn a_drag_owns_text_input_and_cancels_preedit_without_changing_the_draft() {
    let (mut ui, id) = editor();
    ui.ime_preedit("候", None).unwrap();
    let drag = ui
        .runtime
        .begin_drag(
            id,
            DragPayload {
                format: "text/plain".into(),
                data: b"drag".to_vec(),
            },
            DropEffects::COPY,
            Point::new(5.0, 5.0),
        )
        .unwrap();
    assert!(ui.caret_bounds().is_none());
    ui.dispatch(InputEvent::KeyDown {
        key: Key::Character("x".into()),
        modifiers: Modifiers::default(),
        repeat: false,
    })
    .unwrap();
    ui.text_input("x").unwrap();
    ui.ime_preedit("補", None).unwrap();
    ui.ime_commit("補").unwrap();
    assert_eq!(ui.document(id).unwrap().text(), "draft");
    assert!(ui.document(id).unwrap().composition().is_none());
    ui.runtime.cancel_drag(drag).unwrap();
    ui.text_input("y").unwrap();
    assert!(ui.document(id).unwrap().text().contains('y'));
}
