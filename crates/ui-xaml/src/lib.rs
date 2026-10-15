//! A small, explicit XAML profile compiled to a Rust window definition.
//!
//! This profile supports a `Window` containing a tree of stack panels, text,
//! editors, buttons, check boxes, radio buttons, icons, separators and static
//! dropdown/split menus and a command-oriented Ribbon. Elements use
//! [`UI_NAMESPACE`]; `x:Name` uses [`XAML_NAMESPACE`]. It does not load WPF types,
//! execute arbitrary markup extensions or resolve external entities. Explicit
//! property bindings are compiled into renderer-independent definitions.
//! Unknown syntax is an error instead of being silently ignored.
//!
//! ```
//! let window = rust_desktop_ui_xaml::parse(
//!     r#"<Window xmlns="urn:rust-ui-engine:ui" Title="Example">
//!          <TextBox Text="Hello" PlaceholderText="Enter text" />
//!        </Window>"#,
//! ).unwrap();
//! let rust_desktop_ui_xaml::ElementKind::TextBox(editor) = window.content.kind else {
//!     panic!("expected a text editor");
//! };
//! assert_eq!(editor.text, "Hello");
//! ```

use std::{
    collections::{BTreeMap, HashSet},
    fmt, fs,
    io::Read,
    path::Path,
};

use roxmltree::{Attribute, Document, Node, ParsingOptions};
use unicode_segmentation::UnicodeSegmentation;

mod binding;
mod codegen;
mod form_parse;
mod forms;
mod panels;
pub use binding::{BindingDefinition, BindingMode, UpdateSourceTrigger};
pub use forms::*;
pub use panels::*;
mod docking;
mod ribbon;

pub use docking::{
    DockNodeDefinition, DockPaneDefinition, DockWorkspaceDefinition, MAX_DOCK_PANES,
    validate_docking, validate_window_docking,
};

pub use ribbon::{
    RibbonDefinition, RibbonGroupDefinition, RibbonItemDefinition, RibbonItemKindDefinition,
    RibbonItemSize, RibbonMenuEntryDefinition, RibbonTabDefinition, validate_ribbon,
};

/// Namespace for this library's controls; not the WPF presentation namespace.
pub const UI_NAMESPACE: &str = "urn:rust-ui-engine:ui";
/// XAML language namespace. Only the `Name` directive is supported.
pub const XAML_NAMESPACE: &str = "http://schemas.microsoft.com/winfx/2006/xaml";
/// Maximum UTF-8 source length accepted by the compiler, including comments.
pub const MAX_SOURCE_BYTES: usize = 1_048_576;
/// Largest supported size or spacing value, in logical pixels.
pub const MAX_DIMENSION: f64 = 16_384.0;
/// Largest supported control font size, in logical pixels.
pub const MAX_FONT_SIZE: f64 = 512.0;
/// Largest `MaxLength` value accepted by this profile, in extended graphemes.
pub const MAX_TEXT_LENGTH: usize = 1_048_576;
/// Maximum UTF-8 length of an editor's validation message.
pub const MAX_VALIDATION_MESSAGE_BYTES: usize = 4_096;
/// Maximum content depth, counting the Window's direct child as depth one.
pub const MAX_ELEMENT_DEPTH: usize = 32;
/// Maximum number of content elements; the Window itself is not counted.
pub const MAX_ELEMENTS: usize = 256;
/// Maximum XML nodes, including comments and whitespace text nodes.
pub const MAX_XML_NODES: u32 = 4_096;
/// Maximum nested menu entry depth; top-level entries have depth one.
pub const MAX_MENU_DEPTH: usize = 8;
/// Maximum aggregate UTF-8 menu labels and command identifiers per button.
pub const MAX_MENU_TEXT_BYTES: usize = 1_048_576;
/// Maximum UTF-8 text for a plain tooltip.
pub const MAX_TOOLTIP_BYTES: usize = 4_096;

/// Initial form appearance, independent of the platform and control renderer.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Theme {
    #[default]
    Light,
    Dark,
}

/// Placement of the native window title bar.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum TitleBar {
    /// A platform-provided title bar above the form's content.
    #[default]
    Native,
    /// The title and native window controls share the Ribbon's header row.
    ///
    /// Requires a flush, automatically sized Ribbon at the content origin;
    /// see [`validate_window_chrome`]. Platform support is supplied by the host.
    Ribbon,
}

/// Validated startup properties for a native form.
#[derive(Clone, Debug, PartialEq)]
pub struct WindowDefinition {
    pub title: String,
    pub width: f64,
    pub height: f64,
    /// Left, top, right, bottom padding in logical pixels.
    pub padding: [f32; 4],
    pub theme: Theme,
    pub title_bar: TitleBar,
    pub content: ElementDefinition,
}

impl Default for WindowDefinition {
    fn default() -> Self {
        Self {
            title: "Rust UI Form".into(),
            width: 640.0,
            height: 360.0,
            padding: [32.0; 4],
            theme: Theme::Light,
            title_bar: TitleBar::Native,
            content: ElementDefinition::default(),
        }
    }
}

/// Validate the content placement required by a Ribbon-integrated title bar.
///
/// [`TitleBar::Ribbon`] requires zero Window padding. Its content must be a
/// Ribbon or a vertical StackPanel whose first child is the sole Ribbon. Both
/// the Ribbon and its optional parent must have zero margin and automatic
/// width/height; the parent also requires zero padding. Its spacing and surface
/// remain configurable, and later children may use ordinary layout properties.
///
/// Native title bars add no placement restrictions. This function does not
/// validate other element properties; hosts accepting manually constructed
/// definitions must also perform their normal content and command validation.
pub fn validate_window_chrome(window: &WindowDefinition) -> Result<(), String> {
    if window.title_bar == TitleBar::Native {
        return Ok(());
    }
    if window.title.len() > 4096 {
        return Err("Window TitleBar=Ribbon requires Title of at most 4096 UTF-8 bytes".into());
    }
    if window.padding != [0.0; 4] {
        return Err("Window TitleBar=Ribbon requires Padding=0".into());
    }
    let root = &window.content;
    let ribbon = match &root.kind {
        ElementKind::Ribbon(_) => root,
        ElementKind::StackPanel(panel) => {
            if panel.orientation != Orientation::Vertical {
                return Err("Window TitleBar=Ribbon requires a vertical root StackPanel".into());
            }
            validate_chrome_placement(&root.common, "root StackPanel")?;
            if panel.padding != [0.0; 4] {
                return Err("Window TitleBar=Ribbon requires root StackPanel Padding=0".into());
            }
            panel
                .children
                .first()
                .filter(|child| matches!(child.kind, ElementKind::Ribbon(_)))
                .ok_or("Window TitleBar=Ribbon requires Ribbon as the first StackPanel child")?
        }
        _ => {
            return Err(
                "Window TitleBar=Ribbon requires a Ribbon or a vertical root StackPanel".into(),
            );
        }
    };
    validate_chrome_placement(&ribbon.common, "Ribbon")?;
    let mut elements = vec![root];
    let mut ribbon_count = 0;
    while let Some(element) = elements.pop() {
        match &element.kind {
            ElementKind::Ribbon(_) => {
                ribbon_count += 1;
                if ribbon_count > 1 {
                    return Err("Window TitleBar=Ribbon requires exactly one Ribbon".into());
                }
            }
            ElementKind::DockWorkspace(workspace) => {
                validate_docking(workspace)?;
                elements.extend(
                    workspace
                        .panes()
                        .into_iter()
                        .map(|pane| pane.content.as_ref()),
                );
            }
            _ => elements.extend(element.children()),
        }
    }
    Ok(())
}

fn validate_chrome_placement(common: &CommonProperties, element: &str) -> Result<(), String> {
    if common.margin != [0.0; 4]
        || common.width.is_some()
        || common.height.is_some()
        || common.min_width != 0.0
        || common.min_height != 0.0
        || common.max_width.is_some()
        || common.max_height.is_some()
        || common.horizontal_alignment != Alignment::Stretch
        || common.vertical_alignment != Alignment::Stretch
        || common.visibility != Visibility::Visible
        || common.bindings.keys().any(|property| {
            matches!(
                property.as_str(),
                "Width"
                    | "Height"
                    | "MinWidth"
                    | "MinHeight"
                    | "MaxWidth"
                    | "MaxHeight"
                    | "Visibility"
            )
        })
    {
        return Err(format!(
            "Window TitleBar=Ribbon requires {element} Margin=0 and automatic Width/Height, unconstrained stretch alignment and no bound geometry"
        ));
    }
    Ok(())
}

/// Properties shared by every content element in a Window.
#[derive(Clone, Debug, PartialEq)]
pub struct CommonProperties {
    /// Unique within the Window, supplied by `x:Name`.
    pub name: Option<String>,
    /// `None` uses automatic sizing defined by the container and form adapter.
    pub width: Option<f32>,
    /// `None` uses the control's default height, or auto height for a panel.
    pub height: Option<f32>,
    /// Left, top, right, bottom margin in logical pixels.
    pub margin: [f32; 4],
    pub is_enabled: bool,
    pub visibility: Visibility,
    /// Optional accessible name, supplied by `AutomationName`.
    pub automation_name: Option<String>,
    /// Plain text shown by the control's tooltip, supplied by `ToolTip`.
    pub tool_tip: Option<String>,
    pub min_width: f32,
    pub min_height: f32,
    pub max_width: Option<f32>,
    pub max_height: Option<f32>,
    pub horizontal_alignment: Alignment,
    pub vertical_alignment: Alignment,
    /// Zero-based Grid cell, with positive spans.
    pub grid_row: u16,
    pub grid_column: u16,
    pub grid_row_span: u16,
    pub grid_column_span: u16,
    pub dock: DockSide,
    /// Property path relative to the inherited context.
    pub data_context: Option<String>,
    pub bindings: BTreeMap<String, BindingDefinition>,
}

impl Default for CommonProperties {
    fn default() -> Self {
        Self {
            name: None,
            width: None,
            height: None,
            margin: [0.0; 4],
            is_enabled: true,
            visibility: Visibility::Visible,
            automation_name: None,
            tool_tip: None,
            min_width: 0.0,
            min_height: 0.0,
            max_width: None,
            max_height: None,
            horizontal_alignment: Alignment::Stretch,
            vertical_alignment: Alignment::Stretch,
            grid_row: 0,
            grid_column: 0,
            grid_row_span: 1,
            grid_column_span: 1,
            dock: DockSide::Left,
            data_context: None,
            bindings: BTreeMap::new(),
        }
    }
}

/// A validated element and its properties in the form's content tree.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ElementDefinition {
    pub common: CommonProperties,
    pub kind: ElementKind,
}

impl ElementDefinition {
    /// Content children in source order, including the contents of docking panes.
    pub fn children(&self) -> Vec<&ElementDefinition> {
        match &self.kind {
            ElementKind::StackPanel(p) | ElementKind::WrapPanel(p) => p.children.iter().collect(),
            ElementKind::Grid(p) => p.children.iter().collect(),
            ElementKind::DockPanel(p) => p.children.iter().collect(),
            ElementKind::Border(p) => vec![&p.child],
            ElementKind::ScrollViewer(p) => vec![&p.child],
            ElementKind::DockWorkspace(p) => p
                .panes()
                .into_iter()
                .map(|pane| pane.content.as_ref())
                .collect(),
            _ => Vec::new(),
        }
    }

    /// Mutable content children in the same order as [`Self::children`].
    pub fn children_mut(&mut self) -> Vec<&mut ElementDefinition> {
        fn panes<'a>(
            node: &'a mut DockNodeDefinition,
            output: &mut Vec<&'a mut ElementDefinition>,
        ) {
            match node {
                DockNodeDefinition::Split { first, second, .. } => {
                    panes(first, output);
                    panes(second, output);
                }
                DockNodeDefinition::Group { panes, .. } => {
                    output.extend(panes.iter_mut().map(|pane| pane.content.as_mut()))
                }
            }
        }
        match &mut self.kind {
            ElementKind::StackPanel(p) | ElementKind::WrapPanel(p) => {
                p.children.iter_mut().collect()
            }
            ElementKind::Grid(p) => p.children.iter_mut().collect(),
            ElementKind::DockPanel(p) => p.children.iter_mut().collect(),
            ElementKind::Border(p) => vec![&mut p.child],
            ElementKind::ScrollViewer(p) => vec![&mut p.child],
            ElementKind::DockWorkspace(p) => {
                let mut output = Vec::new();
                panes(&mut p.root, &mut output);
                output
            }
            _ => Vec::new(),
        }
    }
}

/// Supported layout elements. Menu buttons own static menu entries separately.
#[derive(Clone, Debug, PartialEq)]
pub enum ElementKind {
    StackPanel(StackPanelDefinition),
    WrapPanel(StackPanelDefinition),
    Grid(GridDefinition),
    Border(BorderDefinition),
    ScrollViewer(ScrollViewerDefinition),
    DockPanel(DockPanelDefinition),
    ListBox(SelectionDefinition),
    ComboBox(SelectionDefinition),
    LookupEdit(LookupDefinition),
    ButtonEdit(ButtonEditDefinition),
    SpinBox(SpinBoxDefinition),
    DateEdit(TemporalEditDefinition),
    TimeEdit(TemporalEditDefinition),
    DockWorkspace(DockWorkspaceDefinition),
    Ribbon(RibbonDefinition),
    TextBox(EditBoxDefinition),
    TextBlock(TextBlockDefinition),
    Button(ButtonDefinition),
    ToggleButton(ToggleButtonDefinition),
    DropDownButton(MenuButtonDefinition),
    SplitButton(MenuButtonDefinition),
    CheckBox(CheckBoxDefinition),
    RadioButton(RadioButtonDefinition),
    Icon(IconDefinition),
    Separator(SeparatorDefinition),
}

impl Default for ElementKind {
    fn default() -> Self {
        Self::TextBox(EditBoxDefinition::default())
    }
}

/// Main axis of a stack panel.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Orientation {
    #[default]
    Vertical,
    Horizontal,
}

/// Theme-relative presentation of a stack panel's background.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SurfaceKindDefinition {
    #[default]
    Transparent,
    Card,
}

/// Sequential layout with spacing and content padding.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StackPanelDefinition {
    pub orientation: Orientation,
    pub spacing: f32,
    /// Left, top, right, bottom padding in logical pixels.
    pub padding: [f32; 4],
    pub surface: SurfaceKindDefinition,
    pub children: Vec<ElementDefinition>,
}

/// Theme-relative text color when no explicit foreground is supplied.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum TextToneDefinition {
    #[default]
    Primary,
    Secondary,
}

/// Non-editable text content.
#[derive(Clone, Debug, PartialEq)]
pub struct TextBlockDefinition {
    pub text: String,
    /// `None` uses the current control theme's font size.
    pub font_size: Option<f32>,
    /// OpenType weight in 1..=1000; `None` uses the renderer's default.
    pub font_weight: Option<u16>,
    /// Optional straight-alpha sRGB color in red/green/blue/alpha byte order.
    /// An explicit foreground takes precedence over the text tone.
    pub foreground: Option<[u8; 4]>,
    pub tone: TextToneDefinition,
    pub wrap: bool,
}

impl Default for TextBlockDefinition {
    fn default() -> Self {
        Self {
            text: String::new(),
            font_size: None,
            font_weight: None,
            foreground: None,
            tone: TextToneDefinition::Primary,
            wrap: true,
        }
    }
}

/// Validate typography before mounting a manually constructed text block.
pub fn validate_text_block(text: &TextBlockDefinition) -> Result<(), String> {
    if text
        .font_size
        .is_some_and(|size| !size.is_finite() || size <= 0.0 || size > MAX_FONT_SIZE as f32)
    {
        return Err("TextBlock FontSize must be finite and in (0, 512]".into());
    }
    if text
        .font_weight
        .is_some_and(|weight| !(1..=1000).contains(&weight))
    {
        return Err("TextBlock FontWeight must be in [1, 1000]".into());
    }
    Ok(())
}

/// A button with literal content and an optional application command name.
#[derive(Clone, Debug, PartialEq)]
pub struct ButtonDefinition {
    pub content: String,
    pub command: Option<String>,
    pub style: ButtonStyleDefinition,
    pub icon: Option<IconSymbol>,
    pub icon_size: f32,
    pub icon_spacing: f32,
}

impl Default for ButtonDefinition {
    fn default() -> Self {
        Self {
            content: String::new(),
            command: None,
            style: ButtonStyleDefinition::default(),
            icon: None,
            icon_size: 16.0,
            icon_spacing: 8.0,
        }
    }
}

/// A styled button with a static menu. SplitButton requires a primary command;
/// DropDownButton forbids one. Menu children are not layout children.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MenuButtonDefinition {
    pub button: ButtonDefinition,
    pub items: Vec<MenuEntryDefinition>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum MenuEntryDefinition {
    Item(MenuItemDefinition),
    Separator,
}

/// A leaf has a command; a submenu instead has nonempty items.
#[derive(Clone, Debug, PartialEq)]
pub struct MenuItemDefinition {
    pub header: String,
    pub command: Option<String>,
    pub is_enabled: bool,
    pub items: Vec<MenuEntryDefinition>,
}

/// A two-state button. A bound command's registered checked state takes
/// precedence over `is_checked` when the example mounts the definition.
#[derive(Clone, Debug, PartialEq)]
pub struct ToggleButtonDefinition {
    pub content: String,
    pub command: Option<String>,
    pub is_checked: bool,
    pub style: ButtonStyleDefinition,
    pub icon: Option<IconSymbol>,
    pub icon_size: f32,
    pub icon_spacing: f32,
}

impl Default for ToggleButtonDefinition {
    fn default() -> Self {
        Self {
            content: String::new(),
            command: None,
            is_checked: false,
            style: ButtonStyleDefinition::default(),
            icon: None,
            icon_size: 16.0,
            icon_spacing: 8.0,
        }
    }
}

/// Built-in vector symbols; these do not refer to fonts, files or resources.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum IconSymbol {
    #[default]
    Check,
    Close,
    Add,
    Remove,
    ChevronDown,
    ChevronRight,
    Save,
    Pin,
    FolderOpen,
    Copy,
    Edit,
    Eraser,
    Sun,
    Moon,
    Lock,
}

/// A standalone symbol; the XAML `Symbol` attribute is required.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct IconDefinition {
    pub symbol: IconSymbol,
    pub foreground: Option<[u8; 4]>,
}

/// A noninteractive rule with a positive thickness, in logical pixels.
#[derive(Clone, Debug, PartialEq)]
pub struct SeparatorDefinition {
    pub orientation: Orientation,
    pub thickness: f32,
    pub foreground: Option<[u8; 4]>,
}

impl Default for SeparatorDefinition {
    fn default() -> Self {
        Self {
            orientation: Orientation::Horizontal,
            thickness: 1.0,
            foreground: None,
        }
    }
}

/// A two-state check box with literal content.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CheckBoxDefinition {
    pub content: String,
    pub is_checked: bool,
    pub style: ButtonStyleDefinition,
}

/// A mutually exclusive choice. Named groups belong to one mounted view;
/// unnamed groups contain only radio buttons with the same immediate parent.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RadioButtonDefinition {
    pub content: String,
    pub group_name: Option<String>,
    pub is_checked: bool,
    pub style: ButtonStyleDefinition,
}

#[derive(Eq, Hash, PartialEq)]
enum RadioGroupKey {
    Named(String),
    Parent(usize),
}

/// Optional metrics and normal-state brushes shared by button-like controls.
/// `None` keeps the live theme fallback; colors are red, green, blue, alpha.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ButtonStyleDefinition {
    pub font_size: Option<f32>,
    pub padding: Option<[f32; 4]>,
    pub corner_radius: Option<f32>,
    pub border_thickness: Option<f32>,
    pub background: Option<[u8; 4]>,
    pub foreground: Option<[u8; 4]>,
    pub border_brush: Option<[u8; 4]>,
    pub focus_brush: Option<[u8; 4]>,
}

/// Startup properties for a single-line text editor.
#[derive(Clone, Debug, PartialEq)]
pub struct EditBoxDefinition {
    pub text: String,
    pub placeholder: String,
    /// Allows focus, selection and copy while preventing user text mutations.
    pub is_read_only: bool,
    /// Maximum extended graphemes. `None` means no grapheme limit (`MaxLength="0"`).
    pub max_length: Option<usize>,
    /// Require a non-whitespace value when the draft is accepted.
    pub is_required: bool,
    /// Message shown after a failed required-value check.
    pub validation_message: String,
    /// Attempt to accept the draft when keyboard/window focus is lost.
    pub commit_on_lost_focus: bool,
    /// `None` uses the current control theme's metric.
    pub font_size: Option<f32>,
    /// Optional left, top, right, bottom content padding in logical pixels.
    pub padding: Option<[f32; 4]>,
    /// Uniform corner radius in logical pixels, clamped to the rendered bounds.
    pub corner_radius: Option<f32>,
    /// Uniform border thickness in logical pixels.
    pub border_thickness: Option<f32>,
    /// Optional straight-alpha sRGB colors, in red/green/blue/alpha byte order.
    pub background: Option<[u8; 4]>,
    pub foreground: Option<[u8; 4]>,
    pub border_brush: Option<[u8; 4]>,
    pub focus_brush: Option<[u8; 4]>,
    pub placeholder_foreground: Option<[u8; 4]>,
    pub selection_brush: Option<[u8; 4]>,
    /// Validation border and focus-accent color; `None` follows the theme.
    pub validation_brush: Option<[u8; 4]>,
}

impl Default for EditBoxDefinition {
    fn default() -> Self {
        Self {
            text: String::new(),
            placeholder: String::new(),
            is_read_only: false,
            max_length: None,
            is_required: false,
            validation_message: "A value is required.".into(),
            commit_on_lost_focus: true,
            font_size: None,
            padding: None,
            corner_radius: None,
            border_thickness: None,
            background: None,
            foreground: None,
            border_brush: None,
            focus_brush: None,
            placeholder_foreground: None,
            selection_brush: None,
            validation_brush: None,
        }
    }
}

/// A source diagnostic with one-based line and column numbers.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Diagnostic {
    pub line: u32,
    pub column: u32,
    pub message: String,
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}:{}: {}", self.line, self.column, self.message)
    }
}

impl std::error::Error for Diagnostic {}

/// Parse and validate the supported form profile.
///
/// Dimensions must be finite and in `(0, MAX_DIMENSION]`; spacing must be in
/// `[0, MAX_DIMENSION]`. Padding and margin accept one value, horizontal/vertical
/// values, or left/top/right/bottom values separated by commas. Names match
/// `[A-Za-z_][A-Za-z0-9_]*`. Namespace aliases may be chosen freely.
pub fn parse(source: &str) -> Result<WindowDefinition, Diagnostic> {
    if source.len() > MAX_SOURCE_BYTES {
        return Err(Diagnostic {
            line: 1,
            column: 1,
            message: format!("source exceeds the {MAX_SOURCE_BYTES}-byte limit"),
        });
    }
    let document = Document::parse_with_options(
        source,
        ParsingOptions {
            allow_dtd: false,
            nodes_limit: MAX_XML_NODES,
            ..ParsingOptions::default()
        },
    )
    .map_err(|error| Diagnostic {
        line: error.pos().row,
        column: error.pos().col,
        message: format!("invalid XML: {error}"),
    })?;
    if let Some(node) = document.descendants().find(Node::is_pi) {
        return Err(at_node(node, "processing instructions are not supported"));
    }
    let window = document.root_element();
    require_element(window, "Window")?;
    let mut definition = WindowDefinition::default();
    for attribute in window.attributes() {
        reject_extension(&document, attribute)?;
        if attribute.namespace().is_some() {
            return Err(at_attribute(
                &document,
                attribute,
                "Window does not support namespaced properties or directives",
            ));
        }
        match attribute.name() {
            "Title" => definition.title = attribute.value().into(),
            "Width" => definition.width = dimension(&document, attribute)?,
            "Height" => definition.height = dimension(&document, attribute)?,
            "Padding" => definition.padding = thickness(&document, attribute)?,
            "TitleBar" => {
                definition.title_bar = match attribute.value() {
                    "Native" => TitleBar::Native,
                    "Ribbon" => TitleBar::Ribbon,
                    _ => {
                        return Err(at_attribute(
                            &document,
                            attribute,
                            "TitleBar must be Native or Ribbon",
                        ));
                    }
                };
            }
            "Theme" => {
                definition.theme = match attribute.value() {
                    "Light" => Theme::Light,
                    "Dark" => Theme::Dark,
                    _ => {
                        return Err(at_attribute(
                            &document,
                            attribute,
                            "Theme must be Light or Dark",
                        ));
                    }
                };
            }
            property => {
                return Err(at_attribute(
                    &document,
                    attribute,
                    format!("unsupported Window property '{property}'"),
                ));
            }
        }
    }
    let mut content = None;
    let mut names = HashSet::new();
    let mut checked_radio_groups = HashSet::new();
    let mut count = 0;
    for child in window.children() {
        if child.is_element() {
            if content.is_some() {
                return Err(at_node(
                    child,
                    "Window must contain exactly one root element",
                ));
            }
            content = Some(parse_element(
                child,
                1,
                0,
                &mut count,
                &mut names,
                &mut checked_radio_groups,
            )?);
        } else {
            require_empty_content(child)?;
        }
    }
    definition.content =
        content.ok_or_else(|| at_node(window, "Window must contain exactly one root element"))?;
    if ribbon::count(&definition.content) > 1 {
        return Err(at_node(window, "Window supports at most one Ribbon"));
    }
    validate_window_docking(&definition).map_err(|message| at_node(window, message))?;
    validate_window_chrome(&definition).map_err(|message| at_node(window, message))?;
    Ok(definition)
}

fn parse_element(
    node: Node<'_, '_>,
    depth: usize,
    parent: usize,
    count: &mut usize,
    names: &mut HashSet<String>,
    checked_radio_groups: &mut HashSet<RadioGroupKey>,
) -> Result<ElementDefinition, Diagnostic> {
    if depth > MAX_ELEMENT_DEPTH {
        return Err(at_node(
            node,
            format!("content exceeds the maximum element depth of {MAX_ELEMENT_DEPTH}"),
        ));
    }
    *count += 1;
    let element_id = *count;
    if *count > MAX_ELEMENTS {
        return Err(at_node(
            node,
            format!("content exceeds the {MAX_ELEMENTS}-element limit"),
        ));
    }
    if node.tag_name().namespace() != Some(UI_NAMESPACE) {
        return Err(at_node(
            node,
            format!("expected a content element in namespace '{UI_NAMESPACE}'"),
        ));
    }
    let element_name = node.tag_name().name();
    let mut kind = match element_name {
        "StackPanel" => ElementKind::StackPanel(StackPanelDefinition::default()),
        "DockWorkspace" => ElementKind::DockWorkspace(DockWorkspaceDefinition::default()),
        "Ribbon" => ElementKind::Ribbon(RibbonDefinition::default()),
        "TextBox" => ElementKind::TextBox(EditBoxDefinition::default()),
        "TextBlock" => ElementKind::TextBlock(TextBlockDefinition::default()),
        "Button" => ElementKind::Button(ButtonDefinition::default()),
        "ToggleButton" => ElementKind::ToggleButton(ToggleButtonDefinition::default()),
        "DropDownButton" => ElementKind::DropDownButton(MenuButtonDefinition::default()),
        "SplitButton" => ElementKind::SplitButton(MenuButtonDefinition::default()),
        "CheckBox" => ElementKind::CheckBox(CheckBoxDefinition::default()),
        "RadioButton" => ElementKind::RadioButton(RadioButtonDefinition::default()),
        "Icon" => ElementKind::Icon(IconDefinition::default()),
        "Separator" => ElementKind::Separator(SeparatorDefinition::default()),
        name if form_parse::new_kind(name).is_some() => form_parse::new_kind(name).unwrap(),
        name => {
            return Err(at_node(
                node,
                format!("unsupported content element '{name}'"),
            ));
        }
    };
    let document = node.document();
    let mut common = CommonProperties::default();
    for attribute in node.attributes() {
        if attribute.namespace().is_none() && attribute.name() == "DataContext" {
            form_parse::common_property(document, attribute, &mut common)?;
            continue;
        }
        if attribute.namespace().is_none() && attribute.value().trim_start().starts_with("{Binding")
        {
            let property = attribute.name();
            if !form_parse::is_binding_property(&kind, property) {
                return Err(at_attribute(
                    document,
                    attribute,
                    format!("{element_name}.{property} does not support binding markup extensions"),
                ));
            }
            let editable = matches!(
                property,
                "Text" | "Value" | "SelectedValue" | "SelectedValues" | "IsChecked"
            ) && !matches!(kind, ElementKind::TextBlock(_));
            let mut binding = binding::parse_binding(document, attribute, editable)?;
            if matches!(property, "SelectedValue" | "SelectedValues" | "IsChecked")
                && !attribute.value().contains("UpdateSourceTrigger")
            {
                binding.update_source_trigger = UpdateSourceTrigger::PropertyChanged;
            }
            common.bindings.insert(property.into(), binding);
            continue;
        }
        reject_extension(document, attribute)?;
        if attribute.namespace() == Some(XAML_NAMESPACE) && attribute.name() == "Name" {
            let name = attribute.value();
            identifier(document, attribute, "x:Name")?;
            if !names.insert(name.into()) {
                return Err(at_attribute(
                    document,
                    attribute,
                    format!("duplicate x:Name '{name}' in this Window"),
                ));
            }
            common.name = Some(name.into());
            continue;
        }
        if attribute.namespace().is_some() {
            return Err(at_attribute(
                document,
                attribute,
                format!(
                    "unsupported {element_name} namespace or directive; only x:Name is supported"
                ),
            ));
        }
        match attribute.name() {
            "AutomationName" => common.automation_name = Some(attribute.value().into()),
            "ToolTip" => {
                if attribute.value().len() > MAX_TOOLTIP_BYTES {
                    return Err(at_attribute(
                        document,
                        attribute,
                        "ToolTip exceeds the 4096-byte UTF-8 limit",
                    ));
                }
                common.tool_tip = Some(attribute.value().into());
            }
            "Width" => {
                common.width = if attribute.value() == "Auto" {
                    None
                } else {
                    Some(editor_dimension(document, attribute)?)
                }
            }
            "Height" => {
                common.height = if attribute.value() == "Auto" {
                    None
                } else {
                    Some(editor_dimension(document, attribute)?)
                }
            }
            "Margin" => common.margin = thickness(document, attribute)?,
            "IsEnabled" => common.is_enabled = boolean(document, attribute)?,
            property => {
                if form_parse::common_property(document, attribute, &mut common)? {
                    continue;
                }
                let supported = match &mut kind {
                    ElementKind::Ribbon(ribbon) if property == "SelectedTab" => {
                        identifier(document, attribute, "SelectedTab")?;
                        ribbon.selected_tab = Some(attribute.value().into());
                        true
                    }
                    ElementKind::Ribbon(ribbon) if property == "IsMinimized" => {
                        ribbon.is_minimized = boolean(document, attribute)?;
                        true
                    }
                    ElementKind::DropDownButton(menu) | ElementKind::SplitButton(menu) => {
                        if property == "Command" && element_name == "DropDownButton" {
                            false
                        } else {
                            menu_button_property(document, attribute, &mut menu.button)?
                        }
                    }
                    ElementKind::TextBox(editor) => editor_property(document, attribute, editor)?,
                    ElementKind::StackPanel(panel) | ElementKind::WrapPanel(panel) => {
                        match property {
                            "Orientation" => {
                                panel.orientation = match attribute.value() {
                                    "Vertical" => Orientation::Vertical,
                                    "Horizontal" => Orientation::Horizontal,
                                    _ => {
                                        return Err(at_attribute(
                                            document,
                                            attribute,
                                            "Orientation must be Vertical or Horizontal",
                                        ));
                                    }
                                };
                                true
                            }
                            "Spacing" => {
                                panel.spacing =
                                    editor_metric(document, attribute, MAX_DIMENSION, true)?;
                                true
                            }
                            "Padding" => {
                                panel.padding = thickness(document, attribute)?;
                                true
                            }
                            "Surface" => {
                                panel.surface = match attribute.value() {
                                    "Transparent" => SurfaceKindDefinition::Transparent,
                                    "Card" => SurfaceKindDefinition::Card,
                                    _ => {
                                        return Err(at_attribute(
                                            document,
                                            attribute,
                                            "Surface must be Transparent or Card",
                                        ));
                                    }
                                };
                                true
                            }
                            _ => false,
                        }
                    }
                    ElementKind::TextBlock(text) => text_block_property(document, attribute, text)?,
                    ElementKind::Button(button) => match property {
                        "Icon" => {
                            button.icon = Some(icon_symbol(document, attribute)?);
                            true
                        }
                        "IconSize" => {
                            button.icon_size =
                                editor_metric(document, attribute, MAX_FONT_SIZE, false)?;
                            true
                        }
                        "IconSpacing" => {
                            button.icon_spacing =
                                editor_metric(document, attribute, MAX_DIMENSION, true)?;
                            true
                        }
                        "Content" => {
                            button.content = attribute.value().into();
                            true
                        }
                        "Command" => {
                            identifier(document, attribute, "Command")?;
                            button.command = Some(attribute.value().into());
                            true
                        }
                        _ => button_style_property(document, attribute, &mut button.style)?,
                    },
                    ElementKind::ToggleButton(button) => match property {
                        "Icon" => {
                            button.icon = Some(icon_symbol(document, attribute)?);
                            true
                        }
                        "IconSize" => {
                            button.icon_size =
                                editor_metric(document, attribute, MAX_FONT_SIZE, false)?;
                            true
                        }
                        "IconSpacing" => {
                            button.icon_spacing =
                                editor_metric(document, attribute, MAX_DIMENSION, true)?;
                            true
                        }
                        "Content" => {
                            button.content = attribute.value().into();
                            true
                        }
                        "Command" => {
                            identifier(document, attribute, "Command")?;
                            button.command = Some(attribute.value().into());
                            true
                        }
                        "IsChecked" => {
                            button.is_checked = boolean(document, attribute)?;
                            true
                        }
                        _ => button_style_property(document, attribute, &mut button.style)?,
                    },
                    ElementKind::CheckBox(check_box) => match property {
                        "Content" => {
                            check_box.content = attribute.value().into();
                            true
                        }
                        "IsChecked" => {
                            check_box.is_checked = boolean(document, attribute)?;
                            true
                        }
                        _ => button_style_property(document, attribute, &mut check_box.style)?,
                    },
                    ElementKind::RadioButton(radio) => match property {
                        "Content" => {
                            radio.content = attribute.value().into();
                            true
                        }
                        "GroupName" => {
                            identifier(document, attribute, "GroupName")?;
                            radio.group_name = Some(attribute.value().into());
                            true
                        }
                        "IsChecked" => {
                            radio.is_checked = boolean(document, attribute)?;
                            true
                        }
                        _ => button_style_property(document, attribute, &mut radio.style)?,
                    },
                    ElementKind::Icon(icon) => match property {
                        "Symbol" => {
                            icon.symbol = icon_symbol(document, attribute)?;
                            true
                        }
                        "Foreground" => {
                            icon.foreground = Some(color(document, attribute)?);
                            true
                        }
                        _ => false,
                    },
                    ElementKind::Separator(separator) => match property {
                        "Orientation" => {
                            separator.orientation = match attribute.value() {
                                "Horizontal" => Orientation::Horizontal,
                                "Vertical" => Orientation::Vertical,
                                _ => {
                                    return Err(at_attribute(
                                        document,
                                        attribute,
                                        "Orientation must be Horizontal or Vertical",
                                    ));
                                }
                            };
                            true
                        }
                        "Thickness" => {
                            separator.thickness =
                                editor_metric(document, attribute, MAX_DIMENSION, false)?;
                            true
                        }
                        "Foreground" => {
                            separator.foreground = Some(color(document, attribute)?);
                            true
                        }
                        _ => false,
                    },
                    _ => form_parse::property(document, attribute, &mut kind)?,
                };
                if !supported {
                    return Err(at_attribute(
                        document,
                        attribute,
                        format!("unsupported {element_name} property '{property}'"),
                    ));
                }
            }
        }
    }
    let mut menu_text_bytes = 0;
    if let ElementKind::DockWorkspace(workspace) = &mut kind {
        *workspace = docking::parse_workspace(node, depth, count, names, checked_radio_groups)?;
    } else if let ElementKind::Ribbon(ribbon) = &mut kind {
        ribbon::parse_children(node, depth, count, ribbon)?;
    } else if matches!(
        kind,
        ElementKind::Grid(_)
            | ElementKind::Border(_)
            | ElementKind::ScrollViewer(_)
            | ElementKind::DockPanel(_)
            | ElementKind::ListBox(_)
            | ElementKind::ComboBox(_)
            | ElementKind::LookupEdit(_)
            | ElementKind::ButtonEdit(_)
            | ElementKind::SpinBox(_)
            | ElementKind::DateEdit(_)
            | ElementKind::TimeEdit(_)
    ) {
        form_parse::parse_children(
            node,
            depth,
            element_id,
            count,
            names,
            checked_radio_groups,
            &mut kind,
        )?;
    } else {
        for child in node.children() {
            if child.is_element() {
                if let ElementKind::StackPanel(panel) | ElementKind::WrapPanel(panel) = &mut kind {
                    panel.children.push(parse_element(
                        child,
                        depth + 1,
                        element_id,
                        count,
                        names,
                        checked_radio_groups,
                    )?);
                } else if let ElementKind::DropDownButton(menu) | ElementKind::SplitButton(menu) =
                    &mut kind
                {
                    menu.items.push(parse_menu_entry(
                        child,
                        depth + 1,
                        1,
                        count,
                        &mut menu_text_bytes,
                    )?);
                } else {
                    return Err(at_node(
                        child,
                        format!("{element_name} cannot contain child elements"),
                    ));
                }
            } else {
                require_empty_content(child)?;
            }
        }
    }
    form_parse::validate(node, &kind, &common)?;
    if let ElementKind::TextBox(editor) = &kind {
        validate_initial_text(node, editor)?;
    }
    if matches!(kind, ElementKind::Icon(_)) && node.attribute("Symbol").is_none() {
        return Err(at_node(node, "Icon requires a Symbol attribute"));
    }
    let button_icon = match &kind {
        ElementKind::DropDownButton(menu) | ElementKind::SplitButton(menu) => {
            Some((&menu.button.content, menu.button.icon))
        }
        ElementKind::Button(button) => Some((&button.content, button.icon)),
        ElementKind::ToggleButton(button) => Some((&button.content, button.icon)),
        _ => None,
    };
    if let ElementKind::DropDownButton(menu) | ElementKind::SplitButton(menu) = &kind {
        if menu.items.is_empty() {
            return Err(at_node(node, "A menu button requires nonempty menu items"));
        }
        if matches!(kind, ElementKind::SplitButton(_)) && menu.button.command.is_none() {
            return Err(at_node(node, "SplitButton requires a Command"));
        }
    }
    if let Some((content, icon)) = button_icon {
        if icon.is_none()
            && let Some(attribute) = node
                .attributes()
                .find(|attribute| matches!(attribute.name(), "IconSize" | "IconSpacing"))
        {
            return Err(at_attribute(
                document,
                attribute,
                "IconSize and IconSpacing require Icon",
            ));
        }
        if icon.is_some()
            && content.trim().is_empty()
            && common
                .automation_name
                .as_deref()
                .is_none_or(|name| name.trim().is_empty())
        {
            return Err(at_node(
                node,
                "an icon-only button requires a nonempty AutomationName",
            ));
        }
    }
    if let ElementKind::RadioButton(radio) = &kind
        && radio.is_checked
    {
        let group = radio
            .group_name
            .as_ref()
            .map_or(RadioGroupKey::Parent(parent), |name| {
                RadioGroupKey::Named(name.clone())
            });
        if !checked_radio_groups.insert(group) {
            return Err(at_node(
                node,
                "a RadioButton group may have at most one initial IsChecked=True",
            ));
        }
    }
    Ok(ElementDefinition { common, kind })
}

fn identifier(
    document: &Document<'_>,
    attribute: Attribute<'_, '_>,
    property: &str,
) -> Result<(), Diagnostic> {
    let mut chars = attribute.value().chars();
    if !chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        || !chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        return Err(at_attribute(
            document,
            attribute,
            format!("{property} must match [A-Za-z_][A-Za-z0-9_]*"),
        ));
    }
    Ok(())
}

fn text_block_property(
    document: &Document<'_>,
    attribute: Attribute<'_, '_>,
    text: &mut TextBlockDefinition,
) -> Result<bool, Diagnostic> {
    match attribute.name() {
        "Text" => text.text = attribute.value().into(),
        "FontSize" => {
            text.font_size = Some(editor_metric(document, attribute, MAX_FONT_SIZE, false)?)
        }
        "FontWeight" => {
            let value = attribute.value();
            let weight = match value {
                "Normal" => Some(400),
                "SemiBold" => Some(600),
                "Bold" => Some(700),
                _ if !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()) => {
                    value.parse::<u16>().ok()
                }
                _ => None,
            };
            text.font_weight = Some(
                weight
                    .filter(|weight| (1..=1000).contains(weight))
                    .ok_or_else(|| {
                        at_attribute(
                            document,
                            attribute,
                            "FontWeight must be Normal, SemiBold, Bold or an integer in [1, 1000]",
                        )
                    })?,
            );
        }
        "Foreground" => text.foreground = Some(color(document, attribute)?),
        "TextTone" => {
            text.tone = match attribute.value() {
                "Primary" => TextToneDefinition::Primary,
                "Secondary" => TextToneDefinition::Secondary,
                _ => {
                    return Err(at_attribute(
                        document,
                        attribute,
                        "TextTone must be Primary or Secondary",
                    ));
                }
            };
        }
        "TextWrapping" => {
            text.wrap = match attribute.value() {
                "Wrap" => true,
                "NoWrap" => false,
                _ => {
                    return Err(at_attribute(
                        document,
                        attribute,
                        "TextWrapping must be Wrap or NoWrap",
                    ));
                }
            };
        }
        _ => return Ok(false),
    }
    Ok(true)
}

fn menu_button_property(
    document: &Document<'_>,
    attribute: Attribute<'_, '_>,
    button: &mut ButtonDefinition,
) -> Result<bool, Diagnostic> {
    match attribute.name() {
        "Content" => button.content = attribute.value().into(),
        "Command" => {
            identifier(document, attribute, "Command")?;
            button.command = Some(attribute.value().into());
        }
        "Icon" => button.icon = Some(icon_symbol(document, attribute)?),
        "IconSize" => button.icon_size = editor_metric(document, attribute, MAX_FONT_SIZE, false)?,
        "IconSpacing" => {
            button.icon_spacing = editor_metric(document, attribute, MAX_DIMENSION, true)?
        }
        _ => return button_style_property(document, attribute, &mut button.style),
    }
    Ok(true)
}

fn parse_menu_entry(
    node: Node<'_, '_>,
    depth: usize,
    menu_depth: usize,
    count: &mut usize,
    text_bytes: &mut usize,
) -> Result<MenuEntryDefinition, Diagnostic> {
    *count += 1;
    if depth > MAX_ELEMENT_DEPTH || menu_depth > MAX_MENU_DEPTH || *count > MAX_ELEMENTS {
        return Err(at_node(
            node,
            "Menu exceeds the element count or depth limit",
        ));
    }
    if node.tag_name().namespace() != Some(UI_NAMESPACE) {
        return Err(at_node(node, "Menu entries require the UI namespace"));
    }
    if node.tag_name().name() == "Separator" {
        if node.attributes().len() != 0 {
            return Err(at_node(node, "A menu Separator accepts no attributes"));
        }
        for child in node.children() {
            if child.is_element() {
                return Err(at_node(child, "A menu Separator accepts no children"));
            }
            require_empty_content(child)?;
        }
        return Ok(MenuEntryDefinition::Separator);
    }
    require_element(node, "MenuItem")?;
    let mut item = MenuItemDefinition {
        header: String::new(),
        command: None,
        is_enabled: true,
        items: Vec::new(),
    };
    for attribute in node.attributes() {
        reject_extension(node.document(), attribute)?;
        if attribute.namespace().is_some() {
            return Err(at_attribute(
                node.document(),
                attribute,
                "MenuItem accepts no namespaced properties",
            ));
        }
        match attribute.name() {
            "Header" => item.header = attribute.value().into(),
            "Command" => {
                identifier(node.document(), attribute, "Command")?;
                item.command = Some(attribute.value().into());
            }
            "IsEnabled" => item.is_enabled = boolean(node.document(), attribute)?,
            _ => {
                return Err(at_attribute(
                    node.document(),
                    attribute,
                    "Unsupported MenuItem property",
                ));
            }
        }
    }
    if item.header.trim().is_empty() {
        return Err(at_node(node, "MenuItem requires a nonempty Header"));
    }
    *text_bytes += item.header.len() + item.command.as_ref().map_or(0, String::len);
    if *text_bytes > MAX_MENU_TEXT_BYTES {
        return Err(at_node(node, "Menu text exceeds the 1 MiB UTF-8 limit"));
    }
    for child in node.children() {
        if child.is_element() {
            item.items.push(parse_menu_entry(
                child,
                depth + 1,
                menu_depth + 1,
                count,
                text_bytes,
            )?);
        } else {
            require_empty_content(child)?;
        }
    }
    if item.command.is_some() != item.items.is_empty() {
        return Err(at_node(
            node,
            "MenuItem requires either a Command or nonempty submenu, exclusively",
        ));
    }
    Ok(MenuEntryDefinition::Item(item))
}

fn icon_symbol(
    document: &Document<'_>,
    attribute: Attribute<'_, '_>,
) -> Result<IconSymbol, Diagnostic> {
    match attribute.value() {
        "Check" => Ok(IconSymbol::Check),
        "Close" => Ok(IconSymbol::Close),
        "Add" => Ok(IconSymbol::Add),
        "Remove" => Ok(IconSymbol::Remove),
        "ChevronDown" => Ok(IconSymbol::ChevronDown),
        "ChevronRight" => Ok(IconSymbol::ChevronRight),
        "Save" => Ok(IconSymbol::Save),
        "Pin" => Ok(IconSymbol::Pin),
        "FolderOpen" => Ok(IconSymbol::FolderOpen),
        "Copy" => Ok(IconSymbol::Copy),
        "Edit" => Ok(IconSymbol::Edit),
        "Eraser" => Ok(IconSymbol::Eraser),
        "Sun" => Ok(IconSymbol::Sun),
        "Moon" => Ok(IconSymbol::Moon),
        "Lock" => Ok(IconSymbol::Lock),
        _ => Err(at_attribute(
            document,
            attribute,
            format!(
                "{} must be Check, Close, Add, Remove, ChevronDown, ChevronRight, Save, Pin, FolderOpen, Copy, Edit, Eraser, Sun, Moon or Lock",
                attribute.name()
            ),
        )),
    }
}

fn editor_property(
    document: &Document<'_>,
    attribute: Attribute<'_, '_>,
    definition: &mut EditBoxDefinition,
) -> Result<bool, Diagnostic> {
    match attribute.name() {
        "Text" => definition.text = attribute.value().into(),
        "PlaceholderText" => definition.placeholder = attribute.value().into(),
        "IsReadOnly" => definition.is_read_only = boolean(document, attribute)?,
        "MaxLength" => definition.max_length = max_length(document, attribute)?,
        "IsRequired" => definition.is_required = boolean(document, attribute)?,
        "CommitOnLostFocus" => definition.commit_on_lost_focus = boolean(document, attribute)?,
        "ValidationMessage" => {
            if attribute.value().len() > MAX_VALIDATION_MESSAGE_BYTES {
                return Err(at_attribute(
                    document,
                    attribute,
                    format!(
                        "ValidationMessage exceeds the {MAX_VALIDATION_MESSAGE_BYTES}-byte UTF-8 limit"
                    ),
                ));
            }
            definition.validation_message = attribute.value().into();
        }
        "FontSize" => {
            definition.font_size = Some(editor_metric(document, attribute, MAX_FONT_SIZE, false)?);
        }
        "Padding" => definition.padding = Some(thickness(document, attribute)?),
        "CornerRadius" => {
            definition.corner_radius =
                Some(editor_metric(document, attribute, MAX_DIMENSION, true)?);
        }
        "BorderThickness" => {
            definition.border_thickness =
                Some(editor_metric(document, attribute, MAX_DIMENSION, true)?);
        }
        "Background" => definition.background = Some(color(document, attribute)?),
        "Foreground" => definition.foreground = Some(color(document, attribute)?),
        "BorderBrush" => definition.border_brush = Some(color(document, attribute)?),
        "FocusBrush" => definition.focus_brush = Some(color(document, attribute)?),
        "PlaceholderForeground" => {
            definition.placeholder_foreground = Some(color(document, attribute)?)
        }
        "SelectionBrush" => definition.selection_brush = Some(color(document, attribute)?),
        "ValidationBrush" => definition.validation_brush = Some(color(document, attribute)?),
        _ => return Ok(false),
    }
    Ok(true)
}

fn button_style_property(
    document: &Document<'_>,
    attribute: Attribute<'_, '_>,
    definition: &mut ButtonStyleDefinition,
) -> Result<bool, Diagnostic> {
    match attribute.name() {
        "FontSize" => {
            definition.font_size = Some(editor_metric(document, attribute, MAX_FONT_SIZE, false)?)
        }
        "Padding" => definition.padding = Some(thickness(document, attribute)?),
        "CornerRadius" => {
            definition.corner_radius =
                Some(editor_metric(document, attribute, MAX_DIMENSION, true)?)
        }
        "BorderThickness" => {
            definition.border_thickness =
                Some(editor_metric(document, attribute, MAX_DIMENSION, true)?)
        }
        "Background" => definition.background = Some(color(document, attribute)?),
        "Foreground" => definition.foreground = Some(color(document, attribute)?),
        "BorderBrush" => definition.border_brush = Some(color(document, attribute)?),
        "FocusBrush" => definition.focus_brush = Some(color(document, attribute)?),
        _ => return Ok(false),
    }
    Ok(true)
}

fn validate_initial_text(
    node: Node<'_, '_>,
    definition: &EditBoxDefinition,
) -> Result<(), Diagnostic> {
    // Match the single-line control's policy before checking the initial value.
    // Preserve the source text in the definition; mounting normalizes it too.
    if let Some(limit) = definition.max_length {
        let text: String = definition
            .text
            .chars()
            .filter(|c| !matches!(c, '\r' | '\n' | '\t' | '\u{85}' | '\u{2028}' | '\u{2029}'))
            .collect();
        if text.graphemes(true).count() > limit {
            return Err(at_attribute(
                node.document(),
                node.attribute_node("Text")
                    .expect("nonempty Text has an attribute"),
                "Text exceeds MaxLength after single-line normalization",
            ));
        }
    }
    Ok(())
}

fn boolean(document: &Document<'_>, attribute: Attribute<'_, '_>) -> Result<bool, Diagnostic> {
    match attribute.value() {
        "True" | "true" => Ok(true),
        "False" | "false" => Ok(false),
        _ => Err(at_attribute(
            document,
            attribute,
            format!("{} must be True or False", attribute.name()),
        )),
    }
}

fn max_length(
    document: &Document<'_>,
    attribute: Attribute<'_, '_>,
) -> Result<Option<usize>, Diagnostic> {
    let value = attribute.value();
    let parsed = (!value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()))
        .then(|| value.parse::<usize>().ok())
        .flatten()
        .filter(|length| *length <= MAX_TEXT_LENGTH)
        .ok_or_else(|| {
            at_attribute(
                document,
                attribute,
                format!("MaxLength must be an integer in [0, {MAX_TEXT_LENGTH}]"),
            )
        })?;
    Ok((parsed != 0).then_some(parsed))
}

fn require_element(node: Node<'_, '_>, name: &str) -> Result<(), Diagnostic> {
    if node.tag_name().namespace() != Some(UI_NAMESPACE) || node.tag_name().name() != name {
        return Err(at_node(
            node,
            format!("expected '{name}' in namespace '{UI_NAMESPACE}'"),
        ));
    }
    Ok(())
}

fn require_empty_content(node: Node<'_, '_>) -> Result<(), Diagnostic> {
    if node.is_text() && !node.text().unwrap_or_default().trim().is_empty() {
        return Err(at_node(
            node,
            "text content is not supported; use the Text property",
        ));
    }
    Ok(())
}

fn reject_extension(
    document: &Document<'_>,
    attribute: Attribute<'_, '_>,
) -> Result<(), Diagnostic> {
    if attribute.value().trim_start().starts_with('{') {
        return Err(at_attribute(
            document,
            attribute,
            "markup extensions and bindings are not supported in this profile",
        ));
    }
    Ok(())
}

fn dimension(document: &Document<'_>, attribute: Attribute<'_, '_>) -> Result<f64, Diagnostic> {
    attribute
        .value()
        .trim()
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite() && *value > 0.0 && *value <= MAX_DIMENSION)
        .ok_or_else(|| {
            at_attribute(
                document,
                attribute,
                format!(
                    "{} must be a finite number in (0, {MAX_DIMENSION}]",
                    attribute.name()
                ),
            )
        })
}

fn editor_dimension(
    document: &Document<'_>,
    attribute: Attribute<'_, '_>,
) -> Result<f32, Diagnostic> {
    let value = dimension(document, attribute)? as f32;
    if value == 0.0 {
        return Err(at_attribute(
            document,
            attribute,
            format!(
                "{} is too small for the editor's logical pixel precision",
                attribute.name()
            ),
        ));
    }
    Ok(value)
}

fn editor_metric(
    document: &Document<'_>,
    attribute: Attribute<'_, '_>,
    maximum: f64,
    allow_zero: bool,
) -> Result<f32, Diagnostic> {
    attribute
        .value()
        .trim()
        .parse::<f64>()
        .ok()
        .filter(|value| {
            value.is_finite()
                && *value <= maximum
                && if allow_zero { *value >= 0.0 } else { *value > 0.0 && (*value as f32) > 0.0 }
        })
        .map(|value| value as f32)
        .ok_or_else(|| {
            at_attribute(
                document,
                attribute,
                format!(
                    "{} must be a finite number in {}0, {maximum}] representable as an editor metric",
                    attribute.name(),
                    if allow_zero { "[" } else { "(" }
                ),
            )
        })
}

fn color(document: &Document<'_>, attribute: Attribute<'_, '_>) -> Result<[u8; 4], Diagnostic> {
    let value = attribute.value();
    let digits = value
        .strip_prefix('#')
        .filter(|digits| {
            matches!(digits.len(), 6 | 8) && digits.bytes().all(|byte| byte.is_ascii_hexdigit())
        })
        .ok_or_else(|| {
            at_attribute(
                document,
                attribute,
                format!("{} must be #RRGGBB or #AARRGGBB", attribute.name()),
            )
        })?;
    // ASCII-only validated hex makes each byte pair a valid UTF-8 slice.
    let byte = |offset| u8::from_str_radix(&digits[offset..offset + 2], 16).expect("validated hex");
    let (offset, alpha) = if digits.len() == 8 {
        (2, byte(0))
    } else {
        (0, 255)
    };
    Ok([byte(offset), byte(offset + 2), byte(offset + 4), alpha])
}

fn thickness(
    document: &Document<'_>,
    attribute: Attribute<'_, '_>,
) -> Result<[f32; 4], Diagnostic> {
    let error = || {
        at_attribute(
            document,
            attribute,
            format!(
                "{} needs 1, 2, or 4 comma-separated finite numbers in [0, {MAX_DIMENSION}]",
                attribute.name()
            ),
        )
    };
    let mut values = [0.0; 4];
    let mut count = 0;
    for item in attribute.value().split(',') {
        let value = item.trim().parse::<f64>().map_err(|_| error())?;
        if count == 4 || !value.is_finite() || !(0.0..=MAX_DIMENSION).contains(&value) {
            return Err(error());
        }
        values[count] = value as f32;
        count += 1;
    }
    match count {
        1 => Ok([values[0]; 4]),
        2 => Ok([values[0], values[1], values[0], values[1]]),
        4 => Ok(values),
        _ => Err(error()),
    }
}

fn at_node(node: Node<'_, '_>, message: impl Into<String>) -> Diagnostic {
    at_offset(node.document(), node.range().start, message)
}

fn at_attribute(
    document: &Document<'_>,
    attribute: Attribute<'_, '_>,
    message: impl Into<String>,
) -> Diagnostic {
    at_offset(document, attribute.range().start, message)
}

fn at_offset(document: &Document<'_>, offset: usize, message: impl Into<String>) -> Diagnostic {
    let position = document.text_pos_at(offset);
    Diagnostic {
        line: position.row,
        column: position.col,
        message: message.into(),
    }
}

/// Compile XAML into a Rust `window_definition()` function for `include!`.
///
/// The generated code references `rust_desktop_ui_xaml`, which must also be a
/// regular dependency of the consuming crate. All user strings are emitted as
/// escaped Rust literals; XAML never supplies executable Rust expressions.
pub fn compile(source: &str) -> Result<String, Diagnostic> {
    Ok(codegen::window(&parse(source)?))
}

/// A file build error. Parse errors display `path:line:column: message`.
#[derive(Debug)]
pub struct BuildError {
    pub path: std::path::PathBuf,
    pub diagnostic: Option<Diagnostic>,
    io_error: Option<std::io::Error>,
}

impl fmt::Display for BuildError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}:", self.path.display())?;
        if let Some(diagnostic) = &self.diagnostic {
            write!(formatter, "{diagnostic}")
        } else if let Some(error) = &self.io_error {
            write!(formatter, " {error}")
        } else {
            write!(formatter, " compilation failed")
        }
    }
}

impl std::error::Error for BuildError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.diagnostic
            .as_ref()
            .map(|value| value as &dyn std::error::Error)
            .or_else(|| {
                self.io_error
                    .as_ref()
                    .map(|value| value as &dyn std::error::Error)
            })
    }
}

/// Compile a UTF-8 XAML file into an existing output directory.
///
/// Intended for `build.rs`; callers should emit their own
/// `cargo:rerun-if-changed=...` directive and pass an output path inside
/// `OUT_DIR`. Reading is bounded even if the source file is too large.
pub fn compile_file(input: impl AsRef<Path>, output: impl AsRef<Path>) -> Result<(), BuildError> {
    let input = input.as_ref();
    let output = output.as_ref();
    let read_error = |error| BuildError {
        path: input.to_owned(),
        diagnostic: None,
        io_error: Some(error),
    };
    let mut source = String::new();
    fs::File::open(input)
        .map_err(read_error)?
        .take(MAX_SOURCE_BYTES as u64 + 1)
        .read_to_string(&mut source)
        .map_err(read_error)?;
    let generated = compile(&source).map_err(|diagnostic| BuildError {
        path: input.to_owned(),
        diagnostic: Some(diagnostic),
        io_error: None,
    })?;
    fs::write(output, generated).map_err(|error| BuildError {
        path: output.to_owned(),
        diagnostic: None,
        io_error: Some(error),
    })
}

#[cfg(test)]
mod docking_tests;
#[cfg(test)]
mod form_tests;
#[cfg(test)]
mod ribbon_tests;
#[cfg(test)]
mod tests;
