//! Compiled samples map to one retained gallery with accepted-key bindings.
use super::*;
use rust_desktop_ui_controls::{
    GalleryGroup, GalleryItem, GalleryModel, GallerySettings, SelectionItem,
};
use rust_desktop_ui_xaml::GalleryDefinition;
pub(super) fn model(value: &GalleryDefinition) -> Result<GalleryModel, String> {
    GalleryModel::new(
        value
            .groups
            .iter()
            .map(|g| GalleryGroup::new(&g.key, &g.header))
            .collect(),
        value
            .items
            .iter()
            .map(|i| GalleryItem {
                item: SelectionItem {
                    key: i.key.clone(),
                    text: i.text.clone(),
                    columns: i.columns.clone(),
                    enabled: i.is_enabled,
                },
                group: i.group.clone(),
                icon: i.icon.map(icon_symbol),
                image_resource: i.image_resource.clone(),
            })
            .collect(),
    )
    .map_err(err)
}
pub(super) fn settings(value: &GalleryDefinition) -> GallerySettings {
    GallerySettings {
        sample_width: value.sample_width,
        sample_height: value.sample_height,
        gap: value.gap,
        group_header_height: value.group_header_height,
        is_read_only: value.is_read_only,
    }
}
impl MountedView {
    pub(super) fn mount_gallery(
        &mut self,
        ui: &mut Controls,
        parent: WidgetId,
        value: &GalleryDefinition,
        style: LayoutStyle,
        label: &str,
    ) -> Result<WidgetId, String> {
        let parts = ui
            .gallery(parent, label, model(value)?, settings(value), style)
            .map_err(err)?;
        if let Some(template) = &value.item_template {
            ui.set_gallery_item_template(parts.container, Some(forms::item_template(template)?))
                .map_err(err)?;
        }
        if let Some(key) = &value.selected_key {
            ui.set_gallery_selected_key(parts.container, Some(key.clone()))
                .map_err(err)?;
        }
        Ok(parts.container)
    }
}
#[cfg(test)]
use crate::gallery_model as demo;
#[cfg(test)]
mod tests {
    use super::*;
    use rust_desktop_ui_controls::{BindingContext, BindingValidator, DataValue, Theme};
    use rust_desktop_ui_core::Size;
    use std::{cell::RefCell, rc::Rc};
    #[test]
    fn gallery_compiled_template_preview_validator_binding_and_editor_retention() {
        let window=rust_desktop_ui_xaml::parse("<Window xmlns='urn:rust-ui-engine:ui' xmlns:x='http://schemas.microsoft.com/winfx/2006/xaml'><Window.Resources><DataTemplate x:Key='Caption'><TextBlock Text='{Binding Text}'/></DataTemplate></Window.Resources><StackPanel><TextBox x:Name='Draft' Text='Accepted'/><Gallery x:Name='Samples' Height='220' Width='360' SelectedKey='{Binding Chosen}' ItemTemplate='{StaticResource Caption}'><GalleryGroup Key='styles' Header='Styles'><GalleryItem Key='a' Text='Alpha' Icon='Check'/><GalleryItem Key='b' Text='Beta' Icon='Edit'/><GalleryItem Key='c' Text='Denied'/></GalleryGroup></Gallery></StackPanel></Window>").unwrap();
        let context = Rc::new(RefCell::new(
            BindingContext::new(DataValue::object([("Chosen", "a".into())])).unwrap(),
        ));
        context
            .borrow_mut()
            .set_field_validator(
                "Chosen",
                Some(BindingValidator::new(|v| {
                    if *v == DataValue::from("c") {
                        Err("Denied".into())
                    } else {
                        Ok(())
                    }
                })),
            )
            .unwrap();
        let mut ui = Controls::new(Theme::FluentLight).unwrap();
        let root = ui.root();
        let mut view =
            MountedView::mount_with_context(&mut ui, root, &window.content, context.clone())
                .unwrap();
        ui.update(Size::new(640.0, 480.0)).unwrap();
        let id = view.find_name("Samples").unwrap();
        let draft = view.find_name("Draft").unwrap();
        ui.set_text(draft, "Retained draft").unwrap();
        let row = ui.gallery_item_widget(id, "a").unwrap();
        ui.preview_gallery_item(id, "b").unwrap();
        view.sync_bindings(&mut ui);
        assert_eq!(
            context.borrow().get("Chosen").unwrap(),
            &DataValue::from("a")
        );
        ui.accept_gallery_preview(id).unwrap();
        view.sync_bindings(&mut ui);
        assert_eq!(
            context.borrow().get("Chosen").unwrap(),
            &DataValue::from("b")
        );
        ui.preview_gallery_item(id, "c").unwrap();
        assert!(ui.accept_gallery_preview(id).is_err());
        assert_eq!(ui.gallery_selected_key(id), Some("b"));
        ui.update(Size::new(420.0, 360.0)).unwrap();
        assert_eq!(ui.gallery_item_widget(id, "a"), Some(row));
        assert_eq!(ui.document(draft).unwrap().text(), "Retained draft");
        assert_eq!(ui.accepted_value(draft), Some("Accepted"));
        assert_eq!(ui.drain_gallery_selection_changes().len(), 1);
    }
    #[test]
    fn gallery_invalid_initial_key_or_image_resource_rolls_back_mount() {
        for body in [
            "<Gallery SelectedKey='{Binding Key}'><GalleryItem Key='a' Text='Alpha'/></Gallery>",
            "<Gallery><GalleryItem Key='a' Text='Alpha' ImageResource='Missing'/></Gallery>",
        ] {
            let window = rust_desktop_ui_xaml::parse(&format!(
                "<Window xmlns='urn:rust-ui-engine:ui'>{body}</Window>"
            ))
            .unwrap();
            let context = Rc::new(RefCell::new(
                BindingContext::new(DataValue::object([("Key", "missing".into())])).unwrap(),
            ));
            let mut ui = Controls::new(Theme::FluentLight).unwrap();
            let before = ui.runtime.tree().preorder();
            let root = ui.root();
            assert!(
                MountedView::mount_with_context(&mut ui, root, &window.content, context).is_err()
            );
            assert_eq!(ui.runtime.tree().preorder(), before);
        }
    }
    #[test]
    fn gallery_real_sample_transfers_same_nodes_across_all_hosts_with_document_draft() {
        let window = rust_desktop_ui_xaml::parse(include_str!("../GalleryWindow.xaml")).unwrap();
        let mut ui = Controls::new(Theme::FluentLight).unwrap();
        demo::register(&mut ui).unwrap();
        let root = ui.root();
        let mut view =
            MountedView::mount_with_context(&mut ui, root, &window.content, demo::model().unwrap())
                .unwrap();
        ui.update(Size::new(1140.0, 840.0)).unwrap();
        let mut state = demo::State::install(&mut ui, &view).unwrap().unwrap();
        state.sync(&mut ui).unwrap();
        let id = view.find_name("Samples").unwrap();
        let row = ui.gallery_item_widget(id, "blue").unwrap();
        let document = view.find_name("GalleryDocument").unwrap();
        ui.set_text(document, "Retained document draft").unwrap();
        for command in [
            "GalleryRibbon",
            "GalleryPopup",
            "GalleryHome",
            "GalleryPopup",
        ] {
            state.command(&mut ui, &mut view, command).unwrap();
            ui.update(Size::new(1140.0, 840.0)).unwrap();
            state.sync(&mut ui).unwrap();
            assert_eq!(ui.gallery_item_widget(id, "blue"), Some(row));
            assert_eq!(ui.gallery_selected_key(id), Some("blue"));
            let r = ui.runtime.bounds(row).unwrap();
            assert!(r.width > 60.0 && r.height > 60.0, "{command}: {r:?}");
            assert!(
                r.intersection(ui.runtime.clip(row).unwrap()).is_some(),
                "{command}: {r:?}"
            );
        }
        ui.close_gallery_popup(id).unwrap();
        state.command(&mut ui, &mut view, "GalleryGuard").unwrap();
        ui.preview_gallery_item(id, "red").unwrap();
        assert!(ui.accept_gallery_preview(id).is_err());
        assert_eq!(
            ui.document(document).unwrap().text(),
            "Retained document draft"
        );
        assert_eq!(ui.accepted_value(document), Some("Черновик документа"));
        ui.preview_gallery_item(id, "green").unwrap();
        ui.accept_gallery_preview(id).unwrap();
        view.sync_bindings(&mut ui);
        state.sync(&mut ui).unwrap();
        assert_eq!(
            ui.label_text(view.find_name("GalleryAccepted").unwrap()),
            Some("green")
        );
    }
}
