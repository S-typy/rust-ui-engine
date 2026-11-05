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
        "[>=]0",
        "0 ????/????",
        "_(*0)",
        "0.00.0",
        "0.0#0",
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
#[test]
fn named_calendar_fields_are_culture_specific_and_keep_gregorian_dates() {
    let value = WorkbookValue::Scalar(CellValue::Date(DateValue::new(2026, 11, 4).unwrap()));
    for (culture, format, expected) in [
        (
            UiCulture::Russian,
            "dddd, d mmmm yyyy",
            "среда, 4 ноября 2026",
        ),
        (UiCulture::Russian, "ddd mmm mmmmm", "ср ноя н"),
        (UiCulture::Russian, "mmmm yyyy", "ноябрь 2026"),
        (
            UiCulture::English,
            "dddd, mmmm d, yyyy",
            "Wednesday, November 4, 2026",
        ),
        (UiCulture::English, "ddd mmm mmmmm", "Wed Nov N"),
        (
            UiCulture::Arabic,
            "dddd d mmmm yyyy",
            "الأربعاء 4 نوفمبر 2026",
        ),
        (
            UiCulture::Russian,
            "[$-409]dddd, mmmm d, yyyy",
            "Wednesday, November 4, 2026",
        ),
        (
            UiCulture::English,
            "[$-ru-RU]dddd, d mmmm yyyy",
            "среда, 4 ноября 2026",
        ),
    ] {
        let result = format_value(&value, format, culture);
        assert_eq!(result.text, expected, "{format}");
        assert!(result.diagnostic.is_none(), "{:?}", result.diagnostic);
    }
    for (date, expected) in [
        ("0001-01-01", "Monday"),
        ("1900-03-01", "Thursday"),
        ("2000-01-01", "Saturday"),
        ("2024-02-29", "Thursday"),
        ("9999-12-31", "Friday"),
    ] {
        assert_eq!(
            display(CellValue::Date(date.parse().unwrap()), "[$-en-US]dddd"),
            expected
        );
    }
}

#[test]
fn twelve_hour_fields_preserve_midnight_noon_case_and_quoted_markers() {
    for (hour, expected) in [
        (0, "12:05 AM"),
        (11, "11:05 AM"),
        (12, "12:05 PM"),
        (23, "11:05 PM"),
    ] {
        assert_eq!(
            display(
                CellValue::Time(TimeValue::new(hour, 5, 8, 0).unwrap()),
                "hh:mm AM/PM"
            ),
            expected
        );
    }
    let time = CellValue::Time(TimeValue::new(16, 5, 8, 123_456_789).unwrap());
    assert_eq!(display(time.clone(), "h:mm:ss.000 am/pm"), "4:05:08.123 pm");
    assert_eq!(display(time.clone(), "h:mm A/P"), "4:05 P");
    assert_eq!(display(time.clone(), "h:mm a/p"), "4:05 p");
    assert_eq!(display(time, "hh:mm \"AM/PM\""), "16:05 AM/PM");
}

#[test]
fn engineering_exponents_round_across_groups_without_losing_exact_values() {
    for (input, format, expected) in [
        ("12200000", "#0.0E+0", "12,2E+6"),
        ("12200000", "##0.00E+00", "12,20E+06"),
        ("999.995", "##0.00E+00", "1,00E+03"),
        ("99999.995", "##0.00E+00", "100,00E+03"),
        ("0.000999995", "##0.00E+00", "1,00E-03"),
        ("0.000123", "##0.0e+0", "123,0e-6"),
        ("-12345", "##0.00E+00", "-12,35E+03"),
        ("1", "000.00E+00", "001,00E+00"),
        ("0", "##0.00E+00", "0,00E+00"),
        (
            "9007199254740993",
            "##0.000000000000000E+00",
            "9,007199254740993E+15",
        ),
    ] {
        assert_eq!(
            display(CellValue::Decimal(input.parse().unwrap()), format),
            expected,
            "{input} {format}"
        );
    }
    for (culture, format, expected) in [
        (UiCulture::Russian, "[$-en-US]#,##0.00", "1,234.50"),
        (UiCulture::English, "[Red][$-0419]#,##0.00", "1\u{a0}234,50"),
        (
            UiCulture::Russian,
            "[$-401]#,##0.00",
            "1\u{66c}234\u{66b}50",
        ),
    ] {
        let result = format_value(
            &WorkbookValue::Scalar(CellValue::Real(1234.5)),
            format,
            culture,
        );
        assert_eq!(result.text, expected);
        assert!(result.diagnostic.is_none());
        assert_eq!(
            result.foreground,
            format.contains("Red").then_some(0xff0000)
        );
    }
}

#[test]
fn unsupported_format_modifiers_remain_explicit_and_never_change_the_value() {
    for format in [
        "[$-fr-FR]0",
        "[$-F800]0",
        "[$€-409]0",
        "[$-409][$-419]0",
        "[Red][Blue]0",
        "0#0.0E+0",
        "#######0E+0",
        "###.0E+0",
        "0.0E++0",
    ] {
        let value = WorkbookValue::Scalar(CellValue::Integer(25));
        let result = format_value(&value, format, UiCulture::Russian);
        assert!(result.diagnostic.is_some(), "{format}");
        assert_eq!(result.text, "25");
        assert_eq!(result.foreground, None);
        assert_eq!(value, WorkbookValue::Scalar(CellValue::Integer(25)));
    }
}
