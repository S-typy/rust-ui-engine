use super::*;

fn field(name: &str) -> QueryExpression {
    QueryExpression::field(name)
}
fn integer(value: i64) -> QueryExpression {
    QueryExpression::literal(CellValue::Integer(value))
}
fn text(value: &str) -> QueryExpression {
    QueryExpression::literal(CellValue::Text(value.into()))
}
fn compare(
    left: QueryExpression,
    operator: ComparisonOperator,
    right: QueryExpression,
) -> Criteria {
    Criteria::Compare {
        left,
        operator,
        right,
    }
}
fn arithmetic(
    left: QueryExpression,
    operator: ArithmeticOperator,
    right: QueryExpression,
) -> QueryExpression {
    QueryExpression::Arithmetic {
        left: Box::new(left),
        operator,
        right: Box::new(right),
    }
}
fn aggregate(function: AggregateFunction, name: &str) -> Aggregate {
    Aggregate {
        function,
        expression: Some(field(name)),
    }
}
fn count() -> Aggregate {
    Aggregate {
        function: AggregateFunction::CountRows,
        expression: None,
    }
}

fn table() -> DataTable {
    let mut fields = vec![
        FieldSchema::new("Category", FieldType::Text),
        FieldSchema::new("Amount", FieldType::Integer),
        FieldSchema::new("Price", FieldType::Decimal),
        FieldSchema::new("Ratio", FieldType::Real),
        FieldSchema::new("Active", FieldType::Bool),
        FieldSchema::new("Date", FieldType::Date),
        FieldSchema::new("Time", FieldType::Time),
        FieldSchema::new("Bytes", FieldType::Binary),
    ];
    fields[0].nullable = true;
    fields[1].nullable = true;
    let schema = TableSchema::new(fields).unwrap();
    let row = |key,
               category: Option<&str>,
               amount: Option<i64>,
               price: &str,
               ratio,
               active,
               date: &str,
               time: &str,
               bytes: Vec<u8>| {
        DataRow::new(
            key,
            vec![
                category
                    .map(|value| CellValue::Text(value.into()))
                    .unwrap_or_default(),
                amount.map(CellValue::Integer).unwrap_or_default(),
                CellValue::Decimal(price.parse().unwrap()),
                CellValue::Real(ratio),
                CellValue::Bool(active),
                CellValue::Date(date.parse().unwrap()),
                CellValue::Time(time.parse().unwrap()),
                CellValue::Binary(bytes),
            ],
        )
    };
    DataTable::new(
        schema,
        vec![
            row(
                30,
                Some("Бета"),
                Some(12),
                "1.25",
                1.5,
                true,
                "2026-10-16",
                "10:30",
                vec![255],
            ),
            row(
                -4,
                Some("АЛЬФА"),
                Some(8),
                "2.75",
                -0.0,
                false,
                "0001-01-01",
                "00:00",
                vec![],
            ),
            row(
                i64::MAX,
                None,
                None,
                "-1",
                0.0,
                true,
                "9999-12-31",
                "23:59:59.999999999",
                vec![0, 255],
            ),
            row(
                9,
                Some("альфа"),
                Some(8),
                "0.1",
                2.5,
                false,
                "2000-02-29",
                "12:45",
                vec![0],
            ),
        ],
    )
    .unwrap()
}

#[test]
fn typed_query_orders_all_scalars_and_ties_by_signed_key() {
    let table = table();
    assert_eq!(
        QueryEngine::execute(&table, &QueryRequest::default())
            .unwrap()
            .row_keys,
        vec![-4, 9, 30, i64::MAX]
    );
    let expected = [
        ("Category", vec![i64::MAX, -4, 30, 9]),
        ("Amount", vec![i64::MAX, -4, 9, 30]),
        ("Price", vec![i64::MAX, 9, 30, -4]),
        ("Ratio", vec![-4, i64::MAX, 30, 9]),
        ("Active", vec![-4, 9, 30, i64::MAX]),
        ("Date", vec![-4, 9, 30, i64::MAX]),
        ("Time", vec![-4, 30, 9, i64::MAX]),
        ("Bytes", vec![-4, 9, i64::MAX, 30]),
    ];
    for (name, expected) in expected {
        let request = QueryRequest {
            order_by: vec![QueryOrder {
                expression: field(name),
                direction: SortDirection::Ascending,
            }],
            ..Default::default()
        };
        assert_eq!(
            QueryEngine::execute(&table, &request).unwrap().row_keys,
            expected,
            "{name}"
        );
    }
    let request = QueryRequest {
        order_by: vec![QueryOrder {
            expression: field("Amount"),
            direction: SortDirection::Descending,
        }],
        ..Default::default()
    };
    assert_eq!(
        QueryEngine::execute(&table, &request).unwrap().row_keys,
        vec![30, -4, 9, i64::MAX]
    );
    let mut reordered = table.rows().to_vec();
    reordered.reverse();
    let reordered = DataTable::new(table.schema().clone(), reordered).unwrap();
    assert_eq!(
        QueryEngine::execute(&table, &request).unwrap(),
        QueryEngine::execute(&reordered, &request).unwrap()
    );
}

#[test]
fn three_valued_logic_and_null_predicates_do_not_conflate_empty_values() {
    let table = table();
    let unknown = compare(
        field("Amount"),
        ComparisonOperator::Equal,
        QueryExpression::literal(CellValue::Null),
    );
    let values = [
        (Criteria::Constant(false), TruthValue::False),
        (Criteria::Constant(true), TruthValue::True),
        (unknown.clone(), TruthValue::Unknown),
    ];
    for (left, a) in &values {
        for (right, b) in &values {
            let expected_and = if *a == TruthValue::False || *b == TruthValue::False {
                TruthValue::False
            } else if *a == TruthValue::Unknown || *b == TruthValue::Unknown {
                TruthValue::Unknown
            } else {
                TruthValue::True
            };
            let expected_or = if *a == TruthValue::True || *b == TruthValue::True {
                TruthValue::True
            } else if *a == TruthValue::Unknown || *b == TruthValue::Unknown {
                TruthValue::Unknown
            } else {
                TruthValue::False
            };
            assert_eq!(
                QueryEngine::evaluate_criteria(
                    &table,
                    30,
                    &Criteria::All(vec![left.clone(), right.clone()])
                )
                .unwrap(),
                expected_and
            );
            assert_eq!(
                QueryEngine::evaluate_criteria(
                    &table,
                    30,
                    &Criteria::Any(vec![left.clone(), right.clone()])
                )
                .unwrap(),
                expected_or
            );
        }
    }
    assert_eq!(
        QueryEngine::evaluate_criteria(&table, 30, &Criteria::Not(Box::new(unknown.clone())))
            .unwrap(),
        TruthValue::Unknown
    );
    assert!(
        QueryEngine::execute(
            &table,
            &QueryRequest {
                filter: Some(unknown),
                ..Default::default()
            }
        )
        .unwrap()
        .row_keys
        .is_empty()
    );
    assert_eq!(
        QueryEngine::execute(
            &table,
            &QueryRequest {
                filter: Some(Criteria::IsNull(field("Amount"))),
                ..Default::default()
            }
        )
        .unwrap()
        .row_keys,
        vec![i64::MAX]
    );
    assert_eq!(
        QueryEngine::evaluate_criteria(&table, 30, &Criteria::All(vec![])).unwrap(),
        TruthValue::True
    );
    assert_eq!(
        QueryEngine::evaluate_criteria(&table, 30, &Criteria::Any(vec![])).unwrap(),
        TruthValue::False
    );
}

#[test]
fn expression_types_text_functions_checked_arithmetic_and_coalesce() {
    let table = table();
    let expression = arithmetic(field("Amount"), ArithmeticOperator::Multiply, integer(3));
    assert_eq!(
        QueryEngine::evaluate_expression(&table, 30, &expression).unwrap(),
        CellValue::Integer(36)
    );
    assert_eq!(
        QueryEngine::evaluate_expression(&table, i64::MAX, &expression).unwrap(),
        CellValue::Null
    );
    let fallback = QueryExpression::Coalesce(Box::new(expression), Box::new(integer(0)));
    assert_eq!(
        QueryEngine::expression_type(table.schema(), &fallback).unwrap(),
        ExpressionType {
            kind: Some(FieldType::Integer),
            nullable: false
        }
    );
    assert_eq!(
        QueryEngine::evaluate_expression(&table, i64::MAX, &fallback).unwrap(),
        CellValue::Integer(0)
    );
    assert_eq!(
        QueryEngine::evaluate_expression(
            &table,
            -4,
            &QueryExpression::Lowercase(Box::new(field("Category")))
        )
        .unwrap(),
        CellValue::Text("альфа".into())
    );
    assert_eq!(
        QueryEngine::evaluate_expression(
            &table,
            -4,
            &QueryExpression::TextLength(Box::new(text("a🙂я")))
        )
        .unwrap(),
        CellValue::Integer(3)
    );
    let invalid = arithmetic(integer(1), ArithmeticOperator::Divide, integer(0));
    assert!(matches!(
        QueryEngine::evaluate_expression(&table, 30, &invalid),
        Err(DataError::Validation(_))
    ));
    // Coalesce and boolean criteria short-circuit execution, but compile every branch.
    assert_eq!(
        QueryEngine::evaluate_expression(
            &table,
            30,
            &QueryExpression::Coalesce(Box::new(integer(7)), Box::new(invalid.clone()))
        )
        .unwrap(),
        CellValue::Integer(7)
    );
    let invalid_predicate = compare(invalid, ComparisonOperator::Equal, integer(0));
    assert_eq!(
        QueryEngine::evaluate_criteria(
            &table,
            30,
            &Criteria::All(vec![Criteria::Constant(false), invalid_predicate])
        )
        .unwrap(),
        TruthValue::False
    );
    let mixed = arithmetic(
        integer(1),
        ArithmeticOperator::Add,
        QueryExpression::literal(CellValue::Real(1.0)),
    );
    assert!(matches!(
        QueryEngine::expression_type(table.schema(), &mixed),
        Err(DataError::Validation(_))
    ));
    let overflow = arithmetic(integer(i64::MAX), ArithmeticOperator::Add, integer(1));
    assert!(QueryEngine::evaluate_expression(&table, 30, &overflow).is_err());
    let underflow = arithmetic(integer(i64::MIN), ArithmeticOperator::Divide, integer(-1));
    assert!(QueryEngine::evaluate_expression(&table, 30, &underflow).is_err());
    let non_finite = arithmetic(
        QueryExpression::literal(CellValue::Real(f64::MAX)),
        ArithmeticOperator::Multiply,
        QueryExpression::literal(CellValue::Real(2.0)),
    );
    assert!(QueryEngine::evaluate_expression(&table, 30, &non_finite).is_err());
}

#[test]
fn schema_and_resource_validation_happens_before_rows_or_short_circuits() {
    let populated = table();
    let empty = DataTable::new(populated.schema().clone(), vec![]).unwrap();
    for table in [&empty, &populated] {
        let request = QueryRequest {
            filter: Some(Criteria::Any(vec![
                Criteria::Constant(true),
                Criteria::IsNull(field("missing")),
            ])),
            ..Default::default()
        };
        assert_eq!(
            QueryEngine::execute(table, &request),
            Err(DataError::UnknownField("missing".into()))
        );
        let request = QueryRequest {
            filter: Some(compare(
                field("Amount"),
                ComparisonOperator::Equal,
                text("8"),
            )),
            ..Default::default()
        };
        assert!(matches!(
            QueryEngine::execute(table, &request),
            Err(DataError::Validation(_))
        ));
        assert!(
            QueryEngine::evaluate_expression(
                table,
                30,
                &QueryExpression::literal(CellValue::Real(f64::NAN))
            )
            .is_err()
        );
        for aggregate in [
            Aggregate {
                function: AggregateFunction::Sum,
                expression: None,
            },
            Aggregate {
                function: AggregateFunction::CountRows,
                expression: Some(field("Amount")),
            },
            aggregate(AggregateFunction::Sum, "Category"),
        ] {
            assert!(
                QueryEngine::execute(
                    table,
                    &QueryRequest {
                        aggregates: vec![aggregate],
                        ..Default::default()
                    }
                )
                .is_err()
            );
        }
    }
    let mut deep = Criteria::Constant(true);
    for _ in 0..MAX_QUERY_DEPTH {
        deep = Criteria::Not(Box::new(deep));
    }
    assert!(matches!(
        QueryEngine::execute(
            &empty,
            &QueryRequest {
                filter: Some(deep),
                ..Default::default()
            }
        ),
        Err(DataError::Capacity(_))
    ));
    assert!(matches!(
        QueryEngine::execute(
            &empty,
            &QueryRequest {
                filter: Some(Criteria::All(vec![
                    Criteria::Constant(true);
                    MAX_QUERY_NODES
                ])),
                ..Default::default()
            }
        ),
        Err(DataError::Capacity(_))
    ));
    assert!(matches!(
        QueryEngine::expression_type(empty.schema(), &text(&"x".repeat(MAX_QUERY_LITERAL_BYTES))),
        Err(DataError::Capacity(_))
    ));
    let plan = QueryEngine::compile(empty.schema(), &QueryRequest::default()).unwrap();
    let changed = DataTable::new(
        TableSchema::new(vec![FieldSchema::new("Other", FieldType::Integer)]).unwrap(),
        vec![],
    )
    .unwrap();
    assert!(matches!(plan.execute(&changed), Err(DataError::Schema(_))));
}

#[test]
fn grouping_and_aggregates_have_explicit_null_empty_and_exact_decimal_semantics() {
    let table = table();
    let request = QueryRequest {
        group_by: vec![QueryExpression::Lowercase(Box::new(field("Category")))],
        order_by: vec![QueryOrder {
            expression: field("Price"),
            direction: SortDirection::Descending,
        }],
        aggregates: vec![
            count(),
            aggregate(AggregateFunction::CountValues, "Amount"),
            aggregate(AggregateFunction::Sum, "Amount"),
            aggregate(AggregateFunction::Average, "Amount"),
            aggregate(AggregateFunction::Sum, "Price"),
            aggregate(AggregateFunction::Average, "Price"),
            aggregate(AggregateFunction::Minimum, "Date"),
            aggregate(AggregateFunction::Maximum, "Bytes"),
        ],
        ..Default::default()
    };
    let result = QueryEngine::execute(&table, &request).unwrap();
    assert_eq!(result.row_keys, vec![-4, 30, 9, i64::MAX]);
    assert_eq!(
        &result.aggregates[..3],
        &[
            CellValue::Integer(4),
            CellValue::Integer(3),
            CellValue::Integer(28)
        ]
    );
    assert_eq!(
        result.aggregates[3],
        CellValue::Decimal("9.3333333333333333333333333333".parse().unwrap())
    );
    assert_eq!(
        result.aggregates[4],
        CellValue::Decimal("3.1".parse().unwrap())
    );
    assert_eq!(
        result.aggregates[5],
        CellValue::Decimal("0.775".parse().unwrap())
    );
    assert_eq!(
        result.aggregates[6],
        CellValue::Date("0001-01-01".parse().unwrap())
    );
    assert_eq!(result.aggregates[7], CellValue::Binary(vec![255]));
    assert_eq!(result.groups.len(), 3);
    assert_eq!(result.groups[0].key, vec![CellValue::Null]);
    assert_eq!(result.groups[0].aggregates[1], CellValue::Integer(0));
    assert_eq!(result.groups[0].aggregates[2], CellValue::Null);
    assert_eq!(result.groups[1].key, vec![CellValue::Text("альфа".into())]);
    assert_eq!(result.groups[1].row_keys, vec![-4, 9]);
    assert_eq!(result.groups[1].aggregates[2], CellValue::Integer(16));
    let empty = DataTable::new(table.schema().clone(), vec![]).unwrap();
    let result = QueryEngine::execute(&empty, &request).unwrap();
    assert!(result.groups.is_empty() && result.row_keys.is_empty());
    assert_eq!(
        &result.aggregates[..2],
        &[CellValue::Integer(0), CellValue::Integer(0)]
    );
    assert!(
        result.aggregates[2..]
            .iter()
            .all(|value| *value == CellValue::Null)
    );
}

#[test]
fn text_matching_treats_quotes_wildcards_and_code_as_data() {
    let table = table();
    for (operator, pattern, expected) in [
        (TextMatchOperator::Contains, "ЛЬ", vec![-4, 9]),
        (TextMatchOperator::StartsWith, "аЛь", vec![-4, 9]),
        (TextMatchOperator::EndsWith, "Фа", vec![-4, 9]),
        (
            TextMatchOperator::Contains,
            "%' OR 1=1; DROP TABLE Items; --",
            vec![],
        ),
    ] {
        let request = QueryRequest {
            filter: Some(Criteria::TextMatch {
                text: field("Category"),
                pattern: text(pattern),
                operator,
                comparison: TextComparison::UnicodeLowercase,
            }),
            ..Default::default()
        };
        assert_eq!(
            QueryEngine::execute(&table, &request).unwrap().row_keys,
            expected
        );
    }
    let request = QueryRequest {
        filter: Some(Criteria::TextMatch {
            text: field("Category"),
            pattern: text("АЛЬ"),
            operator: TextMatchOperator::Contains,
            comparison: TextComparison::Ordinal,
        }),
        ..Default::default()
    };
    assert_eq!(
        QueryEngine::execute(&table, &request).unwrap().row_keys,
        vec![-4]
    );
}

#[test]
fn numeric_aggregate_overflow_and_decimal_rounding_are_checked_without_float_conversion() {
    let schema = TableSchema::new(vec![FieldSchema::new("Value", FieldType::Integer)]).unwrap();
    let table = DataTable::new(
        schema,
        vec![
            DataRow::new(1, vec![CellValue::Integer(i64::MAX)]),
            DataRow::new(2, vec![CellValue::Integer(i64::MAX)]),
        ],
    )
    .unwrap();
    let query = |function| QueryRequest {
        aggregates: vec![aggregate(function, "Value")],
        ..Default::default()
    };
    assert!(matches!(
        QueryEngine::execute(&table, &query(AggregateFunction::Sum)),
        Err(DataError::Validation(_))
    ));
    assert_eq!(
        QueryEngine::execute(&table, &query(AggregateFunction::Average))
            .unwrap()
            .aggregates,
        vec![CellValue::Decimal(
            DecimalValue::new(i128::from(i64::MAX), 0).unwrap()
        )]
    );
    for sign in ["", "-"] {
        let schema = TableSchema::new(vec![FieldSchema::new("Value", FieldType::Decimal)]).unwrap();
        let value = format!("{sign}0.0000000000000000000000000001")
            .parse()
            .unwrap();
        let table = DataTable::new(
            schema,
            vec![
                DataRow::new(1, vec![CellValue::Decimal(value)]),
                DataRow::new(2, vec![CellValue::Decimal("0".parse().unwrap())]),
            ],
        )
        .unwrap();
        assert_eq!(
            QueryEngine::execute(&table, &query(AggregateFunction::Average))
                .unwrap()
                .aggregates,
            vec![CellValue::Decimal(value)]
        );
    }
    let schema = TableSchema::new(vec![FieldSchema::new("Value", FieldType::Decimal)]).unwrap();
    let table = DataTable::new(
        schema,
        vec![
            DataRow::new(
                0,
                vec![CellValue::Decimal(DecimalValue::new(i128::MAX, 0).unwrap())],
            ),
            DataRow::new(1, vec![CellValue::Decimal("1".parse().unwrap())]),
        ],
    )
    .unwrap();
    assert!(QueryEngine::execute(&table, &query(AggregateFunction::Sum)).is_err());
}

#[test]
fn real_aggregates_use_key_order_and_fail_instead_of_returning_infinity() {
    let schema = TableSchema::new(vec![FieldSchema::new("Value", FieldType::Real)]).unwrap();
    let rows = vec![
        DataRow::new(3, vec![CellValue::Real(1.0)]),
        DataRow::new(1, vec![CellValue::Real(1e16)]),
        DataRow::new(2, vec![CellValue::Real(-1e16)]),
    ];
    let query = QueryRequest {
        order_by: vec![QueryOrder {
            expression: field("Value"),
            direction: SortDirection::Descending,
        }],
        aggregates: vec![
            aggregate(AggregateFunction::Sum, "Value"),
            aggregate(AggregateFunction::Average, "Value"),
        ],
        ..Default::default()
    };
    let first = DataTable::new(schema.clone(), rows.clone()).unwrap();
    let second = DataTable::new(schema.clone(), rows.into_iter().rev().collect()).unwrap();
    let expected = QueryEngine::execute(&first, &query).unwrap();
    assert_eq!(
        expected.aggregates,
        vec![CellValue::Real(1.0), CellValue::Real(1.0 / 3.0)]
    );
    assert_eq!(QueryEngine::execute(&second, &query).unwrap(), expected);
    let overflowing = DataTable::new(
        schema,
        vec![
            DataRow::new(0, vec![CellValue::Real(f64::MAX)]),
            DataRow::new(1, vec![CellValue::Real(f64::MAX)]),
        ],
    )
    .unwrap();
    assert!(matches!(
        QueryEngine::execute(&overflowing, &query),
        Err(DataError::Validation(_))
    ));
}

#[test]
fn source_query_is_read_only_and_observable_refresh_updates_its_input() {
    let adapter = CollectionAdapter::new(table());
    let collection = adapter.handle();
    let mut source = DataSource::new(adapter).unwrap();
    let legacy = DataQuery {
        filter: "Бета".into(),
        sort: vec![],
    };
    source.set_query(legacy.clone()).unwrap();
    let revision = source.revision();
    let request = QueryRequest {
        aggregates: vec![aggregate(AggregateFunction::Sum, "Amount")],
        ..Default::default()
    };
    assert_eq!(
        source.evaluate_query(&request).unwrap().aggregates,
        vec![CellValue::Integer(28)]
    );
    assert_eq!(source.query(), &legacy);
    assert_eq!(source.revision(), revision);
    assert_eq!(source.len(), 1);
    collection
        .update_cell(30, "Amount", CellValue::Integer(22))
        .unwrap();
    assert_eq!(
        source.evaluate_query(&request).unwrap().aggregates,
        vec![CellValue::Integer(28)]
    );
    source.synchronize().unwrap();
    assert_eq!(
        source.evaluate_query(&request).unwrap().aggregates,
        vec![CellValue::Integer(38)]
    );
    let accepted = source.table().clone();
    assert!(
        source
            .evaluate_query(&QueryRequest {
                filter: Some(Criteria::IsNull(field("missing"))),
                ..Default::default()
            })
            .is_err()
    );
    assert_eq!(source.table(), &accepted);
    assert_eq!(source.query(), &legacy);
}

#[test]
fn query_scratch_and_result_growth_is_bounded() {
    let schema = TableSchema::new(vec![FieldSchema::new("Text", FieldType::Text)]).unwrap();
    let table = DataTable::with_limits(
        schema,
        vec![DataRow::new(1, vec![CellValue::Text("x".repeat(3000))])],
        DataLimits {
            max_bytes: 4096,
            ..Default::default()
        },
    )
    .unwrap();
    let request = QueryRequest {
        order_by: vec![
            QueryOrder {
                expression: field("Text"),
                direction: SortDirection::Ascending
            };
            8
        ],
        ..Default::default()
    };
    assert!(matches!(
        QueryEngine::execute(&table, &request),
        Err(DataError::Capacity(_))
    ));
}

#[test]
fn effective_scalar_limits_apply_to_expressions_criteria_and_unicode_expansion() {
    let schema = TableSchema::new(vec![FieldSchema::new("Text", FieldType::Text)]).unwrap();
    let table = DataTable::with_limits(
        schema,
        vec![DataRow::new(1, vec![CellValue::Text("İ".into())])],
        DataLimits {
            max_text_bytes: 2,
            max_binary_bytes: 2,
            ..Default::default()
        },
    )
    .unwrap();
    let lower = QueryExpression::Lowercase(Box::new(field("Text")));
    assert!(matches!(
        QueryEngine::evaluate_expression(&table, 1, &lower),
        Err(DataError::Capacity(_))
    ));
    let criteria = Criteria::TextMatch {
        text: field("Text"),
        pattern: text("i"),
        operator: TextMatchOperator::Contains,
        comparison: TextComparison::UnicodeLowercase,
    };
    assert!(matches!(
        QueryEngine::evaluate_criteria(&table, 1, &criteria),
        Err(DataError::Capacity(_))
    ));
    assert!(matches!(
        QueryEngine::execute(
            &table,
            &QueryRequest {
                filter: Some(criteria),
                ..Default::default()
            }
        ),
        Err(DataError::Capacity(_))
    ));
    assert_eq!(
        QueryEngine::evaluate_expression(&table, 1, &text("ab")).unwrap(),
        CellValue::Text("ab".into())
    );
    assert!(matches!(
        QueryEngine::evaluate_expression(&table, 1, &text("abc")),
        Err(DataError::Capacity(_))
    ));
    assert!(matches!(
        QueryEngine::evaluate_expression(
            &table,
            1,
            &QueryExpression::literal(CellValue::Binary(vec![1, 2, 3]))
        ),
        Err(DataError::Capacity(_))
    ));
}

#[test]
fn provider_dispatch_requires_full_capabilities_version_and_schema() {
    use std::{cell::Cell, rc::Rc};
    struct Provider {
        table: DataTable,
        capabilities: QueryCapabilities,
        metadata: bool,
        schema_reads: Rc<Cell<usize>>,
        loads: Rc<Cell<usize>>,
        pushed: Rc<Cell<usize>>,
    }
    impl DataAdapter for Provider {
        fn load(&mut self) -> Result<DataTable, DataError> {
            self.loads.set(self.loads.get() + 1);
            Ok(self.table.clone())
        }
        fn write_cell(
            &mut self,
            _: i64,
            _: &str,
            _: &CellValue,
            _: &CellValue,
        ) -> Result<DataTable, DataError> {
            panic!("read-only query must not write")
        }
        fn query_capabilities(&self) -> QueryCapabilities {
            self.capabilities
        }
        fn query_schema(&mut self) -> Result<Option<TableSchema>, DataError> {
            self.schema_reads.set(self.schema_reads.get() + 1);
            Ok(self.metadata.then(|| self.table.schema().clone()))
        }
        fn query_pushdown(&mut self, query: &QueryRequest) -> Result<QueryResult, DataError> {
            self.pushed.set(self.pushed.get() + 1);
            QueryEngine::execute(&self.table, query)
        }
    }
    let request = QueryRequest {
        filter: Some(compare(
            field("Amount"),
            ComparisonOperator::Greater,
            integer(5),
        )),
        order_by: vec![QueryOrder {
            expression: QueryExpression::Coalesce(Box::new(field("Amount")), Box::new(integer(0))),
            direction: SortDirection::Descending,
        }],
        group_by: vec![field("Active")],
        aggregates: vec![count()],
    };
    let table = table();
    let expected = QueryEngine::execute(&table, &request).unwrap();
    let plan = QueryEngine::compile(table.schema(), &request).unwrap();
    assert_eq!(plan.required_capabilities(), QueryCapabilities::complete());
    let full = QueryCapabilities::complete();
    let cases = [
        (full, true, (1, 0, 1)),
        (full, false, (1, 1, 0)),
        (QueryCapabilities::local_only(), true, (0, 1, 0)),
        (
            QueryCapabilities {
                semantics_version: QUERY_SEMANTICS_VERSION + 1,
                ..full
            },
            true,
            (0, 1, 0),
        ),
        (
            QueryCapabilities {
                filtering: false,
                ..full
            },
            true,
            (1, 1, 0),
        ),
        (
            QueryCapabilities {
                expressions: false,
                ..full
            },
            true,
            (1, 1, 0),
        ),
        (
            QueryCapabilities {
                ordering: false,
                ..full
            },
            true,
            (1, 1, 0),
        ),
        (
            QueryCapabilities {
                grouping: false,
                ..full
            },
            true,
            (1, 1, 0),
        ),
        (
            QueryCapabilities {
                aggregates: false,
                ..full
            },
            true,
            (1, 1, 0),
        ),
    ];
    for (capabilities, metadata, calls) in cases {
        let mut provider = Provider {
            table: table.clone(),
            capabilities,
            metadata,
            schema_reads: Rc::default(),
            loads: Rc::default(),
            pushed: Rc::default(),
        };
        assert_eq!(
            QueryEngine::execute_provider(&mut provider, &request).unwrap(),
            expected
        );
        assert_eq!(
            (
                provider.schema_reads.get(),
                provider.loads.get(),
                provider.pushed.get()
            ),
            calls
        );
        let push_count = provider.pushed.get();
        let invalid = QueryRequest {
            filter: Some(Criteria::IsNull(field("does-not-exist"))),
            ..Default::default()
        };
        assert!(matches!(
            QueryEngine::execute_provider(&mut provider, &invalid),
            Err(DataError::UnknownField(_))
        ));
        assert_eq!(
            provider.pushed.get(),
            push_count,
            "invalid queries never reach pushdown"
        );
    }
    // No explicit sort still needs stable signed-key ordering from a provider.
    let empty = QueryEngine::compile(table.schema(), &QueryRequest::default()).unwrap();
    assert_eq!(
        empty.execution(QueryCapabilities {
            ordering: false,
            ..full
        }),
        QueryExecution::Local
    );
}

#[test]
fn decimal_expression_precision_and_supported_operators_are_explicit() {
    let table = table();
    let decimal =
        |value: &str| QueryExpression::literal(CellValue::Decimal(value.parse().unwrap()));
    for (left, operator, right, expected) in [
        (
            "9007199254740993.01",
            ArithmeticOperator::Add,
            "0.02",
            "9007199254740993.03",
        ),
        ("-3.125", ArithmeticOperator::Subtract, "0.875", "-4"),
        ("1.25", ArithmeticOperator::Multiply, "0.8", "1"),
        (
            "0.000000000000000000000000001",
            ArithmeticOperator::Multiply,
            "0.10",
            "0.0000000000000000000000000001",
        ),
    ] {
        let expression = arithmetic(decimal(left), operator, decimal(right));
        assert_eq!(
            QueryEngine::evaluate_expression(&table, 30, &expression).unwrap(),
            CellValue::Decimal(expected.parse().unwrap())
        );
    }
    let division = arithmetic(decimal("1"), ArithmeticOperator::Divide, decimal("2"));
    assert!(matches!(
        QueryEngine::expression_type(table.schema(), &division),
        Err(DataError::Validation(_))
    ));
    let scale = arithmetic(
        decimal("0.0000000000000000000000000001"),
        ArithmeticOperator::Multiply,
        decimal("0.1"),
    );
    assert!(matches!(
        QueryEngine::evaluate_expression(&table, 30, &scale),
        Err(DataError::Validation(_))
    ));
}

#[cfg(feature = "sqlite")]
mod sqlite {
    use super::*;
    use rusqlite::{params_from_iter, types::Value};

    fn adapter(table: &DataTable) -> SqliteAdapter {
        let connection = SqliteConnection::open_in_memory().unwrap();
        connection.execute_batch("CREATE TABLE Items (Id INTEGER PRIMARY KEY, Category TEXT COLLATE NOCASE, Amount INTEGER, Price TEXT, Ratio REAL, Active INTEGER, Date TEXT, Time TEXT, Bytes BLOB);").unwrap();
        for row in table.rows() {
            let values =
                std::iter::once(Value::Integer(row.key)).chain(row.values.iter().map(|value| {
                    match value {
                        CellValue::Null => Value::Null,
                        CellValue::Bool(value) => Value::Integer(i64::from(*value)),
                        CellValue::Integer(value) => Value::Integer(*value),
                        CellValue::Real(value) => Value::Real(*value),
                        CellValue::Text(value) => Value::Text(value.clone()),
                        CellValue::Decimal(value) => Value::Text(value.to_string()),
                        CellValue::Date(value) => Value::Text(value.to_string()),
                        CellValue::Time(value) => Value::Text(value.to_string()),
                        CellValue::Binary(value) => Value::Blob(value.clone()),
                    }
                }));
            connection
                .execute(
                    "INSERT INTO Items VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                    params_from_iter(values),
                )
                .unwrap();
        }
        SqliteAdapter::from_connection(
            connection,
            SqliteTableConfig::new("Items", "Id", table.schema().clone()).unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn typed_criteria_order_groups_and_aggregates_match_collection_and_sqlite() {
        let table = table();
        let mut collection = CollectionAdapter::new(table.clone());
        let mut sqlite = adapter(&table);
        assert_eq!(sqlite.query_capabilities(), QueryCapabilities::local_only());
        assert_eq!(
            collection.query_capabilities(),
            QueryCapabilities::local_only()
        );
        let mut requests = vec![
            QueryRequest::default(),
            QueryRequest {
                filter: Some(Criteria::Any(vec![
                    Criteria::IsNull(field("Category")),
                    compare(field("Amount"), ComparisonOperator::LessOrEqual, integer(8)),
                ])),
                group_by: vec![
                    QueryExpression::Lowercase(Box::new(field("Category"))),
                    field("Active"),
                ],
                order_by: vec![
                    QueryOrder {
                        expression: field("Amount"),
                        direction: SortDirection::Descending,
                    },
                    QueryOrder {
                        expression: field("Price"),
                        direction: SortDirection::Ascending,
                    },
                ],
                aggregates: vec![
                    count(),
                    aggregate(AggregateFunction::CountValues, "Amount"),
                    aggregate(AggregateFunction::Sum, "Price"),
                    aggregate(AggregateFunction::Average, "Amount"),
                    aggregate(AggregateFunction::Average, "Ratio"),
                ],
            },
        ];
        for field_schema in table.schema().fields() {
            for direction in [SortDirection::Ascending, SortDirection::Descending] {
                requests.push(QueryRequest {
                    order_by: vec![QueryOrder {
                        expression: field(&field_schema.name),
                        direction,
                    }],
                    group_by: vec![field(&field_schema.name)],
                    aggregates: vec![
                        aggregate(AggregateFunction::Minimum, &field_schema.name),
                        aggregate(AggregateFunction::Maximum, &field_schema.name),
                    ],
                    ..Default::default()
                });
            }
        }
        requests.push(QueryRequest {
            filter: Some(Criteria::TextMatch {
                text: field("Category"),
                pattern: text("%' OR 1=1; DROP TABLE Items; --"),
                operator: TextMatchOperator::Contains,
                comparison: TextComparison::Ordinal,
            }),
            ..Default::default()
        });
        for request in requests {
            let local = QueryEngine::execute_provider(&mut collection, &request).unwrap();
            let storage = QueryEngine::execute_provider(&mut sqlite, &request).unwrap();
            assert_eq!(local, storage, "{request:?}");
        }
        assert_eq!(sqlite.load().unwrap().rows().len(), 4);
        let errors = [
            QueryRequest {
                filter: Some(compare(
                    field("Amount"),
                    ComparisonOperator::Equal,
                    text("8"),
                )),
                ..Default::default()
            },
            QueryRequest {
                order_by: vec![QueryOrder {
                    expression: field("Amount; DROP TABLE Items"),
                    direction: SortDirection::Ascending,
                }],
                ..Default::default()
            },
            QueryRequest {
                aggregates: vec![aggregate(AggregateFunction::Sum, "Category")],
                ..Default::default()
            },
            QueryRequest {
                filter: Some(compare(
                    arithmetic(integer(1), ArithmeticOperator::Divide, integer(0)),
                    ComparisonOperator::Equal,
                    integer(0),
                )),
                ..Default::default()
            },
        ];
        for request in errors {
            let local = QueryEngine::execute_provider(&mut collection, &request);
            let storage = QueryEngine::execute_provider(&mut sqlite, &request);
            assert!(local.is_err());
            assert_eq!(local, storage);
        }
    }
}
