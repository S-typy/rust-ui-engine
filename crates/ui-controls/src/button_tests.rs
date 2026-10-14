use crate::*;
use rust_desktop_ui_core::{
    Axis, Color, DrawCommand, Edges, InputEvent, Key, LayoutKind, LayoutStyle, Length, Modifiers,
    Mutation, Point, PointerButton, Size, TextRun, WidgetId,
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

fn center(ui: &Controls, id: WidgetId) -> Point {
    let bounds = ui.runtime.bounds(id).unwrap();
    Point::new(
        bounds.x + bounds.width * 0.5,
        bounds.y + bounds.height * 0.5,
    )
}

fn pointer(ui: &mut Controls, position: Point, down: bool) {
    ui.dispatch(if down {
        InputEvent::PointerDown {
            position,
            button: PointerButton::Primary,
            modifiers: Modifiers::default(),
        }
    } else {
        InputEvent::PointerUp {
            position,
            button: PointerButton::Primary,
            modifiers: Modifiers::default(),
        }
    })
    .unwrap();
}

fn move_pointer(ui: &mut Controls, position: Point) {
    ui.dispatch(InputEvent::PointerMoved { position }).unwrap();
}

fn key(ui: &mut Controls, key: Key, repeat: bool) {
    ui.dispatch(InputEvent::KeyDown {
        key,
        modifiers: Modifiers::default(),
        repeat,
    })
    .unwrap();
}

fn key_up(ui: &mut Controls, key: Key) {
    ui.dispatch(InputEvent::KeyUp {
        key,
        modifiers: Modifiers::default(),
    })
    .unwrap();
}

fn focus(ui: &mut Controls, id: WidgetId) {
    ui.runtime.apply(Mutation::Focus(Some(id))).unwrap();
}

fn text<'a>(ui: &'a Controls, caption: &str) -> &'a TextRun {
    ui.scene()
        .texts
        .iter()
        .find(|run| run.text == caption)
        .unwrap()
}

fn body_color(ui: &Controls, id: WidgetId) -> Color {
    let bounds = ui.runtime.bounds(id).unwrap();
    ui.scene()
        .rounded_rectangles
        .iter()
        .find(|shape| shape.bounds == bounds && shape.stroke_width.is_none())
        .map(|shape| shape.color)
        .or_else(|| {
            ui.scene()
                .rectangles
                .iter()
                .find(|shape| shape.bounds == bounds)
                .map(|shape| shape.color)
        })
        .expect("button has a background")
}

#[test]
fn above_content_measure_matches_paint_and_preserves_the_full_command_label() {
    for theme in [Theme::FluentLight, Theme::FluentDark, Theme::Compact] {
        let mut ui = Controls::new(theme).unwrap();
        let caption = "Сохранить изменения документа и все параметры текущего представления";
        let style = ButtonStyle {
            padding: Some(Edges::all(4.0)),
            ..Default::default()
        };
        let icon = ButtonIcon {
            symbol: IconSymbol::Save,
            size: 32.0,
            spacing: 4.0,
        };
        let desired = ui
            .measure_button_content(
                caption,
                &style,
                Some(icon),
                ButtonContentLayout::Above,
                88.0,
            )
            .unwrap();
        let id = ui
            .button(ui.root(), caption, None, layout(88.0, desired.height))
            .unwrap();
        ui.set_button_icon(id, Some(icon)).unwrap();
        ui.set_button_style(id, style).unwrap();
        ui.set_button_content_layout(id, ButtonContentLayout::Above)
            .unwrap();
        frame(&mut ui);
        let run = ui.scene().texts[0].clone();
        assert!(run.wrap && run.text.ends_with('…'));
        assert_eq!(run.align, rust_desktop_ui_core::TextAlign::Center);
        let measured = ui.measure_text(&run.text, run.bounds.width, true).unwrap();
        let two_lines = ui.measure_text("Ag\nAg", 88.0, true).unwrap();
        assert!(measured.height <= two_lines.height + 0.001);
        assert!(measured.height <= run.clip.height + 0.001);
        assert!(measured.width <= run.clip.width + 0.001);
        let bounds = ui.runtime.bounds(id).unwrap();
        let path = &ui.scene().stroke_paths[0];
        assert!(path.bounds().unwrap().y + path.bounds().unwrap().height < run.bounds.y);
        assert!(run.bounds.y + measured.height <= bounds.y + bounds.height + 0.001);
        assert_eq!(
            ui.semantics()
                .into_iter()
                .find(|node| node.id == id)
                .unwrap()
                .label,
            caption
        );
        assert_eq!(
            ui.button_content_layout(id),
            Some(ButtonContentLayout::Above)
        );
    }
}

#[test]
fn icon_only_hides_visual_caption_and_retains_toggle_actions_with_text_fallback() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let command = CommandId::from("pin");
    ui.commands_mut()
        .register_toggle(command.clone(), "Pin document", false);
    let id = ui
        .button(
            ui.root(),
            "Pin document",
            Some(command.clone()),
            layout(100.0, 36.0),
        )
        .unwrap();
    ui.set_button_content_layout(id, ButtonContentLayout::IconOnly)
        .unwrap();
    ui.set_button_icon(id, Some(ButtonIcon::new(IconSymbol::Pin)))
        .unwrap();
    frame(&mut ui);
    assert!(ui.scene().texts.is_empty());
    let semantic = ui
        .semantics()
        .into_iter()
        .find(|node| node.id == id)
        .unwrap();
    assert_eq!(semantic.label, "Pin document");
    assert_eq!(semantic.role, Role::ToggleButton);
    let position = center(&ui, id);
    pointer(&mut ui, position, true);
    pointer(&mut ui, position, false);
    assert_eq!(ui.commands().checked(&command), Some(true));
    assert_eq!(ui.commands_mut().drain_invocations().len(), 1);
    ui.set_button_icon(id, None).unwrap();
    frame(&mut ui);
    assert_eq!(text(&ui, "Pin document").text, "Pin document");
    assert_eq!(ui.commands().checked(&command), Some(true));
}

#[test]
fn key_tip_badges_do_not_change_content_layout_focus_or_accessibility() {
    let mut ui = Controls::new(Theme::FluentDark).unwrap();
    let id = ui
        .button(ui.root(), "Save", None, layout(100.0, 40.0))
        .unwrap();
    frame(&mut ui);
    focus(&mut ui, id);
    frame(&mut ui);
    let bounds = ui.runtime.bounds(id).unwrap();
    let caption = text(&ui, "Save").clone();
    let semantic = ui
        .semantics()
        .into_iter()
        .find(|node| node.id == id)
        .unwrap();
    ui.set_key_tip(id, Some("S12".into())).unwrap();
    let stats = ui.update(Size::new(600.0, 480.0)).unwrap();
    assert_eq!(stats.layout_passes, 0);
    assert_eq!(ui.runtime.bounds(id), Some(bounds));
    assert_eq!(ui.runtime.focused(), Some(id));
    assert_eq!(text(&ui, "Save").bounds, caption.bounds);
    assert_eq!(text(&ui, "Save").clip, caption.clip);
    assert_eq!(ui.key_tip(id), Some("S12"));
    assert!(ui.scene().texts.iter().any(|run| run.text == "S12"));
    let after = ui
        .semantics()
        .into_iter()
        .find(|node| node.id == id)
        .unwrap();
    assert_eq!(
        (after.label, after.role, after.actions),
        (semantic.label, semantic.role, semantic.actions)
    );
    assert!(ui.set_key_tip(id, Some("bad tip".into())).is_err());
    assert_eq!(ui.key_tip(id), Some("S12"));
    ui.set_key_tip(id, None).unwrap();
    let stats = ui.update(Size::new(600.0, 480.0)).unwrap();
    assert_eq!(stats.layout_passes, 0);
    assert!(!ui.scene().texts.iter().any(|run| run.text == "S12"));
    assert_eq!(ui.runtime.focused(), Some(id));
}

#[test]
fn flat_and_tab_chrome_are_opt_in_and_follow_the_current_theme() {
    for theme in [Theme::FluentLight, Theme::FluentDark] {
        let mut ui = Controls::new(theme).unwrap();
        let standard = ui
            .button(ui.root(), "Standard", None, layout(100.0, 36.0))
            .unwrap();
        let flat = ui
            .button(ui.root(), "Flat", None, layout(100.0, 36.0))
            .unwrap();
        let tab = ui
            .toggle(ui.root(), "Home", None, true, layout(100.0, 36.0))
            .unwrap();
        ui.set_button_chrome(flat, ButtonChrome::Flat).unwrap();
        ui.set_button_chrome(tab, ButtonChrome::Tab).unwrap();
        frame(&mut ui);
        assert_eq!(ui.button_chrome(standard), Some(ButtonChrome::Standard));
        assert_eq!(body_color(&ui, standard), theme.tokens().surface);
        assert_eq!(body_color(&ui, flat).a, 0.0);
        let bounds = ui.runtime.bounds(tab).unwrap();
        assert!(
            ui.scene()
                .rectangles
                .iter()
                .any(|shape| shape.color == theme.tokens().focus
                    && shape.bounds.y >= bounds.y + bounds.height - 3.001
                    && shape.bounds.height <= 3.001)
        );
        let position = center(&ui, flat);
        move_pointer(&mut ui, position);
        frame(&mut ui);
        assert_eq!(body_color(&ui, flat), theme.tokens().hover);
        ui.set_enabled(flat, false).unwrap();
        frame(&mut ui);
        assert_eq!(body_color(&ui, flat).a, 0.0);
    }
}

#[test]
fn split_above_uses_independent_primary_and_menu_areas_with_one_command_activation() {
    for has_icon in [false, true] {
        let mut ui = Controls::new(Theme::FluentLight).unwrap();
        let command = CommandId::from("save");
        let alternate = CommandId::from("save_as");
        ui.commands_mut().register(command.clone(), "Save document");
        ui.commands_mut().register(alternate.clone(), "Save as");
        let split = ui
            .split_button(
                ui.root(),
                "Save document",
                Some(command.clone()),
                &[MenuEntry::Item(MenuItem::new("Save as", alternate))],
                layout(100.0, 88.0),
            )
            .unwrap();
        if has_icon {
            ui.set_button_icon(
                split.primary,
                Some(ButtonIcon {
                    symbol: IconSymbol::Save,
                    size: 32.0,
                    spacing: 4.0,
                }),
            )
            .unwrap();
        }
        ui.set_split_button_presentation(split, ButtonContentLayout::Above)
            .unwrap();
        frame(&mut ui);
        let primary = ui.runtime.bounds(split.primary).unwrap();
        let dropdown = ui.runtime.bounds(split.dropdown).unwrap();
        assert_eq!(primary.height, if has_icon { 40.0 } else { 68.0 });
        assert_eq!(primary.y + primary.height, dropdown.y);
        assert_eq!(primary.height + dropdown.height, 88.0);
        assert_eq!(
            ui.scene()
                .texts
                .iter()
                .filter(|run| run.text == "Save document")
                .count(),
            1
        );
        let position = center(&ui, split.primary);
        pointer(&mut ui, position, true);
        pointer(&mut ui, position, false);
        assert_eq!(ui.commands_mut().drain_invocations().len(), 1);
        let position = center(&ui, split.dropdown);
        pointer(&mut ui, position, true);
        pointer(&mut ui, position, false);
        assert!(ui.commands_mut().drain_invocations().is_empty());
        assert_eq!(ui.popup_expanded(split.dropdown), Some(true));
        key(&mut ui, Key::Escape, false);
        assert_eq!(ui.popup_expanded(split.dropdown), Some(false));
        assert_eq!(ui.runtime.focused(), Some(split.dropdown));
    }
}

#[test]
fn button_content_measure_rejects_invalid_requests_without_changing_controls() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let id = ui
        .button(ui.root(), "Unchanged", None, layout(120.0, 36.0))
        .unwrap();
    assert!(
        ui.measure_button_content(
            "text",
            &ButtonStyle::default(),
            None,
            ButtonContentLayout::Above,
            f32::NAN
        )
        .is_err()
    );
    assert!(
        ui.measure_button_content(
            "text",
            &ButtonStyle::default(),
            Some(ButtonIcon {
                size: f32::INFINITY,
                ..ButtonIcon::new(IconSymbol::Save)
            }),
            ButtonContentLayout::Above,
            100.0
        )
        .is_err()
    );
    let label = ui.label(ui.root(), "label", layout(120.0, 36.0)).unwrap();
    assert!(
        ui.set_button_content_layout(label, ButtonContentLayout::Above)
            .is_err()
    );
    assert!(ui.set_button_chrome(label, ButtonChrome::Flat).is_err());
    assert!(ui.set_key_tip(label, Some("A".into())).is_err());
    assert_eq!(
        ui.button_content_layout(id),
        Some(ButtonContentLayout::Leading)
    );
    assert_eq!(ui.button_chrome(id), Some(ButtonChrome::Standard));
    frame(&mut ui);
    assert_eq!(text(&ui, "Unchanged").text, "Unchanged");
}

#[test]
fn split_icon_only_matches_measure_and_preserves_primary_name_and_fallback() {
    for theme in [Theme::FluentLight, Theme::FluentDark, Theme::Compact] {
        for has_icon in [false, true] {
            let mut ui = Controls::new(theme).unwrap();
            ui.commands_mut().register("save", "Save document");
            let icon = has_icon.then(|| ButtonIcon::new(IconSymbol::Save));
            let measured = ui
                .measure_split_button_content(
                    "Save document",
                    &ButtonStyle::default(),
                    icon,
                    ButtonContentLayout::IconOnly,
                    512.0,
                )
                .unwrap();
            let split = ui
                .split_button(
                    ui.root(),
                    "Save document",
                    Some("save".into()),
                    &[MenuEntry::Item(MenuItem::new("Save", "save"))],
                    layout(measured.width, measured.height),
                )
                .unwrap();
            ui.set_button_icon(split.primary, icon).unwrap();
            ui.set_split_button_presentation(split, ButtonContentLayout::IconOnly)
                .unwrap();
            frame(&mut ui);
            let primary = ui.runtime.bounds(split.primary).unwrap();
            let arrow = ui.runtime.bounds(split.dropdown).unwrap();
            assert!((primary.x + primary.width - arrow.x).abs() < 0.001);
            assert!((primary.width + arrow.width - measured.width).abs() < 0.001);
            assert_eq!(
                ui.semantics()
                    .into_iter()
                    .find(|node| node.id == split.primary)
                    .unwrap()
                    .label,
                "Save document"
            );
            assert_eq!(
                ui.scene()
                    .texts
                    .iter()
                    .any(|run| run.text == "Save document"),
                !has_icon
            );
            ui.activate(split.primary).unwrap();
            assert_eq!(ui.commands_mut().drain_invocations().len(), 1);
            ui.activate(split.dropdown).unwrap();
            assert!(ui.commands_mut().drain_invocations().is_empty());
            assert!(ui.popup_open());
        }
    }
}

#[test]
fn split_footer_caption_aligns_with_above_buttons_instead_of_centering_its_row() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let icon = ButtonIcon {
        symbol: IconSymbol::Save,
        size: 32.0,
        spacing: 4.0,
    };
    let regular = ui
        .button(ui.root(), "Save", None, layout(100.0, 88.0))
        .unwrap();
    ui.set_button_icon(regular, Some(icon)).unwrap();
    ui.set_button_content_layout(regular, ButtonContentLayout::Above)
        .unwrap();
    ui.set_button_style(
        regular,
        ButtonStyle {
            padding: Some(Edges::all(4.0)),
            ..Default::default()
        },
    )
    .unwrap();
    ui.commands_mut().register("export_more", "Export options");
    let split = ui
        .split_button(
            ui.root(),
            "Export",
            None,
            &[MenuEntry::Item(MenuItem::new(
                "Export options",
                "export_more",
            ))],
            layout(100.0, 88.0),
        )
        .unwrap();
    ui.set_button_icon(split.primary, Some(icon)).unwrap();
    ui.set_split_button_presentation(split, ButtonContentLayout::Above)
        .unwrap();
    frame(&mut ui);
    let regular_offset = text(&ui, "Save").bounds.y - ui.runtime.bounds(regular).unwrap().y;
    let split_offset = text(&ui, "Export").bounds.y - ui.runtime.bounds(split.container).unwrap().y;
    assert_eq!(regular_offset, 40.0);
    assert_eq!(split_offset, 44.0);
}

fn has_shape_color(ui: &Controls, color: Color) -> bool {
    ui.scene()
        .rectangles
        .iter()
        .any(|shape| shape.color == color)
        || ui
            .scene()
            .rounded_rectangles
            .iter()
            .any(|shape| shape.color == color)
}

#[test]
fn button_style_rejection_is_atomic_during_an_active_press() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let id = ui
        .toggle(ui.root(), "Keep", None, true, layout(180.0, 40.0))
        .unwrap();
    let saved = ButtonStyle {
        font_size: Some(18.0),
        padding: Some(Edges::all(7.0)),
        corner_radius: Some(6.0),
        border_width: Some(2.0),
        normal: ButtonAppearance {
            foreground: Some(Color::rgb(32, 48, 64)),
            ..Default::default()
        },
        ..Default::default()
    };
    ui.set_button_style(id, saved).unwrap();
    frame(&mut ui);
    let point = center(&ui, id);
    pointer(&mut ui, point, true);
    frame(&mut ui);
    let state = ui.button_visual_state(id).unwrap();
    let scene = ui.scene().clone();
    let bounds = ui.runtime.bounds(id);
    let mut invalid = vec![
        ButtonStyle {
            font_size: Some(0.0),
            ..saved
        },
        ButtonStyle {
            font_size: Some(513.0),
            ..saved
        },
        ButtonStyle {
            font_size: Some(f32::NAN),
            ..saved
        },
        ButtonStyle {
            padding: Some(Edges {
                left: -1.0,
                ..Edges::all(7.0)
            }),
            ..saved
        },
        ButtonStyle {
            padding: Some(Edges {
                bottom: f32::INFINITY,
                ..Edges::all(7.0)
            }),
            ..saved
        },
        ButtonStyle {
            corner_radius: Some(16385.0),
            ..saved
        },
        ButtonStyle {
            border_width: Some(-1.0),
            ..saved
        },
        ButtonStyle {
            border_width: Some(f32::NAN),
            ..saved
        },
    ];
    for appearance in 0..5 {
        for color in [
            Color {
                r: f32::NAN,
                g: 0.0,
                b: 0.0,
                a: 1.0,
            },
            Color {
                r: 0.0,
                g: 1.1,
                b: 0.0,
                a: 1.0,
            },
            Color {
                r: 0.0,
                g: 0.0,
                b: -0.1,
                a: 1.0,
            },
            Color {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: f32::INFINITY,
            },
        ] {
            let mut style = saved;
            let overrides = match appearance {
                0 => &mut style.normal,
                1 => &mut style.hovered,
                2 => &mut style.pressed,
                3 => &mut style.checked,
                _ => &mut style.disabled,
            };
            *overrides = ButtonAppearance {
                checkmark: Some(color),
                ..Default::default()
            };
            invalid.push(style);
        }
    }
    for style in invalid {
        assert!(ui.set_button_style(id, style).is_err());
        assert_eq!(ui.button_style(id), Some(&saved));
        assert_eq!(ui.button_visual_state(id), Some(state));
        assert_eq!(ui.runtime.bounds(id), bounds);
        assert_eq!(ui.runtime.captured(), Some(id));
        assert_eq!(ui.checked(id), Some(true));
        assert!(!ui.needs_update());
        assert_eq!(ui.scene(), &scene);
        assert!(ui.drain_activated().is_empty());
    }
}

#[test]
fn button_style_targets_only_button_kinds_and_never_a_recycled_widget() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let root = ui.root();
    let label = ui.label(root, "Label", layout(100.0, 24.0)).unwrap();
    let editor = ui
        .edit_box(root, "Text", "draft", layout(100.0, 36.0))
        .unwrap();
    let panel = ui.panel(root, layout(100.0, 40.0)).unwrap();
    for id in [root, label, editor, panel] {
        assert!(ui.set_button_style(id, ButtonStyle::default()).is_err());
        assert_eq!(ui.button_style(id), None);
        assert_eq!(ui.button_visual_state(id), None);
    }
    assert_eq!(ui.document(editor).unwrap().text(), "draft");
    let button = ui
        .button(root, "Button", None, layout(120.0, 36.0))
        .unwrap();
    let toggle = ui
        .toggle(root, "Toggle", None, false, layout(120.0, 36.0))
        .unwrap();
    let check = ui
        .checkbox(root, "Check", false, layout(120.0, 32.0))
        .unwrap();
    for id in [button, toggle, check] {
        ui.set_button_style(
            id,
            ButtonStyle {
                corner_radius: Some(9.0),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(ui.button_style(id).unwrap().corner_radius, Some(9.0));
    }
    ui.runtime.apply(Mutation::Remove(button)).unwrap();
    assert_eq!(ui.button_style(button), None);
    assert_eq!(ui.button_visual_state(button), None);
    assert!(ui.set_button_style(button, ButtonStyle::default()).is_err());
    let replacement = ui
        .button(root, "Replacement", None, layout(120.0, 36.0))
        .unwrap();
    assert_ne!(replacement, button);
    assert_eq!(ui.button_style(replacement), Some(&ButtonStyle::default()));
}

#[test]
fn button_overrides_survive_theme_changes_and_reset_to_current_defaults() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let id = ui
        .button(ui.root(), "Themed", None, layout(160.0, 40.0))
        .unwrap();
    let background = Color::rgb(43, 67, 91);
    let foreground = Color::rgb(210, 220, 230);
    let style = ButtonStyle {
        normal: ButtonAppearance {
            background: Some(background),
            foreground: Some(foreground),
            ..Default::default()
        },
        ..Default::default()
    };
    ui.set_button_style(id, style).unwrap();
    for theme in [
        Theme::FluentLight,
        Theme::FluentDark,
        Theme::Light,
        Theme::Dark,
        Theme::Compact,
    ] {
        ui.set_theme(theme).unwrap();
        frame(&mut ui);
        assert_eq!(body_color(&ui, id), background);
        assert_eq!(text(&ui, "Themed").color, foreground);
        assert_eq!(text(&ui, "Themed").font_size, theme.tokens().font_size);
        assert!(has_shape_color(&ui, theme.tokens().border));
        assert_eq!(ui.button_style(id), Some(&style));
    }
    ui.set_button_style(id, ButtonStyle::default()).unwrap();
    frame(&mut ui);
    assert_eq!(body_color(&ui, id), Theme::Compact.tokens().surface);
    assert_eq!(text(&ui, "Themed").color, Theme::Compact.tokens().text);
}

#[test]
fn checked_hover_pressed_and_focus_are_independent_and_disabled_has_priority() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let id = ui
        .toggle(ui.root(), "States", None, true, layout(160.0, 40.0))
        .unwrap();
    let normal = Color::rgb(21, 31, 41);
    let checked = Color::rgb(51, 61, 71);
    let hovered = Color::rgb(81, 91, 101);
    let pressed = Color::rgb(111, 121, 131);
    let disabled = Color::rgb(141, 151, 161);
    let accent = Color::rgb(220, 21, 180);
    ui.set_button_style(
        id,
        ButtonStyle {
            normal: ButtonAppearance {
                background: Some(normal),
                accent: Some(accent),
                ..Default::default()
            },
            checked: ButtonAppearance {
                background: Some(checked),
                ..Default::default()
            },
            hovered: ButtonAppearance {
                background: Some(hovered),
                ..Default::default()
            },
            pressed: ButtonAppearance {
                background: Some(pressed),
                ..Default::default()
            },
            disabled: ButtonAppearance {
                background: Some(disabled),
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .unwrap();
    frame(&mut ui);
    focus(&mut ui, id);
    frame(&mut ui);
    let state = ui.button_visual_state(id).unwrap();
    assert!(state.enabled && state.checked && state.focused);
    assert!(!state.hovered && !state.pressed);
    assert_eq!(body_color(&ui, id), checked);
    assert!(has_shape_color(&ui, accent));
    let point = center(&ui, id);
    move_pointer(&mut ui, point);
    frame(&mut ui);
    let state = ui.button_visual_state(id).unwrap();
    assert!(state.checked && state.hovered && state.focused);
    assert_eq!(body_color(&ui, id), hovered);
    pointer(&mut ui, point, true);
    frame(&mut ui);
    let state = ui.button_visual_state(id).unwrap();
    assert!(state.checked && state.hovered && state.pressed && state.focused);
    assert_eq!(body_color(&ui, id), pressed);
    assert!(has_shape_color(&ui, accent));
    ui.set_enabled(id, false).unwrap();
    frame(&mut ui);
    let state = ui.button_visual_state(id).unwrap();
    assert!(!state.enabled && !state.hovered && !state.pressed && !state.focused);
    assert!(state.checked);
    assert_eq!(body_color(&ui, id), disabled);
    assert_eq!(text(&ui, "States").color, ui.theme().tokens().muted);
    assert!(!has_shape_color(&ui, accent));
    pointer(&mut ui, point, false);
    assert!(ui.drain_activated().is_empty());
    assert_eq!(ui.checked(id), Some(true));
}

#[test]
fn command_and_ancestor_disabling_cancel_the_press_without_latching_local_state() {
    let mut ui = Controls::new(Theme::FluentDark).unwrap();
    ui.commands_mut().register("run", "Run");
    let panel = ui
        .transparent_panel(ui.root(), layout(200.0, 150.0))
        .unwrap();
    let button = ui
        .button(panel, "Run", Some("run".into()), layout(160.0, 36.0))
        .unwrap();
    frame(&mut ui);
    let point = center(&ui, button);
    for by_command in [true, false] {
        pointer(&mut ui, point, true);
        assert!(ui.button_visual_state(button).unwrap().pressed);
        if by_command {
            ui.commands_mut().set_enabled(&"run".into(), false);
        } else {
            ui.set_enabled(panel, false).unwrap();
        }
        frame(&mut ui);
        let state = ui.button_visual_state(button).unwrap();
        assert!(!state.enabled && !state.pressed && !state.focused && !state.hovered);
        let semantics = ui.semantics();
        let node = semantics.iter().find(|node| node.id == button).unwrap();
        assert!(node.disabled && node.actions.is_empty());
        assert_eq!(body_color(&ui, button), ui.theme().tokens().canvas);
        assert_eq!(text(&ui, "Run").color, ui.theme().tokens().muted);
        pointer(&mut ui, point, false);
        ui.perform_action(button, SemanticAction::Activate, None)
            .unwrap();
        assert!(ui.commands_mut().drain_invocations().is_empty());
        if by_command {
            ui.commands_mut().set_enabled(&"run".into(), true);
        } else {
            ui.set_enabled(panel, true).unwrap();
        }
        frame(&mut ui);
        assert!(ui.button_visual_state(button).unwrap().enabled);
        assert!(ui.runtime.tree().node(button).unwrap().props.enabled);
    }
}

#[test]
fn pointer_drag_out_back_and_release_outside_preserve_capture_but_cancel_activation() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    ui.commands_mut().register("run", "Run");
    let id = ui
        .button(ui.root(), "Run", Some("run".into()), layout(160.0, 36.0))
        .unwrap();
    frame(&mut ui);
    let inside = center(&ui, id);
    let outside = Point::new(570.0, 440.0);
    pointer(&mut ui, inside, true);
    assert!(ui.button_visual_state(id).unwrap().pressed);
    move_pointer(&mut ui, outside);
    assert_eq!(ui.runtime.captured(), Some(id));
    assert!(!ui.button_visual_state(id).unwrap().pressed);
    move_pointer(&mut ui, inside);
    assert!(ui.button_visual_state(id).unwrap().pressed);
    pointer(&mut ui, outside, false);
    assert_eq!(ui.runtime.captured(), None);
    assert!(!ui.button_visual_state(id).unwrap().pressed);
    assert!(ui.commands_mut().drain_invocations().is_empty());
    pointer(&mut ui, inside, true);
    ui.dispatch(InputEvent::WindowFocused(false)).unwrap();
    pointer(&mut ui, inside, false);
    assert!(!ui.button_visual_state(id).unwrap().pressed);
    assert!(ui.commands_mut().drain_invocations().is_empty());
    ui.dispatch(InputEvent::WindowFocused(true)).unwrap();
    pointer(&mut ui, inside, true);
    pointer(&mut ui, inside, false);
    assert_eq!(ui.commands_mut().drain_invocations().len(), 1);
}

#[test]
fn space_activates_on_release_enter_once_and_focus_transfer_cancels_keyboard_press() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    ui.commands_mut().register("run", "Run");
    let first = ui
        .button(ui.root(), "Run", Some("run".into()), layout(140.0, 36.0))
        .unwrap();
    let second = ui
        .button(ui.root(), "Other", None, layout(140.0, 36.0))
        .unwrap();
    frame(&mut ui);
    focus(&mut ui, first);
    key(&mut ui, Key::Space, false);
    key(&mut ui, Key::Space, true);
    assert!(ui.button_visual_state(first).unwrap().pressed);
    assert!(ui.commands_mut().drain_invocations().is_empty());
    key_up(&mut ui, Key::Space);
    assert!(!ui.button_visual_state(first).unwrap().pressed);
    assert_eq!(ui.commands_mut().drain_invocations().len(), 1);
    key_up(&mut ui, Key::Space);
    assert!(ui.commands_mut().drain_invocations().is_empty());
    key(&mut ui, Key::Enter, false);
    key(&mut ui, Key::Enter, true);
    key_up(&mut ui, Key::Enter);
    assert_eq!(ui.commands_mut().drain_invocations().len(), 1);
    key(&mut ui, Key::Space, false);
    focus(&mut ui, second);
    frame(&mut ui);
    assert!(!ui.button_visual_state(first).unwrap().pressed);
    key_up(&mut ui, Key::Space);
    assert!(ui.commands_mut().drain_invocations().is_empty());
}

#[test]
fn rounded_button_keeps_original_geometry_clips_and_transparent_border_interior() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let panel = ui.transparent_panel(ui.root(), layout(80.0, 30.0)).unwrap();
    let id = ui
        .button(panel, "Clipped", None, layout(120.0, 40.0))
        .unwrap();
    let border = Color::rgb(181, 71, 39);
    let clear = Color {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 0.0,
    };
    ui.set_button_style(
        id,
        ButtonStyle {
            corner_radius: Some(16384.0),
            border_width: Some(16384.0),
            normal: ButtonAppearance {
                background: Some(clear),
                border: Some(border),
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .unwrap();
    frame(&mut ui);
    let bounds = ui.runtime.bounds(id).unwrap();
    let clip = ui.runtime.clip(id).unwrap();
    assert_eq!(bounds.width, 120.0);
    assert!(clip.width <= 80.0 && clip.height <= 30.0);
    let shapes: Vec<_> = ui
        .scene()
        .rounded_rectangles
        .iter()
        .filter(|shape| shape.bounds == bounds)
        .collect();
    assert!(
        shapes
            .iter()
            .all(|shape| shape.is_valid() && shape.clip == clip)
    );
    let stroke = shapes.iter().find(|shape| shape.color == border).unwrap();
    assert_eq!(stroke.radius, 20.0);
    assert_eq!(stroke.stroke_width, Some(20.0));
    assert!(
        !shapes
            .iter()
            .any(|shape| shape.color == border && shape.stroke_width.is_none())
    );
    assert!(
        !ui.scene()
            .rectangles
            .iter()
            .any(|shape| shape.color == border && shape.bounds == bounds)
    );
    let fill_index = ui
        .scene()
        .commands()
        .iter()
        .position(|command| match command {
            DrawCommand::RoundedRectangle(index) => {
                let shape = ui.scene().rounded_rectangles[*index];
                shape.bounds == bounds && shape.stroke_width.is_none()
            }
            _ => false,
        })
        .unwrap();
    let border_index = ui
        .scene()
        .commands()
        .iter()
        .position(|command| match command {
            DrawCommand::RoundedRectangle(index) => {
                ui.scene().rounded_rectangles[*index].color == border
            }
            _ => false,
        })
        .unwrap();
    assert!(fill_index < border_index);
}

#[test]
fn button_content_metrics_preserve_widget_size_and_color_only_updates_skip_layout() {
    let mut ui = Controls::new(Theme::FluentDark).unwrap();
    let id = ui
        .button(ui.root(), "Metrics", None, layout(220.0, 64.0))
        .unwrap();
    let style = ButtonStyle {
        font_size: Some(22.0),
        padding: Some(Edges {
            left: 13.0,
            top: 7.0,
            right: 17.0,
            bottom: 9.0,
        }),
        corner_radius: Some(8.0),
        ..Default::default()
    };
    ui.set_button_style(id, style).unwrap();
    frame(&mut ui);
    let bounds = ui.runtime.bounds(id).unwrap();
    let run = text(&ui, "Metrics");
    assert_eq!(run.font_size, 22.0);
    assert_eq!(run.bounds.x, bounds.x + 13.0);
    assert_eq!(run.bounds.width, bounds.width - 30.0);
    assert!(run.bounds.y >= bounds.y + 7.0);
    assert!(run.bounds.y + run.bounds.height <= bounds.y + bounds.height - 9.0);
    let color = Color::rgb(172, 191, 212);
    ui.set_button_style(
        id,
        ButtonStyle {
            normal: ButtonAppearance {
                foreground: Some(color),
                ..Default::default()
            },
            ..style
        },
    )
    .unwrap();
    let delta = ui.update(Size::new(600.0, 480.0)).unwrap();
    assert_eq!(delta.layout_passes, 0);
    assert_eq!(ui.runtime.bounds(id), Some(bounds));
    assert_eq!(text(&ui, "Metrics").color, color);
    assert_eq!(text(&ui, "Metrics").font_size, 22.0);
}

#[test]
fn shared_toggle_commands_preserve_initial_registry_and_invert_current_value_without_frames() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let command = CommandId::from("shared");
    ui.commands_mut().register(command.clone(), "Shared");
    ui.commands_mut().set_checked(&command, Some(true));
    let first = ui
        .toggle(
            ui.root(),
            "First",
            Some(command.clone()),
            false,
            layout(120.0, 36.0),
        )
        .unwrap();
    let second = ui
        .toggle(
            ui.root(),
            "Second",
            Some(command.clone()),
            false,
            layout(120.0, 36.0),
        )
        .unwrap();
    assert_eq!(ui.commands().checked(&command), Some(true));
    assert_eq!(ui.checked(first), Some(true));
    assert_eq!(ui.checked(second), Some(true));
    frame(&mut ui);
    ui.activate(first).unwrap();
    assert_eq!(ui.commands().checked(&command), Some(false));
    for id in [first, second] {
        assert_eq!(
            ui.semantics()
                .iter()
                .find(|node| node.id == id)
                .unwrap()
                .checked,
            Some(false)
        );
    }
    ui.activate(second).unwrap();
    assert_eq!(ui.commands().checked(&command), Some(true));
    for id in [first, second] {
        assert_eq!(
            ui.semantics()
                .iter()
                .find(|node| node.id == id)
                .unwrap()
                .checked,
            Some(true)
        );
    }
    assert_eq!(
        ui.commands_mut().drain_invocations(),
        vec![
            CommandInvocation {
                id: command.clone(),
                checked: Some(false)
            },
            CommandInvocation {
                id: command.clone(),
                checked: Some(true)
            },
        ]
    );
    frame(&mut ui);
    for id in [first, second] {
        assert_eq!(ui.checked(id), Some(true));
        assert!(ui.button_visual_state(id).unwrap().checked);
        assert_eq!(
            ui.semantics()
                .iter()
                .find(|node| node.id == id)
                .unwrap()
                .checked,
            Some(true)
        );
    }
}

#[test]
fn fluent_checkbox_paints_a_compact_indicator_and_distinct_checkmark_without_filling_its_row() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let id = ui
        .checkbox(ui.root(), "Check label", true, layout(200.0, 32.0))
        .unwrap();
    let indicator = Color::rgb(45, 95, 145);
    let checkmark = Color::rgb(250, 231, 47);
    ui.set_button_style(
        id,
        ButtonStyle {
            checked: ButtonAppearance {
                background: Some(indicator),
                checkmark: Some(checkmark),
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .unwrap();
    for theme in [Theme::FluentLight, Theme::FluentDark] {
        ui.set_theme(theme).unwrap();
        frame(&mut ui);
        let bounds = ui.runtime.bounds(id).unwrap();
        assert!(
            !ui.scene()
                .rectangles
                .iter()
                .any(|shape| shape.bounds == bounds)
        );
        assert!(
            !ui.scene()
                .rounded_rectangles
                .iter()
                .any(|shape| shape.bounds == bounds && shape.stroke_width.is_none())
        );
        let fill = ui
            .scene()
            .rounded_rectangles
            .iter()
            .find(|shape| shape.color == indicator && shape.stroke_width.is_none())
            .unwrap();
        assert!(fill.bounds.width > 0.0 && fill.bounds.width <= 18.0);
        assert_eq!(fill.bounds.width, fill.bounds.height);
        assert!(
            fill.bounds.x >= bounds.x
                && fill.bounds.x + fill.bounds.width <= bounds.x + bounds.width
        );
        assert!(
            fill.bounds.y >= bounds.y
                && fill.bounds.y + fill.bounds.height <= bounds.y + bounds.height
        );
        assert!(has_shape_color(&ui, checkmark));
        assert_eq!(text(&ui, "Check label").color, theme.tokens().text);
        assert_eq!(
            ui.semantics()
                .iter()
                .find(|node| node.id == id)
                .unwrap()
                .checked,
            Some(true)
        );
    }
    ui.set_checked(id, false).unwrap();
    frame(&mut ui);
    assert!(!has_shape_color(&ui, checkmark));
}

#[test]
fn huge_corner_radius_and_zero_border_keep_a_valid_focus_indicator() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let id = ui
        .button(ui.root(), "Focus", None, layout(120.0, 36.0))
        .unwrap();
    let accent = Color::rgb(190, 35, 150);
    ui.set_button_style(
        id,
        ButtonStyle {
            corner_radius: Some(16384.0),
            border_width: Some(0.0),
            normal: ButtonAppearance {
                accent: Some(accent),
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .unwrap();
    frame(&mut ui);
    focus(&mut ui, id);
    frame(&mut ui);
    let bounds = ui.runtime.bounds(id).unwrap();
    let focus: Vec<_> = ui
        .scene()
        .rounded_rectangles
        .iter()
        .filter(|shape| shape.color == accent)
        .collect();
    assert!(!focus.is_empty());
    assert!(
        focus
            .iter()
            .all(|shape| shape.is_valid() && shape.stroke_width.is_some_and(|width| width > 0.0))
    );
    assert!(focus.iter().all(|shape| shape.bounds.width > 0.0
        && shape.bounds.height > 0.0
        && shape.bounds.width <= bounds.width
        && shape.bounds.height <= bounds.height));
    ui.runtime.apply(Mutation::Focus(None)).unwrap();
    frame(&mut ui);
    assert!(!has_shape_color(&ui, accent));
}

#[test]
fn reenabled_commands_take_effect_before_a_frame_for_controls_and_ancestors() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let gate = CommandId::from("gate");
    let run = CommandId::from("run");
    ui.commands_mut().register(gate.clone(), "Gate");
    ui.commands_mut().register(run.clone(), "Run");
    let parent = ui
        .button(
            ui.root(),
            "Gate",
            Some(gate.clone()),
            LayoutStyle {
                kind: LayoutKind::Stack(Axis::Vertical),
                ..layout(200.0, 100.0)
            },
        )
        .unwrap();
    let child = ui
        .button(parent, "Run", Some(run.clone()), layout(140.0, 36.0))
        .unwrap();
    frame(&mut ui);
    for command in [&run, &gate] {
        ui.commands_mut().set_enabled(command, false);
        frame(&mut ui);
        assert!(!ui.button_visual_state(child).unwrap().enabled);
        ui.activate(child).unwrap();
        assert!(ui.commands_mut().drain_invocations().is_empty());

        // The last paint still contains disabled props; current policy must win.
        ui.commands_mut().set_enabled(command, true);
        assert!(ui.button_visual_state(child).unwrap().enabled);
        assert!(
            !ui.semantics()
                .iter()
                .find(|node| node.id == child)
                .unwrap()
                .disabled
        );
        ui.activate(child).unwrap();
        assert_eq!(
            ui.commands_mut().drain_invocations(),
            vec![CommandInvocation {
                id: run.clone(),
                checked: None
            }]
        );
        frame(&mut ui);
    }

    // Registry enablement never overrides the ancestor's explicit local policy.
    ui.set_enabled(parent, false).unwrap();
    ui.commands_mut().set_enabled(&gate, true);
    assert!(!ui.button_visual_state(child).unwrap().enabled);
    ui.activate(child).unwrap();
    assert!(ui.commands_mut().drain_invocations().is_empty());
}

#[test]
fn space_press_is_not_restored_when_focus_returns_before_release() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    ui.commands_mut().register("run", "Run");
    let first = ui
        .button(ui.root(), "Run", Some("run".into()), layout(140.0, 36.0))
        .unwrap();
    let second = ui
        .button(ui.root(), "Other", None, layout(140.0, 36.0))
        .unwrap();
    frame(&mut ui);
    for observed_by_frame in [true, false] {
        focus(&mut ui, first);
        key(&mut ui, Key::Space, false);
        assert!(ui.button_visual_state(first).unwrap().pressed);
        focus(&mut ui, second);
        if observed_by_frame {
            frame(&mut ui);
        } else {
            move_pointer(&mut ui, Point::new(570.0, 440.0));
        }
        focus(&mut ui, first);
        key_up(&mut ui, Key::Space);
        assert!(!ui.button_visual_state(first).unwrap().pressed);
        assert!(ui.commands_mut().drain_invocations().is_empty());
    }
}

#[test]
fn escape_cancels_pointer_capture_and_space_press_without_activation() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    ui.commands_mut().register("run", "Run");
    let id = ui
        .button(ui.root(), "Run", Some("run".into()), layout(140.0, 36.0))
        .unwrap();
    frame(&mut ui);
    let point = center(&ui, id);
    pointer(&mut ui, point, true);
    assert_eq!(ui.runtime.captured(), Some(id));
    key(&mut ui, Key::Escape, false);
    assert_eq!(ui.runtime.captured(), None);
    assert!(!ui.button_visual_state(id).unwrap().pressed);
    pointer(&mut ui, point, false);
    assert!(ui.commands_mut().drain_invocations().is_empty());

    focus(&mut ui, id);
    key(&mut ui, Key::Space, false);
    assert!(ui.button_visual_state(id).unwrap().pressed);
    key(&mut ui, Key::Escape, false);
    assert!(!ui.button_visual_state(id).unwrap().pressed);
    key_up(&mut ui, Key::Space);
    assert!(ui.commands_mut().drain_invocations().is_empty());
}

#[test]
fn tiny_focused_buttons_and_oversized_padding_never_emit_negative_geometry() {
    for checkbox in [false, true] {
        let mut ui = Controls::new(Theme::FluentLight).unwrap();
        let panel = ui.transparent_panel(ui.root(), layout(0.5, 0.5)).unwrap();
        let id = if checkbox {
            ui.checkbox(panel, "Tiny", true, layout(1.0, 1.0)).unwrap()
        } else {
            ui.button(panel, "Tiny", None, layout(1.0, 1.0)).unwrap()
        };
        let accent = Color::rgb(181, 20, 90);
        ui.set_button_style(
            id,
            ButtonStyle {
                padding: Some(Edges::all(16384.0)),
                corner_radius: Some(16384.0),
                border_width: Some(16384.0),
                normal: ButtonAppearance {
                    accent: Some(accent),
                    ..Default::default()
                },
                ..Default::default()
            },
        )
        .unwrap();
        frame(&mut ui);
        focus(&mut ui, id);
        frame(&mut ui);
        assert!(ui.button_visual_state(id).unwrap().focused);
        assert!(!has_shape_color(&ui, accent));
        for shape in &ui.scene().rounded_rectangles {
            assert!(shape.is_valid());
            assert!(shape.clip.width <= 0.5 && shape.clip.height <= 0.5);
        }
        for shape in &ui.scene().rectangles {
            assert!(shape.bounds.is_valid());
            assert!(shape.bounds.width >= 0.0 && shape.bounds.height >= 0.0);
            if shape.bounds == ui.runtime.bounds(ui.root()).unwrap() {
                continue;
            }
            assert!(shape.bounds.width <= 0.5 && shape.bounds.height <= 0.5);
        }
        for run in &ui.scene().texts {
            assert!(run.bounds.is_valid() && run.clip.is_valid());
            assert!(run.bounds.width >= 0.0 && run.bounds.height >= 0.0);
            assert!(run.clip.width <= 0.5 && run.clip.height <= 0.5);
        }
    }
}
