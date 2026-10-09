//! Platform-independent layout, geometry and primitive appearance contracts.

use crate::{Color, Rect, UiTree, WidgetId};
use std::{collections::HashMap, fmt};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    pub const ZERO: Self = Self::new(0.0, 0.0);
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Size {
    pub width: f32,
    pub height: f32,
}

impl Size {
    pub const ZERO: Self = Self::new(0.0, 0.0);
    pub const fn new(width: f32, height: f32) -> Self {
        Self { width, height }
    }
    pub fn is_valid(self) -> bool {
        self.width.is_finite() && self.height.is_finite() && self.width >= 0.0 && self.height >= 0.0
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Edges {
    pub left: f32,
    pub right: f32,
    pub top: f32,
    pub bottom: f32,
}

impl Edges {
    pub const fn all(value: f32) -> Self {
        Self {
            left: value,
            right: value,
            top: value,
            bottom: value,
        }
    }
    pub fn horizontal(self) -> f32 {
        self.left + self.right
    }
    pub fn vertical(self) -> f32 {
        self.top + self.bottom
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Axis {
    Horizontal,
    #[default]
    Vertical,
}

/// Percentage values are fractions of the available parent dimension (1.0 = 100%).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Length {
    #[default]
    Auto,
    Px(f32),
    Percent(f32),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Align {
    Start,
    Center,
    End,
    #[default]
    Stretch,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Justify {
    #[default]
    Start,
    Center,
    End,
    SpaceBetween,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LayoutKind {
    #[default]
    Leaf,
    /// Single line with fixed child main-axis sizes; no grow/shrink distribution.
    Stack(Axis),
    /// Children share the padded area; offsets are parent-local logical pixels.
    /// The panel needs an explicit or parent-assigned size, not child intrinsic sizing.
    Overlay,
    Flex(Axis),
    /// Row-major auto placement with explicit tracks and optional child placement.
    Grid,
}

/// Grid track size; percentages are fractions (1.0 = 100%).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum GridTrack {
    #[default]
    Auto,
    Px(f32),
    Percent(f32),
    Fr(f32),
}

/// A one-based start line, or automatic placement, spanning at least one track.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GridPlacement {
    pub start: Option<u16>,
    pub span: u16,
}

impl Default for GridPlacement {
    fn default() -> Self {
        Self {
            start: None,
            span: 1,
        }
    }
}

/// Maximum explicit grid tracks, span and addressed track on either axis.
pub const MAX_GRID_TRACKS: usize = 1024;

/// Small owned style API; no layout-backend types cross this boundary.
#[derive(Clone, Debug, PartialEq)]
pub struct LayoutStyle {
    pub kind: LayoutKind,
    pub width: Length,
    pub height: Length,
    pub min_size: Size,
    /// Positive infinity means unbounded maximum for that axis.
    pub max_size: Size,
    pub padding: Edges,
    /// Finite logical-pixel outer spacing; negative margins are supported.
    pub margin: Edges,
    pub gap: f32,
    pub flex_grow: f32,
    pub flex_shrink: f32,
    pub align: Align,
    pub justify: Justify,
    pub grid_columns: Vec<GridTrack>,
    pub grid_rows: Vec<GridTrack>,
    pub grid_column: GridPlacement,
    pub grid_row: GridPlacement,
    /// Used for a child of Overlay; ignored by Stack/Flex/Grid containers.
    pub offset: Point,
}

impl Default for LayoutStyle {
    fn default() -> Self {
        Self {
            kind: LayoutKind::Leaf,
            width: Length::Auto,
            height: Length::Auto,
            min_size: Size::ZERO,
            max_size: Size::new(f32::INFINITY, f32::INFINITY),
            padding: Edges::default(),
            margin: Edges::default(),
            gap: 0.0,
            flex_grow: 0.0,
            flex_shrink: 1.0,
            align: Align::Stretch,
            justify: Justify::Start,
            grid_columns: Vec::new(),
            grid_rows: Vec::new(),
            grid_column: GridPlacement::default(),
            grid_row: GridPlacement::default(),
            offset: Point::ZERO,
        }
    }
}

impl LayoutStyle {
    pub fn validate(&self) -> Result<(), &'static str> {
        let nonnegative = |x: f32| x.is_finite() && x >= 0.0;
        for value in [self.width, self.height] {
            if let Length::Px(x) | Length::Percent(x) = value
                && !nonnegative(x)
            {
                return Err("Layout lengths must be finite and nonnegative");
            }
        }
        if !self.min_size.is_valid() {
            return Err("Minimum size must be finite and nonnegative");
        }
        for (min, max) in [
            (self.min_size.width, self.max_size.width),
            (self.min_size.height, self.max_size.height),
        ] {
            if max.is_nan() || max < min {
                return Err("Maximum size must be at least the minimum");
            }
        }
        for x in [
            self.padding.left,
            self.padding.right,
            self.padding.top,
            self.padding.bottom,
            self.padding.horizontal(),
            self.padding.vertical(),
            self.gap,
            self.flex_grow,
            self.flex_shrink,
        ] {
            if !nonnegative(x) {
                return Err("Padding, gap and flex factors must be finite and nonnegative");
            }
        }
        if !self.offset.is_finite() {
            return Err("Overlay offset must be finite");
        }
        if [
            self.margin.left,
            self.margin.right,
            self.margin.top,
            self.margin.bottom,
            self.margin.horizontal(),
            self.margin.vertical(),
        ]
        .into_iter()
        .any(|x| !x.is_finite())
        {
            return Err("Margins and their axis sums must be finite");
        }
        for tracks in [&self.grid_columns, &self.grid_rows] {
            if tracks.len() > MAX_GRID_TRACKS {
                return Err("Too many explicit grid tracks");
            }
            for track in tracks {
                if let GridTrack::Px(x) | GridTrack::Percent(x) | GridTrack::Fr(x) = *track
                    && !nonnegative(x)
                {
                    return Err("Grid track sizes must be finite and nonnegative");
                }
            }
        }
        for placement in [self.grid_column, self.grid_row] {
            let start = usize::from(placement.start.unwrap_or(1));
            let span = usize::from(placement.span);
            if start == 0 || span == 0 || start + span - 1 > MAX_GRID_TRACKS {
                return Err("Grid placement must address 1..=1024 with a positive span");
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Border {
    pub width: f32,
    pub color: Color,
}

/// Rectangle appearance, including optional primitive hover/focus feedback.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Paint {
    pub background: Option<Color>,
    pub border: Option<Border>,
    pub hover_background: Option<Color>,
    pub focus_border: Option<Border>,
}

impl Paint {
    pub fn validate(&self) -> Result<(), &'static str> {
        for color in [
            self.background,
            self.hover_background,
            self.border.map(|x| x.color),
            self.focus_border.map(|x| x.color),
        ]
        .into_iter()
        .flatten()
        {
            if [color.r, color.g, color.b, color.a]
                .into_iter()
                .any(|x| !x.is_finite() || !(0.0..=1.0).contains(&x))
            {
                return Err("Colors must contain finite normalized channels");
            }
        }
        for border in [self.border, self.focus_border].into_iter().flatten() {
            if !border.width.is_finite() || border.width < 0.0 {
                return Err("Border width must be finite and nonnegative");
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct NodeProps {
    pub style: LayoutStyle,
    pub paint: Paint,
    /// Hidden nodes retain their layout space; their subtree does not paint or receive input.
    pub visible: bool,
    /// Disabled ancestors exclude the whole subtree from hit testing and focus.
    pub enabled: bool,
    pub focusable: bool,
    pub hit_test: bool,
    pub z_index: i32,
    /// Clip children to this node's border box; the root always clips to the viewport.
    pub clip: bool,
    /// Applied to children after layout. The host determines scroll bounds.
    pub scroll: Point,
    /// Translation only; rotation, scaling and arbitrary transforms are not implemented.
    pub translation: Point,
}

impl Default for NodeProps {
    fn default() -> Self {
        Self {
            style: LayoutStyle::default(),
            paint: Paint::default(),
            visible: true,
            enabled: true,
            focusable: false,
            hit_test: true,
            z_index: 0,
            clip: false,
            scroll: Point::ZERO,
            translation: Point::ZERO,
        }
    }
}

impl NodeProps {
    pub fn validate(&self) -> Result<(), &'static str> {
        self.style.validate()?;
        self.paint.validate()?;
        if !self.scroll.is_finite() || self.scroll.x < 0.0 || self.scroll.y < 0.0 {
            return Err("Scroll offsets must be finite and nonnegative");
        }
        if !self.translation.is_finite() {
            return Err("Translation must be finite");
        }
        Ok(())
    }
}

/// Computed data is separate from the retained logical tree.
#[derive(Clone, Debug, Default)]
pub struct LayoutSnapshot {
    /// Parent-local border boxes. Every live node must have a finite nonnegative box.
    pub boxes: HashMap<WidgetId, Rect>,
    pub measured_nodes: usize,
    pub arranged_nodes: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub enum LayoutError {
    InvalidViewport,
    InvalidStyle {
        node: WidgetId,
        message: &'static str,
    },
    MissingNode(WidgetId),
    Backend(String),
}

impl fmt::Display for LayoutError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidViewport => f.write_str("Viewport must be finite and nonnegative"),
            Self::InvalidStyle { node, message } => {
                write!(f, "Invalid style for {node:?}: {message}")
            }
            Self::MissingNode(id) => write!(f, "Layout node is missing: {id:?}"),
            Self::Backend(message) => write!(f, "Layout failed: {message}"),
        }
    }
}
impl std::error::Error for LayoutError {}

pub trait LayoutEngine {
    /// Compute all border boxes. Root geometry is always (0, 0, viewport.width, viewport.height).
    fn compute(&mut self, tree: &UiTree, viewport: Size) -> Result<LayoutSnapshot, LayoutError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_grid_and_margin_styles_are_rejected_atomically() {
        let mut tree = UiTree::new();
        let root = tree.root();
        let original = tree.node(root).unwrap().props.style.clone();
        let invalid = [
            LayoutStyle {
                grid_columns: vec![GridTrack::Fr(f32::NAN)],
                ..LayoutStyle::default()
            },
            LayoutStyle {
                grid_rows: vec![GridTrack::Px(-1.0)],
                ..LayoutStyle::default()
            },
            LayoutStyle {
                grid_column: GridPlacement {
                    start: Some(0),
                    span: 1,
                },
                ..LayoutStyle::default()
            },
            LayoutStyle {
                grid_row: GridPlacement {
                    start: None,
                    span: 0,
                },
                ..LayoutStyle::default()
            },
            LayoutStyle {
                grid_row: GridPlacement {
                    start: Some(1024),
                    span: 2,
                },
                ..LayoutStyle::default()
            },
            LayoutStyle {
                grid_rows: vec![GridTrack::Auto; MAX_GRID_TRACKS + 1],
                ..LayoutStyle::default()
            },
            LayoutStyle {
                margin: Edges::all(f32::MAX),
                ..LayoutStyle::default()
            },
        ];
        for style in invalid {
            assert!(tree.set_style(root, style).is_err());
            assert_eq!(tree.node(root).unwrap().props.style, original);
        }
        assert!(
            LayoutStyle {
                margin: Edges::all(-3.0),
                ..LayoutStyle::default()
            }
            .validate()
            .is_ok()
        );
    }
}
