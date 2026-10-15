use crate::{form::Form, forms_model, generated_groups, view::MountedView};
use rust_desktop_ui_controls::{BindingContext, Controls, DataValue, GroupAppearance, Role, Theme};
use rust_desktop_ui_core::{Point, Rect, Size};
use rust_desktop_ui_platform_winit::{DesktopApp, PlatformEvent, accesskit};
use rust_desktop_ui_xaml::{ElementKind, WindowDefinition};
use std::{cell::RefCell, rc::Rc};

const SIZE: Size = Size::new(400.0, 300.0);

fn definition(content: &str) -> WindowDefinition {
    rust_desktop_ui_xaml::parse(&format!(
        "<Window xmlns='urn:rust-ui-engine:ui' xmlns:x='http://schemas.microsoft.com/winfx/2006/xaml' Width='400' Height='300' Padding='0'>{content}</Window>"
    )).unwrap()
}

fn form(content: &str) -> Form {
    let mut form = Form::new(definition(content), false).unwrap();
    form.update(SIZE, 1.0).unwrap();
    form
}

fn bounds(form: &Form, name: &str) -> Rect {
    form.ui
        .runtime
        .bounds(form.find_name(name).unwrap())
        .unwrap()
}

fn near(actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs() < 0.1,
        "actual={actual} expected={expected}"
    );
}

#[test]
fn compiled_groups_demo_mounts_all_appearances_and_keeps_collapsed_content() {
    let mut form = Form::new_with_model(
        generated_groups::window_definition(),
        false,
        Some(forms_model::model().unwrap()),
    )
    .unwrap();
    form.update(Size::new(1060.0, 800.0), 1.0).unwrap();
    for (name, appearance) in [
        ("PrimaryGroup", GroupAppearance::Card),
        ("OutlineGroup", GroupAppearance::Outline),
        ("PlainGroup", GroupAppearance::Plain),
        ("StandardGroup", GroupAppearance::Standard),
    ] {
        let group = form.find_name(name).unwrap();
        assert_eq!(
            form.ui.group_settings(group).unwrap().appearance,
            appearance
        );
        assert_eq!(form.view.roles[&group], Role::Group);
    }
    let archived = form.find_name("ArchiveEditor").unwrap();
    let group = form.find_name("AdvancedGroup").unwrap();
    assert!(form.ui.runtime.bounds(archived).is_none());
    form.ui.set_expander_expanded(group, true).unwrap();
    form.update(Size::new(1060.0, 800.0), 1.0).unwrap();
    assert_eq!(form.find_name("ArchiveEditor"), Some(archived));
    assert_eq!(
        form.ui.document(archived).unwrap().text(),
        "Новая версия документа"
    );
    assert!(form.ui.runtime.bounds(archived).is_some());
}

#[test]
fn rich_group_chrome_and_hidden_header_are_projected_into_controls() {
    use rust_desktop_ui_controls::{GroupHeaderStyle, IconSymbol};
    let form = form(
        "<StackPanel Spacing='0'><GroupBox x:Name='Styled' Header='Caption' Appearance='Standard' HeaderStyle='Light' HeaderIcon='Lock' CornerRadius='3' HeaderForeground='#80402010' HeaderBackground='#00112233' Padding='0'><TextBox Height='36'/></GroupBox><GroupBox x:Name='Hidden' Header='Accessible' ShowHeader='False' Padding='0'><TextBox Height='36'/></GroupBox></StackPanel>",
    );
    let settings = form
        .ui
        .group_settings(form.find_name("Styled").unwrap())
        .unwrap();
    assert_eq!(settings.appearance, GroupAppearance::Standard);
    assert_eq!(settings.header_style, GroupHeaderStyle::Light);
    assert_eq!(settings.header_icon, Some(IconSymbol::Lock));
    assert_eq!(settings.corner_radius, Some(3.0));
    assert!(settings.header_foreground.is_some());
    assert!(settings.header_background.is_some());
    near(bounds(&form, "Styled").height, 72.0);
    near(bounds(&form, "Hidden").height, 36.0);
}

#[test]
fn collapsible_group_box_binding_uses_retained_auto_height() {
    let model = Rc::new(RefCell::new(
        BindingContext::new(DataValue::object([("Open", true.into())])).unwrap(),
    ));
    let mut form = Form::new_with_model(definition("<GroupBox x:Name='Group' Header='Details' IsCollapsible='True' IsExpanded='{Binding Open}' HeaderStyle='Light'><TextBox x:Name='Editor' Text='Accepted'/></GroupBox>"), false, Some(model.clone())).unwrap();
    form.update(SIZE, 1.0).unwrap();
    let group = form.find_name("Group").unwrap();
    let editor = form.find_name("Editor").unwrap();
    form.ui.set_text(editor, "Draft").unwrap();
    model.borrow_mut().set("Open", false.into()).unwrap();
    form.update(SIZE, 1.0).unwrap();
    near(bounds(&form, "Group").height, 36.0);
    assert_eq!(form.ui.document(editor).unwrap().text(), "Draft");
    assert_eq!(form.ui.accepted_value(editor), Some("Accepted"));
    form.ui.set_expander_expanded(group, true).unwrap();
    form.update(SIZE, 1.0).unwrap();
    assert_eq!(model.borrow().get("Open").unwrap(), &DataValue::Bool(true));
    assert!(form.ui.runtime.bounds(editor).is_some());
}

#[test]
fn tabbed_group_binding_round_trips_and_keeps_hidden_page_drafts() {
    let model = Rc::new(RefCell::new(
        BindingContext::new(DataValue::object([
            ("Index", 0.0.into()),
            ("Title", "Accepted".into()),
            ("Second", "Second".into()),
        ]))
        .unwrap(),
    ));
    let mut form = Form::new_with_model(definition("<StackPanel><TabbedGroup x:Name='Tabs' SelectedIndex='{Binding Index}'><GroupTab Header='First'><TextBox x:Name='First' Text='{Binding Title}'/></GroupTab><GroupTab Header='Second'><TextBox x:Name='Second' Text='{Binding Second}'/></GroupTab><GroupTab Header='Unavailable' IsEnabled='False'/></TabbedGroup><SpinBox x:Name='IndexEditor' Minimum='0' Maximum='1' Value='{Binding SelectedIndex, ElementName=Tabs}'/></StackPanel>"), false, Some(model.clone())).unwrap();
    form.update(SIZE, 1.0).unwrap();
    let group = form.find_name("Tabs").unwrap();
    let first = form.find_name("First").unwrap();
    let second = form.find_name("Second").unwrap();
    let nodes = form.ui.runtime.tree().preorder();
    form.ui.set_text(first, "Uncommitted").unwrap();
    form.ui.set_selected_tab(group, 1).unwrap();
    form.update(SIZE, 1.0).unwrap();
    assert_eq!(
        model.borrow().get("Index").unwrap(),
        &DataValue::Number(1.0)
    );
    assert_eq!(form.ui.document(first).unwrap().text(), "Uncommitted");
    assert_eq!(form.ui.accepted_value(first), Some("Accepted"));
    assert!(form.ui.runtime.bounds(first).is_none());
    assert!(form.ui.runtime.bounds(second).is_some());
    model.borrow_mut().set("Index", 0.0.into()).unwrap();
    model
        .borrow_mut()
        .set("Second", "Updated while hidden".into())
        .unwrap();
    form.update(SIZE, 1.0).unwrap();
    assert_eq!(form.ui.selected_tab(group), Some(0));
    assert_eq!(form.ui.document(first).unwrap().text(), "Uncommitted");
    assert_eq!(
        form.ui.document(second).unwrap().text(),
        "Updated while hidden"
    );
    assert_eq!(form.ui.runtime.tree().preorder(), nodes);
    assert!(form.ui.set_selected_tab(group, 2).is_err());
    assert_eq!(form.ui.selected_tab(group), Some(0));
}

#[test]
fn root_tabbed_group_fills_height_and_preserves_separate_radio_parents() {
    let mut form = form(
        "<TabbedGroup x:Name='Tabs' Padding='10'><GroupTab Header='First'><Grid x:Name='Body'><RadioButton x:Name='First' Content='First' IsChecked='True' VerticalAlignment='Bottom'/></Grid></GroupTab><GroupTab Header='Second'><RadioButton x:Name='Second' Content='Second' IsChecked='True'/></GroupTab></TabbedGroup>",
    );
    near(bounds(&form, "Tabs").height, SIZE.height);
    near(bounds(&form, "Body").height, SIZE.height - 48.0 - 20.0);
    near(
        bounds(&form, "First").y + bounds(&form, "First").height,
        SIZE.height - 10.0,
    );
    let group = form.find_name("Tabs").unwrap();
    form.ui.set_selected_tab(group, 1).unwrap();
    form.update(SIZE, 1.0).unwrap();
    assert_eq!(
        form.ui.checked(form.find_name("First").unwrap()),
        Some(true)
    );
    assert_eq!(
        form.ui.checked(form.find_name("Second").unwrap()),
        Some(true)
    );
}

#[test]
fn invalid_manual_tab_selection_is_rejected_before_mount() {
    let mut definition = definition(
        "<StackPanel><Button Content='Before'/><TabbedGroup><GroupTab Header='First'/><GroupTab Header='Second' IsEnabled='False'/></TabbedGroup></StackPanel>",
    );
    let ElementKind::StackPanel(panel) = &mut definition.content.kind else {
        unreachable!()
    };
    let ElementKind::TabbedGroup(group) = &mut panel.children[1].kind else {
        unreachable!()
    };
    group.selected_index = 1;
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let before = ui.runtime.tree().preorder();
    let root = ui.root();
    assert!(MountedView::mount(&mut ui, root, &definition.content).is_err());
    assert_eq!(ui.runtime.tree().preorder(), before);
}

#[test]
fn horizontal_auto_tabbed_group_includes_page_child_margins() {
    let form = form(
        "<StackPanel Orientation='Horizontal' Spacing='0'><TabbedGroup x:Name='Tabs' Padding='16'><GroupTab Header='A'><TextBox x:Name='Editor' Width='300' Margin='20,0,20,0'/></GroupTab></TabbedGroup><Button x:Name='After' Width='40'/></StackPanel>",
    );
    near(bounds(&form, "Tabs").width, 372.0);
    near(bounds(&form, "After").x, 372.0);
    near(bounds(&form, "Editor").x, 36.0);
}

#[test]
fn auto_group_headers_measure_the_same_single_line_that_is_painted() {
    for tabbed in [false, true] {
        let make = |header: &str| {
            let group = if tabbed {
                format!(
                    "<TabbedGroup x:Name='Group' Padding='0'><GroupTab Header='{header}'/></TabbedGroup>"
                )
            } else {
                format!(
                    "<GroupBox x:Name='Group' Header='{header}' HeaderIcon='Edit' Padding='0'/>"
                )
            };
            form(&format!(
                "<StackPanel Orientation='Horizontal'>{group}</StackPanel>"
            ))
        };
        let newline = make("Long&#10;Caption");
        let single_line = make("Long Caption");
        near(
            bounds(&newline, "Group").width,
            bounds(&single_line, "Group").width,
        );
    }
}

#[test]
fn selected_index_fallback_is_numeric_and_rejects_fractional_values() {
    let form = form(
        "<TabbedGroup x:Name='Tabs' SelectedIndex='{Binding Missing, FallbackValue=1}'><GroupTab Header='A'/><GroupTab Header='B'/></TabbedGroup>",
    );
    assert_eq!(
        form.ui.selected_tab(form.find_name("Tabs").unwrap()),
        Some(1)
    );
    assert!(Form::new(definition("<TabbedGroup SelectedIndex='{Binding Missing, FallbackValue=0.5}'><GroupTab Header='A'/><GroupTab Header='B'/></TabbedGroup>"), false).is_err());
}

#[test]
fn tabbed_group_accessibility_has_selected_tabs_and_only_active_page() {
    let mut form = form(
        "<TabbedGroup x:Name='Tabs' AutomationName='Document sections'><GroupTab Header='First'><TextBox x:Name='First'/></GroupTab><GroupTab Header='Second'><TextBox x:Name='Second'/></GroupTab></TabbedGroup>",
    );
    let tree = form.accessibility(1.0);
    let tabs: Vec<_> = tree
        .nodes
        .iter()
        .filter(|(_, node)| node.role() == accesskit::Role::Tab)
        .collect();
    assert_eq!(tabs.len(), 2);
    assert_eq!(
        tabs.iter()
            .filter(|(_, node)| node.is_selected() == Some(true))
            .count(),
        1
    );
    let inactive = form.accessible.id(form.find_name("Second").unwrap());
    assert!(inactive.is_none_or(|id| tree.nodes.iter().all(|(node, _)| *node != id)));
}

#[test]
fn expander_accessibility_header_inherits_name_and_routes_expand_collapse() {
    let mut form = form(
        "<Expander x:Name='Group' Header='Short' AutomationName='Accessible section'><TextBox x:Name='Editor' Text='Retained'/></Expander>",
    );
    let group = form.find_name("Group").unwrap();
    let editor = form.find_name("Editor").unwrap();
    let header = form.ui.runtime.tree().children(group).unwrap()[0];
    let tree = form.accessibility(1.5);
    let group_id = form.accessible.id(group).unwrap();
    let header_id = form.accessible.id(header).unwrap();
    let editor_id = form.accessible.id(editor).unwrap();
    let group_node = &tree.nodes.iter().find(|(id, _)| *id == group_id).unwrap().1;
    let header_node = &tree
        .nodes
        .iter()
        .find(|(id, _)| *id == header_id)
        .unwrap()
        .1;
    assert_eq!(group_node.role(), accesskit::Role::Group);
    assert_eq!(group_node.label(), Some("Accessible section"));
    assert_eq!(header_node.role(), accesskit::Role::Button);
    assert_eq!(header_node.label(), Some("Accessible section"));
    assert_eq!(header_node.is_expanded(), Some(true));
    assert!(header_node.supports_action(accesskit::Action::Collapse));
    assert!(form.scene().texts.iter().any(|run| run.text == "Short"));
    assert!(
        !form
            .scene()
            .texts
            .iter()
            .any(|run| run.text == "Accessible section")
    );
    for expanded in [false, true] {
        form.event(PlatformEvent::Accessibility(accesskit::ActionRequest {
            action: if expanded {
                accesskit::Action::Expand
            } else {
                accesskit::Action::Collapse
            },
            target_tree: accesskit::TreeId::ROOT,
            target_node: header_id,
            data: None,
        }))
        .unwrap();
        form.update(SIZE, 1.0).unwrap();
        let tree = form.accessibility(1.5);
        let node = &tree
            .nodes
            .iter()
            .find(|(id, _)| *id == header_id)
            .unwrap()
            .1;
        assert_eq!(node.is_expanded(), Some(expanded));
        assert_eq!(tree.nodes.iter().any(|(id, _)| *id == editor_id), expanded);
        assert_eq!(form.find_name("Editor"), Some(editor));
        assert_eq!(form.ui.document(editor).unwrap().text(), "Retained");
    }
}

#[test]
fn group_headers_inherit_context_and_element_name_reads_the_group_property() {
    let model = Rc::new(RefCell::new(
        BindingContext::new(DataValue::object([(
            "Form",
            DataValue::object([("Header", "Initial".into()), ("Name", "Anna".into())]),
        )]))
        .unwrap(),
    ));
    let mut form = Form::new_with_model(definition(r#"<StackPanel><GroupBox x:Name="Group" DataContext="{Binding Form}" Header="{Binding Header}"><TextBox x:Name="Name" Text="{Binding Name}"/></GroupBox><TextBlock x:Name="Mirror" Text="{Binding Header, ElementName=Group}"/></StackPanel>"#), false, Some(model.clone())).unwrap();
    form.update(SIZE, 1.0).unwrap();
    let group = form.find_name("Group").unwrap();
    let editor = form.find_name("Name").unwrap();
    let mirror = form.find_name("Mirror").unwrap();
    assert_eq!(form.ui.group_header(group), Some("Initial"));
    assert_eq!(form.ui.label_text(mirror), Some("Initial"));
    assert_eq!(form.ui.document(editor).unwrap().text(), "Anna");
    model
        .borrow_mut()
        .set("Form.Header", "Changed".into())
        .unwrap();
    form.update(SIZE, 1.0).unwrap();
    assert_eq!(form.ui.group_header(group), Some("Changed"));
    assert_eq!(form.ui.label_text(mirror), Some("Changed"));
    form.ui.set_text(editor, "Maria").unwrap();
    form.ui.commit_edit(editor).unwrap();
    form.update(SIZE, 1.0).unwrap();
    assert_eq!(
        model.borrow().get("Form.Name").unwrap().as_str(),
        Some("Maria")
    );
}

#[test]
fn expansion_round_trips_and_preserves_uncommitted_editor_and_nested_state() {
    let model = Rc::new(RefCell::new(
        BindingContext::new(DataValue::object([
            ("Expanded", true.into()),
            ("Text", "Accepted".into()),
        ]))
        .unwrap(),
    ));
    let mut form = Form::new_with_model(definition(r#"<StackPanel Spacing="0"><Expander x:Name="Outer" Header="Outer" IsExpanded="{Binding Expanded}"><Expander x:Name="Inner" Header="Inner"><TextBox x:Name="Editor" Text="{Binding Text}" CommitOnLostFocus="False"/></Expander></Expander><CheckBox x:Name="Toggle" IsChecked="{Binding IsExpanded, ElementName=Outer}"/><Border x:Name="After" Height="20"><Grid/></Border></StackPanel>"#), false, Some(model.clone())).unwrap();
    form.update(SIZE, 1.0).unwrap();
    let outer = form.find_name("Outer").unwrap();
    let inner = form.find_name("Inner").unwrap();
    let editor = form.find_name("Editor").unwrap();
    let toggle = form.find_name("Toggle").unwrap();
    let nodes = form.ui.runtime.tree().preorder();
    let expanded_after = bounds(&form, "After").y;
    form.ui.set_text(editor, "Draft remains").unwrap();
    form.ui.set_expander_expanded(outer, false).unwrap();
    form.update(SIZE, 1.0).unwrap();
    assert_eq!(
        model.borrow().get("Expanded").unwrap(),
        &DataValue::Bool(false)
    );
    assert_eq!(form.ui.checked(toggle), Some(false));
    assert!(form.ui.runtime.bounds(editor).is_none());
    assert!(bounds(&form, "After").y < expanded_after);
    assert_eq!(form.ui.document(editor).unwrap().text(), "Draft remains");
    assert_eq!(form.ui.accepted_value(editor), Some("Accepted"));
    assert_eq!(form.ui.expander_expanded(inner), Some(true));
    model.borrow_mut().set("Expanded", true.into()).unwrap();
    form.update(SIZE, 1.0).unwrap();
    assert!(form.ui.runtime.bounds(editor).is_some());
    assert_eq!(form.ui.runtime.tree().preorder(), nodes);
    assert_eq!(form.ui.document(editor).unwrap().text(), "Draft remains");
    assert_eq!(form.ui.is_edit_dirty(editor), Some(true));
    assert_eq!(form.ui.checked(toggle), Some(true));
}

#[test]
fn is_expanded_binding_fallback_is_boolean_and_rejects_other_literals() {
    let mut form = form(
        r#"<Expander x:Name="Group" Header="Fallback" IsExpanded="{Binding Missing, FallbackValue=False}"><TextBox x:Name="Editor"/></Expander>"#,
    );
    let group = form.find_name("Group").unwrap();
    assert_eq!(form.ui.expander_expanded(group), Some(false));
    assert!(
        form.ui
            .runtime
            .bounds(form.find_name("Editor").unwrap())
            .is_none()
    );
    form.ui.set_expander_expanded(group, true).unwrap();
    form.update(SIZE, 1.0).unwrap();
    assert!(
        Form::new(
            definition(r#"<Expander IsExpanded="{Binding Missing, FallbackValue=not_a_bool}"/>"#),
            false
        )
        .is_err()
    );
}

#[test]
fn invalid_manual_group_fails_before_mounting_any_siblings() {
    for expander in [false, true] {
        for header in [false, true] {
            let tag = if expander { "Expander" } else { "GroupBox" };
            let mut definition = definition(&format!(
                "<StackPanel><Button Content='First'/><{tag}/></StackPanel>"
            ));
            let ElementKind::StackPanel(panel) = &mut definition.content.kind else {
                unreachable!()
            };
            let (caption, padding) = match &mut panel.children[1].kind {
                ElementKind::GroupBox(group) => (&mut group.header, &mut group.padding),
                ElementKind::Expander(group) => (&mut group.header, &mut group.padding),
                _ => unreachable!(),
            };
            if header {
                *caption = "a".repeat(4097);
            } else {
                padding[0] = f32::NAN;
            }
            let mut ui = Controls::new(Theme::FluentLight).unwrap();
            ui.update(SIZE).unwrap();
            let before = ui.runtime.tree().preorder();
            let root = ui.root();
            assert!(MountedView::mount(&mut ui, root, &definition.content).is_err());
            assert_eq!(ui.runtime.tree().preorder(), before);
        }
    }
}

#[test]
fn root_group_stretches_and_grid_child_fills_remaining_content_height() {
    let mut form = form(
        "<GroupBox x:Name='Group' Header='Stretch' Padding='10'><Grid x:Name='Body'><TextBox x:Name='Editor' VerticalAlignment='Bottom'/></Grid></GroupBox>",
    );
    for size in [SIZE, Size::new(600.0, 400.0)] {
        form.update(size, 1.0).unwrap();
        near(bounds(&form, "Group").height, size.height);
        near(bounds(&form, "Body").height, size.height - 48.0 - 20.0);
        near(
            bounds(&form, "Editor").y + bounds(&form, "Editor").height,
            size.height - 10.0,
        );
    }
}

#[test]
fn auto_expander_shrinks_to_header_but_explicit_height_is_retained() {
    for fixed in [false, true] {
        let height = if fixed { "Height='200'" } else { "" };
        let mut form = form(&format!(
            "<Expander x:Name='Group' Header='Details' Padding='10' {height}><Grid x:Name='Body'><TextBox Height='36'/></Grid></Expander>"
        ));
        let group = form.find_name("Group").unwrap();
        near(
            bounds(&form, "Group").height,
            if fixed { 200.0 } else { 104.0 },
        );
        if fixed {
            near(bounds(&form, "Body").height, 132.0);
        }
        form.ui.set_expander_expanded(group, false).unwrap();
        form.update(SIZE, 1.0).unwrap();
        near(
            bounds(&form, "Group").height,
            if fixed { 200.0 } else { 48.0 },
        );
    }
}

#[test]
fn horizontal_auto_group_measures_header_and_content_insets() {
    let form = form(
        "<StackPanel Orientation='Horizontal' Spacing='0'><GroupBox x:Name='Group' Header='A' Padding='10'><TextBox x:Name='Editor' Width='240' Margin='3,0,7,0'/></GroupBox><Button x:Name='After' Width='50'/></StackPanel>",
    );
    near(bounds(&form, "Group").width, 270.0);
    near(bounds(&form, "Editor").x, 13.0);
    near(bounds(&form, "After").x, 270.0);
}

#[test]
fn collapsing_scroll_content_reclamps_extent_without_recreating_or_idle_layout() {
    let mut form = form(
        "<ScrollViewer x:Name='Scroll' Height='100'><Expander x:Name='Group' Header='Long section'><Border x:Name='Content' Height='400'><Grid/></Border></Expander></ScrollViewer>",
    );
    let scroll = form.find_name("Scroll").unwrap();
    let group = form.find_name("Group").unwrap();
    let nodes = form.ui.runtime.tree().preorder();
    form.ui
        .scroll_to_point(scroll, Point::new(0.0, 999.0))
        .unwrap();
    form.update(SIZE, 1.0).unwrap();
    assert!(form.ui.scroll_offset(scroll).unwrap().y > 300.0);
    form.ui.set_expander_expanded(group, false).unwrap();
    form.update(SIZE, 1.0).unwrap();
    assert_eq!(form.ui.scroll_offset(scroll), Some(Point::ZERO));
    assert_eq!(form.ui.runtime.tree().preorder(), nodes);
    form.update(SIZE, 1.0).unwrap();
    let before = form.ui.stats();
    form.update(SIZE, 1.0).unwrap();
    assert_eq!(form.ui.stats().layout_passes, before.layout_passes);
}
