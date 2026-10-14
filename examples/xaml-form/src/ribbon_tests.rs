use crate::{form::Form, generated, view::MountedView};
use rust_desktop_ui_controls::{Controls, RibbonPresentation, Role, Theme};
use rust_desktop_ui_core::{
    InputEvent, Key, Modifiers, Mutation, Point, PointerButton, Rect, Size, WidgetId,
};
use rust_desktop_ui_platform_winit::{DesktopApp, PlatformEvent, accesskit};

const WIDTH: f32 = 960.0;
const HEIGHT: f32 = 640.0;
const CONTENT: &str = r#"
    <StackPanel Spacing="12">
        <Ribbon x:Name="MainRibbon" SelectedTab="Home">
            <Ribbon.QuickAccessToolbar><Button Command="Pin" /></Ribbon.QuickAccessToolbar>
            <RibbonTab Id="Home" Header="Home" KeyTip="H">
                <RibbonGroup Id="Document" Header="Document" CollapsePriority="1">
                    <Button Command="Confirm" KeyTip="C" Size="Large" Icon="Check" ToolTip="Accept the editor value" />
                    <ToggleButton Command="Pin" KeyTip="P" Size="Small" Icon="Pin" ToolTip="Pin the document" />
                    <SplitButton Command="Confirm" KeyTip="S">
                        <MenuItem Command="Pin" />
                    </SplitButton>
                </RibbonGroup>
            </RibbonTab>
            <RibbonTab Id="View" Header="View" KeyTip="V">
                <RibbonGroup Id="ViewOptions" Header="View options">
                    <Button Command="Confirm" KeyTip="A" />
                </RibbonGroup>
            </RibbonTab>
            <RibbonTab Id="Contextual" Header="Contextual" KeyTip="T" Context="Selection">
                <RibbonGroup Id="Selection" Header="Selection">
                    <Button Command="Confirm" KeyTip="A" />
                </RibbonGroup>
            </RibbonTab>
        </Ribbon>
        <TextBox x:Name="Message" Height="36" IsRequired="True" CommitOnLostFocus="True" />
        <Button x:Name="BodyPin" Content="Body pin" Command="Pin" Height="36" />
    </StackPanel>
"#;

fn definition(content: &str) -> rust_desktop_ui_xaml::WindowDefinition {
    rust_desktop_ui_xaml::parse(&format!(
        r#"<Window xmlns="urn:rust-ui-engine:ui" xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml" Width="960" Height="640" Padding="24">{content}</Window>"#
    ))
    .unwrap()
}

fn form() -> Form {
    let mut form = Form::new(definition(CONTENT), false).unwrap();
    update(&mut form, WIDTH);
    form
}

fn update(form: &mut Form, width: f32) {
    form.update(Size::new(width, HEIGHT), 1.0).unwrap();
}

fn key_down(form: &mut Form, key: Key, control: bool) {
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

fn key_up(form: &mut Form, key: Key) {
    form.event(PlatformEvent::Input(InputEvent::KeyUp {
        key,
        modifiers: Modifiers::default(),
    }))
    .unwrap();
}

fn stroke(form: &mut Form, key: Key) {
    key_down(form, key.clone(), false);
    key_up(form, key);
}

fn letter(form: &mut Form, text: &str) {
    let key = Key::Character(text.into());
    key_down(form, key.clone(), false);
    form.event(PlatformEvent::Text(text.into())).unwrap();
    key_up(form, key);
}

fn focus_editor(form: &mut Form) {
    form.ui
        .runtime
        .apply(Mutation::Focus(Some(form.editor())))
        .unwrap();
}

fn accessible_action(form: &mut Form, id: WidgetId, action: accesskit::Action) {
    form.accessibility(1.0);
    let target_node = form.accessible.id(id).unwrap();
    form.event(PlatformEvent::Accessibility(accesskit::ActionRequest {
        action,
        target_tree: accesskit::TreeId::ROOT,
        target_node,
        data: None,
    }))
    .unwrap();
}

fn click(form: &mut Form, id: WidgetId) {
    let bounds = form.ui.runtime.bounds(id).unwrap();
    let position = Point::new(
        bounds.x + bounds.width / 2.0,
        bounds.y + bounds.height / 2.0,
    );
    for input in [
        InputEvent::PointerDown {
            position,
            button: PointerButton::Primary,
            modifiers: Modifiers::default(),
        },
        InputEvent::PointerUp {
            position,
            button: PointerButton::Primary,
            modifiers: Modifiers::default(),
        },
    ] {
        form.event(PlatformEvent::Input(input)).unwrap();
    }
}

fn tab_id(form: &Form, label: &str) -> WidgetId {
    form.ui
        .semantics()
        .into_iter()
        .find(|node| node.role == Role::Tab && node.label == label)
        .unwrap_or_else(|| panic!("missing tab {label}"))
        .id
}

fn control_geometry(form: &Form) -> Vec<(String, String, Rect)> {
    let mut result = form
        .ui
        .semantics()
        .into_iter()
        .filter(|node| {
            matches!(
                node.role,
                Role::Button | Role::ToggleButton | Role::Tab | Role::TextBox
            )
        })
        .map(|node| (format!("{:?}", node.role), node.label, node.bounds))
        .collect::<Vec<_>>();
    result.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then_with(|| a.1.cmp(&b.1))
            .then_with(|| a.2.y.total_cmp(&b.2.y))
            .then_with(|| a.2.x.total_cmp(&b.2.x))
    });
    result
}

fn qat_pin(form: &Form) -> rust_desktop_ui_controls::SemanticNode {
    form.ui
        .semantics()
        .into_iter()
        .filter(|node| node.role == Role::ToggleButton && node.label == "Закрепить")
        .min_by(|a, b| a.bounds.y.total_cmp(&b.bounds.y))
        .expect("the fixture has a QAT Pin command")
}

#[test]
fn ribbon_generated_definition_matches_source_and_mounts_with_editor() {
    let source = rust_desktop_ui_xaml::parse(include_str!("../MainWindow.xaml")).unwrap();
    let compiled = generated::window_definition();
    assert_eq!(compiled, source);
    let mut form = Form::new(compiled, false).unwrap();
    form.update(Size::new(source.width as f32, source.height as f32), 1.0)
        .unwrap();
    let mounted = form.view.ribbon.as_ref().expect("sample contains Ribbon");
    assert_eq!(form.find_name("MainRibbon"), Some(mounted.wrapper));
    assert!(mounted.ribbon.container().is_some());
    let ribbon_bounds = form.ui.runtime.bounds(mounted.wrapper).unwrap();
    let editor_bounds = form.ui.runtime.bounds(form.editor()).unwrap();
    assert!(ribbon_bounds.width > 0.0 && ribbon_bounds.height > 0.0);
    assert!(editor_bounds.y >= ribbon_bounds.y + ribbon_bounds.height);
    assert!(editor_bounds.width > 0.0);
    assert!(!form.scene().texts.is_empty());
}

#[test]
fn ribbon_mount_rejects_unknown_commands_and_action_only_toggle_before_tree_mutation() {
    for content in [
        CONTENT.replace("Command=\"Pin\" /></Ribbon.QuickAccessToolbar>", "Command=\"Missing\" /></Ribbon.QuickAccessToolbar>"),
        CONTENT.replace("<MenuItem Command=\"Pin\" />", "<MenuItem Command=\"Missing\" />"),
        CONTENT.replace(
            "<RibbonGroup Id=\"Selection\" Header=\"Selection\">\n                    <Button Command=\"Confirm\"",
            "<RibbonGroup Id=\"Selection\" Header=\"Selection\">\n                    <Button Command=\"Missing\"",
        ),
        CONTENT.replace("<ToggleButton Command=\"Pin\"", "<ToggleButton Command=\"Confirm\""),
    ] {
        let mut ui = Controls::new(Theme::FluentLight).unwrap();
        ui.commands_mut().register("Confirm", "Confirm");
        ui.commands_mut().register_toggle("Pin", "Pin", true);
        let root = ui.root();
        let before = ui.runtime.tree().len();
        let result = MountedView::mount(&mut ui, root, &definition(&content).content);
        assert!(result.is_err(), "unexpectedly mounted invalid commands: {content}");
        assert_eq!(ui.runtime.tree().len(), before, "preflight left retained nodes");
        assert_eq!(ui.commands().checked(&"Pin".into()), Some(true));
        assert_eq!(ui.commands().checked(&"Confirm".into()), None);
        assert!(ui.commands_mut().drain_invocations().is_empty());
    }
}

#[test]
fn ribbon_mount_rejects_conflicting_command_appearance_before_creating_siblings() {
    for property in ["Icon=\"Close\"", "ToolTip=\"A conflicting tooltip\""] {
        let content = CONTENT.replace(
            "<SplitButton Command=\"Confirm\" KeyTip=\"S\">",
            &format!("<SplitButton Command=\"Confirm\" KeyTip=\"S\" {property}>"),
        );
        let mut ui = Controls::new(Theme::FluentLight).unwrap();
        ui.commands_mut().register("Confirm", "Confirm");
        ui.commands_mut().register_toggle("Pin", "Pin", false);
        let root = ui.root();
        let count = ui.runtime.tree().len();
        assert!(matches!(
            MountedView::mount(&mut ui, root, &definition(&content).content),
            Err(error) if error.contains("Conflicting")
        ));
        assert_eq!(ui.runtime.tree().len(), count);
    }
}

#[test]
fn ribbon_reflow_and_theme_changes_preserve_editor_document_accepted_value_and_undo() {
    let mut form = form();
    let editor = form.editor();
    let wrapper = form.view.ribbon.as_ref().unwrap().wrapper;
    focus_editor(&mut form);
    form.event(PlatformEvent::Text("accepted".into())).unwrap();
    stroke(&mut form, Key::Enter);
    form.event(PlatformEvent::Text(" draft".into())).unwrap();
    assert_eq!(form.ui.accepted_value(editor), Some("accepted"));
    assert_eq!(form.text(), "accepted draft");
    let selection = form.ui.document(editor).unwrap().selection();
    for (width, theme) in [
        (360.0, Theme::FluentLight),
        (96.0, Theme::FluentDark),
        (WIDTH, Theme::FluentLight),
    ] {
        form.ui.set_theme(theme).unwrap();
        update(&mut form, width);
        assert_eq!(form.editor(), editor);
        assert_eq!(form.view.ribbon.as_ref().unwrap().wrapper, wrapper);
        assert_eq!(form.ui.runtime.focused(), Some(editor));
        assert_eq!(form.text(), "accepted draft");
        assert_eq!(form.ui.accepted_value(editor), Some("accepted"));
        assert_eq!(form.ui.document(editor).unwrap().selection(), selection);
        assert!(form.ui.document(editor).unwrap().can_undo());
        let ribbon_bounds = form.ui.runtime.bounds(wrapper).unwrap();
        let editor_bounds = form.ui.runtime.bounds(editor).unwrap();
        assert!(editor_bounds.y >= ribbon_bounds.y + ribbon_bounds.height);
        if width == 96.0 {
            assert!(matches!(
                form.view.ribbon.as_ref().unwrap().ribbon.presentations(),
                [RibbonPresentation::Overflow(_)]
            ));
        }
    }
    key_down(&mut form, Key::Character("z".into()), true);
    assert_eq!(form.text(), "accepted");
    assert_eq!(form.ui.accepted_value(editor), Some("accepted"));
}

#[test]
fn ribbon_tabs_process_pointer_and_accessibility_activation_and_preserve_siblings() {
    let mut form = form();
    let editor = form.editor();
    let body_pin = form.find_name("BodyPin").unwrap();
    let view = tab_id(&form, "View");
    click(&mut form, view);
    update(&mut form, WIDTH);
    assert_eq!(
        form.view.ribbon.as_ref().unwrap().ribbon.active_tab(),
        "View"
    );
    let home = tab_id(&form, "Home");
    form.accessibility(1.0);
    let target_node = form.accessible.id(home).unwrap();
    form.event(PlatformEvent::Accessibility(accesskit::ActionRequest {
        action: accesskit::Action::Click,
        target_tree: accesskit::TreeId::ROOT,
        target_node,
        data: None,
    }))
    .unwrap();
    update(&mut form, WIDTH);
    assert_eq!(
        form.view.ribbon.as_ref().unwrap().ribbon.active_tab(),
        "Home"
    );
    assert_eq!(form.editor(), editor);
    assert_eq!(form.find_name("BodyPin"), Some(body_pin));
    let tree = form.accessibility(1.5);
    for (label, selected) in [("Home", true), ("View", false)] {
        let (_, node) = tree
            .nodes
            .iter()
            .find(|(_, node)| node.label() == Some(label))
            .unwrap();
        assert_eq!(node.role(), accesskit::Role::Tab);
        assert_eq!(node.is_selected(), Some(selected));
        assert!(node.supports_action(accesskit::Action::Click));
    }
    assert!(
        !tree
            .nodes
            .iter()
            .any(|(_, node)| node.label() == Some("Contextual"))
    );
}

#[test]
fn ribbon_keytips_activate_once_suppress_paired_text_and_restore_editor_focus() {
    let mut form = form();
    focus_editor(&mut form);
    form.event(PlatformEvent::Text("draft".into())).unwrap();
    stroke(&mut form, Key::F10);
    letter(&mut form, "h");
    letter(&mut form, "p");
    update(&mut form, WIDTH);
    assert!(!form.view.ribbon.as_ref().unwrap().ribbon.key_tips_visible());
    assert_eq!(form.ui.commands().checked(&"Pin".into()), Some(true));
    assert_eq!(
        form.ui.checked(form.find_name("BodyPin").unwrap()),
        Some(true)
    );
    assert_eq!(form.ui.runtime.focused(), Some(form.editor()));
    assert_eq!(form.text(), "draft");
    assert_eq!(form.ui.accepted_value(form.editor()), Some("draft"));
    form.event(PlatformEvent::Text("!".into())).unwrap();
    assert_eq!(form.text(), "draft!");
    stroke(&mut form, Key::F10);
    letter(&mut form, "1");
    assert_eq!(form.ui.commands().checked(&"Pin".into()), Some(false));
    assert_eq!(form.text(), "draft!");
    assert_eq!(form.ui.runtime.focused(), Some(form.editor()));
}

#[test]
fn ribbon_keytip_badges_preserve_control_names_geometry_and_editor_selection() {
    for theme in [Theme::FluentLight, Theme::FluentDark] {
        let mut form = form();
        form.ui.set_theme(theme).unwrap();
        update(&mut form, WIDTH);
        focus_editor(&mut form);
        form.event(PlatformEvent::Text("draft".into())).unwrap();
        stroke(&mut form, Key::ArrowLeft);
        let selection = form.ui.document(form.editor()).unwrap().selection();
        let before = control_geometry(&form);
        let presentations = form
            .view
            .ribbon
            .as_ref()
            .unwrap()
            .ribbon
            .presentations()
            .to_vec();

        for (key, badges) in [
            (Key::F10, &["H", "V", "1"][..]),
            (Key::Character("h".into()), &["C", "P", "S"][..]),
            (Key::Escape, &["H", "V", "1"][..]),
        ] {
            stroke(&mut form, key);
            update(&mut form, WIDTH);
            assert_eq!(control_geometry(&form), before);
            assert_eq!(
                form.view.ribbon.as_ref().unwrap().ribbon.presentations(),
                presentations
            );
            for badge in badges {
                assert!(
                    form.scene().texts.iter().any(|run| run.text == *badge),
                    "missing independent KeyTip badge {badge}"
                );
                assert!(!form.ui.semantics().iter().any(|node| node.label == *badge));
            }
            assert_eq!(form.text(), "draft");
            assert_eq!(
                form.ui.document(form.editor()).unwrap().selection(),
                selection
            );
        }
        stroke(&mut form, Key::Escape);
        update(&mut form, WIDTH);
        assert_eq!(control_geometry(&form), before);
        assert_eq!(form.ui.runtime.focused(), Some(form.editor()));
        assert!(
            !form
                .scene()
                .texts
                .iter()
                .any(|run| { matches!(run.text.as_str(), "H" | "V" | "1" | "C" | "P" | "S") })
        );
        assert_eq!(form.ui.commands().checked(&"Pin".into()), Some(false));
    }
}

#[test]
fn ribbon_icon_only_qat_retains_accessible_name_tooltip_and_shared_activation() {
    let mut form = form();
    let qat = qat_pin(&form);
    assert_eq!(qat.tooltip.as_deref(), Some("Pin the document"));
    assert_eq!(qat.bounds.width, qat.bounds.height);
    assert!(qat.bounds.width <= 32.0);
    assert!(!form.scene().texts.iter().any(|run| {
        run.text == "Закрепить" && run.bounds.intersection(qat.bounds).is_some()
    }));
    let tree = form.accessibility(1.5);
    let (_, accessible) = tree
        .nodes
        .iter()
        .find(|(id, _)| Some(*id) == form.accessible.id(qat.id))
        .unwrap();
    assert_eq!(accessible.label(), Some("Закрепить"));
    assert!(accessible.supports_action(accesskit::Action::Click));
    accessible_action(&mut form, qat.id, accesskit::Action::Click);
    update(&mut form, WIDTH);
    assert_eq!(qat_pin(&form).checked, Some(true));
    assert_eq!(
        form.ui.checked(form.find_name("BodyPin").unwrap()),
        Some(true)
    );
}

#[test]
fn ribbon_qat_without_an_icon_remains_visible_as_text_and_can_activate() {
    let content = CONTENT.replace(" Icon=\"Pin\"", "");
    let mut form = Form::new(definition(&content), false).unwrap();
    update(&mut form, WIDTH);
    let qat = qat_pin(&form);
    assert!(form.scene().texts.iter().any(|run| {
        run.text == "Закрепить" && run.bounds.intersection(qat.bounds).is_some()
    }));
    assert!(qat.bounds.width > qat.bounds.height);
    click(&mut form, qat.id);
    update(&mut form, WIDTH);
    assert_eq!(form.ui.commands().checked(&"Pin".into()), Some(true));
    assert_eq!(
        form.ui.checked(form.find_name("BodyPin").unwrap()),
        Some(true)
    );
}

#[test]
fn ribbon_keytips_cancel_on_escape_blur_and_disabled_wrapper_without_consuming_editing() {
    let mut form = form();
    focus_editor(&mut form);
    stroke(&mut form, Key::F10);
    letter(&mut form, "h");
    stroke(&mut form, Key::Escape);
    assert!(form.view.ribbon.as_ref().unwrap().ribbon.key_tips_visible());
    stroke(&mut form, Key::Escape);
    assert!(!form.view.ribbon.as_ref().unwrap().ribbon.key_tips_visible());
    assert_eq!(form.ui.runtime.focused(), Some(form.editor()));
    stroke(&mut form, Key::F10);
    form.event(PlatformEvent::Input(InputEvent::WindowFocused(false)))
        .unwrap();
    assert!(!form.view.ribbon.as_ref().unwrap().ribbon.key_tips_visible());
    form.event(PlatformEvent::Input(InputEvent::WindowFocused(true)))
        .unwrap();
    let wrapper = form.view.ribbon.as_ref().unwrap().wrapper;
    form.ui.set_enabled(wrapper, false).unwrap();
    focus_editor(&mut form);
    stroke(&mut form, Key::F10);
    letter(&mut form, "p");
    assert_eq!(form.text(), "p");
    assert_eq!(form.ui.commands().checked(&"Pin".into()), Some(false));
    assert_eq!(form.ui.runtime.focused(), Some(form.editor()));
}

#[test]
fn ribbon_accessibility_focus_cancels_keytips_and_accepts_text_without_a_new_keydown() {
    let mut form = form();
    focus_editor(&mut form);
    stroke(&mut form, Key::F10);
    letter(&mut form, "h");
    assert!(form.view.ribbon.as_ref().unwrap().ribbon.key_tips_visible());
    let editor = form.editor();
    accessible_action(&mut form, editor, accesskit::Action::Focus);
    assert!(!form.view.ribbon.as_ref().unwrap().ribbon.key_tips_visible());
    assert_eq!(form.ui.runtime.focused(), Some(editor));
    form.event(PlatformEvent::Text("direct ".into())).unwrap();
    letter(&mut form, "p");
    assert_eq!(form.text(), "direct p");
    assert_eq!(form.ui.commands().checked(&"Pin".into()), Some(false));
}

#[test]
fn ribbon_accessibility_click_cancels_keytips_without_restoring_previous_focus() {
    let mut form = form();
    focus_editor(&mut form);
    stroke(&mut form, Key::F10);
    letter(&mut form, "h");
    let ribbon_focus = form.ui.runtime.focused();
    assert_ne!(ribbon_focus, Some(form.editor()));
    let body_pin = form.find_name("BodyPin").unwrap();
    accessible_action(&mut form, body_pin, accesskit::Action::Click);
    assert!(!form.view.ribbon.as_ref().unwrap().ribbon.key_tips_visible());
    assert_eq!(form.ui.runtime.focused(), ribbon_focus);
    assert_eq!(form.ui.commands().checked(&"Pin".into()), Some(true));
    focus_editor(&mut form);
    form.event(PlatformEvent::Text("after click".into()))
        .unwrap();
    assert_eq!(form.text(), "after click");
    assert_eq!(form.ui.commands().checked(&"Pin".into()), Some(true));
}

#[test]
fn ribbon_contextual_tab_uses_model_context_and_returns_to_available_tab() {
    let mut form = form();
    assert!(
        !form
            .view
            .ribbon
            .as_mut()
            .unwrap()
            .ribbon
            .set_active_tab("Contextual")
    );
    form.view
        .ribbon
        .as_mut()
        .unwrap()
        .ribbon
        .set_context(Some("Selection"));
    update(&mut form, WIDTH);
    let context = tab_id(&form, "Contextual");
    click(&mut form, context);
    update(&mut form, WIDTH);
    assert_eq!(
        form.view.ribbon.as_ref().unwrap().ribbon.active_tab(),
        "Contextual"
    );
    form.view.ribbon.as_mut().unwrap().ribbon.set_context(None);
    update(&mut form, WIDTH);
    assert_eq!(
        form.view.ribbon.as_ref().unwrap().ribbon.active_tab(),
        "Home"
    );
    assert!(
        !form
            .ui
            .semantics()
            .iter()
            .any(|node| node.role == Role::Tab && node.label == "Contextual")
    );
    assert_eq!(form.find_name("Message"), Some(form.editor()));
}

#[test]
fn ribbon_split_arrow_only_opens_menu_and_selection_shares_toggle_with_body() {
    let content = CONTENT.replace("CommitOnLostFocus=\"True\"", "CommitOnLostFocus=\"False\"");
    let mut form = Form::new(definition(&content), false).unwrap();
    update(&mut form, WIDTH);
    focus_editor(&mut form);
    form.event(PlatformEvent::Text("draft".into())).unwrap();
    let nodes = form.ui.semantics();
    let arrow = nodes
        .iter()
        .find(|node| {
            node.has_popup && node.role == Role::Button && form.ui.command_id(node.id).is_none()
        })
        .unwrap();
    assert!(arrow.label.contains("Подтвердить"));
    assert!(!form.scene().texts.iter().any(|run| run.text == "More"));
    let primary = nodes
        .iter()
        .find(|node| {
            node.parent == arrow.parent && node.label == "Подтвердить" && node.role == Role::Button
        })
        .unwrap()
        .id;
    let arrow = arrow.id;
    let bounds = form.ui.runtime.bounds(arrow).unwrap();
    assert!(
        bounds.width > 0.0 && bounds.height > 0.0,
        "split dropdown bounds: {bounds:?}"
    );
    assert_eq!(
        form.ui.runtime.hit_test(Point::new(
            bounds.x + bounds.width / 2.0,
            bounds.y + bounds.height / 2.0
        )),
        Some(arrow),
        "split dropdown center is not its hit area: {bounds:?}"
    );
    click(&mut form, arrow);
    assert!(
        form.ui.popup_open(),
        "the dropdown click must open its menu before reflow"
    );
    update(&mut form, WIDTH);
    assert!(form.ui.popup_open());
    assert_eq!(form.ui.accepted_value(form.editor()), Some(""));
    assert_eq!(form.ui.commands().checked(&"Pin".into()), Some(false));
    let tree = form.accessibility(1.0);
    let (_, menu_item) = tree
        .nodes
        .iter()
        .find(|(_, node)| node.role() == accesskit::Role::MenuItemCheckBox)
        .unwrap();
    assert_eq!(menu_item.toggled(), Some(accesskit::Toggled::False));
    stroke(&mut form, Key::Enter);
    update(&mut form, WIDTH);
    assert!(!form.ui.popup_open());
    assert_eq!(form.ui.commands().checked(&"Pin".into()), Some(true));
    assert_eq!(
        form.ui.checked(form.find_name("BodyPin").unwrap()),
        Some(true)
    );
    assert_eq!(form.ui.accepted_value(form.editor()), Some(""));
    click(&mut form, primary);
    update(&mut form, WIDTH);
    assert_eq!(form.ui.accepted_value(form.editor()), Some("draft"));
    assert!(!form.ui.popup_open());
}
