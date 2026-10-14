use super::*;
use crate::{CommandInvocation, SemanticAction};
use rust_desktop_ui_core::{Modifiers, Point, PointerButton, Size};
use std::{cell::Cell, rc::Rc};

// Global-to-local clip subtraction can lose a few f32 ULPs. Permit only a
// thousandth of a logical pixel; actual visible glyph clipping still fails.
const TEXT_GEOMETRY_EPSILON: f32 = 0.001;

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
    frame(&mut ui, &mut ribbon, 60.0);
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
    for width in [200.0, 60.0] {
        frame(&mut ui, &mut ribbon, width);
        if width == 200.0 {
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
            ("work / flag", "flag", Some(width == 200.0)),
            ("work / primary", "primary", Some(width == 200.0)),
            ("work / secondary", "secondary", Some(width == 200.0)),
            ("other / plain", "plain", None),
        ] {
            // At medium width the other group remains expanded, so test its
            // ordinary command through the corresponding body placement.
            if width == 200.0 && command == "plain" {
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
    for width in [900.0, 300.0, 60.0] {
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
        (Theme::Compact, 60.0),
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
    frame(&mut ui, &mut ribbon, 60.0);
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
        for width in [300.0, 60.0, 900.0] {
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
        .find(|node| node.label == "primary menu" && node.disabled && !node.has_popup)
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
    // Compact text-only QAT fits more entries than the previous wide buttons.
    // Exceed the actual capacity, rather than relying on a legacy 104px width.
    let capacity = RibbonWidths::measure(
        &mut ui,
        &ribbon.model,
        &ribbon.command_appearances,
        ribbon.tips,
    )
    .unwrap()
    .qat_capacity(888.0);
    while ribbon.model.quick_access.len() <= capacity + crate::MAX_MENU_ITEMS {
        ribbon.add_quick_access(format!("over-limit-{}", ribbon.model.quick_access.len()).into());
    }
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

#[test]
fn explicit_mount_width_avoids_an_invalid_intermediate_default_layout() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let mut ribbon = Ribbon::new(RibbonModel {
        quick_access: (0..270)
            .map(|index| format!("command-{index}").into())
            .collect(),
        tabs: vec![],
    })
    .unwrap();
    let initial = ui.runtime.tree().preorder();
    assert!(ribbon.preflight(&mut ui, 800.0).is_err());
    let height = ribbon.preflight(&mut ui, 2200.0).unwrap();
    assert_eq!(ui.runtime.tree().preorder(), initial);
    assert_eq!(ribbon.container(), None);
    let root = ui.root();
    let container = ribbon.mount_with_width(&mut ui, root, 2200.0).unwrap();
    assert_eq!(ribbon.width, 2200.0);
    assert_eq!(
        ui.runtime
            .tree()
            .node(container)
            .unwrap()
            .props
            .style
            .height,
        Length::Px(height)
    );
    assert!(ui.commands_mut().drain_invocations().is_empty());
}

#[test]
fn text_and_tooltip_preflight_failures_preserve_live_tree_popup_and_legacy_state() {
    let (mut ui, mut ribbon, _) = fixture();
    frame(&mut ui, &mut ribbon, 60.0);
    open_menu(&mut ui, &mut ribbon, "flag");
    let before = ui.runtime.tree().preorder();
    let focused = ui.runtime.focused();
    let presentations = ribbon.presentations().to_vec();
    ribbon.model.tabs[0].groups[0].label = "x".repeat(1024 * 1024 + 1);
    ribbon.dirty = true;
    assert!(ribbon.update(&mut ui, 900.0).is_err());
    assert_eq!(ui.runtime.tree().preorder(), before);
    assert_eq!(ui.runtime.focused(), focused);
    assert_eq!(ribbon.presentations(), presentations);
    assert!(ui.popup_open());

    let mut fresh = Ribbon::new(RibbonModel {
        quick_access: vec![],
        tabs: vec![RibbonTab {
            id: "home".into(),
            label: "Home".into(),
            key_tip: "H".into(),
            context: None,
            groups: vec![group(
                "legacy",
                vec![RibbonItem::new("plain", RibbonItemKind::Toggle, "L")],
            )],
        }],
    })
    .unwrap();
    ui.commands_mut().register("plain", "x".repeat(4097));
    let root = ui.root();
    for width in [f32::NAN, -1.0, 900.0] {
        assert!(fresh.mount_with_width(&mut ui, root, width).is_err());
        assert_eq!(fresh.parent, None);
        assert_eq!(fresh.container(), None);
        assert!(!fresh.legacy_toggles_initialized);
        assert_eq!(ui.commands().checked(&"plain".into()), None);
        assert_eq!(ui.runtime.tree().preorder(), before);
    }
}

#[test]
fn generated_menu_text_is_checked_before_removing_existing_children() {
    let (mut ui, mut ribbon, _) = fixture();
    let before = ui.runtime.tree().preorder();
    let old_width = ribbon.width;
    // A secondary command has no body tooltip: this specifically exercises the
    // generated menu's own aggregate text validation.
    ui.commands_mut()
        .register("secondary", "x".repeat(1024 * 1024));
    ribbon.dirty = true;
    let error = ribbon.update(&mut ui, 901.0).unwrap_err();
    assert!(error.to_string().contains("Menu text exceeds"));
    assert_eq!(ui.runtime.tree().preorder(), before);
    assert_eq!(ribbon.width, old_width);
}

#[test]
fn invalid_keytip_badge_preflight_preserves_the_live_ribbon() {
    let (mut ui, mut ribbon, _) = fixture();
    let before = ui.runtime.tree().preorder();
    ribbon.model.tabs[0].key_tip = "Z".repeat(1024 * 1024 + 1);
    ribbon.dirty = true;
    assert!(ribbon.update(&mut ui, 900.0).is_err());
    assert_eq!(ui.runtime.tree().preorder(), before);
    assert!(ui.commands_mut().drain_invocations().is_empty());
}

#[test]
fn command_appearance_is_shared_by_qat_and_body_and_rejected_atomically() {
    let (mut ui, mut ribbon, _) = fixture();
    let appearance = RibbonCommandAppearance {
        icon: Some(ButtonIcon::new(crate::IconSymbol::Pin)),
        tool_tip: Some("Pin this document".into()),
    };
    ribbon
        .set_command_appearance("flag", appearance.clone())
        .unwrap();
    frame(&mut ui, &mut ribbon, 900.0);
    for id in placements(&ui, &ribbon, "flag") {
        assert_eq!(
            ui.button_icon(id).map(|icon| (icon.symbol, icon.size)),
            appearance.icon.map(|icon| (icon.symbol, 16.0))
        );
        assert_eq!(ui.tooltip(id), Some("Pin this document"));
    }
    let before = ui.runtime.tree().preorder();
    for invalid in [
        RibbonCommandAppearance {
            icon: Some(ButtonIcon {
                size: f32::NAN,
                ..ButtonIcon::new(crate::IconSymbol::Pin)
            }),
            ..Default::default()
        },
        RibbonCommandAppearance {
            tool_tip: Some("x".repeat(4097)),
            ..Default::default()
        },
    ] {
        assert!(ribbon.set_command_appearance("flag", invalid).is_err());
        assert_eq!(
            ribbon.command_appearances.get(&"flag".into()),
            Some(&appearance)
        );
        assert!(!ribbon.dirty);
        assert_eq!(ui.runtime.tree().preorder(), before);
    }
    ui.set_theme(Theme::FluentDark).unwrap();
    frame(&mut ui, &mut ribbon, 300.0);
    frame(&mut ui, &mut ribbon, 900.0);
    for id in placements(&ui, &ribbon, "flag") {
        assert_eq!(
            ui.button_icon(id).map(|icon| (icon.symbol, icon.size)),
            appearance.icon.map(|icon| (icon.symbol, 16.0))
        );
        assert_eq!(ui.tooltip(id), Some("Pin this document"));
    }
    ribbon
        .set_command_appearance(
            "flag",
            RibbonCommandAppearance {
                tool_tip: Some(String::new()),
                ..Default::default()
            },
        )
        .unwrap();
    frame(&mut ui, &mut ribbon, 900.0);
    for id in placements(&ui, &ribbon, "flag") {
        assert_eq!(ui.button_icon(id), None);
        assert_eq!(ui.tooltip(id), None);
    }
}

#[test]
fn rebuilding_ribbon_preserves_sibling_editor_session_and_foreign_popup() {
    let (mut ui, mut ribbon, _) = fixture();
    let root = ui.root();
    let editor = ui
        .edit_box(
            root,
            "Document",
            "",
            LayoutStyle {
                height: Length::Px(36.0),
                ..Default::default()
            },
        )
        .unwrap();
    ui.set_editor_validation(
        editor,
        crate::EditorValidation {
            is_required: true,
            ..Default::default()
        },
    )
    .unwrap();
    ui.runtime.apply(Mutation::Focus(Some(editor))).unwrap();
    ui.text_input(" ").unwrap();
    assert!(matches!(
        ui.commit_edit(editor).unwrap(),
        crate::CommitOutcome::Invalid(_)
    ));
    let selection = ui.document(editor).unwrap().selection();
    let diagnostic = ui.validation_error(editor).unwrap().to_owned();
    assert!(ui.document(editor).unwrap().can_undo());
    frame(&mut ui, &mut ribbon, 300.0);
    assert_eq!(ui.runtime.focused(), Some(editor));
    let anchor = ui
        .dropdown_button(
            root,
            "External",
            &[MenuEntry::Item(MenuItem::new("Plain", "plain"))],
            LayoutStyle {
                height: Length::Px(36.0),
                ..Default::default()
            },
        )
        .unwrap();
    ui.update(Size::new(900.0, 600.0)).unwrap();
    ui.activate(anchor).unwrap();
    let focused = ui.runtime.focused();
    let menu_ids: Vec<_> = ui
        .semantics()
        .iter()
        .filter(|node| matches!(node.role, Role::Menu | Role::MenuItem))
        .map(|node| node.id)
        .collect();
    assert!(ui.popup_open());
    frame(&mut ui, &mut ribbon, 900.0);
    assert!(ui.popup_open());
    assert_eq!(ui.runtime.focused(), focused);
    assert_eq!(ui.popup_anchor(), Some(anchor));
    assert!(menu_ids.iter().all(|id| ui.runtime.tree().contains(*id)));
    assert_eq!(ui.document(editor).unwrap().text(), " ");
    assert_eq!(ui.document(editor).unwrap().selection(), selection);
    assert!(ui.document(editor).unwrap().can_undo());
    assert_eq!(ui.accepted_value(editor), Some(""));
    assert_eq!(ui.validation_error(editor), Some(diagnostic.as_str()));
}

#[test]
fn keytips_rebuild_keeps_tab_focus_and_escape_or_command_returns_to_editor() {
    let (mut ui, mut ribbon, _) = fixture();
    let root = ui.root();
    let editor = ui
        .edit_box(root, "Document", "draft", LayoutStyle::default())
        .unwrap();
    ui.runtime.apply(Mutation::Focus(Some(editor))).unwrap();
    assert!(key(&mut ribbon, &mut ui, Key::Alt));
    frame(&mut ui, &mut ribbon, 900.0);
    assert_eq!(ui.runtime.focused(), ribbon.tabs.get("home").copied());
    assert!(key(&mut ribbon, &mut ui, Key::Character("H".into())));
    frame(&mut ui, &mut ribbon, 900.0);
    assert_eq!(ui.runtime.focused(), ribbon.tabs.get("home").copied());
    assert!(key(&mut ribbon, &mut ui, Key::Escape));
    frame(&mut ui, &mut ribbon, 900.0);
    assert_eq!(ui.runtime.focused(), ribbon.tabs.get("home").copied());
    assert!(key(&mut ribbon, &mut ui, Key::Escape));
    frame(&mut ui, &mut ribbon, 900.0);
    assert_eq!(ui.runtime.focused(), Some(editor));
    tips(&mut ribbon, &mut ui, &["H", "F"]);
    frame(&mut ui, &mut ribbon, 900.0);
    assert_eq!(ui.runtime.focused(), Some(editor));
    assert_invocation(&mut ui, "flag", Some(true));
    assert_eq!(ui.document(editor).unwrap().text(), "draft");
}

#[test]
fn pointer_blur_and_tab_cancel_keytips_without_forcing_previous_focus() {
    let (mut ui, mut ribbon, _) = fixture();
    let root = ui.root();
    let editor = ui
        .edit_box(root, "Document", "", LayoutStyle::default())
        .unwrap();
    for event in [
        InputEvent::PointerDown {
            position: Point::new(1.0, 1.0),
            button: PointerButton::Primary,
            modifiers: Modifiers::default(),
        },
        InputEvent::WindowFocused(false),
        InputEvent::KeyDown {
            key: Key::Tab,
            repeat: false,
            modifiers: Modifiers::default(),
        },
    ] {
        ui.runtime.apply(Mutation::Focus(Some(editor))).unwrap();
        assert!(key(&mut ribbon, &mut ui, Key::Alt));
        let current = ui.runtime.focused();
        assert!(!ribbon.handle_input(&mut ui, &event).unwrap());
        assert!(!ribbon.key_tips_visible());
        assert_eq!(ribbon.key_tip_return_focus, None);
        assert_eq!(ui.runtime.focused(), current);
        assert_ne!(ui.runtime.focused(), Some(editor));
    }
}

#[test]
fn tab_arrows_home_and_end_switch_visible_tabs_and_keep_focus_after_reflow() {
    let (mut ui, mut ribbon, _) = fixture();
    ribbon.model.tabs.extend([
        RibbonTab {
            id: "context".into(),
            label: "Context".into(),
            key_tip: "C".into(),
            context: Some("selected".into()),
            groups: vec![],
        },
        RibbonTab {
            id: "view".into(),
            label: "View".into(),
            key_tip: "V".into(),
            context: None,
            groups: vec![],
        },
    ]);
    ribbon.dirty = true;
    frame(&mut ui, &mut ribbon, 900.0);
    ui.runtime
        .apply(Mutation::Focus(ribbon.tabs.get("home").copied()))
        .unwrap();
    for (input, selected) in [
        (Key::ArrowRight, "view"),
        (Key::ArrowRight, "home"),
        (Key::ArrowLeft, "view"),
        (Key::Home, "home"),
        (Key::End, "view"),
    ] {
        assert!(key(&mut ribbon, &mut ui, input));
        frame(&mut ui, &mut ribbon, 900.0);
        assert_eq!(ribbon.active_tab(), selected);
        assert_eq!(ui.runtime.focused(), ribbon.tabs.get(selected).copied());
    }
    ribbon.set_context(Some("selected"));
    frame(&mut ui, &mut ribbon, 300.0);
    assert!(key(&mut ribbon, &mut ui, Key::ArrowLeft));
    frame(&mut ui, &mut ribbon, 300.0);
    assert_eq!(ribbon.active_tab(), "context");
    assert_eq!(ui.runtime.focused(), ribbon.tabs.get("context").copied());
    ribbon.set_context(None);
    frame(&mut ui, &mut ribbon, 300.0);
    assert_eq!(ui.runtime.focused(), ribbon.tabs.get("home").copied());
}

#[test]
fn measured_visual_captions_fit_small_and_split_text_clips_in_each_theme_and_keytip_state() {
    let (mut ui, mut ribbon, _) = fixture();
    let labels = [
        ("flag", "Закрепить текущий документ"),
        ("primary", "Подтвердить значение"),
        ("plain", "Действие с длинной подписью"),
    ];
    for (command, label) in labels {
        ui.commands_mut().register(command, label);
        ribbon
            .set_command_appearance(
                command,
                RibbonCommandAppearance {
                    icon: Some(ButtonIcon::new(crate::IconSymbol::Pin)),
                    ..Default::default()
                },
            )
            .unwrap();
    }
    for theme in [Theme::FluentLight, Theme::FluentDark, Theme::Compact] {
        ui.set_theme(theme).unwrap();
        for tips in [Tips::Hidden, Tips::Tabs, Tips::Commands] {
            ribbon.tips = tips;
            ribbon.dirty = true;
            frame(&mut ui, &mut ribbon, 2200.0);
            assert!(
                ribbon
                    .presentations()
                    .iter()
                    .all(|p| matches!(p, RibbonPresentation::Full(_)))
            );
            let runs: Vec<_> = ui
                .scene()
                .texts
                .iter()
                .filter(|run| labels.iter().any(|(_, label)| run.text.starts_with(label)))
                .cloned()
                .collect();
            assert_eq!(
                runs.len(),
                3,
                "QAT is icon-only; Small x2 and Split caption must paint: {:?}",
                ui.scene()
                    .texts
                    .iter()
                    .map(|run| (&run.text, run.bounds, run.clip))
                    .collect::<Vec<_>>()
            );
            for run in runs {
                let measured = ui
                    .measure_text(&run.text, run.bounds.width, run.wrap)
                    .unwrap();
                assert!(
                    measured.width <= run.clip.width + TEXT_GEOMETRY_EPSILON,
                    "{theme:?}/{tips:?} {:?}: shaped {} exceeds actual text clip {}",
                    run.text,
                    measured.width,
                    run.clip.width
                );
                assert!(run.clip.x >= run.bounds.x - 0.01);
                assert!(
                    measured.height <= run.clip.height + TEXT_GEOMETRY_EPSILON,
                    "{theme:?}/{tips:?} {:?}: shaped height {} exceeds clip {}; bounds {:?}, clip {:?}",
                    run.text,
                    measured.height,
                    run.clip.height,
                    run.bounds,
                    run.clip
                );
            }
        }
    }
}

#[test]
fn measured_widths_drive_qat_overflow_and_group_collapse_then_restore_full_content() {
    let (mut ui, mut ribbon, _) = fixture();
    ui.commands_mut()
        .register("flag", "Закрепить выбранный документ");
    ribbon
        .set_command_appearance(
            "flag",
            RibbonCommandAppearance {
                icon: Some(ButtonIcon::new(crate::IconSymbol::Pin)),
                ..Default::default()
            },
        )
        .unwrap();
    let profile = RibbonWidths::measure(
        &mut ui,
        &ribbon.model,
        &ribbon.command_appearances,
        Tips::Hidden,
    )
    .unwrap();
    let required = profile.group("home", "work").items[0];
    assert!(required > 128.0 && required < MAX_COMMAND_WIDTH);
    assert_eq!(profile.qat[0], QAT_TARGET);
    // One pixel below icon-only QAT + menu reserve hides the first command.
    let width = QAT_TARGET * 2.0 + 4.0 - 1.0 + 12.0;
    frame(&mut ui, &mut ribbon, width);
    let qat = ui
        .runtime
        .tree()
        .children(ribbon.container().unwrap())
        .unwrap()[0];
    let qat_children = ui.runtime.tree().children(qat).unwrap();
    assert_eq!(qat_children.len(), 1);
    let more = qat_children[0];
    assert_eq!(ui.command_id(more), None);
    assert!(matches!(
        ribbon.presentations(),
        [RibbonPresentation::Overflow(_)]
    ));
    ui.activate(more).unwrap();
    ribbon.process(&mut ui).unwrap();
    assert_eq!(
        ui.semantics()
            .iter()
            .filter(|node| node.role == Role::MenuItem)
            .count(),
        2
    );
    ui.close_popup().unwrap();
    frame(&mut ui, &mut ribbon, 2200.0);
    assert!(
        ribbon
            .presentations()
            .iter()
            .all(|p| matches!(p, RibbonPresentation::Full(_)))
    );
    let ids = placements(&ui, &ribbon, "flag");
    assert_eq!(ids.len(), 2);
    assert_eq!(ui.runtime.bounds(ids[0]).unwrap().width, QAT_TARGET);
    assert!(ui.runtime.bounds(ids[1]).unwrap().width >= required);
    assert!(ui.commands_mut().drain_invocations().is_empty());
}

#[test]
fn measured_qat_capacity_is_used_for_atomic_menu_preflight() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    ui.commands_mut().register("command-0", "W".repeat(100));
    let mut ribbon = Ribbon::new(RibbonModel {
        tabs: vec![],
        quick_access: (0..257)
            .map(|index| format!("command-{index}").into())
            .collect(),
    })
    .unwrap();
    let root = ui.root();
    ribbon.mount_with_width(&mut ui, root, 1000.0).unwrap();
    frame(&mut ui, &mut ribbon, 1000.0);
    let first = ui
        .runtime
        .tree()
        .preorder()
        .into_iter()
        .find(|id| ui.command_id(*id).is_some_and(|id| id.0 == "command-0"))
        .unwrap();
    assert_eq!(ui.runtime.bounds(first).unwrap().width, MAX_COMMAND_WIDTH);
    let before = ui.runtime.tree().preorder();
    let error = ribbon.update(&mut ui, 320.0).unwrap_err();
    assert!(error.to_string().contains("QAT overflow would contain 257"));
    assert_eq!(ui.runtime.tree().preorder(), before);
    assert_eq!(ribbon.width, 1000.0);
}

#[test]
fn measured_collapsed_russian_headers_and_commands_fit_their_actual_dropdown_text_clips() {
    let (mut ui, mut ribbon, _) = fixture();
    ribbon.model.tabs[0].groups[0].label = "Документ".into();
    ribbon.model.tabs[0].groups[1].label = "Редактирование".into();
    ribbon.model.tabs[0].groups[1].items.push(RibbonItem::new(
        "primary",
        RibbonItemKind::Large,
        "X",
    ));
    // Both expanded groups exceed the viewport; their complete collapsed names
    // still fit. Extra commands make this independent of compact item widths.
    for index in 0..3 {
        for group in &mut ribbon.model.tabs[0].groups {
            group.items.push(RibbonItem::new(
                "primary",
                RibbonItemKind::Large,
                format!("Z{index}"),
            ));
        }
    }
    for theme in [Theme::FluentLight, Theme::FluentDark, Theme::Compact] {
        ui.set_theme(theme).unwrap();
        ribbon.dirty = true;
        // Equivalent to a 360px form with 16px outer padding on either side.
        frame(&mut ui, &mut ribbon, 328.0);
        assert_eq!(
            ribbon
                .presentations()
                .iter()
                .filter(|p| matches!(p, RibbonPresentation::Collapsed(_)))
                .count(),
            2
        );
        let runs: Vec<_> = ui
            .scene()
            .texts
            .iter()
            .filter(|run| matches!(run.text.as_str(), "Документ" | "Редактирование"))
            .cloned()
            .collect();
        assert_eq!(runs.len(), 2);
        for run in runs {
            let shaped = ui.measure_text(&run.text, 4096.0, false).unwrap();
            assert!(
                shaped.width <= run.clip.width + TEXT_GEOMETRY_EPSILON,
                "{theme:?} {:?}: {}px text exceeds {}px dropdown clip",
                run.text,
                shaped.width,
                run.clip.width
            );
            assert!(shaped.height <= run.clip.height + TEXT_GEOMETRY_EPSILON);
            assert!(
                run.clip.x >= -TEXT_GEOMETRY_EPSILON
                    && run.clip.x + run.clip.width <= 328.0 + TEXT_GEOMETRY_EPSILON
            );
        }
        frame(&mut ui, &mut ribbon, 148.0);
        assert!(matches!(
            ribbon.presentations(),
            [RibbonPresentation::Overflow(_)]
        ));
        assert!(
            !ui.scene()
                .texts
                .iter()
                .any(|run| matches!(run.text.as_str(), "Документ" | "Редактирование"))
        );
    }
}

#[test]
fn a_wide_collapsed_header_never_expands_an_already_narrow_full_group() {
    let (mut ui, mut ribbon, _) = fixture();
    ribbon.model.tabs[0].groups[0].label = "Название группы длиннее её единственной кнопки".into();
    ribbon.model.tabs[0].groups[0].items =
        vec![RibbonItem::new("flag", RibbonItemKind::Small, "F")];
    ribbon.model.tabs[0].groups[1].items = vec![
        RibbonItem::new("primary", RibbonItemKind::Large, "P"),
        RibbonItem::new("plain", RibbonItemKind::Large, "O"),
    ];
    ribbon.dirty = true;
    frame(&mut ui, &mut ribbon, 200.0);
    assert_eq!(
        ribbon.presentations(),
        &[
            RibbonPresentation::Full("work".into()),
            RibbonPresentation::Collapsed("other".into()),
        ]
    );
    let body = placements(&ui, &ribbon, "flag")
        .into_iter()
        .find(|id| {
            ui.runtime.bounds(*id).unwrap().height > 0.0 && ui.runtime.bounds(*id).unwrap().y > 60.0
        })
        .unwrap();
    assert_eq!(ui.runtime.bounds(body).unwrap().width, 56.0);
    frame(&mut ui, &mut ribbon, 60.0);
    assert!(matches!(
        ribbon.presentations(),
        [RibbonPresentation::Overflow(_)]
    ));
}

#[test]
fn fluent_ribbon_uses_compact_qat_three_small_rows_and_integrated_large_split() {
    let (mut ui, mut ribbon, _) = fixture();
    ribbon.model.tabs[0].groups[0].items.splice(
        1..1,
        [
            RibbonItem::new("plain", RibbonItemKind::Small, "A"),
            RibbonItem::new("secondary", RibbonItemKind::Small, "S"),
        ],
    );
    ribbon.model.tabs[0].groups[1].items[0].kind = RibbonItemKind::Large;
    for command in ["flag", "plain", "primary", "secondary"] {
        ribbon
            .set_command_appearance(
                command,
                RibbonCommandAppearance {
                    icon: Some(ButtonIcon::new(crate::IconSymbol::Save)),
                    ..Default::default()
                },
            )
            .unwrap();
    }
    frame(&mut ui, &mut ribbon, 900.0);
    let flag = placements(&ui, &ribbon, "flag");
    let qat = flag[0];
    let small = flag[1];
    assert_eq!(ui.runtime.bounds(qat).unwrap().width, QAT_TARGET);
    assert_eq!(
        ui.button_content_layout(qat),
        Some(ButtonContentLayout::IconOnly)
    );
    assert_eq!(ui.button_icon(qat).unwrap().size, 16.0);
    assert_eq!(ui.tooltip(qat), Some("flag"));
    let semantics = ui.semantics();
    assert_eq!(
        semantics.iter().find(|node| node.id == qat).unwrap().label,
        "flag"
    );
    let qat_bounds = ui.runtime.bounds(qat).unwrap();
    assert!(
        !ui.scene()
            .texts
            .iter()
            .any(|run| run.text == "flag" && run.bounds.y < qat_bounds.y + qat_bounds.height)
    );

    let column = ui.runtime.tree().parent(small).unwrap();
    let rows = ui.runtime.tree().children(column).unwrap();
    assert_eq!(rows.len(), 3);
    let first = ui.runtime.bounds(rows[0]).unwrap();
    for (index, row) in rows.iter().enumerate() {
        let bounds = ui.runtime.bounds(*row).unwrap();
        assert_eq!(bounds.x, first.x);
        assert_eq!(bounds.height, first.height);
        assert_eq!(bounds.y, first.y + index as f32 * (first.height + 2.0));
        assert_eq!(
            ui.button_content_layout(*row),
            Some(ButtonContentLayout::Leading)
        );
        assert_eq!(ui.button_chrome(*row), Some(ButtonChrome::Flat));
    }
    let primary = placements(&ui, &ribbon, "primary")[0];
    let split = ui.runtime.tree().parent(primary).unwrap();
    let arrow = ui.runtime.tree().children(split).unwrap()[1];
    let primary_bounds = ui.runtime.bounds(primary).unwrap();
    let arrow_bounds = ui.runtime.bounds(arrow).unwrap();
    let split_bounds = ui.runtime.bounds(split).unwrap();
    assert_eq!(ui.button_icon(primary).unwrap().size, 32.0);
    assert_eq!(split_bounds.height, first.height * 3.0 + 4.0);
    assert_eq!(arrow_bounds.y, primary_bounds.y + primary_bounds.height);
    assert_eq!(arrow_bounds.width, primary_bounds.width);
    assert_eq!(
        arrow_bounds.height + primary_bounds.height,
        split_bounds.height
    );
    assert!(!ui.scene().texts.iter().any(|run| run.text == "More"));
    assert_eq!(
        semantics
            .iter()
            .find(|node| node.id == arrow)
            .unwrap()
            .label,
        "primary menu"
    );

    let large = *placements(&ui, &ribbon, "plain").last().unwrap();
    assert_eq!(
        ui.button_content_layout(large),
        Some(ButtonContentLayout::Above)
    );
    assert_eq!(ui.button_icon(large).unwrap().size, 32.0);
    assert!(
        ui.semantics()
            .iter()
            .any(|node| node.role == Role::Separator)
    );
    ui.activate(arrow).unwrap();
    assert!(ui.popup_open());
    assert!(ui.commands_mut().drain_invocations().is_empty());
}

#[test]
fn keytip_badges_preserve_widget_ids_semantic_names_and_every_layout_rectangle() {
    let (mut ui, mut ribbon, _) = fixture();
    let ids = ui.runtime.tree().preorder();
    let snapshot = |ui: &Controls| {
        ui.semantics()
            .into_iter()
            .map(|node| (node.id, node.label, node.bounds, node.role))
            .collect::<Vec<_>>()
    };
    let before = snapshot(&ui);
    let presentations = ribbon.presentations().to_vec();
    for (key_value, expected) in [
        (Key::F10, vec!["1", "2", "H"]),
        (Key::Character("H".into()), vec!["F", "O", "P"]),
        (Key::Escape, vec!["1", "2", "H"]),
        (Key::Escape, vec![]),
    ] {
        assert!(key(&mut ribbon, &mut ui, key_value));
        frame(&mut ui, &mut ribbon, 900.0);
        assert_eq!(ui.runtime.tree().preorder(), ids);
        assert_eq!(snapshot(&ui), before);
        assert_eq!(ribbon.presentations(), presentations);
        let mut badges: Vec<_> = ids.iter().filter_map(|id| ui.key_tip(*id)).collect();
        badges.sort();
        assert_eq!(badges, expected);
    }
    assert!(ui.commands_mut().drain_invocations().is_empty());
}

#[test]
fn bounded_large_and_text_only_fallback_keep_complete_names_and_tabs_never_overlap() {
    let (mut ui, mut ribbon, _) = fixture();
    let label = "Очень длинное название команды с дополнительными словами";
    ui.commands_mut().register("plain", label);
    ribbon.model.tabs[0].groups[1].items[0].kind = RibbonItemKind::Large;
    ribbon.model.tabs.push(RibbonTab {
        id: "view".into(),
        label: "Настройки представления".into(),
        key_tip: "V".into(),
        context: None,
        groups: vec![],
    });
    ribbon.dirty = true;
    frame(&mut ui, &mut ribbon, 900.0);
    let plain = placements(&ui, &ribbon, "plain");
    assert_eq!(plain.len(), 2);
    assert!(ui.runtime.bounds(plain[0]).unwrap().width > QAT_TARGET);
    assert_eq!(ui.button_icon(plain[0]), None);
    let large = plain[1];
    assert!(ui.runtime.bounds(large).unwrap().width <= 160.0);
    assert_eq!(
        ui.button_content_layout(large),
        Some(ButtonContentLayout::Above)
    );
    let node = ui
        .semantics()
        .into_iter()
        .find(|node| node.id == large)
        .unwrap();
    assert_eq!(node.label, label);
    assert_eq!(node.tooltip.as_deref(), Some(label));
    let bounds = ui.runtime.bounds(large).unwrap();
    let caption = ui
        .scene()
        .texts
        .iter()
        .find(|run| run.bounds.intersection(bounds).is_some() && run.text.ends_with('…'))
        .unwrap();
    let mut engine = rust_desktop_ui_text::TextEngine::new();
    let shaped = engine.prepare(caption, 1.0).unwrap();
    assert!(shaped.width <= caption.clip.width + TEXT_GEOMETRY_EPSILON);
    assert!(shaped.height <= caption.clip.height + TEXT_GEOMETRY_EPSILON);
    for width in [180.0, 60.0, 900.0] {
        frame(&mut ui, &mut ribbon, width);
        let mut tabs: Vec<_> = ui
            .semantics()
            .into_iter()
            .filter(|node| node.role == Role::Tab)
            .collect();
        tabs.sort_by(|a, b| a.bounds.x.total_cmp(&b.bounds.x));
        assert_eq!(tabs.len(), 2);
        assert!(
            tabs[0].bounds.x + tabs[0].bounds.width <= tabs[1].bounds.x + TEXT_GEOMETRY_EPSILON
        );
        let ribbon_bounds = ui.runtime.bounds(ribbon.container().unwrap()).unwrap();
        assert!(
            tabs[1].bounds.x + tabs[1].bounds.width
                <= ribbon_bounds.x + ribbon_bounds.width - 6.0 + TEXT_GEOMETRY_EPSILON
        );
        assert!(
            tabs.iter()
                .all(|node| ui.button_chrome(node.id) == Some(ButtonChrome::Tab))
        );
    }
}

#[test]
fn split_generated_names_and_explicit_tooltips_are_preflighted_atomically() {
    let (mut ui, mut ribbon, _) = fixture();
    ui.commands_mut().register("primary", "x".repeat(5000));
    ribbon
        .set_command_appearance(
            "primary",
            RibbonCommandAppearance {
                tool_tip: Some("Short explanation".into()),
                ..Default::default()
            },
        )
        .unwrap();
    frame(&mut ui, &mut ribbon, 900.0);
    let primary = placements(&ui, &ribbon, "primary")[0];
    let container = ui.runtime.tree().parent(primary).unwrap();
    let dropdown = ui.runtime.tree().children(container).unwrap()[1];
    assert_eq!(ui.tooltip(primary), Some("Short explanation"));
    assert_eq!(ui.tooltip(dropdown), Some("Short explanation"));
    ui.activate(dropdown).unwrap();
    let before = ui.runtime.tree().preorder();
    let focused = ui.runtime.focused();
    ui.commands_mut()
        .register("primary", "x".repeat(1024 * 1024));
    ribbon.dirty = true;
    assert!(ribbon.preflight(&mut ui, 900.0).is_err());
    assert!(ribbon.update(&mut ui, 900.0).is_err());
    assert_eq!(ui.runtime.tree().preorder(), before);
    assert_eq!(ui.runtime.focused(), focused);
    assert!(ui.popup_open());
    assert!(ui.commands_mut().drain_invocations().is_empty());
}

#[test]
fn multiline_large_caption_has_bounded_height_and_does_not_enlarge_other_rows() {
    let (mut ui, mut ribbon, _) = fixture();
    ribbon.model.tabs[0].groups[1].items[0].kind = RibbonItemKind::Large;
    ribbon.model.tabs[0].groups[1].items[0].command = "multiline".into();
    ui.commands_mut().register("multiline", "Открыть документ");
    ribbon
        .set_command_appearance(
            "multiline",
            RibbonCommandAppearance {
                icon: Some(ButtonIcon::new(crate::IconSymbol::FolderOpen)),
                ..Default::default()
            },
        )
        .unwrap();
    frame(&mut ui, &mut ribbon, 900.0);
    let height = ui
        .runtime
        .bounds(ribbon.container().unwrap())
        .unwrap()
        .height;
    let small = ui
        .runtime
        .bounds(placements(&ui, &ribbon, "flag")[1])
        .unwrap()
        .height;
    ui.commands_mut().register(
        "multiline",
        "Открыть\n\nновый\nдокумент\nв\nрабочей\nобласти",
    );
    ribbon.dirty = true;
    frame(&mut ui, &mut ribbon, 900.0);
    assert_eq!(
        ui.runtime
            .bounds(ribbon.container().unwrap())
            .unwrap()
            .height,
        height
    );
    assert_eq!(
        ui.runtime
            .bounds(placements(&ui, &ribbon, "flag")[1])
            .unwrap()
            .height,
        small
    );
    let body = placements(&ui, &ribbon, "multiline")[0];
    let bounds = ui.runtime.bounds(body).unwrap();
    let caption = ui
        .scene()
        .texts
        .iter()
        .find(|run| run.bounds.intersection(bounds).is_some() && run.text.starts_with("Открыть"))
        .unwrap();
    assert!(!caption.text.contains('\n'));
    let mut engine = rust_desktop_ui_text::TextEngine::new();
    let shaped = engine.prepare(caption, 1.0).unwrap();
    assert!(shaped.height <= caption.clip.height + TEXT_GEOMETRY_EPSILON);
}

#[test]
fn compact_split_reserves_two_complete_caption_lines_in_its_independent_menu_area() {
    let (mut ui, mut ribbon, _) = fixture();
    let label = "Подтвердить значение";
    ui.commands_mut().register("primary", label);
    ribbon
        .set_command_appearance(
            "primary",
            RibbonCommandAppearance {
                icon: Some(ButtonIcon::new(crate::IconSymbol::Save)),
                ..Default::default()
            },
        )
        .unwrap();
    ui.set_theme(Theme::Compact).unwrap();
    frame(&mut ui, &mut ribbon, 900.0);
    let primary = placements(&ui, &ribbon, "primary")[0];
    let split = ui.runtime.tree().parent(primary).unwrap();
    let dropdown = ui.runtime.tree().children(split).unwrap()[1];
    let menu_bounds = ui.runtime.bounds(dropdown).unwrap();
    let padding = ui.button_style(dropdown).unwrap().padding.unwrap();
    let caption = ui
        .scene()
        .texts
        .iter()
        .find(|run| run.text == label && run.bounds.intersection(menu_bounds).is_some())
        .expect("the complete caption belongs to the separate menu hit area");
    let mut engine = rust_desktop_ui_text::TextEngine::new();
    let shaped = engine.prepare(caption, 1.0).unwrap();
    let mut rows: Vec<_> = shaped
        .selection_rects(0..label.len())
        .into_iter()
        .map(|rect| rect.y)
        .collect();
    rows.sort_by(f32::total_cmp);
    rows.dedup_by(|left, right| (*left - *right).abs() < 0.01);
    assert_eq!(rows.len(), 2, "exercise a complete two-line caption");
    assert!(
        shaped.height + padding.vertical() <= menu_bounds.height + TEXT_GEOMETRY_EPSILON,
        "Compact Split menu area {} must reserve shaped caption {} plus padding {}",
        menu_bounds.height,
        shaped.height,
        padding.vertical(),
    );
    assert!(shaped.height <= caption.clip.height + TEXT_GEOMETRY_EPSILON);
    let small = placements(&ui, &ribbon, "flag")[1];
    let row_height = ui.runtime.bounds(small).unwrap().height;
    assert_eq!(
        ui.runtime.bounds(split).unwrap().height,
        row_height * 3.0 + 4.0
    );
    assert!(ui.commands_mut().drain_invocations().is_empty());
}
