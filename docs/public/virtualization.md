# Viewport virtualization

The core geometry and identity APIs support bounded viewport materialization
without allocating a record for every logical row. Data fetching, source
ordering, selection, and editor values remain the consumer's responsibility.
The ListBox runtime uses these APIs; ComboBox and lookup popups reuse that
ListBox runtime. TreeGrid retains its separate sparse hierarchy index.

## Geometry and anchors

`SparseAxis` combines a logical count, a default item extent, and sparse measured
overrides. `offset_of` returns item boundaries, `extent_at` returns the current
measurement or estimate, and `index_at` uses half-open ranges. The total extent
is outside the last item. `replace` and `set_extent` validate a candidate before
publishing it; invalid values and capacity failures leave the old axis intact.

`range(scroll, viewport, overscan, max_realized)` returns separate visible and
materialized ranges, the clamped scroll offset, and total extent. Overscan counts
items before and after the visible range. The materialization budget trims
overscan first. If the visible items alone exceed that budget, the method returns
an error instead of omitting visible rows. An empty viewport has no overscan.

`StableScrollAnchor<K>` captures a stable key, its old logical index, and the
offset inside that item. After changing order or measurements, the consumer
resolves the key in its new source and restores the offset. When the key has
disappeared, restoration uses its old index, clamped to the final item; an empty
source restores zero. When the row shrinks, the restored offset stays inside
that row. A consumer then clamps scrolling to its actual viewport.

Geometry uses `f64`. Bounds are explicit:

| Resource | Limit |
| --- | --- |
| Logical items | 1,000,000,000 |
| Each estimated or measured extent | 1 through 1,000,000 logical pixels |
| Aggregate extent | 1,000,000,000,000 logical pixels |
| Sparse measured overrides | 65,536 |
| Overscan on each side | 1,024 items |
| Materialized items | 4,096 |

Repeated override indices are errors. Overrides equal to the default are
discarded; one replacement still accepts at most 65,536 supplied entries.
Offset lookup is logarithmic in the number of measurements. Offset-to-index
lookup uses binary search over the logical count and sparse prefix sums.

## Identity and recycling

`RecyclingSlots<K>` assigns opaque `VirtualLease` values to the current keys.
Reconciliation preserves a lease for a key that remains materialized and
reports released and newly assigned keys. A released slot may be reused, but
its next assignment receives a different generation. `resolve` rejects old
leases and leases from another pool. Duplicate keys, capacity errors, and
generation exhaustion fail before changing assignments.

The pool contains identities, not controls or editor state. Its allocated slots
are bounded by its configured capacity and retain the maximum allocation needed
so far. Consumers must dispose of, or completely reset, a visual released from
one key before presenting another key.

## ListBox integration

`ListVirtualizationSettings` controls `default_row_height`, `overscan`, and
`max_realized_rows`. Defaults are 32 pixels, two items on each side, and the core
limit of 4,096. `Controls::set_list_virtualization` validates the current
viewport and preserves its key anchor before changing settings.

`set_list_item_height(list, key, Some(height))` records an explicit measurement;
`None` returns that key to the default height. Measurements follow keys through
source reorder and disappear when the key is removed. A size change above the
viewport preserves its anchored key and offset. Page navigation and selection
visibility use the same variable geometry.

ListBox currently accepts at most 100,000 source items and 16 MiB of combined
key/caption/column text, with at most 32 text columns per item. The complete
source remains in memory; UI virtualization does not perform data paging.
List heights must be 1 through 4,096 pixels in quarter-pixel steps. Aggregate
item extent must not exceed 4,194,296 pixels. This stricter ListBox bound,
including its eight pixels of content padding, preserves quarter-pixel
positions when converting core geometry to the renderer's `f32` coordinates.
An unsupported total extent is an error, not a silently rounded position.

Materialization retains a row's `WidgetId` and subtree while its key remains
in the materialized range. Caption, enabled state, selection, semantic name,
and position update in place. When a key leaves that range, its entire subtree
is removed. A new key receives a fresh widget identity even when the identity
pool reuses a slot. Documents, undo history, composition, and handlers therefore
cannot migrate to another key. Focus within a removed subtree returns to the
ListBox. Offscreen row editor state is not persisted by virtualization; an
application that needs persistence stores it by key outside the visual tree.

`list_row_widget`, `list_row_lease`, and `list_resolve_lease` expose current
materialization identities. `list_virtualization_stats` reports visible and
materialized ranges, current row count, allocated identity slots, measured
overrides, and cumulative creations/releases. Before the first arrange, the
runtime uses a provisional 320-pixel viewport. A later smaller viewport reduces
live rows while retaining the pool's bounded allocation peak.

The runtime clones only the materialized item slice during scrolling. Replacing
the complete source rebuilds the key-to-index map; this work is proportional to
the source size. Height and virtualization settings reuse the immutable index
and rebuild only bounded sparse measurements. Creating replacement rows can
temporarily coexist with the
previous viewport's rows to preserve failure handling. After reconciliation,
only the current materialized range remains.

Font shaping is validated for materialized content. Offscreen captions are not
shaped eagerly; an invalid text run in such a row produces a normal control
error when that row enters the materialized range.

## Verification

Core tests compare sparse geometry against a dense reference, test a million
logical rows with two overrides and billion-row offsets near the extent limit,
and check bounded recycling across 10,000 changes. ListBox regressions mount
100,000 rows and verify bounded live controls, actual variable row rectangles,
keyboard paging, anchors across reorder/removal/measurement changes, and atomic
invalid settings. They also verify retained editor draft/selection/undo/IME and
subtree destruction with stale lease and accessibility requests.

Existing TreeGrid tests independently verify a million logical rows with a
bounded viewport and a million-child branch represented by three index
segments. These are runtime and model tests; they do not establish native
screen-reader or hardware rendering results.
