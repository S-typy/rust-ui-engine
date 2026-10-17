use super::*;
use crate::{EditError, TextLimits};
fn provider() -> LexiconProvider {
    LexiconProvider::new(
        LexiconMetadata {
            id: "handwritten-tests".into(),
            language: "ru-RU".into(),
            source: "Handwritten project test words".into(),
            license: "Apache-2.0".into(),
        },
        ["привет", "мир", "текст", "кот", "кит", "котик"].map(String::from),
    )
    .unwrap()
}
#[test]
fn version_tracks_every_mutation_and_never_resurrects_after_undo_clone() {
    let mut doc = TextDocument::new("кот");
    let first = doc.version();
    doc.set_selection(0, 0).unwrap();
    assert_eq!(doc.version(), first);
    doc.set_preedit("preedit", None).unwrap();
    assert_eq!(doc.version(), first);
    doc.cancel_preedit();
    doc.set_text("мир");
    assert_ne!(doc.version(), first);
    assert!(doc.undo());
    assert_eq!(doc.text(), "кот");
    assert_ne!(doc.version(), first);
    let undo = doc.version();
    assert!(doc.redo());
    assert_ne!(doc.version(), undo);
    assert_ne!(doc.clone().version(), doc.version());
    let copy = doc.clone();
    doc.set_text("текст");
    doc = copy;
    assert_ne!(doc.version(), first);
}
#[test]
fn replacement_is_one_undo_preserves_old_selection_and_rejection_is_atomic() {
    let mut doc = TextDocument::new("👩‍💻 привт мир");
    doc.set_selection(0, 0).unwrap();
    let before = doc.version();
    let selected = doc.selection();
    assert_eq!(
        doc.try_replace_range(1..3, "x", TextLimits::default()),
        Err(EditError::InvalidSelection)
    );
    assert_eq!(doc.version(), before);
    let start = "👩‍💻 ".len();
    doc.try_replace_range(
        start..start + "привт".len(),
        "привет",
        TextLimits::default(),
    )
    .unwrap();
    assert_eq!(doc.text(), "👩‍💻 привет мир");
    assert!(doc.undo());
    assert_eq!(doc.text(), "👩‍💻 привт мир");
    assert_eq!(doc.selection(), selected);
    doc.set_preedit("pending", None).unwrap();
    let composition = doc.composition().cloned();
    assert!(
        doc.try_replace_range(
            0..doc.text().len(),
            "too long",
            TextLimits {
                max_bytes: 1,
                max_graphemes: None
            }
        )
        .is_err()
    );
    assert_eq!(doc.composition(), composition.as_ref());
}
#[test]
fn cyrillic_boundaries_suggestions_and_exact_revision_replacement() {
    let mut doc = TextDocument::new("👩‍💻 Привт, мир! 123");
    let mut session = SpellSession::new(Default::default()).unwrap();
    let request = session.request(&doc).unwrap();
    let result = provider().check(&request).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(&doc.text()[result[0].range.clone()], "Привт");
    assert_eq!(result[0].suggestions, ["привет"]);
    assert_eq!(
        session.complete(&doc, &request, Ok(result)).unwrap(),
        SpellCompletion::Accepted
    );
    let replacement = session.suggestion(&doc, 0, 0).unwrap();
    assert_eq!(replacement.candidate(&doc).unwrap(), "👩‍💻 привет, мир! 123");
    doc.set_text("different");
    assert!(replacement.candidate(&doc).is_err());
}
#[test]
fn stale_foreign_cancelled_and_aba_payloads_are_never_inspected() {
    let mut doc = TextDocument::new("привт");
    let mut session = SpellSession::new(Default::default()).unwrap();
    let old = session.request(&doc).unwrap();
    doc.set_text("мир");
    doc.set_text("привт");
    let malformed = || {
        Ok(vec![SpellIssue {
            range: std::ops::Range {
                start: usize::MAX,
                end: 0,
            },
            word: String::new(),
            suggestions: vec![],
        }])
    };
    assert_eq!(
        session.complete(&doc, &old, malformed()).unwrap(),
        SpellCompletion::Stale
    );
    let current = session.request(&doc).unwrap();
    assert!(old.is_cancelled());
    let mut foreign = SpellSession::new(Default::default()).unwrap();
    let other = foreign.request(&doc).unwrap();
    assert_eq!(
        session.complete(&doc, &other, malformed()).unwrap(),
        SpellCompletion::Stale
    );
    session.cancel();
    assert_eq!(
        session.complete(&doc, &current, malformed()).unwrap(),
        SpellCompletion::Stale
    );
}
#[test]
fn malformed_completion_keeps_pending_for_correct_retry_and_bounds_suggestions() {
    let doc = TextDocument::new("привт");
    let mut session = SpellSession::new(Default::default()).unwrap();
    let request = session.request(&doc).unwrap();
    for issue in [
        SpellIssue {
            range: 1..4,
            word: "ри".into(),
            suggestions: vec![],
        },
        SpellIssue {
            range: 0..10,
            word: "привт".into(),
            suggestions: vec!["two words".into()],
        },
    ] {
        assert!(session.complete(&doc, &request, Ok(vec![issue])).is_err());
        assert!(session.is_pending());
    }
    session
        .complete(&doc, &request, provider().check(&request))
        .unwrap();
    assert_eq!(session.issues(&doc).unwrap().len(), 1);
}
#[test]
fn ignores_are_versioned_language_scoped_and_custom_dictionary_is_explicit() {
    let mut doc = TextDocument::new("привт привт");
    let mut session = SpellSession::new(Default::default()).unwrap();
    let req = session.request(&doc).unwrap();
    session
        .complete(&doc, &req, provider().check(&req))
        .unwrap();
    session.ignore_once(&doc, 0).unwrap();
    assert_eq!(session.issues(&doc).unwrap().len(), 1);
    let req = session.request(&doc).unwrap();
    assert_eq!(provider().check(&req).unwrap().len(), 1);
    doc.set_text("привт");
    let req = session.request(&doc).unwrap();
    assert_eq!(provider().check(&req).unwrap().len(), 1);
    session.ignore_all("привт").unwrap();
    let req = session.request(&doc).unwrap();
    assert!(provider().check(&req).unwrap().is_empty());
    doc.set_text("прект");
    session.add_custom_word("прект").unwrap();
    let req = session.request(&doc).unwrap();
    assert!(provider().check(&req).unwrap().is_empty());
    session
        .set_settings(SpellSettings {
            language: "en-US".into(),
            ..Default::default()
        })
        .unwrap();
    let req = session.request(&doc).unwrap();
    assert!(provider().check(&req).is_err());
}
#[test]
fn settings_metadata_words_and_work_are_bounded() {
    assert!(
        SpellSession::new(SpellSettings {
            language: "invalid tag".into(),
            ..Default::default()
        })
        .is_err()
    );
    assert!(spelling_words(&"a ".repeat(4097)).is_err());
    assert!(spelling_words(&"a".repeat(513)).is_err());
    let mut work = MAX_SPELL_WORK;
    assert!(super::lexicon::distance(&["a"], &["b"], &mut work).is_err());
    let words = spelling_words("e\u{301} мир 👩‍💻").unwrap();
    assert_eq!(words[0].range, 0..3);
}
#[test]
fn comparison_uppercase_and_composition_policies_are_reproducible() {
    let doc = TextDocument::new("ПРИВЕТ ПРИВТ");
    let mut session = SpellSession::new(SpellSettings {
        ignore_uppercase: true,
        ..Default::default()
    })
    .unwrap();
    let req = session.request(&doc).unwrap();
    assert!(provider().check(&req).unwrap().is_empty());
    session
        .set_settings(SpellSettings {
            comparison: SpellComparison::Ordinal,
            ..Default::default()
        })
        .unwrap();
    let req = session.request(&doc).unwrap();
    assert_eq!(provider().check(&req).unwrap().len(), 2);
    let mut doc = doc;
    doc.set_preedit("x", None).unwrap();
    assert!(session.request(&doc).is_err());
    assert_eq!(
        session.complete(&doc, &req, Ok(vec![])).unwrap(),
        SpellCompletion::Stale
    );
}

#[test]
fn mismatch_scans_and_duplicate_input_are_bounded_not_only_distance_cells() {
    let metadata = provider().metadata().clone();
    assert!(
        LexiconProvider::new(
            metadata.clone(),
            std::iter::repeat_n("кот".to_owned(), MAX_LEXICON_WORDS + 1)
        )
        .is_err()
    );
    let lexicon = LexiconProvider::new(
        metadata,
        (0..256).map(|i| format!("{}word{i}", "a".repeat(50))),
    )
    .unwrap();
    let doc = TextDocument::new("x ".repeat(256));
    let mut session = SpellSession::new(Default::default()).unwrap();
    let request = session.request(&doc).unwrap();
    assert!(
        lexicon
            .check(&request)
            .unwrap_err()
            .0
            .contains("work budget")
    );
}

#[test]
fn direct_document_sync_cancels_worker_and_policy_comparison_has_separate_keys() {
    let mut doc = TextDocument::new("ПРИВТ");
    let mut session = SpellSession::new(Default::default()).unwrap();
    session.ignore_all("ПРИВТ").unwrap();
    let request = session.request(&doc).unwrap();
    assert!(provider().check(&request).unwrap().is_empty());
    session
        .set_settings(SpellSettings {
            comparison: SpellComparison::Ordinal,
            ..Default::default()
        })
        .unwrap();
    let request = session.request(&doc).unwrap();
    assert_eq!(provider().check(&request).unwrap().len(), 1);
    doc.set_text("changed");
    session.synchronize(&doc);
    assert!(request.is_cancelled());
}
