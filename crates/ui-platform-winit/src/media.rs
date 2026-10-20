//! File video provider. Opening and decoding must run on the media worker.
//! No audio renderer, network resolver or camera activation is performed here.
use rust_desktop_ui_core::media::{MediaError, MediaProvider};
use std::{path::Path, sync::atomic::AtomicBool};
#[cfg(target_os = "windows")]
mod windows;

pub fn open_video_file(
    path: &Path,
    cancelled: &AtomicBool,
) -> Result<Box<dyn MediaProvider>, MediaError> {
    #[cfg(target_os = "windows")]
    {
        windows::open(path, cancelled)
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (path, cancelled);
        Err(MediaError(
            "A video provider is not configured for this platform".into(),
        ))
    }
}
