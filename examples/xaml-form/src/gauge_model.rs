//! Explicit application updates for four read-only measurement views.
use crate::view::MountedView;
use rust_desktop_ui_controls::Controls;
use rust_desktop_ui_core::{Length, Mutation, WidgetId};
pub(crate) fn register(ui: &mut Controls) {
    for (id, label) in [
        ("GaugeLow", "Норма 35"),
        ("GaugeHigh", "Высокое 85"),
        ("GaugeInvalid", "Вне шкалы"),
        ("GaugeCompact", "Компактно"),
        ("GaugeRestore", "Развернуть"),
    ] {
        ui.commands_mut().register(id, label);
    }
}
pub(crate) struct State {
    ids: Vec<WidgetId>,
    status: WidgetId,
    message: String,
}
impl State {
    pub(crate) fn install(ui: &mut Controls, view: &MountedView) -> Result<Option<Self>, String> {
        if view.find_name("GaugeCircular").is_none() {
            return Ok(None);
        }
        let ids = ["GaugeCircular", "GaugeLinear", "GaugeDigital", "GaugeState"]
            .into_iter()
            .map(|name| {
                view.find_name(name)
                    .ok_or_else(|| format!("Missing {name}"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut state = Self {
            ids,
            status: view.find_name("GaugeStatus").ok_or("Missing GaugeStatus")?,
            message: "Готово".into(),
        };
        state.refresh(ui)?;
        Ok(Some(state))
    }
    pub(crate) fn command(&mut self, ui: &mut Controls, command: &str) -> Result<bool, String> {
        match command {
            "GaugeLow" | "GaugeHigh" => {
                let value = if command == "GaugeLow" { 35.0 } else { 85.0 };
                for id in &self.ids {
                    ui.set_gauge_value(*id, value).map_err(|e| e.to_string())?;
                }
                self.message = "Принято".into();
            }
            "GaugeInvalid" => {
                let before = ui.gauge_value(self.ids[0]);
                let rejected = ui.set_gauge_value(self.ids[0], 101.0).is_err();
                if !rejected || ui.gauge_value(self.ids[0]) != before {
                    return Err("Invalid gauge update was published".into());
                }
                self.message = "Отклонено без изменения значения".into();
            }
            "GaugeCompact" | "GaugeRestore" => {
                for (index, id) in self.ids.iter().enumerate() {
                    let mut style = ui
                        .runtime
                        .tree()
                        .node(*id)
                        .ok_or("Removed gauge")?
                        .props
                        .style
                        .clone();
                    style.width = if command == "GaugeCompact" {
                        Length::Px(150.0)
                    } else {
                        Length::Percent(1.0)
                    };
                    style.height = Length::Px(if command == "GaugeCompact" {
                        52.0
                    } else {
                        [260.0, 180.0, 140.0, 140.0][index]
                    });
                    ui.runtime
                        .apply(Mutation::SetStyle { node: *id, style })
                        .map_err(|e| e.to_string())?;
                }
                self.message = "Размер изменён; значение сохранено".into();
            }
            _ => return Ok(false),
        }
        self.refresh(ui)?;
        Ok(true)
    }
    pub(crate) fn refresh(&mut self, ui: &mut Controls) -> Result<(), String> {
        ui.set_label(
            self.status,
            format!(
                "{} · {}",
                self.message,
                ui.gauge_semantic_value(self.ids[0]).unwrap_or("")
            ),
        )
        .map_err(|e| e.to_string())
    }
}
