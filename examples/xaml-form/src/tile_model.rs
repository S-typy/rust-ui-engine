//! Application-owned documents, command accounting and selection validation.
use crate::view::MountedView;
use rust_desktop_ui_controls::{
    BindingContext, BindingValidator, Controls, DataValue, IconSymbol, TileEventKind, TileItem,
    TileModel, TileSize,
};
use rust_desktop_ui_core::WidgetId;
use std::{cell::RefCell, rc::Rc};

fn items(large: bool) -> Vec<TileItem> {
    (0..if large { 100_000 } else { 24 })
        .map(|i| {
            let mut item = TileItem::new(format!("doc-{i}"), format!("Документ {}", i + 1));
            item.item.columns = vec![if large {
                "Коллекция документов".into()
            } else {
                format!("Раздел {} · версия {}", i / 6 + 1, i % 3 + 1)
            }];
            item.group = if i < if large { 50_000 } else { 12 } {
                "work"
            } else {
                "reference"
            }
            .into();
            item.size = if large {
                TileSize::Small
            } else {
                match i % 6 {
                    0 => TileSize::Large,
                    2 => TileSize::Wide,
                    4 => TileSize::Medium,
                    _ => TileSize::Small,
                }
            };
            item.icon = Some(if i % 2 == 0 {
                IconSymbol::Edit
            } else {
                IconSymbol::FolderOpen
            });
            item.command = Some("TilesDemoOpen".into());
            item.item.enabled = i != 7;
            item
        })
        .collect()
}
pub(crate) fn model() -> Result<Rc<RefCell<BindingContext>>, String> {
    let context = Rc::new(RefCell::new(
        BindingContext::new(DataValue::object([
            ("Selected", DataValue::Null),
            ("Locked", false.into()),
            ("Veto", true.into()),
        ]))
        .map_err(err)?,
    ));
    Ok(context)
}
pub(crate) fn register(ui: &mut Controls) {
    for (id, label) in [
        ("TilesDemoMany", "100 000 документов"),
        ("TilesDemoReset", "Пример с группами"),
        ("TilesDemoLast", "К последнему"),
        ("TilesDemoOpen", "Открыть документ"),
    ] {
        ui.commands_mut().register(id, label);
    }
}
pub(crate) struct State {
    owner: WidgetId,
    selected: WidgetId,
    status: WidgetId,
    geometry: WidgetId,
    error: WidgetId,
    opened: u64,
    reordered: u64,
    last: String,
    veto: Rc<RefCell<bool>>,
    context: Rc<RefCell<BindingContext>>,
}
impl State {
    pub(crate) fn install(ui: &mut Controls, view: &MountedView) -> Result<Option<Self>, String> {
        let Some(owner) = view.find_name("Documents") else {
            return Ok(None);
        };
        let groups = ui
            .tile_model(owner)
            .ok_or("Tile model missing")?
            .groups()
            .to_vec();
        ui.set_tile_model(owner, TileModel::new(groups, items(false)).map_err(err)?)
            .map_err(err)?;
        let context = view
            .bindings
            .as_ref()
            .ok_or("Tile demo requires bindings")?
            .context();
        let find = |name| {
            view.find_name(name)
                .ok_or_else(|| format!("Missing {name}"))
        };
        let veto = Rc::new(RefCell::new(true));
        let rule = veto.clone();
        context
            .borrow_mut()
            .set_field_validator(
                "Selected",
                Some(BindingValidator::new(move |value| {
                    if *rule.borrow() && *value == DataValue::String("doc-2".into()) {
                        Err("Документ 3 запрещён правилом формы".into())
                    } else {
                        Ok(())
                    }
                })),
            )
            .map_err(err)?;
        let mut state = Self {
            owner,
            selected: find("TilesSelected")?,
            status: find("TilesStatus")?,
            geometry: find("TilesGeometry")?,
            error: find("TilesError")?,
            opened: 0,
            reordered: 0,
            last: String::new(),
            veto,
            context,
        };
        state.refresh(ui)?;
        Ok(Some(state))
    }
    pub(crate) fn refresh(&mut self, ui: &mut Controls) -> Result<(), String> {
        *self.veto.borrow_mut() =
            matches!(self.context.borrow().get("Veto"), Ok(DataValue::Bool(true)));
        for event in ui.drain_tile_events() {
            if event.owner == self.owner {
                match event.kind {
                    TileEventKind::Activated { key, .. } => {
                        self.opened += 1;
                        self.last = key;
                    }
                    TileEventKind::Reordered { .. } => self.reordered += 1,
                    _ => {}
                }
            }
        }
        let selected = ui
            .tile_selected_keys(self.owner)
            .and_then(|keys| keys.first())
            .map_or("не выбран", String::as_str);
        label(ui, self.selected, format!("Документ: {selected}"))?;
        label(
            ui,
            self.status,
            format!(
                "Открытий: {} · перестановок: {} · последнее: {}",
                self.opened, self.reordered, self.last
            ),
        )?;
        if let Some(stats) = ui.tile_stats(self.owner) {
            label(
                ui,
                self.geometry,
                format!(
                    "Записей: {} · создано сейчас: {} · мест: {}",
                    stats.total, stats.realized, stats.allocated_slots
                ),
            )?;
        }
        label(
            ui,
            self.error,
            ui.tile_error(self.owner).unwrap_or("Ошибок нет").to_owned(),
        )?;
        Ok(())
    }
    pub(crate) fn command(&mut self, ui: &mut Controls, id: &str) -> Result<bool, String> {
        match id {
            "TilesDemoMany" | "TilesDemoReset" => {
                self.context
                    .borrow_mut()
                    .set("Selected", DataValue::Null)
                    .map_err(err)?;
                let groups = ui
                    .tile_model(self.owner)
                    .ok_or("Tile model missing")?
                    .groups()
                    .to_vec();
                // Large source ownership stays in the bounded TileModel rather
                // than duplicating 100000 records in the small form value tree.
                let model = TileModel::new(groups, items(id == "TilesDemoMany")).map_err(err)?;
                ui.set_tile_model(self.owner, model).map_err(err)?;
            }
            "TilesDemoLast" => {
                let key = ui
                    .tile_model(self.owner)
                    .and_then(|m| m.items().last())
                    .map(|t| t.item.key.clone());
                if let Some(key) = key {
                    ui.focus_tile(self.owner, &key).map_err(err)?;
                }
            }
            "TilesDemoOpen" => {}
            _ => return Ok(false),
        }
        self.refresh(ui)?;
        Ok(true)
    }
}
fn label(ui: &mut Controls, id: WidgetId, text: String) -> Result<(), String> {
    if ui.label_text(id) != Some(text.as_str()) {
        ui.set_label(id, text).map_err(err)?;
    }
    Ok(())
}
fn err(error: impl std::fmt::Display) -> String {
    error.to_string()
}
#[cfg(test)]
mod tests {
    use super::*;
    use rust_desktop_ui_platform_winit::{DesktopApp, PlatformEvent};
    #[test]
    fn real_form_large_source_command_and_reset_keep_bounded_bindings_and_editor() {
        let definition = rust_desktop_ui_xaml::parse(include_str!("../TileWindow.xaml")).unwrap();
        let context = model().unwrap();
        let mut form =
            crate::form::Form::new_with_model(definition, false, Some(context.clone())).unwrap();
        let size = rust_desktop_ui_core::Size::new(1120.0, 780.0);
        form.update(size, 1.0).unwrap();
        let owner = form.find_name("Documents").unwrap();
        let editor = form.find_name("TileDocument").unwrap();
        form.ui
            .set_text(editor, "Uncommitted source change")
            .unwrap();
        assert!(form.ui.commands_mut().activate(&"TilesDemoMany".into()));
        form.event(PlatformEvent::Tick).unwrap();
        form.update(size, 1.0).unwrap();
        assert_eq!(form.ui.tile_model(owner).unwrap().items().len(), 100000);
        assert!(form.ui.tile_stats(owner).unwrap().realized < 100);
        assert_eq!(
            form.ui.document(editor).unwrap().text(),
            "Uncommitted source change"
        );
        assert!(context.borrow().get("Rows").is_err());
        assert!(form.ui.commands_mut().activate(&"TilesDemoLast".into()));
        form.event(PlatformEvent::Tick).unwrap();
        form.update(size, 1.0).unwrap();
        assert!(form.ui.tile_item_widget(owner, "doc-99999").is_some());
        assert!(form.ui.commands_mut().activate(&"TilesDemoReset".into()));
        form.event(PlatformEvent::Tick).unwrap();
        form.update(size, 1.0).unwrap();
        assert_eq!(form.ui.tile_model(owner).unwrap().items().len(), 24);
        assert_eq!(
            form.ui.document(editor).unwrap().text(),
            "Uncommitted source change"
        );
    }
}
