use image::{ImageDecoder, ImageReader};
use rust_desktop_ui_core::{Bitmap, MAX_BITMAP_BYTES, MAX_BITMAP_DIMENSION, Size};
use std::{
    fmt,
    io::{Cursor, Read},
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

pub const MAX_IMAGE_SOURCE_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_SVG_NODES: usize = 4096;
pub const MAX_SVG_BITMAP_BYTES: usize = 16 * 1024 * 1024;
/// Maximum UTF-8 message payload retained by a decoding diagnostic.
pub const MAX_IMAGE_DIAGNOSTIC_BYTES: usize = 4096;
static NEXT_SOURCE_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ImageFormat {
    Png,
    Jpeg,
    Bmp,
    Ico,
    Svg,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImageError {
    Limit(&'static str),
    Invalid(String),
    Unsupported(String),
    Io(String),
}
impl fmt::Display for ImageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Limit(v) => write!(f, "Image limit: {v}"),
            Self::Invalid(v) => write!(f, "Invalid image: {v}"),
            Self::Unsupported(v) => write!(f, "Unsupported image feature: {v}"),
            Self::Io(v) => write!(f, "Image input: {v}"),
        }
    }
}
impl std::error::Error for ImageError {}

impl ImageError {
    fn bounded(self) -> Self {
        fn message(value: String) -> String {
            if value.len() <= MAX_IMAGE_DIAGNOSTIC_BYTES {
                return value.into_boxed_str().into_string();
            }
            let mut end = MAX_IMAGE_DIAGNOSTIC_BYTES - 3;
            while !value.is_char_boundary(end) {
                end -= 1;
            }
            // Allocate a bounded replacement: truncating in place would retain
            // an attacker-sized String capacity in each control's diagnostic.
            let mut bounded = String::with_capacity(end + 3);
            bounded.push_str(&value[..end]);
            bounded.push_str("...");
            bounded
        }
        match self {
            Self::Invalid(value) => Self::Invalid(message(value)),
            Self::Unsupported(value) => Self::Unsupported(message(value)),
            Self::Io(value) => Self::Io(message(value)),
            error @ Self::Limit(_) => error,
        }
    }
}

/// SVG scale controls raster preparation, not its logical intrinsic dimensions.
/// Raster files preserve their original pixels; JPEG orientation is applied.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DecodeOptions {
    pub svg_scale: f32,
    pub current_color: [u8; 3],
}
impl Default for DecodeOptions {
    fn default() -> Self {
        Self {
            svg_scale: 1.0,
            current_color: [0; 3],
        }
    }
}
impl DecodeOptions {
    pub fn validate(self) -> Result<(), ImageError> {
        if !self.svg_scale.is_finite() || !(0.125..=8.0).contains(&self.svg_scale) {
            return Err(ImageError::Limit(
                "SVG scale must be finite and in [0.125, 8]",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct ImageAsset {
    bitmap: Bitmap,
    natural_size: Size,
    format: ImageFormat,
}
impl ImageAsset {
    pub fn bitmap(&self) -> &Bitmap {
        &self.bitmap
    }
    pub fn natural_size(&self) -> Size {
        self.natural_size
    }
    pub fn format(&self) -> ImageFormat {
        self.format
    }
}

/// Bounded, immutable encoded bytes. Cloning shares identity and data; loading
/// never accesses URLs or resolves resources relative to a document location.
#[derive(Clone)]
pub struct ImageSource(Arc<SourceData>);
struct SourceData {
    id: u64,
    bytes: Box<[u8]>,
    format: ImageFormat,
}
impl fmt::Debug for ImageSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ImageSource")
            .field("id", &self.id())
            .field("format", &self.format())
            .field("bytes", &self.bytes().len())
            .finish()
    }
}
impl ImageSource {
    pub fn from_bytes(bytes: Vec<u8>, format: ImageFormat) -> Result<Self, ImageError> {
        source_limit(&bytes)?;
        let id = NEXT_SOURCE_ID
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |v| v.checked_add(1))
            .map_err(|_| ImageError::Limit("source identity exhausted"))?;
        Ok(Self(Arc::new(SourceData {
            id,
            bytes: bytes.into_boxed_slice(),
            format,
        })))
    }
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, ImageError> {
        let mut bytes = Vec::new();
        std::fs::File::open(path)
            .map_err(|e| ImageError::Io(e.to_string()).bounded())?
            .take(MAX_IMAGE_SOURCE_BYTES as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| ImageError::Io(e.to_string()).bounded())?;
        let format = detect_format(&bytes)?;
        Self::from_bytes(bytes, format)
    }
    pub fn id(&self) -> u64 {
        self.0.id
    }
    pub fn bytes(&self) -> &[u8] {
        &self.0.bytes
    }
    pub fn format(&self) -> ImageFormat {
        self.0.format
    }
    pub fn decode(&self, options: DecodeOptions) -> Result<ImageAsset, ImageError> {
        decode(self.bytes(), self.format(), options)
    }
}

pub fn detect_format(bytes: &[u8]) -> Result<ImageFormat, ImageError> {
    source_limit(bytes)?;
    if let Ok(format) = image::guess_format(bytes) {
        return match format {
            image::ImageFormat::Png => Ok(ImageFormat::Png),
            image::ImageFormat::Jpeg => Ok(ImageFormat::Jpeg),
            image::ImageFormat::Bmp => Ok(ImageFormat::Bmp),
            image::ImageFormat::Ico => Ok(ImageFormat::Ico),
            _ => Err(ImageError::Unsupported("raster format".into())),
        };
    }
    let text =
        std::str::from_utf8(bytes).map_err(|_| ImageError::Invalid("unknown format".into()))?;
    if text
        .trim_start_matches('\u{feff}')
        .trim_start()
        .starts_with('<')
    {
        return Ok(ImageFormat::Svg);
    }
    Err(ImageError::Invalid("unknown format".into()))
}

fn source_limit(bytes: &[u8]) -> Result<(), ImageError> {
    if bytes.is_empty() || bytes.len() > MAX_IMAGE_SOURCE_BYTES {
        Err(ImageError::Limit("encoded source must contain 1..=16 MiB"))
    } else {
        Ok(())
    }
}
fn dimensions(width: u32, height: u32) -> Result<(), ImageError> {
    if width == 0
        || height == 0
        || width > MAX_BITMAP_DIMENSION
        || height > MAX_BITMAP_DIMENSION
        || u64::from(width) * u64::from(height) * 4 > MAX_BITMAP_BYTES as u64
    {
        Err(ImageError::Limit(
            "decoded image must fit 8192px per side and 64 MiB RGBA",
        ))
    } else {
        Ok(())
    }
}

pub fn decode(
    bytes: &[u8],
    format: ImageFormat,
    options: DecodeOptions,
) -> Result<ImageAsset, ImageError> {
    decode_inner(bytes, format, options).map_err(ImageError::bounded)
}

fn decode_inner(
    bytes: &[u8],
    format: ImageFormat,
    options: DecodeOptions,
) -> Result<ImageAsset, ImageError> {
    source_limit(bytes)?;
    options.validate()?;
    if format == ImageFormat::Svg {
        return decode_svg(bytes, options);
    }
    let raster_format = match format {
        ImageFormat::Png => image::ImageFormat::Png,
        ImageFormat::Jpeg => image::ImageFormat::Jpeg,
        ImageFormat::Bmp => image::ImageFormat::Bmp,
        ImageFormat::Ico => image::ImageFormat::Ico,
        ImageFormat::Svg => unreachable!(),
    };
    let mut reader = ImageReader::with_format(Cursor::new(bytes), raster_format);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_BITMAP_DIMENSION);
    limits.max_image_height = Some(MAX_BITMAP_DIMENSION);
    limits.max_alloc = Some(MAX_BITMAP_BYTES as u64 * 2);
    reader.limits(limits);
    let mut decoder = reader
        .into_decoder()
        .map_err(|e| ImageError::Invalid(e.to_string()))?;
    let (width, height) = decoder.dimensions();
    dimensions(width, height)?;
    let orientation = decoder
        .orientation()
        .map_err(|e| ImageError::Invalid(e.to_string()))?;
    let mut image = image::DynamicImage::from_decoder(decoder)
        .map_err(|e| ImageError::Invalid(e.to_string()))?;
    image.apply_orientation(orientation);
    let rgba = image.into_rgba8();
    let (width, height) = rgba.dimensions();
    let bitmap = Bitmap::new_rgba8(width, height, rgba.into_raw())
        .map_err(|e| ImageError::Invalid(e.to_string()))?;
    Ok(ImageAsset {
        bitmap,
        natural_size: Size::new(width as f32, height as f32),
        format,
    })
}

fn decode_svg(bytes: &[u8], options: DecodeOptions) -> Result<ImageAsset, ImageError> {
    let text =
        std::str::from_utf8(bytes).map_err(|_| ImageError::Invalid("SVG requires UTF-8".into()))?;
    crate::svg::validate_source(text)?;
    let [r, g, b] = options.current_color;
    let settings = resvg::usvg::Options {
        resources_dir: None,
        // User-agent color is inherited by shapes using currentColor. Explicit
        // color declarations in the source keep their ordinary CSS precedence.
        style_sheet: Some(format!("svg {{ color: #{r:02x}{g:02x}{b:02x}; }}")),
        image_href_resolver: resvg::usvg::ImageHrefResolver {
            resolve_data: Box::new(|_, _, _| None),
            resolve_string: Box::new(|_, _| None),
        },
        ..Default::default()
    };
    let tree = resvg::usvg::Tree::from_data(bytes, &settings)
        .map_err(|e| ImageError::Invalid(e.to_string()))?;
    let size = tree.size();
    let width = (size.width() * options.svg_scale).ceil();
    let height = (size.height() * options.svg_scale).ceil();
    if !width.is_finite()
        || !height.is_finite()
        || width > MAX_BITMAP_DIMENSION as f32
        || height > MAX_BITMAP_DIMENSION as f32
    {
        return Err(ImageError::Limit("SVG raster dimensions"));
    }
    dimensions(width as u32, height as u32)?;
    if width as u64 * height as u64 * 4 > MAX_SVG_BITMAP_BYTES as u64 {
        return Err(ImageError::Limit("SVG bitmap exceeds 16 MiB"));
    }
    crate::svg::validate_render(&tree, width as u32, height as u32, options.svg_scale)?;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(width as u32, height as u32)
        .ok_or(ImageError::Limit("SVG raster allocation"))?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(options.svg_scale, options.svg_scale),
        &mut pixmap.as_mut(),
    );
    // tiny-skia stores premultiplied sRGB; Scene's immutable Bitmap is straight
    // sRGB and the GPU performs premultiplication after linearization.
    let mut rgba = pixmap.take();
    for pixel in rgba.chunks_exact_mut(4) {
        let alpha = u32::from(pixel[3]);
        for channel in &mut pixel[..3] {
            *channel = (u32::from(*channel) * 255 + alpha / 2)
                .checked_div(alpha)
                .unwrap_or(0)
                .min(255) as u8;
        }
    }
    let bitmap = Bitmap::new_rgba8(width as u32, height as u32, rgba)
        .map_err(|e| ImageError::Invalid(e.to_string()))?;
    Ok(ImageAsset {
        bitmap,
        natural_size: Size::new(size.width(), size.height()),
        format: ImageFormat::Svg,
    })
}

#[cfg(test)]
mod diagnostic_tests {
    use super::*;

    #[test]
    fn oversized_diagnostics_keep_category_utf8_and_bounded_allocation() {
        for error in [
            ImageError::Invalid("путь".repeat(1500)),
            ImageError::Unsupported("特性".repeat(1500)),
            ImageError::Io("path".repeat(1500)),
        ] {
            let category = std::mem::discriminant(&error);
            let bounded = error.bounded();
            assert_eq!(std::mem::discriminant(&bounded), category);
            let (ImageError::Invalid(value)
            | ImageError::Unsupported(value)
            | ImageError::Io(value)) = bounded
            else {
                panic!("string diagnostic")
            };
            assert!(value.len() <= MAX_IMAGE_DIAGNOSTIC_BYTES);
            assert!(value.capacity() <= MAX_IMAGE_DIAGNOSTIC_BYTES);
            assert!(value.ends_with("..."));
        }
    }

    #[test]
    fn source_and_free_decode_bound_untrusted_reference_diagnostics() {
        let source = format!(
            "<svg width='8' height='8'><rect width='1' height='1' fill='url(#{})'/></svg>",
            "missing".repeat(900)
        );
        let image = ImageSource::from_bytes(source.into_bytes(), ImageFormat::Svg).unwrap();
        for result in [
            image.decode(DecodeOptions::default()),
            decode(image.bytes(), image.format(), DecodeOptions::default()),
        ] {
            let Err(ImageError::Invalid(message)) = result else {
                panic!("missing reference must remain Invalid")
            };
            assert!(message.starts_with("missing SVG reference"));
            assert!(message.ends_with("..."));
            assert!(message.capacity() <= MAX_IMAGE_DIAGNOSTIC_BYTES);
        }
    }
}
