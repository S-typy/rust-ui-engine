//! Explicit-path PDF interchange utility. It never opens windows or submits print jobs.
use rust_desktop_ui_controls::{pdf::PdfDocument, printing::export_pdf};
use rust_desktop_ui_core::{pages::*, *};
use rust_desktop_ui_text::TextEngine;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("sample") if args.len() == 3 => {
            let mut scene = Scene::default();
            scene.fill(Rect::new(0.0, 0.0, 400.0, 500.0), Color::rgb(255, 255, 255));
            scene.text(TextRun::new(
                "Привет, PDF! Café",
                Rect::new(24.0, 24.0, 350.0, 32.0),
                Color::rgb(0, 0, 0),
                20.0,
            ));
            scene.fill(Rect::new(30.0, 90.0, 150.0, 30.0), Color::rgb(10, 80, 150));
            let document = PrintDocument::new(
                "PDF sample",
                vec![PrintPage {
                    size: Size::new(400.0, 500.0),
                    scene,
                }],
            )?;
            std::fs::write(
                &args[2],
                export_pdf(&document, &mut TextEngine::new(), || false)?,
            )?;
        }
        Some("inspect" | "edit") if args.len() >= 3 => {
            let mut document = PdfDocument::open(&std::fs::read(&args[2])?)?;
            println!(
                "pages={} fields={} bookmarks={}",
                document.pages().len(),
                document.fields().len(),
                document.bookmarks().len()
            );
            for diagnostic in document.diagnostics() {
                println!("diagnostic: {}", diagnostic.message);
            }
            for page in document.pages() {
                for text in &page.text {
                    println!("{}", text.text);
                }
            }
            if args[1] == "edit" {
                let destination = args.get(3).ok_or("edit requires destination")?;
                if !document.fields().is_empty() {
                    document.set_field(0, "Клиент")?;
                }
                document.add_annotation(0, Rect::new(30.0, 40.0, 20.0, 20.0), "Заметка")?;
                std::fs::write(destination, document.save()?)?;
            }
        }
        _ => return Err(
            "Usage: document_output_codec sample OUT.pdf | inspect IN.pdf | edit IN.pdf OUT.pdf"
                .into(),
        ),
    }
    Ok(())
}
