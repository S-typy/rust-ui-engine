//! Finite, bounded Cartesian data and the shared drawing/hit-test geometry.
use rust_desktop_ui_core::{Point, Rect};
use rust_desktop_ui_data::{CellValue, DataTable, FieldType};
use std::collections::HashSet;

pub const MAX_CHART_SERIES: usize = 16;
pub const MAX_CHART_POINTS: usize = 4096;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ChartView {
    #[default]
    Line,
    Scatter,
    Bar,
    /// The same line geometry, without axes or a legend in the presenter.
    Sparkline,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChartRange {
    pub minimum: f64,
    pub maximum: f64,
}
impl ChartRange {
    fn validate(self) -> Result<(), ChartError> {
        if !self.minimum.is_finite()
            || !self.maximum.is_finite()
            || self.minimum >= self.maximum
            || !(self.maximum - self.minimum).is_finite()
        {
            return Err(ChartError::InvalidRange);
        }
        Ok(())
    }
    fn fraction(self, value: f64) -> Result<f64, ChartError> {
        let value = (value - self.minimum) / (self.maximum - self.minimum);
        if !value.is_finite() || value.abs() > 1e12 {
            return Err(ChartError::CoordinateOverflow);
        }
        Ok(value)
    }
    pub fn ticks(self) -> Result<Vec<f64>, ChartError> {
        self.validate()?;
        let mut values = Vec::new();
        for index in 0..=4 {
            let value = if index == 4 {
                self.maximum
            } else {
                self.minimum + (self.maximum - self.minimum) * (index as f64 / 4.0)
            };
            if values.last() != Some(&value) {
                values.push(value);
            }
        }
        Ok(values)
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct ChartSettings {
    pub view: ChartView,
    pub x_range: Option<ChartRange>,
    pub y_range: Option<ChartRange>,
    pub legend: bool,
    pub x_title: String,
    pub y_title: String,
}
impl Default for ChartSettings {
    fn default() -> Self {
        Self {
            view: ChartView::Line,
            x_range: None,
            y_range: None,
            legend: true,
            x_title: String::new(),
            y_title: String::new(),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChartPoint {
    pub id: i64,
    pub x: f64,
    pub y: Option<f64>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct ChartSeries {
    pub id: u64,
    pub label: String,
    /// Input order is the line traversal order. NULL breaks a line.
    pub points: Vec<ChartPoint>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ChartPointKey {
    pub series: u64,
    pub point: i64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChartError {
    Capacity,
    InvalidLabel,
    DuplicateIdentity,
    NonFinite,
    InvalidRange,
    CoordinateOverflow,
    InvalidViewport,
    UnknownPoint,
    UnsupportedField,
    InexactInteger,
}
impl std::fmt::Display for ChartError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Chart: {self:?}")
    }
}
impl std::error::Error for ChartError {}

fn label(value: &str) -> Result<(), ChartError> {
    if value.len() > 256 || value.chars().any(char::is_control) {
        return Err(ChartError::InvalidLabel);
    }
    Ok(())
}
impl ChartSeries {
    /// Reads an already accepted snapshot; performs no provider I/O or conversion
    /// through display strings. Integers outside ±2^53 are rejected.
    pub fn from_table(
        table: &DataTable,
        id: u64,
        title: impl Into<String>,
        x_field: &str,
        y_field: &str,
    ) -> Result<Self, ChartError> {
        let title = title.into();
        label(&title)?;
        if table.rows().len() > MAX_CHART_POINTS {
            return Err(ChartError::Capacity);
        }
        if x_field.len() > 256 || y_field.len() > 256 {
            return Err(ChartError::UnsupportedField);
        }
        let field = |name: &str| {
            let index = table
                .schema()
                .field_index(name)
                .ok_or(ChartError::UnsupportedField)?;
            if !matches!(
                table.schema().fields()[index].kind,
                FieldType::Integer | FieldType::Real
            ) {
                return Err(ChartError::UnsupportedField);
            }
            Ok(index)
        };
        let x = field(x_field)?;
        let y = field(y_field)?;
        let number = |v: &CellValue| match v {
            CellValue::Integer(v)
                if (-9_007_199_254_740_992..=9_007_199_254_740_992).contains(v) =>
            {
                Ok(*v as f64)
            }
            CellValue::Integer(_) => Err(ChartError::InexactInteger),
            CellValue::Real(v) if v.is_finite() => Ok(*v),
            _ => Err(ChartError::UnsupportedField),
        };
        let points = table
            .rows()
            .iter()
            .map(|row| {
                Ok(ChartPoint {
                    id: row.key,
                    x: number(&row.values[x])?,
                    y: if row.values[y] == CellValue::Null {
                        None
                    } else {
                        Some(number(&row.values[y])?)
                    },
                })
            })
            .collect::<Result<_, ChartError>>()?;
        Ok(Self {
            id,
            label: title,
            points,
        })
    }
}

#[derive(Clone, Debug)]
pub struct ChartModel {
    series: Vec<ChartSeries>,
    settings: ChartSettings,
    x_range: ChartRange,
    y_range: ChartRange,
    selected: Option<ChartPointKey>,
}
fn automatic(values: impl Iterator<Item = f64>, zero: bool) -> Result<ChartRange, ChartError> {
    let mut minimum = f64::INFINITY;
    let mut maximum = f64::NEG_INFINITY;
    for v in values {
        minimum = minimum.min(v);
        maximum = maximum.max(v);
    }
    if minimum == f64::INFINITY {
        return Ok(ChartRange {
            minimum: 0.0,
            maximum: 1.0,
        });
    }
    if zero {
        minimum = minimum.min(0.0);
        maximum = maximum.max(0.0);
    }
    if minimum == maximum {
        let padding = minimum.abs().max(1.0) * 0.05;
        minimum -= padding;
        maximum += padding;
    }
    let range = ChartRange { minimum, maximum };
    range.validate()?;
    Ok(range)
}
impl ChartModel {
    pub fn new(series: Vec<ChartSeries>, settings: ChartSettings) -> Result<Self, ChartError> {
        if series.len() > MAX_CHART_SERIES {
            return Err(ChartError::Capacity);
        }
        label(&settings.x_title)?;
        label(&settings.y_title)?;
        let mut identities = HashSet::new();
        let mut count = 0usize;
        for s in &series {
            label(&s.label)?;
            if !identities.insert(s.id) {
                return Err(ChartError::DuplicateIdentity);
            }
            count = count
                .checked_add(s.points.len())
                .ok_or(ChartError::Capacity)?;
            if count > MAX_CHART_POINTS {
                return Err(ChartError::Capacity);
            }
            let mut keys = HashSet::new();
            for p in &s.points {
                if !keys.insert(p.id) {
                    return Err(ChartError::DuplicateIdentity);
                }
                if !p.x.is_finite() || p.y.is_some_and(|v| !v.is_finite()) {
                    return Err(ChartError::NonFinite);
                }
            }
        }
        let mut x_range = match settings.x_range {
            Some(r) => r,
            None => automatic(
                series.iter().flat_map(|s| s.points.iter().map(|p| p.x)),
                false,
            )?,
        };
        let y_range = match settings.y_range {
            Some(r) => r,
            None => automatic(
                series
                    .iter()
                    .flat_map(|s| s.points.iter().filter_map(|p| p.y)),
                settings.view == ChartView::Bar,
            )?,
        };
        if settings.view == ChartView::Bar && settings.x_range.is_none() {
            let mut xs: Vec<f64> = series
                .iter()
                .flat_map(|s| s.points.iter().map(|p| p.x))
                .collect();
            xs.sort_by(f64::total_cmp);
            xs.dedup();
            if let Some(gap) = xs.windows(2).map(|w| w[1] - w[0]).reduce(f64::min) {
                // A half category interval on either side retains complete endpoint
                // bars. The geometry uses at most 80% of this category interval.
                let pad = gap * 0.5;
                x_range.minimum = (x_range.minimum - pad).next_down();
                x_range.maximum = (x_range.maximum + pad).next_up();
            }
        }
        x_range.validate()?;
        y_range.validate()?;
        // The complete normalized candidate is checked before publication, including
        // points outside explicit ranges. Extremely ill-conditioned scales refuse.
        for p in series.iter().flat_map(|s| &s.points) {
            x_range.fraction(p.x)?;
            if let Some(y) = p.y {
                y_range.fraction(y)?;
            }
        }
        Ok(Self {
            series,
            settings,
            x_range,
            y_range,
            selected: None,
        })
    }
    pub fn replace(
        &mut self,
        series: Vec<ChartSeries>,
        settings: ChartSettings,
    ) -> Result<(), ChartError> {
        let mut candidate = Self::new(series, settings)?;
        candidate.selected = self
            .selected
            .filter(|key| candidate.point(*key).is_some_and(|p| p.y.is_some()));
        *self = candidate;
        Ok(())
    }
    pub fn series(&self) -> &[ChartSeries] {
        &self.series
    }
    pub fn settings(&self) -> &ChartSettings {
        &self.settings
    }
    pub fn x_range(&self) -> ChartRange {
        self.x_range
    }
    pub fn y_range(&self) -> ChartRange {
        self.y_range
    }
    pub fn selected(&self) -> Option<ChartPointKey> {
        self.selected
    }
    pub fn point(&self, key: ChartPointKey) -> Option<&ChartPoint> {
        self.series
            .iter()
            .find(|s| s.id == key.series)?
            .points
            .iter()
            .find(|p| p.id == key.point)
    }
    pub fn select(&mut self, key: Option<ChartPointKey>) -> Result<(), ChartError> {
        if key.is_some_and(|key| self.point(key).is_none_or(|p| p.y.is_none())) {
            return Err(ChartError::UnknownPoint);
        }
        self.selected = key;
        Ok(())
    }
    pub fn format_number(value: f64) -> String {
        let fixed = format!("{value:.3}");
        if fixed.len() > 12 || value != 0.0 && fixed.parse::<f64>().ok() == Some(0.0) {
            return format!("{value:.3e}");
        }
        let result = fixed.trim_end_matches('0').trim_end_matches('.');
        if result == "-0" {
            "0".into()
        } else {
            result.into()
        }
    }
    /// Geometry is clipped to `plot`. Positions are converted to f32 only after
    /// f64 range reduction; drawing and hit testing share these exact positions.
    pub fn geometry(&self, plot: Rect) -> Result<ChartGeometry, ChartError> {
        if [plot.x, plot.y, plot.width, plot.height]
            .iter()
            .any(|v| !v.is_finite() || v.abs() > 1e6)
            || plot.width < 1.0
            || plot.height < 1.0
        {
            return Err(ChartError::InvalidViewport);
        }
        let position = |x: f64, y: f64| {
            Point::new(
                (f64::from(plot.x) + x.clamp(0.0, 1.0) * f64::from(plot.width)) as f32,
                (f64::from(plot.y) + (1.0 - y.clamp(0.0, 1.0)) * f64::from(plot.height)) as f32,
            )
        };
        let mut result = ChartGeometry {
            plot,
            x_ticks: self.x_range.ticks()?,
            y_ticks: self.y_range.ticks()?,
            points: Vec::new(),
            segments: Vec::new(),
            bars: Vec::new(),
        };
        let mut xs: Vec<f64> = self
            .series
            .iter()
            .flat_map(|s| &s.points)
            .map(|p| self.x_range.fraction(p.x))
            .collect::<Result<_, _>>()?;
        xs.sort_by(f64::total_cmp);
        xs.dedup();
        let slot = xs
            .windows(2)
            .map(|w| w[1] - w[0])
            .filter(|v| *v > 0.0)
            .fold(0.75f64, f64::min)
            * 0.8;
        let count = self.series.len().max(1) as f64;
        let baseline = if self.y_range.minimum >= 0.0 {
            0.0
        } else if self.y_range.maximum <= 0.0 {
            1.0
        } else {
            self.y_range.fraction(0.0)?
        };
        for (series_index, series) in self.series.iter().enumerate() {
            let mut previous = None;
            for point in &series.points {
                let Some(value) = point.y else {
                    previous = None;
                    continue;
                };
                let x = self.x_range.fraction(point.x)?;
                let y = self.y_range.fraction(value)?;
                let key = ChartPointKey {
                    series: series.id,
                    point: point.id,
                };
                if self.settings.view != ChartView::Bar
                    && (0.0..=1.0).contains(&x)
                    && (0.0..=1.0).contains(&y)
                {
                    result.points.push(ChartMark {
                        key,
                        position: position(x, y),
                    });
                }
                match self.settings.view {
                    ChartView::Line | ChartView::Sparkline => {
                        if let Some((px, py)) = previous
                            && let Some((a, b)) = clip_segment((px, py), (x, y))
                        {
                            result.segments.push(ChartLine {
                                series: series.id,
                                start: position(a.0, a.1),
                                end: position(b.0, b.1),
                            });
                        }
                        previous = Some((x, y));
                    }
                    ChartView::Scatter => {}
                    ChartView::Bar => {
                        let left = (x - slot / 2.0 + slot * series_index as f64 / count).max(0.0);
                        let right =
                            (x - slot / 2.0 + slot * (series_index + 1) as f64 / count).min(1.0);
                        let top = y.max(baseline).min(1.0);
                        let bottom = y.min(baseline).max(0.0);
                        if right > left && top >= bottom && (top > bottom || y == baseline) {
                            let a = position(left, top);
                            let b = position(right, bottom);
                            result.bars.push(ChartBar {
                                key,
                                bounds: Rect::new(a.x, a.y, b.x - a.x, b.y - a.y),
                            });
                            result.points.push(ChartMark {
                                key,
                                position: position((left + right) / 2.0, y),
                            });
                        }
                    }
                }
            }
        }
        Ok(result)
    }
}
// Liang–Barsky against the unit plot. Candidate normalization is already bounded.
fn clip_segment(a: (f64, f64), b: (f64, f64)) -> Option<((f64, f64), (f64, f64))> {
    let dx = b.0 - a.0;
    let dy = b.1 - a.1;
    let mut lo = 0.0f64;
    let mut hi = 1.0f64;
    for (p, q) in [(-dx, a.0), (dx, 1.0 - a.0), (-dy, a.1), (dy, 1.0 - a.1)] {
        if p == 0.0 {
            if q < 0.0 {
                return None;
            }
        } else {
            let r = q / p;
            if p < 0.0 {
                lo = lo.max(r);
            } else {
                hi = hi.min(r);
            }
            if lo > hi {
                return None;
            }
        }
    }
    Some((
        (a.0 + lo * dx, a.1 + lo * dy),
        (a.0 + hi * dx, a.1 + hi * dy),
    ))
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChartMark {
    pub key: ChartPointKey,
    pub position: Point,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChartLine {
    pub series: u64,
    pub start: Point,
    pub end: Point,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChartBar {
    pub key: ChartPointKey,
    pub bounds: Rect,
}
#[derive(Clone, Debug)]
pub struct ChartGeometry {
    pub plot: Rect,
    pub x_ticks: Vec<f64>,
    pub y_ticks: Vec<f64>,
    pub points: Vec<ChartMark>,
    pub segments: Vec<ChartLine>,
    pub bars: Vec<ChartBar>,
}
impl ChartGeometry {
    /// Bars win over markers; equal-distance marks use series/input order.
    pub fn hit_test(&self, point: Point, radius: f32) -> Option<ChartPointKey> {
        if !radius.is_finite()
            || radius < 0.0
            || !point.x.is_finite()
            || !point.y.is_finite()
            || point.x < self.plot.x
            || point.x > self.plot.x + self.plot.width
            || point.y < self.plot.y
            || point.y > self.plot.y + self.plot.height
        {
            return None;
        }
        if let Some(bar) = self
            .bars
            .iter()
            .rev()
            .find(|bar| bar.bounds.contains(point.x, point.y))
        {
            return Some(bar.key);
        }
        let mut nearest = None;
        let mut distance = f64::from(radius).powi(2);
        for mark in &self.points {
            let d = (f64::from(mark.position.x) - f64::from(point.x)).powi(2)
                + (f64::from(mark.position.y) - f64::from(point.y)).powi(2);
            if d <= distance && (d < distance || nearest.is_none()) {
                distance = d;
                nearest = Some(mark.key);
            }
        }
        nearest
    }
}
#[cfg(test)]
#[path = "chart_model_tests.rs"]
mod tests;
