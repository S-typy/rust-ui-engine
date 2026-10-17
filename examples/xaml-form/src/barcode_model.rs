//! Explicit sample updates for the barcode window.
use crate::view::MountedView;
use rust_desktop_ui_controls::Controls;
use rust_desktop_ui_core::WidgetId;

pub(crate) fn register(ui: &mut Controls) {
    for (id, label) in [
        ("BarcodeChange", "Другое значение"),
        ("BarcodeReject", "Неверная цифра"),
        ("BarcodeRestore", "Восстановить"),
    ] {
        ui.commands_mut().register(id, label);
    }
}
pub(crate) struct State {
    code: WidgetId,
    status: WidgetId,
    message: String,
}
impl State {
    pub(crate) fn install(ui: &mut Controls, view: &MountedView) -> Result<Option<Self>, String> {
        let Some(code) = view.find_name("BarcodeEan13") else {
            return Ok(None);
        };
        let status = view
            .find_name("BarcodeStatus")
            .ok_or("Missing BarcodeStatus")?;
        let mut state = Self {
            code,
            status,
            message: "Готово".into(),
        };
        state.refresh(ui)?;
        Ok(Some(state))
    }
    pub(crate) fn command(&mut self, ui: &mut Controls, command: &str) -> Result<bool, String> {
        let value = match command {
            "BarcodeChange" => "590123412345",
            "BarcodeRestore" => "400638133393",
            "BarcodeReject" => "4006381333932",
            _ => return Ok(false),
        };
        self.message = match ui.set_barcode_value(self.code, value) {
            Ok(()) => "Принято".into(),
            Err(e) => format!("Отклонено: {e}"),
        };
        self.refresh(ui)?;
        Ok(true)
    }
    pub(crate) fn refresh(&mut self, ui: &mut Controls) -> Result<(), String> {
        ui.set_label(
            self.status,
            format!(
                "{} · {}",
                self.message,
                ui.barcode_value(self.code).unwrap_or("")
            ),
        )
        .map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod host_tests {
    #[test]
    fn barcode_demo_mounts_through_real_form_and_canonical_payload() {
        let definition =
            rust_desktop_ui_xaml::parse(include_str!("../BarcodeWindow.xaml")).unwrap();
        let mut form = crate::form::Form::new(definition, false).unwrap();
        form.ui
            .update(rust_desktop_ui_core::Size::new(1000.0, 800.0))
            .unwrap();
        let owner = form.view.find_name("BarcodeEan13").unwrap();
        assert_eq!(form.ui.barcode_value(owner), Some("4006381333931"));
        assert!(form.ui.barcode_geometry(owner).is_some());
    }
}
