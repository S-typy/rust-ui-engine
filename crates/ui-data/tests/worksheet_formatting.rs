use rust_desktop_ui_data::{CellValue, spreadsheet::SpreadsheetSession, workbook::*};
fn a(row: u32, col: u32) -> CellAddress {
    CellAddress::new(row, col).unwrap()
}

#[test]
fn format_range_preserves_inputs_other_styles_and_selection_with_one_undo() {
    let mut book = Workbook::new("Лист").unwrap();
    let mut cell = WorkbookCell::value(CellValue::Integer(12));
    cell.style.bold = true;
    cell.style.foreground = Some(0x123456);
    book.apply(&[
        WorkbookEdit::Set {
            sheet: 0,
            address: a(0, 0),
            cell: cell.clone(),
        },
        WorkbookEdit::Set {
            sheet: 0,
            address: a(0, 1),
            cell: WorkbookCell::formula("=A1*2"),
        },
    ])
    .unwrap();
    let mut session = SpreadsheetSession::new(book.clone());
    session.select(a(0, 0), a(0, 2)).unwrap();
    let selection = session.selection();
    let active = session.active();
    let generation = session.generation();
    assert!(session.set_number_format("#,##0.00").unwrap());
    assert_eq!(session.generation(), generation + 1);
    assert_eq!(session.selection(), selection);
    assert_eq!(session.active(), active);
    let formatted = session.workbook().sheets()[0].cell(a(0, 0)).unwrap();
    assert_eq!(formatted.input, cell.input);
    assert!(formatted.style.bold);
    assert_eq!(formatted.style.foreground, Some(0x123456));
    assert_eq!(formatted.style.number_format, "#,##0.00");
    assert_eq!(
        session.workbook().sheets()[0].cell(a(0, 1)).unwrap().input,
        book.sheets()[0].cell(a(0, 1)).unwrap().input
    );
    assert_eq!(
        session.workbook().value(0, a(0, 1)).unwrap(),
        book.value(0, a(0, 1)).unwrap()
    );
    assert_eq!(
        session.workbook().value(0, a(0, 1)).unwrap().display(),
        "24"
    );
    assert!(session.workbook().sheets()[0].cell(a(0, 2)).is_some());
    assert!(!session.set_number_format("#,##0.00").unwrap());
    assert_eq!(session.generation(), generation + 1);
    assert!(session.undo().unwrap());
    assert_eq!(session.workbook(), &book);
    assert_eq!(session.selection(), selection);
    assert_eq!(session.active(), active);
    assert!(session.redo().unwrap());
    assert_eq!(
        session.workbook().sheets()[0]
            .cell(a(0, 2))
            .unwrap()
            .style
            .number_format,
        "#,##0.00"
    );
}

#[test]
fn format_merged_cells_addresses_only_the_anchor_and_failure_is_atomic() {
    let mut book = Workbook::new("Merged").unwrap();
    book.apply(&[
        WorkbookEdit::Set {
            sheet: 0,
            address: a(0, 0),
            cell: WorkbookCell::value(CellValue::Integer(3)),
        },
        WorkbookEdit::Merge {
            sheet: 0,
            range: CellRange::new(a(0, 0), a(1, 1)).unwrap(),
        },
    ])
    .unwrap();
    let mut session = SpreadsheetSession::new(book);
    session.select(a(1, 1), a(1, 1)).unwrap();
    assert!(session.set_number_format("0.00%").unwrap());
    assert_eq!(session.workbook().sheets()[0].cells().len(), 1);
    let before = session.workbook().clone();
    let generation = session.generation();
    for format in ["x".repeat(257), "0\0".into()] {
        assert!(session.set_number_format(&format).is_err());
        assert_eq!(session.workbook(), &before);
        assert_eq!(session.generation(), generation);
    }
    assert!(session.set_number_format("General").unwrap());
    assert_eq!(
        session.workbook().sheets()[0]
            .cell(a(0, 0))
            .unwrap()
            .style
            .number_format,
        ""
    );
    assert!(!session.set_number_format("GENERAL").unwrap());
}

#[test]
fn format_capacity_failure_keeps_the_original_workbook_and_history() {
    let mut book = Workbook::new("Limit").unwrap();
    let edits: Vec<_> = (0..MAX_CELLS)
        .map(|row| WorkbookEdit::Set {
            sheet: 0,
            address: a(row as u32, 0),
            cell: WorkbookCell::value(CellValue::Integer(1)),
        })
        .collect();
    book.apply(&edits).unwrap();
    let mut session = SpreadsheetSession::new(book.clone());
    session.select(a(0, 1), a(0, 1)).unwrap();
    assert!(session.set_number_format("0.00").is_err());
    assert_eq!(session.workbook(), &book);
    assert!(!session.can_undo());
}
