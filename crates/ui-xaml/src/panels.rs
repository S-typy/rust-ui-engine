//! Backend-independent container and placement definitions.
use crate::{ElementDefinition, IconSymbol, SurfaceKindDefinition};

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
    /// Caption interrupts the upper outline.
    Standard,
}

/// Header presentation independent of the surrounding group border.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum GroupHeaderStyleDefinition {
    #[default]
    Normal,
    Light,
    Accent,
}

/// A titled container. An empty group has no implicit content element.
#[derive(Clone, Debug, PartialEq)]
pub struct GroupBoxDefinition {
    pub header: String,
    pub appearance: GroupBoxAppearanceDefinition,
    pub header_style: GroupHeaderStyleDefinition,
    pub header_icon: Option<IconSymbol>,
    pub show_header: bool,
    pub corner_radius: Option<f32>,
    pub header_foreground: Option<[u8; 4]>,
    pub header_background: Option<[u8; 4]>,
    /// Left, top, right, bottom content padding in logical pixels.
    pub padding: [f32; 4],
    pub is_collapsible: bool,
    pub is_expanded: bool,
    pub child: Option<Box<ElementDefinition>>,
}

impl Default for GroupBoxDefinition {
    fn default() -> Self {
        Self {
            header: String::new(),
            appearance: GroupBoxAppearanceDefinition::Card,
            header_style: GroupHeaderStyleDefinition::Normal,
            header_icon: None,
            show_header: true,
            corner_radius: None,
            header_foreground: None,
            header_background: None,
            padding: [16.0; 4],
            is_collapsible: false,
            is_expanded: true,
            child: None,
        }
    }
}

/// A titled container whose content can be collapsed without discarding it.
#[derive(Clone, Debug, PartialEq)]
pub struct ExpanderDefinition {
    pub header: String,
    pub appearance: GroupBoxAppearanceDefinition,
    pub header_style: GroupHeaderStyleDefinition,
    pub header_icon: Option<IconSymbol>,
    pub show_header: bool,
    pub corner_radius: Option<f32>,
    pub header_foreground: Option<[u8; 4]>,
    pub header_background: Option<[u8; 4]>,
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
            header_style: GroupHeaderStyleDefinition::Normal,
            header_icon: None,
            show_header: true,
            corner_radius: None,
            header_foreground: None,
            header_background: None,
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
    validate_group_properties(&group.header, group.padding, group.corner_radius)?;
    if group.is_collapsible && !group.show_header {
        return Err("collapsible GroupBox requires ShowHeader=True".into());
    }
    if !group.is_collapsible && !group.is_expanded {
        return Err("GroupBox IsExpanded=False requires IsCollapsible=True".into());
    }
    Ok(())
}

/// Validate expander properties from a manually constructed definition.
///
/// Collapsed content participates in the same tree checks as expanded content.
pub fn validate_expander(expander: &ExpanderDefinition) -> Result<(), String> {
    validate_group_properties(&expander.header, expander.padding, expander.corner_radius)?;
    if !expander.show_header {
        return Err("Expander requires ShowHeader=True".into());
    }
    Ok(())
}

fn validate_group_properties(
    header: &str,
    padding: [f32; 4],
    corner_radius: Option<f32>,
) -> Result<(), String> {
    if header.len() > 4096 {
        return Err("group Header must contain at most 4096 UTF-8 bytes".into());
    }
    if padding
        .iter()
        .any(|value| !value.is_finite() || !(0.0..=crate::MAX_DIMENSION as f32).contains(value))
    {
        return Err("group Padding must be finite and in 0..16384".into());
    }
    if corner_radius.is_some_and(|value| {
        !value.is_finite() || !(0.0..=crate::MAX_DIMENSION as f32).contains(&value)
    }) {
        return Err("group CornerRadius must be finite and in 0..16384".into());
    }
    Ok(())
}

/// A retained group whose pages are selected by a zero-based index.
#[derive(Clone, Debug, PartialEq)]
pub struct TabbedGroupDefinition {
    pub selected_index: usize,
    pub padding: [f32; 4],
    pub tabs: Vec<GroupTabDefinition>,
}

impl Default for TabbedGroupDefinition {
    fn default() -> Self {
        Self {
            selected_index: 0,
            padding: [16.0; 4],
            tabs: Vec::new(),
        }
    }
}

/// Static page metadata; the optional child owns ordinary element properties.
#[derive(Clone, Debug, PartialEq)]
pub struct GroupTabDefinition {
    pub header: String,
    pub header_icon: Option<IconSymbol>,
    pub is_enabled: bool,
    pub child: Option<Box<ElementDefinition>>,
}

impl Default for GroupTabDefinition {
    fn default() -> Self {
        Self {
            header: String::new(),
            header_icon: None,
            is_enabled: true,
            child: None,
        }
    }
}

/// Validate page metadata and initial selection without mounting controls.
pub fn validate_tabbed_group(group: &TabbedGroupDefinition) -> Result<(), String> {
    validate_group_properties("", group.padding, None)?;
    if !(1..=64).contains(&group.tabs.len()) {
        return Err("TabbedGroup requires 1..64 GroupTab pages".into());
    }
    if !group
        .tabs
        .get(group.selected_index)
        .is_some_and(|tab| tab.is_enabled)
    {
        return Err("TabbedGroup SelectedIndex must reference an enabled page".into());
    }
    for tab in &group.tabs {
        if tab.header.trim().is_empty() || tab.header.len() > 4096 {
            return Err("GroupTab Header must be nonempty and at most 4096 UTF-8 bytes".into());
        }
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
