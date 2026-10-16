use super::selection::CompositeKind;
use super::*;
use crate::DataValue;
use std::{cell::RefCell, rc::Rc};

#[derive(Clone)]
pub(crate) struct TemporalValueParser(Rc<RefCell<Context>>);
#[derive(Clone)]
struct Context {
    kind: CompositeKind,
    accepted: EditorValue,
    assignment: Option<EditorValue>,
}
pub(crate) struct TemporalAssignmentGuard {
    parser: TemporalValueParser,
    previous: Option<EditorValue>,
}
impl Drop for TemporalAssignmentGuard {
    fn drop(&mut self) {
        self.parser.assignment(self.previous.take());
    }
}
impl TemporalValueParser {
    pub(crate) fn assign_storage(
        &self,
        value: &DataValue,
    ) -> Result<TemporalAssignmentGuard, String> {
        let typed = self.parse_storage(value)?;
        Ok(TemporalAssignmentGuard {
            parser: self.clone(),
            previous: self.assignment(Some(typed)),
        })
    }
    pub(crate) fn parse(&self, text: &str) -> Result<EditorValue, String> {
        let context = self.0.borrow();
        if let Some(value) = &context.assignment {
            return Ok(value.clone());
        }
        let value = match &context.kind {
            CompositeKind::Date(settings) => EditorValue::Date(settings.parse(text)?),
            CompositeKind::Time(settings) => {
                let mut value = if text.trim().is_empty() {
                    None
                } else {
                    Some(TimeValue::parse(text).map_err(|e| e.to_string())?)
                };
                if !settings.show_seconds
                    && text.trim().len() == 5
                    && let Some(time) = &mut value
                    && let EditorValue::Time(Some(previous)) = context.accepted
                    && (previous.hour, previous.minute) == (time.hour, time.minute)
                {
                    time.second = previous.second;
                }
                EditorValue::Time(value)
            }
            CompositeKind::DateTime(settings) => {
                let mut value = if text.trim().is_empty() {
                    None
                } else {
                    Some(
                        DateTimeValue::parse_display(text, settings.date_format)
                            .map_err(|e| e.to_string())?,
                    )
                };
                if !settings.show_seconds
                    && text.trim().len() == 16
                    && let Some(value) = &mut value
                    && let EditorValue::DateTime(Some(previous)) = context.accepted
                    && (previous.time.hour, previous.time.minute)
                        == (value.time.hour, value.time.minute)
                {
                    value.time.second = previous.time.second;
                }
                EditorValue::DateTime(value)
            }
            CompositeKind::DateRange(settings) => EditorValue::DateRange(settings.parse(text)?),
            _ => return Err("Not a temporal editor".into()),
        };
        Self::check(&context.kind, &value)?;
        Ok(value)
    }
    pub(crate) fn parse_storage(&self, value: &DataValue) -> Result<EditorValue, String> {
        let context = self.0.borrow();
        let text = || match value {
            DataValue::String(text) => Ok(text.as_str()),
            _ => Err("Expected canonical date/time string or null".to_owned()),
        };
        let parsed = match &context.kind {
            CompositeKind::Date(_) => EditorValue::Date(if *value == DataValue::Null {
                None
            } else {
                Some(DateValue::parse_iso(text()?).map_err(|e| e.to_string())?)
            }),
            CompositeKind::Time(_) => EditorValue::Time(if *value == DataValue::Null {
                None
            } else {
                Some(TimeValue::parse(text()?).map_err(|e| e.to_string())?)
            }),
            CompositeKind::DateTime(_) => EditorValue::DateTime(if *value == DataValue::Null {
                None
            } else {
                Some(DateTimeValue::parse_iso(text()?).map_err(|e| e.to_string())?)
            }),
            CompositeKind::DateRange(_) => EditorValue::DateRange(if *value == DataValue::Null {
                None
            } else {
                let DataValue::Object(fields) = value else {
                    return Err("Range requires object {start,end} or null".into());
                };
                if fields.len() != 2 {
                    return Err("Range requires exactly start and end".into());
                }
                let field = |name: &str| match fields.get(name) {
                    Some(DataValue::String(text)) => {
                        DateValue::parse_iso(text).map_err(|e| e.to_string())
                    }
                    _ => Err("Range endpoints require canonical date strings".into()),
                };
                Some(
                    DateRangeValue::new(field("start")?, field("end")?)
                        .map_err(|e| e.to_string())?,
                )
            }),
            _ => return Err("Not a temporal editor".into()),
        };
        Self::check(&context.kind, &parsed)?;
        Ok(parsed)
    }
    pub(crate) fn format_value(&self, value: &EditorValue) -> Result<String, String> {
        let context = self.0.borrow();
        Self::check(&context.kind, value)?;
        Ok(match (&context.kind, value) {
            (CompositeKind::Date(s), EditorValue::Date(v)) => {
                v.map_or(String::new(), |v| v.to_display(s.format))
            }
            (CompositeKind::Time(s), EditorValue::Time(v)) => {
                v.map_or(String::new(), |v| v.to_display(s.show_seconds))
            }
            (CompositeKind::DateTime(s), EditorValue::DateTime(v)) => v
                .map_or(String::new(), |v| {
                    v.to_display(s.date_format, s.show_seconds)
                }),
            (CompositeKind::DateRange(s), EditorValue::DateRange(v)) => {
                v.map_or(String::new(), |v| v.to_display(s.format))
            }
            _ => return Err("Temporal value kind mismatch".into()),
        })
    }
    fn check(kind: &CompositeKind, value: &EditorValue) -> Result<(), String> {
        match (kind, value) {
            (CompositeKind::Date(s), EditorValue::Date(v)) => s.check(*v),
            (CompositeKind::Time(s), EditorValue::Time(v)) => s.check(*v),
            (CompositeKind::DateTime(s), EditorValue::DateTime(v)) => s.check(*v),
            (CompositeKind::DateRange(s), EditorValue::DateRange(v)) => s.check(*v),
            _ => Err("Temporal value kind mismatch".into()),
        }
    }
    pub(super) fn assignment(&self, value: Option<EditorValue>) -> Option<EditorValue> {
        std::mem::replace(&mut self.0.borrow_mut().assignment, value)
    }
}

impl Controls {
    pub(crate) fn temporal_value_parser(&self, id: WidgetId) -> Option<TemporalValueParser> {
        self.temporal_parsers.get(&id).cloned()
    }
    pub(super) fn refresh_temporal_parser(&mut self, id: WidgetId) {
        let Some(composite) = self.rich.composites.get(&id) else {
            self.temporal_parsers.remove(&id);
            return;
        };
        if !matches!(
            composite.kind,
            CompositeKind::Date(_)
                | CompositeKind::Time(_)
                | CompositeKind::DateTime(_)
                | CompositeKind::DateRange(_)
        ) {
            return;
        }
        let Some(value) = self.rich.value_baselines.get(&id) else {
            return;
        };
        if let Some(parser) = self.temporal_parsers.get(&id) {
            let mut context = parser.0.borrow_mut();
            context.kind = composite.kind.clone();
            context.accepted = value.clone();
        } else {
            self.temporal_parsers.insert(
                id,
                TemporalValueParser(Rc::new(RefCell::new(Context {
                    kind: composite.kind.clone(),
                    accepted: value.clone(),
                    assignment: None,
                }))),
            );
        }
    }
}
