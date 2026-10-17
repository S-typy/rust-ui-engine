//! Read-only chart data with independent pointer/keyboard point selection.
use super::*;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChartDisplayStatus {
    Full,
    Compact,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChartSelectionChange {
    pub control: WidgetId,
    pub previous: Option<ChartPointKey>,
    pub selected: Option<ChartPointKey>,
}
pub(super) struct ChartState {
    pub model: ChartModel,
    pub summary: String,
    pub event: Option<ChartSelectionChange>,
}
#[derive(Clone, Copy)]
pub(super) struct ChartPress {
    owner: WidgetId,
    key: Option<ChartPointKey>,
}
fn error(e: impl std::fmt::Display) -> ControlError {
    ControlError::Invalid(e.to_string())
}
impl Controls {
    fn chart_summary(model: &ChartModel) -> String {
        if let Some(key) = model.selected() {
            let series = model.series().iter().find(|s| s.id == key.series).unwrap();
            let point = model.point(key).unwrap();
            format!(
                "{}: X {}, Y {}",
                series.label,
                ChartModel::format_number(point.x),
                ChartModel::format_number(point.y.unwrap())
            )
        } else {
            format!("Точка не выбрана. Серий: {}", model.series().len())
        }
    }
    fn preflight_chart(
        &mut self,
        model: &ChartModel,
        style: &LayoutStyle,
        bounds: Rect,
    ) -> Result<String, ControlError> {
        let summary = Self::chart_summary(model);
        self.preflight_new_editor_text(&format!("Компактная диаграмма. {summary}"), style, false)?;
        for value in std::iter::once(summary.as_str())
            .chain(model.series().iter().map(|s| s.label.as_str()))
            .chain([
                model.settings().x_title.as_str(),
                model.settings().y_title.as_str(),
            ])
        {
            if value.contains(['\u{2028}', '\u{2029}']) {
                return Err(error("Chart labels must be single-line"));
            }
            self.preflight_new_editor_text(value, style, false)?;
        }
        for series in model.series() {
            self.preflight_new_editor_text(
                &format!(
                    "Компактная диаграмма. {}: X -1.797e308, Y -1.797e308",
                    series.label
                ),
                style,
                false,
            )?;
        }
        for value in model
            .x_range()
            .ticks()
            .map_err(error)?
            .into_iter()
            .chain(model.y_range().ticks().map_err(error)?)
        {
            self.preflight_new_editor_text(&ChartModel::format_number(value), style, false)?;
        }
        if let Some(plot) = chart_paint::plot(model, bounds) {
            model.geometry(plot).map_err(error)?;
        }
        Ok(summary)
    }
    pub fn chart(
        &mut self,
        parent: WidgetId,
        model: ChartModel,
        mut style: LayoutStyle,
    ) -> Result<WidgetId, ControlError> {
        if style.width == Length::Auto {
            style.width = Length::Px(640.0)
        }
        if style.height == Length::Auto {
            style.height = Length::Px(if model.settings().view == ChartView::Sparkline {
                80.0
            } else {
                400.0
            })
        }
        style.padding = Edges::all(0.0);
        let summary = self.preflight_chart(&model, &style, Rect::new(0.0, 0.0, 640.0, 400.0))?;
        let id = self.transparent_panel(parent, style)?;
        self.set_role(id, Role::Group);
        self.edit_props(id, |p| p.focusable = true)?;
        self.controls.get_mut(&id).unwrap().label = "Диаграмма".into();
        self.charts.insert(
            id,
            ChartState {
                model,
                summary,
                event: None,
            },
        );
        self.scene_dirty = true;
        Ok(id)
    }
    pub(super) fn chart_accessible_description(&self, id: WidgetId) -> Option<String> {
        match (self.chart_semantic_value(id), self.badge_description(id)) {
            (Some(a), Some(b)) => Some(format!("{a}. {b}")),
            (Some(a), None) => Some(a.into()),
            (None, b) => b,
        }
    }
    pub fn chart_model(&self, id: WidgetId) -> Option<&ChartModel> {
        self.charts.get(&id).map(|s| &s.model)
    }
    pub fn chart_semantic_value(&self, id: WidgetId) -> Option<&str> {
        self.charts.get(&id).map(|s| s.summary.as_str())
    }
    pub fn chart_display_status(&self, id: WidgetId) -> Option<ChartDisplayStatus> {
        Some(
            if chart_paint::plot(self.chart_model(id)?, self.runtime.bounds(id)?).is_some() {
                ChartDisplayStatus::Full
            } else {
                ChartDisplayStatus::Compact
            },
        )
    }
    pub fn chart_geometry(&self, id: WidgetId) -> Result<Option<ChartGeometry>, ControlError> {
        let m = self.chart_model(id).ok_or_else(|| error("Not a Chart"))?;
        let Some(bounds) = self.runtime.bounds(id) else {
            return Ok(None);
        };
        chart_paint::plot(m, bounds)
            .map(|p| m.geometry(p).map_err(error))
            .transpose()
    }
    pub fn set_chart_model(&mut self, id: WidgetId, model: ChartModel) -> Result<(), ControlError> {
        if !self.charts.contains_key(&id) {
            return Err(error("Not a Chart"));
        }
        let style = self
            .runtime
            .tree()
            .node(id)
            .ok_or_else(|| error("Removed Chart"))?
            .props
            .style
            .clone();
        let summary = self.preflight_chart(
            &model,
            &style,
            self.runtime
                .bounds(id)
                .unwrap_or(Rect::new(0.0, 0.0, 640.0, 400.0)),
        )?;
        self.cancel_chart_owner(id)?;
        self.charts.insert(
            id,
            ChartState {
                model,
                summary,
                event: None,
            },
        );
        self.scene_dirty = true;
        Ok(())
    }
    pub fn set_chart_selected(
        &mut self,
        id: WidgetId,
        key: Option<ChartPointKey>,
    ) -> Result<(), ControlError> {
        self.select_chart(id, key, false)
    }
    fn select_chart(
        &mut self,
        id: WidgetId,
        key: Option<ChartPointKey>,
        user: bool,
    ) -> Result<(), ControlError> {
        let entry = self.charts.get(&id).ok_or_else(|| error("Not a Chart"))?;
        let previous = entry.model.selected();
        if previous == key {
            if !user {
                self.cancel_chart_owner(id)?;
            }
            return Ok(());
        }
        let mut model = entry.model.clone();
        model.select(key).map_err(error)?;
        let summary = Self::chart_summary(&model);
        self.measure_text(&summary, 4096.0, false)?;
        self.measure_text(&format!("Компактная диаграмма. {summary}"), 4096.0, false)?;
        if !user {
            self.cancel_chart_owner(id)?;
        }
        let entry = self.charts.get_mut(&id).unwrap();
        entry.model = model;
        entry.summary = summary;
        if user {
            let previous = entry.event.map(|e| e.previous).unwrap_or(previous);
            entry.event = (previous != key).then_some(ChartSelectionChange {
                control: id,
                previous,
                selected: key,
            });
        } else {
            entry.event = None
        }
        self.scene_dirty = true;
        Ok(())
    }
    pub fn take_chart_selection_changes(&mut self) -> Vec<ChartSelectionChange> {
        self.charts
            .values_mut()
            .filter_map(|s| s.event.take())
            .collect()
    }
    fn cancel_chart_owner(&mut self, id: WidgetId) -> Result<(), ControlError> {
        if self.chart_press.is_some_and(|p| p.owner == id) {
            self.cancel_chart_interaction()?;
        }
        Ok(())
    }
    pub(super) fn cancel_chart_interaction(&mut self) -> Result<(), ControlError> {
        if let Some(p) = self.chart_press.take()
            && self.runtime.captured() == Some(p.owner)
        {
            self.runtime.apply(Mutation::CapturePointer(None))?;
        }
        Ok(())
    }
    pub(super) fn sync_charts(&mut self) -> Result<(), ControlError> {
        if let Some(p) = self.chart_press
            && (!self.is_enabled(p.owner) || self.runtime.captured() != Some(p.owner))
        {
            self.cancel_chart_interaction()?;
        }
        self.charts
            .retain(|id, _| self.runtime.tree().node(*id).is_some());
        Ok(())
    }
    pub(super) fn chart_before_input(&mut self, event: &InputEvent) -> Result<bool, ControlError> {
        if matches!(
            event,
            InputEvent::WindowFocused(false)
                | InputEvent::KeyDown {
                    key: Key::Escape,
                    ..
                }
        ) && self.chart_press.is_some()
        {
            self.cancel_chart_interaction()?;
            return Ok(matches!(event, InputEvent::KeyDown { .. }));
        }
        self.sync_charts()?;
        if self.popup.is_some() && matches!(event, InputEvent::KeyDown { .. }) {
            return Ok(false);
        }
        if let InputEvent::PointerDown { position, .. } = event
            && self.popup.is_some()
            && self
                .runtime
                .hit_test(*position)
                .is_some_and(|id| self.charts.contains_key(&id))
        {
            self.popup_pointer_down(*position)?;
            self.update(self.viewport)?;
        }
        if let Some(p) = self.chart_press {
            match event {
                InputEvent::PointerUp {
                    position,
                    button: PointerButton::Primary,
                    ..
                } => {
                    self.cancel_chart_interaction()?;
                    if self.runtime.hit_test(*position) == Some(p.owner)
                        && self.is_enabled(p.owner)
                        && self
                            .runtime
                            .bounds(p.owner)
                            .zip(self.runtime.clip(p.owner))
                            .and_then(|(b, c)| b.intersection(c))
                            .is_some_and(|r| r.contains(position.x, position.y))
                    {
                        let hit = self
                            .chart_geometry(p.owner)?
                            .and_then(|g| g.hit_test(*position, 10.0));
                        if hit == p.key {
                            self.select_chart(p.owner, hit, true)?;
                        }
                    }
                    return Ok(true);
                }
                InputEvent::PointerMoved { .. } => return Ok(true),
                _ => {}
            }
        }
        match event {
            InputEvent::PointerDown {
                position,
                button: PointerButton::Primary,
                ..
            } => {
                let id = self
                    .runtime
                    .hit_test(*position)
                    .filter(|id| self.charts.contains_key(id) && self.is_enabled(*id));
                if let Some(id) = id {
                    if self.chart_display_status(id) == Some(ChartDisplayStatus::Compact) {
                        return Ok(false);
                    }
                    let key = self
                        .chart_geometry(id)?
                        .and_then(|g| g.hit_test(*position, 10.0));
                    if !self.try_popup_focus(id)? {
                        return Ok(false);
                    }
                    self.synchronize_editor_focus()?;
                    self.runtime.apply(Mutation::CapturePointer(Some(id)))?;
                    self.chart_press = Some(ChartPress { owner: id, key });
                    return Ok(true);
                }
            }
            InputEvent::KeyDown { key, modifiers, .. }
                if !modifiers.alt && !modifiers.control && !modifiers.super_key =>
            {
                if let Some(id) = self
                    .runtime
                    .focused()
                    .filter(|id| self.charts.contains_key(id) && self.is_enabled(*id))
                {
                    let model = self.chart_model(id).unwrap();
                    let points: Vec<_> = self
                        .chart_geometry(id)?
                        .map(|g| g.points.into_iter().map(|p| p.key).collect())
                        .unwrap_or_default();
                    let index = model
                        .selected()
                        .and_then(|k| points.iter().position(|p| *p == k));
                    let selected = match key {
                        Key::Escape => Some(None),
                        Key::Home => Some(points.first().copied()),
                        Key::End => Some(points.last().copied()),
                        Key::ArrowRight | Key::ArrowDown => Some(
                            points
                                .get(
                                    index
                                        .map(|i| (i + 1).min(points.len().saturating_sub(1)))
                                        .unwrap_or(0),
                                )
                                .copied(),
                        ),
                        Key::ArrowLeft | Key::ArrowUp => {
                            Some(points.get(index.unwrap_or(0).saturating_sub(1)).copied())
                        }
                        _ => None,
                    };
                    if let Some(selected) = selected {
                        self.select_chart(id, selected, true)?;
                        return Ok(true);
                    }
                }
            }
            _ => {}
        }
        Ok(false)
    }
}
