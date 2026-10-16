//! Numeric and calendar values use invariant storage and explicit display formats.
use super::selection::{Composite, CompositeKind, EditorControl, RichAction, SelectionChanged};
use super::*;

#[derive(Clone, Debug, PartialEq)]
pub struct SpinBoxSettings {
    pub minimum: f64,
    pub maximum: f64,
    pub step: f64,
}
impl Default for SpinBoxSettings {
    fn default() -> Self {
        Self {
            minimum: -1.0e15,
            maximum: 1.0e15,
            step: 1.0,
        }
    }
}
impl SpinBoxSettings {
    pub fn validate(&self) -> Result<(), ControlError> {
        if !self.minimum.is_finite()
            || !self.maximum.is_finite()
            || !self.step.is_finite()
            || self.minimum > self.maximum
            || self.step <= 0.0
        {
            return Err(ControlError::Invalid(
                "SpinBox requires finite minimum <= maximum and step > 0".into(),
            ));
        }
        Ok(())
    }
    fn parse(&self, text: &str) -> Result<f64, String> {
        let value = text
            .trim()
            .parse::<f64>()
            .map_err(|_| "Введите число.".to_owned())?;
        if !value.is_finite() || value < self.minimum || value > self.maximum {
            return Err(format!(
                "Значение должно быть от {} до {}.",
                self.minimum, self.maximum
            ));
        }
        Ok(value)
    }
}
#[derive(Clone, Debug, PartialEq)]
pub enum EditorValue {
    Number(f64),
    Decimal(Option<DecimalValue>),
    Date(Option<DateValue>),
    Time(Option<TimeValue>),
    DateTime(Option<DateTimeValue>),
    DateRange(Option<DateRangeValue>),
}
#[derive(Clone, Debug, PartialEq)]
pub struct ValueChanged {
    pub id: WidgetId,
    pub value: EditorValue,
}

impl Controls {
    pub fn editor_validator(&self, id: WidgetId) -> Option<&EditorValidator> {
        self.controls.get(&id)?.validator.as_ref()
    }
    pub fn label_text(&self, id: WidgetId) -> Option<&str> {
        self.controls.get(&id).map(|c| c.label.as_str())
    }
    /// Preflight a draft without changing the document, accepted value or diagnostics.
    pub fn validate_edit(&self, id: WidgetId, text: &str) -> Result<(), String> {
        self.editor_control(id).map_err(|e| e.to_string())?;
        self.validate_editor_value(id, text)
    }
    pub fn drain_value_changes(&mut self) -> Vec<ValueChanged> {
        std::mem::take(&mut self.rich.values)
    }
    pub fn composite_read_only(&self, id: WidgetId) -> Option<bool> {
        self.rich
            .composites
            .get(&id)
            .map(|c| c.read_only)
            .or_else(|| {
                self.memo_editor(id)
                    .and_then(|editor| self.is_read_only(editor))
            })
    }
    /// Typed validation remains independent from this application validator chain.
    pub fn chain_editor_validator(
        &mut self,
        id: WidgetId,
        validator: EditorValidator,
    ) -> Result<(), ControlError> {
        self.editor_control(id)?;
        let previous = self.controls[&id].validator.clone();
        self.set_editor_validator(
            id,
            Some(EditorValidator::new(move |text| {
                if let Some(previous) = &previous {
                    previous.check(text)?;
                }
                validator.check(text)
            })),
        )
    }
    pub fn spin_box(
        &mut self,
        parent: WidgetId,
        label: impl Into<String>,
        value: f64,
        settings: SpinBoxSettings,
        style: LayoutStyle,
    ) -> Result<EditorControl, ControlError> {
        settings.validate()?;
        settings
            .parse(&value.to_string())
            .map_err(ControlError::Invalid)?;
        let mut parts =
            self.create_editor_parts(parent, label.into(), &value.to_string(), style)?;
        self.add_step_buttons(&mut parts)?;
        self.set_role(parts.editor, Role::SpinButton);
        self.rich.composites.insert(
            parts.container,
            Composite {
                parts: parts.clone(),
                kind: CompositeKind::Spin(settings),
                read_only: false,
            },
        );
        self.rich
            .value_baselines
            .insert(parts.container, EditorValue::Number(value));
        Ok(parts)
    }

    pub(super) fn add_step_buttons(
        &mut self,
        parts: &mut EditorControl,
    ) -> Result<(), ControlError> {
        for (label, icon, direction) in [
            ("Уменьшить", IconSymbol::Remove, -1),
            ("Увеличить", IconSymbol::Add, 1),
        ] {
            let id = self.add_editor_button(parts, label, Some(icon))?;
            self.rich.actions.insert(
                id,
                RichAction::Step {
                    owner: parts.container,
                    direction,
                },
            );
            parts.buttons.push(id);
        }
        Ok(())
    }
    pub fn spin_value(&self, id: WidgetId) -> Option<f64> {
        let c = self.rich.composites.get(&id)?;
        let CompositeKind::Spin(settings) = &c.kind else {
            return None;
        };
        settings.parse(self.accepted_value(c.parts.editor)?).ok()
    }

    pub fn set_spin_value(
        &mut self,
        id: WidgetId,
        value: f64,
    ) -> Result<CommitOutcome, ControlError> {
        let c = self
            .rich
            .composites
            .get(&id)
            .ok_or_else(|| ControlError::Invalid("Not a SpinBox".into()))?;
        if !matches!(c.kind, CompositeKind::Spin(_)) {
            return Err(ControlError::Invalid("Not a SpinBox".into()));
        }
        self.set_editor_value(c.parts.editor, &value.to_string())
    }

    pub fn spin_settings(&self, id: WidgetId) -> Option<&SpinBoxSettings> {
        match &self.rich.composites.get(&id)?.kind {
            CompositeKind::Spin(s) => Some(s),
            _ => None,
        }
    }

    pub fn set_spin_settings(
        &mut self,
        id: WidgetId,
        settings: SpinBoxSettings,
    ) -> Result<(), ControlError> {
        settings.validate()?;
        let value = self
            .spin_value(id)
            .ok_or_else(|| ControlError::Invalid("Not a SpinBox".into()))?;
        settings
            .parse(&value.to_string())
            .map_err(ControlError::Invalid)?;
        self.rich.composites.get_mut(&id).unwrap().kind = CompositeKind::Spin(settings);
        Ok(())
    }

    pub(super) fn validate_rich_value(&self, id: WidgetId, text: &str) -> Result<(), String> {
        let Some(owner) = self.rich_owner(id) else {
            return Ok(());
        };
        let c = &self.rich.composites[&owner];
        if c.parts.editor != id {
            return Ok(());
        }
        if self.parse_temporal_draft(owner, text)?.is_some() {
            return Ok(());
        }
        match &c.kind {
            CompositeKind::Numeric(s) => {
                s.parse(text)?;
            }
            CompositeKind::Spin(s) => {
                s.parse(text)?;
            }
            CompositeKind::Date(s) => {
                s.parse(text)?;
            }
            CompositeKind::Time(s) => {
                s.parse(text)?;
            }
            CompositeKind::Combo {
                items,
                editable,
                lookup,
                keys,
                ..
            } => {
                if (!editable || *lookup)
                    && !text.is_empty()
                    && !items.iter().any(|i| {
                        i.text == text
                            && (i.enabled || self.rich.suppress_combo_commit == Some(owner))
                    })
                {
                    return Err("Выберите значение из списка.".into());
                }
                if self.rich.suppress_combo_commit != Some(owner) {
                    let next = items
                        .iter()
                        .find(|i| i.text == text && i.enabled && keys.contains(&i.key))
                        .or_else(|| items.iter().find(|i| i.text == text && i.enabled))
                        .map(|i| vec![i.key.clone()])
                        .unwrap_or_default();
                    if &next != keys
                        && let Some(validator) = self.rich.selection_validators.get(&owner)
                    {
                        validator.check(&next)?;
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }
    pub(super) fn rich_value_committed(&mut self, editor: WidgetId) -> Result<(), ControlError> {
        let Some(owner) = self.rich_owner(editor) else {
            return Ok(());
        };
        let c = self.rich.composites[&owner].clone();
        let text = self.accepted_value(editor).unwrap_or("").to_owned();
        let temporal = self
            .parse_temporal_draft(owner, &text)
            .map_err(ControlError::Invalid)?;
        let value = if temporal.is_some() {
            temporal
        } else {
            match c.kind {
                CompositeKind::Numeric(s) => Some(EditorValue::Decimal(
                    s.parse(&text).map_err(ControlError::Invalid)?,
                )),
                CompositeKind::Spin(s) => Some(EditorValue::Number(
                    s.parse(&text).map_err(ControlError::Invalid)?,
                )),
                CompositeKind::Date(s) => Some(EditorValue::Date(
                    s.parse(&text).map_err(ControlError::Invalid)?,
                )),
                CompositeKind::Time(s) => Some(EditorValue::Time(
                    s.parse(&text).map_err(ControlError::Invalid)?,
                )),
                CompositeKind::Combo { items, keys, .. }
                    if self.rich.suppress_combo_commit != Some(owner) =>
                {
                    let next = items
                        .iter()
                        .find(|i| i.text == text && i.enabled && keys.contains(&i.key))
                        .or_else(|| items.iter().find(|i| i.text == text && i.enabled))
                        .map(|i| vec![i.key.clone()])
                        .unwrap_or_default();
                    if keys != next {
                        if let CompositeKind::Combo { keys, .. } =
                            &mut self.rich.composites.get_mut(&owner).unwrap().kind
                        {
                            keys.clone_from(&next)
                        }
                        self.rich.changes.push(SelectionChanged {
                            id: owner,
                            keys: next,
                        });
                    }
                    None
                }
                _ => None,
            }
        };
        if let Some(value) = value
            && self.rich.value_baselines.get(&owner) != Some(&value)
        {
            self.rich.value_baselines.insert(owner, value.clone());
            self.rich.values.push(ValueChanged { id: owner, value });
            self.refresh_temporal_parser(owner);
        }
        Ok(())
    }
    pub(super) fn step_value(
        &mut self,
        owner: WidgetId,
        direction: i32,
    ) -> Result<(), ControlError> {
        if !self.rich_editable(owner) {
            return Ok(());
        }
        let c = self.rich.composites[&owner].clone();
        if self
            .document(c.parts.editor)
            .is_some_and(|d| d.composition().is_some())
        {
            return Ok(());
        }
        let draft = self.document(c.parts.editor).unwrap().text().to_owned();
        match c.kind {
            CompositeKind::Numeric(s) => {
                let Ok(current) = s.parse(&draft) else {
                    self.commit_edit(c.parts.editor)?;
                    return Ok(());
                };
                let next = s
                    .stepped(current, direction > 0)
                    .map_err(ControlError::Invalid)?;
                let text = s.format(Some(next))?;
                self.set_text(c.parts.editor, &text)?;
                self.commit_edit(c.parts.editor)?;
            }
            CompositeKind::Spin(s) => {
                let Ok(current) = s.parse(&draft) else {
                    self.commit_edit(c.parts.editor)?;
                    return Ok(());
                };
                let next = (current + s.step * direction as f64).clamp(s.minimum, s.maximum);
                self.set_spin_value(owner, next)?;
            }
            CompositeKind::Time(s) => {
                let Ok(Some(EditorValue::Time(current))) = self.parse_temporal_draft(owner, &draft)
                else {
                    self.commit_edit(c.parts.editor)?;
                    return Ok(());
                };
                let minimum = s.minimum.map_or(0, TimeValue::seconds);
                let maximum = s.maximum.map_or_else(
                    || {
                        if !s.show_seconds && s.step_seconds.is_multiple_of(60) {
                            86340 + current.map_or(0, |value| i32::from(value.second))
                        } else {
                            86399
                        }
                    },
                    TimeValue::seconds,
                );
                let current = current.map_or(minimum, TimeValue::seconds);
                let next = (current + s.step_seconds as i32 * direction).clamp(minimum, maximum);
                self.set_time_value(owner, Some(TimeValue::from_seconds(next)))?;
            }
            CompositeKind::DateTime(s) => {
                let Ok(Some(EditorValue::DateTime(current))) =
                    self.parse_temporal_draft(owner, &draft)
                else {
                    self.commit_edit(c.parts.editor)?;
                    return Ok(());
                };
                let min = s.minimum.unwrap_or(DateTimeValue {
                    date: DateValue {
                        year: 1,
                        month: 1,
                        day: 1,
                    },
                    time: TimeValue {
                        hour: 0,
                        minute: 0,
                        second: 0,
                    },
                });
                let max = s.maximum.unwrap_or(DateTimeValue {
                    date: DateValue {
                        year: 9999,
                        month: 12,
                        day: 31,
                    },
                    time: TimeValue {
                        hour: 23,
                        minute: 59,
                        second: 59,
                    },
                });
                let current = current.unwrap_or(min);
                let next = current
                    .checked_add_seconds(i64::from(s.step_seconds) * i64::from(direction))
                    .unwrap_or(if direction < 0 { min } else { max })
                    .max(min)
                    .min(max);
                self.set_date_time_value(owner, Some(next))?;
            }
            _ => {}
        }
        Ok(())
    }
}
