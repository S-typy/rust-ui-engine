//! Coalesced, thread-safe notifications for an otherwise sleeping native loop.
use std::sync::{
    Arc, Weak,
    atomic::{AtomicBool, Ordering},
};
use winit::{
    event_loop::{ActiveEventLoop, EventLoopProxy},
    window::{Window, WindowId},
};

pub(crate) enum HostEvent {
    Accessibility(accesskit_winit::Event, WakeToken),
    Wake(WindowId, WakeToken),
}
// A queued token keeps the allocation identity reserved even if every handle
// has been dropped. Native WindowId/ HWND reuse cannot route it to a new view.
#[derive(Clone)]
pub(crate) struct WakeToken(Weak<State>);

#[derive(Clone)]
struct AccessibilityForwarder {
    id: WindowId,
    proxy: EventLoopProxy<HostEvent>,
    token: WakeToken,
}
impl AccessibilityForwarder {
    fn send(&self, window_event: accesskit_winit::WindowEvent) {
        let _ = self.proxy.send_event(HostEvent::Accessibility(
            accesskit_winit::Event {
                window_id: self.id,
                window_event,
            },
            self.token.clone(),
        ));
    }
}
impl accesskit::ActivationHandler for AccessibilityForwarder {
    fn request_initial_tree(&mut self) -> Option<accesskit::TreeUpdate> {
        self.send(accesskit_winit::WindowEvent::InitialTreeRequested);
        None
    }
}
impl accesskit::ActionHandler for AccessibilityForwarder {
    fn do_action(&mut self, request: accesskit::ActionRequest) {
        self.send(accesskit_winit::WindowEvent::ActionRequested(request));
    }
}
impl accesskit::DeactivationHandler for AccessibilityForwarder {
    fn deactivate_accessibility(&mut self) {
        self.send(accesskit_winit::WindowEvent::AccessibilityDeactivated);
    }
}

pub(crate) fn accessibility_adapter(
    event_loop: &ActiveEventLoop,
    window: &Window,
    proxy: EventLoopProxy<HostEvent>,
    waker: &AppWaker,
) -> accesskit_winit::Adapter {
    let forwarder = AccessibilityForwarder {
        id: window.id(),
        proxy,
        token: WakeToken(Arc::downgrade(&waker.0)),
    };
    accesskit_winit::Adapter::with_direct_handlers(
        event_loop,
        window,
        forwarder.clone(),
        forwarder.clone(),
        forwarder,
    )
}

/// A window-scoped notification handle. May be cloned into workers or collection
/// callbacks. `wake` queues at most one outstanding notification; the host calls
/// `DesktopApp::event(Tick)` and requests a frame, where pending data is applied.
/// It never runs application code on the calling thread. Closed windows reject
/// notifications; a recreated native window receives a new handle.
#[derive(Clone)]
pub struct AppWaker(Arc<State>);
struct State {
    pending: AtomicBool,
    active: AtomicBool,
    send: Box<dyn Fn(WakeToken) -> bool + Send + Sync>,
}
impl std::fmt::Debug for AppWaker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppWaker")
            .field("active", &self.0.active.load(Ordering::Acquire))
            .field("pending", &self.0.pending.load(Ordering::Acquire))
            .finish()
    }
}
impl AppWaker {
    fn new(send: impl Fn(WakeToken) -> bool + Send + Sync + 'static) -> Self {
        Self(Arc::new(State {
            pending: AtomicBool::new(false),
            active: AtomicBool::new(true),
            send: Box::new(send),
        }))
    }
    pub(crate) fn for_window(proxy: EventLoopProxy<HostEvent>, id: WindowId) -> Self {
        Self::new(move |token| proxy.send_event(HostEvent::Wake(id, token)).is_ok())
    }
    /// True means the notification was queued or coalesced with a pending one.
    /// False means the native window or its event loop has closed.
    pub fn wake(&self) -> bool {
        if !self.0.active.load(Ordering::Acquire) {
            return false;
        }
        if self.0.pending.swap(true, Ordering::AcqRel) {
            return self.0.active.load(Ordering::Acquire);
        }
        if !(self.0.send)(WakeToken(Arc::downgrade(&self.0))) {
            self.deactivate();
            return false;
        }
        true
    }
    pub(crate) fn matches(&self, token: &WakeToken) -> bool {
        self.0.active.load(Ordering::Acquire) && token.0.ptr_eq(&Arc::downgrade(&self.0))
    }
    pub(crate) fn acknowledge(&self, token: &WakeToken) -> bool {
        if !self.matches(token) {
            return false;
        }
        self.0.pending.store(false, Ordering::Release);
        true
    }
    pub(crate) fn deactivate(&self) {
        self.0.active.store(false, Ordering::Release);
        self.0.pending.store(false, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;
    #[test]
    fn concurrent_notifications_coalesce_and_rearm_without_polling() {
        let sent = Arc::new(AtomicUsize::new(0));
        let count = sent.clone();
        let waker = AppWaker::new(move |_| {
            count.fetch_add(1, Ordering::Relaxed);
            true
        });
        std::thread::scope(|scope| {
            for _ in 0..8 {
                let w = &waker;
                scope.spawn(move || {
                    for _ in 0..1000 {
                        assert!(w.wake());
                    }
                });
            }
        });
        assert_eq!(sent.load(Ordering::Relaxed), 1);
        assert!(waker.acknowledge(&WakeToken(Arc::downgrade(&waker.0))));
        assert!(waker.wake());
        assert_eq!(sent.load(Ordering::Relaxed), 2);
        waker.deactivate();
        assert!(!waker.wake());
        assert_eq!(sent.load(Ordering::Relaxed), 2);
    }
    #[test]
    fn a_closed_loop_rejects_future_notifications() {
        let waker = AppWaker::new(|_| false);
        assert!(!waker.wake());
        assert!(!waker.clone().wake());
    }
    #[test]
    fn queued_tokens_cannot_acknowledge_recreated_windows() {
        let (sender, receiver) = std::sync::mpsc::channel();
        let old = AppWaker::new(move |token| sender.send(token).is_ok());
        assert!(old.wake());
        let stale = receiver.recv().unwrap();
        old.deactivate();
        assert!(!old.acknowledge(&stale));
        drop(old);
        assert!(stale.0.upgrade().is_none());

        let (sender, receiver) = std::sync::mpsc::channel();
        let current = AppWaker::new(move |token| sender.send(token).is_ok());
        assert!(current.wake());
        let live = receiver.recv().unwrap();
        // Accessibility uses the same lifetime identity, without acknowledging
        // or rearming a pending data notification.
        assert!(current.matches(&live));
        assert!(!current.matches(&stale));
        // The native ID may be equal: identity is still checked independently.
        assert!(!current.acknowledge(&stale));
        assert!(current.wake());
        assert!(receiver.try_recv().is_err()); // Stale delivery did not rearm.
        assert!(current.acknowledge(&live));
        assert!(current.wake());
        assert!(receiver.try_recv().is_ok());
    }
}
