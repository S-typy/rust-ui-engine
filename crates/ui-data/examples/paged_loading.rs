//! Application-owned workers deliver bounded pages; the owner never joins a
//! worker while reading rows. This finite CLI only joins workers at shutdown.
use rust_desktop_ui_data::*;
use std::sync::{Arc, mpsc};
use std::time::Duration;
fn schema() -> TableSchema {
    TableSchema::new(vec![FieldSchema::new("caption", FieldType::Text)]).unwrap()
}
fn result(request: &PageRequest, revision: u64) -> Result<PageResponse, String> {
    // A finite provider applies the full typed query before slicing. Real remote
    // adapters implement the same declared semantics against their own snapshot.
    let table = DataTable::new(
        schema(),
        (0..4)
            .map(|key| DataRow::new(key, vec![CellValue::Text(format!("Row {key}"))]))
            .collect(),
    )
    .map_err(|e| e.to_string())?;
    let queried = QueryEngine::execute(&table, &request.query).map_err(|e| e.to_string())?;
    let offset = usize::try_from(request.offset).map_err(|_| "Offset out of range")?;
    let rows = queried
        .row_keys
        .iter()
        .skip(offset)
        .take(request.limit)
        .map(|key| table.row(*key).unwrap().clone())
        .collect::<Vec<_>>();
    let has_more = offset.saturating_add(rows.len()) < queried.row_keys.len();
    let total = if rows.is_empty() && offset != 0 {
        PageTotal::Known(queried.row_keys.len() as u64)
    } else {
        PageTotal::Unknown
    };
    Ok(PageResponse {
        table: DataTable::new(schema(), rows).map_err(|e| e.to_string())?,
        total,
        has_more,
        snapshot_revision: revision,
    })
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let settings = PageCacheSettings {
        page_size: 2,
        max_pages: 1,
        max_rows: 2,
        max_pending: 2,
        ..Default::default()
    };
    let mut source = PagedDataSource::new(
        schema(),
        DataLimits::default(),
        settings,
        QueryCapabilities::complete(),
    )?;
    let (wake_tx, wake_rx) = mpsc::channel();
    source
        .set_completion_waker(Some(Arc::new(move || {
            let _ = wake_tx.send(());
        })))
        .unwrap();
    let old = source.request_window(0, 2)?.pop().unwrap();
    let old_sender = source.completion_sender();
    let (release_tx, release_rx) = mpsc::channel();
    let old_worker = std::thread::spawn(move || {
        release_rx.recv().unwrap();
        let response = result(&old, 1);
        old_sender.try_complete(&old, response).unwrap_err().error
    });
    source.set_query(QueryRequest {
        filter: Some(Criteria::Constant(true)),
        ..Default::default()
    })?;
    let request = source.request_window(0, 2)?.pop().unwrap();
    let sender = source.completion_sender();
    let worker = std::thread::spawn(move || {
        let response = result(&request, 2);
        sender.try_complete(&request, response).unwrap();
    });
    wake_rx.recv_timeout(Duration::from_secs(5))?;
    assert_eq!(
        source.drain_completions(16).unwrap().results[0].1,
        Ok(PageCompletion::Applied)
    );
    assert_eq!(source.total(), PageTotal::Unknown);
    source.set_selected_key(Some(0))?;
    release_tx.send(())?;
    assert_eq!(old_worker.join().unwrap(), PageSendError::Cancelled);
    worker.join().unwrap();
    assert_eq!(source.row_at(0).unwrap().key, 0);
    assert_eq!(source.snapshot_revision(), Some(2));
    let next = source.request_window(2, 2)?.pop().unwrap();
    source.complete(next.token, result(&next, 2))?;
    assert_eq!(source.total(), PageTotal::Known(4));
    assert_eq!(source.stats().pages, 1);
    assert!(source.cached_row(0).is_none());
    assert_eq!(source.selected_key(), Some(0));
    let again = source.request_window(0, 2)?.pop().unwrap();
    source.complete(again.token, Err("Temporary provider error".into()))?;
    let retry = source.retry(0)?;
    assert_ne!(retry.token, again.token);
    source.complete(retry.token, result(&retry, 2))?;
    assert_eq!(source.row_at(0).unwrap().key, 0);
    assert_eq!(source.selected_key(), Some(0));
    println!(
        "PASS: worker delivery, stale cancellation, coherent revision, unknown total, bounded eviction, stable selected key, explicit retry"
    );
    Ok(())
}
