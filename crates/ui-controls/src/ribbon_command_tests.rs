use super::*;
use crate::{CommandInvocation, SemanticAction};
use rust_desktop_ui_core::{Modifiers, Point, PointerButton, Size};
use std::{cell::Cell, rc::Rc};

fn group(id: &str, items: Vec<RibbonItem>) -> RibbonGroup {
    RibbonGroup {
        id: id.into(),
        label: id.into(),
        collapse_priority: 2,
        items,
    }
}
fn fixture() -> (Controls, Ribbon, Rc<Cell<usize>>) {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    for id in ["flag", "primary", "secondary"] {
        ui.commands_mut().register_toggle(id, id, false);
    }
    ui.commands_mut().register("plain", "plain");
    let callbacks = Rc::new(Cell::new(0));
    for id in ["flag", "primary", "secondary", "plain"] {
        let counter = callbacks.clone();
        ui.commands_mut()
            .on_execute(&id.into(), move || counter.set(counter.get() + 1));
    }
    let mut ribbon = Ribbon::new(RibbonModel {
        quick_access: vec!["flag".into(), "plain".into()],
        tabs: vec![RibbonTab {
            id: "home".into(),
            label: "Home".into(),
            key_tip: "H".into(),
            context: None,
            groups: vec![
                group(
                    "work",
                    vec![
                        RibbonItem::new("flag", RibbonItemKind::Small, "F"),
                        RibbonItem::new(
                            "primary",
                            RibbonItemKind::Split {
                                menu: vec!["secondary".into(), "plain".into()],
                            },
                            "P",
                        ),
                    ],
                ),
                RibbonGroup {
                    collapse_priority: 0,
                    ..group(
                        "other",
                        vec![RibbonItem::new("plain", RibbonItemKind::Small, "O")],
                    )
                },
            ],
        }],
    })
    .unwrap();
    let root = ui.root();
    ribbon.mount(&mut ui, root).unwrap();
    frame(&mut ui, &mut ribbon, 900.0);
    (ui, ribbon, callbacks)
}
fn frame(ui: &mut Controls, ribbon: &mut Ribbon, width: f32) {
    ribbon.update(ui, width).unwrap();
    ui.update(Size::new(width, 600.0)).unwrap();
}
fn placements(ui: &Controls, ribbon: &Ribbon, command: &str) -> Vec<WidgetId> {
    ui.runtime.tree().preorder().into_iter().filter(|id| {
        matches!(ribbon.actions.get(id), Some(Action::Command(candidate)) if candidate.0 == command)
    }).collect()
}
fn open_menu(ui: &mut Controls, ribbon: &mut Ribbon, label: &str) {
    ui.drain_activated();
    let anchors: Vec<_> = ui
        .runtime
        .tree()
        .preorder()
        .into_iter()
        .filter(|id| matches!(ribbon.actions.get(id), Some(Action::Dropdown)))
        .collect();
    for anchor in anchors {
        ui.activate(anchor).unwrap();
        assert!(ribbon.process(ui).unwrap().is_empty());
        assert!(ui.popup_open());
        if ui
            .semantics()
            .iter()
            .any(|node| node.role == Role::MenuItem && node.label == label)
        {
            return;
        }
        ui.close_popup().unwrap();
    }
    panic!("no dropdown contains {label}");
}
fn menu_item(ui: &Controls, label: &str) -> WidgetId {
    ui.semantics()
        .into_iter()
        .find(|node| node.role == Role::MenuItem && node.label == label)
        .unwrap()
        .id
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
fn tips(ribbon: &mut Ribbon, ui: &mut Controls, characters: &[&str]) {
    assert!(key(ribbon, ui, Key::Alt));
    for character in characters {
        assert!(key(ribbon, ui, Key::Character((*character).into())));
    }
    assert!(!ribbon.key_tips_visible());
}
fn assert_state(ui: &Controls, command: &str, ids: &[WidgetId], value: Option<bool>) {
    assert_eq!(ui.commands().checked(&command.into()), value);
    let semantics = ui.semantics();
    for id in ids {
        let node = semantics.iter().find(|node| node.id == *id).unwrap();
        assert_eq!(node.checked, value);
        if node.role != Role::MenuItem {
            assert_eq!(
                node.role,
                if value.is_some() {
                    Role::ToggleButton
                } else {
                    Role::Button
                }
            );
        }
        assert_eq!(
            node.actions.contains(&SemanticAction::Toggle),
            value.is_some()
        );
    }
}
fn assert_invocation(ui: &mut Controls, command: &str, checked: Option<bool>) {
    assert_eq!(
        ui.commands_mut().drain_invocations(),
        vec![CommandInvocation {
            id: command.into(),
            checked
        }]
    );
}

#[test]
fn qat_and_small_body_share_toggle_state_and_activate_consecutively_without_a_frame() {
    let (mut ui, mut ribbon, callbacks) = fixture();
    let ids = placements(&ui, &ribbon, "flag");
    assert_eq!(ids.len(), 2);
    assert_state(&ui, "flag", &ids, Some(false));
    ui.activate(ids[0]).unwrap();
    assert_state(&ui, "flag", &ids, Some(true));
    ui.activate(ids[1]).unwrap();
    assert_state(&ui, "flag", &ids, Some(false));
    assert_eq!(callbacks.get(), 2);
    assert_eq!(
        ui.commands_mut().drain_invocations(),
        vec![
            CommandInvocation {
                id: "flag".into(),
                checked: Some(true)
            },
            CommandInvocation {
                id: "flag".into(),
                checked: Some(false)
            },
        ]
    );
    assert!(ribbon.process(&mut ui).unwrap().is_empty());
    assert_eq!(
        callbacks.get(),
        2,
        "Ribbon.process must not invoke already activated commands"
    );
}

#[test]
fn qat_pointer_enter_and_space_each_activate_once() {
    let (mut ui, mut ribbon, callbacks) = fixture();
    let id = placements(&ui, &ribbon, "flag")[0];
    let bounds = ui.runtime.bounds(id).unwrap();
    let point = Point::new(
        bounds.x + bounds.width / 2.0,
        bounds.y + bounds.height / 2.0,
    );
    ui.dispatch(InputEvent::PointerDown {
        position: point,
        button: PointerButton::Primary,
        modifiers: Modifiers::default(),
    })
    .unwrap();
    assert_eq!(callbacks.get(), 0);
    ui.dispatch(InputEvent::PointerUp {
        position: point,
        button: PointerButton::Primary,
        modifiers: Modifiers::default(),
    })
    .unwrap();
    assert_invocation(&mut ui, "flag", Some(true));
    ui.dispatch(InputEvent::KeyDown {
        key: Key::Enter,
        modifiers: Modifiers::default(),
        repeat: false,
    })
    .unwrap();
    ui.dispatch(InputEvent::KeyDown {
        key: Key::Enter,
        modifiers: Modifiers::default(),
        repeat: true,
    })
    .unwrap();
    assert_invocation(&mut ui, "flag", Some(false));
    ui.dispatch(InputEvent::KeyDown {
        key: Key::Space,
        modifiers: Modifiers::default(),
        repeat: false,
    })
    .unwrap();
    assert_eq!(callbacks.get(), 2);
    ui.dispatch(InputEvent::KeyUp {
        key: Key::Space,
        modifiers: Modifiers::default(),
    })
    .unwrap();
    assert_invocation(&mut ui, "flag", Some(true));
    assert!(ribbon.process(&mut ui).unwrap().is_empty());
    assert_eq!(callbacks.get(), 3);
}

#[test]
fn qat_more_preserves_toggle_semantics_and_latest_state_when_reopened() {
    let (mut ui, mut ribbon, callbacks) = fixture();
    frame(&mut ui, &mut ribbon, 100.0);
    assert!(placements(&ui, &ribbon, "flag").is_empty());
    open_menu(&mut ui, &mut ribbon, "flag");
    let id = menu_item(&ui, "flag");
    assert_state(&ui, "flag", &[id], Some(false));
    ui.activate(id).unwrap();
    assert!(!ui.popup_open());
    assert_invocation(&mut ui, "flag", Some(true));
    open_menu(&mut ui, &mut ribbon, "flag");
    let id = menu_item(&ui, "flag");
    assert_state(&ui, "flag", &[id], Some(true));
    ui.activate(id).unwrap();
    assert_invocation(&mut ui, "flag", Some(false));
    assert_eq!(callbacks.get(), 2);
}

#[test]
fn collapsed_and_all_commands_menus_keep_primary_secondary_and_plain_actions() {
    let (mut ui, mut ribbon, callbacks) = fixture();
    for width in [300.0, 100.0] {
        frame(&mut ui, &mut ribbon, width);
        if width == 300.0 {
            assert!(
                ribbon
                    .presentations()
                    .iter()
                    .any(|p| matches!(p, RibbonPresentation::Collapsed(_)))
            );
        } else {
            assert!(matches!(
                &ribbon.presentations()[0],
                RibbonPresentation::Overflow(_)
            ));
        }
        for (label, command, checked) in [
            ("work / flag", "flag", Some(width == 300.0)),
            ("work / primary", "primary", Some(width == 300.0)),
            ("work / secondary", "secondary", Some(width == 300.0)),
            ("other / plain", "plain", None),
        ] {
            // At medium width the other group remains expanded, so test its
            // ordinary command through the corresponding body placement.
            if width == 300.0 && command == "plain" {
                let id = *placements(&ui, &ribbon, command).last().unwrap();
                ui.activate(id).unwrap();
            } else {
                open_menu(&mut ui, &mut ribbon, label);
                let id = menu_item(&ui, label);
                ui.activate(id).unwrap();
                assert!(!ui.popup_open());
            }
            assert_invocation(&mut ui, command, checked);
        }
    }
    assert_eq!(callbacks.get(), 8);
}

#[test]
fn full_split_primary_and_dropdown_secondary_use_the_same_activation_contract() {
    let (mut ui, mut ribbon, callbacks) = fixture();
    let primary = placements(&ui, &ribbon, "primary")[0];
    assert_state(&ui, "primary", &[primary], Some(false));
    ui.activate(primary).unwrap();
    assert_invocation(&mut ui, "primary", Some(true));
    open_menu(&mut ui, &mut ribbon, "secondary");
    assert_eq!(
        callbacks.get(),
        1,
        "opening a dropdown is not command activation"
    );
    let secondary = menu_item(&ui, "secondary");
    assert_state(&ui, "secondary", &[secondary], Some(false));
    ui.activate(secondary).unwrap();
    assert_invocation(&mut ui, "secondary", Some(true));
    open_menu(&mut ui, &mut ribbon, "secondary");
    let plain = menu_item(&ui, "plain");
    assert_state(&ui, "plain", &[plain], None);
    ui.activate(plain).unwrap();
    assert_invocation(&mut ui, "plain", None);
    assert_eq!(callbacks.get(), 3);
}

#[test]
fn body_and_numeric_keytips_toggle_even_when_all_presentations_are_in_overflow() {
    let (mut ui, mut ribbon, callbacks) = fixture();
    for width in [900.0, 300.0, 100.0] {
        frame(&mut ui, &mut ribbon, width);
        tips(&mut ribbon, &mut ui, &["H", "F"]);
        assert_invocation(&mut ui, "flag", Some(true));
        tips(&mut ribbon, &mut ui, &["1"]);
        assert_invocation(&mut ui, "flag", Some(false));
    }
    tips(&mut ribbon, &mut ui, &["H", "P"]);
    assert_invocation(&mut ui, "primary", Some(true));
    assert_eq!(callbacks.get(), 7);
}

#[test]
fn live_some_none_transitions_update_body_qat_and_open_menu_without_rebuilding_widgets() {
    let (mut ui, mut ribbon, callbacks) = fixture();
    // Put the same command into the split dropdown as an additional placement.
    if let RibbonItemKind::Split { menu } = &mut ribbon.model.tabs[0].groups[0].items[1].kind {
        menu.push("flag".into());
    }
    ribbon.dirty = true;
    frame(&mut ui, &mut ribbon, 900.0);
    let ids = placements(&ui, &ribbon, "flag");
    open_menu(&mut ui, &mut ribbon, "secondary");
    let menu = menu_item(&ui, "flag");
    let mut all = ids.clone();
    all.push(menu);
    for value in [Some(true), None, Some(false)] {
        ui.commands_mut().set_checked(&"flag".into(), value);
        assert_state(&ui, "flag", &all, value);
        frame(&mut ui, &mut ribbon, 900.0);
        assert_eq!(placements(&ui, &ribbon, "flag"), ids);
        assert_eq!(menu_item(&ui, "flag"), menu);
        assert_state(&ui, "flag", &all, value);
    }
    ui.commands_mut().set_checked(&"flag".into(), None);
    ui.activate(menu).unwrap();
    assert_invocation(&mut ui, "flag", None);
    assert_eq!(callbacks.get(), 1);
}

#[test]
fn legacy_toggle_hints_include_hidden_context_initialize_once_and_never_register_missing_commands()
{
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    ui.commands_mut().register("legacy", "Legacy");
    ui.commands_mut().register_toggle("preset", "Preset", true);
    let mut ribbon = Ribbon::new(RibbonModel {
        quick_access: vec!["legacy".into(), "preset".into()],
        tabs: vec![RibbonTab {
            id: "context".into(),
            label: "Context".into(),
            key_tip: "C".into(),
            context: Some("selected".into()),
            groups: vec![group(
                "hidden",
                vec![
                    RibbonItem::new("legacy", RibbonItemKind::Toggle, "L"),
                    RibbonItem::new("preset", RibbonItemKind::Toggle, "P"),
                    RibbonItem::new("missing", RibbonItemKind::Toggle, "M"),
                ],
            )],
        }],
    })
    .unwrap();
    let root = ui.root();
    ribbon.mount(&mut ui, root).unwrap();
    frame(&mut ui, &mut ribbon, 900.0);
    assert_eq!(ui.commands().checked(&"legacy".into()), Some(false));
    assert_eq!(ui.commands().checked(&"preset".into()), Some(true));
    assert!(!ui.commands().contains(&"missing".into()));
    tips(&mut ribbon, &mut ui, &["1"]);
    assert_invocation(&mut ui, "legacy", Some(true));
    ui.commands_mut().set_checked(&"legacy".into(), None);
    ui.commands_mut().register("missing", "Later plain command");
    ribbon.set_context(Some("selected"));
    for (theme, width) in [
        (Theme::FluentDark, 900.0),
        (Theme::Compact, 100.0),
        (Theme::Light, 900.0),
    ] {
        ui.set_theme(theme).unwrap();
        frame(&mut ui, &mut ribbon, width);
        assert_eq!(ui.commands().checked(&"legacy".into()), None);
        assert_eq!(ui.commands().checked(&"missing".into()), None);
        assert_eq!(ui.commands().checked(&"preset".into()), Some(true));
        assert!(ui.commands_mut().drain_invocations().is_empty());
    }
    ribbon.mount(&mut ui, root).unwrap();
    assert_eq!(ui.commands().checked(&"legacy".into()), None);
    assert_eq!(ui.commands().checked(&"missing".into()), None);
}

#[test]
fn registry_disabled_and_can_execute_rejections_do_not_flip_emit_or_close_popup() {
    let (mut ui, mut ribbon, callbacks) = fixture();
    let gate = Rc::new(Cell::new(false));
    let check = gate.clone();
    ui.commands_mut()
        .set_can_execute(&"flag".into(), move || check.get());
    for id in placements(&ui, &ribbon, "flag") {
        ui.activate(id).unwrap();
    }
    tips(&mut ribbon, &mut ui, &["1"]);
    tips(&mut ribbon, &mut ui, &["H", "F"]);
    frame(&mut ui, &mut ribbon, 100.0);
    open_menu(&mut ui, &mut ribbon, "flag");
    let id = menu_item(&ui, "flag");
    ui.activate(id).unwrap();
    assert!(ui.popup_open());
    assert_eq!(ui.commands().checked(&"flag".into()), Some(false));
    assert_eq!(callbacks.get(), 0);
    assert!(ui.commands_mut().drain_invocations().is_empty());
    gate.set(true);
    ui.commands_mut().set_enabled(&"flag".into(), false);
    ui.activate(id).unwrap();
    assert!(ui.popup_open());
    assert_eq!(callbacks.get(), 0);
    ui.commands_mut().set_enabled(&"flag".into(), true);
    ui.activate(id).unwrap();
    assert!(!ui.popup_open());
    assert_invocation(&mut ui, "flag", Some(true));
    assert_eq!(callbacks.get(), 1);
}

#[test]
fn keytips_reject_unmounted_removed_hidden_and_disabled_ribbons_and_clear_prefix() {
    let (mut ui, mut ribbon, callbacks) = fixture();
    let mut unmounted = Ribbon::new(ribbon.model().clone()).unwrap();
    assert!(!key(&mut unmounted, &mut ui, Key::Alt));
    let container = ribbon.container().unwrap();
    assert!(key(&mut ribbon, &mut ui, Key::Alt));
    assert!(key(&mut ribbon, &mut ui, Key::Character("H".into())));
    ui.set_enabled(container, false).unwrap();
    assert!(!key(&mut ribbon, &mut ui, Key::Character("F".into())));
    assert!(!ribbon.key_tips_visible());
    ui.set_enabled(container, true).unwrap();
    assert!(!key(&mut ribbon, &mut ui, Key::Character("F".into())));
    let root = ui.root();
    let mut props = ui.runtime.tree().node(root).unwrap().props.clone();
    props.enabled = false;
    ui.runtime
        .apply(Mutation::SetProps { node: root, props })
        .unwrap();
    assert!(!key(&mut ribbon, &mut ui, Key::Alt));
    let mut props = ui.runtime.tree().node(root).unwrap().props.clone();
    props.enabled = true;
    ui.runtime
        .apply(Mutation::SetProps { node: root, props })
        .unwrap();
    let mut props = ui.runtime.tree().node(root).unwrap().props.clone();
    props.visible = false;
    ui.runtime
        .apply(Mutation::SetProps { node: root, props })
        .unwrap();
    assert!(!key(&mut ribbon, &mut ui, Key::Alt));
    let mut props = ui.runtime.tree().node(root).unwrap().props.clone();
    props.visible = true;
    ui.runtime
        .apply(Mutation::SetProps { node: root, props })
        .unwrap();
    ui.remove(container).unwrap();
    assert!(!key(&mut ribbon, &mut ui, Key::Alt));
    assert_eq!(callbacks.get(), 0);
    assert!(ui.commands_mut().drain_invocations().is_empty());
}

#[test]
fn theme_width_context_and_qat_rebuilds_preserve_state_and_restore_focus_deterministically() {
    let (mut ui, mut ribbon, callbacks) = fixture();
    ui.commands_mut().set_checked(&"flag".into(), Some(true));
    for theme in [
        Theme::FluentDark,
        Theme::Light,
        Theme::Dark,
        Theme::Compact,
        Theme::FluentLight,
    ] {
        let before = placements(&ui, &ribbon, "flag");
        ui.runtime
            .apply(Mutation::Focus(before.last().copied()))
            .unwrap();
        ui.set_theme(theme).unwrap();
        frame(&mut ui, &mut ribbon, 900.0);
        let after = placements(&ui, &ribbon, "flag");
        assert_eq!(ui.runtime.focused(), after.first().copied());
        assert_state(&ui, "flag", &after, Some(true));
        for width in [300.0, 100.0, 900.0] {
            frame(&mut ui, &mut ribbon, width);
            assert_eq!(ui.commands().checked(&"flag".into()), Some(true));
        }
    }
    ribbon.remove_quick_access(&"flag".into());
    frame(&mut ui, &mut ribbon, 900.0);
    ribbon.add_quick_access("flag".into());
    ribbon.set_context(Some("irrelevant"));
    frame(&mut ui, &mut ribbon, 900.0);
    assert_eq!(ui.commands().checked(&"flag".into()), Some(true));
    assert_eq!(callbacks.get(), 0);
    assert!(ui.commands_mut().drain_invocations().is_empty());
}

#[test]
fn rejected_remount_preserves_the_live_ribbon_and_initialization_policy() {
    let (mut ui, mut ribbon, _) = fixture();
    let original_parent = ui.root();
    let other_parent = ui.panel(original_parent, LayoutStyle::default()).unwrap();
    frame(&mut ui, &mut ribbon, 900.0);
    let container = ribbon.container().unwrap();
    let ids = placements(&ui, &ribbon, "flag");
    let scene = ui.scene().clone();
    let dirty = ribbon.dirty;
    ui.commands_mut().set_checked(&"flag".into(), None);
    assert!(ribbon.mount(&mut ui, other_parent).is_err());
    assert_eq!(ribbon.parent, Some(original_parent));
    assert_eq!(ribbon.container(), Some(container));
    assert_eq!(ui.runtime.tree().parent(container), Some(original_parent));
    assert_eq!(placements(&ui, &ribbon, "flag"), ids);
    assert_eq!(ribbon.dirty, dirty);
    assert_eq!(ui.scene(), &scene);
    assert!(ribbon.legacy_toggles_initialized);
    assert_eq!(ui.commands().checked(&"flag".into()), None);
    // The original live mount remains usable after rejection.
    ribbon.mount(&mut ui, original_parent).unwrap();
    frame(&mut ui, &mut ribbon, 900.0);
    assert_eq!(ui.commands().checked(&"flag".into()), None);

    ui.remove(container).unwrap();
    let dirty = ribbon.dirty;
    let action_count = ribbon.actions.len();
    let error = ribbon.mount(&mut ui, original_parent).unwrap_err();
    assert!(error.to_string().contains("container was removed"));
    assert_eq!(ribbon.parent, Some(original_parent));
    assert_eq!(ribbon.container(), Some(container));
    assert_eq!(ribbon.dirty, dirty);
    assert_eq!(ribbon.actions.len(), action_count);
    assert!(ribbon.legacy_toggles_initialized);

    let mut fresh = Ribbon::new(ribbon.model().clone()).unwrap();
    assert!(fresh.mount(&mut ui, container).is_err());
    assert_eq!(fresh.parent, None);
    assert_eq!(fresh.container(), None);
    assert!(!fresh.legacy_toggles_initialized);
    assert_eq!(ui.commands().checked(&"flag".into()), None);
}

#[test]
fn empty_split_and_empty_group_keep_disabled_buttons_without_empty_popups() {
    let (mut ui, mut ribbon, _) = fixture();
    if let RibbonItemKind::Split { menu } = &mut ribbon.model.tabs[0].groups[0].items[1].kind {
        menu.clear();
    }
    ribbon.model.tabs[0].groups.push(RibbonGroup {
        id: "empty".into(),
        label: "Empty".into(),
        collapse_priority: 0,
        items: Vec::new(),
    });
    ribbon.dirty = true;
    frame(&mut ui, &mut ribbon, 900.0);
    let empty_split = ui
        .semantics()
        .into_iter()
        .find(|node| node.label == "More" && node.disabled && !node.has_popup)
        .expect("empty Split secondary remains present and disabled");
    ui.activate(empty_split.id).unwrap();
    assert!(!ui.popup_open());
    let primary = placements(&ui, &ribbon, "primary")[0];
    ui.activate(primary).unwrap();
    assert_invocation(&mut ui, "primary", Some(true));
    // An empty-only model can also collapse below its normal group width.
    let mut empty = Ribbon::new(RibbonModel {
        tabs: vec![RibbonTab {
            id: "empty-tab".into(),
            label: "Empty".into(),
            key_tip: "E".into(),
            context: None,
            groups: vec![ribbon.model.tabs[0].groups.last().unwrap().clone()],
        }],
        quick_access: Vec::new(),
    })
    .unwrap();
    let root = ui.root();
    empty.mount(&mut ui, root).unwrap();
    empty.update(&mut ui, 19.0).unwrap();
    ui.update(Size::new(900.0, 600.0)).unwrap();
    let all = ui
        .semantics()
        .into_iter()
        .find(|node| node.label == "All commands")
        .unwrap();
    assert!(all.disabled);
    assert!(!all.has_popup);
}

#[test]
fn menu_count_rejection_preserves_tree_popup_and_command_state_before_rebuild() {
    let (mut ui, mut ribbon, _) = fixture();
    ribbon.model.quick_access = (0..260)
        .map(|index| format!("qat-{index}").into())
        .collect();
    ribbon.dirty = true;
    frame(&mut ui, &mut ribbon, 900.0);
    let anchor = ui
        .runtime
        .tree()
        .preorder()
        .into_iter()
        .find(|id| matches!(ribbon.actions.get(id), Some(Action::Dropdown)))
        .unwrap();
    ui.activate(anchor).unwrap();
    ribbon.process(&mut ui).unwrap();
    assert!(ui.popup_open());
    let ids = ui.runtime.tree().preorder();
    let width = ribbon.width;
    let presentations = ribbon.presentations().to_vec();
    let error = ribbon.update(&mut ui, 20.0).unwrap_err();
    assert!(error.to_string().contains("QAT overflow"));
    assert!(error.to_string().contains("256"));
    assert_eq!(ui.runtime.tree().preorder(), ids);
    assert!(ui.popup_open());
    assert_eq!(ribbon.width, width);
    assert_eq!(ribbon.presentations(), presentations);
    // A model edit is retained for correction, while its failed rebuild leaves
    // the currently usable visual tree and command values untouched.
    ribbon.add_quick_access("over-limit-1".into());
    ribbon.add_quick_access("over-limit-2".into());
    ribbon.add_quick_access("over-limit-3".into());
    ribbon.add_quick_access("over-limit-4".into());
    assert!(ribbon.update(&mut ui, 900.0).is_err());
    assert_eq!(ui.runtime.tree().preorder(), ids);
    ui.commands_mut().set_checked(&"flag".into(), None);
    let mut fresh = Ribbon::new(ribbon.model().clone()).unwrap();
    let root = ui.root();
    assert!(fresh.mount(&mut ui, root).is_err());
    assert_eq!(fresh.parent, None);
    assert_eq!(fresh.container(), None);
    assert!(!fresh.legacy_toggles_initialized);
    assert_eq!(ui.commands().checked(&"flag".into()), None);
    assert_eq!(ui.runtime.tree().preorder(), ids);
}
