use crate::{form::Form, generated};
use rust_desktop_ui_controls::{
    CommitOutcome, EditBoxStyle, EditorSettings, EditorValidation, Theme,
};
use rust_desktop_ui_core::{Color, Edges, InputEvent, Key, Modifiers, Mutation, Size};
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
    let style = form.ui.edit_box_style(form.editor).unwrap();
    assert_eq!(style.font_size, Some(14.0));
    assert_eq!(
        style.padding,
        Some(Edges {
            left: 12.0,
            top: 5.0,
            right: 12.0,
            bottom: 5.0
        })
    );
    assert_eq!(style.corner_radius, Some(4.0));
    assert_eq!(style.border_width, Some(1.0));
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
fn xaml_style_mounts_metrics_and_rgba_colors_and_preserves_them_across_theme_changes() {
    let definition = rust_desktop_ui_xaml::parse(
        r##"<Window xmlns="urn:rust-ui-engine:ui">
            <TextBox Text="Styled" FontSize="20" Padding="7,3,11,4" CornerRadius="6" BorderThickness="2"
                Background="#112233" Foreground="#445566" BorderBrush="#778899" FocusBrush="#AABBCC"
                PlaceholderForeground="#FF123456" SelectionBrush="#80336699" />
        </Window>"##,
    ).unwrap();
    let mut form = Form::new(definition, false).unwrap();
    form.update(Size::new(640.0, 280.0), 1.0).unwrap();
    let style = *form.ui.edit_box_style(form.editor).unwrap();
    assert_eq!(style.font_size, Some(20.0));
    assert_eq!(
        style.padding,
        Some(Edges {
            left: 7.0,
            top: 3.0,
            right: 11.0,
            bottom: 4.0
        })
    );
    assert_eq!(style.corner_radius, Some(6.0));
    assert_eq!(style.border_width, Some(2.0));
    assert_eq!(style.normal.background, Some(Color::rgb(17, 34, 51)));
    assert_eq!(style.normal.foreground, Some(Color::rgb(68, 85, 102)));
    assert_eq!(style.normal.border, Some(Color::rgb(119, 136, 153)));
    assert_eq!(style.normal.accent, Some(Color::rgb(170, 187, 204)));
    assert_eq!(style.normal.placeholder, Some(Color::rgb(18, 52, 86)));
    assert_eq!(
        style.normal.selection,
        Some(Color {
            r: 51.0 / 255.0,
            g: 102.0 / 255.0,
            b: 153.0 / 255.0,
            a: 128.0 / 255.0
        })
    );
    for theme in [Theme::FluentLight, Theme::FluentDark] {
        form.ui.set_theme(theme).unwrap();
        form.update(Size::new(640.0, 280.0), 1.0).unwrap();
        let text = form
            .scene()
            .texts
            .iter()
            .find(|text| text.text == "Styled")
            .unwrap();
        assert_eq!(text.font_size, 20.0);
        assert_eq!(text.color, Color::rgb(68, 85, 102));
        assert_eq!(form.ui.edit_box_style(form.editor), Some(&style));
        assert_eq!(form.text(), "Styled");
    }
    assert!(form.ui.drain_text_changes().is_empty());
}

#[test]
fn omitted_style_uses_live_theme_and_accessibility_uses_current_editor_settings() {
    let mut definition = rust_desktop_ui_xaml::WindowDefinition::default();
    definition.edit_box.placeholder = "Initial hint".into();
    let mut form = Form::new(definition, false).unwrap();
    assert_eq!(
        form.ui.edit_box_style(form.editor),
        Some(&EditBoxStyle::default())
    );
    form.ui
        .set_editor_settings(
            form.editor,
            EditorSettings {
                placeholder: "Updated hint".into(),
                is_read_only: true,
                max_length: Some(8),
                ..Default::default()
            },
        )
        .unwrap();
    form.update(Size::new(640.0, 280.0), 1.0).unwrap();
    let light_color = form
        .scene()
        .texts
        .iter()
        .find(|text| text.text == "Updated hint")
        .unwrap()
        .color;
    form.ui.set_theme(Theme::FluentDark).unwrap();
    form.update(Size::new(640.0, 280.0), 1.0).unwrap();
    let dark_color = form
        .scene()
        .texts
        .iter()
        .find(|text| text.text == "Updated hint")
        .unwrap()
        .color;
    assert_ne!(light_color, dark_color);
    let tree = form.accessibility(1.0);
    let node = &tree
        .nodes
        .iter()
        .find(|(id, _)| *id == accesskit::NodeId(2))
        .unwrap()
        .1;
    assert_eq!(node.description(), Some("Updated hint"));
    assert!(node.is_read_only());
    assert_eq!(form.ui.max_length(form.editor), Some(Some(8)));
    assert!(form.ui.drain_text_changes().is_empty());
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

#[test]
fn required_form_reports_error_then_accepts_and_cancels_with_visible_status() {
    let mut definition = generated::window_definition();
    definition.edit_box.validation_brush = Some([180, 30, 60, 255]);
    let mut form = Form::new(definition, false).unwrap();
    form.update(Size::new(640.0, 280.0), 1.0).unwrap();
    assert_eq!(form.ui.accepted_value(form.editor), Some(""));
    assert_eq!(form.ui.validation_error(form.editor), None);
    assert_eq!(
        form.ui.edit_box_style(form.editor).unwrap().invalid.border,
        Some(Color::rgb(180, 30, 60))
    );
    key(&mut form, Key::Tab, false);
    key(&mut form, Key::Enter, false);
    form.update(Size::new(640.0, 280.0), 1.0).unwrap();
    let message = "Введите текст перед подтверждением.";
    assert_eq!(form.ui.validation_error(form.editor), Some(message));
    assert_eq!(form.ui.accepted_value(form.editor), Some(""));
    assert!(form.scene().texts.iter().any(|run| run.text == message));
    let tree = form.accessibility(1.0);
    let edit = &tree
        .nodes
        .iter()
        .find(|(id, _)| *id == accesskit::NodeId(2))
        .unwrap()
        .1;
    assert_eq!(edit.invalid(), Some(accesskit::Invalid::True));
    assert!(edit.is_required());
    let status_id = edit.error_message().unwrap();
    let status = &tree
        .nodes
        .iter()
        .find(|(id, _)| *id == status_id)
        .unwrap()
        .1;
    assert_eq!(status.label(), Some(message));
    assert_eq!(status.live(), Some(accesskit::Live::Polite));
    form.event(PlatformEvent::Text("Готово".into())).unwrap();
    form.update(Size::new(640.0, 280.0), 1.0).unwrap();
    assert_eq!(form.ui.validation_error(form.editor), None);
    assert_eq!(form.ui.is_edit_dirty(form.editor), Some(true));
    assert!(
        form.scene()
            .texts
            .iter()
            .any(|run| run.text == "Есть неподтверждённые изменения.")
    );
    key(&mut form, Key::Enter, false);
    form.update(Size::new(640.0, 280.0), 1.0).unwrap();
    assert_eq!(form.ui.accepted_value(form.editor), Some("Готово"));
    assert_eq!(form.ui.is_edit_dirty(form.editor), Some(false));
    assert!(
        form.scene()
            .texts
            .iter()
            .any(|run| run.text == "Значение подтверждено.")
    );
    form.event(PlatformEvent::Text("!".into())).unwrap();
    key(&mut form, Key::Escape, false);
    form.update(Size::new(640.0, 280.0), 1.0).unwrap();
    assert_eq!(form.text(), "Готово");
    assert_eq!(form.ui.is_edit_dirty(form.editor), Some(false));
    assert!(!form.ui.document(form.editor).unwrap().can_undo());
    assert!(
        form.scene()
            .texts
            .iter()
            .any(|run| run.text == "Изменения отменены.")
    );
    let tree = form.accessibility(1.0);
    let edit = &tree
        .nodes
        .iter()
        .find(|(id, _)| *id == accesskit::NodeId(2))
        .unwrap()
        .1;
    assert_eq!(edit.invalid(), None);
    assert_eq!(edit.error_message(), None);
    assert!(form.ui.drain_edit_events().is_empty());
}

#[test]
fn lost_focus_acceptance_can_be_disabled_and_invalid_value_does_not_trap_focus() {
    for automatic in [false, true] {
        let mut definition = generated::window_definition();
        definition.edit_box.commit_on_lost_focus = automatic;
        let mut form = Form::new(definition, false).unwrap();
        form.update(Size::new(640.0, 280.0), 1.0).unwrap();
        key(&mut form, Key::Tab, false);
        form.event(PlatformEvent::Text("value".into())).unwrap();
        form.event(PlatformEvent::Input(InputEvent::WindowFocused(false)))
            .unwrap();
        form.update(Size::new(640.0, 280.0), 1.0).unwrap();
        assert_eq!(
            form.ui.accepted_value(form.editor),
            Some(if automatic { "value" } else { "" })
        );
        assert_eq!(form.ui.is_edit_dirty(form.editor), Some(!automatic));
        assert_eq!(form.ime_target(), None);
    }
    let mut form = form();
    key(&mut form, Key::Tab, false);
    form.event(PlatformEvent::Text(" ".into())).unwrap();
    form.event(PlatformEvent::Input(InputEvent::WindowFocused(false)))
        .unwrap();
    form.update(Size::new(640.0, 280.0), 1.0).unwrap();
    assert_eq!(form.ui.accepted_value(form.editor), Some(""));
    assert_eq!(form.text(), " ");
    assert!(form.ui.validation_error(form.editor).is_some());
    assert_eq!(form.ime_target(), None);
}

#[test]
fn validation_status_wraps_and_clips_long_message_and_tracks_live_required_rules() {
    let mut definition = generated::window_definition();
    let message = "Ошибка ".repeat(200);
    definition.edit_box.validation_message = message.clone();
    let mut form = Form::new(definition, false).unwrap();
    form.update(Size::new(320.0, 180.0), 1.0).unwrap();
    assert!(matches!(
        form.ui.commit_edit(form.editor).unwrap(),
        CommitOutcome::Invalid(_)
    ));
    form.update(Size::new(320.0, 180.0), 1.0).unwrap();
    let run = form
        .scene()
        .texts
        .iter()
        .find(|run| run.text == message)
        .unwrap();
    assert!(run.wrap);
    assert!(run.clip.width <= 256.0 && run.clip.height <= 56.0);
    assert!(run.is_valid());
    let tree = form.accessibility(1.0);
    let edit = &tree
        .nodes
        .iter()
        .find(|(id, _)| *id == accesskit::NodeId(2))
        .unwrap()
        .1;
    let error_id = edit.error_message().unwrap();
    assert_eq!(
        tree.nodes
            .iter()
            .find(|(id, _)| *id == error_id)
            .unwrap()
            .1
            .label(),
        Some(message.as_str())
    );
    form.ui
        .set_editor_validation(form.editor, EditorValidation::default())
        .unwrap();
    let tree = form.accessibility(1.0);
    let edit = &tree
        .nodes
        .iter()
        .find(|(id, _)| *id == accesskit::NodeId(2))
        .unwrap()
        .1;
    assert!(!edit.is_required());
    assert_eq!(edit.invalid(), None);
    assert_eq!(edit.error_message(), None);
}

#[test]
fn composition_stays_separate_from_acceptance_and_form_drains_transaction_events() {
    let mut form = form();
    key(&mut form, Key::Tab, false);
    form.event(PlatformEvent::Text("draft".into())).unwrap();
    form.event(PlatformEvent::ImePreedit("語".into(), Some((3, 3))))
        .unwrap();
    assert_eq!(
        form.ui.commit_edit(form.editor).unwrap(),
        CommitOutcome::CompositionActive
    );
    assert_eq!(form.ui.accepted_value(form.editor), Some(""));
    assert_eq!(form.ui.validation_error(form.editor), None);
    assert!(form.ui.drain_edit_events().is_empty());
    form.event(PlatformEvent::ImeCommit("語".into())).unwrap();
    assert_eq!(form.ui.accepted_value(form.editor), Some(""));
    key(&mut form, Key::Enter, false);
    assert_eq!(form.ui.accepted_value(form.editor), Some("draft語"));
    assert!(form.ui.drain_edit_events().is_empty());
}
