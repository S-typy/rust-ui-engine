use crate::{
    CommandId, CommandRegistry, CommitOutcome, EditBoxStyle, EditBoxVisualState, EditEvent,
    EditorSettings, EditorValidation, EditorValidator, Theme,
};
use rust_desktop_ui_core::{
    Axis, Border, Color, Edges, FrameStats, InputEvent, Key, LayoutKind, LayoutStyle, Length,
    Mutation, NodeProps, Paint, Point, PointerButton, Rect, RuntimeError, Scene, Size, TextAlign,
    TextRun, UiRuntime, UiTree, WidgetId,
};
use rust_desktop_ui_layout::TaffyLayout;
use rust_desktop_ui_text::{
    EditError, PreparedText, TextCaret, TextDocument, TextEngine, TextError, TextLimit, TextLimits,
};
use std::{borrow::Cow, collections::HashMap, fmt};

#[derive(Debug)]
pub enum ControlError {
    Runtime(RuntimeError),
    Text(String),
    Edit(EditError),
    Invalid(String),
}
impl fmt::Display for ControlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Runtime(e) => e.fmt(f),
            Self::Edit(e) => e.fmt(f),
            Self::Text(e) | Self::Invalid(e) => f.write_str(e),
        }
    }
}
impl std::error::Error for ControlError {}
impl From<RuntimeError> for ControlError {
    fn from(value: RuntimeError) -> Self {
        Self::Runtime(value)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Panel,
    Label,
    Button,
    ToggleButton,
    CheckBox,
    TextBox,
    ScrollView,
    TabList,
    Tab,
    Menu,
    MenuItem,
    Group,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SemanticAction {
    Focus,
    Activate,
    SetValue,
    Toggle,
    Scroll,
}
#[derive(Clone, Debug)]
pub struct SemanticNode {
    pub id: WidgetId,
    pub parent: Option<WidgetId>,
    pub role: Role,
    pub label: String,
    pub value: Option<String>,
    pub checked: Option<bool>,
    pub selected: bool,
    pub disabled: bool,
    pub read_only: bool,
    /// Last failed business validation of the draft; not an IME error.
    pub validation_error: Option<String>,
    pub focused: bool,
    pub bounds: Rect,
    pub actions: Vec<SemanticAction>,
    pub children: Vec<WidgetId>,
}

/// One draft string change (IME preedit excluded). Drain after input or updates.
/// Initial values, selection, IME preedit and rejected/no-op edits emit no event.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextChanged {
    pub id: WidgetId,
    pub text: String,
}

#[derive(Clone, Debug)]
pub struct MenuItem {
    pub label: String,
    pub command: CommandId,
    pub toggle: bool,
}
impl MenuItem {
    pub fn new(label: impl Into<String>, command: impl Into<CommandId>) -> Self {
        Self {
            label: label.into(),
            command: command.into(),
            toggle: false,
        }
    }
    pub fn toggling(mut self) -> Self {
        self.toggle = true;
        self
    }
}

#[derive(Clone, Debug)]
pub struct TabControl {
    pub container: WidgetId,
    pub tabs: Vec<WidgetId>,
    pub pages: Vec<WidgetId>,
}
#[derive(Clone, Copy, Debug)]
pub struct ScrollViewer {
    pub viewport: WidgetId,
    pub content: WidgetId,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClipboardRequest {
    Copy(String),
    Paste,
}

enum Kind {
    Panel,
    Label,
    Button,
    Toggle,
    CheckBox,
    TextBox(TextDocument),
    Scroll { content: WidgetId, extent: f32 },
    Tab { group: WidgetId, index: usize },
    MenuItem { toggle: bool },
}
impl Kind {
    fn clickable(&self) -> bool {
        matches!(
            self,
            Self::Button | Self::Toggle | Self::CheckBox | Self::Tab { .. } | Self::MenuItem { .. }
        )
    }
    fn role(&self) -> Role {
        match self {
            Self::Panel => Role::Panel,
            Self::Label => Role::Label,
            Self::Button => Role::Button,
            Self::Toggle => Role::ToggleButton,
            Self::CheckBox => Role::CheckBox,
            Self::TextBox(_) => Role::TextBox,
            Self::Scroll { .. } => Role::ScrollView,
            Self::Tab { .. } => Role::Tab,
            Self::MenuItem { .. } => Role::MenuItem,
        }
    }
}
struct Control {
    kind: Kind,
    label: String,
    command: Option<CommandId>,
    enabled: bool,
    checked: bool,
    role: Option<Role>,
    tooltip: Option<String>,
    editor: EditorSettings,
    accepted_value: String,
    validation: EditorValidation,
    validator: Option<EditorValidator>,
    validation_error: Option<String>,
    edit_box_style: EditBoxStyle,
    single_line: bool,
    text_scroll_x: f32,
    text_scroll: f32,
    default_height: bool,
    transparent_panel: bool,
    caret: TextCaret,
}
struct Tabs {
    tabs: Vec<WidgetId>,
    pages: Vec<WidgetId>,
    selected: usize,
}
struct Popup {
    container: WidgetId,
    anchor: WidgetId,
    items: Vec<WidgetId>,
}

/// Owns control state while exposing the independent retained runtime for composition.
pub struct Controls {
    pub runtime: UiRuntime,
    layout: TaffyLayout,
    engine: TextEngine,
    root: WidgetId,
    overlay: WidgetId,
    theme: Theme,
    controls: HashMap<WidgetId, Control>,
    tabs: HashMap<WidgetId, Tabs>,
    commands: CommandRegistry,
    pressed: Option<WidgetId>,
    keyboard_pressed: bool,
    selecting: Option<WidgetId>,
    popup: Option<Popup>,
    clipboard: Vec<ClipboardRequest>,
    activated: Vec<WidgetId>,
    text_changes: Vec<TextChanged>,
    edit_events: Vec<EditEvent>,
    observed_editor_focus: Option<WidgetId>,
    viewport: Size,
    scene: Scene,
    text_layouts: HashMap<WidgetId, PreparedText>,
    scene_dirty: bool,
    total: FrameStats,
    caret_visible: bool,
    custom_scenes: HashMap<WidgetId, Scene>,
}

impl Controls {
    pub fn new(theme: Theme) -> Result<Self, ControlError> {
        let mut tree = UiTree::new();
        let overlay = tree.root();
        tree.set_style(
            overlay,
            LayoutStyle {
                kind: LayoutKind::Overlay,
                ..Default::default()
            },
        )
        .map_err(RuntimeError::from)?;
        let mut runtime = UiRuntime::new(tree);
        let root = runtime.insert(
            overlay,
            NodeProps {
                style: LayoutStyle {
                    kind: LayoutKind::Flex(Axis::Vertical),
                    padding: Edges::all(8.0),
                    gap: 8.0,
                    ..Default::default()
                },
                paint: Paint {
                    background: Some(theme.tokens().canvas),
                    ..Default::default()
                },
                clip: true,
                ..Default::default()
            },
        )?;
        Ok(Self {
            runtime,
            layout: TaffyLayout::new(),
            engine: TextEngine::new(),
            root,
            overlay,
            theme,
            controls: HashMap::new(),
            tabs: HashMap::new(),
            commands: CommandRegistry::default(),
            pressed: None,
            keyboard_pressed: false,
            selecting: None,
            popup: None,
            clipboard: Vec::new(),
            activated: Vec::new(),
            text_changes: Vec::new(),
            edit_events: Vec::new(),
            observed_editor_focus: None,
            viewport: Size::ZERO,
            scene: Scene::default(),
            text_layouts: HashMap::new(),
            scene_dirty: true,
            total: FrameStats::default(),
            caret_visible: true,
            custom_scenes: HashMap::new(),
        })
    }
    pub fn root(&self) -> WidgetId {
        self.root
    }
    pub fn theme(&self) -> Theme {
        self.theme
    }
    pub fn commands(&self) -> &CommandRegistry {
        &self.commands
    }
    pub fn commands_mut(&mut self) -> &mut CommandRegistry {
        self.scene_dirty = true;
        &mut self.commands
    }
    pub fn scene(&self) -> &Scene {
        &self.scene
    }
    /// Custom paint in global logical coordinates, inserted at this node's z-order.
    /// Commands are additionally intersected with this node's ancestor clip.
    pub fn set_custom_scene(&mut self, id: WidgetId, scene: Scene) -> Result<(), ControlError> {
        if self.runtime.tree().node(id).is_none() {
            return Err(ControlError::Invalid("Unknown custom paint node".into()));
        }
        if self.custom_scenes.get(&id) != Some(&scene) {
            self.custom_scenes.insert(id, scene);
            self.scene_dirty = true;
        }
        Ok(())
    }
    pub fn stats(&self) -> FrameStats {
        self.total
    }
    /// Called by an optional host timer; there is no timer work without an editable focus.
    pub fn blink_caret(&mut self) -> bool {
        if self
            .runtime
            .focused()
            .and_then(|id| self.document(id))
            .is_none()
        {
            return false;
        }
        self.caret_visible = !self.caret_visible;
        self.scene_dirty = true;
        true
    }
    pub fn needs_update(&self) -> bool {
        self.scene_dirty || self.runtime.needs_update()
    }
    pub fn drain_activated(&mut self) -> Vec<WidgetId> {
        std::mem::take(&mut self.activated)
    }
    pub fn drain_clipboard_requests(&mut self) -> Vec<ClipboardRequest> {
        std::mem::take(&mut self.clipboard)
    }
    pub fn drain_text_changes(&mut self) -> Vec<TextChanged> {
        std::mem::take(&mut self.text_changes)
    }
    fn record_text_change(&mut self, id: WidgetId, before: &str) {
        if let Some(document) = self.document(id)
            && document.text() != before
        {
            self.text_changes.push(TextChanged {
                id,
                text: document.text().to_owned(),
            });
            self.controls.get_mut(&id).unwrap().validation_error = None;
        }
    }
    /// Business editing events, independent of the draft's `TextChanged` queue.
    pub fn drain_edit_events(&mut self) -> Vec<EditEvent> {
        std::mem::take(&mut self.edit_events)
            .into_iter()
            .filter(|event| {
                let id = match event {
                    EditEvent::Committed { id, .. }
                    | EditEvent::Canceled { id }
                    | EditEvent::ValidationFailed { id, .. } => *id,
                };
                self.runtime.tree().node(id).is_some()
            })
            .collect()
    }
    /// Initial or most recently accepted string, excluding the current draft.
    pub fn accepted_value(&self, id: WidgetId) -> Option<&str> {
        self.editor_control(id)
            .ok()
            .map(|c| c.accepted_value.as_str())
    }
    /// Whether the draft differs from its accepted baseline. IME is separate.
    pub fn is_edit_dirty(&self, id: WidgetId) -> Option<bool> {
        Some(self.document(id)?.text() != self.accepted_value(id)?)
    }
    pub fn validation_error(&self, id: WidgetId) -> Option<&str> {
        self.editor_control(id).ok()?.validation_error.as_deref()
    }
    pub fn editor_validation(&self, id: WidgetId) -> Option<&EditorValidation> {
        self.editor_control(id).ok().map(|c| &c.validation)
    }
    fn editor_control(&self, id: WidgetId) -> Result<&Control, ControlError> {
        self.controls
            .get(&id)
            .filter(|c| {
                self.runtime.tree().node(id).is_some() && matches!(c.kind, Kind::TextBox(_))
            })
            .ok_or_else(|| ControlError::Invalid("Control is not a TextBox".into()))
    }
    /// Install built-in validation without validating or changing the baseline.
    /// Rules run on a confirmation attempt, not on every keystroke.
    pub fn set_editor_validation(
        &mut self,
        id: WidgetId,
        validation: EditorValidation,
    ) -> Result<(), ControlError> {
        self.editor_control(id)?;
        validation.validate()?;
        let control = self.controls.get_mut(&id).unwrap();
        if control.validation != validation {
            control.validation = validation;
            control.validation_error = None;
            self.scene_dirty = true;
        }
        Ok(())
    }
    /// Install synchronous application validation after the built-in rules.
    /// Replacing it clears the previous diagnostic, without accepting the draft.
    pub fn set_editor_validator(
        &mut self,
        id: WidgetId,
        validator: Option<EditorValidator>,
    ) -> Result<(), ControlError> {
        self.editor_control(id)?;
        let control = self.controls.get_mut(&id).unwrap();
        control.validator = validator;
        control.validation_error = None;
        self.scene_dirty = true;
        Ok(())
    }
    fn validate_editor_value(&self, id: WidgetId, value: &str) -> Result<(), String> {
        let control = &self.controls[&id];
        control.validation.check(value)?;
        if let Some(validator) = &control.validator {
            validator.check(value)?;
        }
        Ok(())
    }
    /// Validate and accept the draft. Invalid input is an ordinary outcome,
    /// preserving the document, selection, history and accepted value.
    /// Application calls are permitted for read-only/disabled editors; user
    /// keyboard and focus routes are gated by editability.
    pub fn commit_edit(&mut self, id: WidgetId) -> Result<CommitOutcome, ControlError> {
        self.editor_control(id)?;
        let document = self.document(id).unwrap();
        if document.composition().is_some() {
            return Ok(CommitOutcome::CompositionActive);
        }
        let value = document.text().to_owned();
        if let Err(message) = self.validate_editor_value(id, &value) {
            self.controls.get_mut(&id).unwrap().validation_error = Some(message.clone());
            self.edit_events.push(EditEvent::ValidationFailed {
                id,
                message: message.clone(),
            });
            self.scene_dirty = true;
            return Ok(CommitOutcome::Invalid(message));
        }
        let control = self.controls.get_mut(&id).unwrap();
        if control.validation_error.take().is_some() {
            self.scene_dirty = true;
        }
        if control.accepted_value == value {
            return Ok(CommitOutcome::Unchanged);
        }
        control.accepted_value.clone_from(&value);
        self.edit_events.push(EditEvent::Committed { id, value });
        Ok(CommitOutcome::Committed)
    }
    /// Restore the baseline without validating it. Cancelling clears the whole
    /// undo/redo history, preventing Undo from resurrecting a discarded draft.
    /// The caret moves to the end. This is also an application-owned operation.
    pub fn cancel_edit(&mut self, id: WidgetId) -> Result<bool, ControlError> {
        self.editor_control(id)?;
        let control = self.controls.get_mut(&id).unwrap();
        let Kind::TextBox(document) = &mut control.kind else {
            unreachable!()
        };
        if document.text() == control.accepted_value
            && document.composition().is_none()
            && !document.can_undo()
            && !document.can_redo()
            && control.validation_error.is_none()
        {
            return Ok(false);
        }
        let before = document.text().to_owned();
        *document = TextDocument::new(control.accepted_value.clone());
        control.caret = TextCaret {
            index: document.selection().focus,
            ..Default::default()
        };
        control.validation_error = None;
        self.caret_visible = true;
        self.scene_dirty = true;
        self.record_text_change(id, &before);
        self.edit_events.push(EditEvent::Canceled { id });
        Ok(true)
    }
    /// Atomically load and accept an application value after normalization,
    /// input limits and business validation. Invalid leaves every state and
    /// event queue unchanged. Success clears composition and undo/redo history.
    pub fn set_editor_value(
        &mut self,
        id: WidgetId,
        text: &str,
    ) -> Result<CommitOutcome, ControlError> {
        let control = self.editor_control(id)?;
        let value = normalize_editor_text(text, control.single_line).map_err(edit_error)?;
        editor_limits(control.editor.max_length)
            .validate(&value)
            .map_err(edit_error)?;
        if let Err(message) = self.validate_editor_value(id, &value) {
            return Ok(CommitOutcome::Invalid(message));
        }
        let value = value.into_owned();
        let before = self.document(id).unwrap().text().to_owned();
        let control = self.controls.get_mut(&id).unwrap();
        let changed = control.accepted_value != value;
        control.kind = Kind::TextBox(TextDocument::new(value.clone()));
        control.caret = TextCaret {
            index: value.len(),
            ..Default::default()
        };
        control.accepted_value.clone_from(&value);
        control.validation_error = None;
        self.caret_visible = true;
        self.scene_dirty = true;
        self.record_text_change(id, &before);
        if changed {
            self.edit_events.push(EditEvent::Committed { id, value });
            Ok(CommitOutcome::Committed)
        } else {
            Ok(CommitOutcome::Unchanged)
        }
    }
    pub fn document(&self, id: WidgetId) -> Option<&TextDocument> {
        match &self.controls.get(&id)?.kind {
            Kind::TextBox(document) => Some(document),
            _ => None,
        }
    }
    pub fn checked(&self, id: WidgetId) -> Option<bool> {
        self.controls
            .get(&id)
            .filter(|c| matches!(c.kind, Kind::Toggle | Kind::CheckBox))
            .map(|c| c.checked)
    }
    pub fn set_checked(&mut self, id: WidgetId, checked: bool) -> Result<(), ControlError> {
        let control = self
            .controls
            .get_mut(&id)
            .ok_or_else(|| ControlError::Invalid("Unknown control".into()))?;
        control.checked = checked;
        if let Some(command) = &control.command {
            self.commands.set_checked(command, Some(checked));
        }
        self.scene_dirty = true;
        Ok(())
    }
    pub fn set_enabled(&mut self, id: WidgetId, enabled: bool) -> Result<(), ControlError> {
        self.controls
            .get_mut(&id)
            .ok_or_else(|| ControlError::Invalid("Unknown control".into()))?
            .enabled = enabled;
        self.scene_dirty = true;
        self.refresh_paints()?;
        self.synchronize_editor_focus()?;
        self.cancel_inactive_preedit();
        Ok(())
    }
    /// Apply behavior and presentation text as one validated update. Rejection
    /// preserves all settings, committed text, composition, selection and history.
    pub fn set_editor_settings(
        &mut self,
        id: WidgetId,
        settings: EditorSettings,
    ) -> Result<(), ControlError> {
        let Some(Control {
            kind: Kind::TextBox(document),
            editor,
            accepted_value,
            ..
        }) = self.controls.get_mut(&id)
        else {
            return Err(ControlError::Invalid("Control is not a TextBox".into()));
        };
        editor_limits(settings.max_length)
            .validate(document.text())
            .map_err(edit_error)?;
        editor_limits(settings.max_length)
            .validate(accepted_value)
            .map_err(edit_error)?;
        TextLimits::default()
            .validate(&settings.placeholder)
            .map_err(edit_error)?;
        if *editor == settings {
            return Ok(());
        }
        if settings.is_read_only {
            document.cancel_preedit();
        }
        *editor = settings;
        self.scene_dirty = true;
        self.refresh_paints()
    }
    pub fn editor_settings(&self, id: WidgetId) -> Option<&EditorSettings> {
        self.controls
            .get(&id)
            .filter(|c| matches!(c.kind, Kind::TextBox(_)))
            .map(|c| &c.editor)
    }
    /// Theme-relative appearance overrides. Default resets every override.
    /// Invalid metrics or colors are rejected without changing the current style.
    pub fn set_edit_box_style(
        &mut self,
        id: WidgetId,
        style: EditBoxStyle,
    ) -> Result<(), ControlError> {
        let control = self
            .controls
            .get_mut(&id)
            .filter(|c| matches!(c.kind, Kind::TextBox(_)))
            .ok_or_else(|| ControlError::Invalid("Control is not a TextBox".into()))?;
        style.validate()?;
        if control.edit_box_style != style {
            control.edit_box_style = style;
            self.scene_dirty = true;
        }
        Ok(())
    }
    pub fn edit_box_style(&self, id: WidgetId) -> Option<&EditBoxStyle> {
        self.controls
            .get(&id)
            .filter(|c| matches!(c.kind, Kind::TextBox(_)))
            .map(|c| &c.edit_box_style)
    }
    /// Resolves effective ancestor state as well as the editor's local input policy.
    pub fn edit_box_visual_state(&self, id: WidgetId) -> Option<EditBoxVisualState> {
        let settings = self.editor_settings(id)?;
        Some(EditBoxVisualState::from_flags(
            self.is_enabled(id),
            settings.is_read_only,
            self.runtime.focused() == Some(id),
            self.runtime.hovered() == Some(id),
        ))
    }
    /// Prevent user changes while retaining focus, selection, navigation and copy.
    /// Application `set_text` is still allowed and obeys the editor's limits.
    pub fn set_read_only(&mut self, id: WidgetId, read_only: bool) -> Result<(), ControlError> {
        let mut settings = self
            .editor_settings(id)
            .cloned()
            .ok_or_else(|| ControlError::Invalid("Control is not a TextBox".into()))?;
        settings.is_read_only = read_only;
        self.set_editor_settings(id, settings)
    }
    pub fn is_read_only(&self, id: WidgetId) -> Option<bool> {
        self.editor_settings(id).map(|s| s.is_read_only)
    }
    /// Whether this editor can currently accept user text (including ancestor state).
    pub fn is_text_editable(&self, id: WidgetId) -> bool {
        self.is_read_only(id) == Some(false) && self.is_enabled(id)
    }
    /// Maximum extended graphemes in committed text; `None` removes that limit.
    /// `Some(0)` allows only empty text. The independent one-MiB byte cap remains.
    /// A limit below the current length is rejected without truncating text/history.
    pub fn set_max_length(
        &mut self,
        id: WidgetId,
        maximum: Option<usize>,
    ) -> Result<(), ControlError> {
        let mut settings = self
            .editor_settings(id)
            .cloned()
            .ok_or_else(|| ControlError::Invalid("Control is not a TextBox".into()))?;
        settings.max_length = maximum;
        self.set_editor_settings(id, settings)
    }
    /// Outer `None` means the ID is not a text editor; inner `None` means no grapheme limit.
    pub fn max_length(&self, id: WidgetId) -> Option<Option<usize>> {
        self.editor_settings(id).map(|s| s.max_length)
    }
    pub fn set_label(
        &mut self,
        id: WidgetId,
        label: impl Into<String>,
    ) -> Result<(), ControlError> {
        self.controls
            .get_mut(&id)
            .ok_or_else(|| ControlError::Invalid("Unknown control".into()))?
            .label = label.into();
        self.scene_dirty = true;
        Ok(())
    }
    pub fn set_role(&mut self, id: WidgetId, role: Role) {
        if let Some(control) = self.controls.get_mut(&id) {
            control.role = Some(role);
        }
    }
    pub fn set_tooltip(&mut self, id: WidgetId, text: impl Into<String>) {
        if let Some(control) = self.controls.get_mut(&id) {
            control.tooltip = Some(text.into());
        }
    }
    /// Placeholder is presentation only; it never changes the document or its accessible name.
    pub fn set_placeholder(
        &mut self,
        id: WidgetId,
        text: impl Into<String>,
    ) -> Result<(), ControlError> {
        let mut settings = self
            .editor_settings(id)
            .cloned()
            .ok_or_else(|| ControlError::Invalid("Control is not a TextBox".into()))?;
        settings.placeholder = text.into();
        self.set_editor_settings(id, settings)
    }
    pub fn set_theme(&mut self, theme: Theme) -> Result<(), ControlError> {
        if self.theme == theme {
            return Ok(());
        }
        self.theme = theme;
        let height = theme.tokens().control_height;
        let ids: Vec<_> = self
            .controls
            .iter()
            .filter(|(_, c)| c.default_height)
            .map(|(&id, _)| id)
            .collect();
        for id in ids {
            if let Some(node) = self.runtime.tree().node(id) {
                let mut style = node.props.style.clone();
                style.height = Length::Px(height);
                self.runtime.apply(Mutation::SetStyle { node: id, style })?;
            }
        }
        self.runtime.apply(Mutation::SetPaint {
            node: self.root,
            paint: Paint {
                background: Some(theme.tokens().canvas),
                ..Default::default()
            },
        })?;
        self.scene_dirty = true;
        self.refresh_paints()
    }
    fn add(
        &mut self,
        parent: WidgetId,
        kind: Kind,
        label: String,
        command: Option<CommandId>,
        mut style: LayoutStyle,
    ) -> Result<WidgetId, ControlError> {
        let interactive = kind.clickable() || matches!(kind, Kind::TextBox(_));
        let default_height =
            style.height == Length::Auto && !matches!(kind, Kind::Panel | Kind::Scroll { .. });
        if default_height {
            style.height = Length::Px(self.theme.tokens().control_height);
            style.flex_shrink = 0.0;
        }
        let id = self.runtime.insert(
            parent,
            NodeProps {
                style,
                focusable: interactive,
                hit_test: !matches!(kind, Kind::Label),
                clip: matches!(kind, Kind::Panel | Kind::TextBox(_)),
                ..Default::default()
            },
        )?;
        let accepted_value = match &kind {
            Kind::TextBox(document) => document.text().to_owned(),
            _ => String::new(),
        };
        self.controls.insert(
            id,
            Control {
                kind,
                label,
                command,
                enabled: true,
                checked: false,
                role: None,
                tooltip: None,
                editor: EditorSettings::default(),
                accepted_value,
                validation: EditorValidation::default(),
                validator: None,
                validation_error: None,
                edit_box_style: EditBoxStyle::default(),
                single_line: false,
                text_scroll_x: 0.0,
                text_scroll: 0.0,
                default_height,
                transparent_panel: false,
                caret: TextCaret::default(),
            },
        );
        self.scene_dirty = true;
        self.refresh_paints()?;
        Ok(id)
    }
    pub fn panel(
        &mut self,
        parent: WidgetId,
        style: LayoutStyle,
    ) -> Result<WidgetId, ControlError> {
        let style = if style.kind == LayoutKind::Leaf {
            LayoutStyle {
                kind: LayoutKind::Stack(Axis::Vertical),
                ..style
            }
        } else {
            style
        };
        self.add(parent, Kind::Panel, String::new(), None, style)
    }
    /// Creates a layout container with clipping and semantics but no background.
    /// Transparency is preserved across theme and enabled-state changes.
    pub fn transparent_panel(
        &mut self,
        parent: WidgetId,
        style: LayoutStyle,
    ) -> Result<WidgetId, ControlError> {
        let id = self.panel(parent, style)?;
        self.controls
            .get_mut(&id)
            .expect("new panel")
            .transparent_panel = true;
        self.refresh_paints()?;
        Ok(id)
    }
    pub fn label(
        &mut self,
        parent: WidgetId,
        label: impl Into<String>,
        style: LayoutStyle,
    ) -> Result<WidgetId, ControlError> {
        self.add(parent, Kind::Label, label.into(), None, style)
    }
    /// Measures themed text content in logical pixels, using the same system-font
    /// shaping and limits as painting. Width excludes any control padding.
    /// Explicit control dimensions are never changed by this operation.
    pub fn measure_text(
        &mut self,
        text: &str,
        width: f32,
        wrap: bool,
    ) -> Result<Size, ControlError> {
        // Match TextEngine's one-MiB limit before cloning caller-owned input.
        if text.len() > 1024 * 1024 {
            return Err(ControlError::Text(TextError::TextTooLong.to_string()));
        }
        let prepared = self
            .engine
            .prepare(
                &TextRun {
                    bounds: Rect::new(0.0, 0.0, width, 0.0),
                    clip: Rect::new(0.0, 0.0, 0.0, 0.0),
                    text: text.to_owned(),
                    font_size: self.theme.tokens().font_size,
                    color: self.theme.tokens().text,
                    weight: 400,
                    align: TextAlign::Start,
                    wrap,
                },
                1.0,
            )
            .map_err(|e| ControlError::Text(e.to_string()))?;
        Ok(Size::new(prepared.width, prepared.height))
    }
    pub fn button(
        &mut self,
        parent: WidgetId,
        label: impl Into<String>,
        command: Option<CommandId>,
        style: LayoutStyle,
    ) -> Result<WidgetId, ControlError> {
        self.add(parent, Kind::Button, label.into(), command, style)
    }
    pub fn toggle(
        &mut self,
        parent: WidgetId,
        label: impl Into<String>,
        command: Option<CommandId>,
        checked: bool,
        style: LayoutStyle,
    ) -> Result<WidgetId, ControlError> {
        let id = self.add(parent, Kind::Toggle, label.into(), command, style)?;
        self.set_checked(id, checked)?;
        Ok(id)
    }
    pub fn checkbox(
        &mut self,
        parent: WidgetId,
        label: impl Into<String>,
        checked: bool,
        style: LayoutStyle,
    ) -> Result<WidgetId, ControlError> {
        let id = self.add(parent, Kind::CheckBox, label.into(), None, style)?;
        self.set_checked(id, checked)?;
        Ok(id)
    }
    pub fn text_box(
        &mut self,
        parent: WidgetId,
        label: impl Into<String>,
        value: impl Into<String>,
        style: LayoutStyle,
    ) -> Result<WidgetId, ControlError> {
        let value = value.into();
        TextLimits::default().validate(&value).map_err(edit_error)?;
        self.add(
            parent,
            Kind::TextBox(TextDocument::new(value)),
            label.into(),
            None,
            style,
        )
    }
    /// A single-line editor. CR, LF, tab, NEL and Unicode line/paragraph separators
    /// are removed from initial, pasted,
    /// programmatic and committed text; Enter confirms without inserting a newline.
    pub fn edit_box(
        &mut self,
        parent: WidgetId,
        label: impl Into<String>,
        value: impl Into<String>,
        style: LayoutStyle,
    ) -> Result<WidgetId, ControlError> {
        let value = value.into();
        let value = normalize_editor_text(&value, true).map_err(edit_error)?;
        let id = self.text_box(parent, label, value.as_ref(), style)?;
        if let Some(control) = self.controls.get_mut(&id) {
            control.single_line = true;
        }
        Ok(id)
    }
    pub fn scroll_view(
        &mut self,
        parent: WidgetId,
        mut style: LayoutStyle,
        extent: f32,
    ) -> Result<ScrollViewer, ControlError> {
        if !extent.is_finite() || extent < 0.0 {
            return Err(ControlError::Invalid("Invalid scroll extent".into()));
        }
        style.kind = LayoutKind::Overlay;
        let viewport = self.panel(parent, style)?;
        let content = self.panel(
            viewport,
            LayoutStyle {
                kind: LayoutKind::Stack(Axis::Vertical),
                width: Length::Percent(1.0),
                height: Length::Px(extent),
                gap: self.theme.tokens().spacing,
                padding: Edges::all(6.0),
                ..Default::default()
            },
        )?;
        self.controls
            .get_mut(&viewport)
            .ok_or_else(|| ControlError::Invalid("Missing scroll viewport".into()))?
            .kind = Kind::Scroll { content, extent };
        self.edit_props(viewport, |props| {
            props.focusable = true;
            props.clip = true;
        })?;
        Ok(ScrollViewer { viewport, content })
    }
    pub fn tab_control(
        &mut self,
        parent: WidgetId,
        titles: &[&str],
        mut style: LayoutStyle,
    ) -> Result<TabControl, ControlError> {
        style.kind = LayoutKind::Flex(Axis::Vertical);
        let container = self.panel(parent, style)?;
        let header = self.panel(
            container,
            LayoutStyle {
                kind: LayoutKind::Flex(Axis::Horizontal),
                height: Length::Px(self.theme.tokens().control_height),
                flex_shrink: 0.0,
                gap: 4.0,
                ..Default::default()
            },
        )?;
        self.set_role(header, Role::TabList);
        let content = self.panel(
            container,
            LayoutStyle {
                kind: LayoutKind::Overlay,
                height: Length::Px(0.0),
                flex_grow: 1.0,
                ..Default::default()
            },
        )?;
        let mut tabs = Vec::new();
        let mut pages = Vec::new();
        for (index, title) in titles.iter().enumerate() {
            tabs.push(self.add(
                header,
                Kind::Tab {
                    group: container,
                    index,
                },
                (*title).into(),
                None,
                LayoutStyle {
                    flex_grow: 1.0,
                    ..Default::default()
                },
            )?);
            let page = self.panel(
                content,
                LayoutStyle {
                    kind: LayoutKind::Stack(Axis::Vertical),
                    gap: 6.0,
                    padding: Edges::all(8.0),
                    ..Default::default()
                },
            )?;
            self.edit_props(page, |props| props.visible = index == 0)?;
            pages.push(page);
        }
        self.tabs.insert(
            container,
            Tabs {
                tabs: tabs.clone(),
                pages: pages.clone(),
                selected: 0,
            },
        );
        Ok(TabControl {
            container,
            tabs,
            pages,
        })
    }
    pub fn select_tab(&mut self, container: WidgetId, selected: usize) -> Result<(), ControlError> {
        let state = self
            .tabs
            .get_mut(&container)
            .ok_or_else(|| ControlError::Invalid("Unknown tab control".into()))?;
        if selected >= state.pages.len() {
            return Err(ControlError::Invalid("Tab index is out of range".into()));
        }
        state.selected = selected;
        let pages = state.pages.clone();
        for (index, page) in pages.into_iter().enumerate() {
            self.edit_props(page, |props| props.visible = index == selected)?;
        }
        self.scene_dirty = true;
        Ok(())
    }
    pub fn selected_tab(&self, container: WidgetId) -> Option<usize> {
        self.tabs.get(&container).map(|s| s.selected)
    }
    pub fn remove(&mut self, id: WidgetId) -> Result<(), ControlError> {
        self.runtime.apply(Mutation::Remove(id))?;
        self.controls
            .retain(|id, _| self.runtime.tree().node(*id).is_some());
        self.tabs
            .retain(|id, _| self.runtime.tree().node(*id).is_some());
        self.custom_scenes
            .retain(|id, _| self.runtime.tree().node(*id).is_some());
        self.text_changes
            .retain(|event| self.runtime.tree().node(event.id).is_some());
        self.edit_events.retain(|event| {
            let id = match event {
                EditEvent::Committed { id, .. }
                | EditEvent::Canceled { id }
                | EditEvent::ValidationFailed { id, .. } => *id,
            };
            self.runtime.tree().node(id).is_some()
        });
        self.scene_dirty = true;
        Ok(())
    }
    pub fn popup_menu(
        &mut self,
        anchor: WidgetId,
        items: &[MenuItem],
    ) -> Result<WidgetId, ControlError> {
        self.close_popup()?;
        let bounds = self
            .runtime
            .bounds(anchor)
            .ok_or_else(|| ControlError::Invalid("Popup anchor is not laid out".into()))?;
        let height = (items.len() as f32 * self.theme.tokens().control_height + 12.0)
            .min(self.viewport.height.max(1.0));
        let width = 240.0_f32.min(self.viewport.width.max(1.0));
        let viewer = self.scroll_view(
            self.overlay,
            LayoutStyle {
                width: Length::Px(width),
                height: Length::Px(height),
                offset: Point::new(
                    bounds.x.min((self.viewport.width - width).max(0.0)),
                    (bounds.y + bounds.height).min((self.viewport.height - height).max(0.0)),
                ),
                ..Default::default()
            },
            items.len() as f32 * self.theme.tokens().control_height + 12.0,
        )?;
        self.edit_props(viewer.viewport, |props| props.z_index = 1000)?;
        self.set_role(viewer.viewport, Role::Menu);
        self.edit_props(viewer.content, |props| props.style.gap = 0.0)?;
        let mut ids = Vec::new();
        for item in items {
            if item.toggle && self.commands.checked(&item.command).is_none() {
                self.commands.set_checked(&item.command, Some(false));
            }
            ids.push(self.add(
                viewer.content,
                Kind::MenuItem {
                    toggle: item.toggle,
                },
                item.label.clone(),
                Some(item.command.clone()),
                LayoutStyle::default(),
            )?);
        }
        self.popup = Some(Popup {
            container: viewer.viewport,
            anchor,
            items: ids.clone(),
        });
        self.update(self.viewport)?;
        if let Some(id) = ids.into_iter().find(|&id| self.is_enabled(id)) {
            self.runtime.apply(Mutation::Focus(Some(id)))?;
        }
        self.scene_dirty = true;
        Ok(viewer.viewport)
    }
    pub fn close_popup(&mut self) -> Result<(), ControlError> {
        if let Some(popup) = self.popup.take() {
            self.remove(popup.container)?;
            if self.runtime.tree().node(popup.anchor).is_some() && self.is_enabled(popup.anchor) {
                self.runtime.apply(Mutation::Focus(Some(popup.anchor)))?;
            }
        }
        Ok(())
    }
    pub fn popup_open(&self) -> bool {
        self.popup.is_some()
    }
    fn edit_props(
        &mut self,
        id: WidgetId,
        edit: impl FnOnce(&mut NodeProps),
    ) -> Result<(), ControlError> {
        let mut props = self
            .runtime
            .tree()
            .node(id)
            .ok_or_else(|| ControlError::Invalid("Unknown node".into()))?
            .props
            .clone();
        edit(&mut props);
        self.runtime.apply(Mutation::SetProps { node: id, props })?;
        Ok(())
    }
    fn is_enabled(&self, id: WidgetId) -> bool {
        if !self.controls.get(&id).is_some_and(|c| {
            c.enabled
                && c.command
                    .as_ref()
                    .is_none_or(|id| self.commands.enabled(id))
        }) {
            return false;
        }
        self.effective_enabled(id)
    }
    fn effective_enabled(&self, mut id: WidgetId) -> bool {
        loop {
            let Some(node) = self.runtime.tree().node(id) else {
                return false;
            };
            if !node.props.enabled || !node.props.visible {
                return false;
            }
            let Some(parent) = self.runtime.tree().parent(id) else {
                return true;
            };
            id = parent;
        }
    }
    fn refresh_paints(&mut self) -> Result<(), ControlError> {
        let tokens = self.theme.tokens();
        // Use the current control/command state for every ancestor, independent
        // of HashMap iteration order. Store only local enabled state on nodes so
        // re-enabling a container also restores its enabled descendants.
        let local_enabled: HashMap<_, _> = self
            .controls
            .iter()
            .map(|(&id, control)| {
                (
                    id,
                    control.enabled
                        && control
                            .command
                            .as_ref()
                            .is_none_or(|command| self.commands.enabled(command)),
                )
            })
            .collect();
        let mut updates = Vec::new();
        for (&id, control) in &mut self.controls {
            let Some(node) = self.runtime.tree().node(id) else {
                continue;
            };
            let enabled = local_enabled[&id];
            let mut paint_enabled = enabled;
            let mut parent = self.runtime.tree().parent(id);
            while let Some(ancestor) = parent {
                let Some(ancestor_node) = self.runtime.tree().node(ancestor) else {
                    break;
                };
                paint_enabled &= local_enabled
                    .get(&ancestor)
                    .copied()
                    .unwrap_or(ancestor_node.props.enabled);
                if !paint_enabled {
                    break;
                }
                parent = self.runtime.tree().parent(ancestor);
            }
            if let Some(checked) = control
                .command
                .as_ref()
                .and_then(|id| self.commands.checked(id))
            {
                control.checked = checked;
            }
            let selected = match control.kind {
                Kind::Tab { group, index } => {
                    self.tabs.get(&group).is_some_and(|t| t.selected == index)
                }
                _ => control.checked,
            };
            let interactive = control.kind.clickable() || matches!(control.kind, Kind::TextBox(_));
            let paint = if matches!(control.kind, Kind::Label) || control.transparent_panel {
                Paint::default()
            } else {
                Paint {
                    background: Some(if !paint_enabled || control.editor.is_read_only {
                        tokens.canvas
                    } else if self.pressed == Some(id) {
                        tokens.pressed
                    } else if selected {
                        tokens.selected
                    } else {
                        tokens.surface
                    }),
                    border: interactive.then_some(Border {
                        width: 1.0,
                        color: tokens.border,
                    }),
                    hover_background: (interactive
                        && paint_enabled
                        && !control.editor.is_read_only)
                        .then_some(if self.pressed == Some(id) {
                            tokens.pressed
                        } else {
                            tokens.hover
                        }),
                    focus_border: interactive.then_some(Border {
                        width: 2.0,
                        color: tokens.focus,
                    }),
                }
            };
            let mut props = node.props.clone();
            props.enabled = enabled;
            props.paint = paint;
            if props != node.props {
                updates.push((id, props));
            }
        }
        for (node, props) in updates {
            self.runtime.apply(Mutation::SetProps { node, props })?;
        }
        if self.pressed.is_some_and(|id| !self.is_enabled(id)) {
            self.pressed = None;
            self.keyboard_pressed = false;
        }
        Ok(())
    }
    pub fn update(&mut self, viewport: Size) -> Result<FrameStats, ControlError> {
        self.refresh_paints()?;
        self.synchronize_editor_focus()?;
        self.cancel_inactive_preedit();
        let mut delta = self.runtime.update(&mut self.layout, viewport)?;
        self.viewport = viewport;
        let scrolls: Vec<_> = self
            .controls
            .iter()
            .filter_map(|(&id, c)| matches!(c.kind, Kind::Scroll { .. }).then_some(id))
            .collect();
        for id in scrolls {
            let value = self
                .runtime
                .tree()
                .node(id)
                .map_or(0.0, |n| n.props.scroll.y);
            self.scroll_to(id, value)?;
        }
        if self.runtime.needs_update() {
            add_stats(&mut delta, self.runtime.update(&mut self.layout, viewport)?);
        }
        if delta.paint_passes > 0 || self.scene_dirty {
            self.rebuild_scene()?;
            if delta.paint_passes == 0 {
                delta.paint_passes = 1;
                delta.painted_nodes = self.controls.len() as u64;
            }
            self.scene_dirty = false;
        }
        add_stats(&mut self.total, delta);
        Ok(delta)
    }
    pub fn dispatch(&mut self, event: InputEvent) -> Result<(), ControlError> {
        self.update(self.viewport)?;
        self.caret_visible = true;
        if let InputEvent::KeyDown {
            key: Key::Escape,
            modifiers,
            repeat,
        } = event
        {
            if repeat || modifiers != rust_desktop_ui_core::Modifiers::default() {
                return Ok(());
            }
            if self.popup.is_some() {
                self.close_popup()?;
            } else if let Some(id) = self
                .runtime
                .focused()
                .filter(|&id| self.is_text_editable(id))
            {
                if self
                    .document(id)
                    .is_some_and(|document| document.composition().is_some())
                {
                    self.cancel_preedit();
                } else {
                    self.cancel_edit(id)?;
                }
            }
            return Ok(());
        }
        if self.popup.is_some() && self.popup_key(&event)? {
            return Ok(());
        }
        if let InputEvent::PointerDown { position, .. } = &event
            && let Some(popup) = &self.popup
            && !self
                .runtime
                .bounds(popup.container)
                .is_some_and(|r| r.contains(position.x, position.y))
        {
            self.close_popup()?;
            self.update(self.viewport)?;
        }
        let hit = match &event {
            InputEvent::PointerDown { position, .. }
            | InputEvent::PointerUp { position, .. }
            | InputEvent::Wheel { position, .. } => self.runtime.hit_test(*position),
            _ => None,
        };
        let report = self.runtime.dispatch(event.clone());
        if let Some(error) = report.errors.into_iter().next() {
            return Err(error.into());
        }
        self.synchronize_editor_focus()?;
        self.cancel_inactive_preedit();
        match event {
            InputEvent::PointerDown {
                position,
                button: PointerButton::Primary,
                modifiers,
            } => {
                if let Some(id) =
                    hit.filter(|&id| self.is_enabled(id) && self.runtime.focused() == Some(id))
                {
                    if self.controls.get(&id).is_some_and(|c| c.kind.clickable()) {
                        self.pressed = Some(id);
                        self.keyboard_pressed = false;
                        self.runtime.apply(Mutation::CapturePointer(Some(id)))?;
                    } else if matches!(
                        self.controls.get(&id).map(|c| &c.kind),
                        Some(Kind::TextBox(_))
                    ) {
                        self.place_caret(id, position, modifiers.shift)?;
                        self.selecting = Some(id);
                        self.runtime.apply(Mutation::CapturePointer(Some(id)))?;
                    }
                }
                self.scene_dirty = true;
            }
            InputEvent::PointerMoved { position } => {
                if let Some(id) = self.selecting {
                    self.place_caret(id, position, true)?;
                }
            }
            InputEvent::PointerUp {
                button: PointerButton::Primary,
                ..
            } => {
                self.selecting = None;
                if let Some(id) = self.pressed.take()
                    && !self.keyboard_pressed
                    && hit == Some(id)
                    && self.is_enabled(id)
                {
                    self.activate(id)?;
                }
                self.keyboard_pressed = false;
                self.scene_dirty = true;
            }
            InputEvent::KeyDown {
                key,
                modifiers,
                repeat,
            } => {
                if let Some(id) = self.runtime.focused().filter(|&id| self.is_enabled(id)) {
                    if matches!(
                        self.controls.get(&id).map(|c| &c.kind),
                        Some(Kind::TextBox(_))
                    ) {
                        self.edit_key(id, &key, modifiers, repeat)?;
                    } else if self.controls.get(&id).is_some_and(|c| c.kind.clickable()) {
                        if key == Key::Enter && !repeat {
                            self.activate(id)?;
                        }
                        if key == Key::Space && !repeat {
                            self.pressed = Some(id);
                            self.keyboard_pressed = true;
                            self.scene_dirty = true;
                        }
                        if matches!(key, Key::ArrowLeft | Key::ArrowRight) {
                            self.tab_arrow(id, key == Key::ArrowRight)?;
                        }
                    } else if matches!(
                        self.controls.get(&id).map(|c| &c.kind),
                        Some(Kind::Scroll { .. })
                    ) {
                        let current = self
                            .runtime
                            .tree()
                            .node(id)
                            .map_or(0.0, |n| n.props.scroll.y);
                        let desired = match key {
                            Key::ArrowDown => Some(current + 40.0),
                            Key::ArrowUp => Some(current - 40.0),
                            Key::Home => Some(0.0),
                            Key::End => Some(f32::MAX),
                            Key::PageDown => Some(current + self.viewport.height * 0.8),
                            Key::PageUp => Some(current - self.viewport.height * 0.8),
                            _ => None,
                        };
                        if let Some(desired) = desired {
                            self.scroll_to(id, desired)?;
                        }
                    }
                }
            }
            InputEvent::KeyUp {
                key: Key::Space, ..
            } => {
                if let Some(id) = self.pressed.take()
                    && self.keyboard_pressed
                    && self.runtime.focused() == Some(id)
                    && self.is_enabled(id)
                {
                    self.activate(id)?;
                }
                self.keyboard_pressed = false;
                self.scene_dirty = true;
            }
            InputEvent::Wheel { delta, .. } => {
                if let Some(id) = hit.and_then(|id| self.scroll_ancestor(id)) {
                    let current = self
                        .runtime
                        .tree()
                        .node(id)
                        .map_or(0.0, |n| n.props.scroll.y);
                    self.scroll_to(id, current + delta.y)?;
                }
            }
            InputEvent::WindowFocused(false) => {
                self.pressed = None;
                self.selecting = None;
                self.keyboard_pressed = false;
                self.cancel_preedit();
                self.close_popup()?;
                self.scene_dirty = true;
            }
            _ => {}
        }
        Ok(())
    }
    pub fn activate(&mut self, id: WidgetId) -> Result<(), ControlError> {
        if !self.is_enabled(id) {
            return Ok(());
        }
        let Some(control) = self.controls.get_mut(&id) else {
            return Ok(());
        };
        if !control.kind.clickable() {
            return Ok(());
        }
        if matches!(
            control.kind,
            Kind::Toggle | Kind::CheckBox | Kind::MenuItem { toggle: true }
        ) {
            control.checked = !control.checked;
            if let Some(command) = &control.command {
                self.commands.set_checked(command, Some(control.checked));
            }
        }
        let tab = match control.kind {
            Kind::Tab { group, index } => Some((group, index)),
            _ => None,
        };
        let menu = matches!(control.kind, Kind::MenuItem { .. });
        if let Some(command) = &control.command {
            self.commands.execute(command);
        }
        self.activated.push(id);
        self.scene_dirty = true;
        if let Some((group, index)) = tab {
            self.select_tab(group, index)?;
        }
        if menu {
            self.close_popup()?;
        }
        Ok(())
    }
    fn tab_arrow(&mut self, id: WidgetId, forward: bool) -> Result<(), ControlError> {
        let Some(Control {
            kind: Kind::Tab { group, index },
            ..
        }) = self.controls.get(&id)
        else {
            return Ok(());
        };
        let (group, index) = (*group, *index);
        let Some(tabs) = self.tabs.get(&group) else {
            return Ok(());
        };
        if tabs.tabs.is_empty() {
            return Ok(());
        }
        let next = if forward {
            (index + 1) % tabs.tabs.len()
        } else {
            (index + tabs.tabs.len() - 1) % tabs.tabs.len()
        };
        let next_id = tabs.tabs[next];
        self.select_tab(group, next)?;
        self.runtime.apply(Mutation::Focus(Some(next_id)))?;
        Ok(())
    }
    fn popup_key(&mut self, event: &InputEvent) -> Result<bool, ControlError> {
        let InputEvent::KeyDown { key, .. } = event else {
            return Ok(false);
        };
        if *key == Key::Tab {
            self.close_popup()?;
            return Ok(false);
        }
        if !matches!(key, Key::ArrowDown | Key::ArrowUp | Key::Home | Key::End) {
            return Ok(false);
        }
        let Some(popup) = &self.popup else {
            return Ok(false);
        };
        let items: Vec<_> = popup
            .items
            .iter()
            .copied()
            .filter(|&id| self.is_enabled(id))
            .collect();
        if items.is_empty() {
            return Ok(true);
        }
        let current = items
            .iter()
            .position(|id| Some(*id) == self.runtime.focused())
            .unwrap_or(0);
        let next = match key {
            Key::Home => 0,
            Key::End => items.len() - 1,
            Key::ArrowDown => (current + 1) % items.len(),
            _ => (current + items.len() - 1) % items.len(),
        };
        let id = items[next];
        let viewport = popup.container;
        self.runtime.apply(Mutation::Focus(Some(id)))?;
        if let (Some(bounds), Some(clip)) = (self.runtime.bounds(id), self.runtime.bounds(viewport))
        {
            let current = self
                .runtime
                .tree()
                .node(viewport)
                .map_or(0.0, |n| n.props.scroll.y);
            if bounds.y < clip.y {
                self.scroll_to(viewport, current + bounds.y - clip.y)?;
            } else if bounds.y + bounds.height > clip.y + clip.height {
                self.scroll_to(
                    viewport,
                    current + bounds.y + bounds.height - clip.y - clip.height,
                )?;
            }
        }
        self.scene_dirty = true;
        Ok(true)
    }
    fn scroll_ancestor(&self, mut id: WidgetId) -> Option<WidgetId> {
        loop {
            if matches!(
                self.controls.get(&id).map(|c| &c.kind),
                Some(Kind::Scroll { .. })
            ) {
                return Some(id);
            }
            id = self.runtime.tree().parent(id)?;
        }
    }
    pub fn scroll_to(&mut self, id: WidgetId, desired: f32) -> Result<(), ControlError> {
        let Some(Control {
            kind: Kind::Scroll { extent, .. },
            ..
        }) = self.controls.get(&id)
        else {
            return Ok(());
        };
        if !desired.is_finite() {
            return Err(ControlError::Invalid("Scroll must be finite".into()));
        }
        let height = self.runtime.bounds(id).map_or(0.0, |r| r.height);
        let next = desired.clamp(0.0, (*extent - height).max(0.0));
        self.edit_props(id, |props| props.scroll.y = next)
    }
    pub fn set_scroll_extent(&mut self, id: WidgetId, extent: f32) -> Result<(), ControlError> {
        if !extent.is_finite() || extent < 0.0 {
            return Err(ControlError::Invalid("Invalid scroll extent".into()));
        }
        let Some(Control {
            kind: Kind::Scroll {
                content,
                extent: old,
            },
            ..
        }) = self.controls.get_mut(&id)
        else {
            return Err(ControlError::Invalid("Unknown scroll viewer".into()));
        };
        *old = extent;
        let content = *content;
        self.edit_props(content, |props| props.style.height = Length::Px(extent))
    }
    fn place_caret(
        &mut self,
        id: WidgetId,
        point: Point,
        extend: bool,
    ) -> Result<(), ControlError> {
        if self
            .document(id)
            .is_some_and(|document| document.composition().is_some())
        {
            self.cancel_preedit();
            self.rebuild_scene()?;
        }
        let Some(layout) = self.text_layouts.get(&id) else {
            return Ok(());
        };
        let next_caret = layout.hit_test_caret(point);
        let index = next_caret.index;
        if let Some(Control {
            kind: Kind::TextBox(document),
            caret,
            ..
        }) = self.controls.get_mut(&id)
        {
            let anchor = if extend {
                document.selection().anchor
            } else {
                index
            };
            document
                .set_selection(anchor, index)
                .map_err(|e| ControlError::Text(e.to_string()))?;
            *caret = next_caret;
            document.cancel_preedit();
            self.scene_dirty = true;
        }
        Ok(())
    }
    fn edit_key(
        &mut self,
        id: WidgetId,
        key: &Key,
        modifiers: rust_desktop_ui_core::Modifiers,
        repeat: bool,
    ) -> Result<(), ControlError> {
        let shortcut = (modifiers.control && !modifiers.alt) || modifiers.super_key;
        let confirm_key = *key == Key::Enter
            && !repeat
            && !modifiers.shift
            && !modifiers.alt
            && self.controls.get(&id).is_some_and(|control| {
                if control.single_line {
                    !modifiers.control && !modifiers.super_key
                } else {
                    shortcut
                }
            });
        if confirm_key && self.is_text_editable(id) {
            self.commit_edit(id)?;
            return Ok(());
        }
        let Some(Control {
            kind: Kind::TextBox(document),
            caret,
            single_line,
            editor:
                EditorSettings {
                    is_read_only: read_only,
                    max_length,
                    ..
                },
            ..
        }) = self.controls.get_mut(&id)
        else {
            return Ok(());
        };
        if document.composition().is_some() {
            return Ok(());
        }
        // Ctrl+Alt is commonly AltGr, whose printable text arrives separately.
        if matches!(
            key,
            Key::ArrowUp | Key::ArrowDown | Key::PageUp | Key::PageDown
        ) && let Some(layout) = self.text_layouts.get(&id)
        {
            if caret.index != document.selection().focus {
                *caret = TextCaret {
                    index: document.selection().focus,
                    ..Default::default()
                };
            }
            let geometry = layout.caret_geometry(*caret);
            let direction = if matches!(key, Key::ArrowUp | Key::PageUp) {
                -1.0
            } else {
                1.0
            };
            let distance = if matches!(key, Key::PageUp | Key::PageDown) {
                self.runtime
                    .bounds(id)
                    .map_or(geometry.height, |r| r.height)
            } else {
                geometry.height
            };
            *caret = layout.hit_test_caret(Point::new(
                geometry.x,
                geometry.y + geometry.height * 0.5 + direction * distance,
            ));
            let anchor = if modifiers.shift {
                document.selection().anchor
            } else {
                caret.index
            };
            document
                .set_selection(anchor, caret.index)
                .map_err(|e| ControlError::Text(e.to_string()))?;
            self.scene_dirty = true;
            return Ok(());
        }
        if matches!(key, Key::ArrowLeft | Key::ArrowRight)
            && !shortcut
            && (modifiers.shift || document.selection().anchor == document.selection().focus)
            && let Some(layout) = self.text_layouts.get(&id)
        {
            if caret.index != document.selection().focus {
                *caret = TextCaret {
                    index: document.selection().focus,
                    ..Default::default()
                };
            }
            *caret = layout.move_caret_visual(*caret, *key == Key::ArrowRight);
            let anchor = if modifiers.shift {
                document.selection().anchor
            } else {
                caret.index
            };
            document
                .set_selection(anchor, caret.index)
                .map_err(|e| ControlError::Text(e.to_string()))?;
            self.scene_dirty = true;
            return Ok(());
        }
        let before = document.text().to_owned();
        let selection_before = document.selection();
        let limits = editor_limits(*max_length);
        match key {
            Key::ArrowLeft if shortcut => document.move_word_left(modifiers.shift),
            Key::ArrowRight if shortcut => document.move_word_right(modifiers.shift),
            Key::ArrowLeft => document.move_left(modifiers.shift),
            Key::ArrowRight => document.move_right(modifiers.shift),
            Key::Home if shortcut => document.move_document_start(modifiers.shift),
            Key::End if shortcut => document.move_document_end(modifiers.shift),
            Key::Home => document.move_home(modifiers.shift),
            Key::End => document.move_end(modifiers.shift),
            Key::Backspace if !*read_only => {
                document.backspace();
            }
            Key::Delete if !*read_only => {
                document.delete_forward();
            }
            Key::Enter
                if !*single_line
                    && !*read_only
                    && !modifiers.control
                    && !modifiers.super_key
                    && !modifiers.alt =>
            {
                if let Err(error) = document.try_insert("\n", limits) {
                    return user_edit(Err(error));
                }
            }
            Key::Character(value) if shortcut => match value.to_lowercase().as_str() {
                "a" => document.select_all(),
                "c" if !repeat => self
                    .clipboard
                    .push(ClipboardRequest::Copy(document.selected_text().to_owned())),
                "x" if !repeat && !*read_only => {
                    self.clipboard
                        .push(ClipboardRequest::Copy(document.selected_text().to_owned()));
                    document.insert("");
                }
                "v" if !repeat && !*read_only => self.clipboard.push(ClipboardRequest::Paste),
                "z" if !repeat && modifiers.shift && !*read_only => {
                    if let Err(error) = document.try_redo(limits) {
                        return user_edit(Err(error));
                    }
                }
                "z" if !repeat && !*read_only => {
                    if let Err(error) = document.try_undo(limits) {
                        return user_edit(Err(error));
                    }
                }
                "y" if !repeat && !*read_only => {
                    if let Err(error) = document.try_redo(limits) {
                        return user_edit(Err(error));
                    }
                }
                _ => {}
            },
            _ => {}
        }
        if document.selection() != selection_before || document.text() != before {
            *caret = TextCaret {
                index: document.selection().focus,
                ..Default::default()
            };
        }
        self.scene_dirty = true;
        self.record_text_change(id, &before);
        Ok(())
    }
    /// Insert user text or paste. Over-limit input is rejected as a whole.
    pub fn text_input(&mut self, text: &str) -> Result<(), ControlError> {
        self.synchronize_editor_focus()?;
        self.caret_visible = true;
        if let Some(id) = self
            .runtime
            .focused()
            .filter(|&id| self.is_text_editable(id))
            && let Some(Control {
                kind: Kind::TextBox(document),
                single_line,
                editor: EditorSettings { max_length, .. },
                caret,
                ..
            }) = self.controls.get_mut(&id)
        {
            let normalized = match normalize_editor_text(text, *single_line) {
                Ok(text) => text,
                Err(error) => return user_edit(Err(error)),
            };
            // Empty/filtered-only input must not erase the current selection.
            if normalized.is_empty() {
                return Ok(());
            }
            let before = document.text().to_owned();
            if let Err(error) = document.try_insert(&normalized, editor_limits(*max_length)) {
                return user_edit(Err(error));
            }
            *caret = TextCaret {
                index: document.selection().focus,
                ..Default::default()
            };
            self.scene_dirty = true;
            self.record_text_change(id, &before);
        }
        Ok(())
    }
    pub fn ime_preedit(
        &mut self,
        text: &str,
        cursor: Option<(usize, usize)>,
    ) -> Result<(), ControlError> {
        self.synchronize_editor_focus()?;
        self.caret_visible = true;
        if let Some(id) = self
            .runtime
            .focused()
            .filter(|&id| self.is_text_editable(id))
            && let Some(Control {
                kind: Kind::TextBox(document),
                single_line,
                editor: EditorSettings { max_length, .. },
                ..
            }) = self.controls.get_mut(&id)
        {
            if *single_line && text.chars().any(single_line_separator) {
                // Cursor offsets refer to the original preedit, so do not filter it.
                document.cancel_preedit();
                self.scene_dirty = true;
                return Ok(());
            }
            user_edit(document.try_set_preedit(text, cursor, editor_limits(*max_length)))?;
            self.scene_dirty = true;
        }
        Ok(())
    }
    pub fn ime_commit(&mut self, text: &str) -> Result<(), ControlError> {
        self.synchronize_editor_focus()?;
        self.caret_visible = true;
        if let Some(id) = self
            .runtime
            .focused()
            .filter(|&id| self.is_text_editable(id))
            && let Some(Control {
                kind: Kind::TextBox(document),
                single_line,
                editor: EditorSettings { max_length, .. },
                caret,
                ..
            }) = self.controls.get_mut(&id)
        {
            let before = document.text().to_owned();
            let result = normalize_editor_text(text, *single_line).and_then(|text| {
                if text.is_empty() {
                    Ok(())
                } else {
                    document.try_commit(&text, editor_limits(*max_length))
                }
            });
            // The IME has ended its transaction even when its result is rejected.
            document.cancel_preedit();
            self.scene_dirty = true;
            if let Err(error) = result {
                return user_edit(Err(error));
            }
            *caret = TextCaret {
                index: document.selection().focus,
                ..Default::default()
            };
            self.record_text_change(id, &before);
            self.scene_dirty = true;
        }
        Ok(())
    }
    pub fn cancel_preedit(&mut self) {
        for control in self.controls.values_mut() {
            if let Kind::TextBox(document) = &mut control.kind {
                document.cancel_preedit();
            }
        }
        self.scene_dirty = true;
    }
    // Also observes public runtime focus mutations at the next Controls boundary.
    // A disappearing, hidden or disabled editor is never implicitly accepted.
    fn synchronize_editor_focus(&mut self) -> Result<(), ControlError> {
        let current = self
            .runtime
            .focused()
            .filter(|&id| self.document(id).is_some());
        let previous = self.observed_editor_focus;
        self.observed_editor_focus = current;
        if current != previous
            && let Some(id) = previous
        {
            let composing = self
                .document(id)
                .is_some_and(|document| document.composition().is_some());
            if composing {
                if let Some(Control {
                    kind: Kind::TextBox(document),
                    ..
                }) = self.controls.get_mut(&id)
                {
                    document.cancel_preedit();
                    self.scene_dirty = true;
                }
            } else if self.is_text_editable(id)
                && self
                    .editor_settings(id)
                    .is_some_and(|settings| settings.commit_on_focus_lost)
            {
                self.commit_edit(id)?;
            }
        }
        Ok(())
    }
    fn cancel_inactive_preedit(&mut self) {
        let focused = self
            .runtime
            .focused()
            .filter(|&id| self.is_text_editable(id));
        for (&id, control) in &mut self.controls {
            if Some(id) != focused
                && let Kind::TextBox(document) = &mut control.kind
                && document.composition().is_some()
            {
                document.cancel_preedit();
                self.scene_dirty = true;
            }
        }
    }
    pub fn caret_bounds(&self) -> Option<Rect> {
        let id = self.runtime.focused()?;
        if !self.is_enabled(id) {
            return None;
        }
        let document = self.document(id)?;
        let mut caret = self.controls.get(&id)?.caret;
        if caret.index != document.display_selection().focus {
            caret = TextCaret {
                index: document.display_selection().focus,
                ..Default::default()
            };
        }
        self.text_layouts
            .get(&id)
            .map(|layout| layout.caret_geometry(caret))
    }
    /// Application-owned, undoable draft update, allowed for disabled/read-only
    /// editors. Does not accept the business value; use `set_editor_value` to load
    /// a value or `commit_edit` to confirm. Input limit failures are atomic.
    pub fn set_text(&mut self, id: WidgetId, text: &str) -> Result<(), ControlError> {
        let Some(Control {
            kind: Kind::TextBox(document),
            single_line,
            editor: EditorSettings { max_length, .. },
            caret,
            ..
        }) = self.controls.get_mut(&id)
        else {
            return Err(ControlError::Invalid("Control is not a TextBox".into()));
        };
        let text = normalize_editor_text(text, *single_line).map_err(edit_error)?;
        if text == document.text() {
            return Ok(());
        }
        let before = document.text().to_owned();
        document
            .try_set_text(&text, editor_limits(*max_length))
            .map_err(edit_error)?;
        *caret = TextCaret {
            index: document.selection().focus,
            ..Default::default()
        };
        self.scene_dirty = true;
        self.record_text_change(id, &before);
        Ok(())
    }
    fn rebuild_scene(&mut self) -> Result<(), ControlError> {
        self.scene.clear();
        self.text_layouts.clear();
        let tokens = self.theme.tokens();
        let mut pending = vec![self.overlay];
        while let Some(id) = pending.pop() {
            let Some(node) = self.runtime.tree().node(id) else {
                continue;
            };
            let mut children = self
                .runtime
                .tree()
                .children(id)
                .unwrap_or_default()
                .to_vec();
            children.sort_by_key(|child| {
                self.runtime
                    .tree()
                    .node(*child)
                    .map_or(0, |n| n.props.z_index)
            });
            pending.extend(children.into_iter().rev());
            let (Some(bounds), Some(clip)) = (self.runtime.bounds(id), self.runtime.clip(id))
            else {
                continue;
            };
            let paint = node.props.paint;
            let editor_style = self.edit_box_visual_state(id).map(|state| {
                let control = &self.controls[&id];
                let mut style = control.edit_box_style.resolve(self.theme, state);
                if state != EditBoxVisualState::Disabled && control.validation_error.is_some() {
                    style.apply_invalid(self.theme, control.edit_box_style.invalid);
                }
                style
            });
            let color = if self.runtime.hovered() == Some(id) {
                paint.hover_background.or(paint.background)
            } else {
                paint.background
            };
            if let Some(style) = editor_style {
                let underline = self.theme.is_fluent();
                let width = if style.focus_visible && !underline {
                    style.border_width.max(2.0)
                } else {
                    style.border_width
                };
                let border_color = if style.focus_visible && !underline {
                    style.accent
                } else {
                    style.border
                };
                let inset = width.min(bounds.width * 0.5).min(bounds.height * 0.5);
                let radius = style
                    .corner_radius
                    .min(bounds.width.min(bounds.height) * 0.5);
                // Background covers the whole shape; the border is a separate
                // ring so translucent colors blend without filling its center.
                editor_fill(&mut self.scene, bounds, clip, radius, style.background);
                if inset > 0.0 {
                    if radius > 0.0 {
                        self.scene
                            .rounded_stroke(bounds, clip, radius, inset, border_color);
                    } else {
                        draw_border(
                            &mut self.scene,
                            bounds,
                            clip,
                            Border {
                                width: inset,
                                color: border_color,
                            },
                        );
                    }
                }
                if style.focus_visible && underline {
                    let indent = radius.max(2.0).min((bounds.width - 2.0).max(0.0) * 0.5);
                    let thickness = 2.0_f32.min(bounds.height);
                    fill_clipped(
                        &mut self.scene,
                        Rect::new(
                            bounds.x + indent,
                            bounds.y + bounds.height - thickness,
                            (bounds.width - indent * 2.0).max(0.0),
                            thickness,
                        ),
                        clip,
                        style.accent,
                    );
                }
            } else {
                if let Some(color) = color {
                    fill_clipped(&mut self.scene, bounds, clip, color);
                }
                if let Some(border) = paint.border {
                    draw_border(&mut self.scene, bounds, clip, border);
                }
                if self.runtime.focused() == Some(id)
                    && let Some(border) = paint.focus_border
                {
                    draw_border(&mut self.scene, bounds, clip, border);
                }
            }
            if let Some(custom) = self.custom_scenes.get(&id) {
                append_clipped(&mut self.scene, custom, clip);
            }
            let enabled = self.effective_enabled(id);
            let Some(control) = self.controls.get_mut(&id) else {
                continue;
            };
            if matches!(control.kind, Kind::Panel | Kind::Scroll { .. }) {
                // Container labels name semantic nodes; their content is composed by children/custom scenes.
                continue;
            }
            let vertical_padding = match control.kind {
                Kind::Label => 0.0,
                Kind::TextBox(_) => 5.0,
                _ => ((bounds.height - tokens.font_size * 1.4) * 0.5).clamp(0.0, 5.0),
            };
            let padding = editor_style.map_or(
                Edges {
                    left: 8.0,
                    right: 8.0,
                    top: vertical_padding,
                    bottom: vertical_padding,
                },
                |style| style.padding,
            );
            let mut text_bounds = Rect::new(
                bounds.x + padding.left.min(bounds.width),
                bounds.y + padding.top.min(bounds.height),
                (bounds.width - padding.horizontal()).max(0.0),
                (bounds.height - padding.vertical()).max(0.0),
            );
            let mut label = control.label.clone();
            let editable = matches!(control.kind, Kind::TextBox(_));
            let align = if control.kind.clickable()
                && !matches!(control.kind, Kind::CheckBox | Kind::MenuItem { .. })
            {
                TextAlign::Center
            } else {
                TextAlign::Start
            };
            if let Kind::TextBox(document) = &control.kind {
                label = document.display_text();
            }
            if matches!(
                control.kind,
                Kind::CheckBox | Kind::MenuItem { toggle: true }
            ) {
                let check = Rect::new(
                    bounds.x + 8.0,
                    bounds.y + (bounds.height - 16.0) * 0.5,
                    16.0,
                    16.0,
                );
                draw_border(
                    &mut self.scene,
                    check,
                    clip,
                    Border {
                        width: 1.0,
                        color: tokens.border,
                    },
                );
                if control.checked {
                    fill_clipped(
                        &mut self.scene,
                        Rect::new(check.x + 3.0, check.y + 3.0, 10.0, 10.0),
                        clip,
                        tokens.focus,
                    );
                }
                text_bounds.x += 24.0;
                text_bounds.width = (text_bounds.width - 24.0).max(0.0);
            }
            let mut run = TextRun {
                bounds: text_bounds,
                clip: text_bounds
                    .intersection(clip)
                    .unwrap_or(Rect::new(0.0, 0.0, 0.0, 0.0)),
                text: label,
                font_size: editor_style.map_or(tokens.font_size, |style| style.font_size),
                color: editor_style
                    .map_or(if enabled { tokens.text } else { tokens.muted }, |style| {
                        style.foreground
                    }),
                weight: if matches!(control.kind, Kind::Tab { .. }) {
                    600
                } else {
                    400
                },
                align,
                wrap: !control.kind.clickable() && !control.single_line,
            };
            if editable {
                if text_bounds.width <= 0.0 || text_bounds.height <= 0.0 {
                    continue;
                }
                // The same shaping engine provides glyph positioning and caret geometry.
                run.bounds.y -= control.text_scroll;
                run.bounds.height += control.text_scroll;
                let mut layout = self
                    .engine
                    .prepare(&run, 1.0)
                    .map_err(|e| ControlError::Text(e.to_string()))?;
                if control.single_line {
                    // Center against actual document metrics, never placeholder glyphs.
                    let padding = ((text_bounds.height - layout.height.ceil()) * 0.5).max(0.0);
                    run.bounds.y = text_bounds.y + padding;
                    run.bounds.height = (text_bounds.height - padding).max(0.0);
                    // Give an unwrapped RTL line its full intrinsic width before
                    // scrolling. Changing this width with the scroll offset would
                    // move Start alignment and cancel the horizontal translation.
                    run.bounds.width = layout.width.max(text_bounds.width);
                    layout = self
                        .engine
                        .prepare(&run, 1.0)
                        .map_err(|e| ControlError::Text(e.to_string()))?;
                    control.text_scroll = 0.0;
                    let active = TextCaret {
                        index: match &control.kind {
                            Kind::TextBox(document) => document.display_selection().focus,
                            _ => 0,
                        },
                        ..control.caret
                    };
                    let caret = layout.caret_geometry(active);
                    if self.runtime.focused() == Some(id) {
                        control.text_scroll_x =
                            horizontal_caret_scroll(control.text_scroll_x, caret, text_bounds);
                    } else if run.text.is_empty() {
                        control.text_scroll_x = 0.0;
                    }
                    run.bounds.x -= control.text_scroll_x;
                    layout = self
                        .engine
                        .prepare(&run, 1.0)
                        .map_err(|e| ControlError::Text(e.to_string()))?;
                }
                if let Kind::TextBox(document) = &control.kind {
                    let focused = enabled && self.runtime.focused() == Some(id);
                    let selection = document.display_selection();
                    if control.caret.index != selection.focus {
                        control.caret = TextCaret {
                            index: selection.focus,
                            ..Default::default()
                        };
                    }
                    let caret = layout.caret_geometry(control.caret);
                    if focused
                        && !control.single_line
                        && (caret.y < text_bounds.y
                            || caret.y + caret.height > text_bounds.y + text_bounds.height)
                    {
                        let adjustment = if caret.y < text_bounds.y {
                            caret.y - text_bounds.y
                        } else {
                            caret.y + caret.height - text_bounds.y - text_bounds.height
                        };
                        control.text_scroll = (control.text_scroll + adjustment).max(0.0);
                        run.bounds.y = text_bounds.y - control.text_scroll;
                        run.bounds.height = text_bounds.height + control.text_scroll;
                        layout = self
                            .engine
                            .prepare(&run, 1.0)
                            .map_err(|e| ControlError::Text(e.to_string()))?;
                    }
                    if focused {
                        for selection_rect in layout.selection_rects(
                            selection.anchor.min(selection.focus)
                                ..selection.anchor.max(selection.focus),
                        ) {
                            fill_clipped(
                                &mut self.scene,
                                selection_rect,
                                run.clip,
                                editor_style.map_or(tokens.selected, |style| style.selection),
                            );
                        }
                    }
                    if document.text().is_empty()
                        && document.composition().is_none()
                        && !control.editor.placeholder.is_empty()
                    {
                        let mut placeholder = run.clone();
                        placeholder.text = control.editor.placeholder.clone();
                        placeholder.color =
                            editor_style.map_or(tokens.muted, |style| style.placeholder);
                        placeholder.wrap = false;
                        self.scene.text(placeholder);
                    } else {
                        self.scene.text(run.clone());
                    }
                    if focused
                        && self.caret_visible
                        && document.composition().is_none_or(|c| c.cursor.is_some())
                    {
                        fill_clipped(
                            &mut self.scene,
                            layout.caret_geometry(control.caret),
                            run.clip,
                            editor_style.map_or(tokens.focus, |style| style.accent),
                        );
                    }
                    if let Some(composition) = document.composition() {
                        let start = composition.replacement.start;
                        for part in layout.selection_rects(start..start + composition.text.len()) {
                            let line =
                                Rect::new(part.x, part.y + part.height - 1.0, part.width, 1.0);
                            fill_clipped(
                                &mut self.scene,
                                line,
                                run.clip,
                                editor_style.map_or(tokens.focus, |style| style.accent),
                            );
                        }
                    }
                }
                self.text_layouts.insert(id, layout);
            } else if !run.text.is_empty() {
                if control.kind.clickable() {
                    // System-font ascent/descent can exceed a font-size multiplier.
                    // Spend only the actual spare height on decorative vertical padding.
                    let height = self
                        .engine
                        .prepare(&run, 1.0)
                        .map_err(|e| ControlError::Text(e.to_string()))?
                        .height
                        .ceil();
                    let padding = ((bounds.height - height) * 0.5).clamp(0.0, 5.0);
                    run.bounds.y = bounds.y + padding;
                    run.bounds.height = (bounds.height - 2.0 * padding).max(0.0);
                    run.clip = run
                        .bounds
                        .intersection(clip)
                        .unwrap_or(Rect::new(0.0, 0.0, 0.0, 0.0));
                }
                self.scene.text(run);
            }
        }
        Ok(())
    }
    pub fn semantics(&self) -> Vec<SemanticNode> {
        self.runtime
            .tree()
            .preorder()
            .into_iter()
            .filter_map(|id| {
                let bounds = self.runtime.bounds(id)?;
                let props = &self.runtime.tree().node(id)?.props;
                let children = self
                    .runtime
                    .tree()
                    .children(id)
                    .unwrap_or_default()
                    .iter()
                    .copied()
                    .filter(|child| self.runtime.bounds(*child).is_some())
                    .collect();
                // Structural runtime nodes keep the accessibility tree connected.
                let Some(control) = self.controls.get(&id) else {
                    return Some(SemanticNode {
                        id,
                        parent: self.runtime.tree().parent(id),
                        role: Role::Panel,
                        label: String::new(),
                        value: None,
                        checked: None,
                        selected: false,
                        disabled: !self.effective_enabled(id),
                        read_only: false,
                        validation_error: None,
                        focused: self.runtime.focused() == Some(id),
                        bounds,
                        actions: Vec::new(),
                        children,
                    });
                };
                let role = control.role.unwrap_or_else(|| control.kind.role());
                let value = if let Kind::TextBox(document) = &control.kind {
                    Some(document.text().to_owned())
                } else {
                    None
                };
                let checked = matches!(
                    control.kind,
                    Kind::Toggle | Kind::CheckBox | Kind::MenuItem { toggle: true }
                )
                .then_some(control.checked);
                let selected = match control.kind {
                    Kind::Tab { group, index } => {
                        self.tabs.get(&group).is_some_and(|t| t.selected == index)
                    }
                    _ => role == Role::Tab && control.checked,
                };
                let mut actions = Vec::new();
                let enabled = self.is_enabled(id);
                if enabled {
                    if props.focusable {
                        actions.push(SemanticAction::Focus);
                    }
                    if control.kind.clickable() {
                        actions.push(SemanticAction::Activate);
                    }
                    if value.is_some() && !control.editor.is_read_only {
                        actions.push(SemanticAction::SetValue);
                    }
                    if checked.is_some() {
                        actions.push(SemanticAction::Toggle);
                    }
                    if matches!(control.kind, Kind::Scroll { .. }) {
                        actions.push(SemanticAction::Scroll);
                    }
                }
                Some(SemanticNode {
                    id,
                    parent: self.runtime.tree().parent(id),
                    role,
                    label: control.label.clone(),
                    value,
                    checked,
                    selected,
                    disabled: !enabled,
                    read_only: control.editor.is_read_only,
                    validation_error: control.validation_error.clone(),
                    focused: self.runtime.focused() == Some(id),
                    bounds,
                    actions,
                    children,
                })
            })
            .collect()
    }
    pub fn perform_action(
        &mut self,
        id: WidgetId,
        action: SemanticAction,
        value: Option<&str>,
    ) -> Result<(), ControlError> {
        self.synchronize_editor_focus()?;
        if !self.is_enabled(id) {
            return Ok(());
        }
        match action {
            SemanticAction::Focus => self.runtime.apply(Mutation::Focus(Some(id)))?,
            SemanticAction::Activate | SemanticAction::Toggle => self.activate(id)?,
            SemanticAction::SetValue => {
                if self.is_text_editable(id) {
                    // Accessibility is a user route: rejecting input must not close the host.
                    match self.set_text(id, value.unwrap_or_default()) {
                        Ok(()) | Err(ControlError::Edit(EditError::LimitExceeded { .. })) => {}
                        Err(error) => return Err(error),
                    }
                }
            }
            SemanticAction::Scroll => self.scroll_to(
                id,
                value
                    .unwrap_or("0")
                    .parse()
                    .map_err(|_| ControlError::Invalid("Invalid accessible scroll value".into()))?,
            )?,
        }
        self.synchronize_editor_focus()?;
        Ok(())
    }
}

fn add_stats(target: &mut FrameStats, value: FrameStats) {
    target.layout_passes += value.layout_passes;
    target.measured_nodes += value.measured_nodes;
    target.arranged_nodes += value.arranged_nodes;
    target.paint_passes += value.paint_passes;
    target.painted_nodes += value.painted_nodes;
    target.semantics_nodes += value.semantics_nodes;
}

fn editor_fill(scene: &mut Scene, bounds: Rect, clip: Rect, radius: f32, color: Color) {
    if radius > 0.0 {
        scene.rounded_fill(bounds, clip, radius, color);
    } else {
        fill_clipped(scene, bounds, clip, color);
    }
}

fn editor_limits(max_graphemes: Option<usize>) -> TextLimits {
    TextLimits {
        max_graphemes,
        ..Default::default()
    }
}

fn edit_error(error: EditError) -> ControlError {
    ControlError::Edit(error)
}

fn user_edit(result: Result<(), EditError>) -> Result<(), ControlError> {
    match result {
        Ok(()) | Err(EditError::LimitExceeded { .. }) => Ok(()),
        Err(error) => Err(edit_error(error)),
    }
}

fn single_line_separator(c: char) -> bool {
    matches!(c, '\r' | '\n' | '\t' | '\u{85}' | '\u{2028}' | '\u{2029}')
}

fn normalize_editor_text(text: &str, single_line: bool) -> Result<Cow<'_, str>, EditError> {
    let maximum = TextLimits::default().max_bytes;
    if single_line && text.chars().any(single_line_separator) {
        // Check the normalized byte size before allocating a copy of a large paste.
        let bytes: usize = text
            .chars()
            .filter(|&c| !single_line_separator(c))
            .map(char::len_utf8)
            .sum();
        if bytes > maximum {
            return Err(EditError::LimitExceeded {
                kind: TextLimit::Bytes,
                maximum,
            });
        }
        Ok(Cow::Owned(
            text.chars()
                .filter(|&c| !single_line_separator(c))
                .collect(),
        ))
    } else {
        TextLimits::default().validate(text)?;
        Ok(Cow::Borrowed(text))
    }
}

fn horizontal_caret_scroll(previous: f32, caret: Rect, viewport: Rect) -> f32 {
    // RTL trailing whitespace can hang left of the line's intrinsic width.
    // The offset must therefore be signed: a negative value scrolls it right.
    previous
        .max(caret.x + caret.width - viewport.x - viewport.width)
        .min(caret.x - viewport.x)
}

fn fill_clipped(scene: &mut Scene, bounds: Rect, clip: Rect, color: Color) {
    if let Some(bounds) = bounds.intersection(clip) {
        scene.fill(bounds, color);
    }
}
fn append_clipped(target: &mut Scene, source: &Scene, clip: Rect) {
    for command in source.commands() {
        match *command {
            rust_desktop_ui_core::DrawCommand::Rectangle(index) => {
                if let Some(rect) = source.rectangles.get(index) {
                    fill_clipped(target, rect.bounds, clip, rect.color);
                }
            }
            rust_desktop_ui_core::DrawCommand::Text(index) => {
                if let Some(run) = source.texts.get(index)
                    && let Some(effective) = run.clip.intersection(clip)
                {
                    let mut run = run.clone();
                    run.clip = effective;
                    target.text(run);
                }
            }
            rust_desktop_ui_core::DrawCommand::RoundedRectangle(index) => {
                if let Some(rounded) = source.rounded_rectangles.get(index)
                    && let Some(effective) = rounded.clip.intersection(clip)
                {
                    if let Some(width) = rounded.stroke_width {
                        target.rounded_stroke(
                            rounded.bounds,
                            effective,
                            rounded.radius,
                            width,
                            rounded.color,
                        );
                    } else {
                        target.rounded_fill(
                            rounded.bounds,
                            effective,
                            rounded.radius,
                            rounded.color,
                        );
                    }
                }
            }
        }
    }
}
fn draw_border(scene: &mut Scene, bounds: Rect, clip: Rect, border: Border) {
    let w = border
        .width
        .min(bounds.width * 0.5)
        .min(bounds.height * 0.5);
    for rect in [
        Rect::new(bounds.x, bounds.y, bounds.width, w),
        Rect::new(bounds.x, bounds.y + bounds.height - w, bounds.width, w),
        Rect::new(
            bounds.x,
            bounds.y + w,
            w,
            (bounds.height - 2.0 * w).max(0.0),
        ),
        Rect::new(
            bounds.x + bounds.width - w,
            bounds.y + w,
            w,
            (bounds.height - 2.0 * w).max(0.0),
        ),
    ] {
        fill_clipped(scene, rect, clip, border.color);
    }
}

#[cfg(test)]
mod scrolling_tests {
    use super::*;

    #[test]
    fn hanging_rtl_caret_scrolls_into_view_without_font_dependent_metrics() {
        let viewport = Rect::new(20.0, 13.0, 176.0, 26.0);
        let mut offset = 0.0;
        // Includes macOS and Linux trailing-space geometry, followed by
        // navigation right and back to the hanging space.
        for x in [16.110_352, 16.359_985, 1220.0, 1200.0, 16.110_352] {
            let caret = Rect::new(x, 16.5, 1.0, 17.0);
            offset = horizontal_caret_scroll(offset, caret, viewport);
            let visible_x = caret.x - offset;
            assert!(visible_x >= viewport.x);
            assert!(visible_x + caret.width <= viewport.x + viewport.width);
            assert_eq!(horizontal_caret_scroll(offset, caret, viewport), offset);
        }
        assert!(offset < 0.0);
    }
}
