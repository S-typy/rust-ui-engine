use super::*;
use rust_desktop_ui_text::TextEngine;

fn sample() -> RichDocument {
    let mut p = Paragraph::text("Привет 👩‍💻 world");
    p.format(
        0..12,
        RunStyle {
            bold: true,
            color: [0, 90, 180],
            ..Default::default()
        },
    )
    .unwrap();
    RichDocument {
        blocks: vec![
            RichBlock::Paragraph(p),
            RichBlock::Table(RichTable {
                columns: vec![160.0, 160.0],
                rows: vec![
                    vec![Paragraph::text("A"), Paragraph::text("1")],
                    vec![Paragraph::text("Б"), Paragraph::text("2")],
                ],
            }),
        ],
        ..Default::default()
    }
}
#[test]
fn unicode_ranges_never_split_graphemes() {
    let mut p = Paragraph::text("A👩‍💻e\u{301}Z");
    let original = p.clone();
    assert!(p.format(2..4, RunStyle::default()).is_err());
    assert_eq!(p, original);
    let end = "A👩‍💻".len();
    p.replace(1..end, "🦀", RunStyle::default()).unwrap();
    assert_eq!(p.plain_text(), "A🦀e\u{301}Z");
}
#[test]
fn partial_format_preserves_other_runs() {
    let mut p = Paragraph::text("alpha beta gamma");
    p.format(
        6..10,
        RunStyle {
            italic: true,
            color: [255, 0, 0],
            ..Default::default()
        },
    )
    .unwrap();
    p.map_styles(0..16, |s| s.bold = true).unwrap();
    assert!(p.runs.iter().all(|r| r.style.bold));
    assert!(p.runs[1].style.italic);
    assert_eq!(p.runs[1].style.color, [255, 0, 0]);
}
#[test]
fn candidate_validation_is_atomic() {
    let mut d = sample();
    let old = d.clone();
    assert!(
        d.transact(|next| {
            next.blocks.push(RichBlock::Image(RichImage {
                media: "missing".into(),
                width: 1.0,
                height: 1.0,
                alt: String::new(),
            }));
            Ok(())
        })
        .is_err()
    );
    assert_eq!(d, old);
}
#[test]
fn fragments_keep_styles_and_reject_conflicts_atomically() {
    let mut d = sample();
    d.styles.insert(
        "Heading".into(),
        RunStyle {
            bold: true,
            ..Default::default()
        },
    );
    d.apply_paragraph_style(0, "Heading").unwrap();
    let f = d.fragment(0..1).unwrap();
    let encoded = f.to_json().unwrap();
    assert_eq!(RichDocument::from_json(&encoded).unwrap(), f);
    let old = d.clone();
    let mut bad = f.clone();
    bad.styles.get_mut("Heading").unwrap().italic = true;
    assert!(d.insert_fragment(1, &bad).is_err());
    assert_eq!(d, old);
    d.insert_fragment(1, &f).unwrap();
    assert_eq!(d.blocks.len(), 3);
}
#[test]
fn json_rejects_unknown_fields_versions_and_ragged_tables() {
    let d = sample();
    assert!(
        RichDocument::from_json(
            &d.to_json()
                .unwrap()
                .replace("\"version\": 1", "\"version\": 2")
        )
        .is_err()
    );
    assert!(RichDocument::from_json("{\"version\":1,\"mystery\":true}").is_err());
    let mut d = d;
    let RichBlock::Table(t) = &mut d.blocks[1] else {
        panic!()
    };
    t.rows[0].pop();
    assert!(d.validate().is_err());
}
#[test]
fn docx_real_zip_roundtrip_preserves_text_and_typography() {
    let d = sample();
    let exported = d.to_docx().unwrap();
    assert_eq!(&exported.bytes[..2], b"PK");
    let imported = RichDocument::from_docx(&exported.bytes).unwrap();
    assert_eq!(d, imported.document);
}
#[test]
fn docx_unicode_xml_escaping_tabs_and_linebreaks() {
    let mut d = RichDocument::from_text("<&\"'>\t日本\nمرحبا").unwrap();
    let RichBlock::Paragraph(p) = &mut d.blocks[0] else {
        panic!()
    };
    p.runs[0].text.push_str("\nnext");
    assert_eq!(
        RichDocument::from_docx(&d.to_docx().unwrap().bytes)
            .unwrap()
            .document,
        d
    );
}
#[test]
fn docx_rejects_corrupt_and_oversize_input() {
    assert!(RichDocument::from_docx(b"not a zip").is_err());
    let mut bytes = sample().to_docx().unwrap().bytes;
    bytes.truncate(bytes.len() / 2);
    assert!(RichDocument::from_docx(&bytes).is_err());
}
#[test]
fn pagination_shapes_bidi_and_splits_long_paragraph_without_lost_ranges() {
    let mut d = RichDocument::from_text(&"English русский العربية 👩‍💻 ".repeat(80)).unwrap();
    d.page = PageSettings {
        width: 300.0,
        height: 260.0,
        margin: 24.0,
        ..Default::default()
    };
    let layout = d.paginate(&mut TextEngine::new()).unwrap();
    assert!(layout.pages.len() > 2);
    assert!(!layout.actual_fonts.is_empty());
    let mut end = 0;
    for page in &layout.pages {
        for p in &page.placements {
            let range = p.text_range.clone().unwrap();
            assert_eq!(range.start, end);
            assert!(range.end > range.start);
            end = range.end;
            assert!(p.bounds.y + p.bounds.height <= 236.01);
        }
    }
    let RichBlock::Paragraph(p) = &d.blocks[0] else {
        panic!()
    };
    assert_eq!(end, p.plain_text().len());
}
#[test]
fn pagination_rejects_too_tall_table_row() {
    let d = RichDocument {
        page: PageSettings {
            width: 300.0,
            height: 200.0,
            margin: 24.0,
            ..Default::default()
        },
        blocks: vec![RichBlock::Table(RichTable {
            columns: vec![200.0],
            rows: vec![vec![Paragraph::text("long text ".repeat(200))]],
        })],
        ..Default::default()
    };
    assert!(matches!(
        d.paginate(&mut TextEngine::new()),
        Err(DocumentError::Unsupported(_))
    ));
}
#[test]
fn media_and_page_geometry_are_validated() {
    let mut d = sample();
    d.page.margin = f32::NAN;
    assert!(d.validate().is_err());
    let mut d = sample();
    d.media.insert(
        "unsafe/path".into(),
        DocumentMedia {
            content_type: "image/png".into(),
            bytes: vec![1],
        },
    );
    assert!(d.validate().is_err());
}

#[test]
fn docx_preserves_asymmetric_page_geometry() {
    let mut d = sample();
    d.page.margins = Some([60.0, 70.0, 80.0, 90.0]);
    let imported = RichDocument::from_docx(&d.to_docx().unwrap().bytes).unwrap();
    assert_eq!(d.page.margins(), imported.document.page.margins());
    assert_eq!(d.plain_text(), imported.document.plain_text());
}

#[test]
fn docx_image_bytes_and_page_placement_roundtrip() {
    const PNG: &[u8] = &[
        137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 2, 0, 0, 0, 2, 8, 6,
        0, 0, 0, 114, 182, 13, 36, 0, 0, 0, 22, 73, 68, 65, 84, 120, 156, 99, 96, 136, 223, 241,
        255, 255, 49, 159, 255, 12, 96, 2, 200, 1, 0, 89, 226, 10, 77, 76, 54, 151, 197, 0, 0, 0,
        0, 73, 69, 78, 68, 174, 66, 96, 130,
    ];
    let mut d = sample();
    d.media.insert(
        "checker".into(),
        DocumentMedia {
            content_type: "image/png".into(),
            bytes: PNG.to_vec(),
        },
    );
    d.blocks.push(RichBlock::Image(RichImage {
        media: "checker".into(),
        width: 80.0,
        height: 60.0,
        alt: "Original\tchecker\r\nsecond line\rthird line".into(),
    }));
    let exported = d.to_docx().unwrap();
    let imported = RichDocument::from_docx(&exported.bytes).unwrap();
    assert_eq!(d, imported.document);
    let layout = d.paginate(&mut TextEngine::new()).unwrap();
    assert_eq!(
        layout
            .pages
            .iter()
            .map(|p| p.scene.images.len())
            .sum::<usize>(),
        1
    );
    let mixed = rewrite_document(&exported.bytes, |s| {
        s.replacen(
            "<w:drawing>",
            "<w:t><!--comment before text-->hidden suffix</w:t><w:drawing>",
            1,
        )
    });
    assert!(matches!(
        RichDocument::from_docx(&mixed),
        Err(DocumentError::Unsupported(message)) if message == "inline image mixed with text"
    ));
}

fn rewrite_document(bytes: &[u8], map: impl Fn(&str) -> String) -> Vec<u8> {
    use std::io::{Cursor, Read, Write};
    use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};
    let mut input = ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut output = ZipWriter::new(Cursor::new(Vec::new()));
    for index in 0..input.len() {
        let mut file = input.by_index(index).unwrap();
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes).unwrap();
        if file.name() == "word/document.xml" {
            bytes = map(std::str::from_utf8(&bytes).unwrap()).into_bytes();
        }
        output
            .start_file(file.name(), SimpleFileOptions::default())
            .unwrap();
        output.write_all(&bytes).unwrap();
    }
    output.finish().unwrap().into_inner()
}

#[test]
fn docx_rejects_unknown_active_content_without_publishing() {
    let bytes = sample().to_docx().unwrap().bytes;
    for changed in [
        rewrite_document(&bytes, |s| {
            s.replace("<w:body>", "<w:body><w:altChunk r:id=\"external\"/>")
        }),
        rewrite_document(&bytes, |s| {
            s.replacen("<w:r>", "<w:r><w:fldChar w:fldCharType=\"begin\"/>", 1)
        }),
        rewrite_document(&bytes, |s| {
            s.replacen("<w:tcPr>", "<w:tcPr><w:gridSpan w:val=\"2\"/>", 1)
        }),
    ] {
        assert!(matches!(
            RichDocument::from_docx(&changed),
            Err(DocumentError::Unsupported(_))
        ));
    }
}

#[test]
fn docx_rejects_external_entities_and_invalid_xml_characters() {
    let bytes = sample().to_docx().unwrap().bytes;
    let changed = rewrite_document(&bytes, |s| {
        s.replace(
            "?><w:document",
            "?><!DOCTYPE document [<!ENTITY external SYSTEM 'file:///private'>]><w:document",
        )
    });
    assert!(RichDocument::from_docx(&changed).is_err());
    let d = RichDocument::from_text("invalid\u{1}").unwrap();
    assert!(matches!(d.to_docx(), Err(DocumentError::Unsupported(_))));
}

#[test]
fn docx_text_comments_preserve_all_text_and_nested_elements_are_rejected() {
    let document = RichDocument::from_text("ab").unwrap();
    let bytes = document.to_docx().unwrap().bytes;
    let commented = rewrite_document(&bytes, |s| {
        s.replace(">ab</w:t>", ">a<!--comment between text nodes-->b</w:t>")
    });
    assert_eq!(
        RichDocument::from_docx(&commented).unwrap().document,
        document
    );
    let nested = rewrite_document(&bytes, |s| s.replace(">ab</w:t>", ">a<w:br/>b</w:t>"));
    assert!(matches!(
        RichDocument::from_docx(&nested),
        Err(DocumentError::Unsupported(message)) if message.contains("must contain scalar text")
    ));
}

#[test]
fn docx_preserves_xml_whitespace_in_paragraphs_and_table_cells() {
    let text = "first\nsecond\tthird & <literal>";
    let mut paragraph = Paragraph::text(text);
    paragraph.runs[0].style.bold = true;
    paragraph.runs[0].style.italic = true;
    let document = RichDocument {
        blocks: vec![
            RichBlock::Paragraph(paragraph.clone()),
            RichBlock::Table(RichTable {
                columns: vec![240.0],
                rows: vec![vec![paragraph]],
            }),
        ],
        ..Default::default()
    };
    document.validate().unwrap();
    let export = document.to_docx().unwrap();
    let imported = RichDocument::from_docx(&export.bytes).unwrap();
    assert_eq!(imported.document, document);
    assert!(export.diagnostics.is_empty());
}

#[test]
fn docx_never_bypasses_paragraph_and_table_cell_cr_nul_validation() {
    for text in ["invalid\rparagraph", "invalid\0paragraph"] {
        let paragraph = Paragraph::text(text);
        assert!(paragraph.validate().is_err());
        for block in [
            RichBlock::Paragraph(paragraph.clone()),
            RichBlock::Table(RichTable {
                columns: vec![240.0],
                rows: vec![vec![paragraph.clone()]],
            }),
        ] {
            let document = RichDocument {
                blocks: vec![block],
                ..Default::default()
            };
            assert!(document.validate().is_err());
            assert!(matches!(document.to_docx(), Err(DocumentError::Invalid(_))));
        }
    }
}

#[test]
fn missing_font_request_is_preserved_and_actual_faces_reported() {
    let mut d = RichDocument::from_text("Fallback font").unwrap();
    let RichBlock::Paragraph(p) = &mut d.blocks[0] else {
        panic!()
    };
    p.runs[0].style.family = "Definitely missing font 9af891".into();
    let before = d.clone();
    let layout = d.paginate(&mut TextEngine::new()).unwrap();
    assert_eq!(d, before);
    assert!(!layout.actual_fonts.is_empty());
    assert!(
        layout
            .actual_fonts
            .iter()
            .all(|n| n != "Definitely missing font 9af891")
    );
}
