use super::*;
use crate::{
    AggregateFunction as A, ComparisonOperator, DataRow, DecimalValue, FieldSchema, FieldType,
    TableSchema,
};

fn fixture() -> DataTable {
    let mut amount = FieldSchema::new("Amount", FieldType::Integer);
    amount.nullable = true;
    let mut region = FieldSchema::new("Region", FieldType::Text);
    region.nullable = true;
    DataTable::new(
        TableSchema::new(vec![
            region,
            FieldSchema::new("City", FieldType::Text),
            FieldSchema::new("Year", FieldType::Integer),
            amount,
        ])
        .unwrap(),
        vec![
            DataRow::new(
                i64::MIN,
                vec![
                    CellValue::Text("N".into()),
                    CellValue::Text("A".into()),
                    CellValue::Integer(2025),
                    CellValue::Integer(10),
                ],
            ),
            DataRow::new(
                0,
                vec![
                    CellValue::Text("N".into()),
                    CellValue::Text("A".into()),
                    CellValue::Integer(2026),
                    CellValue::Integer(20),
                ],
            ),
            DataRow::new(
                i64::MAX,
                vec![
                    CellValue::Text("N".into()),
                    CellValue::Text("B".into()),
                    CellValue::Integer(2025),
                    CellValue::Null,
                ],
            ),
            DataRow::new(
                7,
                vec![
                    CellValue::Text("S".into()),
                    CellValue::Text("C".into()),
                    CellValue::Integer(2025),
                    CellValue::Integer(100),
                ],
            ),
        ],
    )
    .unwrap()
}
fn definition() -> PivotDefinition {
    PivotDefinition {
        row_fields: vec!["Region".into(), "City".into()],
        column_fields: vec!["Year".into()],
        measures: [
            A::CountRows,
            A::CountValues,
            A::Sum,
            A::Average,
            A::Minimum,
            A::Maximum,
        ]
        .into_iter()
        .map(|function| PivotMeasure {
            title: format!("{function:?}"),
            aggregate: Aggregate {
                function,
                expression: if function == A::CountRows {
                    None
                } else {
                    Some(QueryExpression::field("Amount"))
                },
            },
        })
        .collect(),
        filter: None,
    }
}
fn version() -> PivotSourceVersion {
    PivotSourceVersion {
        source: PivotSourceId::new(),
        revision: 1,
    }
}
fn path<'a>(paths: &'a [PivotPath], values: &[CellValue]) -> &'a PivotPathId {
    paths.iter().find(|p| p.values() == values).unwrap().id()
}

#[test]
fn pivot_totals_aggregate_original_rows_not_child_averages() {
    let p = PivotSnapshot::build(&fixture(), version(), &definition()).unwrap();
    let root = p.row_paths()[0].id();
    let col = p.column_paths()[0].id();
    assert_eq!(p.value(root, col, 0).unwrap(), &CellValue::Integer(4));
    assert_eq!(p.value(root, col, 1).unwrap(), &CellValue::Integer(3));
    assert_eq!(p.value(root, col, 2).unwrap(), &CellValue::Integer(130));
    assert_eq!(
        p.value(root, col, 3).unwrap(),
        &CellValue::Decimal("43.3333333333333333333333333333".parse().unwrap())
    );
    let north = path(p.row_paths(), &[CellValue::Text("N".into())]);
    assert_eq!(
        p.value(north, col, 3).unwrap(),
        &CellValue::Decimal(DecimalValue::new(15, 0).unwrap())
    );
    assert_eq!(p.value(north, col, 4).unwrap(), &CellValue::Integer(10));
    assert_eq!(p.value(north, col, 5).unwrap(), &CellValue::Integer(20));
    assert_eq!(
        p.row_paths()
            .iter()
            .map(PivotPath::depth)
            .collect::<Vec<_>>(),
        vec![0, 1, 2, 2, 1, 2]
    );
}

#[test]
fn pivot_empty_intersection_null_and_empty_filter_use_query_semantics() {
    let p = PivotSnapshot::build(&fixture(), version(), &definition()).unwrap();
    let south = path(p.row_paths(), &[CellValue::Text("S".into())]);
    let year = path(p.column_paths(), &[CellValue::Integer(2026)]);
    assert_eq!(p.value(south, year, 0).unwrap(), &CellValue::Integer(0));
    for i in 2..6 {
        assert_eq!(p.value(south, year, i).unwrap(), &CellValue::Null);
    }
    assert!(
        p.drill_down(south, year, p.source_version())
            .unwrap()
            .row_keys
            .is_empty()
    );
    let mut d = definition();
    d.filter = Some(Criteria::Constant(false));
    let empty = PivotSnapshot::build(&fixture(), version(), &d).unwrap();
    assert_eq!(empty.row_paths().len(), 1);
    assert_eq!(empty.column_paths().len(), 1);
    assert_eq!(empty.matched_row_count(), 0);
    assert_eq!(empty.intersection_count(), 1);
    assert_eq!(
        empty
            .value(empty.row_paths()[0].id(), empty.column_paths()[0].id(), 0)
            .unwrap(),
        &CellValue::Integer(0)
    );
}

#[test]
fn pivot_filter_and_drilldown_keep_full_signed_keys_and_reject_foreign_or_stale_source() {
    let mut d = definition();
    d.filter = Some(Criteria::Compare {
        left: QueryExpression::field("Region"),
        operator: ComparisonOperator::Equal,
        right: QueryExpression::literal(CellValue::Text("N".into())),
    });
    let v = version();
    let p = PivotSnapshot::build(&fixture(), v.clone(), &d).unwrap();
    let row = p.row_paths()[0].id();
    let col = p.column_paths()[0].id();
    assert_eq!(
        p.drill_down(row, col, &v).unwrap().row_keys,
        vec![i64::MIN, 0, i64::MAX]
    );
    assert!(matches!(
        p.drill_down(row, col, &version()),
        Err(DataError::Conflict(_))
    ));
    let stale = PivotSourceVersion {
        source: v.source.clone(),
        revision: 2,
    };
    assert!(matches!(
        p.drill_down(row, col, &stale),
        Err(DataError::Conflict(_))
    ));
    assert!(p.value(col, row, 0).is_err());
    assert!(p.value(row, col, usize::MAX).is_err());
}

#[test]
fn pivot_path_identity_is_stable_after_insertion_and_reordering_but_typed() {
    let table = fixture();
    let v = version();
    let old = PivotSnapshot::build(&table, v.clone(), &definition()).unwrap();
    let mut rows = table.rows().to_vec();
    rows.reverse();
    rows.push(DataRow::new(
        8,
        vec![
            CellValue::Text("A".into()),
            CellValue::Text("new".into()),
            CellValue::Integer(2024),
            CellValue::Integer(5),
        ],
    ));
    let new = PivotSnapshot::build(
        &DataTable::new(table.schema().clone(), rows).unwrap(),
        v,
        &definition(),
    )
    .unwrap();
    for p in old.row_paths() {
        assert_eq!(p.id(), path(new.row_paths(), p.values()));
    }
    for p in old.column_paths() {
        assert_eq!(p.id(), path(new.column_paths(), p.values()));
    }
    let mut rows = table.rows().to_vec();
    rows[0].values[0] = CellValue::Null;
    rows[1].values[0] = CellValue::Text(String::new());
    let typed = PivotSnapshot::build(
        &DataTable::new(table.schema().clone(), rows).unwrap(),
        version(),
        &definition(),
    )
    .unwrap();
    assert_ne!(
        path(typed.row_paths(), &[CellValue::Null]),
        path(typed.row_paths(), &[CellValue::Text(String::new())])
    );
    assert_eq!(typed.row_paths()[1].values(), &[CellValue::Null]);
}

#[test]
fn pivot_failed_overflow_definition_and_limits_preserve_previous_snapshot() {
    let table = fixture();
    let p = PivotSnapshot::build(&table, version(), &definition()).unwrap();
    let before = p.clone();
    let mut rows = table.rows().to_vec();
    rows[0].values[3] = CellValue::Integer(i64::MAX);
    assert!(
        PivotSnapshot::build(
            &DataTable::new(table.schema().clone(), rows).unwrap(),
            version(),
            &definition()
        )
        .is_err()
    );
    let mut d = definition();
    d.column_fields.push("Region".into());
    assert!(PivotSnapshot::build(&table, version(), &d).is_err());
    d = definition();
    d.measures[0].aggregate.expression = Some(QueryExpression::field("Missing"));
    assert!(PivotSnapshot::build(&table, version(), &d).is_err());
    assert_eq!(
        p.value(p.row_paths()[0].id(), p.column_paths()[0].id(), 2)
            .unwrap(),
        before
            .value(before.row_paths()[0].id(), before.column_paths()[0].id(), 2)
            .unwrap()
    );
}

#[test]
fn pivot_decimal_average_and_sum_remain_exact() {
    let table = DataTable::new(
        TableSchema::new(vec![FieldSchema::new("Value", FieldType::Decimal)]).unwrap(),
        vec![
            DataRow::new(1, vec![CellValue::Decimal("0.1".parse().unwrap())]),
            DataRow::new(2, vec![CellValue::Decimal("0.2".parse().unwrap())]),
        ],
    )
    .unwrap();
    let d = PivotDefinition {
        measures: [A::Sum, A::Average]
            .into_iter()
            .map(|function| PivotMeasure {
                title: format!("{function:?}"),
                aggregate: Aggregate {
                    function,
                    expression: Some(QueryExpression::field("Value")),
                },
            })
            .collect(),
        ..Default::default()
    };
    let p = PivotSnapshot::build(&table, version(), &d).unwrap();
    let r = p.row_paths()[0].id();
    let c = p.column_paths()[0].id();
    assert_eq!(
        p.value(r, c, 0).unwrap(),
        &CellValue::Decimal("0.3".parse().unwrap())
    );
    assert_eq!(
        p.value(r, c, 1).unwrap(),
        &CellValue::Decimal("0.15".parse().unwrap())
    );
}

#[test]
fn pivot_sparse_large_source_and_capacity_failures_are_explicit() {
    let schema = TableSchema::new(vec![FieldSchema::new("Axis", FieldType::Integer)]).unwrap();
    let rows = (0..100_000)
        .map(|i| DataRow::new(i, vec![CellValue::Integer(i % 10)]))
        .collect();
    let table = DataTable::new(schema.clone(), rows).unwrap();
    let d = PivotDefinition {
        row_fields: vec!["Axis".into()],
        measures: vec![PivotMeasure {
            title: "Count".into(),
            aggregate: Aggregate {
                function: A::CountRows,
                expression: None,
            },
        }],
        ..Default::default()
    };
    let p = PivotSnapshot::build(&table, version(), &d).unwrap();
    assert_eq!(p.matched_row_count(), 100_000);
    assert_eq!(p.row_paths().len(), 11);
    assert_eq!(p.intersection_count(), 11);
    let rows = (0..10_000)
        .map(|i| DataRow::new(i, vec![CellValue::Integer(i)]))
        .collect();
    assert!(matches!(
        PivotSnapshot::build(&DataTable::new(schema, rows).unwrap(), version(), &d),
        Err(DataError::Capacity(_))
    ));
    let table = DataTable::new(
        TableSchema::new(vec![FieldSchema::new("Axis", FieldType::Text)]).unwrap(),
        vec![DataRow::new(
            1,
            vec![CellValue::Text("x".repeat(MAX_PIVOT_PATH_BYTES + 1))],
        )],
    )
    .unwrap();
    assert!(matches!(
        PivotSnapshot::build(&table, version(), &d),
        Err(DataError::Capacity(_))
    ));
}

#[test]
fn pivot_every_intersection_matches_manual_original_members_including_subtotals() {
    let table = fixture();
    let p = PivotSnapshot::build(&table, version(), &definition()).unwrap();
    for row in p.row_paths() {
        for col in p.column_paths() {
            let mut expected = table
                .rows()
                .iter()
                .filter(|record| {
                    row.fields()
                        .iter()
                        .zip(row.values())
                        .chain(col.fields().iter().zip(col.values()))
                        .all(|(field, value)| {
                            &record.values[table.schema().field_index(field).unwrap()] == value
                        })
                })
                .collect::<Vec<_>>();
            expected.sort_by_key(|r| r.key);
            let keys = expected.iter().map(|r| r.key).collect::<Vec<_>>();
            assert_eq!(
                p.drill_down(row.id(), col.id(), p.source_version())
                    .unwrap()
                    .row_keys,
                keys
            );
            assert_eq!(
                p.value(row.id(), col.id(), 0).unwrap(),
                &CellValue::Integer(expected.len() as i64)
            );
            let values = expected
                .iter()
                .filter_map(|r| {
                    if let CellValue::Integer(v) = r.values[3] {
                        Some(v)
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();
            assert_eq!(
                p.value(row.id(), col.id(), 1).unwrap(),
                &CellValue::Integer(values.len() as i64)
            );
            assert_eq!(
                p.value(row.id(), col.id(), 2).unwrap(),
                &if values.is_empty() {
                    CellValue::Null
                } else {
                    CellValue::Integer(values.iter().sum())
                }
            );
        }
    }
}

#[test]
fn pivot_typed_identity_normalizes_real_zero_and_supports_binary_date_time() {
    let fields = vec![
        FieldSchema::new("Real", FieldType::Real),
        FieldSchema::new("Bytes", FieldType::Binary),
        FieldSchema::new("Date", FieldType::Date),
        FieldSchema::new("Time", FieldType::Time),
    ];
    let date: crate::DateValue = "2026-10-17".parse().unwrap();
    let time: crate::TimeValue = "12:30:45.123456789".parse().unwrap();
    let table = DataTable::new(
        TableSchema::new(fields).unwrap(),
        vec![
            DataRow::new(
                1,
                vec![
                    CellValue::Real(-0.0),
                    CellValue::Binary(vec![0, 1]),
                    CellValue::Date(date),
                    CellValue::Time(time),
                ],
            ),
            DataRow::new(
                2,
                vec![
                    CellValue::Real(0.0),
                    CellValue::Binary(vec![0, 1]),
                    CellValue::Date(date),
                    CellValue::Time(time),
                ],
            ),
        ],
    )
    .unwrap();
    let mut d = PivotDefinition {
        row_fields: vec!["Real".into(), "Bytes".into()],
        column_fields: vec!["Date".into(), "Time".into()],
        measures: vec![PivotMeasure {
            title: "Rows".into(),
            aggregate: Aggregate {
                function: A::CountRows,
                expression: None,
            },
        }],
        filter: None,
    };
    let p = PivotSnapshot::build(&table, version(), &d).unwrap();
    assert_eq!(p.row_paths().len(), 3);
    assert_eq!(p.column_paths().len(), 3);
    assert_eq!(p.intersection_count(), 9);
    assert_eq!(
        p.value(p.row_paths()[2].id(), p.column_paths()[2].id(), 0)
            .unwrap(),
        &CellValue::Integer(2)
    );
    let mut reversed = table.rows().to_vec();
    reversed[0].values[0] = CellValue::Real(0.0);
    reversed[1].values[0] = CellValue::Real(-0.0);
    let q = PivotSnapshot::build(
        &DataTable::new(table.schema().clone(), reversed).unwrap(),
        version(),
        &d,
    )
    .unwrap();
    assert_eq!(p.row_paths()[2].id(), q.row_paths()[2].id());
    std::mem::swap(&mut d.row_fields, &mut d.column_fields);
    let moved = PivotSnapshot::build(&table, version(), &d).unwrap();
    assert_ne!(p.row_paths()[2].id(), moved.column_paths()[2].id());
}

#[test]
fn pivot_work_ast_and_combined_snapshot_budgets_refuse_without_partial_publication() {
    let table = fixture();
    let v = version();
    let p = PivotSnapshot::build(&table, v.clone(), &definition()).unwrap();
    let mut d = definition();
    let mut filter = Criteria::Constant(true);
    for _ in 0..40 {
        filter = Criteria::Not(Box::new(filter));
    }
    d.filter = Some(filter);
    assert!(matches!(
        PivotSnapshot::build(&table, v.clone(), &d),
        Err(DataError::Capacity(_))
    ));
    d.filter = Some(Criteria::All(vec![
        Criteria::Constant(true);
        crate::MAX_QUERY_TERMS + 1
    ]));
    assert!(matches!(
        PivotSnapshot::build(&table, v.clone(), &d),
        Err(DataError::Capacity(_))
    ));
    let mut limits = table.limits();
    limits.max_bytes = 2000;
    let small =
        DataTable::with_limits(table.schema().clone(), table.rows().to_vec(), limits).unwrap();
    assert!(matches!(
        PivotSnapshot::build(&small, v.clone(), &definition()),
        Err(DataError::Capacity(_))
    ));
    let large = DataTable::new(
        TableSchema::new(vec![FieldSchema::new("Axis", FieldType::Integer)]).unwrap(),
        (0..100_000)
            .map(|i| DataRow::new(i, vec![CellValue::Integer(i % 10)]))
            .collect(),
    )
    .unwrap();
    let costly = PivotDefinition {
        row_fields: vec!["Axis".into()],
        filter: Some(Criteria::All(vec![Criteria::Constant(true); 100])),
        measures: vec![PivotMeasure {
            title: "Count".into(),
            aggregate: Aggregate {
                function: A::CountRows,
                expression: None,
            },
        }],
        ..Default::default()
    };
    assert!(matches!(
        PivotSnapshot::build(&large, v.clone(), &costly),
        Err(DataError::Capacity(_))
    ));
    assert_eq!(p.source_version(), &v);
    assert_eq!(p.matched_row_count(), 4);
}

#[test]
fn pivot_axis_name_rejection_does_not_copy_unbounded_input_into_error() {
    let table = fixture();
    for name in [
        String::new(),
        "x".repeat(257),
        "x".repeat(1024 * 1024),
        "Region\0suffix".into(),
    ] {
        let mut d = definition();
        d.row_fields = vec![name];
        let error = PivotSnapshot::build(&table, version(), &d).unwrap_err();
        assert!(matches!(error, DataError::Validation(_)));
        assert!(error.to_string().len() < 160);
    }
    let mut d = definition();
    d.row_fields = vec!["x".repeat(256)];
    assert!(matches!(
        PivotSnapshot::build(&table, version(), &d),
        Err(DataError::UnknownField(_))
    ));
}
