//! Native desktop host. UI and application models remain independent of the window system.
//! Accessibility bounds and IME anchors are updated on every rendered frame and DPI change.

mod host;
mod ime;
mod input;

pub use accesskit;
pub use host::run;
use rust_desktop_ui_core::{InputEvent, Rect, Scene, Size};
pub use rust_desktop_ui_render_wgpu::RendererStats;
use std::time::Duration;

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
    fn update(&mut self, viewport: Size, scale: f32) -> Result<(), String>;
    fn scene(&self) -> &Scene;
    fn event(&mut self, event: PlatformEvent) -> Result<EventResponse, String>;
    fn title(&self) -> String;
    /// A caret in logical client coordinates. `None` disables native IME input.
    fn ime_cursor(&self) -> Option<Rect> {
        None
    }
    /// Stable editor identity. Changing it resets the native IME session before
    /// accepting composition for another field; return the focused text widget.
    fn ime_target(&self) -> Option<rust_desktop_ui_core::WidgetId> {
        None
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
}

impl Default for RunOptions {
    fn default() -> Self {
        Self {
            width: 1200.0,
            height: 800.0,
            min_size: Some(Size::new(480.0, 320.0)),
            show_render_stats: true,
            smoke_test: false,
        }
    }
}
