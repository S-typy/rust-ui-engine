# Chart data and geometry

`ChartModel` contains a bounded accepted snapshot of numeric series, settings and
an optional selected item. Cartesian views are `Line`, `Scatter`, `Bar`, and
`Sparkline`, `StackedBar`, `Area` and `StackedArea`. Typed `ChartFamily` observations add `Pie`, `Heatmap`, `Treemap`,
`Sunburst` and `Sankey`. Sparkline uses the same line geometry with a compact
presentation. Logarithmic and time axes remain outside this profile.

`ChartSeries` has a stable `u64` ID, label and points. Each point has a full signed
`i64` ID unique within its series, a finite `x` and optional finite `y`. Point order
is the line traversal order; it is not automatically sorted. NULL breaks a line
and produces no point marker or bar. Repeated x coordinates are allowed; bars
with the same x within a series overlap in input order. A point is identified by
`ChartPointKey { series, point }`, never by its vector position.

`ChartModel::replace` validates the complete candidate before publication. The
selected key survives a reorder or value update and becomes empty if the key is
removed or its y becomes NULL. Failed replacement and unknown/null selection
leave the old model and selection intact. Data replacement does not perform
provider I/O or edit any source row.

## Ranges and geometry

Both axes use finite linear ranges with positive finite spans. Automatic empty
ranges are 0..1. Constant ranges expand symmetrically by 5% of the larger of 1 and
the absolute value. Automatic bar y includes zero; automatic bar x pads by half
the minimum distinct x interval, rounded outward, so endpoint bars fit fully.
An unrepresentable finite expansion is rejected. Explicit ranges clip the
geometry; they do not rewrite point values. A bar's zero baseline is clipped to
the explicit y range when zero lies outside it.

Range reduction and line clipping use f64, with conversion to f32 only after
normalization to the local plot. A candidate whose normalized coordinate has
absolute magnitude above 1e12 or is nonfinite is rejected as ill-conditioned.
This avoids silently collapsing extreme outliers on a tiny explicit scale.
The plot must have finite coordinates of absolute magnitude at most 1e6 and
width/height in 1..1e6 logical units. Tight layout is a presenter concern; invalid
geometry is never published as a successful model operation.

`geometry(plot)` produces `ChartMark`, `ChartLine` and `ChartBar` arrays plus
evenly spaced axis ticks. NULL discontinuities are preserved when clipping.
Grouped bars have distinct positions by series order; their point markers use
the actual bar center. The same returned geometry drives drawing and hit tests.
`hit_test` first chooses the last drawn containing bar, then the nearest marker
within a finite nonnegative radius. Equal marker distances use series/input
order. Plot endpoints are inclusive for hit testing. Out-of-plot input returns
no point. Stored observations are never interpolated, decimated or aggregated;
line and area geometry connects adjacent values with linear segments.

### Accumulation and filled areas

`StackedBar` aligns equal numeric X values across series; their input order sets
stack order. Positive and negative observations accumulate independently from
zero. NULL or absent categories add no span. Duplicate X in one series is an
error, including +0/-0. The automatic Y range includes cumulative endpoints.
Finite overflow and a nonzero addition rounded entirely away are rejected before
publication. The original Y remains unchanged; `describe` adds the cumulative
interval. Explicit ranges clip each interval, including spans entirely above or
below zero. Bars share category width, and the containing visible span is selected.

`Area` requires strictly increasing X in each series, including NULL positions.
It fills each adjacent non-NULL pair to zero and keeps the line and point markers.
Zero crossings split the fill into two simple polygons. Explicit ranges clip the
polygons before conversion to f32; visible fill can remain when both observations
are offscreen. Independent series overlap in series order. Fills are painted
first, then lines and markers. NULL breaks both the line and the area.

`StackedArea` uses identical, strictly increasing X coordinates in every series,
including NULL positions. Positive and negative values accumulate separately in
series order. Its automatic Y range includes known cumulative endpoints. Missing
values break their band and every band above it; lower bands remain available.
An upper observation whose baseline is unknown keeps its raw value and key,
but has no cumulative marker or interval. Its description explicitly reports
that accumulation is unavailable. Unknown baselines are never replaced by zero.

Between observations, signed sums follow the interpolated source values.
Every zero crossing in a lower series splits the upper geometry, so a changing
baseline has the correct bend. A band changing sign switches between the
negative and positive envelopes without connecting the empty space between them.
Subdivision vertices create no new point identities. The same contour hit test,
original-endpoint selection, keyboard navigation and view transforms apply.
There is a budget of 8192 strips before viewport clipping, in addition to the
series/point limits. Exceeding it returns `Capacity` before model replacement;
it does not increase the renderer's per-frame path limit.

`ChartGeometry.areas` exposes the same clipped contours used for painting and
hit testing. Marker proximity takes precedence; a click inside a fill chooses
the nearer original endpoint by X (ties choose the first). That endpoint can lie
outside the viewport. A selected offscreen endpoint still has a semantic value,
but no marker is fabricated at the clip boundary. Keyboard navigation continues
to use visible observation markers. Fill contours are grouped into bounded paths
without changing their geometry or the shared renderer's path limits.

## View transforms

`set_view_ranges(x, y)` sets each explicit range or uses `None` to restore the
range derived from the observations. `zoom_at(factor, anchor)` scales both spans:
values above one zoom in, and the normalized anchor `(0, 0)` is the lower-left,
`(1, 1)` the upper-right data-space corner. `pan(x, y)` shifts each range by that
fraction of its current span; positive offsets move toward larger coordinates.

Transforms validate a complete candidate before publication. Factors must be
finite and positive, anchors finite and within `[0, 1]`, and offsets finite.
Collapsed, overflowing or ill-conditioned ranges are rejected by the common
range/geometry validation. Finite very wide ranges are allowed. An unchanged
candidate returns `false`; a unit zoom or zero pan does not pin automatic axes.
Source observations and selected keys remain unchanged, including selections
outside the new view. Non-Cartesian families return `InvalidView`.

## Snapshot adapter and limits

`ChartSeries::from_table` reads an existing `DataTable` using explicit x/y field
names. It accepts Integer and Real fields and preserves row keys unchanged.
NULL is allowed only for y. Integer values outside ±2^53 are refused rather than
rounded; Decimal, dates, time, text and binary fields need an application-owned
explicit conversion. The helper does not parse display text or issue a query.

- At most 16 series and 4096 points in total.
- Unique series IDs and point IDs within each series.
- Series labels and axis titles at most 256 UTF-8 bytes, without control characters.
- At most five distinct ticks per axis; duplicate floating-point ticks are removed.
- Invalid input produces a small typed `ChartError` without copying large payloads.

The drawing presenter must preflight final composed labels through the text
engine before replacing its retained control. Model validation does not claim
font availability or successful native rendering. Data/geometry unit results,
retained input checks and native visual acceptance are separate evidence.

## Partitions, cells, hierarchies and flows

Use `ChartModel::from_family(data, settings)` and `replace_family` for these
views. X/Y titles and explicit Cartesian ranges are rejected. `series()` is
empty and `point()` returns `None`; observations remain available through
`family()`. The legacy `x_range()` and `y_range()` unit intervals describe no
data axis for these views. Their geometry contains no numeric axis ticks.

| Family | Observations and limits |
| --- | --- |
| Pie | Up to 16 `ChartSlice` values with stable signed IDs, labels and finite nonnegative weights. Zero slices have no region. |
| Heatmap | Up to 64 named rows and 64 named columns, with up to 4096 uniquely positioned `HeatmapCell` observations. Negative values are valid; `None` produces an empty cell. |
| Treemap / Sunburst | Up to 128 `ChartHierarchyNode` values, eight levels, 16 roots and 16 children per branch. Leaves supply nonnegative weights; branches use `None` and sum their children. Missing parents and cycles are errors. |
| Sankey | Up to 64 `ChartFlowNode` and 128 positive `ChartFlowLink` values, arranged in an acyclic graph of at most eight ranks. Missing endpoints, self links and cycles are errors. |

All labels are single-line and at most 256 UTF-8 bytes. Sums must remain finite.
Every candidate is validated before replacement. Selection survives reordered or
changed data within the same family while its key remains selectable; switching
data families clears it. Treemap and Sunburst share hierarchy identities.
`ChartPointKey.series` is namespace 0 for these items and namespace 1 for Sankey
links. `describe(key)` returns a slice value and percentage, row/column/value,
hierarchy path and sum, or flow endpoints/weight and node incoming/outgoing totals.

Treemap uses recursive proportional rectangles, splitting along the longer
dimension. Branch headers and insets reserve visible space; leaf areas therefore
represent proportions within their parent's content area. Sunburst uses weighted
angular spans and equal radial depth bands. Flow ranks follow the longest path
from a source; stable input order controls vertical order. Node height is the
larger of incoming and outgoing weight, so an imbalance leaves unused capacity.
Cycles, automated crossing minimization and interactive node positioning are not
part of this flow layout. Unconnected zero-weight nodes have no region.

`ChartGeometry.regions` contains the exact rectangles or bounded polygons used by
painting and hit testing. Curves are sampled into at most 256 edges per polygon.
The last painted containing region wins; outside a region, marker proximity does
not select empty space. `points` contains keyboard anchors, not fabricated X/Y
observations. Subpixel regions may collapse at the final f32 conversion; zero and
missing partitions are omitted. The shared renderer's per-frame path budget also
applies when composing multiple diagrams.
