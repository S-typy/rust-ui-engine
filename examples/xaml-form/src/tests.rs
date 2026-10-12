use crate::{form::Form, generated};
use rust_desktop_ui_core::{InputEvent, Key, Modifiers, Mutation, Size};
use rust_desktop_ui_platform_winit::{DesktopApp, PlatformEvent, accesskit};

fn form() -> Form {
    let mut form = Form::new(generated::window_definition(), false).unwrap();
    form.update(Size::new(640.0, 280.0), 1.0).unwrap();
    form
}

fn key(form: &mut Form, key: Key, control: bool) {
    form.event(PlatformEvent::Input(InputEvent::KeyDown {
        key,
        modifiers: Modifiers {
            control,
            ..Default::default()
        },
        repeat: false,
    }))
    .unwrap();
}

#[test]
fn compiled_markup_matches_source_and_mounts_named_editbox() {
    let expected = rust_desktop_ui_xaml::parse(include_str!("../MainWindow.xaml")).unwrap();
    let compiled = generated::window_definition();
    assert_eq!(compiled, expected);
    let mut form = form();
    assert_eq!(form.title(), expected.title);
    assert_eq!(form.find_name("Message"), Some(form.editor));
    assert_eq!(form.find_name("Missing"), None);
    let bounds = form.ui.runtime.bounds(form.editor).unwrap();
    assert_eq!(
        (bounds.x, bounds.y, bounds.width, bounds.height),
        (32.0, 32.0, 576.0, 36.0)
    );
    assert_eq!(form.text(), "");
    assert_eq!(form.ui.is_read_only(form.editor), Some(false));
    assert_eq!(form.ui.max_length(form.editor), Some(Some(64)));
    assert!(
        form.scene()
            .texts
            .iter()
            .any(|text| text.text == "Введите текст…")
    );
    form.update(Size::new(360.0, 220.0), 2.0).unwrap();
    assert_eq!(form.ui.runtime.bounds(form.editor).unwrap().width, 296.0);
}

#[test]
fn tab_edit_selection_undo_and_resize_keep_document() {
    let mut form = form();
    key(&mut form, Key::Tab, false);
    assert_eq!(form.ui.runtime.focused(), Some(form.editor));
    form.event(PlatformEvent::Text("Привет 👋".into())).unwrap();
    assert_eq!(form.text(), "Привет 👋");
    key(&mut form, Key::Character("a".into()), true);
    form.event(PlatformEvent::Paste("Rust\r\nUI\t!".into()))
        .unwrap();
    assert_eq!(form.text(), "RustUI!");
    key(&mut form, Key::Enter, false);
    assert_eq!(form.text(), "RustUI!");
    key(&mut form, Key::Character("z".into()), true);
    assert_eq!(form.text(), "Привет 👋");
    form.update(Size::new(320.0, 180.0), 1.5).unwrap();
    assert_eq!(form.text(), "Привет 👋");
    assert!(form.ime_cursor().is_some());
    form.event(PlatformEvent::Input(InputEvent::WindowFocused(false)))
        .unwrap();
    assert_eq!(form.ime_target(), None);
}

#[test]
fn composition_is_display_only_until_commit_and_blur_cancels() {
    let mut form = form();
    form.ui
        .runtime
        .apply(Mutation::Focus(Some(form.editor)))
        .unwrap();
    form.event(PlatformEvent::ImePreedit("日本".into(), Some((6, 6))))
        .unwrap();
    assert_eq!(form.text(), "");
    form.update(Size::new(640.0, 280.0), 1.0).unwrap();
    assert!(
        !form
            .scene()
            .texts
            .iter()
            .any(|text| text.text == "Введите текст…")
    );
    form.event(PlatformEvent::ImeCommit("日本".into())).unwrap();
    assert_eq!(form.text(), "日本");
    form.event(PlatformEvent::ImePreedit("語".into(), Some((3, 3))))
        .unwrap();
    form.event(PlatformEvent::Input(InputEvent::WindowFocused(false)))
        .unwrap();
    assert!(
        form.ui
            .document(form.editor)
            .unwrap()
            .composition()
            .is_none()
    );
    assert_eq!(form.text(), "日本");
}

#[test]
fn accessibility_exposes_label_value_scaled_bounds_and_actions() {
    let mut form = form();
    let tree = form.accessibility(1.5);
    let node = &tree
        .nodes
        .iter()
        .find(|(id, _)| *id == accesskit::NodeId(2))
        .unwrap()
        .1;
    assert_eq!(node.role(), accesskit::Role::TextInput);
    assert_eq!(node.label(), Some("Текст"));
    assert_eq!(node.value(), Some(""));
    assert_eq!(node.bounds().unwrap().x0, 48.0);
    form.event(PlatformEvent::Accessibility(accesskit::ActionRequest {
        action: accesskit::Action::SetValue,
        target_tree: accesskit::TreeId::ROOT,
        target_node: accesskit::NodeId(2),
        data: Some(accesskit::ActionData::Value("Данные".into())),
    }))
    .unwrap();
    assert_eq!(form.text(), "Данные");
    form.ui.set_enabled(form.editor, false).unwrap();
    form.event(PlatformEvent::Accessibility(accesskit::ActionRequest {
        action: accesskit::Action::SetValue,
        target_tree: accesskit::TreeId::ROOT,
        target_node: accesskit::NodeId(2),
        data: Some(accesskit::ActionData::Value("Запрещено".into())),
    }))
    .unwrap();
    assert_eq!(form.text(), "Данные");
}

#[test]
fn readonly_form_keeps_focus_copy_and_programmatic_updates_without_ime_or_setvalue() {
    let mut definition = generated::window_definition();
    definition.edit_box.text = "Read".into();
    definition.edit_box.is_read_only = true;
    definition.edit_box.max_length = Some(4);
    let mut form = Form::new(definition, false).unwrap();
    form.update(Size::new(640.0, 280.0), 1.0).unwrap();
    key(&mut form, Key::Tab, false);
    form.update(Size::new(640.0, 280.0), 1.0).unwrap();
    assert_eq!(form.ui.runtime.focused(), Some(form.editor));
    assert_eq!(form.ime_target(), None);
    assert!(form.ime_cursor().is_none());
    assert!(form.ui.caret_bounds().is_some());
    assert!(form.wake_after().is_some());
    key(&mut form, Key::Character("a".into()), true);
    let copied = form
        .event(PlatformEvent::Input(InputEvent::KeyDown {
            key: Key::Character("c".into()),
            modifiers: Modifiers {
                control: true,
                ..Default::default()
            },
            repeat: false,
        }))
        .unwrap();
    assert!(matches!(copied.clipboard.as_slice(),
        [rust_desktop_ui_platform_winit::ClipboardRequest::Copy(text)] if text == "Read"));
    form.event(PlatformEvent::Paste("edit".into())).unwrap();
    form.event(PlatformEvent::ImePreedit("語".into(), Some((3, 3))))
        .unwrap();
    form.event(PlatformEvent::ImeCommit("語".into())).unwrap();
    assert_eq!(form.text(), "Read");
    assert!(
        form.ui
            .document(form.editor)
            .unwrap()
            .composition()
            .is_none()
    );
    let tree = form.accessibility(1.0);
    let node = &tree
        .nodes
        .iter()
        .find(|(id, _)| *id == accesskit::NodeId(2))
        .unwrap()
        .1;
    assert!(node.is_read_only());
    assert!(!node.is_disabled());
    assert!(node.supports_action(accesskit::Action::Focus));
    assert!(!node.supports_action(accesskit::Action::SetValue));
    form.event(PlatformEvent::Accessibility(accesskit::ActionRequest {
        action: accesskit::Action::SetValue,
        target_tree: accesskit::TreeId::ROOT,
        target_node: accesskit::NodeId(2),
        data: Some(accesskit::ActionData::Value("edit".into())),
    }))
    .unwrap();
    assert_eq!(form.text(), "Read");
    form.ui.set_text(form.editor, "Code").unwrap();
    assert!(form.ui.set_text(form.editor, "Longer").is_err());
    assert_eq!(form.text(), "Code");
    form.event(PlatformEvent::Tick).unwrap();
    assert!(form.ui.drain_text_changes().is_empty());
}

#[test]
fn form_rejects_overlimit_input_and_consumes_change_events() {
    let mut definition = generated::window_definition();
    definition.edit_box.max_length = Some(2);
    let mut form = Form::new(definition, true).unwrap();
    form.update(Size::new(640.0, 280.0), 1.0).unwrap();
    key(&mut form, Key::Tab, false);
    form.event(PlatformEvent::Text("e\u{301}👩‍💻".into()))
        .unwrap();
    assert_eq!(form.text(), "e\u{301}👩‍💻");
    assert!(form.ui.drain_text_changes().is_empty());
    form.event(PlatformEvent::Paste("!".into())).unwrap();
    assert_eq!(form.text(), "e\u{301}👩‍💻");
    key(&mut form, Key::Character("a".into()), true);
    form.event(PlatformEvent::Accessibility(accesskit::ActionRequest {
        action: accesskit::Action::SetValue,
        target_tree: accesskit::TreeId::ROOT,
        target_node: accesskit::NodeId(2),
        data: Some(accesskit::ActionData::Value("abc".into())),
    }))
    .unwrap();
    assert_eq!(form.text(), "e\u{301}👩‍💻");
    form.event(PlatformEvent::Paste(
        "a\u{85}\u{2028}\u{2029}\r\n\tb".into(),
    ))
    .unwrap();
    assert_eq!(form.text(), "ab");
    assert!(form.ui.drain_text_changes().is_empty());
}
