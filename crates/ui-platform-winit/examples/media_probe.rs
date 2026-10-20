//! Explicit local-file decoding probe. Does not open a window or audio device.
use rust_desktop_ui_core::media::MediaProvider;
use std::{path::PathBuf, sync::atomic::AtomicBool, time::Duration};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let path = PathBuf::from(args.next().ok_or("Supply an owned video fixture path")?);
    let output = args.next().map(PathBuf::from);
    let cancel = AtomicBool::new(false);
    let mut provider = rust_desktop_ui_platform_winit::media::open_video_file(&path, &cancel)?;
    println!("provider: {:?}", provider.info());
    let duration = provider.info().duration.ok_or("No duration")?;
    let positions = [Duration::ZERO, duration / 2, Duration::ZERO, duration / 3];
    for (index, position) in positions.into_iter().enumerate() {
        let frame = provider
            .frame_at(position, &cancel)?
            .ok_or("Unexpected end of stream")?;
        assert!(frame.position >= position);
        println!(
            "frame {index}: requested={position:?} actual={:?} {}x{} bytes={}",
            frame.position,
            frame.bitmap.width(),
            frame.bitmap.height(),
            frame.bitmap.pixels().len()
        );
        if index == 0
            && let Some(path) = &output
        {
            let mut bytes = format!(
                "P6\n{} {}\n255\n",
                frame.bitmap.width(),
                frame.bitmap.height()
            )
            .into_bytes();
            for pixel in frame.bitmap.pixels().chunks_exact(4) {
                bytes.extend_from_slice(&pixel[..3]);
            }
            std::fs::write(path, bytes)?;
        }
    }
    cancel.store(true, std::sync::atomic::Ordering::Release);
    assert!(provider.frame_at(Duration::ZERO, &cancel).is_err());
    drop(provider);
    // Reopen verifies independent decoder state and balanced runtime lifetime.
    cancel.store(false, std::sync::atomic::Ordering::Release);
    let mut second = rust_desktop_ui_platform_winit::media::open_video_file(&path, &cancel)?;
    assert!(MediaProvider::frame_at(&mut *second, Duration::ZERO, &cancel)?.is_some());
    println!("seek/cancel/reopen: PASS; audio/camera/window actions: none");
    Ok(())
}
