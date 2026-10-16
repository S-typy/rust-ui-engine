//! Application-owned rating values and one business validation rule.
use crate::view::MountedView;
use rust_desktop_ui_controls::{
    BindingContext, BindingValidator, Controls, DataValue, RatingPreview, SemanticAction,
};
use rust_desktop_ui_core::{Mutation, WidgetId};
use std::{cell::RefCell, rc::Rc};

pub(crate) fn model() -> Result<Rc<RefCell<BindingContext>>, String> {
    let mut model = BindingContext::new(DataValue::object([
        ("Score", DataValue::Number(2.5)),
        ("Locked", false.into()),
    ]))
    .map_err(err)?;
    model
        .set_field_validator(
            "Score",
            Some(BindingValidator::new(|value| match value {
                DataValue::Null => Ok(()),
                DataValue::Number(value) if value.is_finite() && (0.0..=4.0).contains(value) => {
                    Ok(())
                }
                _ => Err("Оценка качества не должна превышать 4 из 5".into()),
            })),
        )
        .map_err(err)?;
    Ok(Rc::new(RefCell::new(model)))
}
pub(crate) fn register(ui: &mut Controls) {
    for (id, label) in [
        ("RatingDemoEmpty", "Без оценки"),
        ("RatingDemoZero", "Ноль"),
        ("RatingDemoFraction", "2,5"),
        ("RatingDemoReject", "Проверить 5"),
        ("RatingDemoFocus", "Клавиатура"),
    ] {
        ui.commands_mut().register(id, label);
    }
}
pub(crate) struct State {
    owner: WidgetId,
    accepted: WidgetId,
    preview: WidgetId,
    error: WidgetId,
    activity: WidgetId,
    context: Rc<RefCell<BindingContext>>,
    changes: u64,
}
impl State {
    pub(crate) fn install(ui: &mut Controls, view: &MountedView) -> Result<Option<Self>, String> {
        let Some(owner) = view.find_name("QualityScore") else {
            return Ok(None);
        };
        let required = |name| {
            view.find_name(name)
                .ok_or_else(|| format!("Missing {name}"))
        };
        let mut state = Self {
            owner,
            accepted: required("RatingAccepted")?,
            preview: required("RatingPreview")?,
            error: required("RatingError")?,
            activity: required("RatingActivity")?,
            context: view
                .bindings
                .as_ref()
                .ok_or("Rating requires bindings")?
                .context(),
            changes: 0,
        };
        state.refresh(ui)?;
        Ok(Some(state))
    }
    pub(crate) fn refresh(&mut self, ui: &mut Controls) -> Result<(), String> {
        for event in ui.drain_rating_changed() {
            if event.id == self.owner {
                self.changes = self.changes.saturating_add(1);
            }
        }
        let value = ui.rating_value(self.owner).ok_or("Rating removed")?;
        let preview = match ui.rating_preview(self.owner) {
            Some(RatingPreview::Value(value)) => {
                format!("Предпросмотр: {} · значение ещё не принято", display(value))
            }
            _ => "Предпросмотр не активен".into(),
        };
        let error = ui
            .rating_error(self.owner)
            .unwrap_or("Ошибок нет")
            .to_owned();
        set_label(ui, self.accepted, format!("Принято: {}", display(value)))?;
        set_label(ui, self.preview, preview)?;
        set_label(ui, self.error, error)?;
        set_label(
            ui,
            self.activity,
            format!("Подтверждений: {}", self.changes),
        )
    }
    pub(crate) fn command(&mut self, ui: &mut Controls, command: &str) -> Result<bool, String> {
        match command {
            "RatingDemoEmpty" => {
                self.context
                    .borrow_mut()
                    .set("Score", DataValue::Null)
                    .map_err(err)?;
            }
            "RatingDemoZero" => {
                self.context
                    .borrow_mut()
                    .set("Score", DataValue::Number(0.0))
                    .map_err(err)?;
            }
            "RatingDemoFraction" => {
                self.context
                    .borrow_mut()
                    .set("Score", DataValue::Number(2.5))
                    .map_err(err)?;
            }
            "RatingDemoReject" => {
                ui.perform_action(self.owner, SemanticAction::SetValue, Some("5"))
                    .map_err(err)?;
            }
            "RatingDemoFocus" => {
                ui.runtime
                    .apply(Mutation::Focus(Some(self.owner)))
                    .map_err(err)?;
            }
            _ => return Ok(false),
        };
        Ok(true)
    }
}
fn display(value: Option<f64>) -> String {
    value.map_or_else(
        || "без оценки".into(),
        |value| format!("{} из 5", value.to_string().replace('.', ",")),
    )
}
fn set_label(ui: &mut Controls, id: WidgetId, value: String) -> Result<(), String> {
    if ui.label_text(id) != Some(value.as_str()) {
        ui.set_label(id, value).map_err(err)?;
    }
    Ok(())
}
fn err(error: impl std::fmt::Display) -> String {
    error.to_string()
}
