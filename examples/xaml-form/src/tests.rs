use crate::{form::Form, generated_controls as generated};
use rust_desktop_ui_controls::{
    CommitOutcome, EditBoxStyle, EditorSettings, EditorValidation, Theme,
};
use rust_desktop_ui_core::{Color, Edges, InputEvent, Key, Modifiers, Mutation, Size};
use rust_desktop_ui_platform_winit::{DesktopApp, PlatformEvent, accesskit};

fn editor_definition(
    definition: &mut rust_desktop_ui_xaml::WindowDefinition,
) -> &mut rust_desktop_ui_xaml::EditBoxDefinition {
    fn find(
        element: &mut rust_desktop_ui_xaml::ElementDefinition,
    ) -> Option<&mut rust_desktop_ui_xaml::EditBoxDefinition> {
        match &mut element.kind {
            rust_desktop_ui_xaml::ElementKind::TextBox(editor) => Some(editor),
            rust_desktop_ui_xaml::ElementKind::StackPanel(panel) => {
                panel.children.iter_mut().find_map(find)
            }
            _ => None,
        }
    }
    find(&mut definition.content).expect("test markup contains an editor")
}

fn form() -> Form {
    let mut form = Form::new(generated::window_definition(), false).unwrap();
    form.update(Size::new(640.0, 512.0), 1.0).unwrap();
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
    form.accessibility(1.0);
}

fn markup(content: &str) -> Form {
    let source = format!(
        r#"<Window xmlns="urn:rust-ui-engine:ui" xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">{content}</Window>"#
    );
    let mut form = Form::new(rust_desktop_ui_xaml::parse(&source).unwrap(), false).unwrap();
    form.update(Size::new(640.0, 400.0), 1.0).unwrap();
    form
}

fn tab(form: &mut Form, shift: bool) {
    form.event(PlatformEvent::Input(InputEvent::KeyDown {
        key: Key::Tab,
        modifiers: Modifiers {
            shift,
            ..Default::default()
        },
        repeat: false,
    }))
    .unwrap();
}

fn click(form: &mut Form, name: &str) {
    let bounds = form
        .ui
        .runtime
        .bounds(form.find_name(name).unwrap())
        .unwrap();
    let position = rust_desktop_ui_core::Point::new(
        bounds.x + bounds.width / 2.0,
        bounds.y + bounds.height / 2.0,
    );
    for input in [
        InputEvent::PointerDown {
            position,
            button: rust_desktop_ui_core::PointerButton::Primary,
            modifiers: Modifiers::default(),
        },
        InputEvent::PointerUp {
            position,
            button: rust_desktop_ui_core::PointerButton::Primary,
            modifiers: Modifiers::default(),
        },
    ] {
        form.event(PlatformEvent::Input(input)).unwrap();
    }
}

fn accessible_action(
    form: &mut Form,
    name: &str,
    action: accesskit::Action,
    data: Option<accesskit::ActionData>,
) {
    form.accessibility(1.0);
    let target_node = form.accessible.id(form.find_name(name).unwrap()).unwrap();
    form.event(PlatformEvent::Accessibility(accesskit::ActionRequest {
        action,
        target_tree: accesskit::TreeId::ROOT,
        target_node,
        data,
    }))
    .unwrap();
}

#[test]
fn compiled_markup_matches_source_and_mounts_named_editbox() {
    let expected = rust_desktop_ui_xaml::parse(include_str!("../ControlsWindow.xaml")).unwrap();
    let compiled = generated::window_definition();
    assert_eq!(compiled, expected);
    let mut form = form();
    assert_eq!(form.title(), expected.title);
    assert_eq!(form.find_name("Message"), Some(form.editor()));
    assert_eq!(form.find_name("Missing"), None);
    let bounds = form.ui.runtime.bounds(form.editor()).unwrap();
    assert_eq!(
        (bounds.x, bounds.y, bounds.width, bounds.height),
        (32.0, 104.0, 576.0, 36.0)
    );
    assert_eq!(form.text(), "");
    assert_eq!(form.ui.is_read_only(form.editor()), Some(false));
    assert_eq!(form.ui.max_length(form.editor()), Some(Some(64)));
    let style = form.ui.edit_box_style(form.editor()).unwrap();
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
    assert_eq!(form.ui.runtime.bounds(form.editor()).unwrap().width, 296.0);
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
    form.update(Size::new(640.0, 400.0), 1.0).unwrap();
    let style = *form.ui.edit_box_style(form.editor()).unwrap();
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
        form.update(Size::new(640.0, 400.0), 1.0).unwrap();
        let text = form
            .scene()
            .texts
            .iter()
            .find(|text| text.text == "Styled")
            .unwrap();
        assert_eq!(text.font_size, 20.0);
        assert_eq!(text.color, Color::rgb(68, 85, 102));
        assert_eq!(form.ui.edit_box_style(form.editor()), Some(&style));
        assert_eq!(form.text(), "Styled");
    }
    assert!(form.ui.drain_text_changes().is_empty());
}

#[test]
fn omitted_style_uses_live_theme_and_accessibility_uses_current_editor_settings() {
    let mut definition = rust_desktop_ui_xaml::WindowDefinition::default();
    editor_definition(&mut definition).placeholder = "Initial hint".into();
    let mut form = Form::new(definition, false).unwrap();
    assert_eq!(
        form.ui.edit_box_style(form.editor()),
        Some(&EditBoxStyle::default())
    );
    form.ui
        .set_editor_settings(
            form.editor(),
            EditorSettings {
                placeholder: "Updated hint".into(),
                is_read_only: true,
                max_length: Some(8),
                ..Default::default()
            },
        )
        .unwrap();
    form.update(Size::new(640.0, 400.0), 1.0).unwrap();
    let light_color = form
        .scene()
        .texts
        .iter()
        .find(|text| text.text == "Updated hint")
        .unwrap()
        .color;
    form.ui.set_theme(Theme::FluentDark).unwrap();
    form.update(Size::new(640.0, 400.0), 1.0).unwrap();
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
        .find(|(id, _)| *id == form.accessible.id(form.editor()).unwrap())
        .unwrap()
        .1;
    assert_eq!(node.description(), Some("Updated hint"));
    assert!(node.is_read_only());
    assert_eq!(form.ui.max_length(form.editor()), Some(Some(8)));
    assert!(form.ui.drain_text_changes().is_empty());
}

#[test]
fn tab_edit_selection_undo_and_resize_keep_document() {
    let mut form = form();
    key(&mut form, Key::Tab, false);
    assert_eq!(form.ui.runtime.focused(), Some(form.editor()));
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
        .apply(Mutation::Focus(Some(form.editor())))
        .unwrap();
    form.event(PlatformEvent::ImePreedit("日本".into(), Some((6, 6))))
        .unwrap();
    assert_eq!(form.text(), "");
    form.update(Size::new(640.0, 400.0), 1.0).unwrap();
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
            .document(form.editor())
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
        .find(|(id, _)| *id == form.accessible.id(form.editor()).unwrap())
        .unwrap()
        .1;
    assert_eq!(node.role(), accesskit::Role::TextInput);
    assert_eq!(node.label(), Some("Текст"));
    assert_eq!(node.value(), Some(""));
    assert_eq!(node.bounds().unwrap().x0, 48.0);
    form.event(PlatformEvent::Accessibility(accesskit::ActionRequest {
        action: accesskit::Action::SetValue,
        target_tree: accesskit::TreeId::ROOT,
        target_node: form.accessible.id(form.editor()).unwrap(),
        data: Some(accesskit::ActionData::Value("Данные".into())),
    }))
    .unwrap();
    assert_eq!(form.text(), "Данные");
    form.ui.set_enabled(form.editor(), false).unwrap();
    form.event(PlatformEvent::Accessibility(accesskit::ActionRequest {
        action: accesskit::Action::SetValue,
        target_tree: accesskit::TreeId::ROOT,
        target_node: form.accessible.id(form.editor()).unwrap(),
        data: Some(accesskit::ActionData::Value("Запрещено".into())),
    }))
    .unwrap();
    assert_eq!(form.text(), "Данные");
}

#[test]
fn readonly_form_keeps_focus_copy_and_programmatic_updates_without_ime_or_setvalue() {
    let mut definition = generated::window_definition();
    editor_definition(&mut definition).text = "Read".into();
    editor_definition(&mut definition).is_read_only = true;
    editor_definition(&mut definition).max_length = Some(4);
    let mut form = Form::new(definition, false).unwrap();
    form.update(Size::new(640.0, 400.0), 1.0).unwrap();
    key(&mut form, Key::Tab, false);
    form.update(Size::new(640.0, 400.0), 1.0).unwrap();
    assert_eq!(form.ui.runtime.focused(), Some(form.editor()));
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
            .document(form.editor())
            .unwrap()
            .composition()
            .is_none()
    );
    let tree = form.accessibility(1.0);
    let node = &tree
        .nodes
        .iter()
        .find(|(id, _)| *id == form.accessible.id(form.editor()).unwrap())
        .unwrap()
        .1;
    assert!(node.is_read_only());
    assert!(!node.is_disabled());
    assert!(node.supports_action(accesskit::Action::Focus));
    assert!(!node.supports_action(accesskit::Action::SetValue));
    form.event(PlatformEvent::Accessibility(accesskit::ActionRequest {
        action: accesskit::Action::SetValue,
        target_tree: accesskit::TreeId::ROOT,
        target_node: form.accessible.id(form.editor()).unwrap(),
        data: Some(accesskit::ActionData::Value("edit".into())),
    }))
    .unwrap();
    assert_eq!(form.text(), "Read");
    form.ui.set_text(form.editor(), "Code").unwrap();
    assert!(form.ui.set_text(form.editor(), "Longer").is_err());
    assert_eq!(form.text(), "Code");
    form.event(PlatformEvent::Tick).unwrap();
    assert!(form.ui.drain_text_changes().is_empty());
}

#[test]
fn form_rejects_overlimit_input_and_consumes_change_events() {
    let mut definition = generated::window_definition();
    editor_definition(&mut definition).max_length = Some(2);
    let mut form = Form::new(definition, true).unwrap();
    form.update(Size::new(640.0, 400.0), 1.0).unwrap();
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
        target_node: form.accessible.id(form.editor()).unwrap(),
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
    editor_definition(&mut definition).validation_brush = Some([180, 30, 60, 255]);
    let mut form = Form::new(definition, false).unwrap();
    form.update(Size::new(640.0, 400.0), 1.0).unwrap();
    assert_eq!(form.ui.accepted_value(form.editor()), Some(""));
    assert_eq!(form.ui.validation_error(form.editor()), None);
    assert_eq!(
        form.ui
            .edit_box_style(form.editor())
            .unwrap()
            .invalid
            .border,
        Some(Color::rgb(180, 30, 60))
    );
    key(&mut form, Key::Tab, false);
    key(&mut form, Key::Enter, false);
    form.update(Size::new(640.0, 400.0), 1.0).unwrap();
    let message = "Введите текст перед подтверждением.";
    assert_eq!(form.ui.validation_error(form.editor()), Some(message));
    assert_eq!(form.ui.accepted_value(form.editor()), Some(""));
    assert!(form.scene().texts.iter().any(|run| run.text == message));
    let tree = form.accessibility(1.0);
    let edit = &tree
        .nodes
        .iter()
        .find(|(id, _)| *id == form.accessible.id(form.editor()).unwrap())
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
    form.update(Size::new(640.0, 400.0), 1.0).unwrap();
    assert_eq!(form.ui.validation_error(form.editor()), None);
    assert_eq!(form.ui.is_edit_dirty(form.editor()), Some(true));
    assert!(
        form.scene()
            .texts
            .iter()
            .any(|run| run.text == "Есть неподтверждённые изменения.")
    );
    key(&mut form, Key::Enter, false);
    form.update(Size::new(640.0, 400.0), 1.0).unwrap();
    assert_eq!(form.ui.accepted_value(form.editor()), Some("Готово"));
    assert_eq!(form.ui.is_edit_dirty(form.editor()), Some(false));
    assert!(
        form.scene()
            .texts
            .iter()
            .any(|run| run.text == "Значение подтверждено.")
    );
    form.event(PlatformEvent::Text("!".into())).unwrap();
    key(&mut form, Key::Escape, false);
    form.update(Size::new(640.0, 400.0), 1.0).unwrap();
    assert_eq!(form.text(), "Готово");
    assert_eq!(form.ui.is_edit_dirty(form.editor()), Some(false));
    assert!(!form.ui.document(form.editor()).unwrap().can_undo());
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
        .find(|(id, _)| *id == form.accessible.id(form.editor()).unwrap())
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
        editor_definition(&mut definition).commit_on_lost_focus = automatic;
        let mut form = Form::new(definition, false).unwrap();
        form.update(Size::new(640.0, 400.0), 1.0).unwrap();
        key(&mut form, Key::Tab, false);
        form.event(PlatformEvent::Text("value".into())).unwrap();
        form.event(PlatformEvent::Input(InputEvent::WindowFocused(false)))
            .unwrap();
        form.update(Size::new(640.0, 400.0), 1.0).unwrap();
        assert_eq!(
            form.ui.accepted_value(form.editor()),
            Some(if automatic { "value" } else { "" })
        );
        assert_eq!(form.ui.is_edit_dirty(form.editor()), Some(!automatic));
        assert_eq!(form.ime_target(), None);
    }
    let mut form = form();
    key(&mut form, Key::Tab, false);
    form.event(PlatformEvent::Text(" ".into())).unwrap();
    form.event(PlatformEvent::Input(InputEvent::WindowFocused(false)))
        .unwrap();
    form.update(Size::new(640.0, 400.0), 1.0).unwrap();
    assert_eq!(form.ui.accepted_value(form.editor()), Some(""));
    assert_eq!(form.text(), " ");
    assert!(form.ui.validation_error(form.editor()).is_some());
    assert_eq!(form.ime_target(), None);
}

#[test]
fn validation_status_wraps_and_clips_long_message_and_tracks_live_required_rules() {
    let mut definition = generated::window_definition();
    let message = "Ошибка ".repeat(200);
    editor_definition(&mut definition).validation_message = message.clone();
    let mut form = Form::new(definition, false).unwrap();
    form.update(Size::new(320.0, 448.0), 1.0).unwrap();
    assert!(matches!(
        form.ui.commit_edit(form.editor()).unwrap(),
        CommitOutcome::Invalid(_)
    ));
    form.update(Size::new(320.0, 448.0), 1.0).unwrap();
    let run = form
        .scene()
        .texts
        .iter()
        .find(|run| run.text == message)
        .unwrap();
    assert!(run.wrap);
    assert!(run.clip.width > 0.0 && run.clip.width <= 256.0);
    assert!(run.clip.height > 0.0 && run.clip.height <= 56.0);
    assert!(run.is_valid());
    let tree = form.accessibility(1.0);
    let edit = &tree
        .nodes
        .iter()
        .find(|(id, _)| *id == form.accessible.id(form.editor()).unwrap())
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
        .set_editor_validation(form.editor(), EditorValidation::default())
        .unwrap();
    let tree = form.accessibility(1.0);
    let edit = &tree
        .nodes
        .iter()
        .find(|(id, _)| *id == form.accessible.id(form.editor()).unwrap())
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
        form.ui.commit_edit(form.editor()).unwrap(),
        CommitOutcome::CompositionActive
    );
    assert_eq!(form.ui.accepted_value(form.editor()), Some(""));
    assert_eq!(form.ui.validation_error(form.editor()), None);
    assert!(form.ui.drain_edit_events().is_empty());
    form.event(PlatformEvent::ImeCommit("語".into())).unwrap();
    assert_eq!(form.ui.accepted_value(form.editor()), Some(""));
    key(&mut form, Key::Enter, false);
    assert_eq!(form.ui.accepted_value(form.editor()), Some("draft語"));
    assert!(form.ui.drain_edit_events().is_empty());
}

#[test]
fn nested_tree_resizes_without_replacing_widgets_and_tab_skips_disabled_subtrees() {
    let mut form = markup(
        r#"<StackPanel x:Name="Outer" Spacing="8" Padding="4">
        <TextBox x:Name="First" />
        <StackPanel x:Name="Disabled" IsEnabled="False"><Button x:Name="Skip" Content="Skip" /></StackPanel>
        <StackPanel x:Name="Row" Orientation="Horizontal" Spacing="12">
            <CheckBox x:Name="Check" Content="Enabled option" />
            <Button x:Name="Last" Content="A longer button caption" />
        </StackPanel>
    </StackPanel>"#,
    );
    let first = form.find_name("First").unwrap();
    let check = form.find_name("Check").unwrap();
    let last = form.find_name("Last").unwrap();
    let row = form.find_name("Row").unwrap();
    assert_eq!(form.ui.runtime.tree().parent(check), Some(row));
    let check_bounds = form.ui.runtime.bounds(check).unwrap();
    let last_bounds = form.ui.runtime.bounds(last).unwrap();
    assert!(check_bounds.width > 40.0 && last_bounds.width > 80.0);
    assert!((last_bounds.x - check_bounds.x - check_bounds.width - 12.0).abs() < 0.01);
    for expected in [first, check, last, first] {
        tab(&mut form, false);
        assert_eq!(form.ui.runtime.focused(), Some(expected));
    }
    for expected in [last, check, first] {
        tab(&mut form, true);
        assert_eq!(form.ui.runtime.focused(), Some(expected));
    }
    form.event(PlatformEvent::Text("retained".into())).unwrap();
    form.accessibility(1.0);
    let accessible_first = form.accessible.id(first);
    let original_width = form.ui.runtime.bounds(first).unwrap().width;
    form.update(Size::new(480.0, 360.0), 1.5).unwrap();
    form.accessibility(1.5);
    assert_eq!(form.find_name("First"), Some(first));
    assert_eq!(form.ui.runtime.focused(), Some(first));
    assert_eq!(form.ui.document(first).unwrap().text(), "retained");
    assert_eq!(form.accessible.id(first), accessible_first);
    assert_eq!(
        form.ui.runtime.bounds(first).unwrap().width,
        original_width - 160.0
    );
    assert_eq!(
        form.ui.runtime.bounds(last).unwrap().width,
        last_bounds.width
    );
}

#[test]
fn confirm_command_and_checkbox_use_pointer_keyboard_and_accessible_routes() {
    let mut form = form();
    let message = form.editor();
    let option = form.find_name("Option").unwrap();
    click(&mut form, "Confirm");
    assert!(form.ui.validation_error(message).is_some());
    assert_eq!(form.ui.accepted_value(message), Some(""));
    click(&mut form, "Message");
    form.event(PlatformEvent::Text("First".into())).unwrap();
    click(&mut form, "Confirm");
    assert_eq!(form.ui.accepted_value(message), Some("First"));
    assert_eq!(form.ui.validation_error(message), None);
    assert!(form.ui.commands_mut().drain_invocations().is_empty());
    click(&mut form, "Option");
    assert_eq!(form.ui.checked(option), Some(true));
    assert_eq!(form.ui.is_read_only(message), Some(true));
    click(&mut form, "Message");
    form.event(PlatformEvent::Text("blocked".into())).unwrap();
    assert_eq!(form.text(), "First");
    tab(&mut form, false);
    assert_eq!(form.ui.runtime.focused(), Some(option));
    key(&mut form, Key::Space, false);
    assert_eq!(form.ui.checked(option), Some(true));
    form.event(PlatformEvent::Input(InputEvent::KeyUp {
        key: Key::Space,
        modifiers: Modifiers::default(),
    }))
    .unwrap();
    assert_eq!(form.ui.checked(option), Some(false));
    assert_eq!(form.ui.is_read_only(message), Some(false));
    accessible_action(
        &mut form,
        "Message",
        accesskit::Action::SetValue,
        Some(accesskit::ActionData::Value("Accessible".into())),
    );
    assert_eq!(form.ui.accepted_value(message), Some("First"));
    accessible_action(&mut form, "Confirm", accesskit::Action::Click, None);
    assert_eq!(form.ui.accepted_value(message), Some("Accessible"));
    accessible_action(&mut form, "Option", accesskit::Action::Click, None);
    assert_eq!(form.ui.checked(option), Some(true));
}

#[test]
fn multiple_editors_keep_independent_values_errors_composition_and_accessible_identity() {
    let mut form = markup(
        r#"<StackPanel>
        <TextBox x:Name="A" AutomationName="First editor" IsRequired="True" ValidationMessage="First required" CommitOnLostFocus="False" />
        <TextBox x:Name="B" AutomationName="Second editor" IsRequired="True" ValidationMessage="Second required" CommitOnLostFocus="False" />
    </StackPanel>"#,
    );
    let a = form.find_name("A").unwrap();
    let b = form.find_name("B").unwrap();
    assert_eq!(
        form.ui.commit_edit(a).unwrap(),
        CommitOutcome::Invalid("First required".into())
    );
    assert_eq!(
        form.ui.commit_edit(b).unwrap(),
        CommitOutcome::Invalid("Second required".into())
    );
    let tree = form.accessibility(1.0);
    let a_id = form.accessible.id(a).unwrap();
    let b_id = form.accessible.id(b).unwrap();
    let a_node = &tree.nodes.iter().find(|(id, _)| *id == a_id).unwrap().1;
    let b_node = &tree.nodes.iter().find(|(id, _)| *id == b_id).unwrap().1;
    assert_ne!(a_node.error_message(), b_node.error_message());
    for (node, message) in [(a_node, "First required"), (b_node, "Second required")] {
        assert!(node.is_required());
        assert_eq!(
            tree.nodes
                .iter()
                .find(|(id, _)| Some(*id) == node.error_message())
                .unwrap()
                .1
                .label(),
            Some(message)
        );
    }
    accessible_action(&mut form, "A", accesskit::Action::Focus, None);
    form.event(PlatformEvent::Text("accepted".into())).unwrap();
    key(&mut form, Key::Enter, false);
    form.event(PlatformEvent::Text(" draft".into())).unwrap();
    tab(&mut form, false);
    assert_eq!(form.ime_target(), Some(b));
    assert_eq!(form.ui.accepted_value(a), Some("accepted"));
    assert_eq!(form.ui.document(a).unwrap().text(), "accepted draft");
    form.event(PlatformEvent::ImePreedit("語".into(), Some((3, 3))))
        .unwrap();
    assert_eq!(form.ime_composition_active(), Some(true));
    assert_eq!(form.ui.document(b).unwrap().text(), "");
    form.update(Size::new(500.0, 360.0), 2.0).unwrap();
    assert_eq!(form.ime_target(), Some(b));
    assert_eq!(form.ime_composition_active(), Some(true));
    form.event(PlatformEvent::ImeCommit("語".into())).unwrap();
    key(&mut form, Key::Enter, false);
    accessible_action(&mut form, "A", accesskit::Action::Focus, None);
    key(&mut form, Key::Escape, false);
    assert_eq!(form.ui.document(a).unwrap().text(), "accepted");
    assert_eq!(form.ui.document(b).unwrap().text(), "語");
    assert_eq!(form.ui.accepted_value(b), Some("語"));
    assert!(!form.ui.document(a).unwrap().can_undo());
    assert!(form.ui.document(b).unwrap().can_undo());
    let tree = form.accessibility(2.0);
    assert_eq!(form.accessible.id(a), Some(a_id));
    assert_eq!(form.accessible.id(b), Some(b_id));
    assert_eq!(tree.focus, a_id);
    assert_eq!(form.ime_target(), Some(a));
}

#[test]
fn accessibility_preserves_hierarchy_and_names_without_changing_visible_content() {
    let mut form = markup(
        r#"<StackPanel x:Name="Container" AutomationName="Form group">
        <TextBlock x:Name="Caption" Text="Visible title" AutomationName="Spoken title" />
        <StackPanel x:Name="Inactive" IsEnabled="False">
            <Button x:Name="Button" Content="Visible button" AutomationName="Spoken button" />
            <CheckBox x:Name="Check" Content="Visible check" AutomationName="Spoken check" IsChecked="True" />
            <TextBox x:Name="Read" Text="Data" IsReadOnly="True" />
        </StackPanel>
    </StackPanel>"#,
    );
    let tree = form.accessibility(1.5);
    let inactive = form.find_name("Inactive").unwrap();
    let inactive_id = form.accessible.id(inactive).unwrap();
    let parent = &tree
        .nodes
        .iter()
        .find(|(id, _)| *id == inactive_id)
        .unwrap()
        .1;
    for name in ["Button", "Check", "Read"] {
        let widget = form.find_name(name).unwrap();
        let id = form.accessible.id(widget).unwrap();
        assert!(parent.children().contains(&id));
        let node = &tree
            .nodes
            .iter()
            .find(|(node_id, _)| *node_id == id)
            .unwrap()
            .1;
        assert!(node.is_disabled());
        assert!(!node.supports_action(accesskit::Action::Focus));
        assert!(!node.supports_action(accesskit::Action::Click));
        assert!(!node.supports_action(accesskit::Action::SetValue));
    }
    let check = form.find_name("Check").unwrap();
    let check_node = &tree
        .nodes
        .iter()
        .find(|(id, _)| Some(*id) == form.accessible.id(check))
        .unwrap()
        .1;
    assert_eq!(check_node.label(), Some("Spoken check"));
    assert_eq!(check_node.toggled(), Some(accesskit::Toggled::True));
    for visible in ["Visible title", "Visible button", "Visible check"] {
        assert!(form.scene().texts.iter().any(|run| run.text == visible));
    }
    accessible_action(&mut form, "Check", accesskit::Action::Click, None);
    assert_eq!(form.ui.checked(check), Some(true));
    form.ui.set_enabled(inactive, true).unwrap();
    form.update(Size::new(640.0, 400.0), 1.0).unwrap();
    accessible_action(&mut form, "Check", accesskit::Action::Click, None);
    assert_eq!(form.ui.checked(check), Some(false));
}

#[test]
fn legacy_single_editor_and_forms_without_editors_mount_without_injected_elements() {
    let legacy = markup(r#"<TextBox x:Name="Legacy" Text="Original" />"#);
    let editor = legacy.find_name("Legacy").unwrap();
    let bounds = legacy.ui.runtime.bounds(editor).unwrap();
    assert_eq!(
        (bounds.x, bounds.y, bounds.width, bounds.height),
        (32.0, 32.0, 576.0, 36.0)
    );
    assert_eq!(legacy.ui.document(editor).unwrap().text(), "Original");
    assert_eq!(legacy.view.names.len(), 1);
    let mut empty = markup(
        r#"<StackPanel><TextBlock x:Name="Status" Text="Application text" /><Button x:Name="Plain" Content="Action" /></StackPanel>"#,
    );
    assert!(empty.view.editors.is_empty());
    assert_eq!(empty.ime_target(), None);
    assert_eq!(empty.wake_after(), None);
    assert!(
        empty
            .scene()
            .texts
            .iter()
            .any(|run| run.text == "Application text")
    );
    tab(&mut empty, false);
    assert_eq!(empty.ui.runtime.focused(), empty.find_name("Plain"));
    key(&mut empty, Key::Enter, false);
    assert_eq!(empty.ime_target(), None);
}

#[test]
fn command_registration_is_explicit_and_generic_view_preserves_registry_behavior() {
    use crate::view::MountedView;
    use rust_desktop_ui_controls::Controls;
    let definition = rust_desktop_ui_xaml::parse(r#"<Window xmlns="urn:rust-ui-engine:ui"><Button Command="Save" Content="Save document" /></Window>"#).unwrap();
    assert!(
        matches!(Form::new(definition.clone(), false), Err(error) if error.contains("Unregistered XAML command 'Save'"))
    );
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let root = ui.root();
    ui.commands_mut().register("Save", "Registered label");
    let view = MountedView::mount(&mut ui, root, &definition.content).unwrap();
    assert!(view.editors.is_empty());
    ui.update(Size::new(640.0, 400.0)).unwrap();
    let button = ui
        .semantics()
        .into_iter()
        .find(|node| node.role == rust_desktop_ui_controls::Role::Button)
        .unwrap()
        .id;
    ui.activate(button).unwrap();
    assert_eq!(ui.commands_mut().drain_invocations()[0].id.0, "Save");
    ui.commands_mut().set_enabled(&"Save".into(), false);
    ui.update(Size::new(640.0, 400.0)).unwrap();
    ui.activate(button).unwrap();
    assert!(ui.commands_mut().drain_invocations().is_empty());
    let definition = rust_desktop_ui_xaml::parse(
        r#"<Window xmlns="urn:rust-ui-engine:ui"><Button Command="Confirm" /></Window>"#,
    )
    .unwrap();
    assert!(
        matches!(Form::new(definition, false), Err(error) if error.contains("requires a TextBox named Message"))
    );
}

#[test]
fn programmatic_tree_rejects_duplicate_names_and_excessive_depth_before_mount() {
    use rust_desktop_ui_xaml::{
        ElementDefinition, ElementKind, StackPanelDefinition, WindowDefinition,
    };
    let mut leaf = ElementDefinition::default();
    leaf.common.name = Some("Duplicate".into());
    let definition = WindowDefinition {
        content: ElementDefinition {
            kind: ElementKind::StackPanel(StackPanelDefinition {
                children: vec![leaf.clone(), leaf],
                ..Default::default()
            }),
            ..Default::default()
        },
        ..Default::default()
    };
    assert!(
        matches!(Form::new(definition, false), Err(error) if error.contains("Duplicate XAML name"))
    );
    let mut content = ElementDefinition::default();
    for _ in 0..rust_desktop_ui_xaml::MAX_ELEMENT_DEPTH {
        content = ElementDefinition {
            kind: ElementKind::StackPanel(StackPanelDefinition {
                children: vec![content],
                ..Default::default()
            }),
            ..Default::default()
        };
    }
    assert!(
        matches!(Form::new(WindowDefinition { content, ..Default::default() }, false), Err(error) if error.contains("depth limit"))
    );
    let content = ElementDefinition {
        kind: ElementKind::StackPanel(StackPanelDefinition {
            children: vec![ElementDefinition::default(); rust_desktop_ui_xaml::MAX_ELEMENTS],
            ..Default::default()
        }),
        ..Default::default()
    };
    assert!(
        matches!(Form::new(WindowDefinition { content, ..Default::default() }, false), Err(error) if error.contains("element count"))
    );
}

#[test]
fn initial_readonly_requests_are_reflected_in_both_controls_and_demo_names_are_typed() {
    for (read_only, checked) in [(false, false), (true, false), (false, true), (true, true)] {
        let mut form = markup(&format!(
            r#"<StackPanel><TextBox x:Name="Message" IsReadOnly="{read_only}" /><CheckBox x:Name="Option" IsChecked="{checked}" /></StackPanel>"#
        ));
        assert_eq!(
            form.ui.is_read_only(form.editor()),
            Some(read_only || checked)
        );
        let option = form.find_name("Option").unwrap();
        assert_eq!(form.ui.checked(option), Some(read_only || checked));
        click(&mut form, "Option");
        assert_eq!(
            form.ui.is_read_only(form.editor()),
            Some(!(read_only || checked))
        );
    }
    for (name, element) in [
        ("Status", "Button"),
        ("Help", "TextBox"),
        ("Option", "TextBlock"),
        ("Message", "CheckBox"),
        ("Confirm", "StackPanel"),
    ] {
        let definition = rust_desktop_ui_xaml::parse(&format!(r#"<Window xmlns="urn:rust-ui-engine:ui" xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"><{element} x:Name="{name}" /></Window>"#)).unwrap();
        assert!(
            matches!(Form::new(definition, false), Err(error) if error.contains("example name") && error.contains(name))
        );
    }
}

#[test]
fn accessibility_ids_survive_hide_show_and_removed_targets_never_alias_replacements() {
    fn visible(form: &mut Form, widget: rust_desktop_ui_core::WidgetId, visible: bool) {
        let mut props = form.ui.runtime.tree().node(widget).unwrap().props.clone();
        props.visible = visible;
        form.ui
            .runtime
            .apply(Mutation::SetProps {
                node: widget,
                props,
            })
            .unwrap();
        form.update(Size::new(640.0, 400.0), 1.0).unwrap();
    }
    let mut form = form();
    let message = form.editor();
    let help = form.find_name("Help").unwrap();
    form.ui.commit_edit(message).unwrap();
    let tree = form.accessibility(1.0);
    let message_id = form.accessible.id(message).unwrap();
    let help_id = form.accessible.id(help).unwrap();
    let node = &tree
        .nodes
        .iter()
        .find(|(id, _)| *id == message_id)
        .unwrap()
        .1;
    let error_id = node.error_message().unwrap();
    assert_eq!(node.described_by(), &[help_id]);
    visible(&mut form, message, false);
    visible(&mut form, help, false);
    let tree = form.accessibility(1.0);
    assert!(
        !tree
            .nodes
            .iter()
            .any(|(id, _)| [message_id, help_id, error_id].contains(id))
    );
    assert_eq!(form.accessible.id(message), Some(message_id));
    visible(&mut form, message, true);
    let tree = form.accessibility(1.0);
    let node = &tree
        .nodes
        .iter()
        .find(|(id, _)| *id == message_id)
        .unwrap()
        .1;
    assert!(node.described_by().is_empty());
    assert_eq!(node.error_message(), Some(error_id));
    form.ui.remove(message).unwrap();
    let replacement = form
        .ui
        .edit_box(
            form.ui.root(),
            "Replacement",
            "safe",
            rust_desktop_ui_core::LayoutStyle::default(),
        )
        .unwrap();
    form.update(Size::new(640.0, 400.0), 1.0).unwrap();
    let tree = form.accessibility(1.0);
    assert_eq!(form.accessible.id(message), None);
    assert_ne!(form.accessible.id(replacement), Some(message_id));
    assert!(!tree.nodes.iter().any(|(id, _)| *id == error_id));
    form.event(PlatformEvent::Accessibility(accesskit::ActionRequest {
        action: accesskit::Action::SetValue,
        target_tree: accesskit::TreeId::ROOT,
        target_node: message_id,
        data: Some(accesskit::ActionData::Value("stale".into())),
    }))
    .unwrap();
    assert_eq!(form.ui.document(replacement).unwrap().text(), "safe");
}

#[test]
fn xaml_layout_panels_leave_the_canvas_visible_behind_rounded_editors_in_both_themes() {
    let mut form = form();
    let editor = form.editor();
    let container = form.find_name("Form").unwrap();
    for theme in [Theme::FluentLight, Theme::FluentDark] {
        form.ui.set_theme(theme).unwrap();
        for enabled in [true, false, true] {
            form.ui.set_enabled(container, enabled).unwrap();
            form.update(Size::new(640.0, 400.0), 1.0).unwrap();
            for (&widget, &role) in &form.view.roles {
                if role == rust_desktop_ui_controls::Role::Panel {
                    assert_eq!(
                        form.ui.runtime.tree().node(widget).unwrap().props.paint,
                        rust_desktop_ui_core::Paint::default()
                    );
                }
            }
            let bounds = form.ui.runtime.bounds(editor).unwrap();
            let corner = rust_desktop_ui_core::Point::new(bounds.x + 0.25, bounds.y + 0.25);
            let background = form
                .scene()
                .rectangles
                .iter()
                .rev()
                .find(|rect| rect.bounds.contains(corner.x, corner.y))
                .unwrap();
            assert_eq!(background.color, theme.tokens().canvas);
            let editor_fill = form
                .scene()
                .rounded_rectangles
                .iter()
                .find(|rect| rect.bounds == bounds && rect.stroke_width.is_none())
                .unwrap();
            assert!(editor_fill.radius >= 4.0);
            assert_eq!(
                editor_fill.color,
                if enabled {
                    theme.tokens().surface
                } else {
                    theme.tokens().canvas
                }
            );
        }
    }
}

#[test]
fn xaml_button_styles_mount_for_all_button_like_controls_and_keep_theme_overrides() {
    for tag in ["Button", "ToggleButton", "CheckBox", "RadioButton"] {
        let mut form = markup(&format!(
            r##"<{tag} x:Name="Styled" Content="Styled action" Width="240" Height="48" FontSize="20" Padding="7,3,11,4" CornerRadius="6" BorderThickness="2" Background="#112233" Foreground="#80445566" BorderBrush="#778899" FocusBrush="#AABBCC"/>"##
        ));
        let widget = form.find_name("Styled").unwrap();
        let style = *form.ui.button_style(widget).unwrap();
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
        assert_eq!(
            style.normal.foreground,
            Some(Color {
                r: 68.0 / 255.0,
                g: 85.0 / 255.0,
                b: 102.0 / 255.0,
                a: 128.0 / 255.0
            })
        );
        assert_eq!(style.normal.border, Some(Color::rgb(119, 136, 153)));
        assert_eq!(style.normal.accent, Some(Color::rgb(170, 187, 204)));
        for theme in [Theme::FluentLight, Theme::FluentDark] {
            form.ui.set_theme(theme).unwrap();
            form.update(Size::new(640.0, 400.0), 1.0).unwrap();
            assert_eq!(form.ui.button_style(widget), Some(&style));
            let run = form
                .scene()
                .texts
                .iter()
                .find(|run| run.text == "Styled action")
                .unwrap();
            assert_eq!(run.font_size, 20.0);
            assert_eq!(Some(run.color), style.normal.foreground);
        }
    }
}

#[test]
fn horizontal_auto_button_widths_measure_declared_font_and_padding() {
    let mut form = markup(
        r#"<StackPanel Orientation="Horizontal" Spacing="8">
        <Button x:Name="AutoButton" Content="Wide caption" FontSize="24" Padding="20,6,30,6" Height="60"/>
        <ToggleButton x:Name="AutoToggle" Content="Pin caption" FontSize="20" Padding="9,5,17,5" Height="60"/>
        <CheckBox x:Name="AutoCheck" Content="Check caption" FontSize="18" Padding="4,5,10,5" Height="60"/>
    </StackPanel>"#,
    );
    for (name, text, font, padding, extra) in [
        ("AutoButton", "Wide caption", 24.0, 50.0, 0.0),
        ("AutoToggle", "Pin caption", 20.0, 26.0, 0.0),
        ("AutoCheck", "Check caption", 18.0, 14.0, 26.0),
    ] {
        let text_width = form
            .ui
            .measure_text_with_font_size(text, 16384.0, false, font)
            .unwrap()
            .width;
        let bounds = form
            .ui
            .runtime
            .bounds(form.find_name(name).unwrap())
            .unwrap();
        assert!(
            (bounds.width - (text_width + padding + extra)).abs() < 0.01,
            "{name}"
        );
        let run = form
            .scene()
            .texts
            .iter()
            .find(|run| run.text == text)
            .unwrap();
        assert_eq!(run.font_size, font);
        assert!(
            (run.bounds.width - text_width).abs() < 0.01,
            "{name} content width"
        );
    }
}

#[test]
fn toggle_button_keyboard_pointer_and_accessibility_share_checked_state() {
    let mut form = markup(
        r#"<StackPanel x:Name="Container"><ToggleButton x:Name="Toggle" Content="Pin" AutomationName="Keep pinned" IsChecked="True"/><Button Content="Disabled" IsEnabled="False"/></StackPanel>"#,
    );
    let toggle = form.find_name("Toggle").unwrap();
    assert_eq!(form.ui.checked(toggle), Some(true));
    tab(&mut form, false);
    assert_eq!(form.ui.runtime.focused(), Some(toggle));
    key(&mut form, Key::Space, false);
    assert_eq!(form.ui.checked(toggle), Some(true));
    form.event(PlatformEvent::Input(InputEvent::KeyUp {
        key: Key::Space,
        modifiers: Modifiers::default(),
    }))
    .unwrap();
    assert_eq!(form.ui.checked(toggle), Some(false));
    key(&mut form, Key::Enter, false);
    assert_eq!(form.ui.checked(toggle), Some(true));
    let tree = form.accessibility(1.0);
    let node = &tree
        .nodes
        .iter()
        .find(|(id, _)| Some(*id) == form.accessible.id(toggle))
        .unwrap()
        .1;
    assert_eq!(node.role(), accesskit::Role::Button);
    assert_eq!(node.label(), Some("Keep pinned"));
    assert_eq!(node.toggled(), Some(accesskit::Toggled::True));
    assert!(node.supports_action(accesskit::Action::Click));
    accessible_action(&mut form, "Toggle", accesskit::Action::Click, None);
    assert_eq!(form.ui.checked(toggle), Some(false));
    click(&mut form, "Toggle");
    assert_eq!(form.ui.checked(toggle), Some(true));
    let container = form.find_name("Container").unwrap();
    form.ui.set_enabled(container, false).unwrap();
    form.update(Size::new(640.0, 400.0), 1.0).unwrap();
    accessible_action(&mut form, "Toggle", accesskit::Action::Click, None);
    assert_eq!(form.ui.checked(toggle), Some(true));
    let tree = form.accessibility(1.0);
    let node = &tree
        .nodes
        .iter()
        .find(|(id, _)| Some(*id) == form.accessible.id(toggle))
        .unwrap()
        .1;
    assert!(node.is_disabled());
    assert!(!node.supports_action(accesskit::Action::Click));
}

#[test]
fn toggle_command_registration_owns_initial_state_and_rejects_missing_checked_contract() {
    use crate::view::MountedView;
    use rust_desktop_ui_controls::Controls;
    let source = r#"<Window xmlns="urn:rust-ui-engine:ui" xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"><StackPanel><ToggleButton x:Name="One" Content="One" Command="Pin" IsChecked="True"/><ToggleButton x:Name="Two" Content="Two" Command="Pin" IsChecked="False"/></StackPanel></Window>"#;
    let definition = rust_desktop_ui_xaml::parse(source).unwrap();
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let root = ui.root();
    let before = ui.runtime.tree().len();
    assert!(
        matches!(MountedView::mount(&mut ui,root,&definition.content),Err(error) if error.contains("Unregistered XAML command"))
    );
    assert_eq!(ui.runtime.tree().len(), before);
    ui.commands_mut().register("Pin", "Pin");
    assert!(
        matches!(MountedView::mount(&mut ui,root,&definition.content),Err(error) if error.contains("registered checked state"))
    );
    assert_eq!(ui.runtime.tree().len(), before);
    ui.commands_mut().register_toggle("Pin", "Pin", false);
    let view = MountedView::mount(&mut ui, root, &definition.content).unwrap();
    let one = view.find_name("One").unwrap();
    let two = view.find_name("Two").unwrap();
    ui.update(Size::new(640.0, 400.0)).unwrap();
    assert_eq!(ui.checked(one), Some(false));
    assert_eq!(ui.checked(two), Some(false));
    ui.activate(one).unwrap();
    ui.update(Size::new(640.0, 400.0)).unwrap();
    assert_eq!(ui.checked(one), Some(true));
    assert_eq!(ui.checked(two), Some(true));
    assert_eq!(ui.commands().checked(&"Pin".into()), Some(true));
    let invocation = ui.commands_mut().drain_invocations();
    assert_eq!(invocation.len(), 1);
    assert_eq!(invocation[0].checked, Some(true));
    ui.commands_mut().set_can_execute(&"Pin".into(), || false);
    ui.activate(two).unwrap();
    ui.update(Size::new(640.0, 400.0)).unwrap();
    assert_eq!(ui.checked(two), Some(true));
    assert!(ui.commands_mut().drain_invocations().is_empty());
}

#[test]
fn bound_buttons_share_live_command_state_through_native_accessibility_and_keyboard() {
    let mut form = markup(
        r#"<StackPanel><Button x:Name="One" Content="One" Command="Pin"/><ToggleButton x:Name="Two" Content="Two" Command="Pin" IsChecked="True"/></StackPanel>"#,
    );
    let ids = [
        form.find_name("One").unwrap(),
        form.find_name("Two").unwrap(),
    ];
    let assert_nodes = |form: &mut Form, checked: Option<accesskit::Toggled>, disabled: bool| {
        let tree = form.accessibility(1.0);
        for widget in ids {
            let id = form.accessible.id(widget).unwrap();
            let node = &tree
                .nodes
                .iter()
                .find(|(candidate, _)| *candidate == id)
                .unwrap()
                .1;
            assert_eq!(node.role(), accesskit::Role::Button);
            assert_eq!(node.toggled(), checked);
            assert_eq!(node.is_disabled(), disabled);
            assert_eq!(node.supports_action(accesskit::Action::Click), !disabled);
        }
    };
    assert_nodes(&mut form, Some(accesskit::Toggled::False), false);
    accessible_action(&mut form, "One", accesskit::Action::Click, None);
    assert_nodes(&mut form, Some(accesskit::Toggled::True), false);
    assert_eq!(form.ui.commands().checked(&"Pin".into()), Some(true));
    form.ui
        .runtime
        .apply(Mutation::Focus(Some(ids[1])))
        .unwrap();
    key(&mut form, Key::Space, false);
    assert_nodes(&mut form, Some(accesskit::Toggled::True), false);
    form.event(PlatformEvent::Input(InputEvent::KeyUp {
        key: Key::Space,
        modifiers: Modifiers::default(),
    }))
    .unwrap();
    assert_nodes(&mut form, Some(accesskit::Toggled::False), false);
    form.ui.commands_mut().set_checked(&"Pin".into(), None);
    assert_nodes(&mut form, None, false);
    accessible_action(&mut form, "Two", accesskit::Action::Click, None);
    assert_nodes(&mut form, None, false);
    form.ui
        .commands_mut()
        .set_checked(&"Pin".into(), Some(true));
    form.ui.commands_mut().set_enabled(&"Pin".into(), false);
    accessible_action(&mut form, "One", accesskit::Action::Click, None);
    assert_nodes(&mut form, Some(accesskit::Toggled::True), true);
    assert_eq!(form.ui.commands().checked(&"Pin".into()), Some(true));
}

#[test]
fn popup_preflight_checks_all_commands_and_manual_ast_before_tree_mutation() {
    use crate::view::MountedView;
    use rust_desktop_ui_controls::Controls;
    use rust_desktop_ui_xaml::{ElementKind, MenuEntryDefinition};
    let source = r#"<Window xmlns="urn:rust-ui-engine:ui"><DropDownButton><MenuItem Header="Sub"><MenuItem Header="Leaf" Command="Unknown"/></MenuItem></DropDownButton></Window>"#;
    let mut definition = rust_desktop_ui_xaml::parse(source).unwrap();
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let root = ui.root();
    let count = ui.runtime.tree().len();
    assert!(
        matches!(MountedView::mount(&mut ui, root, &definition.content),Err(error) if error.contains("Unregistered"))
    );
    assert_eq!(ui.runtime.tree().len(), count);
    ui.commands_mut().register("Unknown", "Known");
    definition.content.common.tool_tip = Some("x".repeat(4097));
    assert!(MountedView::mount(&mut ui, root, &definition.content).is_err());
    assert_eq!(ui.runtime.tree().len(), count);
    definition.content.common.tool_tip = None;
    definition.content.common.width = Some(f32::NAN);
    assert!(MountedView::mount(&mut ui, root, &definition.content).is_err());
    assert_eq!(ui.runtime.tree().len(), count);
    definition.content.common.width = None;
    let ElementKind::DropDownButton(menu) = &mut definition.content.kind else {
        unreachable!()
    };
    let MenuEntryDefinition::Item(item) = &mut menu.items[0] else {
        unreachable!()
    };
    item.command = Some("Unknown".into());
    assert!(MountedView::mount(&mut ui, root, &definition.content).is_err());
    assert_eq!(ui.runtime.tree().len(), count);
}

#[test]
fn popup_sample_parts_geometry_accessibility_and_primary_command_are_independent() {
    let mut form = form();
    let split = form.find_name("AcceptSplit").unwrap();
    let parts = &form.view.splits[&split];
    let (primary, dropdown) = (parts.primary, parts.dropdown);
    for (name, x, width) in [("ActionMenu", 32.0, 160.0), ("AcceptSplit", 200.0, 240.0)] {
        let bounds = form
            .ui
            .runtime
            .bounds(form.find_name(name).unwrap())
            .unwrap();
        assert_eq!(
            (bounds.x, bounds.y, bounds.width, bounds.height),
            (x, 433.0, width, 36.0)
        );
    }
    form.ui.set_text(form.editor(), "accepted").unwrap();
    form.ui.activate(primary).unwrap();
    form.update(Size::new(640.0, 512.0), 1.0).unwrap();
    assert_eq!(form.ui.accepted_value(form.editor()), Some("accepted"));
    assert!(!form.ui.popup_open());
    form.ui.commands_mut().set_enabled(&"Confirm".into(), false);
    assert!(!form.ui.is_enabled(primary));
    assert!(form.ui.is_enabled(dropdown));
    form.ui
        .perform_action(
            dropdown,
            rust_desktop_ui_controls::SemanticAction::Expand,
            None,
        )
        .unwrap();
    form.update(Size::new(640.0, 512.0), 1.0).unwrap();
    assert!(form.ui.popup_open());
    let tree = form.accessibility(1.0);
    let arrow = &tree
        .nodes
        .iter()
        .find(|(id, _)| Some(*id) == form.accessible.id(dropdown))
        .unwrap()
        .1;
    assert_eq!(arrow.has_popup(), Some(accesskit::HasPopup::Menu));
    assert_eq!(arrow.is_expanded(), Some(true));
    assert_eq!(arrow.tooltip(), Some("Подтвердить или выбрать действие"));
    assert!(arrow.supports_action(accesskit::Action::Collapse));
    let disabled = &tree
        .nodes
        .iter()
        .find(|(_, node)| {
            node.role() == accesskit::Role::MenuItem && node.label() == Some("Недоступно")
        })
        .unwrap()
        .1;
    assert!(disabled.is_disabled());
    assert!(!disabled.supports_action(accesskit::Action::Click));
    form.ui
        .perform_action(
            dropdown,
            rust_desktop_ui_controls::SemanticAction::Collapse,
            None,
        )
        .unwrap();
    assert!(!form.ui.popup_open());
}

#[test]
fn popup_nested_accessibility_actions_activate_shared_pin_and_preserve_connected_tree() {
    let mut form = form();
    accessible_action(&mut form, "ActionMenu", accesskit::Action::Expand, None);
    form.update(Size::new(640.0, 512.0), 1.0).unwrap();
    let tree = form.accessibility(1.0);
    let target = tree
        .nodes
        .iter()
        .find(|(_, node)| node.role() == accesskit::Role::MenuItem && node.label() == Some("Режим"))
        .unwrap()
        .0;
    form.event(PlatformEvent::Accessibility(accesskit::ActionRequest {
        action: accesskit::Action::Expand,
        target_tree: accesskit::TreeId::ROOT,
        target_node: target,
        data: None,
    }))
    .unwrap();
    form.update(Size::new(640.0, 512.0), 1.0).unwrap();
    let tree = form.accessibility(1.0);
    let menus = tree
        .nodes
        .iter()
        .filter(|(_, node)| node.role() == accesskit::Role::Menu)
        .count();
    assert_eq!(menus, 2);
    let nodes: std::collections::HashMap<_, _> =
        tree.nodes.iter().map(|(id, node)| (*id, node)).collect();
    let mut pending = vec![accesskit::NodeId(1)];
    let mut visited = std::collections::HashSet::new();
    while let Some(id) = pending.pop() {
        assert!(visited.insert(id), "cycle or duplicate child");
        pending.extend_from_slice(nodes.get(&id).expect("dangling child").children());
    }
    assert_eq!(
        visited.len(),
        nodes.len(),
        "orphan popup accessibility node"
    );
    let target = tree
        .nodes
        .iter()
        .find(|(_, node)| {
            node.role() == accesskit::Role::MenuItemCheckBox && node.label() == Some("Закрепить")
        })
        .unwrap()
        .0;
    form.event(PlatformEvent::Accessibility(accesskit::ActionRequest {
        action: accesskit::Action::Click,
        target_tree: accesskit::TreeId::ROOT,
        target_node: target,
        data: None,
    }))
    .unwrap();
    assert!(!form.ui.popup_open());
    assert_eq!(form.ui.commands().checked(&"Pin".into()), Some(true));
    assert_eq!(form.ui.checked(form.find_name("Pin").unwrap()), Some(true));
    assert_eq!(form.ui.runtime.focused(), form.find_name("ActionMenu"));
}

#[test]
fn popup_tooltip_markup_requests_a_wakeup_and_draws_without_taking_focus() {
    let mut form = markup(
        r#"<DropDownButton x:Name="Menu" Content="Menu" ToolTip="Choose a command"><MenuItem Header="Pin" Command="Pin"/></DropDownButton>"#,
    );
    assert!(form.wake_after().is_none());
    let id = form.find_name("Menu").unwrap();
    let bounds = form.ui.runtime.bounds(id).unwrap();
    let focus = form.ui.runtime.focused();
    form.event(PlatformEvent::Input(InputEvent::PointerMoved {
        position: rust_desktop_ui_core::Point::new(bounds.x + 10.0, bounds.y + 10.0),
    }))
    .unwrap();
    let deadline = form.ui.tooltip_deadline().expect("hover schedules tooltip");
    assert!(form.wake_after().is_some());
    assert!(form.ui.tooltip_tick(deadline).unwrap());
    form.update(Size::new(640.0, 400.0), 1.0).unwrap();
    assert!(form.ui.tooltip_bounds().is_some());
    assert!(
        form.scene()
            .texts
            .iter()
            .any(|run| run.text == "Choose a command")
    );
    assert_eq!(form.ui.runtime.focused(), focus);
    let tree = form.accessibility(1.0);
    let owner = &tree
        .nodes
        .iter()
        .find(|(candidate, _)| Some(*candidate) == form.accessible.id(id))
        .unwrap()
        .1;
    assert_eq!(owner.tooltip(), Some("Choose a command"));
    form.event(PlatformEvent::Input(InputEvent::WindowFocused(false)))
        .unwrap();
    assert!(form.ui.tooltip_bounds().is_none());
    assert!(form.wake_after().is_none());
}

#[test]
fn sample_button_row_keeps_geometry_and_tab_order_skips_the_disabled_button() {
    let mut form = form();
    let pin = form.find_name("Pin").unwrap();
    let disabled = form.find_name("Disabled").unwrap();
    for (id, x, width) in [(pin, 200.0, 120.0), (disabled, 328.0, 112.0)] {
        let bounds = form.ui.runtime.bounds(id).unwrap();
        assert_eq!(
            (bounds.x, bounds.y, bounds.width, bounds.height),
            (x, 188.0, width, 36.0)
        );
    }
    for name in [
        "Message",
        "Option",
        "Confirm",
        "Pin",
        "AlignLeft",
        "ActionMenu",
    ] {
        tab(&mut form, false);
        assert_eq!(form.ui.runtime.focused(), form.find_name(name));
    }
    let split = form.find_name("AcceptSplit").unwrap();
    let parts = &form.view.splits[&split];
    let (primary, dropdown) = (parts.primary, parts.dropdown);
    for target in [primary, dropdown, form.editor()] {
        tab(&mut form, false);
        assert_eq!(form.ui.runtime.focused(), Some(target));
    }
    tab(&mut form, true);
    assert_eq!(form.ui.runtime.focused(), Some(dropdown));
    tab(&mut form, true);
    assert_eq!(form.ui.runtime.focused(), Some(primary));
    tab(&mut form, true);
    assert_eq!(form.ui.runtime.focused(), form.find_name("ActionMenu"));
    tab(&mut form, true);
    assert_eq!(form.ui.runtime.focused(), form.find_name("AlignLeft"));
    tab(&mut form, true);
    assert_eq!(form.ui.runtime.focused(), Some(pin));
    form.update(Size::new(480.0, 360.0), 1.0).unwrap();
    let bounds = form.ui.runtime.bounds(disabled).unwrap();
    assert!(bounds.x + bounds.width <= 448.0);
    click(&mut form, "Disabled");
    assert!(form.ui.drain_activated().is_empty());
    assert!(form.ui.commands_mut().drain_invocations().is_empty());
    form.update(Size::new(320.0, 360.0), 1.0).unwrap();
    assert_eq!(form.ui.runtime.bounds(form.ui.root()).unwrap().width, 320.0);
    assert_eq!(form.ui.runtime.bounds(form.editor()).unwrap().width, 256.0);
    let pin_bounds = form.ui.runtime.bounds(pin).unwrap();
    assert_eq!(pin_bounds.width, 120.0);
    let pin_fill = form
        .scene()
        .rounded_rectangles
        .iter()
        .find(|rect| rect.bounds == pin_bounds && rect.stroke_width.is_none())
        .unwrap();
    assert!(pin_fill.clip.x + pin_fill.clip.width <= 288.0);
}

#[test]
fn radio_groups_are_scoped_to_each_mount_and_unnamed_groups_to_each_parent() {
    use crate::view::MountedView;
    use rust_desktop_ui_controls::Controls;
    let source = r#"<Window xmlns="urn:rust-ui-engine:ui" xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"><StackPanel>
        <RadioButton x:Name="NamedA" GroupName="Shared" IsChecked="True"/>
        <RadioButton x:Name="DefaultA" IsChecked="True"/>
        <RadioButton x:Name="DefaultB"/>
        <StackPanel>
            <RadioButton x:Name="NamedB" GroupName="Shared"/>
            <RadioButton x:Name="NestedDefault" IsChecked="True"/>
        </StackPanel>
    </StackPanel></Window>"#;
    let definition = rust_desktop_ui_xaml::parse(source).unwrap();
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let root = ui.root();
    let first = MountedView::mount(&mut ui, root, &definition.content).unwrap();
    let second = MountedView::mount(&mut ui, root, &definition.content).unwrap();
    let first_named = first.find_name("NamedA").unwrap();
    let second_named = second.find_name("NamedA").unwrap();
    ui.activate(first.find_name("NamedB").unwrap()).unwrap();
    assert_eq!(ui.checked(first_named), Some(false));
    assert_eq!(ui.checked(first.find_name("NamedB").unwrap()), Some(true));
    assert_eq!(ui.checked(second_named), Some(true));
    assert_eq!(ui.checked(second.find_name("NamedB").unwrap()), Some(false));
    ui.activate(first.find_name("DefaultB").unwrap()).unwrap();
    assert_eq!(
        ui.checked(first.find_name("DefaultA").unwrap()),
        Some(false)
    );
    assert_eq!(ui.checked(first.find_name("DefaultB").unwrap()), Some(true));
    assert_eq!(
        ui.checked(first.find_name("NestedDefault").unwrap()),
        Some(true)
    );
    assert_eq!(ui.checked(first.find_name("NamedB").unwrap()), Some(true));
    assert_eq!(
        ui.checked(second.find_name("DefaultA").unwrap()),
        Some(true)
    );
    // Even root radios mounted into the very same parent have distinct default groups.
    let root_radio = rust_desktop_ui_xaml::parse(
        r#"<Window xmlns="urn:rust-ui-engine:ui"><RadioButton IsChecked="True"/></Window>"#,
    )
    .unwrap();
    let third = MountedView::mount(&mut ui, root, &root_radio.content).unwrap();
    let fourth = MountedView::mount(&mut ui, root, &root_radio.content).unwrap();
    assert_eq!(ui.checked(third.radios[0]), Some(true));
    assert_eq!(ui.checked(fourth.radios[0]), Some(true));
}

#[test]
fn radio_manual_ast_rejects_ambiguous_or_invalid_groups_before_any_tree_mutation() {
    use crate::view::MountedView;
    use rust_desktop_ui_controls::Controls;
    use rust_desktop_ui_xaml::{
        ElementDefinition, ElementKind, RadioButtonDefinition, StackPanelDefinition,
    };
    let radio = |group: Option<&str>| ElementDefinition {
        kind: ElementKind::RadioButton(RadioButtonDefinition {
            group_name: group.map(str::to_owned),
            is_checked: true,
            ..Default::default()
        }),
        ..Default::default()
    };
    let panel = |children| ElementDefinition {
        kind: ElementKind::StackPanel(StackPanelDefinition {
            children,
            ..Default::default()
        }),
        ..Default::default()
    };
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let root = ui.root();
    for invalid in [
        panel(vec![radio(None), radio(None)]),
        panel(vec![radio(Some("G")), panel(vec![radio(Some("G"))])]),
        radio(Some("")),
        radio(Some("invalid name")),
    ] {
        let before = ui.runtime.tree().len();
        let error = MountedView::mount(&mut ui, root, &invalid)
            .err()
            .expect("invalid group must fail");
        assert!(
            error.contains("at most one initial") || error.contains("GroupName"),
            "{error}"
        );
        assert_eq!(ui.runtime.tree().len(), before);
    }
    assert!(
        MountedView::mount(
            &mut ui,
            root,
            &panel(vec![radio(None), panel(vec![radio(None)])])
        )
        .is_ok()
    );
}

#[test]
fn radio_keyboard_navigation_and_accessibility_keep_one_selected_peer() {
    let mut form = markup(
        r#"<StackPanel>
        <Button x:Name="Before" Content="Before"/>
        <StackPanel x:Name="Group">
            <RadioButton x:Name="A" Content="First" GroupName="G" AutomationName="First choice"/>
            <RadioButton x:Name="B" Content="Second" GroupName="G" IsChecked="True"/>
            <RadioButton x:Name="C" Content="Unavailable" GroupName="G" IsEnabled="False"/>
        </StackPanel>
        <Button x:Name="After" Content="After"/>
    </StackPanel>"#,
    );
    for name in ["Before", "B", "After"] {
        tab(&mut form, false);
        assert_eq!(form.ui.runtime.focused(), form.find_name(name));
    }
    tab(&mut form, true);
    assert_eq!(form.ui.runtime.focused(), form.find_name("B"));
    for (arrow, selected) in [
        (Key::ArrowRight, "A"),
        (Key::ArrowDown, "B"),
        (Key::ArrowLeft, "A"),
        (Key::ArrowUp, "B"),
    ] {
        key(&mut form, arrow, false);
        assert_eq!(form.ui.runtime.focused(), form.find_name(selected));
        for name in ["A", "B", "C"] {
            assert_eq!(
                form.ui.checked(form.find_name(name).unwrap()),
                Some(name == selected)
            );
        }
    }
    key(&mut form, Key::Enter, false);
    key(&mut form, Key::Space, false);
    form.event(PlatformEvent::Input(InputEvent::KeyUp {
        key: Key::Space,
        modifiers: Modifiers::default(),
    }))
    .unwrap();
    assert_eq!(form.ui.checked(form.find_name("B").unwrap()), Some(true));
    accessible_action(&mut form, "A", accesskit::Action::Click, None);
    click(&mut form, "A"); // Reselecting does not clear a radio button.
    accessible_action(&mut form, "C", accesskit::Action::Click, None);
    let tree = form.accessibility(1.0);
    for name in ["A", "B", "C"] {
        let widget = form.find_name(name).unwrap();
        let node = &tree
            .nodes
            .iter()
            .find(|(id, _)| Some(*id) == form.accessible.id(widget))
            .unwrap()
            .1;
        assert_eq!(node.role(), accesskit::Role::RadioButton);
        assert_eq!(
            node.toggled(),
            Some(if name == "A" {
                accesskit::Toggled::True
            } else {
                accesskit::Toggled::False
            })
        );
        assert_eq!(node.supports_action(accesskit::Action::Click), name != "C");
        assert_eq!(node.is_disabled(), name == "C");
        if name == "A" {
            assert_eq!(node.label(), Some("First choice"));
        }
    }
    let group = form.find_name("Group").unwrap();
    form.ui.set_enabled(group, false).unwrap();
    form.update(Size::new(640.0, 400.0), 1.0).unwrap();
    accessible_action(&mut form, "B", accesskit::Action::Click, None);
    assert_eq!(form.ui.checked(form.find_name("A").unwrap()), Some(true));
}

#[test]
fn radio_sample_geometry_and_auto_width_follow_shared_font_and_indicator_metrics() {
    let form = form();
    assert_eq!(generated::window_definition().height, 512.0);
    for (name, x) in [
        ("AlignLeft", 32.0),
        ("AlignCenter", 160.0),
        ("AlignRight", 288.0),
    ] {
        let bounds = form
            .ui
            .runtime
            .bounds(form.find_name(name).unwrap())
            .unwrap();
        assert_eq!(
            (bounds.x, bounds.y, bounds.width, bounds.height),
            (x, 232.0, 120.0, 32.0)
        );
    }
    assert_eq!(
        form.ui
            .runtime
            .bounds(form.find_name("Help").unwrap())
            .unwrap()
            .y,
        321.0
    );
    assert_eq!(
        form.ui
            .runtime
            .bounds(form.find_name("Status").unwrap())
            .unwrap()
            .y,
        369.0
    );
    let mut auto = markup(
        r#"<StackPanel Orientation="Horizontal"><RadioButton x:Name="Auto" Content="Wide choice" FontSize="20" Padding="9,5,17,5" Height="50"/></StackPanel>"#,
    );
    let radio = auto.find_name("Auto").unwrap();
    let width = auto
        .ui
        .measure_text_with_font_size("Wide choice", 16384.0, false, 20.0)
        .unwrap()
        .width;
    assert!((auto.ui.runtime.bounds(radio).unwrap().width - width - 26.0 - 26.0).abs() < 0.01);
    let text = auto
        .scene()
        .texts
        .iter()
        .find(|text| text.text == "Wide choice")
        .unwrap();
    assert_eq!(text.font_size, 20.0);
    assert!((text.bounds.width - width).abs() < 0.01);
}

#[test]
fn icon_and_separator_sample_geometry_preserves_existing_controls() {
    use rust_desktop_ui_controls::IconSymbol;
    let form = form();
    for (name, x, symbol) in [
        ("CheckIcon", 32.0, IconSymbol::Check),
        ("CloseIcon", 64.0, IconSymbol::Close),
        ("AddIcon", 96.0, IconSymbol::Add),
        ("RemoveIcon", 128.0, IconSymbol::Remove),
        ("ChevronDownIcon", 160.0, IconSymbol::ChevronDown),
        ("ChevronRightIcon", 192.0, IconSymbol::ChevronRight),
        ("SaveIcon", 224.0, IconSymbol::Save),
        ("DisabledIcon", 265.0, IconSymbol::Pin),
    ] {
        let id = form.find_name(name).unwrap();
        let bounds = form.ui.runtime.bounds(id).unwrap();
        assert_eq!(
            (bounds.x, bounds.y, bounds.width, bounds.height),
            (x, 276.0, 24.0, 24.0),
            "{name}"
        );
        assert_eq!(form.ui.icon_symbol(id), Some(symbol));
        assert!(form.ui.runtime.tree().children(id).unwrap().is_empty());
    }
    for (name, expected) in [
        ("VRule", (256.0, 272.0, 1.0, 32.0)),
        ("HRule", (32.0, 312.0, 576.0, 1.0)),
    ] {
        let bounds = form
            .ui
            .runtime
            .bounds(form.find_name(name).unwrap())
            .unwrap();
        assert_eq!(
            (bounds.x, bounds.y, bounds.width, bounds.height),
            expected,
            "{name}"
        );
    }
    for (name, symbol) in [("Confirm", IconSymbol::Check), ("Pin", IconSymbol::Pin)] {
        let id = form.find_name(name).unwrap();
        let icon = form.ui.button_icon(id).unwrap();
        assert_eq!((icon.symbol, icon.size, icon.spacing), (symbol, 16.0, 8.0));
        assert_eq!(form.ui.runtime.bounds(id).unwrap().y, 188.0);
        assert!(form.ui.runtime.tree().children(id).unwrap().is_empty());
    }
}

#[test]
fn icon_separator_defaults_follow_container_axis_and_explicit_bounds() {
    let form = markup(
        r#"<StackPanel>
        <Icon x:Name="Default" Symbol="Check"/>
        <Icon x:Name="Wide" Symbol="Save" Width="48" Height="20"/>
        <Separator x:Name="Horizontal" Thickness="2"/>
        <Separator x:Name="VerticalFallback" Orientation="Vertical" Thickness="3"/>
        <StackPanel Orientation="Horizontal" Height="40">
            <Separator x:Name="VerticalStretch" Orientation="Vertical" Thickness="2"/>
            <Separator x:Name="HorizontalFallback"/>
            <Separator x:Name="Explicit" Orientation="Vertical" Width="10" Height="25"/>
        </StackPanel>
    </StackPanel>"#,
    );
    for (name, expected) in [
        ("Default", (16.0, 16.0)),
        ("Wide", (48.0, 20.0)),
        ("Horizontal", (576.0, 2.0)),
        ("VerticalFallback", (3.0, 16.0)),
        ("VerticalStretch", (2.0, 40.0)),
        ("HorizontalFallback", (16.0, 1.0)),
        ("Explicit", (10.0, 25.0)),
    ] {
        let bounds = form
            .ui
            .runtime
            .bounds(form.find_name(name).unwrap())
            .unwrap();
        assert_eq!((bounds.width, bounds.height), expected, "{name}");
    }
}

#[test]
fn separator_thickness_updates_omitted_short_axis_and_keeps_explicit_dimensions() {
    let mut form = markup(
        r#"<StackPanel>
        <Separator x:Name="AutoHorizontal"/>
        <Separator x:Name="FixedHorizontal" Height="3"/>
        <StackPanel Orientation="Horizontal" Height="40">
            <Separator x:Name="AutoVertical" Orientation="Vertical"/>
            <Separator x:Name="FixedVertical" Orientation="Vertical" Width="3"/>
        </StackPanel>
    </StackPanel>"#,
    );
    for name in [
        "AutoHorizontal",
        "FixedHorizontal",
        "AutoVertical",
        "FixedVertical",
    ] {
        let id = form.find_name(name).unwrap();
        let mut settings = *form.ui.separator_settings(id).unwrap();
        settings.thickness = 4.0;
        form.ui.set_separator_settings(id, settings).unwrap();
    }
    form.update(Size::new(640.0, 400.0), 1.0).unwrap();
    for (name, expected) in [
        ("AutoHorizontal", (576.0, 4.0)),
        ("FixedHorizontal", (576.0, 3.0)),
        ("AutoVertical", (4.0, 40.0)),
        ("FixedVertical", (3.0, 40.0)),
    ] {
        let bounds = form
            .ui
            .runtime
            .bounds(form.find_name(name).unwrap())
            .unwrap();
        assert_eq!((bounds.width, bounds.height), expected, "{name}");
    }
}

#[test]
fn icon_button_auto_width_includes_only_real_caption_gap_and_accessible_actions() {
    let mut form = markup(
        r#"<StackPanel Orientation="Horizontal">
        <Button x:Name="Caption" Content="Save document" Icon="Save" IconSize="24" IconSpacing="7" FontSize="20" Padding="9,5,17,5" Height="48"/>
        <Icon x:Name="Decoration" Symbol="Add" AutomationName="Available"/>
        <Separator x:Name="Rule" Orientation="Vertical"/>
        <Button x:Name="Only" Icon="Close" IconSpacing="100" AutomationName="Close document"/>
        <ToggleButton x:Name="Toggle" Icon="Pin" AutomationName="Keep pinned"/>
    </StackPanel>"#,
    );
    let caption = form.find_name("Caption").unwrap();
    let measured = form
        .ui
        .measure_text_with_font_size("Save document", 16384.0, false, 20.0)
        .unwrap()
        .width;
    assert!(
        (form.ui.runtime.bounds(caption).unwrap().width - (measured + 26.0 + 24.0 + 7.0)).abs()
            < 0.01
    );
    for name in ["Only", "Toggle"] {
        let id = form.find_name(name).unwrap();
        let bounds = form.ui.runtime.bounds(id).unwrap();
        assert_eq!((bounds.width, bounds.height), (36.0, 36.0));
        assert!(form.ui.runtime.tree().children(id).unwrap().is_empty());
    }
    for name in ["Caption", "Only", "Toggle", "Caption"] {
        tab(&mut form, false);
        assert_eq!(form.ui.runtime.focused(), form.find_name(name));
    }
    accessible_action(&mut form, "Toggle", accesskit::Action::Click, None);
    assert_eq!(
        form.ui.checked(form.find_name("Toggle").unwrap()),
        Some(true)
    );
    let tree = form.accessibility(1.0);
    for (name, label) in [("Only", "Close document"), ("Toggle", "Keep pinned")] {
        let widget = form.find_name(name).unwrap();
        let node = &tree
            .nodes
            .iter()
            .find(|(id, _)| Some(*id) == form.accessible.id(widget))
            .unwrap()
            .1;
        assert_eq!(node.role(), accesskit::Role::Button);
        assert_eq!(node.label(), Some(label));
        assert!(node.supports_action(accesskit::Action::Click));
        assert!(node.children().is_empty());
    }
}

#[test]
fn icon_and_separator_semantics_are_noninteractive_and_names_do_not_draw_text() {
    let mut form = markup(
        r##"<StackPanel>
        <Icon x:Name="Image" Symbol="Save" AutomationName="Saved document" Foreground="#80112233"/>
        <Icon x:Name="DisabledImage" Symbol="Pin" AutomationName="Pinned" IsEnabled="False"/>
        <Separator x:Name="Rule" Foreground="#405060"/>
        <Button x:Name="Action" Content="Action"/>
    </StackPanel>"##,
    );
    let image = form.find_name("Image").unwrap();
    let foreground = Color {
        r: 17.0 / 255.0,
        g: 34.0 / 255.0,
        b: 51.0 / 255.0,
        a: 128.0 / 255.0,
    };
    assert_eq!(
        form.ui.icon_style(image).unwrap().foreground,
        Some(foreground)
    );
    assert_eq!(
        form.ui
            .separator_settings(form.find_name("Rule").unwrap())
            .unwrap()
            .foreground,
        Some(Color::rgb(64, 80, 96))
    );
    for theme in [Theme::FluentDark, Theme::FluentLight] {
        form.ui.set_theme(theme).unwrap();
        form.update(Size::new(640.0, 400.0), 1.0).unwrap();
        assert_eq!(
            form.ui.icon_style(image).unwrap().foreground,
            Some(foreground)
        );
        let tree = form.accessibility(1.0);
        for (name, role) in [
            ("Image", accesskit::Role::Image),
            ("DisabledImage", accesskit::Role::Image),
            ("Rule", accesskit::Role::Splitter),
        ] {
            let widget = form.find_name(name).unwrap();
            let node = &tree
                .nodes
                .iter()
                .find(|(id, _)| Some(*id) == form.accessible.id(widget))
                .unwrap()
                .1;
            assert_eq!(node.role(), role);
            assert!(!node.supports_action(accesskit::Action::Click));
            assert!(!node.supports_action(accesskit::Action::Focus));
            assert!(!node.supports_action(accesskit::Action::SetValue));
            assert_eq!(node.is_disabled(), name == "DisabledImage");
            if name == "Image" {
                assert_eq!(node.label(), Some("Saved document"));
            }
            let bounds = form.ui.runtime.bounds(widget).unwrap();
            assert_ne!(
                form.ui.runtime.hit_test(rust_desktop_ui_core::Point::new(
                    bounds.x + bounds.width / 2.0,
                    bounds.y + bounds.height / 2.0
                )),
                Some(widget)
            );
        }
        assert!(
            !form
                .scene()
                .texts
                .iter()
                .any(|run| matches!(run.text.as_str(), "Saved document" | "Pinned"))
        );
    }
    tab(&mut form, false);
    assert_eq!(form.ui.runtime.focused(), form.find_name("Action"));
}

#[test]
fn icon_manual_ast_preflight_rejects_invalid_metrics_names_and_separator_before_mutation() {
    use crate::view::MountedView;
    use rust_desktop_ui_controls::Controls;
    use rust_desktop_ui_xaml::{
        ButtonDefinition, ElementDefinition, ElementKind, IconSymbol, SeparatorDefinition,
        ToggleButtonDefinition,
    };
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let root = ui.root();
    let mut invalid = vec![
        ElementKind::Button(ButtonDefinition {
            icon: Some(IconSymbol::Save),
            ..Default::default()
        }),
        ElementKind::ToggleButton(ToggleButtonDefinition {
            icon: Some(IconSymbol::Pin),
            ..Default::default()
        }),
        ElementKind::Button(ButtonDefinition {
            icon_size: 20.0,
            ..Default::default()
        }),
    ];
    for size in [0.0, 513.0, f32::NAN] {
        invalid.push(ElementKind::Button(ButtonDefinition {
            content: "Caption".into(),
            icon: Some(IconSymbol::Add),
            icon_size: size,
            ..Default::default()
        }));
    }
    for spacing in [-1.0, 16385.0, f32::INFINITY] {
        invalid.push(ElementKind::ToggleButton(ToggleButtonDefinition {
            content: "Caption".into(),
            icon: Some(IconSymbol::Add),
            icon_spacing: spacing,
            ..Default::default()
        }));
    }
    for thickness in [0.0, -1.0, 16385.0, f32::NAN] {
        invalid.push(ElementKind::Separator(SeparatorDefinition {
            thickness,
            ..Default::default()
        }));
    }
    for kind in invalid {
        let definition = ElementDefinition {
            kind,
            ..Default::default()
        };
        let count = ui.runtime.tree().len();
        assert!(MountedView::mount(&mut ui, root, &definition).is_err());
        assert_eq!(ui.runtime.tree().len(), count);
    }
}
