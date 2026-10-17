//! Mount wait content once; application sessions control the temporary surface.
use super::*;
impl MountedView {
    pub(super) fn mount_loading(
        &mut self,
        ui: &mut Controls,
        parent: WidgetId,
        element: &ElementDefinition,
        style: LayoutStyle,
        width: f32,
        context: &str,
    ) -> Result<WidgetId, String> {
        let ElementKind::LoadingOverlay(value) = &element.kind else {
            return Err("Expected LoadingOverlay".into());
        };
        let parts = ui
            .loading_overlay(parent, &value.label, style)
            .map_err(err)?;
        if let Some(child) = &value.child {
            self.mount_element(ui, parts.content, child, Axis::Vertical, width, context)?;
        }
        Ok(parts.container)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use rust_desktop_ui_controls::{
        BindingContext, DataValue, LoadingResult, NotificationSpec, Theme,
    };
    use rust_desktop_ui_core::Size;
    use std::{cell::RefCell, rc::Rc};
    #[test]
    fn compiled_wait_keeps_child_binding_and_toast_geometry() {
        let window=rust_desktop_ui_xaml::parse("<Window xmlns='urn:rust-ui-engine:ui' xmlns:x='http://schemas.microsoft.com/winfx/2006/xaml'><StackPanel><NotificationCenter x:Name='Toasts'/><LoadingOverlay x:Name='Wait' Height='280'><TextBox x:Name='Draft' Text='{Binding Draft}'/></LoadingOverlay></StackPanel></Window>").unwrap();
        let context = Rc::new(RefCell::new(
            BindingContext::new(DataValue::object([("Draft", "Accepted".into())])).unwrap(),
        ));
        let mut ui = Controls::new(Theme::FluentLight).unwrap();
        let root = ui.root();
        let mut view =
            MountedView::mount_with_context(&mut ui, root, &window.content, context.clone())
                .unwrap();
        let size = Size::new(700.0, 500.0);
        ui.update(size).unwrap();
        view.update_form_layout(&mut ui, size).unwrap();
        let center = view.find_name("Toasts").unwrap();
        let owner = view.find_name("Wait").unwrap();
        let editor = view.find_name("Draft").unwrap();
        ui.set_text(editor, "Retained draft").unwrap();
        let token = ui.begin_loading(owner, "Work", true).unwrap();
        ui.update(size).unwrap();
        let cancel = ui.loading_parts(owner).unwrap().cancel;
        let b = ui.runtime.bounds(cancel).unwrap();
        assert_eq!(b.intersection(ui.runtime.clip(cancel).unwrap()), Some(b));
        ui.finish_loading(token, LoadingResult::Failed("Failed".into()))
            .unwrap();
        ui.update(size).unwrap();
        view.sync_bindings(&mut ui);
        assert_eq!(ui.document(editor).unwrap().text(), "Retained draft");
        assert_eq!(
            context.borrow().get("Draft").unwrap(),
            &DataValue::String("Accepted".into())
        );
        let id = ui
            .post_notification(center, NotificationSpec::new("Done", "Message"))
            .unwrap();
        ui.update(size).unwrap();
        let parts = ui.notification_parts(id).unwrap();
        let bounds = ui.runtime.bounds(parts.dismiss).unwrap();
        assert_eq!(
            bounds.intersection(ui.runtime.clip(parts.dismiss).unwrap()),
            Some(bounds)
        );
    }
}
