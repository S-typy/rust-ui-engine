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

/// Theme-relative presentation of a titled content group.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum GroupBoxAppearanceDefinition {
    /// Filled surface with a header separator.
    #[default]
    Card,
    /// Transparent content with an outline.
    Outline,
    /// Header and content without a surrounding surface.
    Plain,
}

/// A static titled container. An empty group has no implicit content element.
#[derive(Clone, Debug, PartialEq)]
pub struct GroupBoxDefinition {
    pub header: String,
    pub appearance: GroupBoxAppearanceDefinition,
    /// Left, top, right, bottom content padding in logical pixels.
    pub padding: [f32; 4],
    pub child: Option<Box<ElementDefinition>>,
}

impl Default for GroupBoxDefinition {
    fn default() -> Self {
        Self {
            header: String::new(),
            appearance: GroupBoxAppearanceDefinition::Card,
            padding: [16.0; 4],
            child: None,
        }
    }
}

/// A titled container whose content can be collapsed without discarding it.
#[derive(Clone, Debug, PartialEq)]
pub struct ExpanderDefinition {
    pub header: String,
    pub appearance: GroupBoxAppearanceDefinition,
    /// Left, top, right, bottom content padding in logical pixels.
    pub padding: [f32; 4],
    pub is_expanded: bool,
    pub child: Option<Box<ElementDefinition>>,
}

impl Default for ExpanderDefinition {
    fn default() -> Self {
        Self {
            header: String::new(),
            appearance: GroupBoxAppearanceDefinition::Card,
            padding: [16.0; 4],
            is_expanded: true,
            child: None,
        }
    }
}

/// Validate group properties from a manually constructed definition.
///
/// The child tree is validated by the host and [`crate::validate_window_docking`].
pub fn validate_group_box(group: &GroupBoxDefinition) -> Result<(), String> {
    validate_group_properties(&group.header, group.padding)
}

/// Validate expander properties from a manually constructed definition.
///
/// Collapsed content participates in the same tree checks as expanded content.
pub fn validate_expander(expander: &ExpanderDefinition) -> Result<(), String> {
    validate_group_properties(&expander.header, expander.padding)
}

fn validate_group_properties(header: &str, padding: [f32; 4]) -> Result<(), String> {
    if header.len() > 4096 {
        return Err("group Header must contain at most 4096 UTF-8 bytes".into());
    }
    if padding
        .iter()
        .any(|value| !value.is_finite() || !(0.0..=crate::MAX_DIMENSION as f32).contains(value))
    {
        return Err("group Padding must be finite and in 0..16384".into());
    }
    Ok(())
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
