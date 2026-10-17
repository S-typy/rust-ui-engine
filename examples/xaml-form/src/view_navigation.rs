//! Mount each navigation page once into its stable route host.
use super::*;
use rust_desktop_ui_controls::{
    NavigationModel, NavigationPage, NavigationSection, NavigationSettings, NavigationView,
};
use rust_desktop_ui_xaml::{NavigationDefinition, NavigationViewDefinition};

fn model(value: &NavigationDefinition) -> Result<NavigationModel, String> {
    NavigationModel::new(
        value
            .sections
            .iter()
            .map(|s| NavigationSection {
                id: s.id.clone(),
                label: s.header.clone(),
                icon: s.icon.map(super::icon_symbol),
            })
            .collect(),
        value
            .pages
            .iter()
            .map(|p| NavigationPage {
                route: p.route.clone(),
                section: p.section.clone(),
                label: p.header.clone(),
                icon: p.icon.map(super::icon_symbol),
                enabled: p.is_enabled,
            })
            .collect(),
        if value.route.is_empty() {
            value
                .pages
                .iter()
                .find(|p| p.is_enabled)
                .map_or("", |p| p.route.as_str())
        } else {
            &value.route
        },
        value.history_limit,
    )
    .map_err(err)
}
impl MountedView {
    pub(super) fn mount_navigation(
        &mut self,
        ui: &mut Controls,
        parent: WidgetId,
        element: &ElementDefinition,
        style: LayoutStyle,
        available_width: f32,
        context: &str,
    ) -> Result<WidgetId, String> {
        let ElementKind::NavigationPane(value) = &element.kind else {
            return Err("Expected NavigationPane".into());
        };
        let p = ui
            .navigation_pane(
                parent,
                element
                    .common
                    .automation_name
                    .as_deref()
                    .or(element.common.name.as_deref())
                    .unwrap_or("Разделы"),
                model(value)?,
                NavigationSettings {
                    view: match value.view {
                        NavigationViewDefinition::Side => NavigationView::Side,
                        NavigationViewDefinition::Accordion => NavigationView::Accordion,
                        NavigationViewDefinition::Hamburger => NavigationView::Hamburger,
                        NavigationViewDefinition::Office => NavigationView::Office,
                        NavigationViewDefinition::Frame => NavigationView::Frame,
                    },
                    pane_width: value.pane_width,
                    history_limit: value.history_limit,
                    show_history: value.show_history,
                },
                style,
            )
            .map_err(err)?;
        for page in &value.pages {
            let host = ui.navigation_page_host(p.container, &page.route).unwrap();
            self.mount_element(
                ui,
                host,
                &page.content,
                Axis::Vertical,
                (available_width - value.pane_width).max(80.0),
                context,
            )?;
        }
        Ok(p.container)
    }
}

#[cfg(test)]
use crate::navigation_model as demo;

#[cfg(test)]
use crate::navigation_diagnostics as diagnostics;

#[cfg(test)]
mod tests {
    use super::*;
    use rust_desktop_ui_controls::{BindingContext, DataValue, NavigationMode, Theme};
    use rust_desktop_ui_core::{Mutation, Size};
    use std::{cell::RefCell, rc::Rc};
    fn definition() -> ElementDefinition {
        rust_desktop_ui_xaml::parse("<Window xmlns='urn:rust-ui-engine:ui' xmlns:x='http://schemas.microsoft.com/winfx/2006/xaml'><NavigationPane x:Name='Nav' Route='{Binding Route}' Width='720' Height='420'><NavigationSection Id='work' Header='Работа'><NavigationPage Route='home' Header='Главная'><TextBox x:Name='Draft' Text='{Binding Draft}'/></NavigationPage><NavigationPage Route='settings' Header='Параметры'><TextBlock x:Name='Info' Text='Settings'/></NavigationPage></NavigationSection></NavigationPane></Window>").unwrap().content
    }
    #[test]
    fn compiled_navigation_preserves_editor_names_and_bindings_through_routes_and_views() {
        let mut ui = Controls::new(Theme::FluentLight).unwrap();
        let context = Rc::new(RefCell::new(
            BindingContext::new(DataValue::object([
                ("Route", DataValue::from("home")),
                ("Draft", DataValue::from("accepted")),
            ]))
            .unwrap(),
        ));
        let root = ui.root();
        let mut view =
            MountedView::mount_with_context(&mut ui, root, &definition(), Rc::clone(&context))
                .unwrap();
        ui.update(Size::new(900.0, 600.0)).unwrap();
        let owner = view.find_name("Nav").unwrap();
        let editor = view.find_name("Draft").unwrap();
        ui.runtime.apply(Mutation::Focus(Some(editor))).unwrap();
        ui.update(Size::new(900.0, 600.0)).unwrap();
        ui.set_text(editor, "draft").unwrap();
        for mode in [
            NavigationView::Accordion,
            NavigationView::Office,
            NavigationView::Hamburger,
            NavigationView::Frame,
        ] {
            ui.set_navigation_view(owner, mode).unwrap();
        }
        ui.navigate(owner, "settings", NavigationMode::Push)
            .unwrap();
        view.bindings.as_mut().unwrap().sync(&mut ui);
        assert_eq!(
            context.borrow().get("Route").unwrap(),
            &DataValue::from("settings")
        );
        assert_eq!(
            context.borrow().get("Draft").unwrap(),
            &DataValue::from("accepted")
        );
        ui.navigation_back(owner).unwrap();
        ui.update(Size::new(900.0, 600.0)).unwrap();
        assert_eq!(ui.document(editor).unwrap().text(), "draft");
        assert_eq!(ui.runtime.focused(), Some(editor));
        assert_eq!(view.find_name("Draft"), Some(editor));
    }
    #[test]
    fn invalid_initial_route_binding_rolls_back_entire_navigation_tree() {
        let mut ui = Controls::new(Theme::FluentLight).unwrap();
        let root = ui.root();
        let count = ui.runtime.tree().len();
        let context = Rc::new(RefCell::new(
            BindingContext::new(DataValue::object([
                ("Route", DataValue::from("unknown")),
                ("Draft", DataValue::from("accepted")),
            ]))
            .unwrap(),
        ));
        assert!(MountedView::mount_with_context(&mut ui, root, &definition(), context).is_err());
        assert_eq!(ui.runtime.tree().len(), count);
    }
    #[test]
    fn initial_route_binding_can_use_valid_fallback_without_hiding_diagnostic() {
        let mut definition = definition();
        definition
            .common
            .bindings
            .get_mut("Route")
            .unwrap()
            .fallback_value = Some("home".into());
        let context = Rc::new(RefCell::new(
            BindingContext::new(DataValue::object([("Draft", DataValue::from("accepted"))]))
                .unwrap(),
        ));
        let mut ui = Controls::new(Theme::FluentLight).unwrap();
        let root = ui.root();
        let view = MountedView::mount_with_context(&mut ui, root, &definition, context).unwrap();
        assert_eq!(
            ui.navigation_route(view.find_name("Nav").unwrap()),
            Some("home")
        );
        assert!(!view.bindings.as_ref().unwrap().diagnostics().is_empty());
    }
    #[test]
    fn navigation_frame_without_explicit_height_fills_resized_window() {
        let definition=rust_desktop_ui_xaml::parse("<Window xmlns='urn:rust-ui-engine:ui' xmlns:x='http://schemas.microsoft.com/winfx/2006/xaml'><NavigationFrame x:Name='Nav'><NavigationSection Id='work' Header='Work'><NavigationPage Route='home' Header='Home'><TextBox/></NavigationPage></NavigationSection></NavigationFrame></Window>").unwrap();
        let mut ui = Controls::new(Theme::FluentLight).unwrap();
        let root = ui.root();
        ui.runtime
            .apply(Mutation::SetStyle {
                node: root,
                style: LayoutStyle {
                    kind: LayoutKind::Stack(Axis::Vertical),
                    width: Length::Percent(1.0),
                    height: Length::Percent(1.0),
                    ..Default::default()
                },
            })
            .unwrap();
        let mut view = MountedView::mount(&mut ui, root, &definition.content).unwrap();
        let owner = view.find_name("Nav").unwrap();
        for size in [Size::new(600.0, 480.0), Size::new(800.0, 640.0)] {
            ui.update(size).unwrap();
            view.update_form_layout(&mut ui, size).unwrap();
            let bounds = ui.runtime.bounds(owner).unwrap();
            assert_eq!(bounds.width, size.width);
            assert_eq!(bounds.height, size.height);
            assert!(
                ui.runtime
                    .bounds(ui.navigation_page_host(owner, "home").unwrap())
                    .unwrap()
                    .height
                    > size.height - 60.0
            );
        }
    }
    #[test]
    fn navigation_demo_mounts_all_pages_and_preserves_nested_content_during_commands() {
        let definition =
            rust_desktop_ui_xaml::parse(include_str!("../NavigationWindow.xaml")).unwrap();
        let mut ui = Controls::new(Theme::FluentLight).unwrap();
        demo::register(&mut ui);
        ui.commands_mut().register("Light", "Светлая тема");
        ui.commands_mut().register("Dark", "Тёмная тема");
        let root = ui.root();
        let context = demo::model().unwrap();
        let mut view =
            MountedView::mount_with_context(&mut ui, root, &definition.content, context).unwrap();
        ui.update(Size::new(1120.0, 800.0)).unwrap();
        let mut state = demo::State::install(&mut ui, &view).unwrap().unwrap();
        let owner = view.find_name("Navigation").unwrap();
        let editor = view.find_name("NavigationDraft").unwrap();
        assert!(diagnostics::snapshot(&ui, &view).contains("\"route\":\"home\""));
        let ids = ui.runtime.tree().preorder();
        ui.set_text(editor, "несохранённый черновик").unwrap();
        for command in [
            "NavigationAccordion",
            "NavigationOffice",
            "NavigationHamburger",
            "NavigationFrame",
            "NavigationSide",
        ] {
            assert!(state.command(&mut ui, &mut view, command).unwrap());
            ui.update(Size::new(1120.0, 800.0)).unwrap();
            assert_eq!(ui.runtime.tree().preorder(), ids);
        }
        assert!(
            state
                .command(&mut ui, &mut view, "NavigationExternal")
                .unwrap()
        );
        assert_eq!(ui.navigation_route(owner), Some("settings"));
        assert!(
            state
                .command(&mut ui, &mut view, "NavigationAvailability")
                .unwrap()
        );
        assert_eq!(ui.navigation_route(owner), Some("settings"));
        assert_eq!(
            ui.document(editor).unwrap().text(),
            "несохранённый черновик"
        );
        assert!(!state.command(&mut ui, &mut view, "Unknown").unwrap());
    }
}
