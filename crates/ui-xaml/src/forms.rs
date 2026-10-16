//! Selection and typed editor definitions for compiled forms.
use crate::{EditBoxDefinition, ElementDefinition, IconSymbol};
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone, Debug, PartialEq)]
pub struct SelectionItemDefinition {
    pub value: String,
    pub display: String,
    pub is_enabled: bool,
}

impl Default for SelectionItemDefinition {
    fn default() -> Self {
        Self {
            value: String::new(),
            display: String::new(),
            is_enabled: true,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SelectionDefinition {
    pub items: Vec<SelectionItemDefinition>,
    pub selected_value: Option<String>,
    pub display_member_path: Option<String>,
    pub value_member_path: Option<String>,
    pub placeholder: String,
    pub is_read_only: bool,
    pub is_editable: bool,
    pub selection_mode: SelectionMode,
    pub selected_values: Vec<String>,
    pub text: String,
    pub item_presentation: ListItemPresentationDefinition,
    pub selection_presentation: ComboSelectionPresentationDefinition,
    pub commit_on_lost_focus: Option<bool>,
    pub item_template: Option<Box<ElementDefinition>>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ListItemPresentationDefinition {
    #[default]
    Plain,
    CheckBox,
    RadioButton,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ComboSelectionPresentationDefinition {
    #[default]
    Text,
    Tokens,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SelectionMode {
    #[default]
    Single,
    Multiple,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LookupColumnDefinition {
    pub field: String,
    pub header: String,
    pub width: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LookupDefinition {
    pub selection: SelectionDefinition,
    pub columns: Vec<LookupColumnDefinition>,
    pub search_columns: Vec<usize>,
    pub search_mode: LookupSearchModeDefinition,
    pub search_comparison: LookupSearchComparisonDefinition,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LookupSearchModeDefinition {
    #[default]
    Contains,
    StartsWith,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LookupSearchComparisonDefinition {
    Ordinal,
    #[default]
    UnicodeLowercase,
}

impl Default for LookupDefinition {
    fn default() -> Self {
        Self {
            selection: SelectionDefinition {
                is_editable: true,
                ..Default::default()
            },
            columns: Vec::new(),
            search_columns: Vec::new(),
            search_mode: LookupSearchModeDefinition::default(),
            search_comparison: LookupSearchComparisonDefinition::default(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EditorButtonPlacement {
    Leading,
    #[default]
    Trailing,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EditorButtonDefinition {
    pub command: String,
    pub icon: Option<IconSymbol>,
    pub content: String,
    pub tool_tip: Option<String>,
    pub placement: EditorButtonPlacement,
    pub width: f32,
    pub is_visible: bool,
    pub is_enabled: bool,
    pub is_enabled_when_read_only: bool,
    pub is_tab_stop: bool,
    pub content_template: Option<Box<ElementDefinition>>,
}

impl Default for EditorButtonDefinition {
    fn default() -> Self {
        Self {
            command: String::new(),
            icon: None,
            content: String::new(),
            tool_tip: None,
            placement: EditorButtonPlacement::Trailing,
            width: 30.0,
            is_visible: true,
            is_enabled: true,
            is_enabled_when_read_only: false,
            is_tab_stop: true,
            content_template: None,
        }
    }
}

/// Checks button metadata and its noninteractive visual subtree before mounting.
pub fn validate_editor_button(button: &EditorButtonDefinition) -> Result<(), String> {
    if !crate::binding::valid_identifier(&button.command) {
        return Err("EditorButton requires a valid Command".into());
    }
    if button.content.trim().is_empty() || button.content.len() > 4096 {
        return Err(
            "EditorButton requires nonempty Content up to 4096 bytes for its accessible name"
                .into(),
        );
    }
    if !button.width.is_finite() || button.width <= 0.0 || button.width > 1024.0 {
        return Err("EditorButton Width must be finite and in (0, 1024]".into());
    }
    if button
        .tool_tip
        .as_ref()
        .is_some_and(|tip| tip.len() > crate::MAX_TOOLTIP_BYTES)
    {
        return Err("EditorButton ToolTip exceeds its text limit".into());
    }
    if let Some(content) = &button.content_template {
        crate::validate_header_content(content)?;
    }
    Ok(())
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ButtonEditDefinition {
    pub editor: EditBoxDefinition,
    pub buttons: Vec<EditorButtonDefinition>,
}

/// Multiline text edited in a retained popup; display dimensions are logical pixels.
#[derive(Clone, Debug, PartialEq)]
pub struct MemoEditDefinition {
    pub editor: EditBoxDefinition,
    pub popup_width: f32,
    pub popup_height: f32,
}

impl Default for MemoEditDefinition {
    fn default() -> Self {
        Self {
            editor: EditBoxDefinition::default(),
            popup_width: 480.0,
            popup_height: 300.0,
        }
    }
}

pub fn validate_memo_edit(memo: &MemoEditDefinition) -> Result<(), String> {
    if [(memo.popup_width, 160.0), (memo.popup_height, 120.0)]
        .into_iter()
        .any(|(dimension, minimum)| {
            !dimension.is_finite() || !(minimum..=crate::MAX_DIMENSION as f32).contains(&dimension)
        })
    {
        return Err(
            "MemoEdit requires finite PopupWidth in [160, 16384] and PopupHeight in [120, 16384]"
                .into(),
        );
    }
    if memo
        .editor
        .max_length
        .is_some_and(|limit| memo.editor.text.graphemes(true).count() > limit)
    {
        return Err("MemoEdit Text exceeds MaxLength".into());
    }
    Ok(())
}

/// Initially empty protected input. A definition stores configuration only;
/// secret values are supplied through the runtime's explicit password API.
#[derive(Clone, Debug, PartialEq)]
pub struct PasswordBoxDefinition {
    pub max_length: usize,
    pub is_read_only: bool,
    pub allow_paste: bool,
    pub allow_reveal: bool,
    pub placeholder: String,
    pub reveal_duration_ms: u32,
    pub style: crate::ButtonStyleDefinition,
    pub placeholder_foreground: Option<[u8; 4]>,
    pub selection_brush: Option<[u8; 4]>,
}
impl Default for PasswordBoxDefinition {
    fn default() -> Self {
        Self {
            max_length: 256,
            is_read_only: false,
            allow_paste: true,
            allow_reveal: true,
            placeholder: String::new(),
            reveal_duration_ms: 3000,
            style: Default::default(),
            placeholder_foreground: None,
            selection_brush: None,
        }
    }
}
pub fn validate_password_box(password: &PasswordBoxDefinition) -> Result<(), String> {
    let style = &password.style;
    if password.max_length > 4096
        || password.placeholder.len() > 4096
        || !(1..=10_000).contains(&password.reveal_duration_ms)
        || style.font_size.is_some_and(|size| {
            !size.is_finite() || size <= 0.0 || size > crate::MAX_FONT_SIZE as f32
        })
        || [style.corner_radius, style.border_thickness]
            .into_iter()
            .flatten()
            .any(|n| !n.is_finite() || !(0.0..=crate::MAX_DIMENSION as f32).contains(&n))
        || style.padding.is_some_and(|edges| {
            edges
                .into_iter()
                .any(|n| !n.is_finite() || !(0.0..=crate::MAX_DIMENSION as f32).contains(&n))
        })
    {
        return Err("Invalid PasswordBox settings or appearance".into());
    }
    Ok(())
}

/// Ordinary bindings can control availability and geometry, never a password.
pub fn validate_password_bindings(common: &crate::CommonProperties) -> Result<(), String> {
    if common.bindings.keys().any(|property| {
        !matches!(
            property.as_str(),
            "IsEnabled" | "Visibility" | "Width" | "Height"
        )
    }) {
        return Err(
            "PasswordBox permits only IsEnabled, Visibility, Width and Height bindings".into(),
        );
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq)]
pub struct SpinBoxDefinition {
    pub editor: EditBoxDefinition,
    pub value: f64,
    pub minimum: f64,
    pub maximum: f64,
    pub increment: f64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NumericCultureDefinition {
    #[default]
    Russian,
    English,
    Arabic,
}

/// Exact decimal settings use invariant strings, independently of display culture.
#[derive(Clone, Debug, PartialEq)]
pub struct NumericEditDefinition {
    pub editor: EditBoxDefinition,
    pub value: Option<String>,
    pub minimum: String,
    pub maximum: String,
    pub increment: String,
    pub precision: u8,
    pub allow_empty: bool,
    pub culture: NumericCultureDefinition,
    pub use_grouping: bool,
    pub show_calculator: bool,
}
impl Default for NumericEditDefinition {
    fn default() -> Self {
        Self {
            editor: Default::default(),
            value: None,
            minimum: "-1000000000000000".into(),
            maximum: "1000000000000000".into(),
            increment: "1".into(),
            precision: 2,
            allow_empty: true,
            culture: Default::default(),
            use_grouping: false,
            show_calculator: true,
        }
    }
}

pub fn validate_numeric_edit(
    numeric: &NumericEditDefinition,
    has_value_binding: bool,
) -> Result<(), String> {
    use rust_desktop_ui_data::DecimalValue;
    let parse = |text: &str| -> Result<DecimalValue, String> {
        if text.len() > 70 {
            return Err("Decimal literal exceeds its bounded storage syntax".into());
        }
        let value = text
            .parse::<DecimalValue>()
            .map_err(|error| error.to_string())?;
        if value.to_string() != text {
            return Err("Decimal literals require canonical invariant syntax".into());
        }
        Ok(value)
    };
    let minimum = parse(&numeric.minimum)?;
    let maximum = parse(&numeric.maximum)?;
    let increment = parse(&numeric.increment)?;
    if numeric.precision > DecimalValue::MAX_SCALE
        || minimum > maximum
        || increment.coefficient() <= 0
        || [minimum, maximum, increment]
            .iter()
            .any(|value| value.scale() > numeric.precision)
    {
        return Err("NumericEdit requires ordered exact bounds, positive Increment and precision-compatible values".into());
    }
    if let Some(value) = &numeric.value {
        let value = parse(value)?;
        if value.scale() > numeric.precision || value < minimum || value > maximum {
            return Err("NumericEdit Value exceeds its range or precision".into());
        }
    } else if !numeric.allow_empty && !has_value_binding {
        return Err("NumericEdit requires Value when AllowEmpty=False".into());
    }
    Ok(())
}

impl Default for SpinBoxDefinition {
    fn default() -> Self {
        Self {
            editor: EditBoxDefinition::default(),
            value: 0.0,
            minimum: -1e12,
            maximum: 1e12,
            increment: 1.0,
        }
    }
}

/// ISO values use `YYYY-MM-DD` or `HH:mm[:ss]`. A format only selects their display.
#[derive(Clone, Debug, PartialEq)]
pub struct TemporalEditDefinition {
    pub editor: EditBoxDefinition,
    pub value: Option<String>,
    pub minimum: Option<String>,
    pub maximum: Option<String>,
    pub format: String,
    pub allow_empty: bool,
    pub time_step: u32,
}
impl Default for TemporalEditDefinition {
    fn default() -> Self {
        Self {
            editor: Default::default(),
            value: None,
            minimum: None,
            maximum: None,
            format: String::new(),
            allow_empty: true,
            time_step: 60,
        }
    }
}
