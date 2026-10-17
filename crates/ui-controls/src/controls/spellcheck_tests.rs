use super::*;
use rust_desktop_ui_text::spellcheck::{LexiconMetadata, LexiconProvider, SpellProvider};
fn provider() -> LexiconProvider {
    LexiconProvider::new(
        LexiconMetadata {
            id: "test".into(),
            language: "ru-RU".into(),
            source: "Handwritten project tests".into(),
            license: "Apache-2.0".into(),
        },
        ["привет".to_owned(), "мир".to_owned()],
    )
    .unwrap()
}
fn fixture() -> (Controls, WidgetId) {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let editor = ui
        .text_box(
            ui.root(),
            "Spelling",
            "привт мир",
            LayoutStyle {
                width: Length::Px(360.0),
                height: Length::Px(80.0),
                ..Default::default()
            },
        )
        .unwrap();
    ui.set_spellcheck(editor, Some(Default::default())).unwrap();
    ui.update(Size::new(600.0, 400.0)).unwrap();
    (ui, editor)
}
fn check(ui: &mut Controls, id: WidgetId) {
    let request = ui.request_spellcheck(id).unwrap();
    let result = provider().check(&request);
    assert_eq!(
        ui.complete_spellcheck(id, &request, result).unwrap(),
        SpellCompletion::Accepted
    );
}
#[test]
fn spelling_replacement_changes_only_draft_and_is_one_undo() {
    let (mut ui, id) = fixture();
    check(&mut ui, id);
    let accepted = ui.accepted_value(id).unwrap().to_owned();
    ui.apply_spelling_suggestion(id, 0, 0).unwrap();
    assert_eq!(ui.document(id).unwrap().text(), "привет мир");
    assert_eq!(ui.accepted_value(id), Some(accepted.as_str()));
    assert_eq!(ui.drain_text_changes().len(), 1);
    assert!(ui.drain_edit_events().is_empty());
    assert!(ui.spellcheck_issues(id).is_none());
    let Kind::TextBox(document) = &mut ui.controls.get_mut(&id).unwrap().kind else {
        unreachable!()
    };
    assert!(document.undo());
    assert_eq!(document.text(), "привт мир");
}
#[test]
fn direct_document_aba_and_replacement_copies_cannot_accept_old_completion() {
    let (mut ui, id) = fixture();
    let request = ui.request_spellcheck(id).unwrap();
    let Kind::TextBox(document) = &mut ui.controls.get_mut(&id).unwrap().kind else {
        unreachable!()
    };
    let old = document.clone();
    document.set_text("другой");
    document.set_text("привт мир");
    let invalid = || {
        Ok(vec![SpellIssue {
            range: std::ops::Range {
                start: usize::MAX,
                end: 0,
            },
            word: "bad".into(),
            suggestions: vec![],
        }])
    };
    assert_eq!(
        ui.complete_spellcheck(id, &request, invalid()).unwrap(),
        SpellCompletion::Stale
    );
    let Kind::TextBox(document) = &mut ui.controls.get_mut(&id).unwrap().kind else {
        unreachable!()
    };
    *document = old;
    assert_eq!(
        ui.complete_spellcheck(id, &request, invalid()).unwrap(),
        SpellCompletion::Stale
    );
    check(&mut ui, id);
}
#[test]
fn replacement_limits_and_shape_errors_preserve_draft_selection_history_and_accepted() {
    let (mut ui, id) = fixture();
    ui.set_max_length(id, Some(9)).unwrap();
    check(&mut ui, id);
    let selection = ui.document(id).unwrap().selection();
    let version = ui.document(id).unwrap().version();
    assert!(ui.apply_spelling_suggestion(id, 0, 0).is_err());
    assert_eq!(ui.document(id).unwrap().version(), version);
    assert_eq!(ui.document(id).unwrap().selection(), selection);
    assert_eq!(ui.accepted_value(id), Some("привт мир"));
    assert!(!ui.document(id).unwrap().can_undo());
    ui.set_max_length(id, None).unwrap();
    let request = ui.request_spellcheck(id).unwrap();
    let bad = format!("a{}", "\u{301}".repeat(33));
    ui.complete_spellcheck(
        id,
        &request,
        Ok(vec![SpellIssue {
            range: 0..10,
            word: "привт".into(),
            suggestions: vec![bad],
        }]),
    )
    .unwrap();
    assert!(ui.apply_spelling_suggestion(id, 0, 0).is_err());
    assert_eq!(ui.document(id).unwrap().version(), version);
}
#[test]
fn readonly_disabled_composition_and_removed_owner_discard_payloads() {
    let (mut ui, id) = fixture();
    let request = ui.request_spellcheck(id).unwrap();
    ui.set_read_only(id, true).unwrap();
    assert_eq!(
        ui.complete_spellcheck(id, &request, Err(SpellError("x".repeat(5000))))
            .unwrap(),
        SpellCompletion::Stale
    );
    assert!(request.is_cancelled());
    ui.set_read_only(id, false).unwrap();
    let request = ui.request_spellcheck(id).unwrap();
    ui.set_enabled(id, false).unwrap();
    assert_eq!(
        ui.complete_spellcheck(id, &request, Ok(vec![])).unwrap(),
        SpellCompletion::Stale
    );
    ui.set_enabled(id, true).unwrap();
    let request = ui.request_spellcheck(id).unwrap();
    ui.runtime.apply(Mutation::Focus(Some(id))).unwrap();
    ui.ime_preedit("композиция", None).unwrap();
    assert_eq!(
        ui.complete_spellcheck(id, &request, Ok(vec![])).unwrap(),
        SpellCompletion::Stale
    );
    assert!(ui.request_spellcheck(id).is_err());
    ui.cancel_preedit();
    let request = ui.request_spellcheck(id).unwrap();
    ui.remove(id).unwrap();
    assert!(request.is_cancelled());
    assert_eq!(
        ui.complete_spellcheck(id, &request, Ok(vec![])).unwrap(),
        SpellCompletion::Stale
    );
}
#[test]
fn memo_retains_normal_commit_cancel_semantics_and_ignores_are_explicit() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let memo = ui
        .memo_edit(
            ui.root(),
            "Memo",
            "привт мир",
            Default::default(),
            LayoutStyle {
                width: Length::Px(200.0),
                height: Length::Px(40.0),
                ..Default::default()
            },
        )
        .unwrap();
    ui.set_spellcheck(memo.container, Some(Default::default()))
        .unwrap();
    ui.update(Size::new(700.0, 500.0)).unwrap();
    ui.open_memo(memo).unwrap();
    ui.update(Size::new(700.0, 500.0)).unwrap();
    check(&mut ui, memo.container);
    ui.apply_spelling_suggestion(memo.container, 0, 0).unwrap();
    assert_eq!(ui.document(memo.editor).unwrap().text(), "привет мир");
    assert_eq!(ui.accepted_value(memo.editor), Some("привт мир"));
    ui.cancel_memo(memo).unwrap();
    assert_eq!(ui.document(memo.editor).unwrap().text(), "привт мир");
    ui.open_memo(memo).unwrap();
    ui.update(Size::new(700.0, 500.0)).unwrap();
    check(&mut ui, memo.container);
    ui.ignore_spelling_once(memo.container, 0).unwrap();
    assert!(ui.spellcheck_issues(memo.container).unwrap().is_empty());
    ui.add_spelling_word(memo.container, "привт").unwrap();
    check(&mut ui, memo.container);
    assert!(ui.spellcheck_issues(memo.container).unwrap().is_empty());
}

#[test]
fn correction_uses_ordinary_binding_trigger_without_accepting_business_value() {
    use crate::{
        BindingContext, BindingProperty, BindingUpdateTrigger, ControlBinding, ControlBindingMode,
        ControlBindings, DataValue,
    };
    use std::{cell::RefCell, rc::Rc};
    for trigger in [
        BindingUpdateTrigger::LostFocus,
        BindingUpdateTrigger::PropertyChanged,
    ] {
        let (mut ui, id) = fixture();
        let model = Rc::new(RefCell::new(
            BindingContext::new(DataValue::object([("Text", "привт мир".into())])).unwrap(),
        ));
        let mut bindings = ControlBindings::new(model.clone());
        let mut binding = ControlBinding::new(id, BindingProperty::Text, "Text").unwrap();
        binding.mode = ControlBindingMode::TwoWay;
        binding.trigger = trigger;
        bindings.add(binding).unwrap();
        bindings.initialize(&mut ui).unwrap();
        check(&mut ui, id);
        ui.apply_spelling_suggestion(id, 0, 0).unwrap();
        bindings.sync(&mut ui);
        let expected = if trigger == BindingUpdateTrigger::PropertyChanged {
            "привет мир"
        } else {
            "привт мир"
        };
        assert_eq!(model.borrow().get("Text").unwrap().as_str(), Some(expected));
        assert_eq!(ui.accepted_value(id), Some("привт мир"));
        ui.commit_edit(id).unwrap();
        bindings.sync(&mut ui);
        assert_eq!(
            model.borrow().get("Text").unwrap().as_str(),
            Some("привет мир")
        );
    }
}
