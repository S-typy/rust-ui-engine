use crate::form::Form;
use rust_desktop_ui_core::{Length, Mutation, Point, Rect, Size};
use rust_desktop_ui_platform_winit::DesktopApp;

fn form(content: &str, size: Size) -> Form {
    let definition = rust_desktop_ui_xaml::parse(&format!(
        "<Window xmlns='urn:rust-ui-engine:ui' xmlns:x='http://schemas.microsoft.com/winfx/2006/xaml' Width='{}' Height='{}' Padding='0'>{content}</Window>",
        size.width, size.height,
    )).unwrap();
    let mut form = Form::new(definition, false).unwrap();
    form.update(size, 1.0).unwrap();
    form
}

fn bounds(form: &Form, name: &str) -> Rect {
    form.ui
        .runtime
        .bounds(form.find_name(name).unwrap())
        .unwrap()
}

fn assert_rect(actual: Rect, expected: Rect) {
    for (actual, expected) in [
        (actual.x, expected.x),
        (actual.y, expected.y),
        (actual.width, expected.width),
        (actual.height, expected.height),
    ] {
        assert!(
            (actual - expected).abs() < 0.01,
            "actual={actual} expected={expected}"
        );
    }
}

#[test]
fn horizontal_sliders_stretch_with_margins_and_resize_without_changing_explicit_widths() {
    let mut form = form(
        r#"<StackPanel>
        <Slider x:Name="Stretch" Margin="10,0,20,0"/>
        <RangeSlider x:Name="Range" Margin="10,0,20,0"/>
        <Slider x:Name="Fixed" Width="120"/>
        <Slider x:Name="Left" HorizontalAlignment="Left"/>
        <Slider x:Name="Vertical" Orientation="Vertical"/>
    </StackPanel>"#,
        Size::new(600.0, 600.0),
    );
    for width in [600.0, 360.0] {
        form.update(Size::new(width, 600.0), 1.0).unwrap();
        for name in ["Stretch", "Range"] {
            let rect = bounds(&form, name);
            assert!(
                (rect.width - (width - 30.0)).abs() < 0.01,
                "{name}: {rect:?}"
            );
            assert_eq!(rect.x, 10.0);
        }
        assert_eq!(bounds(&form, "Fixed").width, 120.0);
        assert_eq!(bounds(&form, "Left").width, 160.0);
        assert_eq!(
            bounds(&form, "Vertical").width,
            form.ui.theme().tokens().control_height
        );
    }
}

#[test]
fn border_padding_and_stroke_reserve_space_and_paint_rounded_surface() {
    let size = Size::new(400.0, 300.0);
    let form = form(
        "<Border x:Name='Frame' Padding='5' BorderThickness='2' CornerRadius='10' Background='#FF112233' BorderBrush='#FF778899'><Grid x:Name='Content'><TextBox x:Name='Editor' Width='80' HorizontalAlignment='Center' VerticalAlignment='Bottom'/></Grid></Border>",
        size,
    );
    assert_rect(bounds(&form, "Frame"), Rect::new(0.0, 0.0, 400.0, 300.0));
    assert_rect(bounds(&form, "Content"), Rect::new(7.0, 7.0, 386.0, 286.0));
    assert_rect(bounds(&form, "Editor"), Rect::new(160.0, 257.0, 80.0, 36.0));
    let rounded = &form.ui.scene().rounded_rectangles;
    assert!(
        rounded
            .iter()
            .any(|r| r.bounds == Rect::new(0.0, 0.0, 400.0, 300.0)
                && r.radius == 10.0
                && r.stroke_width == Some(2.0))
    );
    assert!(
        rounded
            .iter()
            .any(|r| r.bounds == Rect::new(0.0, 0.0, 400.0, 300.0)
                && r.radius == 10.0
                && r.stroke_width.is_none())
    );
}

#[test]
fn dock_panel_reserves_all_four_edges_in_order_and_last_child_fills() {
    let size = Size::new(400.0, 300.0);
    let form = form(
        "<DockPanel><Border x:Name='Left' DockPanel.Dock='Left' Width='40'><Grid/></Border><Border x:Name='Right' DockPanel.Dock='Right' Width='30'><Grid/></Border><Border x:Name='Top' DockPanel.Dock='Top' Height='20'><Grid/></Border><Border x:Name='Bottom' DockPanel.Dock='Bottom' Height='25'><Grid/></Border><Border x:Name='Fill' DockPanel.Dock='Bottom'><Grid/></Border></DockPanel>",
        size,
    );
    assert_rect(bounds(&form, "Left"), Rect::new(0.0, 0.0, 40.0, 300.0));
    assert_rect(bounds(&form, "Right"), Rect::new(370.0, 0.0, 30.0, 300.0));
    assert_rect(bounds(&form, "Top"), Rect::new(40.0, 0.0, 330.0, 20.0));
    assert_rect(bounds(&form, "Bottom"), Rect::new(40.0, 275.0, 330.0, 25.0));
    assert_rect(bounds(&form, "Fill"), Rect::new(40.0, 20.0, 330.0, 255.0));
}

#[test]
fn dock_panel_last_child_fill_false_preserves_final_edge_size() {
    let form = form(
        "<DockPanel LastChildFill='False'><Border DockPanel.Dock='Top' Height='20'><Grid/></Border><Border x:Name='Last' DockPanel.Dock='Right' Width='30'><Grid/></Border></DockPanel>",
        Size::new(400.0, 300.0),
    );
    assert_rect(bounds(&form, "Last"), Rect::new(370.0, 20.0, 30.0, 280.0));
}

#[test]
fn grid_spans_margins_and_child_alignment_track_resized_cells() {
    let mut form = form(
        "<Grid Spacing='10'><Grid.RowDefinitions><RowDefinition Height='40'/><RowDefinition Height='*'/></Grid.RowDefinitions><Grid.ColumnDefinitions><ColumnDefinition Width='80'/><ColumnDefinition Width='*'/></Grid.ColumnDefinitions><Border x:Name='Header' Grid.ColumnSpan='2' Margin='5'><Grid/></Border><TextBox x:Name='Editor' Grid.Row='1' Grid.Column='1' Width='100' MaxHeight='30' HorizontalAlignment='Right' VerticalAlignment='Bottom'/></Grid>",
        Size::new(400.0, 300.0),
    );
    assert_rect(bounds(&form, "Header"), Rect::new(5.0, 5.0, 390.0, 30.0));
    assert_rect(
        bounds(&form, "Editor"),
        Rect::new(300.0, 270.0, 100.0, 30.0),
    );
    form.update(Size::new(600.0, 400.0), 1.0).unwrap();
    assert_rect(bounds(&form, "Header"), Rect::new(5.0, 5.0, 590.0, 30.0));
    assert_rect(
        bounds(&form, "Editor"),
        Rect::new(500.0, 370.0, 100.0, 30.0),
    );
}

#[test]
fn scroll_viewer_clips_two_axes_and_reclamps_on_viewport_resize() {
    let mut form = form(
        "<ScrollViewer x:Name='Scroll' Width='200' Height='100' HorizontalScrollBarVisibility='Auto' VerticalScrollBarVisibility='Auto'><Border x:Name='Content' Width='500' Height='400' Background='#FFEEEEEE'><Grid/></Border></ScrollViewer>",
        Size::new(400.0, 300.0),
    );
    let scroll = form.find_name("Scroll").unwrap();
    let content = form.find_name("Content").unwrap();
    form.ui
        .scroll_to_point(scroll, Point::new(999.0, 999.0))
        .unwrap();
    form.update(Size::new(400.0, 300.0), 1.0).unwrap();
    assert_eq!(
        form.ui.scroll_offset(scroll),
        Some(Point::new(300.0, 300.0))
    );
    assert_rect(
        bounds(&form, "Content"),
        Rect::new(-300.0, -300.0, 500.0, 400.0),
    );
    assert_rect(
        form.ui.runtime.clip(content).unwrap(),
        Rect::new(0.0, 0.0, 200.0, 100.0),
    );
    let mut style = form
        .ui
        .runtime
        .tree()
        .node(scroll)
        .unwrap()
        .props
        .style
        .clone();
    style.width = Length::Px(300.0);
    style.height = Length::Px(150.0);
    form.ui
        .runtime
        .apply(Mutation::SetStyle {
            node: scroll,
            style,
        })
        .unwrap();
    form.update(Size::new(400.0, 300.0), 1.0).unwrap();
    assert_eq!(
        form.ui.scroll_offset(scroll),
        Some(Point::new(200.0, 250.0))
    );
    assert_rect(
        form.ui.runtime.clip(content).unwrap(),
        Rect::new(0.0, 0.0, 300.0, 150.0),
    );
}

#[test]
fn wrap_panel_reflows_real_xaml_children_at_the_available_width() {
    let mut form = form(
        "<Grid><WrapPanel x:Name='Wrap' Orientation='Horizontal' Spacing='5'><Button x:Name='A' Width='80'/><Button x:Name='B' Width='80'/><Button x:Name='C' Width='80'/></WrapPanel></Grid>",
        Size::new(200.0, 120.0),
    );
    assert_rect(bounds(&form, "C"), Rect::new(0.0, 41.0, 80.0, 36.0));
    form.update(Size::new(300.0, 120.0), 1.0).unwrap();
    assert_rect(bounds(&form, "C"), Rect::new(170.0, 0.0, 80.0, 36.0));
}

#[test]
fn idle_form_with_scroll_and_border_does_not_relayout_or_repaint() {
    let size = Size::new(300.0, 200.0);
    let mut form = form(
        "<ScrollViewer><Border Background='#FFEEDDCC' BorderBrush='#FF123456' BorderThickness='1' Height='400'><Grid/></Border></ScrollViewer>",
        size,
    );
    // The first settled frame applies measured content extents.
    form.update(size, 1.0).unwrap();
    let before = form.ui.stats();
    for _ in 0..3 {
        form.update(size, 1.0).unwrap();
    }
    let after = form.ui.stats();
    assert_eq!(before.layout_passes, after.layout_passes);
    assert_eq!(before.paint_passes, after.paint_passes);
}

#[test]
fn hidden_retains_space_but_collapsed_removes_and_restores_layout_and_identity() {
    let size = Size::new(300.0, 200.0);
    let mut form = form(
        "<StackPanel Spacing='0'><Border x:Name='First' Height='30' Visibility='Hidden'><Grid/></Border><Border x:Name='Collapsed' Height='40' Visibility='Collapsed'><Grid/></Border><Border x:Name='Last' Height='50'><Grid/></Border></StackPanel>",
        size,
    );
    let first = form.find_name("First").unwrap();
    let collapsed = form.find_name("Collapsed").unwrap();
    assert!(form.ui.runtime.bounds(first).is_none());
    assert!(form.ui.runtime.bounds(collapsed).is_none());
    assert_rect(bounds(&form, "Last"), Rect::new(0.0, 30.0, 300.0, 50.0));
    let mut props = form
        .ui
        .runtime
        .tree()
        .node(collapsed)
        .unwrap()
        .props
        .clone();
    props.visible = true;
    props.collapsed = false;
    form.ui
        .runtime
        .apply(Mutation::SetProps {
            node: collapsed,
            props,
        })
        .unwrap();
    form.update(size, 1.0).unwrap();
    assert_eq!(form.find_name("Collapsed"), Some(collapsed));
    assert_rect(
        bounds(&form, "Collapsed"),
        Rect::new(0.0, 30.0, 300.0, 40.0),
    );
    assert_rect(bounds(&form, "Last"), Rect::new(0.0, 70.0, 300.0, 50.0));
}

#[test]
fn scroll_auto_extent_includes_wide_descendants_and_hidden_but_not_collapsed_content() {
    let size = Size::new(400.0, 300.0);
    let mut form = form(
        "<ScrollViewer x:Name='Scroll' Width='200' Height='100'><StackPanel><Border x:Name='Wide' Width='500' Height='400' Visibility='Hidden'><Grid/></Border></StackPanel></ScrollViewer>",
        size,
    );
    let scroll = form.find_name("Scroll").unwrap();
    let wide = form.find_name("Wide").unwrap();
    form.ui
        .scroll_to_point(scroll, Point::new(999.0, 999.0))
        .unwrap();
    form.update(size, 1.0).unwrap();
    assert_eq!(
        form.ui.scroll_offset(scroll),
        Some(Point::new(300.0, 300.0))
    );
    let mut props = form.ui.runtime.tree().node(wide).unwrap().props.clone();
    props.collapsed = true;
    form.ui
        .runtime
        .apply(Mutation::SetProps { node: wide, props })
        .unwrap();
    form.update(size, 1.0).unwrap();
    assert_eq!(form.ui.scroll_offset(scroll), Some(Point::ZERO));
}

#[test]
fn collapsed_ancestor_restores_hidden_descendants_without_recreating_them() {
    let size = Size::new(300.0, 200.0);
    let mut form = form(
        "<StackPanel Spacing='0'><StackPanel x:Name='Parent' Visibility='Collapsed' Spacing='0'><Border x:Name='Hidden' Height='30' Visibility='Hidden'><Grid/></Border><TextBox x:Name='Editor' Height='40'/></StackPanel><Border x:Name='Last' Height='50'><Grid/></Border></StackPanel>",
        size,
    );
    let parent = form.find_name("Parent").unwrap();
    let hidden = form.find_name("Hidden").unwrap();
    let editor = form.find_name("Editor").unwrap();
    assert!(form.ui.runtime.bounds(editor).is_none());
    assert_rect(bounds(&form, "Last"), Rect::new(0.0, 0.0, 300.0, 50.0));
    let mut props = form.ui.runtime.tree().node(parent).unwrap().props.clone();
    props.visible = true;
    props.collapsed = false;
    form.ui
        .runtime
        .apply(Mutation::SetProps {
            node: parent,
            props,
        })
        .unwrap();
    form.update(size, 1.0).unwrap();
    assert_eq!(form.find_name("Editor"), Some(editor));
    assert_eq!(form.find_name("Hidden"), Some(hidden));
    assert!(form.ui.runtime.bounds(hidden).is_none());
    assert_rect(
        form.ui.runtime.layout_bounds(hidden).unwrap(),
        Rect::new(0.0, 0.0, 300.0, 30.0),
    );
    assert_rect(bounds(&form, "Editor"), Rect::new(0.0, 30.0, 300.0, 40.0));
    assert_rect(bounds(&form, "Last"), Rect::new(0.0, 70.0, 300.0, 50.0));
}
