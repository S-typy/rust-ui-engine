use super::*;
use rust_desktop_ui_controls::{GaugeRange, GaugeSettings, GaugeTone, GaugeView};
use rust_desktop_ui_xaml::{GaugeDefinition, GaugeToneDefinition, GaugeViewDefinition};
pub(super) fn settings(v: &GaugeDefinition, bound: bool) -> GaugeSettings {
    GaugeSettings {
        view: match v.view {
            GaugeViewDefinition::Circular => GaugeView::Circular,
            GaugeViewDefinition::Linear => GaugeView::Linear,
            GaugeViewDefinition::Digital => GaugeView::Digital,
            GaugeViewDefinition::State => GaugeView::State,
        },
        minimum: v.minimum,
        maximum: v.maximum,
        value: if bound { v.minimum } else { v.value },
        major_ticks: v.major_ticks,
        minor_ticks: v.minor_ticks,
        precision: v.precision,
        unit: v.unit.clone(),
        ranges: v
            .ranges
            .iter()
            .map(|r| GaugeRange {
                start: r.start,
                end: r.end,
                label: r.label.clone(),
                tone: match r.tone {
                    GaugeToneDefinition::Neutral => GaugeTone::Neutral,
                    GaugeToneDefinition::Accent => GaugeTone::Accent,
                    GaugeToneDefinition::Good => GaugeTone::Good,
                    GaugeToneDefinition::Warning => GaugeTone::Warning,
                    GaugeToneDefinition::Danger => GaugeTone::Danger,
                },
            })
            .collect(),
    }
}
pub(super) fn mount(
    ui: &mut Controls,
    parent: WidgetId,
    v: &GaugeDefinition,
    bound: bool,
    mut style: LayoutStyle,
) -> Result<WidgetId, String> {
    if style.width == Length::Auto {
        style.width = Length::Percent(1.0);
    }
    ui.gauge(parent, settings(v, bound), style).map_err(err)
}
#[cfg(test)]
mod tests {
    use super::*;
    use rust_desktop_ui_controls::{BindingContext, DataValue, Theme};
    use rust_desktop_ui_core::Size;
    use std::{cell::RefCell, rc::Rc};
    fn content(body: &str) -> ElementDefinition {
        rust_desktop_ui_xaml::parse(&format!("<Window xmlns='urn:rust-ui-engine:ui' xmlns:x='http://schemas.microsoft.com/winfx/2006/xaml'>{body}</Window>")).unwrap().content
    }
    #[test]
    fn gauge_adapter_initial_binding_invalid_replacement_and_typed_fallback() {
        let mut ui = Controls::new(Theme::FluentLight).unwrap();
        let root = ui.root();
        let model = Rc::new(RefCell::new(
            BindingContext::new(DataValue::object([("Value", DataValue::Number(85.0))])).unwrap(),
        ));
        let definition =
            content("<CircularGauge x:Name='G' Minimum='50' Value='{Binding Value}'/>");
        let mut view =
            MountedView::mount_with_context(&mut ui, root, &definition, model.clone()).unwrap();
        let id = view.names["G"];
        ui.update(Size::new(600.0, 400.0)).unwrap();
        assert_eq!(ui.gauge_value(id), Some(85.0));
        model
            .borrow_mut()
            .set("Value", DataValue::Number(10.0))
            .unwrap();
        view.sync_bindings(&mut ui);
        assert_eq!(ui.gauge_value(id), Some(85.0));
        let ids = ui.runtime.tree().live_ids();
        assert!(MountedView::mount_with_context(&mut ui, root, &definition, model).is_err());
        assert_eq!(ui.runtime.tree().live_ids(), ids);
        let fallback = content("<DigitalGauge Value='{Binding Missing, FallbackValue=65}'/>");
        let view = MountedView::mount(&mut ui, root, &fallback).unwrap();
        assert!(
            view.roles
                .keys()
                .any(|id| ui.gauge_value(*id) == Some(65.0))
        );
    }
    #[test]
    fn gauge_demo_mounts_through_real_form_and_has_complete_geometry() {
        let definition = rust_desktop_ui_xaml::parse(include_str!("../GaugeWindow.xaml")).unwrap();
        for dark in [false, true] {
            let mut form =
                crate::form::Form::new_with_model(definition.clone(), false, None).unwrap();
            form.ui
                .set_theme(if dark {
                    Theme::FluentDark
                } else {
                    Theme::FluentLight
                })
                .unwrap();
            form.ui.update(Size::new(1000.0, 820.0)).unwrap();
            for name in ["GaugeCircular", "GaugeLinear", "GaugeDigital", "GaugeState"] {
                let id = form.view.find_name(name).unwrap();
                assert_eq!(form.ui.gauge_value(id), Some(35.0));
                assert_eq!(
                    form.ui.gauge_display_status(id),
                    Some(rust_desktop_ui_controls::GaugeDisplayStatus::Full)
                );
            }
        }
    }
    #[test]
    fn gauge_demo_commands_have_visible_scene_captions_after_wrapping() {
        let definition = rust_desktop_ui_xaml::parse(include_str!("../GaugeWindow.xaml")).unwrap();
        for theme in [Theme::FluentLight, Theme::FluentDark] {
            let mut form =
                crate::form::Form::new_with_model(definition.clone(), false, None).unwrap();
            form.ui.set_theme(theme).unwrap();
            for width in [1000.0, 760.0] {
                form.ui.update(Size::new(width, 820.0)).unwrap();
                for (name, caption) in [
                    ("GaugeLow", "Норма 35"),
                    ("GaugeHigh", "Высокое 85"),
                    ("GaugeInvalid", "Вне шкалы"),
                    ("GaugeCompact", "Компактно"),
                    ("GaugeRestore", "Развернуть"),
                    ("GaugeLight", "Светлая"),
                    ("GaugeDark", "Тёмная"),
                ] {
                    let id = form.view.find_name(name).unwrap();
                    let button = form.ui.runtime.bounds(id).unwrap();
                    let run = form
                        .ui
                        .scene()
                        .texts
                        .iter()
                        .find(|r| r.text == caption && r.bounds.intersection(button).is_some())
                        .unwrap_or_else(|| panic!("Missing drawn caption {name} at {width}"));
                    let visible = run.bounds.intersection(run.clip).unwrap();
                    assert!(visible.width > 0.0 && visible.height > 0.0);
                    assert!(
                        visible.width + 0.01 >= run.bounds.width
                            && visible.height + 0.01 >= run.bounds.height,
                        "Clipped caption {name}"
                    );
                }
            }
        }
    }
}
