use super::*;

pub(super) struct GaugeState {
    pub(super) settings: GaugeSettings,
    pub(super) value_text: String,
    pub(super) semantic_value: String,
}
impl Controls {
    fn preflight_gauge(
        &mut self,
        settings: &GaugeSettings,
        style: &LayoutStyle,
    ) -> Result<(), ControlError> {
        settings.validate()?;
        for text in std::iter::once(settings.semantic_value())
            .chain(
                settings
                    .ticks()
                    .into_iter()
                    .filter(|t| t.1)
                    .map(|t| settings.format_number(t.0)),
            )
            .chain(settings.ranges.iter().map(|r| r.label.clone()))
        {
            self.preflight_new_editor_text(&text, style, false)?;
        }
        Ok(())
    }
    pub fn gauge(
        &mut self,
        parent: WidgetId,
        settings: GaugeSettings,
        mut style: LayoutStyle,
    ) -> Result<WidgetId, ControlError> {
        self.preflight_gauge(&settings, &style)?;
        if style.width == Length::Auto {
            style.width = Length::Px(300.0);
        }
        if style.height == Length::Auto {
            style.height = Length::Px(match settings.view {
                GaugeView::Circular => 260.0,
                GaugeView::Linear => 150.0,
                _ => 100.0,
            });
        }
        style.padding = Edges::all(0.0);
        let id = self.transparent_panel(parent, style)?;
        self.set_role(id, Role::Meter);
        self.controls.get_mut(&id).unwrap().label = match settings.view {
            GaugeView::Circular => "Круговая шкала",
            GaugeView::Linear => "Линейная шкала",
            GaugeView::Digital => "Цифровая шкала",
            GaugeView::State => "Индикатор состояния",
        }
        .into();
        self.gauges.insert(
            id,
            GaugeState {
                value_text: settings.value_text(),
                semantic_value: settings.semantic_value(),
                settings,
            },
        );
        Ok(id)
    }
    pub fn gauge_settings(&self, id: WidgetId) -> Option<&GaugeSettings> {
        self.gauges.get(&id).map(|s| &s.settings)
    }
    pub fn gauge_value(&self, id: WidgetId) -> Option<f64> {
        self.gauge_settings(id).map(|s| s.value)
    }
    pub fn gauge_semantic_value(&self, id: WidgetId) -> Option<&str> {
        self.gauges.get(&id).map(|s| s.semantic_value.as_str())
    }
    pub fn set_gauge(&mut self, id: WidgetId, settings: GaugeSettings) -> Result<(), ControlError> {
        if !self.gauges.contains_key(&id) {
            return Err(ControlError::Invalid("Not a Gauge".into()));
        }
        let style = self
            .runtime
            .tree()
            .node(id)
            .ok_or_else(|| ControlError::Invalid("Removed Gauge".into()))?
            .props
            .style
            .clone();
        self.preflight_gauge(&settings, &style)?;
        self.gauges.insert(
            id,
            GaugeState {
                value_text: settings.value_text(),
                semantic_value: settings.semantic_value(),
                settings,
            },
        );
        self.scene_dirty = true;
        Ok(())
    }
    pub fn set_gauge_value(&mut self, id: WidgetId, value: f64) -> Result<(), ControlError> {
        let mut settings = self
            .gauge_settings(id)
            .ok_or_else(|| ControlError::Invalid("Not a Gauge".into()))?
            .clone();
        settings.value = value;
        self.set_gauge(id, settings)
    }
    pub fn gauge_display_status(&self, id: WidgetId) -> Option<GaugeDisplayStatus> {
        Some(gauge_paint::status(
            &self.gauges.get(&id)?.settings,
            self.runtime.bounds(id)?,
            self.runtime.clip(id)?,
        ))
    }
    pub(super) fn retain_gauges(&mut self) {
        self.gauges
            .retain(|id, _| self.runtime.tree().node(*id).is_some());
    }
}
