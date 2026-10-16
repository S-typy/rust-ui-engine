//! Status containers reuse the ordinary form children and command registry.
use super::*;
use rust_desktop_ui_controls::{
    ProgressBarSettings, StatusBarItemSettings, StatusBarOptions, StatusItemWidth, StatusPlacement,
    ZoomControlSettings,
};
use rust_desktop_ui_core::{Align, GridTrack};
use rust_desktop_ui_xaml::StatusBarPlacement;

impl MountedView {
    pub(super) fn mount_status_element(
        &mut self,
        ui: &mut Controls,
        parent: WidgetId,
        element: &ElementDefinition,
        mut style: LayoutStyle,
        width: f32,
        context: &str,
    ) -> Result<WidgetId, String> {
        if style.width == Length::Auto
            && matches!(
                element.kind,
                ElementKind::ProgressBar(_) | ElementKind::ZoomControl(_)
            )
        {
            style.width = Length::Percent(1.0);
        }
        match &element.kind {
            ElementKind::StatusBar(bar) | ElementKind::RibbonStatusBar(bar) => {
                let parts = if matches!(element.kind, ElementKind::RibbonStatusBar(_)) {
                    ui.ribbon_status_bar(parent, style)
                } else {
                    ui.status_bar(parent, style)
                }
                .map_err(err)?;
                ui.configure_status_bar(
                    parts,
                    StatusBarOptions {
                        customizable: bar.is_customizable,
                        resize_grip: bar.show_resize_grip,
                    },
                )
                .map_err(err)?;
                let managed = bar.is_customizable || bar.show_resize_grip || bar.items.iter().any(|item| {
                    matches!(&item.kind, ElementKind::StatusBarItem(settings) if settings.key.is_some() || settings.label.is_some() || !settings.can_hide)
                });
                for (index, item) in bar.items.iter().enumerate() {
                    let ElementKind::StatusBarItem(settings) = &item.kind else {
                        return Err("StatusBar accepts only StatusBarItem children".into());
                    };
                    let region = match settings.placement {
                        StatusBarPlacement::Left => parts.left,
                        StatusBarPlacement::Right => parts.right,
                    };
                    let host =
                        self.mount_element(ui, region, item, Axis::Horizontal, width, context)?;
                    if managed {
                        let key = settings
                            .key
                            .clone()
                            .or_else(|| item.common.name.clone())
                            .unwrap_or_else(|| format!("section-{index}"));
                        let label = settings
                            .label
                            .clone()
                            .or_else(|| item.common.automation_name.clone())
                            .or_else(|| item.common.name.clone())
                            .unwrap_or_else(|| key.clone());
                        let item_width = item.common.width.map_or(
                            StatusItemWidth::Stretch {
                                minimum: item.common.min_width.max(80.0),
                            },
                            StatusItemWidth::Fixed,
                        );
                        ui.register_status_bar_item(
                            parts,
                            host,
                            StatusBarItemSettings {
                                key,
                                label,
                                placement: match settings.placement {
                                    StatusBarPlacement::Left => StatusPlacement::Left,
                                    StatusBarPlacement::Right => StatusPlacement::Right,
                                },
                                width: item_width,
                                visible: item.common.visibility
                                    == rust_desktop_ui_xaml::Visibility::Visible,
                                can_hide: settings.can_hide,
                            },
                        )
                        .map_err(err)?;
                    }
                }
                Ok(parts.container)
            }
            ElementKind::StatusBarItem(item) => {
                style.kind = LayoutKind::Grid;
                style.grid_columns = vec![GridTrack::Fr(1.0)];
                style.grid_rows = vec![GridTrack::Fr(1.0)];
                if element.common.vertical_alignment == rust_desktop_ui_xaml::Alignment::Stretch {
                    style.vertical_alignment = Some(Align::Center);
                }
                if item.placement == StatusBarPlacement::Left
                    && element.common.width.is_none()
                    && !element.common.bindings.contains_key("Width")
                {
                    style.width = Length::Auto;
                    style.flex_grow = 1.0;
                    style.flex_shrink = 1.0;
                }
                let id = ui.transparent_panel(parent, style).map_err(err)?;
                ui.set_role(id, Role::Group);
                if let Some(child) = &item.child {
                    let mut child = child.as_ref().clone();
                    if child.common.vertical_alignment == rust_desktop_ui_xaml::Alignment::Stretch {
                        child.common.vertical_alignment = rust_desktop_ui_xaml::Alignment::Center;
                    }
                    self.mount_element(ui, id, &child, Axis::Vertical, width, context)?;
                }
                Ok(id)
            }
            ElementKind::ProgressBar(progress) => {
                let id = ui
                    .progress_bar(
                        parent,
                        ProgressBarSettings {
                            minimum: progress.minimum,
                            maximum: progress.maximum,
                            value: progress.value,
                        },
                        style,
                    )
                    .map_err(err)?;
                ui.set_progress_indeterminate(id, progress.is_indeterminate)
                    .map_err(err)?;
                if progress.reduced_motion {
                    ui.set_reduced_motion(true);
                }
                Ok(id)
            }
            ElementKind::ZoomControl(zoom) => ui
                .zoom_control(
                    parent,
                    ZoomControlSettings {
                        minimum: zoom.minimum,
                        maximum: zoom.maximum,
                        value: zoom.value,
                        step: zoom.step,
                    },
                    style,
                )
                .map_err(err),
            _ => Err("Expected a status element".into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::form::Form;
    use rust_desktop_ui_core::Size;
    use rust_desktop_ui_platform_winit::DesktopApp;

    #[test]
    fn status_text_is_centered_and_explicit_top_alignment_is_preserved() {
        let definition = rust_desktop_ui_xaml::parse(
            r#"<Window xmlns="urn:rust-ui-engine:ui" xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"><StatusBar IsCustomizable="True" Height="50"><StatusBarItem x:Name="CenterHost" Key="center" Width="160"><TextBlock x:Name="Centered" Text="Status" Height="18"/></StatusBarItem><StatusBarItem x:Name="TopHost" Key="top" Width="160"><TextBlock x:Name="Top" Text="Top" Height="18" VerticalAlignment="Top"/></StatusBarItem></StatusBar></Window>"#,
        )
        .unwrap();
        let mut form = Form::new(definition, false).unwrap();
        form.update(Size::new(640.0, 300.0), 1.0).unwrap();
        let rect = |name| {
            form.ui
                .runtime
                .bounds(form.find_name(name).unwrap())
                .unwrap()
        };
        let center = rect("Centered");
        let host = rect("CenterHost");
        assert!((center.y + center.height / 2.0 - host.y - host.height / 2.0).abs() < 0.1);
        assert_eq!(rect("Top").y, rect("TopHost").y);
    }
}
