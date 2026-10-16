use super::*;

mod foundations {
    use super::*;

    #[test]
    fn decimal_precision_edges_and_canonical_roundtrip() {
        for text in [
            "170141183460469231731687303715884105727",
            "-170141183460469231731687303715884105728",
            "-17014118346.0469231731687303715884105728",
            "0.0000000000000000000000000001",
            "9007199254740993.1234567890123456789",
        ] {
            let value: DecimalValue = text.parse().unwrap();
            assert_eq!(value.to_string(), text);
            assert_eq!(value.to_string().parse::<DecimalValue>().unwrap(), value);
        }
        assert_eq!(
            "+0001.2300".parse::<DecimalValue>().unwrap().to_string(),
            "1.23"
        );
        assert_eq!(
            "-0.000".parse::<DecimalValue>().unwrap(),
            DecimalValue::new(0, 0).unwrap()
        );
        assert_eq!(
            DecimalValue::new(i128::MIN, 0).unwrap().coefficient(),
            i128::MIN
        );
        assert_eq!(DecimalValue::new(12300, 4).unwrap().scale(), 2);
        for invalid in [
            "",
            "-",
            ".5",
            "1.",
            "1e3",
            "1,3",
            "NaN",
            " 1",
            "١",
            "1.2.3",
            "170141183460469231731687303715884105728",
            "-170141183460469231731687303715884105729",
            "0.00000000000000000000000000001",
        ] {
            assert!(invalid.parse::<DecimalValue>().is_err(), "{invalid}");
        }
        assert!(DecimalValue::new(0, 29).is_err());
        assert_eq!(
            format!("{}.0", i128::MAX).parse::<DecimalValue>().unwrap(),
            DecimalValue::new(i128::MAX, 0).unwrap()
        );
        assert_eq!(
            format!("{}.000", i128::MIN)
                .parse::<DecimalValue>()
                .unwrap(),
            DecimalValue::new(i128::MIN, 0).unwrap()
        );
    }

    #[test]
    fn decimal_ordering_does_not_overflow_rescaling() {
        let ordered = [
            "-170141183460469231731687303715884105728",
            "-10",
            "-1.01",
            "-1",
            "-0.0000000000000000000000000001",
            "0",
            "0.0000000000000000000000000001",
            "0.09",
            "0.1",
            "0.101",
            "1",
            "1.01",
            "9",
            "10",
            "170141183460469231731687303715884105727",
        ];
        let values: Vec<DecimalValue> = ordered.iter().map(|text| text.parse().unwrap()).collect();
        for (i, left) in values.iter().enumerate() {
            for (j, right) in values.iter().enumerate() {
                assert_eq!(
                    left.cmp(right),
                    i.cmp(&j),
                    "{} vs {}",
                    ordered[i],
                    ordered[j]
                );
            }
        }
        // Exhaustively compare a bounded subdomain against exact integer scaling.
        for left in -40i128..=40 {
            for right in -40i128..=40 {
                for scale in 0..=3 {
                    let a = DecimalValue::new(left, 0).unwrap();
                    let b = DecimalValue::new(right, scale).unwrap();
                    assert_eq!(a.cmp(&b), (left * 10i128.pow(u32::from(scale))).cmp(&right));
                }
            }
        }
    }

    #[test]
    fn gregorian_dates_validate_centuries_boundaries_and_editor_storage_formats() {
        for year in [4, 400, 1600, 2000, 2024, 2400] {
            assert!(DateValue::new(year, 2, 29).is_ok());
        }
        for year in [1, 100, 1900, 2025, 2100, 9999] {
            assert!(DateValue::new(year, 2, 29).is_err());
        }
        for parts in [
            (0, 1, 1),
            (10000, 1, 1),
            (2024, 0, 1),
            (2024, 13, 1),
            (2024, 4, 31),
            (2024, 1, 0),
        ] {
            assert!(DateValue::new(parts.0, parts.1, parts.2).is_err());
        }
        for iso in ["0001-01-01", "2000-02-29", "9999-12-31"] {
            let value: DateValue = iso.parse().unwrap();
            assert_eq!(value.to_string(), iso);
            assert_eq!(DateValue::parse_display(&value.display()).unwrap(), value);
            assert_eq!(
                FieldSchema::new("Date", FieldType::Date)
                    .parse(&value.display())
                    .unwrap(),
                CellValue::Date(value)
            );
        }
        for invalid in [
            "2024-2-29",
            "24-02-29",
            "2024/02/29",
            "2024-02-29Z",
            "２０２４-02-29",
        ] {
            assert!(invalid.parse::<DateValue>().is_err());
        }
        assert!(DateValue::parse_display("2024-02-29").is_err());
        assert!(DateValue::parse_display("29.02.2023").is_err());
    }

    #[test]
    fn time_preserves_nanoseconds_without_zones_or_rounding() {
        for (input, canonical, display) in [
            ("00:00", "00:00:00", "00:00"),
            (
                "23:59:59.999999999",
                "23:59:59.999999999",
                "23:59:59.999999999",
            ),
            (
                "12:34:00.000000001",
                "12:34:00.000000001",
                "12:34:00.000000001",
            ),
            ("01:02:03.1200", "01:02:03.12", "01:02:03.12"),
        ] {
            let value: TimeValue = input.parse().unwrap();
            assert_eq!(value.to_string(), canonical);
            assert_eq!(value.display(), display);
            assert_eq!(value.display().parse::<TimeValue>().unwrap(), value);
        }
        for invalid in [
            "24:00",
            "12:60",
            "23:59:60",
            "12:34.1",
            "1:02",
            "01:02:03.",
            "01:02:03.1234567890",
            "12:00Z",
            "12:00:00+01:00",
            "12:00:00.２",
        ] {
            assert!(invalid.parse::<TimeValue>().is_err(), "{invalid}");
        }
        assert!(TimeValue::new(0, 0, 0, 1_000_000_000).is_err());
    }

    #[test]
    fn scalar_display_parse_roundtrip_and_strict_nullable_types() {
        let cases = [
            (
                FieldType::Decimal,
                CellValue::Decimal("123.0001".parse().unwrap()),
            ),
            (
                FieldType::Date,
                CellValue::Date(DateValue::new(2024, 2, 29).unwrap()),
            ),
            (
                FieldType::Time,
                CellValue::Time(TimeValue::new(23, 1, 2, 3).unwrap()),
            ),
            (
                FieldType::Binary,
                CellValue::Binary(vec![0, 1, 127, 128, 255]),
            ),
        ];
        for (kind, value) in cases {
            let mut field = FieldSchema::new("Value", kind);
            assert_eq!(field.parse(&value.display()).unwrap(), value);
            assert!(field.validate(&CellValue::Null).is_err());
            assert!(field.validate(&CellValue::Integer(0)).is_err());
            field.nullable = true;
            assert_eq!(field.parse(" ").unwrap(), CellValue::Null);
        }
        let binary = FieldSchema::new("Bytes", FieldType::Binary);
        assert_eq!(binary.parse("0x").unwrap(), CellValue::Binary(vec![]));
        assert_eq!(
            binary.parse("0XfF0a").unwrap(),
            CellValue::Binary(vec![255, 10])
        );
        for invalid in ["", "0x1", "ff", "0xGG", "0x00 01", "0xф"] {
            assert!(binary.parse(invalid).is_err());
        }
    }

    #[test]
    fn binary_payload_is_bounded_for_snapshots_and_failed_edits() {
        let schema = TableSchema::new(vec![FieldSchema::new("Bytes", FieldType::Binary)]).unwrap();
        let limits = DataLimits {
            max_binary_bytes: 2,
            ..DataLimits::default()
        };
        let table = DataTable::with_limits(
            schema.clone(),
            vec![DataRow::new(1, vec![CellValue::Binary(vec![0, 255])])],
            limits,
        )
        .unwrap();
        let mut source = DataSource::new(CollectionAdapter::new(table.clone())).unwrap();
        assert!(
            source
                .set_cell(1, "Bytes", CellValue::Binary(vec![1, 2, 3]))
                .is_err()
        );
        assert_eq!(source.table(), &table);
        assert_eq!(source.revision(), 0);
        assert!(
            DataLimits {
                max_binary_bytes: 1024 * 1024 + 1,
                ..DataLimits::default()
            }
            .validate()
            .is_err()
        );
        let mut tight = limits;
        tight.max_bytes = size_of::<DataRow>()
            + 4 * size_of::<usize>()
            + size_of::<CellValue>()
            + "Bytes".len() * 3
            + 1;
        assert!(DataTable::with_limits(schema, table.rows().to_vec(), tight).is_err());
    }

    #[test]
    fn new_scalar_queries_use_typed_order_and_exact_filter_text() {
        let variants = [
            (
                FieldType::Decimal,
                vec![
                    CellValue::Decimal("10".parse().unwrap()),
                    CellValue::Decimal("2".parse().unwrap()),
                ],
            ),
            (
                FieldType::Date,
                vec![
                    CellValue::Date("2024-01-01".parse().unwrap()),
                    CellValue::Date("2023-12-31".parse().unwrap()),
                ],
            ),
            (
                FieldType::Time,
                vec![
                    CellValue::Time("01:01:01.1".parse().unwrap()),
                    CellValue::Time("01:01:01.000000001".parse().unwrap()),
                ],
            ),
            (
                FieldType::Binary,
                vec![
                    CellValue::Binary(vec![255]),
                    CellValue::Binary(vec![1, 255]),
                ],
            ),
        ];
        for (kind, values) in variants {
            let mut field = FieldSchema::new("Value", kind);
            field.nullable = true;
            let schema = TableSchema::new(vec![field]).unwrap();
            let table = DataTable::new(
                schema,
                vec![
                    DataRow::new(10, vec![values[0].clone()]),
                    DataRow::new(20, vec![values[1].clone()]),
                    DataRow::new(30, vec![CellValue::Null]),
                ],
            )
            .unwrap();
            let mut source = DataSource::new(CollectionAdapter::new(table)).unwrap();
            source
                .set_query(DataQuery {
                    sort: vec![SortDescriptor {
                        field: "Value".into(),
                        direction: SortDirection::Ascending,
                    }],
                    filter: String::new(),
                })
                .unwrap();
            assert_eq!(keys(&source), vec![30, 20, 10]);
            source
                .set_query(DataQuery {
                    sort: vec![],
                    filter: values[1].display(),
                })
                .unwrap();
            assert_eq!(keys(&source), vec![20]);
        }
    }

    #[test]
    fn live_schema_change_adopts_atomically_or_preserves_last_good_query() {
        let adapter = CollectionAdapter::new(table());
        let collection = adapter.handle();
        let mut source = DataSource::new(adapter).unwrap();
        source.set_query(query(SortDirection::Ascending)).unwrap();
        let revision = source.revision();
        let before = source.table().clone();
        let new_schema =
            TableSchema::new(vec![FieldSchema::new("Price", FieldType::Decimal)]).unwrap();
        let new_rows = vec![DataRow::new(
            20,
            vec![CellValue::Decimal("12.5".parse().unwrap())],
        )];
        collection
            .replace_schema(new_schema.clone(), new_rows.clone())
            .unwrap();
        assert!(source.synchronize().is_err());
        assert_eq!(source.table(), &before);
        assert_eq!(source.revision(), revision);
        assert!(matches!(
            source.set_cell_expected(20, "Count", &CellValue::Integer(2), CellValue::Integer(7)),
            Err(DataError::Conflict(_))
        ));
        assert_eq!(collection.snapshot().rows(), new_rows);
        assert!(source.refresh_with_query(DataQuery::default()).unwrap());
        assert_eq!(source.table().schema(), &new_schema);
        assert_eq!(source.table().rows(), new_rows);
        assert_eq!(
            source.changes_since(revision).changes,
            vec![DataChange::Reset]
        );
        assert!(source.last_error().is_none());
        assert!(!source.synchronize().unwrap());
    }

    #[test]
    fn schema_metadata_change_notifies_even_when_cells_are_equal() {
        let adapter = CollectionAdapter::new(table());
        let collection = adapter.handle();
        let mut source = DataSource::new(adapter).unwrap();
        let mut fields = schema().fields().to_vec();
        fields[0].title = "Updated title".into();
        fields[0].read_only = true;
        collection
            .replace_schema(TableSchema::new(fields).unwrap(), table().rows().to_vec())
            .unwrap();
        assert!(source.synchronize().unwrap());
        assert_eq!(source.changes_since(0).changes, vec![DataChange::Reset]);
        assert!(matches!(
            source.set_cell(20, "Name", CellValue::Text("B".into())),
            Err(DataError::ReadOnly(_))
        ));
    }

    #[test]
    fn failed_adapter_replacement_keeps_live_adapter_and_recovery_is_explicit() {
        let original = CollectionAdapter::new(table());
        let handle = original.handle();
        let mut source = DataSource::new(original).unwrap();
        let replacement = DataTable::new(
            TableSchema::new(vec![FieldSchema::new("Date", FieldType::Date)]).unwrap(),
            vec![DataRow::new(
                1,
                vec![CellValue::Date("2000-02-29".parse().unwrap())],
            )],
        )
        .unwrap();
        assert!(
            source
                .replace_adapter(
                    CollectionAdapter::new(replacement.clone()),
                    query(SortDirection::Ascending)
                )
                .is_err()
        );
        assert_eq!(source.table(), &table());
        handle
            .update_cell(20, "Count", CellValue::Integer(12))
            .unwrap();
        assert!(source.synchronize().unwrap());
        assert_eq!(source.value(20, "Count"), Some(&CellValue::Integer(12)));
        assert!(
            source
                .replace_adapter(
                    CollectionAdapter::new(replacement.clone()),
                    DataQuery::default()
                )
                .unwrap()
        );
        assert_eq!(source.table(), &replacement);
    }

    #[test]
    fn reentrant_schema_change_stays_pending_after_successful_source_write() {
        use std::{cell::Cell, rc::Rc};
        let adapter = CollectionAdapter::new(table());
        let collection = adapter.handle();
        let mut source = DataSource::new(adapter).unwrap();
        source.set_query(query(SortDirection::Ascending)).unwrap();
        let changed = Rc::new(Cell::new(false));
        let nested = collection.clone();
        let once = changed.clone();
        let _subscription = collection
            .subscribe(move |_| {
                if !once.replace(true) {
                    nested
                        .replace_schema(
                            TableSchema::new(vec![FieldSchema::new("Price", FieldType::Decimal)])
                                .unwrap(),
                            vec![DataRow::new(
                                20,
                                vec![CellValue::Decimal("1.25".parse().unwrap())],
                            )],
                        )
                        .unwrap();
                }
            })
            .unwrap();
        assert!(source.set_cell(20, "Count", CellValue::Integer(7)).unwrap());
        assert_eq!(source.value(20, "Count"), Some(&CellValue::Integer(7)));
        assert_eq!(collection.revision(), 2);
        // The returned snapshot belongs to the write, not its observer's reset.
        // Its cursor must not skip the later schema notification.
        let revision = source.revision();
        assert!(source.synchronize().is_err());
        assert_eq!(source.revision(), revision);
        assert_eq!(source.value(20, "Count"), Some(&CellValue::Integer(7)));
        assert!(source.refresh_with_query(DataQuery::default()).unwrap());
        assert_eq!(
            source.value(20, "Price"),
            Some(&CellValue::Decimal("1.25".parse().unwrap()))
        );
        assert!(!source.synchronize().unwrap());
    }

    #[test]
    fn source_wake_subscription_survives_adapter_replacement_and_drops_bridge() {
        use std::{cell::Cell, rc::Rc};
        let original = CollectionAdapter::new(table());
        let old_handle = original.handle();
        let mut source = DataSource::new(original).unwrap();
        let wakes = Rc::new(Cell::new(0));
        let wake_counter = wakes.clone();
        let token = source
            .subscribe_notifications(Box::new(move |_| wake_counter.set(wake_counter.get() + 1)))
            .unwrap()
            .unwrap();
        old_handle
            .update_cell(20, "Count", CellValue::Integer(5))
            .unwrap();
        assert_eq!(wakes.get(), 1);
        let replacement = CollectionAdapter::new(table());
        let new_handle = replacement.handle();
        source
            .replace_adapter(replacement, DataQuery::default())
            .unwrap();
        let before = wakes.get();
        old_handle
            .update_cell(20, "Count", CellValue::Integer(6))
            .unwrap();
        assert_eq!(wakes.get(), before);
        new_handle
            .update_cell(20, "Count", CellValue::Integer(7))
            .unwrap();
        assert_eq!(wakes.get(), before + 1);
        assert!(token.is_active());
        drop(source);
        assert!(!token.is_active());
        let before = wakes.get();
        new_handle
            .update_cell(20, "Count", CellValue::Integer(8))
            .unwrap();
        assert_eq!(wakes.get(), before);
    }

    #[test]
    fn failed_notification_bridge_registration_preserves_old_live_adapter() {
        let original = CollectionAdapter::new(table());
        let handle = original.handle();
        let mut source = DataSource::new(original).unwrap();
        let replacement = CollectionAdapter::new(table());
        let next = replacement.handle();
        let subscriptions: Vec<_> = (0..MAX_COLLECTION_SUBSCRIPTIONS)
            .map(|_| next.subscribe(|_| {}).unwrap())
            .collect();
        assert!(matches!(
            source.replace_adapter(replacement, DataQuery::default()),
            Err(DataError::Capacity(_))
        ));
        handle
            .update_cell(20, "Count", CellValue::Integer(11))
            .unwrap();
        assert!(source.synchronize().unwrap());
        assert_eq!(source.value(20, "Count"), Some(&CellValue::Integer(11)));
        drop(subscriptions);
    }

    #[test]
    fn source_wake_coalesces_nested_collection_hints_without_cross_hub_feedback() {
        use std::{cell::Cell, rc::Rc};
        let adapter = CollectionAdapter::new(table());
        let collection = adapter.handle();
        let mut source = DataSource::new(adapter).unwrap();
        let delivered = Rc::new(Cell::new(0));
        let count = delivered.clone();
        let writer = collection.clone();
        let _subscription = source
            .subscribe_notifications(Box::new(move |_| {
                let next = count.get() + 1;
                count.set(next);
                // A finite test cap exposes feedback without hanging a regression.
                if next <= 20 {
                    writer
                        .update_cell(20, "Count", CellValue::Integer(next + 2))
                        .unwrap();
                }
            }))
            .unwrap()
            .unwrap();
        source.set_query(query(SortDirection::Ascending)).unwrap();
        assert_eq!(delivered.get(), 1);
        assert_eq!(collection.revision(), 1);
        // The nested change is not lost: the scheduled update reads it later.
        assert_eq!(
            collection.snapshot().value(20, "Count"),
            Some(&CellValue::Integer(3))
        );
        assert_eq!(source.value(20, "Count"), Some(&CellValue::Integer(2)));
    }

    #[cfg(feature = "sqlite")]
    mod sqlite {
        use super::*;
        use rusqlite::{Connection, params};

        fn scalar_schema() -> TableSchema {
            TableSchema::new(vec![
                FieldSchema::new("Price", FieldType::Decimal),
                FieldSchema::new("Day", FieldType::Date),
                FieldSchema::new("Clock", FieldType::Time),
                FieldSchema::new("Bytes", FieldType::Binary),
            ])
            .unwrap()
        }

        fn scalar_connection() -> Connection {
            let connection = Connection::open_in_memory().unwrap();
            connection.execute_batch("CREATE TABLE ValuesTable (Id INTEGER PRIMARY KEY, Price TEXT NOT NULL, Day TEXT NOT NULL, Clock TEXT NOT NULL, Bytes BLOB NOT NULL);").unwrap();
            connection
                .execute(
                    "INSERT INTO ValuesTable VALUES (1, ?1, ?2, ?3, ?4)",
                    params![
                        "9007199254740993.1234567890123456789",
                        "2000-02-29",
                        "23:59:59.999999999",
                        vec![0u8, 128, 255]
                    ],
                )
                .unwrap();
            connection
        }

        fn scalar_config() -> SqliteTableConfig {
            SqliteTableConfig::new("ValuesTable", "Id", scalar_schema()).unwrap()
        }

        #[test]
        fn exact_scalar_storage_load_write_and_expected_conflict() {
            let mut source = DataSource::new(
                SqliteAdapter::from_connection(scalar_connection(), scalar_config()).unwrap(),
            )
            .unwrap();
            let price = CellValue::Decimal("9007199254740993.1234567890123456789".parse().unwrap());
            assert_eq!(source.value(1, "Price"), Some(&price));
            assert_eq!(
                source.value(1, "Day"),
                Some(&CellValue::Date("2000-02-29".parse().unwrap()))
            );
            assert_eq!(
                source.value(1, "Clock"),
                Some(&CellValue::Time("23:59:59.999999999".parse().unwrap()))
            );
            assert_eq!(
                source.value(1, "Bytes"),
                Some(&CellValue::Binary(vec![0, 128, 255]))
            );
            let next_price =
                CellValue::Decimal("-17014118346.0469231731687303715884105728".parse().unwrap());
            source
                .set_cell_expected(1, "Price", &price, next_price.clone())
                .unwrap();
            let revision = source.revision();
            assert!(matches!(
                source.set_cell_expected(1, "Price", &price, price.clone()),
                Err(DataError::Conflict(_))
            ));
            assert_eq!(source.revision(), revision);
            assert_eq!(source.value(1, "Price"), Some(&next_price));
            for (field, value) in [
                ("Day", CellValue::Date("0001-01-01".parse().unwrap())),
                (
                    "Clock",
                    CellValue::Time("00:00:00.000000001".parse().unwrap()),
                ),
                ("Bytes", CellValue::Binary(vec![])),
            ] {
                source.set_cell(1, field, value.clone()).unwrap();
                assert_eq!(source.value(1, field), Some(&value));
            }
            assert!(!source.refresh().unwrap());
        }

        #[test]
        fn storage_rejects_wrong_classes_noncanonical_text_and_invalid_dates() {
            for (field, sql) in [
                ("Price", "'1.00'"),
                ("Price", "'1e2'"),
                ("Price", "X'31'"),
                ("Day", "'29.02.2000'"),
                ("Day", "'1900-02-29'"),
                ("Clock", "'12:34'"),
                ("Clock", "'23:59:60'"),
                ("Bytes", "'0x00'"),
                ("Bytes", "123"),
            ] {
                let connection = scalar_connection();
                connection
                    .execute_batch(&format!("UPDATE ValuesTable SET {field} = {sql}"))
                    .unwrap();
                assert!(
                    DataSource::new(
                        SqliteAdapter::from_connection(connection, scalar_config()).unwrap()
                    )
                    .is_err(),
                    "{field}: {sql}"
                );
            }
        }

        #[test]
        fn numeric_affinity_cannot_silently_round_decimal_writes() {
            let connection = Connection::open_in_memory().unwrap();
            connection.execute_batch("CREATE TABLE Prices (Id INTEGER PRIMARY KEY, Price NUMERIC); INSERT INTO Prices VALUES (1, NULL)").unwrap();
            let mut price = FieldSchema::new("Price", FieldType::Decimal);
            price.nullable = true;
            let config =
                SqliteTableConfig::new("Prices", "Id", TableSchema::new(vec![price]).unwrap())
                    .unwrap();
            let mut source =
                DataSource::new(SqliteAdapter::from_connection(connection, config).unwrap())
                    .unwrap();
            assert!(
                source
                    .set_cell(
                        1,
                        "Price",
                        CellValue::Decimal("9007199254740993.123456789".parse().unwrap())
                    )
                    .is_err()
            );
            assert_eq!(source.value(1, "Price"), Some(&CellValue::Null));
            assert_eq!(source.revision(), 0);
            // An independent reload confirms that transaction rollback kept NULL.
            assert!(!source.refresh().unwrap());
        }

        #[test]
        fn invalid_trigger_result_rolls_back_binary_and_other_scalar_changes() {
            let connection = scalar_connection();
            connection.execute_batch("CREATE TRIGGER invalid_date AFTER UPDATE OF Bytes ON ValuesTable BEGIN UPDATE ValuesTable SET Day='1900-02-29' WHERE Id=NEW.Id; END;").unwrap();
            let mut source = DataSource::new(
                SqliteAdapter::from_connection(connection, scalar_config()).unwrap(),
            )
            .unwrap();
            let before = source.table().clone();
            assert!(
                source
                    .set_cell(1, "Bytes", CellValue::Binary(vec![1, 2]))
                    .is_err()
            );
            assert_eq!(source.table(), &before);
            assert_eq!(source.revision(), 0);
            assert!(!source.refresh().unwrap());
        }

        #[test]
        fn binary_sqlite_limits_reject_load_and_write_without_truncation() {
            let limits = DataLimits {
                max_binary_bytes: 2,
                ..DataLimits::default()
            };
            assert!(
                DataSource::new(
                    SqliteAdapter::with_limits(scalar_connection(), scalar_config(), limits)
                        .unwrap()
                )
                .is_err()
            );
            let limits = DataLimits {
                max_binary_bytes: 3,
                ..DataLimits::default()
            };
            let mut source = DataSource::new(
                SqliteAdapter::with_limits(scalar_connection(), scalar_config(), limits).unwrap(),
            )
            .unwrap();
            assert!(
                source
                    .set_cell(1, "Bytes", CellValue::Binary(vec![1, 2, 3, 4]))
                    .is_err()
            );
            assert_eq!(
                source.value(1, "Bytes"),
                Some(&CellValue::Binary(vec![0, 128, 255]))
            );
            assert!(!source.refresh().unwrap());
        }
    }
}

fn schema() -> TableSchema {
    let mut count = FieldSchema::new("Count", FieldType::Integer);
    count.nullable = true;
    TableSchema::new(vec![FieldSchema::new("Name", FieldType::Text), count]).unwrap()
}

fn table() -> DataTable {
    DataTable::new(
        schema(),
        vec![
            DataRow::new(
                20,
                vec![CellValue::Text("Груша".into()), CellValue::Integer(2)],
            ),
            DataRow::new(
                -10,
                vec![CellValue::Text("Яблоко".into()), CellValue::Integer(10)],
            ),
            DataRow::new(
                i64::MAX,
                vec![CellValue::Text("Слива".into()), CellValue::Null],
            ),
        ],
    )
    .unwrap()
}

fn keys(source: &DataSource) -> Vec<i64> {
    (0..source.len())
        .map(|index| source.row_key(index).unwrap())
        .collect()
}

fn query(direction: SortDirection) -> DataQuery {
    DataQuery {
        sort: vec![SortDescriptor {
            field: "Count".into(),
            direction,
        }],
        filter: String::new(),
    }
}

#[test]
fn schema_rejects_duplicates_empty_and_nul_names() {
    assert!(TableSchema::new(vec![]).is_err());
    for name in ["", "bad\0name"] {
        assert!(TableSchema::new(vec![FieldSchema::new(name, FieldType::Text)]).is_err());
    }
    assert!(
        TableSchema::new(vec![
            FieldSchema::new("A", FieldType::Text),
            FieldSchema::new("A", FieldType::Text)
        ])
        .is_err()
    );
}

#[test]
fn table_rejects_duplicate_keys_wrong_width_and_wrong_type() {
    let row = table().rows()[0].clone();
    assert!(DataTable::new(schema(), vec![row.clone(), row]).is_err());
    assert!(DataTable::new(schema(), vec![DataRow::new(1, vec![])]).is_err());
    assert!(
        DataTable::new(
            schema(),
            vec![DataRow::new(
                1,
                vec![CellValue::Bool(true), CellValue::Null]
            )]
        )
        .is_err()
    );
}

#[test]
fn field_parser_preserves_integer_edges_and_nullable_text() {
    let integer = FieldSchema::new("I", FieldType::Integer);
    assert_eq!(
        integer.parse(&i64::MAX.to_string()).unwrap(),
        CellValue::Integer(i64::MAX)
    );
    assert_eq!(
        integer.parse(&i64::MIN.to_string()).unwrap(),
        CellValue::Integer(i64::MIN)
    );
    assert!(integer.parse("9223372036854775808").is_err());
    assert!(integer.parse("1.0").is_err());
    let mut nullable = integer;
    nullable.nullable = true;
    assert_eq!(nullable.parse(" ").unwrap(), CellValue::Null);
    let mut text = FieldSchema::new("T", FieldType::Text);
    text.nullable = true;
    assert_eq!(text.parse("").unwrap(), CellValue::Text(String::new()));
    assert_eq!(text.parse("  x ").unwrap(), CellValue::Text("  x ".into()));
}

#[test]
fn field_parser_rejects_nonfinite_real_and_invalid_boolean() {
    let real = FieldSchema::new("R", FieldType::Real);
    for invalid in ["NaN", "inf", "-inf", "1e999"] {
        assert!(real.parse(invalid).is_err());
    }
    assert_eq!(real.parse("2.5").unwrap(), CellValue::Real(2.5));
    let boolean = FieldSchema::new("B", FieldType::Bool);
    assert_eq!(boolean.parse("ДА").unwrap(), CellValue::Bool(true));
    assert_eq!(boolean.parse("0").unwrap(), CellValue::Bool(false));
    assert!(boolean.parse("2").is_err());
}

#[test]
fn capacity_counts_utf8_bytes_and_enforces_rows_and_limits() {
    let one = TableSchema::new(vec![FieldSchema::new("Text", FieldType::Text)]).unwrap();
    let row = DataRow::new(1, vec![CellValue::Text("ЯЯ".into())]);
    let limits = DataLimits {
        max_text_bytes: 3,
        ..DataLimits::default()
    };
    assert!(DataTable::with_limits(one.clone(), vec![row.clone()], limits).is_err());
    let limits = DataLimits {
        max_text_bytes: 4,
        ..DataLimits::default()
    };
    assert!(DataTable::with_limits(one.clone(), vec![row.clone()], limits).is_ok());
    let limits = DataLimits {
        max_rows: 0,
        ..DataLimits::default()
    };
    assert!(DataTable::with_limits(one.clone(), vec![row.clone()], limits).is_err());
    let limits = DataLimits {
        max_bytes: 1,
        ..DataLimits::default()
    };
    assert!(DataTable::with_limits(one, vec![row], limits).is_err());
    assert!(
        DataLimits {
            max_rows: 100_001,
            ..DataLimits::default()
        }
        .validate()
        .is_err()
    );
}

#[test]
fn typed_sort_places_nulls_and_uses_stable_signed_key_ties() {
    let mut rows = table().rows().to_vec();
    rows.push(DataRow::new(
        i64::MIN,
        vec![CellValue::Text("Tie".into()), CellValue::Integer(2)],
    ));
    let mut source = DataSource::new(CollectionAdapter::new(
        DataTable::new(schema(), rows).unwrap(),
    ))
    .unwrap();
    source.set_query(query(SortDirection::Ascending)).unwrap();
    assert_eq!(keys(&source), vec![i64::MAX, i64::MIN, 20, -10]);
    source.set_query(query(SortDirection::Descending)).unwrap();
    assert_eq!(keys(&source), vec![-10, i64::MIN, 20, i64::MAX]);
    assert_eq!(source.position(i64::MIN), Some(1));
}

#[test]
fn unicode_search_and_invalid_query_are_atomic() {
    let mut source = DataSource::new(CollectionAdapter::new(table())).unwrap();
    source
        .set_query(DataQuery {
            filter: "ЯБЛ".into(),
            ..DataQuery::default()
        })
        .unwrap();
    assert_eq!(keys(&source), vec![-10]);
    assert_eq!(source.row(20).unwrap().key, 20); // Hidden records remain addressable.
    let before = source.query().clone();
    let revision = source.revision();
    assert!(
        source
            .set_query(DataQuery {
                sort: vec![SortDescriptor {
                    field: "Missing".into(),
                    direction: SortDirection::Ascending
                }],
                filter: String::new()
            })
            .is_err()
    );
    assert_eq!(source.query(), &before);
    assert_eq!(source.revision(), revision);
    assert!(source.last_error().is_some());
    assert!(!source.set_query(before).unwrap());
    assert!(source.last_error().is_none());
}

#[test]
fn collection_edit_reorders_view_and_does_not_publish_failed_candidates() {
    let mut source = DataSource::new(CollectionAdapter::new(table())).unwrap();
    source.set_query(query(SortDirection::Ascending)).unwrap();
    source
        .set_cell(20, "Count", CellValue::Integer(30))
        .unwrap();
    assert_eq!(keys(&source), vec![i64::MAX, -10, 20]);
    let revision = source.revision();
    assert!(matches!(
        source.set_cell_expected(20, "Count", &CellValue::Integer(2), CellValue::Integer(8)),
        Err(DataError::Conflict(_))
    ));
    assert!(
        source
            .set_cell(20, "Count", CellValue::Text("wrong".into()))
            .is_err()
    );
    assert_eq!(source.value(20, "Count"), Some(&CellValue::Integer(30)));
    assert_eq!(source.revision(), revision);
    assert!(
        !source
            .set_cell(20, "Count", CellValue::Integer(30))
            .unwrap()
    );
}

#[test]
fn collection_handle_refreshes_external_replacement_and_detects_stale_edits() {
    let adapter = CollectionAdapter::new(table());
    let handle = adapter.handle();
    let mut source = DataSource::new(adapter).unwrap();
    let mut rows = handle.snapshot().rows().to_vec();
    rows[0].values[0] = CellValue::Text("Changed outside source".into());
    rows.remove(1);
    handle
        .replace(DataTable::new(schema(), rows).unwrap())
        .unwrap();
    assert_eq!(source.len(), 3);
    assert!(matches!(
        source.set_cell_expected(
            20,
            "Name",
            &CellValue::Text("Груша".into()),
            CellValue::Text("Overwrite".into())
        ),
        Err(DataError::Conflict(_))
    ));
    assert_eq!(source.len(), 3);
    assert_eq!(source.revision(), 0);
    assert!(source.refresh().unwrap());
    assert_eq!(source.len(), 2);
    assert!(source.row(-10).is_none());
    assert_eq!(
        source.value(20, "Name"),
        Some(&CellValue::Text("Changed outside source".into()))
    );
    assert!(!source.refresh().unwrap());
}

#[test]
fn readonly_fields_and_failed_capacity_edit_do_not_mutate_collection() {
    let mut field = FieldSchema::new("Name", FieldType::Text);
    field.read_only = true;
    let schema = TableSchema::new(vec![field]).unwrap();
    let table = DataTable::new(
        schema,
        vec![DataRow::new(1, vec![CellValue::Text("A".into())])],
    )
    .unwrap();
    let mut source = DataSource::new(CollectionAdapter::new(table)).unwrap();
    assert!(matches!(
        source.set_cell(1, "Name", CellValue::Text("B".into())),
        Err(DataError::ReadOnly(_))
    ));
    let limits = DataLimits {
        max_text_bytes: 1,
        ..DataLimits::default()
    };
    let schema = TableSchema::new(vec![FieldSchema::new("Name", FieldType::Text)]).unwrap();
    let table = DataTable::with_limits(
        schema,
        vec![DataRow::new(1, vec![CellValue::Text("A".into())])],
        limits,
    )
    .unwrap();
    let adapter = CollectionAdapter::new(table);
    let handle = adapter.handle();
    let mut source = DataSource::new(adapter).unwrap();
    assert!(
        source
            .set_cell(1, "Name", CellValue::Text("AB".into()))
            .is_err()
    );
    assert_eq!(
        handle.snapshot().value(1, "Name"),
        Some(&CellValue::Text("A".into()))
    );
    let replacement = DataTable::new(
        handle.snapshot().schema().clone(),
        vec![DataRow::new(1, vec![CellValue::Text("AB".into())])],
    )
    .unwrap();
    assert!(handle.replace(replacement).is_err());
    assert_eq!(handle.snapshot().limits(), limits);
}

#[test]
fn cached_reads_do_not_call_adapter_and_refresh_failure_preserves_snapshot() {
    use std::{
        cell::{Cell, RefCell},
        rc::Rc,
    };
    struct Adapter {
        calls: Rc<Cell<usize>>,
        fail: Rc<RefCell<Option<DataError>>>,
    }
    impl DataAdapter for Adapter {
        fn load(&mut self) -> Result<DataTable, DataError> {
            self.calls.set(self.calls.get() + 1);
            if let Some(error) = self.fail.borrow().as_ref() {
                Err(error.clone())
            } else {
                Ok(table())
            }
        }
        fn write_cell(
            &mut self,
            _: i64,
            _: &str,
            _: &CellValue,
            _: &CellValue,
        ) -> Result<DataTable, DataError> {
            unreachable!()
        }
    }
    let calls = Rc::new(Cell::new(0));
    let fail = Rc::new(RefCell::new(None));
    let mut source = DataSource::new(Adapter {
        calls: calls.clone(),
        fail: fail.clone(),
    })
    .unwrap();
    for _ in 0..100 {
        assert!(source.value(20, "Name").is_some());
        assert!(source.row_key(0).is_some());
        assert_eq!(source.position(20), Some(0));
    }
    source.set_query(query(SortDirection::Ascending)).unwrap();
    assert_eq!(calls.get(), 1);
    let before = keys(&source);
    *fail.borrow_mut() = Some(DataError::Adapter("offline".into()));
    assert!(source.refresh().is_err());
    assert_eq!(keys(&source), before);
    assert_eq!(source.table(), &table());
}

#[cfg(feature = "sqlite")]
mod sqlite_tests {
    use super::*;
    use rusqlite::{Connection, OpenFlags};
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT: AtomicU64 = AtomicU64::new(1);

    fn connection() -> Connection {
        let connection = Connection::open_in_memory().unwrap();
        connection.execute_batch("CREATE TABLE Items (id INTEGER PRIMARY KEY, Name TEXT NOT NULL, Count INTEGER CHECK (Count IS NULL OR Count >= 0)); INSERT INTO Items VALUES (20, 'Груша', 2), (-10, 'Яблоко', 10), (9223372036854775807, 'Слива', NULL);").unwrap();
        connection
    }
    fn config() -> SqliteTableConfig {
        SqliteTableConfig::new("Items", "id", schema()).unwrap()
    }
    fn source() -> DataSource {
        DataSource::new(SqliteAdapter::from_connection(connection(), config()).unwrap()).unwrap()
    }
    fn pair() -> (Connection, Connection) {
        let uri = format!(
            "file:rust_ui_data_{}_{}?mode=memory&cache=shared",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        );
        let flags = OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_CREATE
            | OpenFlags::SQLITE_OPEN_URI;
        let first = Connection::open_with_flags(&uri, flags).unwrap();
        first.execute_batch("CREATE TABLE Items (id INTEGER PRIMARY KEY, Name TEXT NOT NULL, Count INTEGER CHECK (Count IS NULL OR Count >= 0)); INSERT INTO Items VALUES (20, 'Груша', 2);").unwrap();
        let second = Connection::open_with_flags(uri, flags).unwrap();
        (first, second)
    }

    #[test]
    fn sqlite_and_collection_queries_have_identical_typed_semantics() {
        let mut sqlite = source();
        let mut collection = DataSource::new(CollectionAdapter::new(table())).unwrap();
        for query in [
            query(SortDirection::Ascending),
            query(SortDirection::Descending),
            DataQuery {
                filter: "ЯБЛ".into(),
                ..DataQuery::default()
            },
        ] {
            sqlite.set_query(query.clone()).unwrap();
            collection.set_query(query).unwrap();
            assert_eq!(keys(&sqlite), keys(&collection));
        }
    }

    #[test]
    fn sqlite_roundtrips_integer_edges_boolean_real_and_null() {
        let connection = Connection::open_in_memory().unwrap();
        connection.execute_batch("CREATE TABLE T (id INTEGER PRIMARY KEY, I INTEGER, B INTEGER, R REAL, T TEXT); INSERT INTO T VALUES (-9223372036854775808, 9223372036854775807, 1, 2.5, NULL);").unwrap();
        let mut text = FieldSchema::new("T", FieldType::Text);
        text.nullable = true;
        let schema = TableSchema::new(vec![
            FieldSchema::new("I", FieldType::Integer),
            FieldSchema::new("B", FieldType::Bool),
            FieldSchema::new("R", FieldType::Real),
            text,
        ])
        .unwrap();
        let config = SqliteTableConfig::new("T", "id", schema).unwrap();
        let mut source =
            DataSource::new(SqliteAdapter::from_connection(connection, config).unwrap()).unwrap();
        assert_eq!(source.row_key(0), Some(i64::MIN));
        assert_eq!(
            source.value(i64::MIN, "I"),
            Some(&CellValue::Integer(i64::MAX))
        );
        source
            .set_cell(i64::MIN, "I", CellValue::Integer(i64::MIN))
            .unwrap();
        source
            .set_cell(i64::MIN, "B", CellValue::Bool(false))
            .unwrap();
        source
            .set_cell(i64::MIN, "R", CellValue::Real(1.25))
            .unwrap();
        source
            .set_cell(
                i64::MIN,
                "T",
                CellValue::Text("'quoted'; DROP TABLE T; --".into()),
            )
            .unwrap();
        assert!(!source.refresh().unwrap());
        assert_eq!(
            source.value(i64::MIN, "I"),
            Some(&CellValue::Integer(i64::MIN))
        );
        assert_eq!(source.value(i64::MIN, "B"), Some(&CellValue::Bool(false)));
    }

    #[test]
    fn sqlite_null_expected_predicate_updates_and_restores_null() {
        let mut source = source();
        source
            .set_cell(i64::MAX, "Count", CellValue::Integer(7))
            .unwrap();
        source.set_cell(i64::MAX, "Count", CellValue::Null).unwrap();
        assert_eq!(source.value(i64::MAX, "Count"), Some(&CellValue::Null));
    }

    #[test]
    fn sqlite_constraint_error_rolls_back_and_preserves_source_revision() {
        let mut source = source();
        assert!(
            source
                .set_cell(20, "Count", CellValue::Integer(-1))
                .is_err()
        );
        assert_eq!(source.revision(), 0);
        assert_eq!(source.value(20, "Count"), Some(&CellValue::Integer(2)));
        assert!(!source.refresh().unwrap());
    }

    #[test]
    fn sqlite_external_changes_and_deleted_rows_conflict_then_refresh() {
        let (connection, external) = pair();
        let mut source =
            DataSource::new(SqliteAdapter::from_connection(connection, config()).unwrap()).unwrap();
        external
            .execute("UPDATE Items SET Count = 4 WHERE id = 20", [])
            .unwrap();
        assert!(matches!(
            source.set_cell(20, "Count", CellValue::Integer(8)),
            Err(DataError::Conflict(_))
        ));
        assert_eq!(source.value(20, "Count"), Some(&CellValue::Integer(2)));
        source.refresh().unwrap();
        assert_eq!(source.value(20, "Count"), Some(&CellValue::Integer(4)));
        external
            .execute("DELETE FROM Items WHERE id = 20", [])
            .unwrap();
        assert!(matches!(
            source.set_cell(20, "Count", CellValue::Integer(9)),
            Err(DataError::Conflict(_))
        ));
        source.refresh().unwrap();
        assert!(source.is_empty());
    }

    #[test]
    fn sqlite_nocase_collation_does_not_hide_external_case_changes() {
        let (connection, external) = pair();
        connection.execute_batch("DROP TABLE Items; CREATE TABLE Items (id INTEGER PRIMARY KEY, Name TEXT COLLATE NOCASE, Count INTEGER); INSERT INTO Items VALUES (20, 'original', 2);").unwrap();
        let mut source =
            DataSource::new(SqliteAdapter::from_connection(connection, config()).unwrap()).unwrap();
        external
            .execute("UPDATE Items SET Name = 'ORIGINAL' WHERE id = 20", [])
            .unwrap();
        assert!(matches!(
            source.set_cell(20, "Name", CellValue::Text("new".into())),
            Err(DataError::Conflict(_))
        ));
        assert_eq!(
            external
                .query_row("SELECT Name FROM Items", [], |row| row.get::<_, String>(0))
                .unwrap(),
            "ORIGINAL"
        );
        assert_eq!(source.revision(), 0);
    }

    #[test]
    fn sqlite_external_storage_type_change_is_not_overwritten() {
        let (connection, external) = pair();
        connection.execute_batch("DROP TABLE Items; CREATE TABLE Items (id INTEGER PRIMARY KEY, Name TEXT, Count); INSERT INTO Items VALUES (20, 'original', 2);").unwrap();
        let mut source =
            DataSource::new(SqliteAdapter::from_connection(connection, config()).unwrap()).unwrap();
        external
            .execute("UPDATE Items SET Count = 2.0 WHERE id = 20", [])
            .unwrap();
        assert!(source.set_cell(20, "Count", CellValue::Integer(3)).is_err());
        assert_eq!(
            external
                .query_row("SELECT typeof(Count) FROM Items", [], |row| row
                    .get::<_, String>(0))
                .unwrap(),
            "real"
        );
        assert_eq!(source.revision(), 0);
    }

    #[test]
    fn sqlite_trigger_substitution_is_not_silently_accepted() {
        let (connection, external) = pair();
        connection.execute_batch("CREATE TRIGGER Coerce AFTER UPDATE OF Name ON Items BEGIN UPDATE Items SET Name = 'changed by trigger' WHERE id = NEW.id; END;").unwrap();
        let mut source =
            DataSource::new(SqliteAdapter::from_connection(connection, config()).unwrap()).unwrap();
        assert!(
            source
                .set_cell(20, "Name", CellValue::Text("Requested".into()))
                .is_err()
        );
        assert_eq!(
            external
                .query_row("SELECT Name FROM Items WHERE id = 20", [], |row| row
                    .get::<_, String>(0))
                .unwrap(),
            "Груша"
        );
    }

    #[test]
    fn sqlite_snapshot_capacity_failure_rolls_back_trigger_and_edit() {
        let (connection, external) = pair();
        connection.execute_batch("CREATE TRIGGER AddRow AFTER UPDATE ON Items BEGIN INSERT INTO Items VALUES (30, 'new', 1); END;").unwrap();
        let limits = DataLimits {
            max_rows: 1,
            ..DataLimits::default()
        };
        let mut source =
            DataSource::new(SqliteAdapter::with_limits(connection, config(), limits).unwrap())
                .unwrap();
        assert!(matches!(
            source.set_cell(20, "Count", CellValue::Integer(5)),
            Err(DataError::Capacity(_))
        ));
        assert_eq!(
            external
                .query_row("SELECT COUNT(*) FROM Items", [], |row| row.get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert_eq!(
            external
                .query_row("SELECT Count FROM Items WHERE id = 20", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            2
        );
    }

    #[test]
    fn sqlite_rejects_duplicate_keys_wrong_types_and_blob_values() {
        for sql in [
            "CREATE TABLE Items (id, Name, Count); INSERT INTO Items VALUES (1,'a',1),(1,'b',2);",
            "CREATE TABLE Items (id, Name, Count); INSERT INTO Items VALUES ('1','a',1);",
            "CREATE TABLE Items (id, Name, Count); INSERT INTO Items VALUES (1,'a','2');",
            "CREATE TABLE Items (id, Name, Count); INSERT INTO Items VALUES (1,X'6162',2);",
        ] {
            let connection = Connection::open_in_memory().unwrap();
            connection.execute_batch(sql).unwrap();
            assert!(
                DataSource::new(SqliteAdapter::from_connection(connection, config()).unwrap())
                    .is_err()
            );
        }
    }

    #[test]
    fn sqlite_qualified_fields_reject_unknown_names_instead_of_string_literals() {
        let schema = TableSchema::new(vec![FieldSchema::new("Missing", FieldType::Text)]).unwrap();
        let config = SqliteTableConfig::new("Items", "id", schema).unwrap();
        assert!(
            DataSource::new(SqliteAdapter::from_connection(connection(), config).unwrap()).is_err()
        );
    }

    #[test]
    fn sqlite_quotes_identifiers_and_parameterizes_values() {
        let connection = Connection::open_in_memory().unwrap();
        connection.execute_batch("CREATE TABLE \"Odd\"\"Table\" (\"Key\" INTEGER PRIMARY KEY, \"A B\" TEXT); INSERT INTO \"Odd\"\"Table\" VALUES (1, 'before');").unwrap();
        let schema = TableSchema::new(vec![FieldSchema::new("A B", FieldType::Text)]).unwrap();
        let config = SqliteTableConfig::new("Odd\"Table", "Key", schema).unwrap();
        let mut source =
            DataSource::new(SqliteAdapter::from_connection(connection, config).unwrap()).unwrap();
        source
            .set_cell(1, "A B", CellValue::Text("'; DROP TABLE any; --".into()))
            .unwrap();
        assert_eq!(
            source.value(1, "A B"),
            Some(&CellValue::Text("'; DROP TABLE any; --".into()))
        );
    }

    #[test]
    fn sqlite_row_and_text_limits_do_not_silently_truncate() {
        let limits = DataLimits {
            max_rows: 2,
            ..DataLimits::default()
        };
        assert!(matches!(
            DataSource::new(SqliteAdapter::with_limits(connection(), config(), limits).unwrap()),
            Err(DataError::Capacity(_))
        ));
        let limits = DataLimits {
            max_text_bytes: 1,
            ..DataLimits::default()
        };
        assert!(matches!(
            DataSource::new(SqliteAdapter::with_limits(connection(), config(), limits).unwrap()),
            Err(DataError::Capacity(_))
        ));
    }

    #[test]
    fn sqlite_open_existing_never_creates_a_missing_database() {
        let path = std::env::temp_dir().join(format!(
            "rust-ui-data-missing-{}-{}.db",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        assert!(!path.exists());
        assert!(SqliteAdapter::open(&path, config()).is_err());
        assert!(!path.exists());
        assert!(SqliteAdapter::open(":memory:", config()).is_err());
        assert!(SqliteAdapter::open("", config()).is_err());
    }
}
