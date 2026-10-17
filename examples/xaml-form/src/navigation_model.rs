//! Application commands for one retained navigation workspace.
use crate::view::MountedView;
use rust_desktop_ui_controls::{
    BindingContext, Controls, DataValue, NavigationValidator, NavigationView,
};
use rust_desktop_ui_core::WidgetId;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

pub(crate) fn model() -> Result<Rc<RefCell<BindingContext>>, String> {
    BindingContext::new(DataValue::object([
        ("Route", DataValue::from("home")),
        ("Draft", DataValue::from("Текст рабочего черновика")),
    ]))
    .map(|c| Rc::new(RefCell::new(c)))
    .map_err(err)
}
pub(crate) fn register(ui: &mut Controls) {
    for (id, label) in [
        ("NavigationSide", "Боковая панель"),
        ("NavigationAccordion", "Аккордеон"),
        ("NavigationHamburger", "Компактно"),
        ("NavigationOffice", "Панель разделов"),
        ("NavigationFrame", "Только страница"),
        ("NavigationExternal", "Открыть параметры"),
        ("NavigationAvailability", "Доступность параметров"),
        ("NavigationGuard", "Запретить переход в документ"),
    ] {
        ui.commands_mut().register(id, label);
    }
}
pub(crate) struct State {
    owner: WidgetId,
    status: WidgetId,
    context: Rc<RefCell<BindingContext>>,
    settings_enabled: bool,
    guard: Rc<Cell<bool>>,
}
impl State {
    pub(crate) fn install(ui: &mut Controls, view: &MountedView) -> Result<Option<Self>, String> {
        let Some(owner) = view.find_name("Navigation") else {
            return Ok(None);
        };
        let guard = Rc::new(Cell::new(false));
        let check = Rc::clone(&guard);
        ui.set_navigation_validator(
            owner,
            Some(NavigationValidator::new(move |route| {
                if route == "document" && check.get() {
                    Err("Переход в документ временно запрещён".into())
                } else {
                    Ok(())
                }
            })),
        )
        .map_err(err)?;
        Ok(Some(Self {
            owner,
            status: view
                .find_name("NavigationStatus")
                .ok_or("Missing NavigationStatus")?,
            context: view
                .bindings
                .as_ref()
                .ok_or("Navigation bindings missing")?
                .context(),
            settings_enabled: true,
            guard,
        }))
    }
    pub(crate) fn command(
        &mut self,
        ui: &mut Controls,
        view: &mut MountedView,
        command: &str,
    ) -> Result<bool, String> {
        let mode = match command {
            "NavigationSide" => Some(NavigationView::Side),
            "NavigationAccordion" => Some(NavigationView::Accordion),
            "NavigationHamburger" => Some(NavigationView::Hamburger),
            "NavigationOffice" => Some(NavigationView::Office),
            "NavigationFrame" => Some(NavigationView::Frame),
            _ => None,
        };
        if let Some(mode) = mode {
            ui.set_navigation_view(self.owner, mode).map_err(err)?;
        } else if command == "NavigationExternal" {
            self.context
                .borrow_mut()
                .set("Route", "settings".into())
                .map_err(err)?;
            view.sync_bindings(ui);
        } else if command == "NavigationAvailability" {
            let enabled = !self.settings_enabled;
            ui.set_navigation_page_enabled(self.owner, "settings", enabled)
                .map_err(err)?;
            self.settings_enabled = enabled;
        } else if command == "NavigationGuard" {
            self.guard.set(!self.guard.get());
        } else {
            return Ok(false);
        }
        self.sync(ui)?;
        Ok(true)
    }
    pub(crate) fn sync(&mut self, ui: &mut Controls) -> Result<(), String> {
        let model = ui
            .navigation_model(self.owner)
            .ok_or("Navigation removed")?;
        let text = format!(
            "Маршрут: {} · история: {}/{} · вид: {:?} · параметры: {} · переход в документ: {}{}",
            model.route(),
            model.history_position() + 1,
            model.history().len(),
            ui.navigation_settings(self.owner).unwrap().view,
            if self.settings_enabled {
                "доступны"
            } else {
                "отключены"
            },
            if self.guard.get() {
                "запрещён"
            } else {
                "разрешён"
            },
            ui.navigation_error(self.owner)
                .map_or_else(String::new, |error| format!(" · {error}"))
        );
        ui.set_label(self.status, text).map_err(err)
    }
}
fn err(error: impl std::fmt::Display) -> String {
    error.to_string()
}
