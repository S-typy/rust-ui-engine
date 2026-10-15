//! Selection and typed editor definitions for compiled forms.
use crate::{EditBoxDefinition, IconSymbol};

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
}

impl Default for LookupDefinition {
    fn default() -> Self {
        Self {
            selection: SelectionDefinition {
                is_editable: true,
                ..Default::default()
            },
            columns: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct EditorButtonDefinition {
    pub command: String,
    pub icon: Option<IconSymbol>,
    pub content: String,
    pub tool_tip: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ButtonEditDefinition {
    pub editor: EditBoxDefinition,
    pub buttons: Vec<EditorButtonDefinition>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SpinBoxDefinition {
    pub editor: EditBoxDefinition,
    pub value: f64,
    pub minimum: f64,
    pub maximum: f64,
    pub increment: f64,
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
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TemporalEditDefinition {
    pub editor: EditBoxDefinition,
    pub value: Option<String>,
    pub minimum: Option<String>,
    pub maximum: Option<String>,
    pub format: String,
}
