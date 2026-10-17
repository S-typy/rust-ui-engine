//! Exact color and bounded linear-gradient values, independent of rendering.
use crate::Color;
use std::{fmt, str::FromStr};

pub const MAX_GRADIENT_STOPS: usize = 32;

/// Straight-alpha sRGB channels. Transparent colors retain their RGB channels.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Srgba8 {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}
impl Srgba8 {
    pub const BLACK: Self = Self::new(0, 0, 0, 255);
    pub const WHITE: Self = Self::new(255, 255, 255, 255);
    pub const TRANSPARENT: Self = Self::new(0, 0, 0, 0);
    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self::new(r, g, b, 255)
    }
    pub fn linear_rgba(self) -> [f64; 4] {
        [
            decode(self.r),
            decode(self.g),
            decode(self.b),
            f64::from(self.a) / 255.0,
        ]
    }
}
impl From<Srgba8> for Color {
    fn from(value: Srgba8) -> Self {
        Self {
            r: f32::from(value.r) / 255.0,
            g: f32::from(value.g) / 255.0,
            b: f32::from(value.b) / 255.0,
            a: f32::from(value.a) / 255.0,
        }
    }
}
impl fmt::Display for Srgba8 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "#{:02X}{:02X}{:02X}{:02X}",
            self.a, self.r, self.g, self.b
        )
    }
}
impl FromStr for Srgba8 {
    type Err = ColorValueError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let digits = value
            .strip_prefix('#')
            .filter(|digits| {
                matches!(digits.len(), 6 | 8) && digits.bytes().all(|b| b.is_ascii_hexdigit())
            })
            .ok_or(ColorValueError("Color must be #RRGGBB or #AARRGGBB"))?;
        let byte = |offset| u8::from_str_radix(&digits[offset..offset + 2], 16).unwrap();
        let (offset, alpha) = if digits.len() == 8 {
            (2, byte(0))
        } else {
            (0, 255)
        };
        Ok(Self::new(
            byte(offset),
            byte(offset + 2),
            byte(offset + 4),
            alpha,
        ))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ColorValueError(pub &'static str);
impl fmt::Display for ColorValueError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for ColorValueError {}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GradientStop {
    pub offset: f64,
    pub color: Srgba8,
}
impl GradientStop {
    pub const fn new(offset: f64, color: Srgba8) -> Self {
        Self { offset, color }
    }
}

/// A validated gradient. Duplicate offsets retain their order and form a hard
/// edge: the last stop at the sampled offset wins. Angles are clockwise degrees
/// from left-to-right, in `0..=360`. Stops outside the first/last offset extend
/// those colors. Accessors expose immutable data to preserve this invariant.
#[derive(Clone, Debug, PartialEq)]
pub struct LinearGradient {
    angle: f64,
    stops: Vec<GradientStop>,
}
impl LinearGradient {
    pub fn new(angle: f64, stops: Vec<GradientStop>) -> Result<Self, ColorValueError> {
        if !angle.is_finite() || !(0.0..=360.0).contains(&angle) {
            return Err(ColorValueError(
                "Gradient angle must be finite and in [0, 360]",
            ));
        }
        if !(2..=MAX_GRADIENT_STOPS).contains(&stops.len()) {
            return Err(ColorValueError("A gradient requires 2..=32 stops"));
        }
        if stops
            .iter()
            .any(|s| !s.offset.is_finite() || !(0.0..=1.0).contains(&s.offset))
            || stops.windows(2).any(|s| s[0].offset > s[1].offset)
        {
            return Err(ColorValueError(
                "Gradient offsets must be finite, sorted and in [0, 1]",
            ));
        }
        Ok(Self { angle, stops })
    }
    pub fn angle(&self) -> f64 {
        self.angle
    }
    pub fn stops(&self) -> &[GradientStop] {
        &self.stops
    }
    /// Sample the normalized gradient axis, with premultiplied linear-light
    /// color interpolation and straight-alpha sRGB output. RGB remains useful
    /// even when both endpoint alphas are zero.
    pub fn sample(&self, position: f64) -> Result<Srgba8, ColorValueError> {
        if !position.is_finite() {
            return Err(ColorValueError("Sample position must be finite"));
        }
        let position = position.clamp(0.0, 1.0);
        let index = self.stops.partition_point(|s| s.offset <= position);
        if index == 0 {
            return Ok(self.stops[0].color);
        }
        if index == self.stops.len() {
            return Ok(self.stops[index - 1].color);
        }
        let left = self.stops[index - 1];
        let right = self.stops[index];
        let t = (position - left.offset) / (right.offset - left.offset);
        Ok(interpolate(left.color, right.color, t))
    }
    /// Map normalized rectangle coordinates onto this gradient's axis.
    pub fn position(&self, x: f64, y: f64) -> Result<f64, ColorValueError> {
        if !x.is_finite()
            || !y.is_finite()
            || !(0.0..=1.0).contains(&x)
            || !(0.0..=1.0).contains(&y)
        {
            return Err(ColorValueError(
                "Preview coordinates must be finite and in [0, 1]",
            ));
        }
        let angle = self.angle.to_radians();
        let (sin, cos) = angle.sin_cos();
        let start = cos.min(0.0) + sin.min(0.0);
        let length = cos.abs() + sin.abs();
        Ok(((x * cos + y * sin - start) / length).clamp(0.0, 1.0))
    }
}
impl Default for LinearGradient {
    fn default() -> Self {
        Self::new(
            0.0,
            vec![
                GradientStop::new(0.0, Srgba8::BLACK),
                GradientStop::new(1.0, Srgba8::WHITE),
            ],
        )
        .unwrap()
    }
}

/// Editor value; it does not imply that every scene primitive accepts brushes.
#[derive(Clone, Debug, PartialEq)]
pub enum BrushValue {
    Solid(Srgba8),
    Linear(LinearGradient),
}
impl Default for BrushValue {
    fn default() -> Self {
        Self::Solid(Srgba8::BLACK)
    }
}

fn decode(channel: u8) -> f64 {
    let value = f64::from(channel) / 255.0;
    if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}
fn encode(value: f64) -> u8 {
    let value = if value <= 0.0031308 {
        value * 12.92
    } else {
        1.055 * value.powf(1.0 / 2.4) - 0.055
    };
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}
fn interpolate(left: Srgba8, right: Srgba8, t: f64) -> Srgba8 {
    if t <= 0.0 {
        return left;
    }
    if t >= 1.0 {
        return right;
    }
    let a = left.linear_rgba();
    let b = right.linear_rgba();
    let alpha = a[3] + (b[3] - a[3]) * t;
    let channel = |index| {
        let value = if alpha == 0.0 {
            a[index] + (b[index] - a[index]) * t
        } else {
            (a[index] * a[3] * (1.0 - t) + b[index] * b[3] * t) / alpha
        };
        encode(value)
    };
    Srgba8::new(
        channel(0),
        channel(1),
        channel(2),
        (alpha * 255.0).round() as u8,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_channels_and_alpha_round_trip_including_transparent_rgb() {
        for alpha in 0..=255 {
            for channel in 0..=255 {
                let value = Srgba8::new(channel, 255 - channel, channel / 2, alpha);
                assert_eq!(value.to_string().parse::<Srgba8>(), Ok(value));
                assert_eq!(
                    value.to_string().to_lowercase().parse::<Srgba8>(),
                    Ok(value)
                );
            }
        }
        assert_eq!(
            "#123456".parse::<Srgba8>().unwrap(),
            Srgba8::rgb(0x12, 0x34, 0x56)
        );
        assert_eq!(
            "#00123456".parse::<Srgba8>().unwrap().to_string(),
            "#00123456"
        );
        for value in [
            "",
            "#123",
            "#123456789",
            "#GG000000",
            " #123456",
            "#123456 ",
            "#１２３４５６",
        ] {
            assert!(value.parse::<Srgba8>().is_err(), "{value}");
        }
    }
    #[test]
    fn gradient_rejects_invalid_bounds_before_exposing_value() {
        let stops = LinearGradient::default().stops().to_vec();
        for angle in [f64::NAN, f64::INFINITY, -1.0, 361.0] {
            assert!(LinearGradient::new(angle, stops.clone()).is_err());
        }
        for offset in [f64::NAN, f64::INFINITY, -0.01, 1.01] {
            assert!(
                LinearGradient::new(
                    0.0,
                    vec![GradientStop::new(offset, Srgba8::BLACK), stops[1]]
                )
                .is_err()
            );
        }
        assert!(LinearGradient::new(0.0, vec![stops[1], stops[0]]).is_err());
        assert!(LinearGradient::new(0.0, vec![stops[0]; 33]).is_err());
        assert!(LinearGradient::new(0.0, vec![stops[0]]).is_err());
    }
    #[test]
    fn duplicate_stops_form_hard_edges_and_premultiplied_alpha_avoids_color_leaks() {
        let red = Srgba8::rgb(255, 0, 0);
        let blue = Srgba8::rgb(0, 0, 255);
        let gradient = LinearGradient::new(
            90.0,
            vec![
                GradientStop::new(0.25, red),
                GradientStop::new(0.5, red),
                GradientStop::new(0.5, blue),
                GradientStop::new(0.75, blue),
            ],
        )
        .unwrap();
        assert_eq!(gradient.sample(0.0).unwrap(), red);
        assert_eq!(gradient.sample(0.5 - f64::EPSILON).unwrap(), red);
        assert_eq!(gradient.sample(0.5).unwrap(), blue);
        assert_eq!(gradient.sample(1.0).unwrap(), blue);
        assert!(gradient.sample(f64::NAN).is_err());
        assert!((gradient.position(0.7, 0.2).unwrap() - 0.2).abs() < 1e-12);
        assert!(gradient.position(0.0, f64::NAN).is_err());
        let transparent = Srgba8::new(255, 0, 0, 0);
        let gradient = LinearGradient::new(
            0.0,
            vec![
                GradientStop::new(0.0, transparent),
                GradientStop::new(1.0, blue),
            ],
        )
        .unwrap();
        assert_eq!(gradient.sample(0.0).unwrap(), transparent);
        assert_eq!(gradient.sample(0.5).unwrap(), Srgba8::new(0, 0, 255, 128));
        assert_eq!(
            LinearGradient::default().sample(0.5).unwrap(),
            Srgba8::rgb(188, 188, 188)
        );
    }
}
