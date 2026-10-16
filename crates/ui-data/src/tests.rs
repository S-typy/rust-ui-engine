use super::*;

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
