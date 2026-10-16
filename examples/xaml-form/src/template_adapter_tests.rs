use crate::{form::Form, view::MountedView};
use rust_desktop_ui_controls::{BindingContext, Controls, DataValue, Role, Theme};
use rust_desktop_ui_core::{Mutation, Size, WidgetId};
use rust_desktop_ui_platform_winit::DesktopApp;
use rust_desktop_ui_xaml::{ControlTemplateDefinition, ElementDefinition, ElementKind};
use std::{cell::RefCell, rc::Rc};

const SIZE: Size = Size::new(640.0, 480.0);
fn parse(body: &str) -> rust_desktop_ui_xaml::WindowDefinition {
    rust_desktop_ui_xaml::parse(&format!(r#"<Window xmlns="urn:rust-ui-engine:ui" xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml" Padding="0">{body}</Window>"#)).unwrap()
}
fn template(text: &str) -> ControlTemplateDefinition {
    let window = parse(&format!(
        r#"<Window.Resources><ControlTemplate x:Key="Action" TargetType="Button"><Border Padding="2"><ContentPresenter x:Name="PART_Content"/></Border></ControlTemplate></Window.Resources><Button Content="{text}" Template="{{StaticResource Action}}"/>"#
    ));
    *window.content.common.control_template.unwrap()
}

#[test]
fn auto_height_button_template_is_centered_on_mount_and_repeated_replacement() {
    fn window(icon_height: u32) -> rust_desktop_ui_xaml::WindowDefinition {
        parse(&format!(
            r#"<Window.Resources><ControlTemplate x:Key="Action" TargetType="Button"><StackPanel Orientation="Horizontal" Spacing="8"><Icon Symbol="Check" Width="20" Height="{icon_height}"/><ContentPresenter x:Name="PART_Content"/></StackPanel></ControlTemplate></Window.Resources><Button x:Name="Action" Content="Template caption" Width="260" Height="60" Template="{{StaticResource Action}}"/>"#
        ))
    }
    fn assert_centered(form: &Form, owner: WidgetId) {
        let content = form.ui.button_content(owner).unwrap();
        let owner_bounds = form.ui.runtime.bounds(owner).unwrap();
        let bounds = form.ui.runtime.bounds(content).unwrap();
        assert!(bounds.height < owner_bounds.height - 8.0, "{bounds:?}");
        assert!(
            ((bounds.y + bounds.height * 0.5) - (owner_bounds.y + owner_bounds.height * 0.5)).abs()
                < 0.5,
            "owner={owner_bounds:?}, content={bounds:?}"
        );
        for child in form.ui.runtime.tree().children(content).unwrap() {
            let child = form.ui.runtime.bounds(*child).unwrap();
            assert!(child.y > owner_bounds.y + 4.0, "{child:?}");
            assert!(
                child.y + child.height < owner_bounds.y + owner_bounds.height - 4.0,
                "{child:?}"
            );
        }
    }
    for dark in [false, true] {
        let mut form = Form::new(window(20), dark).unwrap();
        form.update(SIZE, 1.0).unwrap();
        let owner = form.find_name("Action").unwrap();
        assert_centered(&form, owner);
        form.ui.runtime.apply(Mutation::Focus(Some(owner))).unwrap();
        for icon_height in [28, 16, 20] {
            let old = form.ui.button_content(owner).unwrap();
            let template = window(icon_height).content.common.control_template.unwrap();
            form.view
                .replace_control_template(&mut form.ui, owner, &template)
                .unwrap();
            form.update(SIZE, 1.0).unwrap();
            assert_centered(&form, owner);
            assert_eq!(form.ui.runtime.focused(), Some(owner));
            assert_eq!(form.find_name("Action"), Some(owner));
            assert!(!form.ui.runtime.tree().contains(old));
        }
    }
}
#[test]
fn control_template_swap_preserves_behavior_owner_focus_and_binding_engine() {
    let context = Rc::new(RefCell::new(
        BindingContext::new(DataValue::object([("Caption", "Bound caption".into())])).unwrap(),
    ));
    let mut form = Form::new_with_model(parse(r#"<StackPanel><Button x:Name="Action" Content="{Binding Caption}" ToolTip="Original tip"/><TextBox x:Name="Message" Text="Original"/></StackPanel>"#), false, Some(context.clone())).unwrap();
    form.update(SIZE, 1.0).unwrap();
    let owner = form.find_name("Action").unwrap();
    let editor = form.find_name("Message").unwrap();
    form.ui.set_text(editor, "Uncommitted draft").unwrap();
    form.ui.runtime.apply(Mutation::Focus(Some(owner))).unwrap();
    form.view
        .replace_control_template(&mut form.ui, owner, &template("First decoration"))
        .unwrap();
    form.update(SIZE, 1.0).unwrap();
    let first = form.ui.button_content(owner).unwrap();
    for caption in ["Second decoration", "Third decoration"] {
        form.view
            .replace_control_template(&mut form.ui, owner, &template(caption))
            .unwrap();
        form.update(SIZE, 1.0).unwrap();
        assert_eq!(form.find_name("Action"), Some(owner));
        assert_eq!(form.ui.runtime.focused(), Some(owner));
        assert_eq!(
            form.ui.document(editor).unwrap().text(),
            "Uncommitted draft"
        );
        assert_eq!(form.view.roles[&owner], Role::Button);
    }
    assert!(!form.ui.runtime.tree().contains(first));
    assert!(form.view.bindings.is_some());
    assert!(Rc::ptr_eq(
        &form.view.bindings.as_ref().unwrap().context(),
        &context
    ));
    context
        .borrow_mut()
        .set("Caption", "Updated owner caption".into())
        .unwrap();
    form.update(SIZE, 1.0).unwrap();
    let semantic = form
        .ui
        .semantics()
        .into_iter()
        .find(|node| node.id == owner)
        .unwrap();
    assert_eq!(semantic.label, "Updated owner caption");
    assert_eq!(semantic.tooltip.as_deref(), Some("Original tip"));
    assert_eq!(semantic.role, Role::Button);
    assert!(semantic.focused);
}
#[test]
fn group_header_swap_preserves_child_editor_draft_focus_and_stable_name() {
    let mut form = Form::new(parse(r#"<GroupBox x:Name="Group" Header="Original accessible name"><GroupBox.HeaderTemplate><TextBlock Text="Old presentation"/></GroupBox.HeaderTemplate><TextBox x:Name="Message" Text="Original"/></GroupBox>"#), false).unwrap();
    form.update(SIZE, 1.0).unwrap();
    let owner = form.find_name("Group").unwrap();
    let editor = form.find_name("Message").unwrap();
    let header = form.ui.group_header_id(owner).unwrap();
    let previous = form.ui.button_content(header).unwrap();
    let header_bounds = form.ui.runtime.bounds(header).unwrap();
    let content_bounds = form.ui.runtime.bounds(previous).unwrap();
    assert!(
        ((header_bounds.y + header_bounds.height * 0.5)
            - (content_bounds.y + content_bounds.height * 0.5))
            .abs()
            < 0.5
    );
    form.ui.set_text(editor, "Draft remains").unwrap();
    form.ui
        .runtime
        .apply(Mutation::Focus(Some(editor)))
        .unwrap();
    let replacement = parse(r#"<StackPanel Orientation="Horizontal"><Icon Symbol="Check"/><TextBlock Text="New presentation"/></StackPanel>"#).content;
    form.view
        .replace_group_header_template(&mut form.ui, owner, &replacement)
        .unwrap();
    form.update(SIZE, 1.0).unwrap();
    assert_eq!(form.find_name("Message"), Some(editor));
    assert_eq!(form.ui.runtime.focused(), Some(editor));
    assert_eq!(form.ui.group_header_id(owner), Some(header));
    assert_eq!(
        form.ui.group_header(owner),
        Some("Original accessible name")
    );
    assert_eq!(form.ui.document(editor).unwrap().text(), "Draft remains");
    assert!(!form.ui.runtime.tree().contains(previous));
}
#[test]
fn invalid_template_parts_and_binding_configuration_roll_back_without_leaks() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    ui.update(SIZE).unwrap();
    let root = ui.root();
    let mut view = MountedView::mount(
        &mut ui,
        root,
        &parse(r#"<Button x:Name="Action" Content="Original"/>"#).content,
    )
    .unwrap();
    let owner = view.find_name("Action").unwrap();
    view.replace_control_template(&mut ui, owner, &template("Valid"))
        .unwrap();
    let content = ui.button_content(owner);
    let ids = ui.runtime.tree().live_ids();
    let mut invalid = template("Invalid");
    invalid.content_part = "MissingRequiredPart".into();
    assert!(
        view.replace_control_template(&mut ui, owner, &invalid)
            .is_err()
    );
    let mut invalid = template("Binding failure");
    fn inject(element: &mut ElementDefinition) {
        if matches!(element.kind, ElementKind::TextBlock(_)) {
            element.common.bindings.insert(
                "Text".into(),
                rust_desktop_ui_xaml::BindingDefinition {
                    path: "Text".into(),
                    element_name: Some("MissingOwner".into()),
                    ..Default::default()
                },
            );
        }
        for child in element.children_mut() {
            inject(child);
        }
    }
    inject(&mut invalid.visual);
    assert!(
        view.replace_control_template(&mut ui, owner, &invalid)
            .unwrap_err()
            .contains("Unknown ElementName")
    );
    assert_eq!(ui.runtime.tree().live_ids(), ids);
    assert_eq!(ui.button_content(owner), content);
    assert_eq!(view.find_name("Action"), Some(owner));
}

#[test]
fn replacement_private_names_do_not_capture_existing_editors_or_another_instance() {
    let mut form = Form::new(parse(r#"<StackPanel><GroupBox x:Name="Group" Header="Header"><TextBox x:Name="Message" Text="Draft"/></GroupBox><Button x:Name="One" Content="One"/><Button x:Name="Two" Content="Two"/></StackPanel>"#), false).unwrap();
    form.update(SIZE, 1.0).unwrap();
    let editor = form.find_name("Message").unwrap();
    let group = form.find_name("Group").unwrap();
    let header = parse(r#"<TextBlock x:Name="Message" Text="Decoration"/>"#).content;
    form.view
        .replace_group_header_template(&mut form.ui, group, &header)
        .unwrap();
    assert_eq!(form.find_name("Message"), Some(editor));
    let template = template("Reusable");
    for name in ["One", "Two", "One"] {
        let owner = form.find_name(name).unwrap();
        form.view
            .replace_control_template(&mut form.ui, owner, &template)
            .unwrap();
        assert_eq!(form.find_name("Message"), Some(editor));
    }
    form.update(SIZE, 1.0).unwrap();
    assert_eq!(form.ui.document(editor).unwrap().text(), "Draft");
}

#[test]
fn decorative_writes_are_rejected_before_shared_model_or_owner_changes() {
    let mut form = Form::new(
        parse(r#"<Button x:Name="Action" Content="Original"/>"#),
        false,
    )
    .unwrap();
    let owner = form.find_name("Action").unwrap();
    let before = form.ui.runtime.tree().live_ids();
    let mut definition = template("Forbidden write");
    let part = definition.visual.children_mut().into_iter().next().unwrap();
    part.common.bindings.insert(
        "Text".into(),
        rust_desktop_ui_xaml::BindingDefinition {
            path: "Caption".into(),
            mode: rust_desktop_ui_xaml::BindingMode::OneWayToSource,
            ..Default::default()
        },
    );
    assert!(
        form.view
            .replace_control_template(&mut form.ui, owner, &definition)
            .unwrap_err()
            .contains("OneWay or OneTime")
    );
    assert_eq!(form.ui.runtime.tree().live_ids(), before);
}
