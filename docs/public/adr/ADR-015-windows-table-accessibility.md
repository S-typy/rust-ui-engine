# Windows table accessibility

Status: accepted, 2026-11-03. Virtualization is extended by
[ADR-016](ADR-016-virtual-table-accessibility.md).

## Decision

Keep AccessKit as the platform-neutral accessibility transport. Add Grid,
GridItem, Table and TableItem COM patterns to a reviewed local copy of the pinned
Windows adapter. The patch is selected through Cargo; no registry files or
system accessibility settings are modified. Attribution and exact source
provenance remain in `vendor/accesskit_windows/BACKPORT.md` and the dependency
inventory. Controls remain independent of AccessKit and the native host.

GridControl and PivotGrid already project logical row/column coordinates and
stable materialized cell identities. Spreadsheet exposes a bounded semantic
snapshot through `Spreadsheet::semantics`; the example translates it into the
same AccessKit roles, indices, spans and headers.

## Contract

Grid/Table require explicit counts representable as signed 32-bit integers.
Coordinates are zero-based. `GetItem` returns the materialized cell covering the
requested coordinate, including an empty value or an anchor spanning merged
cells. Out-of-range requests return E_INVALIDARG; valid but unmaterialized cells
return UIA_E_ELEMENTNOTAVAILABLE. Overlapping duplicate cells are rejected.
Hidden subtrees and nested tables do not contribute cells to an enclosing grid.

GridItem reports row/column/span and its containing grid. Table/TableItem expose
materialized headers with corresponding coordinates. Properties are read from
the current snapshot; retained providers track coordinate changes and reject
removed nodes or disposed owners. Count/span/coordinate changes enqueue ordinary
UIA property notifications through the existing adapter event queue.

The API follows the [Grid provider contract](https://learn.microsoft.com/en-us/windows/win32/api/uiautomationcore/nn-uiautomationcore-igridprovider)
and [GridItem provider contract](https://learn.microsoft.com/en-us/windows/win32/api/uiautomationcore/nn-uiautomationcore-igriditemprovider).

## Limits and validation

The initial contract covers materialized cells; ADR-016 adds opt-in
VirtualizedItem/ItemContainer realization. A plain query does not
scroll, mutate selection, commit a draft or allocate an entire virtual table.
Direct COM tests exercise patterns, properties, headers, spans, updates and
lifetime with a null window handle and no native input. Projection tests cover
the bounded worksheet and protection of formula drafts. These checks do not
establish live Narrator, cross-process navigation or mixed-DPI acceptance.
