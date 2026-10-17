# Asynchronous row pages

`PagedDataSource` coordinates typed row requests, cancellation, retry, and a bounded
cache. The application owns the provider, transport, and worker execution. Reading
a cached row never calls an adapter or waits for a worker. This source is separate
from the synchronous, complete-snapshot `DataSource`; an unknown total is not
represented as a fabricated full table.

```rust
use rust_desktop_ui_data::*;

let schema = TableSchema::new(vec![FieldSchema::new("caption", FieldType::Text)])?;
let mut pages = PagedDataSource::new(
    schema,
    DataLimits::default(),
    PageCacheSettings::default(),
    QueryCapabilities::complete(),
)?;
let requests = pages.request_window(0, 64)?;
// Send each owned PageRequest to an application worker. Its response is a
// complete typed page, returned through complete or completion_sender.
# Ok::<(), DataError>(())
```

## Queries and request identity

A request contains an opaque `PageToken`, aligned offset, page size, shared typed
`QueryRequest`, and a cooperative `PageCancellation`. Tokens are specific to one
source, query generation, and request sequence. `set_query` validates the entire
query and provider capabilities before cancelling any existing work. Reapplying
the same query does nothing; `refresh` explicitly starts a new generation for the
same query. Both clear pages, errors, total, and the accepted provider revision.

Row pages support the typed filter and ordering profile. Grouping and aggregates
are rejected. The provider must promise the required semantics version and every
operator used by the complete request. Even an empty explicit ordering requires
stable ascending signed-key ordering, as specified by `QueryEngine`. There is no
partial-query or incomplete-cache local fallback. Claim only the capabilities the
provider implements.

`complete(token, result)` first checks request identity. Late, foreign, cancelled,
or duplicate responses return `IgnoredStale` without inspecting or publishing
their payload. `cancel`, `cancel_all`, query replacement, and source destruction
set the cooperative flag. Providers should check the flag around expensive work;
the source still rejects a stale completion if cancellation arrives too late.

A provider error finishes the request and publishes a bounded, printable error
message (at most 2048 UTF-8 bytes). `page_status` reports it. `request_window` does
not repeatedly retry a failed page: `retry(offset)` explicitly issues a new token.
Other accepted pages remain available. A malformed successful response returns an
error and leaves its request pending so the provider may send a corrected response
or an explicit failure. Publication failures do not change accepted rows, total,
provider revision, selection, or cache recency.

## Pages, total, and coherent snapshots

`PageResponse` contains a `DataTable`, `PageTotal::Unknown | Known(u64)`, `has_more`,
and a provider `snapshot_revision`. The schema must exactly match the source,
including field types/nullability. Row count, cell payloads, aggregate page bytes,
and stable-key uniqueness are validated before publication. Exact integers,
decimals, dates, binary values, and `Null` remain typed.

The first accepted page fixes the provider revision for the generation. Every
subsequent accepted page must use that revision. Providers must not reuse a
revision for changed data or ordering. A mismatch requires an explicit refresh;
it never silently mixes pages from different snapshots. The cache can detect
contradictory keys in resident pages, but cannot verify a remote provider's global
key uniqueness after pages have been evicted.

A nonfinal page must contain the full requested page size. A final nonempty page,
or an empty first page, establishes an exact total even if the response originally
reported `Unknown`. An empty noninitial page must provide a known total; an empty
page beyond that total is valid. New totals and end flags must agree with every
resident page and the previously accepted total. Unknown totals remain unknown
until such evidence arrives. Offset arithmetic is checked as `u64`; keys are signed
`i64` identities and never derived from offsets.

`row_at(index)` returns only a resident row. `page_status` distinguishes missing,
pending, ready, and failed pages. `cached_row(key)` locates a resident identity.
`selected_key` is separate from residency and survives page eviction and query
replacement. An application can retain an off-page selection or clear it explicitly
with `set_selected_key(None)`; this API does not guess whether an unloaded key was
deleted in the remote source.

## Bounds and eviction

`PageCacheSettings` defaults to 256 rows per page, eight resident pages, 8192 rows,
32 MiB of counted cache payload, eight pending requests, and eight mailbox results.
Settings can be changed when constructing another source; they are immutable for
one source's lifetime.

Hard limits are 4096 rows per page, 256 cached pages, 65536 cached rows, 64 MiB of
counted cache payload, 64 pending requests, and 64 mailbox results. `DataLimits`
can impose smaller field/cell/page limits. Payload accounting includes schema
strings, rows, cells, indices, and text/binary payload, consistent with `DataTable`;
it counts lengths, not allocator overhead or spare capacities retained by incoming
`Vec`, `String`, and map allocations. Providers should supply compact tables; the
byte limit is a logical payload budget, not a resident-heap quota. Error records are separately bounded by
`max_pages` and 2048 bytes per error. A candidate response and mailbox may each use
additional bounded memory; the cache limit is not a total process-memory limit.

`request_window` aligns a bounded range to pages, touches resident pages, and
returns only missing requests. A window cannot span more than `max_pages` or
request more than `max_rows`; insufficient pending capacity rejects the whole
request operation. Page publication evicts least-recently-used resident pages
until page, row, and byte budgets all fit. Ordinary getters do not change recency.
A single page that cannot fit is rejected before any eviction.

## Worker delivery

The optional cloneable `PageCompletionSender` is `Send + Sync`. A worker calls
`try_complete(&request, result)`. Its mailbox has both a result-count limit and the
same byte budget as the cache. `Full`, `Busy`, `Closed`, `Cancelled`,
`ForeignRequest`, and `PayloadLimit` return a `PageSendFailure` containing the owned
payload; the provider decides whether to retry delivery or discard cancelled work.
No sender waits for the UI. Errors are normalized before enqueueing. A successful
send means queued, not necessarily accepted by the source.

The owning event loop calls `drain_completions(1..=64)`. It uses a nonblocking lock
attempt and validates each dequeued completion independently. A malformed page
cannot roll back other successful pages in the same drain. The optional
`set_completion_waker` callback runs after a successful enqueue and after releasing
the queue lock. Full or failed sends do not invoke it. The callback should only
schedule owner work; it must be quick and must not panic. Source destruction
cancels requests and closes delivery. An already running wake callback may finish
concurrently with destruction; it is a wake hint, not ownership of the source.

## Integration and checks

Current `GridControl` takes a complete `DataSourceHandle`. Connecting a paged source
to a window, loading indicator, selection view, or a provider-specific grid bridge
is application-owned; this API does not add a second grid or treat the resident
window as a complete sortable dataset. Network protocols, authentication, worker
pools, and remote writeback are outside this row-page contract.

`cargo run -p rust-desktop-ui-data --example paged_loading` runs a finite worker
example with a stale request, wake delivery, unknown total, eviction, retained
selection, and explicit retry. Regression tests cover out-of-order and foreign
responses, query/capacity atomicity, coherent totals/revisions, typed values,
byte/row/page limits, mailbox reentrancy and cancellation, and request overflow.
