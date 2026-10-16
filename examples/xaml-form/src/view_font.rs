//! FontPicker uses the same descriptor for its accepted value and shaped preview.
use super::*;
use rust_desktop_ui_controls::{FontDescriptor, FontPickerSettings, FontSlant};
use rust_desktop_ui_xaml::FontPickerDefinition;

impl MountedView {
    pub(super) fn mount_font(
        &mut self,
        ui: &mut Controls,
        parent: WidgetId,
        definition: &FontPickerDefinition,
        style: LayoutStyle,
        label: &str,
    ) -> Result<WidgetId, String> {
        let font = FontDescriptor {
            family: definition.family.clone(),
            size: definition.size,
            weight: definition.weight,
            width: definition.width,
            slant: match definition.style.as_str() {
                "Italic" => FontSlant::Italic,
                "Oblique" => FontSlant::Oblique(definition.angle),
                _ => FontSlant::Normal,
            },
        };
        let p = ui
            .font_picker(
                parent,
                label,
                font,
                FontPickerSettings {
                    preview_text: definition.preview_text.clone(),
                    read_only: definition.is_read_only,
                },
                style,
            )
            .map_err(err)?;
        self.editors
            .extend([p.family.editor, p.face.editor, p.size.editor]);
        Ok(p.container)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_desktop_ui_controls::{BindingContext, DataValue, Theme, font_descriptor_to_value};
    use rust_desktop_ui_core::Size;
    use std::{cell::RefCell, rc::Rc};
    #[test]
    fn compiled_font_picker_updates_preview_and_two_way_descriptor() {
        let mut ui = Controls::new(Theme::FluentLight).unwrap();
        let value = FontDescriptor {
            family: "Arial".into(),
            size: 21.0,
            ..Default::default()
        };
        let definition=rust_desktop_ui_xaml::parse("<Window xmlns='urn:rust-ui-engine:ui' xmlns:x='http://schemas.microsoft.com/winfx/2006/xaml'><FontPicker x:Name='Font' Value='{Binding SelectedFont}' Width='640'/></Window>").unwrap().content;
        let model = Rc::new(RefCell::new(
            BindingContext::new(DataValue::object([(
                "SelectedFont",
                font_descriptor_to_value(&value),
            )]))
            .unwrap(),
        ));
        let root = ui.root();
        let mut view =
            MountedView::mount_with_context(&mut ui, root, &definition, Rc::clone(&model)).unwrap();
        ui.update(Size::new(800.0, 500.0)).unwrap();
        let id = view.find_name("Font").unwrap();
        assert_eq!(ui.font_value(id), Some(&value));
        let changed = FontDescriptor {
            size: 32.0,
            slant: FontSlant::Italic,
            ..value
        };
        ui.set_font_value(id, changed.clone()).unwrap();
        view.bindings.as_mut().unwrap().sync(&mut ui);
        assert_eq!(
            model.borrow().get("SelectedFont").unwrap(),
            &font_descriptor_to_value(&changed)
        );
        let parts = ui.font_parts(id).unwrap();
        assert!(ui.label_text(parts.status).unwrap().contains("32"));
    }
}
