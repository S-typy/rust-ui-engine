//! One application-owned gallery transferred between retained hosts.
use crate::view::MountedView;
use rust_desktop_ui_controls::{
    BindingContext, Controls, DataValue, IconSymbol, SelectionValidator,
};
use rust_desktop_ui_controls::{ImageFormat, ImageSource};
use rust_desktop_ui_core::{LayoutStyle, Length, Mutation, Size, WidgetId};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
pub(crate) fn model() -> Result<Rc<RefCell<BindingContext>>, String> {
    BindingContext::new(DataValue::object([("SelectedSample", "blue".into())]))
        .map(|c| Rc::new(RefCell::new(c)))
        .map_err(err)
}
pub(crate) fn register(ui: &mut Controls) -> Result<(), String> {
    for (id, label) in [
        ("GalleryHome", "В рабочую область"),
        ("GalleryRibbon", "В ленту"),
        ("GalleryPopup", "Все образцы"),
        ("GalleryReadOnly", "Только просмотр"),
        ("GalleryGuard", "Проверка красного образца"),
    ] {
        ui.commands_mut().register(id, label);
    }
    for (name, color) in [
        ("GalleryBlue", "#3388cc"),
        ("GalleryGreen", "#28a66a"),
        ("GalleryRed", "#e05045"),
    ] {
        let svg = format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="112" height="80"><rect x="1" y="1" width="110" height="78" rx="10" fill="{color}"/><path d="M18 52 L42 28 L58 44 L80 20 L96 52 Z" fill="white" fill-opacity="0.8"/></svg>"##
        );
        ui.register_image_source(
            name,
            ImageSource::from_bytes(svg.into_bytes(), ImageFormat::Svg).map_err(err)?,
        )
        .map_err(err)?;
    }
    Ok(())
}
pub(crate) struct State {
    owner: WidgetId,
    home: WidgetId,
    ribbon: WidgetId,
    status: WidgetId,
    preview: WidgetId,
    image: WidgetId,
    icon: WidgetId,
    guard: Rc<Cell<bool>>,
    accepted_changes: usize,
    host: &'static str,
    last: Option<String>,
}
impl State {
    pub(crate) fn install(ui: &mut Controls, view: &MountedView) -> Result<Option<Self>, String> {
        let Some(owner) = view.find_name("Samples") else {
            return Ok(None);
        };
        if ui.gallery_parts(owner).is_none() {
            return Ok(None);
        }
        let named = |name| {
            view.find_name(name)
                .ok_or_else(|| format!("Missing {name}"))
        };
        ui.configure_gallery_popup(owner, named("GalleryPopupButton")?, Size::new(560.0, 420.0))
            .map_err(err)?;
        let guard = Rc::new(Cell::new(false));
        let check = guard.clone();
        ui.set_gallery_selection_validator(
            owner,
            Some(SelectionValidator::new(move |keys| {
                if check.get() && keys.iter().any(|k| k == "red") {
                    Err("Красный образец временно недоступен".into())
                } else {
                    Ok(())
                }
            })),
        )
        .map_err(err)?;
        Ok(Some(Self {
            owner,
            home: named("GalleryStandaloneHost")?,
            ribbon: named("GalleryRibbonHost")?,
            status: named("GalleryStatus")?,
            preview: named("GalleryPreview")?,
            image: named("GalleryPreviewImage")?,
            icon: named("GalleryPreviewIcon")?,
            guard,
            accepted_changes: 0,
            host: "Рабочая область",
            last: None,
        }))
    }
    pub(crate) fn command(
        &mut self,
        ui: &mut Controls,
        _view: &mut MountedView,
        command: &str,
    ) -> Result<bool, String> {
        let result = match command {
            "GalleryHome" | "GalleryRibbon" => {
                let parent = if command == "GalleryHome" {
                    self.home
                } else {
                    self.ribbon
                };
                let result = ui.set_gallery_host(
                    self.owner,
                    parent,
                    LayoutStyle {
                        width: Length::Percent(1.0),
                        height: Length::Percent(1.0),
                        ..Default::default()
                    },
                );
                if result.is_ok() {
                    self.host = if parent == self.home {
                        "Рабочая область"
                    } else {
                        "Ribbon"
                    };
                }
                result
            }
            "GalleryPopup" => ui.open_gallery_popup(self.owner),
            "GalleryReadOnly" => {
                let mut settings = ui
                    .gallery_settings(self.owner)
                    .ok_or("Gallery removed")?
                    .clone();
                settings.is_read_only = !settings.is_read_only;
                ui.set_gallery_settings(self.owner, settings)
            }
            "GalleryGuard" => {
                self.guard.set(!self.guard.get());
                Ok(())
            }
            _ => return Ok(false),
        };
        if let Err(error) = result {
            ui.set_label(self.status, error.to_string()).map_err(err)?;
        } else {
            self.sync(ui)?;
        }
        Ok(true)
    }
    pub(crate) fn sync(&mut self, ui: &mut Controls) -> Result<(), String> {
        self.accepted_changes += ui
            .drain_gallery_selection_changes()
            .into_iter()
            .filter(|e| e.owner == self.owner)
            .count();
        let _ = ui.drain_gallery_previews();
        let key = ui
            .gallery_preview_key(self.owner)
            .or_else(|| ui.gallery_selected_key(self.owner))
            .map(str::to_owned);
        if key != self.last {
            if let Some(item) = key
                .as_deref()
                .and_then(|key| {
                    ui.gallery_model(self.owner)?
                        .items()
                        .iter()
                        .find(|i| i.item.key == key)
                })
                .cloned()
            {
                ui.set_label(self.preview, &item.item.text).map_err(err)?;
                let has_image = item.image_resource.is_some();
                for (id, visible) in [(self.image, has_image), (self.icon, item.icon.is_some())] {
                    let mut props = ui
                        .runtime
                        .tree()
                        .node(id)
                        .ok_or("Preview removed")?
                        .props
                        .clone();
                    props.visible = visible;
                    props.collapsed = !visible;
                    ui.runtime
                        .apply(Mutation::SetProps { node: id, props })
                        .map_err(err)?;
                }
                if let Some(image) = item.image_resource {
                    ui.set_image_resource(self.image, image).map_err(err)?;
                }
                ui.set_icon_symbol(self.icon, item.icon.unwrap_or(IconSymbol::Check))
                    .map_err(err)?;
            }
            self.last = key;
        }
        let host = if ui
            .gallery_popup(self.owner)
            .is_some_and(|p| ui.flyout_is_open(p))
        {
            "Всплывающая панель"
        } else {
            self.host
        };
        ui.set_label(
            self.status,
            format!(
                "Размещение: {host} · подтверждений: {} · только просмотр: {} · проверка: {}{}",
                self.accepted_changes,
                ui.gallery_settings(self.owner)
                    .is_some_and(|s| s.is_read_only),
                self.guard.get(),
                ui.gallery_error(self.owner)
                    .map_or_else(String::new, |error| format!(" · {error}"))
            ),
        )
        .map_err(err)
    }
}
fn err(error: impl std::fmt::Display) -> String {
    error.to_string()
}

#[cfg(test)]
mod host_tests {
    #[test]
    fn gallery_demo_mounts_through_real_form_and_registered_images() {
        let definition =
            rust_desktop_ui_xaml::parse(include_str!("../GalleryWindow.xaml")).unwrap();
        let mut form =
            crate::form::Form::new_with_model(definition, false, Some(super::model().unwrap()))
                .unwrap();
        form.ui
            .update(rust_desktop_ui_core::Size::new(1140.0, 840.0))
            .unwrap();
        let owner = form.view.find_name("Samples").unwrap();
        assert_eq!(form.ui.gallery_selected_key(owner), Some("blue"));
        let sample = form.ui.gallery_item_widget(owner, "blue").unwrap();
        assert!(form.ui.runtime.bounds(sample).unwrap().width > 60.0);
        let settings = form.ui.gallery_settings(owner).unwrap().clone();
        let captions = form
            .ui
            .gallery_model(owner)
            .unwrap()
            .items()
            .iter()
            .map(|item| item.item.text.clone())
            .collect::<Vec<_>>();
        let caption_width = settings.sample_width - 28.0;
        for caption in captions {
            let size = form
                .ui
                .measure_styled_text(
                    &caption,
                    caption_width,
                    rust_desktop_ui_controls::TextStyle::default(),
                )
                .unwrap();
            assert!(
                size.width <= caption_width + 0.01,
                "caption exceeds sample: {caption}"
            );
            assert!(
                size.height <= settings.sample_height - 48.0,
                "caption exceeds image remainder: {caption}"
            );
        }
    }
}
