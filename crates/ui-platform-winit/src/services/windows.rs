use super::*;
mod clipboard;
mod shell;
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    rc::Rc,
};
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, WPARAM},
    UI::{Shell::*, WindowsAndMessaging::*},
};
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
const SUBCLASS: usize = 0x5255_4953;
const TRAY_MESSAGE: u32 = WM_APP + 0x531;
pub(super) fn owner_alive(window: &winit::window::Window) -> bool {
    let Ok(handle) = window.window_handle() else {
        return false;
    };
    let RawWindowHandle::Win32(handle) = handle.as_raw() else {
        return false;
    };
    // SAFETY: read-only check, handle is never dereferenced by Rust.
    unsafe { IsWindow(handle.hwnd.get() as HWND) != 0 }
}
fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}
fn native(message: &str) -> ServiceError {
    ServiceError::new(ServiceErrorKind::Native, message)
}
struct Signals {
    alive: Rc<Cell<bool>>,
    attached: Cell<bool>,
    taskbar_ready: Cell<bool>,
    taskbar_message: u32,
    restart_message: u32,
    restart: Cell<bool>,
    tray_serial: Cell<u16>,
    identity: RefCell<Option<shell::WindowIdentity>>,
    events: RefCell<VecDeque<PlatformServiceEvent>>,
    waker: AppWaker,
}
impl Signals {
    fn push(&self, event: PlatformServiceEvent) {
        let mut q = self.events.borrow_mut();
        if q.len() < 128 {
            q.push_back(event);
        }
        drop(q);
        self.waker.wake();
    }
}
pub(super) struct Backend {
    _window: Arc<winit::window::Window>,
    hwnd: HWND,
    #[allow(clippy::redundant_allocation)]
    signals: Box<Rc<Signals>>,
    tray: Option<shell::Tray>,
    next_tray: u16,
}
impl Backend {
    pub(super) fn new(window: Arc<winit::window::Window>, waker: AppWaker) -> ServiceResult<Self> {
        let RawWindowHandle::Win32(handle) = window
            .window_handle()
            .map_err(|_| native("No native window handle"))?
            .as_raw()
        else {
            return Err(native("Expected a Win32 window"));
        };
        let hwnd = handle.hwnd.get() as HWND;
        // SAFETY: null-terminated names and a live UI-thread HWND.
        let taskbar_message =
            unsafe { RegisterWindowMessageW(wide("TaskbarButtonCreated").as_ptr()) };
        let restart_message = unsafe { RegisterWindowMessageW(wide("TaskbarCreated").as_ptr()) };
        if taskbar_message == 0 || restart_message == 0 {
            return Err(native("Cannot register shell lifecycle messages"));
        }
        let signals = Box::new(Rc::new(Signals {
            alive: Rc::new(Cell::new(true)),
            attached: Cell::new(false),
            taskbar_ready: Cell::new(false),
            taskbar_message,
            restart_message,
            restart: Cell::new(false),
            tray_serial: Cell::new(0),
            identity: RefCell::new(None),
            events: RefCell::new(VecDeque::new()),
            waker,
        }));
        // SAFETY: the boxed Rc remains pinned until subclass removal. Callback
        // clones Rc before any native reentrant call.
        if unsafe {
            SetWindowSubclass(
                hwnd,
                Some(subclass),
                SUBCLASS,
                std::ptr::from_ref(signals.as_ref()) as usize,
            )
        } == 0
        {
            return Err(native("Cannot install shell observer"));
        }
        signals.attached.set(true);
        Ok(Self {
            _window: window,
            hwnd,
            signals,
            tray: None,
            next_tray: 1,
        })
    }
    pub(super) fn execute(
        &mut self,
        request: PlatformServiceRequest,
    ) -> ServiceResult<PlatformServiceValue> {
        if !self.is_alive() {
            return Err(ServiceError::new(
                ServiceErrorKind::OwnerClosed,
                "Native service owner closed",
            ));
        }
        request.validate()?;
        match request {
            PlatformServiceRequest::ReadClipboard(format) => {
                return clipboard::read(self.hwnd, format).map(PlatformServiceValue::Clipboard);
            }
            PlatformServiceRequest::WriteClipboard(content) => {
                clipboard::write(self.hwnd, &content)?
            }
            PlatformServiceRequest::SetTray(value) => {
                if let Some(value) = value {
                    let id = self.next_tray;
                    let next = id.checked_add(1).ok_or_else(|| {
                        ServiceError::new(ServiceErrorKind::Capacity, "Tray identities exhausted")
                    })?;
                    let candidate =
                        shell::Tray::new(self.hwnd, id, &value, self.signals.alive.clone())?;
                    self.signals.tray_serial.set(id);
                    self.tray = Some(candidate);
                    self.next_tray = next;
                    self.signals.events.borrow_mut().clear();
                } else {
                    self.signals.tray_serial.set(0);
                    self.signals.events.borrow_mut().clear();
                    self.tray = None;
                }
            }
            PlatformServiceRequest::SetTaskbarProgress(value) => {
                self.taskbar_ready()?;
                shell::progress(self.hwnd, value)?;
            }
            PlatformServiceRequest::SetTaskbarOverlay { image, description } => {
                self.taskbar_ready()?;
                shell::overlay(self.hwnd, image.as_ref(), &description)?;
            }
            PlatformServiceRequest::PublishJumpList(value) => {
                let existing = self
                    .signals
                    .identity
                    .borrow()
                    .as_ref()
                    .map(|v| v.id.clone());
                if existing.as_ref().is_some_and(|id| *id != value.app_id) {
                    return Err(invalid(
                        "Window already owns a different Jump List application ID",
                    ));
                }
                let first = existing.is_none();
                if first {
                    let identity = shell::WindowIdentity::bind(self.hwnd, &value.app_id)?;
                    *self.signals.identity.borrow_mut() = Some(identity);
                }
                if let Err(error) = shell::jump_list(&value) {
                    if first {
                        let identity = self.signals.identity.borrow_mut().take();
                        drop(identity);
                    }
                    return Err(error);
                }
            }
            PlatformServiceRequest::DeleteJumpList { app_id } => shell::delete_jump_list(&app_id)?,
            PlatformServiceRequest::Dispatch { .. } => unreachable!(),
        }
        Ok(PlatformServiceValue::Done)
    }
    fn taskbar_ready(&self) -> ServiceResult<()> {
        if self.signals.taskbar_ready.get() {
            Ok(())
        } else {
            Err(ServiceError::new(
                ServiceErrorKind::NotReady,
                "Taskbar button is not ready",
            ))
        }
    }
    pub(super) fn is_alive(&self) -> bool {
        // The lifetime flag rejects a reused HWND after our WM_DESTROY.
        self.signals.alive.get()
            && self.signals.attached.get()
            && unsafe { IsWindow(self.hwnd) } != 0
    }
    pub(super) fn event(&mut self) -> Option<PlatformServiceEvent> {
        if !self.is_alive() {
            return None;
        }
        if self.signals.restart.replace(false)
            && let Some(tray) = &self.tray
            && let Err(error) = tray.add()
        {
            return Some(PlatformServiceEvent::ShellError(error));
        }
        let event = self.signals.events.borrow_mut().pop_front()?;
        if let PlatformServiceEvent::TrayAction(0) = event {
            let tray = self.tray.as_ref()?;
            let selected = tray.menu();
            if !self.is_alive() {
                return None;
            }
            return match selected {
                Ok(Some(id)) => Some(PlatformServiceEvent::TrayAction(id)),
                Ok(None) => None,
                Err(error) => Some(PlatformServiceEvent::ShellError(error)),
            };
        }
        Some(event)
    }
}
impl Drop for Backend {
    fn drop(&mut self) {
        self.signals.tray_serial.set(0);
        self.signals.events.borrow_mut().clear();
        self.tray = None;
        let identity = self.signals.identity.borrow_mut().take();
        drop(identity);
        if self.signals.attached.replace(false) {
            // SAFETY: remove before freeing the pinned callback data.
            unsafe {
                RemoveWindowSubclass(self.hwnd, Some(subclass), SUBCLASS);
            }
            if self.signals.alive.get() && self.signals.taskbar_ready.get() {
                let _ = shell::progress(self.hwnd, TaskbarProgress::None);
                let _ = shell::overlay(self.hwnd, None, "");
            }
        }
        self.signals.alive.set(false);
    }
}
unsafe extern "system" fn subclass(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    id: usize,
    data: usize,
) -> LRESULT {
    // SAFETY: installed with address of a pinned Rc and removed before free.
    let state = unsafe { (&*(data as *const Rc<Signals>)).clone() };
    if message == WM_DESTROY {
        // Retire native service operations before any reentrant cleanup. Keep
        // `attached` separate so WM_NCDESTROY still removes the subclass.
        state.alive.set(false);
        let serial = state.tray_serial.replace(0);
        if serial != 0 {
            let data = NOTIFYICONDATAW {
                cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
                hWnd: hwnd,
                uID: u32::from(serial),
                ..Default::default()
            };
            unsafe {
                Shell_NotifyIconW(NIM_DELETE, &data);
            }
        }
        state.events.borrow_mut().clear();
        let identity = state.identity.borrow_mut().take();
        drop(identity);
    }
    if message == WM_NCDESTROY {
        state.alive.set(false);
        state.attached.set(false);
        state.tray_serial.set(0);
        unsafe {
            RemoveWindowSubclass(hwnd, Some(subclass), id);
        }
    } else if state.alive.get() && message == state.taskbar_message {
        state.taskbar_ready.set(true);
        state.waker.wake();
    } else if state.alive.get() && message == state.restart_message {
        state.restart.set(true);
        state.taskbar_ready.set(false);
        state.waker.wake();
    } else if message == TRAY_MESSAGE {
        let serial = ((lparam as usize >> 16) & 0xffff) as u16;
        if state.alive.get() && serial != 0 && serial == state.tray_serial.get() {
            let event = (lparam as u32) & 0xffff;
            if event == NIN_SELECT || event == (NIN_SELECT | 1) {
                state.push(PlatformServiceEvent::TrayActivated);
            } else if event == WM_CONTEXTMENU {
                state.push(PlatformServiceEvent::TrayAction(0));
            }
        }
        return 0;
    }
    // SAFETY: preserve all unrelated messages and native ownership.
    unsafe { DefSubclassProc(hwnd, message, wparam, lparam) }
}
