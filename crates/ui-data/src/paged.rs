//! Bounded asynchronous row pages. Providers own transport and worker execution.
use crate::{
    CellValue, DataError, DataLimits, DataRow, DataTable, QueryCapabilities, QueryEngine,
    QueryExecution, QueryRequest, TableSchema,
};
use std::{
    collections::{BTreeMap, HashSet},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};
mod mailbox;
pub use mailbox::{PageCompletionSender, PageDrain, PageSendError, PageSendFailure};
#[cfg(test)]
mod tests;
static NEXT_SOURCE: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PageTotal {
    Unknown,
    Known(u64),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PageCacheSettings {
    pub page_size: usize,
    pub max_pages: usize,
    pub max_rows: usize,
    /// Logical payload budget (lengths, not incoming allocation spare capacity).
    pub max_bytes: usize,
    pub max_pending: usize,
    pub mailbox_capacity: usize,
}
impl Default for PageCacheSettings {
    fn default() -> Self {
        Self {
            page_size: 256,
            max_pages: 8,
            max_rows: 8192,
            max_bytes: 32 * 1024 * 1024,
            max_pending: 8,
            mailbox_capacity: 8,
        }
    }
}
impl PageCacheSettings {
    pub fn validate(&self) -> Result<(), DataError> {
        if !(1..=4096).contains(&self.page_size)
            || !(1..=256).contains(&self.max_pages)
            || self.max_rows < self.page_size
            || self.max_rows > 65536
            || self.max_bytes == 0
            || self.max_bytes > 64 * 1024 * 1024
            || !(1..=64).contains(&self.max_pending)
            || !(1..=64).contains(&self.mailbox_capacity)
        {
            return Err(capacity("invalid page/cache/mailbox bounds"));
        }
        Ok(())
    }
}
/// Cooperative cancellation. Providers should check it around expensive work;
/// token validation independently prevents a late response from being published.
#[derive(Clone, Debug)]
pub struct PageCancellation(Arc<AtomicBool>);
impl PageCancellation {
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}
/// Opaque source-scoped request identity. A retry always has a new sequence.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PageToken {
    source: u64,
    generation: u64,
    sequence: u64,
    offset: u64,
}
impl PageToken {
    pub fn offset(self) -> u64 {
        self.offset
    }
    pub fn generation(self) -> u64 {
        self.generation
    }
}
#[derive(Clone, Debug)]
pub struct PageRequest {
    pub token: PageToken,
    pub offset: u64,
    pub limit: usize,
    pub query: Arc<QueryRequest>,
    pub cancellation: PageCancellation,
}
/// One page from a coherent provider snapshot. Stable signed keys are never row
/// indices. Providers must not reuse snapshot_revision for different row order/data.
#[derive(Clone, Debug, PartialEq)]
pub struct PageResponse {
    pub table: DataTable,
    pub total: PageTotal,
    pub has_more: bool,
    pub snapshot_revision: u64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PageStatus {
    Missing,
    Pending(PageToken),
    Ready { rows: usize, has_more: bool },
    Failed(String),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PageCompletion {
    Applied,
    Failed,
    IgnoredStale,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PageCacheStats {
    pub pages: usize,
    pub rows: usize,
    pub bytes: usize,
    pub pending: usize,
    pub errors: usize,
}
struct CachedPage {
    response: PageResponse,
    bytes: usize,
    used: u64,
}
struct Pending {
    token: PageToken,
    cancel: PageCancellation,
}

/// Owner-thread cache and request coordinator. No method performs provider I/O;
/// getters never wait for a worker or fabricate unloaded rows.
pub struct PagedDataSource {
    source: u64,
    generation: u64,
    sequence: u64,
    clock: u64,
    revision: u64,
    schema: TableSchema,
    limits: DataLimits,
    settings: PageCacheSettings,
    capabilities: QueryCapabilities,
    query: Arc<QueryRequest>,
    pages: BTreeMap<u64, CachedPage>,
    pending: BTreeMap<u64, Pending>,
    errors: BTreeMap<u64, (String, u64)>,
    total: PageTotal,
    snapshot_revision: Option<u64>,
    selected: Option<i64>,
    mailbox: Arc<mailbox::Shared>,
}
impl PagedDataSource {
    pub fn new(
        schema: TableSchema,
        limits: DataLimits,
        settings: PageCacheSettings,
        capabilities: QueryCapabilities,
    ) -> Result<Self, DataError> {
        settings.validate()?;
        limits.validate()?;
        if limits.max_rows < settings.page_size {
            return Err(capacity("DataLimits.max_rows is below page size"));
        }
        let empty = DataTable::with_limits(schema.clone(), Vec::new(), limits)?;
        if payload_bytes(&empty)? > settings.max_bytes {
            return Err(capacity("schema exceeds page cache bytes"));
        }
        let query = QueryRequest::default();
        validate_query(&schema, &query, capabilities)?;
        let source = NEXT_SOURCE
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |v| v.checked_add(1))
            .map_err(|_| DataError::RevisionOverflow)?;
        Ok(Self {
            source,
            generation: 0,
            sequence: 0,
            clock: 0,
            revision: 0,
            schema,
            limits,
            settings,
            capabilities,
            query: Arc::new(query),
            pages: BTreeMap::new(),
            pending: BTreeMap::new(),
            errors: BTreeMap::new(),
            total: PageTotal::Unknown,
            snapshot_revision: None,
            selected: None,
            mailbox: Arc::new(mailbox::Shared::new(source, settings)),
        })
    }
    pub fn schema(&self) -> &TableSchema {
        &self.schema
    }
    pub fn settings(&self) -> PageCacheSettings {
        self.settings
    }
    pub fn capabilities(&self) -> QueryCapabilities {
        self.capabilities
    }
    pub fn query(&self) -> &QueryRequest {
        &self.query
    }
    pub fn generation(&self) -> u64 {
        self.generation
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn total(&self) -> PageTotal {
        self.total
    }
    pub fn snapshot_revision(&self) -> Option<u64> {
        self.snapshot_revision
    }
    pub fn selected_key(&self) -> Option<i64> {
        self.selected
    }
    /// Selection identity is independent of the current page. Existence outside
    /// the cache belongs to the provider/application, not a guessed row index.
    pub fn set_selected_key(&mut self, key: Option<i64>) -> Result<(), DataError> {
        if self.selected != key {
            let next = self.next_revision()?;
            self.selected = key;
            self.revision = next;
        }
        Ok(())
    }
    pub fn stats(&self) -> PageCacheStats {
        PageCacheStats {
            pages: self.pages.len(),
            rows: self
                .pages
                .values()
                .map(|p| p.response.table.rows().len())
                .sum(),
            bytes: self.pages.values().map(|p| p.bytes).sum(),
            pending: self.pending.len(),
            errors: self.errors.len(),
        }
    }
    pub fn cached_offsets(&self) -> impl Iterator<Item = u64> + '_ {
        self.pages.keys().copied()
    }
    pub fn page(&self, offset: u64) -> Option<&PageResponse> {
        self.pages.get(&self.align(offset)).map(|p| &p.response)
    }
    pub fn row_at(&self, index: u64) -> Option<&DataRow> {
        let offset = self.align(index);
        self.pages
            .get(&offset)?
            .response
            .table
            .rows()
            .get((index - offset) as usize)
    }
    pub fn cached_row(&self, key: i64) -> Option<&DataRow> {
        self.pages.values().find_map(|p| p.response.table.row(key))
    }
    pub fn page_status(&self, offset: u64) -> PageStatus {
        let offset = self.align(offset);
        if let Some(p) = self.pending.get(&offset) {
            PageStatus::Pending(p.token)
        } else if let Some(p) = self.pages.get(&offset) {
            PageStatus::Ready {
                rows: p.response.table.rows().len(),
                has_more: p.response.has_more,
            }
        } else if let Some((e, _)) = self.errors.get(&offset) {
            PageStatus::Failed(e.clone())
        } else {
            PageStatus::Missing
        }
    }
    /// Validate a complete query before cancelling outstanding work. Selection
    /// survives query changes. Reapplying the same query is a no-op; refresh
    /// explicitly starts another provider snapshot even with the same query.
    pub fn set_query(&mut self, query: QueryRequest) -> Result<bool, DataError> {
        validate_query(&self.schema, &query, self.capabilities)?;
        if *self.query == query {
            return Ok(false);
        }
        self.reset(Some(Arc::new(query)))?;
        Ok(true)
    }
    pub fn refresh(&mut self) -> Result<(), DataError> {
        self.reset(None)
    }
    fn reset(&mut self, query: Option<Arc<QueryRequest>>) -> Result<(), DataError> {
        let generation = self
            .generation
            .checked_add(1)
            .ok_or(DataError::RevisionOverflow)?;
        let revision = self.next_revision()?;
        for pending in self.pending.values() {
            pending.cancel.0.store(true, Ordering::Release);
        }
        self.pending.clear();
        self.pages.clear();
        self.errors.clear();
        self.total = PageTotal::Unknown;
        self.snapshot_revision = None;
        self.generation = generation;
        self.revision = revision;
        if let Some(query) = query {
            self.query = query;
        }
        self.mailbox.clear_cancelled();
        Ok(())
    }
    /// Request all missing pages intersecting a bounded window. Cached/pending
    /// pages are reused; failed pages need explicit retry. Capacity failures leave
    /// pending requests and cache recency unchanged.
    pub fn request_window(
        &mut self,
        start: u64,
        count: usize,
    ) -> Result<Vec<PageRequest>, DataError> {
        if count == 0 {
            return Ok(Vec::new());
        }
        if count > self.settings.max_rows {
            return Err(capacity("window row count exceeds cache limit"));
        }
        let end = start
            .checked_add(count as u64)
            .ok_or_else(|| capacity("page window overflow"))?;
        let mut offsets = Vec::new();
        let mut offset = self.align(start);
        while offset < end {
            if self.total_is_before(offset) {
                break;
            }
            offsets.push(offset);
            if offsets.len() > self.settings.max_pages {
                return Err(capacity("window spans too many pages"));
            }
            offset = offset
                .checked_add(self.settings.page_size as u64)
                .ok_or_else(|| capacity("page offset overflow"))?;
        }
        let missing: Vec<_> = offsets
            .iter()
            .copied()
            .filter(|o| {
                !self.pages.contains_key(o)
                    && !self.pending.contains_key(o)
                    && !self.errors.contains_key(o)
            })
            .collect();
        if self.pending.len() + missing.len() > self.settings.max_pending {
            return Err(capacity("too many pending page requests"));
        }
        let sequence = self
            .sequence
            .checked_add(missing.len() as u64)
            .ok_or(DataError::RevisionOverflow)?;
        let clock = self
            .clock
            .checked_add(1)
            .ok_or(DataError::RevisionOverflow)?;
        let revision = if missing.is_empty() {
            self.revision
        } else {
            self.next_revision()?
        };
        let mut requests = Vec::with_capacity(missing.len());
        for offset in missing {
            requests.push(self.make_request(offset));
        }
        debug_assert_eq!(self.sequence, sequence);
        for offset in offsets {
            if let Some(p) = self.pages.get_mut(&offset) {
                p.used = clock;
            }
        }
        self.clock = clock;
        self.revision = revision;
        Ok(requests)
    }
    /// Retry a failed page, keeping other accepted pages and selection intact.
    pub fn retry(&mut self, offset: u64) -> Result<PageRequest, DataError> {
        let offset = self.align(offset);
        if !self.errors.contains_key(&offset) {
            return Err(DataError::Validation("retry requires a failed page".into()));
        }
        if self.pending.len() >= self.settings.max_pending {
            return Err(capacity("too many pending page requests"));
        }
        self.sequence
            .checked_add(1)
            .ok_or(DataError::RevisionOverflow)?;
        let revision = self.next_revision()?;
        let request = self.make_request(offset);
        self.errors.remove(&offset);
        self.revision = revision;
        Ok(request)
    }
    pub fn cancel(&mut self, token: PageToken) -> Result<bool, DataError> {
        if !self.current(token) {
            return Ok(false);
        }
        let revision = self.next_revision()?;
        self.pending
            .remove(&token.offset)
            .unwrap()
            .cancel
            .0
            .store(true, Ordering::Release);
        self.revision = revision;
        Ok(true)
    }
    pub fn cancel_all(&mut self) -> Result<(), DataError> {
        if self.pending.is_empty() {
            return Ok(());
        }
        let revision = self.next_revision()?;
        for p in self.pending.values() {
            p.cancel.0.store(true, Ordering::Release);
        }
        self.pending.clear();
        self.revision = revision;
        Ok(())
    }
    /// Stale/foreign/duplicate results are ignored before inspecting the payload.
    /// A malformed successful response leaves its request pending for a corrected
    /// response. Provider errors complete the request and enable explicit retry.
    pub fn complete(
        &mut self,
        token: PageToken,
        result: Result<PageResponse, String>,
    ) -> Result<PageCompletion, DataError> {
        if !self.current(token) {
            return Ok(PageCompletion::IgnoredStale);
        }
        let revision = self.next_revision()?;
        let clock = self
            .clock
            .checked_add(1)
            .ok_or(DataError::RevisionOverflow)?;
        let response = match result {
            Ok(response) => response,
            Err(message) => {
                let message = bounded_error(&message);
                self.errors.insert(token.offset, (message, clock));
                while self.errors.len() > self.settings.max_pages {
                    let oldest = *self
                        .errors
                        .iter()
                        .min_by_key(|(offset, (_, used))| (*used, **offset))
                        .unwrap()
                        .0;
                    self.errors.remove(&oldest);
                }
                self.finish_pending(token);
                self.clock = clock;
                self.revision = revision;
                return Ok(PageCompletion::Failed);
            }
        };
        let (bytes, total) = self.validate_response(token.offset, &response)?;
        let mut evict = Vec::new();
        let mut pages = self.pages.len() + usize::from(!self.pages.contains_key(&token.offset));
        let mut rows = response.table.rows().len();
        let mut used_bytes = bytes;
        for (offset, p) in &self.pages {
            if *offset != token.offset {
                rows += p.response.table.rows().len();
                used_bytes += p.bytes;
            }
        }
        let mut order: Vec<_> = self
            .pages
            .iter()
            .filter(|(o, _)| **o != token.offset)
            .map(|(o, p)| (p.used, *o))
            .collect();
        order.sort();
        for (_, offset) in order {
            if pages <= self.settings.max_pages
                && rows <= self.settings.max_rows
                && used_bytes <= self.settings.max_bytes
            {
                break;
            }
            let old = &self.pages[&offset];
            pages -= 1;
            rows -= old.response.table.rows().len();
            used_bytes -= old.bytes;
            evict.push(offset);
        }
        if pages > self.settings.max_pages
            || rows > self.settings.max_rows
            || used_bytes > self.settings.max_bytes
        {
            return Err(capacity("page cannot fit cache limits"));
        }
        for offset in evict {
            self.pages.remove(&offset);
        }
        self.snapshot_revision = Some(response.snapshot_revision);
        self.total = total;
        self.pages.insert(
            token.offset,
            CachedPage {
                response,
                bytes,
                used: clock,
            },
        );
        self.errors.remove(&token.offset);
        self.finish_pending(token);
        self.clock = clock;
        self.revision = revision;
        Ok(PageCompletion::Applied)
    }
    fn validate_response(
        &self,
        offset: u64,
        response: &PageResponse,
    ) -> Result<(usize, PageTotal), DataError> {
        if response.table.schema() != &self.schema {
            return Err(DataError::Schema("page schema changed".into()));
        }
        if self
            .snapshot_revision
            .is_some_and(|r| r != response.snapshot_revision)
        {
            return Err(DataError::Conflict(
                "page belongs to a different provider snapshot; refresh required".into(),
            ));
        }
        let rows = response.table.rows().len();
        if rows > self.settings.page_size || rows > self.limits.max_rows {
            return Err(capacity("response exceeds requested page size"));
        }
        let bytes = validate_payload(&response.table, self.limits)?;
        if bytes > self.settings.max_bytes {
            return Err(capacity("page exceeds cache byte limit"));
        }
        let end = offset
            .checked_add(rows as u64)
            .ok_or_else(|| capacity("response range overflow"))?;
        if response.has_more && rows != self.settings.page_size {
            return Err(DataError::Validation(
                "nonfinal pages must contain the requested row count".into(),
            ));
        }
        let total = match (self.total, response.total) {
            (PageTotal::Known(a), PageTotal::Known(b)) if a != b => {
                return Err(DataError::Conflict(
                    "page total changed within a snapshot".into(),
                ));
            }
            (PageTotal::Known(a), _) | (_, PageTotal::Known(a)) => PageTotal::Known(a),
            _ if !response.has_more => {
                if rows == 0 && offset != 0 {
                    return Err(DataError::Validation(
                        "empty noninitial page requires a known total".into(),
                    ));
                }
                PageTotal::Known(end)
            }
            _ => PageTotal::Unknown,
        };
        if let PageTotal::Known(total) = total {
            validate_range(offset, rows, response.has_more, total)?;
            for (o, p) in &self.pages {
                if *o != offset {
                    validate_range(
                        *o,
                        p.response.table.rows().len(),
                        p.response.has_more,
                        total,
                    )?;
                }
            }
        }
        let keys: HashSet<_> = response.table.rows().iter().map(|r| r.key).collect();
        if self
            .pages
            .iter()
            .filter(|(o, _)| **o != offset)
            .any(|(_, p)| {
                p.response
                    .table
                    .rows()
                    .iter()
                    .any(|r| keys.contains(&r.key))
            })
        {
            return Err(DataError::Conflict(
                "stable row key appears in multiple cached pages".into(),
            ));
        }
        Ok((bytes, total))
    }
    fn make_request(&mut self, offset: u64) -> PageRequest {
        self.sequence += 1;
        let token = PageToken {
            source: self.source,
            generation: self.generation,
            sequence: self.sequence,
            offset,
        };
        let cancel = PageCancellation(Arc::new(AtomicBool::new(false)));
        self.pending.insert(
            offset,
            Pending {
                token,
                cancel: cancel.clone(),
            },
        );
        PageRequest {
            token,
            offset,
            limit: self.settings.page_size,
            query: self.query.clone(),
            cancellation: cancel,
        }
    }
    fn current(&self, token: PageToken) -> bool {
        token.source == self.source
            && token.generation == self.generation
            && self
                .pending
                .get(&token.offset)
                .is_some_and(|p| p.token == token && !p.cancel.is_cancelled())
    }
    fn finish_pending(&mut self, token: PageToken) {
        if let Some(p) = self.pending.remove(&token.offset) {
            p.cancel.0.store(true, Ordering::Release);
        }
    }
    fn align(&self, index: u64) -> u64 {
        index / self.settings.page_size as u64 * self.settings.page_size as u64
    }
    fn total_is_before(&self, index: u64) -> bool {
        matches!(self.total,PageTotal::Known(total) if index>=total)
    }
    fn next_revision(&self) -> Result<u64, DataError> {
        self.revision
            .checked_add(1)
            .ok_or(DataError::RevisionOverflow)
    }
}
impl Drop for PagedDataSource {
    fn drop(&mut self) {
        for p in self.pending.values() {
            p.cancel.0.store(true, Ordering::Release);
        }
        self.mailbox.close();
    }
}
fn validate_query(
    schema: &TableSchema,
    query: &QueryRequest,
    capabilities: QueryCapabilities,
) -> Result<(), DataError> {
    if !query.group_by.is_empty() || !query.aggregates.is_empty() {
        return Err(DataError::Validation(
            "row pages do not contain groups or aggregates".into(),
        ));
    }
    let compiled = QueryEngine::compile(schema, query)?;
    if compiled.execution(capabilities) != QueryExecution::Pushdown {
        return Err(DataError::Validation(
            "provider does not support the complete typed query".into(),
        ));
    }
    Ok(())
}
fn validate_range(offset: u64, rows: usize, has_more: bool, total: u64) -> Result<(), DataError> {
    let end = offset
        .checked_add(rows as u64)
        .ok_or_else(|| capacity("response range overflow"))?;
    let valid = if offset >= total {
        rows == 0 && !has_more
    } else {
        rows != 0 && end <= total && has_more == (end < total)
    };
    if !valid {
        Err(DataError::Conflict(
            "page rows/end flag disagree with total".into(),
        ))
    } else {
        Ok(())
    }
}
fn payload_bytes(table: &DataTable) -> Result<usize, DataError> {
    let mut bytes = table
        .schema()
        .fields()
        .iter()
        .map(|f| f.name.len() * 2 + f.title.len())
        .sum::<usize>();
    for row in table.rows() {
        bytes = bytes
            .checked_add(std::mem::size_of::<DataRow>() + 4 * std::mem::size_of::<usize>())
            .ok_or_else(|| capacity("page size overflow"))?;
        for value in &row.values {
            let payload = match value {
                CellValue::Text(v) => v.len(),
                CellValue::Binary(v) => v.len(),
                _ => 0,
            };
            bytes = bytes
                .checked_add(std::mem::size_of::<CellValue>())
                .and_then(|v| v.checked_add(payload))
                .ok_or_else(|| capacity("page size overflow"))?;
        }
    }
    Ok(bytes)
}
fn validate_payload(table: &DataTable, limits: DataLimits) -> Result<usize, DataError> {
    if table.schema().fields().len() > limits.max_fields || table.rows().len() > limits.max_rows {
        return Err(capacity("page fields/rows exceed limits"));
    }
    for row in table.rows() {
        for value in &row.values {
            if match value {
                CellValue::Text(v) => v.len() > limits.max_text_bytes,
                CellValue::Binary(v) => v.len() > limits.max_binary_bytes,
                _ => false,
            } {
                return Err(capacity("page cell exceeds payload limit"));
            }
        }
    }
    let bytes = payload_bytes(table)?;
    if bytes > limits.max_bytes {
        return Err(capacity("page byte limit exceeded"));
    }
    Ok(bytes)
}
fn bounded_error(message: &str) -> String {
    let mut s = String::new();
    for c in message.chars() {
        if s.len() + c.len_utf8() > 2048 {
            break;
        }
        if !c.is_control() {
            s.push(c)
        }
    }
    if s.is_empty() {
        s.push_str("Page request failed")
    }
    s
}
fn capacity(message: &str) -> DataError {
    DataError::Capacity(message.into())
}
