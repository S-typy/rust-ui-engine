use super::*;
use rust_desktop_ui_core::StrokeSegment;
pub(super) fn plot(model: &ChartModel, b: Rect) -> Option<Rect> {
    if model.settings().view == ChartView::Sparkline {
        return (b.width >= 32.0 && b.height >= 24.0)
            .then(|| Rect::new(b.x + 8.0, b.y + 8.0, b.width - 16.0, b.height - 16.0));
    }
    let rows = if model.settings().legend {
        model.series().len().div_ceil(4)
    } else {
        0
    };
    let top = 36.0 + rows as f32 * 26.0;
    (b.width >= 520.0 && b.height >= 240.0 + rows as f32 * 26.0).then(|| {
        Rect::new(
            b.x + 82.0,
            b.y + top,
            b.width - 106.0,
            b.height - top - 76.0,
        )
    })
}
fn color(theme: Theme, index: usize) -> Color {
    if theme.is_high_contrast() {
        return theme.tokens().text;
    }
    let light = [
        Color::rgb(0, 95, 184),
        Color::rgb(182, 74, 0),
        Color::rgb(0, 125, 91),
        Color::rgb(129, 58, 179),
        Color::rgb(180, 35, 90),
        Color::rgb(55, 95, 115),
    ];
    let dark = [
        Color::rgb(92, 183, 255),
        Color::rgb(255, 167, 92),
        Color::rgb(88, 212, 168),
        Color::rgb(199, 152, 255),
        Color::rgb(255, 133, 176),
        Color::rgb(156, 202, 218),
    ];
    if theme.is_dark() {
        dark[index % dark.len()]
    } else {
        light[index % light.len()]
    }
}
#[allow(clippy::too_many_arguments)]
fn text(
    scene: &mut Scene,
    engine: &mut TextEngine,
    value: &str,
    bounds: Rect,
    clip: Rect,
    color: Color,
    size: f32,
    align: TextAlign,
) -> Result<(), ControlError> {
    if bounds.width <= 0.0 || bounds.height < 14.0 {
        return Ok(());
    }
    let mut run = TextRun::new(value, bounds, color, size);
    run.align = align;
    run.clip = clip;
    let mut fitted = engine
        .fit_text(
            &run,
            &[],
            rust_desktop_ui_core::TextTrimming::CharacterEllipsis,
            Some(1),
        )
        .map_err(|e| ControlError::Text(e.to_string()))?;
    let height = engine
        .prepare(&fitted.run, 1.0)
        .map_err(|e| ControlError::Text(e.to_string()))?
        .height;
    fitted.run.bounds.y += (bounds.height - height).max(0.0) * 0.5;
    scene.text(fitted.run);
    Ok(())
}
impl Controls {
    pub(super) fn paint_chart(
        &mut self,
        id: WidgetId,
        bounds: Rect,
        clip: Rect,
    ) -> Result<bool, ControlError> {
        let Some(entry) = self.charts.get(&id) else {
            return Ok(false);
        };
        let model = &entry.model;
        let tokens = self.theme.tokens();
        self.scene.rounded_fill(bounds, clip, 6.0, tokens.surface);
        if self.runtime.focused() == Some(id) {
            self.scene
                .rounded_stroke(bounds, clip, 6.0, 2.0, tokens.focus);
        }
        let Some(plot) = plot(model, bounds) else {
            text(
                &mut self.scene,
                &mut self.engine,
                &format!("Компактная диаграмма. {}", entry.summary),
                Rect::new(
                    bounds.x + 8.0,
                    bounds.y + 4.0,
                    (bounds.width - 16.0).max(0.0),
                    (bounds.height - 8.0).max(0.0),
                ),
                clip,
                tokens.text,
                13.0,
                TextAlign::Start,
            )?;
            return Ok(true);
        };
        let geometry = model
            .geometry(plot)
            .map_err(|e| ControlError::Invalid(e.to_string()))?;
        let plot_clip = clip
            .intersection(plot)
            .unwrap_or(Rect::new(plot.x, plot.y, 0.0, 0.0));
        let spark = model.settings().view == ChartView::Sparkline;
        if !spark {
            for value in &geometry.y_ticks {
                let fraction = (*value - model.y_range().minimum)
                    / (model.y_range().maximum - model.y_range().minimum);
                let y = plot.y + (1.0 - fraction) as f32 * plot.height;
                self.scene.stroke_path(
                    &[StrokeSegment {
                        start: Point::new(plot.x, y),
                        end: Point::new(plot.x + plot.width, y),
                    }],
                    clip,
                    1.0,
                    tokens.border,
                );
                text(
                    &mut self.scene,
                    &mut self.engine,
                    &ChartModel::format_number(*value),
                    Rect::new(bounds.x + 2.0, y - 10.0, 72.0, 20.0),
                    clip,
                    tokens.muted,
                    12.0,
                    TextAlign::End,
                )?;
            }
            for value in &geometry.x_ticks {
                let fraction = (*value - model.x_range().minimum)
                    / (model.x_range().maximum - model.x_range().minimum);
                let x = plot.x + fraction as f32 * plot.width;
                self.scene.stroke_path(
                    &[StrokeSegment {
                        start: Point::new(x, plot.y + plot.height),
                        end: Point::new(x, plot.y + plot.height + 4.0),
                    }],
                    clip,
                    1.0,
                    tokens.border,
                );
                text(
                    &mut self.scene,
                    &mut self.engine,
                    &ChartModel::format_number(*value),
                    Rect::new(
                        (x - 36.0).clamp(bounds.x + 4.0, bounds.x + bounds.width - 76.0),
                        plot.y + plot.height + 6.0,
                        72.0,
                        20.0,
                    ),
                    clip,
                    tokens.muted,
                    12.0,
                    TextAlign::Center,
                )?;
            }
            text(
                &mut self.scene,
                &mut self.engine,
                &model.settings().y_title,
                Rect::new(plot.x, bounds.y + 4.0, plot.width, 24.0),
                clip,
                tokens.text,
                14.0,
                TextAlign::Start,
            )?;
            text(
                &mut self.scene,
                &mut self.engine,
                &model.settings().x_title,
                Rect::new(plot.x, plot.y + plot.height + 28.0, plot.width, 22.0),
                clip,
                tokens.text,
                13.0,
                TextAlign::Center,
            )?;
            if model.settings().legend {
                let width = (bounds.width - 32.0) / 4.0;
                for (i, s) in model.series().iter().enumerate() {
                    let x = bounds.x + 16.0 + (i % 4) as f32 * width;
                    let y = bounds.y + 30.0 + (i / 4) as f32 * 26.0;
                    self.scene.rounded_fill(
                        Rect::new(x, y + 8.0, 12.0, 12.0),
                        clip,
                        3.0,
                        color(self.theme, i),
                    );
                    text(
                        &mut self.scene,
                        &mut self.engine,
                        &s.label,
                        Rect::new(x + 18.0, y, width - 24.0, 26.0),
                        clip,
                        tokens.text,
                        13.0,
                        TextAlign::Start,
                    )?;
                }
            }
            text(
                &mut self.scene,
                &mut self.engine,
                &entry.summary,
                Rect::new(
                    bounds.x + 16.0,
                    bounds.y + bounds.height - 26.0,
                    bounds.width - 32.0,
                    22.0,
                ),
                clip,
                tokens.muted,
                12.0,
                TextAlign::Start,
            )?;
        }
        for (i, s) in model.series().iter().enumerate() {
            let color = color(self.theme, i);
            let lines: Vec<_> = geometry
                .segments
                .iter()
                .filter(|v| v.series == s.id)
                .map(|v| StrokeSegment {
                    start: v.start,
                    end: v.end,
                })
                .collect();
            for chunk in lines.chunks(rust_desktop_ui_core::MAX_STROKE_SEGMENTS) {
                self.scene.stroke_path(chunk, plot_clip, 2.0, color)
            }
            for bar in geometry.bars.iter().filter(|v| v.key.series == s.id) {
                self.scene.rounded_fill(bar.bounds, plot_clip, 1.0, color)
            }
            for p in geometry.points.iter().filter(|v| v.key.series == s.id) {
                let selected = model.selected() == Some(p.key);
                if selected {
                    self.scene.rounded_fill(
                        Rect::new(p.position.x - 7.0, p.position.y - 7.0, 14.0, 14.0),
                        clip,
                        7.0,
                        tokens.text,
                    );
                    self.scene.rounded_fill(
                        Rect::new(p.position.x - 5.0, p.position.y - 5.0, 10.0, 10.0),
                        clip,
                        5.0,
                        tokens.surface,
                    );
                }
                if model.settings().view != ChartView::Bar || selected {
                    self.scene.rounded_fill(
                        Rect::new(p.position.x - 3.0, p.position.y - 3.0, 6.0, 6.0),
                        clip,
                        3.0,
                        color,
                    );
                }
            }
        }
        Ok(true)
    }
}
