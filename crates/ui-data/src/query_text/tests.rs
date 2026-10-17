use super::*;
fn schema() -> TableSchema {
    TableSchema::new(vec![
        FieldSchema::new("Name", FieldType::Text),
        FieldSchema::new("Count", FieldType::Integer),
        FieldSchema::new("Amount", FieldType::Decimal),
    ])
    .unwrap()
}
#[test]
fn exact_literal_round_trip_all_types_and_escaping() {
    let values = vec![
        CellValue::Null,
        CellValue::Bool(true),
        CellValue::Integer(i64::MIN),
        CellValue::Integer(i64::MAX),
        CellValue::Real(f64::from_bits(1)),
        CellValue::Real(-0.0),
        CellValue::Decimal("-170141183460469231731687303715884105728".parse().unwrap()),
        CellValue::Date("2000-02-29".parse().unwrap()),
        CellValue::Time("23:59:59.123456789".parse().unwrap()),
        CellValue::Binary(vec![0, 10, 255]),
        CellValue::Text("a\"b\\c\n\0é😀".into()),
    ];
    for v in values {
        let expression = QueryExpression::Literal(v);
        let text = format_expression_text(&expression).unwrap();
        assert_eq!(
            parse_expression_text(&schema(), &text).unwrap(),
            expression,
            "{text}"
        );
    }
}
#[test]
fn criteria_expression_round_trip_and_shared_type_diagnostics() {
    let text = r#"all(ge(add(field("Count"),integer("2")),integer("5")),any(icontains(lower(field("Name")),text("é")),not(is_null(field("Name")))))"#;
    let value = parse_criteria_text(&schema(), text).unwrap();
    assert_eq!(
        parse_criteria_text(&schema(), &format_criteria_text(&value).unwrap()).unwrap(),
        value
    );
    for invalid in [
        r#"eq(field("Missing"),null())"#,
        r#"eq(field("Count"),text("2"))"#,
        r#"eq(div(field("Amount"),decimal("2")),decimal("1"))"#,
        "all(true(),)",
        "true() garbage",
        "exec()",
    ] {
        assert!(
            parse_criteria_text(&schema(), invalid).is_err(),
            "{invalid}"
        );
    }
    assert!(parse_expression_text(&schema(), r#"real("NaN")"#).is_err());
    assert!(parse_expression_text(&schema(), r#"date("1900-02-29")"#).is_err());
}
#[test]
fn early_text_depth_node_and_formatter_limits() {
    assert!(parse_criteria_text(&schema(), &"x".repeat(MAX_QUERY_TEXT_BYTES + 1)).is_err());
    assert!(
        parse_criteria_text(
            &schema(),
            &(String::from("not(").repeat(18) + "true()" + &")".repeat(18))
        )
        .is_err()
    );
    assert!(
        parse_criteria_text(
            &schema(),
            &format!("all({})", vec!["true()"; 129].join(","))
        )
        .is_err()
    );
    assert!(
        format_expression_text(&QueryExpression::Literal(CellValue::Text(
            "x".repeat(16385)
        )))
        .is_err()
    );
    let mut value = Criteria::Constant(true);
    for _ in 0..18 {
        value = Criteria::Not(Box::new(value));
    }
    assert!(format_criteria_text(&value).is_err());
}
#[test]
fn unicode_escape_and_completion_are_bounded() {
    let value = parse_expression_text(&schema(), r#"text("\u{1F600}\n\"\\")"#).unwrap();
    assert_eq!(
        value,
        QueryExpression::Literal(CellValue::Text("😀\n\"\\".into()))
    );
    for text in [
        r#"text("\u{D800}")"#,
        r#"text("\u{110000}")"#,
        r#"text("\q")"#,
    ] {
        assert!(parse_expression_text(&schema(), text).is_err());
    }
    assert_eq!(
        query_text_completions(&schema(), "Nam"),
        vec!["field(\"Name\")"]
    );
    assert!(query_text_completions(&schema(), &"x".repeat(257)).is_empty());
}
