//! Bounded inline display text and grapheme-safe trimming.
use super::*;
use rust_desktop_ui_core::TextSpan;
pub use rust_desktop_ui_core::TextTrimming;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TextDisplaySettings {
    pub trimming: TextTrimming,
    pub max_lines: Option<usize>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct InlineSpan {
    pub text: String,
    pub font_size: Option<f32>,
    pub font_weight: Option<u16>,
    pub foreground: Option<Color>,
}

/// Validates a complete inline collection without mounting or changing a view.
pub fn validate_text_inlines(inlines: &[InlineSpan]) -> Result<(), ControlError> {
    if inlines.len() > 256 {
        return Err(ControlError::Invalid(
            "At most 256 inline runs are supported".into(),
        ));
    }
    let mut text = String::new();
    let mut ends = Vec::new();
    for inline in inlines {
        TextStyle {
            font_size: inline.font_size,
            font_weight: inline.font_weight,
            foreground: inline.foreground,
            ..Default::default()
        }
        .validate()?;
        if text.len().saturating_add(inline.text.len()) > 1_048_576 {
            return Err(ControlError::Invalid("Inline text exceeds one MiB".into()));
        }
        text.push_str(&inline.text);
        ends.push(text.len());
    }
    TextLimits::default().validate(&text).map_err(edit_error)?;
    let boundaries = TextDocument::new(text).grapheme_boundaries();
    if ends
        .iter()
        .any(|end| boundaries.binary_search(end).is_err())
    {
        return Err(ControlError::Invalid(
            "An inline style boundary cannot split a Unicode grapheme".into(),
        ));
    }
    Ok(())
}

#[derive(Clone, Default)]
struct DisplayText {
    settings: TextDisplaySettings,
    spans: Vec<TextSpan>,
}

#[derive(Default)]
pub(super) struct TextDisplays {
    items: HashMap<WidgetId, DisplayText>,
}

impl Controls {
    pub fn set_text_display(
        &mut self,
        id: WidgetId,
        settings: TextDisplaySettings,
    ) -> Result<(), ControlError> {
        if self.text_style(id).is_none() {
            return Err(ControlError::Invalid(
                "Text display requires a live TextBlock".into(),
            ));
        }
        if settings
            .max_lines
            .is_some_and(|count| !(1..=256).contains(&count))
        {
            return Err(ControlError::Invalid("MaxLines must be in 1..=256".into()));
        }
        self.text_displays.items.entry(id).or_default().settings = settings;
        self.scene_dirty = true;
        Ok(())
    }

    pub fn text_display(&self, id: WidgetId) -> Option<TextDisplaySettings> {
        self.text_style(id)?;
        Some(
            self.text_displays
                .items
                .get(&id)
                .map_or(TextDisplaySettings::default(), |value| value.settings),
        )
    }

    /// Inline ranges are shaped together. Empty runs have no range; a style
    /// boundary inside one Unicode grapheme is rejected atomically.
    pub fn set_text_inlines(
        &mut self,
        id: WidgetId,
        inlines: Vec<InlineSpan>,
    ) -> Result<(), ControlError> {
        validate_text_inlines(&inlines)?;
        let style = self
            .text_style(id)
            .ok_or_else(|| ControlError::Invalid("Inlines require a live TextBlock".into()))?;
        if inlines.len() > 256 {
            return Err(ControlError::Invalid(
                "At most 256 inline runs are supported".into(),
            ));
        }
        let mut text = String::new();
        let mut spans = Vec::new();
        for inline in inlines {
            TextStyle {
                font_size: inline.font_size,
                font_weight: inline.font_weight,
                foreground: inline.foreground,
                ..Default::default()
            }
            .validate()?;
            if text.len().saturating_add(inline.text.len()) > 1_048_576 {
                return Err(ControlError::Invalid("Inline text exceeds one MiB".into()));
            }
            let start = text.len();
            text.push_str(&inline.text);
            if start != text.len() {
                spans.push(TextSpan {
                    range: start..text.len(),
                    font_size: inline.font_size,
                    weight: inline.font_weight,
                    color: inline.foreground,
                });
            }
        }
        let run = self.display_run(
            text.clone(),
            style,
            Rect::new(0.0, 0.0, 16_384.0, 0.0),
            Rect::new(0.0, 0.0, 0.0, 0.0),
            true,
        );
        self.engine
            .prepare_styled(&run, &spans, 1.0)
            .map_err(|error| ControlError::Text(error.to_string()))?;
        self.controls.get_mut(&id).unwrap().label = text;
        self.text_displays.items.entry(id).or_default().spans = spans;
        self.scene_dirty = true;
        Ok(())
    }

    /// Measures the same fitted paragraph that is submitted to the renderer.
    /// Width excludes the ordinary TextBlock left/right content insets.
    pub fn measure_text_display(&mut self, id: WidgetId, width: f32) -> Result<Size, ControlError> {
        if !width.is_finite() || !(0.0..=16_384.0).contains(&width) {
            return Err(ControlError::Invalid(
                "Text width must be in [0,16384]".into(),
            ));
        }
        let style = self
            .text_style(id)
            .ok_or_else(|| ControlError::Invalid("Unknown TextBlock".into()))?;
        let display = self
            .text_displays
            .items
            .get(&id)
            .cloned()
            .unwrap_or_default();
        let run = self.display_run(
            self.controls[&id].label.clone(),
            style,
            Rect::new(0.0, 0.0, width, 16_384.0),
            Rect::new(0.0, 0.0, 0.0, 0.0),
            true,
        );
        let fitted = self
            .engine
            .fit_text(
                &run,
                &display.spans,
                display.settings.trimming,
                display.settings.max_lines,
            )
            .map_err(|error| ControlError::Text(error.to_string()))?;
        Ok(Size::new(fitted.measured.width, fitted.measured.height))
    }

    fn display_run(
        &self,
        text: String,
        style: TextStyle,
        bounds: Rect,
        clip: Rect,
        enabled: bool,
    ) -> TextRun {
        TextRun {
            text,
            bounds,
            clip,
            font_size: style.font_size.unwrap_or(self.theme.tokens().font_size),
            color: style.color(self.theme, enabled),
            weight: style.font_weight.unwrap_or(400),
            align: TextAlign::Start,
            wrap: style.wrap,
        }
    }

    pub(super) fn clear_text_inlines(&mut self, id: WidgetId) {
        if let Some(display) = self.text_displays.items.get_mut(&id) {
            display.spans.clear();
        }
    }

    pub(super) fn retain_text_displays(&mut self) {
        self.text_displays
            .items
            .retain(|id, _| self.runtime.tree().node(*id).is_some());
    }

    pub(super) fn paint_text_display(
        &mut self,
        id: WidgetId,
        bounds: Rect,
        clip: Rect,
    ) -> Result<bool, ControlError> {
        let Some(mut display) = self.text_displays.items.get(&id).cloned() else {
            return Ok(false);
        };
        let bounds = Rect::new(
            bounds.x + 8.0_f32.min(bounds.width),
            bounds.y,
            (bounds.width - 16.0).max(0.0),
            bounds.height,
        );
        let Some(clip) = bounds.intersection(clip) else {
            return Ok(true);
        };
        let style = self.controls[&id].text_style;
        let enabled = self.is_enabled(id);
        if !enabled {
            for span in &mut display.spans {
                span.color = None;
            }
        }
        let run = self.display_run(
            self.controls[&id].label.clone(),
            style,
            bounds,
            clip,
            enabled,
        );
        let fitted = self
            .engine
            .fit_text(
                &run,
                &display.spans,
                display.settings.trimming,
                display.settings.max_lines,
            )
            .map_err(|error| ControlError::Text(error.to_string()))?;
        self.scene
            .styled_text(fitted.run, fitted.spans)
            .map_err(|error| ControlError::Text(error.into()))?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn inline_label_measure_matches_paint_and_preserves_accessible_name() {
        let mut ui = Controls::new(Theme::FluentLight).unwrap();
        let id = ui
            .label(
                ui.root(),
                "",
                LayoutStyle {
                    width: Length::Px(160.0),
                    height: Length::Px(40.0),
                    ..Default::default()
                },
            )
            .unwrap();
        ui.set_text_inlines(
            id,
            vec![InlineSpan {
                text: "A strong title followed by a long caption".into(),
                font_weight: Some(700),
                ..Default::default()
            }],
        )
        .unwrap();
        ui.set_text_display(
            id,
            TextDisplaySettings {
                trimming: TextTrimming::WordEllipsis,
                max_lines: Some(1),
            },
        )
        .unwrap();
        ui.set_text_auto_height(id, true).unwrap();
        ui.update(Size::new(200.0, 100.0)).unwrap();
        assert!(ui.scene().texts.iter().any(|run| run.text.ends_with('…')));
        assert_eq!(
            ui.semantics()
                .iter()
                .find(|node| node.id == id)
                .unwrap()
                .label,
            "A strong title followed by a long caption"
        );
        let measured = ui.measure_text_display(id, 144.0).unwrap();
        assert_eq!(
            ui.runtime.bounds(id).unwrap().height,
            measured.height.ceil()
        );
        ui.set_label(id, "A replacement").unwrap();
        assert!(ui.text_displays.items[&id].spans.is_empty());
    }
}
