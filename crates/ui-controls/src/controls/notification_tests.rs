use super::*;
use rust_desktop_ui_core::Modifiers;
use std::{cell::Cell, rc::Rc, time::Duration};

fn layout(w: f32, h: f32) -> LayoutStyle {
    LayoutStyle {
        width: Length::Px(w),
        height: Length::Px(h),
        flex_shrink: 0.0,
        ..Default::default()
    }
}
fn frame(ui: &mut Controls) {
    ui.update(Size::new(700.0, 500.0)).unwrap();
}

#[test]
fn toast_action_label_preflight_preserves_queue_and_fits_after_resize() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let center = ui
        .notification_center(
            ui.root(),
            NotificationCenterSettings {
                width: 640.0,
                ..Default::default()
            },
            layout(0.0, 0.0),
        )
        .unwrap();
    let command = CommandId::from("action-bounds");
    ui.commands.register(command.clone(), "Action");
    let make = |label: &str| {
        let mut spec = NotificationSpec::new("Saved", "Ready");
        spec.timeout = None;
        spec.actions.push(NotificationAction {
            label: label.into(),
            command: command.clone(),
        });
        spec
    };
    let first = ui
        .post_notification(center.container, make("Open details"))
        .unwrap();
    frame(&mut ui);
    let tree = ui.runtime.tree().preorder();
    let parts = ui.notification_parts(first).unwrap().clone();
    let history = ui.notification_history(center.container).unwrap().clone();
    assert!(
        ui.post_notification(center.container, make(&"W".repeat(64)))
            .is_err()
    );
    assert!(
        ui.post_notification(center.container, make("Line one\nLine two"))
            .is_err()
    );
    assert_eq!(ui.runtime.tree().preorder(), tree);
    assert_eq!(ui.notification_history(center.container), Some(&history));
    assert_eq!(ui.notification_parts(first).unwrap().card, parts.card);
    let second = ui
        .post_notification(center.container, make("Apply"))
        .unwrap();
    assert_eq!(second.serial, first.serial + 1);
    for theme in [Theme::FluentLight, Theme::FluentDark] {
        ui.set_theme(theme).unwrap();
        for width in [700.0, 264.0, 420.0] {
            ui.update(Size::new(width, 500.0)).unwrap();
            let button = parts.actions[0];
            let bounds = ui.runtime.bounds(button).unwrap();
            let style = *ui.button_style(button).unwrap();
            let required = ui
                .measure_button_content(
                    "Open details",
                    &style,
                    None,
                    ButtonContentLayout::Leading,
                    16_384.0,
                )
                .unwrap();
            assert!(bounds.width + 0.01 >= required.width);
            assert!(bounds.height + 0.01 >= required.height);
            let run = ui
                .scene()
                .texts
                .iter()
                .find(|run| run.text == "Open details")
                .unwrap();
            assert!(run.clip.width + 0.01 >= run.bounds.width);
            assert!(run.clip.height + 0.01 >= run.bounds.height);
        }
    }
}

#[test]
fn toast_wrap_height_matches_actual_label_content_insets() {
    for theme in [Theme::FluentLight, Theme::FluentDark] {
        for width in [240.0, 360.0, 640.0] {
            let mut ui = Controls::new(theme).unwrap();
            let center = ui
                .notification_center(
                    ui.root(),
                    NotificationCenterSettings {
                        width,
                        ..Default::default()
                    },
                    layout(0.0, 0.0),
                )
                .unwrap();
            let spec = NotificationSpec::new(
                "Документ готов к сохранению",
                "Работа завершена. Черновик и фокус сохранены.",
            );
            let id = ui.post_notification(center.container, spec).unwrap();
            frame(&mut ui);
            let parts = ui.notification_parts(id).unwrap().clone();
            for label in [parts.title, parts.message] {
                let bounds = ui.runtime.bounds(label).unwrap();
                let text = ui.label_text(label).unwrap().to_owned();
                let style = ui.text_style(label).unwrap();
                let measured = ui
                    .measure_styled_text(&text, bounds.width - 16.0, style)
                    .unwrap();
                assert!(
                    bounds.height + 0.01 >= measured.height,
                    "{theme:?} width={width}: label {text:?} has {bounds:?}, needs {measured:?}"
                );
                assert!(
                    ui.scene()
                        .texts
                        .iter()
                        .any(|run| run.text == text && run.wrap)
                );
            }
        }
    }
}
fn fixture() -> (Controls, NotificationCenter, WidgetId) {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let editor = ui
        .edit_box(
            ui.root(),
            "Retained editor",
            "accepted",
            layout(300.0, 36.0),
        )
        .unwrap();
    let center = ui
        .notification_center(ui.root(), Default::default(), layout(0.0, 0.0))
        .unwrap();
    ui.runtime.apply(Mutation::Focus(Some(editor))).unwrap();
    frame(&mut ui);
    ui.set_text(editor, "pending draft").unwrap();
    (ui, center, editor)
}
fn click(ui: &mut Controls, id: WidgetId) {
    let bounds = ui.runtime.bounds(id).unwrap();
    let position = Point::new(
        bounds.x + bounds.width / 2.0,
        bounds.y + bounds.height / 2.0,
    );
    assert_eq!(ui.runtime.hit_test(position), Some(id));
    for event in [
        InputEvent::PointerDown {
            position,
            button: PointerButton::Primary,
            modifiers: Modifiers::default(),
        },
        InputEvent::PointerUp {
            position,
            button: PointerButton::Primary,
            modifiers: Modifiers::default(),
        },
    ] {
        ui.dispatch(event).unwrap();
    }
    frame(ui);
}
#[test]
fn toast_show_pointer_action_and_dismiss_preserve_editor_focus_draft_and_ime() {
    let (mut ui, center, editor) = fixture();
    let calls = Rc::new(Cell::new(0));
    let seen = calls.clone();
    let command = CommandId::from("notify-action");
    ui.commands.register(command.clone(), "Action");
    ui.commands
        .on_execute(&command, move || seen.set(seen.get() + 1));
    ui.ime_preedit("draft composition", None).unwrap();
    let composition = ui.document(editor).unwrap().composition().cloned();
    let mut spec = NotificationSpec::new("Saved", "A short message");
    spec.timeout = None;
    spec.actions.push(NotificationAction {
        label: "Open details".into(),
        command: command.clone(),
    });
    let id = ui.post_notification(center.container, spec).unwrap();
    frame(&mut ui);
    assert_eq!(ui.runtime.focused(), Some(editor));
    let button = ui.notification_parts(id).unwrap().actions[0];
    click(&mut ui, button);
    assert_eq!(ui.runtime.focused(), Some(editor));
    assert_eq!(calls.get(), 1);
    ui.perform_action(button, SemanticAction::Activate, None)
        .unwrap();
    assert_eq!(calls.get(), 1);
    assert_eq!(ui.runtime.focused(), Some(editor));
    assert_eq!(ui.document(editor).unwrap().text(), "pending draft");
    assert_eq!(ui.accepted_value(editor), Some("accepted"));
    assert_eq!(
        ui.document(editor).unwrap().composition(),
        composition.as_ref()
    );
    assert!(!ui.notification_is_pending(id));
    assert!(matches!(
        ui.notification_history(center.container)
            .unwrap()
            .back()
            .unwrap()
            .reason,
        NotificationCloseReason::Action(_)
    ));
    let next = ui
        .post_notification(
            center.container,
            NotificationSpec::new("Dismiss", "Dismiss preserves focus"),
        )
        .unwrap();
    frame(&mut ui);
    let parts = ui.notification_parts(next).unwrap().clone();
    let bounds = ui.runtime.bounds(parts.message).unwrap();
    ui.dispatch(InputEvent::PointerDown {
        position: Point::new(bounds.x + 3.0, bounds.y + 3.0),
        button: PointerButton::Primary,
        modifiers: Modifiers::default(),
    })
    .unwrap();
    assert_eq!(ui.runtime.focused(), Some(editor));
    assert_eq!(
        ui.document(editor).unwrap().composition(),
        composition.as_ref()
    );
    let dismiss = parts.dismiss;
    click(&mut ui, dismiss);
    assert_eq!(ui.runtime.focused(), Some(editor));
}
#[test]
fn toast_disabled_actions_release_outside_and_keyboard_repeat_never_double_execute() {
    let (mut ui, c, _) = fixture();
    let command = CommandId::from("action");
    let calls = Rc::new(Cell::new(0));
    let seen = calls.clone();
    ui.commands.register(command.clone(), "Action");
    ui.commands
        .on_execute(&command, move || seen.set(seen.get() + 1));
    let mut spec = NotificationSpec::new("Title", "Message");
    spec.actions.push(NotificationAction {
        label: "Action".into(),
        command: command.clone(),
    });
    let id = ui.post_notification(c.container, spec).unwrap();
    frame(&mut ui);
    let button = ui.notification_parts(id).unwrap().actions[0];
    ui.commands.set_enabled(&command, false);
    frame(&mut ui);
    ui.perform_action(button, SemanticAction::Activate, None)
        .unwrap();
    assert_eq!(calls.get(), 0);
    assert!(ui.notification_is_pending(id));
    ui.commands.set_enabled(&command, true);
    frame(&mut ui);
    let b = ui.runtime.bounds(button).unwrap();
    let position = Point::new(b.x + 3.0, b.y + 3.0);
    ui.dispatch(InputEvent::PointerDown {
        position,
        button: PointerButton::Primary,
        modifiers: Modifiers::default(),
    })
    .unwrap();
    ui.dispatch(InputEvent::PointerUp {
        position: Point::new(10.0, 10.0),
        button: PointerButton::Primary,
        modifiers: Modifiers::default(),
    })
    .unwrap();
    assert_eq!(calls.get(), 0);
    ui.runtime.apply(Mutation::Focus(Some(button))).unwrap();
    for repeat in [false, true, true] {
        ui.dispatch(InputEvent::KeyDown {
            key: Key::Enter,
            modifiers: Modifiers::default(),
            repeat,
        })
        .unwrap();
    }
    ui.dispatch(InputEvent::KeyUp {
        key: Key::Enter,
        modifiers: Modifiers::default(),
    })
    .unwrap();
    assert_eq!(calls.get(), 1);
}
#[test]
fn toast_timers_start_on_visibility_cancel_with_owner_and_queue_is_bounded() {
    let (mut ui, c, _) = fixture();
    let mut ids = Vec::new();
    for i in 0..MAX_PENDING_NOTIFICATIONS {
        let mut s = NotificationSpec::new(format!("Toast {i}"), "Message");
        s.timeout = Some(Duration::from_secs(1));
        ids.push(ui.post_notification(c.container, s).unwrap());
    }
    let tree = ui.runtime.tree().preorder();
    assert!(
        ui.post_notification(c.container, NotificationSpec::new("Overflow", "No room"))
            .is_err()
    );
    assert_eq!(ui.runtime.tree().preorder(), tree);
    frame(&mut ui);
    let deadline = ui.notification_deadline().unwrap();
    let visible = ids
        .iter()
        .filter(|id| {
            ui.runtime
                .bounds(ui.notification_parts(**id).unwrap().card)
                .is_some()
        })
        .count();
    assert_eq!(visible, 3);
    assert!(ui.tick(deadline).unwrap());
    frame(&mut ui);
    assert!(!ui.notification_is_pending(ids[0]));
    assert!(ui.notification_is_pending(ids[3]));
    assert!(ui.notification_deadline().unwrap() > deadline);
    ui.set_enabled(c.container, false).unwrap();
    frame(&mut ui);
    assert!(ui.notification_deadline().is_none());
    assert!(ids.iter().all(|id| !ui.notification_is_pending(*id)));
    assert_eq!(
        ui.notification_history(c.container).unwrap().len(),
        MAX_PENDING_NOTIFICATIONS
    );
    ui.set_enabled(c.container, true).unwrap();
    let fresh = ui
        .post_notification(c.container, NotificationSpec::new("New", "After enable"))
        .unwrap();
    frame(&mut ui);
    let card = ui.notification_parts(fresh).unwrap().card;
    ui.remove(c.container).unwrap();
    assert!(!ui.runtime.tree().contains(card));
    assert!(ui.notification_deadline().is_none());
}
#[test]
fn toast_preflight_and_owned_parts_fail_atomically_and_buttons_fit_small_viewport() {
    let (mut ui, c, _) = fixture();
    let before = ui.runtime.tree().preorder();
    let mut bad = NotificationSpec::new("bad", "x".repeat(1025));
    assert!(ui.post_notification(c.container, bad.clone()).is_err());
    bad.message = "ok".into();
    bad.actions.push(NotificationAction {
        label: "Unknown".into(),
        command: CommandId::from("unknown"),
    });
    assert!(ui.post_notification(c.container, bad).is_err());
    assert_eq!(ui.runtime.tree().preorder(), before);
    let id = ui
        .post_notification(c.container, NotificationSpec::new("Stable", "Original"))
        .unwrap();
    frame(&mut ui);
    let parts = ui.notification_parts(id).unwrap().clone();
    assert!(ui.remove(parts.message).is_err());
    let parent = ui.runtime.tree().parent(parts.message).unwrap();
    ui.runtime
        .apply(Mutation::Reparent {
            node: parts.message,
            parent: ui.root(),
        })
        .unwrap();
    let tree = ui.runtime.tree().preorder();
    assert!(
        ui.post_notification(c.container, NotificationSpec::new("No", "Mutation"))
            .is_err()
    );
    assert_eq!(ui.runtime.tree().preorder(), tree);
    assert!(ui.notification_is_pending(id));
    ui.runtime
        .apply(Mutation::Reparent {
            node: parts.message,
            parent,
        })
        .unwrap();
    ui.update(Size::new(300.0, 220.0)).unwrap();
    let b = ui.runtime.bounds(parts.dismiss).unwrap();
    assert_eq!(
        b.intersection(ui.runtime.clip(parts.dismiss).unwrap()),
        Some(b)
    );
    ui.update(Size::new(180.0, 100.0)).unwrap();
    assert!(ui.runtime.bounds(parts.card).is_none());
    assert!(ui.notification_is_pending(id));
}
fn loading_fixture() -> (Controls, LoadingOverlay, WidgetId) {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let parts = ui
        .loading_overlay(ui.root(), "Working", layout(500.0, 280.0))
        .unwrap();
    let editor = ui
        .edit_box(parts.content, "Document", "accepted", layout(300.0, 36.0))
        .unwrap();
    ui.runtime.apply(Mutation::Focus(Some(editor))).unwrap();
    frame(&mut ui);
    ui.set_text(editor, "draft").unwrap();
    (ui, parts, editor)
}
#[test]
fn loading_cancel_error_success_preserve_document_and_reject_stale_worker_generations() {
    let (mut ui, p, editor) = loading_fixture();
    let token = ui.begin_loading(p.container, "Loading", true).unwrap();
    frame(&mut ui);
    assert_eq!(ui.accepted_value(editor), Some("accepted"));
    assert_eq!(ui.document(editor).unwrap().text(), "draft");
    assert_eq!(ui.runtime.focused(), Some(p.cancel));
    assert!(ui.update_loading(token, "Half", Some(0.5)).unwrap());
    assert_eq!(ui.progress_value(p.progress), Some(0.5));
    click(&mut ui, p.cancel);
    assert_eq!(
        ui.take_loading_result(p.container),
        Some(LoadingResult::Cancelled)
    );
    assert_eq!(ui.runtime.focused(), Some(editor));
    assert!(!ui.update_loading(token, "stale", Some(1.0)).unwrap());
    assert!(
        !ui.finish_loading(token, LoadingResult::Failed("x".repeat(5000)))
            .unwrap()
    );
    let next = ui.begin_loading(p.container, "Again", true).unwrap();
    assert!(!ui.finish_loading(token, LoadingResult::Succeeded).unwrap());
    assert!(
        ui.finish_loading(next, LoadingResult::Failed("Failure".into()))
            .unwrap()
    );
    frame(&mut ui);
    assert!(!ui.is_enabled(p.overlay));
    assert_eq!(
        ui.take_loading_result(p.container),
        Some(LoadingResult::Failed("Failure".into()))
    );
    let final_token = ui.begin_loading(p.container, "Final", false).unwrap();
    assert!(
        ui.finish_loading(final_token, LoadingResult::Succeeded)
            .unwrap()
    );
    assert_eq!(
        ui.take_loading_result(p.container),
        Some(LoadingResult::Succeeded)
    );
    assert_eq!(ui.document(editor).unwrap().text(), "draft");
    assert_eq!(ui.accepted_value(editor), Some("accepted"));
}
#[test]
fn loading_completion_does_not_restore_focus_to_reparented_content() {
    let (mut ui, p, editor) = loading_fixture();
    let token = ui.begin_loading(p.container, "Work", true).unwrap();
    ui.runtime
        .apply(Mutation::Reparent {
            node: editor,
            parent: ui.root(),
        })
        .unwrap();
    assert!(ui.finish_loading(token, LoadingResult::Succeeded).unwrap());
    assert_ne!(ui.runtime.focused(), Some(editor));
    assert_eq!(ui.document(editor).unwrap().text(), "draft");
    assert_eq!(ui.accepted_value(editor), Some("accepted"));
}
#[test]
fn loading_never_steals_other_panel_focus_and_finishes_in_background() {
    let (mut ui, p, editor) = loading_fixture();
    let other = ui
        .edit_box(ui.root(), "Other", "external", layout(300.0, 36.0))
        .unwrap();
    ui.runtime.apply(Mutation::Focus(Some(other))).unwrap();
    let token = ui.begin_loading(p.container, "Work", true).unwrap();
    assert_eq!(ui.runtime.focused(), Some(other));
    ui.finish_loading(token, LoadingResult::Succeeded).unwrap();
    assert_eq!(ui.runtime.focused(), Some(other));
    ui.take_loading_result(p.container);
    ui.runtime.apply(Mutation::Focus(Some(editor))).unwrap();
    let token = ui.begin_loading(p.container, "Work", true).unwrap();
    ui.dispatch(InputEvent::WindowFocused(false)).unwrap();
    assert!(ui.finish_loading(token, LoadingResult::Succeeded).unwrap());
    assert_eq!(ui.runtime.focused(), None);
    ui.take_loading_result(p.container);
    let token = ui.begin_loading(p.container, "Background", true).unwrap();
    assert_eq!(ui.runtime.focused(), None);
    assert!(ui.finish_loading(token, LoadingResult::Cancelled).unwrap());
    assert_eq!(ui.document(editor).unwrap().text(), "draft");
}
#[test]
fn loading_invalid_progress_owner_hide_resize_and_part_tampering_are_safe() {
    let (mut ui, p, _) = loading_fixture();
    let token = ui.begin_loading(p.container, "Work", true).unwrap();
    frame(&mut ui);
    let state = ui.loading_status(p.container).cloned();
    let tree = ui.runtime.tree().preorder();
    assert!(ui.update_loading(token, "Invalid", Some(f64::NAN)).is_err());
    assert_eq!(ui.loading_status(p.container), state.as_ref());
    assert_eq!(ui.runtime.tree().preorder(), tree);
    ui.edit_props(p.container, |n| n.visible = false).unwrap();
    assert!(ui.finish_loading(token, LoadingResult::Succeeded).unwrap());
    frame(&mut ui);
    assert_eq!(
        ui.take_loading_result(p.container),
        Some(LoadingResult::Cancelled)
    );
    assert_eq!(ui.progress_indeterminate(p.progress), Some(false));
    ui.edit_props(p.container, |n| n.visible = true).unwrap();
    frame(&mut ui);
    let token = ui.begin_loading(p.container, "Work", true).unwrap();
    ui.edit_props(p.container, |n| n.style.height = Length::Px(100.0))
        .unwrap();
    frame(&mut ui);
    assert_eq!(
        ui.take_loading_result(p.container),
        Some(LoadingResult::Cancelled)
    );
    assert!(!ui.update_loading(token, "late", None).unwrap());
    assert!(ui.remove(p.message).is_err());
    let parent = ui.runtime.tree().parent(p.message).unwrap();
    ui.runtime
        .apply(Mutation::Reparent {
            node: p.message,
            parent: ui.root(),
        })
        .unwrap();
    assert!(ui.begin_loading(p.container, "No mutation", true).is_err());
    ui.runtime
        .apply(Mutation::Reparent {
            node: p.message,
            parent,
        })
        .unwrap();
    ui.remove(p.container).unwrap();
    assert!(ui.loading_status(p.container).is_none());
}
