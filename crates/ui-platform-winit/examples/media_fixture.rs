//! Generate an owned three-second video fixture with the Windows H.264 encoder.
//! Explicit output path is required; an existing file is never overwritten.
#[cfg(target_os = "windows")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use windows::{
        Win32::{Media::MediaFoundation::*, System::Com::*},
        core::PCWSTR,
    };
    let path =
        std::path::PathBuf::from(std::env::args_os().nth(1).ok_or("Supply a new .mp4 path")?);
    // Reserve the destination before handing its path to the writer.
    let reservation = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)?;
    drop(reservation);
    let path = path.canonicalize()?;
    let mut name: Vec<u16> =
        std::os::windows::ffi::OsStrExt::encode_wide(path.as_os_str()).collect();
    if name.starts_with(&[92, 92, 63, 92]) {
        name.drain(..4);
    }
    name.push(0);
    unsafe {
        CoInitializeEx(None, COINIT_MULTITHREADED).ok()?;
        struct Com;
        impl Drop for Com {
            fn drop(&mut self) {
                unsafe { CoUninitialize() }
            }
        }
        let _com = Com;
        MFStartup(MF_VERSION, MFSTARTUP_FULL)?;
        struct Mf;
        impl Drop for Mf {
            fn drop(&mut self) {
                unsafe {
                    let _ = MFShutdown();
                }
            }
        }
        let _mf = Mf;
        let writer = MFCreateSinkWriterFromURL(PCWSTR(name.as_ptr()), None, None)?;
        let output = MFCreateMediaType()?;
        output.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
        output.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_H264)?;
        output.SetUINT32(&MF_MT_AVG_BITRATE, 400_000)?;
        output.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)?;
        output.SetUINT64(&MF_MT_FRAME_SIZE, (160u64 << 32) | 96)?;
        output.SetUINT64(&MF_MT_FRAME_RATE, (10u64 << 32) | 1)?;
        output.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, (1u64 << 32) | 1)?;
        let stream = writer.AddStream(&output)?;
        let input = MFCreateMediaType()?;
        input.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
        input.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_RGB32)?;
        input.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)?;
        input.SetUINT64(&MF_MT_FRAME_SIZE, (160u64 << 32) | 96)?;
        input.SetUINT64(&MF_MT_FRAME_RATE, (10u64 << 32) | 1)?;
        input.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, (1u64 << 32) | 1)?;
        input.SetUINT32(&MF_MT_DEFAULT_STRIDE, 160 * 4)?;
        writer.SetInputMediaType(stream, &input, None)?;
        writer.BeginWriting()?;
        for n in 0..30 {
            let mut pixels = Vec::with_capacity(160 * 96 * 4);
            for y in 0..96 {
                for x in 0..160 {
                    let rgb = if y < 20 {
                        [230, 20, 10]
                    } else if (20..60).contains(&y) && (n * 4..n * 4 + 25).contains(&x) {
                        [10, 220, 30]
                    } else {
                        [10, 40, 200]
                    };
                    pixels.extend_from_slice(&[rgb[2], rgb[1], rgb[0], 0]);
                }
            }
            let buffer = MFCreateMemoryBuffer(pixels.len() as u32)?;
            let mut ptr = std::ptr::null_mut();
            buffer.Lock(&mut ptr, None, None)?;
            // SAFETY: the newly allocated buffer has exactly pixels.len() bytes.
            std::ptr::copy_nonoverlapping(pixels.as_ptr(), ptr, pixels.len());
            buffer.Unlock()?;
            buffer.SetCurrentLength(pixels.len() as u32)?;
            let sample = MFCreateSample()?;
            sample.AddBuffer(&buffer)?;
            sample.SetSampleTime(n as i64 * 1_000_000)?;
            sample.SetSampleDuration(1_000_000)?;
            writer.WriteSample(stream, &sample)?;
        }
        writer.Finalize()?;
    }
    println!("Created owned H.264 fixture: {}", path.display());
    Ok(())
}
#[cfg(not(target_os = "windows"))]
fn main() {
    eprintln!("The fixture encoder requires Windows Media Foundation.");
}
