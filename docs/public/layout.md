# Retained layout

`rust-desktop-ui-layout` implements `ui-core::LayoutEngine` using Taffy 0.14.0.
Public styles use owned Rust types and do not expose Taffy nodes or styles.
Layout returns parent-local border boxes in fractional logical pixels. The root
always occupies `(0, 0, viewport.width, viewport.height)`, ignoring its own size
constraints and margin. Zero-sized viewports are valid.

## Panels and spacing

| Panel | Behavior |
|---|---|
| `Leaf` | No children; explicit size/minimum or a registered intrinsic measure callback. |
| `Stack(Axis)` | A single line; child main-axis sizes do not grow or shrink. |
| `Flex(Axis)` | A single line with `flex_grow` and `flex_shrink` distribution. |
| `Wrap(Axis)` | Flex lines wrap when the available main-axis space is exhausted. |
| `Overlay` | Independent children share the padded content area. |
| `Grid` | Explicit or implicit tracks with row-major automatic placement. |

`Length::Px`, `Percent` and `Auto` specify dimensions. Percent values are fractions
of the available parent dimension: `0.5` means 50%. `min_size` and `max_size`
constrain dimensions; positive infinity is an unbounded maximum. `padding` is
inside the border box. `margin` is outside it and changes arrangement without
becoming painted or hittable space. Margins are finite logical pixel values and
may be negative; padding, gap and sizes are nonnegative. There is no margin
collapse or automatic margin value.

For Stack and Flex, `align` controls the cross axis and `justify` controls the
main axis. Wrap uses the same controls per line; lines start at the beginning
of the cross axis, with `gap` between lines and between children. Children keep
their retained identities when available space moves them to a different line.

Optional child `horizontal_alignment` and `vertical_alignment` override inherited
alignment on physical axes. Grid supports both axes independently; Stack/Flex/Wrap
apply the property corresponding to their cross axis. A child cannot align within
unused main-axis space that its container has not allocated to it.
Overlay's automatic dimensions fill the padded area after subtracting
margins, subject to constraints. An explicit child `vertical_alignment` of Start,
Center or End preserves its measured Auto height and positions it accordingly;
Stretch or an omitted alignment keeps the fill behavior. Fixed children use
horizontal `justify` and vertical `align`, with explicit child vertical alignment
taking precedence. Overlay `offset` shifts the resulting child box without affecting
siblings. An Overlay needs an explicit or parent-assigned size because its
children do not contribute intrinsic panel size.

## Grid tracks and placement

```rust
use rust_desktop_ui_core::{
    Edges, GridPlacement, GridTrack, LayoutEngine, LayoutKind, LayoutStyle,
    NodeProps, Size, UiTree,
};
use rust_desktop_ui_layout::TaffyLayout;

let mut tree = UiTree::new();
let root = tree.root();
tree.set_style(root, LayoutStyle {
    kind: LayoutKind::Grid,
    grid_columns: vec![GridTrack::Px(160.0), GridTrack::Fr(1.0)],
    grid_rows: vec![GridTrack::Px(48.0), GridTrack::Fr(1.0)],
    gap: 8.0,
    padding: Edges::all(12.0),
    ..LayoutStyle::default()
})?;
let header = tree.insert(root, NodeProps {
    style: LayoutStyle {
        grid_column: GridPlacement { start: Some(1), span: 2 },
        grid_row: GridPlacement { start: Some(1), span: 1 },
        ..LayoutStyle::default()
    },
    ..NodeProps::default()
})?;
let snapshot = TaffyLayout::new().compute(&tree, Size::new(800.0, 600.0))?;
assert_eq!(snapshot.boxes[&header].width, 776.0);
# Ok::<(), Box<dyn std::error::Error>>(())
```

`GridTrack::Px` fixes a track; `Percent` resolves a fraction of the grid content
dimension; `Fr` distributes available space proportionally; `Auto` uses intrinsic
contributions and normal grid track sizing. The same `gap` separates rows and
columns. Missing explicit tracks are implicit `Auto` tracks. Automatic placement
is row-major and preserves child order. Explicit placements can overlap; paint
order and hit testing continue to use retained sibling order and `z_index`.

`GridPlacement::start` is an optional **one-based** line. `None` chooses automatic
placement. `span` defaults to one and must be positive. For Grid, `align` controls
item alignment within cells on both axes, and `justify` distributes the column
tracks horizontally. Width, height, min/max and margins still apply to items.
Individual children can override the two cell axes through `horizontal_alignment`
and `vertical_alignment`. These alignments move the border box after sizing;
explicit dimensions and min/max constraints remain effective.
Named lines, negative line indices, dense/column flow, repeat, subgrid and public
minmax track expressions are not part of this API.

The [compiled XAML form adapter](forms.md) maps omitted Grid axes to one `Fr(1)`
track and zero-based `Grid.Row`/`Grid.Column` to explicit one-based placements.
This differs deliberately from the backend API's implicit Auto tracks and
automatic placement. DockPanel edge reservation, Border padding/stroke space,
and ScrollViewer content measurement are implemented by that adapter and controls,
using these retained layout primitives.

## Validation and invalidation

Style setters validate before mutation. Non-finite dimensions, invalid ranges,
zero grid starts/spans and more than 1,024 explicit or addressed tracks per axis
are rejected. The adapter also rejects more than 4,096 cumulative child spans per
axis in one grid and a tree depth exceeding 128 edges, before entering recursive
backend layout. Computed boxes are checked for finite nonnegative dimensions.
Errors preserve the runtime's last successful scene and leave layout pending.

Style changes request layout; a layout pass currently rebuilds and computes the
whole backend tree. Paint, hover, focus, visual translation and scrolling do not
request layout. Hidden nodes (`visible = false`) retain their layout space.
`NodeProps::collapsed = true` maps the whole subtree to `Display::None`: its
boxes are zero-sized and its occupied space is removed. Toggling this flag
invalidates layout, while restoring it preserves widget identities and content.
Collapsed nodes are also excluded from painting and input, regardless of the
`visible` field. Clipping, scrolling,
visual translation, painting and hit testing are applied by the retained runtime
after arrangement. Paint border width does not change layout padding or size.

Text measurement is separate: an application/control may measure text and assign
dimensions, or register a leaf callback as described below. The adapter itself
does not depend on a text engine. Geometry remains fractional until the host
applies DPI; the adapter does not round to physical pixels. See [retained runtime](retained-ui.md) and
[text preparation](text.md) for those coordinate contracts.

## Intrinsic measurement for custom controls

`TaffyLayout::set_measure(widget, callback)` registers application-owned intrinsic
content measurement. The callback receives `MeasureInput`: known width/height
and `MeasureSpace::{Definite, MinContent, MaxContent}` on each axis. It returns
`Result<Size, LayoutError>` in logical content pixels, excluding padding. Layout
applies explicit dimensions, min/max constraints, padding, margins and alignment.
Known dimensions override the callback's returned axis. Unregistered leaves
measure as zero intrinsic content.

Measurement may be requested several times with different constraints during
one layout. The callback must be deterministic for the current content and must
not modify the retained tree. Negative or non-finite results reject the complete
layout pass and preserve the previous successful scene. A callback's returned
error propagates through the same path; callbacks must not panic.

`Controls::set_measure` accepts only leaf layout nodes and automatically requests
layout. `Controls::invalidate_measure(widget)` requests fresh measurement when
external content changes. `remove_measure` restores the default measurement
without replacing the widget. At most 4096 callbacks are retained; removed nodes
release callbacks and detached nodes retain them. Paint-only updates do not call
measure callbacks.

For applications using `UiRuntime` directly, `UiRuntime::invalidate_measure`
provides the same invalidation. `MeasuredLayout::new(callback)` implements
`LayoutEngine` with a callback receiving both the `WidgetId` and `MeasureInput`.
`TaffyLayout::compute_with_measure` supplies a one-pass callback that overrides
the registered callbacks for that computation. No backend layout types cross
these interfaces. [SplitContainer and scrolling](splitter-scroll.md) use the
same retained geometry, clipping and visibility rules.

`Controls::update` settles text height, composite hosts, scroll clamping and
anchored overlays before publishing a scene. One successful update therefore
contains the final geometry of nested splitters and slider thumbs after text
reflow. Settlement is bounded to twice the supported layout depth plus eight
passes. Circular intrinsic-size rules return `RuntimeError::InvalidLayout`
instead of silently publishing intermediate geometry. Change or remove the
conflicting rule and call `update` again to recover.

The underlying algorithms and semantics are documented by
[Taffy](https://docs.rs/taffy/0.14.0/taffy/).
