# Cartesian chart data and geometry

`ChartModel` contains a bounded accepted snapshot of numeric series, settings and
an optional selected point. Supported views are `Line`, `Scatter`, `Bar`, and
`Sparkline`. Sparkline uses the same line geometry with a compact presentation.
Pie, hierarchical, heatmap, flow, stacked, logarithmic, time and categorical axes
are not implemented by this profile. It does not provide a general chart suite.

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
no point. There is no interpolation, decimation or hidden data aggregation.

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
