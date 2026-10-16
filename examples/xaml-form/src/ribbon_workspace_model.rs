//! Application-owned document commands and explicit Ribbon preferences.
use crate::view::MountedView;
use rust_desktop_ui_controls::{
    BindingContext, Controls, DataValue, RibbonGroup, RibbonItem, RibbonItemKind, RibbonSettings,
    RibbonTab,
};
use rust_desktop_ui_core::WidgetId;
use std::{cell::RefCell, rc::Rc};

pub(crate) fn model() -> Result<Rc<RefCell<BindingContext>>, String> {
    Ok(Rc::new(RefCell::new(
        BindingContext::new(DataValue::object([
            ("Title", "Новый документ".into()),
            ("Font", "Segoe UI".into()),
            ("Size", "14".into()),
            (
                "Fonts",
                DataValue::List(vec!["Segoe UI".into(), "Arial".into(), "Consolas".into()]),
            ),
        ]))
        .map_err(err)?,
    )))
}

pub(crate) fn register(ui: &mut Controls) {
    for (id, label) in [
        ("StudioSave", "Сохранить"),
        ("StudioExport", "Экспорт"),
        ("StudioPdf", "Документ PDF"),
        ("StudioText", "Текстовый документ"),
        ("StudioSaveSettings", "Запомнить QAT"),
        ("StudioRestoreSettings", "Вернуть QAT"),
        ("StudioExtension", "Команда расширения"),
    ] {
        ui.commands_mut().register(id, label);
    }
    ui.commands_mut()
        .register_toggle("StudioContext", "Контекст документа", false);
    ui.commands_mut()
        .register_toggle("StudioMerge", "Команды расширения", false);
}

pub(crate) struct State {
    status: WidgetId,
    settings: Option<String>,
    saves: usize,
}
impl State {
    pub(crate) fn install(view: &MountedView) -> Option<Self> {
        view.find_name("RibbonStudioStatus").map(|status| Self {
            status,
            settings: None,
            saves: 0,
        })
    }
    pub(crate) fn command(
        &mut self,
        ui: &mut Controls,
        view: &mut MountedView,
        command: &str,
    ) -> Result<bool, String> {
        if !command.starts_with("Studio") {
            return Ok(false);
        }
        let mounted = view.ribbon.as_mut().ok_or("Missing document Ribbon")?;
        let ribbon = &mut mounted.ribbon;
        let status = match command {
            "StudioSave" => {
                self.saves += 1;
                format!("Команда сохранения выполнена: {}", self.saves)
            }
            "StudioSaveSettings" => {
                self.settings = Some(ribbon.settings().to_json().map_err(err)?);
                "Настройки QAT запомнены в этом запуске".into()
            }
            "StudioRestoreSettings" => {
                if let Some(json) = &self.settings {
                    ribbon
                        .apply_settings(ui, &RibbonSettings::from_json(json).map_err(err)?)
                        .map_err(err)?;
                    "Настройки QAT восстановлены".into()
                } else {
                    "Сначала запомните настройки QAT".into()
                }
            }
            "StudioContext" => {
                let visible = ui.commands().checked(&"StudioContext".into()) == Some(true);
                ribbon.set_context(visible.then_some("Document"));
                format!(
                    "Контекст документа: {}",
                    if visible {
                        "показан"
                    } else {
                        "скрыт"
                    }
                )
            }
            "StudioMerge" => {
                let merged = ui.commands().checked(&"StudioMerge".into()) == Some(true);
                if merged {
                    ribbon
                        .merge_tabs(
                            ui,
                            "document-extension",
                            vec![RibbonTab {
                                id: "Extension".into(),
                                label: "Расширение".into(),
                                key_tip: "X".into(),
                                context: None,
                                groups: vec![RibbonGroup {
                                    id: "ExtensionTools".into(),
                                    label: "Команды документа".into(),
                                    collapse_priority: 0,
                                    items: vec![RibbonItem::new(
                                        "StudioExtension",
                                        RibbonItemKind::Large,
                                        "E",
                                    )],
                                }],
                            }],
                        )
                        .map_err(err)?;
                } else {
                    ribbon.unmerge_tabs(ui, "document-extension").map_err(err)?;
                }
                format!(
                    "Команды расширения: {}",
                    if merged {
                        "подключены"
                    } else {
                        "отключены"
                    }
                )
            }
            "StudioExport" | "StudioPdf" | "StudioText" | "StudioExtension" => {
                format!(
                    "Выполнена команда: {}",
                    ui.commands().label(&command.into()).unwrap_or(command)
                )
            }
            _ => return Ok(false),
        };
        ui.set_label(self.status, status).map_err(err)?;
        Ok(true)
    }
}
fn err(value: impl std::fmt::Display) -> String {
    value.to_string()
}
