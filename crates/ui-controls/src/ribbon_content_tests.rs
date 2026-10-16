use super::*;
use rust_desktop_ui_core::{Modifiers, Size};
fn fixture() -> (Controls, Ribbon, WidgetId, WidgetId) {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    ui.commands_mut().register("save", "Save");
    ui.commands_mut().register("flag", "Flag");
    ui.commands_mut().set_checked(&"flag".into(), Some(false));
    let tabs = vec![
        RibbonTab {
            id: "home".into(),
            label: "Home".into(),
            key_tip: "H".into(),
            context: None,
            groups: vec![RibbonGroup {
                id: "format".into(),
                label: "Formatting".into(),
                collapse_priority: 0,
                items: vec![RibbonItem::new("flag", RibbonItemKind::Large, "B")],
            }],
        },
        RibbonTab {
            id: "other".into(),
            label: "Other".into(),
            key_tip: "O".into(),
            context: None,
            groups: Vec::new(),
        },
    ];
    let mut ribbon = Ribbon::new(RibbonModel {
        tabs,
        quick_access: vec!["save".into()],
    })
    .unwrap();
    ribbon
        .add_content(RibbonContentSpec {
            id: "font".into(),
            tab_id: "home".into(),
            group_id: "format".into(),
            label: "Font name".into(),
            key_tip: "FF".into(),
            preferred_width: 240.0,
            minimum_width: 160.0,
            height: 84.0,
        })
        .unwrap();
    ribbon
        .set_backstage(Some(RibbonBackstageSpec {
            label: "File".into(),
            key_tip: "F".into(),
            items: vec![
                RibbonBackstageItem::Page {
                    id: "info".into(),
                    label: "Info".into(),
                    key_tip: "I".into(),
                },
                RibbonBackstageItem::Page {
                    id: "options".into(),
                    label: "Options".into(),
                    key_tip: "O".into(),
                },
                RibbonBackstageItem::Command {
                    command: "flag".into(),
                    key_tip: "B".into(),
                },
            ],
        }))
        .unwrap();
    let root = ui.root();
    ribbon.mount_with_width(&mut ui, root, 900.0).unwrap();
    let editor = ui
        .edit_box(
            ribbon.content_host("font").unwrap(),
            "Font",
            "accepted",
            LayoutStyle {
                height: Length::Px(36.0),
                ..Default::default()
            },
        )
        .unwrap();
    let backstage = ui
        .edit_box(
            ribbon.backstage_page_host("info").unwrap(),
            "Description",
            "saved",
            LayoutStyle {
                height: Length::Px(36.0),
                ..Default::default()
            },
        )
        .unwrap();
    frame(&mut ui, &mut ribbon, 900.0);
    (ui, ribbon, editor, backstage)
}
fn frame(ui: &mut Controls, ribbon: &mut Ribbon, width: f32) {
    ribbon.update(ui, width).unwrap();
    ui.update(Size::new(width, 600.0)).unwrap();
    ribbon.update(ui, width).unwrap();
    ui.update(Size::new(width, 600.0)).unwrap();
}
fn focus(ui: &mut Controls, id: WidgetId) {
    ui.runtime.apply(Mutation::Focus(Some(id))).unwrap();
    ui.update(Size::new(900.0, 600.0)).unwrap();
}
fn key(ribbon: &mut Ribbon, ui: &mut Controls, key: Key) -> bool {
    ribbon
        .handle_input(
            ui,
            &InputEvent::KeyDown {
                key,
                modifiers: Modifiers::default(),
                repeat: false,
            },
        )
        .unwrap()
}
#[test]
fn content_retains_draft_selection_undo_focus_and_composition_during_visible_reflow() {
    let (mut ui, mut ribbon, editor, _) = fixture();
    let host = ribbon.content_host("font").unwrap();
    focus(&mut ui, editor);
    ui.text_input(" draft").unwrap();
    ui.ime_preedit("candidate", None).unwrap();
    let selection = ui.document(editor).unwrap().selection();
    for width in [850.0, 600.0, 900.0] {
        ui.set_theme(Theme::FluentDark).unwrap();
        frame(&mut ui, &mut ribbon, width);
        assert_eq!(ribbon.content_host("font"), Some(host));
        assert_eq!(ui.runtime.focused(), Some(editor));
        assert_eq!(ui.document(editor).unwrap().text(), "accepted draft");
        assert_eq!(ui.accepted_value(editor), Some("accepted"));
        assert_eq!(ui.document(editor).unwrap().selection(), selection);
        assert!(ui.document(editor).unwrap().can_undo());
        assert!(ui.document(editor).unwrap().composition().is_some());
    }
}
#[test]
fn hidden_content_keeps_unaccepted_draft_cancels_composition_and_restores_on_reveal() {
    let (mut ui, mut ribbon, editor, _) = fixture();
    focus(&mut ui, editor);
    ui.text_input(" draft").unwrap();
    ui.ime_preedit("candidate", None).unwrap();
    ribbon.set_active_tab("other");
    frame(&mut ui, &mut ribbon, 900.0);
    assert!(!ui.is_enabled(editor));
    assert_ne!(ui.runtime.focused(), Some(editor));
    assert_eq!(ui.accepted_value(editor), Some("accepted"));
    assert_eq!(ui.document(editor).unwrap().text(), "accepted draft");
    assert!(ui.document(editor).unwrap().composition().is_none());
    ribbon.set_active_tab("home");
    frame(&mut ui, &mut ribbon, 900.0);
    assert!(ui.is_enabled(editor));
    assert_eq!(ui.document(editor).unwrap().text(), "accepted draft");
    assert!(ui.document(editor).unwrap().can_undo());
}
#[test]
fn collapsed_content_popup_keeps_identity_and_draft_across_resize_theme_close_reopen() {
    let (mut ui, mut ribbon, editor, _) = fixture();
    let host = ribbon.content_host("font");
    frame(&mut ui, &mut ribbon, 140.0);
    assert!(!ui.is_enabled(editor));
    ribbon.open_content_group(&mut ui, "format").unwrap();
    frame(&mut ui, &mut ribbon, 140.0);
    assert!(ribbon.content_popup_is_open(&ui));
    focus(&mut ui, editor);
    ui.text_input(" draft").unwrap();
    for width in [180.0, 700.0, 150.0] {
        ui.set_theme(Theme::FluentDark).unwrap();
        frame(&mut ui, &mut ribbon, width);
        assert!(ribbon.content_popup_is_open(&ui));
        assert_eq!(ribbon.content_host("font"), host);
        assert_eq!(ui.runtime.focused(), Some(editor));
        assert_eq!(ui.accepted_value(editor), Some("accepted"));
    }
    ribbon.close_content_groups(&mut ui).unwrap();
    frame(&mut ui, &mut ribbon, 150.0);
    assert_eq!(ui.document(editor).unwrap().text(), "accepted draft");
    assert_eq!(ui.accepted_value(editor), Some("accepted"));
    ribbon.open_content_group(&mut ui, "format").unwrap();
    frame(&mut ui, &mut ribbon, 150.0);
    assert!(ui.is_enabled(editor));
    assert_eq!(ui.document(editor).unwrap().text(), "accepted draft");
}
#[test]
fn content_keytip_reaches_overflow_editor_and_retains_command_exactly_once() {
    let (mut ui, mut ribbon, editor, _) = fixture();
    frame(&mut ui, &mut ribbon, 90.0);
    for pressed in [
        Key::Alt,
        Key::Character("H".into()),
        Key::Character("F".into()),
        Key::Character("F".into()),
    ] {
        assert!(key(&mut ribbon, &mut ui, pressed));
        frame(&mut ui, &mut ribbon, 90.0);
    }
    assert!(ribbon.content_popup_is_open(&ui));
    assert_eq!(ui.runtime.focused(), Some(editor));
    assert!(ui.commands_mut().drain_invocations().is_empty());
    ribbon.close_content_groups(&mut ui).unwrap();
    frame(&mut ui, &mut ribbon, 90.0);
    for pressed in [
        Key::Alt,
        Key::Character("H".into()),
        Key::Character("B".into()),
    ] {
        key(&mut ribbon, &mut ui, pressed);
        frame(&mut ui, &mut ribbon, 90.0);
    }
    assert_eq!(ui.commands_mut().drain_invocations().len(), 1);
    assert_eq!(ui.commands().checked(&"flag".into()), Some(true));
}
#[test]
fn backstage_pages_retain_drafts_and_escape_returns_external_focus() {
    let (mut ui, mut ribbon, _, editor) = fixture();
    let root = ui.root();
    let external = ui
        .edit_box(
            root,
            "Document",
            "outside",
            LayoutStyle {
                height: Length::Px(36.0),
                ..Default::default()
            },
        )
        .unwrap();
    frame(&mut ui, &mut ribbon, 900.0);
    focus(&mut ui, external);
    ribbon.open_backstage(&mut ui).unwrap();
    assert!(!key(&mut ribbon, &mut ui, Key::Character("x".into())));
    ui.text_input(" draft").unwrap();
    assert_eq!(ui.document(editor).unwrap().text(), "saved draft");
    ribbon.select_backstage_page(&mut ui, "options").unwrap();
    frame(&mut ui, &mut ribbon, 600.0);
    assert_eq!(ui.accepted_value(editor), Some("saved"));
    assert_eq!(ui.document(editor).unwrap().text(), "saved draft");
    ribbon.select_backstage_page(&mut ui, "info").unwrap();
    frame(&mut ui, &mut ribbon, 600.0);
    assert_eq!(ui.runtime.focused(), Some(editor));
    assert!(key(&mut ribbon, &mut ui, Key::Escape));
    assert_eq!(ui.runtime.focused(), Some(external));
    assert_eq!(ui.accepted_value(editor), Some("saved"));
    ribbon.open_backstage(&mut ui).unwrap();
    assert_eq!(ui.document(editor).unwrap().text(), "saved draft");
}
#[test]
fn backstage_keytips_toggle_command_once_and_native_caption_stays_uncovered() {
    let (mut ui, mut ribbon, _, _) = fixture();
    ribbon
        .set_header(Some(RibbonHeader {
            title: "Document".into(),
            height: 40.0,
            leading_inset: 8.0,
            trailing_inset: 138.0,
        }))
        .unwrap();
    frame(&mut ui, &mut ribbon, 900.0);
    assert!(key(&mut ribbon, &mut ui, Key::Alt));
    assert!(key(&mut ribbon, &mut ui, Key::Character("F".into())));
    frame(&mut ui, &mut ribbon, 900.0);
    assert!(ribbon.backstage_is_open(&ui));
    let root = ribbon.backstage_page_host("info").unwrap();
    let mut ancestors = std::iter::successors(Some(root), |id| ui.runtime.tree().parent(*id));
    assert!(ancestors.any(|id| {
        ui.runtime
            .bounds(id)
            .is_some_and(|r| r.y >= 40.0 && r.height > 400.0)
    }));
    assert!(key(&mut ribbon, &mut ui, Key::Character("B".into())));
    assert!(!ribbon.backstage_is_open(&ui));
    assert_eq!(ui.commands_mut().drain_invocations().len(), 1);
    assert_eq!(ui.commands().checked(&"flag".into()), Some(true));
}
#[test]
fn registration_preflight_rejects_duplicates_bad_bounds_and_prefixes_without_mutation() {
    let (_, ribbon, _, _) = fixture();
    let mut candidate = Ribbon::new(ribbon.model().clone()).unwrap();
    let slot = RibbonContentSpec {
        id: "font".into(),
        tab_id: "home".into(),
        group_id: "format".into(),
        label: "Font".into(),
        key_tip: "FF".into(),
        preferred_width: 240.0,
        minimum_width: 160.0,
        height: 84.0,
    };
    candidate.add_content(slot.clone()).unwrap();
    assert!(candidate.add_content(slot.clone()).is_err());
    let mut bad = slot;
    bad.id = "other".into();
    bad.key_tip = "F".into();
    assert!(candidate.add_content(bad.clone()).is_err());
    bad.key_tip = "Z".into();
    bad.minimum_width = 500.0;
    assert!(candidate.add_content(bad).is_err());
    assert_eq!(candidate.retained.specs.len(), 1);
}

#[test]
fn retained_surfaces_cancel_preedit_before_escape_and_owner_disable_closes_them() {
    let (mut ui, mut ribbon, editor, page) = fixture();
    ribbon.open_backstage(&mut ui).unwrap();
    ui.ime_preedit("candidate", None).unwrap();
    assert!(key(&mut ribbon, &mut ui, Key::Escape));
    assert!(ribbon.backstage_is_open(&ui));
    assert!(ui.document(page).unwrap().composition().is_none());
    ui.set_enabled(ribbon.container().unwrap(), false).unwrap();
    ui.update(Size::new(900.0, 600.0)).unwrap();
    assert!(!ribbon.backstage_is_open(&ui));
    ui.set_enabled(ribbon.container().unwrap(), true).unwrap();
    frame(&mut ui, &mut ribbon, 140.0);
    ribbon.open_content_group(&mut ui, "format").unwrap();
    frame(&mut ui, &mut ribbon, 140.0);
    focus(&mut ui, editor);
    ui.ime_preedit("candidate", None).unwrap();
    assert!(key(&mut ribbon, &mut ui, Key::Escape));
    assert!(ribbon.content_popup_is_open(&ui));
    assert!(ui.document(editor).unwrap().composition().is_none());
    ui.set_enabled(ribbon.container().unwrap(), false).unwrap();
    ui.update(Size::new(140.0, 600.0)).unwrap();
    assert!(!ribbon.content_popup_is_open(&ui));
}
#[test]
fn merged_keytip_collisions_preserve_model_tree_and_retained_draft_atomically() {
    let (mut ui, mut ribbon, editor, _) = fixture();
    focus(&mut ui, editor);
    ui.text_input(" draft").unwrap();
    let before = ui.runtime.tree().preorder();
    let conflicting = RibbonTab {
        id: "home".into(),
        label: "Home".into(),
        key_tip: "H".into(),
        context: None,
        groups: vec![RibbonGroup {
            id: "new".into(),
            label: "New".into(),
            collapse_priority: 0,
            items: vec![RibbonItem::new("save", RibbonItemKind::Large, "FF")],
        }],
    };
    assert!(
        ribbon
            .merge_tabs(&mut ui, "document", vec![conflicting])
            .is_err()
    );
    assert_eq!(ui.runtime.tree().preorder(), before);
    assert_eq!(ribbon.model.tabs[0].groups.len(), 1);
    assert_eq!(ui.runtime.focused(), Some(editor));
    assert_eq!(ui.accepted_value(editor), Some("accepted"));
    let conflicting = RibbonTab {
        id: "file".into(),
        label: "File".into(),
        key_tip: "F".into(),
        context: None,
        groups: Vec::new(),
    };
    assert!(
        ribbon
            .merge_tabs(&mut ui, "document", vec![conflicting])
            .is_err()
    );
    assert_eq!(ui.runtime.tree().preorder(), before);
}
#[test]
fn content_only_group_has_real_full_collapsed_and_overflow_access() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let mut ribbon = Ribbon::new(RibbonModel {
        tabs: vec![RibbonTab {
            id: "home".into(),
            label: "Home".into(),
            key_tip: "H".into(),
            context: None,
            groups: vec![RibbonGroup {
                id: "edit".into(),
                label: "Editor".into(),
                collapse_priority: 0,
                items: Vec::new(),
            }],
        }],
        quick_access: Vec::new(),
    })
    .unwrap();
    ribbon
        .add_content(RibbonContentSpec {
            id: "content".into(),
            tab_id: "home".into(),
            group_id: "edit".into(),
            label: "Editor".into(),
            key_tip: "E".into(),
            preferred_width: 240.0,
            minimum_width: 180.0,
            height: 84.0,
        })
        .unwrap();
    let root = ui.root();
    ribbon.mount_with_width(&mut ui, root, 500.0).unwrap();
    let editor = ui
        .edit_box(
            ribbon.content_host("content").unwrap(),
            "Value",
            "",
            LayoutStyle {
                height: Length::Px(36.0),
                ..Default::default()
            },
        )
        .unwrap();
    frame(&mut ui, &mut ribbon, 500.0);
    assert!(ui.is_enabled(editor));
    for width in [130.0, 70.0] {
        frame(&mut ui, &mut ribbon, width);
        assert!(!ui.is_enabled(editor));
        let anchor = ribbon.group_anchors["edit"];
        assert!(ui.is_enabled(anchor));
        ui.perform_action(anchor, crate::SemanticAction::Activate, None)
            .unwrap();
        ribbon.process(&mut ui).unwrap();
        frame(&mut ui, &mut ribbon, width);
        assert!(ribbon.content_popup_is_open(&ui));
        assert!(ui.is_enabled(editor));
        ribbon.close_content_groups(&mut ui).unwrap();
        frame(&mut ui, &mut ribbon, width);
    }
}

#[test]
fn rejected_open_preserves_visible_hosts_focus_and_minimized_state() {
    let (mut ui, mut ribbon, editor, _) = fixture();
    focus(&mut ui, editor);
    ui.text_input(" draft").unwrap();
    ui.set_enabled(ribbon.container().unwrap(), false).unwrap();
    let before = ui.runtime.tree().preorder();
    let parent = ui
        .runtime
        .tree()
        .parent(ribbon.content_host("font").unwrap());
    assert!(ribbon.open_content_group(&mut ui, "format").is_err());
    assert!(ribbon.open_backstage(&mut ui).is_err());
    assert_eq!(ui.runtime.tree().preorder(), before);
    assert_eq!(
        ui.runtime
            .tree()
            .parent(ribbon.content_host("font").unwrap()),
        parent
    );
    assert!(!ribbon.is_minimized());
    assert!(!ribbon.transient);
    ui.set_enabled(ribbon.container().unwrap(), true).unwrap();
    ribbon.set_minimized(true);
    frame(&mut ui, &mut ribbon, 900.0);
    ui.dispatch(InputEvent::WindowFocused(false)).unwrap();
    let before = ui.runtime.tree().preorder();
    assert!(ribbon.open_content_group(&mut ui, "format").is_err());
    assert!(ribbon.open_backstage(&mut ui).is_err());
    assert!(!ribbon.transient);
    assert_eq!(ui.runtime.tree().preorder(), before);
    assert_eq!(ui.accepted_value(editor), Some("accepted"));
}
#[test]
fn removed_required_parts_fail_before_rebuild_and_whole_owner_prunes_overlays() {
    for part in 0..3 {
        let (mut ui, mut ribbon, _, _) = fixture();
        let id = match part {
            0 => ribbon.content_host("font").unwrap(),
            1 => ribbon.backstage_page_host("info").unwrap(),
            _ => ui
                .runtime
                .tree()
                .children(
                    ui.runtime
                        .tree()
                        .parent(
                            ui.runtime
                                .tree()
                                .parent(ribbon.backstage_page_host("info").unwrap())
                                .unwrap(),
                        )
                        .unwrap(),
                )
                .unwrap()[0],
        };
        ui.remove(id).unwrap();
        let before = ui.runtime.tree().preorder();
        assert!(ribbon.update(&mut ui, 700.0).is_err());
        assert_eq!(ui.runtime.tree().preorder(), before);
        assert!(ribbon.open_backstage(&mut ui).is_err());
        ui.remove(ribbon.container().unwrap()).unwrap();
        ui.update(Size::new(900.0, 600.0)).unwrap();
        assert!(!ribbon.backstage_is_open(&ui));
        assert!(!ribbon.content_popup_is_open(&ui));
    }
}

#[test]
fn backstage_keytip_activation_cancels_preedit_without_accepting_page_draft() {
    let (mut ui, mut ribbon, _, editor) = fixture();
    ribbon.open_backstage(&mut ui).unwrap();
    ui.text_input(" draft").unwrap();
    ui.ime_preedit("candidate", None).unwrap();
    assert!(key(&mut ribbon, &mut ui, Key::Alt));
    assert!(ui.document(editor).unwrap().composition().is_none());
    assert_eq!(ui.accepted_value(editor), Some("saved"));
    assert!(key(&mut ribbon, &mut ui, Key::Character("B".into())));
    assert_eq!(ui.commands_mut().drain_invocations().len(), 1);
    assert_eq!(ui.document(editor).unwrap().text(), "saved draft");
    assert_eq!(ui.accepted_value(editor), Some("saved"));
}

#[test]
fn inline_and_collapsed_content_keytips_use_retained_panel_anchors() {
    let (mut ui, mut ribbon, editor, _) = fixture();
    let host = ribbon.content_host("font").unwrap();
    focus(&mut ui, editor);
    ui.text_input(" draft").unwrap();
    for width in [900.0, 140.0, 900.0] {
        frame(&mut ui, &mut ribbon, width);
        assert!(key(&mut ribbon, &mut ui, Key::Alt));
        assert!(key(&mut ribbon, &mut ui, Key::Character("H".into())));
        frame(&mut ui, &mut ribbon, width);
        if width > 140.0 {
            assert_eq!(ui.key_tip(host), Some("FF"));
            assert!(ui.scene().texts.iter().any(|run| run.text == "FF"));
        }
        assert!(key(&mut ribbon, &mut ui, Key::Character("F".into())));
        assert!(key(&mut ribbon, &mut ui, Key::Character("F".into())));
        frame(&mut ui, &mut ribbon, width);
        assert_eq!(ui.runtime.focused(), Some(editor));
        assert_eq!(ribbon.content_host("font"), Some(host));
        assert_eq!(ui.document(editor).unwrap().text(), "accepted draft");
        assert!(ui.commands_mut().drain_invocations().is_empty());
        assert_eq!(ui.key_tip(host), None);
        assert!(
            ui.set_button_content_layout(host, ButtonContentLayout::Above)
                .is_err()
        );
        ribbon.close_content_groups(&mut ui).unwrap();
    }
}

#[test]
fn backstage_keytip_open_restores_focus_from_before_alt_after_page_changes() {
    let (mut ui, mut ribbon, _, editor) = fixture();
    let external = ui
        .edit_box(
            ui.root(),
            "Document",
            "outside",
            LayoutStyle {
                height: Length::Px(36.0),
                ..Default::default()
            },
        )
        .unwrap();
    frame(&mut ui, &mut ribbon, 900.0);
    focus(&mut ui, external);
    for pressed in [
        Key::Alt,
        Key::Character("F".into()),
        Key::Character("I".into()),
    ] {
        assert!(key(&mut ribbon, &mut ui, pressed));
        frame(&mut ui, &mut ribbon, 900.0);
    }
    assert_eq!(ui.runtime.focused(), Some(editor));
    ui.text_input(" draft").unwrap();
    ribbon.select_backstage_page(&mut ui, "options").unwrap();
    frame(&mut ui, &mut ribbon, 600.0);
    ribbon.select_backstage_page(&mut ui, "info").unwrap();
    frame(&mut ui, &mut ribbon, 600.0);
    assert_eq!(ui.runtime.focused(), Some(editor));
    assert!(key(&mut ribbon, &mut ui, Key::Escape));
    frame(&mut ui, &mut ribbon, 600.0);
    assert!(!ribbon.backstage_is_open(&ui));
    assert_eq!(ui.runtime.focused(), Some(external));
    assert_eq!(ui.document(editor).unwrap().text(), "saved draft");
    assert_eq!(ui.accepted_value(editor), Some("saved"));
    assert!(ui.commands_mut().drain_invocations().is_empty());
}
