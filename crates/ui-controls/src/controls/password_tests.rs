use super::*;
use rust_desktop_ui_core::Modifiers;
use std::time::{Duration, Instant};

fn frame(ui: &mut Controls) {
    ui.update(Size::new(400.0, 200.0)).unwrap();
}
fn setup() -> (Controls, WidgetId) {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let root = ui.root();
    let id = ui
        .password_box(
            root,
            "Account password",
            PasswordBoxSettings::default(),
            LayoutStyle {
                width: Length::Px(260.0),
                height: Length::Px(36.0),
                ..Default::default()
            },
        )
        .unwrap();
    ui.runtime.apply(Mutation::Focus(Some(id))).unwrap();
    frame(&mut ui);
    (ui, id)
}
fn key(ui: &mut Controls, key: Key, control: bool) {
    ui.dispatch(InputEvent::KeyDown {
        key,
        modifiers: Modifiers {
            control,
            ..Default::default()
        },
        repeat: false,
    })
    .unwrap();
}
fn value(ui: &Controls, id: WidgetId) -> String {
    ui.password_value(id).unwrap().expose_secret().to_owned()
}

#[test]
fn unshapeable_secret_stays_masked_and_usable_when_reveal_is_requested() {
    let (mut ui, id) = setup();
    let secret = format!("A{}", "\u{301}".repeat(4500));
    ui.set_password_value(id, &secret).unwrap();
    frame(&mut ui);
    ui.drain_password_events();
    assert!(!ui.reveal_password(id).unwrap());
    assert_eq!(ui.password_is_revealed(id), Some(false));
    assert!(ui.drain_password_events().is_empty());
    assert_eq!(value(&ui, id), secret);
    frame(&mut ui);
    assert!(!format!("{:?}", ui.scene).contains(&secret));
    assert_eq!(ui.take_password_value(id).unwrap().expose_secret(), secret);
    frame(&mut ui);
}

#[test]
fn reveal_button_stays_at_right_edge_after_resize_and_inside_narrow_field() {
    let (mut ui, id) = setup();
    let button = ui.password_reveal_button(id).unwrap();
    for width in [260.0, 410.0, 18.0] {
        let mut style = ui.runtime.tree().node(id).unwrap().props.style.clone();
        style.width = Length::Px(width);
        ui.runtime
            .apply(Mutation::SetStyle { node: id, style })
            .unwrap();
        frame(&mut ui);
        let owner = ui.runtime.bounds(id).unwrap();
        let child = ui.runtime.bounds(button).unwrap();
        assert!((child.x + child.width - owner.x - owner.width).abs() < 0.01);
        assert!(child.x >= owner.x && child.width <= owner.width);
        assert!((child.width - width.min(30.0)).abs() < 0.01);
    }
}

#[test]
fn secret_is_absent_from_ordinary_documents_events_semantics_and_scene_debug() {
    let (mut ui, id) = setup();
    let secret = "Secret-Я👩‍💻e\u{301}";
    ui.text_input(secret).unwrap();
    frame(&mut ui);
    assert_eq!(value(&ui, id), secret);
    assert!(ui.document(id).is_none());
    assert!(ui.accepted_value(id).is_none());
    assert!(ui.drain_text_changes().is_empty());
    assert!(ui.drain_edit_events().is_empty());
    assert!(
        ui.scene()
            .texts
            .iter()
            .all(|run| !run.text.contains("Secret"))
    );
    let nodes = ui.semantics();
    let node = nodes.iter().find(|node| node.id == id).unwrap();
    assert_eq!(node.role, Role::PasswordBox);
    assert!(node.value.is_none());
    for debug in [
        format!("{nodes:?}"),
        format!("{:?}", ui.password_value(id)),
        format!("{:?}", ui.drain_password_events()),
    ] {
        assert!(!debug.contains(secret));
    }
    assert!(ui.reveal_password(id).unwrap());
    frame(&mut ui);
    assert!(
        ui.scene()
            .texts
            .iter()
            .any(|run| run.sensitive && run.text == secret)
    );
    assert!(!format!("{:?}", ui.scene()).contains(secret));
    assert!(!format!("{:?}", ui.semantics()).contains(secret));
    assert!(!format!("{:?}", ui.text_layouts).contains(secret));
}

#[test]
fn password_clipboard_and_history_routes_never_copy_cut_or_restore_secret() {
    let (mut ui, id) = setup();
    ui.text_input("a👩‍💻e\u{301}").unwrap();
    key(&mut ui, Key::Character("a".into()), true);
    for shortcut in ["c", "x", "z", "y"] {
        key(&mut ui, Key::Character(shortcut.into()), true);
    }
    assert!(ui.drain_clipboard_requests().is_empty());
    assert_eq!(value(&ui, id), "a👩‍💻e\u{301}");
    key(&mut ui, Key::Backspace, false);
    assert_eq!(value(&ui, id), "");
    key(&mut ui, Key::Character("z".into()), true);
    assert_eq!(value(&ui, id), "");
    key(&mut ui, Key::Character("v".into()), true);
    assert_eq!(ui.drain_clipboard_requests(), vec![ClipboardRequest::Paste]);
    ui.paste_input("pasted").unwrap();
    assert_eq!(value(&ui, id), "pasted");
    let mut settings = ui.password_settings(id).unwrap().clone();
    settings.allow_paste = false;
    ui.set_password_settings(id, settings).unwrap();
    key(&mut ui, Key::Character("v".into()), true);
    assert!(ui.drain_clipboard_requests().is_empty());
    ui.paste_input("blocked").unwrap();
    assert_eq!(value(&ui, id), "pasted");
    ui.text_input("!").unwrap();
    assert_eq!(value(&ui, id), "pasted!");
}

#[test]
fn grapheme_limits_settings_and_readonly_preserve_value_atomically() {
    let (mut ui, id) = setup();
    let mut settings = ui.password_settings(id).unwrap().clone();
    settings.max_length = 2;
    ui.set_password_settings(id, settings.clone()).unwrap();
    ui.text_input("👩‍💻e\u{301}").unwrap();
    ui.text_input("x").unwrap();
    assert_eq!(value(&ui, id), "👩‍💻e\u{301}");
    assert!(ui.set_password_value(id, "abc").is_err());
    settings.max_length = 1;
    assert!(ui.set_password_settings(id, settings.clone()).is_err());
    assert_eq!(ui.password_settings(id).unwrap().max_length, 2);
    key(&mut ui, Key::Backspace, false);
    assert_eq!(value(&ui, id), "👩‍💻");
    settings.max_length = 2;
    settings.is_read_only = true;
    ui.set_password_settings(id, settings).unwrap();
    ui.text_input("x").unwrap();
    key(&mut ui, Key::Backspace, false);
    assert_eq!(value(&ui, id), "👩‍💻");
    assert!(
        ui.semantics()
            .iter()
            .find(|node| node.id == id)
            .unwrap()
            .read_only
    );
    ui.set_password_value(id, "ok").unwrap();
    assert_eq!(value(&ui, id), "ok");
}

#[test]
fn reveal_timeout_blur_disable_detach_and_input_purge_previous_frame() {
    let (mut ui, id) = setup();
    ui.set_password_value(id, "transient-secret").unwrap();
    assert!(ui.reveal_password(id).unwrap());
    frame(&mut ui);
    ui.tick(Instant::now() + Duration::from_secs(11)).unwrap();
    assert_eq!(ui.password_is_revealed(id), Some(false));
    assert!(
        ui.scene()
            .texts
            .iter()
            .all(|run| run.text != "transient-secret")
    );
    assert!(ui.reveal_password(id).unwrap());
    frame(&mut ui);
    ui.dispatch(InputEvent::WindowFocused(false)).unwrap();
    assert_eq!(ui.password_is_revealed(id), Some(false));
    assert!(!ui.reveal_password(id).unwrap());
    ui.dispatch(InputEvent::WindowFocused(true)).unwrap();
    ui.runtime.apply(Mutation::Focus(Some(id))).unwrap();
    assert!(ui.reveal_password(id).unwrap());
    frame(&mut ui);
    ui.set_enabled(id, false).unwrap();
    assert_eq!(ui.password_is_revealed(id), Some(false));
    assert!(
        ui.scene()
            .texts
            .iter()
            .all(|run| run.text != "transient-secret")
    );
    ui.set_enabled(id, true).unwrap();
    ui.runtime.apply(Mutation::Focus(Some(id))).unwrap();
    assert!(ui.reveal_password(id).unwrap());
    frame(&mut ui);
    ui.text_input("!").unwrap();
    assert_eq!(ui.password_is_revealed(id), Some(false));
    assert!(ui.reveal_password(id).unwrap());
    frame(&mut ui);
    ui.runtime.apply(Mutation::Detach(id)).unwrap();
    frame(&mut ui);
    assert_eq!(ui.password_is_revealed(id), Some(false));
    assert_eq!(value(&ui, id), "transient-secret!");
}

#[test]
fn reveal_button_retains_owner_and_submission_is_metadata_only() {
    let (mut ui, id) = setup();
    ui.text_input("value").unwrap();
    let button = ui.password_reveal_button(id).unwrap();
    ui.runtime.apply(Mutation::Focus(Some(button))).unwrap();
    frame(&mut ui);
    ui.activate(button).unwrap();
    frame(&mut ui);
    assert_eq!(ui.runtime.focused(), Some(id));
    assert_eq!(ui.password_is_revealed(id), Some(true));
    key(&mut ui, Key::Enter, false);
    assert_eq!(ui.password_is_revealed(id), Some(false));
    assert!(
        ui.drain_password_events()
            .iter()
            .any(|event| event.kind == PasswordEventKind::Submitted)
    );
    let taken = ui.take_password_value(id).unwrap();
    assert_eq!(taken.expose_secret(), "value");
    assert!(!format!("{taken:?}").contains("value"));
    assert_eq!(value(&ui, id), "");
    ui.remove(id).unwrap();
    assert!(ui.password_value(id).is_none());
    assert!(ui.drain_password_events().is_empty());
}

#[test]
fn ime_draft_is_masked_commit_is_atomic_and_escape_cancels_composition() {
    let (mut ui, id) = setup();
    ui.text_input("a").unwrap();
    ui.ime_preedit("漢字", Some((3, 6))).unwrap();
    frame(&mut ui);
    assert_eq!(value(&ui, id), "a");
    assert_eq!(ui.password_composition_active(id), Some(true));
    assert!(!ui.reveal_password(id).unwrap());
    assert!(ui.scene().texts.iter().all(|run| !run.text.contains('漢')));
    assert!(ui.caret_bounds().unwrap().is_valid());
    key(&mut ui, Key::Escape, false);
    assert_eq!(ui.password_composition_active(id), Some(false));
    ui.ime_preedit("漢字", None).unwrap();
    ui.ime_commit("漢字").unwrap();
    assert_eq!(value(&ui, id), "a漢字");
    ui.ime_preedit("draft", None).unwrap();
    ui.ime_commit("invalid\nvalue").unwrap();
    assert_eq!(value(&ui, id), "a漢字");
    assert_eq!(ui.password_composition_active(id), Some(false));
    assert!(ui.drain_text_changes().is_empty());
}

#[test]
fn narrow_password_bounds_clip_text_and_reveal_button_without_leaking() {
    let (mut ui, id) = setup();
    ui.edit_props(id, |props| props.style.width = Length::Px(18.0))
        .unwrap();
    ui.text_input("👩‍💻long-password").unwrap();
    frame(&mut ui);
    assert!(ui.scene().texts.iter().all(TextRun::is_valid));
    let bounds = ui.runtime.bounds(id).unwrap();
    let button = ui.password_reveal_button(id).unwrap();
    let clip = ui.runtime.clip(button).unwrap();
    assert!(clip.x >= bounds.x && clip.x + clip.width <= bounds.x + bounds.width);
    let stats = ui.update(Size::new(400.0, 200.0)).unwrap();
    assert_eq!(stats.paint_passes, 0);
}
