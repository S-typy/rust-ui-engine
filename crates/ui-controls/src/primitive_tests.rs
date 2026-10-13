use crate::*;
use rust_desktop_ui_core::{
    Axis, Color, DrawCommand, Edges, InputEvent, Key, LayoutKind, LayoutStyle, Length,
    MAX_STROKE_SEGMENTS, Modifiers, Mutation, Point, Rect, Scene, Size, StrokeSegment,
};

fn layout(width: f32, height: f32) -> LayoutStyle {
    LayoutStyle {
        width: Length::Px(width),
        height: Length::Px(height),
        flex_shrink: 0.0,
        ..Default::default()
    }
}
fn frame(ui: &mut Controls) {
    ui.update(Size::new(600.0, 480.0)).unwrap();
}
fn translucent() -> Color {
    Color {
        a: 0.4,
        ..Color::rgb(10, 70, 130)
    }
}

#[test]
fn every_symbol_is_a_single_bounded_vector_path_without_text_or_font_dependency() {
    for symbol in [
        IconSymbol::Check,
        IconSymbol::Close,
        IconSymbol::Add,
        IconSymbol::Remove,
        IconSymbol::ChevronDown,
        IconSymbol::ChevronRight,
        IconSymbol::Save,
        IconSymbol::Pin,
    ] {
        let mut ui = Controls::new(Theme::FluentLight).unwrap();
        let id = ui.icon(ui.root(), symbol, LayoutStyle::default()).unwrap();
        ui.set_label(id, "Meaningful accessible name").unwrap();
        frame(&mut ui);
        let bounds = ui.runtime.bounds(id).unwrap();
        assert_eq!((bounds.width, bounds.height), (16.0, 16.0));
        assert_eq!(ui.scene().stroke_paths.len(), 1);
        let path = &ui.scene().stroke_paths[0];
        assert!(path.is_valid());
        assert!(!path.segments.is_empty() && path.segments.len() <= MAX_STROKE_SEGMENTS);
        assert_eq!(path.width, 1.5);
        assert_eq!(path.color, ui.theme().tokens().text);
        assert_eq!(path.bounds().unwrap().intersection(bounds), path.bounds());
        assert!(ui.scene().texts.is_empty());
        let semantics = ui.semantics();
        let node = semantics.iter().find(|node| node.id == id).unwrap();
        assert_eq!(node.role, Role::Image);
        assert_eq!(node.label, "Meaningful accessible name");
        assert!(node.actions.is_empty());
    }
}

#[test]
fn rectangular_icon_bounds_preserve_uniform_scale_and_center_the_geometry() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let wide = ui
        .icon(ui.root(), IconSymbol::Add, layout(32.0, 16.0))
        .unwrap();
    let tall = ui
        .icon(ui.root(), IconSymbol::Add, layout(16.0, 32.0))
        .unwrap();
    frame(&mut ui);
    for (id, path) in [wide, tall].into_iter().zip(&ui.scene().stroke_paths) {
        let bounds = ui.runtime.bounds(id).unwrap();
        let center = Point::new(
            bounds.x + bounds.width / 2.0,
            bounds.y + bounds.height / 2.0,
        );
        assert_eq!(path.width, 1.5);
        assert_eq!(
            path.segments[0],
            StrokeSegment {
                start: Point::new(center.x - 5.0, center.y),
                end: Point::new(center.x + 5.0, center.y)
            }
        );
        assert_eq!(
            path.segments[1],
            StrokeSegment {
                start: Point::new(center.x, center.y - 5.0),
                end: Point::new(center.x, center.y + 5.0)
            }
        );
    }
}

#[test]
fn icon_colors_follow_theme_ancestor_enabled_and_explicit_alpha_overrides() {
    let mut ui = Controls::new(Theme::Light).unwrap();
    let parent = ui
        .transparent_panel(ui.root(), layout(120.0, 100.0))
        .unwrap();
    let id = ui
        .icon(parent, IconSymbol::Close, LayoutStyle::default())
        .unwrap();
    for theme in [
        Theme::Light,
        Theme::Dark,
        Theme::Compact,
        Theme::FluentLight,
        Theme::FluentDark,
    ] {
        ui.set_theme(theme).unwrap();
        ui.set_enabled(parent, true).unwrap();
        frame(&mut ui);
        assert_eq!(ui.scene().stroke_paths[0].color, theme.tokens().text);
        assert_eq!(ui.runtime.bounds(id).unwrap().width, 16.0);
        ui.set_enabled(parent, false).unwrap();
        frame(&mut ui);
        assert_eq!(ui.scene().stroke_paths[0].color, theme.tokens().muted);
    }
    let style = IconStyle {
        foreground: Some(translucent()),
        disabled_foreground: Some(Color::rgb(1, 2, 3)),
    };
    ui.set_icon_style(id, style).unwrap();
    frame(&mut ui);
    assert_eq!(
        ui.scene().stroke_paths[0].color,
        style.disabled_foreground.unwrap()
    );
    ui.set_enabled(parent, true).unwrap();
    ui.set_theme(Theme::Light).unwrap();
    frame(&mut ui);
    assert_eq!(ui.scene().stroke_paths[0].color, translucent());
    assert_eq!(
        ui.scene().stroke_paths.len(),
        1,
        "crossings share one alpha coverage operation"
    );
}

#[test]
fn decorative_controls_do_not_intercept_pointer_or_tab_or_offer_actions() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let panel = ui
        .transparent_panel(
            ui.root(),
            LayoutStyle {
                kind: LayoutKind::Overlay,
                ..layout(160.0, 40.0)
            },
        )
        .unwrap();
    let button = ui
        .button(panel, "Under", None, layout(160.0, 40.0))
        .unwrap();
    let icon = ui
        .icon(panel, IconSymbol::Add, layout(160.0, 40.0))
        .unwrap();
    let separator = ui
        .separator(panel, SeparatorSettings::default(), layout(160.0, 40.0))
        .unwrap();
    frame(&mut ui);
    let b = ui.runtime.bounds(button).unwrap();
    assert_eq!(
        ui.runtime.hit_test(Point::new(b.x + 8.0, b.y + 8.0)),
        Some(button)
    );
    ui.dispatch(InputEvent::KeyDown {
        key: Key::Tab,
        modifiers: Modifiers::default(),
        repeat: false,
    })
    .unwrap();
    assert_eq!(ui.runtime.focused(), Some(button));
    for (id, role) in [(icon, Role::Image), (separator, Role::Separator)] {
        let props = &ui.runtime.tree().node(id).unwrap().props;
        assert!(!props.focusable && !props.hit_test);
        let nodes = ui.semantics();
        let node = nodes.iter().find(|node| node.id == id).unwrap();
        assert_eq!(node.role, role);
        assert!(node.actions.is_empty());
        assert!(node.label.is_empty());
        ui.activate(id).unwrap();
    }
    assert!(ui.drain_activated().is_empty());
}

#[test]
fn primitive_style_and_constructor_validation_is_atomic() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let root = ui.root();
    let icon = ui
        .icon(root, IconSymbol::Save, LayoutStyle::default())
        .unwrap();
    let separator = ui
        .separator(root, SeparatorSettings::default(), layout(80.0, 12.0))
        .unwrap();
    frame(&mut ui);
    let scene = ui.scene().clone();
    let count = ui.runtime.tree().len();
    for channel in [f32::NAN, f32::INFINITY, -0.1, 1.1] {
        let invalid = Color {
            a: channel,
            ..translucent()
        };
        assert!(
            ui.set_icon_style(
                icon,
                IconStyle {
                    foreground: Some(invalid),
                    ..Default::default()
                }
            )
            .is_err()
        );
        assert!(
            ui.set_icon_style(
                icon,
                IconStyle {
                    disabled_foreground: Some(invalid),
                    ..Default::default()
                }
            )
            .is_err()
        );
        assert!(
            ui.set_separator_settings(
                separator,
                SeparatorSettings {
                    foreground: Some(invalid),
                    ..Default::default()
                }
            )
            .is_err()
        );
    }
    for thickness in [f32::NAN, f32::INFINITY, 0.0, -1.0, 16385.0] {
        let settings = SeparatorSettings {
            thickness,
            ..Default::default()
        };
        assert!(ui.set_separator_settings(separator, settings).is_err());
        assert!(
            ui.separator(root, settings, LayoutStyle::default())
                .is_err()
        );
    }
    assert_eq!(ui.runtime.tree().len(), count);
    assert_eq!(ui.icon_style(icon), Some(&IconStyle::default()));
    assert_eq!(
        ui.separator_settings(separator),
        Some(&SeparatorSettings::default())
    );
    assert_eq!(ui.scene(), &scene);
    assert_eq!(ui.update(Size::new(600.0, 480.0)).unwrap().layout_passes, 0);
}

#[test]
fn primitive_accessors_reject_wrong_kinds_removed_and_recycled_handles() {
    let mut ui = Controls::new(Theme::Light).unwrap();
    let root = ui.root();
    let icon = ui
        .icon(root, IconSymbol::Pin, LayoutStyle::default())
        .unwrap();
    let separator = ui
        .separator(root, SeparatorSettings::default(), layout(40.0, 10.0))
        .unwrap();
    let checkbox = ui
        .checkbox(root, "Check", false, layout(120.0, 32.0))
        .unwrap();
    assert!(
        ui.set_button_icon(checkbox, Some(ButtonIcon::new(IconSymbol::Check)))
            .is_err()
    );
    assert!(ui.set_button_icon(icon, None).is_err());
    assert!(ui.set_icon_symbol(separator, IconSymbol::Save).is_err());
    assert!(ui.set_icon_style(separator, IconStyle::default()).is_err());
    assert!(
        ui.set_separator_settings(icon, SeparatorSettings::default())
            .is_err()
    );
    assert_eq!(ui.icon_symbol(separator), None);
    assert_eq!(ui.icon_style(separator), None);
    assert_eq!(ui.separator_settings(icon), None);
    ui.runtime.apply(Mutation::Remove(icon)).unwrap();
    ui.runtime.apply(Mutation::Remove(separator)).unwrap();
    let replacement = ui
        .icon(root, IconSymbol::Add, LayoutStyle::default())
        .unwrap();
    assert_ne!(replacement, icon);
    assert_eq!(ui.icon_symbol(icon), None);
    assert_eq!(ui.icon_style(icon), None);
    assert_eq!(ui.separator_settings(separator), None);
    assert!(ui.set_icon_symbol(icon, IconSymbol::Close).is_err());
    assert!(ui.set_icon_style(icon, IconStyle::default()).is_err());
    assert!(
        ui.set_separator_settings(separator, SeparatorSettings::default())
            .is_err()
    );
    assert_eq!(ui.icon_symbol(replacement), Some(IconSymbol::Add));
}

#[test]
fn separators_draw_a_centered_rule_without_filling_the_layout_box() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let color = translucent();
    for orientation in [Axis::Horizontal, Axis::Vertical] {
        let id = ui
            .separator(
                ui.root(),
                SeparatorSettings {
                    orientation,
                    thickness: 2.0,
                    foreground: Some(color),
                },
                layout(100.0, 20.0),
            )
            .unwrap();
        ui.set_enabled(id, false).unwrap();
    }
    frame(&mut ui);
    let lines: Vec<_> = ui
        .scene()
        .rectangles
        .iter()
        .filter(|rect| rect.color == color)
        .collect();
    assert_eq!(lines.len(), 2);
    assert_eq!(
        (lines[0].bounds.width, lines[0].bounds.height),
        (100.0, 2.0)
    );
    assert_eq!((lines[1].bounds.width, lines[1].bounds.height), (2.0, 20.0));
    let nodes: Vec<_> = ui
        .semantics()
        .into_iter()
        .filter(|node| node.role == Role::Separator)
        .collect();
    for (line, node) in lines.iter().zip(nodes) {
        assert_eq!(
            line.bounds.x + line.bounds.width / 2.0,
            node.bounds.x + node.bounds.width / 2.0
        );
        assert_eq!(
            line.bounds.y + line.bounds.height / 2.0,
            node.bounds.y + node.bounds.height / 2.0
        );
    }
}

#[test]
fn separator_manages_only_original_auto_axes_and_color_changes_need_no_layout() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let auto = ui
        .separator(
            ui.root(),
            SeparatorSettings::default(),
            LayoutStyle::default(),
        )
        .unwrap();
    let explicit = ui
        .separator(ui.root(), SeparatorSettings::default(), layout(60.0, 20.0))
        .unwrap();
    frame(&mut ui);
    assert_eq!(
        ui.runtime.tree().node(auto).unwrap().props.style.height,
        Length::Px(1.0)
    );
    let vertical = SeparatorSettings {
        orientation: Axis::Vertical,
        thickness: 3.0,
        foreground: None,
    };
    ui.set_separator_settings(auto, vertical).unwrap();
    ui.set_separator_settings(explicit, vertical).unwrap();
    assert_eq!(
        ui.runtime.tree().node(auto).unwrap().props.style.width,
        Length::Px(3.0)
    );
    assert_eq!(
        ui.runtime.tree().node(auto).unwrap().props.style.height,
        Length::Auto
    );
    assert_eq!(
        ui.runtime.tree().node(explicit).unwrap().props.style,
        layout(60.0, 20.0)
    );
    frame(&mut ui);
    ui.set_separator_settings(
        explicit,
        SeparatorSettings {
            foreground: Some(translucent()),
            ..vertical
        },
    )
    .unwrap();
    assert_eq!(ui.update(Size::new(600.0, 480.0)).unwrap().layout_passes, 0);
    ui.set_theme(Theme::FluentDark).unwrap();
    frame(&mut ui);
    assert!(
        ui.scene()
            .rectangles
            .iter()
            .any(|rect| rect.color == translucent())
    );
}

#[test]
fn button_icon_centers_combined_content_and_keeps_label_semantics_and_checked_state() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let id = ui
        .toggle(ui.root(), "Save", None, true, layout(200.0, 44.0))
        .unwrap();
    ui.set_button_icon(id, Some(ButtonIcon::new(IconSymbol::Save)))
        .unwrap();
    ui.set_button_style(
        id,
        ButtonStyle {
            padding: Some(Edges::all(8.0)),
            normal: ButtonAppearance {
                foreground: Some(translucent()),
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .unwrap();
    frame(&mut ui);
    let bounds = ui.runtime.bounds(id).unwrap();
    let run = ui
        .scene()
        .texts
        .iter()
        .find(|run| run.text == "Save")
        .unwrap();
    let path = &ui.scene().stroke_paths[0];
    let icon_left = path.segments[0].start.x - 2.0;
    assert!((run.bounds.x - icon_left - 24.0).abs() < 0.01);
    assert!(
        (icon_left + (24.0 + run.bounds.width) / 2.0 - bounds.x - bounds.width / 2.0).abs() < 0.01
    );
    assert_eq!(path.color, run.color);
    assert_eq!(path.color, translucent());
    assert_eq!(ui.checked(id), Some(true));
    let nodes = ui.semantics();
    let node = nodes.iter().find(|node| node.id == id).unwrap();
    assert_eq!(node.label, "Save");
    assert!(node.children.is_empty());
    ui.set_icon_symbol(id, IconSymbol::Pin).unwrap_err();
    ui.set_button_icon(id, Some(ButtonIcon::new(IconSymbol::Pin)))
        .unwrap();
    frame(&mut ui);
    assert_eq!(ui.scene().texts[0].text, "Save");
    assert_eq!(ui.checked(id), Some(true));
}

#[test]
fn later_explicit_separator_dimensions_survive_settings_and_orientation_changes() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let id = ui
        .separator(
            ui.root(),
            SeparatorSettings::default(),
            LayoutStyle::default(),
        )
        .unwrap();
    let mut style = ui.runtime.tree().node(id).unwrap().props.style.clone();
    style.width = Length::Px(75.0);
    style.height = Length::Px(25.0);
    ui.runtime
        .apply(Mutation::SetStyle {
            node: id,
            style: style.clone(),
        })
        .unwrap();
    for orientation in [Axis::Vertical, Axis::Horizontal] {
        ui.set_separator_settings(
            id,
            SeparatorSettings {
                orientation,
                thickness: 5.0,
                foreground: Some(translucent()),
            },
        )
        .unwrap();
        assert_eq!(ui.runtime.tree().node(id).unwrap().props.style, style);
    }
    frame(&mut ui);
    assert_eq!(ui.runtime.bounds(id).unwrap().width, 75.0);
    assert_eq!(ui.runtime.bounds(id).unwrap().height, 25.0);
    let line = ui
        .scene()
        .rectangles
        .iter()
        .find(|line| line.color == translucent())
        .unwrap();
    assert_eq!((line.bounds.width, line.bounds.height), (75.0, 5.0));
}

#[test]
fn clipped_icon_keeps_its_original_center_and_separator_uses_the_same_ancestor_clip() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let parent = ui
        .transparent_panel(
            ui.root(),
            LayoutStyle {
                kind: LayoutKind::Overlay,
                ..layout(16.0, 16.0)
            },
        )
        .unwrap();
    let icon = ui
        .icon(parent, IconSymbol::Add, layout(32.0, 16.0))
        .unwrap();
    let separator = ui
        .separator(
            parent,
            SeparatorSettings {
                thickness: 2.0,
                foreground: Some(translucent()),
                ..Default::default()
            },
            layout(32.0, 16.0),
        )
        .unwrap();
    frame(&mut ui);
    let parent_bounds = ui.runtime.bounds(parent).unwrap();
    let icon_bounds = ui.runtime.bounds(icon).unwrap();
    assert_eq!(icon_bounds.width, 32.0);
    let path = &ui.scene().stroke_paths[0];
    assert_eq!(path.clip, parent_bounds);
    assert_eq!(path.segments[0].end.x, icon_bounds.x + 21.0);
    assert!(path.segments[0].end.x > path.clip.x + path.clip.width);
    let line = ui
        .scene()
        .rectangles
        .iter()
        .find(|line| line.color == translucent())
        .unwrap();
    assert_eq!(line.bounds.width, 16.0);
    assert_eq!(ui.runtime.bounds(separator).unwrap().width, 32.0);
}

#[test]
fn icon_only_buttons_center_without_gap_and_disabled_icon_matches_text_palette() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let id = ui.button(ui.root(), "", None, layout(100.0, 40.0)).unwrap();
    ui.set_button_icon(
        id,
        Some(ButtonIcon {
            spacing: 16384.0,
            ..ButtonIcon::new(IconSymbol::Add)
        }),
    )
    .unwrap();
    frame(&mut ui);
    let bounds = ui.runtime.bounds(id).unwrap();
    let segment = ui.scene().stroke_paths[0].segments[0];
    assert_eq!(
        (segment.start.x + segment.end.x) / 2.0,
        bounds.x + bounds.width / 2.0
    );
    assert_eq!(segment.start.y, bounds.y + bounds.height / 2.0);
    assert!(ui.scene().texts.is_empty());
    ui.set_label(id, "Disabled").unwrap();
    ui.set_enabled(id, false).unwrap();
    frame(&mut ui);
    assert_eq!(
        ui.scene().stroke_paths[0].color,
        Theme::FluentLight.tokens().muted
    );
    // A deliberately oversized gap can consume the text allocation without changing its label.
    assert_eq!(
        ui.semantics()
            .into_iter()
            .find(|node| node.id == id)
            .unwrap()
            .label,
        "Disabled"
    );
}

#[test]
fn invalid_button_icons_preserve_press_focus_scene_and_previous_icon() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let id = ui
        .button(ui.root(), "Save", None, layout(160.0, 36.0))
        .unwrap();
    let icon = ButtonIcon::new(IconSymbol::Save);
    ui.set_button_icon(id, Some(icon)).unwrap();
    frame(&mut ui);
    ui.runtime.apply(Mutation::Focus(Some(id))).unwrap();
    ui.dispatch(InputEvent::KeyDown {
        key: Key::Space,
        modifiers: Modifiers::default(),
        repeat: false,
    })
    .unwrap();
    frame(&mut ui);
    let state = ui.button_visual_state(id).unwrap();
    let scene = ui.scene().clone();
    for size in [f32::NAN, f32::INFINITY, 0.0, -1.0, 513.0] {
        assert!(
            ui.set_button_icon(id, Some(ButtonIcon { size, ..icon }))
                .is_err()
        );
    }
    for spacing in [f32::NAN, f32::INFINITY, -1.0, 16385.0] {
        assert!(
            ui.set_button_icon(id, Some(ButtonIcon { spacing, ..icon }))
                .is_err()
        );
    }
    assert_eq!(ui.button_icon(id), Some(&icon));
    assert_eq!(ui.button_visual_state(id), Some(state));
    assert_eq!(ui.runtime.focused(), Some(id));
    assert_eq!(ui.scene(), &scene);
    ui.runtime.apply(Mutation::Remove(id)).unwrap();
    assert_eq!(ui.button_icon(id), None);
    assert!(ui.set_button_icon(id, None).is_err());
}

#[test]
fn removing_icon_restores_legacy_button_scene_without_explicit_style() {
    let mut ui = Controls::new(Theme::Compact).unwrap();
    let id = ui
        .button(ui.root(), "Same", None, layout(160.0, 40.0))
        .unwrap();
    frame(&mut ui);
    let original = ui.scene().clone();
    ui.set_button_icon(id, Some(ButtonIcon::new(IconSymbol::Close)))
        .unwrap();
    frame(&mut ui);
    assert_eq!(ui.scene().stroke_paths.len(), 1);
    ui.set_button_icon(id, None).unwrap();
    frame(&mut ui);
    assert_eq!(ui.scene(), &original);
}

#[test]
fn custom_strokes_keep_geometry_alpha_and_painter_order_when_ancestor_clipped() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let panel = ui.transparent_panel(ui.root(), layout(50.0, 30.0)).unwrap();
    frame(&mut ui);
    let clip = Rect::new(-20.0, -20.0, 100.0, 100.0);
    let segments = [StrokeSegment {
        start: Point::new(-10.0, 5.0),
        end: Point::new(80.0, 5.0),
    }];
    let mut scene = Scene::default();
    scene.fill(Rect::new(1.0, 1.0, 5.0, 5.0), Color::rgb(1, 2, 3));
    scene.stroke_path(&segments, clip, 3.0, translucent());
    scene.fill(Rect::new(1.0, 1.0, 3.0, 3.0), Color::rgb(3, 2, 1));
    ui.set_custom_scene(panel, scene).unwrap();
    frame(&mut ui);
    let path = &ui.scene().stroke_paths[0];
    assert_eq!(path.segments, segments);
    assert_eq!(path.width, 3.0);
    assert_eq!(path.color, translucent());
    assert_eq!(
        path.clip,
        clip.intersection(ui.runtime.clip(panel).unwrap()).unwrap()
    );
    let commands = ui.scene().commands();
    let index = commands
        .iter()
        .position(|command| matches!(command, DrawCommand::StrokePath(0)))
        .unwrap();
    assert!(matches!(commands[index - 1], DrawCommand::Rectangle(_)));
    assert!(matches!(commands[index + 1], DrawCommand::Rectangle(_)));
}

#[test]
fn tiny_button_and_icon_geometries_remain_valid_and_clip_to_their_content() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    for size in [0.0, 0.001, 1.0, 8.0] {
        ui.icon(ui.root(), IconSymbol::Check, layout(size, size))
            .unwrap();
        let id = ui
            .button(ui.root(), "Wide text", None, layout(size, size))
            .unwrap();
        ui.set_button_style(
            id,
            ButtonStyle {
                padding: Some(Edges::all(0.0)),
                ..Default::default()
            },
        )
        .unwrap();
        ui.set_button_icon(
            id,
            Some(ButtonIcon {
                size: 512.0,
                spacing: 0.0,
                symbol: IconSymbol::Pin,
            }),
        )
        .unwrap();
    }
    frame(&mut ui);
    assert!(ui.scene().stroke_paths.iter().all(|path| path.is_valid()));
    assert!(ui.scene().texts.iter().all(|run| run.is_valid()));
}
