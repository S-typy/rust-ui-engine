use crate::*;
use rust_desktop_ui_core::{
    Axis, Color, DrawCommand, Edges, InputEvent, Key, LayoutKind, LayoutStyle, Length, Modifiers,
    Mutation, Point, PointerButton, Rect, Scene, Size, TextAlign, TextRun, WidgetId,
};

fn style(width: f32, height: f32) -> LayoutStyle {
    LayoutStyle {
        width: Length::Px(width),
        height: Length::Px(height),
        flex_shrink: 0.0,
        ..Default::default()
    }
}
fn ui() -> Controls {
    Controls::new(Theme::Light).unwrap()
}
fn frame(ui: &mut Controls) {
    ui.update(Size::new(600.0, 480.0)).unwrap();
}
fn center(ui: &Controls, id: WidgetId) -> Point {
    let r = ui.runtime.bounds(id).unwrap();
    Point::new(r.x + r.width * 0.5, r.y + r.height * 0.5)
}
fn pointer(ui: &mut Controls, position: Point, down: bool) {
    let event = if down {
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
    };
    ui.dispatch(event).unwrap();
}
fn key(ui: &mut Controls, key: Key, modifiers: Modifiers) {
    ui.dispatch(InputEvent::KeyDown {
        key,
        modifiers,
        repeat: false,
    })
    .unwrap();
}
fn focus(ui: &mut Controls, id: WidgetId) {
    ui.runtime.apply(Mutation::Focus(Some(id))).unwrap();
}

#[test]
fn transparent_panels_preserve_clipping_semantics_and_the_background_below_them() {
    let mut ui = ui();
    let surface = ui.panel(ui.root(), style(240.0, 160.0)).unwrap();
    let panel = ui.transparent_panel(surface, style(200.0, 120.0)).unwrap();
    let child = ui
        .button(panel, "Action", None, style(100.0, 36.0))
        .unwrap();
    for theme in [Theme::FluentLight, Theme::FluentDark] {
        ui.set_theme(theme).unwrap();
        for enabled in [false, true] {
            ui.set_enabled(panel, enabled).unwrap();
            frame(&mut ui);
            let bounds = ui.runtime.bounds(panel).unwrap();
            assert!(
                !ui.scene()
                    .rectangles
                    .iter()
                    .any(|rect| rect.bounds == bounds)
            );
            let surface_bounds = ui.runtime.bounds(surface).unwrap();
            assert!(ui.scene().rectangles.iter().any(|rect| {
                rect.bounds == surface_bounds && rect.color == theme.tokens().surface
            }));
            assert!(ui.runtime.tree().node(panel).unwrap().props.clip);
            let semantics = ui.semantics();
            let node = semantics.iter().find(|node| node.id == panel).unwrap();
            assert_eq!(node.role, Role::Panel);
            assert_eq!(node.children, vec![child]);
            assert_eq!(node.disabled, !enabled);
            assert_eq!(
                semantics
                    .iter()
                    .find(|node| node.id == child)
                    .unwrap()
                    .disabled,
                !enabled
            );
        }
    }
}

#[test]
fn disabled_container_paints_descendants_without_latching_their_enabled_state() {
    let mut ui = ui();
    let parent = ui.panel(ui.root(), style(220.0, 180.0)).unwrap();
    let inner = ui.panel(parent, style(200.0, 150.0)).unwrap();
    let button = ui
        .button(inner, "Action", None, style(140.0, 36.0))
        .unwrap();
    let toggle = ui
        .toggle(inner, "Selected", None, true, style(140.0, 36.0))
        .unwrap();
    let checkbox = ui
        .checkbox(inner, "Check", true, style(140.0, 32.0))
        .unwrap();
    for theme in [Theme::FluentLight, Theme::FluentDark] {
        ui.set_theme(theme).unwrap();
        ui.set_enabled(parent, false).unwrap();
        frame(&mut ui);
        let tokens = theme.tokens();
        for id in [button, toggle, checkbox] {
            let node = ui.runtime.tree().node(id).unwrap();
            assert!(node.props.enabled, "inherited disabled is not local state");
            assert_eq!(node.props.paint.background, Some(tokens.canvas));
            assert_eq!(node.props.paint.hover_background, None);
            assert!(ui.semantics().iter().find(|n| n.id == id).unwrap().disabled);
            let bounds = ui.runtime.bounds(id).unwrap();
            let shape = if id == checkbox {
                Rect::new(
                    bounds.x + 8.0,
                    bounds.y + (bounds.height - 18.0) * 0.5,
                    18.0,
                    18.0,
                )
            } else {
                bounds
            };
            assert!(ui.scene().rounded_rectangles.iter().any(|r| {
                r.bounds == shape && r.stroke_width.is_none() && r.color == tokens.canvas
            }));
        }
        ui.set_enabled(parent, true).unwrap();
        frame(&mut ui);
        assert_eq!(
            ui.runtime
                .tree()
                .node(button)
                .unwrap()
                .props
                .paint
                .background,
            Some(tokens.surface)
        );
        for id in [toggle, checkbox] {
            assert_eq!(
                ui.runtime.tree().node(id).unwrap().props.paint.background,
                Some(tokens.selected)
            );
            assert_eq!(ui.checked(id), Some(true));
            assert!(!ui.semantics().iter().find(|n| n.id == id).unwrap().disabled);
        }
    }
}

#[test]
fn button_activates_on_release_inside_and_keyboard_space_release() {
    let mut ui = ui();
    ui.commands_mut().register("save", "Save");
    let id = ui
        .button(ui.root(), "Save", Some("save".into()), style(140.0, 36.0))
        .unwrap();
    frame(&mut ui);
    let point = center(&ui, id);
    pointer(&mut ui, point, true);
    assert_eq!(ui.runtime.captured(), Some(id));
    assert!(ui.commands_mut().drain_invocations().is_empty());
    pointer(&mut ui, Point::new(590.0, 450.0), false);
    assert!(ui.commands_mut().drain_invocations().is_empty());
    pointer(&mut ui, point, true);
    pointer(&mut ui, point, false);
    assert_eq!(ui.commands_mut().drain_invocations().len(), 1);
    key(&mut ui, Key::Space, Modifiers::default());
    assert!(ui.commands_mut().drain_invocations().is_empty());
    ui.dispatch(InputEvent::KeyUp {
        key: Key::Space,
        modifiers: Modifiers::default(),
    })
    .unwrap();
    assert_eq!(ui.commands_mut().drain_invocations().len(), 1);
    key(&mut ui, Key::Enter, Modifiers::default());
    assert_eq!(ui.commands_mut().drain_invocations().len(), 1);
}

#[test]
fn disabled_commands_and_disabled_ancestors_block_semantic_activation() {
    let mut ui = ui();
    ui.commands_mut().register("save", "Save");
    let parent = ui
        .panel(
            ui.root(),
            LayoutStyle {
                kind: LayoutKind::Stack(Axis::Vertical),
                ..style(200.0, 100.0)
            },
        )
        .unwrap();
    let id = ui
        .button(parent, "Save", Some("save".into()), style(140.0, 36.0))
        .unwrap();
    frame(&mut ui);
    ui.commands_mut().set_enabled(&"save".into(), false);
    frame(&mut ui);
    ui.perform_action(id, SemanticAction::Activate, None)
        .unwrap();
    assert!(ui.commands_mut().drain_invocations().is_empty());
    let semantic = ui.semantics().into_iter().find(|n| n.id == id).unwrap();
    assert!(semantic.disabled);
    assert!(semantic.actions.is_empty());
    ui.commands_mut().set_enabled(&"save".into(), true);
    ui.set_enabled(parent, false).unwrap();
    frame(&mut ui);
    ui.activate(id).unwrap();
    assert!(ui.commands_mut().drain_invocations().is_empty());
    assert!(ui.semantics().iter().find(|n| n.id == id).unwrap().disabled);
}

#[test]
fn toggle_checkbox_and_themes_keep_retained_identity() {
    let mut ui = ui();
    ui.commands_mut().register("bold", "Bold");
    let toggle = ui
        .toggle(
            ui.root(),
            "Bold",
            Some("bold".into()),
            false,
            LayoutStyle::default(),
        )
        .unwrap();
    let check = ui
        .checkbox(ui.root(), "Enabled", false, LayoutStyle::default())
        .unwrap();
    frame(&mut ui);
    ui.activate(toggle).unwrap();
    ui.activate(check).unwrap();
    assert_eq!(ui.checked(toggle), Some(true));
    assert_eq!(ui.checked(check), Some(true));
    assert_eq!(ui.commands().checked(&"bold".into()), Some(true));
    for theme in [Theme::Dark, Theme::Compact, Theme::Light] {
        ui.set_theme(theme).unwrap();
        frame(&mut ui);
        assert!(ui.runtime.tree().node(toggle).is_some());
        assert_eq!(ui.checked(toggle), Some(true));
    }
    let stats = ui.stats();
    let idle = ui.update(Size::new(600.0, 480.0)).unwrap();
    assert_eq!(idle.layout_passes, 0);
    assert_eq!(idle.paint_passes, 0);
    assert_eq!(ui.stats(), stats);
}

#[test]
fn tab_pages_hide_and_remove_their_focus_candidates() {
    let mut ui = ui();
    let tabs = ui
        .tab_control(ui.root(), &["First", "Second"], style(400.0, 220.0))
        .unwrap();
    let first = ui
        .button(tabs.pages[0], "First action", None, style(180.0, 32.0))
        .unwrap();
    let second = ui
        .button(tabs.pages[1], "Second action", None, style(180.0, 32.0))
        .unwrap();
    frame(&mut ui);
    assert!(ui.runtime.bounds(first).is_some());
    assert!(ui.runtime.bounds(second).is_none());
    focus(&mut ui, first);
    ui.select_tab(tabs.container, 1).unwrap();
    frame(&mut ui);
    assert_eq!(ui.runtime.focused(), None);
    assert!(ui.runtime.bounds(first).is_none());
    assert!(ui.runtime.bounds(second).is_some());
    focus(&mut ui, tabs.tabs[1]);
    key(&mut ui, Key::ArrowLeft, Modifiers::default());
    assert_eq!(ui.selected_tab(tabs.container), Some(0));
}

#[test]
fn scroll_clamps_on_resize_and_does_not_relayout_on_wheel() {
    let mut ui = ui();
    let viewer = ui
        .scroll_view(ui.root(), style(300.0, 100.0), 800.0)
        .unwrap();
    ui.label(viewer.content, "Scrollable content", style(250.0, 32.0))
        .unwrap();
    frame(&mut ui);
    let layout = ui.stats().layout_passes;
    ui.dispatch(InputEvent::Wheel {
        position: Point::new(40.0, 70.0),
        delta: Point::new(0.0, 80.0),
        modifiers: Modifiers::default(),
    })
    .unwrap();
    frame(&mut ui);
    assert_eq!(
        ui.runtime
            .tree()
            .node(viewer.viewport)
            .unwrap()
            .props
            .scroll
            .y,
        80.0
    );
    assert_eq!(ui.stats().layout_passes, layout);
    ui.set_scroll_extent(viewer.viewport, 40.0).unwrap();
    frame(&mut ui);
    assert_eq!(
        ui.runtime
            .tree()
            .node(viewer.viewport)
            .unwrap()
            .props
            .scroll
            .y,
        0.0
    );
}

#[test]
fn popup_keyboard_skips_disabled_items_toggles_and_restores_focus() {
    let mut ui = ui();
    ui.commands_mut().register("off", "Unavailable");
    ui.commands_mut().set_enabled(&"off".into(), false);
    ui.commands_mut().register("flag", "Flag");
    let anchor = ui
        .button(ui.root(), "Menu", None, style(140.0, 34.0))
        .unwrap();
    frame(&mut ui);
    focus(&mut ui, anchor);
    ui.popup_menu(
        anchor,
        &[
            MenuItem::new("Unavailable", "off"),
            MenuItem::new("Flag", "flag").toggling(),
        ],
    )
    .unwrap();
    let focused = ui.runtime.focused().unwrap();
    assert_ne!(focused, anchor);
    assert_eq!(
        ui.semantics()
            .iter()
            .find(|n| n.id == focused)
            .unwrap()
            .label,
        "Flag"
    );
    key(&mut ui, Key::Enter, Modifiers::default());
    assert!(!ui.popup_open());
    assert_eq!(ui.runtime.focused(), Some(anchor));
    assert_eq!(ui.commands().checked(&"flag".into()), Some(true));
    assert_eq!(ui.commands_mut().drain_invocations().len(), 1);
}

#[test]
fn textbox_unicode_ime_clipboard_and_undo_are_document_operations() {
    let mut ui = ui();
    let id = ui
        .text_box(ui.root(), "Editor", "Привет 👩‍💻", style(300.0, 110.0))
        .unwrap();
    frame(&mut ui);
    focus(&mut ui, id);
    key(&mut ui, Key::Backspace, Modifiers::default());
    assert_eq!(ui.document(id).unwrap().text(), "Привет ");
    let ctrl = Modifiers {
        control: true,
        ..Default::default()
    };
    key(&mut ui, Key::Character("z".into()), ctrl);
    assert_eq!(ui.document(id).unwrap().text(), "Привет 👩‍💻");
    key(&mut ui, Key::Character("a".into()), ctrl);
    key(&mut ui, Key::Character("c".into()), ctrl);
    assert_eq!(
        ui.drain_clipboard_requests(),
        vec![ClipboardRequest::Copy("Привет 👩‍💻".into())]
    );
    ui.ime_preedit("日本", Some((6, 6))).unwrap();
    frame(&mut ui);
    assert_eq!(ui.document(id).unwrap().text(), "Привет 👩‍💻");
    key(&mut ui, Key::Enter, Modifiers::default());
    assert_eq!(ui.document(id).unwrap().text(), "Привет 👩‍💻");
    ui.ime_commit("日本").unwrap();
    frame(&mut ui);
    assert_eq!(ui.document(id).unwrap().text(), "日本");
    assert!(ui.caret_bounds().unwrap().is_valid());
    key(&mut ui, Key::Character("v".into()), ctrl);
    assert_eq!(ui.drain_clipboard_requests(), vec![ClipboardRequest::Paste]);
    ui.text_input(" русский").unwrap();
    assert_eq!(ui.document(id).unwrap().text(), "日本 русский");
    assert!(ui.blink_caret());
    frame(&mut ui);
}

#[test]
fn textbox_hit_testing_and_bidi_visual_arrows_remain_on_graphemes() {
    let mut ui = ui();
    let id = ui
        .text_box(ui.root(), "Mixed", "abc אבג 👩‍💻", style(340.0, 110.0))
        .unwrap();
    frame(&mut ui);
    focus(&mut ui, id);
    key(&mut ui, Key::Home, Modifiers::default());
    for _ in 0..15 {
        key(&mut ui, Key::ArrowRight, Modifiers::default());
        frame(&mut ui);
        let doc = ui.document(id).unwrap();
        assert!(doc.grapheme_boundaries().contains(&doc.selection().focus));
    }
    let p = center(&ui, id);
    pointer(&mut ui, p, true);
    pointer(&mut ui, p, false);
    let doc = ui.document(id).unwrap();
    assert!(doc.grapheme_boundaries().contains(&doc.selection().focus));
}

#[test]
fn custom_scene_and_text_are_ordered_below_popup_backgrounds() {
    let mut ui = ui();
    let anchor = ui
        .button(ui.root(), "Menu", None, style(140.0, 34.0))
        .unwrap();
    let custom = ui.panel(ui.root(), style(400.0, 200.0)).unwrap();
    ui.commands_mut().register("act", "Action");
    frame(&mut ui);
    let mut scene = Scene::default();
    scene.text(TextRun {
        bounds: Rect::new(20.0, 70.0, 250.0, 40.0),
        clip: Rect::new(0.0, 0.0, 600.0, 480.0),
        text: "CUSTOM".into(),
        font_size: 18.0,
        color: Color::rgb(0, 0, 0),
        weight: 400,
        align: TextAlign::Start,
        wrap: false,
    });
    ui.set_custom_scene(custom, scene).unwrap();
    ui.popup_menu(anchor, &[MenuItem::new("Action", "act")])
        .unwrap();
    frame(&mut ui);
    let commands = ui.scene().commands();
    let custom_index = commands
        .iter()
        .position(|c| matches!(c,DrawCommand::Text(i) if ui.scene().texts[*i].text=="CUSTOM"))
        .unwrap();
    let menu_index = commands
        .iter()
        .position(|c| matches!(c,DrawCommand::Text(i) if ui.scene().texts[*i].text=="Action"))
        .unwrap();
    assert!(custom_index < menu_index);
    assert!(
        commands[custom_index + 1..menu_index]
            .iter()
            .any(|c| matches!(c, DrawCommand::Rectangle(_)))
    );
}

#[test]
fn ribbon_keeps_parent_order_on_resize_and_keytips_reach_overflow_commands() {
    let mut ui = ui();
    for (id, label) in [("new", "New"), ("save", "Save"), ("flag", "Flag")] {
        ui.commands_mut().register(id, label);
    }
    let groups = (0..3)
        .map(|n| RibbonGroup {
            id: format!("g{n}"),
            label: format!("Group {n}"),
            collapse_priority: n,
            items: vec![
                RibbonItem::new("new", RibbonItemKind::Large, format!("{n}N")),
                RibbonItem::new("save", RibbonItemKind::Small, format!("{n}S")),
                RibbonItem::new("flag", RibbonItemKind::Toggle, format!("{n}F")),
            ],
        })
        .collect();
    let mut ribbon = Ribbon::new(RibbonModel {
        tabs: vec![RibbonTab {
            id: "home".into(),
            label: "Home".into(),
            key_tip: "H".into(),
            context: None,
            groups,
        }],
        quick_access: vec!["save".into()],
    })
    .unwrap();
    let root = ui.root();
    let container = ribbon.mount(&mut ui, root).unwrap();
    let below = ui.label(root, "Below ribbon", style(100.0, 30.0)).unwrap();
    for width in [900.0, 450.0, 160.0] {
        ribbon.update(&mut ui, width).unwrap();
        frame(&mut ui);
        let children = ui.runtime.tree().children(root).unwrap();
        assert!(
            children.iter().position(|&id| id == container)
                < children.iter().position(|&id| id == below)
        );
    }
    for key in [
        Key::Alt,
        Key::Character("H".into()),
        Key::Character("0".into()),
        Key::Character("F".into()),
    ] {
        assert!(
            ribbon
                .handle_input(
                    &mut ui,
                    &InputEvent::KeyDown {
                        key,
                        modifiers: Modifiers::default(),
                        repeat: false
                    }
                )
                .unwrap()
        );
        ribbon.update(&mut ui, 160.0).unwrap();
        frame(&mut ui);
    }
    assert_eq!(ui.commands().checked(&"flag".into()), Some(true));
    assert_eq!(
        ui.commands_mut().drain_invocations().last().unwrap().id,
        CommandId::from("flag")
    );
}

#[test]
fn semantics_are_connected_and_exclude_hidden_tab_pages() {
    let mut ui = ui();
    let tabs = ui
        .tab_control(ui.root(), &["Visible", "Hidden"], style(350.0, 180.0))
        .unwrap();
    let hidden = ui
        .button(tabs.pages[1], "Hidden action", None, style(120.0, 30.0))
        .unwrap();
    frame(&mut ui);
    let nodes = ui.semantics();
    let by_id: std::collections::HashMap<_, _> = nodes.iter().map(|node| (node.id, node)).collect();
    assert!(!by_id.contains_key(&hidden));
    assert!(!by_id.contains_key(&tabs.pages[1]));
    assert_eq!(nodes.iter().filter(|node| node.parent.is_none()).count(), 1);
    assert_eq!(
        nodes.iter().find(|node| node.parent.is_none()).unwrap().id,
        ui.runtime.tree().root()
    );
    for node in &nodes {
        if let Some(parent) = node.parent {
            assert!(by_id[&parent].children.contains(&node.id));
        }
        for child in &node.children {
            assert_eq!(by_id[child].parent, Some(node.id));
        }
    }
}

#[test]
fn altgr_does_not_trigger_ctrl_shortcuts_or_ribbon_keytips() {
    let mut ui = ui();
    let id = ui
        .text_box(ui.root(), "Editor", "base", style(300.0, 90.0))
        .unwrap();
    frame(&mut ui);
    focus(&mut ui, id);
    let altgr = Modifiers {
        control: true,
        alt: true,
        ..Default::default()
    };
    key(&mut ui, Key::Character("a".into()), altgr);
    assert!(ui.document(id).unwrap().selection().is_empty());
    ui.text_input("ą").unwrap();
    assert_eq!(ui.document(id).unwrap().text(), "baseą");
    let mut ribbon = Ribbon::new(RibbonModel::default()).unwrap();
    assert!(
        !ribbon
            .handle_input(
                &mut ui,
                &InputEvent::KeyDown {
                    key: Key::Alt,
                    modifiers: altgr,
                    repeat: false
                }
            )
            .unwrap()
    );
    assert!(!ribbon.key_tips_visible());
}

#[test]
fn textbox_vertical_navigation_and_focus_change_cancel_composition() {
    let mut ui = ui();
    let first = ui
        .text_box(
            ui.root(),
            "First",
            "one\ntwo\nthree\nfour\nfive\nsix",
            style(300.0, 100.0),
        )
        .unwrap();
    let second = ui
        .text_box(ui.root(), "Second", "untouched", style(300.0, 90.0))
        .unwrap();
    frame(&mut ui);
    focus(&mut ui, first);
    let ctrl = Modifiers {
        control: true,
        ..Default::default()
    };
    key(&mut ui, Key::Home, ctrl);
    key(&mut ui, Key::ArrowDown, Modifiers::default());
    assert_eq!(ui.document(first).unwrap().selection().focus, 4);
    key(&mut ui, Key::ArrowUp, Modifiers::default());
    assert_eq!(ui.document(first).unwrap().selection().focus, 0);
    key(&mut ui, Key::PageDown, Modifiers::default());
    assert!(ui.document(first).unwrap().selection().focus >= 14);
    ui.ime_preedit("日本", Some((6, 6))).unwrap();
    frame(&mut ui);
    assert!(ui.document(first).unwrap().composition().is_some());
    key(&mut ui, Key::Tab, Modifiers::default());
    frame(&mut ui);
    assert_eq!(ui.runtime.focused(), Some(second));
    assert!(ui.document(first).unwrap().composition().is_none());
    assert_eq!(
        ui.document(first).unwrap().text(),
        "one\ntwo\nthree\nfour\nfive\nsix"
    );
}

#[test]
fn ribbon_rejects_ambiguous_keytips_and_reserved_qat_prefixes() {
    let model = |tips: &[&str], tabtip: &str| RibbonModel {
        tabs: vec![RibbonTab {
            id: "home".into(),
            label: "Home".into(),
            key_tip: tabtip.into(),
            context: None,
            groups: vec![RibbonGroup {
                id: "group".into(),
                label: "Group".into(),
                collapse_priority: 0,
                items: tips
                    .iter()
                    .map(|tip| RibbonItem::new("act", RibbonItemKind::Small, *tip))
                    .collect(),
            }],
        }],
        quick_access: Vec::new(),
    };
    assert!(Ribbon::new(model(&["A", "AB"], "H")).is_err());
    assert!(Ribbon::new(model(&["AB", "A"], "H")).is_err());
    assert!(Ribbon::new(model(&["A"], "1A")).is_err());
    assert!(Ribbon::new(model(&["A B"], "H")).is_err());
    assert!(Ribbon::new(model(&["AB", "AC"], "H")).is_ok());
}

#[test]
fn late_pointer_press_after_window_blur_does_not_capture_or_activate() {
    let mut ui = ui();
    let button = ui
        .button(ui.root(), "Action", None, style(140.0, 36.0))
        .unwrap();
    let editor = ui
        .text_box(ui.root(), "Editor", "text", style(300.0, 90.0))
        .unwrap();
    frame(&mut ui);
    ui.dispatch(InputEvent::WindowFocused(false)).unwrap();
    for id in [button, editor] {
        let position = center(&ui, id);
        pointer(&mut ui, position, true);
        pointer(&mut ui, position, false);
        assert_eq!(ui.runtime.captured(), None);
        assert_eq!(ui.runtime.focused(), None);
    }
    assert!(ui.drain_activated().is_empty());
}

#[test]
fn semantic_container_names_do_not_overpaint_content_and_explicit_labels_keep_their_bounds() {
    let mut ui = ui();
    let panel = ui.panel(ui.root(), style(300.0, 100.0)).unwrap();
    ui.set_label(panel, "Container semantics").unwrap();
    let label = ui.label(panel, "Файл", style(180.0, 20.0)).unwrap();
    frame(&mut ui);
    assert!(
        ui.semantics()
            .iter()
            .any(|node| node.id == panel && node.label == "Container semantics")
    );
    assert!(
        !ui.scene()
            .texts
            .iter()
            .any(|run| run.text == "Container semantics")
    );
    let run = ui
        .scene()
        .texts
        .iter()
        .find(|run| run.text == "Файл")
        .unwrap();
    let bounds = ui.runtime.bounds(label).unwrap();
    assert_eq!(bounds.height, 20.0);
    assert_eq!(run.bounds.y, bounds.y);
    assert_eq!(run.clip.height, 20.0);
}

#[test]
fn ribbon_metrics_fit_real_captions_and_small_buttons_in_every_theme() {
    let mut ui = ui();
    ui.commands_mut().register("action", "Действие");
    let groups = ["Файл", "Редактирование", "Длинная подпись группы"]
        .into_iter()
        .enumerate()
        .map(|(index, label)| RibbonGroup {
            id: format!("group{index}"),
            label: label.into(),
            collapse_priority: 0,
            items: (0..3)
                .map(|item| {
                    RibbonItem::new("action", RibbonItemKind::Small, format!("G{index}I{item}"))
                })
                .collect(),
        })
        .collect();
    let mut ribbon = Ribbon::new(RibbonModel {
        tabs: vec![RibbonTab {
            id: "home".into(),
            label: "Главная".into(),
            key_tip: "H".into(),
            context: None,
            groups,
        }],
        quick_access: vec!["action".into()],
    })
    .unwrap();
    let root = ui.root();
    let container = ribbon.mount(&mut ui, root).unwrap();
    let mut engine = rust_desktop_ui_text::TextEngine::new();
    for theme in [Theme::Light, Theme::Dark, Theme::Compact] {
        ui.set_theme(theme).unwrap();
        ribbon.update(&mut ui, 984.0).unwrap();
        ui.update(Size::new(1000.0, 600.0)).unwrap();
        assert!(
            ribbon
                .presentations()
                .iter()
                .all(|p| matches!(p, RibbonPresentation::Full(_)))
        );
        for run in &ui.scene().texts {
            let prepared = engine.prepare(run, 1.0).unwrap();
            assert!(
                prepared.height <= run.clip.height + 0.001,
                "{theme:?} {:?}: shaped {} must fit clip {}",
                run.text,
                prepared.height,
                run.clip.height
            );
        }
        let height = ui.runtime.bounds(container).unwrap().height;
        ribbon.update(&mut ui, 160.0).unwrap();
        ui.update(Size::new(1000.0, 600.0)).unwrap();
        assert_eq!(
            ui.runtime.bounds(container).unwrap().height,
            height,
            "Overflow must retain the measured theme height"
        );
    }
}

#[test]
fn themed_text_measurement_matches_paint_metrics_and_rejects_invalid_widths() {
    let mut ui = ui();
    assert!(ui.measure_text("text", f32::NAN, true).is_err());
    assert!(ui.measure_text("text", -1.0, true).is_err());
    assert!(
        ui.measure_text(&"x".repeat(1024 * 1024 + 1), 100.0, true)
            .is_err()
    );
    let text = "Пример длинной подписи";
    let measured = ui.measure_text(text, 104.0, true).unwrap();
    assert!(measured.height.is_finite() && measured.height > 0.0);
    ui.label(ui.root(), text, style(120.0, measured.height.ceil()))
        .unwrap();
    frame(&mut ui);
    let run = ui
        .scene()
        .texts
        .iter()
        .find(|run| run.text == text)
        .unwrap();
    let prepared = rust_desktop_ui_text::TextEngine::new()
        .prepare(run, 1.0)
        .unwrap();
    assert_eq!(prepared.height, measured.height);
    assert!(prepared.height <= run.clip.height);
}

#[test]
fn editbox_placeholder_is_presentation_and_keeps_empty_caret_mapping() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let id = ui
        .edit_box(
            ui.root(),
            "Имя",
            "",
            LayoutStyle {
                width: Length::Px(260.0),
                ..Default::default()
            },
        )
        .unwrap();
    ui.set_placeholder(id, "Введите имя").unwrap();
    frame(&mut ui);
    assert_eq!(ui.document(id).unwrap().text(), "");
    let semantics = ui
        .semantics()
        .into_iter()
        .find(|node| node.id == id)
        .unwrap();
    assert_eq!(semantics.label, "Имя");
    assert_eq!(semantics.value, Some(String::new()));
    assert!(ui.scene().texts.iter().any(|run| run.text == "Введите имя"));
    focus(&mut ui, id);
    frame(&mut ui);
    let bounds = ui.runtime.bounds(id).unwrap();
    let caret = ui.caret_bounds().unwrap();
    assert_eq!(bounds.height, 36.0);
    assert_eq!(caret.x, bounds.x + 12.0);
    assert!(caret.y >= bounds.y + 5.0 && caret.y + caret.height <= bounds.y + bounds.height - 5.0);
    let position = Point::new(bounds.x + 180.0, caret.y + caret.height * 0.5);
    pointer(&mut ui, position, true);
    pointer(&mut ui, position, false);
    assert_eq!(ui.document(id).unwrap().selection().focus, 0);
    ui.ime_preedit("日本", Some((6, 6))).unwrap();
    frame(&mut ui);
    assert!(!ui.scene().texts.iter().any(|run| run.text == "Введите имя"));
    assert_eq!(ui.document(id).unwrap().text(), "");
    ui.cancel_preedit();
    frame(&mut ui);
    assert!(ui.scene().texts.iter().any(|run| run.text == "Введите имя"));
    ui.text_input("Анна").unwrap();
    frame(&mut ui);
    assert!(!ui.scene().texts.iter().any(|run| run.text == "Введите имя"));
    assert_eq!(ui.document(id).unwrap().text(), "Анна");
}

#[test]
fn editbox_filters_single_line_inputs_without_changing_multiline_textbox() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let id = ui
        .edit_box(ui.root(), "Name", "a\r\nb\tc", style(260.0, 36.0))
        .unwrap();
    let multiline = ui
        .text_box(ui.root(), "Notes", "a\nb", style(260.0, 90.0))
        .unwrap();
    frame(&mut ui);
    focus(&mut ui, id);
    assert_eq!(ui.document(id).unwrap().text(), "abc");
    key(&mut ui, Key::Enter, Modifiers::default());
    ui.text_input("\tD\r\nE").unwrap();
    assert_eq!(ui.document(id).unwrap().text(), "abcDE");
    ui.set_text(id, "x\ny\tz\r").unwrap();
    assert_eq!(ui.document(id).unwrap().text(), "xyz");
    key(
        &mut ui,
        Key::Character("a".into()),
        Modifiers {
            control: true,
            ..Default::default()
        },
    );
    ui.text_input("\r\n\t").unwrap();
    assert_eq!(ui.document(id).unwrap().selected_text(), "xyz");
    ui.ime_preedit("a\nb", Some((3, 3))).unwrap();
    assert!(ui.document(id).unwrap().composition().is_none());
    ui.set_text(id, "").unwrap();
    ui.ime_commit("Я\n🙂\t").unwrap();
    assert_eq!(ui.document(id).unwrap().text(), "Я🙂");
    focus(&mut ui, multiline);
    key(&mut ui, Key::Enter, Modifiers::default());
    ui.text_input("c\td").unwrap();
    assert_eq!(ui.document(multiline).unwrap().text(), "a\nb\nc\td");
}

#[test]
fn fluent_editbox_uses_rounded_skin_and_bottom_focus_without_changing_identity() {
    let mut ui = Controls::new(Theme::Light).unwrap();
    let id = ui
        .edit_box(
            ui.root(),
            "Name",
            "Ada",
            LayoutStyle {
                width: Length::Px(240.0),
                ..Default::default()
            },
        )
        .unwrap();
    frame(&mut ui);
    assert!(ui.scene().rounded_rectangles.is_empty());
    for theme in [Theme::FluentLight, Theme::FluentDark] {
        ui.set_theme(theme).unwrap();
        focus(&mut ui, id);
        frame(&mut ui);
        let bounds = ui.runtime.bounds(id).unwrap();
        assert_eq!(bounds.height, 36.0);
        assert_eq!(ui.document(id).unwrap().text(), "Ada");
        let rounded = &ui.scene().rounded_rectangles;
        assert_eq!(rounded.len(), 2);
        assert_eq!(rounded[0].bounds, bounds);
        assert_eq!(rounded[0].radius, 4.0);
        assert_eq!(rounded[0].color, theme.chrome().editor_fill);
        assert_eq!(rounded[0].stroke_width, None);
        assert_eq!(rounded[1].bounds, bounds);
        assert_eq!(rounded[1].radius, 4.0);
        assert_eq!(rounded[1].color, theme.chrome().control_border);
        assert_eq!(rounded[1].stroke_width, Some(1.0));
        let rounded_commands: Vec<_> = ui
            .scene()
            .commands()
            .iter()
            .filter(|command| matches!(command, DrawCommand::RoundedRectangle(_)))
            .copied()
            .collect();
        assert_eq!(
            rounded_commands,
            vec![
                DrawCommand::RoundedRectangle(0),
                DrawCommand::RoundedRectangle(1)
            ]
        );
        assert!(ui.scene().rectangles.iter().any(|r| r.bounds
            == Rect::new(bounds.x + 4.0, bounds.y + 34.0, bounds.width - 8.0, 2.0)
            && r.color == theme.tokens().focus));
        let run = ui
            .scene()
            .texts
            .iter()
            .find(|run| run.text == "Ada")
            .unwrap();
        assert_eq!(run.font_size, 14.0);
        assert!(!run.wrap);
    }
    ui.set_theme(Theme::Light).unwrap();
    frame(&mut ui);
    assert!(ui.scene().rounded_rectangles.is_empty());
    assert_eq!(ui.runtime.bounds(id).unwrap().height, 34.0);
}

#[test]
fn editbox_horizontal_scroll_preserves_caret_hit_testing_and_clip() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let text = "0123456789 ".repeat(20);
    let id = ui
        .edit_box(ui.root(), "Long value", &text, style(200.0, 36.0))
        .unwrap();
    frame(&mut ui);
    focus(&mut ui, id);
    frame(&mut ui);
    let bounds = ui.runtime.bounds(id).unwrap();
    let caret = ui.caret_bounds().unwrap();
    assert!(caret.x + caret.width <= bounds.x + bounds.width - 12.0 + 0.01);
    assert!(caret.x >= bounds.x + 12.0);
    let run = ui
        .scene()
        .texts
        .iter()
        .find(|run| run.text == text)
        .unwrap();
    assert!(run.bounds.x < bounds.x);
    assert_eq!(run.clip.x, bounds.x + 12.0);
    key(&mut ui, Key::Home, Modifiers::default());
    frame(&mut ui);
    assert_eq!(ui.caret_bounds().unwrap().x, bounds.x + 12.0);
    key(&mut ui, Key::End, Modifiers::default());
    frame(&mut ui);
    let caret = ui.caret_bounds().unwrap();
    let position = Point::new(bounds.x + 13.0, caret.y + caret.height * 0.5);
    pointer(&mut ui, position, true);
    pointer(&mut ui, position, false);
    assert!(ui.document(id).unwrap().selection().focus < text.len());
    key(
        &mut ui,
        Key::Character("a".into()),
        Modifiers {
            control: true,
            ..Default::default()
        },
    );
    frame(&mut ui);
    for rect in ui
        .scene()
        .rectangles
        .iter()
        .filter(|r| r.color == Theme::FluentLight.tokens().selected)
    {
        assert!(rect.bounds.x >= bounds.x + 12.0);
        assert!(rect.bounds.x + rect.bounds.width <= bounds.x + bounds.width - 12.0);
    }
}

#[test]
fn editbox_disable_and_hide_revoke_caret_and_cancel_composition() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let id = ui
        .edit_box(ui.root(), "Name", "Ada", style(200.0, 36.0))
        .unwrap();
    frame(&mut ui);
    focus(&mut ui, id);
    ui.ime_preedit("日本", Some((6, 6))).unwrap();
    frame(&mut ui);
    assert!(ui.caret_bounds().is_some());
    ui.set_enabled(id, false).unwrap();
    assert!(ui.caret_bounds().is_none());
    ui.text_input("ignored").unwrap();
    ui.ime_commit("ignored").unwrap();
    frame(&mut ui);
    assert_eq!(ui.document(id).unwrap().text(), "Ada");
    assert!(ui.document(id).unwrap().composition().is_none());
    assert!(!ui.blink_caret());

    ui.set_enabled(id, true).unwrap();
    focus(&mut ui, id);
    ui.ime_preedit("日本", Some((6, 6))).unwrap();
    frame(&mut ui);
    let mut props = ui.runtime.tree().node(id).unwrap().props.clone();
    props.visible = false;
    ui.runtime
        .apply(Mutation::SetProps { node: id, props })
        .unwrap();
    assert!(ui.caret_bounds().is_none());
    ui.ime_commit("ignored").unwrap();
    frame(&mut ui);
    assert_eq!(ui.document(id).unwrap().text(), "Ada");
    assert!(ui.document(id).unwrap().composition().is_none());
    assert!(ui.scene().rounded_rectangles.is_empty());
    assert!(!ui.blink_caret());
}

#[test]
fn editbox_rtl_horizontal_scroll_keeps_both_document_ends_visible() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let text = "שלום עולם ".repeat(20);
    let id = ui
        .edit_box(ui.root(), "RTL", &text, style(200.0, 36.0))
        .unwrap();
    frame(&mut ui);
    focus(&mut ui, id);
    for key_value in [Key::Home, Key::End, Key::Home] {
        key(&mut ui, key_value, Modifiers::default());
        frame(&mut ui);
        let bounds = ui.runtime.bounds(id).unwrap();
        let caret = ui.caret_bounds().unwrap();
        assert!(
            caret.x >= bounds.x + 12.0
                && caret.x + caret.width <= bounds.x + bounds.width - 12.0 + 0.01,
            "RTL caret {caret:?} must stay inside the padded editor {bounds:?}"
        );
    }
}

fn control_modifier() -> Modifiers {
    Modifiers {
        control: true,
        ..Default::default()
    }
}

#[test]
fn readonly_editor_keeps_navigation_copy_and_focus_but_blocks_user_mutations() {
    let mut ui = ui();
    let id = ui
        .edit_box(ui.root(), "Account", "Ada", style(260.0, 36.0))
        .unwrap();
    frame(&mut ui);
    focus(&mut ui, id);
    ui.text_input("!").unwrap();
    ui.drain_text_changes();
    ui.set_read_only(id, true).unwrap();
    assert_eq!(ui.is_read_only(id), Some(true));
    assert!(!ui.is_text_editable(id));
    assert_eq!(ui.runtime.focused(), Some(id));

    key(&mut ui, Key::Home, Modifiers::default());
    assert_eq!(ui.document(id).unwrap().selection().focus, 0);
    key(&mut ui, Key::ArrowRight, Modifiers::default());
    assert_eq!(ui.document(id).unwrap().selection().focus, 1);
    key(&mut ui, Key::Character("a".into()), control_modifier());
    key(&mut ui, Key::Character("c".into()), control_modifier());
    assert_eq!(
        ui.drain_clipboard_requests(),
        vec![ClipboardRequest::Copy("Ada!".into())]
    );
    let selection = ui.document(id).unwrap().selection();
    for key_value in [Key::Backspace, Key::Delete, Key::Enter] {
        key(&mut ui, key_value, Modifiers::default());
    }
    for shortcut in ["x", "v", "z", "y"] {
        key(&mut ui, Key::Character(shortcut.into()), control_modifier());
    }
    ui.text_input("replacement").unwrap();
    ui.ime_preedit("日本", Some((6, 6))).unwrap();
    ui.ime_commit("日本").unwrap();
    ui.perform_action(id, SemanticAction::SetValue, Some("replacement"))
        .unwrap();
    assert_eq!(ui.document(id).unwrap().text(), "Ada!");
    assert_eq!(ui.document(id).unwrap().selection(), selection);
    assert!(ui.document(id).unwrap().composition().is_none());
    assert!(ui.document(id).unwrap().can_undo());
    assert!(!ui.document(id).unwrap().can_redo());
    assert!(ui.drain_clipboard_requests().is_empty());
    assert!(ui.drain_text_changes().is_empty());

    let semantic = ui.semantics().into_iter().find(|n| n.id == id).unwrap();
    assert!(semantic.read_only);
    assert!(!semantic.disabled);
    assert!(semantic.actions.contains(&SemanticAction::Focus));
    assert!(!semantic.actions.contains(&SemanticAction::SetValue));

    ui.set_read_only(id, false).unwrap();
    assert!(ui.is_text_editable(id));
    key(&mut ui, Key::Character("z".into()), control_modifier());
    assert_eq!(ui.document(id).unwrap().text(), "Ada");
}

#[test]
fn readonly_transition_cancels_preedit_and_programmatic_updates_remain_available() {
    let mut ui = ui();
    let id = ui
        .edit_box(ui.root(), "Account", "Ada", style(260.0, 36.0))
        .unwrap();
    frame(&mut ui);
    focus(&mut ui, id);
    key(&mut ui, Key::Character("a".into()), control_modifier());
    let selection = ui.document(id).unwrap().selection();
    ui.ime_preedit("日本", Some((6, 6))).unwrap();
    ui.set_read_only(id, true).unwrap();
    assert!(ui.document(id).unwrap().composition().is_none());
    assert_eq!(ui.document(id).unwrap().selection(), selection);
    assert_eq!(ui.document(id).unwrap().text(), "Ada");
    assert!(!ui.document(id).unwrap().can_undo());
    assert!(ui.drain_text_changes().is_empty());

    ui.set_text(id, "Grace").unwrap();
    assert_eq!(ui.document(id).unwrap().text(), "Grace");
    ui.set_enabled(id, false).unwrap();
    ui.set_text(id, "Lin").unwrap();
    assert_eq!(ui.document(id).unwrap().text(), "Lin");
    assert!(!ui.is_text_editable(id));
    let changes = ui.drain_text_changes();
    assert_eq!(changes.len(), 2);
    assert_eq!((changes[0].id, changes[0].text.as_str()), (id, "Grace"));
    assert_eq!((changes[1].id, changes[1].text.as_str()), (id, "Lin"));
}

#[test]
fn maxlength_counts_resulting_graphemes_and_replacement_not_inserted_bytes() {
    let mut ui = ui();
    let id = ui
        .edit_box(ui.root(), "Name", "e", style(260.0, 36.0))
        .unwrap();
    frame(&mut ui);
    focus(&mut ui, id);
    assert_eq!(ui.max_length(id), Some(None));
    ui.set_max_length(id, Some(1)).unwrap();
    assert_eq!(ui.max_length(id), Some(Some(1)));
    ui.text_input("\u{301}").unwrap();
    assert_eq!(ui.document(id).unwrap().text(), "e\u{301}");
    assert_eq!(ui.document(id).unwrap().selection().focus, 3);
    ui.drain_text_changes();

    let selection = ui.document(id).unwrap().selection();
    ui.text_input("🙂").unwrap();
    assert_eq!(ui.document(id).unwrap().text(), "e\u{301}");
    assert_eq!(ui.document(id).unwrap().selection(), selection);
    assert!(ui.drain_text_changes().is_empty());

    key(&mut ui, Key::Character("a".into()), control_modifier());
    key(&mut ui, Key::Character("v".into()), control_modifier());
    assert_eq!(ui.drain_clipboard_requests(), vec![ClipboardRequest::Paste]);
    ui.text_input("👩‍💻").unwrap();
    assert_eq!(ui.document(id).unwrap().text(), "👩‍💻");
    assert_eq!(ui.document(id).unwrap().selection().focus, "👩‍💻".len());

    key(&mut ui, Key::Character("a".into()), control_modifier());
    ui.drain_text_changes();
    let selection = ui.document(id).unwrap().selection();
    assert!(ui.set_text(id, "AB").is_err());
    assert!(ui.set_max_length(id, Some(0)).is_err());
    assert_eq!(ui.max_length(id), Some(Some(1)));
    assert_eq!(ui.document(id).unwrap().text(), "👩‍💻");
    assert_eq!(ui.document(id).unwrap().selection(), selection);
    assert!(ui.drain_text_changes().is_empty());
    key(&mut ui, Key::Character("z".into()), control_modifier());
    assert_eq!(ui.document(id).unwrap().text(), "e\u{301}");
}

#[test]
fn maxlength_rejection_preserves_redo_and_zero_means_no_committed_graphemes() {
    let mut ui = ui();
    let id = ui
        .edit_box(ui.root(), "Code", "", style(260.0, 36.0))
        .unwrap();
    frame(&mut ui);
    focus(&mut ui, id);
    ui.text_input("ABC").unwrap();
    key(&mut ui, Key::Character("z".into()), control_modifier());
    ui.set_max_length(id, Some(0)).unwrap();
    ui.drain_text_changes();
    let selection = ui.document(id).unwrap().selection();
    ui.text_input("x").unwrap();
    assert!(ui.set_text(id, "x").is_err());
    key(&mut ui, Key::Character("y".into()), control_modifier());
    assert_eq!(ui.document(id).unwrap().text(), "");
    assert_eq!(ui.document(id).unwrap().selection(), selection);
    assert!(ui.document(id).unwrap().can_redo());
    assert!(!ui.document(id).unwrap().can_undo());
    assert!(ui.drain_text_changes().is_empty());
    ui.set_max_length(id, None).unwrap();
    key(&mut ui, Key::Character("y".into()), control_modifier());
    assert_eq!(ui.document(id).unwrap().text(), "ABC");
}

#[test]
fn lowering_maxlength_rejects_oversized_undo_without_losing_history() {
    let mut ui = ui();
    let id = ui
        .edit_box(ui.root(), "Code", "long", style(260.0, 36.0))
        .unwrap();
    frame(&mut ui);
    focus(&mut ui, id);
    ui.set_text(id, "x").unwrap();
    // The cancellation baseline must also fit a newly tightened input limit.
    ui.commit_edit(id).unwrap();
    ui.set_max_length(id, Some(1)).unwrap();
    ui.drain_text_changes();
    let selection = ui.document(id).unwrap().selection();
    key(&mut ui, Key::Character("z".into()), control_modifier());
    assert_eq!(ui.document(id).unwrap().text(), "x");
    assert_eq!(ui.document(id).unwrap().selection(), selection);
    assert!(ui.document(id).unwrap().can_undo());
    assert!(!ui.document(id).unwrap().can_redo());
    assert!(ui.drain_text_changes().is_empty());
    ui.set_max_length(id, None).unwrap();
    key(&mut ui, Key::Character("z".into()), control_modifier());
    assert_eq!(ui.document(id).unwrap().text(), "long");
    key(&mut ui, Key::Character("y".into()), control_modifier());
    assert_eq!(ui.document(id).unwrap().text(), "x");
}

#[test]
fn ime_draft_can_exceed_maxlength_but_rejected_commit_preserves_selection_and_history() {
    let mut ui = ui();
    let id = ui
        .edit_box(ui.root(), "Code", "a", style(260.0, 36.0))
        .unwrap();
    frame(&mut ui);
    focus(&mut ui, id);
    ui.set_max_length(id, Some(1)).unwrap();
    key(&mut ui, Key::Character("a".into()), control_modifier());
    let selection = ui.document(id).unwrap().selection();
    ui.ime_preedit("nihon", Some((5, 5))).unwrap();
    assert_eq!(ui.document(id).unwrap().display_text(), "nihon");
    assert_eq!(ui.document(id).unwrap().text(), "a");
    assert!(ui.drain_text_changes().is_empty());
    assert!(ui.set_text(id, "too long").is_err());
    assert_eq!(ui.document(id).unwrap().display_text(), "nihon");
    assert_eq!(ui.document(id).unwrap().selection(), selection);
    ui.ime_commit("日本").unwrap();
    assert!(ui.document(id).unwrap().composition().is_none());
    assert_eq!(ui.document(id).unwrap().text(), "a");
    assert_eq!(ui.document(id).unwrap().selection(), selection);
    assert!(!ui.document(id).unwrap().can_undo());
    assert!(ui.drain_text_changes().is_empty());

    ui.ime_preedit("ni", Some((2, 2))).unwrap();
    ui.ime_commit("日").unwrap();
    assert_eq!(ui.document(id).unwrap().text(), "日");
    let changes = ui.drain_text_changes();
    assert_eq!(changes.len(), 1);
    assert_eq!((changes[0].id, changes[0].text.as_str()), (id, "日"));
    key(&mut ui, Key::Character("z".into()), control_modifier());
    assert_eq!(ui.document(id).unwrap().text(), "a");
    assert!(!ui.document(id).unwrap().can_undo());
}

#[test]
fn single_line_filtered_only_input_and_ime_commit_do_not_delete_selection() {
    let mut ui = ui();
    let id = ui
        .edit_box(ui.root(), "Code", "keep", style(260.0, 36.0))
        .unwrap();
    frame(&mut ui);
    focus(&mut ui, id);
    key(&mut ui, Key::Character("a".into()), control_modifier());
    let selection = ui.document(id).unwrap().selection();
    let separators = "\r\n\t\u{85}\u{2028}\u{2029}";
    ui.text_input(separators).unwrap();
    ui.ime_preedit("draft", Some((5, 5))).unwrap();
    ui.ime_commit(separators).unwrap();
    assert_eq!(ui.document(id).unwrap().text(), "keep");
    assert_eq!(ui.document(id).unwrap().selection(), selection);
    assert!(ui.document(id).unwrap().composition().is_none());
    assert!(!ui.document(id).unwrap().can_undo());
    assert!(ui.drain_text_changes().is_empty());

    for separator in ['\r', '\n', '\t', '\u{85}', '\u{2028}', '\u{2029}'] {
        ui.ime_preedit("draft", Some((5, 5))).unwrap();
        let preedit = format!("a{separator}b");
        ui.ime_preedit(&preedit, None).unwrap();
        assert!(ui.document(id).unwrap().composition().is_none());
        assert_eq!(ui.document(id).unwrap().text(), "keep");
        assert_eq!(ui.document(id).unwrap().selection(), selection);
    }
    ui.text_input("A\u{85}B\u{2028}C\u{2029}D").unwrap();
    assert_eq!(ui.document(id).unwrap().text(), "ABCD");
}

#[test]
fn text_changed_reports_each_actual_value_change_with_editor_identity() {
    let mut ui = ui();
    let first = ui
        .edit_box(ui.root(), "First", "a", style(260.0, 36.0))
        .unwrap();
    let second = ui
        .edit_box(ui.root(), "Second", "x", style(260.0, 36.0))
        .unwrap();
    frame(&mut ui);
    assert!(ui.drain_text_changes().is_empty());
    focus(&mut ui, first);
    ui.set_text(first, "a").unwrap();
    key(&mut ui, Key::End, Modifiers::default());
    ui.ime_preedit("draft", Some((5, 5))).unwrap();
    ui.cancel_preedit();
    assert!(ui.drain_text_changes().is_empty());

    ui.text_input("b").unwrap();
    key(&mut ui, Key::Backspace, Modifiers::default());
    key(&mut ui, Key::Character("z".into()), control_modifier());
    key(&mut ui, Key::Character("y".into()), control_modifier());
    ui.set_text(second, "y").unwrap();
    ui.perform_action(second, SemanticAction::SetValue, Some("z"))
        .unwrap();
    ui.perform_action(second, SemanticAction::SetValue, Some("z"))
        .unwrap();
    key(&mut ui, Key::Character("a".into()), control_modifier());
    key(&mut ui, Key::Character("x".into()), control_modifier());
    assert_eq!(
        ui.drain_clipboard_requests(),
        vec![ClipboardRequest::Copy("a".into())]
    );
    let changes: Vec<_> = ui
        .drain_text_changes()
        .into_iter()
        .map(|change| (change.id, change.text))
        .collect();
    assert_eq!(
        changes,
        vec![
            (first, "ab".into()),
            (first, "a".into()),
            (first, "ab".into()),
            (first, "a".into()),
            (second, "y".into()),
            (second, "z".into()),
            (first, "".into()),
        ]
    );
    assert!(ui.drain_text_changes().is_empty());
}

#[test]
fn oversized_editor_input_is_rejected_before_document_or_widget_changes() {
    let mut ui = ui();
    let id = ui
        .edit_box(ui.root(), "Code", "keep", style(260.0, 36.0))
        .unwrap();
    frame(&mut ui);
    focus(&mut ui, id);
    key(&mut ui, Key::Character("a".into()), control_modifier());
    let selection = ui.document(id).unwrap().selection();
    let oversized = "x".repeat(1024 * 1024 + 1);
    ui.text_input(&oversized).unwrap();
    assert!(ui.set_text(id, &oversized).is_err());
    ui.ime_preedit(&oversized, None).unwrap();
    assert!(ui.document(id).unwrap().composition().is_none());
    ui.ime_preedit("draft", Some((5, 5))).unwrap();
    ui.ime_commit(&oversized).unwrap();
    assert!(ui.document(id).unwrap().composition().is_none());
    assert_eq!(ui.document(id).unwrap().text(), "keep");
    assert_eq!(ui.document(id).unwrap().selection(), selection);
    assert!(!ui.document(id).unwrap().can_undo());
    assert!(ui.drain_text_changes().is_empty());
    let nodes = ui.runtime.tree().preorder();
    assert!(
        ui.edit_box(ui.root(), "Too large", &oversized, style(260.0, 36.0))
            .is_err()
    );
    assert!(
        ui.text_box(ui.root(), "Too large", &oversized, style(260.0, 90.0))
            .is_err()
    );
    assert_eq!(ui.runtime.tree().preorder(), nodes);
    frame(&mut ui);
}

#[test]
fn accessible_value_changes_obey_limits_and_readonly_without_losing_existing_text() {
    let mut ui = ui();
    let id = ui
        .edit_box(ui.root(), "Code", "A", style(260.0, 36.0))
        .unwrap();
    frame(&mut ui);
    ui.set_max_length(id, Some(1)).unwrap();
    ui.perform_action(id, SemanticAction::SetValue, Some("AB"))
        .unwrap();
    assert_eq!(ui.document(id).unwrap().text(), "A");
    assert!(!ui.document(id).unwrap().can_undo());
    assert!(ui.drain_text_changes().is_empty());
    ui.perform_action(id, SemanticAction::SetValue, Some("🙂"))
        .unwrap();
    assert_eq!(ui.document(id).unwrap().text(), "🙂");
    ui.drain_text_changes();
    ui.set_read_only(id, true).unwrap();
    ui.perform_action(id, SemanticAction::SetValue, Some("B"))
        .unwrap();
    assert_eq!(ui.document(id).unwrap().text(), "🙂");
    assert!(ui.drain_text_changes().is_empty());
}

#[test]
fn rejected_input_keeps_visual_caret_at_bidi_boundaries() {
    let mut ui = ui();
    let id = ui
        .edit_box(ui.root(), "Mixed", "abc אבג", style(260.0, 36.0))
        .unwrap();
    ui.set_max_length(id, Some(7)).unwrap();
    frame(&mut ui);
    focus(&mut ui, id);
    for direction in [Key::ArrowLeft, Key::ArrowRight] {
        for _ in 0..10 {
            key(&mut ui, direction.clone(), Modifiers::default());
            frame(&mut ui);
            let selection = ui.document(id).unwrap().selection();
            let caret = ui.caret_bounds().unwrap();
            ui.text_input("x").unwrap();
            frame(&mut ui);
            assert_eq!(ui.document(id).unwrap().selection(), selection);
            assert_eq!(ui.caret_bounds().unwrap(), caret);
        }
    }
    assert_eq!(ui.document(id).unwrap().text(), "abc אבג");
    assert!(ui.drain_text_changes().is_empty());
}

#[test]
fn readonly_copy_and_blocked_shortcuts_keep_visual_caret_at_bidi_boundaries() {
    let mut ui = ui();
    let id = ui
        .edit_box(ui.root(), "Mixed", "abc אבג", style(260.0, 36.0))
        .unwrap();
    ui.set_read_only(id, true).unwrap();
    frame(&mut ui);
    focus(&mut ui, id);
    for direction in [Key::ArrowLeft, Key::ArrowRight] {
        for _ in 0..10 {
            key(&mut ui, direction.clone(), Modifiers::default());
            frame(&mut ui);
            let selection = ui.document(id).unwrap().selection();
            let caret = ui.caret_bounds().unwrap();
            for (operation, modifiers) in [
                (Key::Character("c".into()), control_modifier()),
                (Key::Backspace, Modifiers::default()),
                (Key::Delete, Modifiers::default()),
                (Key::Character("x".into()), control_modifier()),
                (Key::Character("v".into()), control_modifier()),
                (Key::Character("z".into()), control_modifier()),
                (Key::Character("y".into()), control_modifier()),
            ] {
                key(&mut ui, operation, modifiers);
                frame(&mut ui);
                assert_eq!(ui.document(id).unwrap().selection(), selection);
                assert_eq!(ui.caret_bounds().unwrap(), caret);
            }
        }
    }
    assert_eq!(ui.document(id).unwrap().text(), "abc אבג");
    assert!(ui.drain_text_changes().is_empty());
}

#[test]
fn disabling_editor_ancestor_cancels_preedit_and_blocks_all_user_value_routes() {
    let mut ui = ui();
    let parent = ui.panel(ui.root(), style(300.0, 80.0)).unwrap();
    let id = ui
        .edit_box(parent, "Name", "old", style(260.0, 36.0))
        .unwrap();
    frame(&mut ui);
    focus(&mut ui, id);
    key(&mut ui, Key::Character("a".into()), control_modifier());
    let selection = ui.document(id).unwrap().selection();
    ui.ime_preedit("candidate", Some((9, 9))).unwrap();
    ui.set_enabled(parent, false).unwrap();
    assert!(ui.document(id).unwrap().composition().is_none());
    assert!(!ui.is_text_editable(id));
    ui.text_input("ignored").unwrap();
    ui.ime_preedit("ignored", None).unwrap();
    ui.ime_commit("ignored").unwrap();
    ui.perform_action(id, SemanticAction::SetValue, Some("ignored"))
        .unwrap();
    assert_eq!(ui.document(id).unwrap().text(), "old");
    assert_eq!(ui.document(id).unwrap().selection(), selection);
    assert!(ui.document(id).unwrap().composition().is_none());
    assert!(!ui.document(id).unwrap().can_undo());
    assert!(ui.drain_text_changes().is_empty());
    let semantic = ui
        .semantics()
        .into_iter()
        .find(|node| node.id == id)
        .unwrap();
    assert!(semantic.disabled);
    assert!(!semantic.read_only);
    assert!(semantic.actions.is_empty());

    ui.set_enabled(parent, true).unwrap();
    focus(&mut ui, id);
    assert!(ui.is_text_editable(id));
    ui.text_input("new").unwrap();
    assert_eq!(ui.document(id).unwrap().text(), "new");
    let changes = ui.drain_text_changes();
    assert_eq!(changes.len(), 1);
    assert_eq!((changes[0].id, changes[0].text.as_str()), (id, "new"));
}

#[test]
fn editor_settings_reject_as_one_transaction_without_losing_preedit_or_history() {
    let mut ui = ui();
    let id = ui
        .edit_box(ui.root(), "Name", "Ada", style(260.0, 36.0))
        .unwrap();
    ui.set_placeholder(id, "Original hint").unwrap();
    frame(&mut ui);
    focus(&mut ui, id);
    ui.text_input("!").unwrap();
    key(&mut ui, Key::Character("a".into()), control_modifier());
    ui.ime_preedit("draft", Some((5, 5))).unwrap();
    frame(&mut ui);
    ui.drain_text_changes();
    let settings = ui.editor_settings(id).unwrap().clone();
    let selection = ui.document(id).unwrap().selection();
    let composition = ui.document(id).unwrap().composition().cloned();
    let caret = ui.caret_bounds().unwrap();

    for rejected in [
        EditorSettings {
            is_read_only: true,
            max_length: Some(1),
            placeholder: "Changed hint".into(),
            ..Default::default()
        },
        EditorSettings {
            is_read_only: true,
            max_length: Some(8),
            placeholder: "x".repeat(1_048_577),
            ..Default::default()
        },
    ] {
        assert!(ui.set_editor_settings(id, rejected).is_err());
        frame(&mut ui);
        assert_eq!(ui.editor_settings(id), Some(&settings));
        assert_eq!(ui.document(id).unwrap().text(), "Ada!");
        assert_eq!(ui.document(id).unwrap().selection(), selection);
        assert_eq!(ui.document(id).unwrap().composition(), composition.as_ref());
        assert_eq!(ui.caret_bounds().unwrap(), caret);
        assert!(ui.document(id).unwrap().can_undo());
        assert!(!ui.document(id).unwrap().can_redo());
        assert!(ui.drain_text_changes().is_empty());
    }
    ui.cancel_preedit();
    key(&mut ui, Key::Character("z".into()), control_modifier());
    assert_eq!(ui.document(id).unwrap().text(), "Ada");
}

#[test]
fn editor_settings_and_legacy_setters_share_one_policy_without_spurious_changes() {
    let mut ui = ui();
    let id = ui
        .edit_box(ui.root(), "Name", "Ada", style(260.0, 36.0))
        .unwrap();
    let settings = EditorSettings {
        is_read_only: false,
        max_length: Some(12),
        placeholder: "Name here".into(),
        ..Default::default()
    };
    ui.set_editor_settings(id, settings.clone()).unwrap();
    frame(&mut ui);
    focus(&mut ui, id);
    ui.ime_preedit("draft", Some((5, 5))).unwrap();
    let composition = ui.document(id).unwrap().composition().cloned();
    ui.set_editor_settings(id, settings).unwrap();
    ui.set_max_length(id, Some(16)).unwrap();
    ui.set_placeholder(id, "Updated hint").unwrap();
    assert_eq!(ui.document(id).unwrap().composition(), composition.as_ref());
    ui.set_read_only(id, true).unwrap();
    assert_eq!(
        ui.editor_settings(id),
        Some(&EditorSettings {
            is_read_only: true,
            max_length: Some(16),
            placeholder: "Updated hint".into(),
            ..Default::default()
        })
    );
    assert!(ui.document(id).unwrap().composition().is_none());
    assert_eq!(ui.document(id).unwrap().text(), "Ada");
    assert_eq!(ui.runtime.focused(), Some(id));
    assert!(ui.drain_text_changes().is_empty());
    ui.set_read_only(id, false).unwrap();
    ui.text_input("!").unwrap();
    assert_eq!(ui.document(id).unwrap().text(), "Ada!");
    assert_eq!(ui.drain_text_changes().len(), 1);
}

fn assert_editor_colors(
    ui: &Controls,
    id: WidgetId,
    background: Color,
    foreground: Color,
    border: Color,
) {
    let bounds = ui.runtime.bounds(id).unwrap();
    assert!(
        ui.scene()
            .rounded_rectangles
            .iter()
            .any(|r| r.bounds == bounds && r.color == border && r.stroke_width == Some(1.0))
    );
    assert!(
        ui.scene()
            .rounded_rectangles
            .iter()
            .any(|r| r.bounds == bounds && r.color == background && r.stroke_width.is_none())
    );
    assert_eq!(
        ui.scene()
            .texts
            .iter()
            .find(|r| r.text == "Ada")
            .unwrap()
            .color,
        foreground
    );
}

#[test]
fn editor_visual_states_resolve_focus_readonly_and_disabled_ancestors_in_scene() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let parent = ui.panel(ui.root(), style(300.0, 80.0)).unwrap();
    let id = ui
        .edit_box(parent, "Name", "Ada", style(260.0, 36.0))
        .unwrap();
    let normal = Color::rgb(11, 21, 31);
    let hovered = Color::rgb(41, 51, 61);
    let focused = Color::rgb(71, 81, 91);
    let readonly = Color::rgb(101, 111, 121);
    let disabled = Color::rgb(131, 141, 151);
    let foreground = Color::rgb(12, 22, 32);
    let border = Color::rgb(42, 52, 62);
    let readonly_accent = Color::rgb(72, 82, 92);
    let readonly_selection = Color::rgb(102, 112, 122);
    ui.set_edit_box_style(
        id,
        EditBoxStyle {
            normal: EditBoxAppearance {
                background: Some(normal),
                foreground: Some(foreground),
                border: Some(border),
                ..Default::default()
            },
            hovered: EditBoxAppearance {
                background: Some(hovered),
                ..Default::default()
            },
            focused: EditBoxAppearance {
                background: Some(focused),
                foreground: Some(focused),
                accent: Some(focused),
                ..Default::default()
            },
            read_only: EditBoxAppearance {
                background: Some(readonly),
                accent: Some(readonly_accent),
                selection: Some(readonly_selection),
                ..Default::default()
            },
            disabled: EditBoxAppearance {
                background: Some(disabled),
                foreground: Some(disabled),
                accent: Some(disabled),
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .unwrap();
    frame(&mut ui);
    assert_eq!(
        ui.edit_box_visual_state(id),
        Some(EditBoxVisualState::Normal)
    );
    assert_editor_colors(&ui, id, normal, foreground, border);
    let position = center(&ui, id);
    ui.dispatch(InputEvent::PointerMoved { position }).unwrap();
    frame(&mut ui);
    assert_eq!(
        ui.edit_box_visual_state(id),
        Some(EditBoxVisualState::Hovered)
    );
    assert_editor_colors(&ui, id, hovered, foreground, border);
    focus(&mut ui, id);
    frame(&mut ui);
    assert_eq!(
        ui.edit_box_visual_state(id),
        Some(EditBoxVisualState::Focused)
    );
    assert_editor_colors(&ui, id, focused, focused, border);
    ui.set_read_only(id, true).unwrap();
    key(&mut ui, Key::Character("a".into()), control_modifier());
    frame(&mut ui);
    assert_eq!(
        ui.edit_box_visual_state(id),
        Some(EditBoxVisualState::ReadOnlyFocused)
    );
    assert_editor_colors(&ui, id, readonly, foreground, border);
    assert!(
        ui.scene()
            .rectangles
            .iter()
            .any(|r| r.color == readonly_selection)
    );
    let caret = ui.caret_bounds().unwrap();
    assert!(
        ui.scene()
            .rectangles
            .iter()
            .any(|r| r.color == readonly_accent && r.bounds == caret)
    );
    let bounds = ui.runtime.bounds(id).unwrap();
    assert!(
        ui.scene()
            .rectangles
            .iter()
            .any(|r| r.color == readonly_accent && r.bounds.y == bounds.y + bounds.height - 2.0)
    );
    assert!(!ui.scene().texts.iter().any(|r| r.color == focused));

    ui.runtime.apply(Mutation::Focus(None)).unwrap();
    frame(&mut ui);
    assert_eq!(
        ui.edit_box_visual_state(id),
        Some(EditBoxVisualState::ReadOnly)
    );
    assert_editor_colors(&ui, id, readonly, foreground, border);
    assert!(
        !ui.scene()
            .rectangles
            .iter()
            .any(|r| r.color == readonly_accent)
    );
    focus(&mut ui, id);
    ui.set_enabled(parent, false).unwrap();
    frame(&mut ui);
    assert_eq!(
        ui.edit_box_visual_state(id),
        Some(EditBoxVisualState::Disabled)
    );
    assert_editor_colors(&ui, id, disabled, disabled, border);
    assert!(ui.caret_bounds().is_none());
    assert!(!ui.scene().rectangles.iter().any(|r| r.color == disabled
        || r.color == readonly_accent
        || r.color == readonly_selection));
    assert!(ui.drain_text_changes().is_empty());
}

#[test]
fn editor_style_partial_overrides_follow_theme_and_reset_to_theme_defaults() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let id = ui
        .edit_box(ui.root(), "Name", "Ada", style(260.0, 48.0))
        .unwrap();
    let foreground = Color::rgb(153, 45, 61);
    ui.set_edit_box_style(
        id,
        EditBoxStyle {
            normal: EditBoxAppearance {
                foreground: Some(foreground),
                ..Default::default()
            },
            padding: Some(Edges::all(10.0)),
            font_size: Some(20.0),
            ..Default::default()
        },
    )
    .unwrap();
    for theme in [Theme::FluentLight, Theme::FluentDark] {
        ui.set_theme(theme).unwrap();
        frame(&mut ui);
        assert_editor_colors(
            &ui,
            id,
            theme.chrome().editor_fill,
            foreground,
            theme.chrome().control_border,
        );
        let run = ui.scene().texts.iter().find(|r| r.text == "Ada").unwrap();
        assert_eq!(run.font_size, 20.0);
        assert_eq!(run.clip.x, ui.runtime.bounds(id).unwrap().x + 10.0);
        ui.set_read_only(id, true).unwrap();
        frame(&mut ui);
        assert_editor_colors(
            &ui,
            id,
            theme.chrome().editor_read_only,
            foreground,
            theme.chrome().control_border,
        );
        ui.set_enabled(id, false).unwrap();
        frame(&mut ui);
        assert_editor_colors(
            &ui,
            id,
            theme.chrome().editor_read_only,
            theme.chrome().disabled_text,
            theme.chrome().control_border,
        );
        ui.set_enabled(id, true).unwrap();
        ui.set_read_only(id, false).unwrap();
    }
    ui.set_edit_box_style(id, EditBoxStyle::default()).unwrap();
    frame(&mut ui);
    let tokens = Theme::FluentDark.tokens();
    let chrome = Theme::FluentDark.chrome();
    assert_editor_colors(
        &ui,
        id,
        chrome.editor_fill,
        tokens.text,
        chrome.control_border,
    );
    let run = ui.scene().texts.iter().find(|r| r.text == "Ada").unwrap();
    assert_eq!(run.font_size, tokens.font_size);
    assert_eq!(run.clip.x, ui.runtime.bounds(id).unwrap().x + 12.0);
    assert_eq!(ui.edit_box_style(id), Some(&EditBoxStyle::default()));
    assert!(ui.drain_text_changes().is_empty());
}

#[test]
fn editor_style_geometry_drives_text_caret_hit_testing_and_selection_clip() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let text = "0123456789 ".repeat(20);
    let id = ui
        .edit_box(ui.root(), "Code", &text, style(220.0, 64.0))
        .unwrap();
    let padding = Edges {
        left: 30.0,
        right: 18.0,
        top: 8.0,
        bottom: 12.0,
    };
    ui.set_edit_box_style(
        id,
        EditBoxStyle {
            font_size: Some(20.0),
            padding: Some(padding),
            corner_radius: Some(10.0),
            border_width: Some(3.0),
            ..Default::default()
        },
    )
    .unwrap();
    frame(&mut ui);
    focus(&mut ui, id);
    key(&mut ui, Key::Home, Modifiers::default());
    frame(&mut ui);
    let bounds = ui.runtime.bounds(id).unwrap();
    let clip = Rect::new(
        bounds.x + padding.left,
        bounds.y + padding.top,
        bounds.width - padding.horizontal(),
        bounds.height - padding.vertical(),
    );
    let run = ui.scene().texts.iter().find(|r| r.text == text).unwrap();
    assert_eq!(run.font_size, 20.0);
    assert_eq!(run.clip, clip);
    assert_eq!(ui.caret_bounds().unwrap().x, clip.x);
    assert!(
        ui.scene()
            .rounded_rectangles
            .iter()
            .any(|r| r.bounds == bounds && r.radius == 10.0 && r.stroke_width.is_none())
    );
    assert!(
        ui.scene()
            .rounded_rectangles
            .iter()
            .any(|r| r.bounds == bounds && r.radius == 10.0 && r.stroke_width == Some(3.0))
    );
    let caret = ui.caret_bounds().unwrap();
    let left = Point::new(clip.x, caret.y + caret.height * 0.5);
    pointer(&mut ui, left, true);
    pointer(&mut ui, left, false);
    assert_eq!(ui.document(id).unwrap().selection().focus, 0);
    key(&mut ui, Key::End, Modifiers::default());
    frame(&mut ui);
    let caret = ui.caret_bounds().unwrap();
    assert!(caret.x >= clip.x && caret.x + caret.width <= clip.x + clip.width + 0.01);
    assert!(caret.y >= clip.y && caret.y + caret.height <= clip.y + clip.height + 0.01);
    let near_left = Point::new(clip.x + 1.0, caret.y + caret.height * 0.5);
    pointer(&mut ui, near_left, true);
    pointer(&mut ui, near_left, false);
    let offset = ui.document(id).unwrap().selection().focus;
    assert!(offset > 0 && offset < text.len());
    assert!(
        ui.document(id)
            .unwrap()
            .grapheme_boundaries()
            .contains(&offset)
    );
    key(&mut ui, Key::Character("a".into()), control_modifier());
    frame(&mut ui);
    let selection: Vec<_> = ui
        .scene()
        .rectangles
        .iter()
        .filter(|r| r.color == Theme::FluentLight.tokens().selected)
        .collect();
    assert!(!selection.is_empty());
    for rectangle in selection {
        assert_eq!(rectangle.bounds.intersection(clip), Some(rectangle.bounds));
    }
    assert!(ui.drain_text_changes().is_empty());
}

#[test]
fn editor_color_style_preserves_document_preedit_history_caret_and_layout() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let id = ui
        .edit_box(ui.root(), "Mixed", "abc אבג", style(260.0, 48.0))
        .unwrap();
    frame(&mut ui);
    focus(&mut ui, id);
    ui.text_input("!").unwrap();
    ui.drain_text_changes();
    let first_style = EditBoxStyle {
        normal: EditBoxAppearance {
            foreground: Some(Color::rgb(170, 40, 60)),
            ..Default::default()
        },
        ..Default::default()
    };
    for _ in 0..10 {
        key(&mut ui, Key::ArrowLeft, Modifiers::default());
        frame(&mut ui);
        let caret = ui.caret_bounds().unwrap();
        let selection = ui.document(id).unwrap().selection();
        for appearance in [first_style, EditBoxStyle::default()] {
            ui.set_edit_box_style(id, appearance).unwrap();
            let delta = ui.update(Size::new(600.0, 480.0)).unwrap();
            assert_eq!(delta.layout_passes, 0);
            assert_eq!(ui.caret_bounds().unwrap(), caret);
            assert_eq!(ui.document(id).unwrap().selection(), selection);
            assert_eq!(ui.document(id).unwrap().text(), "abc אבג!");
            assert_eq!(ui.runtime.focused(), Some(id));
            ui.set_edit_box_style(id, appearance).unwrap();
            let idle = ui.update(Size::new(600.0, 480.0)).unwrap();
            assert_eq!(idle.layout_passes, 0);
            assert_eq!(idle.paint_passes, 0);
        }
    }
    ui.ime_preedit("draft", Some((5, 5))).unwrap();
    frame(&mut ui);
    let composition = ui.document(id).unwrap().composition().cloned();
    let selection = ui.document(id).unwrap().selection();
    ui.set_edit_box_style(
        id,
        EditBoxStyle {
            font_size: Some(18.0),
            padding: Some(Edges::all(8.0)),
            ..first_style
        },
    )
    .unwrap();
    frame(&mut ui);
    assert_eq!(ui.document(id).unwrap().composition(), composition.as_ref());
    assert_eq!(ui.document(id).unwrap().selection(), selection);
    assert!(ui.drain_text_changes().is_empty());
    ui.cancel_preedit();
    key(&mut ui, Key::Character("z".into()), control_modifier());
    assert_eq!(ui.document(id).unwrap().text(), "abc אבג");
}

#[test]
fn invalid_editor_style_rejection_preserves_active_style_and_composition() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let id = ui
        .edit_box(ui.root(), "Name", "Ada", style(260.0, 48.0))
        .unwrap();
    let accepted = EditBoxStyle {
        font_size: Some(18.0),
        corner_radius: Some(6.0),
        ..Default::default()
    };
    ui.set_edit_box_style(id, accepted).unwrap();
    frame(&mut ui);
    focus(&mut ui, id);
    ui.ime_preedit("draft", Some((5, 5))).unwrap();
    frame(&mut ui);
    let composition = ui.document(id).unwrap().composition().cloned();
    let caret = ui.caret_bounds().unwrap();
    for rejected in [
        EditBoxStyle {
            font_size: Some(f32::NAN),
            ..Default::default()
        },
        EditBoxStyle {
            font_size: Some(0.0),
            ..Default::default()
        },
        EditBoxStyle {
            font_size: Some(513.0),
            ..Default::default()
        },
        EditBoxStyle {
            padding: Some(Edges {
                bottom: -1.0,
                ..Edges::default()
            }),
            ..Default::default()
        },
        EditBoxStyle {
            corner_radius: Some(f32::INFINITY),
            ..Default::default()
        },
        EditBoxStyle {
            border_width: Some(16385.0),
            ..Default::default()
        },
        EditBoxStyle {
            focused: EditBoxAppearance {
                accent: Some(Color {
                    a: 1.1,
                    ..Color::rgb(1, 2, 3)
                }),
                ..Default::default()
            },
            ..Default::default()
        },
    ] {
        assert!(ui.set_edit_box_style(id, rejected).is_err());
        frame(&mut ui);
        assert_eq!(ui.edit_box_style(id), Some(&accepted));
        assert_eq!(ui.document(id).unwrap().composition(), composition.as_ref());
        assert_eq!(ui.document(id).unwrap().text(), "Ada");
        assert_eq!(ui.caret_bounds().unwrap(), caret);
        assert!(!ui.document(id).unwrap().can_undo());
        assert!(ui.drain_text_changes().is_empty());
    }
}

#[test]
fn editor_settings_and_style_reject_other_controls_and_removed_handles() {
    let mut ui = ui();
    let label = ui.label(ui.root(), "Label", style(260.0, 36.0)).unwrap();
    let editor = ui
        .edit_box(ui.root(), "Name", "Ada", style(260.0, 36.0))
        .unwrap();
    ui.remove(editor).unwrap();
    for id in [label, editor] {
        assert!(
            ui.set_editor_settings(id, EditorSettings::default())
                .is_err()
        );
        assert!(ui.set_edit_box_style(id, EditBoxStyle::default()).is_err());
        assert!(ui.editor_settings(id).is_none());
        assert!(ui.edit_box_style(id).is_none());
        assert!(ui.edit_box_visual_state(id).is_none());
    }
    frame(&mut ui);
    assert!(ui.scene().texts.iter().any(|r| r.text == "Label"));
    assert!(ui.drain_text_changes().is_empty());
}

#[test]
fn editor_rounded_stroke_keeps_transparent_background_and_large_radius_focus() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let id = ui
        .edit_box(ui.root(), "Name", "Ada", style(260.0, 48.0))
        .unwrap();
    let background = Color {
        a: 0.0,
        ..Color::rgb(11, 22, 33)
    };
    let border = Color {
        a: 0.5,
        ..Color::rgb(44, 55, 66)
    };
    ui.set_edit_box_style(
        id,
        EditBoxStyle {
            corner_radius: Some(16_384.0),
            border_width: Some(3.0),
            normal: EditBoxAppearance {
                background: Some(background),
                border: Some(border),
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
    let outline = ui
        .scene()
        .rounded_rectangles
        .iter()
        .find(|r| r.color == border)
        .unwrap();
    assert_eq!(outline.bounds, bounds);
    assert_eq!(outline.radius, bounds.height * 0.5);
    assert_eq!(outline.stroke_width, Some(3.0));
    assert!(!ui.scene().rectangles.iter().any(|r| r.color == border));
    assert!(
        ui.scene()
            .rounded_rectangles
            .iter()
            .filter(|r| r.color == background)
            .all(|r| r.stroke_width.is_none())
    );
    assert!(
        ui.scene()
            .rectangles
            .iter()
            .any(|r| r.color == Theme::FluentLight.tokens().focus
                && r.bounds.y == bounds.y + bounds.height - 2.0
                && r.bounds.width > 1.0)
    );
    assert!(ui.scene().rounded_rectangles.iter().all(|r| r.is_valid()));
}

#[test]
fn editor_nonfluent_focus_does_not_reduce_explicit_border_width() {
    let mut ui = Controls::new(Theme::Light).unwrap();
    let id = ui
        .edit_box(ui.root(), "Name", "Ada", style(260.0, 48.0))
        .unwrap();
    ui.set_edit_box_style(
        id,
        EditBoxStyle {
            corner_radius: Some(12.0),
            border_width: Some(8.0),
            ..Default::default()
        },
    )
    .unwrap();
    frame(&mut ui);
    let bounds = ui.runtime.bounds(id).unwrap();
    let outline = ui
        .scene()
        .rounded_rectangles
        .iter()
        .find(|r| r.bounds == bounds && r.stroke_width.is_some())
        .unwrap();
    assert_eq!(outline.stroke_width, Some(8.0));
    assert_eq!(outline.color, Theme::Light.tokens().border);
    focus(&mut ui, id);
    frame(&mut ui);
    let outline = ui
        .scene()
        .rounded_rectangles
        .iter()
        .find(|r| r.bounds == bounds && r.stroke_width.is_some())
        .unwrap();
    assert_eq!(outline.stroke_width, Some(8.0));
    assert_eq!(outline.color, Theme::Light.tokens().focus);
}

#[test]
fn editor_placeholder_and_composition_use_resolved_state_colors() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let id = ui
        .edit_box(ui.root(), "Name", "", style(260.0, 48.0))
        .unwrap();
    ui.set_placeholder(id, "Hint").unwrap();
    let normal = Color::rgb(12, 34, 56);
    let focused = Color::rgb(78, 90, 12);
    let readonly = Color::rgb(34, 56, 78);
    let disabled = Color::rgb(90, 12, 34);
    let accent = Color::rgb(56, 78, 90);
    ui.set_edit_box_style(
        id,
        EditBoxStyle {
            normal: EditBoxAppearance {
                placeholder: Some(normal),
                ..Default::default()
            },
            focused: EditBoxAppearance {
                placeholder: Some(focused),
                accent: Some(accent),
                ..Default::default()
            },
            read_only: EditBoxAppearance {
                placeholder: Some(readonly),
                ..Default::default()
            },
            disabled: EditBoxAppearance {
                placeholder: Some(disabled),
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .unwrap();
    frame(&mut ui);
    assert_eq!(
        ui.scene()
            .texts
            .iter()
            .find(|r| r.text == "Hint")
            .unwrap()
            .color,
        normal
    );
    focus(&mut ui, id);
    frame(&mut ui);
    assert_eq!(
        ui.scene()
            .texts
            .iter()
            .find(|r| r.text == "Hint")
            .unwrap()
            .color,
        focused
    );
    ui.ime_preedit("draft", Some((5, 5))).unwrap();
    frame(&mut ui);
    assert!(!ui.scene().texts.iter().any(|r| r.text == "Hint"));
    assert!(
        ui.scene()
            .rectangles
            .iter()
            .any(|r| r.color == accent && r.bounds.height == 1.0 && r.bounds.width > 1.0)
    );
    ui.set_read_only(id, true).unwrap();
    frame(&mut ui);
    assert_eq!(
        ui.scene()
            .texts
            .iter()
            .find(|r| r.text == "Hint")
            .unwrap()
            .color,
        readonly
    );
    ui.set_enabled(id, false).unwrap();
    frame(&mut ui);
    assert_eq!(
        ui.scene()
            .texts
            .iter()
            .find(|r| r.text == "Hint")
            .unwrap()
            .color,
        disabled
    );
    assert_eq!(ui.document(id).unwrap().text(), "");
    assert!(ui.drain_text_changes().is_empty());
}

#[test]
fn custom_scene_clipping_preserves_rounded_stroke_and_painter_order() {
    let mut ui = ui();
    let panel = ui.panel(ui.root(), style(200.0, 100.0)).unwrap();
    frame(&mut ui);
    let bounds = Rect::new(-10.0, -10.0, 140.0, 70.0);
    let source_clip = Rect::new(-5.0, -5.0, 160.0, 90.0);
    let effective_clip = source_clip
        .intersection(ui.runtime.clip(panel).unwrap())
        .unwrap();
    let fill = Color::rgb(12, 34, 56);
    let border = Color {
        a: 0.5,
        ..Color::rgb(78, 90, 12)
    };
    let mut custom = Scene::default();
    custom.rounded_fill(bounds, source_clip, 12.0, fill);
    custom.rounded_stroke(bounds, source_clip, 12.0, 3.0, border);
    ui.set_custom_scene(panel, custom).unwrap();
    frame(&mut ui);
    let rounded = &ui.scene().rounded_rectangles;
    assert_eq!(rounded.len(), 2);
    for rectangle in rounded {
        assert_eq!(rectangle.bounds, bounds);
        assert_eq!(rectangle.clip, effective_clip);
        assert_eq!(rectangle.radius, 12.0);
    }
    assert_eq!(rounded[0].stroke_width, None);
    assert_eq!(rounded[0].color, fill);
    assert_eq!(rounded[1].stroke_width, Some(3.0));
    assert_eq!(rounded[1].color, border);
    let rounded_commands: Vec<_> = ui
        .scene()
        .commands()
        .iter()
        .filter(|command| matches!(command, DrawCommand::RoundedRectangle(_)))
        .copied()
        .collect();
    assert_eq!(
        rounded_commands,
        vec![
            DrawCommand::RoundedRectangle(0),
            DrawCommand::RoundedRectangle(1)
        ]
    );
}
