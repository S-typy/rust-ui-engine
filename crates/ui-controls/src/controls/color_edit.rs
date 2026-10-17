//! Retained nullable ColorEdit/BrushEdit values and atomic accepted-value changes.
use super::color_model::{Draft, error};
use super::*;
use rust_desktop_ui_core::{Bitmap, GridPlacement, GridTrack};
use std::{cell::RefCell, collections::VecDeque, rc::Rc};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ColorEditor {
    pub container: WidgetId,
    pub editor: WidgetId,
    pub button: WidgetId,
    pub preview: WidgetId,
}
#[derive(Clone, Debug, PartialEq)]
pub struct BrushChanged {
    pub id: WidgetId,
    pub value: Option<BrushValue>,
}
type Validation = dyn Fn(Option<&BrushValue>) -> Result<(), String>;
#[derive(Clone)]
pub struct BrushValidator(Rc<Validation>);
impl BrushValidator {
    pub fn new(check: impl Fn(Option<&BrushValue>) -> Result<(), String> + 'static) -> Self {
        Self(Rc::new(check))
    }
    pub fn check(&self, value: Option<&BrushValue>) -> Result<(), String> {
        (self.0)(value)
    }
}
pub(super) struct Entry {
    pub parts: ColorEditor,
    pub brush: bool,
    pub value: Option<BrushValue>,
    pub settings: ColorEditorSettings,
    pub draft: Draft,
    pub popup: Option<ColorPopup>,
    pub pending: bool,
    pub validator: Rc<RefCell<Option<BrushValidator>>>,
    pub recent: Vec<Srgba8>,
    pub error: Option<String>,
    pub bitmap: Option<Bitmap>,
    pub preview_bitmap: Option<Bitmap>,
    pub last_fields: Vec<String>,
    pub observed_stop: Option<String>,
    pub source_state: Rc<RefCell<(Option<BrushValue>, bool)>>,
}
#[derive(Default)]
pub(super) struct ColorEditors {
    pub entries: HashMap<WidgetId, Entry>,
    pub changes: VecDeque<BrushChanged>,
}

impl Controls {
    pub fn color_edit(
        &mut self,
        parent: WidgetId,
        label: impl Into<String>,
        value: Option<Srgba8>,
        settings: ColorEditorSettings,
        style: LayoutStyle,
    ) -> Result<ColorEditor, ControlError> {
        self.make_color_editor(
            parent,
            label.into(),
            value.map(BrushValue::Solid),
            settings,
            style,
            false,
        )
    }
    pub fn brush_edit(
        &mut self,
        parent: WidgetId,
        label: impl Into<String>,
        value: Option<BrushValue>,
        settings: ColorEditorSettings,
        style: LayoutStyle,
    ) -> Result<ColorEditor, ControlError> {
        self.make_color_editor(parent, label.into(), value, settings, style, true)
    }
    fn make_color_editor(
        &mut self,
        parent: WidgetId,
        label: String,
        value: Option<BrushValue>,
        settings: ColorEditorSettings,
        mut style: LayoutStyle,
        brush: bool,
    ) -> Result<ColorEditor, ControlError> {
        settings.validate()?;
        settings.validate_value(value.as_ref())?;
        if self.color_editors.entries.len() >= 64 {
            return Err(error("At most 64 color/brush editors are supported"));
        }
        let text = display(value.as_ref());
        self.measure_text(&label, 4096.0, false)?;
        self.preflight_new_editor_text(&text, &style, true)?;
        let bitmap = super::color_popup::bitmap(value.as_ref())?;
        let height = self.theme.tokens().control_height;
        let auto_height = style.height == Length::Auto;
        style.kind = LayoutKind::Grid;
        style.grid_columns = vec![
            GridTrack::Px(height),
            GridTrack::Fr(1.0),
            GridTrack::Px(height),
        ];
        style.grid_rows = vec![GridTrack::Fr(1.0)];
        style.gap = 4.0;
        if auto_height {
            style.height = Length::Px(height);
        }
        let container = self.transparent_panel(parent, style)?;
        let result = (|| {
            self.set_role(container, Role::Group);
            self.set_automation_name(container, &label)?;
            self.controls.get_mut(&container).unwrap().default_height = auto_height;
            let preview = self.transparent_panel(container, cell(1))?;
            self.set_automation_name(preview, "Принятый цвет или кисть")?;
            let editor = self.edit_box(container, &label, &text, cell(2))?;
            self.set_editor_settings(
                editor,
                EditorSettings {
                    is_read_only: brush || settings.is_read_only,
                    max_length: Some(128),
                    ..Default::default()
                },
            )?;
            let button = self.button(container, "Выбрать цвет или кисть", None, cell(3))?;
            self.set_button_icon(button, Some(ButtonIcon::new(IconSymbol::ChevronDown)))?;
            self.set_button_content_layout(button, ButtonContentLayout::IconOnly)?;
            self.edit_props(button, |p| p.enabled = !settings.is_read_only)?;
            let parts = ColorEditor {
                container,
                editor,
                button,
                preview,
            };
            self.color_editors.entries.insert(
                container,
                Entry {
                    parts,
                    brush,
                    value: value.clone(),
                    settings,
                    draft: Draft::new(value.clone()),
                    popup: None,
                    pending: false,
                    validator: Rc::new(RefCell::new(None)),
                    recent: Vec::new(),
                    error: None,
                    bitmap,
                    preview_bitmap: None,
                    last_fields: Vec::new(),
                    observed_stop: None,
                    source_state: Rc::new(RefCell::new((value.clone(), false))),
                },
            );
            Ok(parts)
        })();
        if result.is_err() {
            self.remove(container)?;
        }
        result
    }
    pub fn color_editor_parts(&self, id: WidgetId) -> Option<ColorEditor> {
        self.color_editors.entries.get(&id).map(|e| e.parts)
    }
    pub fn color_editor_settings(&self, id: WidgetId) -> Option<&ColorEditorSettings> {
        self.color_editors.entries.get(&id).map(|e| &e.settings)
    }
    pub fn is_brush_editor(&self, id: WidgetId) -> bool {
        self.color_editors.entries.get(&id).is_some_and(|e| e.brush)
    }
    pub fn color_value(&self, id: WidgetId) -> Option<Option<Srgba8>> {
        let entry = self.color_editors.entries.get(&id)?;
        if entry.brush {
            return None;
        }
        Some(entry.value.as_ref().map(|v| match v {
            BrushValue::Solid(c) => *c,
            _ => unreachable!(),
        }))
    }
    pub fn brush_value(&self, id: WidgetId) -> Option<Option<&BrushValue>> {
        self.color_editors
            .entries
            .get(&id)
            .map(|e| e.value.as_ref())
    }
    pub fn color_recent(&self, id: WidgetId) -> Option<&[Srgba8]> {
        self.color_editors
            .entries
            .get(&id)
            .map(|e| e.recent.as_slice())
    }
    pub fn color_error(&self, id: WidgetId) -> Option<&str> {
        self.color_editors.entries.get(&id)?.error.as_deref()
    }
    pub fn color_editor_is_dirty(&self, id: WidgetId) -> bool {
        self.color_editors.entries.get(&id).is_some_and(|e| {
            e.pending && (e.draft.value != e.value || self.color_popup_fields_dirty(e))
                || self.document(e.parts.editor).is_some_and(|d| {
                    d.text() != self.accepted_value(e.parts.editor).unwrap_or("")
                        || d.composition().is_some()
                })
        })
    }
    pub fn set_color_value(
        &mut self,
        id: WidgetId,
        value: Option<Srgba8>,
    ) -> Result<(), ControlError> {
        if self.color_value(id).is_none() {
            return Err(error("Not a ColorEdit"));
        }
        self.set_brush_value_with_policy(
            id,
            value.map(BrushValue::Solid),
            ColorReplacePolicy::RejectDirty,
        )
    }
    pub fn set_brush_value(
        &mut self,
        id: WidgetId,
        value: Option<BrushValue>,
    ) -> Result<(), ControlError> {
        self.set_brush_value_with_policy(id, value, ColorReplacePolicy::RejectDirty)
    }
    /// Programmatic changes emit no user event. Equal accepted values preserve drafts.
    pub fn set_brush_value_with_policy(
        &mut self,
        id: WidgetId,
        value: Option<BrushValue>,
        policy: ColorReplacePolicy,
    ) -> Result<(), ControlError> {
        let entry = self
            .color_editors
            .entries
            .get(&id)
            .ok_or_else(|| error("Not a color/brush editor"))?;
        self.check_color_parts(id)?;
        self.validate_brush_candidate(id, value.as_ref(), false)?;
        if entry.value == value && policy == ColorReplacePolicy::RejectDirty {
            return Ok(());
        }
        if policy == ColorReplacePolicy::RejectDirty && self.color_editor_is_dirty(id) {
            return Err(error(
                "Color editor has a pending draft; use CancelPending explicitly",
            ));
        }
        let bitmap = super::color_popup::bitmap(value.as_ref())?;
        let text = display(value.as_ref());
        self.preflight_editor_text(entry.parts.editor, &text)?;
        self.cancel_color_popup(id)?;
        self.write_color_inline(id, &text)?;
        let entry = self.color_editors.entries.get_mut(&id).unwrap();
        entry.value = value.clone();
        entry.draft.reset(value);
        entry.bitmap = bitmap;
        entry.error = None;
        self.scene_dirty = true;
        self.refresh_color_source_status();
        Ok(())
    }
    pub fn color_source_validator(&self, id: WidgetId) -> Option<BrushValidator> {
        let entry = self.color_editors.entries.get(&id)?;
        let settings = entry.settings.clone();
        let brush = entry.brush;
        let validator = Rc::clone(&entry.validator);
        let state = Rc::clone(&entry.source_state);
        Some(BrushValidator::new(move |value| {
            settings.validate_value(value).map_err(|e| e.to_string())?;
            if !brush && matches!(value, Some(BrushValue::Linear(_))) {
                return Err("ColorEdit accepts only solid colors".into());
            }
            let source = state.borrow();
            if source.1 && source.0.as_ref() != value {
                return Err("Color source has a pending draft".into());
            }
            if let Some(validator) = validator.borrow().as_ref() {
                validator.check(value)?;
            }
            Ok(())
        }))
    }
    pub(super) fn refresh_color_source_status(&self) {
        for (&id, entry) in &self.color_editors.entries {
            *entry.source_state.borrow_mut() =
                (entry.value.clone(), self.color_editor_is_dirty(id));
        }
    }
    pub fn set_color_validator(
        &mut self,
        id: WidgetId,
        validator: Option<BrushValidator>,
    ) -> Result<(), ControlError> {
        let entry = self
            .color_editors
            .entries
            .get(&id)
            .ok_or_else(|| error("Not a color/brush editor"))?;
        *entry.validator.borrow_mut() = validator;
        Ok(())
    }
    pub(crate) fn chain_color_validator(
        &mut self,
        id: WidgetId,
        validator: BrushValidator,
    ) -> Result<(), ControlError> {
        let entry = self
            .color_editors
            .entries
            .get(&id)
            .ok_or_else(|| error("Not a color/brush editor"))?;
        let previous = entry.validator.borrow().clone();
        *entry.validator.borrow_mut() = Some(BrushValidator::new(move |value| {
            if let Some(v) = &previous {
                v.check(value)?;
            }
            validator.check(value)
        }));
        Ok(())
    }
    pub fn set_color_read_only(&mut self, id: WidgetId, value: bool) -> Result<(), ControlError> {
        let entry = self
            .color_editors
            .entries
            .get_mut(&id)
            .ok_or_else(|| error("Not a color/brush editor"))?;
        entry.settings.is_read_only = value;
        let parts = entry.parts;
        let brush = entry.brush;
        self.set_read_only(parts.editor, value || brush)?;
        self.edit_props(parts.button, |p| p.enabled = !value)?;
        if value {
            self.cancel_color_popup(id)?;
        }
        Ok(())
    }
    pub fn drain_brush_changes(&mut self) -> Vec<BrushChanged> {
        self.color_editors.changes.drain(..).collect()
    }
    pub(super) fn validate_brush_candidate(
        &self,
        id: WidgetId,
        value: Option<&BrushValue>,
        user: bool,
    ) -> Result<(), ControlError> {
        let entry = self
            .color_editors
            .entries
            .get(&id)
            .ok_or_else(|| error("Not a color/brush editor"))?;
        entry.settings.validate_value(value)?;
        if !entry.brush && matches!(value, Some(BrushValue::Linear(_))) {
            return Err(error("ColorEdit accepts only solid colors"));
        }
        if let Some(validator) = entry.validator.borrow().as_ref() {
            validator.check(value).map_err(error)?;
        }
        if user && entry.value.as_ref() != value && self.color_editors.changes.len() >= 256 {
            return Err(error("Drain brush changes before another user commit"));
        }
        Ok(())
    }
    pub(super) fn color_editable(&self, id: WidgetId) -> bool {
        self.color_editors.entries.get(&id).is_some_and(|e| {
            !e.settings.is_read_only
                && self.effective_enabled(id)
                && self.activation_available(id)
                && self.effective_enabled(e.parts.editor)
                && (e.brush || self.is_text_editable(e.parts.editor))
        })
    }
    pub(super) fn color_inline_owner(&self, editor: WidgetId) -> Option<WidgetId> {
        self.color_editors
            .entries
            .iter()
            .find_map(|(&id, e)| (e.parts.editor == editor).then_some(id))
    }
    pub(super) fn commit_color_inline(
        &mut self,
        owner: WidgetId,
    ) -> Result<CommitOutcome, ControlError> {
        self.check_color_parts(owner)?;
        if !self.color_editable(owner) {
            return Ok(CommitOutcome::Unchanged);
        }
        let entry = self.color_editors.entries.get(&owner).unwrap();
        if entry.brush || entry.pending {
            return Ok(CommitOutcome::Unchanged);
        }
        let editor = entry.parts.editor;
        let document = self
            .document(editor)
            .ok_or_else(|| error("Color editor part is missing"))?;
        if document.composition().is_some() {
            return Ok(CommitOutcome::CompositionActive);
        }
        let text = document.text().to_owned();
        let value = parse_inline(&text).and_then(|value| {
            self.validate_editor_value(editor, &text)?;
            self.validate_brush_candidate(owner, value.as_ref(), true)
                .map_err(|e| e.to_string())?;
            Ok(value)
        });
        let value = match value {
            Ok(v) => v,
            Err(message) => {
                let message = self.safe_color_error(&message);
                self.color_editors.entries.get_mut(&owner).unwrap().error = Some(message.clone());
                self.controls.get_mut(&editor).unwrap().validation_error = Some(message.clone());
                self.scene_dirty = true;
                return Ok(CommitOutcome::Invalid(message));
            }
        };
        let changed = self.color_editors.entries[&owner].value != value;
        let bitmap = super::color_popup::bitmap(value.as_ref())?;
        self.controls.get_mut(&editor).unwrap().accepted_value = text;
        self.controls.get_mut(&editor).unwrap().validation_error = None;
        self.publish_brush(owner, value, bitmap, changed);
        Ok(if changed {
            CommitOutcome::Committed
        } else {
            CommitOutcome::Unchanged
        })
    }
    pub(super) fn set_color_inline_application(
        &mut self,
        owner: WidgetId,
        text: &str,
    ) -> Result<CommitOutcome, ControlError> {
        let value = parse_inline(text).map_err(error)?;
        let changed = self.color_editors.entries[&owner].value != value;
        self.set_brush_value(owner, value)?;
        Ok(if changed {
            CommitOutcome::Committed
        } else {
            CommitOutcome::Unchanged
        })
    }
    pub(super) fn write_color_inline(
        &mut self,
        owner: WidgetId,
        text: &str,
    ) -> Result<(), ControlError> {
        let id = self.color_editors.entries[&owner].parts.editor;
        self.preflight_editor_text(id, text)?;
        let before = self.document(id).unwrap().text().to_owned();
        let control = self.controls.get_mut(&id).unwrap();
        control.kind = Kind::TextBox(TextDocument::new(text));
        control.accepted_value = text.to_owned();
        control.caret = TextCaret {
            index: text.len(),
            ..Default::default()
        };
        control.validation_error = None;
        self.record_text_change(id, &before);
        self.scene_dirty = true;
        Ok(())
    }
    pub(super) fn publish_brush(
        &mut self,
        owner: WidgetId,
        value: Option<BrushValue>,
        bitmap: Option<Bitmap>,
        changed: bool,
    ) {
        let entry = self.color_editors.entries.get_mut(&owner).unwrap();
        entry.value = value.clone();
        entry.bitmap = bitmap;
        entry.error = None;
        if changed {
            let colors = match &value {
                Some(BrushValue::Solid(c)) => vec![*c],
                Some(BrushValue::Linear(g)) => g.stops().iter().map(|s| s.color).collect(),
                None => Vec::new(),
            };
            for color in colors {
                entry.recent.retain(|c| *c != color);
                entry.recent.insert(0, color);
            }
            entry.recent.truncate(entry.settings.recent_limit);
            self.color_editors
                .changes
                .push_back(BrushChanged { id: owner, value });
        }
        self.scene_dirty = true;
        self.refresh_color_source_status();
    }
    pub(super) fn preflight_color_removal(&self, id: WidgetId) -> Result<(), ControlError> {
        if self.color_editors.entries.values().any(|e| {
            [e.parts.editor, e.parts.button, e.parts.preview].contains(&id)
                || e.popup.as_ref().is_some_and(|p| p.required.contains(&id))
        }) {
            return Err(error(
                "Remove the color/brush owner, not one of its required parts",
            ));
        }
        Ok(())
    }
    pub(super) fn prune_color_editors(&mut self) -> Result<(), ControlError> {
        let invalid: Vec<_> = self
            .color_editors
            .entries
            .iter()
            .filter_map(|(&id, e)| {
                (!self.runtime.tree().contains(id)
                    || [e.parts.editor, e.parts.button, e.parts.preview]
                        .iter()
                        .any(|p| !self.runtime.tree().contains(*p))
                    || e.popup.as_ref().is_some_and(|p| {
                        p.required.iter().any(|p| !self.runtime.tree().contains(*p))
                    }))
                .then_some(id)
            })
            .collect();
        for id in invalid {
            let entry = self.color_editors.entries.remove(&id).unwrap();
            if let Some(popup) = entry.popup
                && self.runtime.tree().contains(popup.flyout.container)
            {
                self.remove(popup.flyout.container)?;
            }
        }
        self.color_editors
            .changes
            .retain(|e| self.color_editors.entries.contains_key(&e.id));
        Ok(())
    }
}
pub(super) fn display(value: Option<&BrushValue>) -> String {
    match value {
        None => String::new(),
        Some(BrushValue::Solid(c)) => c.to_string(),
        Some(BrushValue::Linear(g)) => {
            format!(
                "Линейная · {} точек · {}°",
                g.stops().len(),
                super::color_model::compact_number(g.angle())
            )
        }
    }
}
pub(super) fn parse_inline(text: &str) -> Result<Option<BrushValue>, String> {
    if text.is_empty() {
        Ok(None)
    } else {
        text.parse::<Srgba8>()
            .map(|c| Some(BrushValue::Solid(c)))
            .map_err(|e| e.to_string())
    }
}
fn cell(column: u16) -> LayoutStyle {
    LayoutStyle {
        width: Length::Percent(1.0),
        height: Length::Percent(1.0),
        grid_column: GridPlacement {
            start: Some(column),
            span: 1,
        },
        ..Default::default()
    }
}
