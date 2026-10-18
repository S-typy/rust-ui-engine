# Diagram canvas and designer

`spatial::Diagram` combines one accepted `DiagramGraph`, selection, viewport,
tool and bounded undo/redo history. Viewer and designer share this model;
`set_read_only` prevents mutations while retaining navigation and selection.
`Controls::diagram` creates a focusable retained GPU canvas.

A node has a stable u64 ID, label, rectangle, shape and lock flag. Supported
shapes are rectangle, rounded rectangle and ellipse. Four named ports identify
north/east/south/west anchors independently of a node's position or size.
Connectors store endpoint node IDs and ports. Moving or resizing a node updates
its anchors without changing connector identity. Removing nodes also removes
their incident connectors. A graph can contain cycles and self-connections.

`DiagramEdit` stages inserts, connections, removal, movement, resize, rename and
automatic layout. The entire candidate is validated before publication. A
duplicate ID, missing endpoint, invalid rectangle or locked node in a batch
leaves the document and history unchanged. Undo/redo stores at most 64 accepted
graph snapshots. A continuous pointer drag creates one history entry; Escape,
capture loss, disabling the canvas or losing window focus restores the initial
document. Starting a new accepted edit discards redo history.

Orthogonal routing first attempts direct two-bend paths, then bounded corridors
around nearby obstacles and graph extents. Interior segments avoid node bounds
with a four-pixel clearance; the short endpoint exits connect named ports.
`DiagramRoute::blocked` explicitly reports when this bounded search did not find
a clear route. Such a connector is shown with a muted fallback and a
`[route blocked]` caption. This router does not promise a solution for every
obstacle configuration. Connector crossings are permitted.

Connector captions are placed beside a free route segment after coalescing
collinear port-exit segments. Caption bounds avoid every node and remain inside
the viewport. Dense or offscreen routes can omit a caption when no free space
is available; their complete labels remain in `table` data.

Automatic layout uses deterministic topological columns, actual node sizes and
spacing. Cyclic leftovers share a final column. Locked nodes retain their
coordinates; applications can inspect the resulting routes for obstructions.
Large/invalid resulting coordinates are rejected atomically. Selection and
business meaning of nodes belong to the application.

Primary click selects a node; Shift+click toggles membership in the selection.
Dragging selected nodes moves the set. The bottom-right handle resizes one
node. Middle/right drag and dragging empty canvas pan. Arrows move selected
nodes, Shift+arrows resize a single selection, Delete removes, Control+A selects
all and Control+Z/Y performs undo/redo. Control+arrows pans and Page Up/Down or
the wheel zooms around an anchor. Home/End selects the first/last node. The
`DiagramTool` palette also permits insertion on an empty-canvas click.

`to_json`/`from_json` use strict version-1 JSON. Unknown fields, unsupported
versions, duplicate identities, dangling references, invalid coordinates and
oversized documents fail before replacing accepted state. `restore_json` is an
undoable explicit replacement. Filesystem I/O and save locations are app-owned.
Viewport, selection, history and WidgetId values are not serialized.

Bounds: 1024 nodes, 4096 connectors, 512 UTF-8 bytes per label, coordinates
within ±1,000,000, node width 32…8192 and height 24…8192 logical pixels, zoom
0.1…8, and an 8 MiB JSON input limit. Text and strokes use existing GPU Scene
commands. Ellipse interiors use a bounded geometric strip approximation and
their contours use 64 line segments.

`table` exposes node labels, positions and connection descriptions. The example
offers a synchronized keyboard-accessible ListBox beside the canvas; the canvas
accessible value reports the selection and available operations. This does not
add a native graph/connection accessibility pattern. The example is
`cargo run -p xaml-form -- --diagram`; Save/Restore buttons demonstrate explicit
JSON round trips in application memory and do not write a file.
The list shows compact coordinates rounded to two decimal places while each
selectable row retains its complete, unrounded accessible name and connections.
