use crate::*;
use rust_desktop_ui_core::{
    Axis, Color, DrawCommand, InputEvent, Key, LayoutKind, LayoutStyle, Length, Modifiers,
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
                prepared.height <= run.clip.height,
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
    assert!(ui.ime_preedit("a\nb", Some((3, 3))).is_err());
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
        assert_eq!(rounded[1].radius, 3.0);
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
