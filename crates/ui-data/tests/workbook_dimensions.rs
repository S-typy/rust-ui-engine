use rust_desktop_ui_data::{spreadsheet::SpreadsheetSession, workbook::*};

#[test]
fn dimensions_are_sparse_atomic_and_participate_in_undo() {
    let mut session = SpreadsheetSession::new(Workbook::new("Dimensions").unwrap());
    let original = session.workbook().clone();
    session
        .apply(&[
            WorkbookEdit::RowHeight {
                sheet: 0,
                row: MAX_ROWS - 1,
                height: Some(47.5),
            },
            WorkbookEdit::ColumnWidth {
                sheet: 0,
                column: MAX_COLUMNS - 1,
                width: Some(280.0),
            },
        ])
        .unwrap();
    let dimensions = session.workbook().sheets()[0].dimensions();
    assert_eq!(dimensions.rows().len(), 1);
    assert_eq!(dimensions.columns().len(), 1);
    assert_eq!(dimensions.row_height(MAX_ROWS - 1), Some(47.5));
    assert_eq!(dimensions.row_height(MAX_ROWS), None);
    assert_eq!(dimensions.column_width(0), Some(112.0));
    assert!(session.workbook().sheets()[0].cells().is_empty());
    let resized = session.workbook().clone();
    assert!(session.undo().unwrap());
    assert_eq!(session.workbook(), &original);
    assert!(session.redo().unwrap());
    assert_eq!(session.workbook(), &resized);
    for height in [f64::NAN, f64::INFINITY, 0.0, -1.0, 4096.1] {
        assert!(
            session
                .apply(&[
                    WorkbookEdit::ColumnWidth {
                        sheet: 0,
                        column: 0,
                        width: Some(256.0)
                    },
                    WorkbookEdit::RowHeight {
                        sheet: 0,
                        row: 0,
                        height: Some(height)
                    },
                ])
                .is_err()
        );
        assert_eq!(session.workbook(), &resized);
    }
    session
        .apply(&[
            WorkbookEdit::DefaultDimensions {
                sheet: 0,
                row_height: 40.0,
                column_width: 120.0,
            },
            WorkbookEdit::RowHeight {
                sheet: 0,
                row: MAX_ROWS - 1,
                height: None,
            },
        ])
        .unwrap();
    let dimensions = session.workbook().sheets()[0].dimensions();
    assert!(dimensions.rows().is_empty());
    assert_eq!(dimensions.row_height(MAX_ROWS - 1), Some(40.0));
    assert_eq!(dimensions.column_width(MAX_COLUMNS - 1), Some(280.0));
}

#[test]
fn empty_sheet_and_last_row_dimensions_survive_xlsx() {
    let mut book = Workbook::new("Empty with geometry").unwrap();
    book.apply(&[
        WorkbookEdit::DefaultDimensions {
            sheet: 0,
            row_height: 32.0,
            column_width: 140.0,
        },
        WorkbookEdit::RowHeight {
            sheet: 0,
            row: MAX_ROWS - 1,
            height: Some(56.5),
        },
        WorkbookEdit::ColumnWidth {
            sheet: 0,
            column: MAX_COLUMNS - 1,
            width: Some(210.0),
        },
    ])
    .unwrap();
    let loaded = Workbook::from_xlsx(&book.to_xlsx().unwrap()).unwrap();
    assert!(loaded.diagnostics.is_empty(), "{:?}", loaded.diagnostics);
    assert_eq!(
        book.sheets()[0].dimensions(),
        loaded.workbook.sheets()[0].dimensions()
    );
    assert!(loaded.workbook.sheets()[0].cells().is_empty());
}

#[test]
fn dimension_capacity_is_independent_of_cell_capacity_and_rolls_back() {
    let mut book = Workbook::new("Sparse").unwrap();
    let edits: Vec<_> = (0..MAX_DIMENSIONS as u32)
        .map(|row| WorkbookEdit::RowHeight {
            sheet: 0,
            row,
            height: Some(31.0),
        })
        .collect();
    book.apply(&edits).unwrap();
    let before = book.clone();
    assert!(
        book.apply(&[WorkbookEdit::ColumnWidth {
            sheet: 0,
            column: 0,
            width: Some(150.0)
        }])
        .is_err()
    );
    assert_eq!(book, before);
    book.apply(&[
        WorkbookEdit::RowHeight {
            sheet: 0,
            row: 0,
            height: None,
        },
        WorkbookEdit::ColumnWidth {
            sheet: 0,
            column: 0,
            width: Some(150.0),
        },
    ])
    .unwrap();
    assert_eq!(
        book.sheets()[0].dimensions().rows().len(),
        MAX_DIMENSIONS - 1
    );
}
