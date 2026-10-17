//! Bounded, owner-scoped native services. Data is never interpreted by Controls.
#[cfg(any(target_os = "windows", test))]
mod codec;
mod queue;
#[cfg(test)]
mod tests;
#[cfg(target_os = "windows")]
mod windows;

use crate::AppWaker;
pub use queue::{PlatformServiceHandle, ServiceRequestId};
use std::{fmt, sync::Arc};

/// Availability is a capability, not a promise that a later OS call succeeds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlatformCapabilities {
    pub dispatcher: bool,
    pub text_clipboard: bool,
    pub html_clipboard: bool,
    pub image_clipboard: bool,
    pub tray: bool,
    pub taskbar: bool,
    pub jump_list: bool,
}
impl PlatformCapabilities {
    pub fn current() -> Self {
        let native = cfg!(target_os = "windows");
        Self {
            dispatcher: true,
            text_clipboard: native,
            html_clipboard: native,
            image_clipboard: native,
            tray: native,
            taskbar: native,
            jump_list: native,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ServiceErrorKind {
    Unsupported,
    Invalid,
    Busy,
    OwnerClosed,
    Capacity,
    Native,
    NotReady,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServiceError {
    pub kind: ServiceErrorKind,
    /// Bounded diagnostic; never contains clipboard content.
    pub message: String,
    /// A clipboard write can fail after the OS accepted some formats.
    pub clipboard_formats_written: usize,
}
impl ServiceError {
    fn new(kind: ServiceErrorKind, message: &str) -> Self {
        Self {
            kind,
            message: message.into(),
            clipboard_formats_written: 0,
        }
    }
}
impl fmt::Display for ServiceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for ServiceError {}
pub type ServiceResult<T> = Result<T, ServiceError>;
fn invalid(message: &str) -> ServiceError {
    ServiceError::new(ServiceErrorKind::Invalid, message)
}

/// Straight-alpha RGBA8, row-major, without padding. At most 2048² pixels.
#[derive(Clone, PartialEq, Eq)]
pub struct ClipboardImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}
impl fmt::Debug for ClipboardImage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ClipboardImage")
            .field("width", &self.width)
            .field("height", &self.height)
            .finish_non_exhaustive()
    }
}
impl ClipboardImage {
    pub fn validate(&self) -> ServiceResult<()> {
        if self.width == 0
            || self.height == 0
            || self.width > 2048
            || self.height > 2048
            || u64::from(self.width) * u64::from(self.height) * 4 != self.rgba.len() as u64
        {
            return Err(invalid(
                "Image requires bounded dimensions and exactly width*height*4 bytes",
            ));
        }
        Ok(())
    }
}
/// Omitted formats are absent; empty text is distinct from absence. HTML is a
/// raw UTF-8 fragment. Reading it does not execute or fetch its contents.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct ClipboardContent {
    pub text: Option<String>,
    pub html: Option<String>,
    pub image: Option<ClipboardImage>,
}
impl fmt::Debug for ClipboardContent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ClipboardContent")
            .field("text", &self.text.is_some())
            .field("html", &self.html.is_some())
            .field("image", &self.image.is_some())
            .finish()
    }
}
impl ClipboardContent {
    pub fn validate(&self) -> ServiceResult<()> {
        if self.text.is_none() && self.html.is_none() && self.image.is_none() {
            return Err(invalid("At least one clipboard format is required"));
        }
        if self
            .text
            .as_ref()
            .is_some_and(|s| s.len() > 1_048_576 || s.contains('\0'))
        {
            return Err(invalid("Clipboard text exceeds its limit or contains NUL"));
        }
        if self
            .html
            .as_ref()
            .is_some_and(|s| s.len() > 4_194_304 || s.contains('\0'))
        {
            return Err(invalid("Clipboard HTML exceeds its limit or contains NUL"));
        }
        if let Some(image) = &self.image {
            image.validate()?;
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClipboardFormat {
    Text,
    Html,
    Image,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrayAction {
    pub id: u32,
    pub label: String,
    pub enabled: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrayIcon {
    pub image: ClipboardImage,
    pub tooltip: String,
    pub actions: Vec<TrayAction>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskbarProgress {
    None,
    Indeterminate,
    Normal { completed: u64, total: u64 },
    Error { completed: u64, total: u64 },
    Paused { completed: u64, total: u64 },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JumpListTask {
    pub title: String,
    pub arguments: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JumpList {
    pub app_id: String,
    pub tasks: Vec<JumpListTask>,
}

/// No shell command string is executed by this service. Jump List arguments
/// belong to the current executable and are published only on explicit request.
#[derive(Clone)]
pub enum PlatformServiceRequest {
    Dispatch {
        channel: u32,
        payload: Vec<u8>,
    },
    ReadClipboard(ClipboardFormat),
    WriteClipboard(ClipboardContent),
    SetTray(Option<TrayIcon>),
    SetTaskbarProgress(TaskbarProgress),
    SetTaskbarOverlay {
        image: Option<ClipboardImage>,
        description: String,
    },
    PublishJumpList(JumpList),
    DeleteJumpList {
        app_id: String,
    },
}
impl fmt::Debug for PlatformServiceRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PlatformServiceRequest { payload: [private] }")
    }
}
fn text_limit(value: &str, limit: usize, empty: bool) -> bool {
    (empty || !value.is_empty())
        && value.encode_utf16().count() <= limit
        && !value.chars().any(char::is_control)
}
fn app_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
}
impl PlatformServiceRequest {
    fn validate(&self) -> ServiceResult<usize> {
        let bytes = match self {
            Self::Dispatch { payload, .. } => {
                if payload.len() > 65_536 {
                    return Err(invalid("Dispatch payload exceeds 64 KiB"));
                }
                payload.len()
            }
            Self::ReadClipboard(_) => 0,
            Self::WriteClipboard(content) => {
                content.validate()?;
                content.text.as_ref().map_or(0, String::len)
                    + content.html.as_ref().map_or(0, String::len)
                    + content.image.as_ref().map_or(0, |v| v.rgba.len())
            }
            Self::SetTray(tray) => {
                if let Some(v) = tray {
                    v.image.validate()?;
                    if v.image.width > 256
                        || v.image.height > 256
                        || !text_limit(&v.tooltip, 127, true)
                        || v.actions.len() > 16
                    {
                        return Err(invalid("Tray bounds exceeded"));
                    }
                    let mut ids = std::collections::HashSet::new();
                    if v.actions
                        .iter()
                        .any(|a| a.id == 0 || !ids.insert(a.id) || !text_limit(&a.label, 64, false))
                    {
                        return Err(invalid(
                            "Tray actions require unique nonzero IDs and bounded labels",
                        ));
                    }
                    v.image.rgba.len()
                        + v.tooltip.len()
                        + v.actions.iter().map(|v| v.label.len()).sum::<usize>()
                } else {
                    0
                }
            }
            Self::SetTaskbarProgress(v) => {
                match v {
                    TaskbarProgress::Normal { completed, total }
                    | TaskbarProgress::Error { completed, total }
                    | TaskbarProgress::Paused { completed, total }
                        if *total == 0 || completed > total =>
                    {
                        return Err(invalid("Invalid taskbar progress"));
                    }
                    _ => {}
                }
                0
            }
            Self::SetTaskbarOverlay { image, description } => {
                if !text_limit(description, 128, true) {
                    return Err(invalid("Invalid overlay description"));
                }
                if let Some(v) = image {
                    v.validate()?;
                    if v.width > 256 || v.height > 256 {
                        return Err(invalid("Overlay icon exceeds 256 pixels"));
                    }
                }
                image.as_ref().map_or(0, |v| v.rgba.len()) + description.len()
            }
            Self::PublishJumpList(v) => {
                if !app_id(&v.app_id)
                    || v.tasks.len() > 16
                    || v.tasks.iter().any(|t| {
                        !text_limit(&t.title, 128, false) || !text_limit(&t.arguments, 2048, false)
                    })
                {
                    return Err(invalid("Invalid bounded Jump List"));
                }
                v.app_id.len()
                    + v.tasks
                        .iter()
                        .map(|v| v.title.len() + v.arguments.len())
                        .sum::<usize>()
            }
            Self::DeleteJumpList { app_id: v } => {
                if !app_id(v) {
                    return Err(invalid("Invalid application ID"));
                }
                v.len()
            }
        };
        Ok(bytes)
    }
}
#[derive(Clone)]
pub enum PlatformServiceValue {
    Done,
    Clipboard(ClipboardContent),
    Dispatched { channel: u32, payload: Vec<u8> },
}
impl fmt::Debug for PlatformServiceValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PlatformServiceValue { payload: [private] }")
    }
}
#[derive(Clone, Debug)]
pub struct PlatformServiceCompletion {
    pub id: ServiceRequestId,
    pub result: ServiceResult<PlatformServiceValue>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlatformServiceEvent {
    TrayActivated,
    TrayAction(u32),
    ShellError(ServiceError),
}

pub(crate) struct NativeServices {
    #[cfg(target_os = "windows")]
    owner: Arc<winit::window::Window>,
    queue: queue::ServiceQueue,
    #[cfg(target_os = "windows")]
    native: Result<windows::Backend, ServiceError>,
}
impl NativeServices {
    pub(crate) fn new(window: Arc<winit::window::Window>, waker: AppWaker) -> Self {
        let queue = queue::ServiceQueue::new(waker.clone());
        #[cfg(target_os = "windows")]
        let native = windows::Backend::new(window.clone(), waker);
        // Never accept requests without an installed native lifetime observer.
        // Failure is visible to the first submit, not a queued silent drop.
        #[cfg(target_os = "windows")]
        if native.is_err() {
            queue.close();
        }
        #[cfg(not(target_os = "windows"))]
        let _ = (window, waker);
        Self {
            queue,
            #[cfg(target_os = "windows")]
            owner: window,
            #[cfg(target_os = "windows")]
            native,
        }
    }
    pub(crate) fn handle(&self) -> PlatformServiceHandle {
        self.queue.handle()
    }
    pub(crate) fn close(&mut self) {
        self.queue.close();
        #[cfg(target_os = "windows")]
        {
            self.native = Err(ServiceError::new(
                ServiceErrorKind::OwnerClosed,
                "Service owner is closed",
            ));
        }
    }
    #[cfg(target_os = "windows")]
    fn owner_alive(&self) -> bool {
        windows::owner_alive(&self.owner)
            && self.native.as_ref().is_ok_and(windows::Backend::is_alive)
    }
    pub(crate) fn next(&mut self) -> Option<PlatformServiceCompletion> {
        #[cfg(target_os = "windows")]
        if !self.owner_alive() {
            self.close();
            return None;
        }
        let (id, request) = self.queue.pop()?;
        let result = match request {
            PlatformServiceRequest::Dispatch { channel, payload } => {
                Ok(PlatformServiceValue::Dispatched { channel, payload })
            }
            request => {
                #[cfg(target_os = "windows")]
                {
                    self.native
                        .as_mut()
                        .map_err(|e| e.clone())
                        .and_then(|native| native.execute(request))
                }
                #[cfg(not(target_os = "windows"))]
                {
                    let _ = request;
                    Err(ServiceError::new(
                        ServiceErrorKind::Unsupported,
                        "Native service is unavailable on this platform",
                    ))
                }
            }
        };
        #[cfg(target_os = "windows")]
        if !self.owner_alive() {
            self.close();
            return None;
        }
        Some(PlatformServiceCompletion { id, result })
    }
    pub(crate) fn event(&mut self) -> Option<PlatformServiceEvent> {
        #[cfg(target_os = "windows")]
        if !self.owner_alive() {
            self.close();
            return None;
        }
        #[cfg(target_os = "windows")]
        {
            self.native.as_mut().ok().and_then(windows::Backend::event)
        }
        #[cfg(not(target_os = "windows"))]
        {
            None
        }
    }
}

impl Drop for NativeServices {
    fn drop(&mut self) {
        self.close();
    }
}
