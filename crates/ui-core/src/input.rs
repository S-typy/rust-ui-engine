//! Platform-independent input in logical pixels.

use crate::{Mutation, Point, WidgetId};

/// A mouse or pen button. The runtime currently has one primary pointer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerButton {
    Primary,
    Secondary,
    Middle,
    Other(u16),
}

/// Navigation keys used by the foundation. Text input belongs to a separate API.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Key {
    Tab,
    Enter,
    Space,
    Escape,
    Delete,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    Home,
    End,
    /// A logical key value, not committed text or IME composition.
    Character(String),
    Other,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Modifiers {
    pub shift: bool,
    pub control: bool,
    pub alt: bool,
    pub super_key: bool,
}

/// A normalized host event. Positions and wheel deltas are logical pixels.
#[derive(Debug, Clone, PartialEq)]
pub enum InputEvent {
    PointerMoved {
        position: Point,
    },
    PointerDown {
        position: Point,
        button: PointerButton,
        modifiers: Modifiers,
    },
    PointerUp {
        position: Point,
        button: PointerButton,
        modifiers: Modifiers,
    },
    PointerLeft,
    Wheel {
        position: Point,
        delta: Point,
        modifiers: Modifiers,
    },
    KeyDown {
        key: Key,
        modifiers: Modifiers,
        repeat: bool,
    },
    KeyUp {
        key: Key,
        modifiers: Modifiers,
    },
    WindowFocused(bool),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventPhase {
    Capture,
    Target,
    Bubble,
}

/// One delivery along the immutable route selected at dispatch start.
#[derive(Debug, Clone, PartialEq)]
pub struct RoutedEvent {
    pub input: InputEvent,
    pub target: WidgetId,
    pub current_target: WidgetId,
    pub phase: EventPhase,
}

/// Changes requested by callbacks are applied after the entire route finishes.
#[derive(Default)]
pub struct EventContext {
    pub(crate) propagation_stopped: bool,
    pub(crate) default_prevented: bool,
    pub(crate) commands: Vec<Mutation>,
}

impl EventContext {
    /// Stop deliveries to subsequent nodes. Default behavior remains enabled.
    pub fn stop_propagation(&mut self) {
        self.propagation_stopped = true;
    }

    /// Suppress the runtime's default mouse focus and keyboard Tab behavior.
    pub fn prevent_default(&mut self) {
        self.default_prevented = true;
    }

    pub fn enqueue(&mut self, mutation: Mutation) {
        self.commands.push(mutation);
    }
}
