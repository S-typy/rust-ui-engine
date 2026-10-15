use super::*;
use rust_desktop_ui_core::Modifiers;

fn ui() -> Controls {
    Controls::new(Theme::FluentLight).unwrap()
}
fn settings(header: &str) -> GroupSettings {
    GroupSettings {
        header: header.into(),
        ..Default::default()
    }
}
fn style() -> LayoutStyle {
    LayoutStyle {
        width: Length::Px(340.0),
        flex_shrink: 0.0,
        ..Default::default()
    }
}
fn frame(ui: &mut Controls) -> FrameStats {
    ui.update(Size::new(700.0, 800.0)).unwrap()
}
fn key(ui: &mut Controls, key: Key, down: bool, repeat: bool) {
    ui.dispatch(if down {
        InputEvent::KeyDown {
            key,
            modifiers: Modifiers::default(),
            repeat,
        }
    } else {
        InputEvent::KeyUp {
            key,
            modifiers: Modifiers::default(),
        }
    })
    .unwrap();
}
fn click(ui: &mut Controls, id: WidgetId) {
    let b = ui.runtime.bounds(id).unwrap();
    let position = Point::new(b.x + b.width / 2.0, b.y + b.height / 2.0);
    ui.dispatch(InputEvent::PointerDown {
        position,
        button: PointerButton::Primary,
        modifiers: Modifiers::default(),
    })
    .unwrap();
    ui.dispatch(InputEvent::PointerUp {
        position,
        button: PointerButton::Primary,
        modifiers: Modifiers::default(),
    })
    .unwrap();
}

#[test]
fn group_has_named_semantics_without_a_tab_stop_and_fills_remaining_height() {
    let mut ui = ui();
    let group = ui
        .group_box(
            ui.root(),
            settings("Profile"),
            LayoutStyle {
                height: Length::Px(240.0),
                ..style()
            },
        )
        .unwrap();
    let field = ui
        .edit_box(group.content, "Name", "Anna", LayoutStyle::default())
        .unwrap();
    frame(&mut ui);
    assert_eq!(ui.runtime.bounds(group.header).unwrap().height, 48.0);
    assert_eq!(ui.runtime.bounds(group.content).unwrap().height, 192.0);
    assert_eq!(
        ui.runtime.bounds(field).unwrap().x,
        ui.runtime.bounds(group.content).unwrap().x + 16.0
    );
    let semantics = ui.semantics();
    let node = semantics.iter().find(|n| n.id == group.container).unwrap();
    assert_eq!(node.role, Role::Group);
    assert_eq!(node.label, "Profile");
    assert_eq!(node.expanded, None);
    assert!(node.actions.is_empty());
    assert!(
        !ui.runtime
            .tree()
            .node(group.header)
            .unwrap()
            .props
            .focusable
    );
    key(&mut ui, Key::Tab, true, false);
    assert_eq!(ui.runtime.focused(), Some(field));
}

#[test]
fn collapse_removes_layout_semantics_and_hit_targets_without_removing_nodes() {
    let mut ui = ui();
    let group = ui
        .expander(ui.root(), settings("Advanced"), true, style())
        .unwrap();
    let editor = ui
        .edit_box(group.content, "Name", "Anna", LayoutStyle::default())
        .unwrap();
    frame(&mut ui);
    let nodes = ui.runtime.tree().preorder();
    let expanded = ui.runtime.bounds(group.container).unwrap().height;
    assert!(expanded > 48.0);
    ui.set_expander_expanded(group.container, false).unwrap();
    frame(&mut ui);
    assert_eq!(ui.runtime.tree().preorder(), nodes);
    assert_eq!(ui.runtime.bounds(group.container).unwrap().height, 48.0);
    assert!(ui.runtime.bounds(editor).is_none());
    assert!(
        !ui.semantics()
            .iter()
            .any(|n| n.id == editor || n.id == group.content)
    );
    assert!(!ui.is_enabled(editor));
    ui.set_expander_expanded(group.header, true).unwrap();
    frame(&mut ui);
    assert_eq!(ui.runtime.bounds(group.container).unwrap().height, expanded);
    assert_eq!(ui.document(editor).unwrap().text(), "Anna");
}

#[test]
fn programmatic_collapse_preserves_draft_selection_history_and_accepted_value_but_cancels_ime() {
    let mut ui = ui();
    let group = ui
        .expander(ui.root(), settings("Advanced"), true, style())
        .unwrap();
    let editor = ui
        .edit_box(group.content, "Name", "Anna", LayoutStyle::default())
        .unwrap();
    frame(&mut ui);
    ui.runtime.apply(Mutation::Focus(Some(editor))).unwrap();
    ui.text_input(" draft").unwrap();
    ui.ime_preedit("候補", Some((0, "候補".len()))).unwrap();
    let draft = ui.document(editor).unwrap().text().to_owned();
    let selection = ui.document(editor).unwrap().selection();
    ui.drain_edit_events();
    ui.set_expander_expanded(group.container, false).unwrap();
    assert_eq!(ui.runtime.focused(), Some(group.header));
    assert_eq!(ui.accepted_value(editor), Some("Anna"));
    assert!(ui.drain_edit_events().is_empty());
    ui.set_expander_expanded(group.container, true).unwrap();
    frame(&mut ui);
    let document = ui.document(editor).unwrap();
    assert_eq!(document.text(), draft);
    assert_eq!(document.selection(), selection);
    assert!(document.composition().is_none());
    assert!(document.can_undo());
    ui.runtime.apply(Mutation::Focus(Some(editor))).unwrap();
    ui.dispatch(InputEvent::KeyDown {
        key: Key::Character("z".into()),
        modifiers: Modifiers {
            control: true,
            ..Default::default()
        },
        repeat: false,
    })
    .unwrap();
    assert_eq!(ui.document(editor).unwrap().text(), "Anna");
}

#[test]
fn header_click_commits_valid_draft_but_invalid_draft_does_not_block_collapse() {
    for valid in [true, false] {
        let mut ui = ui();
        let group = ui
            .expander(ui.root(), settings("Advanced"), true, style())
            .unwrap();
        let editor = ui
            .edit_box(group.content, "Name", "Anna", LayoutStyle::default())
            .unwrap();
        ui.set_editor_validator(
            editor,
            Some(EditorValidator::new(move |_| {
                if valid {
                    Ok(())
                } else {
                    Err("Reserved".into())
                }
            })),
        )
        .unwrap();
        frame(&mut ui);
        ui.runtime.apply(Mutation::Focus(Some(editor))).unwrap();
        ui.text_input(" draft").unwrap();
        let draft = ui.document(editor).unwrap().text().to_owned();
        click(&mut ui, group.header);
        frame(&mut ui);
        assert_eq!(ui.expander_expanded(group.container), Some(false));
        assert_eq!(ui.runtime.focused(), Some(group.header));
        assert_eq!(
            ui.accepted_value(editor),
            Some(if valid { draft.as_str() } else { "Anna" })
        );
        assert_eq!(ui.document(editor).unwrap().text(), draft);
        assert_eq!(
            ui.validation_error(editor),
            if valid { None } else { Some("Reserved") }
        );
    }
}

#[test]
fn enter_space_pointer_and_semantic_actions_expand_once_and_are_idempotent() {
    let mut ui = ui();
    let group = ui
        .expander(ui.root(), settings("Advanced"), false, style())
        .unwrap();
    frame(&mut ui);
    ui.runtime
        .apply(Mutation::Focus(Some(group.header)))
        .unwrap();
    key(&mut ui, Key::Enter, true, false);
    key(&mut ui, Key::Enter, true, true);
    key(&mut ui, Key::Enter, false, false);
    assert_eq!(ui.expander_expanded(group.container), Some(true));
    assert_eq!(ui.drain_activated(), [group.header]);
    key(&mut ui, Key::Space, true, false);
    key(&mut ui, Key::Space, true, true);
    assert_eq!(ui.expander_expanded(group.container), Some(true));
    key(&mut ui, Key::Space, false, false);
    assert_eq!(ui.expander_expanded(group.container), Some(false));
    assert_eq!(ui.drain_activated(), [group.header]);
    for _ in 0..2 {
        ui.perform_action(group.container, SemanticAction::Expand, None)
            .unwrap();
    }
    assert_eq!(ui.expander_expanded(group.container), Some(true));
    assert!(ui.drain_activated().is_empty());
    for _ in 0..2 {
        ui.perform_action(group.header, SemanticAction::Collapse, None)
            .unwrap();
    }
    assert_eq!(ui.expander_expanded(group.header), Some(false));
    frame(&mut ui);
    click(&mut ui, group.header);
    assert_eq!(ui.expander_expanded(group.container), Some(true));
    assert_eq!(ui.drain_activated(), [group.header]);
}

#[test]
fn collapse_closes_menu_and_combo_popups_and_returns_focus_to_its_header() {
    let mut ui = ui();
    let group = ui
        .expander(ui.root(), settings("Advanced"), true, style())
        .unwrap();
    ui.commands_mut().register("Open", "Open");
    let menu = ui
        .dropdown_button(
            group.content,
            "Options",
            &[MenuEntry::Item(MenuItem::new("Open", "Open"))],
            LayoutStyle::default(),
        )
        .unwrap();
    let combo = ui
        .combo_box(
            group.content,
            "Choice",
            vec![SelectionItem::new("one", "One")],
            ComboBoxSettings::default(),
            LayoutStyle::default(),
        )
        .unwrap();
    frame(&mut ui);
    for owner in [menu, combo.container] {
        ui.set_expander_expanded(group.container, true).unwrap();
        frame(&mut ui);
        ui.expand_popup(owner).unwrap();
        frame(&mut ui);
        assert!(ui.popup_open());
        ui.set_expander_expanded(group.container, false).unwrap();
        frame(&mut ui);
        assert!(!ui.popup_open());
        assert_eq!(ui.runtime.focused(), Some(group.header));
    }
}

#[test]
fn collapse_during_pointer_capture_cannot_activate_a_stale_release() {
    let mut ui = ui();
    let group = ui
        .expander(ui.root(), settings("Advanced"), true, style())
        .unwrap();
    let button = ui
        .button(group.content, "Action", None, LayoutStyle::default())
        .unwrap();
    frame(&mut ui);
    let b = ui.runtime.bounds(button).unwrap();
    let position = Point::new(b.x + 20.0, b.y + 10.0);
    ui.dispatch(InputEvent::PointerDown {
        position,
        button: PointerButton::Primary,
        modifiers: Modifiers::default(),
    })
    .unwrap();
    assert_eq!(ui.runtime.captured(), Some(button));
    ui.set_expander_expanded(group.container, false).unwrap();
    assert_eq!(ui.runtime.captured(), None);
    ui.set_expander_expanded(group.container, true).unwrap();
    frame(&mut ui);
    ui.dispatch(InputEvent::PointerUp {
        position,
        button: PointerButton::Primary,
        modifiers: Modifiers::default(),
    })
    .unwrap();
    assert!(ui.drain_activated().is_empty());
}

#[test]
fn required_parts_and_invalid_settings_are_rejected_atomically() {
    let mut ui = ui();
    let group = ui
        .expander(ui.root(), settings("Profile"), true, style())
        .unwrap();
    frame(&mut ui);
    let nodes = ui.runtime.tree().preorder();
    let scene = ui.scene().clone();
    for id in [group.header, group.content] {
        assert!(ui.remove(id).is_err());
    }
    for invalid in [f32::NAN, f32::INFINITY, -1.0, 16_385.0] {
        let bad = GroupSettings {
            padding: Edges::all(invalid),
            ..settings("Invalid")
        };
        assert!(ui.set_group_settings(group.container, bad.clone()).is_err());
        assert!(ui.group_box(ui.root(), bad, style()).is_err());
        assert_eq!(ui.group_header(group.container), Some("Profile"));
    }
    assert!(ui.set_group_header(group.header, "я".repeat(2049)).is_err());
    assert!(
        ui.group_box(ui.root(), settings(&"a".repeat(4097)), style())
            .is_err()
    );
    assert_eq!(ui.runtime.tree().preorder(), nodes);
    assert_eq!(ui.scene(), &scene);
    assert_eq!(frame(&mut ui), FrameStats::default());
    ui.remove(group.container).unwrap();
    frame(&mut ui);
    assert!(ui.group_settings(group.container).is_none());
    assert!(ui.groups.is_empty());
}

#[test]
fn unchanged_group_updates_do_not_rebuild_layout_or_scene() {
    let mut ui = ui();
    let group = ui
        .expander(ui.root(), settings("Profile"), true, style())
        .unwrap();
    frame(&mut ui);
    ui.set_group_settings(group.container, settings("Profile"))
        .unwrap();
    ui.set_group_header(group.header, "Profile").unwrap();
    ui.set_label(group.container, "Profile").unwrap();
    ui.set_expander_expanded(group.container, true).unwrap();
    assert_eq!(frame(&mut ui), FrameStats::default());
    ui.set_expander_expanded(group.container, false).unwrap();
    frame(&mut ui);
    ui.set_expander_expanded(group.header, false).unwrap();
    assert_eq!(frame(&mut ui), FrameStats::default());
}

#[test]
fn nested_expanders_keep_independent_state_and_external_focus() {
    let mut ui = ui();
    let outer = ui
        .expander(ui.root(), settings("Outer"), true, style())
        .unwrap();
    let inner = ui
        .expander(
            outer.content,
            settings("Inner"),
            true,
            LayoutStyle::default(),
        )
        .unwrap();
    let editor = ui
        .edit_box(inner.content, "Name", "Anna", LayoutStyle::default())
        .unwrap();
    let outside = ui
        .button(ui.root(), "Outside", None, LayoutStyle::default())
        .unwrap();
    frame(&mut ui);
    ui.runtime.apply(Mutation::Focus(Some(editor))).unwrap();
    ui.set_expander_expanded(outer.container, false).unwrap();
    assert_eq!(ui.runtime.focused(), Some(outer.header));
    assert_eq!(ui.expander_expanded(inner.container), Some(true));
    ui.set_expander_expanded(inner.container, false).unwrap();
    ui.set_expander_expanded(outer.container, true).unwrap();
    frame(&mut ui);
    assert_eq!(ui.expander_expanded(inner.container), Some(false));
    assert!(ui.runtime.bounds(editor).is_none());
    ui.runtime.apply(Mutation::Focus(Some(outside))).unwrap();
    ui.set_expander_expanded(outer.container, false).unwrap();
    assert_eq!(ui.runtime.focused(), Some(outside));
}

#[test]
fn disabled_or_hidden_groups_do_not_offer_activation_or_move_focus() {
    let mut ui = ui();
    let group = ui
        .expander(ui.root(), settings("Advanced"), true, style())
        .unwrap();
    let editor = ui
        .edit_box(group.content, "Name", "Anna", LayoutStyle::default())
        .unwrap();
    frame(&mut ui);
    ui.runtime.apply(Mutation::Focus(Some(editor))).unwrap();
    ui.set_enabled(group.container, false).unwrap();
    ui.perform_action(group.header, SemanticAction::Collapse, None)
        .unwrap();
    assert_eq!(ui.expander_expanded(group.container), Some(true));
    ui.set_expander_expanded(group.container, false).unwrap();
    assert_ne!(ui.runtime.focused(), Some(group.header));
    frame(&mut ui);
    let semantic = ui
        .semantics()
        .into_iter()
        .find(|n| n.id == group.header)
        .unwrap();
    assert!(semantic.disabled && semantic.actions.is_empty());
    ui.edit_props(group.container, |p| p.visible = false)
        .unwrap();
    frame(&mut ui);
    assert!(!ui.semantics().iter().any(|n| n.id == group.container));
}

#[test]
fn accessible_group_name_follows_header_and_respects_automation_override() {
    let mut ui = ui();
    let group = ui
        .expander(ui.root(), settings("Details"), false, style())
        .unwrap();
    ui.set_label(group.container, "Settings").unwrap();
    assert_eq!(ui.group_header(group.header), Some("Settings"));
    ui.set_automation_name(group.container, "Account settings")
        .unwrap();
    frame(&mut ui);
    for id in [group.container, group.header] {
        let semantic = ui.semantics().into_iter().find(|n| n.id == id).unwrap();
        assert_eq!(semantic.label, "Account settings");
        assert_eq!(semantic.expanded, Some(false));
        assert!(!semantic.has_popup);
        assert!(semantic.actions.contains(&SemanticAction::Expand));
    }
    ui.set_group_header(group.container, "Visual heading")
        .unwrap();
    frame(&mut ui);
    assert!(
        ui.scene()
            .texts
            .iter()
            .any(|run| run.text == "Visual heading")
    );
    assert_eq!(
        ui.semantics()
            .into_iter()
            .find(|n| n.id == group.header)
            .unwrap()
            .label,
        "Account settings"
    );
}

#[test]
fn appearances_follow_themes_and_narrow_headers_remain_clipped() {
    for appearance in [
        GroupAppearance::Card,
        GroupAppearance::Outline,
        GroupAppearance::Plain,
    ] {
        let mut ui = ui();
        let group = ui
            .expander(
                ui.root(),
                GroupSettings {
                    appearance,
                    ..settings("A long section heading that must remain inside its header")
                },
                true,
                LayoutStyle {
                    width: Length::Px(80.0),
                    ..style()
                },
            )
            .unwrap();
        frame(&mut ui);
        let light = ui.scene().clone();
        let header = ui.runtime.bounds(group.header).unwrap();
        let run = ui
            .scene()
            .texts
            .iter()
            .find(|run| run.text.starts_with("A long"))
            .unwrap();
        assert!(run.is_valid());
        assert!(!run.wrap);
        assert!(
            run.clip.x >= header.x && run.clip.x + run.clip.width <= header.x + header.width - 32.0
        );
        let container = ui.runtime.bounds(group.container).unwrap();
        let filled = ui
            .scene()
            .rounded_rectangles
            .iter()
            .any(|r| r.bounds == container && r.stroke_width.is_none());
        assert_eq!(filled, appearance == GroupAppearance::Card);
        ui.set_theme(Theme::FluentDark).unwrap();
        frame(&mut ui);
        assert_ne!(ui.scene(), &light);
        assert_eq!(ui.runtime.bounds(group.container), Some(container));
        assert_eq!(frame(&mut ui), FrameStats::default());
    }
}
