# Virtual table accessibility

Status: accepted, 2026-11-04. Extends ADR-015.

## Decision

Extend the pinned AccessKit transport and Windows adapter with an explicit,
generation-checked table realization request. Controls expose logical coordinates
and retain ownership of viewport, selection and editor state. The application
translates a request into that control API; the platform provider never performs
data-source I/O or creates controls.

`Action::RealizeCell` carries zero-based row, column and generation. A table opts
in through `Node::virtualization_generation`. `Node::virtual_cell_spans` describes
merged anchors without materializing the worksheet. Existing action and property
identifiers retain their values. Local changes, original sources and licenses
are recorded in both `vendor/accesskit/BACKPORT.md` and the Windows adapter notice.

## Windows contract

`Grid.GetItem` is an explicit request for a logical cell. For an opted-in table
it may realize that cell and move the viewport, returning the full materialized
provider required for native table navigation. Counts, headers and ordinary
property queries do not trigger realization. Active drafts prevent viewport
movement and cause the request to fail without accepting or cancelling input.

ItemContainer enumeration returns lightweight placeholders supporting Simple,
GridItem and VirtualizedItem without scrolling. A placeholder has no Fragment
interface or invented runtime identity. Unavailable properties return
`UIA_E_ELEMENTNOTAVAILABLE`; after realization properties delegate to the actual
cell. Clients can reacquire that cell through GetItem for fragment navigation.

`VirtualizedItem.Realize` dispatches the typed request without holding the tree
lock, then waits at most two seconds for a published cell. Replaced or removed
owners and stale generations fail. Disabled controls or a protected editor draft
cannot be bypassed. Realization may scroll; it never selects, focuses, commits or
cancels a draft. A successful no-op for an already materialized cell does not
dispatch an action.

`ItemContainer.FindItemByProperty` supports property ID zero with an empty value:
enumeration in row-major order, once per merged anchor. Other property searches
return E_INVALIDARG. The starting item must belong to the same table. Coordinates,
dimensions and span arithmetic are checked; metadata is bounded to 65,536 merged
ranges. No complete cell tree is allocated.

The implementation follows the Microsoft contracts for
[virtualized items](https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-implementingvirtualizeditem)
and [item containers](https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-implementingitemcontainer).

## Identity and application behavior

GridControl includes source structure, expansion and column arrangement in its
generation. Requests from a previous source snapshot are rejected even before
the next accessibility projection. Cards do not advertise table realization.
PivotGrid reuses the same grid contract for aggregates and original records.

Spreadsheet includes workbook revision and active sheet in its generation.
Selection alone does not invalidate virtual coordinates. A merged coordinate
resolves to its anchor. Formula drafts and active composition prevent realization
that would displace the editor. The visible semantic snapshot remains bounded.

When realization moves the viewport away from the keyboard-focused cell, its
semantic node is retained without visible bounds until focus or the source
generation changes. Grid retains at most that cell and its parent row;
Spreadsheet retains one cell. This prevents an unrelated root-focus event from
interrupting native table navigation and does not retain previously visited rows.

## Validation boundary

Direct COM tests cover lifetime, stale requests, merged enumeration, invalid
metadata, disabled controls and reentrant action dispatch. Projection tests check
selection, draft and source revision preservation. Cross-process Windows tests
exercise ItemContainer and GetItem/Realize through the system UIA client for Grid,
Pivot and Spreadsheet. Screen-reader speech, input and DPI checks are separate;
COM unit tests do not establish them. Other operating systems retain their
existing adapters.
