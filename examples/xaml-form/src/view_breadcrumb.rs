//! Compiled breadcrumb paths preserve stable metadata and accepted-only bindings.
use super::*;
use rust_desktop_ui_controls::{BreadcrumbPath, BreadcrumbSegment, BreadcrumbSettings};
use rust_desktop_ui_core::Size;
use rust_desktop_ui_xaml::BreadcrumbDefinition;

pub(super) fn path(definition: &BreadcrumbDefinition) -> BreadcrumbPath {
    BreadcrumbPath {
        text: definition.path.clone(),
        segments: definition
            .segments
            .iter()
            .map(|s| BreadcrumbSegment {
                key: s.key.clone(),
                label: s.label.clone(),
                has_children: s.has_children,
            })
            .collect(),
    }
}
impl MountedView {
    pub(super) fn mount_breadcrumb(
        &mut self,
        ui: &mut Controls,
        parent: WidgetId,
        definition: &BreadcrumbDefinition,
        style: LayoutStyle,
        label: &str,
    ) -> Result<WidgetId, String> {
        let parts = ui
            .breadcrumb(
                parent,
                label,
                path(definition),
                BreadcrumbSettings {
                    editable: definition.is_editable,
                    read_only: definition.is_read_only,
                    history_limit: definition.history_limit,
                    popup_size: Size::new(definition.popup_width, definition.popup_height),
                    ..Default::default()
                },
                style,
            )
            .map_err(err)?;
        self.editors.push(parts.editor);
        Ok(parts.container)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_desktop_ui_controls::{BindingContext, DataValue, Theme, breadcrumb_path_to_value};
    use std::{cell::RefCell, rc::Rc};
    #[test]
    fn compiled_breadcrumb_uses_typed_initial_value_and_rejects_invalid_mount_atomically() {
        let mut ui = Controls::new(Theme::FluentLight).unwrap();
        let definition=rust_desktop_ui_xaml::parse("<Window xmlns='urn:rust-ui-engine:ui' xmlns:x='http://schemas.microsoft.com/winfx/2006/xaml'><Breadcrumb x:Name='Location' AutomationName='Расположение в проекте' Value='{Binding Location}' Width='520'/></Window>").unwrap().content;
        let expected = BreadcrumbPath {
            text: "project:root".into(),
            segments: vec![BreadcrumbSegment::new("root", "Проект")],
        };
        let model = Rc::new(RefCell::new(
            BindingContext::new(DataValue::object([(
                "Location",
                breadcrumb_path_to_value(&expected),
            )]))
            .unwrap(),
        ));
        let root = ui.root();
        let mut view =
            MountedView::mount_with_context(&mut ui, root, &definition, Rc::clone(&model)).unwrap();
        ui.update(Size::new(800.0, 450.0)).unwrap();
        let id = view.find_name("Location").unwrap();
        let editor = ui.breadcrumb_parts(id).unwrap().editor;
        ui.begin_breadcrumb_edit(id).unwrap();
        ui.update(Size::new(800.0, 450.0)).unwrap();
        assert_eq!(
            ui.semantics()
                .iter()
                .find(|node| node.id == editor)
                .unwrap()
                .label,
            "Расположение в проекте"
        );
        assert_eq!(ui.breadcrumb_path(id), Some(&expected));
        ui.set_breadcrumb_draft(id, "draft").unwrap();
        view.bindings.as_mut().unwrap().sync(&mut ui);
        assert_eq!(
            model.borrow().get("Location").unwrap(),
            &breadcrumb_path_to_value(&expected)
        );
        let bad = Rc::new(RefCell::new(
            BindingContext::new(DataValue::object([(
                "Location",
                DataValue::from("not a typed path"),
            )]))
            .unwrap(),
        ));
        let count = ui.runtime.tree().len();
        assert!(MountedView::mount_with_context(&mut ui, root, &definition, bad).is_err());
        assert_eq!(ui.runtime.tree().len(), count);
        assert_eq!(ui.breadcrumb_path(id), Some(&expected));
    }
}
