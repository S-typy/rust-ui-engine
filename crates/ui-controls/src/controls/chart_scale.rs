//! Domain transforms shared by Cartesian geometry, ticks and navigation.
use super::*;

/// Coordinate mapping for Line, Scatter and Sparkline axes. Observations and
/// public ranges retain their original numeric values.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ChartScale {
    #[default]
    Linear,
    /// Base-ten logarithm; all values on this axis must be strictly positive.
    Log10,
}

impl ChartScale {
    pub(super) fn caption(self, title: &str) -> String {
        match self {
            Self::Linear => title.to_owned(),
            Self::Log10 if title.is_empty() => "log10".into(),
            Self::Log10 => format!("{title} · log10"),
        }
    }
    pub(super) fn project(self, value: f64) -> Result<f64, ChartError> {
        if !value.is_finite() {
            return Err(ChartError::NonFinite);
        }
        match self {
            Self::Linear => Ok(value),
            Self::Log10 if value > 0.0 => Ok(value.log10()),
            Self::Log10 => Err(ChartError::InvalidRange),
        }
    }
    fn unproject(self, value: f64) -> Result<f64, ChartError> {
        if !value.is_finite() {
            return Err(ChartError::InvalidRange);
        }
        let smallest = f64::from_bits(1);
        let result = match self {
            Self::Linear => value,
            Self::Log10 if value == smallest.log10() => smallest,
            Self::Log10 if value == f64::MAX.log10() => f64::MAX,
            Self::Log10 => 10.0f64.powf(value),
        };
        if !result.is_finite() || (self == Self::Log10 && result <= 0.0) {
            return Err(ChartError::InvalidRange);
        }
        Ok(result)
    }
    pub(super) fn domain(self, range: ChartRange) -> Result<ChartRange, ChartError> {
        range.validate()?;
        let result = ChartRange {
            minimum: self.project(range.minimum)?,
            maximum: self.project(range.maximum)?,
        };
        result.validate()?;
        Ok(result)
    }
    pub(super) fn raw(self, range: ChartRange) -> Result<ChartRange, ChartError> {
        range.validate()?;
        let result = ChartRange {
            minimum: self.unproject(range.minimum)?,
            maximum: self.unproject(range.maximum)?,
        };
        // Inversion may round both bounds to one subnormal or project to an
        // indistinguishable logarithm. Reject instead of silently collapsing.
        self.domain(result)?;
        Ok(result)
    }
    pub(super) fn fraction(self, range: ChartRange, value: f64) -> Result<f64, ChartError> {
        self.domain(range)?.fraction(self.project(value)?)
    }
    pub(super) fn automatic(
        self,
        values: impl Iterator<Item = f64>,
        zero: bool,
    ) -> Result<ChartRange, ChartError> {
        if self == Self::Linear {
            return automatic(values, zero);
        }
        if zero {
            return Err(ChartError::InvalidView);
        }
        let values = values.collect::<Vec<_>>();
        for value in &values {
            self.project(*value)?;
        }
        let Some(first) = values.first().copied() else {
            return Ok(ChartRange {
                minimum: 1.0,
                maximum: 10.0,
            });
        };
        let minimum = values.iter().copied().fold(first, f64::min);
        let maximum = values.iter().copied().fold(first, f64::max);
        if minimum < maximum {
            // Retain exact raw extrema: inverse(log10(max)) can round below
            // max, which would incorrectly exclude that original observation.
            let range = ChartRange { minimum, maximum };
            self.domain(range)?;
            return Ok(range);
        }
        let value = self.project(first)?;
        self.raw(ChartRange {
            minimum: (value - 0.5).max(f64::from_bits(1).log10()),
            maximum: (value + 0.5).min(f64::MAX.log10()),
        })
    }
    pub(super) fn ticks(self, range: ChartRange) -> Result<Vec<f64>, ChartError> {
        if self == Self::Linear {
            return range.ticks();
        }
        let domain = self.domain(range)?;
        let first = domain.minimum.ceil() as i32;
        let last = domain.maximum.floor() as i32;
        let count = last - first + 1;
        if (2..=9).contains(&count) {
            let mut values = Vec::new();
            for power in first..=last {
                let v = self.unproject(f64::from(power))?;
                if v >= range.minimum && v <= range.maximum && values.last() != Some(&v) {
                    values.push(v);
                }
            }
            if values.len() >= 2 {
                if values.len() > 5 {
                    // Keep endpoint decades but leave enough space for the
                    // presenter's bounded labels at its minimum full width.
                    let last = values.len() - 1;
                    let step = last.div_ceil(4);
                    values = values
                        .into_iter()
                        .enumerate()
                        .filter_map(|(index, value)| {
                            (index % step == 0 || index == last).then_some(value)
                        })
                        .collect();
                }
                return Ok(values);
            }
        }
        let ticks = domain.ticks()?;
        let last = ticks.len() - 1;
        let mut values = Vec::new();
        for (index, value) in ticks.into_iter().enumerate() {
            let v = if index == 0 {
                range.minimum
            } else if index == last {
                range.maximum
            } else {
                self.unproject(value)?
            };
            if v >= range.minimum && v <= range.maximum && values.last() != Some(&v) {
                values.push(v);
            }
        }
        Ok(values)
    }
}
