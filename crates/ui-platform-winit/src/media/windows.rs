use rust_desktop_ui_core::{Bitmap, media::*};
use std::{
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};
use windows::{
    Win32::{
        Media::MediaFoundation::*,
        System::Com::{
            COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize,
            StructuredStorage::{PROPVARIANT, PropVariantToUInt64},
        },
    },
    core::{GUID, Interface, PCWSTR},
};
const VIDEO: u32 = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;
fn error(e: impl ToString) -> MediaError {
    MediaError(e.to_string())
}
struct Runtime;
impl Runtime {
    fn new() -> Result<Self, MediaError> {
        // SAFETY: this is a dedicated worker with balanced COM/MF lifetimes.
        unsafe {
            CoInitializeEx(None, COINIT_MULTITHREADED)
                .ok()
                .map_err(error)?;
            if let Err(e) = MFStartup(MF_VERSION, MFSTARTUP_FULL) {
                CoUninitialize();
                return Err(error(e));
            }
        }
        Ok(Self)
    }
}
impl Drop for Runtime {
    fn drop(&mut self) {
        unsafe {
            let _ = MFShutdown();
            CoUninitialize();
        }
    }
}
struct Video {
    reader: IMFSourceReader,
    _runtime: Runtime,
    info: MediaInfo,
    last: Option<Duration>,
}
pub(super) fn open(
    path: &Path,
    cancelled: &AtomicBool,
) -> Result<Box<dyn MediaProvider>, MediaError> {
    if cancelled.load(Ordering::Acquire) {
        return Err(error("Cancelled"));
    }
    if !path.is_file() {
        return Err(error("Expected a local video file"));
    }
    let path = path.canonicalize().map_err(error)?;
    let name = path.as_os_str().to_string_lossy();
    if name.starts_with("\\\\?\\UNC\\") {
        return Err(error("Network video paths are unsupported"));
    }
    // The source resolver accepts a DOS path, not Rust's canonical \\?\ prefix.
    let mut wide: Vec<u16> =
        std::os::windows::ffi::OsStrExt::encode_wide(path.as_os_str()).collect();
    if wide.starts_with(&[92, 92, 63, 92]) {
        wide.drain(..4);
    }
    wide.push(0);
    let runtime = Runtime::new()?;
    // SAFETY: attributes and media types are owned COM values on the initialized
    // worker; strings are terminated and valid for the duration of the call.
    unsafe {
        let mut attributes = None;
        MFCreateAttributes(&mut attributes, 2).map_err(error)?;
        let attributes = attributes.ok_or_else(|| error("No media attributes"))?;
        attributes
            .SetUINT32(&MF_SOURCE_READER_ENABLE_VIDEO_PROCESSING, 1)
            .map_err(error)?;
        let reader =
            MFCreateSourceReaderFromURL(PCWSTR(wide.as_ptr()), &attributes).map_err(error)?;
        reader
            .SetStreamSelection(MF_SOURCE_READER_ALL_STREAMS.0 as u32, false)
            .map_err(error)?;
        reader.SetStreamSelection(VIDEO, true).map_err(error)?;
        let media = MFCreateMediaType().map_err(error)?;
        media
            .SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)
            .map_err(error)?;
        media
            .SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_RGB32)
            .map_err(error)?;
        reader
            .SetCurrentMediaType(VIDEO, None, &media)
            .map_err(error)?;
        dimensions(&reader.GetCurrentMediaType(VIDEO).map_err(error)?)?;
        let duration = reader
            .GetPresentationAttribute(MF_SOURCE_READER_MEDIASOURCE.0 as u32, &MF_PD_DURATION)
            .map_err(error)?;
        let ticks = PropVariantToUInt64(&duration).map_err(error)?;
        if ticks == 0 || ticks > 7 * 24 * 3600 * 10_000_000 {
            return Err(error("Video duration outside 0..7 days"));
        }
        if cancelled.load(Ordering::Acquire) {
            return Err(error("Cancelled"));
        }
        Ok(Box::new(Video {
            reader,
            _runtime: runtime,
            info: MediaInfo {
                title: path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .chars()
                    .take(256)
                    .collect(),
                duration: Some(Duration::from_nanos(ticks * 100)),
                capabilities: MediaCapabilities {
                    seek: true,
                    capture: true,
                    ..Default::default()
                },
            },
            last: None,
        }))
    }
}
fn dimensions(media: &IMFMediaType) -> Result<(u32, u32), MediaError> {
    let packed = unsafe { media.GetUINT64(&MF_MT_FRAME_SIZE) }.map_err(error)?;
    let (width, height) = ((packed >> 32) as u32, packed as u32);
    if width == 0
        || height == 0
        || width > 8192
        || height > 8192
        || width as u64 * height as u64 * 4 > 64 * 1024 * 1024
    {
        return Err(error("Video frame exceeds bitmap limits"));
    }
    Ok((width, height))
}
impl MediaProvider for Video {
    fn info(&self) -> MediaInfo {
        self.info.clone()
    }
    fn frame_at(
        &mut self,
        position: Duration,
        cancelled: &AtomicBool,
    ) -> Result<Option<MediaFrame>, MediaError> {
        if self.info.duration.is_some_and(|d| position >= d) {
            return Ok(None);
        }
        // Seek only discontinuities. Sequential playback preserves decoder state.
        if self.last.map_or(!position.is_zero(), |p| {
            position < p || position.saturating_sub(p) > Duration::from_millis(500)
        }) {
            let ticks = i64::try_from(position.as_nanos() / 100).map_err(error)?;
            unsafe {
                self.reader
                    .SetCurrentPosition(&GUID::zeroed(), &PROPVARIANT::from(ticks))
            }
            .map_err(error)?;
        }
        for _ in 0..600 {
            if cancelled.load(Ordering::Acquire) {
                return Err(error("Cancelled"));
            }
            let mut flags = 0;
            let mut timestamp = 0;
            let mut sample = None;
            unsafe {
                self.reader.ReadSample(
                    VIDEO,
                    0,
                    None,
                    Some(&mut flags),
                    Some(&mut timestamp),
                    Some(&mut sample),
                )
            }
            .map_err(error)?;
            if flags & MF_SOURCE_READERF_ERROR.0 as u32 != 0 {
                return Err(error("Video source error"));
            }
            if flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 != 0 {
                return Ok(None);
            }
            let Some(sample) = sample else { continue };
            let time = Duration::from_nanos(
                (timestamp.max(0) as u64)
                    .checked_mul(100)
                    .ok_or_else(|| error("Video timestamp overflow"))?,
            );
            if time < position {
                continue;
            }
            let media = unsafe { self.reader.GetCurrentMediaType(VIDEO) }.map_err(error)?;
            if unsafe { media.GetGUID(&MF_MT_SUBTYPE) }.map_err(error)? != MFVideoFormat_RGB32 {
                return Err(error("Video changed to unsupported pixel format"));
            }
            let (width, height) = dimensions(&media)?;
            if unsafe { media.GetUINT32(&MF_MT_VIDEO_ROTATION) }.unwrap_or(0) != 0 {
                return Err(error(
                    "Rotated video requires an explicit transform provider",
                ));
            }
            let buffer = unsafe { sample.ConvertToContiguousBuffer() }.map_err(error)?;
            let bitmap = read_bitmap(&buffer, &media, width, height)?;
            self.last = Some(time);
            return Ok(Some(MediaFrame {
                position: time,
                bitmap,
            }));
        }
        Err(error("Video sample search budget exceeded"))
    }
}
fn read_bitmap(
    buffer: &IMFMediaBuffer,
    media: &IMFMediaType,
    width: u32,
    height: u32,
) -> Result<Bitmap, MediaError> {
    if let Ok(two) = buffer.cast::<IMF2DBuffer2>() {
        let mut top = std::ptr::null_mut();
        let mut start = std::ptr::null_mut();
        let mut pitch = 0;
        let mut len = 0;
        unsafe {
            two.Lock2DSize(
                MF2DBuffer_LockFlags_Read,
                &mut top,
                &mut pitch,
                &mut start,
                &mut len,
            )
        }
        .map_err(error)?;
        struct Unlock(IMF2DBuffer2);
        impl Drop for Unlock {
            fn drop(&mut self) {
                unsafe {
                    let _ = self.0.Unlock2D();
                }
            }
        }
        let _unlock = Unlock(two);
        if start.is_null() || top.is_null() {
            return Err(error("Empty video buffer"));
        }
        let offset = (top as usize)
            .checked_sub(start as usize)
            .ok_or_else(|| error("Video scanline outside buffer"))?;
        // SAFETY: Lock2DSize exposes the returned byte range until Unlock2D.
        let bytes = unsafe { std::slice::from_raw_parts(start, len as usize) };
        return convert(bytes, offset, pitch, width, height);
    }
    let mut start = std::ptr::null_mut();
    let mut len = 0;
    unsafe { buffer.Lock(&mut start, None, Some(&mut len)) }.map_err(error)?;
    struct Unlock(IMFMediaBuffer);
    impl Drop for Unlock {
        fn drop(&mut self) {
            unsafe {
                let _ = self.0.Unlock();
            }
        }
    }
    let _unlock = Unlock(buffer.clone());
    if start.is_null() {
        return Err(error("Empty video buffer"));
    }
    let stride = unsafe { media.GetUINT32(&MF_MT_DEFAULT_STRIDE) }
        .map(|n| n as i32)
        .or_else(|_| unsafe { MFGetStrideForBitmapInfoHeader(MFVideoFormat_RGB32.data1, width) })
        .map_err(error)?;
    let offset = if stride < 0 {
        stride.unsigned_abs() as usize * (height - 1) as usize
    } else {
        0
    };
    let bytes = unsafe { std::slice::from_raw_parts(start, len as usize) };
    convert(bytes, offset, stride, width, height)
}
fn convert(
    bytes: &[u8],
    top: usize,
    stride: i32,
    width: u32,
    height: u32,
) -> Result<Bitmap, MediaError> {
    let row = width as usize * 4;
    if (stride.unsigned_abs() as usize) < row {
        return Err(error("Video stride smaller than row"));
    }
    let mut rgba = vec![0; row * height as usize];
    for y in 0..height as usize {
        let offset = top as i64 + y as i64 * stride as i64;
        if offset < 0 {
            return Err(error("Video scanline outside buffer"));
        }
        let input = bytes
            .get(offset as usize..offset as usize + row)
            .ok_or_else(|| error("Truncated video scanline"))?;
        for (src, dst) in input
            .chunks_exact(4)
            .zip(rgba[y * row..(y + 1) * row].chunks_exact_mut(4))
        {
            dst.copy_from_slice(&[src[2], src[1], src[0], 255]);
        }
    }
    Bitmap::new_rgba8(width, height, rgba).map_err(error)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stride_and_channel_conversion() {
        let bytes = [0, 0, 255, 0, 9, 9, 9, 9, 255, 0, 0, 0, 9, 9, 9, 9];
        let b = convert(&bytes, 8, -8, 1, 2).unwrap();
        assert_eq!(b.pixels(), &[0, 0, 255, 255, 255, 0, 0, 255]);
        assert!(convert(&bytes, 0, -8, 1, 2).is_err());
        assert!(convert(&bytes[..10], 0, 8, 1, 2).is_err());
    }
}
