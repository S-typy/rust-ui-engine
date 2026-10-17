//! Application-owned exact colors and one whole-value validation rule.
use crate::view::MountedView;
use rust_desktop_ui_controls::{
    BindingContext, BindingValidator, BrushValue, ColorReplacePolicy, Controls, DataValue,
    GradientStop, LinearGradient, Srgba8, brush_to_value, color_from_value, color_to_value,
};
use rust_desktop_ui_core::WidgetId;
use std::{cell::RefCell, rc::Rc};

fn initial_color() -> Srgba8 {
    Srgba8::new(30, 136, 229, 180)
}
fn initial_brush() -> BrushValue {
    BrushValue::Linear(
        LinearGradient::new(
            0.0,
            vec![
                GradientStop::new(0.0, Srgba8::new(30, 136, 229, 40)),
                GradientStop::new(0.5, Srgba8::rgb(30, 136, 229)),
                GradientStop::new(0.5, Srgba8::rgb(253, 216, 53)),
                GradientStop::new(1.0, Srgba8::rgb(216, 27, 96)),
            ],
        )
        .unwrap(),
    )
}
pub(crate) fn model() -> Result<Rc<RefCell<BindingContext>>, String> {
    let mut context = BindingContext::new(DataValue::object([
        ("Fill", color_to_value(Some(initial_color()))),
        ("Brush", brush_to_value(Some(&initial_brush()))),
        ("Locked", false.into()),
    ]))
    .map_err(err)?;
    context
        .set_field_validator(
            "Fill",
            Some(BindingValidator::new(|value| {
                let color = color_from_value(value)?;
                if color.is_some_and(|c| c.r > 240 && c.g < 30 && c.b < 30) {
                    Err("Насыщенный красный запрещён правилом формы".into())
                } else {
                    Ok(())
                }
            })),
        )
        .map_err(err)?;
    Ok(Rc::new(RefCell::new(context)))
}
pub(crate) fn register(ui: &mut Controls) {
    for (id, label) in [
        ("ColorDemoNone", "Нет значения"),
        ("ColorDemoTransparent", "Прозрачный синий"),
        ("ColorDemoReject", "Проверить красный"),
        ("ColorDemoReset", "Восстановить значения"),
        ("ColorDemoBrush", "Открыть кисть"),
    ] {
        ui.commands_mut().register(id, label);
    }
}
pub(crate) struct State {
    color: WidgetId,
    brush: WidgetId,
    accepted: WidgetId,
    brush_accepted: WidgetId,
    activity: WidgetId,
    error: WidgetId,
    changes: u64,
}
impl State {
    pub(crate) fn install(ui: &mut Controls, view: &MountedView) -> Result<Option<Self>, String> {
        let Some(color) = view.find_name("FillColor") else {
            return Ok(None);
        };
        let required = |name| {
            view.find_name(name)
                .ok_or_else(|| format!("Missing {name}"))
        };
        let brush = required("FillBrush")?;
        for (owner, label) in [(color, "Цвет заливки"), (brush, "Кисть заливки")]
        {
            let parts = ui.color_editor_parts(owner).ok_or("Color editor missing")?;
            ui.set_automation_name(parts.editor, label).map_err(err)?;
        }
        let mut state = Self {
            color,
            brush,
            accepted: required("ColorAccepted")?,
            brush_accepted: required("BrushAccepted")?,
            activity: required("ColorActivity")?,
            error: required("ColorError")?,
            changes: 0,
        };
        state.refresh(ui)?;
        Ok(Some(state))
    }
    pub(crate) fn refresh(&mut self, ui: &mut Controls) -> Result<(), String> {
        for event in ui.drain_brush_changes() {
            if event.id == self.color || event.id == self.brush {
                self.changes = self.changes.saturating_add(1);
            }
        }
        let color = ui.color_value(self.color).ok_or("Color removed")?;
        set_label(
            ui,
            self.accepted,
            color.map_or_else(
                || "Принято: нет значения".into(),
                |c| format!("Принято: {c} · RGBA {}, {}, {}, {}", c.r, c.g, c.b, c.a),
            ),
        )?;
        let brush = ui.brush_value(self.brush).ok_or("Brush removed")?;
        let summary = match brush {
            None => "Кисть: нет значения".into(),
            Some(BrushValue::Solid(c)) => format!("Сплошная кисть: {c}"),
            Some(BrushValue::Linear(g)) => format!(
                "Линейная кисть: {} точек · угол {}°",
                g.stops().len(),
                g.angle()
            ),
        };
        set_label(ui, self.brush_accepted, summary)?;
        set_label(
            ui,
            self.activity,
            format!("Подтверждений: {}", self.changes),
        )?;
        let error = ui
            .color_error(self.color)
            .or_else(|| ui.color_error(self.brush))
            .unwrap_or("Ошибок нет")
            .to_owned();
        set_label(ui, self.error, error)
    }
    pub(crate) fn command(&mut self, ui: &mut Controls, command: &str) -> Result<bool, String> {
        match command {
            "ColorDemoNone" => ui
                .set_brush_value_with_policy(self.color, None, ColorReplacePolicy::CancelPending)
                .map_err(err)?,
            "ColorDemoTransparent" => ui
                .set_brush_value_with_policy(
                    self.color,
                    Some(BrushValue::Solid(Srgba8::new(18, 52, 86, 0))),
                    ColorReplacePolicy::CancelPending,
                )
                .map_err(err)?,
            "ColorDemoReset" => {
                ui.set_brush_value_with_policy(
                    self.color,
                    Some(BrushValue::Solid(initial_color())),
                    ColorReplacePolicy::CancelPending,
                )
                .map_err(err)?;
                ui.set_brush_value_with_policy(
                    self.brush,
                    Some(initial_brush()),
                    ColorReplacePolicy::CancelPending,
                )
                .map_err(err)?;
            }
            "ColorDemoReject" => {
                if ui.open_color_popup(self.color).is_ok() {
                    ui.set_color_draft(self.color, Srgba8::rgb(255, 0, 0))
                        .map_err(err)?;
                    ui.apply_color_popup(self.color).map_err(err)?;
                }
            }
            "ColorDemoBrush" => {
                let _ = ui.open_color_popup(self.brush);
            }
            _ => return Ok(false),
        };
        Ok(true)
    }
}
fn set_label(ui: &mut Controls, id: WidgetId, value: String) -> Result<(), String> {
    if ui.label_text(id) != Some(&value) {
        ui.set_label(id, value).map_err(err)?;
    }
    Ok(())
}
fn err(error: impl std::fmt::Display) -> String {
    error.to_string()
}
