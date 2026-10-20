//! Native desktop host. UI and application models remain independent of the window system.
//! Accessibility bounds and IME anchors are updated on every rendered frame and DPI change.

mod accessibility;
mod chrome;
mod desktop;
mod dialogs;
mod host;
mod ime;
mod input;
mod modality;
mod multi;
mod multi_host;
pub mod printing;
mod services;
mod splash;
mod wake;
pub use services::*;
pub use splash::{SplashSession, SplashState, SplashWorker};

pub use accessibility::AccessibilitySnapshot;
pub use accesskit;
pub use chrome::{TitleBar, WindowChrome};
pub use desktop::{DesktopPointer, WindowGeometry};
pub use dialogs::*;
pub use host::run;
pub use multi::{DesktopApplication, WindowKey, WindowSpec, run_multi};
use rust_desktop_ui_core::{InputEvent, Rect, Scene, Size};
pub use rust_desktop_ui_render_wgpu::RendererStats;
use std::time::Duration;
pub use wake::AppWaker;

/// Committed text is deliberately separate from logical shortcut keys.
#[derive(Debug, Clone)]
pub enum PlatformEvent {
    Input(InputEvent),
    Text(String),
    ImePreedit(String, Option<(usize, usize)>),
    ImeCommit(String),
    ImeCancel,
    Paste(String),
    Tick,
    Accessibility(accesskit::ActionRequest),
}

#[derive(Debug, Clone)]
pub enum ClipboardRequest {
    Copy(String),
    Paste,
}

#[derive(Debug, Default)]
pub struct EventResponse {
    pub redraw: bool,
    pub clipboard: Vec<ClipboardRequest>,
}

impl EventResponse {
    pub fn redraw() -> Self {
        Self {
            redraw: true,
            clipboard: Vec::new(),
        }
    }
}

/// Implement this trait to run a retained scene in a native GPU window.
/// `accessibility` supplies a complete connected tree; the host sends only changed nodes.
pub trait DesktopApp {
    /// A bounded owner-scoped service/dispatcher handle. None revokes the old
    /// owner before suspension/destruction. Native errors arrive as completions.
    fn platform_services_changed(&mut self, services: Option<PlatformServiceHandle>) {
        let _ = services;
    }
    fn platform_service_completed(
        &mut self,
        completion: PlatformServiceCompletion,
    ) -> Result<EventResponse, String> {
        let _ = completion;
        Ok(EventResponse::default())
    }
    fn platform_service_event(
        &mut self,
        event: PlatformServiceEvent,
    ) -> Result<EventResponse, String> {
        let _ = event;
        Ok(EventResponse::default())
    }

    /// Return at most one queued owned native dialog. The host processes it
    /// outside painting; completion is delivered once on this same UI thread.
    fn take_dialog_request(&mut self) -> Option<DialogRequest> {
        None
    }
    fn dialog_completed(&mut self, completion: DialogCompletion) -> Result<EventResponse, String> {
        let _ = completion;
        Ok(EventResponse::default())
    }
    /// Request a native sizing loop from a pointer-down on an application grip.
    fn take_resize_request(&mut self) -> Option<WindowResizeDirection> {
        None
    }
    /// Install the native window's notification handle. Keep collection
    /// subscriptions alive for this view and call `wake` after publishing data.
    /// None disconnects a closing/suspended window. Timers remain independent.
    fn set_waker(&mut self, waker: Option<AppWaker>) {
        let _ = waker;
    }
    fn update(&mut self, viewport: Size, scale: f32) -> Result<(), String>;
    fn scene(&self) -> &Scene;
    /// Called only after the GPU successfully presents a frame. Skipped,
    /// occluded and failed renders do not trigger this lifecycle notification.
    /// Use a waker if this callback changes application state needing a frame.
    fn frame_presented(&mut self, stats: RendererStats) {
        let _ = stats;
    }
    fn event(&mut self, event: PlatformEvent) -> Result<EventResponse, String>;
    fn title(&self) -> String;
    /// Opt into an integrated Windows title bar at window creation. Other
    /// platforms retain their native decorations. The initial `Some`/`None`
    /// choice is fixed for the window lifetime; geometry and theme are refreshed
    /// after each `update`. The system still owns caption buttons and resizing.
    fn title_bar(&self) -> Option<TitleBar> {
        None
    }
    /// Current native chrome geometry, delivered before every `update`. Reserve
    /// `caption_buttons` in the scene when `custom_title_bar` is true. Native
    /// button bounds can be unavailable before the first visible native frame.
    fn window_chrome_changed(&mut self, chrome: WindowChrome) {
        let _ = chrome;
    }
    /// Native outer position and client size in logical pixels. A host calls this
    /// after creation, moves, resizes and DPI changes; it does not move the window
    /// back to the application's initial placement on subsequent frames.
    fn window_geometry_changed(
        &mut self,
        position: rust_desktop_ui_core::Point,
        size: Size,
        scale: f32,
    ) {
        let _ = (position, size, scale);
    }
    /// A requested native placement is applied. Called before its geometry
    /// notification, including initial creation (revision zero for `run`).
    /// Applications may ignore stale geometry while awaiting a newer revision.
    fn window_placement_applied(&mut self, revision: u64) {
        let _ = revision;
    }
    /// Screen geometry for transforming points between native windows. The
    /// client origin is physical; client size and outer position are logical.
    /// This callback is omitted where the window system hides global positions.
    fn desktop_geometry_changed(&mut self, geometry: WindowGeometry) {
        let _ = geometry;
    }
    /// Windows supplies this immediately before each pointer move, button or
    /// wheel event. Hit testing follows native stacking and client boundaries,
    /// including during capture. Other platforms do not supply this context yet.
    fn desktop_pointer_changed(&mut self, pointer: DesktopPointer) {
        let _ = pointer;
    }
    /// Native capture was cancelled or transferred. Normal release after the
    /// final mouse-up does not call this method. No pointer-up is synthesized.
    fn pointer_capture_lost(&mut self) -> Result<EventResponse, String> {
        Ok(EventResponse::default())
    }
    /// A caret in logical client coordinates. `None` disables native IME input.
    fn ime_cursor(&self) -> Option<Rect> {
        None
    }
    /// Stable editor identity. Changing it resets the native IME session before
    /// accepting composition for another field; return the focused text widget.
    fn ime_target(&self) -> Option<rust_desktop_ui_core::WidgetId> {
        None
    }
    /// Marks protected input. Windows disables native composition for this
    /// target; other platforms receive the Password input-purpose hint.
    /// It does not replace masking, clipboard or accessibility protection.
    fn ime_is_password(&self) -> bool {
        false
    }
    /// Report the editor's composition state so programmatic changes and
    /// same-field pointer placement can cancel the OS composition as well.
    fn ime_composition_active(&self) -> Option<bool> {
        None
    }
    /// Request a timer only while an animation, caret or background request is active.
    fn wake_after(&self) -> Option<Duration> {
        None
    }
    /// All bounds must be in physical client pixels; use `scale` to convert the scene.
    fn accessibility(&mut self, scale: f32) -> accesskit::TreeUpdate {
        let _ = scale;
        let mut node = accesskit::Node::new(accesskit::Role::Window);
        node.set_label(self.title());
        accesskit::TreeUpdate {
            nodes: vec![(accesskit::NodeId(1), node)],
            tree: Some(accesskit::TreeInfo::new(accesskit::NodeId(1))),
            tree_id: accesskit::TreeId::ROOT,
            focus: accesskit::NodeId(1),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowResizeDirection {
    BottomLeft,
    BottomRight,
}
pub(crate) fn begin_resize(
    window: &winit::window::Window,
    direction: WindowResizeDirection,
) -> Result<(), String> {
    window
        .drag_resize_window(match direction {
            WindowResizeDirection::BottomLeft => winit::window::ResizeDirection::SouthWest,
            WindowResizeDirection::BottomRight => winit::window::ResizeDirection::SouthEast,
        })
        .map_err(|error| error.to_string())
}

#[derive(Clone, Debug)]
pub struct RunOptions {
    pub width: f64,
    pub height: f64,
    /// Optional minimum client size in logical pixels. `None` leaves limits to the OS.
    pub min_size: Option<Size>,
    /// Append renderer counters to the native title for diagnostics.
    pub show_render_stats: bool,
    /// Developer smoke: render, resize, render again, then exit with diagnostics.
    pub smoke_test: bool,
    /// During `smoke_test`, destroy the device after resize, then require a
    /// successful frame through the ordinary renderer recreation path.
    /// This is controlled device destruction, not a physical driver failure.
    pub smoke_device_recovery: bool,
    /// Allow initial native activation. False is useful for passive previews
    /// and accessibility inspection while another application keeps focus.
    pub activate: bool,
}

impl Default for RunOptions {
    fn default() -> Self {
        Self {
            width: 1200.0,
            height: 800.0,
            min_size: Some(Size::new(480.0, 320.0)),
            show_render_stats: true,
            smoke_test: false,
            smoke_device_recovery: false,
            activate: true,
        }
    }
}
