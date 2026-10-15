//! Status containers reuse the ordinary form children and command registry.
use super::*;
use rust_desktop_ui_controls::{ProgressBarSettings, ZoomControlSettings};
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
                for item in &bar.items {
                    let ElementKind::StatusBarItem(settings) = &item.kind else {
                        return Err("StatusBar accepts only StatusBarItem children".into());
                    };
                    let region = match settings.placement {
                        StatusBarPlacement::Left => parts.left,
                        StatusBarPlacement::Right => parts.right,
                    };
                    self.mount_element(ui, region, item, Axis::Horizontal, width, context)?;
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
                    self.mount_element(ui, id, child, Axis::Vertical, width, context)?;
                }
                Ok(id)
            }
            ElementKind::ProgressBar(progress) => ui
                .progress_bar(
                    parent,
                    ProgressBarSettings {
                        minimum: progress.minimum,
                        maximum: progress.maximum,
                        value: progress.value,
                    },
                    style,
                )
                .map_err(err),
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
