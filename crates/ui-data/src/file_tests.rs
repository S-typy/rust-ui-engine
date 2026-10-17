use super::*;
use crate::{DataAdapter, DataWrite, FieldSchema};

fn sample() -> (FileTableMapping, DataTable) {
    let kinds = [
        FieldType::Bool,
        FieldType::Integer,
        FieldType::Real,
        FieldType::Text,
        FieldType::Decimal,
        FieldType::Date,
        FieldType::Time,
        FieldType::Binary,
    ];
    let schema = TableSchema::new(
        kinds
            .into_iter()
            .enumerate()
            .map(|(i, kind)| {
                let mut f = FieldSchema::new(format!("f{i}"), kind);
                f.nullable = true;
                f
            })
            .collect(),
    )
    .unwrap();
    let mapping = FileTableMapping::new(schema.clone(), "key").unwrap();
    let table = DataTable::new(
        schema,
        vec![
            DataRow::new(
                i64::MIN,
                vec![
                    CellValue::Bool(true),
                    CellValue::Integer(i64::MAX),
                    CellValue::Real(-0.0),
                    CellValue::Text("Unicode é 🙂,\"\r\nnext".into()),
                    CellValue::Decimal("0.1234567890123456789012345678".parse().unwrap()),
                    CellValue::Date("2000-02-29".parse().unwrap()),
                    CellValue::Time("23:59:59.123456789".parse().unwrap()),
                    CellValue::Binary(vec![0, 255, 3]),
                ],
            ),
            DataRow::new(i64::MAX, vec![CellValue::Null; 8]),
            DataRow::new(
                0,
                vec![
                    CellValue::Bool(false),
                    CellValue::Integer(i64::MIN),
                    CellValue::Real(0.1),
                    CellValue::Text(String::new()),
                    CellValue::Decimal("-12345.67".parse().unwrap()),
                    CellValue::Date("0001-01-01".parse().unwrap()),
                    CellValue::Time("00:00".parse().unwrap()),
                    CellValue::Binary(Vec::new()),
                ],
            ),
        ],
    )
    .unwrap();
    (mapping, table)
}
#[test]
fn file_codecs_round_trip_every_type_and_exact_key_across_encodings() {
    let (mapping, table) = sample();
    for format in [
        DataFileFormat::Json,
        DataFileFormat::Csv(CsvOptions::default()),
        DataFileFormat::Csv(CsvOptions { delimiter: b';' }),
    ] {
        for encoding in [
            FileEncoding::Utf8,
            FileEncoding::Utf8Bom,
            FileEncoding::Utf16Le,
            FileEncoding::Utf16Be,
        ] {
            let options = DataFileOptions {
                format,
                encoding,
                ..Default::default()
            };
            let bytes = encode_data_file(&table, &mapping, options).unwrap();
            let restored = decode_data_file(&bytes, &mapping, options).unwrap();
            assert_eq!(restored, table, "{format:?}/{encoding:?}");
            let CellValue::Real(value) = restored.rows()[0].values[2] else {
                panic!()
            };
            assert!(value.is_sign_negative());
        }
    }
}
#[test]
fn json_duplicate_unknown_missing_and_lossy_scalars_are_rejected() {
    let schema = TableSchema::new(vec![FieldSchema::new("number", FieldType::Integer)]).unwrap();
    let mapping = FileTableMapping::new(schema, "key").unwrap();
    for source in [
        r#"[{"key":"1","number":"2","number":"3"}]"#,
        r#"[{"key":"1","key":"2","number":"3"}]"#,
        r#"[{"key":"1","extra":0,"number":"2"}]"#,
        r#"[{"key":"1"}]"#,
        r#"[{"key":1.0,"number":"2"}]"#,
        r#"[{"key":"1","number":9223372036854775808}]"#,
        r#"[{"key":"1","number":"01"}]"#,
        r#"[{"key":"1","number":2.5}]"#,
        r#"[{"key":"1","number":"2"},{"key":"1","number":"3"}]"#,
    ] {
        assert!(
            decode_data_file(source.as_bytes(), &mapping, DataFileOptions::default()).is_err(),
            "{source}"
        );
    }
    let valid = decode_data_file(
        br#"[{"key":-9223372036854775808,"number":9223372036854775807}]"#,
        &mapping,
        DataFileOptions::default(),
    )
    .unwrap();
    assert_eq!(valid.rows()[0].key, i64::MIN);
    assert_eq!(valid.rows()[0].values[0], CellValue::Integer(i64::MAX));
}
#[test]
fn csv_null_empty_literal_null_quotes_multiline_and_reordered_header_are_distinct() {
    let mut field = FieldSchema::new("text", FieldType::Text);
    field.nullable = true;
    let schema = TableSchema::new(vec![field]).unwrap();
    let mapping = FileTableMapping::new(schema, "key").unwrap();
    let options = DataFileOptions {
        format: DataFileFormat::Csv(CsvOptions::default()),
        ..Default::default()
    };
    let source = "text,key\r\n\\N,1\r\n\"\\N\",2\r\n,3\r\n\"a,\"\"b\"\"\r\nc\",4\r\n";
    let table = decode_data_file(source.as_bytes(), &mapping, options).unwrap();
    assert_eq!(
        table
            .rows()
            .iter()
            .map(|r| r.values[0].clone())
            .collect::<Vec<_>>(),
        vec![
            CellValue::Null,
            CellValue::Text("\\N".into()),
            CellValue::Text("".into()),
            CellValue::Text("a,\"b\"\r\nc".into())
        ]
    );
    let encoded = encode_data_file(&table, &mapping, options).unwrap();
    assert_eq!(
        decode_data_file(&encoded, &mapping, options).unwrap(),
        table
    );
    for bad in [
        "key,text\n1,\"open",
        "key,text\n1,a\"b",
        "key,text\n1,\"closed\"extra",
        "key,text\n1,a,b",
        "key,key\n1,2",
        "key,unknown\n1,x",
    ] {
        assert!(decode_data_file(bad.as_bytes(), &mapping, options).is_err());
    }
}
#[test]
fn file_encoding_and_limits_fail_before_any_table_is_returned() {
    let (mapping, table) = sample();
    let options = DataFileOptions::default();
    for bytes in [vec![0xff], vec![0xff, 0xfe, b'[', 0], vec![0xef, 0xbb]] {
        assert!(decode_data_file(&bytes, &mapping, options).is_err());
    }
    for bytes in [
        vec![0xff, 0xfe, 0x00, 0xd8],
        vec![0xfe, 0xff, 0, 0],
        vec![0xff, 0xfe, 0],
    ] {
        assert!(
            decode_data_file(
                &bytes,
                &mapping,
                DataFileOptions {
                    encoding: FileEncoding::Utf16Le,
                    ..options
                }
            )
            .is_err()
        );
    }
    let small = DataFileOptions {
        max_file_bytes: 16,
        ..options
    };
    assert!(encode_data_file(&table, &mapping, small).is_err());
    let bytes = encode_data_file(&table, &mapping, options).unwrap();
    assert!(decode_data_file(&bytes, &mapping, small).is_err());
    let mut limited = options;
    limited.limits.max_rows = 1;
    assert!(decode_data_file(&bytes, &mapping, limited).is_err());
    limited = options;
    limited.limits.max_text_bytes = 2;
    assert!(decode_data_file(&bytes, &mapping, limited).is_err());
    limited = options;
    limited.limits.max_bytes = 1;
    assert!(decode_data_file(&bytes, &mapping, limited).is_err());
}
struct Directory(std::path::PathBuf);
impl Directory {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let path = std::env::temp_dir().join(format!(
            "rust-ui-file-tests-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
#[test]
fn file_adapter_atomic_batch_conflicts_and_invalid_reload_preserve_accepted_snapshot() {
    let dir = Directory::new();
    let path = dir.0.join("table.json");
    let (mapping, table) = sample();
    let mut adapter = FileDataAdapter::create(
        &path,
        table.clone(),
        mapping.clone(),
        DataFileOptions::default(),
    )
    .unwrap();
    let old = std::fs::read(&path).unwrap();
    let update = DataWrite::Update {
        key: 0,
        field: "f3".into(),
        expected: CellValue::Text("".into()),
        value: CellValue::Text("changed".into()),
    };
    let mut invalid = update.clone();
    if let DataWrite::Update { expected, .. } = &mut invalid {
        *expected = CellValue::Text("wrong".into());
    }
    assert!(adapter.write_batch(&[update.clone(), invalid]).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), old);
    assert_eq!(adapter.accepted_table(), &table);
    let changed = adapter.write_batch(&[update]).unwrap();
    assert_eq!(
        changed.value(0, "f3"),
        Some(&CellValue::Text("changed".into()))
    );
    assert_eq!(adapter.load().unwrap(), changed);
    std::fs::write(&path, b"broken").unwrap();
    assert!(adapter.load().is_err());
    assert_eq!(adapter.accepted_table(), &changed);
    assert!(
        adapter
            .write_cell(
                0,
                "f3",
                &CellValue::Text("changed".into()),
                &CellValue::Text("overwrite".into())
            )
            .is_err()
    );
    assert_eq!(std::fs::read(&path).unwrap(), b"broken");
    assert_eq!(adapter.accepted_table(), &changed);
    assert!(std::fs::read_dir(&dir.0).unwrap().all(|entry| {
        !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .ends_with(".tmp")
    }));
    assert!(FileDataAdapter::create(&path, table, mapping, DataFileOptions::default()).is_err());
}
#[test]
fn file_adapter_readonly_and_shared_writer_lock_do_not_modify_storage() {
    let dir = Directory::new();
    let path = dir.0.join("table.csv");
    let (mapping, table) = sample();
    let options = DataFileOptions {
        format: DataFileFormat::Csv(CsvOptions::default()),
        ..Default::default()
    };
    let mut writer =
        FileDataAdapter::create(&path, table.clone(), mapping.clone(), options).unwrap();
    let bytes = std::fs::read(&path).unwrap();
    let mut reader = FileDataAdapter::open(&path, mapping, options, FileAccess::ReadOnly).unwrap();
    assert!(!reader.write_capabilities().atomic_batch);
    assert!(
        reader
            .write_cell(
                0,
                "f3",
                &CellValue::Text("".into()),
                &CellValue::Text("blocked".into())
            )
            .is_err()
    );
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(dir.0.join("table.csv.ui-data.lock"))
        .unwrap();
    lock.lock().unwrap();
    assert!(
        writer
            .write_cell(
                0,
                "f3",
                &CellValue::Text("".into()),
                &CellValue::Text("locked".into())
            )
            .is_err()
    );
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    drop(lock);
    writer
        .write_cell(
            0,
            "f3",
            &CellValue::Text("".into()),
            &CellValue::Text("unlocked".into()),
        )
        .unwrap();
}

#[test]
fn source_revisions_advance_only_after_complete_file_reload_or_transaction() {
    let dir = Directory::new();
    let path = dir.0.join("source.json");
    let (mapping, table) = sample();
    let adapter = FileDataAdapter::create(
        &path,
        table.clone(),
        mapping.clone(),
        DataFileOptions::default(),
    )
    .unwrap();
    let mut source = crate::DataSource::new(adapter).unwrap();
    let revision = source.revision();
    std::fs::write(&path, br#"[{"key":"1","f0":true},"broken"]"#).unwrap();
    assert!(source.refresh().is_err());
    assert_eq!(source.revision(), revision);
    assert_eq!(source.table(), &table);
    let update = DataWrite::Update {
        key: 0,
        field: "f3".into(),
        expected: CellValue::Text("".into()),
        value: CellValue::Text("save".into()),
    };
    assert!(source.write_batch(std::slice::from_ref(&update)).is_err());
    assert_eq!(source.revision(), revision);
    assert_eq!(source.table(), &table);
    std::fs::write(
        &path,
        encode_data_file(&table, &mapping, DataFileOptions::default()).unwrap(),
    )
    .unwrap();
    assert!(!source.refresh().unwrap());
    let inserted = DataRow::new(55, vec![CellValue::Null; 8]);
    assert!(
        source
            .write_batch(&[
                update,
                DataWrite::Insert(inserted.clone()),
                DataWrite::Delete {
                    expected: table.rows()[1].clone()
                }
            ])
            .unwrap()
    );
    assert_eq!(source.revision(), revision + 1);
    assert_eq!(source.row(55), Some(&inserted));
    assert!(source.row(i64::MAX).is_none());
    let reloaded = decode_data_file(
        &std::fs::read(path).unwrap(),
        &mapping,
        DataFileOptions::default(),
    )
    .unwrap();
    assert_eq!(&reloaded, source.table());
}

#[cfg(windows)]
#[test]
fn failed_windows_atomic_replace_keeps_original_and_cleans_temporary_file() {
    use std::os::windows::fs::OpenOptionsExt;
    let dir = Directory::new();
    let path = dir.0.join("locked.json");
    let (mapping, table) = sample();
    let mut adapter =
        FileDataAdapter::create(&path, table.clone(), mapping, DataFileOptions::default()).unwrap();
    let original = std::fs::read(&path).unwrap();
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(&path)
        .unwrap();
    assert!(
        adapter
            .write_cell(
                0,
                "f3",
                &CellValue::Text("".into()),
                &CellValue::Text("blocked".into())
            )
            .is_err()
    );
    assert_eq!(std::fs::read(&path).unwrap(), original);
    assert_eq!(adapter.accepted_table(), &table);
    assert!(std::fs::read_dir(&dir.0).unwrap().all(|entry| {
        !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .ends_with(".tmp")
    }));
    drop(lock);
    adapter
        .write_cell(
            0,
            "f3",
            &CellValue::Text("".into()),
            &CellValue::Text("success".into()),
        )
        .unwrap();
}

#[test]
fn file_diagnostics_are_bounded_and_never_echo_large_cell_payloads() {
    let (mapping, _) = sample();
    let payload = "private_payload".repeat(20000);
    for source in [
        format!("\"{payload}\""),
        format!("[\"{payload}\"]"),
        format!("[{{\"{payload}\":0}}]"),
    ] {
        let error = decode_data_file(source.as_bytes(), &mapping, DataFileOptions::default())
            .unwrap_err()
            .to_string();
        assert!(error.len() < 1024);
        assert!(!error.contains("private_payload"));
    }
}

#[test]
fn empty_table_schema_and_encoded_byte_limits_are_enforced() {
    let (mapping, table) = sample();
    let empty = DataTable::new(table.schema().clone(), Vec::new()).unwrap();
    for options in [
        DataFileOptions {
            limits: DataLimits {
                max_fields: 1,
                ..Default::default()
            },
            ..Default::default()
        },
        DataFileOptions {
            limits: DataLimits {
                max_bytes: 1,
                ..Default::default()
            },
            ..Default::default()
        },
    ] {
        assert!(encode_data_file(&empty, &mapping, options).is_err());
        assert!(decode_data_file(b"[]", &mapping, options).is_err());
    }
    for encoding in [
        FileEncoding::Utf8Bom,
        FileEncoding::Utf16Le,
        FileEncoding::Utf16Be,
    ] {
        let options = DataFileOptions {
            encoding,
            max_file_bytes: 4,
            ..Default::default()
        };
        assert!(encode_data_file(&empty, &mapping, options).is_err());
    }
}

#[test]
fn json_overflow_integer_tokens_never_bypass_real_exactness_via_f64_dispatch() {
    let schema = TableSchema::new(vec![
        FieldSchema::new("real", FieldType::Real),
        FieldSchema::new("text", FieldType::Text),
    ])
    .unwrap();
    let mapping = FileTableMapping::new(schema, "key").unwrap();
    for token in [
        "18446744073709551617",
        "-9223372036854775809",
        "9007199254740993",
        "18446744073709551615",
    ] {
        let json = format!(r#"[{{"key":"1","real":{token},"text":"kept"}}]"#);
        assert!(
            decode_data_file(json.as_bytes(), &mapping, DataFileOptions::default()).is_err(),
            "{token}"
        );
    }
    for token in ["18446744073709551616", "-18446744073709551616", "1.25e20"] {
        let json =
            format!(r#"[{{"key":"1","real":{token},"text":"18446744073709551617 \\\" digits"}}]"#);
        let table =
            decode_data_file(json.as_bytes(), &mapping, DataFileOptions::default()).unwrap();
        assert_eq!(
            table.rows()[0].values[0],
            CellValue::Real(token.parse().unwrap())
        );
        let encoded = encode_data_file(&table, &mapping, DataFileOptions::default()).unwrap();
        assert_eq!(
            decode_data_file(&encoded, &mapping, DataFileOptions::default()).unwrap(),
            table
        );
    }
}

#[test]
fn every_supported_csv_delimiter_preserves_null_and_typed_records() {
    let (mapping, table) = sample();
    for delimiter in 0x20u8..=0x7e {
        let options = DataFileOptions {
            format: DataFileFormat::Csv(CsvOptions { delimiter }),
            ..Default::default()
        };
        if matches!(delimiter, b'"' | b'\\' | b'N') {
            assert!(options.validate().is_err());
            continue;
        }
        let encoded = encode_data_file(&table, &mapping, options).unwrap();
        assert_eq!(
            decode_data_file(&encoded, &mapping, options).unwrap(),
            table,
            "delimiter {}",
            char::from(delimiter)
        );
    }
}
