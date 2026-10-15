//! Optional native frame extension. No control or application model lives here.

use rust_desktop_ui_core::{Rect, Scene, Size};
use std::sync::Arc;
use winit::dpi::PhysicalSize;
use winit::window::Window;

/// Application-owned title content inside the Windows native frame.
/// Coordinates use logical client pixels. List only empty draggable regions:
/// interactive controls and open popup overlays must be excluded by the app.
#[derive(Clone, Debug, PartialEq)]
pub struct TitleBar {
    pub height: f32,
    pub drag_regions: Vec<Rect>,
    pub dark: bool,
}

/// Native state supplied to the application before layout.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WindowChrome {
    pub custom_title_bar: bool,
    /// Native caption button bounds in logical client coordinates, when known.
    pub caption_buttons: Option<Rect>,
    /// Logical top resize band inside the extended client area. Interactive
    /// title content must stay below this inset. Zero while maximized/native.
    pub resize_border_top: f32,
    pub maximized: bool,
    pub focused: bool,
}

pub(crate) struct NativeChrome {
    #[cfg(target_os = "windows")]
    frame: windows::Frame,
}

impl NativeChrome {
    pub(crate) fn attach(
        window: Arc<Window>,
        bar: TitleBar,
        min_size: Option<Size>,
    ) -> Result<Option<Self>, String> {
        #[cfg(target_os = "windows")]
        {
            windows::Frame::attach(window, bar, min_size)
                .map(|frame| frame.map(|frame| Self { frame }))
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = (window, bar, min_size);
            Ok(None)
        }
    }

    pub(crate) fn state(&self, window: &Window, focused: bool) -> WindowChrome {
        #[cfg(target_os = "windows")]
        let caption_buttons = self.frame.caption_buttons();
        #[cfg(not(target_os = "windows"))]
        let caption_buttons = None;
        #[cfg(target_os = "windows")]
        let resize_border_top = self.frame.resize_border_top();
        #[cfg(not(target_os = "windows"))]
        let resize_border_top = 0.0;
        WindowChrome {
            custom_title_bar: cfg!(target_os = "windows"),
            caption_buttons,
            resize_border_top,
            maximized: window.is_maximized(),
            focused,
        }
    }

    pub(crate) fn update(&self, bar: Option<TitleBar>) {
        #[cfg(target_os = "windows")]
        self.frame.update(bar);
        #[cfg(not(target_os = "windows"))]
        let _ = bar;
    }

    pub(crate) fn request_inner_size(&self, size: PhysicalSize<u32>) -> Option<PhysicalSize<u32>> {
        #[cfg(target_os = "windows")]
        return self.frame.request_inner_size(size);
        #[cfg(not(target_os = "windows"))]
        {
            let _ = size;
            None
        }
    }

    pub(crate) fn paint(&self, scene: &mut Scene, focused: bool) {
        #[cfg(target_os = "windows")]
        self.frame.paint(scene, focused);
        #[cfg(not(target_os = "windows"))]
        let _ = (scene, focused);
    }
}

#[cfg(any(target_os = "windows", test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CaptionButton {
    Minimize,
    Maximize,
    Close,
}

#[cfg(any(target_os = "windows", test))]
#[derive(Clone, Copy)]
struct CaptionPaint {
    maximized: bool,
    dark: bool,
    focused: bool,
    hovered: bool,
    pressed: bool,
}

#[cfg(any(target_os = "windows", test))]
fn paint_caption(
    scene: &mut Scene,
    bounds: Rect,
    clip: Rect,
    button: CaptionButton,
    appearance: CaptionPaint,
) {
    use rust_desktop_ui_core::{Color, Point, StrokeSegment};
    let CaptionPaint {
        maximized,
        dark,
        focused,
        hovered,
        pressed,
    } = appearance;
    let destructive = button == CaptionButton::Close && hovered;
    if hovered {
        let color = if destructive {
            if pressed {
                Color::rgb(170, 35, 22)
            } else {
                Color::rgb(196, 43, 28)
            }
        } else {
            let mut color = if dark {
                Color::rgb(255, 255, 255)
            } else {
                Color::rgb(0, 0, 0)
            };
            color.a = if pressed { 0.14 } else { 0.08 };
            color
        };
        if let Some(fill) = bounds.intersection(clip) {
            scene.fill(fill, color);
        }
    }
    let ink = if destructive {
        Color::rgb(255, 255, 255)
    } else if !focused {
        if dark {
            Color::rgb(160, 160, 160)
        } else {
            Color::rgb(128, 128, 128)
        }
    } else if dark {
        Color::rgb(245, 245, 245)
    } else {
        Color::rgb(32, 32, 32)
    };
    let x = bounds.x + (bounds.width - 10.0) * 0.5;
    let y = bounds.y + (bounds.height - 10.0) * 0.5;
    let segment = |a, b, c, d| StrokeSegment {
        start: Point::new(x + a, y + b),
        end: Point::new(x + c, y + d),
    };
    let strokes = match button {
        CaptionButton::Minimize => vec![segment(0.0, 5.0, 10.0, 5.0)],
        CaptionButton::Close => vec![segment(0.0, 0.0, 10.0, 10.0), segment(0.0, 10.0, 10.0, 0.0)],
        CaptionButton::Maximize if maximized => vec![
            segment(0.0, 3.0, 7.0, 3.0),
            segment(7.0, 3.0, 7.0, 10.0),
            segment(7.0, 10.0, 0.0, 10.0),
            segment(0.0, 10.0, 0.0, 3.0),
            segment(3.0, 2.0, 3.0, 0.0),
            segment(3.0, 0.0, 10.0, 0.0),
            segment(10.0, 0.0, 10.0, 7.0),
            segment(10.0, 7.0, 8.0, 7.0),
        ],
        CaptionButton::Maximize => vec![
            segment(0.0, 0.0, 10.0, 0.0),
            segment(10.0, 0.0, 10.0, 10.0),
            segment(10.0, 10.0, 0.0, 10.0),
            segment(0.0, 10.0, 0.0, 0.0),
        ],
    };
    scene.stroke_path(&strokes, clip, 1.0, ink);
}

#[cfg(any(target_os = "windows", test))]
fn valid_title_bar(bar: &TitleBar) -> bool {
    bar.height.is_finite()
        && bar.height > 0.0
        && bar.height <= 1024.0
        && bar.drag_regions.iter().all(|region| region.is_valid())
}

#[cfg(any(target_os = "windows", test))]
fn is_draggable(bar: &TitleBar, x: f32, y: f32) -> bool {
    x.is_finite()
        && y.is_finite()
        && y >= 0.0
        && y < bar.height
        && bar
            .drag_regions
            .iter()
            .any(|region| region.width > 0.0 && region.height > 0.0 && region.contains(x, y))
}

#[cfg(target_os = "windows")]
mod windows {
    use super::*;
    use std::{
        cell::{Cell, RefCell},
        ffi::c_void,
        mem::size_of,
        ptr,
    };
    use windows_sys::Win32::{
        Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM},
        Graphics::{
            Dwm::{
                DWMWA_CAPTION_BUTTON_BOUNDS, DWMWA_USE_IMMERSIVE_DARK_MODE, DwmDefWindowProc,
                DwmExtendFrameIntoClientArea, DwmGetWindowAttribute, DwmIsCompositionEnabled,
                DwmSetWindowAttribute,
            },
            Gdi::{InvalidateRect, ScreenToClient},
        },
        UI::{
            Controls::MARGINS,
            HiDpi::{GetDpiForWindow, GetSystemMetricsForDpi},
            Input::KeyboardAndMouse::{TME_LEAVE, TME_NONCLIENT, TRACKMOUSEEVENT, TrackMouseEvent},
            Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
            WindowsAndMessaging::{
                GetClientRect, GetWindowRect, HTCAPTION, HTCLIENT, HTCLOSE, HTMAXBUTTON,
                HTMINBUTTON, HTTOP, HTTOPLEFT, HTTOPRIGHT, IsZoomed, MINMAXINFO, NCCALCSIZE_PARAMS,
                SM_CXPADDEDBORDER, SM_CXSIZEFRAME, SM_CYSIZEFRAME, SWP_FRAMECHANGED,
                SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SendMessageW, SetWindowPos,
                TITLEBARINFOEX, WM_CANCELMODE, WM_CAPTURECHANGED, WM_GETMINMAXINFO,
                WM_GETTITLEBARINFOEX, WM_MOUSEMOVE, WM_NCACTIVATE, WM_NCCALCSIZE, WM_NCDESTROY,
                WM_NCHITTEST, WM_NCLBUTTONDOWN, WM_NCLBUTTONUP, WM_NCMOUSELEAVE, WM_NCMOUSEMOVE,
            },
        },
    };
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

    const SUBCLASS_ID: usize = 0x5255_4946;

    struct State {
        bar: RefCell<TitleBar>,
        attached: Cell<bool>,
        min_height: Option<f32>,
        hovered: Cell<Option<CaptionButton>>,
        pressed: Cell<Option<CaptionButton>>,
    }

    pub(super) struct Frame {
        // Keep the HWND alive until the subclass is removed in Drop.
        window: Arc<Window>,
        hwnd: HWND,
        // Stable allocation, owned only here. Callback borrows never survive a
        // native call, which may synchronously reenter the subclass procedure.
        state: Box<State>,
        applied: Cell<Option<(i32, bool)>>,
    }

    impl Frame {
        pub(super) fn attach(
            window: Arc<Window>,
            bar: TitleBar,
            min_size: Option<Size>,
        ) -> Result<Option<Self>, String> {
            if !valid_title_bar(&bar) {
                return Err("Invalid custom title bar geometry".into());
            }
            let handle = window.window_handle().map_err(|error| error.to_string())?;
            let RawWindowHandle::Win32(handle) = handle.as_raw() else {
                return Ok(None);
            };
            let hwnd = handle.hwnd.get() as HWND;
            let mut composed = 0;
            // SAFETY: HWND belongs to the live winit window; outputs are initialized.
            unsafe {
                if DwmIsCompositionEnabled(&mut composed) < 0 || composed == 0 {
                    return Ok(None);
                }
            }
            let desired_client = window.inner_size();
            let frame = Self {
                window,
                hwnd,
                state: Box::new(State {
                    bar: RefCell::new(bar),
                    attached: Cell::new(false),
                    min_height: min_size.map(|size| size.height),
                    hovered: Cell::new(None),
                    pressed: Cell::new(None),
                }),
                applied: Cell::new(None),
            };
            // SAFETY: installation is on the window's event thread. The boxed
            // State remains allocated until the subclass is removed in Drop.
            unsafe {
                if SetWindowSubclass(
                    hwnd,
                    Some(subclass),
                    SUBCLASS_ID,
                    ptr::from_ref(frame.state.as_ref()) as usize,
                ) == 0
                {
                    return Err("Could not install native title bar subclass".into());
                }
            }
            frame.state.attached.set(true);
            frame.apply_appearance()?;
            // SAFETY: update only the frame, keeping window position, visibility
            // and activation unchanged. WM_NCCALCSIZE is handled by our subclass.
            let changed = unsafe {
                SetWindowPos(
                    hwnd,
                    ptr::null_mut(),
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
                )
            };
            if changed == 0 {
                frame.restore_native_frame();
                return Err("Could not extend native client geometry".into());
            }
            // Extending the top changes client height while keeping outer size.
            // Preserve the caller's requested client size before first display.
            if frame.request_inner_size(desired_client).is_none() {
                frame.restore_native_frame();
                return Err("Could not preserve requested client size".into());
            }
            Ok(Some(frame))
        }

        fn restore_native_frame(&self) {
            if self.state.attached.replace(false) {
                let margins = MARGINS::default();
                // SAFETY: rollback happens before showing the window. Remove
                // our subclass first so frame recalculation uses normal winit
                // decorations. No geometry/activation change is requested.
                unsafe {
                    RemoveWindowSubclass(self.hwnd, Some(subclass), SUBCLASS_ID);
                    DwmExtendFrameIntoClientArea(self.hwnd, &margins);
                    SetWindowPos(
                        self.hwnd,
                        ptr::null_mut(),
                        0,
                        0,
                        0,
                        0,
                        SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
                    );
                }
            }
        }

        pub(super) fn resize_border_top(&self) -> f32 {
            // SAFETY: the HWND remains owned by this Frame. This query changes
            // neither focus nor window geometry.
            unsafe {
                if IsZoomed(self.hwnd) != 0 {
                    return 0.0;
                }
                let dpi = GetDpiForWindow(self.hwnd).max(96);
                (GetSystemMetricsForDpi(SM_CYSIZEFRAME, dpi)
                    + GetSystemMetricsForDpi(SM_CXPADDEDBORDER, dpi)) as f32
                    * 96.0
                    / dpi as f32
            }
        }

        pub(super) fn request_inner_size(
            &self,
            size: PhysicalSize<u32>,
        ) -> Option<PhysicalSize<u32>> {
            let mut outer = RECT::default();
            let mut client = RECT::default();
            // SAFETY: the HWND is live on this event thread. Compute actual frame
            // insets instead of winit's standard-caption AdjustWindowRect size.
            // The flags preserve position, z-order, focus and visibility.
            unsafe {
                if GetWindowRect(self.hwnd, &mut outer) == 0
                    || GetClientRect(self.hwnd, &mut client) == 0
                {
                    return None;
                }
                let width = i64::from(size.width) + i64::from(outer.right - outer.left)
                    - i64::from(client.right - client.left);
                let height = i64::from(size.height) + i64::from(outer.bottom - outer.top)
                    - i64::from(client.bottom - client.top);
                if SetWindowPos(
                    self.hwnd,
                    ptr::null_mut(),
                    0,
                    0,
                    width.clamp(1, i64::from(i32::MAX)) as i32,
                    height.clamp(1, i64::from(i32::MAX)) as i32,
                    SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
                ) == 0
                {
                    return None;
                }
            }
            Some(self.window.inner_size())
        }

        pub(super) fn update(&self, bar: Option<TitleBar>) {
            if let Some(bar) = bar.filter(valid_title_bar) {
                *self.state.bar.borrow_mut() = bar;
            } else {
                // A malformed or withdrawn layout may not create draggable UI.
                self.state.bar.borrow_mut().drag_regions.clear();
            }
            if let Err(error) = self.apply_appearance() {
                eprintln!("Native title bar appearance: {error}");
            }
        }

        fn apply_appearance(&self) -> Result<(), String> {
            let (height, dark) = {
                let bar = self.state.bar.borrow();
                (
                    (f64::from(bar.height) * self.window.scale_factor()).ceil() as i32,
                    bar.dark,
                )
            };
            if self.applied.get() == Some((height, dark)) {
                return Ok(());
            }
            let margins = MARGINS {
                cxLeftWidth: 0,
                cxRightWidth: 0,
                cyTopHeight: height,
                cyBottomHeight: 0,
            };
            let dark_value: i32 = i32::from(dark);
            // SAFETY: valid live HWND and correctly sized initialized parameters.
            // Older Windows may reject the optional dark-mode attribute.
            unsafe {
                DwmSetWindowAttribute(
                    self.hwnd,
                    DWMWA_USE_IMMERSIVE_DARK_MODE as u32,
                    ptr::from_ref(&dark_value).cast::<c_void>(),
                    size_of::<i32>() as u32,
                );
                if DwmExtendFrameIntoClientArea(self.hwnd, &margins) < 0 {
                    return Err("DWM could not extend the title frame".into());
                }
            }
            self.applied.set(Some((height, dark)));
            Ok(())
        }

        pub(super) fn caption_buttons(&self) -> Option<Rect> {
            let mut bounds = RECT::default();
            let mut outer = RECT::default();
            // SAFETY: live HWND, initialized output structures and correct sizes.
            unsafe {
                if DwmGetWindowAttribute(
                    self.hwnd,
                    DWMWA_CAPTION_BUTTON_BOUNDS as u32,
                    ptr::from_mut(&mut bounds).cast::<c_void>(),
                    size_of::<RECT>() as u32,
                ) < 0
                    || GetWindowRect(self.hwnd, &mut outer) == 0
                    || bounds.right <= bounds.left
                    || bounds.bottom <= bounds.top
                {
                    return None;
                }
                let mut point = POINT {
                    x: outer.left + bounds.left,
                    y: outer.top + bounds.top,
                };
                if ScreenToClient(self.hwnd, &mut point) == 0 {
                    return None;
                }
                let scale = self.window.scale_factor() as f32;
                Some(Rect::new(
                    point.x as f32 / scale,
                    point.y as f32 / scale,
                    (bounds.right - bounds.left) as f32 / scale,
                    (bounds.bottom - bounds.top) as f32 / scale,
                ))
            }
        }

        pub(super) fn paint(&self, scene: &mut Scene, focused: bool) {
            let Some(union) = self.caption_buttons() else {
                return;
            };
            let mut info = TITLEBARINFOEX {
                cbSize: size_of::<TITLEBARINFOEX>() as u32,
                ..TITLEBARINFOEX::default()
            };
            // SAFETY: synchronous native query with a correctly initialized live
            // output structure. No input or application command is generated.
            unsafe {
                SendMessageW(
                    self.hwnd,
                    WM_GETTITLEBARINFOEX,
                    0,
                    ptr::from_mut(&mut info) as LPARAM,
                );
            }
            let physical = self.window.inner_size();
            let scale = self.window.scale_factor() as f32;
            let viewport = Rect::new(
                0.0,
                0.0,
                physical.width as f32 / scale,
                physical.height as f32 / scale,
            );
            let dark = self.state.bar.borrow().dark;
            let maximized = self.window.is_maximized();
            for (slot, (button, index)) in [
                (CaptionButton::Minimize, 2),
                (CaptionButton::Maximize, 3),
                (CaptionButton::Close, 5),
            ]
            .into_iter()
            .enumerate()
            {
                let native = info.rgrect[index];
                let mut position = POINT {
                    x: native.left,
                    y: native.top,
                };
                // SAFETY: live HWND and initialized point queried above.
                let rectangle = if native.right > native.left
                    && native.bottom > native.top
                    && unsafe { ScreenToClient(self.hwnd, &mut position) } != 0
                {
                    Rect::new(
                        position.x as f32 / scale,
                        position.y as f32 / scale,
                        (native.right - native.left) as f32 / scale,
                        (native.bottom - native.top) as f32 / scale,
                    )
                } else {
                    Rect::new(
                        union.x + slot as f32 * union.width / 3.0,
                        union.y,
                        union.width / 3.0,
                        union.height,
                    )
                };
                // System metrics can reserve the invisible edge outside client.
                // Keep the original center but clip the final paint to the client.
                let Some(visible) = rectangle.intersection(viewport) else {
                    continue;
                };
                paint_caption(
                    scene,
                    rectangle,
                    visible,
                    button,
                    CaptionPaint {
                        dark,
                        focused,
                        maximized,
                        hovered: self.state.hovered.get() == Some(button),
                        pressed: self.state.pressed.get() == Some(button),
                    },
                );
            }
        }
    }

    impl Drop for Frame {
        fn drop(&mut self) {
            if self.state.attached.replace(false) {
                // SAFETY: State and HWND are still alive; this is the owning
                // thread. Removing our ID preserves other subclass chains.
                unsafe {
                    RemoveWindowSubclass(self.hwnd, Some(subclass), SUBCLASS_ID);
                }
            }
        }
    }

    // SAFETY: comctl32 invokes this only for our live registered HWND. No panic
    // crosses the ABI: RefCell access is fallible and there is no indexing,
    // allocation, unchecked user geometry, or application callback here.
    unsafe extern "system" fn subclass(
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
        _id: usize,
        data: usize,
    ) -> LRESULT {
        // SAFETY: data points to the stable State supplied at installation and
        // freed only after RemoveWindowSubclass or WM_NCDESTROY detached it.
        let state = unsafe { &*(data as *const State) };
        if message == WM_NCDESTROY {
            state.attached.set(false);
            // SAFETY: remove this callback before forwarding destruction; keep
            // ownership with Frame, which may outlive HWND while unwinding host.
            return unsafe {
                RemoveWindowSubclass(hwnd, Some(subclass), SUBCLASS_ID);
                DefSubclassProc(hwnd, message, wparam, lparam)
            };
        }
        let caption_button = |hit: WPARAM| match hit as u32 {
            HTMINBUTTON => Some(CaptionButton::Minimize),
            HTMAXBUTTON => Some(CaptionButton::Maximize),
            HTCLOSE => Some(CaptionButton::Close),
            _ => None,
        };
        let invalidate = || {
            // SAFETY: repaint only this live HWND. No focus/input is generated.
            unsafe {
                InvalidateRect(hwnd, ptr::null(), 0);
            }
        };
        if message == WM_NCMOUSEMOVE {
            let hovered = caption_button(wparam);
            if state.hovered.replace(hovered) != hovered {
                invalidate();
            }
            let mut tracking = TRACKMOUSEEVENT {
                cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
                dwFlags: TME_LEAVE | TME_NONCLIENT,
                hwndTrack: hwnd,
                dwHoverTime: 0,
            };
            // SAFETY: registers notifications for existing native pointer input;
            // it never moves the cursor or synthesizes a mouse event.
            unsafe {
                TrackMouseEvent(&mut tracking);
            }
        } else if message == WM_NCLBUTTONDOWN && caption_button(wparam).is_some() {
            let button = caption_button(wparam);
            state.hovered.set(button);
            state.pressed.set(button);
            invalidate();
            // DefWindowProc owns the native caption-button modal interaction and
            // exactly one system command. Repainting reflects its pressed state;
            // no second synthetic command is ever sent by this subclass.
            let result = unsafe { DefSubclassProc(hwnd, message, wparam, lparam) };
            state.pressed.set(None);
            invalidate();
            return result;
        } else if message == WM_CANCELMODE || (message == WM_NCACTIVATE && wparam == 0) {
            let changed = state.hovered.replace(None).is_some();
            let released = state.pressed.replace(None).is_some();
            if changed || released {
                invalidate();
            }
        } else {
            let hover_cleared = (message == WM_NCMOUSELEAVE
                || (message == WM_MOUSEMOVE && state.pressed.get().is_none()))
                && state.hovered.replace(None).is_some();
            let press_cleared = matches!(message, WM_NCLBUTTONUP | WM_CAPTURECHANGED)
                && state.pressed.replace(None).is_some();
            if hover_cleared || press_cleared {
                invalidate();
            }
        }
        if message == WM_NCCALCSIZE && wparam != 0 {
            // SAFETY: WM_NCCALCSIZE with nonzero wParam supplies valid params.
            let original_top = unsafe { (*(lparam as *const NCCALCSIZE_PARAMS)).rgrc[0].top };
            // Preserve native side/bottom resize borders and maximize/taskbar
            // calculations; remove only the caption from the top inset.
            unsafe {
                DefSubclassProc(hwnd, message, wparam, lparam);
                let dpi = GetDpiForWindow(hwnd).max(96);
                let top_border = if IsZoomed(hwnd) != 0 {
                    GetSystemMetricsForDpi(SM_CYSIZEFRAME, dpi)
                        + GetSystemMetricsForDpi(SM_CXPADDEDBORDER, dpi)
                } else {
                    0
                };
                (*(lparam as *mut NCCALCSIZE_PARAMS)).rgrc[0].top =
                    original_top.saturating_add(top_border);
            }
            return 0;
        }
        if message == WM_GETMINMAXINFO {
            // SAFETY: this message supplies valid MINMAXINFO. Preserve native
            // max/work-area calculations and width constraints. Only the minimum
            // client-height conversion differs from the standard caption frame.
            return unsafe {
                let result = DefSubclassProc(hwnd, message, wparam, lparam);
                if let Some(height) = state.min_height {
                    let dpi = GetDpiForWindow(hwnd).max(96);
                    let bottom = GetSystemMetricsForDpi(SM_CYSIZEFRAME, dpi)
                        + GetSystemMetricsForDpi(SM_CXPADDEDBORDER, dpi);
                    let height = state
                        .bar
                        .try_borrow()
                        .map_or(height, |bar| height.max(bar.height));
                    (*(lparam as *mut MINMAXINFO)).ptMinTrackSize.y =
                        ((f64::from(height) * f64::from(dpi) / 96.0).ceil() as i32)
                            .saturating_add(bottom);
                }
                result
            };
        }
        let mut result = 0;
        // SAFETY: pass native messages to DWM before custom hit testing, including
        // NC move/leave for system caption hover/Snap. Preserve winit's focus
        // bookkeeping even if DWM also handles nonclient activation.
        if unsafe { DwmDefWindowProc(hwnd, message, wparam, lparam, &mut result) } != 0 {
            if message == WM_NCACTIVATE {
                unsafe {
                    DefSubclassProc(hwnd, message, wparam, lparam);
                }
            }
            return result;
        }
        if message == WM_NCHITTEST {
            // SAFETY: default subclass chain preserves native side/bottom edges.
            let native = unsafe { DefSubclassProc(hwnd, message, wparam, lparam) };
            if native != HTCLIENT as LRESULT {
                return native;
            }
            let mut outer = RECT::default();
            let mut point = POINT {
                x: (lparam as u16 as i16) as i32,
                y: ((lparam >> 16) as u16 as i16) as i32,
            };
            // SAFETY: live HWND and initialized geometry. Screen coordinates are
            // sign-extended, so monitors left/above the primary display work.
            unsafe {
                let dpi = GetDpiForWindow(hwnd).max(96);
                if IsZoomed(hwnd) == 0 && GetWindowRect(hwnd, &mut outer) != 0 {
                    let border_x = GetSystemMetricsForDpi(SM_CXSIZEFRAME, dpi)
                        + GetSystemMetricsForDpi(SM_CXPADDEDBORDER, dpi);
                    let border_y = GetSystemMetricsForDpi(SM_CYSIZEFRAME, dpi)
                        + GetSystemMetricsForDpi(SM_CXPADDEDBORDER, dpi);
                    if point.y >= outer.top && point.y < outer.top + border_y {
                        return if point.x < outer.left + border_x {
                            HTTOPLEFT
                        } else if point.x >= outer.right - border_x {
                            HTTOPRIGHT
                        } else {
                            HTTOP
                        } as LRESULT;
                    }
                }
                if ScreenToClient(hwnd, &mut point) != 0 {
                    let scale = dpi as f32 / 96.0;
                    if let Ok(bar) = state.bar.try_borrow()
                        && is_draggable(&bar, point.x as f32 / scale, point.y as f32 / scale)
                    {
                        return HTCAPTION as LRESULT;
                    }
                }
            }
            return HTCLIENT as LRESULT;
        }
        // SAFETY: preserve all remaining winit/accessibility/native processing,
        // including system menu, caption commands, drag and double-click maximize.
        unsafe { DefSubclassProc(hwnd, message, wparam, lparam) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dragging_uses_explicit_regions_and_stays_inside_title_height() {
        let bar = TitleBar {
            height: 44.0,
            drag_regions: vec![Rect::new(120.0, 0.0, 400.0, 44.0)],
            dark: false,
        };
        assert!(valid_title_bar(&bar));
        assert!(is_draggable(&bar, 200.0, 20.0));
        assert!(!is_draggable(&bar, 60.0, 20.0)); // QAT
        assert!(!is_draggable(&bar, 600.0, 20.0)); // native buttons
        assert!(!is_draggable(&bar, 200.0, 44.0)); // tabs
        assert!(!is_draggable(&bar, 200.0, -1.0));
        assert!(!is_draggable(&bar, f32::NAN, 20.0));
        let covered = TitleBar {
            drag_regions: vec![],
            ..bar
        };
        assert!(!is_draggable(&covered, 200.0, 20.0)); // popup
    }

    #[test]
    fn malformed_geometry_cannot_be_installed() {
        let mut bar = TitleBar {
            height: 44.0,
            drag_regions: vec![],
            dark: false,
        };
        for height in [0.0, -1.0, f32::INFINITY, f32::NAN, 1025.0] {
            bar.height = height;
            assert!(!valid_title_bar(&bar));
        }
        bar.height = 44.0;
        bar.drag_regions.push(Rect::new(0.0, 0.0, -1.0, 44.0));
        assert!(!valid_title_bar(&bar));
    }

    #[test]
    fn native_caption_overlay_has_distinct_minimize_maximize_restore_and_close_ink() {
        let bounds = Rect::new(200.0, 0.0, 48.0, 30.0);
        let appearance = CaptionPaint {
            maximized: false,
            dark: false,
            focused: true,
            hovered: false,
            pressed: false,
        };
        let mut scene = Scene::default();
        for button in [
            CaptionButton::Minimize,
            CaptionButton::Maximize,
            CaptionButton::Close,
        ] {
            paint_caption(&mut scene, bounds, bounds, button, appearance);
        }
        assert_eq!(scene.stroke_paths.len(), 3);
        assert_eq!(
            scene
                .stroke_paths
                .iter()
                .map(|path| path.segments.len())
                .collect::<Vec<_>>(),
            [1, 4, 2]
        );
        assert!(scene.stroke_paths.iter().all(|path| path.is_valid()
            && path.clip == bounds
            && path.color == rust_desktop_ui_core::Color::rgb(32, 32, 32)));
        assert!(scene.rectangles.is_empty());
        paint_caption(
            &mut scene,
            bounds,
            bounds,
            CaptionButton::Maximize,
            CaptionPaint {
                maximized: true,
                dark: true,
                ..appearance
            },
        );
        assert_eq!(scene.stroke_paths[3].segments.len(), 8);
        assert_eq!(
            scene.stroke_paths[3].color,
            rust_desktop_ui_core::Color::rgb(245, 245, 245)
        );
    }

    #[test]
    fn close_hover_and_press_keep_contrasting_glyph_and_inactive_caption_is_muted() {
        let bounds = Rect::new(200.0, 0.0, 48.0, 30.0);
        let hover = CaptionPaint {
            maximized: false,
            dark: false,
            focused: true,
            hovered: true,
            pressed: false,
        };
        let mut scene = Scene::default();
        paint_caption(&mut scene, bounds, bounds, CaptionButton::Close, hover);
        let hover_fill = scene.rectangles[0].color;
        paint_caption(
            &mut scene,
            bounds,
            bounds,
            CaptionButton::Close,
            CaptionPaint {
                pressed: true,
                ..hover
            },
        );
        assert_ne!(scene.rectangles[1].color, hover_fill);
        assert!(
            scene
                .stroke_paths
                .iter()
                .all(|path| path.color == rust_desktop_ui_core::Color::rgb(255, 255, 255))
        );
        paint_caption(
            &mut scene,
            bounds,
            bounds,
            CaptionButton::Minimize,
            CaptionPaint {
                hovered: false,
                focused: false,
                ..hover
            },
        );
        assert_eq!(scene.rectangles.len(), 2);
        assert_eq!(
            scene.stroke_paths[2].color,
            rust_desktop_ui_core::Color::rgb(128, 128, 128)
        );
    }
}
