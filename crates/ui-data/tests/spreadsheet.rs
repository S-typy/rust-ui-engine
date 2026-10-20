use rust_desktop_ui_data::{CellValue, spreadsheet::SpreadsheetSession, workbook::*};
fn a(s: &str) -> CellAddress {
    CellAddress::parse(s).unwrap()
}
fn r(s: &str, e: &str) -> CellRange {
    CellRange::new(a(s), a(e)).unwrap()
}
fn book() -> Workbook {
    Workbook::new("Данные").unwrap()
}
#[test]
fn merges_reject_hidden_values_and_survive_xlsx() {
    let mut b = book();
    b.apply(&[WorkbookEdit::Set {
        sheet: 0,
        address: a("B1"),
        cell: WorkbookCell::value(CellValue::Integer(4)),
    }])
    .unwrap();
    let before = b.clone();
    assert!(
        b.apply(&[WorkbookEdit::Merge {
            sheet: 0,
            range: r("A1", "C1")
        }])
        .is_err()
    );
    assert_eq!(before, b);
    b.apply(&[
        WorkbookEdit::Clear {
            sheet: 0,
            range: r("B1", "B1"),
        },
        WorkbookEdit::Merge {
            sheet: 0,
            range: r("A1", "C1"),
        },
    ])
    .unwrap();
    let imported = Workbook::from_xlsx(&b.to_xlsx().unwrap()).unwrap();
    assert_eq!(imported.workbook.sheets()[0].merges(), &[r("A1", "C1")]);
    assert!(
        imported.diagnostics.is_empty(),
        "{:?}",
        imported.diagnostics
    );
    assert_eq!(b.sheets()[0].anchor(a("C1")), a("A1"));
    assert!(
        b.apply(&[WorkbookEdit::Clear {
            sheet: 0,
            range: r("B1", "C1")
        }])
        .is_err()
    );
}
#[test]
fn fill_translates_and_undo_is_one_transaction() {
    let mut s = SpreadsheetSession::new(book());
    s.enter("3").unwrap();
    s.select(a("B1"), a("B1")).unwrap();
    s.enter("=$A$1+A1").unwrap();
    s.select(a("B1"), a("B4")).unwrap();
    s.fill(true).unwrap();
    assert_eq!(
        s.workbook().sheets()[0].cell(a("B3")).unwrap().input,
        CellInput::Formula("=($A$1+A3)".into())
    );
    let generation = s.generation();
    s.undo().unwrap();
    assert!(s.workbook().sheets()[0].cell(a("B3")).is_none());
    s.redo().unwrap();
    assert!(s.generation() > generation);
    assert_eq!(s.workbook().value(0, a("B1")).unwrap().display(), "6");
}
#[test]
fn typed_clipboard_preserves_merges_and_relative_references() {
    let mut s = SpreadsheetSession::new(book());
    s.enter("=A2+$B$2").unwrap();
    s.select(a("A1"), a("C1")).unwrap();
    s.merge().unwrap();
    s.select(a("B1"), a("B1")).unwrap();
    assert_eq!(s.selection(), r("A1", "C1"));
    s.paste(&s.copy(), a("D3")).unwrap();
    assert_eq!(
        s.workbook().sheets()[0].cell(a("D3")).unwrap().input,
        CellInput::Formula("=(D4+$B$2)".into())
    );
    assert!(s.workbook().sheets()[0].merges().contains(&r("D3", "F3")));
    s.undo().unwrap();
    assert!(!s.workbook().sheets()[0].merges().contains(&r("D3", "F3")));
}
#[test]
fn quoted_tsv_is_atomic_and_does_not_execute_external_formulas() {
    let mut s = SpreadsheetSession::new(book());
    s.paste_text("\"a\tb\"\t\"hello\nworld\"\n=1+1\t2", false)
        .unwrap();
    assert_eq!(s.workbook().value(0, a("A2")).unwrap().display(), "=1+1");
    assert_eq!(
        s.copy_text().unwrap(),
        "\"a\tb\"\t\"hello\nworld\"\n=1+1\t2"
    );
    let before = s.workbook().clone();
    assert!(s.paste_text("1\t2\n3", false).is_err());
    assert_eq!(&before, s.workbook());
    assert!(s.paste_text("\"unterminated", false).is_err());
    s.select(a("A3"), a("A3")).unwrap();
    s.paste_text("'literal", false).unwrap();
    assert_eq!(
        s.workbook().value(0, a("A3")).unwrap().display(),
        "'literal"
    );
}
#[test]
fn overlap_copy_snapshots_source_and_keeps_merge_integrity() {
    let mut b = book();
    b.apply(&[
        WorkbookEdit::Set {
            sheet: 0,
            address: a("A1"),
            cell: WorkbookCell::value(CellValue::Integer(42)),
        },
        WorkbookEdit::Merge {
            sheet: 0,
            range: r("A1", "B1"),
        },
    ])
    .unwrap();
    b.apply(&[WorkbookEdit::Copy {
        sheet: 0,
        source: r("A1", "B1"),
        destination_sheet: 0,
        destination: a("A2"),
    }])
    .unwrap();
    assert_eq!(b.sheets()[0].merges().len(), 2);
    let before = b.clone();
    assert!(
        b.apply(&[WorkbookEdit::Copy {
            sheet: 0,
            source: r("A1", "A1"),
            destination_sheet: 0,
            destination: a("C3")
        }])
        .is_err()
    );
    assert_eq!(b, before);
}
