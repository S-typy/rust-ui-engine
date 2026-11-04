//! Real clipboard checks in a separate, noninteractive window station.
use super::*;
use std::{ffi::c_void, process::Command, time::Duration};

type Handle = *mut c_void;
const CHILD: &str = "RUST_UI_ISOLATED_CLIPBOARD_TEST";
const TEST: &str = "services::windows::clipboard::native_tests::isolated_native_roundtrip";

#[link(name = "user32")]
unsafe extern "system" {
    fn CreateWindowStationW(name: *const u16, flags: u32, access: u32, security: Handle) -> Handle;
    fn GetProcessWindowStation() -> Handle;
    fn SetProcessWindowStation(station: Handle) -> i32;
    fn CloseWindowStation(station: Handle) -> i32;
    fn CreateDesktopW(
        name: *const u16,
        device: *const u16,
        mode: Handle,
        flags: u32,
        access: u32,
        security: Handle,
    ) -> Handle;
    fn GetThreadDesktop(thread: u32) -> Handle;
    fn SetThreadDesktop(desktop: Handle) -> i32;
    fn CloseDesktop(desktop: Handle) -> i32;
    fn GetUserObjectInformationW(
        object: Handle,
        index: i32,
        data: Handle,
        len: u32,
        needed: *mut u32,
    ) -> i32;
}
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetCurrentThreadId() -> u32;
}

fn object_name(handle: Handle) -> String {
    let mut name = [0_u16; 256];
    let mut needed = 0;
    // SAFETY: handle belongs to this process; buffer and output sizes are exact.
    assert_ne!(
        unsafe {
            GetUserObjectInformationW(
                handle,
                2,
                name.as_mut_ptr().cast(),
                std::mem::size_of_val(&name) as u32,
                &mut needed,
            )
        },
        0
    );
    String::from_utf16(&name[..name.iter().position(|c| *c == 0).unwrap()]).unwrap()
}

struct Station {
    old_station: Handle,
    old_desktop: Handle,
    station: Handle,
    desktop: Handle,
    window: HWND,
}
impl Station {
    fn interactive() -> Self {
        // SAFETY: invoked only by the explicitly destructive ignored fixture.
        // No station/desktop is switched and this hidden window owns all writes.
        unsafe {
            let window = CreateWindowExW(
                0,
                wide("STATIC").as_ptr(),
                wide("Clipboard fixture").as_ptr(),
                0,
                0,
                0,
                1,
                1,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null(),
            );
            assert!(!window.is_null());
            Self {
                old_station: GetProcessWindowStation(),
                old_desktop: GetThreadDesktop(GetCurrentThreadId()),
                station: std::ptr::null_mut(),
                desktop: std::ptr::null_mut(),
                window,
            }
        }
    }

    fn new(name: &str) -> Self {
        // SAFETY: only the dedicated child test process changes station. It has
        // created no GUI objects. Existing handles are borrowed and never closed.
        let mut own = unsafe {
            Self {
                old_station: GetProcessWindowStation(),
                old_desktop: GetThreadDesktop(GetCurrentThreadId()),
                station: std::ptr::null_mut(),
                desktop: std::ptr::null_mut(),
                window: std::ptr::null_mut(),
            }
        };
        unsafe {
            // CWF_CREATE_ONLY rejects an existing station. An unprivileged
            // account may only create the OS-generated station name. Neither
            // branch opens an existing station; all payloads are synthetic.
            own.station = CreateWindowStationW(wide(name).as_ptr(), 1, 0x37f, std::ptr::null_mut());
            if own.station.is_null() && std::io::Error::last_os_error().raw_os_error() == Some(5) {
                own.station =
                    CreateWindowStationW(std::ptr::null(), 1, 0x37f, std::ptr::null_mut());
            }
            assert!(
                !own.station.is_null(),
                "Cannot create isolated station: {}",
                std::io::Error::last_os_error()
            );
            let created_name = object_name(own.station);
            assert!(!created_name.is_empty());
            assert!(!created_name.eq_ignore_ascii_case("WinSta0"));
            assert_ne!(created_name, object_name(own.old_station));
            assert_ne!(SetProcessWindowStation(own.station), 0);
            assert_eq!(object_name(GetProcessWindowStation()), created_name);
            // DESKTOP_CREATEWINDOW | DESKTOP_READOBJECTS | DESKTOP_WRITEOBJECTS.
            own.desktop = CreateDesktopW(
                wide("ClipboardFixture").as_ptr(),
                std::ptr::null(),
                std::ptr::null_mut(),
                0,
                0x83,
                std::ptr::null_mut(),
            );
            assert!(!own.desktop.is_null());
            assert_ne!(SetThreadDesktop(own.desktop), 0);
            own.window = CreateWindowExW(
                0,
                wide("STATIC").as_ptr(),
                wide("Clipboard fixture").as_ptr(),
                0,
                0,
                0,
                1,
                1,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null(),
            );
            assert!(!own.window.is_null());
        }
        own
    }
}
impl Drop for Station {
    fn drop(&mut self) {
        // SAFETY: destroy only owned objects, restore borrowed assignments first.
        // This desktop is never switched onto the physical display.
        unsafe {
            if !self.window.is_null() {
                DestroyWindow(self.window);
            }
            SetThreadDesktop(self.old_desktop);
            SetProcessWindowStation(self.old_station);
            if !self.desktop.is_null() {
                CloseDesktop(self.desktop);
            }
            if !self.station.is_null() {
                CloseWindowStation(self.station);
            }
        }
    }
}

#[test]
#[ignore = "creates an isolated Windows clipboard in an owned child process"]
fn isolated_native_roundtrip() {
    if let Ok(name) = std::env::var(CHILD) {
        assert!(name.starts_with("RustUiClipboardFixture-"));
        let station = Station::new(&name);
        verify_native_roundtrip(station);
        return;
    }
    let before = unsafe { GetClipboardSequenceNumber() };
    let name = format!(
        "RustUiClipboardFixture-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            TEST,
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(CHILD, name)
        .spawn()
        .unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if std::time::Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("isolated clipboard child exceeded 30 seconds");
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    assert!(status.success(), "isolated child failed: {status}");
    assert_eq!(
        unsafe { GetClipboardSequenceNumber() },
        before,
        "interactive clipboard changed during the fixture; no content was read or restored"
    );
}

fn retry_busy<T>(mut operation: impl FnMut() -> ServiceResult<T>) -> ServiceResult<T> {
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    loop {
        match operation() {
            Err(error)
                if error.kind == ServiceErrorKind::Busy
                    && error.clipboard_formats_written == 0
                    && std::time::Instant::now() < deadline =>
            {
                std::thread::sleep(Duration::from_millis(10));
            }
            result => return result,
        }
    }
}

fn verify_native_roundtrip(station: Station) {
    // Other clipboard listeners may briefly own the OS lock after publication.
    // Busy is the documented retryable result, not permission to spin forever
    // or retry partially successful writes.
    let write = |hwnd, value: &ClipboardContent| retry_busy(|| super::write(hwnd, value));
    let read = |hwnd, format| retry_busy(|| super::read(hwnd, format));
    let hwnd = station.window;
    let value = ClipboardContent {
        text: Some("Строка\r\n日本語 한글 🦀 e\u{301}".into()),
        html: Some("<p>Привет <b>日本語 🦀</b></p>".into()),
        image: Some(ClipboardImage {
            width: 2,
            height: 2,
            rgba: vec![255, 0, 0, 255, 0, 255, 0, 128, 0, 0, 255, 0, 7, 11, 19, 220],
        }),
    };
    write(hwnd, &value).unwrap();
    assert_eq!(read(hwnd, ClipboardFormat::Text).unwrap().text, value.text);
    assert_eq!(read(hwnd, ClipboardFormat::Html).unwrap().html, value.html);
    assert_eq!(
        read(hwnd, ClipboardFormat::Image).unwrap().image,
        value.image
    );
    // Native format identifiers are simultaneously present, not just model data.
    unsafe {
        assert_ne!(IsClipboardFormatAvailable(13), 0);
        assert_ne!(IsClipboardFormatAvailable(html_format().unwrap()), 0);
        assert_ne!(IsClipboardFormatAvailable(17), 0);
    }
    // Invalid input must leave all previously published formats intact.
    let sequence = unsafe { GetClipboardSequenceNumber() };
    for invalid_value in [
        ClipboardContent::default(),
        ClipboardContent {
            text: Some("bad\0text".into()),
            ..Default::default()
        },
        ClipboardContent {
            image: Some(ClipboardImage {
                width: 2,
                height: 1,
                rgba: vec![0; 7],
            }),
            ..Default::default()
        },
    ] {
        let error = write(hwnd, &invalid_value).unwrap_err();
        assert_eq!(error.kind, ServiceErrorKind::Invalid);
        assert_eq!(error.clipboard_formats_written, 0);
        assert_eq!(unsafe { GetClipboardSequenceNumber() }, sequence);
    }
    assert_eq!(read(hwnd, ClipboardFormat::Html).unwrap().html, value.html);
    assert_eq!(
        read(hwnd, ClipboardFormat::Image).unwrap().image,
        value.image
    );
    write(
        hwnd,
        &ClipboardContent {
            text: Some(String::new()),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(
        read(hwnd, ClipboardFormat::Text).unwrap().text,
        Some(String::new())
    );
    assert_eq!(
        read(hwnd, ClipboardFormat::Html).unwrap_err().kind,
        ServiceErrorKind::Unsupported
    );
    assert_eq!(
        read(hwnd, ClipboardFormat::Image).unwrap_err().kind,
        ServiceErrorKind::Unsupported
    );
    // Repeat replacement to exercise native ownership and release, then read
    // from a second owner window in the same isolated station.
    for _ in 0..32 {
        write(hwnd, &value).unwrap();
    }
    let peer = unsafe {
        CreateWindowExW(
            0,
            wide("STATIC").as_ptr(),
            wide("Clipboard reader").as_ptr(),
            0,
            0,
            0,
            1,
            1,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null(),
        )
    };
    assert!(!peer.is_null());
    let peer_result = read(peer, ClipboardFormat::Image);
    unsafe {
        DestroyWindow(peer);
    }
    assert_eq!(peer_result.unwrap().image, value.image);
    drop(station);
    println!(
        "NATIVE CLIPBOARD PASS: Unicode, HTML, RGBA, absence, invalid atomicity, repeated replacement, peer owner"
    );
}

/// Explicitly destructive: run this ignored test only with permission to replace
/// the interactive clipboard. It never reads or logs the previous contents.
#[test]
#[ignore = "replaces the interactive clipboard; explicit owner permission required"]
fn interactive_native_roundtrip() {
    let station = Station::interactive();
    verify_native_roundtrip(station);
}
