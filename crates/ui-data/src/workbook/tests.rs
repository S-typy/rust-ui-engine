use super::*;
fn a(text: &str) -> CellAddress {
    CellAddress::parse(text).unwrap()
}
fn set(sheet: usize, address: &str, value: CellValue) -> WorkbookEdit {
    WorkbookEdit::Set {
        sheet,
        address: a(address),
        cell: WorkbookCell::value(value),
    }
}
fn formula(sheet: usize, address: &str, text: &str) -> WorkbookEdit {
    WorkbookEdit::Set {
        sheet,
        address: a(address),
        cell: WorkbookCell::formula(text),
    }
}
fn number(book: &Workbook, sheet: usize, address: &str) -> f64 {
    match book.value(sheet, a(address)).unwrap() {
        WorkbookValue::Scalar(CellValue::Real(n)) => n,
        WorkbookValue::Scalar(CellValue::Integer(n)) => n as f64,
        v => panic!("{v:?}"),
    }
}

#[test]
fn addresses_cover_boundaries_and_reject_aliases() {
    for (text, row, col) in [
        ("A1", 0, 0),
        ("Z9", 8, 25),
        ("AA10", 9, 26),
        ("XFD1048576", MAX_ROWS - 1, MAX_COLUMNS - 1),
    ] {
        let address = a(text);
        assert_eq!((address.row(), address.column()), (row, col));
        assert_eq!(address.a1(), text);
    }
    for text in [
        "A0", "A01", "1A", "A-1", "A1048577", "XFE1", "AAAA1", "$A1", "é1",
    ] {
        assert!(CellAddress::parse(text).is_err(), "{text}");
    }
    assert!(CellRange::new(a("B2"), a("A1")).is_err());
    assert!(CellRange::new(a("A1"), a("XFD2")).is_err());
}
#[test]
fn formula_dependencies_recalculate_across_sheets_and_clear() {
    let mut book = Workbook::new("Input").unwrap();
    book.apply(&[
        WorkbookEdit::AddSheet {
            name: "Итог".into(),
        },
        set(0, "A1", CellValue::Integer(7)),
        set(0, "A2", CellValue::Integer(3)),
        formula(1, "A1", "=SUM(Input!A1:A2)*2"),
        formula(1, "B1", "=A1+1"),
    ])
    .unwrap();
    assert_eq!(number(&book, 1, "B1"), 21.0);
    assert_eq!(book.dependencies(1, a("A1")).unwrap().len(), 2);
    book.apply(&[set(0, "A1", CellValue::Integer(9))]).unwrap();
    assert_eq!(number(&book, 1, "B1"), 25.0);
    book.apply(&[WorkbookEdit::Clear {
        sheet: 0,
        range: CellRange::new(a("A2"), a("A2")).unwrap(),
    }])
    .unwrap();
    assert_eq!(number(&book, 1, "B1"), 19.0);
}
#[test]
fn invalid_batch_is_atomic_including_sheet_creation() {
    let mut book = Workbook::new("Data").unwrap();
    let old = book.clone();
    assert!(
        book.apply(&[
            WorkbookEdit::AddSheet {
                name: "Other".into()
            },
            set(1, "A1", CellValue::Integer(5)),
            formula(1, "B1", "=SUM(")
        ])
        .is_err()
    );
    assert_eq!(book, old);
    assert!(
        book.apply(&[WorkbookEdit::AddSheet {
            name: "data".into()
        }])
        .is_err()
    );
    assert_eq!(book, old);
    assert!(
        book.apply(&[set(0, "A1", CellValue::Real(f64::NAN))])
            .is_err()
    );
    assert_eq!(book, old);
}
#[test]
fn cycles_errors_and_lazy_if_are_defined() {
    let mut book = Workbook::new("Data").unwrap();
    book.apply(&[
        formula(0, "A1", "=B1"),
        formula(0, "B1", "=A1"),
        formula(0, "C1", "=IF(TRUE,12,1/0)"),
        formula(0, "D1", "=1/0"),
        formula(0, "E1", "=UNKNOWN(1)"),
        formula(0, "F1", "='Missing'!A1"),
    ])
    .unwrap();
    for cell in ["A1", "B1"] {
        assert_eq!(
            book.value(0, a(cell)).unwrap(),
            WorkbookValue::Error(FormulaError::Cycle)
        );
    }
    assert_eq!(number(&book, 0, "C1"), 12.0);
    assert_eq!(
        book.value(0, a("D1")).unwrap(),
        WorkbookValue::Error(FormulaError::DivisionByZero)
    );
    assert_eq!(
        book.value(0, a("E1")).unwrap(),
        WorkbookValue::Error(FormulaError::Name)
    );
    assert_eq!(
        book.value(0, a("F1")).unwrap(),
        WorkbookValue::Error(FormulaError::Reference)
    );
    book.apply(&[set(0, "B1", CellValue::Integer(4))]).unwrap();
    assert_eq!(number(&book, 0, "A1"), 4.0);
}
#[test]
fn copy_range_preserves_absolute_axes_and_snapshot_overlap() {
    let mut book = Workbook::new("Data").unwrap();
    book.apply(&[
        set(0, "A1", CellValue::Integer(2)),
        set(0, "A2", CellValue::Integer(9)),
        formula(0, "B1", "=$A1+A$1+$A$1"),
    ])
    .unwrap();
    book.apply(&[WorkbookEdit::Copy {
        sheet: 0,
        source: CellRange::new(a("B1"), a("B1")).unwrap(),
        destination_sheet: 0,
        destination: a("B2"),
    }])
    .unwrap();
    assert_eq!(number(&book, 0, "B2"), 13.0);
    book.apply(&[WorkbookEdit::Copy {
        sheet: 0,
        source: CellRange::new(a("A1"), a("A2")).unwrap(),
        destination_sheet: 0,
        destination: a("A2"),
    }])
    .unwrap();
    assert_eq!(number(&book, 0, "A2"), 2.0);
    assert_eq!(number(&book, 0, "A3"), 9.0);
    let old = book.clone();
    assert!(
        book.apply(&[WorkbookEdit::Copy {
            sheet: 0,
            source: CellRange::new(a("A1"), a("A2")).unwrap(),
            destination_sheet: 0,
            destination: a("A1048576")
        }])
        .is_err()
    );
    assert_eq!(book, old);
}
#[test]
fn copy_past_origin_produces_reference_error() {
    let mut book = Workbook::new("Data").unwrap();
    book.apply(&[formula(0, "B1", "=A1")]).unwrap();
    book.apply(&[WorkbookEdit::Copy {
        sheet: 0,
        source: CellRange::new(a("B1"), a("B1")).unwrap(),
        destination_sheet: 0,
        destination: a("A1"),
    }])
    .unwrap();
    assert_eq!(
        book.value(0, a("A1")).unwrap(),
        WorkbookValue::Error(FormulaError::Reference)
    );
}
#[test]
fn aggregates_comparison_percent_and_rounding() {
    let mut book = Workbook::new("Data").unwrap();
    book.apply(&[
        set(0, "A1", CellValue::Integer(2)),
        set(0, "A2", CellValue::Text("heading".into())),
        set(0, "A3", CellValue::Integer(8)),
        formula(0, "B1", "=AVERAGE(A1:A3)"),
        formula(0, "B2", "=ROUND(12.345,2)"),
        formula(0, "B3", "=IF(MAX(A1:A3)>7,50%,0)"),
        formula(0, "B4", "=COUNT(A1:A3)"),
        formula(0, "B5", "=\"a\"&\"b\""),
    ])
    .unwrap();
    assert_eq!(number(&book, 0, "B1"), 5.0);
    assert_eq!(number(&book, 0, "B2"), 12.35);
    assert_eq!(number(&book, 0, "B3"), 0.5);
    assert_eq!(number(&book, 0, "B4"), 2.0);
    assert_eq!(
        book.value(0, a("B5")).unwrap(),
        WorkbookValue::Scalar(CellValue::Text("ab".into()))
    );
}
#[test]
fn formulas_are_bounded_and_do_not_execute_external_sources() {
    let mut book = Workbook::new("Data").unwrap();
    for text in [
        format!("={}1{}", "(".repeat(200), ")".repeat(200)),
        "=SUM(A1:XFD2)".to_owned(),
        "=WEBSERVICE(\"https://example.invalid\")".into(),
    ] {
        let result = book.apply(&[formula(0, "A1", &text)]);
        if result.is_ok() {
            assert_eq!(
                book.value(0, a("A1")).unwrap(),
                WorkbookValue::Error(FormulaError::Name)
            );
        }
    }
    assert!(
        book.apply(&[formula(0, "B1", "=[file.xlsx]Sheet!A1")])
            .is_err()
    );
}

#[test]
fn ast_and_cross_cell_evaluation_depth_are_bounded() {
    let mut book = Workbook::new("Data").unwrap();
    let too_deep = format!("=A2{}", "+1".repeat(200));
    assert!(book.apply(&[formula(0, "A1", &too_deep)]).is_err());
    let edits: Vec<_> = (1..=140)
        .map(|row| formula(0, &format!("A{row}"), &format!("=A{}+1", row + 1)))
        .collect();
    book.apply(&edits).unwrap();
    assert_eq!(
        book.value(0, a("A1")).unwrap(),
        WorkbookValue::Error(FormulaError::Limit)
    );
}

#[test]
fn xlsx_literal_escape_sequences_are_not_interpreted_as_characters() {
    let mut book = Workbook::new("Data").unwrap();
    let text = "_x0041_ and _xD83E__xDD80_";
    book.apply(&[set(0, "A1", CellValue::Text(text.into()))])
        .unwrap();
    let imported = Workbook::from_xlsx(&book.to_xlsx().unwrap()).unwrap();
    assert_eq!(
        imported.workbook.value(0, a("A1")).unwrap(),
        WorkbookValue::Scalar(CellValue::Text(text.into()))
    );
}
#[test]
fn xlsx_roundtrip_preserves_values_formulas_and_supported_styles() {
    let mut book = Workbook::new("Данные & план").unwrap();
    let mut styled = WorkbookCell::value(CellValue::Text(" Привет <мир> 🦀 ".into()));
    styled.style = CellStyle {
        bold: true,
        italic: true,
        foreground: Some(0x1266aa),
        background: Some(0xfafafa),
        wrap: true,
        number_format: "@".into(),
    };
    book.apply(&[
        WorkbookEdit::Set {
            sheet: 0,
            address: a("A1"),
            cell: styled.clone(),
        },
        set(0, "B1", CellValue::Integer(15)),
        set(0, "C1", CellValue::Bool(true)),
        set(
            0,
            "D1",
            CellValue::Date(crate::DateValue::new(2026, 10, 18).unwrap()),
        ),
        set(
            0,
            "E1",
            CellValue::Time(crate::TimeValue::new(12, 34, 56, 123000000).unwrap()),
        ),
        formula(0, "B2", "=B1*2"),
        formula(0, "C2", "=\"answer\""),
    ])
    .unwrap();
    let bytes = book.to_xlsx().unwrap();
    assert!(bytes.starts_with(b"PK"));
    let imported = Workbook::from_xlsx(&bytes).unwrap();
    assert!(
        imported.diagnostics.is_empty(),
        "{:?}",
        imported.diagnostics
    );
    assert_eq!(
        imported.workbook.sheets()[0].name(),
        book.sheets()[0].name()
    );
    assert_eq!(imported.workbook.sheets()[0].cell(a("A1")), Some(&styled));
    for address in ["A1", "B1", "C1", "D1", "E1", "B2", "C2"] {
        assert_eq!(
            book.value(0, a(address)).unwrap(),
            imported.workbook.value(0, a(address)).unwrap(),
            "{address}"
        );
    }
}
#[test]
fn xlsx_rejects_precision_loss_and_dangerous_packages() {
    let mut book = Workbook::new("Data").unwrap();
    book.apply(&[set(0, "A1", CellValue::Integer(i64::MAX))])
        .unwrap();
    assert!(book.to_xlsx().is_err());
    assert!(Workbook::from_xlsx(b"not a zip").is_err());
    let bytes = xlsx::test_package(&[("../escape.xml", "<x/>")]);
    assert!(Workbook::from_xlsx(&bytes).is_err());
    let bytes = xlsx::test_package(&[(
        "[Content_Types].xml",
        "<!DOCTYPE x [<!ENTITY z SYSTEM 'file:///C:/x'>]><x>&z;</x>",
    )]);
    assert!(Workbook::from_xlsx(&bytes).is_err());
}

#[test]
fn xlsx_preserves_control_whitespace_in_text_formulas_and_style_attributes() {
    let mut book = Workbook::new("Data").unwrap();
    let text = "a\rb\r\nc\td\ne";
    let styled = WorkbookCell {
        input: CellInput::Value(CellValue::Text(text.into())),
        style: CellStyle {
            number_format: "0\r\n\t\" label\"".into(),
            ..CellStyle::default()
        },
    };
    book.apply(&[
        WorkbookEdit::Set {
            sheet: 0,
            address: a("A1"),
            cell: styled.clone(),
        },
        formula(0, "A2", &format!("=\"{text}\"")),
    ])
    .unwrap();
    let imported = Workbook::from_xlsx(&book.to_xlsx().unwrap()).unwrap();
    assert!(
        imported.diagnostics.is_empty(),
        "{:?}",
        imported.diagnostics
    );
    assert_eq!(imported.workbook.sheets()[0].cell(a("A1")), Some(&styled));
    for address in ["A1", "A2"] {
        assert_eq!(
            imported.workbook.sheets()[0].cell(a(address)),
            book.sheets()[0].cell(a(address))
        );
        assert_eq!(
            imported.workbook.value(0, a(address)).unwrap(),
            book.value(0, a(address)).unwrap()
        );
    }
}

#[test]
fn exhausted_recalculation_publishes_errors_instead_of_empty_cells() {
    let expression = format!("=SUM({})", vec!["1"; 200].join(","));
    let mut book = Workbook::new("Data").unwrap();
    let mut edits: Vec<_> = (1..=10_000)
        .map(|row| formula(0, &format!("A{row}"), &expression))
        .collect();
    edits.push(set(0, "A10001", CellValue::Text("literal survives".into())));
    book.apply(&edits).unwrap();
    assert_eq!(number(&book, 0, "A1"), 200.0);
    assert_eq!(
        book.value(0, a("A10000")).unwrap(),
        WorkbookValue::Error(FormulaError::Limit)
    );
    assert_eq!(
        book.value(0, a("A10001")).unwrap(),
        WorkbookValue::Scalar(CellValue::Text("literal survives".into()))
    );
    assert!(
        !book
            .values
            .values()
            .any(|v| matches!(v, WorkbookValue::Scalar(CellValue::Null)))
    );
    assert_eq!(book.values.len(), 10_001);
    let schema = TableSchema::new(vec![crate::FieldSchema::new(
        "value",
        crate::FieldType::Real,
    )])
    .unwrap();
    assert!(
        book.to_table(0, CellRange::new(a("A10000"), a("A10000")).unwrap(), schema)
            .is_err()
    );
}

#[test]
fn repeated_cached_text_ranges_consume_the_evaluation_budget() {
    let mut book = Workbook::new("Data").unwrap();
    let mut edits: Vec<_> = (1..=16_369)
        .map(|row| set(0, &format!("A{row}"), CellValue::Text("x".into())))
        .collect();
    edits.push(formula(
        0,
        "B1",
        &format!("=COUNT({})", vec!["A1:A16369"; 127].join(",")),
    ));
    book.apply(&edits).unwrap();
    assert_eq!(
        book.value(0, a("B1")).unwrap(),
        WorkbookValue::Error(FormulaError::Limit)
    );
    assert_eq!(
        book.value(0, a("A16369")).unwrap(),
        WorkbookValue::Scalar(CellValue::Text("x".into()))
    );
}

#[test]
fn malformed_nested_inline_text_rejects_instead_of_losing_a_suffix() {
    // Exercise the same reader as inlineStr and shared-string runs through
    // a complete package, with no alternate cell-value representation.
    let book = Workbook::new("Data").unwrap();
    let bytes = book.to_xlsx().unwrap();
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut parts = Vec::new();
    for index in 0..zip.len() {
        use std::io::Read;
        let mut file = zip.by_index(index).unwrap();
        let mut value = String::new();
        file.read_to_string(&mut value).unwrap();
        if file.name() == "xl/worksheets/sheet1.xml" {
            value = "<worksheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"><sheetData><row r=\"1\"><c r=\"A1\" t=\"inlineStr\"><is><t>keep<bad/>lost</t></is></c></row></sheetData></worksheet>".into();
        }
        parts.push((file.name().to_owned(), value));
    }
    let refs: Vec<_> = parts
        .iter()
        .map(|(name, value)| (name.as_str(), value.as_str()))
        .collect();
    assert!(Workbook::from_xlsx(&xlsx::test_package(&refs)).is_err());
}

#[test]
fn portable_error_literals_roundtrip_and_internal_errors_require_explicit_conversion() {
    let mut book = Workbook::new("Data").unwrap();
    book.apply(&[WorkbookEdit::Set {
        sheet: 0,
        address: a("A1"),
        cell: WorkbookCell {
            input: CellInput::Error(FormulaError::DivisionByZero),
            style: CellStyle::default(),
        },
    }])
    .unwrap();
    let imported = Workbook::from_xlsx(&book.to_xlsx().unwrap()).unwrap();
    assert_eq!(
        book.value(0, a("A1")).unwrap(),
        imported.workbook.value(0, a("A1")).unwrap()
    );
    book.apply(&[WorkbookEdit::Set {
        sheet: 0,
        address: a("A1"),
        cell: WorkbookCell {
            input: CellInput::Error(FormulaError::Cycle),
            style: CellStyle::default(),
        },
    }])
    .unwrap();
    assert!(book.to_xlsx().is_err());
}
#[test]
fn workbook_query_bridge_uses_schema_and_row_addresses() {
    let mut book = Workbook::new("Data").unwrap();
    book.apply(&[
        set(0, "A3", CellValue::Text("first".into())),
        set(0, "B3", CellValue::Integer(7)),
        set(0, "A4", CellValue::Text("second".into())),
        set(0, "B4", CellValue::Integer(9)),
    ])
    .unwrap();
    let schema = TableSchema::new(vec![
        crate::FieldSchema::new("name", crate::FieldType::Text),
        crate::FieldSchema::new("count", crate::FieldType::Integer),
    ])
    .unwrap();
    let table = book
        .to_table(0, CellRange::new(a("A3"), a("B4")).unwrap(), schema)
        .unwrap();
    assert_eq!(table.rows()[0].key, 2);
    assert_eq!(table.value(3, "count"), Some(&CellValue::Integer(9)));
}
