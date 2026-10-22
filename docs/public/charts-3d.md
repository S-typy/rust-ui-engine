# Projected three-dimensional charts

`spatial::Chart3D` projects accepted `ChartModel` series into a retained GPU
canvas. X/Y values, series labels, stable point keys and selection reuse the
chart model. Each non-null point additionally requires exactly one finite Z
value. Duplicate, missing and unknown Z keys fail before construction.

`replace_data(base, z)` validates a replacement atomically while preserving
camera, lighting and the selected stable key. A removed or null selected point
clears selection; invalid replacement data leaves the whole model unchanged.
`z_values()` exposes accepted keyed coordinates for application transformations.

```rust
use rust_desktop_ui_controls::{ChartModel, ChartPoint, ChartPointKey, ChartSeries, ChartSettings};
use rust_desktop_ui_controls::spatial::Chart3D;

let data = ChartModel::new(vec![ChartSeries {
    id: 1, label: "Measurements".into(),
    points: vec![ChartPoint { id: 7, x: 12.0, y: Some(4.0) }],
}], ChartSettings::default())?;
let chart = Chart3D::new(data, [(ChartPointKey { series: 1, point: 7 }, 3.0)])?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

`Camera3D` supports perspective and orthographic projection, orbit around the
data centre and bounded distance/field of view. Data coordinates are normalized
using accepted X/Y ranges and the Z extent; explicit X/Y ranges clip data before
projection. Invalid camera/light updates leave the previous value intact.
`set_light` supplies a finite nonzero directional light. The camera-facing
marker normal controls bounded diffuse intensity with ambient illumination.

The current geometry is a scatter cloud of opaque camera-facing discs. A disc
has constant camera depth. `Chart3DGeometry.points` is sorted far to near with
stable input order for equal depths. Painting uses that order; hit testing uses
the reverse order and the same disc radius. Consequently a hidden disc is not
selected through a nearer one. Annotation overlays do not create data hits.
This is a projected-data renderer, without triangle meshes, a hardware depth
buffer, volumetric surfaces, bars or intersecting sphere geometry.

`Controls::chart_3d` creates a focusable retained canvas. A primary click selects
a visible marker. Right/middle drag or Control+arrows orbits the camera;
Page Up/Down and the wheel change distance. Unmodified arrows, Home and End
navigate accepted points; Escape clears selection. Keyboard selection can reach
points hidden in the current view. Losing capture, window focus or eligibility
cancels an active camera gesture. Resize and theme changes preserve data and
selection.

`Chart3D::table` and `Controls::spatial_table` expose all data, including hidden
points, with Series/Point/X/Y/Z columns. The example presents these records in
a keyboard-accessible ListBox and synchronizes its selection with the canvas.
The visible rows use compact coordinates rounded to two decimal places; the
selectable row's accessible name retains every field and its original precision.
The canvas accessible name and value describe the selected point. The example's
status announces accepted selection changes, and its list supports keyboard
navigation with Windows Narrator running. This alternative is
a list of descriptive records; it does not introduce a native Grid/Table
accessibility provider.

The existing chart bounds apply: at most 16 series and 4096 points. Rendering
uses the existing GPU Scene path. There is no CPU bitmap fallback. The example
can be opened with `cargo run -p xaml-form -- --charts-3d`.
