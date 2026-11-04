# Local table virtualization extension

This is AccessKit 0.25.1 from the published crate and upstream revision
`ce8164ba92995cfa86005b6259115e08c8244253`, path `accesskit`.
The original authors and Apache-2.0 / MIT license texts are preserved; root
license files come from the same revision as the Windows adapter.

The local extension adds `Action::RealizeCell`, `ActionData::RealizeCell`
and `Node::virtualization_generation`, plus bounded merged-cell metadata through
`VirtualCellSpan` and `Node::virtual_cell_spans`. It is not an upstream release or
backport. Existing action/property discriminants are preserved. Consumers
that do not advertise this action retain their previous behavior.

Coordinates are zero-based and generation-checked. Realization changes only
materialization/viewport; it cannot accept an editor draft, change selection,
focus a control, or launch provider I/O. The Windows adapter bounds requests
and rejects removed, disabled or replaced owners.

`src/virtual_cell_tests.rs` verifies cloning and the serde/schema contracts.
All other upstream implementation files are unchanged. The root Cargo patch
uses this source alongside the matching local Windows adapter; native adapters
for other operating systems retain their existing behavior.

New local changes are Copyright 2026 rust-ui-engine contributors, Apache-2.0.
