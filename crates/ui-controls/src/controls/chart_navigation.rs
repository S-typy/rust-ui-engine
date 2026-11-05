//! Atomic Cartesian view transforms; observations and point identities stay intact.
use super::*;

impl ChartModel {
    /// Set explicit view ranges, or restore data-derived ranges with None.
    /// Non-Cartesian families reject this operation. Selection is retained even
    /// when its observation moves outside the visible range.
    pub fn set_view_ranges(
        &mut self,
        x: Option<ChartRange>,
        y: Option<ChartRange>,
    ) -> Result<bool, ChartError> {
        if self.family.is_some() {
            return Err(ChartError::InvalidView);
        }
        if self.settings.x_range == x && self.settings.y_range == y {
            return Ok(false);
        }
        let mut settings = self.settings.clone();
        settings.x_range = x;
        settings.y_range = y;
        self.replace(self.series.clone(), settings)?;
        Ok(true)
    }

    /// Zoom both axes by a positive factor around a normalized axis-space anchor.
    /// A factor above one zooms in. (0,0) is the lower-left plot corner; (1,1)
    /// is the upper-right. Invalid or ill-conditioned candidates are atomic.
    pub fn zoom_at(&mut self, factor: f64, anchor: (f64, f64)) -> Result<bool, ChartError> {
        if self.family.is_some() {
            return Err(ChartError::InvalidView);
        }
        if !factor.is_finite()
            || factor <= 0.0
            || ![anchor.0, anchor.1]
                .iter()
                .all(|v| v.is_finite() && (0.0..=1.0).contains(v))
        {
            return Err(ChartError::InvalidRange);
        }
        if factor == 1.0 {
            return Ok(false);
        }
        let zoom = |range: ChartRange, anchor: f64| {
            let span = range.maximum - range.minimum;
            let center = range.minimum + span * anchor;
            ChartRange {
                minimum: center - span / factor * anchor,
                maximum: center + span / factor * (1.0 - anchor),
            }
        };
        self.set_view_ranges(
            Some(
                self.settings
                    .x_scale
                    .raw(zoom(self.settings.x_scale.domain(self.x_range)?, anchor.0))?,
            ),
            Some(
                self.settings
                    .y_scale
                    .raw(zoom(self.settings.y_scale.domain(self.y_range)?, anchor.1))?,
            ),
        )
    }

    /// Translate the viewport by fractions of its current axis-domain spans. Positive
    /// offsets move the view toward larger data coordinates, without editing data.
    pub fn pan(&mut self, x: f64, y: f64) -> Result<bool, ChartError> {
        if self.family.is_some() {
            return Err(ChartError::InvalidView);
        }
        if !x.is_finite() || !y.is_finite() {
            return Err(ChartError::InvalidRange);
        }
        if x == 0.0 && y == 0.0 {
            return Ok(false);
        }
        let shift = |range: ChartRange, offset: f64| {
            let delta = (range.maximum - range.minimum) * offset;
            ChartRange {
                minimum: range.minimum + delta,
                maximum: range.maximum + delta,
            }
        };
        self.set_view_ranges(
            Some(if x == 0.0 {
                self.x_range
            } else {
                self.settings
                    .x_scale
                    .raw(shift(self.settings.x_scale.domain(self.x_range)?, x))?
            }),
            Some(if y == 0.0 {
                self.y_range
            } else {
                self.settings
                    .y_scale
                    .raw(shift(self.settings.y_scale.domain(self.y_range)?, y))?
            }),
        )
    }
}
