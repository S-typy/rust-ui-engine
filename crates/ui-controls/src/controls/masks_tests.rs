use super::*;
use crate::UiCulture;
use crate::{
    BindingContext, BindingProperty, BindingUpdateTrigger, ControlBinding, ControlBindingMode,
    ControlBindings, DataValue,
};
use rust_desktop_ui_core::Modifiers;
use std::{cell::RefCell, rc::Rc};

fn setup(pattern: &str, raw: &str) -> (Controls, WidgetId) {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let id = ui
        .masked_edit(
            ui.root(),
            "Masked",
            raw,
            MaskSettings::new(pattern).unwrap(),
            LayoutStyle {
                width: Length::Px(400.0),
                height: Length::Px(40.0),
                ..Default::default()
            },
        )
        .unwrap();
    ui.update(Size::new(640.0, 480.0)).unwrap();
    ui.perform_action(id, SemanticAction::Focus, None).unwrap();
    (ui, id)
}
fn select(ui: &mut Controls, id: WidgetId, anchor: usize, focus: usize) {
    let Kind::TextBox(document) = &mut ui.controls.get_mut(&id).unwrap().kind else {
        unreachable!()
    };
    document.set_selection(anchor, focus).unwrap();
}
fn key(ui: &mut Controls, key: Key, control: bool, shift: bool) {
    ui.dispatch(InputEvent::KeyDown {
        key,
        modifiers: Modifiers {
            control,
            shift,
            ..Default::default()
        },
        repeat: false,
    })
    .unwrap();
}
fn bind(
    ui: &mut Controls,
    id: WidgetId,
    trigger: BindingUpdateTrigger,
) -> (Rc<RefCell<BindingContext>>, ControlBindings) {
    let data = Rc::new(RefCell::new(
        BindingContext::new(DataValue::object([(
            "Raw",
            ui.masked_value(id).unwrap().into(),
        )]))
        .unwrap(),
    ));
    let mut host = ControlBindings::new(Rc::clone(&data));
    let mut binding = ControlBinding::new(id, BindingProperty::Value, "Raw").unwrap();
    binding.mode = ControlBindingMode::TwoWay;
    binding.trigger = trigger;
    host.add(binding).unwrap();
    host.initialize(ui).unwrap();
    (data, host)
}

#[test]
fn mask_typing_skips_literals_retains_incomplete_draft_and_accepted_value() {
    let (mut ui, id) = setup("(00)-00", "1234");
    select(&mut ui, id, 0, 7);
    ui.text_input("5").unwrap();
    assert_eq!(ui.document(id).unwrap().text(), "(5_)-__");
    assert_eq!(ui.document(id).unwrap().selection().focus, 2);
    assert_eq!(ui.masked_value(id).as_deref(), Some("1234"));
    assert!(matches!(
        ui.commit_edit(id).unwrap(),
        CommitOutcome::Invalid(_)
    ));
    ui.text_input("6").unwrap();
    assert_eq!(ui.document(id).unwrap().selection().focus, 5);
    ui.text_input("78").unwrap();
    assert_eq!(ui.commit_edit(id).unwrap(), CommitOutcome::Committed);
    assert_eq!(ui.masked_value(id).as_deref(), Some("5678"));
    ui.set_text(id, "(__)-__").unwrap();
    ui.cancel_edit(id).unwrap();
    assert_eq!(ui.document(id).unwrap().text(), "(56)-78");
    assert!(!ui.document(id).unwrap().can_undo());
}

#[test]
fn mask_raw_and_formatted_paste_are_atomic_with_literal_matching_data() {
    for text in ["1234", "(12)-34"] {
        let (mut ui, id) = setup("(00)-00", "");
        select(&mut ui, id, 0, 7);
        ui.text_input(text).unwrap();
        assert_eq!(ui.document(id).unwrap().text(), "(12)-34");
        assert_eq!(ui.document(id).unwrap().undo_text(), Some("(__)-__"));
        let before = ui.document(id).unwrap().clone();
        select(&mut ui, id, 0, 7);
        for invalid in ["12Z4", "12345", "(12)+34", "12_4"] {
            ui.text_input(invalid).unwrap();
            assert_eq!(ui.document(id).unwrap().text(), before.text());
            assert_eq!(ui.document(id).unwrap().selection().range(), 0..7);
            assert_eq!(ui.document(id).unwrap().undo_text(), before.undo_text());
        }
    }
    for (input, expected) in [("10", "100"), ("100", "100"), ("20", "200")] {
        let (mut ui, id) = setup("0\\09", "");
        select(&mut ui, id, 0, 3);
        ui.text_input(input).unwrap();
        assert_eq!(ui.document(id).unwrap().text(), expected);
        assert_eq!(
            ui.editor_mask_settings(id)
                .unwrap()
                .parse_display(expected)
                .unwrap()
                .storage,
            expected[..1].to_owned() + &expected[2..]
        );
    }
}

#[test]
fn mask_selection_delete_cut_paste_and_undo_preserve_literals() {
    let (mut ui, id) = setup("00-00", "1234");
    select(&mut ui, id, 1, 4);
    key(&mut ui, Key::Delete, false, false);
    assert_eq!(ui.document(id).unwrap().text(), "1_-_4");
    assert_eq!(ui.document(id).unwrap().selection().focus, 1);
    key(&mut ui, Key::Character("z".into()), true, false);
    assert_eq!(ui.document(id).unwrap().text(), "12-34");
    assert_eq!(ui.document(id).unwrap().selection().range(), 1..4);
    key(&mut ui, Key::Character("y".into()), true, false);
    assert_eq!(ui.document(id).unwrap().text(), "1_-_4");
    select(&mut ui, id, 0, 5);
    ui.text_input("5678").unwrap();
    select(&mut ui, id, 1, 4);
    key(&mut ui, Key::Character("x".into()), true, false);
    assert_eq!(ui.document(id).unwrap().text(), "5_-_8");
    assert!(
        matches!(ui.drain_clipboard_requests().as_slice(), [ClipboardRequest::Copy(value)] if value == "6-7")
    );
    ui.text_input("6-7").unwrap();
    assert_eq!(ui.document(id).unwrap().text(), "56-78");
    select(&mut ui, id, 3, 3);
    key(&mut ui, Key::Backspace, false, false);
    assert_eq!(ui.document(id).unwrap().text(), "5_-78");
    assert_eq!(ui.document(id).unwrap().selection().focus, 1);
}

#[test]
fn mask_navigation_skips_literals_and_ctrl_moves_segments() {
    let (mut ui, id) = setup("(00)-00", "1234");
    key(&mut ui, Key::Home, false, false);
    assert_eq!(ui.document(id).unwrap().selection().focus, 1);
    key(&mut ui, Key::ArrowRight, false, false);
    assert_eq!(ui.document(id).unwrap().selection().focus, 2);
    key(&mut ui, Key::ArrowRight, false, true);
    assert_eq!(ui.document(id).unwrap().selection().range(), 2..5);
    key(&mut ui, Key::End, false, false);
    assert_eq!(ui.document(id).unwrap().selection().focus, 7);
    key(&mut ui, Key::ArrowLeft, true, false);
    assert_eq!(ui.document(id).unwrap().selection().focus, 5);
    key(&mut ui, Key::ArrowLeft, true, false);
    assert_eq!(ui.document(id).unwrap().selection().focus, 1);
    key(&mut ui, Key::Delete, true, false);
    assert_eq!(ui.document(id).unwrap().text(), "(__)-34");
}

#[test]
fn mask_unicode_grapheme_input_undo_and_ime_commit_are_atomic() {
    let (mut ui, id) = setup("XX-X", "");
    select(&mut ui, id, 0, 4);
    ui.text_input("e\u{301}👩‍💻🇷🇺").unwrap();
    assert_eq!(ui.document(id).unwrap().text(), "e\u{301}👩‍💻-🇷🇺");
    key(&mut ui, Key::Backspace, false, false);
    assert_eq!(ui.document(id).unwrap().text(), "e\u{301}👩‍💻-_");
    key(&mut ui, Key::Character("z".into()), true, false);
    assert_eq!(ui.document(id).unwrap().text(), "e\u{301}👩‍💻-🇷🇺");
    select(&mut ui, id, 0, "e\u{301}".len());
    ui.ime_preedit("ni", Some((2, 2))).unwrap();
    assert!(ui.document(id).unwrap().composition().is_some());
    ui.ime_commit("你").unwrap();
    assert_eq!(ui.document(id).unwrap().text(), "你👩‍💻-🇷🇺");
    assert!(ui.document(id).unwrap().composition().is_none());
    key(&mut ui, Key::Character("z".into()), true, false);
    assert_eq!(ui.document(id).unwrap().text(), "e\u{301}👩‍💻-🇷🇺");
    let (mut ui, id) = setup("00", "12");
    select(&mut ui, id, 0, 2);
    ui.ime_preedit("phonetic", None).unwrap();
    ui.ime_commit("你").unwrap();
    assert_eq!(ui.document(id).unwrap().text(), "12");
    assert!(!ui.document(id).unwrap().can_undo());
    assert!(ui.document(id).unwrap().composition().is_none());
    assert_eq!(ui.document(id).unwrap().selection().range(), 0..2);
}

#[test]
fn mask_value_binding_separates_raw_display_draft_and_commit() {
    for trigger in [
        BindingUpdateTrigger::LostFocus,
        BindingUpdateTrigger::PropertyChanged,
    ] {
        let (mut ui, id) = setup("00-00", "1234");
        let (data, mut bindings) = bind(&mut ui, id, trigger);
        ui.set_text(id, "5_-__").unwrap();
        bindings.sync(&mut ui);
        assert_eq!(data.borrow().get("Raw").unwrap().as_str(), Some("1234"));
        assert!(matches!(
            ui.commit_edit(id).unwrap(),
            CommitOutcome::Invalid(_)
        ));
        ui.set_text(id, "56-78").unwrap();
        bindings.sync(&mut ui);
        let expected = if trigger == BindingUpdateTrigger::PropertyChanged {
            "5678"
        } else {
            "1234"
        };
        assert_eq!(data.borrow().get("Raw").unwrap().as_str(), Some(expected));
        ui.commit_edit(id).unwrap();
        bindings.sync(&mut ui);
        assert_eq!(data.borrow().get("Raw").unwrap().as_str(), Some("5678"));
        data.borrow_mut().set("Raw", "9012".into()).unwrap();
        bindings.sync(&mut ui);
        assert_eq!(ui.document(id).unwrap().text(), "90-12");
        data.borrow_mut().set("Raw", "wrong".into()).unwrap();
        bindings.sync(&mut ui);
        assert!(!bindings.diagnostics().is_empty());
        assert_eq!(ui.masked_value(id).as_deref(), Some("9012"));
    }
}

#[test]
fn mask_settings_live_culture_reformat_preserves_raw_and_slot_selection() {
    let (mut ui, id) = setup("00.00", "1234");
    let (data, mut bindings) = bind(&mut ui, id, BindingUpdateTrigger::LostFocus);
    select(&mut ui, id, 3, 4);
    let mut settings = ui.editor_mask_settings(id).unwrap().clone();
    settings.culture = UiCulture::Arabic;
    ui.set_editor_mask(id, Some(settings)).unwrap();
    assert_eq!(ui.document(id).unwrap().text(), "12٫34");
    assert_eq!(ui.document(id).unwrap().selection().range(), 4..5);
    assert!(!ui.document(id).unwrap().can_undo());
    bindings.sync(&mut ui);
    assert_eq!(data.borrow().get("Raw").unwrap().as_str(), Some("1234"));
    ui.set_text(id, "56٫78").unwrap();
    assert!(ui.set_editor_mask(id, None).is_err());
    ui.commit_edit(id).unwrap();
    bindings.sync(&mut ui);
    assert_eq!(data.borrow().get("Raw").unwrap().as_str(), Some("5678"));
    ui.set_editor_mask(id, None).unwrap();
    assert_eq!(ui.document(id).unwrap().text(), "5678");
    data.borrow_mut().set("Raw", "plain".into()).unwrap();
    bindings.sync(&mut ui);
    assert_eq!(ui.document(id).unwrap().text(), "plain");
}

#[test]
fn mask_settings_failures_preserve_value_history_selection_and_shared_parser() {
    let (mut ui, id) = setup("00-00", "1234");
    let (data, mut bindings) = bind(&mut ui, id, BindingUpdateTrigger::LostFocus);
    select(&mut ui, id, 1, 4);
    let before = ui.document(id).unwrap().clone();
    let old = ui.editor_mask_settings(id).unwrap().clone();
    assert!(
        ui.set_editor_mask(id, Some(MaskSettings::new("LL").unwrap()))
            .is_err()
    );
    assert_eq!(ui.document(id).unwrap().text(), before.text());
    assert_eq!(ui.document(id).unwrap().selection(), before.selection());
    ui.set_max_length(id, Some(5)).unwrap();
    assert!(
        ui.set_editor_mask(id, Some(MaskSettings::new("(00)-00").unwrap()))
            .is_err()
    );
    assert_eq!(ui.editor_mask_settings(id), Some(&old));
    assert_eq!(ui.document(id).unwrap().selection(), before.selection());
    data.borrow_mut().set("Raw", "5678".into()).unwrap();
    bindings.sync(&mut ui);
    assert_eq!(ui.document(id).unwrap().text(), "56-78");
    assert!(matches!(
        ui.set_masked_value(id, "1").unwrap(),
        CommitOutcome::Invalid(_)
    ));
    assert!(ui.set_text(id, "invalid").is_err());
    assert_eq!(ui.masked_value(id).as_deref(), Some("5678"));
}

#[test]
fn mask_readonly_disabled_limits_and_accessibility_reject_without_mutation() {
    let (mut ui, id) = setup("XX", "AB");
    ui.set_max_length(id, Some(2)).unwrap();
    select(&mut ui, id, 0, 2);
    ui.text_input("👩‍💻🇷🇺").unwrap();
    assert_eq!(ui.document(id).unwrap().text(), "👩‍💻🇷🇺");
    let before = ui.document(id).unwrap().text().to_owned();
    ui.set_read_only(id, true).unwrap();
    key(&mut ui, Key::Backspace, false, false);
    ui.text_input("X").unwrap();
    assert_eq!(ui.document(id).unwrap().text(), before);
    ui.set_read_only(id, false).unwrap();
    ui.perform_action(id, SemanticAction::SetValue, Some("too-long"))
        .unwrap();
    assert_eq!(ui.document(id).unwrap().text(), before);
    ui.set_enabled(id, false).unwrap();
    ui.text_input("Z").unwrap();
    assert_eq!(ui.document(id).unwrap().text(), before);
    ui.set_masked_value(id, "CD").unwrap();
    assert_eq!(ui.masked_value(id).as_deref(), Some("CD"));
}

#[test]
fn mask_attachment_reuses_button_editor_clear_bindings_and_rejects_semantic_editors() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let parts = ui
        .button_edit(
            ui.root(),
            "Code",
            "1234",
            vec![EditorButton {
                label: "Clear".into(),
                icon: Some(IconSymbol::Close),
                action: EditorButtonAction::Clear,
            }],
            LayoutStyle {
                width: Length::Px(300.0),
                ..Default::default()
            },
        )
        .unwrap();
    ui.set_editor_mask(parts.editor, Some(MaskSettings::new("00-00").unwrap()))
        .unwrap();
    assert_eq!(ui.masked_value(parts.editor).as_deref(), Some("1234"));
    let data = Rc::new(RefCell::new(
        BindingContext::new(DataValue::object([("Raw", "1234".into())])).unwrap(),
    ));
    let mut host = ControlBindings::new(Rc::clone(&data));
    let mut binding = ControlBinding::new(parts.container, BindingProperty::Value, "Raw").unwrap();
    binding.mode = ControlBindingMode::TwoWay;
    binding.trigger = BindingUpdateTrigger::LostFocus;
    host.add(binding).unwrap();
    host.initialize(&mut ui).unwrap();
    ui.update(Size::new(640.0, 480.0)).unwrap();
    ui.perform_action(parts.buttons[0], SemanticAction::Activate, None)
        .unwrap();
    assert_eq!(ui.document(parts.editor).unwrap().text(), "__-__");
    ui.commit_edit(parts.editor).unwrap();
    host.sync(&mut ui);
    assert_eq!(data.borrow().get("Raw").unwrap().as_str(), Some(""));
    let multiline = ui
        .text_box(ui.root(), "Multiline", "1234", LayoutStyle::default())
        .unwrap();
    assert!(
        ui.set_editor_mask(multiline, Some(MaskSettings::new("0000").unwrap()))
            .is_err()
    );
    let number = ui
        .numeric_edit(
            ui.root(),
            "Number",
            None,
            NumericEditSettings::default(),
            LayoutStyle::default(),
        )
        .unwrap();
    assert!(
        ui.set_editor_mask(number.editor, Some(MaskSettings::new("0000").unwrap()))
            .is_err()
    );
    ui.remove(parts.container).unwrap();
    assert!(ui.editor_mask_settings(parts.editor).is_none());
}

#[test]
fn mask_settings_respect_business_validation_and_active_composition_atomically() {
    let (mut ui, id) = setup("00-00", "1234");
    let settings = MaskSettings::new("(00)00").unwrap();
    ui.set_editor_validator(
        id,
        Some(EditorValidator::new(|value| {
            if value.starts_with('(') {
                Err("No parentheses".into())
            } else {
                Ok(())
            }
        })),
    )
    .unwrap();
    let before = ui.document(id).unwrap().clone();
    assert!(ui.set_editor_mask(id, Some(settings)).is_err());
    assert_eq!(ui.document(id).unwrap().text(), before.text());
    assert_eq!(ui.document(id).unwrap().selection(), before.selection());
    assert_eq!(
        ui.editor_mask_settings(id).unwrap().format("1234").unwrap(),
        "12-34"
    );
    ui.ime_preedit("5", None).unwrap();
    assert!(ui.set_editor_mask(id, None).is_err());
    assert!(ui.document(id).unwrap().composition().is_some());
}

#[test]
fn mask_async_commit_cancel_and_newer_draft_preserve_accepted_raw_value() {
    use std::sync::{Arc, Mutex, mpsc};
    use std::time::Duration;
    for cancel in [false, true] {
        let (mut ui, id) = setup("00-00", "1234");
        let (data, mut bindings) = bind(&mut ui, id, BindingUpdateTrigger::LostFocus);
        let (release, gate) = mpsc::channel();
        let gate = Arc::new(Mutex::new(gate));
        let (wake, done) = mpsc::channel();
        ui.set_editor_validation_waker(Some(Arc::new(move || {
            let _ = wake.send(());
        })));
        ui.set_async_editor_validator(
            id,
            Some(crate::AsyncEditorValidator::new(move |request| {
                assert_eq!(request.draft, "56-78");
                gate.lock().unwrap().recv().unwrap();
                Ok(())
            })),
        )
        .unwrap();
        ui.set_text(id, "5_-__").unwrap();
        assert!(matches!(
            ui.commit_edit(id).unwrap(),
            CommitOutcome::Invalid(_)
        ));
        ui.set_text(id, "56-78").unwrap();
        assert_eq!(ui.commit_edit(id).unwrap(), CommitOutcome::Pending);
        bindings.sync(&mut ui);
        assert_eq!(data.borrow().get("Raw").unwrap().as_str(), Some("1234"));
        if cancel {
            ui.cancel_edit(id).unwrap();
        }
        release.send(()).unwrap();
        done.recv_timeout(Duration::from_secs(3)).unwrap();
        ui.poll_editor_validation().unwrap();
        bindings.sync(&mut ui);
        let expected = if cancel { "1234" } else { "5678" };
        assert_eq!(ui.masked_value(id).as_deref(), Some(expected));
        assert_eq!(data.borrow().get("Raw").unwrap().as_str(), Some(expected));
    }
}

#[test]
fn mask_focus_loss_commits_only_complete_values_and_does_not_trap_focus() {
    let (mut ui, id) = setup("00-00", "1234");
    let other = ui
        .edit_box(ui.root(), "Other", "", LayoutStyle::default())
        .unwrap();
    ui.update(Size::new(640.0, 480.0)).unwrap();
    ui.set_text(id, "5_-__").unwrap();
    ui.perform_action(other, SemanticAction::Focus, None)
        .unwrap();
    assert_eq!(ui.runtime.focused(), Some(other));
    assert_eq!(ui.masked_value(id).as_deref(), Some("1234"));
    assert_eq!(ui.document(id).unwrap().text(), "5_-__");
    ui.perform_action(id, SemanticAction::Focus, None).unwrap();
    ui.set_text(id, "56-78").unwrap();
    ui.perform_action(other, SemanticAction::Focus, None)
        .unwrap();
    assert_eq!(ui.masked_value(id).as_deref(), Some("5678"));
}

#[test]
fn mask_formatter_preserves_every_accepted_storage_for_small_exhaustive_alphabet() {
    let alphabet = ["1", "2", "A", "é", "👩‍💻"];
    for pattern in ["9900", "llLL", "9L9L", "Xx0", "L-xX", "0\\09"] {
        let settings = MaskSettings::new(pattern).unwrap();
        let mut accepted = 0;
        for length in 0..=4 {
            for mut value in 0..5usize.pow(length) {
                let raw = (0..length)
                    .map(|_| {
                        let next = alphabet[value % 5];
                        value /= 5;
                        next
                    })
                    .collect::<String>();
                if let Ok(display) = settings.format(&raw) {
                    let parsed = settings.parse_display(&display).unwrap();
                    assert!(parsed.complete, "{pattern}: {raw}");
                    assert_eq!(parsed.storage, raw, "{pattern}: {display}");
                    accepted += 1;
                }
            }
        }
        assert!(accepted > 1, "{pattern}");
    }
    let settings = MaskSettings::new("X").unwrap();
    assert!(settings.format("_\u{301}").is_err());
}

#[test]
fn mask_full_raw_paste_uses_the_same_optional_assignment_as_storage_loading() {
    for initial in ["", "1234"] {
        let (mut ui, id) = setup("9900", initial);
        select(&mut ui, id, 0, 4);
        ui.text_input("12").unwrap();
        assert_eq!(ui.document(id).unwrap().text(), "__12");
        assert!(matches!(
            ui.commit_edit(id).unwrap(),
            CommitOutcome::Committed | CommitOutcome::Unchanged
        ));
        assert_eq!(ui.masked_value(id).as_deref(), Some("12"));
    }
    let (mut ui, id) = setup("9900", "");
    select(&mut ui, id, 0, 0);
    ui.text_input("12").unwrap();
    assert_eq!(ui.document(id).unwrap().text(), "__12");
    let (mut ui, id) = setup("99LL", "");
    select(&mut ui, id, 0, 4);
    ui.text_input("AB").unwrap();
    assert_eq!(ui.document(id).unwrap().text(), "__AB");
}

#[test]
fn mask_rejects_raw_grapheme_merging_between_slots_before_draft_or_acceptance() {
    for (first, second) in [("🇷", "🇺"), ("ᄀ", "ᅡ")] {
        let (mut ui, id) = setup("X-X", "");
        select(&mut ui, id, 0, 0);
        ui.text_input(first).unwrap();
        let before = ui.document(id).unwrap().clone();
        ui.text_input(second).unwrap();
        assert_eq!(ui.document(id).unwrap().text(), before.text());
        assert_eq!(ui.document(id).unwrap().selection(), before.selection());
        assert_eq!(ui.document(id).unwrap().undo_text(), before.undo_text());
        assert!(ui.set_text(id, &format!("{first}-{second}")).is_err());
        assert!(matches!(
            ui.set_editor_value(id, &format!("{first}-{second}"))
                .unwrap(),
            CommitOutcome::Invalid(_)
        ));
        assert_eq!(ui.masked_value(id).as_deref(), Some(""));
    }
    let settings = MaskSettings::new("X-X").unwrap();
    assert!(settings.parse_display("🇷-🇺").is_err());
    assert!(settings.parse_display("ᄀ-ᅡ").is_err());
    assert_eq!(settings.parse_display("🇷🇺-🇬🇧").unwrap().storage, "🇷🇺🇬🇧");
}

#[test]
fn mask_display_and_incremental_input_enforce_the_same_raw_byte_budget() {
    let settings = MaskSettings::new("X").unwrap();
    let oversized = "A".to_owned() + &"\u{301}".repeat(MAX_MASK_VALUE_BYTES / 2);
    assert!(settings.format(&oversized).is_err());
    assert!(settings.parse_display(&oversized).is_err());
    let (mut ui, id) = setup(&"X".repeat(256), "");
    let first = ("A".to_owned() + &"\u{1ab0}".repeat(31)).repeat(128);
    select(&mut ui, id, 0, 0);
    ui.text_input(&first).unwrap();
    assert!(ui.document(id).unwrap().text().starts_with(&first));
    let before = ui.document(id).unwrap().clone();
    ui.text_input(&first).unwrap();
    assert_eq!(ui.document(id).unwrap().text(), before.text());
    assert_eq!(ui.document(id).unwrap().selection(), before.selection());
    assert_eq!(ui.document(id).unwrap().undo_text(), before.undo_text());
}

#[test]
fn mask_complete_display_copy_accepts_optional_prompts_without_treating_them_as_data() {
    let (mut ui, id) = setup("9900", "1234");
    select(&mut ui, id, 0, 4);
    ui.text_input("__12").unwrap();
    assert_eq!(ui.document(id).unwrap().text(), "__12");
    assert_eq!(ui.commit_edit(id).unwrap(), CommitOutcome::Committed);
    assert_eq!(ui.masked_value(id).as_deref(), Some("12"));
    let (mut ui, id) = setup("X", "A");
    select(&mut ui, id, 0, 1);
    ui.text_input("_").unwrap();
    assert_eq!(ui.document(id).unwrap().text(), "A");
    let (mut ui, id) = setup("XX", "AB");
    select(&mut ui, id, 0, 2);
    ui.text_input("A_").unwrap();
    assert_eq!(ui.document(id).unwrap().text(), "AB");
}

#[test]
fn mask_shaping_limit_rejects_pathological_grapheme_without_panicking_or_changing_history() {
    let (mut ui, id) = setup("X", "A");
    select(&mut ui, id, 0, 1);
    let before = ui.document(id).unwrap().clone();
    ui.text_input(&("A".to_owned() + &"\u{301}".repeat(4500)))
        .unwrap();
    assert_eq!(ui.document(id).unwrap().text(), before.text());
    assert_eq!(ui.document(id).unwrap().selection(), before.selection());
    assert_eq!(ui.document(id).unwrap().undo_text(), before.undo_text());
}

#[test]
fn mask_pattern_and_segments_have_reversible_storage_and_required_slots() {
    let settings = MaskSettings::new("(000) 000-00-00").unwrap();
    assert_eq!(settings.pattern.slot_count(), 10);
    assert_eq!(settings.format("1234567890").unwrap(), "(123) 456-78-90");
    let parsed = settings.parse_display("(123) 456-78-90").unwrap();
    assert_eq!(parsed.storage, "1234567890");
    assert!(parsed.complete);
    let partial = settings.parse_display("(123) 4__-__-__").unwrap();
    assert_eq!(partial.storage, "1234");
    assert!(!partial.complete);
    assert!(settings.format("1234").is_err());
    assert!(settings.parse_display("1234567890").is_err());
    assert_eq!(settings.format("").unwrap(), "(___) ___-__-__");
    assert!(settings.parse_display("(___) ___-__-__").unwrap().complete);
    let mut required = settings.clone();
    required.allow_empty = false;
    assert!(required.format("").is_err());
    assert!(!required.parse_display("(___) ___-__-__").unwrap().complete);
    let structured = MaskSettings {
        pattern: MaskPattern::from_segments(vec![
            MaskSegment::Literal("(".into()),
            MaskSegment::Field {
                class: MaskClass::Digit,
                minimum: 3,
                maximum: 3,
            },
            MaskSegment::Literal(") ".into()),
            MaskSegment::Field {
                class: MaskClass::Digit,
                minimum: 3,
                maximum: 3,
            },
            MaskSegment::Literal("-".into()),
            MaskSegment::Field {
                class: MaskClass::Digit,
                minimum: 2,
                maximum: 2,
            },
            MaskSegment::Literal("-".into()),
            MaskSegment::Field {
                class: MaskClass::Digit,
                minimum: 2,
                maximum: 2,
            },
        ])
        .unwrap(),
        ..settings
    };
    assert_eq!(structured.format("1234567890").unwrap(), "(123) 456-78-90");
}

#[test]
fn optional_masks_use_deterministic_bounded_assignment_without_losing_chars() {
    let settings = MaskSettings::new("9900-llLL").unwrap();
    for value in ["12AB", "123AB", "1234AB", "12ABC", "12ABCD", "1234ABCD"] {
        let display = settings.format(value).unwrap();
        let parsed = settings.parse_display(&display).unwrap();
        assert_eq!(parsed.storage, value);
        assert!(parsed.complete);
    }
    assert_eq!(settings.format("12AB").unwrap(), "__12-__AB");
    assert!(settings.format("1AB").is_err());
    assert!(settings.format("12345AB").is_err());
    let boundary = MaskSettings::new(&("9".repeat(128) + &"0".repeat(128))).unwrap();
    let value = "1".repeat(128);
    let display = boundary.format(&value).unwrap();
    assert_eq!(display, "_".repeat(128) + &value);
    assert_eq!(boundary.parse_display(&display).unwrap().storage, value);
}

#[test]
fn mask_unicode_graphemes_and_reserved_placeholder_are_unambiguous() {
    let mut settings = MaskSettings::new("L-X-X").unwrap();
    let storage = "e\u{301}👩‍💻🇷🇺";
    let display = settings.format(storage).unwrap();
    assert_eq!(display, "e\u{301}-👩‍💻-🇷🇺");
    assert_eq!(settings.parse_display(&display).unwrap().storage, storage);
    assert!(settings.format("e_🙂").is_err());
    settings.placeholder = '•';
    assert!(settings.format("e•🙂").is_err());
    assert_eq!(settings.format(storage).unwrap(), display);
    settings.placeholder = '\u{301}';
    assert!(settings.validate().is_err());
    settings.placeholder = '🇷';
    assert!(settings.validate().is_err());
    let settings = MaskSettings::new("\\AX").unwrap();
    assert!(settings.format("\u{301}").is_err());
    assert!(MaskSettings::new("X\nX").is_err());
    assert!(MaskSettings::new("XX").unwrap().format("a\nb").is_err());
}

#[test]
fn mask_culture_separators_change_display_without_changing_storage() {
    let mut settings = MaskSettings::new("00/00/0000 00:00 0,000.00").unwrap();
    let storage = "171020261530123456";
    for (culture, display) in [
        (UiCulture::Russian, "17.10.2026 15:30 1\u{a0}234,56"),
        (UiCulture::English, "17/10/2026 15:30 1,234.56"),
        (UiCulture::Arabic, "17/10/2026 15:30 1٬234٫56"),
    ] {
        settings.culture = culture;
        assert_eq!(settings.format(storage).unwrap(), display);
        let parsed = settings.parse_display(display).unwrap();
        assert_eq!(parsed.storage, storage);
        assert!(parsed.complete);
    }
    let literal = MaskSettings::new("00\\.00").unwrap();
    assert_eq!(literal.format("1234").unwrap(), "12.34");
}

#[test]
fn mask_limits_and_sample_values_are_checked_before_formatting() {
    assert!(MaskPattern::compile("---").is_err());
    assert!(MaskPattern::compile("0\\").is_err());
    assert!(MaskPattern::compile(&"0".repeat(257)).is_err());
    assert!(MaskPattern::compile(&("-".repeat(4096) + "0")).is_err());
    assert!(
        MaskPattern::from_segments(vec![MaskSegment::Field {
            class: MaskClass::Digit,
            minimum: 2,
            maximum: 1
        }])
        .is_err()
    );
    for pattern in ["000", "LLL", "XXX", "999", "lll", "xxx"] {
        for placeholder in ['_', '0', 'A'] {
            let mut settings = MaskSettings::new(pattern).unwrap();
            settings.placeholder = placeholder;
            settings.allow_empty = false;
            let sample = settings.sample_value().unwrap();
            assert!(!sample.is_empty());
            settings.validate_storage(&sample).unwrap();
            assert!(
                settings
                    .parse_display(&settings.format(&sample).unwrap())
                    .unwrap()
                    .complete
            );
        }
    }
}
