use rust_desktop_ui_controls::{UiCulture, spreadsheet::format_value};
use rust_desktop_ui_data::{CellValue, DateValue, DecimalValue, workbook::WorkbookValue};

fn display(value: CellValue, code: &str) -> String {
    let result = format_value(&WorkbookValue::Scalar(value), code, UiCulture::Russian);
    assert!(
        result.diagnostic.is_none(),
        "{code}: {:?}",
        result.diagnostic
    );
    result.text
}

#[test]
fn predicates_select_before_rounding_with_exact_decimal_boundaries() {
    let code = "[>9007199254740992]\"HIGH\";[=9007199254740992]\"EDGE\";\"LOW\"";
    for (value, expected) in [
        (9_007_199_254_740_993, "HIGH"),
        (9_007_199_254_740_992, "EDGE"),
        (9_007_199_254_740_991, "LOW"),
    ] {
        assert_eq!(display(CellValue::Integer(value), code), expected);
    }
    for (value, expected) in [
        ("0.1000000000000000000000000001", "HIGH"),
        ("0.1", "EDGE"),
        ("0.0999999999999999999999999999", "LOW"),
    ] {
        assert_eq!(
            display(
                CellValue::Decimal(value.parse().unwrap()),
                "[>.1]\"HIGH\";[=+1e-1]\"EDGE\";\"LOW\""
            ),
            expected
        );
    }
    assert_eq!(
        display(
            CellValue::Decimal(DecimalValue::new(i128::MIN, 0).unwrap()),
            "[<-170141183460469231731687303715884105727]\"MIN\";0"
        ),
        "MIN"
    );
    assert_eq!(
        display(CellValue::Real(f64::from_bits(1)), "[>0]\"TINY\";0"),
        "TINY"
    );
    assert_eq!(
        display(CellValue::Real(1e308), "[>=1E+308]\"LARGE\";0"),
        "LARGE"
    );
    assert_eq!(display(CellValue::Real(-0.0), "[=0]\"ZERO\";0"), "ZERO");
    assert_eq!(
        display(
            CellValue::Decimal("0.995".parse().unwrap()),
            "[<1]0.00\" low\";0"
        ),
        "1,00 low"
    );
}

#[test]
fn all_relations_match_independent_integer_arithmetic() {
    for operator in ["<", "<=", "=", "<>", ">=", ">"] {
        for threshold in [-100, -1, 0, 1, 100] {
            for n in -150..=150 {
                let yes = match operator {
                    "<" => n < threshold,
                    "<=" => n <= threshold,
                    "=" => n == threshold,
                    "<>" => n != threshold,
                    ">=" => n >= threshold,
                    _ => n > threshold,
                };
                let code = format!("[{operator}{}]\"Y\";\"N\"", f64::from(threshold) / 100.0);
                let value = CellValue::Decimal(DecimalValue::new(n.into(), 2).unwrap());
                assert_eq!(
                    display(value, &code),
                    if yes { "Y" } else { "N" },
                    "{n} {code}"
                );
            }
        }
    }
}

#[test]
fn ordered_conditions_default_sections_and_negative_only_signs() {
    for (value, code, expected) in [
        (-120, "[>100]0;[<=-100]0;0", "120"),
        (-10, "[>100]0;[<=-100]0;0", "-10"),
        (-10, "[<0](0);0", "(10)"),
        (-10, "[<=0]0;0", "-10"),
        (-10, "[<1]0;0", "-10"),
        (-10, "[>-10]0;0", "10"),
        (-10, "[>=-10]0;0", "-10"),
        (-10, "[<>-10]0;0", "10"),
        (-10, "[>=0]0;[<>0]0;0", "-10"),
        (-10, "0;[<-10]0;0", "-10"),
        (-10, "[>100]0;0;\"fallback\"", "10"),
        (10, "[>100]0;0;\"fallback\"", "fallback"),
        (10, "[>100]0.00", "10"),
        (120, "[>100]0.00", "120,00"),
        (-10, "[>100]0.00;General", "-10"),
        (-10, "[<0]General;General", "10"),
        (10, "[>0]\"first\";[>5]\"second\";0", "first"),
        (10, "[>0];0", ""),
        (-10, "0;0;\"zero\"", "10"),
    ] {
        assert_eq!(
            display(CellValue::Integer(value), code),
            expected,
            "{value} {code}"
        );
    }
    // Conditional selection uses the original value; formatting does not
    // reselect a section when rounding produces zero.
    assert_eq!(
        display(CellValue::Decimal("-0.001".parse().unwrap()), "[<=0]0.00;0"),
        "-0,00"
    );
}

#[test]
fn colors_locales_fractions_durations_and_text_share_selected_section() {
    let value = WorkbookValue::Scalar(CellValue::Decimal("-1.25".parse().unwrap()));
    let result = format_value(
        &value,
        "[Red][<-1][$-en-US]0.00;[Blue]0.00",
        UiCulture::Russian,
    );
    assert_eq!(result.text, "1.25");
    assert_eq!(result.foreground, Some(0xff0000));
    assert!(result.diagnostic.is_none());
    assert_eq!(
        display(
            CellValue::Decimal("-1.25".parse().unwrap()),
            "[<-1][Blue]# ?/?;0"
        ),
        "1 1/4"
    );
    assert_eq!(
        display(CellValue::Decimal("-1.5".parse().unwrap()), "[<-1][h]:mm;0"),
        "36:00"
    );
    assert_eq!(
        display(CellValue::Text("A;B".into()), "[>1]0;[<0]0;0;\"Text \"@"),
        "Text A;B"
    );
    assert_eq!(display(CellValue::Integer(5), "\"[>1]\"0"), "[>1]5");
    assert_eq!(display(CellValue::Integer(5), "\\[\\>\\1\\]0"), "[>1]5");
}

#[test]
fn malformed_unmatched_and_nonnumeric_conditions_keep_raw_diagnostic() {
    for code in [
        "[>=]0",
        "[==1]0",
        "[!=1]0",
        "[>--1]0",
        "[>1e]0",
        "[>1e2147483647]0",
        "[>1e-1025]0",
        "[>NaN]0",
        "[>0x10]0",
        "[>1.2.3]0",
        "[> 1]0",
        "[>1][<10]0",
        "0;0;[>1]0",
        "0;0;0;[>1]@",
        "[>100]0;[<-100]0",
    ] {
        let result = format_value(
            &WorkbookValue::Scalar(CellValue::Integer(5)),
            code,
            UiCulture::Russian,
        );
        assert!(result.diagnostic.is_some(), "{code}");
        assert_eq!(result.text, "5");
    }
    let result = format_value(
        &WorkbookValue::Scalar(CellValue::Date(DateValue::new(2026, 11, 5).unwrap())),
        "[>1]yyyy;yyyy",
        UiCulture::Russian,
    );
    assert!(result.diagnostic.is_some());
}

#[test]
fn conditional_cells_formulas_xlsx_accessibility_and_print_share_one_formatter() {
    use rust_desktop_ui_controls::{
        Controls, Theme, printing::from_workbook, spreadsheet::Spreadsheet,
    };
    use rust_desktop_ui_core::{LayoutStyle, Length, Size};
    use rust_desktop_ui_data::workbook::*;
    let mut book = Workbook::new("Conditions").unwrap();
    let a = CellAddress::new(0, 0).unwrap();
    let b = CellAddress::new(1, 0).unwrap();
    let code = "[>100][Green]0.0\" high\";[<-100][Red](0.0);0.0\" mid\"";
    let mut first = WorkbookCell::value(CellValue::Integer(-120));
    first.style.number_format = code.into();
    let mut formula = WorkbookCell::formula("=-A1*2");
    formula.style.number_format = code.into();
    book.apply(&[
        WorkbookEdit::Set {
            sheet: 0,
            address: a,
            cell: first,
        },
        WorkbookEdit::Set {
            sheet: 0,
            address: b,
            cell: formula,
        },
    ])
    .unwrap();
    let loaded = Workbook::from_xlsx(&book.to_xlsx().unwrap())
        .unwrap()
        .workbook;
    for address in [a, b] {
        assert_eq!(
            loaded.value(0, address).unwrap(),
            book.value(0, address).unwrap()
        );
        assert_eq!(
            loaded.sheets()[0].cell(address).unwrap(),
            book.sheets()[0].cell(address).unwrap()
        );
    }
    let pages = from_workbook(&loaded, 0, Default::default(), || false).unwrap();
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let root = ui.root();
    let mut sheet = Spreadsheet::mount(
        &mut ui,
        root,
        loaded,
        LayoutStyle {
            width: Length::Percent(1.0),
            height: Length::Percent(1.0),
            ..Default::default()
        },
    )
    .unwrap();
    ui.update(Size::new(1000.0, 700.0)).unwrap();
    sheet.refresh(&mut ui).unwrap();
    let semantics = sheet.semantics(&ui).unwrap();
    for (address, expected) in [(a, "(120,0)"), (b, "240,0 high")] {
        assert_eq!(
            semantics
                .cells
                .iter()
                .find(|c| c.address == address)
                .unwrap()
                .value,
            expected
        );
        assert!(
            pages
                .pages()
                .iter()
                .any(|p| p.scene.texts.iter().any(|t| t.text == expected))
        );
    }
    let mut unmatched = book;
    let mut cell = WorkbookCell::value(CellValue::Integer(0));
    cell.style.number_format = "[>100]0;[<-100]0".into();
    unmatched
        .apply(&[WorkbookEdit::Set {
            sheet: 0,
            address: a,
            cell,
        }])
        .unwrap();
    assert!(from_workbook(&unmatched, 0, Default::default(), || false).is_err());
}

#[test]
fn auto_fit_measures_selected_condition_without_changing_values() {
    use rust_desktop_ui_controls::spreadsheet::{AutoFitAxis, auto_fit_edits};
    use rust_desktop_ui_data::workbook::*;
    use rust_desktop_ui_text::TextEngine;
    let mut book = Workbook::new("Sizing").unwrap();
    let a = CellAddress::new(0, 0).unwrap();
    let b = CellAddress::new(0, 1).unwrap();
    let mut cell = WorkbookCell::value(CellValue::Integer(1));
    cell.style.number_format = "[>0]\"A long conditional display label\";0".into();
    book.apply(&[
        WorkbookEdit::Set {
            sheet: 0,
            address: a,
            cell,
        },
        WorkbookEdit::Set {
            sheet: 0,
            address: b,
            cell: WorkbookCell::value(CellValue::Integer(1)),
        },
    ])
    .unwrap();
    let before = book.clone();
    let edits = auto_fit_edits(
        &book,
        0,
        AutoFitAxis::Columns,
        0..=1,
        UiCulture::Russian,
        &mut TextEngine::new(),
    )
    .unwrap();
    assert_eq!(book, before);
    book.apply(&edits).unwrap();
    let sizes = book.sheets()[0].dimensions();
    assert!(sizes.column_width(0).unwrap() > sizes.column_width(1).unwrap() + 100.0);
    assert_eq!(book.sheets()[0].cells(), before.sheets()[0].cells());
}
