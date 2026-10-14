use super::*;
use rust_desktop_ui_core::Modifiers;

fn layout(width: f32, height: f32) -> LayoutStyle {
    LayoutStyle {
        width: Length::Px(width),
        height: Length::Px(height),
        flex_shrink: 0.0,
        ..Default::default()
    }
}

fn frame(ui: &mut Controls, width: f32) {
    ui.update(Size::new(width, 900.0)).unwrap();
}

fn text_run<'a>(ui: &'a Controls, text: &str) -> &'a TextRun {
    ui.scene()
        .texts
        .iter()
        .find(|run| run.text == text)
        .unwrap()
}

fn assert_measured_height(ui: &mut Controls, id: WidgetId, text: &str) -> f32 {
    let bounds = ui.runtime.bounds(id).unwrap();
    let style = ui.text_style(id).unwrap();
    let measured = ui
        .measure_styled_text(text, (bounds.width - 16.0).max(0.0), style)
        .unwrap();
    assert!((bounds.height - measured.height.ceil()).abs() < 0.001);
    let run = text_run(ui, text);
    assert_eq!(
        run.font_size,
        style.font_size.unwrap_or(ui.theme().tokens().font_size)
    );
    assert_eq!(run.weight, style.font_weight.unwrap_or(400));
    assert_eq!(run.wrap, style.wrap);
    assert!(run.is_valid());
    bounds.height
}

#[test]
fn rejected_text_styles_leave_label_tree_scene_and_focus_unchanged() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let label = ui.label(ui.root(), "Heading", layout(240.0, 38.0)).unwrap();
    let editor = ui
        .edit_box(ui.root(), "Editor", "Value", layout(240.0, 36.0))
        .unwrap();
    let style = TextStyle {
        font_size: Some(20.0),
        font_weight: Some(600),
        tone: TextTone::Secondary,
        ..Default::default()
    };
    ui.set_text_style(label, style).unwrap();
    ui.runtime.apply(Mutation::Focus(Some(editor))).unwrap();
    frame(&mut ui, 400.0);
    let scene = ui.scene().clone();
    let nodes = ui.runtime.tree().preorder();
    let layout = ui.runtime.tree().node(label).unwrap().props.style.clone();
    let mut invalid = vec![];
    for size in [f32::NAN, f32::INFINITY, 0.0, -1.0, 513.0] {
        invalid.push(TextStyle {
            font_size: Some(size),
            ..style
        });
    }
    for weight in [0, 1001] {
        invalid.push(TextStyle {
            font_weight: Some(weight),
            ..style
        });
    }
    for channel in [f32::NAN, f32::INFINITY, -0.1, 1.1] {
        invalid.push(TextStyle {
            foreground: Some(Color {
                r: channel,
                ..Color::rgb(10, 80, 140)
            }),
            ..style
        });
        invalid.push(TextStyle {
            foreground: Some(Color {
                a: channel,
                ..Color::rgb(10, 80, 140)
            }),
            ..style
        });
    }
    for invalid in invalid {
        assert!(ui.set_text_style(label, invalid).is_err());
        assert!(ui.measure_styled_text("Heading", 200.0, invalid).is_err());
        assert_eq!(ui.text_style(label), Some(style));
        assert_eq!(ui.runtime.tree().node(label).unwrap().props.style, layout);
        assert_eq!(ui.runtime.tree().preorder(), nodes);
        assert_eq!(ui.runtime.focused(), Some(editor));
        assert_eq!(ui.scene(), &scene);
    }
    for width in [f32::NAN, f32::INFINITY, -1.0, 16_385.0] {
        assert!(ui.measure_styled_text("Heading", width, style).is_err());
    }
    let stats = ui.update(Size::new(400.0, 900.0)).unwrap();
    assert_eq!(stats.layout_passes, 0);
    assert_eq!(stats.paint_passes, 0);
}

#[test]
fn text_auto_height_tracks_width_content_and_typography_without_replacing_editor() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let text = "A document heading and useful description that wraps across several lines in a narrow form.";
    let label = ui.label(ui.root(), text, LayoutStyle::default()).unwrap();
    ui.set_text_auto_height(label, true).unwrap();
    let editor = ui
        .edit_box(ui.root(), "Editor", "Value", layout(120.0, 36.0))
        .unwrap();
    ui.runtime.apply(Mutation::Focus(Some(editor))).unwrap();
    ui.text_input(" draft").unwrap();
    ui.dispatch(InputEvent::KeyDown {
        key: Key::ArrowLeft,
        modifiers: Modifiers {
            shift: true,
            ..Default::default()
        },
        repeat: false,
    })
    .unwrap();
    frame(&mut ui, 520.0);
    let wide = assert_measured_height(&mut ui, label, text);
    let nodes = ui.runtime.tree().preorder();
    let selection = ui.document(editor).unwrap().selection();

    frame(&mut ui, 180.0);
    let narrow = assert_measured_height(&mut ui, label, text);
    assert!(narrow > wide);
    frame(&mut ui, 520.0);
    assert_eq!(assert_measured_height(&mut ui, label, text), wide);

    let heading = TextStyle {
        font_size: Some(28.0),
        font_weight: Some(600),
        ..Default::default()
    };
    ui.set_text_style(label, heading).unwrap();
    frame(&mut ui, 520.0);
    let larger = assert_measured_height(&mut ui, label, text);
    assert!(larger > wide);
    ui.set_label(label, "Short").unwrap();
    frame(&mut ui, 520.0);
    assert!(assert_measured_height(&mut ui, label, "Short") < larger);

    ui.set_text_style(label, TextStyle::default()).unwrap();
    for theme in [Theme::Compact, Theme::FluentDark, Theme::FluentLight] {
        ui.set_theme(theme).unwrap();
        frame(&mut ui, 520.0);
        assert_measured_height(&mut ui, label, "Short");
        assert_eq!(ui.runtime.tree().preorder(), nodes);
        assert_eq!(ui.runtime.focused(), Some(editor));
        assert_eq!(ui.document(editor).unwrap().text(), "Value draft");
        assert_eq!(ui.document(editor).unwrap().selection(), selection);
        assert!(ui.document(editor).unwrap().can_undo());
        assert_eq!(ui.accepted_value(editor), Some("Value"));
    }
    ui.dispatch(InputEvent::KeyDown {
        key: Key::Character("z".into()),
        modifiers: Modifiers {
            control: true,
            ..Default::default()
        },
        repeat: false,
    })
    .unwrap();
    assert_eq!(ui.document(editor).unwrap().text(), "Value");
}

#[test]
fn fixed_label_height_remains_owned_by_application_until_explicit_auto_height() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let text = "A long description that cannot fit in the deliberately small fixed-height label.";
    let label = ui.label(ui.root(), text, layout(100.0, 15.0)).unwrap();
    ui.set_text_style(
        label,
        TextStyle {
            font_size: Some(24.0),
            ..Default::default()
        },
    )
    .unwrap();
    for theme in [Theme::FluentLight, Theme::FluentDark] {
        ui.set_theme(theme).unwrap();
        frame(&mut ui, 400.0);
        assert_eq!(ui.runtime.bounds(label).unwrap().height, 15.0);
    }
    ui.set_text_auto_height(label, true).unwrap();
    frame(&mut ui, 400.0);
    assert!(assert_measured_height(&mut ui, label, text) > 15.0);
    ui.set_text_auto_height(label, false).unwrap();
    let mut style = ui.runtime.tree().node(label).unwrap().props.style.clone();
    style.height = Length::Px(27.0);
    ui.runtime
        .apply(Mutation::SetStyle { node: label, style })
        .unwrap();
    ui.set_label(label, "Short").unwrap();
    frame(&mut ui, 400.0);
    assert_eq!(ui.runtime.bounds(label).unwrap().height, 27.0);
}

#[test]
fn card_and_secondary_text_follow_theme_while_explicit_text_color_and_clip_survive() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let parent = ui
        .transparent_panel(
            ui.root(),
            LayoutStyle {
                kind: LayoutKind::Overlay,
                ..layout(140.0, 160.0)
            },
        )
        .unwrap();
    let card = ui
        .transparent_panel(
            parent,
            LayoutStyle {
                kind: LayoutKind::Stack(Axis::Vertical),
                ..layout(240.0, 120.0)
            },
        )
        .unwrap();
    let secondary = ui
        .label(card, "Secondary text", layout(220.0, 32.0))
        .unwrap();
    let explicit = ui
        .label(card, "Explicit text", layout(220.0, 32.0))
        .unwrap();
    let explicit_color = Color::rgb(50, 130, 180);
    ui.set_surface(card, SurfaceKind::Card).unwrap();
    ui.set_text_style(
        secondary,
        TextStyle {
            tone: TextTone::Secondary,
            ..Default::default()
        },
    )
    .unwrap();
    ui.set_text_style(
        explicit,
        TextStyle {
            foreground: Some(explicit_color),
            tone: TextTone::Secondary,
            ..Default::default()
        },
    )
    .unwrap();
    frame(&mut ui, 400.0);
    let nodes = ui.runtime.tree().preorder();
    let bounds = ui.runtime.bounds(card).unwrap();
    let clip = bounds.intersection(ui.runtime.clip(card).unwrap()).unwrap();
    assert!(clip.width < bounds.width);
    let mut previous = None;
    for theme in [Theme::FluentLight, Theme::FluentDark, Theme::FluentLight] {
        ui.set_theme(theme).unwrap();
        frame(&mut ui, 400.0);
        let card_shapes: Vec<_> = ui
            .scene()
            .rounded_rectangles
            .iter()
            .filter(|shape| shape.bounds == bounds)
            .collect();
        assert_eq!(card_shapes.len(), 2);
        let fill = card_shapes
            .iter()
            .find(|shape| shape.stroke_width.is_none())
            .unwrap();
        let border = card_shapes
            .iter()
            .find(|shape| shape.stroke_width.is_some())
            .unwrap();
        assert_eq!(fill.color, theme.chrome().card_surface);
        assert_eq!(border.color, theme.chrome().surface_border);
        // Rounded primitives retain the ancestor clip and original geometry;
        // their visible region is the intersection, not the stored clip alone.
        assert!(
            card_shapes.iter().all(
                |shape| shape.bounds.intersection(shape.clip) == Some(clip) && shape.is_valid()
            )
        );
        assert_eq!(text_run(&ui, "Secondary text").color, theme.tokens().muted);
        assert_eq!(text_run(&ui, "Explicit text").color, explicit_color);
        for text in ["Secondary text", "Explicit text"] {
            let run = text_run(&ui, text);
            assert_eq!(run.clip.intersection(clip), Some(run.clip));
        }
        if let Some(previous) = previous {
            assert_ne!(fill.color, previous);
        }
        previous = Some(fill.color);
        assert_eq!(ui.runtime.bounds(card), Some(bounds));
        assert_eq!(ui.runtime.tree().preorder(), nodes);
    }
    ui.set_surface(card, SurfaceKind::Transparent).unwrap();
    frame(&mut ui, 400.0);
    assert_eq!(ui.surface(card), Some(SurfaceKind::Transparent));
    assert!(
        !ui.scene()
            .rounded_rectangles
            .iter()
            .any(|shape| shape.bounds == bounds)
    );
    assert_eq!(ui.runtime.bounds(card), Some(bounds));
    assert_eq!(ui.runtime.tree().preorder(), nodes);
}

#[test]
fn appearance_accessors_reject_wrong_kinds_and_removed_handles() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let panel = ui
        .transparent_panel(ui.root(), layout(200.0, 100.0))
        .unwrap();
    let label = ui.label(panel, "Text", layout(180.0, 24.0)).unwrap();
    ui.set_surface(panel, SurfaceKind::Card).unwrap();
    frame(&mut ui, 400.0);
    let scene = ui.scene().clone();
    assert!(ui.set_text_style(panel, TextStyle::default()).is_err());
    assert!(ui.set_text_auto_height(panel, true).is_err());
    assert!(ui.set_surface(label, SurfaceKind::Card).is_err());
    assert_eq!(ui.text_style(panel), None);
    assert_eq!(ui.surface(label), None);
    assert_eq!(ui.scene(), &scene);
    ui.runtime.apply(Mutation::Remove(panel)).unwrap();
    assert_eq!(ui.text_style(label), None);
    assert_eq!(ui.surface(panel), None);
    assert!(ui.set_text_style(label, TextStyle::default()).is_err());
    assert!(ui.set_text_auto_height(label, true).is_err());
    assert!(ui.set_surface(panel, SurfaceKind::Card).is_err());
}

#[test]
fn small_ribbon_rows_align_icons_and_captions_with_different_text_lengths() {
    use crate::{
        Ribbon, RibbonCommandAppearance, RibbonGroup, RibbonItem, RibbonItemKind, RibbonModel,
        RibbonTab,
    };

    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let commands = [
        ("pin", "Pin", "P"),
        ("confirm", "Confirm document", "C"),
        ("copy", "Copy", "Y"),
    ];
    for (id, label, _) in commands {
        ui.commands_mut().register(id, label);
    }
    let mut ribbon = Ribbon::new(RibbonModel {
        quick_access: vec![],
        tabs: vec![RibbonTab {
            id: "home".into(),
            label: "Home".into(),
            key_tip: "H".into(),
            context: None,
            groups: vec![RibbonGroup {
                id: "document".into(),
                label: "Document".into(),
                collapse_priority: 0,
                items: commands
                    .iter()
                    .map(|(id, _, tip)| RibbonItem::new(*id, RibbonItemKind::Small, *tip))
                    .collect(),
            }],
        }],
    })
    .unwrap();
    for (id, _, _) in commands {
        ribbon
            .set_command_appearance(
                id,
                RibbonCommandAppearance {
                    icon: Some(ButtonIcon::new(IconSymbol::Add)),
                    ..Default::default()
                },
            )
            .unwrap();
    }
    let root = ui.root();
    ribbon.mount_with_width(&mut ui, root, 584.0).unwrap();
    frame(&mut ui, 600.0);
    let semantics = ui.semantics();
    let mut positions = vec![];
    for (command, label, _) in commands {
        let id = semantics
            .iter()
            .find(|node| ui.command_id(node.id).is_some_and(|id| id.0 == command))
            .unwrap()
            .id;
        let bounds = ui.runtime.bounds(id).unwrap();
        let icon = ui
            .scene()
            .stroke_paths
            .iter()
            .find(|path| {
                let point = path.segments[0].start;
                bounds.contains(point.x, point.y)
            })
            .unwrap();
        positions.push((
            icon.segments[0].start.x,
            text_run(&ui, label).bounds.x,
            bounds.y,
        ));
    }
    for pair in positions.windows(2) {
        assert!(
            (pair[0].0 - pair[1].0).abs() < 0.001,
            "icon columns: {positions:?}"
        );
        assert!(
            (pair[0].1 - pair[1].1).abs() < 0.001,
            "caption columns: {positions:?}"
        );
        assert!(pair[0].2 < pair[1].2);
    }
}

#[test]
fn ribbon_explicit_default_foreground_keeps_icons_monochrome_until_style_is_reset() {
    use crate::{
        ButtonAppearance, Ribbon, RibbonCommandAppearance, RibbonGroup, RibbonItem, RibbonItemKind,
        RibbonModel, RibbonTab,
    };

    for theme in [Theme::FluentLight, Theme::FluentDark] {
        let mut ui = Controls::new(theme).unwrap();
        ui.commands_mut().register("confirm", "Confirm");
        let mut ribbon = Ribbon::new(RibbonModel {
            quick_access: vec![],
            tabs: vec![RibbonTab {
                id: "home".into(),
                label: "Home".into(),
                key_tip: "H".into(),
                context: None,
                groups: vec![RibbonGroup {
                    id: "document".into(),
                    label: "Document".into(),
                    collapse_priority: 0,
                    items: vec![RibbonItem::new("confirm", RibbonItemKind::Large, "C")],
                }],
            }],
        })
        .unwrap();
        ribbon
            .set_command_appearance(
                "confirm",
                RibbonCommandAppearance {
                    icon: Some(ButtonIcon::new(IconSymbol::Check)),
                    ..Default::default()
                },
            )
            .unwrap();
        let root = ui.root();
        ribbon.mount_with_width(&mut ui, root, 584.0).unwrap();
        frame(&mut ui, 600.0);
        let id = ui
            .semantics()
            .iter()
            .find(|node| {
                ui.command_id(node.id)
                    .is_some_and(|command| command.0 == "confirm")
            })
            .unwrap()
            .id;
        let nodes = ui.runtime.tree().preorder();
        let icon_colors = |ui: &Controls| {
            let bounds = ui.runtime.bounds(id).unwrap();
            let colors: Vec<_> = ui
                .scene()
                .stroke_paths
                .iter()
                .filter(|path| {
                    path.bounds()
                        .and_then(|painted| painted.intersection(bounds))
                        .is_some()
                })
                .map(|path| path.color)
                .collect();
            assert!(!colors.is_empty());
            colors
        };
        let foreground = theme.tokens().text;
        assert!(icon_colors(&ui).iter().any(|color| *color != foreground));

        ui.set_button_style(
            id,
            ButtonStyle {
                normal: ButtonAppearance {
                    foreground: Some(foreground),
                    ..Default::default()
                },
                ..Default::default()
            },
        )
        .unwrap();
        frame(&mut ui, 600.0);
        assert!(icon_colors(&ui).iter().all(|color| *color == foreground));
        assert_eq!(ui.runtime.tree().preorder(), nodes);

        ui.set_button_style(id, ButtonStyle::default()).unwrap();
        frame(&mut ui, 600.0);
        assert!(icon_colors(&ui).iter().any(|color| *color != foreground));
        assert_eq!(ui.runtime.tree().preorder(), nodes);

        ui.set_enabled(id, false).unwrap();
        frame(&mut ui, 600.0);
        assert!(
            icon_colors(&ui)
                .iter()
                .all(|color| *color == theme.chrome().disabled_text)
        );
        assert_eq!(ui.runtime.tree().preorder(), nodes);
    }
}
