use super::*;
use crate::{
    Criteria, DecimalValue, FieldSchema, FieldType, QueryExpression, QueryOrder, SortDirection,
};
use std::sync::{atomic::AtomicUsize, mpsc};
fn schema() -> TableSchema {
    TableSchema::new(vec![FieldSchema::new("name", FieldType::Text)]).unwrap()
}
fn settings() -> PageCacheSettings {
    PageCacheSettings {
        page_size: 2,
        max_pages: 2,
        max_rows: 4,
        max_bytes: 4096,
        max_pending: 4,
        mailbox_capacity: 4,
    }
}
fn source() -> PagedDataSource {
    PagedDataSource::new(
        schema(),
        DataLimits::default(),
        settings(),
        QueryCapabilities::complete(),
    )
    .unwrap()
}
fn page(keys: &[i64], more: bool, total: PageTotal, version: u64) -> PageResponse {
    PageResponse {
        table: DataTable::new(
            schema(),
            keys.iter()
                .map(|k| DataRow::new(*k, vec![CellValue::Text(format!("row {k}"))]))
                .collect(),
        )
        .unwrap(),
        has_more: more,
        total,
        snapshot_revision: version,
    }
}
fn request(s: &mut PagedDataSource, offset: u64) -> PageRequest {
    s.request_window(offset, 2).unwrap().pop().unwrap()
}
#[test]
fn stale_foreign_duplicate_and_out_of_order_pages_cannot_overwrite_current_query() {
    let mut s = source();
    let old = request(&mut s, 0);
    let foreign = request(&mut source(), 0);
    let query = QueryRequest {
        filter: Some(Criteria::Constant(true)),
        ..Default::default()
    };
    s.set_query(query.clone()).unwrap();
    assert!(old.cancellation.is_cancelled());
    let current = request(&mut s, 0);
    let later = request(&mut s, 2);
    assert_eq!(
        s.complete(old.token, Ok(page(&[90, 91], true, PageTotal::Unknown, 4)))
            .unwrap(),
        PageCompletion::IgnoredStale
    );
    assert_eq!(
        s.complete(foreign.token, Err("foreign".into())).unwrap(),
        PageCompletion::IgnoredStale
    );
    s.complete(later.token, Ok(page(&[3], false, PageTotal::Unknown, 9)))
        .unwrap();
    s.complete(
        current.token,
        Ok(page(&[1, 2], true, PageTotal::Unknown, 9)),
    )
    .unwrap();
    assert_eq!(s.total(), PageTotal::Known(3));
    assert_eq!(s.row_at(2).unwrap().key, 3);
    assert_eq!(s.query(), &query);
    let revision = s.revision();
    assert_eq!(
        s.complete(current.token, Err("duplicate".into())).unwrap(),
        PageCompletion::IgnoredStale
    );
    assert_eq!(s.revision(), revision);
}
#[test]
fn error_retry_gets_new_token_without_losing_accepted_cache_or_selection() {
    let mut s = source();
    s.set_selected_key(Some(i64::MIN)).unwrap();
    let a = request(&mut s, 0);
    s.complete(
        a.token,
        Ok(page(&[i64::MIN, 2], true, PageTotal::Unknown, 1)),
    )
    .unwrap();
    let bad = request(&mut s, 2);
    s.complete(bad.token, Err("é\n".repeat(3000))).unwrap();
    let PageStatus::Failed(error) = s.page_status(2) else {
        panic!("failed state missing")
    };
    assert!(error.len() <= 2048 && !error.contains('\n'));
    assert!(s.request_window(2, 2).unwrap().is_empty());
    let retry = s.retry(2).unwrap();
    assert_ne!(retry.token, bad.token);
    assert!(bad.cancellation.is_cancelled());
    assert_eq!(
        s.complete(bad.token, Ok(page(&[50, 51], true, PageTotal::Unknown, 1)))
            .unwrap(),
        PageCompletion::IgnoredStale
    );
    s.complete(
        retry.token,
        Ok(page(&[3, 4], false, PageTotal::Known(4), 1)),
    )
    .unwrap();
    assert_eq!(s.selected_key(), Some(i64::MIN));
    assert_eq!(s.row_at(0).unwrap().key, i64::MIN);
    assert_eq!(s.stats().errors, 0);
}
#[test]
fn lru_window_cache_stays_bounded_and_selected_key_survives_eviction() {
    let mut s = source();
    for offset in [0, 2] {
        let r = request(&mut s, offset);
        s.complete(
            r.token,
            Ok(page(
                &[offset as i64, offset as i64 + 1],
                true,
                PageTotal::Unknown,
                1,
            )),
        )
        .unwrap();
    }
    s.set_selected_key(Some(3)).unwrap();
    assert!(s.request_window(0, 1).unwrap().is_empty());
    let r = request(&mut s, 4);
    s.complete(r.token, Ok(page(&[4, 5], true, PageTotal::Unknown, 1)))
        .unwrap();
    assert_eq!(s.cached_offsets().collect::<Vec<_>>(), [0, 4]);
    assert_eq!(s.stats().rows, 4);
    assert!(s.stats().bytes <= 4096);
    assert_eq!(s.selected_key(), Some(3));
    assert!(s.cached_row(3).is_none());
    s.refresh().unwrap();
    assert_eq!(s.selected_key(), Some(3));
    assert_eq!(s.total(), PageTotal::Unknown);
    assert_eq!(s.stats().pages, 0);
    s.set_selected_key(None).unwrap();
    assert_eq!(s.selected_key(), None);
}
#[test]
fn malformed_pages_are_atomic_and_can_be_corrected_with_same_pending_token() {
    let mut s = source();
    let a = request(&mut s, 0);
    s.complete(a.token, Ok(page(&[1, 2], true, PageTotal::Unknown, 7)))
        .unwrap();
    let b = request(&mut s, 2);
    let before = s.stats();
    let revision = s.revision();
    for response in [
        page(&[2, 3], true, PageTotal::Unknown, 7),
        page(&[3], true, PageTotal::Unknown, 7),
        page(&[3, 4], true, PageTotal::Known(4), 7),
        page(&[3, 4], true, PageTotal::Unknown, 8),
    ] {
        assert!(s.complete(b.token, Ok(response)).is_err());
        assert_eq!(s.stats(), before);
        assert_eq!(s.revision(), revision);
        assert!(!b.cancellation.is_cancelled());
        assert_eq!(s.row_at(0).unwrap().key, 1);
    }
    s.complete(b.token, Ok(page(&[3, 4], false, PageTotal::Known(4), 7)))
        .unwrap();
    assert_eq!(s.total(), PageTotal::Known(4));
}
#[test]
fn terminal_unknown_total_and_explicit_empty_pages_are_consistent() {
    let mut s = source();
    let r = request(&mut s, 8);
    assert!(
        s.complete(r.token, Ok(page(&[], false, PageTotal::Unknown, 1)))
            .is_err()
    );
    s.complete(r.token, Ok(page(&[], false, PageTotal::Known(3), 1)))
        .unwrap();
    assert_eq!(s.total(), PageTotal::Known(3));
    assert!(s.request_window(8, 2).unwrap().is_empty());
    let r = request(&mut s, 0);
    assert!(
        s.complete(r.token, Ok(page(&[1, 2], true, PageTotal::Known(4), 1)))
            .is_err()
    );
    s.complete(r.token, Ok(page(&[1, 2], true, PageTotal::Unknown, 1)))
        .unwrap();
    let r = request(&mut s, 2);
    s.complete(r.token, Ok(page(&[3], false, PageTotal::Unknown, 1)))
        .unwrap();
    assert_eq!(s.total(), PageTotal::Known(3));
    let mut empty = source();
    let r = request(&mut empty, 0);
    empty
        .complete(r.token, Ok(page(&[], false, PageTotal::Unknown, 2)))
        .unwrap();
    assert_eq!(empty.total(), PageTotal::Known(0));
}
#[test]
fn query_capabilities_and_window_capacity_fail_before_cancellation_or_recency_changes() {
    let mut s = source();
    let r = request(&mut s, 0);
    let revision = s.revision();
    let generation = s.generation();
    assert!(s.request_window(1, 4).is_err());
    assert!(s.request_window(u64::MAX, 2).is_err());
    assert_eq!(s.revision(), revision);
    assert!(!r.cancellation.is_cancelled());
    let bad = QueryRequest {
        order_by: vec![QueryOrder {
            expression: QueryExpression::field("missing"),
            direction: SortDirection::Ascending,
        }],
        ..Default::default()
    };
    assert!(s.set_query(bad).is_err());
    assert_eq!(s.generation(), generation);
    assert!(!r.cancellation.is_cancelled());
    let caps = QueryCapabilities {
        semantics_version: crate::QUERY_SEMANTICS_VERSION,
        ordering: true,
        ..Default::default()
    };
    let mut limited =
        PagedDataSource::new(schema(), DataLimits::default(), settings(), caps).unwrap();
    let pending = request(&mut limited, 0);
    assert!(
        limited
            .set_query(QueryRequest {
                filter: Some(Criteria::Constant(true)),
                ..Default::default()
            })
            .is_err()
    );
    assert!(!pending.cancellation.is_cancelled());
    assert!(
        PagedDataSource::new(
            schema(),
            DataLimits::default(),
            settings(),
            QueryCapabilities::local_only()
        )
        .is_err()
    );
}
#[test]
fn pending_capacity_and_cancel_are_exact_and_do_not_touch_accepted_pages() {
    let mut st = settings();
    st.max_pending = 1;
    let mut s = PagedDataSource::new(
        schema(),
        DataLimits::default(),
        st,
        QueryCapabilities::complete(),
    )
    .unwrap();
    assert!(s.request_window(0, 4).is_err());
    assert_eq!(s.stats().pending, 0);
    let first = request(&mut s, 0);
    assert!(s.request_window(2, 2).is_err());
    assert!(s.cancel(first.token).unwrap());
    assert!(first.cancellation.is_cancelled());
    assert!(!s.cancel(first.token).unwrap());
    let next = request(&mut s, 0);
    assert_ne!(first.token, next.token);
    s.cancel_all().unwrap();
    assert!(next.cancellation.is_cancelled());
    assert_eq!(s.stats().pending, 0);
}
#[test]
fn cache_byte_and_cell_limits_are_enforced_before_eviction() {
    let probe = page(&[1, 2], true, PageTotal::Unknown, 1);
    let size = payload_bytes(&probe.table).unwrap();
    let mut st = settings();
    st.max_bytes = size + 10;
    let mut s = PagedDataSource::new(
        schema(),
        DataLimits::default(),
        st,
        QueryCapabilities::complete(),
    )
    .unwrap();
    let r = request(&mut s, 0);
    s.complete(r.token, Ok(probe)).unwrap();
    let r = request(&mut s, 2);
    s.complete(r.token, Ok(page(&[3, 4], true, PageTotal::Unknown, 1)))
        .unwrap();
    assert_eq!(s.stats().pages, 1);
    assert!(s.row_at(0).is_none());
    let r = request(&mut s, 4);
    let before = s.stats();
    let oversized = PageResponse {
        table: DataTable::new(
            schema(),
            vec![DataRow::new(5, vec![CellValue::Text("x".repeat(1000))])],
        )
        .unwrap(),
        total: PageTotal::Unknown,
        has_more: false,
        snapshot_revision: 1,
    };
    assert!(s.complete(r.token, Ok(oversized)).is_err());
    assert_eq!(s.stats(), before);
    assert!(s.row_at(2).is_some());
    let limits = DataLimits {
        max_text_bytes: 2,
        ..Default::default()
    };
    let mut s =
        PagedDataSource::new(schema(), limits, settings(), QueryCapabilities::complete()).unwrap();
    let r = request(&mut s, 0);
    assert!(
        s.complete(r.token, Ok(page(&[1, 2], true, PageTotal::Unknown, 1)))
            .is_err()
    );
}
#[test]
fn exact_integer_decimal_and_nullable_values_survive_pages() {
    let schema = TableSchema::new(vec![
        FieldSchema::new("integer", FieldType::Integer),
        FieldSchema::new("decimal", FieldType::Decimal),
        FieldSchema {
            nullable: true,
            ..FieldSchema::new("text", FieldType::Text)
        },
    ])
    .unwrap();
    let decimal: DecimalValue = "0.1234567890123456789012345678".parse().unwrap();
    let table = DataTable::new(
        schema.clone(),
        vec![
            DataRow::new(
                i64::MIN,
                vec![
                    CellValue::Integer(i64::MAX),
                    CellValue::Decimal(decimal),
                    CellValue::Null,
                ],
            ),
            DataRow::new(
                i64::MAX,
                vec![
                    CellValue::Integer(i64::MIN),
                    CellValue::Decimal(decimal),
                    CellValue::Text(String::new()),
                ],
            ),
        ],
    )
    .unwrap();
    let mut s = PagedDataSource::new(
        schema,
        DataLimits::default(),
        settings(),
        QueryCapabilities::complete(),
    )
    .unwrap();
    let r = request(&mut s, 0);
    s.complete(
        r.token,
        Ok(PageResponse {
            table: table.clone(),
            total: PageTotal::Known(2),
            has_more: false,
            snapshot_revision: 9,
        }),
    )
    .unwrap();
    assert_eq!(s.page(0).unwrap().table, table);
}
#[test]
fn errors_are_bounded_and_generation_overflow_is_atomic() {
    let mut s = source();
    for offset in [0, 2, 4, 6] {
        let r = request(&mut s, offset);
        s.complete(r.token, Err("failed".into())).unwrap();
    }
    assert_eq!(s.stats().errors, 2);
    let r = request(&mut s, 8);
    s.generation = u64::MAX;
    let before = s.stats();
    assert_eq!(s.refresh(), Err(DataError::RevisionOverflow));
    assert_eq!(s.stats(), before);
    assert!(!r.cancellation.is_cancelled());
    s.generation = r.token.generation;
    s.revision = u64::MAX;
    assert_eq!(
        s.complete(r.token, Ok(page(&[9, 10], true, PageTotal::Unknown, 1))),
        Err(DataError::RevisionOverflow)
    );
    assert!(!r.cancellation.is_cancelled());
}
#[test]
fn mailbox_reentrant_waker_is_outside_lock_and_full_send_does_not_wake() {
    let mut st = settings();
    st.mailbox_capacity = 2;
    let mut s = PagedDataSource::new(
        schema(),
        DataLimits::default(),
        st,
        QueryCapabilities::complete(),
    )
    .unwrap();
    let a = request(&mut s, 0);
    let b = request(&mut s, 2);
    let c = request(&mut s, 4);
    let sender = s.completion_sender();
    let nested = sender.clone();
    let second = b.clone();
    let calls = Arc::new(AtomicUsize::new(0));
    let count = calls.clone();
    let okay = Arc::new(AtomicBool::new(false));
    let success = okay.clone();
    s.set_completion_waker(Some(Arc::new(move || {
        if count.fetch_add(1, Ordering::Relaxed) == 0 {
            success.store(
                nested
                    .try_complete(&second, Ok(page(&[3, 4], true, PageTotal::Unknown, 1)))
                    .is_ok(),
                Ordering::Relaxed,
            )
        }
    })))
    .unwrap();
    sender
        .try_complete(&a, Ok(page(&[1, 2], true, PageTotal::Unknown, 1)))
        .unwrap();
    assert!(okay.load(Ordering::Relaxed));
    assert_eq!(calls.load(Ordering::Relaxed), 2);
    let error = sender
        .try_complete(&c, Ok(page(&[5, 6], true, PageTotal::Unknown, 1)))
        .unwrap_err();
    assert_eq!(error.error, PageSendError::Full);
    assert!(error.result.is_ok());
    assert_eq!(calls.load(Ordering::Relaxed), 2);
    let drained = s.drain_completions(1).unwrap();
    assert_eq!(drained.remaining, 1);
    assert_eq!(drained.results[0].1, Ok(PageCompletion::Applied));
    s.drain_completions(64).unwrap();
    assert_eq!(s.stats().rows, 4);
}
#[test]
fn mailbox_cancellation_drop_and_foreign_tokens_do_not_deliver() {
    let mut s = source();
    let request = request(&mut s, 0);
    let sender = s.completion_sender();
    let mut other = source();
    let foreign = super::tests::request(&mut other, 0);
    assert_eq!(
        sender
            .try_complete(&foreign, Err("foreign".into()))
            .unwrap_err()
            .error,
        PageSendError::ForeignRequest
    );
    sender
        .try_complete(&request, Ok(page(&[1, 2], true, PageTotal::Unknown, 1)))
        .unwrap();
    s.refresh().unwrap();
    assert!(s.drain_completions(64).unwrap().results.is_empty());
    assert_eq!(
        sender
            .try_complete(&request, Err("late".into()))
            .unwrap_err()
            .error,
        PageSendError::Cancelled
    );
    let current = super::tests::request(&mut s, 0);
    drop(s);
    assert!(current.cancellation.is_cancelled());
    assert!(sender.is_closed());
    assert_eq!(
        sender
            .try_complete(&current, Err("late".into()))
            .unwrap_err()
            .error,
        PageSendError::Closed
    );
}
#[test]
fn worker_can_finish_after_query_change_without_blocking_owner_or_replacing_rows() {
    let mut s = source();
    let old = request(&mut s, 0);
    let sender = s.completion_sender();
    let (go, wait) = mpsc::channel();
    let (done, result) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        wait.recv().unwrap();
        let code = sender
            .try_complete(&old, Ok(page(&[90, 91], true, PageTotal::Unknown, 1)))
            .unwrap_err()
            .error;
        done.send(code).unwrap();
    });
    s.refresh().unwrap();
    let current = request(&mut s, 0);
    s.complete(
        current.token,
        Ok(page(&[1, 2], true, PageTotal::Unknown, 2)),
    )
    .unwrap();
    assert_eq!(s.row_at(0).unwrap().key, 1);
    go.send(()).unwrap();
    assert_eq!(
        result
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap(),
        PageSendError::Cancelled
    );
    worker.join().unwrap();
    assert_eq!(s.row_at(0).unwrap().key, 1);
    assert_eq!(s.snapshot_revision(), Some(2));
}

#[test]
fn mailbox_byte_budget_bad_payload_and_duplicate_delivery_remain_bounded() {
    let response = page(&[1, 2], true, PageTotal::Unknown, 1);
    let mut st = settings();
    st.max_bytes = payload_bytes(&response.table).unwrap() + 10;
    let mut s = PagedDataSource::new(
        schema(),
        DataLimits::default(),
        st,
        QueryCapabilities::complete(),
    )
    .unwrap();
    let a = request(&mut s, 0);
    let b = request(&mut s, 2);
    let sender = s.completion_sender();
    sender.try_complete(&a, Ok(response.clone())).unwrap();
    assert_eq!(
        sender
            .try_complete(&b, Ok(page(&[3, 4], true, PageTotal::Unknown, 1)))
            .unwrap_err()
            .error,
        PageSendError::Full
    );
    s.drain_completions(64).unwrap();
    assert_eq!(s.stats().pages, 1);
    assert_eq!(
        sender.try_complete(&a, Ok(response)).unwrap_err().error,
        PageSendError::Cancelled
    );
    let too_many = page(&[3, 4, 5], true, PageTotal::Unknown, 1);
    assert_eq!(
        sender.try_complete(&b, Ok(too_many)).unwrap_err().error,
        PageSendError::PayloadLimit
    );
    assert!(matches!(s.page_status(2), PageStatus::Pending(_)));
    sender
        .try_complete(&b, Ok(page(&[3, 4], true, PageTotal::Unknown, 1)))
        .unwrap();
    s.drain_completions(64).unwrap();
    assert_eq!(s.cached_offsets().collect::<Vec<_>>(), [2]);
    assert!(s.stats().bytes <= st.max_bytes);
}
#[test]
fn terminal_page_does_not_contradict_an_already_cached_later_page() {
    let mut s = source();
    let later = request(&mut s, 4);
    s.complete(later.token, Ok(page(&[5, 6], true, PageTotal::Unknown, 1)))
        .unwrap();
    let earlier = request(&mut s, 0);
    let before = s.stats();
    let revision = s.revision();
    assert!(
        s.complete(earlier.token, Ok(page(&[1], false, PageTotal::Unknown, 1)))
            .is_err()
    );
    assert_eq!(s.total(), PageTotal::Unknown);
    assert_eq!(s.stats(), before);
    assert_eq!(s.revision(), revision);
    assert_eq!(s.row_at(4).unwrap().key, 5);
}
#[test]
fn response_schema_and_request_sequence_overflow_reject_without_partial_mutation() {
    let mut s = source();
    let request = request(&mut s, 0);
    let bad_schema = TableSchema::new(vec![FieldSchema::new("other", FieldType::Integer)]).unwrap();
    let wrong = PageResponse {
        table: DataTable::new(
            bad_schema,
            vec![DataRow::new(1, vec![CellValue::Integer(1)])],
        )
        .unwrap(),
        has_more: false,
        total: PageTotal::Known(1),
        snapshot_revision: 1,
    };
    let before = s.stats();
    assert!(s.complete(request.token, Ok(wrong)).is_err());
    assert_eq!(s.stats(), before);
    assert_eq!(s.snapshot_revision(), None);
    s.sequence = u64::MAX;
    assert_eq!(
        s.request_window(2, 2).unwrap_err(),
        DataError::RevisionOverflow
    );
    assert_eq!(s.stats(), before);
    assert!(!request.cancellation.is_cancelled());
    assert!(!s.set_query(QueryRequest::default()).unwrap());
    assert!(!request.cancellation.is_cancelled());
}
