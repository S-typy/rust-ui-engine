//! View-only navigation shared by pointer, keyboard and programmatic calls.
use super::*;

impl Controls {
    pub(super) fn chart_plot(&self, id: WidgetId) -> Option<Rect> {
        chart_paint::plot(self.chart_model(id)?, self.runtime.bounds(id)?)
    }
    pub(super) fn publish_chart_view(
        &mut self,
        id: WidgetId,
        next: ChartModel,
        cancel: bool,
    ) -> Result<(), ControlError> {
        let style = self
            .runtime
            .tree()
            .node(id)
            .ok_or_else(|| error("Removed Chart"))?
            .props
            .style
            .clone();
        let bounds = self
            .runtime
            .bounds(id)
            .unwrap_or(Rect::new(0.0, 0.0, 640.0, 400.0));
        let summary = self.preflight_chart(&next, &style, bounds)?;
        if cancel {
            self.cancel_chart_owner(id)?;
        }
        let state = self
            .charts
            .get_mut(&id)
            .ok_or_else(|| error("Not a Chart"))?;
        state.model = next;
        state.summary = summary;
        // A pending point selection is independent of the view transform.
        self.scene_dirty = true;
        Ok(())
    }
    /// Zoom Cartesian axes around a normalized lower-left axis-space anchor.
    /// A successful change cancels a captured gesture and keeps point selection.
    pub fn zoom_chart(
        &mut self,
        id: WidgetId,
        factor: f64,
        anchor: (f64, f64),
    ) -> Result<bool, ControlError> {
        let mut next = self
            .chart_model(id)
            .ok_or_else(|| error("Not a Chart"))?
            .clone();
        let changed = next.zoom_at(factor, anchor).map_err(error)?;
        if changed {
            self.publish_chart_view(id, next, true)?;
        }
        Ok(changed)
    }
    /// Pan by fractions of the current X/Y spans, toward larger data coordinates.
    pub fn pan_chart(&mut self, id: WidgetId, x: f64, y: f64) -> Result<bool, ControlError> {
        let mut next = self
            .chart_model(id)
            .ok_or_else(|| error("Not a Chart"))?
            .clone();
        let changed = next.pan(x, y).map_err(error)?;
        if changed {
            self.publish_chart_view(id, next, true)?;
        }
        Ok(changed)
    }
    /// Return both Cartesian axes to data-derived ranges without clearing selection.
    pub fn reset_chart_view(&mut self, id: WidgetId) -> Result<bool, ControlError> {
        let mut next = self
            .chart_model(id)
            .ok_or_else(|| error("Not a Chart"))?
            .clone();
        let changed = next.set_view_ranges(None, None).map_err(error)?;
        if changed {
            self.publish_chart_view(id, next, true)?;
        }
        Ok(changed)
    }
    fn navigable_chart(&self, id: WidgetId) -> bool {
        self.is_enabled(id)
            && self.chart_model(id).is_some_and(|m| m.family().is_none())
            && self.chart_plot(id).is_some()
    }
    pub(super) fn chart_navigation_input(
        &mut self,
        event: &InputEvent,
    ) -> Result<bool, ControlError> {
        if self.popup.is_some() {
            return Ok(false);
        }
        if let Some(press) = self.chart_press
            && let Some(pan) = press.pan
        {
            match event {
                InputEvent::PointerMoved { position } => {
                    if !position.is_finite() {
                        return Ok(true);
                    }
                    let mut next = self.chart_model(press.owner).unwrap().clone();
                    if *position == pan.start {
                        next.set_view_ranges(pan.original_x, pan.original_y)
                            .map_err(error)?;
                    } else {
                        next.set_view_ranges(Some(pan.x), Some(pan.y))
                            .map_err(error)?;
                    }
                    let x = -f64::from(position.x - pan.start.x) / f64::from(pan.plot.width);
                    let y = f64::from(position.y - pan.start.y) / f64::from(pan.plot.height);
                    if next.pan(x, y).is_ok() {
                        self.publish_chart_view(press.owner, next, false)?;
                    }
                    return Ok(true);
                }
                InputEvent::PointerUp {
                    button: PointerButton::Primary,
                    ..
                } => {
                    // Keep the accepted preview, release capture without rollback.
                    self.chart_press = None;
                    if self.runtime.captured() == Some(press.owner) {
                        self.runtime.apply(Mutation::CapturePointer(None))?;
                    }
                    return Ok(true);
                }
                _ => return Ok(true),
            }
        }
        // A selection press remains solely responsible for its capture.
        if self.chart_press.is_some() {
            return Ok(false);
        }
        match event {
            InputEvent::PointerDown {
                position,
                button: PointerButton::Primary,
                modifiers,
            } if modifiers.control && !modifiers.alt && !modifiers.super_key => {
                let Some(id) = self
                    .runtime
                    .hit_test(*position)
                    .filter(|id| self.navigable_chart(*id))
                else {
                    return Ok(false);
                };
                let plot = self.chart_plot(id).unwrap();
                if !plot.contains(position.x, position.y) {
                    return Ok(false);
                }
                let model = self.chart_model(id).unwrap();
                let pan = ChartPan {
                    start: *position,
                    plot,
                    x: model.x_range(),
                    y: model.y_range(),
                    original_x: model.settings().x_range,
                    original_y: model.settings().y_range,
                };
                if !self.try_popup_focus(id)? {
                    return Ok(false);
                }
                self.synchronize_editor_focus()?;
                self.runtime.apply(Mutation::CapturePointer(Some(id)))?;
                self.chart_press = Some(ChartPress {
                    owner: id,
                    key: None,
                    pan: Some(pan),
                });
                return Ok(true);
            }
            InputEvent::Wheel {
                position,
                delta,
                modifiers,
            } if modifiers.control && !modifiers.alt && !modifiers.super_key => {
                let Some(id) = self
                    .runtime
                    .hit_test(*position)
                    .filter(|id| self.navigable_chart(*id))
                else {
                    return Ok(false);
                };
                let plot = self.chart_plot(id).unwrap();
                if !plot.contains(position.x, position.y) {
                    return Ok(false);
                }
                if !delta.is_finite() || delta.y == 0.0 {
                    return Ok(true);
                }
                let anchor = (
                    f64::from((position.x - plot.x) / plot.width),
                    1.0 - f64::from((position.y - plot.y) / plot.height),
                );
                let mut next = self.chart_model(id).unwrap().clone();
                // Wheel deltas describe scrolling; negative Y is wheel-forward.
                let factor = (-f64::from(delta.y) / 240.0).clamp(-2.0, 2.0).exp();
                if next.zoom_at(factor, anchor).is_ok() {
                    self.publish_chart_view(id, next, true)?;
                }
                return Ok(true);
            }
            InputEvent::KeyDown { key, modifiers, .. }
                if modifiers.control && !modifiers.alt && !modifiers.super_key =>
            {
                let Some(id) = self
                    .runtime
                    .focused()
                    .filter(|id| self.navigable_chart(*id))
                else {
                    return Ok(false);
                };
                let mut next = self.chart_model(id).unwrap().clone();
                let result = match key {
                    Key::ArrowLeft => next.pan(-0.1, 0.0),
                    Key::ArrowRight => next.pan(0.1, 0.0),
                    Key::ArrowUp => next.pan(0.0, 0.1),
                    Key::ArrowDown => next.pan(0.0, -0.1),
                    Key::Home => next.set_view_ranges(None, None),
                    Key::Character(s) if s == "+" || s == "=" => next.zoom_at(1.25, (0.5, 0.5)),
                    Key::Character(s) if s == "-" => next.zoom_at(0.8, (0.5, 0.5)),
                    Key::Character(s) if s == "0" => next.set_view_ranges(None, None),
                    _ => return Ok(false),
                };
                if matches!(result, Ok(true)) {
                    self.publish_chart_view(id, next, true)?;
                }
                return Ok(true);
            }
            _ => {}
        }
        Ok(false)
    }
}
