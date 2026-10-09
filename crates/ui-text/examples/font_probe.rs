//! Reports actual system-font coverage without creating a window or GPU device.
use rust_desktop_ui_core::{Color, Rect, TextRun};
use rust_desktop_ui_text::TextEngine;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut engine = TextEngine::new();
    for (script, text) in [
        ("Latin", "Hello world"),
        ("Cyrillic", "Привет мир"),
        ("Arabic", "العربية"),
        ("Hebrew", "שלום עולם"),
        ("CJK", "中文 日本語"),
        ("Emoji", "👩‍💻 🇷🇺"),
    ] {
        let run = TextRun::new(
            text,
            Rect::new(0.0, 0.0, 600.0, 80.0),
            Color::rgb(20, 20, 20),
            24.0,
        );
        let prepared = engine.prepare(&run, 1.5)?;
        println!(
            "{script}: glyph_bitmaps={} missing_glyphs={} physical_size={:.1}x{:.1} rtl={}",
            prepared.glyphs.len(),
            prepared.missing_glyphs,
            prepared.width,
            prepared.height,
            prepared.is_rtl()
        );
    }
    println!("cache: {:?}", engine.stats());
    Ok(())
}
