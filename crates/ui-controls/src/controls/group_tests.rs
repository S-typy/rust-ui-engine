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

#[test]
fn expansion_directions_preserve_ids_and_position_the_header_on_each_side() {
    use super::groups::ExpandDirection;
    for direction in [
        ExpandDirection::Down,
        ExpandDirection::Up,
        ExpandDirection::Left,
        ExpandDirection::Right,
    ] {
        let mut ui = ui();
        let group = ui
            .expander(
                ui.root(),
                GroupSettings {
                    expand_direction: direction,
                    ..settings("Direction")
                },
                true,
                LayoutStyle {
                    width: Length::Px(360.0),
                    height: Length::Px(180.0),
                    ..Default::default()
                },
            )
            .unwrap();
        let editor = ui
            .edit_box(group.content, "Value", "retained", LayoutStyle::default())
            .unwrap();
        frame(&mut ui);
        let header = ui.runtime.bounds(group.header).unwrap();
        let content = ui.runtime.bounds(group.content).unwrap();
        match direction {
            ExpandDirection::Down => assert!(header.y + header.height <= content.y + 0.01),
            ExpandDirection::Up => assert!(content.y + content.height <= header.y + 0.01),
            ExpandDirection::Left => assert!(content.x + content.width <= header.x + 0.01),
            ExpandDirection::Right => assert!(header.x + header.width <= content.x + 0.01),
        }
        ui.set_expander_expanded(group.container, false).unwrap();
        ui.set_expander_expanded(group.container, true).unwrap();
        frame(&mut ui);
        assert_eq!(ui.document(editor).unwrap().text(), "retained");
    }
}

#[test]
fn expander_animation_uses_bounded_ticks_and_reduced_motion_preserves_draft() {
    use super::groups::ExpanderAnimation;
    use std::time::Duration;
    let mut ui = ui();
    let group = ui
        .expander(ui.root(), settings("Animated"), true, style())
        .unwrap();
    let editor = ui
        .edit_box(group.content, "Name", "accepted", LayoutStyle::default())
        .unwrap();
    frame(&mut ui);
    ui.runtime.apply(Mutation::Focus(Some(editor))).unwrap();
    ui.text_input(" draft").unwrap();
    let draft = ui.document(editor).unwrap().text().to_owned();
    ui.set_expander_animation(
        group.container,
        Some(ExpanderAnimation {
            duration: Duration::from_millis(160),
        }),
    )
    .unwrap();
    assert!(
        ui.set_expander_animation(
            group.container,
            Some(ExpanderAnimation {
                duration: Duration::from_secs(10)
            })
        )
        .is_err()
    );
    ui.set_expander_expanded(group.container, false).unwrap();
    assert_eq!(ui.runtime.focused(), Some(group.header));
    assert_eq!(ui.accepted_value(editor), Some("accepted"));
    assert!(!ui.is_enabled(editor));
    let deadline = ui.expander_deadline().unwrap();
    assert!(
        !ui.expander_tick(deadline - Duration::from_millis(1))
            .unwrap()
    );
    assert!(
        ui.expander_tick(deadline + Duration::from_millis(60))
            .unwrap()
    );
    assert!(ui.expander_tick(deadline + Duration::from_secs(2)).unwrap());
    assert!(ui.expander_deadline().is_none());
    frame(&mut ui);
    assert!(ui.runtime.bounds(group.content).is_none());
    ui.set_expander_expanded(group.container, true).unwrap();
    assert!(ui.expander_deadline().is_some());
    ui.set_reduced_motion(true);
    frame(&mut ui);
    assert!(ui.expander_deadline().is_none());
    assert!(ui.is_enabled(editor));
    assert_eq!(ui.document(editor).unwrap().text(), draft);
    assert_eq!(ui.accepted_value(editor), Some("accepted"));
}

#[test]
fn retained_header_templates_keep_static_group_and_expander_names() {
    let mut ui = ui();
    for collapsible in [false, true] {
        let parts = if collapsible {
            ui.collapsible_group_box(ui.root(), settings("Semantic header"), true, style())
                .unwrap()
        } else {
            ui.group_box(ui.root(), settings("Semantic header"), style())
                .unwrap()
        };
        let template = ui
            .transparent_panel(ui.root(), LayoutStyle::default())
            .unwrap();
        ui.label(template, "Custom header", LayoutStyle::default())
            .unwrap();
        ui.set_group_header_content(parts.container, Some(template))
            .unwrap();
        frame(&mut ui);
        assert_eq!(ui.button_content(parts.header), Some(template));
        assert!(
            ui.scene()
                .texts
                .iter()
                .any(|run| run.text == "Custom header")
        );
        assert!(
            !ui.scene()
                .texts
                .iter()
                .any(|run| run.text == "Semantic header")
        );
        assert_eq!(ui.group_header(parts.container), Some("Semantic header"));
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
        GroupAppearance::Standard,
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
        let reserved = if appearance == GroupAppearance::Standard {
            16.0
        } else {
            32.0
        };
        assert!(
            run.clip.x >= header.x
                && run.clip.x + run.clip.width <= header.x + header.width - reserved
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

#[test]
fn hidden_static_header_reclaims_space_and_preserves_its_accessible_group_name() {
    let mut ui = ui();
    let group = ui
        .group_box(
            ui.root(),
            GroupSettings {
                show_header: false,
                ..settings("Accessible group")
            },
            LayoutStyle {
                height: Length::Px(200.0),
                ..style()
            },
        )
        .unwrap();
    let editor = ui
        .edit_box(group.content, "Name", "retained", LayoutStyle::default())
        .unwrap();
    frame(&mut ui);
    let children = ui.runtime.tree().preorder();
    let container = ui.runtime.bounds(group.container).unwrap();
    assert_eq!(ui.runtime.bounds(group.header), None);
    assert_eq!(ui.runtime.bounds(group.content), Some(container));
    assert_eq!(
        ui.semantics()
            .iter()
            .find(|n| n.id == group.container)
            .unwrap()
            .label,
        "Accessible group"
    );
    assert!(!ui.semantics().iter().any(|n| n.id == group.header));
    ui.set_group_settings(group.container, settings("Visible group"))
        .unwrap();
    frame(&mut ui);
    assert_eq!(ui.runtime.bounds(group.header).unwrap().height, 48.0);
    assert_eq!(ui.runtime.bounds(group.content).unwrap().height, 152.0);
    assert_eq!(ui.runtime.tree().preorder(), children);
    assert_eq!(ui.document(editor).unwrap().text(), "retained");
}

#[test]
fn compact_header_changes_geometry_without_remounting_or_changing_expansion() {
    let mut ui = ui();
    let group = ui
        .collapsible_group_box(
            ui.root(),
            GroupSettings {
                header_style: GroupHeaderStyle::Light,
                ..settings("Compact")
            },
            false,
            style(),
        )
        .unwrap();
    let editor = ui
        .edit_box(group.content, "Value", "draft", LayoutStyle::default())
        .unwrap();
    frame(&mut ui);
    assert_eq!(ui.runtime.bounds(group.container).unwrap().height, 36.0);
    assert_eq!(
        ui.scene()
            .texts
            .iter()
            .find(|t| t.text == "Compact")
            .unwrap()
            .weight,
        500
    );
    ui.perform_action(group.header, SemanticAction::Expand, None)
        .unwrap();
    frame(&mut ui);
    assert!(ui.runtime.bounds(editor).is_some());
    let nodes = ui.runtime.tree().preorder();
    ui.set_group_settings(group.container, settings("Normal"))
        .unwrap();
    frame(&mut ui);
    assert_eq!(ui.runtime.bounds(group.header).unwrap().height, 48.0);
    assert_eq!(ui.runtime.tree().preorder(), nodes);
    assert_eq!(ui.expander_expanded(group.container), Some(true));
    assert_eq!(ui.document(editor).unwrap().text(), "draft");
}

#[test]
fn standard_frame_leaves_a_real_transparent_caption_gap() {
    let mut ui = ui();
    let group = ui
        .group_box(
            ui.root(),
            GroupSettings {
                appearance: GroupAppearance::Standard,
                header_icon: Some(IconSymbol::FolderOpen),
                ..settings("Document")
            },
            LayoutStyle {
                height: Length::Px(180.0),
                ..style()
            },
        )
        .unwrap();
    frame(&mut ui);
    let container = ui.runtime.bounds(group.container).unwrap();
    let header = ui.runtime.bounds(group.header).unwrap();
    let line_y = header.y + header.height / 2.0 + 0.5;
    let gap_x = header.x + 52.0;
    let borders: Vec<_> = ui
        .scene()
        .rounded_rectangles
        .iter()
        .filter(|r| r.stroke_width == Some(1.0))
        .collect();
    assert_eq!(borders.len(), 3);
    assert!(
        borders
            .iter()
            .all(|r| r.bounds.y == header.y + header.height / 2.0)
    );
    assert!(!borders.iter().any(|r| r.clip.contains(gap_x, line_y)));
    assert!(borders.iter().any(|r| {
        r.clip
            .contains(container.x + container.width - 10.0, line_y)
    }));
    assert!(
        borders
            .iter()
            .any(|r| r.clip.contains(gap_x, container.y + container.height - 0.5))
    );
    assert!(
        !ui.scene()
            .rounded_rectangles
            .iter()
            .any(|r| r.stroke_width.is_none() && r.bounds.intersection(header).is_some())
    );
    let caption = ui
        .scene()
        .texts
        .iter()
        .find(|t| t.text == "Document")
        .unwrap();
    assert_eq!(caption.bounds.x, header.x + 40.0);
    assert!(!ui.scene().stroke_paths.is_empty());
}

#[test]
fn header_overrides_preserve_alpha_and_radius_across_theme_changes_but_disabled_text_wins() {
    let mut ui = ui();
    let foreground = Color::rgb(92, 35, 144);
    let background = Color {
        a: 0.35,
        ..Color::rgb(115, 210, 150)
    };
    let group = ui
        .group_box(
            ui.root(),
            GroupSettings {
                header_style: GroupHeaderStyle::Accent,
                corner_radius: Some(13.0),
                header_foreground: Some(foreground),
                header_background: Some(background),
                ..settings("Custom")
            },
            LayoutStyle {
                height: Length::Px(180.0),
                ..style()
            },
        )
        .unwrap();
    for theme in [Theme::FluentLight, Theme::FluentDark] {
        ui.set_theme(theme).unwrap();
        frame(&mut ui);
        assert_eq!(
            ui.scene()
                .texts
                .iter()
                .find(|t| t.text == "Custom")
                .unwrap()
                .color,
            foreground
        );
        assert!(
            ui.scene()
                .rounded_rectangles
                .iter()
                .any(|r| r.color == background)
        );
        let bounds = ui.runtime.bounds(group.container).unwrap();
        assert!(
            ui.scene()
                .rounded_rectangles
                .iter()
                .any(|r| r.bounds == bounds && r.radius == 13.0)
        );
    }
    ui.set_enabled(group.container, false).unwrap();
    frame(&mut ui);
    assert_eq!(
        ui.scene()
            .texts
            .iter()
            .find(|t| t.text == "Custom")
            .unwrap()
            .color,
        Theme::FluentDark.chrome().disabled_text
    );
}

#[test]
fn invalid_header_settings_and_hidden_disclosure_are_rejected_before_mutation() {
    let mut ui = ui();
    let group = ui
        .collapsible_group_box(ui.root(), settings("Retain"), true, style())
        .unwrap();
    frame(&mut ui);
    let nodes = ui.runtime.tree().preorder();
    let scene = ui.scene().clone();
    for value in [f32::NAN, f32::INFINITY, -1.0, 16_385.0] {
        let invalid = GroupSettings {
            corner_radius: Some(value),
            ..settings("Bad")
        };
        assert!(
            ui.set_group_settings(group.container, invalid.clone())
                .is_err()
        );
        assert!(ui.group_box(ui.root(), invalid, style()).is_err());
    }
    for value in [f32::NAN, f32::INFINITY, -0.1, 1.1] {
        for background in [false, true] {
            let mut invalid = settings("Bad");
            let color = Color {
                a: value,
                ..Color::rgb(50, 50, 50)
            };
            if background {
                invalid.header_background = Some(color);
            } else {
                invalid.header_foreground = Some(color);
            }
            assert!(
                ui.set_group_settings(group.container, invalid.clone())
                    .is_err()
            );
            assert!(ui.group_box(ui.root(), invalid, style()).is_err());
        }
    }
    let hidden = GroupSettings {
        show_header: false,
        ..settings("Hidden")
    };
    assert!(
        ui.set_group_settings(group.container, hidden.clone())
            .is_err()
    );
    assert!(
        ui.expander(ui.root(), hidden.clone(), true, style())
            .is_err()
    );
    assert!(
        ui.collapsible_group_box(ui.root(), hidden, false, style())
            .is_err()
    );
    assert_eq!(ui.runtime.tree().preorder(), nodes);
    assert_eq!(ui.scene(), &scene);
    assert_eq!(ui.group_header(group.container), Some("Retain"));
    assert_eq!(frame(&mut ui), FrameStats::default());
}

#[test]
fn collapsible_group_box_retains_editor_draft_and_supports_header_and_container_aliases() {
    let mut ui = ui();
    let group = ui
        .collapsible_group_box(
            ui.root(),
            GroupSettings {
                appearance: GroupAppearance::Standard,
                header_icon: Some(IconSymbol::Edit),
                ..settings("Document")
            },
            true,
            style(),
        )
        .unwrap();
    let editor = ui
        .edit_box(group.content, "Name", "accepted", LayoutStyle::default())
        .unwrap();
    frame(&mut ui);
    ui.runtime.apply(Mutation::Focus(Some(editor))).unwrap();
    ui.text_input(" draft").unwrap();
    let draft = ui.document(editor).unwrap().text().to_owned();
    ui.set_expander_expanded(group.header, false).unwrap();
    assert_eq!(ui.runtime.focused(), Some(group.header));
    assert_eq!(ui.accepted_value(editor), Some("accepted"));
    frame(&mut ui);
    click(&mut ui, group.header);
    assert_eq!(ui.expander_expanded(group.container), Some(true));
    assert_eq!(ui.drain_activated(), [group.header]);
    frame(&mut ui);
    assert_eq!(ui.document(editor).unwrap().text(), draft);
    for id in [group.container, group.header] {
        let node = ui.semantics().into_iter().find(|n| n.id == id).unwrap();
        assert_eq!(node.expanded, Some(true));
        assert!(node.actions.contains(&SemanticAction::Collapse));
    }
}
