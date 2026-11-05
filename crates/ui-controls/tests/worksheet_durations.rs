use rust_desktop_ui_controls::{UiCulture, spreadsheet::format_value};
use rust_desktop_ui_data::{CellValue, DateValue, TimeValue, workbook::*};

fn display(value: CellValue, format: &str) -> String {
    let shown = format_value(&WorkbookValue::Scalar(value), format, UiCulture::Russian);
    assert!(
        shown.diagnostic.is_none(),
        "{format}: {:?}",
        shown.diagnostic
    );
    shown.text
}

#[test]
fn duration_totals_do_not_wrap_at_day_boundaries() {
    for (input, format, expected) in [
        ("1.5", "[h]:mm:ss", "36:00:00"),
        ("1.5", "[mm]:ss", "2160:00"),
        ("1.5", "[s]", "129600"),
        ("-1.5", "[h]:mm", "-36:00"),
        ("0.0001", "[ss].000", "08.640"),
        ("1.500001", "[h]:mm:ss.0000", "36:00:00.0864"),
        ("0", "[hh]:mm:ss", "00:00:00"),
        ("0.00000000000001", "[s].000000000", "0.000000000"),
        ("1.99999999", "[h]:mm", "47:59"),
    ] {
        assert_eq!(
            display(CellValue::Decimal(input.parse().unwrap()), format),
            expected
        );
    }
    assert_eq!(display(CellValue::Real(1.5), "[H]:MM:SS"), "36:00:00");
    assert_eq!(display(CellValue::Real(1e-20), "[s]"), "0");
}

#[test]
fn duration_conversion_preserves_integer_precision_and_has_a_finite_budget() {
    assert_eq!(
        display(CellValue::Integer(9_007_199_254_740_993), "[h]"),
        "216172782113783832"
    );
    let value = WorkbookValue::Scalar(CellValue::Decimal(i128::MAX.to_string().parse().unwrap()));
    let shown = format_value(&value, "[h]", UiCulture::English);
    assert!(shown.diagnostic.is_some());
    assert_eq!(shown.text, value.display());
    for text in ["1e30", "1e300"] {
        let value = WorkbookValue::Scalar(CellValue::Real(text.parse().unwrap()));
        let shown = format_value(&value, "[s]", UiCulture::English);
        assert!(shown.diagnostic.is_some());
        assert_eq!(shown.text, value.display());
    }
}

#[test]
fn duration_sections_colors_and_quoted_units_preserve_source_values() {
    let value = WorkbookValue::Scalar(CellValue::Real(-2.5));
    let shown = format_value(&value, "[h]:mm;[Red]([h]:mm);\"нет\"", UiCulture::Russian);
    assert_eq!(shown.text, "(60:00)");
    assert_eq!(shown.foreground, Some(0xff0000));
    assert!(shown.diagnostic.is_none());
    assert_eq!(value, WorkbookValue::Scalar(CellValue::Real(-2.5)));
    assert_eq!(display(CellValue::Integer(0), "[h];[h];\"нет\""), "нет");
    assert_eq!(
        display(CellValue::Real(1.5), "[$-409][Blue]\"Spent: \"[h]\" h\""),
        "Spent: 36 h"
    );
    assert_eq!(display(CellValue::Integer(5), "0 \"[h]\""), "5 [h]");
}

#[test]
fn typed_clocks_keep_nanoseconds_and_dates_are_not_durations() {
    let time = CellValue::Time(TimeValue::new(16, 5, 8, 123_456_789).unwrap());
    assert_eq!(display(time.clone(), "[m]:ss.000"), "965:08.123");
    assert_eq!(display(time.clone(), "[s].000000000"), "57908.123456789");
    assert_eq!(display(time, "hh:mm:ss.000"), "16:05:08.123");
    let value = WorkbookValue::Scalar(CellValue::Date(DateValue::new(2026, 11, 5).unwrap()));
    let shown = format_value(&value, "[h]", UiCulture::Russian);
    assert!(shown.diagnostic.is_some());
    assert_eq!(shown.text, value.display());
}

#[test]
fn malformed_duration_formats_report_diagnostics_without_guessing() {
    for format in [
        "[h][m]",
        "[h]:mm:mm",
        "[m]:hh",
        "[s]:mm",
        "hh:[m]",
        "[hhh]",
        "[h].000",
        "[h]:ss.0000000000",
        "[h] AM/PM",
        "[h]:ss 0",
        "[h]:ss.00.0",
    ] {
        let value = WorkbookValue::Scalar(CellValue::Real(1.5));
        let shown = format_value(&value, format, UiCulture::Russian);
        assert!(shown.diagnostic.is_some(), "{format}");
        assert_eq!(shown.text, value.display());
    }
}

#[test]
fn duration_xlsx_roundtrip_keeps_numeric_days_and_formula_results() {
    let mut book = Workbook::new("Durations").unwrap();
    let address = CellAddress::new(0, 0).unwrap();
    let formula = CellAddress::new(1, 0).unwrap();
    let mut cell = WorkbookCell::value(CellValue::Real(1.5));
    cell.style.number_format = "[h]:mm:ss".into();
    let mut calculated = WorkbookCell::formula("=A1*2");
    calculated.style.number_format = "[h]:mm".into();
    book.apply(&[
        WorkbookEdit::Set {
            sheet: 0,
            address,
            cell,
        },
        WorkbookEdit::Set {
            sheet: 0,
            address: formula,
            cell: calculated,
        },
    ])
    .unwrap();
    let loaded = Workbook::from_xlsx(&book.to_xlsx().unwrap())
        .unwrap()
        .workbook;
    assert_eq!(
        loaded.value(0, address).unwrap(),
        book.value(0, address).unwrap()
    );
    assert_eq!(
        loaded.sheets()[0]
            .cell(address)
            .unwrap()
            .style
            .number_format,
        "[h]:mm:ss"
    );
    assert_eq!(
        rust_desktop_ui_controls::spreadsheet::format_cell(&loaded, 0, address, UiCulture::Russian)
            .unwrap()
            .text,
        "36:00:00"
    );
    assert_eq!(
        rust_desktop_ui_controls::spreadsheet::format_cell(&loaded, 0, formula, UiCulture::Russian)
            .unwrap()
            .text,
        "72:00"
    );
    let pages =
        rust_desktop_ui_controls::printing::from_workbook(&loaded, 0, Default::default(), || false)
            .unwrap();
    for expected in ["36:00:00", "72:00"] {
        assert!(
            pages.pages().iter().any(|page| page
                .scene
                .texts
                .iter()
                .any(|run| run.text == expected))
        );
    }
}
