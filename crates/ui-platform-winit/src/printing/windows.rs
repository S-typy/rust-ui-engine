use super::*;
use rust_desktop_ui_core::*;
use std::{
    mem::{size_of, zeroed},
    ptr::{null, null_mut},
    sync::atomic::Ordering,
};
use windows_sys::Win32::{
    Foundation::RECT,
    Graphics::{Gdi::*, Printing::*},
    Storage::Xps::*,
};

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
fn native(action: &str) -> PrintError {
    PrintError::Native(format!("{action}: {}", std::io::Error::last_os_error()))
}
struct Dc(HDC);
impl Drop for Dc {
    fn drop(&mut self) {
        unsafe {
            DeleteDC(self.0);
        }
    }
}
struct Gdi(HGDIOBJ);
impl Drop for Gdi {
    fn drop(&mut self) {
        unsafe {
            DeleteObject(self.0);
        }
    }
}
struct Job {
    dc: HDC,
    finished: bool,
}
impl Drop for Job {
    fn drop(&mut self) {
        if !self.finished {
            unsafe {
                AbortDoc(self.dc);
            }
        }
    }
}
pub(super) fn printers() -> Result<Vec<String>, PrintError> {
    unsafe {
        let (mut needed, mut count) = (0, 0);
        EnumPrintersW(
            PRINTER_ENUM_LOCAL | PRINTER_ENUM_CONNECTIONS,
            null(),
            4,
            null_mut(),
            0,
            &mut needed,
            &mut count,
        );
        if needed == 0 {
            return Ok(Vec::new());
        }
        if needed > 4 * 1024 * 1024 {
            return Err(PrintError::Invalid("printer enumeration limit".into()));
        }
        let mut storage = vec![0usize; (needed as usize).div_ceil(size_of::<usize>())];
        if EnumPrintersW(
            PRINTER_ENUM_LOCAL | PRINTER_ENUM_CONNECTIONS,
            null(),
            4,
            storage.as_mut_ptr().cast(),
            needed,
            &mut needed,
            &mut count,
        ) == 0
        {
            return Err(native("EnumPrintersW"));
        }
        if count as usize > storage.len() * size_of::<usize>() / size_of::<PRINTER_INFO_4W>() {
            return Err(native("printer count"));
        }
        let entries =
            std::slice::from_raw_parts(storage.as_ptr().cast::<PRINTER_INFO_4W>(), count as usize);
        let mut names = Vec::new();
        let low = storage.as_ptr() as usize;
        let high = low + storage.len() * size_of::<usize>();
        for entry in entries {
            let p = entry.pPrinterName;
            if p.is_null()
                || (p as usize) < low
                || (p as usize) >= high
                || !(p as usize).is_multiple_of(std::mem::align_of::<u16>())
            {
                continue;
            }
            let limit = ((high - p as usize) / 2).min(1025);
            let bytes = std::slice::from_raw_parts(p, limit);
            let Some(end) = bytes.iter().position(|c| *c == 0) else {
                return Err(native("printer name"));
            };
            names.push(String::from_utf16_lossy(&bytes[..end]));
        }
        names.sort();
        names.dedup();
        Ok(names)
    }
}
pub(super) fn print(
    document: &PrintDocument,
    ticket: &PrintTicket,
    pages: &[usize],
    cancelled: &AtomicBool,
) -> Result<PrintReceipt, PrintError> {
    if cancelled.load(Ordering::Acquire) {
        return Err(PrintError::Cancelled);
    }
    unsafe {
        let driver = wide("WINSPOOL");
        let printer = wide(&ticket.printer);
        let dc = Dc(CreateDCW(driver.as_ptr(), printer.as_ptr(), null(), null()));
        if dc.0.is_null() {
            return Err(native("CreateDCW"));
        }
        let dpi_x = GetDeviceCaps(dc.0, LOGPIXELSX as i32) as f32;
        let dpi_y = GetDeviceCaps(dc.0, LOGPIXELSY as i32) as f32;
        let width = GetDeviceCaps(dc.0, HORZRES as i32) as f32;
        let height = GetDeviceCaps(dc.0, VERTRES as i32) as f32;
        if dpi_x <= 0.0 || dpi_y <= 0.0 || width <= 0.0 || height <= 0.0 {
            return Err(native("printer capabilities"));
        }
        if document.pages().iter().any(|p| !p.scene.images.is_empty())
            && GetDeviceCaps(dc.0, SHADEBLENDCAPS as i32) as u32 & SB_PIXEL_ALPHA == 0
        {
            return Err(PrintError::Unsupported(
                "printer driver lacks pixel alpha blending".into(),
            ));
        }
        if !ticket.fit_to_printable_area
            && pages.iter().any(|p| {
                let size = document.pages()[*p].size;
                size.width * dpi_x / 96.0 > width || size.height * dpi_y / 96.0 > height
            })
        {
            return Err(PrintError::Invalid(
                "page exceeds printable area at actual size".into(),
            ));
        }
        if cancelled.load(Ordering::Acquire) {
            return Err(PrintError::Cancelled);
        }
        let title = wide(document.title());
        let output = ticket.output.as_ref().map(|p| {
            use std::os::windows::ffi::OsStrExt;
            p.as_os_str()
                .encode_wide()
                .chain(Some(0))
                .collect::<Vec<_>>()
        });
        let info = DOCINFOW {
            cbSize: size_of::<DOCINFOW>() as i32,
            lpszDocName: title.as_ptr(),
            lpszOutput: output.as_ref().map_or(null(), |v| v.as_ptr()),
            lpszDatatype: null(),
            fwType: 0,
        };
        let job_id = StartDocW(dc.0, &info);
        if job_id <= 0 {
            return Err(native("StartDocW"));
        }
        let mut job = Job {
            dc: dc.0,
            finished: false,
        };
        let mut submitted = 0;
        for _ in 0..ticket.copies {
            for index in pages {
                if cancelled.load(Ordering::Acquire) {
                    return Err(PrintError::Cancelled);
                }
                let page = &document.pages()[*index];
                let factor = if ticket.fit_to_printable_area {
                    (width / (page.size.width * dpi_x / 96.0))
                        .min(height / (page.size.height * dpi_y / 96.0))
                } else {
                    1.0
                };
                let scale = Point::new(dpi_x / 96.0 * factor, dpi_y / 96.0 * factor);
                if StartPage(dc.0) <= 0 {
                    return Err(native("StartPage"));
                }
                for command in page.scene.commands() {
                    if cancelled.load(Ordering::Acquire) {
                        return Err(PrintError::Cancelled);
                    }
                    draw(dc.0, &page.scene, *command, scale)?;
                }
                if EndPage(dc.0) <= 0 {
                    return Err(native("EndPage"));
                }
                submitted += 1;
            }
        }
        if cancelled.load(Ordering::Acquire) {
            return Err(PrintError::Cancelled);
        }
        if EndDoc(dc.0) <= 0 {
            return Err(native("EndDoc"));
        }
        job.finished = true;
        Ok(PrintReceipt {
            job_id,
            pages_submitted: submitted,
        })
    }
}
fn rgb(c: Color) -> u32 {
    (c.r * 255.0).round() as u32
        | ((c.g * 255.0).round() as u32) << 8
        | ((c.b * 255.0).round() as u32) << 16
}
fn rect(r: Rect, s: Point) -> RECT {
    RECT {
        left: (r.x * s.x).round() as i32,
        top: (r.y * s.y).round() as i32,
        right: ((r.x + r.width) * s.x).round() as i32,
        bottom: ((r.y + r.height) * s.y).round() as i32,
    }
}
fn draw(dc: HDC, scene: &Scene, command: DrawCommand, scale: Point) -> Result<(), PrintError> {
    unsafe {
        let saved = SaveDC(dc);
        if saved == 0 {
            return Err(native("SaveDC"));
        }
        let result = (|| {
            match command {
                DrawCommand::Rectangle(i) => {
                    let r = &scene.rectangles[i];
                    let brush = Gdi(CreateSolidBrush(rgb(r.color)));
                    if brush.0.is_null() {
                        return Err(native("CreateSolidBrush"));
                    }
                    if FillRect(dc, &rect(r.bounds, scale), brush.0) == 0 {
                        return Err(native("FillRect"));
                    }
                }
                DrawCommand::RoundedRectangle(i) => {
                    let r = &scene.rounded_rectangles[i];
                    let clip = rect(r.clip, scale);
                    IntersectClipRect(dc, clip.left, clip.top, clip.right, clip.bottom);
                    let brush = Gdi(CreateSolidBrush(rgb(r.color)));
                    let pen = Gdi(CreatePen(
                        PS_SOLID,
                        (r.stroke_width.unwrap_or(0.0) * scale.x).round().max(1.0) as i32,
                        rgb(r.color),
                    ));
                    if brush.0.is_null() || pen.0.is_null() {
                        return Err(native("shape objects"));
                    }
                    let old_brush = SelectObject(
                        dc,
                        if r.stroke_width.is_some() {
                            GetStockObject(NULL_BRUSH)
                        } else {
                            brush.0
                        },
                    );
                    let old_pen = SelectObject(
                        dc,
                        if r.stroke_width.is_some() {
                            pen.0
                        } else {
                            GetStockObject(NULL_PEN)
                        },
                    );
                    let q = rect(r.bounds, scale);
                    let success = RoundRect(
                        dc,
                        q.left,
                        q.top,
                        q.right,
                        q.bottom,
                        (r.radius * 2.0 * scale.x).round() as i32,
                        (r.radius * 2.0 * scale.y).round() as i32,
                    );
                    SelectObject(dc, old_brush);
                    SelectObject(dc, old_pen);
                    if success == 0 {
                        return Err(native("RoundRect"));
                    }
                }
                DrawCommand::StrokePath(i) => {
                    let p = &scene.stroke_paths[i];
                    let clip = rect(p.clip, scale);
                    IntersectClipRect(dc, clip.left, clip.top, clip.right, clip.bottom);
                    let pen = Gdi(CreatePen(
                        PS_SOLID,
                        (p.width * scale.x).round().max(1.0) as i32,
                        rgb(p.color),
                    ));
                    if pen.0.is_null() {
                        return Err(native("CreatePen"));
                    }
                    let old = SelectObject(dc, pen.0);
                    let mut success = true;
                    for s in &p.segments {
                        success &= MoveToEx(
                            dc,
                            (s.start.x * scale.x).round() as i32,
                            (s.start.y * scale.y).round() as i32,
                            null_mut(),
                        ) != 0;
                        success &= LineTo(
                            dc,
                            (s.end.x * scale.x).round() as i32,
                            (s.end.y * scale.y).round() as i32,
                        ) != 0;
                    }
                    SelectObject(dc, old);
                    if !success {
                        return Err(native("LineTo"));
                    }
                }
                DrawCommand::Image(i) => draw_image(dc, &scene.images[i], scale)?,
                DrawCommand::Text(_) => return Err(PrintError::Invalid("unprepared text".into())),
            }
            Ok(())
        })();
        RestoreDC(dc, saved);
        result
    }
}
fn draw_image(dc: HDC, image: &ImagePrimitive, scale: Point) -> Result<(), PrintError> {
    unsafe {
        let b = &image.bitmap;
        let memory = Dc(CreateCompatibleDC(dc));
        if memory.0.is_null() {
            return Err(native("CreateCompatibleDC"));
        }
        let mut info: BITMAPINFO = zeroed();
        info.bmiHeader = BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: b.width() as i32,
            biHeight: -(b.height() as i32),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB,
            ..zeroed()
        };
        let mut pixels = null_mut();
        let bitmap = Gdi(CreateDIBSection(
            dc,
            &info,
            DIB_RGB_COLORS,
            &mut pixels,
            null_mut(),
            0,
        ));
        if bitmap.0.is_null() || pixels.is_null() {
            return Err(native("CreateDIBSection"));
        }
        let target = std::slice::from_raw_parts_mut(pixels.cast::<u8>(), b.pixels().len());
        for (dst, src) in target.chunks_exact_mut(4).zip(b.pixels().chunks_exact(4)) {
            let alpha = f32::from(src[3]) * image.opacity;
            dst.copy_from_slice(&[
                (f32::from(src[2]) * alpha / 255.0).round() as u8,
                (f32::from(src[1]) * alpha / 255.0).round() as u8,
                (f32::from(src[0]) * alpha / 255.0).round() as u8,
                alpha.round() as u8,
            ]);
        }
        let old = SelectObject(memory.0, bitmap.0);
        let clip = rect(image.clip, scale);
        IntersectClipRect(dc, clip.left, clip.top, clip.right, clip.bottom);
        let r = rect(image.bounds, scale);
        let src = image.source;
        let success = if r.right > r.left && r.bottom > r.top {
            AlphaBlend(
                dc,
                r.left,
                r.top,
                r.right - r.left,
                r.bottom - r.top,
                memory.0,
                src.x.round() as i32,
                src.y.round() as i32,
                src.width.round() as i32,
                src.height.round() as i32,
                BLENDFUNCTION {
                    BlendOp: AC_SRC_OVER as u8,
                    BlendFlags: 0,
                    SourceConstantAlpha: 255,
                    AlphaFormat: AC_SRC_ALPHA as u8,
                },
            )
        } else {
            1
        };
        SelectObject(memory.0, old);
        if success == 0 {
            return Err(native("AlphaBlend"));
        }
        Ok(())
    }
}
