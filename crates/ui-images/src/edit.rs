//! Exact RGBA operations and a bounded accepted/draft image session.
use crate::{ImageError, MAX_IMAGE_SOURCE_BYTES};
use image::ImageEncoder;
use rust_desktop_ui_core::Bitmap;
use std::{
    collections::{HashSet, VecDeque},
    io::Write,
};

pub const MAX_IMAGE_HISTORY_BYTES: usize = 128 * 1024 * 1024;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PixelCrop {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}
impl PixelCrop {
    pub fn validate(self, image: &Bitmap) -> Result<(), ImageError> {
        if self.width == 0
            || self.height == 0
            || self
                .x
                .checked_add(self.width)
                .is_none_or(|x| x > image.width())
            || self
                .y
                .checked_add(self.height)
                .is_none_or(|y| y > image.height())
        {
            return Err(ImageError::Invalid(
                "Crop must be a nonempty integer rectangle inside the image".into(),
            ));
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuarterTurn {
    Clockwise,
    HalfTurn,
    CounterClockwise,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImageEditLimits {
    pub history_states: usize,
    pub retained_bytes: usize,
}
impl Default for ImageEditLimits {
    fn default() -> Self {
        Self {
            history_states: 16,
            retained_bytes: MAX_IMAGE_HISTORY_BYTES,
        }
    }
}
impl ImageEditLimits {
    pub fn validate(self) -> Result<(), ImageError> {
        if !(1..=16).contains(&self.history_states)
            || self.retained_bytes == 0
            || self.retained_bytes > MAX_IMAGE_HISTORY_BYTES
        {
            return Err(ImageError::Limit(
                "image history requires 1..16 states and 1..128 MiB retained pixels",
            ));
        }
        Ok(())
    }
}
pub fn crop_bitmap(image: &Bitmap, crop: PixelCrop) -> Result<Bitmap, ImageError> {
    crop.validate(image)?;
    let row = crop.width as usize * 4;
    let mut pixels = Vec::with_capacity(row * crop.height as usize);
    for y in crop.y..crop.y + crop.height {
        let start = (y as usize * image.width() as usize + crop.x as usize) * 4;
        pixels.extend_from_slice(&image.pixels()[start..start + row]);
    }
    Bitmap::new_rgba8(crop.width, crop.height, pixels).map_err(bitmap_error)
}
pub fn rotate_bitmap(image: &Bitmap, turn: QuarterTurn) -> Result<Bitmap, ImageError> {
    let (w, h) = (image.width() as usize, image.height() as usize);
    let (width, height) = if turn == QuarterTurn::HalfTurn {
        (w, h)
    } else {
        (h, w)
    };
    let mut pixels = vec![0; image.pixels().len()];
    for y in 0..h {
        for x in 0..w {
            let (dx, dy) = match turn {
                QuarterTurn::Clockwise => (h - 1 - y, x),
                QuarterTurn::HalfTurn => (w - 1 - x, h - 1 - y),
                QuarterTurn::CounterClockwise => (y, w - 1 - x),
            };
            pixels[(dy * width + dx) * 4..(dy * width + dx) * 4 + 4]
                .copy_from_slice(&image.pixels()[(y * w + x) * 4..(y * w + x) * 4 + 4]);
        }
    }
    Bitmap::new_rgba8(width as u32, height as u32, pixels).map_err(bitmap_error)
}
fn bitmap_error(e: impl ToString) -> ImageError {
    ImageError::Invalid(e.to_string())
}
/// Lossless PNG, bounded to the same 16 MiB encoded limit as ImageSource.
/// No file is opened and no image state changes when encoding fails.
pub fn encode_bitmap_png(image: &Bitmap) -> Result<Vec<u8>, ImageError> {
    struct Bounded(Vec<u8>);
    impl Write for Bounded {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if self
                .0
                .len()
                .checked_add(bytes.len())
                .is_none_or(|n| n > MAX_IMAGE_SOURCE_BYTES)
            {
                return Err(std::io::Error::other("PNG exceeds encoded source limit"));
            }
            self.0.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut out = Bounded(Vec::new());
    image::codecs::png::PngEncoder::new(&mut out)
        .write_image(
            image.pixels(),
            image.width(),
            image.height(),
            image::ExtendedColorType::Rgba8,
        )
        .map_err(bitmap_error)?;
    Ok(out.0)
}
#[derive(Clone, Debug)]
pub struct ImageEditSession {
    accepted: Option<Bitmap>,
    draft: Option<Bitmap>,
    undo: VecDeque<Option<Bitmap>>,
    redo: Vec<Option<Bitmap>>,
    limits: ImageEditLimits,
}
impl ImageEditSession {
    pub fn new(value: Option<Bitmap>, limits: ImageEditLimits) -> Result<Self, ImageError> {
        limits.validate()?;
        if value
            .as_ref()
            .is_some_and(|v| v.pixels().len() > limits.retained_bytes)
        {
            return Err(ImageError::Limit(
                "accepted image exceeds retained pixel budget",
            ));
        }
        Ok(Self {
            accepted: value.clone(),
            draft: value,
            undo: VecDeque::new(),
            redo: Vec::new(),
            limits,
        })
    }
    pub fn accepted(&self) -> Option<&Bitmap> {
        self.accepted.as_ref()
    }
    pub fn draft(&self) -> Option<&Bitmap> {
        self.draft.as_ref()
    }
    pub fn is_dirty(&self) -> bool {
        self.accepted != self.draft
    }
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
    pub fn retained_bytes(&self) -> usize {
        unique_bytes(
            self.accepted
                .iter()
                .chain(self.draft.iter())
                .chain(self.undo.iter().flatten())
                .chain(self.redo.iter().flatten()),
        )
    }
    fn preflight_new(&self, bytes: usize) -> Result<(), ImageError> {
        let retained = unique_bytes(
            self.accepted.iter().chain(self.draft.iter()).chain(
                self.undo
                    .iter()
                    .skip(usize::from(self.undo.len() == self.limits.history_states))
                    .flatten(),
            ),
        );
        if retained
            .checked_add(bytes)
            .is_none_or(|n| n > self.limits.retained_bytes)
        {
            return Err(ImageError::Limit(
                "image edit would exceed retained history pixel budget",
            ));
        }
        Ok(())
    }
    pub fn set_draft(&mut self, value: Option<Bitmap>) -> Result<(), ImageError> {
        if value == self.draft {
            return Ok(());
        }
        let bytes = unique_bytes(
            self.accepted
                .iter()
                .chain(self.draft.iter())
                .chain(
                    self.undo
                        .iter()
                        .skip(usize::from(self.undo.len() == self.limits.history_states))
                        .flatten(),
                )
                .chain(value.iter()),
        );
        if bytes > self.limits.retained_bytes {
            return Err(ImageError::Limit(
                "image edit would exceed retained history pixel budget",
            ));
        }
        if self.undo.len() == self.limits.history_states {
            self.undo.pop_front();
        }
        self.undo.push_back(self.draft.take());
        self.draft = value;
        self.redo.clear();
        Ok(())
    }
    pub fn crop(&mut self, crop: PixelCrop) -> Result<(), ImageError> {
        let image = self
            .draft
            .as_ref()
            .ok_or_else(|| ImageError::Invalid("No image to crop".into()))?;
        crop.validate(image)?;
        if crop.x == 0
            && crop.y == 0
            && crop.width == image.width()
            && crop.height == image.height()
        {
            return Ok(());
        }
        self.preflight_new(crop.width as usize * crop.height as usize * 4)?;
        let value = crop_bitmap(image, crop)?;
        self.set_draft(Some(value))
    }
    pub fn rotate(&mut self, turn: QuarterTurn) -> Result<(), ImageError> {
        let image = self
            .draft
            .as_ref()
            .ok_or_else(|| ImageError::Invalid("No image to rotate".into()))?;
        self.preflight_new(image.pixels().len())?;
        let value = rotate_bitmap(image, turn)?;
        self.set_draft(Some(value))
    }
    pub fn undo(&mut self) -> bool {
        if let Some(previous) = self.undo.pop_back() {
            self.redo.push(std::mem::replace(&mut self.draft, previous));
            true
        } else {
            false
        }
    }
    pub fn redo(&mut self) -> bool {
        if let Some(next) = self.redo.pop() {
            self.undo
                .push_back(std::mem::replace(&mut self.draft, next));
            true
        } else {
            false
        }
    }
    pub fn apply(&mut self) -> bool {
        let changed = self.is_dirty();
        self.accepted = self.draft.clone();
        self.undo.clear();
        self.redo.clear();
        changed
    }
    pub fn cancel(&mut self) {
        self.draft = self.accepted.clone();
        self.undo.clear();
        self.redo.clear();
    }
    pub fn replace(
        &mut self,
        value: Option<Bitmap>,
        discard_draft: bool,
    ) -> Result<(), ImageError> {
        if !discard_draft && self.is_dirty() {
            return Err(ImageError::Invalid(
                "Image draft conflicts with external replacement".into(),
            ));
        }
        let candidate = Self::new(value, self.limits)?;
        *self = candidate;
        Ok(())
    }
}
fn unique_bytes<'a>(values: impl Iterator<Item = &'a Bitmap>) -> usize {
    let mut seen = HashSet::new();
    values
        .filter(|v| seen.insert(v.id()))
        .map(|v| v.pixels().len())
        .sum()
}
#[cfg(test)]
mod tests;
