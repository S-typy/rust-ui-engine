use super::*;
use rust_desktop_ui_core::Modifiers;

fn model() -> WizardModel {
    WizardModel::new(
        vec![
            WizardStep {
                id: "start".into(),
                title: "Параметры".into(),
                next: vec!["remote".into(), "local".into()],
                is_final: false,
            },
            WizardStep {
                id: "remote".into(),
                title: "Сервер".into(),
                next: vec!["finish".into()],
                is_final: false,
            },
            WizardStep {
                id: "local".into(),
                title: "Локальные данные".into(),
                next: vec!["finish".into()],
                is_final: false,
            },
            WizardStep {
                id: "finish".into(),
                title: "Проверка".into(),
                next: vec![],
                is_final: true,
            },
        ],
        "start",
    )
    .unwrap()
}
fn frame(ui: &mut Controls) {
    ui.update(Size::new(900.0, 620.0)).unwrap();
}
fn fixture() -> (Controls, Wizard, WidgetId) {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let p = ui
        .wizard(
            ui.root(),
            "Настройка",
            model(),
            Default::default(),
            LayoutStyle {
                width: Length::Px(760.0),
                height: Length::Px(480.0),
                ..Default::default()
            },
        )
        .unwrap();
    let page = ui.wizard_page(p.container, "start").unwrap();
    let editor = ui
        .edit_box(
            page,
            "Имя",
            "accepted",
            LayoutStyle {
                height: Length::Px(36.0),
                ..Default::default()
            },
        )
        .unwrap();
    frame(&mut ui);
    ui.runtime.apply(Mutation::Focus(Some(editor))).unwrap();
    (ui, p, editor)
}
fn request(ui: &mut Controls, p: Wizard, intent: WizardIntent) -> WizardRequest {
    ui.request_wizard(p.container, intent).unwrap();
    ui.drain_wizard_requests().pop().unwrap()
}
fn advance(ui: &mut Controls, p: Wizard, target: &str) {
    let r = request(ui, p, WizardIntent::Next);
    assert_eq!(
        ui.complete_wizard(&r, WizardDecision::Step(target.into()))
            .unwrap(),
        WizardCompletion::Applied
    );
    frame(ui);
}

#[test]
fn wizard_graph_rejects_unknown_duplicate_cycles_unreachable_and_nonterminal_dead_ends() {
    let source = model();
    let mut steps = source.steps().to_vec();
    steps[1].next = vec!["missing".into()];
    assert!(WizardModel::new(steps, "start").is_err());
    let mut steps = source.steps().to_vec();
    steps[1].next = vec!["start".into()];
    assert!(WizardModel::new(steps, "start").is_err());
    let mut steps = source.steps().to_vec();
    steps[0].next = vec!["remote".into(), "remote".into()];
    assert!(WizardModel::new(steps, "start").is_err());
    let mut steps = source.steps().to_vec();
    steps[0].next = vec!["remote".into()];
    assert!(WizardModel::new(steps, "start").is_err());
    let mut steps = source.steps().to_vec();
    steps[3].is_final = false;
    assert!(WizardModel::new(steps, "start").is_err());
    assert!(WizardModel::new(vec![], "start").is_err());
}

#[test]
fn wizard_branching_back_preserves_draft_identity_selection_and_never_finishes() {
    let (mut ui, p, editor) = fixture();
    ui.set_text(editor, "unsaved draft").unwrap();
    let Kind::TextBox(document) = &mut ui.controls.get_mut(&editor).unwrap().kind else {
        panic!()
    };
    document.set_selection(1, 7).unwrap();
    let page = ui.wizard_page(p.container, "start").unwrap();
    let count = ui.runtime.tree().len();
    advance(&mut ui, p, "remote");
    assert_eq!(ui.accepted_value(editor), Some("accepted"));
    assert_eq!(ui.document(editor).unwrap().text(), "unsaved draft");
    let back = request(&mut ui, p, WizardIntent::Back);
    assert!(ui.complete_wizard(&back, WizardDecision::Finish).is_err());
    assert_eq!(ui.wizard_current_step(p.container), Some("remote"));
    assert_eq!(ui.wizard_result(p.container), None);
    ui.complete_wizard(&back, WizardDecision::Step("start".into()))
        .unwrap();
    frame(&mut ui);
    assert_eq!(ui.wizard_page(p.container, "start"), Some(page));
    assert_eq!(ui.runtime.tree().len(), count);
    assert_eq!(ui.document(editor).unwrap().selection().range(), 1..7);
    assert_eq!(ui.runtime.focused(), Some(editor));
    advance(&mut ui, p, "local");
    assert_eq!(ui.wizard_history(p.container).unwrap(), ["start", "local"]);
    advance(&mut ui, p, "finish");
    let finish = request(&mut ui, p, WizardIntent::Finish);
    assert_eq!(
        ui.complete_wizard(&finish, WizardDecision::Finish).unwrap(),
        WizardCompletion::Applied
    );
    assert_eq!(ui.wizard_result(p.container), Some(WizardResult::Finished));
    assert_eq!(
        ui.complete_wizard(&finish, WizardDecision::Finish).unwrap(),
        WizardCompletion::Stale
    );
    assert_eq!(
        ui.drain_wizard_events()
            .iter()
            .filter(|e| e.kind == WizardEventKind::Finished)
            .count(),
        1
    );
}

#[test]
fn wizard_rejection_and_malformed_completion_preserve_page_draft_and_request() {
    let (mut ui, p, editor) = fixture();
    ui.set_text(editor, "needs validation").unwrap();
    let count = ui.runtime.tree().len();
    let r = request(&mut ui, p, WizardIntent::Next);
    assert!(
        ui.complete_wizard(&r, WizardDecision::Step("finish".into()))
            .is_err()
    );
    assert!(!r.is_cancelled());
    assert_eq!(ui.wizard_current_step(p.container), Some("start"));
    assert_eq!(ui.runtime.tree().len(), count);
    let malformed = "\u{301}".repeat(32);
    assert!(
        ui.complete_wizard(&r, WizardDecision::Reject(malformed))
            .is_err()
    );
    assert!(!r.is_cancelled());
    assert_eq!(
        ui.complete_wizard(&r, WizardDecision::Reject("Имя не прошло проверку".into()))
            .unwrap(),
        WizardCompletion::Rejected
    );
    frame(&mut ui);
    assert_eq!(ui.document(editor).unwrap().text(), "needs validation");
    assert_eq!(ui.accepted_value(editor), Some("accepted"));
    assert_eq!(ui.runtime.tree().len(), count);
    assert!(matches!(
        ui.wizard_status(p.container),
        Some(WizardStatus::Error(_))
    ));
    assert_eq!(ui.wizard_result(p.container), None);
    let current = &ui.step_progress_value(p.progress.container).unwrap().steps[0];
    assert_eq!(current.state, StepState::Error);
}

#[test]
fn wizard_pending_cancel_replaces_request_and_terminal_result_is_exactly_once() {
    let (mut ui, p, editor) = fixture();
    ui.set_text(editor, "draft").unwrap();
    let next = request(&mut ui, p, WizardIntent::Next);
    let cancel = request(&mut ui, p, WizardIntent::Cancel);
    assert!(next.is_cancelled());
    assert_eq!(
        ui.complete_wizard(&next, WizardDecision::Step("local".into()))
            .unwrap(),
        WizardCompletion::Stale
    );
    assert!(ui.complete_wizard(&cancel, WizardDecision::Finish).is_err());
    assert!(!cancel.is_cancelled());
    ui.complete_wizard(&cancel, WizardDecision::Cancel).unwrap();
    assert_eq!(ui.wizard_result(p.container), Some(WizardResult::Cancelled));
    assert!(ui.request_wizard(p.container, WizardIntent::Next).is_err());
    assert_eq!(ui.document(editor).unwrap().text(), "draft");
    assert_eq!(
        ui.drain_wizard_events()
            .iter()
            .filter(|e| e.kind == WizardEventKind::Cancelled)
            .count(),
        1
    );
    ui.reset_wizard(p.container).unwrap();
    assert_eq!(ui.wizard_result(p.container), None);
    assert_eq!(ui.wizard_current_step(p.container), Some("start"));
    assert_eq!(ui.document(editor).unwrap().text(), "draft");
}

#[test]
fn wizard_edit_context_blur_disable_remove_cancel_old_tokens() {
    let (mut ui, p, editor) = fixture();
    let r = request(&mut ui, p, WizardIntent::Next);
    ui.set_text(editor, "changed after request").unwrap();
    assert!(r.is_cancelled());
    let r = request(&mut ui, p, WizardIntent::Next);
    ui.set_wizard_context_version(p.container, 8).unwrap();
    assert!(r.is_cancelled());
    let r = request(&mut ui, p, WizardIntent::Next);
    assert_eq!(r.context_version(), 8);
    ui.dispatch(InputEvent::WindowFocused(false)).unwrap();
    assert!(r.is_cancelled());
    ui.dispatch(InputEvent::WindowFocused(true)).unwrap();
    let r = request(&mut ui, p, WizardIntent::Next);
    ui.set_enabled(p.container, false).unwrap();
    frame(&mut ui);
    assert!(r.is_cancelled());
    ui.set_enabled(p.container, true).unwrap();
    let r = request(&mut ui, p, WizardIntent::Next);
    ui.set_wizard_read_only(p.container, true).unwrap();
    assert!(r.is_cancelled());
    ui.set_wizard_read_only(p.container, false).unwrap();
    let r = request(&mut ui, p, WizardIntent::Next);
    ui.remove(p.container).unwrap();
    frame(&mut ui);
    assert!(r.is_cancelled());
    assert_eq!(
        ui.complete_wizard(&r, WizardDecision::Step("local".into()))
            .unwrap(),
        WizardCompletion::Stale
    );
}

#[test]
fn wizard_owned_tab_and_progress_apis_cannot_bypass_guards() {
    let (mut ui, p, _) = fixture();
    let page = ui.wizard_page(p.container, "start").unwrap();
    let tabs = ui
        .tabs
        .iter()
        .find(|(_, t)| t.pages.contains(&page))
        .map(|(&id, _)| id)
        .unwrap();
    let mut order = ui.tab_order(tabs).unwrap().to_vec();
    order.reverse();
    assert!(ui.set_selected_tab(tabs, 1).is_err());
    assert!(ui.reorder_tabs(tabs, &order).is_err());
    assert!(ui.set_tab_header_content(order[0], None).is_err());
    let mut presentation = ui
        .step_progress_value(p.progress.container)
        .unwrap()
        .clone();
    presentation.current = "local".into();
    presentation.steps[0].state = StepState::Completed;
    presentation.steps[2].state = StepState::Current;
    assert!(
        ui.set_step_progress_value(p.progress.container, presentation)
            .is_err()
    );
    assert!(ui.remove(p.progress.container).is_err());
    assert!(ui.remove(p.next).is_err());
    assert_eq!(ui.wizard_current_step(p.container), Some("start"));
    assert!(ui.drain_wizard_requests().is_empty());
}

#[test]
fn wizard_keyboard_escape_composition_first_then_cancel_and_shortcuts_request_only() {
    let (mut ui, p, editor) = fixture();
    ui.runtime.apply(Mutation::Focus(Some(editor))).unwrap();
    ui.ime_preedit("仮", Some((0, 3))).unwrap();
    ui.dispatch(InputEvent::KeyDown {
        key: Key::Escape,
        modifiers: Modifiers::default(),
        repeat: false,
    })
    .unwrap();
    assert!(ui.document(editor).unwrap().composition().is_none());
    assert!(ui.drain_wizard_requests().is_empty());
    ui.dispatch(InputEvent::KeyDown {
        key: Key::ArrowRight,
        modifiers: Modifiers {
            alt: true,
            ..Default::default()
        },
        repeat: false,
    })
    .unwrap();
    let r = ui.drain_wizard_requests().pop().unwrap();
    assert_eq!(r.intent(), WizardIntent::Next);
    assert_eq!(ui.wizard_current_step(p.container), Some("start"));
    ui.dispatch(InputEvent::KeyDown {
        key: Key::Escape,
        modifiers: Modifiers::default(),
        repeat: false,
    })
    .unwrap();
    let cancel = ui.drain_wizard_requests().pop().unwrap();
    assert_eq!(cancel.intent(), WizardIntent::Cancel);
    assert!(r.is_cancelled());
}

#[test]
fn step_progress_retains_keys_reorders_without_actions_and_reveals_current() {
    let mut ui = Controls::new(Theme::FluentDark).unwrap();
    let value = StepProgressValue {
        current: "s19".into(),
        steps: (0..20)
            .map(|i| StepProgressItem {
                id: format!("s{i}"),
                title: format!("Шаг {i}"),
                state: if i == 19 {
                    StepState::Current
                } else {
                    StepState::Completed
                },
            })
            .collect(),
    };
    let p = ui
        .step_progress(
            ui.root(),
            "Этапы",
            value.clone(),
            LayoutStyle {
                width: Length::Px(420.0),
                height: Length::Px(76.0),
                ..Default::default()
            },
        )
        .unwrap();
    frame(&mut ui);
    let current = ui.step_progress_item(p.container, "s19").unwrap();
    let count = ui.runtime.tree().len();
    assert!(ui.scroll_offset(p.viewer.viewport).unwrap().x > 0.0);
    for width in [240.0, 600.0, 420.0] {
        let mut style = ui
            .runtime
            .tree()
            .node(p.container)
            .unwrap()
            .props
            .style
            .clone();
        style.width = Length::Px(width);
        ui.runtime
            .apply(Mutation::SetStyle {
                node: p.container,
                style,
            })
            .unwrap();
        frame(&mut ui);
        let viewport = ui.runtime.bounds(p.viewer.viewport).unwrap();
        let item = ui.runtime.bounds(current).unwrap();
        assert!(item.x >= viewport.x - 0.1, "{width}: {item:?} {viewport:?}");
        assert!(item.x + item.width <= viewport.x + viewport.width + 0.1);
        assert_eq!(ui.step_progress_item(p.container, "s19"), Some(current));
    }
    let mut reordered = value.clone();
    reordered.steps.reverse();
    ui.set_step_progress_value(p.container, reordered).unwrap();
    frame(&mut ui);
    assert_eq!(ui.step_progress_item(p.container, "s19"), Some(current));
    assert_eq!(ui.runtime.tree().len(), count);
    let semantics = ui.semantics();
    let node = semantics.iter().find(|n| n.id == current).unwrap();
    assert!(node.label.contains("Текущий шаг"));
    assert!(!node.actions.contains(&SemanticAction::Activate));
    assert!(ui.drain_wizard_requests().is_empty());
    let mut bad = value;
    bad.steps[0].id = "s19".into();
    let accepted = ui.step_progress_value(p.container).unwrap().clone();
    assert!(ui.set_step_progress_value(p.container, bad).is_err());
    assert_eq!(ui.step_progress_value(p.container), Some(&accepted));
}

#[test]
fn wizard_narrow_footer_wraps_without_clipping_buttons_or_changing_draft() {
    let (mut ui, parts, editor) = fixture();
    ui.set_text(editor, "draft survives resize").unwrap();
    let mut style = ui
        .runtime
        .tree()
        .node(parts.container)
        .unwrap()
        .props
        .style
        .clone();
    style.width = Length::Px(280.0);
    ui.runtime
        .apply(Mutation::SetStyle {
            node: parts.container,
            style,
        })
        .unwrap();
    frame(&mut ui);
    let owner = ui.runtime.bounds(parts.container).unwrap();
    let mut rows = Vec::new();
    for button in [parts.back, parts.next, parts.cancel] {
        let bounds = ui.runtime.bounds(button).unwrap();
        assert_eq!(bounds.height, 36.0);
        assert!(bounds.x >= owner.x && bounds.x + bounds.width <= owner.x + owner.width);
        assert!(bounds.y >= owner.y && bounds.y + bounds.height <= owner.y + owner.height);
        rows.push(bounds.y);
    }
    assert!(rows.iter().any(|&row| row != rows[0]));
    assert_eq!(ui.document(editor).unwrap().text(), "draft survives resize");
    assert_eq!(ui.accepted_value(editor), Some("accepted"));
    assert!(ui.drain_wizard_requests().is_empty());
}

#[test]
fn wizard_pointer_footer_keeps_ordinary_draft_unaccepted_before_guard_rejection() {
    let (mut ui, p, editor) = fixture();
    ui.set_text(editor, "unsaved").unwrap();
    frame(&mut ui);
    let rect = ui.runtime.bounds(p.next).unwrap();
    let position = Point::new(rect.x + rect.width / 2.0, rect.y + rect.height / 2.0);
    ui.dispatch(InputEvent::PointerDown {
        position,
        button: PointerButton::Primary,
        modifiers: Default::default(),
    })
    .unwrap();
    ui.dispatch(InputEvent::PointerUp {
        position,
        button: PointerButton::Primary,
        modifiers: Default::default(),
    })
    .unwrap();
    let request = ui.drain_wizard_requests().pop().unwrap();
    assert_eq!(request.intent(), WizardIntent::Next);
    assert_eq!(ui.accepted_value(editor), Some("accepted"));
    assert_eq!(ui.document(editor).unwrap().text(), "unsaved");
    ui.complete_wizard(&request, WizardDecision::Reject("Исправьте имя".into()))
        .unwrap();
    frame(&mut ui);
    assert_eq!(ui.accepted_value(editor), Some("accepted"));
    assert_eq!(ui.wizard_current_step(p.container), Some("start"));
}

#[test]
fn wizard_composing_owned_overlay_blocks_completion_and_preserves_pending_request() {
    let (mut ui, p, editor) = fixture();
    let request = request(&mut ui, p, WizardIntent::Next);
    let popup = ui.flyout(editor, Size::new(300.0, 150.0)).unwrap();
    let nested = ui
        .edit_box(popup.content, "Nested", "", LayoutStyle::default())
        .unwrap();
    ui.open_flyout(popup).unwrap();
    ui.runtime.apply(Mutation::Focus(Some(nested))).unwrap();
    frame(&mut ui);
    ui.ime_preedit("仮", Some((0, 3))).unwrap();
    assert!(
        ui.complete_wizard(&request, WizardDecision::Step("local".into()))
            .is_err()
    );
    assert!(!request.is_cancelled());
    assert!(ui.document(nested).unwrap().composition().is_some());
    assert_eq!(ui.wizard_current_step(p.container), Some("start"));
}

#[test]
fn wizard_reparented_required_page_rejects_before_transition_and_moved_editor_does_not_steal_focus()
{
    let (mut ui, p, editor) = fixture();
    advance(&mut ui, p, "remote");
    ui.runtime
        .apply(Mutation::Reparent {
            node: editor,
            parent: ui.root(),
        })
        .unwrap();
    let back = request(&mut ui, p, WizardIntent::Back);
    ui.complete_wizard(&back, WizardDecision::Step("start".into()))
        .unwrap();
    assert_ne!(ui.runtime.focused(), Some(editor));
    let page = ui.wizard_page(p.container, "local").unwrap();
    let parent = ui.runtime.tree().parent(page).unwrap();
    let request = request(&mut ui, p, WizardIntent::Next);
    ui.runtime
        .apply(Mutation::Reparent {
            node: page,
            parent: ui.root(),
        })
        .unwrap();
    assert!(
        ui.complete_wizard(&request, WizardDecision::Step("local".into()))
            .is_err()
    );
    assert!(!request.is_cancelled());
    assert_eq!(ui.wizard_current_step(p.container), Some("start"));
    ui.runtime
        .apply(Mutation::Reparent { node: page, parent })
        .unwrap();
    ui.complete_wizard(&request, WizardDecision::Step("local".into()))
        .unwrap();
}
