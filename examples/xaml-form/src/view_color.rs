//! Compiled exact color/brush values use the retained picker controls.
use super::*;
use rust_desktop_ui_controls::{BrushValue, ColorEditorSettings, Srgba8};
use rust_desktop_ui_xaml::ColorEditorDefinition;

impl MountedView {
    pub(super) fn mount_color(
        &mut self,
        ui: &mut Controls,
        parent: WidgetId,
        value: &ColorEditorDefinition,
        brush: bool,
        style: LayoutStyle,
        label: &str,
    ) -> Result<WidgetId, String> {
        let settings = ColorEditorSettings {
            allow_empty: value.allow_empty,
            is_read_only: value.is_read_only,
            palette: value
                .palette
                .clone()
                .unwrap_or_else(|| ColorEditorSettings::default().palette),
            recent_limit: value.recent_limit,
            popup_size: rust_desktop_ui_core::Size::new(value.popup_width, value.popup_height),
        };
        let initial = value
            .value
            .clone()
            .or_else(|| (!value.allow_empty).then_some(BrushValue::Solid(Srgba8::BLACK)));
        let parts = if brush {
            ui.brush_edit(parent, label, initial, settings, style)
        } else {
            let color = match initial {
                Some(BrushValue::Solid(c)) => Some(c),
                None => None,
                _ => return Err("ColorEdit requires a solid color".into()),
            };
            ui.color_edit(parent, label, color, settings, style)
        }
        .map_err(err)?;
        self.editors.push(parts.editor);
        Ok(parts.container)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_desktop_ui_controls::{
        BindingContext, BindingValidator, CommitOutcome, DataValue, Theme, brush_to_value,
        color_to_value,
    };
    use rust_desktop_ui_core::Size;
    use std::{cell::RefCell, rc::Rc};
    #[test]
    fn color_and_brush_literals_bindings_accept_cancel_and_fallback_are_typed() {
        let definition=rust_desktop_ui_xaml::parse("<Window xmlns='urn:rust-ui-engine:ui' xmlns:x='http://schemas.microsoft.com/winfx/2006/xaml'><StackPanel><ColorEdit x:Name='Color' Value='{Binding Color}' Width='400'/><BrushEdit x:Name='Brush' Value='{Binding Brush}'/><ColorEdit x:Name='Fallback' Value='{Binding Missing, FallbackValue=#00123456}'/></StackPanel></Window>").unwrap().content;
        let model = Rc::new(RefCell::new(
            BindingContext::new(DataValue::object([
                ("Color", color_to_value(Some(Srgba8::WHITE))),
                ("Brush", DataValue::Null),
            ]))
            .unwrap(),
        ));
        model
            .borrow_mut()
            .set_field_validator(
                "Color",
                Some(BindingValidator::new(|v| {
                    if *v == color_to_value(Some(Srgba8::BLACK)) {
                        Err("Black rejected".into())
                    } else {
                        Ok(())
                    }
                })),
            )
            .unwrap();
        let mut ui = Controls::new(Theme::FluentLight).unwrap();
        let root = ui.root();
        let mut view =
            MountedView::mount_with_context(&mut ui, root, &definition, model.clone()).unwrap();
        ui.update(Size::new(900.0, 850.0)).unwrap();
        let color = view.find_name("Color").unwrap();
        let brush = view.find_name("Brush").unwrap();
        let fallback = view.find_name("Fallback").unwrap();
        assert_eq!(
            ui.color_value(fallback),
            Some(Some(Srgba8::new(18, 52, 86, 0)))
        );
        ui.open_color_popup(color).unwrap();
        ui.set_color_draft(color, Srgba8::BLACK).unwrap();
        assert!(matches!(
            ui.apply_color_popup(color).unwrap(),
            CommitOutcome::Invalid(_)
        ));
        ui.set_color_draft(color, Srgba8::new(21, 22, 23, 0))
            .unwrap();
        ui.apply_color_popup(color).unwrap();
        view.bindings.as_mut().unwrap().sync(&mut ui);
        assert_eq!(
            model.borrow().get("Color").unwrap(),
            &color_to_value(Some(Srgba8::new(21, 22, 23, 0)))
        );
        ui.open_color_popup(brush).unwrap();
        let value = BrushValue::Linear(rust_desktop_ui_controls::LinearGradient::default());
        ui.set_brush_draft(brush, Some(value.clone())).unwrap();
        ui.cancel_color_popup(brush).unwrap();
        assert_eq!(model.borrow().get("Brush").unwrap(), &DataValue::Null);
        ui.open_color_popup(brush).unwrap();
        ui.set_brush_draft(brush, Some(value.clone())).unwrap();
        ui.apply_color_popup(brush).unwrap();
        view.bindings.as_mut().unwrap().sync(&mut ui);
        assert_eq!(
            model.borrow().get("Brush").unwrap(),
            &brush_to_value(Some(&value))
        );
    }
    #[test]
    fn invalid_typed_initial_value_and_programmatic_definition_fail_before_remaining_mount() {
        let definition = rust_desktop_ui_xaml::parse(
            "<Window xmlns='urn:rust-ui-engine:ui'><ColorEdit Value='{Binding Color}'/></Window>",
        )
        .unwrap()
        .content;
        let model = Rc::new(RefCell::new(
            BindingContext::new(DataValue::object([("Color", "not a color".into())])).unwrap(),
        ));
        let mut ui = Controls::new(Theme::FluentLight).unwrap();
        let root = ui.root();
        let before = ui.runtime.tree().live_ids();
        assert!(MountedView::mount_with_context(&mut ui, root, &definition, model).is_err());
        assert_eq!(ui.runtime.tree().live_ids(), before);
    }
}
