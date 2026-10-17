use super::*;
use windows_sys::Win32::{
    Foundation::{GlobalFree, HGLOBAL},
    System::{DataExchange::*, Memory::*},
};
struct Open;
impl Open {
    fn new(hwnd: HWND) -> ServiceResult<Self> {
        // SAFETY: HWND belongs to the calling UI thread, no delayed rendering.
        if unsafe { OpenClipboard(hwnd) } == 0 {
            Err(ServiceError::new(
                ServiceErrorKind::Busy,
                "Clipboard is busy",
            ))
        } else {
            Ok(Self)
        }
    }
}
impl Drop for Open {
    fn drop(&mut self) {
        unsafe {
            CloseClipboard();
        }
    }
}
struct Memory(HGLOBAL);
impl Memory {
    fn copy(bytes: &[u8]) -> ServiceResult<Self> {
        // SAFETY: bounded allocation; copy only while locked; movable memory is
        // transferred to Windows only after successful SetClipboardData.
        let memory = Self(unsafe { GlobalAlloc(GMEM_MOVEABLE | GMEM_ZEROINIT, bytes.len()) });
        if memory.0.is_null() {
            return Err(native("Cannot allocate clipboard memory"));
        }
        let target = unsafe { GlobalLock(memory.0) };
        if target.is_null() {
            return Err(native("Cannot lock clipboard memory"));
        }
        unsafe {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), target as *mut u8, bytes.len());
            GlobalUnlock(memory.0);
        }
        Ok(memory)
    }
}
impl Drop for Memory {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                GlobalFree(self.0);
            }
        }
    }
}
fn html_format() -> ServiceResult<u32> {
    let id = unsafe { RegisterClipboardFormatW(wide("HTML Format").as_ptr()) };
    if id == 0 {
        Err(native("Cannot register HTML clipboard format"))
    } else {
        Ok(id)
    }
}
pub(super) fn write(hwnd: HWND, value: &ClipboardContent) -> ServiceResult<()> {
    value.validate()?;
    let mut formats = Vec::new();
    if let Some(text) = &value.text {
        let bytes: Vec<u8> = text
            .encode_utf16()
            .chain(Some(0))
            .flat_map(u16::to_le_bytes)
            .collect();
        formats.push((13, Memory::copy(&bytes)?));
    }
    if let Some(html) = &value.html {
        formats.push((html_format()?, Memory::copy(&codec::html_encode(html))?));
    }
    if let Some(image) = &value.image {
        formats.push((17, Memory::copy(&codec::dib_encode(image))?));
    }
    let _open = Open::new(hwnd)?;
    if unsafe { EmptyClipboard() } == 0 {
        return Err(native("Cannot empty clipboard"));
    }
    for (written, (format, mut memory)) in formats.into_iter().enumerate() {
        if unsafe { SetClipboardData(format, memory.0) }.is_null() {
            let mut error = native("Clipboard write failed after clearing previous contents");
            error.clipboard_formats_written = written;
            return Err(error);
        }
        memory.0 = std::ptr::null_mut();
    }
    Ok(())
}
pub(super) fn read(hwnd: HWND, format: ClipboardFormat) -> ServiceResult<ClipboardContent> {
    let (native_format, limit) = match format {
        ClipboardFormat::Text => (13, 2_097_154),
        ClipboardFormat::Html => (html_format()?, 4_195_328),
        ClipboardFormat::Image => (17, 16_777_340),
    };
    let _open = Open::new(hwnd)?;
    if unsafe { IsClipboardFormatAvailable(native_format) } == 0 {
        return Err(ServiceError::new(
            ServiceErrorKind::Unsupported,
            "Requested clipboard format is absent",
        ));
    }
    let handle = unsafe { GetClipboardData(native_format) };
    if handle.is_null() {
        return Err(native("Cannot read clipboard data"));
    }
    let size = unsafe { GlobalSize(handle) };
    if size == 0 || size > limit + 65_536 {
        return Err(invalid("Clipboard data exceeds its format bound"));
    }
    let data = unsafe { GlobalLock(handle) };
    if data.is_null() {
        return Err(native("Cannot lock clipboard data"));
    }
    // Copy while clipboard ownership and global lock are held. No pointers escape.
    let bytes = unsafe { std::slice::from_raw_parts(data as *const u8, size.min(limit)).to_vec() };
    unsafe {
        GlobalUnlock(handle);
    }
    let mut result = ClipboardContent::default();
    match format {
        ClipboardFormat::Text => {
            if bytes.len() % 2 != 0 {
                return Err(invalid("Misaligned clipboard UTF-16"));
            }
            let units: Vec<u16> = bytes
                .chunks_exact(2)
                .map(|b| u16::from_le_bytes([b[0], b[1]]))
                .collect();
            let end = units
                .iter()
                .position(|v| *v == 0)
                .ok_or_else(|| invalid("Unterminated clipboard UTF-16"))?;
            result.text = Some(
                String::from_utf16(&units[..end])
                    .map_err(|_| invalid("Invalid clipboard UTF-16"))?,
            );
        }
        ClipboardFormat::Html => result.html = Some(codec::html_decode(&bytes)?),
        ClipboardFormat::Image => result.image = Some(codec::dib_decode(&bytes)?),
    }
    result.validate()?;
    Ok(result)
}
