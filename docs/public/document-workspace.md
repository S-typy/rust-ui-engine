# Document workspace

`document_workspace::DocumentWorkspace` coordinates document identity, dirty
revisions, activation and close decisions over the existing `DockLayout`. The
application owns each document's controls, data and storage operations. Docking
owns placement and native window transfer; the document service does not recreate
or serialize document content.

## Identity and opening

`DocumentId` is the existing stable `DockPaneId`. Construct the service from a
validated layout. `register` adds a new clean, closed identity through
`DockLayout::register_closed_pane`; duplicate identities, invalid metadata and the
64-pane catalog limit reject the whole operation. The application constructs the
content first and publishes its arena only after registration succeeds. `open`
restores a closed identity at a supplied `DockPlacement`, or activates it when
already open. `activate` updates the docking selection and service active key.

`mark_changed` advances a checked contents revision and invalidates pending close
consent. `mark_saved` acknowledges only the exact revision actually written. There
is no implicit file write, autosave, dirty-state inference or content disposal.
Hosts should call `mark_changed` for all application changes, including editor
drafts when they form part of the document. A refused transition does not reset
an editor session.

## Coordinated close

`begin_close` accepts one document, a docking group, a native window or all open
documents. Nonclosable members reject the batch before prompting. Clean members
need no prompt. A dirty member yields an immutable `DocumentRequest` of kind
`Decide`. The application presents its own prompt and calls `decide` with `Save`,
`Discard` or `Cancel`. Save yields another request of kind `Save`; after real I/O,
the application calls `complete_save` with success or a bounded error string.

Requests carry private workspace, batch and sequence identities plus the exact
contents revision. Wrong-stage, foreign, duplicate, stale and changed-layout
responses are rejected without changing the current batch or placement.
`cancel_close` explicitly abandons a pending batch, including after an external
layout change. Revisions and request counters fail before integer overflow.

No document closes until every member has consented and every requested save has
succeeded. The final close operations run against a candidate `DockLayout` and
publish together. Cancellation or save failure leaves the entire batch open.
Successful earlier saves remain real saves if a later prompt is cancelled: the
service does not claim to undo external I/O. `Discard` permits closing; it does
not destroy the application's retained contents. `Closed` returns both the closed
identities and those explicitly discarded, allowing the host to decide whether
to retain, dispose or reload their data.

## Presentations and MDI

`DocumentPresentation` selects `Tabbed`, `Mdi` or `Windowed` at the application
integration boundary. A host stages candidate placement, validates current IME
and pointer capture, and only then publishes the presentation. The service's
`set_presentation` records that successful transition; it does not create windows.
Windowed mode uses the existing owned native windows and their 16-window limit.

`DockMdiLayout` provides client-area child frames for open pane identities. It
retains each frame's logical rectangle across close/reopen and viewport changes,
maintains front-to-back order, supports maximize/restore, cascade and tile, and
checks revision overflow. Rectangles remain within a bounded 32768-unit logical
coordinate domain. Tiling rejects a viewport that cannot satisfy every document's
minimum content size; ordinary viewport shrink clips displayed rectangles without
destroying their remembered sizes. A replacement pane catalog with the same keys
but incompatible minimum sizes is rejected before layout or gesture mutation;
update valid stored bounds explicitly before accepting larger minima.

`DockMdiView` supplies retained title, close, maximize and float controls. Dragging
a title or the lower-right resize handle previews the new rectangle; pointer-up
publishes it. Escape, capture loss, native blur or stale layout revision cancels
the gesture. Stable title widget IDs survive reordering. Required chrome parent
relationships are checked before updates. A host composes document scenes using
`DockMdiGeometry::visible_content`, which subtracts front frames, and routes input
using frontmost frame geometry. A foreground header therefore blocks the content
of a child behind it. Actual content arenas remain application-owned.

## Example and validation

Run `xaml-form --documents`. The compiled `DocumentWindow.xaml` uses the existing
Ribbon and DockWorkspace syntax. The example keeps three `Form` instances across
all three presentations, exposes explicit save/close/reopen commands, and uses
native Yes/No/Cancel dialogs. Save is deliberately an application-owned in-memory
store with delayed completion and a command to simulate failure; it is not file
persistence. Editors in this example keep drafts on blur.

Model and host regressions cover foreign/stale close requests, revision and
identity exhaustion, all-or-none batch close, cancellation after earlier saves,
save failure, dynamic registration, native-window close routing, retained widget
identity/draft/selection, IME refusal, MDI clipping/resize/order and drag cancellation.
Native Windows input, rendering and screen-reader evidence is recorded separately;
model tests alone do not establish those results. Linux/macOS runtime, physical
mixed-DPI monitor transfer and persistence of MDI rectangles are not claimed here.

## Long-running native-window limitation

On Windows with Vulkan, repeated `Tabbed` / `Mdi` / `Windowed` transitions
can leave an increasing process handle count after the floating HWNDs have been
destroyed. The checked runs preserved document identities, drafts and accepted
values, and the remaining count increase persisted through a bounded idle period.
Its allocator and cause have not been identified. Sharing the GPU device does not
establish resource stability for a long-running windowed document session; that
profile remains a known limitation. These observations do not establish that
other resource classes or backends are free of leaks.
