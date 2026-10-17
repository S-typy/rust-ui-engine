# Typed pivot snapshots

`PivotSnapshot` builds an immutable cross-tabulation from one accepted
`DataTable`. `PivotDefinition` supplies row and column field names, named measures
using the existing `Aggregate` type, and an optional typed `Criteria` filter.
It performs no provider I/O and does not edit the source. A failed build leaves
every earlier snapshot intact; cloning a successful snapshot shares its storage.

Each axis supports zero to two fields, unique across both axes. Measures support
the existing CountRows, CountValues, Sum, Average, Minimum and Maximum operations.
QueryEngine owns type checking, NULL handling, deterministic ordering, arithmetic
errors and aggregate results. Every prefix subtotal and grand total aggregates
the original matching rows. Average is therefore weighted by the actual non-NULL
members and never computed as an average of child averages. Overflow rejects the
whole candidate.

## Paths, totals and provenance

`row_paths()` and `column_paths()` return preorder lists: a depth-zero grand-total
path followed by lexicographically ordered typed prefixes, with NULL first.
`PivotPath` exposes its field names, values, depth, parent and opaque `PivotPathId`.
The ID includes axis, field names and canonical typed values. It remains stable
when source records reorder or a preceding sibling is inserted. Moving a field
to the other axis or changing a prefix changes that path's identity. NULL, empty
text, numeric values and binary values do not alias; real negative zero and
positive zero follow the query engine's equality semantics. The ID's `Ord` is
opaque byte order, not the display order.

`value(row_id, column_id, measure_index)` reads an aggregate. Only nonempty
intersections and the grand total occupy the sparse map. A missing intersection
uses the query engine's empty-input aggregate: CountRows/CountValues are zero;
Sum/Average/Minimum/Maximum are NULL. Unknown paths, wrong-axis IDs and unknown
measure indices return errors. No dense unbounded cross-product is constructed.

Applications keep one `PivotSourceId` per source identity and pair it with the
accepted revision in `PivotSourceVersion`. They must create a new identity when
replacing a source and pass the current revision when reading drill-down data.
`drill_down` rejects a foreign or stale version before reading path payloads.
Its `PivotDrillDown` contains the version and original signed i64 row keys in
ascending order, including the full i64 minimum/maximum range. These keys are
never synthetic projection keys. The application must check the returned version
again before using a delayed result against a live source; the pure model cannot
observe application mutations by itself.

An empty filtered source retains the two grand-total paths and defined empty
aggregates. Expansion, selection, column visibility and viewport geometry belong
to the retained view; the model does not impose a widget representation.

## Bounds and checks

- At most two fields per axis and eight measures; measure titles contain
  1–256 UTF-8 bytes without control characters.
- At most 10,000 paths per axis including the grand total, 100,000 stored
  intersections, and 4,096 bytes per encoded typed path.
- The existing bounded DataTable supports at most 100,000 input records.
  Building uses at most nine aggregate requests and two axis-order requests.
- A 20-million-unit preflight estimate bounds input rows multiplied by expression
  nodes and query passes. It is a deterministic admission bound, not a CPU-time
  quota. QueryEngine additionally enforces its own working/result bounds.
- The snapshot's accounted definition, typed paths, aggregates and original-key
  arrays must fit the source's `DataLimits.max_bytes`. This is a logical retained
  payload bound; allocator overhead and simultaneous temporary query candidates
  are not a process-memory guarantee. A clone does not duplicate that payload.
- Definition trees are inspected by reference before recursive cloning: at most
  512 nodes, depth 32, 128 operands per boolean group, and 1 MiB of literal bytes.

Tests verify hand-calculated totals and every intersection's original members,
unequal-group averages, exact Decimal sums/means, NULL and empty input, stable
typed paths, binary/date/time keys, real zero, source revisions, malformed input,
overflow refusal, explicit memory/work/path bounds and a 100,000-record source.
The model makes no claims about a remote cube, OLAP server, custom formulas,
distinct count, source writeback or a complete rendered pivot control.
