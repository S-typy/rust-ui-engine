use crate::{form::Form, forms_model, generated, generated_status, view::MountedView};
use rust_desktop_ui_controls::{BindingContext, Controls, DataValue, Role, SemanticAction, Theme};
use rust_desktop_ui_core::{Rect, Size};
use rust_desktop_ui_platform_winit::DesktopApp;
use rust_desktop_ui_xaml::{ElementKind, WindowDefinition};
use std::{cell::RefCell, rc::Rc};

fn definition(content: &str) -> WindowDefinition {
    rust_desktop_ui_xaml::parse(&format!(
        "<Window xmlns='urn:rust-ui-engine:ui' xmlns:x='http://schemas.microsoft.com/winfx/2006/xaml' Width='600' Height='300' Padding='0'>{content}</Window>"
    )).unwrap()
}

fn bounds(form: &Form, name: &str) -> Rect {
    form.ui
        .runtime
        .bounds(form.find_name(name).unwrap())
        .unwrap()
}

fn near(actual: f32, expected: f32) {
    assert!((actual - expected).abs() < 0.2, "{actual} != {expected}");
}

#[test]
fn main_window_status_stays_at_bottom_across_resize_and_ribbon_minimization() {
    let mut form = Form::new(generated::window_definition(), false).unwrap();
    for size in [
        Size::new(1040.0, 720.0),
        Size::new(640.0, 600.0),
        Size::new(300.0, 400.0),
    ] {
        for minimized in [false, true] {
            form.view
                .ribbon
                .as_mut()
                .unwrap()
                .ribbon
                .set_minimized(minimized);
            form.update(size, 1.0).unwrap();
            let bar = bounds(&form, "DocumentStatusBar");
            near(bar.y + bar.height, size.height);
            near(bar.width, size.width);
            assert_eq!(
                form.view.roles[&form.find_name("DocumentStatusBar").unwrap()],
                Role::StatusBar
            );
        }
    }
}

#[test]
fn ordinary_status_anchors_right_items_and_clips_long_left_text() {
    let mut form = Form::new(definition("<Grid><Grid.RowDefinitions><RowDefinition Height='*'/><RowDefinition Height='Auto'/></Grid.RowDefinitions><TextBox x:Name='Editor'/><StatusBar x:Name='Bar' Grid.Row='1'><StatusBarItem x:Name='Left'><TextBlock Text='A very long line of text that must never obscure right commands'/></StatusBarItem><StatusBarItem x:Name='Right' Placement='Right' Width='90'><Button Content='Action' Height='24'/></StatusBarItem></StatusBar></Grid>"), false).unwrap();
    for width in [600.0, 240.0, 120.0] {
        form.update(Size::new(width, 300.0), 1.0).unwrap();
        let bar = bounds(&form, "Bar");
        let left = bounds(&form, "Left");
        let right = bounds(&form, "Right");
        near(bar.y + bar.height, 300.0);
        assert!(left.x + left.width <= right.x + 0.2);
        assert!(right.x + right.width <= width + 0.2);
        assert!(right.x >= 0.0);
    }
}

#[test]
fn compiled_status_gallery_round_trips_zoom_and_progress_without_editor_rebuild() {
    let model = forms_model::model().unwrap();
    let mut form = Form::new_with_model(
        generated_status::window_definition(),
        false,
        Some(model.clone()),
    )
    .unwrap();
    let size = Size::new(1060.0, 760.0);
    form.update(size, 1.0).unwrap();
    let zoom = form.find_name("Zoom").unwrap();
    let progress = form.find_name("WorkProgress").unwrap();
    let editor = form.find_name("StatusMessageEditor").unwrap();
    assert_eq!(form.ui.zoom_value(zoom), Some(100.0));
    assert_eq!(form.ui.progress_value(progress), Some(65.0));
    form.ui.set_text(editor, "Draft message").unwrap();
    form.ui.set_zoom_value(zoom, 150.0).unwrap();
    form.view.sync_bindings(&mut form.ui);
    assert_eq!(
        model.borrow().get("Dashboard.Zoom").unwrap(),
        &DataValue::Number(150.0)
    );
    model
        .borrow_mut()
        .set("Dashboard.Progress", 80.0.into())
        .unwrap();
    form.update(size, 1.0).unwrap();
    assert_eq!(form.ui.progress_value(progress), Some(80.0));
    assert_eq!(form.ui.document(editor).unwrap().text(), "Draft message");
    assert_eq!(form.find_name("StatusMessageEditor"), Some(editor));
    model
        .borrow_mut()
        .set("Dashboard.Zoom", 1000.0.into())
        .unwrap();
    form.view.sync_bindings(&mut form.ui);
    assert_eq!(form.ui.zoom_value(zoom), Some(150.0));
    assert!(
        !form
            .view
            .bindings
            .as_ref()
            .unwrap()
            .diagnostics()
            .is_empty()
    );
}

#[test]
fn status_toggle_uses_same_registry_as_ribbon_and_body() {
    let mut form = Form::new(generated::window_definition(), false).unwrap();
    form.update(Size::new(1040.0, 720.0), 1.0).unwrap();
    let status = form.find_name("StatusPin").unwrap();
    let body = form.find_name("Pin").unwrap();
    form.ui
        .perform_action(status, SemanticAction::Activate, None)
        .unwrap();
    assert_eq!(form.ui.commands().checked(&"Pin".into()), Some(true));
    assert_eq!(form.ui.checked(body), Some(true));
    form.ui
        .perform_action(body, SemanticAction::Activate, None)
        .unwrap();
    assert_eq!(form.ui.checked(status), Some(false));
}

#[test]
fn document_zoom_changes_font_without_replacing_draft_or_accepted_value() {
    let mut form = Form::new(generated::window_definition(), false).unwrap();
    let editor = form.find_name("Message").unwrap();
    let zoom = form.find_name("DocumentZoom").unwrap();
    let accepted = form.ui.accepted_value(editor).unwrap().to_owned();
    form.ui.set_text(editor, "Uncommitted text").unwrap();
    form.ui.set_zoom_value(zoom, 150.0).unwrap();
    form.update(Size::new(1040.0, 720.0), 1.0).unwrap();
    assert_eq!(
        form.ui.edit_box_style(editor).unwrap().font_size,
        Some(21.0)
    );
    assert_eq!(form.ui.document(editor).unwrap().text(), "Uncommitted text");
    assert_eq!(form.ui.accepted_value(editor), Some(accepted.as_str()));
}

#[test]
fn numeric_fallback_and_element_bindings_use_number_values() {
    let model = Rc::new(RefCell::new(
        BindingContext::new(DataValue::object([] as [(&str, DataValue); 0])).unwrap(),
    ));
    let mut form = Form::new_with_model(definition("<StatusBar><StatusBarItem Width='220'><ZoomControl x:Name='Zoom' Value='{Binding Missing, FallbackValue=150}'/></StatusBarItem><StatusBarItem Placement='Right' Width='120'><ProgressBar x:Name='Progress' Maximum='400' Value='{Binding Value, ElementName=Zoom}'/></StatusBarItem></StatusBar>"), false, Some(model)).unwrap();
    form.update(Size::new(600.0, 300.0), 1.0).unwrap();
    assert_eq!(
        form.ui.zoom_value(form.find_name("Zoom").unwrap()),
        Some(150.0)
    );
    assert_eq!(
        form.ui.progress_value(form.find_name("Progress").unwrap()),
        Some(150.0)
    );
}

#[test]
fn invalid_manual_status_tree_is_rejected_before_creating_widgets() {
    let mut definition = definition(
        "<StatusBar><StatusBarItem><TextBlock Text='Ready'/></StatusBarItem></StatusBar>",
    );
    let ElementKind::StatusBar(bar) = &mut definition.content.kind else {
        panic!()
    };
    definition.content = bar.items.remove(0);
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let before = ui.runtime.tree().preorder();
    let root = ui.root();
    assert!(MountedView::mount(&mut ui, root, &definition.content).is_err());
    assert_eq!(ui.runtime.tree().preorder(), before);
}

#[test]
fn collapsed_status_item_releases_its_space_and_keeps_children() {
    let model = Rc::new(RefCell::new(
        BindingContext::new(DataValue::object([("Shown", "Visible".into())])).unwrap(),
    ));
    let mut form = Form::new_with_model(definition("<StatusBar><StatusBarItem x:Name='Left'><TextBlock Text='Ready'/></StatusBarItem><StatusBarItem x:Name='Right' Placement='Right' Width='120' Visibility='{Binding Shown}'><ZoomControl x:Name='Zoom'/></StatusBarItem></StatusBar>"), false, Some(model.clone())).unwrap();
    form.update(Size::new(600.0, 300.0), 1.0).unwrap();
    let left_width = bounds(&form, "Left").width;
    let zoom = form.find_name("Zoom").unwrap();
    model.borrow_mut().set("Shown", "Collapsed".into()).unwrap();
    form.update(Size::new(600.0, 300.0), 1.0).unwrap();
    assert!(bounds(&form, "Left").width > left_width);
    assert_eq!(form.find_name("Zoom"), Some(zoom));
    assert!(!form.ui.semantics().iter().any(|node| node.id == zoom));
}

#[test]
fn status_item_bound_width_matches_literal_width_and_updates_without_growing() {
    let model = Rc::new(RefCell::new(
        BindingContext::new(DataValue::object([("ItemWidth", 100.0.into())])).unwrap(),
    ));
    let mut form = Form::new_with_model(definition("<StatusBar><StatusBarItem x:Name='Fixed' Width='100'><TextBlock Text='Fixed'/></StatusBarItem><StatusBarItem x:Name='Bound' Width='{Binding ItemWidth}'><TextBlock Text='Bound'/></StatusBarItem><StatusBarItem><TextBlock Text='Stretch'/></StatusBarItem></StatusBar>"), false, Some(model.clone())).unwrap();
    form.update(Size::new(600.0, 300.0), 1.0).unwrap();
    near(bounds(&form, "Fixed").width, 100.0);
    near(bounds(&form, "Bound").width, 100.0);
    model.borrow_mut().set("ItemWidth", 150.0.into()).unwrap();
    form.update(Size::new(600.0, 300.0), 1.0).unwrap();
    near(bounds(&form, "Bound").width, 150.0);
}
