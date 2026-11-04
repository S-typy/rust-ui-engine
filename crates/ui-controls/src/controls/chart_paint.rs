use super::*;
use rust_desktop_ui_core::StrokeSegment;
pub(super) fn plot(model: &ChartModel, b: Rect) -> Option<Rect> {
    if model.settings().view == ChartView::Sparkline {
        return (b.width >= 32.0 && b.height >= 24.0)
            .then(|| Rect::new(b.x + 8.0, b.y + 8.0, b.width - 16.0, b.height - 16.0));
    }
    let rows = if model.settings().legend {
        model.legend_labels().len().div_ceil(4)
    } else {
        0
    };
    let top = 36.0 + rows as f32 * 26.0;
    (b.width >= 520.0 && b.height >= 240.0 + rows as f32 * 26.0).then(|| {
        if model.family().is_some() && model.settings().view != ChartView::Heatmap {
            return Rect::new(b.x + 20.0, b.y + top, b.width - 40.0, b.height - top - 38.0);
        }
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
                for (i, label) in model.legend_labels().iter().enumerate() {
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
                        label,
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
        if model.family().is_some() {
            paint_regions(
                &mut self.scene,
                &mut self.engine,
                model,
                &geometry,
                clip,
                self.theme,
            )?;
        }
        // Fill all series before drawing lines/markers, so selection anchors
        // remain visible even where independent areas overlap.
        for (i, series) in model.series().iter().enumerate() {
            let source = color(self.theme, i);
            let weight = if self.theme.is_high_contrast() {
                0.22
            } else {
                0.30
            };
            let brush = Color {
                r: tokens.surface.r + (source.r - tokens.surface.r) * weight,
                g: tokens.surface.g + (source.g - tokens.surface.g) * weight,
                b: tokens.surface.b + (source.b - tokens.surface.b) * weight,
                a: 1.0,
            };
            let mut contours = Vec::new();
            let mut edges = 0;
            for area in geometry
                .areas
                .iter()
                .filter(|a| a.start.series == series.id)
            {
                if edges + area.contour.len() > rust_desktop_ui_core::MAX_FILL_EDGES {
                    self.scene
                        .fill_path(rust_desktop_ui_core::FillPath {
                            contours: std::mem::take(&mut contours),
                            clip: plot_clip,
                            rule: rust_desktop_ui_core::FillRule::NonZero,
                            color: brush,
                        })
                        .map_err(|e| ControlError::Invalid(e.into()))?;
                    edges = 0;
                }
                edges += area.contour.len();
                contours.push(area.contour.clone());
            }
            if !contours.is_empty() {
                self.scene
                    .fill_path(rust_desktop_ui_core::FillPath {
                        contours,
                        clip: plot_clip,
                        rule: rust_desktop_ui_core::FillRule::NonZero,
                        color: brush,
                    })
                    .map_err(|e| ControlError::Invalid(e.into()))?;
            }
        }
        // Complete all stacked spans before drawing any selection marker.
        // Otherwise a later segment can cover the marker on their shared edge.
        for (i, series) in model.series().iter().enumerate() {
            for bar in geometry.bars.iter().filter(|v| v.key.series == series.id) {
                self.scene
                    .rounded_fill(bar.bounds, plot_clip, 1.0, color(self.theme, i));
            }
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
                if !matches!(
                    model.settings().view,
                    ChartView::Bar | ChartView::StackedBar
                ) || selected
                {
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

fn paint_regions(
    scene: &mut Scene,
    engine: &mut TextEngine,
    model: &ChartModel,
    geometry: &ChartGeometry,
    clip: Rect,
    theme: Theme,
) -> Result<(), ControlError> {
    let tokens = theme.tokens();
    let Some(plot_clip) = clip.intersection(geometry.plot) else {
        return Ok(());
    };
    for region in &geometry.regions {
        let mut brush = color(theme, region.color_index);
        let hierarchy = matches!(
            model.settings().view,
            ChartView::Treemap | ChartView::Sunburst
        );
        let flow_link = model.settings().view == ChartView::Sankey && region.key.series == 1;
        if hierarchy || flow_link {
            let t = if flow_link {
                0.27
            } else {
                match region.depth {
                    0 => 0.16,
                    1 => 0.56,
                    depth => (0.86 - (depth - 2) as f32 * 0.06).max(0.56),
                }
            };
            brush = Color {
                r: tokens.surface.r + (brush.r - tokens.surface.r) * t,
                g: tokens.surface.g + (brush.g - tokens.surface.g) * t,
                b: tokens.surface.b + (brush.b - tokens.surface.b) * t,
                a: 1.0,
            };
        }
        if let Some(intensity) = region.intensity {
            // Opaque interpolation avoids seams where adjacent cells meet.
            let t = 0.12 + intensity * 0.88;
            brush = Color {
                r: tokens.surface.r + (brush.r - tokens.surface.r) * t,
                g: tokens.surface.g + (brush.g - tokens.surface.g) * t,
                b: tokens.surface.b + (brush.b - tokens.surface.b) * t,
                a: 1.0,
            };
        }
        match &region.shape {
            ChartRegionShape::Rectangle(bounds) => {
                scene.rounded_fill(*bounds, plot_clip, 0.0, brush);
                if region.intensity.is_some() || hierarchy {
                    scene.rounded_stroke(*bounds, plot_clip, 0.0, 0.5, tokens.surface);
                }
                if bounds.width >= 38.0 && bounds.height >= 22.0 {
                    let label_bounds = Rect::new(
                        bounds.x + 3.0,
                        region.anchor.y - 11.0,
                        bounds.width - 6.0,
                        22.0,
                    );
                    // Select black or white from the actual solid background.
                    let ink = if brush.r * 0.2126 + brush.g * 0.7152 + brush.b * 0.0722 > 0.5 {
                        Color::rgb(0, 0, 0)
                    } else {
                        Color::rgb(255, 255, 255)
                    };
                    text(
                        scene,
                        engine,
                        &region.label,
                        label_bounds,
                        plot_clip,
                        ink,
                        12.0,
                        TextAlign::Center,
                    )?;
                }
            }
            ChartRegionShape::Polygon(points) => {
                scene
                    .fill_path(region.path(points, plot_clip, brush))
                    .map_err(|e| ControlError::Invalid(e.into()))?;
                if matches!(model.settings().view, ChartView::Sunburst | ChartView::Pie) {
                    let edges: Vec<_> = points
                        .iter()
                        .copied()
                        .zip(points.iter().copied().cycle().skip(1))
                        .map(|(start, end)| StrokeSegment { start, end })
                        .collect();
                    for chunk in edges.chunks(rust_desktop_ui_core::MAX_STROKE_SEGMENTS) {
                        scene.stroke_path(chunk, plot_clip, 1.0, tokens.surface);
                    }
                }
            }
        }
    }
    if let Some(region) = geometry
        .regions
        .iter()
        .find(|r| Some(r.key) == model.selected())
    {
        if let ChartRegionShape::Rectangle(bounds) = region.shape {
            scene.rounded_stroke(bounds, plot_clip, 0.0, 2.0, tokens.text);
        } else {
            let p = region.anchor;
            scene.rounded_fill(
                Rect::new(p.x - 7.0, p.y - 7.0, 14.0, 14.0),
                plot_clip,
                7.0,
                tokens.text,
            );
            scene.rounded_fill(
                Rect::new(p.x - 4.0, p.y - 4.0, 8.0, 8.0),
                plot_clip,
                4.0,
                tokens.surface,
            );
        }
    }
    if let Some(ChartFamily::Heatmap {
        columns,
        rows,
        cells,
    }) = model.family()
    {
        let p = geometry.plot;
        if model.settings().legend {
            let lo = cells.iter().filter_map(|c| c.value).reduce(f64::min);
            let hi = cells.iter().filter_map(|c| c.value).reduce(f64::max);
            if let (Some(lo), Some(hi)) = (lo, hi) {
                text(
                    scene,
                    engine,
                    &format!(
                        "Значение: {} … {}",
                        ChartModel::format_number(lo),
                        ChartModel::format_number(hi)
                    ),
                    Rect::new(p.x, p.y - 28.0, p.width, 22.0),
                    clip,
                    tokens.text,
                    13.0,
                    TextAlign::Start,
                )?;
            }
        }
        for (i, row) in rows.iter().enumerate() {
            let height = p.height / rows.len() as f32;
            if height >= 18.0 {
                text(
                    scene,
                    engine,
                    &row.label,
                    Rect::new(p.x - 78.0, p.y + i as f32 * height, 72.0, height),
                    clip,
                    tokens.text,
                    12.0,
                    TextAlign::End,
                )?;
            }
        }
        for (i, column) in columns.iter().enumerate() {
            let width = p.width / columns.len() as f32;
            if width >= 28.0 {
                text(
                    scene,
                    engine,
                    &column.label,
                    Rect::new(p.x + i as f32 * width, p.y + p.height + 4.0, width, 22.0),
                    clip,
                    tokens.text,
                    12.0,
                    TextAlign::Center,
                )?;
            }
        }
    }
    if let Some(ChartFamily::Flow { nodes, .. }) = model.family() {
        let p = geometry.plot;
        for region in geometry.regions.iter().filter(|r| r.key.series == 0) {
            let ChartRegionShape::Rectangle(bounds) = region.shape else {
                continue;
            };
            let label = &nodes
                .iter()
                .find(|n| n.id == region.key.point)
                .unwrap()
                .label;
            let right_side = bounds.x < p.x + p.width * 0.5;
            let width = 110.0f32.min((p.width / 8.0).max(36.0));
            let x = if right_side {
                bounds.x + bounds.width + 4.0
            } else {
                bounds.x - width - 4.0
            };
            if bounds.height >= 18.0 {
                text(
                    scene,
                    engine,
                    label,
                    Rect::new(x, region.anchor.y - 11.0, width, 22.0),
                    plot_clip,
                    tokens.text,
                    12.0,
                    if right_side {
                        TextAlign::Start
                    } else {
                        TextAlign::End
                    },
                )?;
            }
        }
    }
    Ok(())
}
