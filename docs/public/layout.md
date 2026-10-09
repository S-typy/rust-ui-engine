# Retained layout

`rust-desktop-ui-layout` implements `ui-core::LayoutEngine` using Taffy 0.14.0.
Public styles use owned Rust types and do not expose Taffy nodes or styles.
Layout returns parent-local border boxes in fractional logical pixels. The root
always occupies `(0, 0, viewport.width, viewport.height)`, ignoring its own size
constraints and margin. Zero-sized viewports are valid.

## Panels and spacing

| Panel | Behavior |
|---|---|
| `Leaf` | No children; intrinsic size is zero without explicit size/minimum. |
| `Stack(Axis)` | A single line; child main-axis sizes do not grow or shrink. |
| `Flex(Axis)` | A single line with `flex_grow` and `flex_shrink` distribution. |
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
main axis. Overlay's automatic dimensions fill the padded area after subtracting
margins, subject to constraints; fixed children use horizontal `justify` and
vertical `align`. Overlay `offset` shifts the resulting child box without affecting
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
Named lines, negative line indices, dense/column flow, repeat, subgrid and public
minmax track expressions are not part of this API.

## Validation and invalidation

Style setters validate before mutation. Non-finite dimensions, invalid ranges,
zero grid starts/spans and more than 1,024 explicit or addressed tracks per axis
are rejected. The adapter also rejects more than 4,096 cumulative child spans per
axis in one grid and a tree depth exceeding 128 edges, before entering recursive
backend layout. Computed boxes are checked for finite nonnegative dimensions.
Errors preserve the runtime's last successful scene and leave layout pending.

Style changes request layout; a layout pass currently rebuilds and computes the
whole backend tree. Paint, hover, focus, visual translation and scrolling do not
request layout. Hidden nodes retain their layout space. Clipping, scrolling,
visual translation, painting and hit testing are applied by the retained runtime
after arrangement. Paint border width does not change layout padding or size.

Text measurement is separate: an application/control measures text and assigns
appropriate dimensions. The adapter does not call a text engine for intrinsic
Leaf sizing. Geometry remains fractional until the host applies DPI; the adapter
does not round to physical pixels. See [retained runtime](retained-ui.md) and
[text preparation](text.md) for those coordinate contracts.

The underlying algorithms and semantics are documented by
[Taffy](https://docs.rs/taffy/0.14.0/taffy/).
