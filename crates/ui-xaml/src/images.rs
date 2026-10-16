//! Named image resources in compiled forms. Resource bytes belong to the host.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ImageStretchDefinition {
    None,
    Fill,
    #[default]
    Uniform,
    UniformToFill,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ImageSamplingDefinition {
    Nearest,
    #[default]
    Linear,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ImageDefinition {
    /// Application-registered image name, never a URL or filesystem path.
    pub source: String,
    pub stretch: ImageStretchDefinition,
    pub sampling: ImageSamplingDefinition,
    pub opacity: f32,
    /// The currentColor inherited by an SVG resource; None follows the theme.
    pub foreground: Option<[u8; 4]>,
}
impl Default for ImageDefinition {
    fn default() -> Self {
        Self {
            source: String::new(),
            stretch: Default::default(),
            sampling: Default::default(),
            opacity: 1.0,
            foreground: None,
        }
    }
}
pub fn validate_image(image: &ImageDefinition) -> Result<(), String> {
    if image.source.is_empty()
        || image.source.len() > 256
        || !image
            .source
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-' | b'.'))
    {
        return Err("Image Source requires a registered resource name of 1..=256 ASCII letters, digits, _, - or .".into());
    }
    if !image.opacity.is_finite() || !(0.0..=1.0).contains(&image.opacity) {
        return Err("Image Opacity must be finite and in [0, 1]".into());
    }
    if image.foreground.is_some_and(|color| color[3] != 255) {
        return Err("Image Foreground must be opaque; use Opacity for transparency".into());
    }
    Ok(())
}
