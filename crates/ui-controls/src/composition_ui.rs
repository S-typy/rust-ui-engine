use crate::{ControlError, Controls};
use rust_desktop_ui_core::*;
pub(crate) fn error(e: impl ToString) -> ControlError {
    ControlError::Invalid(e.to_string())
}
pub(crate) fn cell(row: u16, col: u16) -> LayoutStyle {
    LayoutStyle {
        grid_row: GridPlacement {
            start: Some(row),
            span: 1,
        },
        grid_column: GridPlacement {
            start: Some(col),
            span: 1,
        },
        ..Default::default()
    }
}
pub(crate) fn toolbar(
    ui: &mut Controls,
    parent: WidgetId,
    row: u16,
    items: &[(&'static str, &'static str)],
) -> Result<Vec<(WidgetId, &'static str)>, ControlError> {
    let host = ui.transparent_panel(
        parent,
        LayoutStyle {
            kind: LayoutKind::Wrap(Axis::Horizontal),
            gap: 4.0,
            ..cell(row, 1)
        },
    )?;
    items
        .iter()
        .map(|(label, action)| {
            let measured = ui.measure_button_content(
                label,
                &crate::ButtonStyle::default(),
                None,
                crate::ButtonContentLayout::Leading,
                2048.0,
            )?;
            Ok((
                ui.button(
                    host,
                    *label,
                    None,
                    LayoutStyle {
                        width: Length::Px(measured.width.ceil() + 4.0),
                        height: Length::Px(measured.height.ceil().max(36.0)),
                        ..Default::default()
                    },
                )?,
                *action,
            ))
        })
        .collect()
}
pub(crate) fn frame(
    ui: &mut Controls,
    parent: WidgetId,
    style: LayoutStyle,
) -> Result<WidgetId, ControlError> {
    ui.panel(
        parent,
        LayoutStyle {
            kind: LayoutKind::Grid,
            grid_columns: vec![GridTrack::Fr(1.0)],
            grid_rows: vec![GridTrack::Auto, GridTrack::Fr(1.0), GridTrack::Auto],
            gap: 6.0,
            ..style
        },
    )
}
pub(crate) fn text(
    scene: &mut Scene,
    text: impl Into<String>,
    bounds: Rect,
    color: Color,
    size: f32,
) {
    scene.text(TextRun::new(text, bounds, color, size));
}
