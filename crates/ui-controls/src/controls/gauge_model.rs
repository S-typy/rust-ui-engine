//! Bounded, read-only measurement scales shared by all gauge presentations.
use super::*;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GaugeView {
    #[default]
    Circular,
    Linear,
    Digital,
    State,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GaugeTone {
    #[default]
    Neutral,
    Accent,
    Good,
    Warning,
    Danger,
}
#[derive(Clone, Debug, PartialEq)]
pub struct GaugeRange {
    pub start: f64,
    pub end: f64,
    pub label: String,
    pub tone: GaugeTone,
}
#[derive(Clone, Debug, PartialEq)]
pub struct GaugeSettings {
    pub view: GaugeView,
    pub minimum: f64,
    pub maximum: f64,
    pub value: f64,
    /// Number of evenly spaced major ticks, including both endpoints.
    pub major_ticks: u8,
    /// Number of minor ticks between each pair of major ticks.
    pub minor_ticks: u8,
    pub precision: u8,
    pub unit: String,
    /// Sorted, nonoverlapping intervals. Gaps have a neutral state.
    pub ranges: Vec<GaugeRange>,
}
impl Default for GaugeSettings {
    fn default() -> Self {
        Self {
            view: GaugeView::Circular,
            minimum: 0.0,
            maximum: 100.0,
            value: 0.0,
            major_ticks: 6,
            minor_ticks: 3,
            precision: 1,
            unit: String::new(),
            ranges: Vec::new(),
        }
    }
}
fn text_bound(value: &str, bytes: usize, graphemes: usize) -> bool {
    value.len() <= bytes
        && TextDocument::new(value).grapheme_count() <= graphemes
        && !value.chars().any(char::is_control)
}
impl GaugeSettings {
    pub fn validate(&self) -> Result<(), ControlError> {
        if !self.minimum.is_finite()
            || !self.maximum.is_finite()
            || self.minimum >= self.maximum
            || !(self.maximum - self.minimum).is_finite()
            || !self.value.is_finite()
            || !(self.minimum..=self.maximum).contains(&self.value)
            || !(2..=21).contains(&self.major_ticks)
            || self.minor_ticks > 4
            || self.precision > 6
            || !text_bound(&self.unit, 128, 32)
            || self.ranges.len() > 16
        {
            return Err(ControlError::Invalid("Gauge requires finite minimum < maximum, in-range value, 2..21 major ticks, 0..4 minor ticks, precision 0..6 and bounded unit/ranges".into()));
        }
        let mut previous = self.minimum;
        for range in &self.ranges {
            if !range.start.is_finite()
                || !range.end.is_finite()
                || range.start < previous
                || range.start < self.minimum
                || range.start >= range.end
                || range.end > self.maximum
                || range.label.is_empty()
                || !text_bound(&range.label, 256, 64)
            {
                return Err(ControlError::Invalid("Gauge ranges require sorted nonoverlapping intervals inside the scale and nonempty bounded labels".into()));
            }
            previous = range.end;
        }
        Ok(())
    }
    pub(super) fn fraction(&self, value: f64) -> f64 {
        ((value - self.minimum) / (self.maximum - self.minimum)).clamp(0.0, 1.0)
    }
    /// Shared boundaries belong to the following range; maximum is inclusive.
    pub fn active_range(&self) -> Option<&GaugeRange> {
        self.ranges.iter().find(|range| {
            self.value >= range.start
                && (self.value < range.end
                    || self.value == self.maximum && range.end == self.maximum)
        })
    }
    pub fn state_label(&self) -> &str {
        self.active_range()
            .map_or("Без диапазона", |r| r.label.as_str())
    }
    /// Scientific notation avoids a huge label or displaying a nonzero value as zero.
    pub fn format_number(&self, value: f64) -> String {
        let fixed = format!("{:.*}", usize::from(self.precision), value);
        if fixed.len() > 16 || value != 0.0 && fixed.parse::<f64>().ok() == Some(0.0) {
            format!("{:.*e}", usize::from(self.precision), value)
        } else {
            fixed
        }
    }
    pub fn value_text(&self) -> String {
        let number = self.format_number(self.value);
        if self.unit.is_empty() {
            number
        } else {
            format!("{number} {}", self.unit)
        }
    }
    pub fn semantic_value(&self) -> String {
        format!("{} · {}", self.value_text(), self.state_label())
    }
    /// Duplicate floating-point ticks are removed; exact endpoints remain present.
    /// Invalid settings return an empty collection, without allocating tick geometry.
    pub fn ticks(&self) -> Vec<(f64, bool)> {
        if self.validate().is_err() {
            return Vec::new();
        }
        let intervals = usize::from(self.major_ticks - 1);
        let subdivisions = usize::from(self.minor_ticks) + 1;
        let total = intervals * subdivisions;
        let mut result: Vec<(f64, bool)> = Vec::with_capacity(total + 1);
        for index in 0..=total {
            let value = if index == total {
                self.maximum
            } else {
                self.minimum + (self.maximum - self.minimum) * (index as f64 / total as f64)
            };
            let major = index % subdivisions == 0;
            if let Some(last) = result.last_mut()
                && last.0 == value
            {
                last.1 |= major;
            } else {
                result.push((value, major));
            }
        }
        result
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GaugeDisplayStatus {
    Full,
    /// Tight viewports show a value text instead of clipped scale geometry.
    Compact,
}
