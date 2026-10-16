//! Immutable bitmap resources and ordered image commands.
use crate::{DrawCommand, Rect, Scene};
use std::{
    fmt,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

pub const MAX_BITMAP_DIMENSION: u32 = 8192;
pub const MAX_BITMAP_BYTES: usize = 64 * 1024 * 1024;
static NEXT_BITMAP_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BitmapError {
    InvalidDimensions,
    InvalidByteLength,
    IdentityExhausted,
}
impl fmt::Display for BitmapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidDimensions => "bitmap dimensions exceed the nonzero 8192px / 64MiB limits",
            Self::InvalidByteLength => "bitmap requires exactly width * height * 4 RGBA8 bytes",
            Self::IdentityExhausted => "bitmap identity space exhausted",
        })
    }
}
impl std::error::Error for BitmapError {}

struct BitmapData {
    id: u64,
    width: u32,
    height: u32,
    pixels: Box<[u8]>,
}
/// Top-down, tightly packed straight-alpha sRGB RGBA8. Clones share immutable
/// pixels and identity. Decoding and source/resource caching belong to the host.
#[derive(Clone)]
pub struct Bitmap(Arc<BitmapData>);
impl Bitmap {
    pub fn new_rgba8(width: u32, height: u32, pixels: Vec<u8>) -> Result<Self, BitmapError> {
        let expected = (width as usize)
            .checked_mul(height as usize)
            .and_then(|n| n.checked_mul(4));
        if width == 0
            || height == 0
            || width > MAX_BITMAP_DIMENSION
            || height > MAX_BITMAP_DIMENSION
            || expected.is_none_or(|n| n > MAX_BITMAP_BYTES)
        {
            return Err(BitmapError::InvalidDimensions);
        }
        if expected != Some(pixels.len()) {
            return Err(BitmapError::InvalidByteLength);
        }
        let id = NEXT_BITMAP_ID
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
            .map_err(|_| BitmapError::IdentityExhausted)?;
        Ok(Self(Arc::new(BitmapData {
            id,
            width,
            height,
            pixels: pixels.into_boxed_slice(),
        })))
    }
    pub fn id(&self) -> u64 {
        self.0.id
    }
    pub fn width(&self) -> u32 {
        self.0.width
    }
    pub fn height(&self) -> u32 {
        self.0.height
    }
    pub fn pixels(&self) -> &[u8] {
        &self.0.pixels
    }
}
impl fmt::Debug for Bitmap {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Bitmap")
            .field("id", &self.id())
            .field("width", &self.width())
            .field("height", &self.height())
            .finish_non_exhaustive()
    }
}
impl PartialEq for Bitmap {
    fn eq(&self, other: &Self) -> bool {
        self.id() == other.id()
    }
}
impl Eq for Bitmap {}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ImageSampling {
    Nearest,
    #[default]
    Linear,
}

/// An axis-aligned image in logical coordinates. Source is in bitmap pixels;
/// clips do not alter its destination/source mapping. Opacity multiplies alpha.
#[derive(Clone, Debug, PartialEq)]
pub struct ImagePrimitive {
    pub bitmap: Bitmap,
    pub bounds: Rect,
    pub source: Rect,
    pub clip: Rect,
    pub opacity: f32,
    pub sampling: ImageSampling,
}
impl ImagePrimitive {
    pub fn new(bitmap: Bitmap, bounds: Rect) -> Self {
        let source = Rect::new(0.0, 0.0, bitmap.width() as f32, bitmap.height() as f32);
        Self {
            bitmap,
            bounds,
            source,
            clip: bounds,
            opacity: 1.0,
            sampling: ImageSampling::Linear,
        }
    }
    pub fn is_valid(&self) -> bool {
        self.bounds.is_valid()
            && self.bounds.width > 0.0
            && self.bounds.height > 0.0
            && self.clip.is_valid()
            && self.source.is_valid()
            && self.source.width > 0.0
            && self.source.height > 0.0
            && self.source.x >= 0.0
            && self.source.y >= 0.0
            && self.source.x + self.source.width <= self.bitmap.width() as f32
            && self.source.y + self.source.height <= self.bitmap.height() as f32
            && self.opacity.is_finite()
            && (0.0..=1.0).contains(&self.opacity)
    }
}
impl Scene {
    /// Append a valid visible image in painter order. Invalid geometry/crops,
    /// non-finite opacity and fully clipped images are ignored.
    pub fn image(&mut self, image: ImagePrimitive) {
        if image.is_valid() && image.bounds.intersection(image.clip).is_some() {
            self.commands.push(DrawCommand::Image(self.images.len()));
            self.images.push(image);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn immutable_resources_are_bounded_and_identity_is_shared_only_by_clones() {
        assert_eq!(
            Bitmap::new_rgba8(0, 1, vec![]).unwrap_err(),
            BitmapError::InvalidDimensions
        );
        assert_eq!(
            Bitmap::new_rgba8(8192, 8192, vec![]).unwrap_err(),
            BitmapError::InvalidDimensions
        );
        assert_eq!(
            Bitmap::new_rgba8(1, 1, vec![0; 3]).unwrap_err(),
            BitmapError::InvalidByteLength
        );
        let a = Bitmap::new_rgba8(1, 1, vec![255, 0, 0, 128]).unwrap();
        assert_eq!(a, a.clone());
        assert_eq!(a.pixels().as_ptr(), a.clone().pixels().as_ptr());
        assert_ne!(a, Bitmap::new_rgba8(1, 1, a.pixels().to_vec()).unwrap());
    }
    #[test]
    fn scene_image_crop_order_append_and_clear_preserve_resources() {
        let b = Bitmap::new_rgba8(2, 1, vec![255; 8]).unwrap();
        let bounds = Rect::new(0.0, 0.0, 20.0, 10.0);
        let mut scene = Scene::default();
        scene.fill(bounds, crate::Color::rgb(0, 0, 0));
        let mut image = ImagePrimitive::new(b.clone(), bounds);
        image.source.width = 1.0;
        scene.image(image.clone());
        scene.append(&scene.clone());
        assert_eq!(
            scene.commands(),
            &[
                DrawCommand::Rectangle(0),
                DrawCommand::Image(0),
                DrawCommand::Rectangle(1),
                DrawCommand::Image(1)
            ]
        );
        assert_eq!(scene.images[1].bitmap, b);
        for source in [
            Rect::new(-1.0, 0.0, 1.0, 1.0),
            Rect::new(0.0, 0.0, 3.0, 1.0),
            Rect::new(0.0, 0.0, f32::NAN, 1.0),
        ] {
            image.source = source;
            scene.image(image.clone());
        }
        assert_eq!(scene.images.len(), 2);
        scene.clear();
        assert!(scene.images.is_empty());
        assert!(scene.commands().is_empty());
    }
}
