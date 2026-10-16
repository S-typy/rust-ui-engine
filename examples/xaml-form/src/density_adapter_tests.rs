use crate::form::Form;
use rust_desktop_ui_controls::{BindingContext, DataValue, ImageFormat, ImageSource};
use rust_desktop_ui_core::{Length, Mutation, Size};
use rust_desktop_ui_platform_winit::DesktopApp;
use rust_desktop_ui_xaml::ThemeDensity;
use std::{cell::RefCell, rc::Rc};

fn definition(content: &str) -> rust_desktop_ui_xaml::WindowDefinition {
    rust_desktop_ui_xaml::parse(&format!("<Window xmlns='urn:rust-ui-engine:ui' xmlns:x='http://schemas.microsoft.com/winfx/2006/xaml' Padding='0' Width='800' Height='1000'>{content}</Window>")).unwrap()
}
fn frame(form: &mut Form) {
    form.update(Size::new(800.0, 1000.0), 1.0).unwrap();
}
fn height(form: &Form, name: &str) -> f32 {
    form.ui
        .runtime
        .bounds(form.find_name(name).unwrap())
        .unwrap()
        .height
}

#[test]
fn default_xaml_control_heights_follow_initial_and_live_density() {
    let content = "<StackPanel><TextBox x:Name='Message' Text='Draft'/><Button x:Name='B' Content='Button'/><ToggleButton x:Name='T' Content='Toggle'/><CheckBox x:Name='C' Content='Check'/><RadioButton x:Name='R' Content='Radio'/><ComboBox x:Name='Combo'/><ButtonEdit x:Name='Buttons'><ButtonEdit.Buttons><EditorButton Command='Clear' Content='Clear'/></ButtonEdit.Buttons></ButtonEdit><SpinBox x:Name='Spin' Value='3'/><DateEdit x:Name='Date' Value='2026-10-16'/><TimeEdit x:Name='Time' Value='09:30'/></StackPanel>";
    for (density, expected) in [
        (ThemeDensity::Comfortable, 36.0),
        (ThemeDensity::Compact, 28.0),
        (ThemeDensity::Touch, 44.0),
    ] {
        let mut definition = definition(content);
        definition.density = density;
        let mut form = Form::new(definition, false).unwrap();
        frame(&mut form);
        for name in [
            "Message", "B", "T", "Combo", "Buttons", "Spin", "Date", "Time",
        ] {
            assert_eq!(height(&form, name), expected, "{name}");
        }
        for name in ["C", "R"] {
            assert_eq!(height(&form, name), expected - 4.0, "{name}");
        }
        let editor = form.find_name("Message").unwrap();
        form.ui.set_text(editor, "Uncommitted draft").unwrap();
        for (command, expected) in [("Touch", 44.0), ("Compact", 28.0), ("Comfortable", 36.0)] {
            assert!(form.ui.commands_mut().activate(&command.into()));
            frame(&mut form);
            for name in [
                "Message", "B", "T", "Combo", "Buttons", "Spin", "Date", "Time",
            ] {
                assert_eq!(height(&form, name), expected, "{name}: {command}");
            }
            assert_eq!(
                form.ui.document(editor).unwrap().text(),
                "Uncommitted draft"
            );
        }
    }
}

#[test]
fn explicit_bound_and_runtime_overridden_heights_survive_density_switch() {
    let context = Rc::new(RefCell::new(
        BindingContext::new(DataValue::object([("Height", 58.0.into())])).unwrap(),
    ));
    let mut form = Form::new_with_model(definition("<StackPanel><TextBox x:Name='Message'/><Button x:Name='Explicit' Content='Explicit' Height='62'/><Button x:Name='Bound' Content='Bound' Height='{Binding Height}'/><Button x:Name='Runtime' Content='Override'/></StackPanel>"), false, Some(context)).unwrap();
    frame(&mut form);
    let runtime = form.find_name("Runtime").unwrap();
    let mut style = form
        .ui
        .runtime
        .tree()
        .node(runtime)
        .unwrap()
        .props
        .style
        .clone();
    style.height = Length::Px(73.0);
    form.ui
        .runtime
        .apply(Mutation::SetStyle {
            node: runtime,
            style,
        })
        .unwrap();
    for command in ["Touch", "Compact", "Comfortable"] {
        form.ui.commands_mut().activate(&command.into());
        frame(&mut form);
        assert_eq!(height(&form, "Explicit"), 62.0);
        assert_eq!(height(&form, "Bound"), 58.0);
        assert_eq!(height(&form, "Runtime"), 73.0);
    }
}

#[test]
fn xaml_image_auto_axes_follow_late_resource_and_preserve_explicit_height() {
    let mut form = Form::new(definition("<StackPanel><Image x:Name='Natural' Source='Late'/><Image x:Name='FixedHeight' Source='Late' Height='40'/><TextBox x:Name='Message'/></StackPanel>"), false).unwrap();
    frame(&mut form);
    let natural = form.find_name("Natural").unwrap();
    let fixed = form.find_name("FixedHeight").unwrap();
    assert_eq!(form.ui.runtime.bounds(natural).unwrap().width, 32.0);
    let source = ImageSource::from_bytes(
        b"<svg width='20' height='10'><rect width='20' height='10' fill='red'/></svg>".to_vec(),
        ImageFormat::Svg,
    )
    .unwrap();
    form.ui.register_image_source("Late", source).unwrap();
    frame(&mut form);
    let rect = form.ui.runtime.bounds(natural).unwrap();
    assert_eq!((rect.width, rect.height), (20.0, 10.0));
    let rect = form.ui.runtime.bounds(fixed).unwrap();
    assert_eq!((rect.width, rect.height), (80.0, 40.0));
}

#[test]
fn image_dimension_bindings_own_axes_even_when_initial_values_equal_intrinsic_size() {
    let context = Rc::new(RefCell::new(
        BindingContext::new(DataValue::object([
            ("Width", 64.0.into()),
            ("Height", 64.0.into()),
        ]))
        .unwrap(),
    ));
    let mut form = Form::new_with_model(definition("<StackPanel><Image x:Name='Image' Source='Compass' Width='{Binding Width}' Height='{Binding Height}'/><TextBox x:Name='Message'/></StackPanel>"), false, Some(context)).unwrap();
    frame(&mut form);
    let id = form.find_name("Image").unwrap();
    let source = ImageSource::from_bytes(
        b"<svg width='20' height='10'><rect width='20' height='10' fill='red'/></svg>".to_vec(),
        ImageFormat::Svg,
    )
    .unwrap();
    form.ui.register_image_source("Compass", source).unwrap();
    frame(&mut form);
    let rect = form.ui.runtime.bounds(id).unwrap();
    assert_eq!((rect.width, rect.height), (64.0, 64.0));
}

#[test]
fn xaml_bar_density_changes_keep_buttons_and_focus_ring_inside_ancestor_clips() {
    use rust_desktop_ui_core::Rect;
    let content = r#"<StackPanel Spacing="12">
        <MenuBar x:Name="Menu"><DropDownButton x:Name="Language" Content="Язык"><MenuItem Header="Светлая" Command="Light"/></DropDownButton></MenuBar>
        <ToolBar x:Name="Tools" Key="document"><Button x:Name="ConfirmButton" Content="Подтвердить" Command="Confirm" Width="150"/><TextBox x:Name="Message" Text="Draft" Width="200"/></ToolBar>
    </StackPanel>"#;
    for density in [
        ThemeDensity::Comfortable,
        ThemeDensity::Touch,
        ThemeDensity::Compact,
    ] {
        let mut definition = definition(content);
        definition.density = density;
        let mut form = Form::new(definition, false).unwrap();
        frame(&mut form);
        let editor = form.find_name("Message").unwrap();
        let menu = form.find_name("Language").unwrap();
        form.ui.set_text(editor, "Retained draft").unwrap();
        form.ui.runtime.apply(Mutation::Focus(Some(menu))).unwrap();
        for command in ["Touch", "Compact", "Comfortable", "Touch"] {
            assert!(form.ui.commands_mut().activate(&command.into()));
            frame(&mut form);
            let expected = form.ui.theme().tokens().control_height;
            assert_eq!(height(&form, "Menu"), expected, "{command}");
            assert_eq!(height(&form, "Tools"), expected + 8.0, "{command}");
            for name in ["Language", "ConfirmButton", "Message"] {
                let id = form.find_name(name).unwrap();
                let bounds = form.ui.runtime.bounds(id).unwrap();
                let clip = form.ui.runtime.clip(id).unwrap();
                assert_eq!(bounds.intersection(clip), Some(bounds), "{name}: {command}");
                assert_eq!(bounds.height, expected, "{name}: {command}");
            }
            let bounds = form.ui.runtime.bounds(menu).unwrap();
            let focus = Rect::new(
                bounds.x + 2.0,
                bounds.y + 2.0,
                bounds.width - 4.0,
                bounds.height - 4.0,
            );
            let ring = form
                .ui
                .scene()
                .rounded_rectangles
                .iter()
                .find(|shape| shape.bounds == focus && shape.stroke_width == Some(2.0))
                .expect("focused menu header must paint its complete focus ring");
            assert_eq!(ring.bounds.intersection(ring.clip), Some(ring.bounds));
            assert_eq!(form.ui.runtime.focused(), Some(menu));
            assert_eq!(form.find_name("Message"), Some(editor));
            assert_eq!(form.ui.document(editor).unwrap().text(), "Retained draft");
        }
    }
}

#[test]
fn xaml_bars_preserve_explicit_bound_and_runtime_heights_when_density_changes() {
    let context = Rc::new(RefCell::new(
        BindingContext::new(DataValue::object([("BarHeight", 58.0.into())])).unwrap(),
    ));
    let content = r#"<StackPanel>
        <MenuBar x:Name="ExplicitMenu" Height="20"><DropDownButton x:Name="ClippedHeader" Content="Меню"><MenuItem Header="Светлая" Command="Light"/></DropDownButton></MenuBar>
        <MenuBar x:Name="RuntimeMenu"><DropDownButton Content="Меню"><MenuItem Header="Светлая" Command="Light"/></DropDownButton></MenuBar>
        <MenuBar x:Name="BoundMenu" Height="{Binding BarHeight}"><DropDownButton Content="Меню"><MenuItem Header="Светлая" Command="Light"/></DropDownButton></MenuBar>
        <ToolBar x:Name="ExplicitTools" Key="explicit" Height="62"><Button Content="Светлая" Command="Light" Width="100"/></ToolBar>
        <ToolBar x:Name="RuntimeTools" Key="runtime"><Button Content="Светлая" Command="Light" Width="100"/></ToolBar>
        <ToolBar x:Name="BoundTools" Key="bound" Height="{Binding BarHeight}"><Button Content="Светлая" Command="Light" Width="100"/></ToolBar>
        <TextBox x:Name="Message"/>
    </StackPanel>"#;
    let mut form = Form::new_with_model(definition(content), false, Some(context.clone())).unwrap();
    frame(&mut form);
    for name in ["RuntimeMenu", "RuntimeTools"] {
        let node = form.find_name(name).unwrap();
        let mut style = form
            .ui
            .runtime
            .tree()
            .node(node)
            .unwrap()
            .props
            .style
            .clone();
        style.height = Length::Px(73.0);
        form.ui
            .runtime
            .apply(Mutation::SetStyle { node, style })
            .unwrap();
    }
    for command in ["Touch", "Compact", "Comfortable"] {
        form.ui.commands_mut().activate(&command.into());
        frame(&mut form);
        for (name, expected) in [
            ("ExplicitMenu", 20.0),
            ("ExplicitTools", 62.0),
            ("BoundMenu", 58.0),
            ("BoundTools", 58.0),
            ("RuntimeMenu", 73.0),
            ("RuntimeTools", 73.0),
        ] {
            assert_eq!(height(&form, name), expected, "{name}: {command}");
        }
        let header = form.find_name("ClippedHeader").unwrap();
        let bounds = form.ui.runtime.bounds(header).unwrap();
        let clip = form.ui.runtime.clip(header).unwrap();
        assert!(bounds.intersection(clip).unwrap().height < bounds.height);
    }
    context.borrow_mut().set("BarHeight", 68.0.into()).unwrap();
    frame(&mut form);
    assert_eq!(height(&form, "BoundMenu"), 68.0);
    assert_eq!(height(&form, "BoundTools"), 68.0);
}
