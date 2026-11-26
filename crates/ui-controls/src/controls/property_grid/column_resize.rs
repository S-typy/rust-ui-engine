use super::*;
use rust_desktop_ui_core::PointerButton;

#[derive(Default)]
pub(super) struct ResizeState {
    drag: Option<ColumnDrag>,
    pointer: Option<Point>,
    highlighted: bool,
}

#[derive(Clone, Copy)]
struct ColumnDrag {
    row: WidgetId,
    start_x: f32,
    offset: f32,
    original: GridTrack,
    applied: GridTrack,
}

impl PropertyGrid {
    /// Whether the host should display the horizontal resize cursor at this window position.
    /// During a captured drag the cursor follows the pointer outside the original row.
    pub fn column_resize_cursor(&self, ui: &Controls, position: Point) -> bool {
        if !ui.window_focused || ui.popup_open() || ui.runtime.drag_session().is_some() {
            return false;
        }
        if let Some(capture) = ui.runtime.captured() {
            return self
                .column_resize
                .drag
                .is_some_and(|d| d.row == capture && ui.is_enabled(d.row));
        }
        position.is_finite() && self.divider_at(ui, position).is_some()
    }

    fn highlight_resize(&mut self, ui: &mut Controls) {
        let highlighted = self.column_resize.drag.is_some()
            || self
                .column_resize
                .pointer
                .is_some_and(|point| self.column_resize_cursor(ui, point));
        if self.column_resize.highlighted != highlighted {
            self.column_resize.highlighted = highlighted;
            self.highlight_column_dividers(ui, highlighted);
        }
    }

    fn end_resize(&mut self, ui: &mut Controls, cancel: bool) -> Result<(), ControlError> {
        if let Some(drag) = self.column_resize.drag.take() {
            if ui.runtime.captured() == Some(drag.row) {
                ui.runtime.apply(Mutation::CapturePointer(None))?;
            }
            // A new width set by the application supersedes the interrupted drag.
            if cancel && self.label_track == drag.applied && self.label_track != drag.original {
                self.set_label_track(ui, drag.original)?;
            }
        }
        self.highlight_resize(ui);
        Ok(())
    }

    pub(super) fn sync_column_resize(&mut self, ui: &mut Controls) -> Result<(), ControlError> {
        if let Some(drag) = self.column_resize.drag
            && (ui.runtime.captured() != Some(drag.row)
                || !ui.is_enabled(drag.row)
                || !ui.window_focused
                || ui.popup_open()
                || ui.runtime.drag_session().is_some()
                || self.label_track != drag.applied)
        {
            return self.end_resize(ui, true);
        }
        self.highlight_resize(ui);
        Ok(())
    }

    fn drag_to(&mut self, ui: &mut Controls, drag: ColumnDrag, x: f32) -> Result<(), ControlError> {
        if !x.is_finite() {
            return Ok(());
        }
        let track = if x == drag.start_x {
            // A press and release alone must preserve even a rounded percentage.
            drag.original
        } else {
            let Some(track) = self.divider_track_for_x(ui, drag.row, x - drag.offset) else {
                return self.end_resize(ui, true);
            };
            track
        };
        if self.label_track != track {
            self.set_label_track(ui, track)?;
            self.column_resize.drag = Some(ColumnDrag {
                applied: track,
                ..drag
            });
        }
        Ok(())
    }

    pub(super) fn column_resize_input(
        &mut self,
        ui: &mut Controls,
        event: &InputEvent,
    ) -> Result<bool, ControlError> {
        match event {
            InputEvent::PointerMoved { position }
            | InputEvent::PointerDown { position, .. }
            | InputEvent::PointerUp { position, .. }
            | InputEvent::Wheel { position, .. } => self.column_resize.pointer = Some(*position),
            InputEvent::PointerLeft | InputEvent::WindowFocused(false) => {
                self.column_resize.pointer = None;
            }
            _ => {}
        }
        self.sync_column_resize(ui)?;
        if let Some(drag) = self.column_resize.drag {
            match event {
                InputEvent::WindowFocused(false) => self.end_resize(ui, true)?,
                InputEvent::KeyDown {
                    key: Key::Escape, ..
                } => {
                    self.end_resize(ui, true)?;
                    return Ok(true);
                }
                InputEvent::PointerMoved { position } => {
                    self.drag_to(ui, drag, position.x)?;
                    return Ok(true);
                }
                InputEvent::PointerUp {
                    position,
                    button: PointerButton::Primary,
                    ..
                } => {
                    self.drag_to(ui, drag, position.x)?;
                    self.end_resize(ui, false)?;
                    return Ok(true);
                }
                InputEvent::PointerDown { .. }
                | InputEvent::PointerUp { .. }
                | InputEvent::Wheel { .. } => {
                    return Ok(true);
                }
                _ => {}
            }
            return Ok(false);
        }
        if !ui.window_focused
            || ui.popup_open()
            || ui.runtime.captured().is_some()
            || ui.runtime.drag_session().is_some()
        {
            return Ok(false);
        }
        match event {
            InputEvent::PointerDown {
                position,
                button: PointerButton::Primary,
                ..
            } => {
                let Some(row) = self.divider_at(ui, *position) else {
                    return Ok(false);
                };
                let Some((_, divider)) = self.divider_geometry(ui, row) else {
                    return Ok(false);
                };
                // Capture without taking focus: a name/value boundary is not an editor action.
                ui.runtime.apply(Mutation::CapturePointer(Some(row)))?;
                self.column_resize.drag = Some(ColumnDrag {
                    row,
                    start_x: position.x,
                    offset: position.x - divider,
                    original: self.label_track,
                    applied: self.label_track,
                });
                self.highlight_resize(ui);
                Ok(true)
            }
            InputEvent::KeyDown {
                key: key @ (Key::ArrowLeft | Key::ArrowRight),
                modifiers,
                ..
            } if modifiers.alt && modifiers.shift && !modifiers.control && !modifiers.super_key => {
                let Some(focus) = ui.runtime.focused() else {
                    return Ok(false);
                };
                let Some(property) = self
                    .properties
                    .iter()
                    .find(|p| ui.flyout_descendant(focus, p.row))
                else {
                    return Ok(false);
                };
                let Some((_, x)) = self.divider_geometry(ui, property.row) else {
                    return Ok(false);
                };
                let step = if *key == Key::ArrowLeft { -8. } else { 8. };
                if let Some(track) = self.divider_track_for_x(ui, property.row, x + step) {
                    self.set_label_track(ui, track)?;
                }
                Ok(true)
            }
            _ => Ok(false),
        }
    }
}
