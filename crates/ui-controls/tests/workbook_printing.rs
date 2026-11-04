//! Worksheet pagination, styling and transactional output regressions.
use rust_desktop_ui_controls::pdf::PdfError;
use rust_desktop_ui_controls::printing::{SheetPrintOptions, from_workbook};
use rust_desktop_ui_core::{Color, Size};
use rust_desktop_ui_data::{
    CellValue,
    workbook::{CellAddress, CellRange, Workbook, WorkbookCell, WorkbookEdit},
};

fn address(row: u32, col: u32) -> CellAddress {
    CellAddress::new(row, col).unwrap()
}
fn range(a: (u32, u32), b: (u32, u32)) -> CellRange {
    CellRange::new(address(a.0, a.1), address(b.0, b.1)).unwrap()
}
fn options() -> SheetPrintOptions {
    SheetPrintOptions {
        page_size: Size::new(400.0, 120.0),
        margin: 10.0,
        column_width: 100.0,
        row_height: 20.0,
        fit_width: true,
    }
}

#[test]
fn fit_width_scales_typography_and_rejects_silent_overflow() {
    let mut book = Workbook::new("Wide").unwrap();
    book.apply(&[WorkbookEdit::Set {
        sheet: 0,
        address: address(0, 2),
        cell: WorkbookCell::value(CellValue::Text("Edge".into())),
    }])
    .unwrap();
    let options = SheetPrintOptions {
        column_width: 200.0,
        ..options()
    };
    assert!(
        from_workbook(
            &book,
            0,
            SheetPrintOptions {
                fit_width: false,
                ..options
            },
            || false
        )
        .is_err()
    );
    let doc = from_workbook(&book, 0, options, || false).unwrap();
    let text = doc.pages()[0]
        .scene
        .texts
        .iter()
        .find(|t| t.text == "Edge")
        .unwrap();
    assert!(text.font_size < 14.0 && text.font_size > 8.0);
    assert!((text.clip.x + text.clip.width - 390.0).abs() < 0.01);
    let impossible = SheetPrintOptions {
        column_width: 2048.0,
        page_size: Size::new(40.0, 120.0),
        margin: 8.0,
        ..options
    };
    assert!(from_workbook(&book, 0, impossible, || false).is_err());
}

#[test]
fn worksheet_pages_keep_merged_cell_whole_styled_and_unique() {
    let mut book = Workbook::new("Report").unwrap();
    let mut edits = Vec::new();
    for row in 0..20 {
        edits.push(WorkbookEdit::Set {
            sheet: 0,
            address: address(row, 2),
            cell: WorkbookCell::value(CellValue::Text(format!("row{row}"))),
        });
    }
    let mut cell = WorkbookCell::value(CellValue::Text("Merged title".into()));
    cell.style.bold = true;
    cell.style.italic = true;
    cell.style.wrap = true;
    cell.style.foreground = Some(0x123456);
    cell.style.background = Some(0xabcdef);
    edits.push(WorkbookEdit::Set {
        sheet: 0,
        address: address(4, 0),
        cell,
    });
    edits.push(WorkbookEdit::Merge {
        sheet: 0,
        range: range((4, 0), (6, 1)),
    });
    book.apply(&edits).unwrap();
    let original = book.clone();
    let output = from_workbook(&book, 0, options(), || false).unwrap();
    assert_eq!(book, original);
    assert_eq!(output.pages().len(), 5);
    assert!(
        output.pages()[0]
            .scene
            .texts
            .iter()
            .any(|t| t.text == "row3")
    );
    assert!(
        !output.pages()[0]
            .scene
            .texts
            .iter()
            .any(|t| t.text == "row4")
    );
    let occurrences: Vec<_> = output
        .pages()
        .iter()
        .enumerate()
        .flat_map(|(page, p)| {
            p.scene
                .texts
                .iter()
                .enumerate()
                .filter(|(_, t)| t.text == "Merged title")
                .map(move |(i, t)| (page, i, t))
        })
        .collect();
    assert_eq!(occurrences.len(), 1);
    let (page, index, text) = occurrences[0];
    assert_eq!(page, 1);
    assert_eq!(text.weight, 700);
    assert!(text.wrap);
    assert_eq!(text.color, Color::rgb(0x12, 0x34, 0x56));
    assert!(text.clip.width > 190.0 && text.clip.height > 59.0);
    assert!(
        output.pages()[page].scene.text_spans(index)[0]
            .font
            .as_ref()
            .is_some()
    );
    assert!(
        output.pages()[page]
            .scene
            .rectangles
            .iter()
            .any(|r| r.color == Color::rgb(0xab, 0xcd, 0xef) && r.bounds == text.clip)
    );
    for row in 0..20 {
        assert_eq!(
            output
                .pages()
                .iter()
                .flat_map(|p| &p.scene.texts)
                .filter(|t| t.text == format!("row{row}"))
                .count(),
            1
        );
    }
}

#[test]
fn worksheet_merge_extent_and_connected_oversize_blocks_are_checked() {
    let mut empty = Workbook::new("Empty").unwrap();
    assert_eq!(
        from_workbook(&empty, 0, options(), || false)
            .unwrap()
            .pages()
            .len(),
        1
    );
    empty
        .apply(&[WorkbookEdit::Merge {
            sheet: 0,
            range: range((7, 0), (9, 1)),
        }])
        .unwrap();
    assert_eq!(
        from_workbook(&empty, 0, options(), || false)
            .unwrap()
            .pages()
            .len(),
        2
    );
    let mut book = Workbook::new("Connected").unwrap();
    book.apply(&[
        WorkbookEdit::Merge {
            sheet: 0,
            range: range((0, 0), (3, 0)),
        },
        WorkbookEdit::Merge {
            sheet: 0,
            range: range((3, 1), (6, 1)),
        },
    ])
    .unwrap();
    assert!(matches!(
        from_workbook(&book, 0, options(), || false),
        Err(PdfError::Invalid(_))
    ));
}

#[test]
fn worksheet_output_rejects_invalid_geometry_range_and_cancel_without_mutation() {
    let mut book = Workbook::new("Validated").unwrap();
    book.apply(&[WorkbookEdit::Set {
        sheet: 0,
        address: address(8, 2),
        cell: WorkbookCell::formula("=1+2"),
    }])
    .unwrap();
    let original = book.clone();
    for invalid in [
        SheetPrintOptions {
            margin: f32::NAN,
            ..options()
        },
        SheetPrintOptions {
            row_height: 0.0,
            ..options()
        },
        SheetPrintOptions {
            page_size: Size::new(1.0, 100.0),
            ..options()
        },
    ] {
        assert!(from_workbook(&book, 0, invalid, || false).is_err());
    }
    assert!(from_workbook(&book, 1, options(), || false).is_err());
    assert!(matches!(
        from_workbook(&book, 0, options(), || true),
        Err(PdfError::Cancelled)
    ));
    let mut calls = 0;
    assert!(matches!(
        from_workbook(&book, 0, options(), || {
            calls += 1;
            calls > 12
        }),
        Err(PdfError::Cancelled)
    ));
    let output = from_workbook(&book, 0, options(), || false).unwrap();
    assert_eq!(
        output
            .pages()
            .iter()
            .flat_map(|p| &p.scene.texts)
            .filter(|t| t.text == "3")
            .count(),
        1
    );
    assert_eq!(book, original);
}

#[test]
fn worksheet_dimensions_control_printed_coordinates_and_page_breaks() {
    let mut book = Workbook::new("Variable geometry").unwrap();
    let mut edits = vec![
        WorkbookEdit::ColumnWidth {
            sheet: 0,
            column: 0,
            width: Some(200.0),
        },
        WorkbookEdit::RowHeight {
            sheet: 0,
            row: 0,
            height: Some(60.0),
        },
    ];
    for row in 0..3 {
        edits.push(WorkbookEdit::Set {
            sheet: 0,
            address: address(row, 1),
            cell: WorkbookCell::value(CellValue::Text(format!("row{row}"))),
        });
    }
    book.apply(&edits).unwrap();
    let options = SheetPrintOptions {
        page_size: Size::new(400.0, 120.0),
        margin: 10.0,
        fit_width: false,
        ..Default::default()
    };
    let printed = from_workbook(&book, 0, options, || false).unwrap();
    assert_eq!(printed.pages().len(), 2);
    let first = printed.pages()[0]
        .scene
        .texts
        .iter()
        .find(|t| t.text == "row0")
        .unwrap();
    assert_eq!(
        first.clip,
        rust_desktop_ui_core::Rect::new(210.0, 10.0, 112.0, 60.0)
    );
    assert!(
        printed.pages()[1]
            .scene
            .texts
            .iter()
            .any(|t| t.text == "row2")
    );
}

#[test]
fn formatted_dates_times_and_engineering_values_survive_xlsx_and_print() {
    use rust_desktop_ui_data::{DateValue, TimeValue};
    let mut book = Workbook::new("Formatted").unwrap();
    let fixtures = [
        (
            CellValue::Date(DateValue::new(2026, 11, 4).unwrap()),
            "[$-en-US]dddd, mmmm d, yyyy",
            "Wednesday, November 4, 2026",
        ),
        (
            CellValue::Time(TimeValue::new(16, 5, 0, 0).unwrap()),
            "h:mm AM/PM",
            "4:05 PM",
        ),
        (CellValue::Real(12_200_000.0), "##0.00E+00", "12,20E+06"),
    ];
    let mut edits = vec![WorkbookEdit::ColumnWidth {
        sheet: 0,
        column: 0,
        width: Some(370.0),
    }];
    for (row, (value, format, _)) in fixtures.iter().enumerate() {
        let mut cell = WorkbookCell::value(value.clone());
        cell.style.number_format = (*format).into();
        edits.push(WorkbookEdit::Set {
            sheet: 0,
            address: address(row as u32, 0),
            cell,
        });
    }
    book.apply(&edits).unwrap();
    let imported = Workbook::from_xlsx(&book.to_xlsx().unwrap()).unwrap();
    assert!(
        imported.diagnostics.is_empty(),
        "{:?}",
        imported.diagnostics
    );
    let document = from_workbook(&imported.workbook, 0, options(), || false).unwrap();
    for (row, (_, _, expected)) in fixtures.iter().enumerate() {
        let display = rust_desktop_ui_controls::spreadsheet::format_cell(
            &imported.workbook,
            0,
            address(row as u32, 0),
            rust_desktop_ui_controls::UiCulture::Russian,
        )
        .unwrap();
        assert_eq!(display.text, *expected);
        assert!(display.diagnostic.is_none());
        assert!(
            document
                .pages()
                .iter()
                .any(|p| p.scene.texts.iter().any(|t| t.text == *expected)),
            "{expected}"
        );
    }
}
