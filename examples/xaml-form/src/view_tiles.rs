//! Compiled tile metadata maps to one virtual collection with stable keys.
use super::*;
use rust_desktop_ui_controls::{
    SelectionItem, SelectionMode, TileGroup, TileItem, TileModel, TileSettings, TileSize,
};
use rust_desktop_ui_xaml::{TileCollectionDefinition, TileSizeDefinition};
pub(super) fn model(value: &TileCollectionDefinition) -> Result<TileModel, String> {
    TileModel::new(
        value
            .groups
            .iter()
            .map(|g| TileGroup::new(&g.key, &g.header))
            .collect(),
        value
            .items
            .iter()
            .map(|i| TileItem {
                item: SelectionItem {
                    key: i.key.clone(),
                    text: i.text.clone(),
                    columns: i.columns.clone(),
                    enabled: i.is_enabled,
                },
                group: i.group.clone(),
                size: match i.size {
                    TileSizeDefinition::Small => TileSize::Small,
                    TileSizeDefinition::Medium => TileSize::Medium,
                    TileSizeDefinition::Wide => TileSize::Wide,
                    TileSizeDefinition::Large => TileSize::Large,
                },
                icon: i.icon.map(icon_symbol),
                command: i.command.clone().map(Into::into),
            })
            .collect(),
    )
    .map_err(err)
}
pub(super) fn settings(value: &TileCollectionDefinition) -> TileSettings {
    TileSettings {
        cell_width: value.cell_width,
        cell_height: value.cell_height,
        gap: value.gap,
        group_header_height: value.group_header_height,
        overscan: value.overscan,
        max_realized: value.max_realized,
        selection: match value.selection_mode {
            rust_desktop_ui_xaml::SelectionMode::Single => SelectionMode::Single,
            rust_desktop_ui_xaml::SelectionMode::Multiple => SelectionMode::Multiple,
        },
        is_read_only: value.is_read_only,
        allow_reorder: value.allow_reorder,
    }
}
impl MountedView {
    pub(super) fn mount_tiles(
        &mut self,
        ui: &mut Controls,
        parent: WidgetId,
        value: &TileCollectionDefinition,
        style: LayoutStyle,
        label: &str,
    ) -> Result<WidgetId, String> {
        let parts = ui
            .tile_collection(parent, label, model(value)?, settings(value), style)
            .map_err(err)?;
        if let Some(template) = &value.item_template {
            ui.set_tile_item_template(parts.container, Some(forms::item_template(template)?))
                .map_err(err)?;
        }
        if !value.selected_keys.is_empty() {
            ui.set_tile_selected_keys(parts.container, value.selected_keys.clone())
                .map_err(err)?;
        }
        Ok(parts.container)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_desktop_ui_controls::{
        BindingContext, BindingValidator, DataValue, Theme, TileEventKind, tile_items_to_value,
    };
    use rust_desktop_ui_core::Size;
    use std::{cell::RefCell, rc::Rc};
    #[test]
    fn tiles_compiled_binding_template_geometry_and_retained_document() {
        let w=rust_desktop_ui_xaml::parse("<Window xmlns='urn:rust-ui-engine:ui' xmlns:x='http://schemas.microsoft.com/winfx/2006/xaml'><Grid><Grid.RowDefinitions><RowDefinition Height='Auto'/><RowDefinition Height='*'/></Grid.RowDefinitions><TextBox x:Name='Draft' Text='Accepted'/><TileCollection x:Name='Tiles' Grid.Row='1' Height='320' SelectedKey='{Binding Key}' ItemsSource='{Binding Rows}'><TileCollection.ItemTemplate><TextBlock Text='{Binding Text}'/></TileCollection.ItemTemplate></TileCollection></Grid></Window>").unwrap();
        let rows = (0..100)
            .map(|i| TileItem::new(i.to_string(), format!("Tile {i}")))
            .collect::<Vec<_>>();
        let context = Rc::new(RefCell::new(
            BindingContext::new(DataValue::object([
                ("Key", DataValue::Null),
                ("Rows", tile_items_to_value(&rows)),
            ]))
            .unwrap(),
        ));
        context
            .borrow_mut()
            .set_field_validator(
                "Key",
                Some(BindingValidator::new(|v| {
                    if *v == DataValue::String("2".into()) {
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
            MountedView::mount_with_context(&mut ui, root, &w.content, context.clone()).unwrap();
        ui.update(Size::new(760.0, 600.0)).unwrap();
        let id = view.find_name("Tiles").unwrap();
        let draft = view.find_name("Draft").unwrap();
        ui.set_text(draft, "Retained draft").unwrap();
        assert!(ui.tile_stats(id).unwrap().realized < 30);
        let row = ui.tile_item_widget(id, "0").unwrap();
        assert!(ui.runtime.bounds(row).unwrap().width > 80.0);
        ui.invoke_tile(id, "1").unwrap();
        view.sync_bindings(&mut ui);
        assert_eq!(
            context.borrow().get("Key").unwrap(),
            &DataValue::String("1".into())
        );
        assert!(ui.invoke_tile(id, "2").is_err());
        view.sync_bindings(&mut ui);
        assert_eq!(ui.tile_selected_keys(id).unwrap(), ["1"]);
        ui.update(Size::new(380.0, 600.0)).unwrap();
        assert_eq!(ui.tile_item_widget(id, "0"), Some(row));
        assert_eq!(ui.document(draft).unwrap().text(), "Retained draft");
        assert_eq!(ui.accepted_value(draft), Some("Accepted"));
        assert_eq!(
            ui.drain_tile_events()
                .iter()
                .filter(|e| matches!(e.kind, TileEventKind::Activated { .. }))
                .count(),
            1
        );
    }
    #[test]
    fn tiles_static_resource_and_initial_invalid_binding_rollback() {
        let w=rust_desktop_ui_xaml::parse("<Window xmlns='urn:rust-ui-engine:ui' xmlns:x='http://schemas.microsoft.com/winfx/2006/xaml'><Window.Resources><DataTemplate x:Key='TileLabel'><TextBlock Text='{Binding Text}'/></DataTemplate></Window.Resources><TileCollection ItemTemplate='{StaticResource TileLabel}' SelectedKey='{Binding Key}'><Tile Key='a' Text='Alpha'/></TileCollection></Window>").unwrap();
        let mut ui = Controls::new(Theme::FluentLight).unwrap();
        let before = ui.runtime.tree().preorder();
        let root = ui.root();
        let context = Rc::new(RefCell::new(
            BindingContext::new(DataValue::object([("Key", "missing".into())])).unwrap(),
        ));
        assert!(MountedView::mount_with_context(&mut ui, root, &w.content, context).is_err());
        assert_eq!(ui.runtime.tree().preorder(), before);
    }
    #[test]
    fn tiles_real_demo_retains_document_and_positive_geometry_after_resize() {
        let window = rust_desktop_ui_xaml::parse(include_str!("../TileWindow.xaml")).unwrap();
        let context = crate::tile_model::model().unwrap();
        let mut ui = Controls::new(Theme::FluentLight).unwrap();
        crate::tile_model::register(&mut ui);
        ui.commands_mut().register("Light", "Light");
        ui.commands_mut().register("Dark", "Dark");
        let root = ui.root();
        let mut view =
            MountedView::mount_with_context(&mut ui, root, &window.content, context).unwrap();
        let _state = crate::tile_model::State::install(&mut ui, &view).unwrap();
        let id = view.find_name("Documents").unwrap();
        let draft = view.find_name("TileDocument").unwrap();
        ui.set_text(draft, "Draft outside virtual collection")
            .unwrap();
        for size in [Size::new(1120.0, 780.0), Size::new(740.0, 540.0)] {
            ui.update(size).unwrap();
            view.sync_bindings(&mut ui);
            let owner = ui.runtime.bounds(id).unwrap();
            assert!(owner.width > 120.0 && owner.height > 120.0, "{owner:?}");
            let row = ui.tile_item_widget(id, "doc-0").unwrap();
            let button = ui.tile_invoke_widget(id, "doc-0").unwrap();
            let bounds = ui.runtime.bounds(row).unwrap();
            let child = ui.runtime.bounds(button).unwrap();
            assert!(bounds.width > 20.0 && bounds.height > 20.0);
            assert!(child.x > bounds.x && child.y > bounds.y);
            let template = view.find_name("TemplateTiles").unwrap();
            for key in ["one", "two"] {
                let item = ui.tile_item_widget(template, key).unwrap();
                let area = ui.runtime.bounds(item).unwrap();
                let mut pending = vec![item];
                let mut labels = 0;
                while let Some(widget) = pending.pop() {
                    pending.extend(ui.runtime.tree().children(widget).unwrap_or(&[]));
                    if ui
                        .semantics()
                        .iter()
                        .any(|n| n.id == widget && n.role == rust_desktop_ui_controls::Role::Label)
                    {
                        let label = ui.runtime.bounds(widget).unwrap();
                        assert_eq!(
                            area.intersection(label),
                            Some(label),
                            "template caption clipped: {label:?}, row {area:?}"
                        );
                        labels += 1;
                    }
                }
                assert_eq!(labels, 2);
            }
            assert_eq!(
                ui.document(draft).unwrap().text(),
                "Draft outside virtual collection"
            );
            assert_eq!(ui.accepted_value(draft), Some("Сохранённый текст"));
        }
    }
}
