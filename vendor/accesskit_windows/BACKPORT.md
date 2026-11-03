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

All other upstream implementation files are unchanged. The root `[patch.crates-io]`
entry affects only this Windows adapter. The registry and other platform adapters
are not modified. Remove the patch only after validating an upstream replacement.

Run the local read-only COM tests with:

```text
cargo test --manifest-path vendor/accesskit_windows/Cargo.toml --lib node::grid::tests --locked -- --test-threads=1
```

Other upstream test filters may create native windows. No live screen-reader or
cross-process UIA acceptance is claimed by these direct COM tests.
