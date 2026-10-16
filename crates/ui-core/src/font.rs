//! Renderer-independent typography selection in logical pixels.

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum FontSlant {
    #[default]
    Normal,
    Italic,
    Oblique(f32),
}

/// Requested typography. A missing family remains a request; the text engine
/// chooses fallback fonts without rewriting this value.
#[derive(Clone, Debug, PartialEq)]
pub struct FontDescriptor {
    pub family: String,
    pub size: f32,
    pub weight: u16,
    /// Percentage of the normal face width, not a geometric scale transform.
    pub width: f32,
    pub slant: FontSlant,
}
impl Default for FontDescriptor {
    fn default() -> Self {
        Self {
            family: "Segoe UI".into(),
            size: 14.0,
            weight: 400,
            width: 100.0,
            slant: FontSlant::Normal,
        }
    }
}
impl FontDescriptor {
    pub fn is_valid(&self) -> bool {
        !self.family.trim().is_empty()
            && self.family.len() <= 256
            && !self.family.chars().any(char::is_control)
            && self.size.is_finite()
            && self.size > 0.0
            && self.size <= 512.0
            && (1..=1000).contains(&self.weight)
            && self.width.is_finite()
            && (25.0..=400.0).contains(&self.width)
            && match self.slant {
                FontSlant::Oblique(angle) => angle.is_finite() && (-45.0..=45.0).contains(&angle),
                _ => true,
            }
    }
}
