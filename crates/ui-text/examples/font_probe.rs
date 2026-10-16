//! Reports actual system-font coverage without creating a window or GPU device.
use rust_desktop_ui_core::{Color, Point, Rect, TextRun};
use rust_desktop_ui_text::TextEngine;
use unicode_segmentation::UnicodeSegmentation;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut engine = TextEngine::new();
    let strict = std::env::args().any(|arg| arg == "--strict");
    let mut failures = Vec::new();
    for (script, text) in [
        ("Latin", "Hello world"),
        ("Cyrillic", "Привет мир"),
        ("Combining", "Cafe\u{301} A\u{30a} e\u{301}\u{323}"),
        ("Arabic", "العَرَبِيَّة"),
        ("Hebrew", "שלום עולם"),
        ("Mixed bidi", "Invoice אבג 123 العربية 42 руб."),
        ("Devanagari", "नमस्ते क्षि क्‍ष"),
        ("Thai", "สวัสดี กำ"),
        ("CJK", "中文 日本語"),
        ("Emoji", "👩‍💻 👍🏽 🇷🇺 😀"),
    ] {
        for scale in [1.0, 1.25, 1.5, 2.0] {
            let run = TextRun::new(
                text,
                Rect::new(17.25, 13.75, 800.0, 120.0),
                Color::rgb(20, 20, 20),
                24.0,
            );
            let prepared = engine.prepare(&run, scale)?;
            println!(
                "{script} scale={scale}: text={text:?} shaped={} bitmaps={} color={} missing={} physical_size={:.3}x{:.3} rtl={}",
                prepared.shaped_glyphs,
                prepared.rasterized_glyphs,
                prepared.color_glyphs,
                prepared.missing_glyphs,
                prepared.width,
                prepared.height,
                prepared.is_rtl()
            );
            for font in prepared.font_runs() {
                println!(
                    "  font={:?} range={:?} physical_size={} rtl={} glyphs={}",
                    font.font_name, font.text_range, font.font_size, font.is_rtl, font.glyph_count
                );
            }
            for (index, grapheme) in text.grapheme_indices(true) {
                let caret = prepared.caret_rect(index);
                let hit =
                    prepared.hit_test_caret(Point::new(caret.x, caret.y + caret.height / 2.0));
                let hit_rect = prepared.caret_geometry(hit);
                if !caret.is_valid() || !hit_rect.is_valid() || (caret.x - hit_rect.x).abs() > 1.0 {
                    failures.push(format!(
                        "{script} scale={scale}: caret/hit geometry differs at {index}"
                    ));
                }
                // Check ink separately from .notdef: an unsupported color font
                // can shape a valid glyph while its rasterizer produces no bitmap.
                if grapheme
                    .chars()
                    .any(|c| !c.is_whitespace() && !c.is_control())
                {
                    let mut isolated = run.clone();
                    isolated.text = grapheme.into();
                    let glyph = engine.prepare(&isolated, scale)?;
                    if glyph.missing_glyphs != 0
                        || !glyph
                            .glyphs
                            .iter()
                            .any(|g| g.bitmap.rgba.chunks_exact(4).any(|p| p[3] != 0))
                    {
                        failures.push(format!(
                            "{script} scale={scale}: no supported ink for {grapheme:?}"
                        ));
                    }
                }
            }
            if prepared.missing_glyphs != 0 {
                failures.push(format!(
                    "{script} scale={scale}: {} missing glyphs",
                    prepared.missing_glyphs
                ));
            }
        }
    }
    println!("cache: {:?}", engine.stats());
    for failure in &failures {
        println!("UNVERIFIED: {failure}");
    }
    if strict && !failures.is_empty() {
        return Err(format!(
            "{} font/geometry checks failed on this machine",
            failures.len()
        )
        .into());
    }
    println!(
        "matrix: 40 cases, {} failures; strict={strict}",
        failures.len()
    );
    Ok(())
}
