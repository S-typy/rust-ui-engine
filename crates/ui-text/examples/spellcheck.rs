//! An application-owned worker with an explicitly supplied, handwritten lexicon.
use rust_desktop_ui_text::{TextDocument, TextLimits, spellcheck::*};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let provider = LexiconProvider::new(
        LexiconMetadata {
            id: "example-words".into(),
            language: "ru-RU".into(),
            source: "Handwritten words in this Apache-2.0 example".into(),
            license: "Apache-2.0".into(),
        },
        ["привет", "мир", "пример"].map(String::from),
    )?;
    let mut document = TextDocument::new("Привт, мир!");
    let mut session = SpellSession::new(Default::default())?;
    let request = session.request(&document)?;
    let work = request.clone();
    let result = std::thread::spawn(move || provider.check(&work))
        .join()
        .expect("example worker");
    assert_eq!(
        session.complete(&document, &request, result)?,
        SpellCompletion::Accepted
    );
    let issue = &session.issues(&document).unwrap()[0];
    println!(
        "UTF-8 {:?}: {} -> {:?}",
        issue.range, issue.word, issue.suggestions
    );
    let replacement = session.suggestion(&document, 0, 0)?;
    replacement.candidate(&document)?;
    document.try_replace_range(
        replacement.range(),
        replacement.text(),
        TextLimits::default(),
    )?;
    assert_eq!(document.text(), "привет, мир!");
    assert!(document.undo());
    assert_eq!(document.text(), "Привт, мир!");
    println!("One-step undo restored the draft; dictionary provenance: handwritten / Apache-2.0");
    let stale = session.request(&document)?;
    document.set_text("другая версия");
    assert_eq!(
        session.complete(&document, &stale, Ok(Vec::new()))?,
        SpellCompletion::Stale
    );
    println!("Changed-document completion rejected as stale");
    Ok(())
}
