use std::{
    collections::BTreeSet,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::Duration,
};

use crate::*;
use rusqlite::{Connection, params};

fn config() -> SqliteTableConfig {
    let mut name = FieldSchema::new("Name", FieldType::Text);
    name.nullable = true;
    let mut number = FieldSchema::new("N", FieldType::Integer);
    number.nullable = true;
    SqliteTableConfig::new("Items", "Id", TableSchema::new(vec![name, number]).unwrap()).unwrap()
}

fn fixture(connection: &Connection) {
    connection.execute_batch("CREATE TABLE Items (Id INTEGER PRIMARY KEY, Name TEXT COLLATE NOCASE, N INTEGER CHECK(N >= 0)); INSERT INTO Items VALUES (-10,'Alpha',1),(20,'alpha',2),(30,NULL,NULL);").unwrap();
}

fn adapter() -> SqliteAdapter {
    let connection = Connection::open_in_memory().unwrap();
    fixture(&connection);
    SqliteAdapter::from_connection(connection, config()).unwrap()
}

fn row(key: i64, name: Option<&str>, n: Option<i64>) -> DataRow {
    DataRow::new(
        key,
        vec![
            name.map_or(CellValue::Null, |value| CellValue::Text(value.into())),
            n.map_or(CellValue::Null, CellValue::Integer),
        ],
    )
}

fn update(key: i64, old: i64, new: i64) -> DataWrite {
    DataWrite::Update {
        key,
        field: "N".into(),
        expected: CellValue::Integer(old),
        value: CellValue::Integer(new),
    }
}

fn sorted(field: &str, direction: SortDirection) -> QueryRequest {
    QueryRequest {
        order_by: vec![QueryOrder {
            expression: QueryExpression::field(field),
            direction,
        }],
        ..QueryRequest::default()
    }
}

struct Database(PathBuf);
impl Database {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Self(std::env::temp_dir().join(format!(
            "rust-ui-sqlite-{}-{}.db",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        )))
    }
    fn connect(&self) -> Connection {
        Connection::open(&self.0).unwrap()
    }
}
impl Drop for Database {
    fn drop(&mut self) {
        for suffix in ["", "-wal", "-shm", "-journal"] {
            let _ = std::fs::remove_file(format!("{}{suffix}", self.0.display()));
        }
    }
}

#[test]
fn sqlite_crud_batch_publishes_once_and_preserves_sequential_expectations() {
    let mut source = DataSource::new(adapter()).unwrap();
    assert!(source.write_capabilities().atomic_batch);
    let batch = [
        DataWrite::Insert(row(i64::MAX, Some("'; DROP TABLE Items; --"), Some(3))),
        update(i64::MAX, 3, 4),
        update(-10, 1, 5),
        DataWrite::Delete {
            expected: row(20, Some("alpha"), Some(2)),
        },
    ];
    assert!(source.write_batch(&batch).unwrap());
    assert_eq!(source.revision(), 1);
    assert_eq!(source.len(), 3);
    assert_eq!(source.value(i64::MAX, "N"), Some(&CellValue::Integer(4)));
    assert!(source.row(20).is_none());
    assert!(!source.refresh().unwrap());
    assert!(
        !source
            .write_batch(&[update(-10, 5, 6), update(-10, 6, 5)])
            .unwrap()
    );
    assert_eq!(source.revision(), 1);
}

#[test]
fn sqlite_constraint_trigger_and_commit_failures_roll_back_entire_batch() {
    for trigger in [
        "",
        "CREATE TRIGGER substitute AFTER UPDATE ON Items BEGIN UPDATE Items SET N=99 WHERE Id=new.Id; END;",
        "CREATE TRIGGER unrelated AFTER UPDATE ON Items BEGIN INSERT INTO Items VALUES(99,'unexpected',1); END;",
        "CREATE TABLE Parent(Id INTEGER PRIMARY KEY); CREATE TABLE Child(P INTEGER REFERENCES Parent(Id) DEFERRABLE INITIALLY DEFERRED); CREATE TRIGGER fk AFTER UPDATE ON Items BEGIN INSERT INTO Child VALUES(123); END;",
    ] {
        let connection = Connection::open_in_memory().unwrap();
        connection.execute_batch("PRAGMA foreign_keys=ON").unwrap();
        fixture(&connection);
        connection.execute_batch(trigger).unwrap();
        let mut source =
            DataSource::new(SqliteAdapter::from_connection(connection, config()).unwrap()).unwrap();
        let before = source.table().clone();
        let final_value = if trigger.is_empty() { -1 } else { 10 };
        let batch = [
            DataWrite::Insert(row(50, Some("first"), Some(5))),
            update(-10, 1, final_value),
        ];
        assert!(source.write_batch(&batch).is_err(), "{trigger}");
        assert_eq!(source.table(), &before);
        assert_eq!(source.revision(), 0);
        assert!(!source.refresh().unwrap());
        assert_eq!(source.table(), &before);
    }
}

#[test]
fn sqlite_full_row_delete_conflict_checks_case_and_unrelated_fields() {
    let database = Database::new();
    let connection = database.connect();
    fixture(&connection);
    let external = database.connect();
    let mut source =
        DataSource::new(SqliteAdapter::from_connection(connection, config()).unwrap()).unwrap();
    external
        .execute("UPDATE Items SET Name='ALPHA' WHERE Id=20", [])
        .unwrap();
    let batch = [
        update(-10, 1, 9),
        DataWrite::Delete {
            expected: row(20, Some("alpha"), Some(2)),
        },
    ];
    assert!(matches!(
        source.write_batch(&batch),
        Err(DataError::Conflict(_))
    ));
    assert_eq!(
        external
            .query_row("SELECT N FROM Items WHERE Id=-10", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(source.revision(), 0);
    assert!(source.refresh().unwrap());
    assert_eq!(
        source.value(20, "Name"),
        Some(&CellValue::Text("ALPHA".into()))
    );
}

#[test]
fn sqlite_writer_lock_keeps_snapshot_and_batch_can_be_retried() {
    let database = Database::new();
    let connection = database.connect();
    fixture(&connection);
    let external = database.connect();
    let mut source =
        DataSource::new(SqliteAdapter::from_connection(connection, config()).unwrap()).unwrap();
    external.execute_batch("BEGIN IMMEDIATE").unwrap();
    let draft = [
        update(-10, 1, 8),
        DataWrite::Insert(row(60, Some("later"), Some(6))),
    ];
    assert!(matches!(
        source.write_batch(&draft),
        Err(DataError::Adapter(_))
    ));
    assert_eq!(source.revision(), 0);
    assert_eq!(source.value(-10, "N"), Some(&CellValue::Integer(1)));
    external.execute_batch("ROLLBACK").unwrap();
    assert!(source.write_batch(&draft).unwrap());
    assert_eq!(source.revision(), 1);
}

#[test]
fn sqlite_all_scalars_insert_update_delete_roundtrip_and_visible_key_guard() {
    let connection = Connection::open_in_memory().unwrap();
    connection.execute_batch("CREATE TABLE Scalars(Id INTEGER PRIMARY KEY, B INTEGER, I INTEGER, R REAL, T TEXT, D TEXT, Day TEXT, Clock TEXT, Bytes BLOB)").unwrap();
    let fields = [
        ("Id", FieldType::Integer),
        ("B", FieldType::Bool),
        ("I", FieldType::Integer),
        ("R", FieldType::Real),
        ("T", FieldType::Text),
        ("D", FieldType::Decimal),
        ("Day", FieldType::Date),
        ("Clock", FieldType::Time),
        ("Bytes", FieldType::Binary),
    ]
    .map(|(name, kind)| {
        let mut field = FieldSchema::new(name, kind);
        field.read_only = name == "Id";
        field
    });
    let schema = TableSchema::new(fields.into()).unwrap();
    let config = SqliteTableConfig::new("Scalars", "Id", schema).unwrap();
    let mut source =
        DataSource::new(SqliteAdapter::from_connection(connection, config).unwrap()).unwrap();
    let mut initial = DataRow::new(
        i64::MIN,
        vec![
            CellValue::Integer(i64::MIN),
            CellValue::Bool(true),
            CellValue::Integer(i64::MAX),
            CellValue::Real(1.25),
            CellValue::Text("text\0data".into()),
            CellValue::Decimal("9007199254740993.1234567890123456789".parse().unwrap()),
            CellValue::Date("2000-02-29".parse().unwrap()),
            CellValue::Time("23:59:59.000000001".parse().unwrap()),
            CellValue::Binary(vec![0, 255, 1]),
        ],
    );
    assert!(
        source
            .write_batch(&[DataWrite::Insert(initial.clone())])
            .unwrap()
    );
    assert_eq!(source.row(i64::MIN), Some(&initial));
    assert!(!source.refresh().unwrap());
    let values = vec![
        CellValue::Bool(false),
        CellValue::Integer(i64::MIN),
        CellValue::Real(-1.5),
        CellValue::Text("".into()),
        CellValue::Decimal("-0.0000000000000000000000000001".parse().unwrap()),
        CellValue::Date("9999-12-31".parse().unwrap()),
        CellValue::Time("00:00:00".parse().unwrap()),
        CellValue::Binary(vec![]),
    ];
    let updates: Vec<_> = source.table().schema().fields()[1..]
        .iter()
        .enumerate()
        .map(|(index, field)| DataWrite::Update {
            key: i64::MIN,
            field: field.name.clone(),
            expected: initial.values[index + 1].clone(),
            value: values[index].clone(),
        })
        .collect();
    assert!(source.write_batch(&updates).unwrap());
    initial.values.splice(1.., values);
    assert_eq!(source.row(i64::MIN), Some(&initial));
    let mut wrong = initial.clone();
    wrong.key = 11;
    assert!(matches!(
        source.write_batch(&[DataWrite::Insert(wrong)]),
        Err(DataError::Validation(_))
    ));
    assert_eq!(source.revision(), 2);
    assert!(
        source
            .write_batch(&[DataWrite::Delete { expected: initial }])
            .unwrap()
    );
    assert!(source.is_empty());
}

#[test]
fn sqlite_pushdown_matches_local_null_type_binary_collation_and_stable_order() {
    let mut adapter = adapter();
    let table = adapter.load().unwrap();
    let mut queries = vec![
        QueryRequest::default(),
        sorted("Name", SortDirection::Ascending),
        sorted("Name", SortDirection::Descending),
        sorted("N", SortDirection::Descending),
    ];
    for op in [
        ComparisonOperator::Equal,
        ComparisonOperator::NotEqual,
        ComparisonOperator::Less,
        ComparisonOperator::LessOrEqual,
        ComparisonOperator::Greater,
        ComparisonOperator::GreaterOrEqual,
    ] {
        queries.push(QueryRequest {
            filter: Some(Criteria::Compare {
                left: QueryExpression::field("Name"),
                operator: op,
                right: QueryExpression::literal(CellValue::Text("Alpha".into())),
            }),
            ..QueryRequest::default()
        });
        queries.push(QueryRequest {
            filter: Some(Criteria::Not(Box::new(Criteria::Compare {
                left: QueryExpression::field("N"),
                operator: op,
                right: QueryExpression::literal(CellValue::Integer(1)),
            }))),
            ..sorted("Name", SortDirection::Descending)
        });
    }
    queries.push(QueryRequest {
        filter: Some(Criteria::Any(vec![
            Criteria::IsNull(QueryExpression::field("Name")),
            Criteria::All(vec![
                Criteria::Constant(true),
                Criteria::Not(Box::new(Criteria::IsNull(QueryExpression::field("N")))),
            ]),
        ])),
        order_by: vec![QueryOrder {
            expression: QueryExpression::literal(CellValue::Integer(100)),
            direction: SortDirection::Descending,
        }],
        ..QueryRequest::default()
    });
    for query in queries {
        assert_eq!(
            adapter.query_execution(&query).unwrap(),
            QueryExecution::Pushdown
        );
        let expected = QueryEngine::execute(&table, &query).unwrap();
        assert_eq!(
            QueryEngine::execute_provider(&mut adapter, &query).unwrap(),
            expected
        );
        let page = adapter.query_page(&query, 0, 3).unwrap();
        assert_eq!(
            page.table
                .rows()
                .iter()
                .map(|row| row.key)
                .collect::<Vec<_>>(),
            expected.row_keys
        );
        assert_eq!(page.total_rows, expected.row_keys.len());
    }
}

#[test]
fn sqlite_pushdown_scalar_order_parity_handles_utf8_and_embedded_nul() {
    let connection = Connection::open_in_memory().unwrap();
    connection.execute_batch("CREATE TABLE T(Id INTEGER PRIMARY KEY, B INTEGER, R REAL, S TEXT COLLATE RTRIM, Day TEXT, Clock TEXT, Bytes BLOB)").unwrap();
    let fields = [
        ("B", FieldType::Bool),
        ("R", FieldType::Real),
        ("S", FieldType::Text),
        ("Day", FieldType::Date),
        ("Clock", FieldType::Time),
        ("Bytes", FieldType::Binary),
    ]
    .map(|(name, kind)| {
        let mut f = FieldSchema::new(name, kind);
        f.nullable = true;
        f
    });
    for (key, text, day, time, blob) in [
        (8, "a ", "2000-02-29", "12:00:00", vec![255u8]),
        (-7, "a\0b", "1999-12-31", "12:00:00.1", vec![0, 255]),
        (3, "\u{10000}", "9999-12-31", "00:00:00", vec![]),
        (2, "\u{ffff}", "0001-01-01", "23:59:59.9", vec![1]),
    ] {
        connection
            .execute(
                "INSERT INTO T VALUES(?1,?2,?3,?4,?5,?6,?7)",
                params![key, key % 2 != 0, key as f64, text, day, time, blob],
            )
            .unwrap();
    }
    connection
        .execute_batch("INSERT INTO T VALUES (0,NULL,NULL,NULL,NULL,NULL,NULL)")
        .unwrap();
    let config =
        SqliteTableConfig::new("T", "Id", TableSchema::new(fields.into()).unwrap()).unwrap();
    let mut adapter = SqliteAdapter::from_connection(connection, config).unwrap();
    let table = adapter.load().unwrap();
    for field in table.schema().fields() {
        for direction in [SortDirection::Ascending, SortDirection::Descending] {
            let query = sorted(&field.name, direction);
            assert_eq!(
                QueryEngine::execute_provider(&mut adapter, &query).unwrap(),
                QueryEngine::execute(&table, &query).unwrap()
            );
        }
        for value in table
            .rows()
            .iter()
            .map(|row| row.values[table.schema().field_index(&field.name).unwrap()].clone())
        {
            for operator in [
                ComparisonOperator::Equal,
                ComparisonOperator::Less,
                ComparisonOperator::GreaterOrEqual,
            ] {
                let query = QueryRequest {
                    filter: Some(Criteria::Compare {
                        left: QueryExpression::field(&field.name),
                        operator,
                        right: QueryExpression::literal(value.clone()),
                    }),
                    ..QueryRequest::default()
                };
                assert_eq!(
                    QueryEngine::execute_provider(&mut adapter, &query).unwrap(),
                    QueryEngine::execute(&table, &query).unwrap()
                );
            }
        }
    }
}

#[test]
fn sqlite_pushdown_preserves_local_query_scalar_and_working_capacity_errors() {
    let connection = Connection::open_in_memory().unwrap();
    fixture(&connection);
    let limits = DataLimits {
        max_text_bytes: 8,
        max_bytes: 600,
        ..DataLimits::default()
    };
    let mut adapter = SqliteAdapter::with_limits(connection, config(), limits).unwrap();
    let table = adapter.load().unwrap();
    let oversized_literal = Criteria::Compare {
        left: QueryExpression::field("Name"),
        operator: ComparisonOperator::Equal,
        right: QueryExpression::literal(CellValue::Text("over the limit".into())),
    };
    let requests = [
        QueryRequest {
            filter: Some(oversized_literal.clone()),
            ..QueryRequest::default()
        },
        QueryRequest {
            order_by: vec![
                QueryOrder {
                    expression: QueryExpression::field("Name"),
                    direction: SortDirection::Ascending
                };
                10
            ],
            ..QueryRequest::default()
        },
    ];
    for request in requests {
        assert_eq!(
            adapter.query_execution(&request).unwrap(),
            QueryExecution::Pushdown
        );
        assert!(matches!(
            QueryEngine::execute(&table, &request),
            Err(DataError::Capacity(_))
        ));
        assert!(matches!(
            QueryEngine::execute_provider(&mut adapter, &request),
            Err(DataError::Capacity(_))
        ));
    }
    // A short-circuited literal is not evaluated locally or by SQL validation.
    let request = QueryRequest {
        filter: Some(Criteria::All(vec![
            Criteria::Constant(false),
            oversized_literal,
        ])),
        ..QueryRequest::default()
    };
    assert_eq!(
        QueryEngine::execute_provider(&mut adapter, &request).unwrap(),
        QueryEngine::execute(&table, &request).unwrap()
    );
}

#[test]
fn sqlite_page_byte_limits_and_unsupported_paged_aggregates_are_explicit() {
    let connection = Connection::open_in_memory().unwrap();
    fixture(&connection);
    let limits = DataLimits {
        max_bytes: 200,
        ..DataLimits::default()
    };
    let mut adapter = SqliteAdapter::with_limits(connection, config(), limits).unwrap();
    assert!(adapter.query_page(&QueryRequest::default(), 0, 1).is_ok());
    assert!(matches!(
        adapter.query_page(&QueryRequest::default(), 0, 3),
        Err(DataError::Capacity(_))
    ));
    let aggregate = QueryRequest {
        aggregates: vec![Aggregate {
            function: AggregateFunction::CountRows,
            expression: None,
        }],
        ..QueryRequest::default()
    };
    assert!(matches!(
        adapter.query_page(&aggregate, 0, 1),
        Err(DataError::Validation(_))
    ));
}

#[test]
fn sqlite_unsupported_query_uses_complete_local_fallback_including_decimal() {
    let connection = Connection::open_in_memory().unwrap();
    connection.execute_batch("CREATE TABLE T(Id INTEGER PRIMARY KEY,D TEXT); INSERT INTO T VALUES (1,'10'),(2,'2'),(3,'-1')").unwrap();
    let config = SqliteTableConfig::new(
        "T",
        "Id",
        TableSchema::new(vec![FieldSchema::new("D", FieldType::Decimal)]).unwrap(),
    )
    .unwrap();
    let mut adapter = SqliteAdapter::from_connection(connection, config).unwrap();
    let query = sorted("D", SortDirection::Ascending);
    assert_eq!(
        adapter.query_execution(&query).unwrap(),
        QueryExecution::Local
    );
    assert_eq!(
        QueryEngine::execute_provider(&mut adapter, &query)
            .unwrap()
            .row_keys,
        vec![3, 2, 1]
    );
    let page = adapter.query_page(&query, 1, 1).unwrap();
    assert_eq!(
        (page.execution, page.total_rows, page.table.rows()[0].key),
        (QueryExecution::Local, 3, 2)
    );
    let mut adapter = super::sqlite_tests::adapter();
    let query = QueryRequest {
        filter: Some(Criteria::TextMatch {
            text: QueryExpression::field("Name"),
            pattern: QueryExpression::literal(CellValue::Text("ALPHA".into())),
            operator: TextMatchOperator::Contains,
            comparison: TextComparison::UnicodeLowercase,
        }),
        ..QueryRequest::default()
    };
    assert_eq!(
        adapter.query_execution(&query).unwrap(),
        QueryExecution::Local
    );
    assert_eq!(
        QueryEngine::execute_provider(&mut adapter, &query)
            .unwrap()
            .row_keys,
        vec![-10, 20]
    );
}

#[test]
fn sqlite_paging_is_bounded_stable_and_does_not_relax_unpaged_snapshot_limits() {
    let connection = Connection::open_in_memory().unwrap();
    connection
        .execute_batch("CREATE TABLE Items(Id INTEGER PRIMARY KEY,Name TEXT,N INTEGER)")
        .unwrap();
    for key in (-20..20).rev() {
        connection
            .execute(
                "INSERT INTO Items VALUES(?1,'same',?2)",
                params![key, key % 3],
            )
            .unwrap();
    }
    let limits = DataLimits {
        max_rows: 3,
        ..DataLimits::default()
    };
    let mut adapter = SqliteAdapter::with_limits(connection, config(), limits).unwrap();
    assert!(matches!(adapter.load(), Err(DataError::Capacity(_))));
    let query = sorted("Name", SortDirection::Descending);
    assert!(matches!(
        QueryEngine::execute_provider(&mut adapter, &query),
        Err(DataError::Capacity(_))
    ));
    let mut keys = Vec::new();
    for offset in (0..42).step_by(3) {
        let page = adapter.query_page(&query, offset, 3).unwrap();
        assert_eq!(page.total_rows, 40);
        assert_eq!(page.execution, QueryExecution::Pushdown);
        keys.extend(page.table.rows().iter().map(|row| row.key));
    }
    assert_eq!(keys, (-20..20).collect::<Vec<_>>());
    for (offset, limit) in [(0, 0), (0, 4), (usize::MAX, 1)] {
        assert!(adapter.query_page(&query, offset, limit).is_err());
    }
    assert!(
        adapter
            .query_page(&query, 100, 3)
            .unwrap()
            .table
            .rows()
            .is_empty()
    );
}

#[test]
fn sqlite_invalid_hidden_storage_and_duplicate_keys_are_not_hidden_by_sql_filter() {
    for sql in [
        "INSERT INTO T VALUES(1,'bad');",
        "INSERT INTO T VALUES(1,1),(1,2);",
    ] {
        let connection = Connection::open_in_memory().unwrap();
        connection.execute_batch("CREATE TABLE T(Id,N)").unwrap();
        connection.execute_batch(sql).unwrap();
        let config = SqliteTableConfig::new(
            "T",
            "Id",
            TableSchema::new(vec![FieldSchema::new("N", FieldType::Integer)]).unwrap(),
        )
        .unwrap();
        let mut adapter = SqliteAdapter::from_connection(connection, config).unwrap();
        let query = QueryRequest {
            filter: Some(Criteria::Constant(false)),
            ..QueryRequest::default()
        };
        assert!(QueryEngine::execute_provider(&mut adapter, &query).is_err());
        assert!(adapter.query_page(&query, 0, 1).is_err());
    }
}

#[test]
fn sqlite_concurrent_schema_change_is_rejected_before_pushdown() {
    let database = Database::new();
    let connection = database.connect();
    fixture(&connection);
    let external = database.connect();
    let mut adapter = SqliteAdapter::from_connection(connection, config()).unwrap();
    adapter.query_schema().unwrap();
    external
        .execute_batch("ALTER TABLE Items ADD COLUMN NewField TEXT")
        .unwrap();
    assert!(matches!(
        adapter.query_pushdown(&QueryRequest::default()),
        Err(DataError::Conflict(_))
    ));
    assert_eq!(
        QueryEngine::execute_provider(&mut adapter, &QueryRequest::default())
            .unwrap()
            .row_keys,
        vec![-10, 20, 30]
    );
}

#[test]
fn sqlite_utf16_database_uses_local_ordering() {
    let connection = Connection::open_in_memory().unwrap();
    connection
        .execute_batch("PRAGMA encoding='UTF-16le'")
        .unwrap();
    fixture(&connection);
    let mut adapter = SqliteAdapter::from_connection(connection, config()).unwrap();
    let query = sorted("Name", SortDirection::Ascending);
    assert_eq!(
        adapter.query_execution(&query).unwrap(),
        QueryExecution::Local
    );
    let expected = QueryEngine::execute(&adapter.load().unwrap(), &query).unwrap();
    assert_eq!(
        QueryEngine::execute_provider(&mut adapter, &query).unwrap(),
        expected
    );
    assert_eq!(
        adapter.query_page(&query, 0, 3).unwrap().execution,
        QueryExecution::Local
    );
}

#[test]
fn sqlite_page_rows_and_count_share_a_snapshot_during_wal_writes() {
    let database = Database::new();
    let connection = database.connect();
    connection.execute_batch("PRAGMA journal_mode=WAL; CREATE TABLE Items(Id INTEGER PRIMARY KEY,Name TEXT,N INTEGER); INSERT INTO Items VALUES(0,'initial',0)").unwrap();
    let writer = database.connect();
    let done = Arc::new(AtomicBool::new(false));
    let finished = done.clone();
    let worker = std::thread::spawn(move || {
        for key in 1..=50 {
            writer
                .execute("INSERT INTO Items VALUES(?1,'next',?1)", [key])
                .unwrap();
            std::thread::sleep(Duration::from_millis(1));
        }
        finished.store(true, Ordering::Release);
    });
    let mut adapter = SqliteAdapter::from_connection(connection, config()).unwrap();
    let mut observed = BTreeSet::new();
    loop {
        let page = adapter
            .query_page(&QueryRequest::default(), 0, 100)
            .unwrap();
        assert_eq!(page.total_rows, page.table.rows().len());
        assert_eq!(
            page.table.rows().last().unwrap().key as usize + 1,
            page.total_rows
        );
        observed.insert(page.total_rows);
        if done.load(Ordering::Acquire) {
            break;
        }
    }
    worker.join().unwrap();
    assert!(observed.len() > 1, "concurrent snapshots were observed");
    assert_eq!(
        adapter
            .query_page(&QueryRequest::default(), 0, 100)
            .unwrap()
            .total_rows,
        51
    );
}
