# Local Windows table-pattern extension

Based on the published `accesskit_windows 0.35.1` crate, upstream revision
`ce8164ba92995cfa86005b6259115e08c8244253`, directory `adapters/windows`.
This is a local extension, not an upstream release or an upstream backport.

The original source, manifest, tests and attribution are retained. LICENSE-APACHE,
LICENSE-MIT, LICENSE.chromium and AUTHORS are copied from that exact revision.
The original crate archive checksum and file hashes are recorded in the project
dependency inventory. Local extension files are Apache-2.0.

Changes made 2026-11-03:

- `src/node.rs` adds four COM interfaces and delegates table patterns, properties
  and property-change notifications to `src/node/grid.rs`.
- `src/node/grid.rs` implements Grid, GridItem, Table and TableItem for explicit
  bounded row/column metadata, spans and headers. Nested/hidden tables are isolated;
  stale owners/nodes fail, and unmaterialized cells return ELEMENTNOTAVAILABLE.
- `src/node/grid/tests.rs` checks the COM contract without creating a window or
  dispatching input, focus, UIA events or screen-reader actions.

Changes made 2026-11-04:

- `src/node/grid/virtualization.rs` implements ItemContainer enumeration and
  generation-checked VirtualizedItem realization using the matching local
  AccessKit extension. An unrealized cell is a Simple-only placeholder, without
  a Fragment interface or synthetic runtime identity. Merged ranges enumerate
  once per anchor. Other properties delegate only after materialization.
- Explicit Grid.GetItem realizes a requested logical cell outside the viewport
  and returns its full provider. Counts, headers and ItemContainer enumeration
  stay passive. Draft/generation guards apply to both realization entry points.
- `src/node.rs` and `src/node/grid.rs` expose the new patterns. The direct COM
  tests add stale/disabled owner, merged enumeration and reentrant dispatch cases.

The same date also adds plain-text range visibility:

- `src/text/visibility.rs` intersects transformed text rectangles with ancestor
  clips and derives bounded visible character ranges. Empty documents retain a
  visible caret; fully clipped characters do not appear as visible text.
- `src/node.rs` supplies GetVisibleRanges through those shared calculations.
- `src/text.rs` clips GetBoundingRectangles and carries exact range endpoints
  in the local ScrollTextIntoView payload. It targets the text owner rather
  than losing the character index in a generic run-level scroll hint.
- Direct tests check transformed LTR/RTL clips, edge-only intersections, empty
  documents and COM endpoint transport. Query tests dispatch no native input.

The same date adds passive text searches:

- `src/text/search.rs` implements bounded `FindText` and `FindAttribute` within
  an existing valid range, without dispatching an action. Text uses Windows
  ordinal matching in either direction and retains complete graphemes.
- `src/text/attributes.rs` extracts the existing attribute conversion into a
  shared mapping for whole ranges and individual runs. Adjacent matching runs
  merge even when another attribute differs. `src/text/search/tests.rs` covers
  Unicode, partial ranges, types, limits and direct COM lifetime behavior.
- Search does not use the consumer's format-unit grouping, which currently
  does not distinguish italic flags. The consumer dependency remains unchanged.

All other upstream implementation files are unchanged. Root `[patch.crates-io]`
entries select this adapter and the matching AccessKit transport extension.
The registry and other platform adapters
are not modified. Remove the patch only after validating an upstream replacement.

Run the local read-only COM tests with:

```text
cargo test --manifest-path vendor/accesskit_windows/Cargo.toml --config 'patch.crates-io.accesskit.path="/absolute/project/vendor/accesskit"' --lib node::grid::tests --locked -- --test-threads=1
```

Other upstream test filters may create native windows. No live screen-reader or
cross-process UIA acceptance is claimed by these direct COM tests.
