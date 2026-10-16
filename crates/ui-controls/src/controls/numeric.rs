//! Decimal editor with a retained calculator and the ordinary editor commit pipeline.
use super::selection::{Composite, CompositeKind};
use super::*;
use crate::EditorValidationStatus;
use rust_desktop_ui_core::{GridPlacement, GridTrack, Modifiers};
use std::{cell::RefCell, rc::Rc};

#[derive(Clone)]
struct DraftSnapshot {
    document: TextDocument,
    caret: TextCaret,
    accepted: String,
    error: Option<String>,
}
#[derive(Clone, Copy)]
struct CalculatorParts {
    popup: Flyout,
    display: WidgetId,
    error: WidgetId,
    apply: WidgetId,
    cancel: WidgetId,
}
struct Entry {
    button: WidgetId,
    binding_settings: Rc<RefCell<NumericEditSettings>>,
    popup: Option<CalculatorParts>,
    model: NumericCalculator,
    keys: HashMap<WidgetId, CalculatorKey>,
    snapshot: Option<DraftSnapshot>,
    staged: Option<String>,
    pending: bool,
}
#[derive(Default)]
pub(super) struct NumericState {
    entries: HashMap<WidgetId, Entry>,
}

impl Controls {
    fn numeric_editable(&self, id: WidgetId) -> bool {
        self.rich_editable(id)
            && self
                .rich
                .composites
                .get(&id)
                .is_some_and(|c| self.is_text_editable(c.parts.editor))
            && self
                .numeric
                .entries
                .get(&id)
                .is_some_and(|e| self.is_enabled(e.button))
    }

    pub fn numeric_edit(
        &mut self,
        parent: WidgetId,
        label: impl Into<String>,
        value: Option<DecimalValue>,
        settings: NumericEditSettings,
        style: LayoutStyle,
    ) -> Result<EditorControl, ControlError> {
        settings.validate()?;
        let text = settings.format(value)?;
        let mut parts = self.create_editor_parts(parent, label.into(), &text, style)?;
        let result = (|| {
            self.add_step_buttons(&mut parts)?;
            let button =
                self.add_editor_button(&parts, "Калькулятор", Some(IconSymbol::ChevronDown))?;
            parts.buttons.push(button);
            self.set_role(parts.editor, Role::SpinButton);
            self.numeric.entries.insert(
                parts.container,
                Entry {
                    button,
                    binding_settings: Rc::new(RefCell::new(settings.clone())),
                    popup: None,
                    model: NumericCalculator::new(value, settings.precision)
                        .map_err(ControlError::Invalid)?,
                    keys: HashMap::new(),
                    snapshot: None,
                    staged: None,
                    pending: false,
                },
            );
            self.rich.composites.insert(
                parts.container,
                Composite {
                    parts: parts.clone(),
                    kind: CompositeKind::Numeric(settings.clone()),
                    read_only: false,
                },
            );
            self.rich
                .value_baselines
                .insert(parts.container, EditorValue::Decimal(value));
            let mut button_settings = self.editor_button_settings(button).unwrap_or_default();
            button_settings.visible = settings.calculator;
            self.set_editor_button_settings(button, button_settings)?;
            Ok(parts.clone())
        })();
        if result.is_err() {
            self.numeric.entries.remove(&parts.container);
            self.remove(parts.container)?;
        }
        result
    }
    pub(crate) fn numeric_binding_settings(
        &self,
        id: WidgetId,
    ) -> Option<Rc<RefCell<NumericEditSettings>>> {
        self.numeric
            .entries
            .get(&id)
            .map(|e| Rc::clone(&e.binding_settings))
    }
    pub fn numeric_settings(&self, id: WidgetId) -> Option<&NumericEditSettings> {
        match &self.rich.composites.get(&id)?.kind {
            CompositeKind::Numeric(settings) => Some(settings),
            _ => None,
        }
    }
    pub fn numeric_value(&self, id: WidgetId) -> Option<Option<DecimalValue>> {
        let settings = self.numeric_settings(id)?;
        settings
            .parse(self.accepted_value(self.rich.composites[&id].parts.editor)?)
            .ok()
    }
    pub fn set_numeric_value(
        &mut self,
        id: WidgetId,
        value: Option<DecimalValue>,
    ) -> Result<CommitOutcome, ControlError> {
        let settings = self
            .numeric_settings(id)
            .ok_or_else(|| ControlError::Invalid("Not a NumericEdit".into()))?;
        if let Err(error) = settings.validate_value(value) {
            return Ok(CommitOutcome::Invalid(error));
        }
        let text = settings.format(value)?;
        self.set_editor_value(self.rich.composites[&id].parts.editor, &text)
    }
    /// Settings changes require a clean editor and a closed calculator, preserving drafts.
    pub fn set_numeric_settings(
        &mut self,
        id: WidgetId,
        settings: NumericEditSettings,
    ) -> Result<(), ControlError> {
        settings.validate()?;
        let old = self
            .numeric_settings(id)
            .cloned()
            .ok_or_else(|| ControlError::Invalid("Not a NumericEdit".into()))?;
        let editor = self.rich.composites[&id].parts.editor;
        if self.is_edit_dirty(editor) == Some(true)
            || self
                .document(editor)
                .is_some_and(|d| d.composition().is_some())
            || self.numeric_calculator_is_open(id)
        {
            return Err(ControlError::Invalid("Accept or cancel the numeric draft and close the calculator before changing settings".into()));
        }
        let value = self.numeric_value(id).unwrap();
        let text = settings.format(value)?;
        // Validate the candidate with the new typed rules before changing presentation.
        self.rich.composites.get_mut(&id).unwrap().kind = CompositeKind::Numeric(settings.clone());
        let shared = self.numeric_binding_settings(id).unwrap();
        *shared.borrow_mut() = settings.clone();
        let result = self.validate_edit(editor, &text);
        *shared.borrow_mut() = old.clone();
        self.rich.composites.get_mut(&id).unwrap().kind = CompositeKind::Numeric(old.clone());
        result.map_err(ControlError::Invalid)?;
        self.rich.composites.get_mut(&id).unwrap().kind = CompositeKind::Numeric(settings.clone());
        *shared.borrow_mut() = settings.clone();
        match self.set_editor_value(editor, &text) {
            Ok(CommitOutcome::Committed | CommitOutcome::Unchanged) => {}
            Ok(other) => {
                *shared.borrow_mut() = old.clone();
                self.rich.composites.get_mut(&id).unwrap().kind = CompositeKind::Numeric(old);
                return Err(ControlError::Invalid(format!(
                    "Numeric settings rejected: {other:?}"
                )));
            }
            Err(error) => {
                *shared.borrow_mut() = old.clone();
                self.rich.composites.get_mut(&id).unwrap().kind = CompositeKind::Numeric(old);
                return Err(error);
            }
        }
        let button = self.numeric.entries[&id].button;
        let mut button_settings = self.editor_button_settings(button).unwrap_or_default();
        button_settings.visible = settings.calculator;
        self.set_editor_button_settings(button, button_settings)
    }
    pub fn numeric_calculator_is_open(&self, id: WidgetId) -> bool {
        self.numeric
            .entries
            .get(&id)
            .and_then(|e| e.popup)
            .is_some_and(|p| self.flyout_is_open(p.popup))
    }
    pub fn numeric_calculator(&self, id: WidgetId) -> Option<&NumericCalculator> {
        self.numeric.entries.get(&id).map(|e| &e.model)
    }
    pub fn open_numeric_calculator(&mut self, id: WidgetId) -> Result<(), ControlError> {
        let settings = self
            .numeric_settings(id)
            .cloned()
            .ok_or_else(|| ControlError::Invalid("Not a NumericEdit".into()))?;
        if !settings.calculator || !self.numeric_editable(id) {
            return Err(ControlError::Invalid(
                "Numeric calculator is unavailable".into(),
            ));
        }
        if self.numeric_calculator_is_open(id) {
            return Ok(());
        }
        let editor = self.rich.composites[&id].parts.editor;
        if self.document(editor).unwrap().composition().is_some() {
            return Err(ControlError::Invalid(
                "Finish composition before opening the calculator".into(),
            ));
        }
        let value = settings
            .parse(self.document(editor).unwrap().text())
            .unwrap_or_else(|_| self.numeric_value(id).unwrap());
        let model =
            NumericCalculator::new(value, settings.precision).map_err(ControlError::Invalid)?;
        if self.numeric.entries[&id].popup.is_none() {
            self.build_numeric_calculator(id)?;
        }
        self.numeric.entries.get_mut(&id).unwrap().model = model;
        self.refresh_numeric_display(id, None)?;
        self.open_flyout(self.numeric.entries[&id].popup.unwrap().popup)
    }
    pub fn press_numeric_calculator(
        &mut self,
        id: WidgetId,
        key: CalculatorKey,
    ) -> Result<(), ControlError> {
        if !self.numeric_calculator_is_open(id) || !self.numeric_editable(id) {
            return Err(ControlError::Invalid(
                "Numeric calculator is not open and editable".into(),
            ));
        }
        self.restore_numeric_draft(id);
        let result = self.numeric.entries.get_mut(&id).unwrap().model.press(key);
        self.refresh_numeric_display(id, result.as_ref().err().map(String::as_str))?;
        result.map_err(ControlError::Invalid)
    }
    pub fn apply_numeric_calculator(
        &mut self,
        id: WidgetId,
    ) -> Result<CommitOutcome, ControlError> {
        if !self.numeric_calculator_is_open(id) || !self.numeric_editable(id) {
            return Err(ControlError::Invalid(
                "Numeric calculator is not open and editable".into(),
            ));
        }
        let settings = self.numeric_settings(id).unwrap().clone();
        let mut model = self.numeric.entries[&id].model.clone();
        let text = model
            .press(CalculatorKey::Equals)
            .and_then(|_| model.value())
            .and_then(|v| settings.format(Some(v)).map_err(|e| e.to_string()));
        let editor = self.rich.composites[&id].parts.editor;
        let text = match text.and_then(|text| self.validate_edit(editor, &text).map(|_| text)) {
            Ok(text) => text,
            Err(error) => {
                self.refresh_numeric_display(id, Some(&error))?;
                return Ok(CommitOutcome::Invalid(error));
            }
        };
        if self.document(editor).unwrap().composition().is_some() {
            return Ok(CommitOutcome::CompositionActive);
        }
        self.restore_numeric_draft(id);
        let control = &self.controls[&editor];
        let snapshot = DraftSnapshot {
            document: self.document(editor).unwrap().clone(),
            caret: control.caret,
            accepted: control.accepted_value.clone(),
            error: control.validation_error.clone(),
        };
        self.set_text(editor, &text)?;
        let entry = self.numeric.entries.get_mut(&id).unwrap();
        entry.snapshot = Some(snapshot);
        entry.staged = Some(text);
        entry.model = model;
        let outcome = self.commit_edit(editor)?;
        self.numeric.entries.get_mut(&id).unwrap().pending = outcome == CommitOutcome::Pending;
        match &outcome {
            CommitOutcome::Committed | CommitOutcome::Unchanged => self.finish_numeric_apply(id)?,
            CommitOutcome::Invalid(error) => {
                self.restore_numeric_draft(id);
                self.refresh_numeric_display(id, Some(error))?;
            }
            _ => {}
        }
        Ok(outcome)
    }
    pub fn cancel_numeric_calculator(&mut self, id: WidgetId) -> Result<(), ControlError> {
        let entry = self
            .numeric
            .entries
            .get(&id)
            .ok_or_else(|| ControlError::Invalid("Not a NumericEdit".into()))?;
        if let Some(parts) = entry.popup {
            self.close_flyout(parts.popup)?;
        }
        Ok(())
    }
    fn finish_numeric_apply(&mut self, id: WidgetId) -> Result<(), ControlError> {
        let entry = self.numeric.entries.get_mut(&id).unwrap();
        entry.snapshot = None;
        entry.staged = None;
        entry.pending = false;
        let popup = entry.popup.unwrap().popup;
        self.close_flyout(popup)
    }
    fn restore_numeric_draft(&mut self, id: WidgetId) {
        let Some(entry) = self.numeric.entries.get_mut(&id) else {
            return;
        };
        let snapshot = entry.snapshot.take();
        let staged = entry.staged.take();
        entry.pending = false;
        let Some(snapshot) = snapshot else {
            return;
        };
        let Some(composite) = self.rich.composites.get(&id) else {
            return;
        };
        let editor = composite.parts.editor;
        // An external accepted value or newer application draft wins over this rollback.
        if self.accepted_value(editor) != Some(snapshot.accepted.as_str())
            || self.document(editor).map(|d| d.text()) != staged.as_deref()
        {
            return;
        }
        self.invalidate_async_editor(editor);
        let before = self.document(editor).unwrap().text().to_owned();
        let control = self.controls.get_mut(&editor).unwrap();
        control.kind = Kind::TextBox(snapshot.document);
        control.caret = snapshot.caret;
        control.validation_error = snapshot.error;
        self.record_text_change(editor, &before);
        self.scene_dirty = true;
    }
    fn refresh_numeric_display(
        &mut self,
        id: WidgetId,
        error: Option<&str>,
    ) -> Result<(), ControlError> {
        let entry = &self.numeric.entries[&id];
        if let Some(parts) = entry.popup {
            let display = entry
                .model
                .display(self.numeric_settings(id).unwrap().culture);
            let separator_key = entry
                .keys
                .iter()
                .find(|(_, key)| **key == CalculatorKey::DecimalSeparator)
                .map(|(id, _)| *id);
            let separator = match self.numeric_settings(id).unwrap().culture {
                crate::UiCulture::Russian => ",",
                crate::UiCulture::English => ".",
                crate::UiCulture::Arabic => "٫",
            };
            if let Some(button) = separator_key {
                self.set_label(button, separator)?;
            }
            self.set_label(parts.display, &display)?;
            self.set_label(parts.error, error.unwrap_or(""))?;
            let (apply, cancel) = calculator_captions(self.numeric_settings(id).unwrap().culture);
            self.set_label(parts.apply, apply)?;
            self.set_label(parts.cancel, cancel)?;
        }
        Ok(())
    }
    fn build_numeric_calculator(&mut self, id: WidgetId) -> Result<(), ControlError> {
        let button = self.numeric.entries[&id].button;
        let popup = self.flyout(button, Size::new(320.0, 420.0))?;
        let result = (|| {
            self.edit_props(popup.content, |props| {
                props.style.kind = LayoutKind::Grid;
                props.style.grid_columns = vec![GridTrack::Fr(1.0); 4];
                props.style.grid_rows = vec![
                    GridTrack::Px(44.0),
                    GridTrack::Px(40.0),
                    GridTrack::Fr(1.0),
                    GridTrack::Fr(1.0),
                    GridTrack::Fr(1.0),
                    GridTrack::Fr(1.0),
                    GridTrack::Fr(1.0),
                    GridTrack::Px(40.0),
                ];
                props.style.gap = 4.0;
            })?;
            let cell = |row, col, span| LayoutStyle {
                width: Length::Percent(1.0),
                height: Length::Percent(1.0),
                grid_row: GridPlacement {
                    start: Some(row),
                    span: 1,
                },
                grid_column: GridPlacement {
                    start: Some(col),
                    span,
                },
                ..Default::default()
            };
            let display = self.label(popup.content, "0", cell(1, 1, 4))?;
            let error = self.label(popup.content, "", cell(2, 1, 4))?;
            let mut keys = HashMap::new();
            let separator = match self.numeric_settings(id).unwrap().culture {
                crate::UiCulture::Russian => ",",
                crate::UiCulture::English => ".",
                crate::UiCulture::Arabic => "٫",
            };
            let rows = [
                [
                    ("C", CalculatorKey::Clear),
                    ("Backspace", CalculatorKey::Backspace),
                    ("±", CalculatorKey::Negate),
                    ("÷", CalculatorKey::Operation(NumericOperation::Divide)),
                ],
                [
                    ("7", CalculatorKey::Digit(7)),
                    ("8", CalculatorKey::Digit(8)),
                    ("9", CalculatorKey::Digit(9)),
                    ("×", CalculatorKey::Operation(NumericOperation::Multiply)),
                ],
                [
                    ("4", CalculatorKey::Digit(4)),
                    ("5", CalculatorKey::Digit(5)),
                    ("6", CalculatorKey::Digit(6)),
                    ("−", CalculatorKey::Operation(NumericOperation::Subtract)),
                ],
                [
                    ("1", CalculatorKey::Digit(1)),
                    ("2", CalculatorKey::Digit(2)),
                    ("3", CalculatorKey::Digit(3)),
                    ("+", CalculatorKey::Operation(NumericOperation::Add)),
                ],
            ];
            for (row, items) in rows.iter().enumerate() {
                for (col, (label, key)) in items.iter().enumerate() {
                    let key_button = self.button(
                        popup.content,
                        *label,
                        None,
                        cell(row as u16 + 3, col as u16 + 1, 1),
                    )?;
                    if *key == CalculatorKey::Backspace {
                        self.set_button_icon(
                            key_button,
                            Some(ButtonIcon::new(IconSymbol::Backspace)),
                        )?;
                        self.set_button_content_layout(
                            key_button,
                            crate::ButtonContentLayout::IconOnly,
                        )?;
                    }
                    keys.insert(key_button, *key);
                }
            }
            for (col, span, label, key) in [
                (1, 2, "0", CalculatorKey::Digit(0)),
                (3, 1, separator, CalculatorKey::DecimalSeparator),
                (4, 1, "=", CalculatorKey::Equals),
            ] {
                let key_button = self.button(popup.content, label, None, cell(7, col, span))?;
                keys.insert(key_button, key);
            }
            let (apply_caption, cancel_caption) =
                calculator_captions(self.numeric_settings(id).unwrap().culture);
            let apply = self.button(popup.content, apply_caption, None, cell(8, 1, 2))?;
            let cancel = self.button(popup.content, cancel_caption, None, cell(8, 3, 2))?;
            let entry = self.numeric.entries.get_mut(&id).unwrap();
            entry.keys = keys;
            entry.popup = Some(CalculatorParts {
                popup,
                display,
                error,
                apply,
                cancel,
            });
            Ok(())
        })();
        if result.is_err() {
            self.remove(popup.container)?;
        }
        result
    }
    pub(super) fn numeric_focus_editor(&self, id: WidgetId) -> Option<WidgetId> {
        self.numeric
            .entries
            .iter()
            .find(|(_, e)| {
                e.popup.is_some_and(|p| {
                    self.flyout_is_open(p.popup) && self.flyout_descendant(id, p.popup.container)
                })
            })
            .map(|(owner, _)| self.rich.composites[owner].parts.editor)
    }
    pub(super) fn numeric_will_close(&mut self, popup: WidgetId) {
        let owner = self
            .numeric
            .entries
            .iter()
            .find(|(_, e)| e.popup.is_some_and(|p| p.popup.container == popup))
            .map(|(owner, _)| *owner);
        if let Some(owner) = owner {
            self.restore_numeric_draft(owner);
        }
    }
    pub(super) fn cancel_unavailable_numeric_validation(&mut self) {
        let ids: Vec<_> = self
            .numeric
            .entries
            .iter()
            .filter(|(id, e)| e.pending && (!self.window_focused || !self.numeric_editable(**id)))
            .map(|(id, _)| *id)
            .collect();
        for id in ids {
            self.restore_numeric_draft(id);
        }
    }
    pub(super) fn sync_numeric(&mut self) -> Result<(), ControlError> {
        self.numeric.entries.retain(|owner, _| {
            self.rich.composites.contains_key(owner) && self.runtime.tree().contains(*owner)
        });
        let open: Vec<_> = self
            .numeric
            .entries
            .keys()
            .copied()
            .filter(|id| self.numeric_calculator_is_open(*id))
            .collect();
        for id in open {
            if !self.numeric_editable(id) {
                self.cancel_numeric_calculator(id)?;
                continue;
            }
            let editor = self.rich.composites[&id].parts.editor;
            if self.numeric.entries[&id].pending
                && self.editor_validation_status(editor) != EditorValidationStatus::Pending
            {
                let accepted = self.accepted_value(editor)
                    == self.numeric.entries[&id].staged.as_deref()
                    && self.validation_error(editor).is_none();
                if accepted {
                    self.finish_numeric_apply(id)?;
                } else {
                    let error = self.validation_error(editor).map(str::to_owned);
                    self.restore_numeric_draft(id);
                    self.refresh_numeric_display(id, error.as_deref())?;
                }
            }
        }
        Ok(())
    }
    pub(super) fn preflight_numeric_removal(&self, id: WidgetId) -> Result<(), ControlError> {
        if self.numeric.entries.iter().any(|(owner, e)| {
            self.runtime.tree().contains(*owner)
                && (e.button == id
                    || e.popup
                        .is_some_and(|p| self.flyout_descendant(id, p.popup.container)))
        }) {
            return Err(ControlError::Invalid(
                "Remove the NumericEdit container, not its required calculator disclosure or popup parts".into(),
            ));
        }
        Ok(())
    }
    pub(super) fn activate_numeric(&mut self, button: WidgetId) -> Result<bool, ControlError> {
        let owner = self
            .numeric
            .entries
            .iter()
            .find(|(_, e)| {
                e.button == button
                    || e.keys.contains_key(&button)
                    || e.popup
                        .is_some_and(|p| p.apply == button || p.cancel == button)
            })
            .map(|(id, _)| *id);
        let Some(owner) = owner else {
            return Ok(false);
        };
        if !self.numeric_editable(owner) {
            return Ok(true);
        }
        let entry = &self.numeric.entries[&owner];
        if entry.button == button {
            if self.numeric_calculator_is_open(owner) {
                self.cancel_numeric_calculator(owner)?;
            } else {
                self.open_numeric_calculator(owner)?;
            }
        } else if let Some(key) = entry.keys.get(&button).copied() {
            let _ = self.press_numeric_calculator(owner, key);
        } else if entry.popup.is_some_and(|p| p.apply == button) {
            self.apply_numeric_calculator(owner)?;
        } else {
            self.cancel_numeric_calculator(owner)?;
        }
        Ok(true)
    }
    pub(super) fn numeric_before_input(
        &mut self,
        event: &InputEvent,
    ) -> Result<bool, ControlError> {
        let InputEvent::KeyDown { key, modifiers, .. } = event else {
            return Ok(false);
        };
        let Some(focused) = self.runtime.focused() else {
            return Ok(false);
        };
        let Some(editor) = self.numeric_focus_editor(focused) else {
            return Ok(false);
        };
        let owner = self.rich_owner(editor).unwrap();
        if modifiers.control || modifiers.alt || modifiers.super_key {
            return Ok(false);
        }
        if *key == Key::Escape {
            self.cancel_numeric_calculator(owner)?;
            return Ok(true);
        }
        if *key == Key::Enter && *modifiers == Modifiers::default() {
            self.apply_numeric_calculator(owner)?;
            return Ok(true);
        }
        let calculator_key = match key {
            Key::Backspace => Some(CalculatorKey::Backspace),
            Key::Delete => Some(CalculatorKey::Clear),
            Key::Character(s) => match s.as_str() {
                "+" => Some(CalculatorKey::Operation(NumericOperation::Add)),
                "-" => Some(CalculatorKey::Operation(NumericOperation::Subtract)),
                "*" => Some(CalculatorKey::Operation(NumericOperation::Multiply)),
                "/" => Some(CalculatorKey::Operation(NumericOperation::Divide)),
                "=" => Some(CalculatorKey::Equals),
                "." | "," | "٫" => Some(CalculatorKey::DecimalSeparator),
                s if s.len() == 1 && s.as_bytes()[0].is_ascii_digit() => {
                    Some(CalculatorKey::Digit(s.as_bytes()[0] - b'0'))
                }
                _ => None,
            },
            _ => None,
        };
        if let Some(key) = calculator_key {
            let _ = self.press_numeric_calculator(owner, key);
            return Ok(true);
        }
        Ok(false)
    }
}

fn calculator_captions(culture: crate::UiCulture) -> (&'static str, &'static str) {
    match culture {
        crate::UiCulture::Russian => ("Применить", "Отмена"),
        crate::UiCulture::English => ("Apply", "Cancel"),
        crate::UiCulture::Arabic => ("تم", "إلغاء"),
    }
}
