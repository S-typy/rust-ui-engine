//! Media provider boundary. Decoders and devices run on a host-owned worker.
use crate::Bitmap;
use std::{sync::atomic::AtomicBool, time::Duration};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MediaError(pub String);
impl std::fmt::Display for MediaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for MediaError {}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MediaCapabilities {
    pub seek: bool,
    pub capture: bool,
    pub volume: bool,
    pub live: bool,
}
#[derive(Clone, Debug)]
pub struct MediaInfo {
    pub title: String,
    pub duration: Option<Duration>,
    pub capabilities: MediaCapabilities,
}
#[derive(Clone, Debug)]
pub struct MediaFrame {
    pub position: Duration,
    pub bitmap: Bitmap,
}
/// Providers are constructed, used and dropped on one worker thread.
/// No native window/UI operations are permitted. Cancellation must be checked
/// between blocking operations; implementations must document their I/O bounds.
/// `frame_at` returns a frame near the requested time at provider granularity.
/// A live source ignores position; None denotes end of stream, not a temporary gap.
pub trait MediaProvider {
    fn info(&self) -> MediaInfo;
    fn frame_at(
        &mut self,
        position: Duration,
        cancelled: &AtomicBool,
    ) -> Result<Option<MediaFrame>, MediaError>;
    fn set_volume(&mut self, _volume: f32) -> Result<(), MediaError> {
        Err(MediaError("Volume is unavailable".into()))
    }
}

/// An explicit finite sequence provider, useful for animations and deterministic
/// playback tests. Pixels are shared; at most 128 MiB of unique frames is accepted.
pub struct ImageSequenceProvider {
    frames: Vec<Bitmap>,
    interval: Duration,
    title: String,
}
impl ImageSequenceProvider {
    pub fn new(
        title: impl Into<String>,
        frames: Vec<Bitmap>,
        interval: Duration,
    ) -> Result<Self, MediaError> {
        let title = title.into();
        let mut ids = std::collections::BTreeSet::new();
        let bytes: usize = frames
            .iter()
            .filter(|f| ids.insert(f.id()))
            .map(|f| f.pixels().len())
            .sum();
        if frames.is_empty()
            || frames.len() > 3600
            || bytes > 128 * 1024 * 1024
            || interval < Duration::from_millis(10)
            || interval > Duration::from_secs(60)
            || title.len() > 1024
        {
            return Err(MediaError("Invalid image sequence limits".into()));
        }
        Ok(Self {
            frames,
            interval,
            title,
        })
    }
}
impl MediaProvider for ImageSequenceProvider {
    fn info(&self) -> MediaInfo {
        MediaInfo {
            title: self.title.clone(),
            duration: Some(self.interval * self.frames.len() as u32),
            capabilities: MediaCapabilities {
                seek: true,
                capture: true,
                ..Default::default()
            },
        }
    }
    fn frame_at(
        &mut self,
        position: Duration,
        cancelled: &AtomicBool,
    ) -> Result<Option<MediaFrame>, MediaError> {
        if cancelled.load(std::sync::atomic::Ordering::Acquire) {
            return Err(MediaError("Cancelled".into()));
        }
        let index = (position.as_nanos() / self.interval.as_nanos()) as usize;
        Ok(self.frames.get(index).map(|bitmap| MediaFrame {
            position: self.interval * index as u32,
            bitmap: bitmap.clone(),
        }))
    }
}
