use rust_desktop_ui_controls::{
    Controls, Theme, UiCulture,
    spreadsheet::{AutoFitAxis, Spreadsheet, auto_fit_edits},
};
use rust_desktop_ui_core::{InputEvent, Key, LayoutStyle, Length, Modifiers, Mutation, Size};
use rust_desktop_ui_data::{CellValue, workbook::*};
use rust_desktop_ui_text::TextEngine;

fn a(row: u32, col: u32) -> CellAddress {
    CellAddress::new(row, col).unwrap()
}
fn set(row: u32, col: u32, cell: WorkbookCell) -> WorkbookEdit {
    WorkbookEdit::Set {
        sheet: 0,
        address: a(row, col),
        cell,
    }
}
fn text(value: &str) -> WorkbookCell {
    WorkbookCell::value(CellValue::Text(value.into()))
}
fn fit(book: &Workbook, axis: AutoFitAxis, first: u32, last: u32) -> Vec<WorkbookEdit> {
    auto_fit_edits(
        book,
        0,
        axis,
        first..=last,
        UiCulture::Russian,
        &mut TextEngine::new(),
    )
    .unwrap()
}

#[test]
fn sparse_columns_measure_glyphs_formatting_and_trailing_spaces() {
    let mut book = Workbook::new("Sizing").unwrap();
    let mut number = WorkbookCell::value(CellValue::Real(12.5));
    number.style.number_format = "0.000000000000".into();
    number.style.bold = true;
    number.style.italic = true;
    book.apply(&[
        set(0, 0, text("iiiiiiiiiiii")),
        set(0, 1, text("WWWWWWWWWWWW")),
        set(0, 2, number),
        set(MAX_ROWS - 1, 3, text("Far away but measured")),
        set(0, 4, text("space")),
        set(0, 5, text("space          ")),
    ])
    .unwrap();
    let before = book.clone();
    let plan = fit(&book, AutoFitAxis::Columns, 0, 5);
    assert_eq!(book, before);
    book.apply(&plan).unwrap();
    let d = book.sheets()[0].dimensions();
    assert!(d.column_width(1).unwrap() > 2.0 * d.column_width(0).unwrap());
    assert!(d.column_width(2).unwrap() > 90.0);
    assert!(d.column_width(3).unwrap() > 130.0);
    assert!(d.column_width(5).unwrap() > d.column_width(4).unwrap() + 20.0);
    assert_eq!(book.sheets()[0].cells(), before.sheets()[0].cells());
    assert_eq!(book.value(0, a(0, 2)), before.value(0, a(0, 2)));
    assert!(fit(&book, AutoFitAxis::Columns, 0, 5).is_empty());
}

#[test]
fn rows_use_current_width_wrap_and_explicit_line_breaks() {
    let mut book = Workbook::new("Rows").unwrap();
    let mut wrapped = text("alpha beta gamma delta epsilon zeta eta theta");
    wrapped.style.wrap = true;
    book.apply(&[
        set(0, 0, wrapped),
        set(1, 0, text("one\ntwo\nthree")),
        WorkbookEdit::ColumnWidth {
            sheet: 0,
            column: 0,
            width: Some(80.0),
        },
    ])
    .unwrap();
    book.apply(&fit(&book, AutoFitAxis::Rows, 0, 1)).unwrap();
    let narrow = book.sheets()[0].dimensions().row_height(0).unwrap();
    assert!(narrow > 60.0);
    assert!(book.sheets()[0].dimensions().row_height(1).unwrap() > 40.0);
    book.apply(&[WorkbookEdit::ColumnWidth {
        sheet: 0,
        column: 0,
        width: Some(400.0),
    }])
    .unwrap();
    book.apply(&fit(&book, AutoFitAxis::Rows, 0, 0)).unwrap();
    assert!(book.sheets()[0].dimensions().row_height(0).unwrap() < narrow);
}

#[test]
fn merged_deficits_only_grow_selected_axes_and_xlsx_keeps_dimensions() {
    let mut book = Workbook::new("Merge").unwrap();
    book.apply(&[
        set(0, 0, text(&"W".repeat(70))),
        WorkbookEdit::Merge {
            sheet: 0,
            range: CellRange::new(a(0, 0), a(0, 2)).unwrap(),
        },
    ])
    .unwrap();
    book.apply(&fit(&book, AutoFitAxis::Columns, 1, 1)).unwrap();
    let d = book.sheets()[0].dimensions();
    assert_eq!(d.column_width(0), Some(DEFAULT_COLUMN_WIDTH));
    assert_eq!(d.column_width(2), Some(DEFAULT_COLUMN_WIDTH));
    assert!(d.column_width(1).unwrap() > DEFAULT_COLUMN_WIDTH * 2.0);
    let restored = Workbook::from_xlsx(&book.to_xlsx().unwrap())
        .unwrap()
        .workbook;
    for column in 0..3 {
        assert!(
            (restored.sheets()[0]
                .dimensions()
                .column_width(column)
                .unwrap()
                - d.column_width(column).unwrap())
            .abs()
                < 0.01
        );
    }
    assert_eq!(restored.sheets()[0].merges(), book.sheets()[0].merges());
}

#[test]
fn invalid_and_unrepresentable_plans_leave_workbook_untouched() {
    let mut book = Workbook::new("Invalid").unwrap();
    book.apply(&[set(0, 0, text(&"W".repeat(1000)))]).unwrap();
    let before = book.clone();
    for (sheet, axis, range) in [
        (1, AutoFitAxis::Columns, 0..=0),
        (0, AutoFitAxis::Columns, 0..=MAX_COLUMNS),
        (0, AutoFitAxis::Rows, 0..=MAX_DIMENSIONS as u32),
        (0, AutoFitAxis::Columns, 0..=0),
    ] {
        assert!(
            auto_fit_edits(
                &book,
                sheet,
                axis,
                range,
                UiCulture::Russian,
                &mut TextEngine::new()
            )
            .is_err()
        );
        assert_eq!(book, before);
    }
    let mut cell = WorkbookCell::value(CellValue::Integer(12));
    cell.style.number_format = "[>]0".into();
    book.apply(&[set(0, 0, cell)]).unwrap();
    assert!(
        auto_fit_edits(
            &book,
            0,
            AutoFitAxis::Columns,
            0..=0,
            UiCulture::Russian,
            &mut TextEngine::new()
        )
        .is_err()
    );
}

fn fixture() -> (Controls, Spreadsheet) {
    let mut book = Workbook::new("View").unwrap();
    book.apply(&[set(0, 0, text("short"))]).unwrap();
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let root = ui.root();
    let mut sheet = Spreadsheet::mount(
        &mut ui,
        root,
        book,
        LayoutStyle {
            width: Length::Percent(1.0),
            height: Length::Percent(1.0),
            ..Default::default()
        },
    )
    .unwrap();
    ui.update(Size::new(1000.0, 700.0)).unwrap();
    sheet.refresh(&mut ui).unwrap();
    (ui, sheet)
}
fn undo(ui: &mut Controls, sheet: &mut Spreadsheet) {
    ui.runtime
        .apply(Mutation::Focus(Some(sheet.parts.canvas)))
        .unwrap();
    assert!(
        sheet
            .input(
                ui,
                &InputEvent::KeyDown {
                    key: Key::Character("z".into()),
                    modifiers: Modifiers {
                        control: true,
                        ..Default::default()
                    },
                    repeat: false
                }
            )
            .unwrap()
    );
}

#[test]
fn view_stages_draft_preserves_editors_and_undoes_size_separately() {
    let (mut ui, mut sheet) = fixture();
    let formula = sheet.parts.formula;
    ui.set_text(formula, "a much longer value for auto sizing")
        .unwrap();
    ui.set_text(sheet.parts.format_code, "0.000").unwrap();
    sheet
        .auto_fit(&mut ui, AutoFitAxis::Columns, 0..=0)
        .unwrap();
    assert_eq!(sheet.parts.formula, formula);
    assert_eq!(
        ui.document(sheet.parts.format_code).unwrap().text(),
        "0.000"
    );
    assert!(
        sheet.session().workbook().sheets()[0]
            .dimensions()
            .column_width(0)
            .unwrap()
            > DEFAULT_COLUMN_WIDTH
    );
    let generation = sheet.session().generation();
    sheet
        .auto_fit(&mut ui, AutoFitAxis::Columns, 0..=0)
        .unwrap();
    assert_eq!(sheet.session().generation(), generation);
    undo(&mut ui, &mut sheet);
    assert_eq!(
        sheet.session().workbook().sheets()[0]
            .dimensions()
            .column_width(0),
        Some(DEFAULT_COLUMN_WIDTH)
    );
    assert_eq!(
        sheet.session().entry(),
        "a much longer value for auto sizing"
    );
    undo(&mut ui, &mut sheet);
    assert_eq!(sheet.session().entry(), "short");
    assert_eq!(sheet.parts.formula, formula);
}

#[test]
fn failed_sizing_preserves_live_draft_and_measured_toolbar_fits() {
    let (mut ui, mut sheet) = fixture();
    let before = sheet.session().workbook().clone();
    let draft = "W".repeat(1000);
    ui.set_text(sheet.parts.formula, &draft).unwrap();
    assert!(
        sheet
            .auto_fit(&mut ui, AutoFitAxis::Columns, 0..=0)
            .is_err()
    );
    assert_eq!(sheet.session().workbook(), &before);
    assert_eq!(ui.document(sheet.parts.formula).unwrap().text(), draft);
    assert!(!sheet.session().can_undo());
    for theme in [Theme::FluentLight, Theme::FluentDark] {
        ui.set_theme(theme).unwrap();
        for size in [Size::new(760.0, 550.0), Size::new(1200.0, 800.0)] {
            ui.update(size).unwrap();
            sheet.refresh(&mut ui).unwrap();
            for id in [sheet.parts.auto_columns, sheet.parts.auto_rows] {
                let bounds = ui.runtime.bounds(id).unwrap();
                let clip = ui.runtime.clip(id).unwrap();
                assert_eq!(bounds.intersection(clip), Some(bounds));
                assert!(bounds.height >= 32.0 && bounds.width >= 90.0);
            }
        }
    }
}

#[test]
fn either_editor_composition_blocks_sizing_without_losing_preedit() {
    for formula in [true, false] {
        let (mut ui, mut sheet) = fixture();
        let editor = if formula {
            sheet.parts.formula
        } else {
            sheet.parts.format_code
        };
        let before = sheet.session().workbook().clone();
        ui.runtime.apply(Mutation::Focus(Some(editor))).unwrap();
        ui.ime_preedit("に", Some((0, 0))).unwrap();
        for axis in [AutoFitAxis::Rows, AutoFitAxis::Columns] {
            assert!(sheet.auto_fit(&mut ui, axis, 0..=0).is_err());
            assert_eq!(sheet.session().workbook(), &before);
            assert!(!sheet.session().can_undo());
            assert!(ui.document(editor).unwrap().composition().is_some());
        }
    }
}

#[test]
fn vertically_merged_text_grows_only_selected_rows() {
    let mut book = Workbook::new("Vertical").unwrap();
    book.apply(&[
        set(0, 0, text(&["line"; 12].join("\n"))),
        WorkbookEdit::Merge {
            sheet: 0,
            range: CellRange::new(a(0, 0), a(2, 0)).unwrap(),
        },
    ])
    .unwrap();
    book.apply(&fit(&book, AutoFitAxis::Rows, 1, 1)).unwrap();
    let d = book.sheets()[0].dimensions();
    assert_eq!(d.row_height(0), Some(DEFAULT_ROW_HEIGHT));
    assert_eq!(d.row_height(2), Some(DEFAULT_ROW_HEIGHT));
    assert!(d.row_height(1).unwrap() > 120.0);
    assert!(fit(&book, AutoFitAxis::Rows, 1, 1).is_empty());
}
