use super::*;
use rust_desktop_ui_core::Modifiers;
fn model() -> NavigationModel {
    NavigationModel::new(
        vec![
            NavigationSection::new("work", "Работа"),
            NavigationSection::new("options", "Параметры"),
        ],
        vec![
            NavigationPage::new("home", "work", "Главная"),
            NavigationPage::new("document", "work", "Документ"),
            NavigationPage::new("settings", "options", "Параметры приложения"),
        ],
        "home",
        4,
    )
    .unwrap()
}
fn fixture() -> (Controls, NavigationPane) {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let p = ui
        .navigation_pane(
            ui.root(),
            "Навигация",
            model(),
            NavigationSettings::default(),
            LayoutStyle {
                width: Length::Px(720.0),
                height: Length::Px(480.0),
                ..Default::default()
            },
        )
        .unwrap();
    frame(&mut ui);
    (ui, p)
}
fn frame(ui: &mut Controls) {
    ui.update(Size::new(900.0, 640.0)).unwrap();
}
fn editor(ui: &mut Controls, p: NavigationPane) -> WidgetId {
    let host = ui.navigation_page_host(p.container, "home").unwrap();
    let panel = ui
        .transparent_panel(
            host,
            LayoutStyle {
                kind: LayoutKind::Stack(Axis::Vertical),
                ..Default::default()
            },
        )
        .unwrap();
    ui.edit_box(
        panel,
        "Документ",
        "accepted",
        LayoutStyle {
            height: Length::Px(36.0),
            ..Default::default()
        },
    )
    .unwrap()
}
#[test]
fn navigation_history_push_replace_back_and_forward_branch_are_bounded() {
    let mut m = model();
    m.navigate("document", NavigationMode::Push).unwrap();
    m.navigate("settings", NavigationMode::Push).unwrap();
    assert!(m.back());
    assert_eq!(m.route(), "document");
    assert_eq!(m.history().len(), 3);
    m.navigate("home", NavigationMode::Push).unwrap();
    assert_eq!(m.history(), ["home", "document", "home"]);
    assert!(!m.forward());
    m.navigate("settings", NavigationMode::Replace).unwrap();
    assert_eq!(m.history(), ["home", "document", "settings"]);
    for _ in 0..8 {
        m.navigate("home", NavigationMode::Push).unwrap();
        m.navigate("settings", NavigationMode::Push).unwrap();
    }
    assert_eq!(m.history().len(), 4);
    assert_eq!(m.history_position(), 3);
    m.set_page_enabled("settings", false).unwrap();
    assert_eq!(m.route(), "settings");
    assert!(m.back());
    assert_eq!(m.route(), "home");
    let old = m.clone();
    assert!(m.navigate("unknown", NavigationMode::Push).is_err());
    assert_eq!(m, old);
}
#[test]
fn navigation_switches_all_views_without_recreating_page_or_edit_session() {
    let (mut ui, p) = fixture();
    let editor = editor(&mut ui, p);
    frame(&mut ui);
    ui.runtime.apply(Mutation::Focus(Some(editor))).unwrap();
    frame(&mut ui);
    ui.set_text(editor, "dirty draft 🦀").unwrap();
    let ids = ui.runtime.tree().preorder();
    for view in [
        NavigationView::Accordion,
        NavigationView::Hamburger,
        NavigationView::Office,
        NavigationView::Frame,
        NavigationView::Side,
    ] {
        ui.set_navigation_view(p.container, view).unwrap();
        frame(&mut ui);
        assert_eq!(ui.runtime.tree().preorder(), ids);
        assert_eq!(ui.runtime.focused(), Some(editor));
        assert_eq!(ui.document(editor).unwrap().text(), "dirty draft 🦀");
        assert_eq!(ui.accepted_value(editor), Some("accepted"));
    }
    assert!(ui.drain_navigation_events().is_empty());
}
#[test]
fn navigation_retains_deep_editor_draft_focus_and_cancels_only_preedit() {
    let (mut ui, p) = fixture();
    let editor = editor(&mut ui, p);
    frame(&mut ui);
    ui.runtime.apply(Mutation::Focus(Some(editor))).unwrap();
    frame(&mut ui);
    ui.set_text(editor, "unfinished").unwrap();
    ui.ime_preedit("あ", Some((3, 3))).unwrap();
    ui.navigate(p.container, "document", NavigationMode::Push)
        .unwrap();
    frame(&mut ui);
    assert_eq!(ui.document(editor).unwrap().text(), "unfinished");
    assert!(ui.document(editor).unwrap().composition().is_none());
    assert_eq!(ui.accepted_value(editor), Some("accepted"));
    assert_eq!(ui.runtime.focused(), Some(p.frame));
    ui.navigation_back(p.container).unwrap();
    frame(&mut ui);
    assert_eq!(ui.runtime.focused(), Some(editor));
    assert!(ui.drain_edit_events().is_empty());
    assert_eq!(ui.drain_navigation_events().len(), 2);
}
#[test]
fn navigation_pointer_selection_does_not_commit_on_lost_focus_and_emits_once() {
    let (mut ui, p) = fixture();
    let editor = editor(&mut ui, p);
    frame(&mut ui);
    ui.runtime.apply(Mutation::Focus(Some(editor))).unwrap();
    frame(&mut ui);
    ui.set_text(editor, "pointer draft").unwrap();
    let button = ui.navigation_selector(p.container, "document").unwrap();
    let r = ui.runtime.bounds(button).unwrap();
    let position = Point::new(r.x + 12.0, r.y + 12.0);
    ui.dispatch(InputEvent::PointerDown {
        position,
        button: PointerButton::Primary,
        modifiers: Modifiers::default(),
    })
    .unwrap();
    frame(&mut ui);
    ui.dispatch(InputEvent::PointerUp {
        position,
        button: PointerButton::Primary,
        modifiers: Modifiers::default(),
    })
    .unwrap();
    frame(&mut ui);
    assert_eq!(ui.navigation_route(p.container), Some("document"));
    assert_eq!(ui.accepted_value(editor), Some("accepted"));
    assert_eq!(ui.document(editor).unwrap().text(), "pointer draft");
    assert_eq!(ui.drain_navigation_events().len(), 1);
    ui.navigation_back(p.container).unwrap();
    frame(&mut ui);
    assert_eq!(ui.runtime.focused(), Some(editor));
}
#[test]
fn navigation_invalid_route_validator_and_settings_leave_history_focus_unchanged() {
    let (mut ui, p) = fixture();
    let editor = editor(&mut ui, p);
    frame(&mut ui);
    ui.runtime.apply(Mutation::Focus(Some(editor))).unwrap();
    frame(&mut ui);
    ui.set_navigation_validator(
        p.container,
        Some(NavigationValidator::new(|route| {
            if route == "settings" {
                Err("blocked".into())
            } else {
                Ok(())
            }
        })),
    )
    .unwrap();
    let old = ui.navigation_model(p.container).unwrap().clone();
    let ids = ui.runtime.tree().preorder();
    assert!(
        ui.navigate(p.container, "settings", NavigationMode::Push)
            .is_err()
    );
    assert!(
        ui.navigate(p.container, "missing", NavigationMode::Replace)
            .is_err()
    );
    assert!(
        ui.set_navigation_settings(
            p.container,
            NavigationSettings {
                pane_width: f32::NAN,
                ..Default::default()
            }
        )
        .is_err()
    );
    assert_eq!(ui.navigation_model(p.container), Some(&old));
    assert_eq!(ui.runtime.tree().preorder(), ids);
    assert_eq!(ui.runtime.focused(), Some(editor));
    assert!(ui.drain_navigation_events().is_empty());
}
#[test]
fn navigation_hides_owned_memo_popup_without_acceptance_and_removal_cleans_overlay() {
    let (mut ui, p) = fixture();
    let host = ui.navigation_page_host(p.container, "home").unwrap();
    let memo = ui
        .memo_edit(
            host,
            "Notes",
            "accepted",
            MemoEditSettings::default(),
            LayoutStyle {
                height: Length::Px(40.0),
                ..Default::default()
            },
        )
        .unwrap();
    frame(&mut ui);
    ui.open_memo(memo).unwrap();
    ui.set_text(memo.editor, "memo draft").unwrap();
    ui.navigate(p.container, "document", NavigationMode::Push)
        .unwrap();
    frame(&mut ui);
    assert!(!ui.memo_is_open(memo));
    assert_eq!(ui.accepted_value(memo.editor), Some("accepted"));
    assert_eq!(ui.document(memo.editor).unwrap().text(), "memo draft");
    assert_eq!(ui.runtime.focused(), Some(p.frame));
    ui.remove(p.container).unwrap();
    assert!(!ui.runtime.tree().contains(memo.popup.container));
    assert!(ui.navigation_model(p.container).is_none());
}
#[test]
fn navigation_required_parts_removal_and_reparent_are_rejected_before_transition() {
    let (mut ui, p) = fixture();
    let page = ui.navigation_page_host(p.container, "home").unwrap();
    assert!(ui.remove(page).is_err());
    assert!(ui.remove(p.back_button).is_err());
    ui.runtime
        .apply(Mutation::Reparent {
            node: page,
            parent: ui.root(),
        })
        .unwrap();
    let before = ui.navigation_model(p.container).unwrap().clone();
    assert!(
        ui.navigate(p.container, "document", NavigationMode::Push)
            .is_err()
    );
    assert_eq!(ui.navigation_model(p.container), Some(&before));
    ui.runtime
        .apply(Mutation::Reparent {
            node: page,
            parent: p.frame,
        })
        .unwrap();
    ui.remove(p.container).unwrap();
    assert!(ui.drain_navigation_events().is_empty());
}

#[test]
fn navigation_internal_reparent_is_rejected_before_page_history_or_settings_change() {
    let (mut ui, p) = fixture();
    let home = ui.navigation_page_host(p.container, "home").unwrap();
    let next = ui.navigation_page_host(p.container, "document").unwrap();
    let selector = ui.navigation_selector(p.container, "document").unwrap();
    for (id, parent) in [(next, home), (selector, home), (p.back_button, home)] {
        let old_parent = ui.runtime.tree().parent(id).unwrap();
        ui.runtime
            .apply(Mutation::Reparent { node: id, parent })
            .unwrap();
        let old = ui.navigation_model(p.container).unwrap().clone();
        let settings = ui.navigation_settings(p.container).unwrap();
        assert!(
            ui.navigate(p.container, "document", NavigationMode::Push)
                .is_err()
        );
        assert!(
            ui.set_navigation_view(p.container, NavigationView::Frame)
                .is_err()
        );
        assert_eq!(ui.navigation_model(p.container), Some(&old));
        assert_eq!(ui.navigation_settings(p.container), Some(settings));
        assert!(!ui.runtime.tree().node(home).unwrap().props.collapsed);
        assert!(ui.runtime.tree().node(next).unwrap().props.collapsed);
        assert!(ui.drain_navigation_events().is_empty());
        ui.runtime
            .apply(Mutation::Reparent {
                node: id,
                parent: old_parent,
            })
            .unwrap();
    }
    ui.navigate(p.container, "document", NavigationMode::Push)
        .unwrap();
}

#[test]
fn navigation_event_backpressure_rejects_without_changing_page_or_history() {
    let (mut ui, p) = fixture();
    for i in 0..256 {
        ui.navigate(
            p.container,
            if i % 2 == 0 { "document" } else { "home" },
            NavigationMode::Push,
        )
        .unwrap();
    }
    let old = ui.navigation_model(p.container).unwrap().clone();
    assert!(
        ui.navigate(p.container, "settings", NavigationMode::Push)
            .is_err()
    );
    assert_eq!(ui.navigation_model(p.container), Some(&old));
    assert_eq!(ui.drain_navigation_events().len(), 256);
    assert!(
        ui.navigate(p.container, "settings", NavigationMode::Push)
            .unwrap()
    );
    assert_eq!(ui.drain_navigation_events().len(), 1);
}

#[test]
fn navigation_metadata_budget_includes_sections_and_invalid_shape_leaves_no_nodes() {
    let sections: Vec<_> = (0..16)
        .map(|i| NavigationSection::new(format!("s{i}"), "𐐀".repeat(128)))
        .collect();
    let pages: Vec<_> = (0..64)
        .map(|i| NavigationPage::new(format!("p{i}"), format!("s{}", i / 4), "𐐀".repeat(96)))
        .collect();
    assert!(NavigationModel::new(sections, pages, "p0", 64).is_err());
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let count = ui.runtime.tree().len();
    let m = NavigationModel::new(
        vec![NavigationSection::new("s", "Section")],
        vec![NavigationPage::new(
            "p",
            "s",
            format!("a{}", "\u{301}".repeat(33)),
        )],
        "p",
        64,
    )
    .unwrap();
    assert!(
        ui.navigation_pane(
            ui.root(),
            "Navigation",
            m,
            NavigationSettings::default(),
            LayoutStyle::default()
        )
        .is_err()
    );
    assert_eq!(ui.runtime.tree().len(), count);
}
#[test]
fn navigation_disabled_current_remains_visible_and_semantics_select_only_current() {
    let (mut ui, p) = fixture();
    ui.set_navigation_page_enabled(p.container, "home", false)
        .unwrap();
    frame(&mut ui);
    assert_eq!(ui.navigation_route(p.container), Some("home"));
    let selected: Vec<_> = ui
        .semantics()
        .into_iter()
        .filter(|n| n.role == Role::Tab && n.selected)
        .collect();
    assert_eq!(selected.len(), 1);
    assert!(selected[0].disabled);
    assert_eq!(selected[0].label, "Главная");
    ui.navigate(p.container, "document", NavigationMode::Push)
        .unwrap();
    assert!(!ui.navigation_back(p.container).unwrap());
    ui.set_navigation_page_enabled(p.container, "home", true)
        .unwrap();
    assert!(ui.navigation_back(p.container).unwrap());
}
#[test]
fn navigation_external_binding_replaces_history_and_invalid_source_preserves_page() {
    use crate::{
        BindingContext, BindingProperty, ControlBinding, ControlBindingMode, ControlBindings,
        DataValue,
    };
    use std::{cell::RefCell, rc::Rc};
    let (mut ui, p) = fixture();
    let context = Rc::new(RefCell::new(
        BindingContext::new(DataValue::object([("Route", DataValue::from("home"))])).unwrap(),
    ));
    let mut bindings = ControlBindings::new(Rc::clone(&context));
    let mut binding = ControlBinding::new(p.container, BindingProperty::Route, "Route").unwrap();
    binding.mode = ControlBindingMode::TwoWay;
    bindings.add(binding).unwrap();
    bindings.initialize(&mut ui).unwrap();
    context
        .borrow_mut()
        .set("Route", "settings".into())
        .unwrap();
    bindings.sync(&mut ui);
    assert_eq!(ui.navigation_route(p.container), Some("settings"));
    assert_eq!(ui.navigation_model(p.container).unwrap().history().len(), 1);
    ui.navigate(p.container, "document", NavigationMode::Push)
        .unwrap();
    bindings.sync(&mut ui);
    assert_eq!(
        context.borrow().get("Route").unwrap(),
        &DataValue::from("document")
    );
    context.borrow_mut().set("Route", "unknown".into()).unwrap();
    bindings.sync(&mut ui);
    assert_eq!(ui.navigation_route(p.container), Some("document"));
    assert!(!bindings.diagnostics().is_empty());
}

#[test]
fn navigation_replacing_application_validator_preserves_source_validation_guard() {
    let (mut ui, p) = fixture();
    ui.chain_navigation_validator(
        p.container,
        NavigationValidator::new(|route| {
            if route == "settings" {
                Err("source rejected".into())
            } else {
                Ok(())
            }
        }),
    )
    .unwrap();
    ui.set_navigation_validator(p.container, Some(NavigationValidator::new(|_| Ok(()))))
        .unwrap();
    ui.set_navigation_validator(p.container, None).unwrap();
    assert!(
        ui.navigate(p.container, "settings", NavigationMode::Push)
            .is_err()
    );
    assert_eq!(ui.navigation_route(p.container), Some("home"));
    assert!(ui.drain_navigation_events().is_empty());
}

#[test]
fn navigation_validator_rejection_is_nonfatal_for_pointer_and_keyboard_input() {
    let (mut ui, p) = fixture();
    let editor = editor(&mut ui, p);
    frame(&mut ui);
    ui.runtime.apply(Mutation::Focus(Some(editor))).unwrap();
    frame(&mut ui);
    ui.set_text(editor, "draft").unwrap();
    ui.set_navigation_validator(
        p.container,
        Some(NavigationValidator::new(|route| {
            if route == "settings" {
                Err("Finish document first".into())
            } else {
                Ok(())
            }
        })),
    )
    .unwrap();
    let target = ui.navigation_selector(p.container, "settings").unwrap();
    let r = ui.runtime.bounds(target).unwrap();
    let position = Point::new(r.x + 10.0, r.y + 10.0);
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
    frame(&mut ui);
    assert_eq!(ui.navigation_route(p.container), Some("home"));
    assert_eq!(
        ui.navigation_error(p.container),
        Some("Finish document first")
    );
    assert!(
        ui.semantics()
            .iter()
            .any(|n| n.label.contains("Finish document first"))
    );
    assert_eq!(ui.accepted_value(editor), Some("accepted"));
    assert_eq!(ui.document(editor).unwrap().text(), "draft");
    assert!(ui.drain_navigation_events().is_empty());
    ui.navigate(p.container, "document", NavigationMode::Push)
        .unwrap();
    assert!(ui.navigation_error(p.container).is_none());
    ui.drain_navigation_events();
    ui.set_navigation_validator(
        p.container,
        Some(NavigationValidator::new(|route| {
            if route == "home" {
                Err("Cannot go back".into())
            } else {
                Ok(())
            }
        })),
    )
    .unwrap();
    ui.runtime.apply(Mutation::Focus(Some(p.frame))).unwrap();
    ui.dispatch(InputEvent::KeyDown {
        key: Key::ArrowLeft,
        modifiers: Modifiers {
            alt: true,
            ..Default::default()
        },
        repeat: false,
    })
    .unwrap();
    frame(&mut ui);
    assert_eq!(ui.navigation_route(p.container), Some("document"));
    assert_eq!(ui.navigation_error(p.container), Some("Cannot go back"));
    assert!(ui.drain_navigation_events().is_empty());
    assert!(ui.navigation_back(p.container).is_err());
}

#[test]
fn navigation_keyboard_end_reveals_offscreen_selector_without_changing_route() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let model = NavigationModel::new(
        vec![NavigationSection::new("s", "Section")],
        (0..32)
            .map(|i| NavigationPage::new(format!("p{i}"), "s", format!("Page {i}")))
            .collect(),
        "p0",
        64,
    )
    .unwrap();
    let p = ui
        .navigation_pane(
            ui.root(),
            "Navigation",
            model,
            NavigationSettings::default(),
            LayoutStyle {
                width: Length::Px(500.0),
                height: Length::Px(240.0),
                ..Default::default()
            },
        )
        .unwrap();
    frame(&mut ui);
    let first = ui.navigation_selector(p.container, "p0").unwrap();
    let last = ui.navigation_selector(p.container, "p31").unwrap();
    ui.runtime.apply(Mutation::Focus(Some(first))).unwrap();
    ui.dispatch(InputEvent::KeyDown {
        key: Key::End,
        modifiers: Modifiers::default(),
        repeat: false,
    })
    .unwrap();
    frame(&mut ui);
    assert_eq!(ui.runtime.focused(), Some(last));
    let bounds = ui.runtime.bounds(last).unwrap();
    let clip = ui.runtime.clip(last).unwrap();
    let visible = bounds.intersection(clip).unwrap();
    assert!((visible.height - bounds.height).abs() < 0.1);
    assert_eq!(ui.navigation_route(p.container), Some("p0"));
    assert!(ui.drain_navigation_events().is_empty());
}

#[test]
fn navigation_selected_route_uses_themed_scene_fill_without_toggle_semantics() {
    let (mut ui, pane) = fixture();
    let home = ui.navigation_selector(pane.container, "home").unwrap();
    let document = ui.navigation_selector(pane.container, "document").unwrap();
    let ordinary = ui
        .button(ui.root(), "Ordinary", None, LayoutStyle::default())
        .unwrap();
    // Internal selection storage alone must not turn an ordinary action into a toggle.
    ui.controls.get_mut(&ordinary).unwrap().checked = true;
    for theme in [Theme::FluentLight, Theme::FluentDark] {
        ui.set_theme(theme).unwrap();
        ui.navigate(pane.container, "home", NavigationMode::Replace)
            .unwrap();
        frame(&mut ui);
        let fill = |ui: &Controls, widget| {
            let bounds = ui.runtime.bounds(widget).unwrap();
            ui.scene()
                .rounded_rectangles
                .iter()
                .find(|shape| shape.bounds == bounds && shape.stroke_width.is_none())
                .map(|shape| shape.color)
        };
        assert_eq!(fill(&ui, home), Some(theme.tokens().selected));
        assert_ne!(fill(&ui, home), fill(&ui, document));
        assert!(!ui.button_visual_state(ordinary).unwrap().checked);
        let semantics = ui.semantics();
        let node = semantics.iter().find(|n| n.id == home).unwrap();
        assert!(node.selected);
        assert_eq!(node.checked, None);
        assert!(!node.actions.contains(&SemanticAction::Toggle));
        ui.navigate(pane.container, "document", NavigationMode::Replace)
            .unwrap();
        frame(&mut ui);
        assert_eq!(fill(&ui, document), Some(theme.tokens().selected));
        assert_ne!(fill(&ui, home), fill(&ui, document));
    }
}
