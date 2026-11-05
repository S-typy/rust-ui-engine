use rust_desktop_ui_controls::{UiCulture, spreadsheet::format_value};
use rust_desktop_ui_data::{CellValue, DecimalValue, workbook::WorkbookValue};

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
fn mixed_improper_fixed_and_sign_sections_preserve_exact_values() {
    for (value, code, expected) in [
        ("5.25", "# ?/?", "5 1/4"),
        ("5.3", "# ??/??", "5  3/10"),
        ("3.141592653589793", "# ???/???", "3  16/113"),
        ("1.875", "# ?/8", "1 7/8"),
        ("1.99", "# ?/8", "2    "),
        ("5.25", "??/??", "21/ 4"),
        ("0.5", "0/8", "4/8"),
        ("0", "0 ?/?", "0    "),
        ("0", "0/0", "0/1"),
        ("-5.25", "# ?/?;[Red](# ?/?)", "(5 1/4)"),
        ("0", "# ?/?;# ?/?;\"нет\"", "нет"),
    ] {
        assert_eq!(
            display(CellValue::Decimal(value.parse().unwrap()), code),
            expected,
            "{value} {code}"
        );
    }
    assert_eq!(
        display(CellValue::Integer(9_007_199_254_740_993), "0 0/8"),
        "9007199254740993 0/8"
    );
    assert_eq!(
        display(CellValue::Integer(9_007_199_254_740_993), "0/8"),
        "72057594037927944/8"
    );
    let result = format_value(
        &WorkbookValue::Scalar(CellValue::Real(-2.25)),
        "# ?/?;[Red](# ?/?)",
        UiCulture::English,
    );
    assert_eq!(result.foreground, Some(0xff0000));
    assert_eq!(result.text, "(2 1/4)");
    assert_eq!(
        display(
            CellValue::Decimal(DecimalValue::new(i128::MIN, 0).unwrap()),
            "0/8"
        ),
        "-1361129467683753853853498429727072845824/8"
    );
    assert_eq!(
        display(CellValue::Real(1e308), "0/8"),
        format!("8{}/8", "0".repeat(308))
    );
    assert_eq!(display(CellValue::Real(-0.0625), "0/8"), "-1/8");
}

#[test]
fn approximation_matches_independent_exhaustive_error_minimization() {
    // Every thousandth between0and1; exact integer error comparisons over all
    // candidates rather than duplicating the continued-fraction implementation.
    for (format, limit, step) in [("0/0", 9, 1), ("00/00", 99, 1), ("000/000", 999, 17)] {
        for n in (0..=1000i128).step_by(step) {
            let text = display(CellValue::Decimal(DecimalValue::new(n, 3).unwrap()), format);
            let (actual_p, actual_q) = text.split_once('/').unwrap();
            let actual = (
                actual_p.parse::<i128>().unwrap(),
                actual_q.parse::<i128>().unwrap(),
            );
            let mut best = (0i128, 1i128);
            for q in 1..=limit {
                for p in 0..=q {
                    let left = (n * q - p * 1000).abs() * best.1;
                    let right = (n * best.1 - best.0 * 1000).abs() * q;
                    if left < right
                        || (left == right && (q < best.1 || (q == best.1 && p > best.0)))
                    {
                        best = (p, q);
                    }
                }
            }
            assert_eq!(actual, best, "{n}/1000 -> {text}");
        }
    }
}

#[test]
fn xlsx_cells_formulas_accessibility_and_pages_share_fraction_presentation() {
    use rust_desktop_ui_controls::{
        Controls, Theme, printing::from_workbook, spreadsheet::Spreadsheet,
    };
    use rust_desktop_ui_core::{LayoutStyle, Length, Size};
    use rust_desktop_ui_data::workbook::*;
    let a = CellAddress::new(0, 0).unwrap();
    let b = CellAddress::new(1, 0).unwrap();
    let mut book = Workbook::new("Fractions").unwrap();
    let mut cell = WorkbookCell::value(CellValue::Real(5.25));
    cell.style.number_format = "# ?/?".into();
    let mut formula = WorkbookCell::formula("=A1*2");
    formula.style.number_format = "# ?/?".into();
    book.apply(&[
        WorkbookEdit::Set {
            sheet: 0,
            address: a,
            cell,
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
    assert_eq!(loaded.value(0, a).unwrap(), book.value(0, a).unwrap());
    assert_eq!(
        loaded.sheets()[0].cell(b).unwrap().input,
        book.sheets()[0].cell(b).unwrap().input
    );
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
    for (address, expected) in [(a, "5 1/4"), (b, "10 1/2")] {
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
}

#[test]
fn budgets_and_unsupported_masks_are_diagnostic() {
    for code in [
        "0 ?/1000000",
        "0 ????/????",
        "0 ?/1?",
        "0 ?/?%",
        "0 ?/?/0",
        "0 ?/",
        "0 0??/??",
        "0.0 ?/?",
        "0 ?/01",
    ] {
        let value = WorkbookValue::Scalar(CellValue::Real(5.25));
        let result = format_value(&value, code, UiCulture::Russian);
        assert!(result.diagnostic.is_some(), "{code}");
        assert_eq!(result.text, value.display());
    }
    assert_eq!(display(CellValue::Real(1e-30), "0/999999"), "0/999999");
    assert_eq!(display(CellValue::Real(1.0), "0/999999"), "999999/999999");
}
