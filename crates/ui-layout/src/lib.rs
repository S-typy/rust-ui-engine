//! Layout adapter for the retained, backend-independent `ui-core` model.
//!
//! `Stack` uses a single line with fixed child main-axis sizes, `Flex` distributes
//! free space through Taffy, `Grid` places children in explicit or automatic tracks,
//! and `Overlay` places independent children in the same
//! padded area. An overlay needs an explicit or parent-assigned size: its children
//! do not contribute intrinsic size. Auto overlay child dimensions fill that area
//! subject to min/max constraints; an explicit vertical alignment other than
//! Stretch keeps Auto height content-sized. Child offsets apply after arrangement.
//!
//! Geometry remains in fractional logical pixels. No rounding, clipping, scroll,
//! paint borders, or visual translations are applied here. Each actual layout pass
//! rebuilds a temporary Taffy tree; the runtime skips this work for paint and idle.

use rust_desktop_ui_core::{
    Align, Axis, GridPlacement, GridTrack, Justify, LayoutEngine, LayoutError, LayoutKind,
    LayoutSnapshot, LayoutStyle, Length, Rect, Size, UiTree, WidgetId,
};
use std::collections::HashMap;
use taffy::{
    AvailableSpace, Dimension, Display, FlexDirection, LengthPercentage, LengthPercentageAuto,
    Position, Style, TaffyTree,
};

/// Maximum root-to-node edge count supported by this recursive backend.
///
/// The retained tree itself is not subject to this limit. Checking depth before
/// calling Taffy prevents a pathological tree from overflowing the native stack.
pub const MAX_LAYOUT_DEPTH: usize = 128;

type MeasureCallback = Box<dyn FnMut(MeasureInput) -> Result<Size, LayoutError>>;

/// Computes layout without exposing third-party style or node types.
#[derive(Default)]
pub struct TaffyLayout {
    measures: HashMap<WidgetId, MeasureCallback>,
}

impl std::fmt::Debug for TaffyLayout {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TaffyLayout")
            .field("custom_measure_count", &self.measures.len())
            .finish()
    }
}

impl TaffyLayout {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register or replace a leaf's intrinsic measure callback. At most 4096
    /// callbacks may be registered. Request runtime invalidation after changing
    /// a callback or its externally owned content.
    pub fn set_measure(
        &mut self,
        id: WidgetId,
        measure: impl FnMut(MeasureInput) -> Result<Size, LayoutError> + 'static,
    ) -> Result<(), LayoutError> {
        if !self.measures.contains_key(&id) && self.measures.len() >= 4096 {
            return Err(LayoutError::Backend(
                "At most 4096 custom measures may be registered".into(),
            ));
        }
        self.measures.insert(id, Box::new(measure));
        Ok(())
    }

    pub fn remove_measure(&mut self, id: WidgetId) -> bool {
        self.measures.remove(&id).is_some()
    }

    /// Discard callbacks for removed nodes; detached nodes retain their callback.
    pub fn retain_measures(&mut self, tree: &UiTree) {
        self.measures.retain(|id, _| tree.contains(*id));
    }

    /// Measure leaf content while keeping backend types outside the public API.
    /// The callback may run several times with different constraints. It must
    /// return finite nonnegative content sizes and must not mutate the tree.
    pub fn compute_with_measure(
        &mut self,
        tree: &UiTree,
        viewport: Size,
        mut measure: impl FnMut(WidgetId, MeasureInput) -> Result<Size, LayoutError>,
    ) -> Result<LayoutSnapshot, LayoutError> {
        Self::compute_measured(tree, viewport, &mut measure)
    }
}

/// Available content space on one axis, before the leaf's own padding.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MeasureSpace {
    Definite(f32),
    MinContent,
    MaxContent,
}

/// Intrinsic measurement request in logical pixels. Known dimensions take
/// precedence over a callback's result; `None` requests intrinsic sizing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MeasureInput {
    pub width: Option<f32>,
    pub height: Option<f32>,
    pub available_width: MeasureSpace,
    pub available_height: MeasureSpace,
}

/// Layout engine with an application-owned leaf measurement function.
///
/// Changes to externally owned content require runtime layout invalidation.
/// The callback is not invoked during paint-only updates.
pub struct MeasuredLayout<F> {
    measure: F,
}

impl<F> MeasuredLayout<F> {
    pub const fn new(measure: F) -> Self {
        Self { measure }
    }
}

impl<F> LayoutEngine for MeasuredLayout<F>
where
    F: FnMut(WidgetId, MeasureInput) -> Result<Size, LayoutError>,
{
    fn compute(&mut self, tree: &UiTree, viewport: Size) -> Result<LayoutSnapshot, LayoutError> {
        TaffyLayout::new().compute_with_measure(tree, viewport, &mut self.measure)
    }
}

fn measure_space(value: AvailableSpace) -> MeasureSpace {
    match value {
        AvailableSpace::Definite(value) => MeasureSpace::Definite(value),
        AvailableSpace::MinContent => MeasureSpace::MinContent,
        AvailableSpace::MaxContent => MeasureSpace::MaxContent,
    }
}

#[derive(Clone, Copy)]
struct BackendNode {
    node: taffy::NodeId,
    // Overlay children use a private padded-area wrapper for percentage sizing.
    wrapper: Option<taffy::NodeId>,
}

fn backend_error(error: taffy::TaffyError) -> LayoutError {
    LayoutError::Backend(error.to_string())
}

impl LayoutEngine for TaffyLayout {
    fn compute(&mut self, tree: &UiTree, viewport: Size) -> Result<LayoutSnapshot, LayoutError> {
        self.retain_measures(tree);
        Self::compute_measured(tree, viewport, &mut |id, input| {
            self.measures
                .get_mut(&id)
                .map_or(Ok(Size::ZERO), |measure| measure(input))
        })
    }
}

impl TaffyLayout {
    fn compute_measured(
        tree: &UiTree,
        viewport: Size,
        measure: &mut impl FnMut(WidgetId, MeasureInput) -> Result<Size, LayoutError>,
    ) -> Result<LayoutSnapshot, LayoutError> {
        if !viewport.is_valid() {
            return Err(LayoutError::InvalidViewport);
        }
        let mut order = Vec::with_capacity(tree.len());
        let mut pending = vec![(tree.root(), 0)];
        while let Some((id, depth)) = pending.pop() {
            if depth > MAX_LAYOUT_DEPTH {
                return Err(LayoutError::Backend(format!(
                    "Tree depth exceeds the supported limit of {MAX_LAYOUT_DEPTH}"
                )));
            }
            let node = tree.node(id).ok_or(LayoutError::MissingNode(id))?;
            node.props
                .style
                .validate()
                .map_err(|message| LayoutError::InvalidStyle { node: id, message })?;
            let children = tree.children(id).ok_or(LayoutError::MissingNode(id))?;
            if node.props.style.kind == LayoutKind::Leaf && !children.is_empty() {
                return Err(LayoutError::InvalidStyle {
                    node: id,
                    message: "Leaf layout nodes cannot contain children",
                });
            }
            if node.props.style.kind == LayoutKind::Grid {
                // Bound implicit track growth before Taffy's indexed placement.
                let spans = children
                    .iter()
                    .try_fold((0usize, 0usize), |(rows, cols), child| {
                        let child = tree.node(*child).ok_or(LayoutError::MissingNode(*child))?;
                        Ok::<_, LayoutError>((
                            rows + usize::from(child.props.style.grid_row.span),
                            cols + usize::from(child.props.style.grid_column.span),
                        ))
                    })?;
                if spans.0 > 4096 || spans.1 > 4096 {
                    return Err(LayoutError::InvalidStyle {
                        node: id,
                        message: "A grid supports at most 4096 cumulative child spans per axis",
                    });
                }
            }
            order.push(id);
            pending.extend(children.iter().rev().map(|&child| (child, depth + 1)));
        }

        let mut backend = TaffyTree::<WidgetId>::new();
        backend.disable_rounding();
        let mut mapping: HashMap<WidgetId, BackendNode> = HashMap::with_capacity(order.len());
        for &id in order.iter().rev() {
            let node = tree.node(id).ok_or(LayoutError::MissingNode(id))?;
            let parent_style = tree
                .parent(id)
                .and_then(|parent| tree.node(parent))
                .map(|n| &n.props.style);
            let mut style = map_style(&node.props.style, parent_style);
            if node.props.collapsed {
                style.display = Display::None;
            }
            if id == tree.root() {
                // Root geometry is the viewport, independent of root size constraints.
                style.size = taffy::Size {
                    width: Dimension::length(viewport.width),
                    height: Dimension::length(viewport.height),
                };
                style.min_size = taffy::Size {
                    width: LengthPercentageAuto::length(viewport.width),
                    height: LengthPercentageAuto::length(viewport.height),
                };
                style.max_size = style.min_size;
                style.margin = taffy::Rect::zero();
            }
            let mut children = Vec::new();
            for &child in tree.children(id).ok_or(LayoutError::MissingNode(id))? {
                let mapped = mapping
                    .get_mut(&child)
                    .ok_or(LayoutError::MissingNode(child))?;
                if node.props.style.kind == LayoutKind::Overlay {
                    let wrapper = backend
                        .new_with_children(overlay_wrapper(&node.props.style), &[mapped.node])
                        .map_err(backend_error)?;
                    mapped.wrapper = Some(wrapper);
                    children.push(wrapper);
                } else {
                    children.push(mapped.node);
                }
            }
            let backend_node = backend
                .new_with_children(style, &children)
                .map_err(backend_error)?;
            if children.is_empty() && node.props.style.kind == LayoutKind::Leaf {
                backend
                    .set_node_context(backend_node, Some(id))
                    .map_err(backend_error)?;
            }
            mapping.insert(
                id,
                BackendNode {
                    node: backend_node,
                    wrapper: None,
                },
            );
        }
        let root = mapping
            .get(&tree.root())
            .ok_or(LayoutError::MissingNode(tree.root()))?;
        let mut measure_error = None;
        backend
            .compute_layout_with_measure(
                root.node,
                taffy::Size {
                    width: AvailableSpace::Definite(viewport.width),
                    height: AvailableSpace::Definite(viewport.height),
                },
                |inputs, _, context, style| {
                    taffy::compute_leaf_layout(inputs, style, |_, _| 0.0, |known, available| {
                        let request = MeasureInput {
                            width: known.width,
                            height: known.height,
                            available_width: measure_space(available.width),
                            available_height: measure_space(available.height),
                        };
                        let size = if measure_error.is_some() {
                            Size::ZERO
                        } else if let Some(id) = context.as_deref() {
                            match measure(*id, request) {
                                Ok(size) if size.is_valid() => size,
                                Ok(_) => {
                                    measure_error = Some(LayoutError::InvalidStyle { node: *id, message: "Measure returned a non-finite or negative size" });
                                    Size::ZERO
                                }
                                Err(error) => {
                                    measure_error = Some(error);
                                    Size::ZERO
                                }
                            }
                        } else {
                            Size::ZERO
                        };
                        taffy::Size { width: known.width.unwrap_or(size.width), height: known.height.unwrap_or(size.height) }
                    })
                },
            )
            .map_err(backend_error)?;
        if let Some(error) = measure_error {
            return Err(error);
        }

        let mut snapshot = LayoutSnapshot {
            boxes: HashMap::with_capacity(order.len()),
            measured_nodes: order.len(),
            arranged_nodes: order.len(),
        };
        for id in order {
            let mapped = mapping.get(&id).ok_or(LayoutError::MissingNode(id))?;
            let result = backend.layout(mapped.node).map_err(backend_error)?;
            let mut bounds = Rect::new(
                result.location.x,
                result.location.y,
                result.size.width,
                result.size.height,
            );
            if let Some(wrapper) = mapped.wrapper {
                let wrapper = backend.layout(wrapper).map_err(backend_error)?;
                let offset = tree
                    .node(id)
                    .ok_or(LayoutError::MissingNode(id))?
                    .props
                    .style
                    .offset;
                bounds.x += wrapper.location.x + offset.x;
                bounds.y += wrapper.location.y + offset.y;
            }
            if id == tree.root() {
                bounds = Rect::new(0.0, 0.0, viewport.width, viewport.height);
            }
            if !bounds.is_valid()
                || !(bounds.x + bounds.width).is_finite()
                || !(bounds.y + bounds.height).is_finite()
            {
                return Err(LayoutError::Backend(format!(
                    "Non-finite or negative computed geometry for {id:?}"
                )));
            }
            snapshot.boxes.insert(id, bounds);
        }
        Ok(snapshot)
    }
}

fn dimension(length: Length) -> Dimension {
    match length {
        Length::Auto => Dimension::auto(),
        Length::Px(value) => Dimension::length(value),
        Length::Percent(value) => Dimension::percent(value),
    }
}

fn maximum(value: f32) -> LengthPercentageAuto {
    if value == f32::INFINITY {
        LengthPercentageAuto::auto()
    } else {
        LengthPercentageAuto::length(value)
    }
}

fn alignment(align: Align) -> taffy::AlignItems {
    match align {
        Align::Start => taffy::AlignItems::START,
        Align::Center => taffy::AlignItems::CENTER,
        Align::End => taffy::AlignItems::END,
        Align::Stretch => taffy::AlignItems::STRETCH,
    }
}

fn justification(justify: Justify) -> taffy::JustifyContent {
    match justify {
        Justify::Start => taffy::JustifyContent::START,
        Justify::Center => taffy::JustifyContent::CENTER,
        Justify::End => taffy::JustifyContent::END,
        Justify::SpaceBetween => taffy::JustifyContent::SPACE_BETWEEN,
    }
}

fn grid_track(track: GridTrack) -> taffy::GridTemplateComponent<String> {
    use taffy::prelude::{auto, fr, length, percent};
    match track {
        GridTrack::Auto => auto(),
        GridTrack::Px(value) => length(value),
        GridTrack::Percent(value) => percent(value),
        GridTrack::Fr(value) => fr(value),
    }
}

fn grid_placement(placement: GridPlacement) -> taffy::Line<taffy::GridPlacement<String>> {
    taffy::Line {
        start: placement.start.map_or(taffy::GridPlacement::Auto, |start| {
            taffy::prelude::line(start as i16)
        }),
        end: taffy::prelude::span(placement.span),
    }
}

fn map_style(style: &LayoutStyle, parent: Option<&LayoutStyle>) -> Style {
    let (flex_grow, flex_shrink) = match parent.map(|p| p.kind) {
        Some(LayoutKind::Stack(_)) => (0.0, 0.0),
        Some(LayoutKind::Overlay) => (f32::from(style.width == Length::Auto), 0.0),
        _ => (style.flex_grow, style.flex_shrink),
    };
    Style {
        display: if style.kind == LayoutKind::Grid {
            Display::Grid
        } else {
            Display::Flex
        },
        size: taffy::Size {
            width: dimension(style.width),
            height: dimension(style.height),
        },
        min_size: taffy::Size {
            width: LengthPercentageAuto::length(style.min_size.width),
            height: LengthPercentageAuto::length(style.min_size.height),
        },
        max_size: taffy::Size {
            width: maximum(style.max_size.width),
            height: maximum(style.max_size.height),
        },
        padding: taffy::Rect {
            left: LengthPercentage::length(style.padding.left),
            right: LengthPercentage::length(style.padding.right),
            top: LengthPercentage::length(style.padding.top),
            bottom: LengthPercentage::length(style.padding.bottom),
        },
        margin: taffy::Rect {
            left: LengthPercentageAuto::length(style.margin.left),
            right: LengthPercentageAuto::length(style.margin.right),
            top: LengthPercentageAuto::length(style.margin.top),
            bottom: LengthPercentageAuto::length(style.margin.bottom),
        },
        grid_template_columns: style.grid_columns.iter().copied().map(grid_track).collect(),
        grid_template_rows: style.grid_rows.iter().copied().map(grid_track).collect(),
        grid_column: grid_placement(style.grid_column),
        grid_row: grid_placement(style.grid_row),
        justify_items: Some(alignment(style.align)),
        gap: taffy::Size {
            width: LengthPercentage::length(style.gap),
            height: LengthPercentage::length(style.gap),
        },
        flex_direction: match style.kind {
            LayoutKind::Stack(Axis::Vertical)
            | LayoutKind::Flex(Axis::Vertical)
            | LayoutKind::Wrap(Axis::Vertical) => FlexDirection::Column,
            _ => FlexDirection::Row,
        },
        flex_grow,
        flex_shrink,
        flex_wrap: if matches!(style.kind, LayoutKind::Wrap(_)) {
            taffy::FlexWrap::Wrap
        } else {
            taffy::FlexWrap::NoWrap
        },
        align_content: Some(taffy::AlignContent::START),
        align_items: Some(alignment(style.align)),
        justify_content: Some(justification(style.justify)),
        // An overlay stretches unspecified Auto height, but an explicit
        // alignment must keep a content-sized child at its requested position.
        align_self: match parent.map(|p| p.kind) {
            Some(
                LayoutKind::Stack(Axis::Vertical)
                | LayoutKind::Flex(Axis::Vertical)
                | LayoutKind::Wrap(Axis::Vertical),
            ) => style.horizontal_alignment.map(alignment),
            Some(LayoutKind::Grid) => style.vertical_alignment.map(alignment),
            Some(LayoutKind::Overlay)
                if style.height == Length::Auto && style.vertical_alignment.is_none() =>
            {
                Some(taffy::AlignSelf::STRETCH)
            }
            _ => style.vertical_alignment.map(alignment),
        },
        justify_self: style.horizontal_alignment.map(alignment),
        ..Style::default()
    }
}

fn overlay_wrapper(style: &LayoutStyle) -> Style {
    Style {
        position: Position::Absolute,
        inset: taffy::Rect {
            left: LengthPercentageAuto::length(style.padding.left),
            right: LengthPercentageAuto::length(style.padding.right),
            top: LengthPercentageAuto::length(style.padding.top),
            bottom: LengthPercentageAuto::length(style.padding.bottom),
        },
        min_size: taffy::Size {
            width: LengthPercentageAuto::length(0.0),
            height: LengthPercentageAuto::length(0.0),
        },
        align_items: Some(alignment(style.align)),
        justify_content: Some(justification(style.justify)),
        ..Style::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_desktop_ui_core::{Edges, NodeProps, Point};

    fn props(style: LayoutStyle) -> NodeProps {
        NodeProps {
            style,
            ..NodeProps::default()
        }
    }

    fn sized(width: f32, height: f32) -> LayoutStyle {
        LayoutStyle {
            width: Length::Px(width),
            height: Length::Px(height),
            ..LayoutStyle::default()
        }
    }

    fn compute(tree: &UiTree, width: f32, height: f32) -> LayoutSnapshot {
        TaffyLayout::new()
            .compute(tree, Size::new(width, height))
            .unwrap()
    }

    fn assert_rect(snapshot: &LayoutSnapshot, node: WidgetId, expected: Rect) {
        let actual = snapshot.boxes[&node];
        for (a, b) in [
            (actual.x, expected.x),
            (actual.y, expected.y),
            (actual.width, expected.width),
            (actual.height, expected.height),
        ] {
            assert!(
                (a - b).abs() < 0.001,
                "actual={actual:?}, expected={expected:?}"
            );
        }
    }

    #[test]
    fn stack_keeps_sizes_and_gap_when_space_runs_out() {
        let mut tree = UiTree::new();
        let root = tree.root();
        tree.set_style(
            root,
            LayoutStyle {
                kind: LayoutKind::Stack(Axis::Vertical),
                padding: Edges::all(5.0),
                gap: 3.0,
                ..LayoutStyle::default()
            },
        )
        .unwrap();
        let first = tree
            .insert(
                root,
                props(LayoutStyle {
                    flex_grow: 5.0,
                    flex_shrink: 5.0,
                    ..sized(20.0, 30.0)
                }),
            )
            .unwrap();
        let second = tree.insert(root, props(sized(40.0, 30.0))).unwrap();
        let snapshot = compute(&tree, 100.0, 50.0);
        assert_rect(&snapshot, first, Rect::new(5.0, 5.0, 20.0, 30.0));
        assert_rect(&snapshot, second, Rect::new(5.0, 38.0, 40.0, 30.0));
        assert_eq!(snapshot.measured_nodes, 3);
        assert_eq!(snapshot.arranged_nodes, 3);
    }

    #[test]
    fn flex_distributes_growth_and_respects_maximum() {
        let mut tree = UiTree::new();
        let root = tree.root();
        tree.set_style(
            root,
            LayoutStyle {
                kind: LayoutKind::Flex(Axis::Horizontal),
                gap: 10.0,
                ..LayoutStyle::default()
            },
        )
        .unwrap();
        let first = tree
            .insert(
                root,
                props(LayoutStyle {
                    width: Length::Px(50.0),
                    flex_grow: 1.0,
                    max_size: Size::new(80.0, f32::INFINITY),
                    ..LayoutStyle::default()
                }),
            )
            .unwrap();
        let second = tree
            .insert(
                root,
                props(LayoutStyle {
                    width: Length::Px(50.0),
                    flex_grow: 1.0,
                    ..LayoutStyle::default()
                }),
            )
            .unwrap();
        let snapshot = compute(&tree, 300.0, 60.0);
        assert_rect(&snapshot, first, Rect::new(0.0, 0.0, 80.0, 60.0));
        assert_rect(&snapshot, second, Rect::new(90.0, 0.0, 210.0, 60.0));
    }

    #[test]
    fn flex_shrink_respects_minimum_and_axis() {
        let mut tree = UiTree::new();
        let root = tree.root();
        tree.set_style(
            root,
            LayoutStyle {
                kind: LayoutKind::Flex(Axis::Vertical),
                ..LayoutStyle::default()
            },
        )
        .unwrap();
        let first = tree
            .insert(
                root,
                props(LayoutStyle {
                    height: Length::Px(100.0),
                    min_size: Size::new(0.0, 80.0),
                    ..LayoutStyle::default()
                }),
            )
            .unwrap();
        let second = tree
            .insert(
                root,
                props(LayoutStyle {
                    height: Length::Px(100.0),
                    ..LayoutStyle::default()
                }),
            )
            .unwrap();
        let snapshot = compute(&tree, 60.0, 120.0);
        assert_rect(&snapshot, first, Rect::new(0.0, 0.0, 60.0, 80.0));
        assert_rect(&snapshot, second, Rect::new(0.0, 80.0, 60.0, 40.0));
    }

    #[test]
    fn overlay_auto_percent_padding_and_offsets_are_parent_local() {
        let mut tree = UiTree::new();
        let root = tree.root();
        tree.set_style(
            root,
            LayoutStyle {
                kind: LayoutKind::Overlay,
                padding: Edges {
                    left: 10.0,
                    right: 20.0,
                    top: 5.0,
                    bottom: 15.0,
                },
                ..LayoutStyle::default()
            },
        )
        .unwrap();
        let auto = tree.insert(root, NodeProps::default()).unwrap();
        let half = tree
            .insert(
                root,
                props(LayoutStyle {
                    width: Length::Percent(0.5),
                    height: Length::Percent(0.5),
                    offset: Point::new(2.0, -3.0),
                    ..LayoutStyle::default()
                }),
            )
            .unwrap();
        let snapshot = compute(&tree, 200.0, 100.0);
        assert_rect(&snapshot, auto, Rect::new(10.0, 5.0, 170.0, 80.0));
        assert_rect(&snapshot, half, Rect::new(12.0, 2.0, 85.0, 40.0));
    }

    #[test]
    fn overlay_auto_height_preserves_explicit_alignment_and_child_measurement() {
        for (alignment, top, height) in [
            (None, 8.0, 64.0),
            (Some(Align::Stretch), 8.0, 64.0),
            (Some(Align::Start), 8.0, 24.0),
            (Some(Align::Center), 28.0, 24.0),
            (Some(Align::End), 48.0, 24.0),
        ] {
            let mut tree = UiTree::new();
            let root = tree.root();
            tree.set_style(
                root,
                LayoutStyle {
                    kind: LayoutKind::Overlay,
                    padding: Edges::all(8.0),
                    ..LayoutStyle::default()
                },
            )
            .unwrap();
            let content = tree
                .insert(
                    root,
                    props(LayoutStyle {
                        kind: LayoutKind::Stack(Axis::Horizontal),
                        width: Length::Px(100.0),
                        vertical_alignment: alignment,
                        ..LayoutStyle::default()
                    }),
                )
                .unwrap();
            let child = tree.insert(content, props(sized(40.0, 24.0))).unwrap();
            let snapshot = compute(&tree, 240.0, 80.0);
            assert_rect(&snapshot, content, Rect::new(8.0, top, 100.0, height));
            assert_rect(&snapshot, child, Rect::new(0.0, 0.0, 40.0, 24.0));
        }
    }

    #[test]
    fn nested_stack_overlay_and_flex_preserve_fractional_local_boxes() {
        let mut tree = UiTree::new();
        let root = tree.root();
        tree.set_style(
            root,
            LayoutStyle {
                kind: LayoutKind::Stack(Axis::Vertical),
                padding: Edges::all(2.5),
                ..LayoutStyle::default()
            },
        )
        .unwrap();
        let overlay = tree
            .insert(
                root,
                props(LayoutStyle {
                    kind: LayoutKind::Overlay,
                    height: Length::Px(80.5),
                    padding: Edges::all(1.25),
                    ..LayoutStyle::default()
                }),
            )
            .unwrap();
        let flex = tree
            .insert(
                overlay,
                props(LayoutStyle {
                    kind: LayoutKind::Flex(Axis::Horizontal),
                    gap: 0.5,
                    ..LayoutStyle::default()
                }),
            )
            .unwrap();
        let left = tree
            .insert(
                flex,
                props(LayoutStyle {
                    flex_grow: 1.0,
                    ..LayoutStyle::default()
                }),
            )
            .unwrap();
        let right = tree
            .insert(
                flex,
                props(LayoutStyle {
                    flex_grow: 1.0,
                    ..LayoutStyle::default()
                }),
            )
            .unwrap();
        let snapshot = compute(&tree, 101.0, 100.0);
        assert_rect(&snapshot, overlay, Rect::new(2.5, 2.5, 96.0, 80.5));
        assert_rect(&snapshot, flex, Rect::new(1.25, 1.25, 93.5, 78.0));
        assert_rect(&snapshot, left, Rect::new(0.0, 0.0, 46.5, 78.0));
        assert_rect(&snapshot, right, Rect::new(47.0, 0.0, 46.5, 78.0));
    }

    #[test]
    fn alignment_and_justification_apply_to_stacks() {
        let mut tree = UiTree::new();
        let root = tree.root();
        tree.set_style(
            root,
            LayoutStyle {
                kind: LayoutKind::Stack(Axis::Horizontal),
                align: Align::Center,
                justify: Justify::SpaceBetween,
                padding: Edges::all(10.0),
                ..LayoutStyle::default()
            },
        )
        .unwrap();
        let first = tree.insert(root, props(sized(20.0, 20.0))).unwrap();
        let second = tree.insert(root, props(sized(30.0, 10.0))).unwrap();
        let snapshot = compute(&tree, 100.0, 60.0);
        assert_rect(&snapshot, first, Rect::new(10.0, 20.0, 20.0, 20.0));
        assert_rect(&snapshot, second, Rect::new(60.0, 25.0, 30.0, 10.0));
    }

    #[test]
    fn root_constraints_do_not_override_viewport_and_resize_has_no_stale_cache() {
        let mut tree = UiTree::new();
        let root = tree.root();
        tree.set_style(
            root,
            LayoutStyle {
                kind: LayoutKind::Overlay,
                min_size: Size::new(300.0, 300.0),
                max_size: Size::new(500.0, 500.0),
                ..sized(400.0, 400.0)
            },
        )
        .unwrap();
        let child = tree.insert(root, NodeProps::default()).unwrap();
        let mut engine = TaffyLayout::new();
        for size in [Size::new(100.0, 70.0), Size::ZERO, Size::new(30.5, 20.25)] {
            let snapshot = engine.compute(&tree, size).unwrap();
            assert_rect(
                &snapshot,
                root,
                Rect::new(0.0, 0.0, size.width, size.height),
            );
            assert_rect(
                &snapshot,
                child,
                Rect::new(0.0, 0.0, size.width, size.height),
            );
        }
    }

    #[test]
    fn root_padding_larger_than_viewport_leaves_zero_content_space() {
        let mut tree = UiTree::new();
        let root = tree.root();
        tree.set_style(
            root,
            LayoutStyle {
                kind: LayoutKind::Overlay,
                padding: Edges::all(10.0),
                ..LayoutStyle::default()
            },
        )
        .unwrap();
        let child = tree.insert(root, NodeProps::default()).unwrap();
        let snapshot = compute(&tree, 5.0, 0.0);
        assert_rect(&snapshot, root, Rect::new(0.0, 0.0, 5.0, 0.0));
        assert_rect(&snapshot, child, Rect::new(10.0, 10.0, 0.0, 0.0));
    }

    #[test]
    fn invalid_viewports_return_errors_without_backend_work() {
        let tree = UiTree::new();
        for size in [
            Size::new(f32::NAN, 10.0),
            Size::new(10.0, f32::INFINITY),
            Size::new(-1.0, 10.0),
        ] {
            assert_eq!(
                TaffyLayout::new().compute(&tree, size).unwrap_err(),
                LayoutError::InvalidViewport
            );
        }
    }

    #[test]
    fn excessive_depth_is_rejected_before_recursive_layout() {
        let mut tree = UiTree::new();
        let mut parent = tree.root();
        tree.set_style(
            parent,
            LayoutStyle {
                kind: LayoutKind::Overlay,
                ..LayoutStyle::default()
            },
        )
        .unwrap();
        for _ in 0..=MAX_LAYOUT_DEPTH {
            parent = tree
                .insert(
                    parent,
                    props(LayoutStyle {
                        kind: LayoutKind::Overlay,
                        ..LayoutStyle::default()
                    }),
                )
                .unwrap();
        }
        assert!(
            matches!(TaffyLayout::new().compute(&tree, Size::new(100.0, 100.0)), Err(LayoutError::Backend(message)) if message.contains("depth"))
        );
    }

    #[test]
    fn deepest_supported_overlay_is_computed() {
        let mut tree = UiTree::new();
        let mut parent = tree.root();
        tree.set_style(
            parent,
            LayoutStyle {
                kind: LayoutKind::Overlay,
                ..LayoutStyle::default()
            },
        )
        .unwrap();
        for _ in 0..MAX_LAYOUT_DEPTH {
            parent = tree
                .insert(
                    parent,
                    props(LayoutStyle {
                        kind: LayoutKind::Overlay,
                        ..LayoutStyle::default()
                    }),
                )
                .unwrap();
        }
        let snapshot = compute(&tree, 100.0, 100.0);
        assert_rect(&snapshot, parent, Rect::new(0.0, 0.0, 100.0, 100.0));
    }

    #[test]
    fn overlay_auto_dimensions_obey_constraints_and_explicit_children_align() {
        let mut tree = UiTree::new();
        let root = tree.root();
        tree.set_style(
            root,
            LayoutStyle {
                kind: LayoutKind::Overlay,
                align: Align::End,
                justify: Justify::Center,
                padding: Edges::all(10.0),
                ..LayoutStyle::default()
            },
        )
        .unwrap();
        let auto = tree
            .insert(
                root,
                props(LayoutStyle {
                    max_size: Size::new(50.0, 30.0),
                    ..LayoutStyle::default()
                }),
            )
            .unwrap();
        let fixed = tree.insert(root, props(sized(20.0, 20.0))).unwrap();
        let snapshot = compute(&tree, 100.0, 100.0);
        assert_rect(&snapshot, auto, Rect::new(25.0, 10.0, 50.0, 30.0));
        assert_rect(&snapshot, fixed, Rect::new(40.0, 70.0, 20.0, 20.0));
    }

    #[test]
    fn auto_overlay_does_not_claim_child_intrinsic_sizing() {
        let mut tree = UiTree::new();
        let root = tree.root();
        tree.set_style(
            root,
            LayoutStyle {
                kind: LayoutKind::Stack(Axis::Vertical),
                ..LayoutStyle::default()
            },
        )
        .unwrap();
        let overlay = tree
            .insert(
                root,
                props(LayoutStyle {
                    kind: LayoutKind::Overlay,
                    padding: Edges::all(2.0),
                    ..LayoutStyle::default()
                }),
            )
            .unwrap();
        let child = tree.insert(overlay, props(sized(50.0, 50.0))).unwrap();
        let snapshot = compute(&tree, 100.0, 100.0);
        assert_rect(&snapshot, overlay, Rect::new(0.0, 0.0, 100.0, 4.0));
        assert_rect(&snapshot, child, Rect::new(2.0, 2.0, 50.0, 50.0));
    }

    #[test]
    fn leaf_children_and_overflowing_arithmetic_are_errors() {
        let mut tree = UiTree::new();
        tree.insert(tree.root(), NodeProps::default()).unwrap();
        assert!(matches!(
            TaffyLayout::new().compute(&tree, Size::new(100.0, 100.0)),
            Err(LayoutError::InvalidStyle { .. })
        ));
        tree.set_style(
            tree.root(),
            LayoutStyle {
                kind: LayoutKind::Overlay,
                ..LayoutStyle::default()
            },
        )
        .unwrap();
        tree.insert(
            tree.root(),
            props(LayoutStyle {
                offset: Point::new(f32::MAX, 0.0),
                ..sized(f32::MAX, 20.0)
            }),
        )
        .unwrap();
        assert!(matches!(
            TaffyLayout::new().compute(&tree, Size::new(100.0, 100.0)),
            Err(LayoutError::Backend(_))
        ));
    }

    #[test]
    fn hidden_nodes_keep_geometry_and_visual_properties_do_not_affect_layout() {
        let mut tree = UiTree::new();
        let root = tree.root();
        tree.set_style(
            root,
            LayoutStyle {
                kind: LayoutKind::Stack(Axis::Horizontal),
                ..LayoutStyle::default()
            },
        )
        .unwrap();
        let hidden = tree
            .insert(
                root,
                NodeProps {
                    visible: false,
                    scroll: Point::new(30.0, 30.0),
                    translation: Point::new(40.0, 40.0),
                    style: sized(20.0, 20.0),
                    ..NodeProps::default()
                },
            )
            .unwrap();
        let next = tree.insert(root, props(sized(20.0, 20.0))).unwrap();
        let snapshot = compute(&tree, 100.0, 50.0);
        assert_rect(&snapshot, hidden, Rect::new(0.0, 0.0, 20.0, 20.0));
        assert_rect(&snapshot, next, Rect::new(20.0, 0.0, 20.0, 20.0));
    }

    #[test]
    fn grid_fixed_fractional_tracks_span_and_resize() {
        let mut tree = UiTree::new();
        let root = tree.root();
        tree.set_style(
            root,
            LayoutStyle {
                kind: LayoutKind::Grid,
                grid_columns: vec![GridTrack::Px(40.0), GridTrack::Fr(1.0), GridTrack::Fr(2.0)],
                grid_rows: vec![GridTrack::Px(20.0), GridTrack::Fr(1.0)],
                padding: Edges::all(5.0),
                gap: 3.0,
                ..LayoutStyle::default()
            },
        )
        .unwrap();
        let header = tree
            .insert(
                root,
                props(LayoutStyle {
                    grid_column: GridPlacement {
                        start: Some(1),
                        span: 3,
                    },
                    grid_row: GridPlacement {
                        start: Some(1),
                        span: 1,
                    },
                    ..LayoutStyle::default()
                }),
            )
            .unwrap();
        let left = tree
            .insert(
                root,
                props(LayoutStyle {
                    grid_column: GridPlacement {
                        start: Some(1),
                        span: 1,
                    },
                    grid_row: GridPlacement {
                        start: Some(2),
                        span: 1,
                    },
                    ..LayoutStyle::default()
                }),
            )
            .unwrap();
        let middle = tree
            .insert(
                root,
                props(LayoutStyle {
                    grid_column: GridPlacement {
                        start: Some(2),
                        span: 1,
                    },
                    grid_row: GridPlacement {
                        start: Some(2),
                        span: 1,
                    },
                    ..LayoutStyle::default()
                }),
            )
            .unwrap();
        let right = tree
            .insert(
                root,
                props(LayoutStyle {
                    grid_column: GridPlacement {
                        start: Some(3),
                        span: 1,
                    },
                    grid_row: GridPlacement {
                        start: Some(2),
                        span: 1,
                    },
                    ..LayoutStyle::default()
                }),
            )
            .unwrap();
        for width in [200.0, 260.0] {
            let layout = compute(&tree, width, 100.0);
            let fr = (width - 56.0) / 3.0;
            assert_rect(&layout, header, Rect::new(5.0, 5.0, width - 10.0, 20.0));
            assert_rect(&layout, left, Rect::new(5.0, 28.0, 40.0, 67.0));
            assert_rect(&layout, middle, Rect::new(48.0, 28.0, fr, 67.0));
            assert_rect(&layout, right, Rect::new(51.0 + fr, 28.0, 2.0 * fr, 67.0));
        }
    }

    #[test]
    fn grid_auto_placement_percent_tracks_and_item_alignment() {
        let mut tree = UiTree::new();
        let root = tree.root();
        tree.set_style(
            root,
            LayoutStyle {
                kind: LayoutKind::Grid,
                grid_columns: vec![GridTrack::Percent(0.25), GridTrack::Fr(1.0)],
                grid_rows: vec![GridTrack::Px(30.0), GridTrack::Px(40.0)],
                align: Align::Center,
                ..LayoutStyle::default()
            },
        )
        .unwrap();
        let first = tree.insert(root, props(sized(20.0, 10.0))).unwrap();
        let second = tree.insert(root, props(sized(20.0, 10.0))).unwrap();
        let third = tree.insert(root, props(sized(20.0, 10.0))).unwrap();
        let layout = compute(&tree, 200.0, 70.0);
        assert_rect(&layout, first, Rect::new(15.0, 10.0, 20.0, 10.0));
        assert_rect(&layout, second, Rect::new(115.0, 10.0, 20.0, 10.0));
        assert_rect(&layout, third, Rect::new(15.0, 45.0, 20.0, 10.0));
    }

    #[test]
    fn margins_participate_in_stack_flex_grid_and_overlay_without_being_painted() {
        for kind in [
            LayoutKind::Stack(Axis::Horizontal),
            LayoutKind::Flex(Axis::Horizontal),
            LayoutKind::Grid,
            LayoutKind::Overlay,
        ] {
            let mut tree = UiTree::new();
            let root = tree.root();
            tree.set_style(
                root,
                LayoutStyle {
                    kind,
                    grid_columns: vec![GridTrack::Fr(1.0)],
                    grid_rows: vec![GridTrack::Fr(1.0)],
                    padding: Edges::all(5.0),
                    margin: Edges::all(100.0),
                    ..LayoutStyle::default()
                },
            )
            .unwrap();
            let child = tree
                .insert(
                    root,
                    props(LayoutStyle {
                        margin: Edges {
                            left: 7.0,
                            right: 11.0,
                            top: 3.0,
                            bottom: 9.0,
                        },
                        ..sized(20.0, 10.0)
                    }),
                )
                .unwrap();
            let layout = compute(&tree, 100.0, 60.0);
            assert_rect(&layout, root, Rect::new(0.0, 0.0, 100.0, 60.0));
            assert_rect(&layout, child, Rect::new(12.0, 8.0, 20.0, 10.0));
        }
    }

    #[test]
    fn auto_overlay_and_flex_sizes_subtract_margins_and_negative_margin_can_overlap() {
        for kind in [LayoutKind::Overlay, LayoutKind::Flex(Axis::Horizontal)] {
            let mut tree = UiTree::new();
            let root = tree.root();
            tree.set_style(
                root,
                LayoutStyle {
                    kind,
                    ..LayoutStyle::default()
                },
            )
            .unwrap();
            let child = tree
                .insert(
                    root,
                    props(LayoutStyle {
                        margin: Edges::all(5.0),
                        flex_grow: 1.0,
                        ..LayoutStyle::default()
                    }),
                )
                .unwrap();
            assert_rect(
                &compute(&tree, 100.0, 60.0),
                child,
                Rect::new(5.0, 5.0, 90.0, 50.0),
            );
        }
        let mut tree = UiTree::new();
        let root = tree.root();
        tree.set_style(
            root,
            LayoutStyle {
                kind: LayoutKind::Stack(Axis::Horizontal),
                ..LayoutStyle::default()
            },
        )
        .unwrap();
        tree.insert(root, props(sized(20.0, 10.0))).unwrap();
        let overlap = tree
            .insert(
                root,
                props(LayoutStyle {
                    margin: Edges {
                        left: -5.0,
                        ..Edges::default()
                    },
                    ..sized(20.0, 10.0)
                }),
            )
            .unwrap();
        assert_rect(
            &compute(&tree, 100.0, 60.0),
            overlap,
            Rect::new(15.0, 0.0, 20.0, 10.0),
        );
    }

    #[test]
    fn oversized_implicit_grid_is_rejected_before_backend_placement() {
        let mut tree = UiTree::new();
        let root = tree.root();
        tree.set_style(
            root,
            LayoutStyle {
                kind: LayoutKind::Grid,
                ..LayoutStyle::default()
            },
        )
        .unwrap();
        for _ in 0..5 {
            tree.insert(
                root,
                props(LayoutStyle {
                    grid_column: GridPlacement {
                        start: None,
                        span: 1024,
                    },
                    ..LayoutStyle::default()
                }),
            )
            .unwrap();
        }
        assert!(matches!(
            TaffyLayout::new().compute(&tree, Size::new(100.0, 100.0)),
            Err(LayoutError::InvalidStyle { .. })
        ));
    }

    #[test]
    fn child_alignment_uses_physical_axes_inside_grid_cells_after_resize() {
        let mut tree = UiTree::new();
        let root = tree.root();
        tree.set_style(
            root,
            LayoutStyle {
                kind: LayoutKind::Grid,
                grid_columns: vec![GridTrack::Px(40.0), GridTrack::Fr(1.0)],
                grid_rows: vec![GridTrack::Fr(1.0)],
                ..Default::default()
            },
        )
        .unwrap();
        let child = tree
            .insert(
                root,
                props(LayoutStyle {
                    width: Length::Px(30.0),
                    height: Length::Px(20.0),
                    horizontal_alignment: Some(Align::Center),
                    vertical_alignment: Some(Align::End),
                    grid_column: GridPlacement {
                        start: Some(2),
                        span: 1,
                    },
                    ..Default::default()
                }),
            )
            .unwrap();
        assert_rect(
            &compute(&tree, 140.0, 100.0),
            child,
            Rect::new(75.0, 80.0, 30.0, 20.0),
        );
        assert_rect(
            &compute(&tree, 240.0, 200.0),
            child,
            Rect::new(125.0, 180.0, 30.0, 20.0),
        );
    }

    #[test]
    fn wrap_panel_reflows_without_changing_child_identity() {
        let mut tree = UiTree::new();
        let root = tree.root();
        tree.set_style(
            root,
            LayoutStyle {
                kind: LayoutKind::Wrap(Axis::Horizontal),
                gap: 5.0,
                ..Default::default()
            },
        )
        .unwrap();
        let children: Vec<_> = (0..3)
            .map(|_| {
                tree.insert(
                    root,
                    props(LayoutStyle {
                        width: Length::Px(60.0),
                        height: Length::Px(20.0),
                        flex_shrink: 0.0,
                        ..Default::default()
                    }),
                )
                .unwrap()
            })
            .collect();
        let narrow = compute(&tree, 130.0, 100.0);
        assert_rect(&narrow, children[0], Rect::new(0.0, 0.0, 60.0, 20.0));
        assert_rect(&narrow, children[1], Rect::new(65.0, 0.0, 60.0, 20.0));
        assert_rect(&narrow, children[2], Rect::new(0.0, 25.0, 60.0, 20.0));
        assert_rect(
            &compute(&tree, 200.0, 100.0),
            children[2],
            Rect::new(130.0, 0.0, 60.0, 20.0),
        );
    }
}

#[cfg(test)]
mod measure_tests {
    use super::*;
    use rust_desktop_ui_core::{Color, Edges, Mutation, NodeProps, Paint, UiRuntime};
    use std::{cell::Cell, rc::Rc};

    #[test]
    fn custom_intrinsic_measure_obeys_known_axes_constraints_padding_and_resize() {
        let mut tree = UiTree::new();
        let root = tree.root();
        tree.set_style(
            root,
            LayoutStyle {
                kind: LayoutKind::Grid,
                grid_columns: vec![GridTrack::Auto, GridTrack::Fr(1.0)],
                grid_rows: vec![GridTrack::Auto],
                ..Default::default()
            },
        )
        .unwrap();
        let leaf = tree
            .insert(
                root,
                NodeProps {
                    style: LayoutStyle {
                        kind: LayoutKind::Leaf,
                        padding: Edges::all(3.0),
                        min_size: Size::new(20.0, 0.0),
                        max_size: Size::new(100.0, f32::INFINITY),
                        ..Default::default()
                    },
                    ..Default::default()
                },
            )
            .unwrap();
        let calls = Rc::new(Cell::new(0));
        let seen = calls.clone();
        let mut layout = TaffyLayout::new();
        layout
            .set_measure(leaf, move |_| {
                seen.set(seen.get() + 1);
                Ok(Size::new(130.0, 24.0))
            })
            .unwrap();
        for width in [400.0, 240.0, 640.0] {
            let snapshot = layout.compute(&tree, Size::new(width, 200.0)).unwrap();
            assert_eq!(snapshot.boxes[&leaf].width, 100.0);
            assert_eq!(snapshot.boxes[&leaf].height, 30.0);
        }
        assert!(calls.get() > 0);
    }

    #[test]
    fn invalid_measure_keeps_last_scene_then_invalidation_remeasures_without_identity_loss() {
        let mut tree = UiTree::new();
        let root = tree.root();
        tree.set_style(
            root,
            LayoutStyle {
                kind: LayoutKind::Stack(Axis::Vertical),
                align: Align::Start,
                ..Default::default()
            },
        )
        .unwrap();
        let leaf = tree
            .insert(
                root,
                NodeProps {
                    style: LayoutStyle {
                        kind: LayoutKind::Leaf,
                        ..Default::default()
                    },
                    paint: Paint {
                        background: Some(Color::rgb(30, 40, 50)),
                        ..Default::default()
                    },
                    focusable: true,
                    ..Default::default()
                },
            )
            .unwrap();
        let size = Rc::new(Cell::new(Size::new(80.0, 24.0)));
        let observed = size.clone();
        let mut layout = MeasuredLayout::new(move |_, _| Ok(observed.get()));
        let mut runtime = UiRuntime::new(tree);
        runtime
            .update(&mut layout, Size::new(200.0, 100.0))
            .unwrap();
        runtime.apply(Mutation::Focus(Some(leaf))).unwrap();
        let old = runtime.bounds(leaf);
        size.set(Size::new(f32::NAN, 24.0));
        runtime.invalidate_measure(leaf).unwrap();
        assert!(
            runtime
                .update(&mut layout, Size::new(200.0, 100.0))
                .is_err()
        );
        assert_eq!(runtime.bounds(leaf), old);
        assert!(runtime.needs_update());
        size.set(Size::new(120.0, 42.0));
        runtime
            .update(&mut layout, Size::new(200.0, 100.0))
            .unwrap();
        assert_eq!(runtime.bounds(leaf).unwrap().width, 120.0);
        assert_eq!(runtime.focused(), Some(leaf));
        assert_eq!(
            runtime
                .update(&mut layout, Size::new(200.0, 100.0))
                .unwrap()
                .layout_passes,
            0
        );
    }

    #[test]
    fn hidden_and_collapsed_preserve_content_and_clipping_excludes_input() {
        let mut tree = UiTree::new();
        let root = tree.root();
        tree.set_style(
            root,
            LayoutStyle {
                kind: LayoutKind::Stack(Axis::Vertical),
                align: Align::Start,
                ..Default::default()
            },
        )
        .unwrap();
        let parent = tree
            .insert(
                root,
                NodeProps {
                    style: LayoutStyle {
                        kind: LayoutKind::Stack(Axis::Vertical),
                        width: Length::Px(60.0),
                        height: Length::Px(40.0),
                        ..Default::default()
                    },
                    clip: true,
                    ..Default::default()
                },
            )
            .unwrap();
        let child = tree
            .insert(
                parent,
                NodeProps {
                    style: LayoutStyle {
                        kind: LayoutKind::Leaf,
                        width: Length::Px(100.0),
                        height: Length::Px(30.0),
                        ..Default::default()
                    },
                    ..Default::default()
                },
            )
            .unwrap();
        let mut runtime = UiRuntime::new(tree);
        let mut layout = TaffyLayout::new();
        runtime
            .update(&mut layout, Size::new(200.0, 200.0))
            .unwrap();
        assert_eq!(runtime.clip(child).unwrap().width, 60.0);
        assert_ne!(
            runtime.hit_test(rust_desktop_ui_core::Point::new(80.0, 10.0)),
            Some(child)
        );
        let mut props = runtime.tree().node(parent).unwrap().props.clone();
        props.visible = false;
        runtime
            .apply(Mutation::SetProps {
                node: parent,
                props: props.clone(),
            })
            .unwrap();
        runtime
            .update(&mut layout, Size::new(200.0, 200.0))
            .unwrap();
        assert!(runtime.bounds(child).is_none());
        assert_eq!(runtime.layout_bounds(parent).unwrap().height, 40.0);
        props.collapsed = true;
        runtime
            .apply(Mutation::SetProps {
                node: parent,
                props: props.clone(),
            })
            .unwrap();
        runtime
            .update(&mut layout, Size::new(200.0, 200.0))
            .unwrap();
        assert_eq!(runtime.layout_bounds(parent).unwrap().height, 0.0);
        props.visible = true;
        props.collapsed = false;
        runtime
            .apply(Mutation::SetProps {
                node: parent,
                props,
            })
            .unwrap();
        runtime
            .update(&mut layout, Size::new(200.0, 200.0))
            .unwrap();
        assert_eq!(runtime.bounds(child).unwrap().width, 100.0);
        assert_eq!(runtime.tree().parent(child), Some(parent));
    }
}
