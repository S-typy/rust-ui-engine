//! Backend-independent container and placement definitions.
use crate::{ElementDefinition, SurfaceKindDefinition};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Alignment {
    Start,
    Center,
    End,
    #[default]
    Stretch,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Visibility {
    #[default]
    Visible,
    Hidden,
    Collapsed,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum DockSide {
    #[default]
    Left,
    Top,
    Right,
    Bottom,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum GridLength {
    #[default]
    Auto,
    Pixel(f32),
    Star(f32),
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct GridDefinition {
    pub rows: Vec<GridLength>,
    pub columns: Vec<GridLength>,
    pub spacing: f32,
    pub padding: [f32; 4],
    pub surface: SurfaceKindDefinition,
    pub children: Vec<ElementDefinition>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct BorderDefinition {
    pub padding: [f32; 4],
    pub background: Option<[u8; 4]>,
    pub border_brush: Option<[u8; 4]>,
    pub border_thickness: f32,
    pub corner_radius: f32,
    pub child: Box<ElementDefinition>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ScrollBarVisibility {
    #[default]
    Auto,
    Hidden,
    Disabled,
    Visible,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ScrollViewerDefinition {
    pub horizontal_scroll_bar_visibility: ScrollBarVisibility,
    pub vertical_scroll_bar_visibility: ScrollBarVisibility,
    pub child: Box<ElementDefinition>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DockPanelDefinition {
    pub last_child_fill: bool,
    pub padding: [f32; 4],
    pub children: Vec<ElementDefinition>,
}

impl Default for DockPanelDefinition {
    fn default() -> Self {
        Self {
            last_child_fill: true,
            padding: [0.0; 4],
            children: Vec::new(),
        }
    }
}
