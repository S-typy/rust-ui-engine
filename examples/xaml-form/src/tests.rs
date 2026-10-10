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
