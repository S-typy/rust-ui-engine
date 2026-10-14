use crate::*;
use rust_desktop_ui_core::{
    Color, InputEvent, Key, LayoutStyle, Length, Modifiers, Mutation, Point, PointerButton, Size,
    WidgetId,
};
use std::{cell::Cell, rc::Rc};

fn bounds(width: f32, height: f32) -> LayoutStyle {
    LayoutStyle {
        width: Length::Px(width),
        height: Length::Px(height),
        ..Default::default()
    }
}

fn frame(ui: &mut Controls) {
    ui.update(Size::new(600.0, 480.0)).unwrap();
}

fn editor(value: &str) -> (Controls, WidgetId) {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let id = ui
        .edit_box(ui.root(), "Name", value, bounds(280.0, 40.0))
        .unwrap();
    frame(&mut ui);
    ui.runtime.apply(Mutation::Focus(Some(id))).unwrap();
    frame(&mut ui);
    (ui, id)
}

fn key(ui: &mut Controls, key: Key, modifiers: Modifiers, repeat: bool) {
    ui.dispatch(InputEvent::KeyDown {
        key,
        modifiers,
        repeat,
    })
    .unwrap();
}

fn ctrl() -> Modifiers {
    Modifiers {
        control: true,
        ..Default::default()
    }
}

fn required(ui: &mut Controls, id: WidgetId) {
    ui.set_editor_validation(
        id,
        EditorValidation {
            is_required: true,
            message: "Name is required".into(),
        },
    )
    .unwrap();
}

fn commit_on_blur(ui: &mut Controls, id: WidgetId) {
    let mut settings = ui.editor_settings(id).unwrap().clone();
    settings.commit_on_focus_lost = true;
    ui.set_editor_settings(id, settings).unwrap();
}

#[test]
fn draft_commit_and_cancel_keep_a_separate_accepted_value() {
    let (mut ui, id) = editor("Ada");
    assert_eq!(ui.accepted_value(id), Some("Ada"));
    assert_eq!(ui.is_edit_dirty(id), Some(false));
    assert!(ui.drain_edit_events().is_empty());
    ui.text_input("!").unwrap();
    assert_eq!(ui.document(id).unwrap().text(), "Ada!");
    assert_eq!(ui.accepted_value(id), Some("Ada"));
    assert_eq!(ui.is_edit_dirty(id), Some(true));
    ui.drain_text_changes();
    assert_eq!(ui.commit_edit(id).unwrap(), CommitOutcome::Committed);
    assert_eq!(ui.accepted_value(id), Some("Ada!"));
    assert_eq!(ui.is_edit_dirty(id), Some(false));
    assert!(ui.document(id).unwrap().can_undo());
    assert_eq!(
        ui.drain_edit_events(),
        vec![EditEvent::Committed {
            id,
            value: "Ada!".into()
        }]
    );
    assert!(ui.drain_text_changes().is_empty());
    assert_eq!(ui.commit_edit(id).unwrap(), CommitOutcome::Unchanged);
    assert!(ui.drain_edit_events().is_empty());
    key(&mut ui, Key::Character("z".into()), ctrl(), false);
    assert_eq!(ui.document(id).unwrap().text(), "Ada");
    assert_eq!(ui.accepted_value(id), Some("Ada!"));
    assert_eq!(ui.is_edit_dirty(id), Some(true));
    ui.drain_text_changes();
    assert!(ui.cancel_edit(id).unwrap());
    assert_eq!(ui.document(id).unwrap().text(), "Ada!");
    assert_eq!(ui.document(id).unwrap().selection().focus, 4);
    assert_eq!(ui.is_edit_dirty(id), Some(false));
    assert!(!ui.document(id).unwrap().can_undo());
    assert!(!ui.document(id).unwrap().can_redo());
    assert_eq!(ui.drain_edit_events(), vec![EditEvent::Canceled { id }]);
    assert_eq!(
        ui.drain_text_changes(),
        vec![TextChanged {
            id,
            text: "Ada!".into()
        }]
    );
    assert!(!ui.cancel_edit(id).unwrap());
    assert!(ui.drain_edit_events().is_empty());
}

#[test]
fn validation_is_deferred_and_required_uses_unicode_whitespace() {
    let (mut ui, id) = editor("\u{2003} ");
    let calls = Rc::new(Cell::new(0));
    let observed = calls.clone();
    required(&mut ui, id);
    ui.set_editor_validator(
        id,
        Some(EditorValidator::new(move |_| {
            observed.set(observed.get() + 1);
            Ok(())
        })),
    )
    .unwrap();
    assert_eq!(calls.get(), 0);
    assert!(ui.validation_error(id).is_none());
    assert_eq!(ui.accepted_value(id), Some("\u{2003} "));
    assert_eq!(
        ui.commit_edit(id).unwrap(),
        CommitOutcome::Invalid("Name is required".into())
    );
    assert_eq!(
        calls.get(),
        0,
        "Required validation should reject before the application validator"
    );
    assert_eq!(ui.validation_error(id), Some("Name is required"));
    assert_eq!(ui.document(id).unwrap().text(), "\u{2003} ");
    assert_eq!(ui.is_edit_dirty(id), Some(false));
    assert_eq!(
        ui.drain_edit_events(),
        vec![EditEvent::ValidationFailed {
            id,
            message: "Name is required".into()
        }]
    );
    assert_eq!(
        ui.commit_edit(id).unwrap(),
        CommitOutcome::Invalid("Name is required".into())
    );
    assert_eq!(ui.drain_edit_events().len(), 1);
    assert!(ui.drain_text_changes().is_empty());
    ui.set_text(id, "  Ada  ").unwrap();
    assert!(ui.validation_error(id).is_none());
    assert_eq!(ui.commit_edit(id).unwrap(), CommitOutcome::Committed);
    assert_eq!(calls.get(), 1);
    assert_eq!(
        ui.accepted_value(id),
        Some("  Ada  "),
        "Required validation must not trim stored values"
    );
}

#[test]
fn rejected_commit_keeps_draft_selection_history_and_does_not_trap_focus() {
    let (mut ui, id) = editor("Ada");
    ui.set_editor_validator(
        id,
        Some(EditorValidator::new(|value| {
            if value.contains('!') {
                Err("No punctuation".into())
            } else {
                Ok(())
            }
        })),
    )
    .unwrap();
    ui.text_input("!").unwrap();
    key(&mut ui, Key::Character("a".into()), ctrl(), false);
    frame(&mut ui);
    let selection = ui.document(id).unwrap().selection();
    let caret = ui.caret_bounds().unwrap();
    ui.drain_text_changes();
    assert_eq!(
        ui.commit_edit(id).unwrap(),
        CommitOutcome::Invalid("No punctuation".into())
    );
    frame(&mut ui);
    assert_eq!(ui.document(id).unwrap().text(), "Ada!");
    assert_eq!(ui.accepted_value(id), Some("Ada"));
    assert_eq!(ui.document(id).unwrap().selection(), selection);
    assert_eq!(ui.caret_bounds().unwrap(), caret);
    assert!(ui.document(id).unwrap().can_undo());
    assert!(ui.drain_text_changes().is_empty());
    let next = ui
        .button(ui.root(), "Next", None, bounds(120.0, 36.0))
        .unwrap();
    frame(&mut ui);
    ui.perform_action(next, SemanticAction::Focus, None)
        .unwrap();
    frame(&mut ui);
    assert_eq!(ui.runtime.focused(), Some(next));
    assert_eq!(ui.document(id).unwrap().text(), "Ada!");
}

#[test]
fn authoritative_value_rejection_preserves_error_composition_selection_and_queues() {
    let (mut ui, id) = editor("Ada");
    required(&mut ui, id);
    ui.set_text(id, "").unwrap();
    ui.commit_edit(id).unwrap();
    ui.ime_preedit("candidate", Some((9, 9))).unwrap();
    frame(&mut ui);
    let selection = ui.document(id).unwrap().selection();
    let composition = ui.document(id).unwrap().composition().cloned();
    let caret = ui.caret_bounds().unwrap();
    ui.drain_edit_events();
    ui.drain_text_changes();
    assert_eq!(
        ui.set_editor_value(id, "\u{2003} ").unwrap(),
        CommitOutcome::Invalid("Name is required".into())
    );
    assert_eq!(ui.accepted_value(id), Some("Ada"));
    assert_eq!(ui.document(id).unwrap().text(), "");
    assert_eq!(ui.document(id).unwrap().selection(), selection);
    assert_eq!(ui.document(id).unwrap().composition(), composition.as_ref());
    assert_eq!(ui.validation_error(id), Some("Name is required"));
    assert_eq!(ui.caret_bounds().unwrap(), caret);
    assert!(ui.document(id).unwrap().can_undo());
    assert!(ui.drain_edit_events().is_empty());
    assert!(ui.drain_text_changes().is_empty());
    assert!(ui.set_editor_value(id, &"x".repeat(1_048_577)).is_err());
    assert_eq!(ui.document(id).unwrap().composition(), composition.as_ref());
    assert_eq!(ui.validation_error(id), Some("Name is required"));
    assert!(ui.drain_edit_events().is_empty());
    assert_eq!(
        ui.set_editor_value(id, "G\nr\ta\rce").unwrap(),
        CommitOutcome::Committed
    );
    assert_eq!(ui.accepted_value(id), Some("Grace"));
    assert_eq!(ui.document(id).unwrap().text(), "Grace");
    assert_eq!(ui.document(id).unwrap().selection().focus, 5);
    assert!(ui.document(id).unwrap().composition().is_none());
    assert!(ui.validation_error(id).is_none());
    assert!(!ui.document(id).unwrap().can_undo());
    assert!(!ui.document(id).unwrap().can_redo());
    assert_eq!(
        ui.drain_edit_events(),
        vec![EditEvent::Committed {
            id,
            value: "Grace".into()
        }]
    );
    assert_eq!(
        ui.drain_text_changes(),
        vec![TextChanged {
            id,
            text: "Grace".into()
        }]
    );
}

#[test]
fn authoritative_value_has_independent_draft_and_accepted_change_notifications() {
    let (mut ui, id) = editor("Ada");
    ui.set_text(id, "Grace").unwrap();
    ui.drain_text_changes();
    assert_eq!(
        ui.set_editor_value(id, "Grace").unwrap(),
        CommitOutcome::Committed
    );
    assert!(ui.drain_text_changes().is_empty());
    assert_eq!(
        ui.drain_edit_events(),
        vec![EditEvent::Committed {
            id,
            value: "Grace".into()
        }]
    );
    ui.set_text(id, "draft").unwrap();
    ui.drain_text_changes();
    assert_eq!(
        ui.set_editor_value(id, "Grace").unwrap(),
        CommitOutcome::Unchanged
    );
    assert_eq!(ui.document(id).unwrap().text(), "Grace");
    assert!(ui.drain_edit_events().is_empty());
    assert_eq!(
        ui.drain_text_changes(),
        vec![TextChanged {
            id,
            text: "Grace".into()
        }]
    );
    assert!(!ui.document(id).unwrap().can_undo());
    assert!(!ui.document(id).unwrap().can_redo());
}

#[test]
fn active_ime_defers_commit_but_cancel_discards_composition_and_draft() {
    let (mut ui, id) = editor("Ada");
    let calls = Rc::new(Cell::new(0));
    let observed = calls.clone();
    ui.set_editor_validator(
        id,
        Some(EditorValidator::new(move |_| {
            observed.set(observed.get() + 1);
            Ok(())
        })),
    )
    .unwrap();
    ui.text_input("!").unwrap();
    ui.ime_preedit("draft", Some((5, 5))).unwrap();
    let composition = ui.document(id).unwrap().composition().cloned();
    ui.drain_text_changes();
    assert_eq!(
        ui.commit_edit(id).unwrap(),
        CommitOutcome::CompositionActive
    );
    assert_eq!(calls.get(), 0);
    assert_eq!(ui.document(id).unwrap().composition(), composition.as_ref());
    assert_eq!(ui.accepted_value(id), Some("Ada"));
    assert!(ui.drain_edit_events().is_empty());
    assert!(ui.cancel_edit(id).unwrap());
    assert_eq!(ui.document(id).unwrap().text(), "Ada");
    assert!(ui.document(id).unwrap().composition().is_none());
    assert!(!ui.document(id).unwrap().can_undo());
    assert_eq!(ui.drain_edit_events(), vec![EditEvent::Canceled { id }]);
    assert_eq!(
        ui.drain_text_changes(),
        vec![TextChanged {
            id,
            text: "Ada".into()
        }]
    );
}

#[test]
fn cancel_clears_redo_or_validation_even_if_draft_equals_baseline() {
    let (mut ui, id) = editor("Ada");
    ui.text_input("!").unwrap();
    key(&mut ui, Key::Character("z".into()), ctrl(), false);
    assert_eq!(ui.is_edit_dirty(id), Some(false));
    assert!(ui.document(id).unwrap().can_redo());
    ui.drain_text_changes();
    assert!(ui.cancel_edit(id).unwrap());
    assert!(!ui.document(id).unwrap().can_redo());
    assert!(!ui.document(id).unwrap().can_undo());
    assert_eq!(ui.drain_edit_events(), vec![EditEvent::Canceled { id }]);
    assert!(ui.drain_text_changes().is_empty());
    ui.set_editor_validator(id, Some(EditorValidator::new(|_| Err("Rejected".into()))))
        .unwrap();
    ui.commit_edit(id).unwrap();
    ui.drain_edit_events();
    assert!(ui.cancel_edit(id).unwrap());
    assert_eq!(ui.accepted_value(id), Some("Ada"));
    assert!(ui.validation_error(id).is_none());
    assert_eq!(ui.drain_edit_events(), vec![EditEvent::Canceled { id }]);
    assert!(ui.drain_text_changes().is_empty());
    assert!(!ui.cancel_edit(id).unwrap());
}

#[test]
fn only_actual_draft_edits_clear_stale_validation_errors() {
    let (mut ui, id) = editor("Ada");
    ui.set_max_length(id, Some(3)).unwrap();
    ui.set_editor_validator(id, Some(EditorValidator::new(|_| Err("Rejected".into()))))
        .unwrap();
    ui.commit_edit(id).unwrap();
    ui.drain_edit_events();
    ui.set_text(id, "Ada").unwrap();
    ui.text_input("!").unwrap();
    ui.ime_preedit("candidate", Some((9, 9))).unwrap();
    assert_eq!(ui.validation_error(id), Some("Rejected"));
    ui.ime_commit("too long").unwrap();
    assert_eq!(ui.document(id).unwrap().text(), "Ada");
    assert!(ui.document(id).unwrap().composition().is_none());
    assert_eq!(ui.validation_error(id), Some("Rejected"));
    assert!(ui.drain_text_changes().is_empty());
    assert!(ui.drain_edit_events().is_empty());
    key(&mut ui, Key::Backspace, Modifiers::default(), false);
    assert_eq!(ui.document(id).unwrap().text(), "Ad");
    assert!(ui.validation_error(id).is_none());
    assert_eq!(ui.accepted_value(id), Some("Ada"));
    assert!(ui.drain_edit_events().is_empty());
}

#[test]
fn replacing_validation_rules_clears_error_without_validating_or_changing_values() {
    let (mut ui, id) = editor("");
    required(&mut ui, id);
    ui.commit_edit(id).unwrap();
    ui.drain_edit_events();
    ui.set_editor_validation(id, EditorValidation::default())
        .unwrap();
    assert!(ui.validation_error(id).is_none());
    let calls = Rc::new(Cell::new(0));
    let observed = calls.clone();
    ui.set_editor_validator(
        id,
        Some(EditorValidator::new(move |_| {
            observed.set(observed.get() + 1);
            Err("No".into())
        })),
    )
    .unwrap();
    assert_eq!(calls.get(), 0);
    assert!(ui.validation_error(id).is_none());
    ui.commit_edit(id).unwrap();
    assert_eq!(calls.get(), 1);
    ui.drain_edit_events();
    ui.set_editor_validator(id, None).unwrap();
    assert!(ui.validation_error(id).is_none());
    assert_eq!(ui.accepted_value(id), Some(""));
    assert_eq!(ui.document(id).unwrap().text(), "");
    assert!(ui.drain_edit_events().is_empty());
    assert!(ui.drain_text_changes().is_empty());
}

#[test]
fn maxlength_must_allow_both_current_draft_and_the_cancel_baseline() {
    let (mut ui, id) = editor("long");
    ui.set_text(id, "x").unwrap();
    ui.ime_preedit("draft", Some((5, 5))).unwrap();
    let composition = ui.document(id).unwrap().composition().cloned();
    let original = ui.editor_settings(id).unwrap().clone();
    let mut rejected = original.clone();
    rejected.max_length = Some(1);
    rejected.is_read_only = true;
    assert!(ui.set_editor_settings(id, rejected).is_err());
    assert_eq!(ui.editor_settings(id), Some(&original));
    assert_eq!(ui.document(id).unwrap().composition(), composition.as_ref());
    assert_eq!(ui.accepted_value(id), Some("long"));
    ui.cancel_preedit();
    ui.set_text(id, "longer").unwrap();
    assert!(ui.set_max_length(id, Some(4)).is_err());
    assert_eq!(ui.max_length(id), Some(None));
    ui.set_text(id, "x").unwrap();
    ui.commit_edit(id).unwrap();
    ui.set_max_length(id, Some(1)).unwrap();
    assert_eq!(ui.accepted_value(id), Some("x"));
    assert!(
        ui.cancel_edit(id).unwrap(),
        "Cancel must clear the history retained by commit"
    );
    assert_eq!(ui.document(id).unwrap().text(), "x");
}

#[test]
fn enter_commit_shortcuts_respect_single_line_modifiers_repeat_and_multiline() {
    let (mut ui, id) = editor("Ada");
    ui.set_text(id, "Grace").unwrap();
    for modifiers in [
        ctrl(),
        Modifiers {
            shift: true,
            ..Default::default()
        },
        Modifiers {
            alt: true,
            ..Default::default()
        },
    ] {
        key(&mut ui, Key::Enter, modifiers, false);
        assert_eq!(ui.accepted_value(id), Some("Ada"));
    }
    key(&mut ui, Key::Enter, Modifiers::default(), true);
    assert_eq!(ui.accepted_value(id), Some("Ada"));
    assert!(ui.drain_edit_events().is_empty());
    key(&mut ui, Key::Enter, Modifiers::default(), false);
    assert_eq!(ui.accepted_value(id), Some("Grace"));
    assert_eq!(
        ui.drain_edit_events(),
        vec![EditEvent::Committed {
            id,
            value: "Grace".into()
        }]
    );

    let multiline = ui
        .text_box(ui.root(), "Notes", "A", bounds(280.0, 100.0))
        .unwrap();
    frame(&mut ui);
    ui.runtime.apply(Mutation::Focus(Some(multiline))).unwrap();
    frame(&mut ui);
    key(&mut ui, Key::Enter, Modifiers::default(), false);
    assert_eq!(ui.document(multiline).unwrap().text(), "A\n");
    assert_eq!(ui.accepted_value(multiline), Some("A"));
    for modifiers in [
        ctrl(),
        Modifiers {
            super_key: true,
            ..Default::default()
        },
    ] {
        key(&mut ui, Key::Enter, modifiers, true);
        assert_eq!(ui.accepted_value(multiline), Some("A"));
        assert_eq!(
            ui.document(multiline).unwrap().text(),
            "A\n",
            "Repeated commit shortcut must not insert a newline"
        );
        assert!(ui.drain_edit_events().is_empty());
    }
    key(&mut ui, Key::Enter, ctrl(), false);
    assert_eq!(ui.accepted_value(multiline), Some("A\n"));
    ui.text_input("B").unwrap();
    key(
        &mut ui,
        Key::Enter,
        Modifiers {
            super_key: true,
            ..Default::default()
        },
        false,
    );
    assert_eq!(ui.accepted_value(multiline), Some("A\nB"));
    assert_eq!(ui.document(multiline).unwrap().text(), "A\nB");
}

#[test]
fn escape_closes_popup_then_cancels_ime_then_rolls_back_draft() {
    let (mut ui, id) = editor("Ada");
    let mut settings = ui.editor_settings(id).unwrap().clone();
    settings.commit_on_focus_lost = false;
    ui.set_editor_settings(id, settings).unwrap();
    ui.set_text(id, "draft").unwrap();
    ui.commands_mut().register("action", "Action");
    ui.popup_menu(id, &[MenuItem::new("Action", "action")])
        .unwrap();
    key(&mut ui, Key::Escape, Modifiers::default(), false);
    assert!(!ui.popup_open());
    assert_eq!(ui.runtime.focused(), Some(id));
    assert_eq!(ui.document(id).unwrap().text(), "draft");
    assert_eq!(ui.accepted_value(id), Some("Ada"));
    assert!(ui.drain_edit_events().is_empty());
    ui.ime_preedit("candidate", Some((9, 9))).unwrap();
    key(&mut ui, Key::Escape, Modifiers::default(), false);
    assert!(ui.document(id).unwrap().composition().is_none());
    assert_eq!(ui.document(id).unwrap().text(), "draft");
    assert!(ui.drain_edit_events().is_empty());
    key(&mut ui, Key::Escape, Modifiers::default(), true);
    key(
        &mut ui,
        Key::Escape,
        Modifiers {
            shift: true,
            ..Default::default()
        },
        false,
    );
    assert_eq!(ui.document(id).unwrap().text(), "draft");
    key(&mut ui, Key::Escape, Modifiers::default(), false);
    assert_eq!(ui.document(id).unwrap().text(), "Ada");
    assert_eq!(ui.drain_edit_events(), vec![EditEvent::Canceled { id }]);
    assert!(!ui.document(id).unwrap().can_undo());
}

#[test]
fn readonly_and_disabled_user_keys_do_not_commit_or_cancel_but_authoritative_load_works() {
    for readonly in [true, false] {
        let (mut ui, id) = editor("Ada");
        ui.set_text(id, "draft").unwrap();
        if readonly {
            ui.set_read_only(id, true).unwrap();
        } else {
            ui.set_enabled(id, false).unwrap();
        }
        key(&mut ui, Key::Enter, Modifiers::default(), false);
        key(&mut ui, Key::Escape, Modifiers::default(), false);
        assert_eq!(ui.accepted_value(id), Some("Ada"));
        assert_eq!(ui.document(id).unwrap().text(), "draft");
        assert!(ui.drain_edit_events().is_empty());
        assert_eq!(
            ui.set_editor_value(id, "Grace").unwrap(),
            CommitOutcome::Committed
        );
        assert_eq!(ui.accepted_value(id), Some("Grace"));
        assert_eq!(ui.document(id).unwrap().text(), "Grace");
        assert!(!ui.document(id).unwrap().can_undo());
    }
}

#[derive(Clone, Copy, Debug)]
enum FocusRoute {
    Tab,
    Pointer,
    Semantic,
    Runtime,
    WindowBlur,
}

fn move_focus(ui: &mut Controls, next: WidgetId, route: FocusRoute) {
    match route {
        FocusRoute::Tab => key(ui, Key::Tab, Modifiers::default(), false),
        FocusRoute::Pointer => {
            let bounds = ui.runtime.bounds(next).unwrap();
            let position = Point::new(
                bounds.x + bounds.width * 0.5,
                bounds.y + bounds.height * 0.5,
            );
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
        FocusRoute::Semantic => ui
            .perform_action(next, SemanticAction::Focus, None)
            .unwrap(),
        FocusRoute::Runtime => ui.runtime.apply(Mutation::Focus(Some(next))).unwrap(),
        FocusRoute::WindowBlur => {
            ui.dispatch(InputEvent::WindowFocused(false)).unwrap();
        }
    }
    frame(ui);
}

#[test]
fn default_commit_on_focus_lost_accepts_once_for_every_supported_focus_route() {
    assert!(EditorSettings::default().commit_on_focus_lost);
    for route in [
        FocusRoute::Tab,
        FocusRoute::Pointer,
        FocusRoute::Semantic,
        FocusRoute::Runtime,
        FocusRoute::WindowBlur,
    ] {
        let (mut ui, id) = editor("Ada");
        let next = ui
            .button(ui.root(), "Next", None, bounds(120.0, 36.0))
            .unwrap();
        frame(&mut ui);
        ui.set_text(id, "Grace").unwrap();
        move_focus(&mut ui, next, route);
        assert_ne!(ui.runtime.focused(), Some(id), "{route:?}");
        assert_eq!(ui.accepted_value(id), Some("Grace"), "{route:?}");
        assert_eq!(
            ui.drain_edit_events(),
            vec![EditEvent::Committed {
                id,
                value: "Grace".into()
            }],
            "{route:?}"
        );
        frame(&mut ui);
        assert!(ui.drain_edit_events().is_empty(), "{route:?}");
    }
}

#[test]
fn explicit_focus_commit_optout_and_invalid_blur_preserve_draft_without_focus_trap() {
    let (mut ui, id) = editor("Ada");
    let mut settings = ui.editor_settings(id).unwrap().clone();
    settings.commit_on_focus_lost = false;
    ui.set_editor_settings(id, settings).unwrap();
    let next = ui
        .button(ui.root(), "Next", None, bounds(120.0, 36.0))
        .unwrap();
    frame(&mut ui);
    ui.set_text(id, "draft").unwrap();
    move_focus(&mut ui, next, FocusRoute::Tab);
    assert_eq!(ui.accepted_value(id), Some("Ada"));
    assert_eq!(ui.document(id).unwrap().text(), "draft");
    assert!(ui.drain_edit_events().is_empty());
    ui.runtime.apply(Mutation::Focus(Some(id))).unwrap();
    frame(&mut ui);
    commit_on_blur(&mut ui, id);
    required(&mut ui, id);
    ui.set_text(id, "").unwrap();
    move_focus(&mut ui, next, FocusRoute::Semantic);
    assert_eq!(ui.runtime.focused(), Some(next));
    assert_eq!(ui.accepted_value(id), Some("Ada"));
    assert_eq!(ui.document(id).unwrap().text(), "");
    assert_eq!(ui.validation_error(id), Some("Name is required"));
    assert_eq!(
        ui.drain_edit_events(),
        vec![EditEvent::ValidationFailed {
            id,
            message: "Name is required".into()
        }]
    );
    frame(&mut ui);
    assert!(ui.drain_edit_events().is_empty());
}

#[test]
fn blur_during_composition_cancels_preedit_and_skips_implicit_commit() {
    for route in [
        FocusRoute::Tab,
        FocusRoute::Pointer,
        FocusRoute::Semantic,
        FocusRoute::Runtime,
        FocusRoute::WindowBlur,
    ] {
        let (mut ui, id) = editor("Ada");
        let next = ui
            .button(ui.root(), "Next", None, bounds(120.0, 36.0))
            .unwrap();
        frame(&mut ui);
        ui.set_text(id, "draft").unwrap();
        ui.ime_preedit("candidate", Some((9, 9))).unwrap();
        move_focus(&mut ui, next, route);
        assert!(
            ui.document(id).unwrap().composition().is_none(),
            "{route:?}"
        );
        assert_eq!(ui.accepted_value(id), Some("Ada"), "{route:?}");
        assert_eq!(ui.document(id).unwrap().text(), "draft", "{route:?}");
        assert!(ui.drain_edit_events().is_empty(), "{route:?}");
        frame(&mut ui);
        assert_eq!(ui.accepted_value(id), Some("Ada"));
        assert!(ui.drain_edit_events().is_empty());
    }
}

#[test]
fn disabling_hiding_or_readonly_transition_does_not_implicitly_commit() {
    for transition in 0..3 {
        let (mut ui, id) = editor("Ada");
        let next = ui
            .button(ui.root(), "Next", None, bounds(120.0, 36.0))
            .unwrap();
        frame(&mut ui);
        ui.set_text(id, "draft").unwrap();
        match transition {
            0 => ui.set_enabled(id, false).unwrap(),
            1 => {
                let mut props = ui.runtime.tree().node(id).unwrap().props.clone();
                props.visible = false;
                ui.runtime
                    .apply(Mutation::SetProps { node: id, props })
                    .unwrap();
            }
            _ => ui.set_read_only(id, true).unwrap(),
        }
        move_focus(&mut ui, next, FocusRoute::Semantic);
        assert_eq!(ui.accepted_value(id), Some("Ada"));
        assert_eq!(ui.document(id).unwrap().text(), "draft");
        assert!(ui.drain_edit_events().is_empty());
    }
}

#[test]
fn validation_error_affects_scene_and_semantics_without_overriding_disabled_palette() {
    let (mut ui, id) = editor("Ada");
    let foreground = Color::rgb(190, 10, 30);
    let border = Color::rgb(200, 30, 50);
    ui.set_edit_box_style(
        id,
        EditBoxStyle {
            invalid: EditBoxAppearance {
                foreground: Some(foreground),
                border: Some(border),
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .unwrap();
    ui.set_editor_validator(id, Some(EditorValidator::new(|_| Err("Rejected".into()))))
        .unwrap();
    ui.commit_edit(id).unwrap();
    frame(&mut ui);
    assert!(
        ui.scene()
            .rounded_rectangles
            .iter()
            .any(|r| r.stroke_width.is_some() && r.color == border)
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
    let semantic = ui
        .semantics()
        .into_iter()
        .find(|node| node.id == id)
        .unwrap();
    assert_eq!(semantic.validation_error.as_deref(), Some("Rejected"));
    assert_eq!(semantic.value.as_deref(), Some("Ada"));
    ui.set_read_only(id, true).unwrap();
    frame(&mut ui);
    assert!(
        ui.scene()
            .rounded_rectangles
            .iter()
            .any(|r| r.stroke_width.is_some() && r.color == border)
    );
    ui.set_enabled(id, false).unwrap();
    frame(&mut ui);
    assert!(
        !ui.scene()
            .rounded_rectangles
            .iter()
            .any(|r| r.stroke_width.is_some() && r.color == border)
    );
    assert_eq!(
        ui.scene()
            .texts
            .iter()
            .find(|r| r.text == "Ada")
            .unwrap()
            .color,
        Theme::FluentLight.chrome().disabled_text
    );
    let semantic = ui
        .semantics()
        .into_iter()
        .find(|node| node.id == id)
        .unwrap();
    assert_eq!(semantic.validation_error.as_deref(), Some("Rejected"));
    assert!(semantic.disabled);
}

#[test]
fn removed_editors_drop_session_events_and_reused_slots_start_with_new_baselines() {
    let (mut ui, id) = editor("Ada");
    ui.set_text(id, "Grace").unwrap();
    ui.commit_edit(id).unwrap();
    ui.set_editor_validator(id, Some(EditorValidator::new(|_| Err("Rejected".into()))))
        .unwrap();
    ui.commit_edit(id).unwrap();
    ui.cancel_edit(id).unwrap();
    ui.remove(id).unwrap();
    let replacement = ui
        .edit_box(ui.root(), "Replacement", "Lin", bounds(280.0, 40.0))
        .unwrap();
    frame(&mut ui);
    assert_ne!(replacement, id);
    assert_eq!(ui.accepted_value(replacement), Some("Lin"));
    assert_eq!(ui.is_edit_dirty(replacement), Some(false));
    assert!(ui.validation_error(replacement).is_none());
    assert!(ui.drain_edit_events().is_empty());
    assert!(ui.drain_text_changes().is_empty());
    assert!(ui.accepted_value(id).is_none());
    assert!(ui.is_edit_dirty(id).is_none());
    assert!(ui.commit_edit(id).is_err());
    assert!(ui.cancel_edit(id).is_err());
    assert!(ui.set_editor_value(id, "stale").is_err());
    assert_eq!(ui.accepted_value(replacement), Some("Lin"));
}

#[test]
fn direct_runtime_removal_invalidates_session_apis_and_discards_queued_events() {
    let (mut ui, removed) = editor("Ada");
    ui.set_text(removed, "Grace").unwrap();
    ui.commit_edit(removed).unwrap();
    ui.set_editor_validator(
        removed,
        Some(EditorValidator::new(|_| Err("Rejected".into()))),
    )
    .unwrap();
    ui.commit_edit(removed).unwrap();
    ui.cancel_edit(removed).unwrap();
    ui.runtime.apply(Mutation::Remove(removed)).unwrap();
    assert!(ui.accepted_value(removed).is_none());
    assert!(ui.is_edit_dirty(removed).is_none());
    assert!(ui.validation_error(removed).is_none());
    assert!(ui.editor_validation(removed).is_none());
    assert!(ui.commit_edit(removed).is_err());
    assert!(ui.cancel_edit(removed).is_err());
    assert!(ui.set_editor_value(removed, "stale").is_err());
    assert!(
        ui.set_editor_validation(removed, EditorValidation::default())
            .is_err()
    );
    assert!(ui.set_editor_validator(removed, None).is_err());
    let replacement = ui
        .edit_box(ui.root(), "Replacement", "Lin", bounds(280.0, 40.0))
        .unwrap();
    assert_ne!(replacement, removed);
    assert_eq!(
        ui.set_editor_value(replacement, "New").unwrap(),
        CommitOutcome::Committed
    );
    frame(&mut ui);
    assert_eq!(ui.accepted_value(replacement), Some("New"));
    assert_eq!(
        ui.drain_edit_events(),
        vec![EditEvent::Committed {
            id: replacement,
            value: "New".into()
        }]
    );
    assert!(ui.drain_edit_events().is_empty());
}
