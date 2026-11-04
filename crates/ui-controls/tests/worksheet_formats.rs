use rust_desktop_ui_controls::{UiCulture, spreadsheet::format_value};
use rust_desktop_ui_data::{
    CellValue, DateValue, DecimalValue, TimeValue, workbook::WorkbookValue,
};
fn display(value: CellValue, format: &str) -> String {
    let result = format_value(&WorkbookValue::Scalar(value), format, UiCulture::Russian);
    assert!(result.diagnostic.is_none(), "{:?}", result.diagnostic);
    result.text
}
#[test]
fn exact_values_never_pass_through_binary_float_for_formatting() {
    assert_eq!(
        display(CellValue::Integer(9_007_199_254_740_993), "#,##0.00"),
        "9\u{a0}007\u{a0}199\u{a0}254\u{a0}740\u{a0}993,00"
    );
    let value = DecimalValue::new(i128::MIN, 0).unwrap();
    assert_eq!(
        display(CellValue::Decimal(value), "0.00"),
        format!("{},00", i128::MIN)
    );
    for (input, format, expected) in [
        ("9.995", "0.00", "10,00"),
        ("-0.005", "0.00", "-0,01"),
        ("0.0001", "0.00", "0,00"),
        ("0.125", "0.0%", "12,5%"),
        ("1234567", "0,,", "1"),
        ("2.5000", "0.###", "2,5"),
        ("9.995", "0.00E+00", "1,00E+01"),
        ("0.00125", "0.00E+00", "1,25E-03"),
        ("0", "0.00E+00", "0,00E+00"),
    ] {
        assert_eq!(
            display(CellValue::Decimal(input.parse().unwrap()), format),
            expected,
            "{input} {format}"
        );
    }
}
#[test]
fn sections_colors_quoted_literals_and_text_are_independent_of_storage() {
    let value = WorkbookValue::Scalar(CellValue::Integer(-25));
    let result = format_value(
        &value,
        "0.00;[Red](0.00);\"нет\";\"Текст: \"@",
        UiCulture::Russian,
    );
    assert_eq!(result.text, "(25,00)");
    assert_eq!(result.foreground, Some(0xff0000));
    assert!(result.diagnostic.is_none());
    assert_eq!(display(CellValue::Integer(0), "0;0;\"нет\""), "нет");
    assert_eq!(
        display(CellValue::Text("A;B".into()), "0;0;0;\"Текст: \"@"),
        "Текст: A;B"
    );
    assert_eq!(
        display(CellValue::Integer(7), "\"id;\"000\" ₽\""),
        "id;007 ₽"
    );
    assert_eq!(display(CellValue::Integer(7), ";;;"), "");
    let result = format_value(
        &WorkbookValue::Scalar(CellValue::Real(1234.5)),
        "#,##0.00",
        UiCulture::English,
    );
    assert_eq!(result.text, "1,234.50");
}
#[test]
fn typed_dates_times_and_format_failures_are_explicit() {
    let date = DateValue::new(2026, 11, 4).unwrap();
    assert_eq!(display(CellValue::Date(date), "dd.mm.yyyy"), "04.11.2026");
    assert_eq!(display(CellValue::Date(date), "yyyy-mm-dd"), "2026-11-04");
    let time = TimeValue::new(16, 5, 8, 123_456_789).unwrap();
    assert_eq!(
        display(CellValue::Time(time), "hh:mm:ss.000"),
        "16:05:08.123"
    );
    assert_eq!(display(CellValue::Time(time), "hh:mm"), "16:05");
    for format in [
        "[>=1]0",
        "0 ?/?",
        "_(*0)",
        "0.00.0",
        "0.0#0",
        "##0.0E+0",
        "\"unclosed",
    ] {
        let result = format_value(
            &WorkbookValue::Scalar(CellValue::Integer(25)),
            format,
            UiCulture::Russian,
        );
        assert!(result.diagnostic.is_some(), "{format}");
        assert_eq!(result.text, "25");
    }
}
