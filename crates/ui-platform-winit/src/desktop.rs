//! Desktop coordinates, placement and capture observation without global input polling.

use crate::{DesktopApp, WindowKey, WindowSpec};
use rust_desktop_ui_core::{Point, Size};
use std::sync::Arc;
use winit::window::Window;

/// Coordinate spaces reported by a native window. Screen origins are physical
/// pixels; sizes and the persisted outer position are logical pixels at `scale`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindowGeometry {
    pub client_origin: Point,
    pub client_size: Size,
    pub outer_position: Point,
    pub scale: f32,
}

impl WindowGeometry {
    /// Convert a logical client point into physical desktop pixels.
    pub fn client_to_screen(self, point: Point) -> Option<Point> {
        if !self.valid() || !finite(point) {
            return None;
        }
        let point = Point::new(
            self.client_origin.x + point.x * self.scale,
            self.client_origin.y + point.y * self.scale,
        );
        finite(point).then_some(point)
    }

    /// Convert physical desktop pixels into logical client coordinates.
    pub fn screen_to_client(self, point: Point) -> Option<Point> {
        if !self.valid() || !finite(point) {
            return None;
        }
        let point = Point::new(
            (point.x - self.client_origin.x) / self.scale,
            (point.y - self.client_origin.y) / self.scale,
        );
        finite(point).then_some(point)
    }

    fn valid(self) -> bool {
        finite(self.client_origin)
            && finite(self.outer_position)
            && self.client_size.is_valid()
            && self.client_size.width > 0.0
            && self.client_size.height > 0.0
            && self.scale.is_finite()
            && self.scale > 0.0
    }
}

fn finite(point: Point) -> bool {
    point.x.is_finite() && point.y.is_finite()
}

/// Native hit context for the immediately following pointer event. `None` means
/// the topmost eligible client area is not one of this application's windows.
/// Physical screen position comes from the event; the host never polls the cursor.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DesktopPointer {
    pub screen_position: Point,
    pub hit_window: Option<WindowKey>,
}

pub(crate) fn geometry(window: &Window) -> Option<WindowGeometry> {
    let scale = window.scale_factor();
    let origin = window.inner_position().ok()?;
    let outer = window.outer_position().ok()?.to_logical::<f32>(scale);
    let size = window.inner_size().to_logical::<f32>(scale);
    let geometry = WindowGeometry {
        client_origin: Point::new(origin.x as f32, origin.y as f32),
        client_size: Size::new(size.width, size.height),
        outer_position: Point::new(outer.x, outer.y),
        scale: scale as f32,
    };
    geometry.valid().then_some(geometry)
}

pub(crate) fn notify_geometry(window: &Window, app: &mut dyn DesktopApp) {
    // Preserve the legacy callback even on systems without a client screen origin.
    if let Ok(outer) = window.outer_position() {
        let scale = window.scale_factor();
        let position = outer.to_logical::<f32>(scale);
        let size = window.inner_size().to_logical::<f32>(scale);
        if size.width > 0.0 && size.height > 0.0 {
            app.window_geometry_changed(
                Point::new(position.x, position.y),
                Size::new(size.width, size.height),
                scale as f32,
            );
        }
    }
    if let Some(geometry) = geometry(window) {
        app.desktop_geometry_changed(geometry);
    }
}

pub(crate) fn pointer<'a>(
    source: &Window,
    client_physical: (f64, f64),
    windows: impl Iterator<Item = (WindowKey, &'a Window)>,
) -> Option<DesktopPointer> {
    #[cfg(target_os = "windows")]
    {
        let origin = geometry(source)?.client_origin;
        let point = Point::new(
            origin.x + client_physical.0 as f32,
            origin.y + client_physical.1 as f32,
        );
        if !finite(point) {
            return None;
        }
        Some(DesktopPointer {
            screen_position: point,
            hit_window: windows::hit_test(point, windows),
        })
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (source, client_physical, windows);
        None
    }
}

pub(crate) fn apply_placement(
    window: &Window,
    spec: &WindowSpec,
    initial: bool,
) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    return windows::apply_placement(window, spec, initial);
    #[cfg(not(target_os = "windows"))]
    {
        if !initial {
            if let Some(position) = spec.position {
                window.set_outer_position(winit::dpi::LogicalPosition::new(position.x, position.y));
            }
            let _ = window.request_inner_size(winit::dpi::LogicalSize::new(
                spec.options.width,
                spec.options.height,
            ));
        }
        Ok(())
    }
}

pub(crate) struct CaptureObserver {
    #[cfg(target_os = "windows")]
    native: windows::CaptureObserver,
}

/// A UIA focus request must acquire actual native client focus before the
/// retained tree accepts it. Foreground activation alone can leave focus null
/// on Windows (for example after a background window was activated by a tool).
pub(crate) fn accessibility_focus(window: &Window) -> bool {
    window.focus_window();
    #[cfg(target_os = "windows")]
    return windows::accessibility_focus(window);
    #[cfg(not(target_os = "windows"))]
    window.has_focus()
}

impl CaptureObserver {
    pub(crate) fn pointer_position(
        &self,
        event: &winit::event::WindowEvent,
        window: &Window,
    ) -> Option<(f64, f64)> {
        #[cfg(target_os = "windows")]
        return self.native.pointer_position(event, window);
        #[cfg(not(target_os = "windows"))]
        {
            let _ = (event, window);
            None
        }
    }
    pub(crate) fn attach(window: Arc<Window>) -> Result<Self, String> {
        #[cfg(target_os = "windows")]
        return Ok(Self {
            native: windows::CaptureObserver::attach(window)?,
        });
        #[cfg(not(target_os = "windows"))]
        {
            let _ = window;
            Ok(Self {})
        }
    }

    pub(crate) fn take_lost(&self) -> bool {
        #[cfg(target_os = "windows")]
        return self.native.take_lost();
        #[cfg(not(target_os = "windows"))]
        false
    }
}

#[cfg(any(target_os = "windows", test))]
#[derive(Default)]
struct CaptureState {
    #[cfg(target_os = "windows")]
    attached: std::cell::Cell<bool>,
    mouse_up_depth: std::cell::Cell<u32>,
    lost: std::cell::Cell<bool>,
    #[cfg(target_os = "windows")]
    positions: std::cell::RefCell<PointerSamples>,
}

#[cfg(any(target_os = "windows", test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SampleKind {
    Button { button: u32, down: bool },
    Wheel,
}

#[cfg(any(target_os = "windows", test))]
#[derive(Clone, Copy, Debug, PartialEq)]
struct PointerSample {
    kind: SampleKind,
    point: (f64, f64),
    screen: bool,
}

#[cfg(any(target_os = "windows", test))]
#[derive(Default)]
struct PointerSamples {
    queue: std::collections::VecDeque<PointerSample>,
    skipped: usize,
}

#[cfg(any(target_os = "windows", test))]
impl PointerSamples {
    fn push(&mut self, sample: PointerSample) -> bool {
        let overflow = self.queue.len() == 64;
        if overflow {
            self.queue.pop_front();
            self.skipped = self.skipped.saturating_add(1);
        }
        self.queue.push_back(sample);
        overflow
    }
    fn take(&mut self, kind: SampleKind) -> Option<PointerSample> {
        // If the native queue overflowed, fall back for exactly the discarded
        // events before consuming retained coordinates. Never use a later point.
        if self.skipped > 0 {
            self.skipped -= 1;
            return None;
        }
        (self.queue.front()?.kind == kind)
            .then(|| self.queue.pop_front())
            .flatten()
    }
}

#[cfg(any(target_os = "windows", test))]
impl CaptureState {
    fn changed(&self, retained: bool, released: bool) {
        if !retained && (!released || self.mouse_up_depth.get() == 0) {
            self.lost.set(true);
        }
    }
    fn cancel(&self) {
        self.lost.set(true);
    }
    fn begin_up(&self) {
        self.mouse_up_depth
            .set(self.mouse_up_depth.get().saturating_add(1));
    }
    fn end_up(&self) {
        self.mouse_up_depth
            .set(self.mouse_up_depth.get().saturating_sub(1));
    }
    fn take(&self) -> bool {
        self.lost.replace(false)
    }
}

/// The installed hook owns one strong reference when the callback starts. A
/// native call can synchronously dispatch application events and retire its
/// observer, so each callback must retain state before making such a call.
#[cfg(any(target_os = "windows", test))]
unsafe fn retain_capture_state(
    pointer: *const std::rc::Rc<CaptureState>,
) -> std::rc::Rc<CaptureState> {
    // SAFETY: the installed hook owns this stable boxed Rc at callback entry.
    // Clone it before native calls can retire the box; the userdata pointer is
    // not accessed afterwards. No raw Rc ownership conversion is performed.
    unsafe { &*pointer }.clone()
}

#[cfg(any(target_os = "windows", test))]
fn eligible_client_hit(
    topmost: usize,
    candidate: usize,
    position: Point,
    geometry: WindowGeometry,
) -> bool {
    topmost != 0
        && topmost == candidate
        && geometry.screen_to_client(position).is_some_and(|local| {
            local.x >= 0.0
                && local.y >= 0.0
                && local.x < geometry.client_size.width
                && local.y < geometry.client_size.height
        })
}

#[cfg(any(target_os = "windows", test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PhysicalPlacement {
    x: i32,
    y: i32,
    width: i32,
    height: i32,
}

#[cfg(any(target_os = "windows", test))]
fn clamp_placement(
    requested: PhysicalPlacement,
    work: PhysicalPlacement,
    minimum: (i32, i32),
) -> Option<PhysicalPlacement> {
    if work.width <= 0 || work.height <= 0 {
        return None;
    }
    let width = requested.width.max(1).min(work.width).max(minimum.0);
    let height = requested.height.max(1).min(work.height).max(minimum.1);
    let x = i64::from(requested.x).clamp(
        i64::from(work.x),
        i64::from(work.x) + i64::from((work.width - width).max(0)),
    );
    let y = i64::from(requested.y).clamp(
        i64::from(work.y),
        i64::from(work.y) + i64::from((work.height - height).max(0)),
    );
    Some(PhysicalPlacement {
        x: x.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32,
        y: y.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32,
        width,
        height,
    })
}

#[cfg(target_os = "windows")]
mod windows {
    use super::*;
    use std::{mem::size_of, ptr};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        WM_LBUTTONDBLCLK, WM_LBUTTONDOWN, WM_MBUTTONDBLCLK, WM_MBUTTONDOWN, WM_MOUSEHWHEEL,
        WM_MOUSEWHEEL, WM_RBUTTONDBLCLK, WM_RBUTTONDOWN, WM_XBUTTONDBLCLK, WM_XBUTTONDOWN,
    };
    use windows_sys::Win32::{
        Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM},
        Graphics::Gdi::{GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromRect},
        UI::{
            Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
            WindowsAndMessaging::{
                GA_ROOT, GetAncestor, GetClientRect, GetWindowRect, SWP_NOACTIVATE,
                SWP_NOOWNERZORDER, SWP_NOSIZE, SWP_NOZORDER, SetWindowPos, WM_CANCELMODE,
                WM_CAPTURECHANGED, WM_LBUTTONUP, WM_MBUTTONUP, WM_NCDESTROY, WM_RBUTTONUP,
                WM_XBUTTONUP, WindowFromPoint,
            },
        },
    };
    use winit::event::{ElementState, MouseButton, WindowEvent};
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

    const SUBCLASS_ID: usize = 0x5255_4950;

    pub(super) fn accessibility_focus(window: &Window) -> bool {
        use windows_sys::Win32::UI::{
            Input::KeyboardAndMouse::{GetFocus, SetFocus},
            WindowsAndMessaging::GetForegroundWindow,
        };
        let Ok(handle) = hwnd(window) else {
            return false;
        };
        // SAFETY: this live window belongs to this event thread. Never take
        // client focus while another process owns the foreground window.
        unsafe {
            if GetForegroundWindow() != handle {
                return false;
            }
            SetFocus(handle);
            GetFocus() == handle
        }
    }

    fn hwnd(window: &Window) -> Result<HWND, String> {
        let handle = window.window_handle().map_err(|error| error.to_string())?;
        let RawWindowHandle::Win32(handle) = handle.as_raw() else {
            return Err("Expected a Win32 desktop window".into());
        };
        Ok(handle.hwnd.get() as HWND)
    }

    pub(super) fn hit_test<'a>(
        position: Point,
        mut windows: impl Iterator<Item = (WindowKey, &'a Window)>,
    ) -> Option<WindowKey> {
        // SAFETY: these read-only desktop queries use a finite event position.
        let root = unsafe {
            let hit = WindowFromPoint(POINT {
                x: position.x.round() as i32,
                y: position.y.round() as i32,
            });
            GetAncestor(hit, GA_ROOT)
        };
        if root.is_null() {
            return None;
        }
        windows.find_map(|(key, window)| {
            let native = hwnd(window).ok()?;
            if native != root {
                return None;
            }
            let geometry = geometry(window)?;
            eligible_client_hit(root as usize, native as usize, position, geometry).then_some(key)
        })
    }

    fn rect_placement(rect: RECT) -> PhysicalPlacement {
        PhysicalPlacement {
            x: rect.left,
            y: rect.top,
            width: rect.right.saturating_sub(rect.left),
            height: rect.bottom.saturating_sub(rect.top),
        }
    }

    pub(super) fn apply_placement(
        window: &Window,
        spec: &WindowSpec,
        initial: bool,
    ) -> Result<(), String> {
        let hwnd = hwnd(window)?;
        // Position first so Windows can select the destination DPI before size
        // conversion. The window remains hidden on creation and never activates.
        if !initial && let Some(position) = spec.position {
            let scale = window.scale_factor();
            // SAFETY: live HWND on its owning event thread; no focus/z-order change.
            if unsafe {
                SetWindowPos(
                    hwnd,
                    ptr::null_mut(),
                    (f64::from(position.x) * scale).round() as i32,
                    (f64::from(position.y) * scale).round() as i32,
                    0,
                    0,
                    SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOZORDER | SWP_NOOWNERZORDER,
                )
            } == 0
            {
                return Err("Could not restore native window position".into());
            }
        }
        let mut outer = RECT::default();
        let mut client = RECT::default();
        // SAFETY: initialized output rects and the live HWND owned by this host.
        if unsafe { GetWindowRect(hwnd, &mut outer) == 0 || GetClientRect(hwnd, &mut client) == 0 }
        {
            return Err("Could not read native window placement".into());
        }
        let frame_x = outer
            .right
            .saturating_sub(outer.left)
            .saturating_sub(client.right.saturating_sub(client.left))
            .max(0);
        let frame_y = outer
            .bottom
            .saturating_sub(outer.top)
            .saturating_sub(client.bottom.saturating_sub(client.top))
            .max(0);
        let scale = window.scale_factor();
        let extent = |value: f64, frame: i32| {
            (value * scale + f64::from(frame))
                .round()
                .clamp(1.0, f64::from(i32::MAX)) as i32
        };
        let requested = PhysicalPlacement {
            x: outer.left,
            y: outer.top,
            width: extent(spec.options.width, frame_x),
            height: extent(spec.options.height, frame_y),
        };
        let candidate = RECT {
            left: requested.x,
            top: requested.y,
            right: requested.x.saturating_add(requested.width),
            bottom: requested.y.saturating_add(requested.height),
        };
        let mut info = MONITORINFO {
            cbSize: size_of::<MONITORINFO>() as u32,
            ..MONITORINFO::default()
        };
        // SAFETY: read-only nearest-monitor/work-area query with initialized storage.
        if unsafe {
            GetMonitorInfoW(
                MonitorFromRect(&candidate, MONITOR_DEFAULTTONEAREST),
                &mut info,
            )
        } == 0
        {
            return Err("Could not determine native monitor work area".into());
        }
        let minimum = spec.options.min_size.map_or((1, 1), |size| {
            (
                extent(f64::from(size.width), frame_x),
                extent(f64::from(size.height), frame_y),
            )
        });
        let placed = clamp_placement(requested, rect_placement(info.rcWork), minimum)
            .ok_or("Native monitor has an empty work area")?;
        // SAFETY: apply only geometry, preserving visibility, activation and order.
        if unsafe {
            SetWindowPos(
                hwnd,
                ptr::null_mut(),
                placed.x,
                placed.y,
                placed.width,
                placed.height,
                SWP_NOACTIVATE | SWP_NOZORDER | SWP_NOOWNERZORDER,
            )
        } == 0
        {
            return Err("Could not apply native window placement".into());
        }
        Ok(())
    }

    pub(super) struct CaptureObserver {
        _window: Arc<Window>,
        hwnd: HWND,
        // Win32 stores an address of the Rc value so callbacks can safely clone
        // it. The Box pins that value when WindowState moves in the host map.
        #[allow(clippy::redundant_allocation)]
        state: Box<std::rc::Rc<CaptureState>>,
    }

    impl CaptureObserver {
        pub(super) fn attach(window: Arc<Window>) -> Result<Self, String> {
            let observer = Self {
                hwnd: hwnd(&window)?,
                _window: window,
                state: Box::default(),
            };
            // SAFETY: the boxed Rc has a stable address while the subclass is
            // installed. Callbacks clone it before native reentrancy can retire
            // this observer and its box.
            if unsafe {
                SetWindowSubclass(
                    observer.hwnd,
                    Some(subclass),
                    SUBCLASS_ID,
                    ptr::from_ref(observer.state.as_ref()) as usize,
                )
            } == 0
            {
                return Err("Could not observe native pointer capture".into());
            }
            observer.state.attached.set(true);
            Ok(observer)
        }
        pub(super) fn take_lost(&self) -> bool {
            self.state.take()
        }
        pub(super) fn pointer_position(
            &self,
            event: &WindowEvent,
            window: &Window,
        ) -> Option<(f64, f64)> {
            let kind = match event {
                WindowEvent::MouseInput { button, state, .. } => SampleKind::Button {
                    button: match button {
                        MouseButton::Left => 1,
                        MouseButton::Right => 2,
                        MouseButton::Middle => 3,
                        MouseButton::Back => 4,
                        MouseButton::Forward => 5,
                        MouseButton::Other(value) => 0x1_0000 + u32::from(*value),
                    },
                    down: *state == ElementState::Pressed,
                },
                WindowEvent::MouseWheel { .. } => SampleKind::Wheel,
                _ => return None,
            };
            let sample = self.state.positions.borrow_mut().take(kind)?;
            if sample.screen {
                let origin = window.inner_position().ok()?;
                Some((
                    sample.point.0 - f64::from(origin.x),
                    sample.point.1 - f64::from(origin.y),
                ))
            } else {
                Some(sample.point)
            }
        }
    }

    impl Drop for CaptureObserver {
        fn drop(&mut self) {
            // SAFETY: Arc keeps HWND alive until after removal; state is still live.
            if self.state.attached.replace(false) {
                unsafe {
                    RemoveWindowSubclass(self.hwnd, Some(subclass), SUBCLASS_ID);
                }
            }
        }
    }

    unsafe extern "system" fn subclass(
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
        _: usize,
        reference: usize,
    ) -> LRESULT {
        // SAFETY: the installed observer owns this boxed Rc at callback entry.
        // Clone before any reentrant native call: buffered application events
        // may retire the observer and box. Only the clone is used afterwards.
        let state = unsafe { retain_capture_state(reference as *const std::rc::Rc<CaptureState>) };
        if message == WM_NCDESTROY {
            state.attached.set(false);
            // SAFETY: remove before native destruction; the owner retains state.
            return unsafe {
                RemoveWindowSubclass(hwnd, Some(subclass), SUBCLASS_ID);
                DefSubclassProc(hwnd, message, wparam, lparam)
            };
        }
        // Translate double-click down messages consistently if a native class
        // emits them: winit itself accepts the ordinary button-down messages.
        let message = match message {
            WM_LBUTTONDBLCLK => WM_LBUTTONDOWN,
            WM_RBUTTONDBLCLK => WM_RBUTTONDOWN,
            WM_MBUTTONDBLCLK => WM_MBUTTONDOWN,
            WM_XBUTTONDBLCLK => WM_XBUTTONDOWN,
            other => other,
        };
        let kind = match message {
            WM_LBUTTONDOWN | WM_LBUTTONUP => Some(SampleKind::Button {
                button: 1,
                down: message == WM_LBUTTONDOWN,
            }),
            WM_RBUTTONDOWN | WM_RBUTTONUP => Some(SampleKind::Button {
                button: 2,
                down: message == WM_RBUTTONDOWN,
            }),
            WM_MBUTTONDOWN | WM_MBUTTONUP => Some(SampleKind::Button {
                button: 3,
                down: message == WM_MBUTTONDOWN,
            }),
            WM_XBUTTONDOWN | WM_XBUTTONUP => Some(SampleKind::Button {
                button: match ((wparam >> 16) & 0xffff) as u32 {
                    1 => 4,
                    2 => 5,
                    value => 0x1_0000 + value,
                },
                down: message == WM_XBUTTONDOWN,
            }),
            WM_MOUSEWHEEL | WM_MOUSEHWHEEL => Some(SampleKind::Wheel),
            _ => None,
        };
        if let Some(kind) = kind
            && let Ok(mut positions) = state.positions.try_borrow_mut()
        {
            let overflow = positions.push(PointerSample {
                kind,
                point: (
                    (lparam as u16 as i16) as f64,
                    ((lparam as u32 >> 16) as u16 as i16) as f64,
                ),
                screen: kind == SampleKind::Wheel,
            });
            if overflow {
                // A delayed native batch must cancel a drag instead of dropping
                // it at an obsolete cached position after losing exact samples.
                state.cancel();
            }
        }
        let up = matches!(
            message,
            WM_LBUTTONUP | WM_RBUTTONUP | WM_MBUTTONUP | WM_XBUTTONUP
        );
        if up {
            state.begin_up();
        }
        if message == WM_CAPTURECHANGED {
            state.changed(lparam as HWND == hwnd, lparam == 0);
        }
        if message == WM_CANCELMODE {
            state.cancel();
        }
        // winit releases capture synchronously inside mouse-up, before producing
        // its PointerUp event. Suppress only that ordinary nested release.
        let result = unsafe { DefSubclassProc(hwnd, message, wparam, lparam) };
        if up {
            state.end_up();
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coordinates_cross_different_dpi_clients_with_native_frame_offsets() {
        let source = WindowGeometry {
            client_origin: Point::new(-1900.0, 48.0),
            client_size: Size::new(800.0, 600.0),
            outer_position: Point::new(-960.0, 0.0),
            scale: 2.0,
        };
        let target = WindowGeometry {
            client_origin: Point::new(120.0, 72.0),
            client_size: Size::new(800.0, 600.0),
            outer_position: Point::new(72.0, 24.0),
            scale: 1.5,
        };
        let desktop = source.client_to_screen(Point::new(1090.0, 96.0)).unwrap();
        assert_eq!(desktop, Point::new(280.0, 240.0));
        let client = target.screen_to_client(desktop).unwrap();
        assert!((client.x - 106.666_664).abs() < 0.0001);
        assert_eq!(client.y, 112.0);
        assert_eq!(
            source.screen_to_client(desktop),
            Some(Point::new(1090.0, 96.0))
        );
    }

    #[test]
    fn coordinates_reject_invalid_or_overflowing_values() {
        let mut geometry = WindowGeometry {
            client_origin: Point::new(0.0, 0.0),
            client_size: Size::new(1.0, 1.0),
            outer_position: Point::new(0.0, 0.0),
            scale: 2.0,
        };
        assert!(
            geometry
                .client_to_screen(Point::new(f32::MAX, 0.0))
                .is_none()
        );
        assert!(
            geometry
                .screen_to_client(Point::new(f32::NAN, 0.0))
                .is_none()
        );
        geometry.scale = 0.0;
        assert!(geometry.screen_to_client(Point::new(0.0, 0.0)).is_none());
    }

    #[test]
    fn native_hit_eligibility_rejects_occluders_caption_and_right_bottom_edges() {
        let geometry = WindowGeometry {
            client_origin: Point::new(300.0, 150.0),
            client_size: Size::new(200.0, 100.0),
            outer_position: Point::new(190.0, 70.0),
            scale: 1.5,
        };
        let inside = Point::new(320.0, 170.0);
        assert!(eligible_client_hit(4, 4, inside, geometry));
        assert!(!eligible_client_hit(99, 4, inside, geometry));
        assert!(!eligible_client_hit(0, 4, inside, geometry));
        assert!(!eligible_client_hit(
            4,
            4,
            Point::new(320.0, 149.0),
            geometry
        ));
        assert!(!eligible_client_hit(
            4,
            4,
            Point::new(600.0, 170.0),
            geometry
        ));
        assert!(!eligible_client_hit(
            4,
            4,
            Point::new(320.0, 300.0),
            geometry
        ));
    }

    #[test]
    fn lost_capture_is_once_and_normal_nested_up_release_is_suppressed() {
        let state = CaptureState::default();
        state.changed(true, false);
        assert!(!state.take());
        state.begin_up();
        state.changed(false, true);
        state.end_up();
        assert!(!state.take());
        state.changed(false, true);
        state.changed(false, true);
        assert!(state.take());
        assert!(!state.take());
        state.begin_up();
        state.cancel();
        state.end_up();
        assert!(state.take());
    }

    #[test]
    fn real_capture_transfer_during_mouse_up_still_cancels_once() {
        let state = CaptureState::default();
        state.begin_up();
        state.changed(true, false);
        state.changed(false, true);
        assert!(!state.take());
        state.changed(false, false);
        state.changed(false, true);
        state.end_up();
        assert!(state.take());
        assert!(!state.take());
    }

    #[test]
    fn capture_callback_guard_survives_reentrant_observer_retirement() {
        let owner = Box::new(std::rc::Rc::new(CaptureState::default()));
        let weak = std::rc::Rc::downgrade(owner.as_ref());
        // SAFETY: the live owner models the installed observer at callback entry.
        let callback = unsafe { retain_capture_state(std::ptr::from_ref(owner.as_ref())) };
        callback.begin_up();
        drop(owner);
        assert!(weak.upgrade().is_some());
        callback.changed(false, false);
        callback.end_up();
        assert!(callback.take());
        drop(callback);
        assert!(weak.upgrade().is_none());
    }

    #[test]
    fn queued_button_and_wheel_points_keep_order_and_do_not_reuse_a_later_position() {
        let down = SampleKind::Button {
            button: 1,
            down: true,
        };
        let up = SampleKind::Button {
            button: 1,
            down: false,
        };
        let mut samples = PointerSamples::default();
        samples.push(PointerSample {
            kind: down,
            point: (10.0, 20.0),
            screen: false,
        });
        samples.push(PointerSample {
            kind: up,
            point: (-120.0, 90.0),
            screen: false,
        });
        samples.push(PointerSample {
            kind: SampleKind::Wheel,
            point: (-1900.0, 42.0),
            screen: true,
        });
        assert!(samples.take(up).is_none());
        assert_eq!(samples.take(down).unwrap().point, (10.0, 20.0));
        assert_eq!(samples.take(up).unwrap().point, (-120.0, 90.0));
        assert!(samples.take(up).is_none());
        let wheel = samples.take(SampleKind::Wheel).unwrap();
        assert!(wheel.screen);
        assert_eq!(wheel.point, (-1900.0, 42.0));
        for index in 0..70 {
            samples.push(PointerSample {
                kind: down,
                point: (f64::from(index), 0.0),
                screen: false,
            });
        }
        for _ in 0..6 {
            assert!(samples.take(down).is_none());
        }
        for index in 6..70 {
            assert_eq!(samples.take(down).unwrap().point.0, f64::from(index));
        }
        assert!(samples.take(down).is_none());
    }

    #[test]
    fn restored_windows_fit_work_area_or_keep_oversized_minimum_caption_reachable() {
        let work = PhysicalPlacement {
            x: -1920,
            y: 40,
            width: 1920,
            height: 1040,
        };
        let requested = PhysicalPlacement {
            x: 50000,
            y: -50000,
            width: 800,
            height: 600,
        };
        assert_eq!(
            clamp_placement(requested, work, (320, 240)),
            Some(PhysicalPlacement {
                x: -800,
                y: 40,
                width: 800,
                height: 600,
            })
        );
        let huge = PhysicalPlacement {
            width: i32::MAX,
            height: i32::MAX,
            ..requested
        };
        assert_eq!(clamp_placement(huge, work, (320, 240)), Some(work));
        assert_eq!(
            clamp_placement(requested, work, (2400, 1500)),
            Some(PhysicalPlacement {
                x: -1920,
                y: 40,
                width: 2400,
                height: 1500,
            })
        );
        assert!(
            clamp_placement(requested, PhysicalPlacement { width: 0, ..work }, (1, 1)).is_none()
        );
    }
}
