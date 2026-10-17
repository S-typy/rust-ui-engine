use super::*;
use rust_desktop_ui_core::StrokeSegment;

pub(super) fn status(settings: &GaugeSettings, bounds: Rect, clip: Rect) -> GaugeDisplayStatus {
    let (width, height) = match settings.view {
        GaugeView::Circular => (220.0, 220.0),
        GaugeView::Linear => (240.0, 140.0),
        GaugeView::Digital | GaugeView::State => (160.0, 80.0),
    };
    if bounds.width >= width
        && bounds.height >= height
        && bounds.x >= clip.x - 0.01
        && bounds.y >= clip.y - 0.01
        && bounds.x + bounds.width <= clip.x + clip.width + 0.01
        && bounds.y + bounds.height <= clip.y + clip.height + 0.01
    {
        GaugeDisplayStatus::Full
    } else {
        GaugeDisplayStatus::Compact
    }
}
#[allow(clippy::too_many_arguments)]
fn text(
    scene: &mut Scene,
    engine: &mut TextEngine,
    text: &str,
    bounds: Rect,
    clip: Rect,
    color: Color,
    size: f32,
) -> Result<(), ControlError> {
    if bounds.width <= 0.0 || bounds.height < 12.0 {
        return Ok(());
    }
    let mut run = TextRun::new(text, bounds, color, size.min(bounds.height / 1.35));
    run.clip = clip;
    run.align = TextAlign::Center;
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
fn polar(center: Point, radius: f32, fraction: f64) -> Point {
    let angle = (135.0 + fraction * 270.0).to_radians();
    Point::new(
        center.x + radius * angle.cos() as f32,
        center.y + radius * angle.sin() as f32,
    )
}
#[allow(clippy::too_many_arguments)]
fn arc(
    scene: &mut Scene,
    center: Point,
    radius: f32,
    from: f64,
    to: f64,
    width: f32,
    clip: Rect,
    color: Color,
) {
    let count = ((to - from) * 64.0).ceil().max(1.0) as usize;
    let segments: Vec<_> = (0..count)
        .map(|i| StrokeSegment {
            start: polar(center, radius, from + (to - from) * i as f64 / count as f64),
            end: polar(
                center,
                radius,
                from + (to - from) * (i + 1) as f64 / count as f64,
            ),
        })
        .collect();
    for chunk in segments.chunks(rust_desktop_ui_core::MAX_STROKE_SEGMENTS) {
        scene.stroke_path(chunk, clip, width, color);
    }
}
impl Controls {
    fn gauge_color(&self, tone: GaugeTone) -> Color {
        let dark = self.theme.is_dark();
        if self.theme.is_high_contrast() {
            return self.theme.tokens().text;
        }
        match tone {
            GaugeTone::Neutral => self.theme.tokens().muted,
            GaugeTone::Accent => self.theme.tokens().focus,
            GaugeTone::Good => {
                if dark {
                    Color::rgb(100, 210, 151)
                } else {
                    Color::rgb(13, 125, 72)
                }
            }
            GaugeTone::Warning => {
                if dark {
                    Color::rgb(255, 201, 92)
                } else {
                    Color::rgb(153, 96, 0)
                }
            }
            GaugeTone::Danger => {
                if dark {
                    Color::rgb(255, 128, 139)
                } else {
                    Color::rgb(190, 36, 58)
                }
            }
        }
    }
    pub(super) fn paint_gauge(
        &mut self,
        id: WidgetId,
        bounds: Rect,
        clip: Rect,
    ) -> Result<bool, ControlError> {
        let Some(state) = self.gauges.get(&id) else {
            return Ok(false);
        };
        let Some(clip) = clip.intersection(bounds) else {
            return Ok(true);
        };
        let s = &state.settings;
        let fg = self.theme.tokens().text;
        let muted = self.theme.tokens().muted;
        let accent = self.theme.tokens().focus;
        let tone = self.gauge_color(s.active_range().map_or(GaugeTone::Neutral, |r| r.tone));
        if status(s, bounds, clip) == GaugeDisplayStatus::Compact {
            text(
                &mut self.scene,
                &mut self.engine,
                &state.semantic_value,
                clip,
                clip,
                fg,
                13.0,
            )?;
            return Ok(true);
        }
        match s.view {
            GaugeView::Digital => {
                text(
                    &mut self.scene,
                    &mut self.engine,
                    &state.value_text,
                    Rect::new(
                        bounds.x + 8.0,
                        bounds.y + 4.0,
                        bounds.width - 16.0,
                        bounds.height - 36.0,
                    ),
                    clip,
                    fg,
                    30.0,
                )?;
                text(
                    &mut self.scene,
                    &mut self.engine,
                    s.state_label(),
                    Rect::new(
                        bounds.x + 8.0,
                        bounds.y + bounds.height - 30.0,
                        bounds.width - 16.0,
                        24.0,
                    ),
                    clip,
                    tone,
                    14.0,
                )?;
            }
            GaugeView::State => {
                self.scene.rounded_fill(
                    Rect::new(
                        bounds.x + 16.0,
                        bounds.y + bounds.height * 0.5 - 12.0,
                        24.0,
                        24.0,
                    ),
                    clip,
                    12.0,
                    tone,
                );
                text(
                    &mut self.scene,
                    &mut self.engine,
                    &state.value_text,
                    Rect::new(
                        bounds.x + 52.0,
                        bounds.y + 8.0,
                        bounds.width - 64.0,
                        bounds.height * 0.5 - 8.0,
                    ),
                    clip,
                    fg,
                    20.0,
                )?;
                text(
                    &mut self.scene,
                    &mut self.engine,
                    s.state_label(),
                    Rect::new(
                        bounds.x + 52.0,
                        bounds.y + bounds.height * 0.5,
                        bounds.width - 64.0,
                        bounds.height * 0.5 - 8.0,
                    ),
                    clip,
                    fg,
                    14.0,
                )?;
            }
            GaugeView::Linear => {
                let x = bounds.x + 34.0;
                let y = bounds.y + 48.0;
                let width = bounds.width - 68.0;
                self.scene
                    .rounded_fill(Rect::new(x, y, width, 12.0), clip, 6.0, muted);
                for r in &s.ranges {
                    let color = self.gauge_color(r.tone);
                    let left = x + width * s.fraction(r.start) as f32;
                    let right = x + width * s.fraction(r.end) as f32;
                    self.scene.fill(
                        Rect::new(left, y, right - left, 12.0)
                            .intersection(clip)
                            .unwrap_or(Rect::new(0.0, 0.0, 0.0, 0.0)),
                        color,
                    );
                }
                let marker = x + width * s.fraction(s.value) as f32;
                self.scene.stroke_path(
                    &[StrokeSegment {
                        start: Point::new(marker, y - 7.0),
                        end: Point::new(marker, y + 18.0),
                    }],
                    clip,
                    3.0,
                    fg,
                );
                let major_count = s.ticks().iter().filter(|t| t.1).count();
                let stride = major_count.div_ceil(((width / 72.0) as usize).max(2));
                let mut major = 0;
                for (value, is_major) in s.ticks() {
                    let tx = x + width * s.fraction(value) as f32;
                    self.scene.stroke_path(
                        &[StrokeSegment {
                            start: Point::new(tx, y + 17.0),
                            end: Point::new(tx, y + if is_major { 24.0 } else { 21.0 }),
                        }],
                        clip,
                        1.0,
                        muted,
                    );
                    if is_major {
                        if value == s.maximum
                            || major % stride == 0 && (major == 0 || tx <= x + width - 72.0)
                        {
                            text(
                                &mut self.scene,
                                &mut self.engine,
                                &s.format_number(value),
                                Rect::new(tx - 34.0, y + 28.0, 68.0, 20.0),
                                clip,
                                muted,
                                11.0,
                            )?;
                        }
                        major += 1;
                    }
                }
                text(
                    &mut self.scene,
                    &mut self.engine,
                    &state.value_text,
                    Rect::new(bounds.x + 8.0, bounds.y + 6.0, bounds.width - 16.0, 30.0),
                    clip,
                    fg,
                    20.0,
                )?;
                text(
                    &mut self.scene,
                    &mut self.engine,
                    s.state_label(),
                    Rect::new(
                        bounds.x + 8.0,
                        bounds.y + bounds.height - 30.0,
                        bounds.width - 16.0,
                        24.0,
                    ),
                    clip,
                    tone,
                    13.0,
                )?;
            }
            GaugeView::Circular => {
                let radius = ((bounds.width - 116.0) * 0.5).min((bounds.height - 110.0) * 0.5);
                let center = Point::new(bounds.x + bounds.width * 0.5, bounds.y + radius + 32.0);
                arc(&mut self.scene, center, radius, 0.0, 1.0, 7.0, clip, muted);
                for r in &s.ranges {
                    let color = self.gauge_color(r.tone);
                    arc(
                        &mut self.scene,
                        center,
                        radius,
                        s.fraction(r.start),
                        s.fraction(r.end),
                        7.0,
                        clip,
                        color,
                    );
                }
                let major_count = s.ticks().iter().filter(|t| t.1).count();
                let stride = major_count.div_ceil(6);
                let mut major = 0;
                for (value, is_major) in s.ticks() {
                    let f = s.fraction(value);
                    self.scene.stroke_path(
                        &[StrokeSegment {
                            start: polar(center, radius - 6.0, f),
                            end: polar(center, radius - if is_major { 15.0 } else { 10.0 }, f),
                        }],
                        clip,
                        1.0,
                        fg,
                    );
                    if is_major {
                        if major % stride == 0 || value == s.maximum {
                            let p = polar(center, radius + 23.0, f);
                            text(
                                &mut self.scene,
                                &mut self.engine,
                                &s.format_number(value),
                                Rect::new(p.x - 32.0, p.y - 9.0, 64.0, 18.0),
                                clip,
                                muted,
                                11.0,
                            )?;
                        }
                        major += 1;
                    }
                }
                self.scene.stroke_path(
                    &[StrokeSegment {
                        start: center,
                        end: polar(center, radius - 20.0, s.fraction(s.value)),
                    }],
                    clip,
                    3.0,
                    accent,
                );
                self.scene.rounded_fill(
                    Rect::new(center.x - 5.0, center.y - 5.0, 10.0, 10.0),
                    clip,
                    5.0,
                    accent,
                );
                text(
                    &mut self.scene,
                    &mut self.engine,
                    &state.value_text,
                    Rect::new(
                        bounds.x + 8.0,
                        bounds.y + bounds.height - 55.0,
                        bounds.width - 16.0,
                        28.0,
                    ),
                    clip,
                    fg,
                    20.0,
                )?;
                text(
                    &mut self.scene,
                    &mut self.engine,
                    s.state_label(),
                    Rect::new(
                        bounds.x + 8.0,
                        bounds.y + bounds.height - 27.0,
                        bounds.width - 16.0,
                        22.0,
                    ),
                    clip,
                    tone,
                    13.0,
                )?;
            }
        }
        Ok(true)
    }
}
