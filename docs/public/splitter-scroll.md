# SplitContainer and scrolling

`Controls::split_container(parent, settings, style)` creates two retained content
hosts and a focusable divider. `SplitContainer` returns `container`, `first`,
`second` and `divider` IDs. Put application controls under either content host.
Resizing, collapsing and restoring panes keep those controls and their documents
alive. Remove the container or an ancestor; removing a required host directly is
rejected. Both content hosts clip overflowing descendants.

`SplitterSettings::axis` is `Horizontal` for side-by-side panes and `Vertical`
for stacked panes. `ratio` is the requested share of pane space, excluding the
divider, in `0..=1`. `first_min`, `second_min`, `first_max`, `second_max` are
nonnegative logical sizes; maxima permit positive infinity. The divider width
is finite in `1..=64`, with a default of 6 logical pixels.

Constraints affect arranged sizes without overwriting the requested ratio, so a
larger viewport can restore the requested proportion. If both minima cannot fit,
the available space is divided proportionally to those minima. If both maxima
leave unused space, it remains after the second pane. Collapsing a pane gives all
pane space to the other one, temporarily ignoring its maximum. The divider stays
available for restoring the pane. Settings validate before any mutation.

Use `splitter_settings`, `split_ratio`, `set_split_ratio`, `set_split_collapsed`
and `configure_splitter` to inspect or change the model. A divider or container
ID addresses the same splitter. Nested splitters settle parent geometry before
arranging their descendants. An idle update does not repeat layout.

The primary pointer button drags the divider with capture; release accepts the
new ratio. Escape, native capture cancellation, focus loss and disabling the
splitter restore the ratio from the beginning of the gesture. Keyboard focus
stays on the divider. Arrow keys along the split axis change the ratio by 0.02;
Ctrl+Arrow changes it by 0.1. Home/End request the two extremes. Enter collapses
or restores the first pane; Shift+Enter selects the second pane. Application
settings changes end an active gesture and apply the new settings.

The divider exposes a Splitter accessibility role, its requested ratio and
Focus/SetValue/Expand/Collapse actions. Its orientation describes the physical
divider, perpendicular to the resize axis. Actual native screen reader behavior
requires platform testing; semantic unit tests alone do not establish it.

## Two-axis ScrollViewer

`scroll_view` returns a viewport and retained content host. `ScrollSettings`
selects each axis independently: `Auto`, `Visible`, `Hidden` or `Disabled`.
Hidden suppresses the bar while preserving scrolling; Disabled resets its offset
and sizes content to the viewport on that axis. Bars overlay the content and do
not reserve layout space. `scroll_to_point` clamps both offsets to the measured
extent. Resize and content extent changes clamp again during the next update.

Wheel input passes to an enclosing viewer when the inner viewer cannot move.
Scroll arrows, PageUp/PageDown, Home/End and thumb dragging use the same offsets.
Scrolling affects visual translation and clipping without rebuilding children.

`bring_into_view(widget)` reveals an arranged retained descendant through all
enclosing viewers, from the innermost outwards. Each inner viewport limits the
area requested from its parent. The method performs the smallest necessary
scroll, preserves keyboard focus and returns whether any offset changed. Hidden,
collapsed, detached or unarranged targets are rejected without scrolling.

`scroll_rect_into_view(viewport, rect)` accepts a rectangle in content-local
coordinates, including an item not currently materialized by a virtual list.
The list owns item realization and the measured extent; this method owns only
the viewport offset. Oversized rectangles align their leading edge, disabled
axes remain unchanged, and all offsets clamp to the current extent. Invalid
rectangles are rejected atomically. Call the normal `Controls::update` after
scrolling before reading the next visual bounds or rendering.
