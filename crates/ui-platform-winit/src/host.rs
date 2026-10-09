use crate::{ClipboardRequest, DesktopApp, EventResponse, PlatformEvent, RunOptions, input};
use accesskit::{Node, NodeId, TreeId};
use accesskit_winit::{
    Adapter, Event as AccessibilityEvent, WindowEvent as AccessibilityWindowEvent,
};
use rust_desktop_ui_core::{InputEvent, Modifiers, Point, Size};
use rust_desktop_ui_render_wgpu::{GpuRenderer, RenderOutcome, SkipReason};
use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
    time::{Duration, Instant},
};
use winit::{
    application::ApplicationHandler,
    dpi::{LogicalPosition, LogicalSize},
    event::{ElementState, Ime, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy},
    window::{Window, WindowId},
};

const RETRY: Duration = Duration::from_millis(16);
const SMOKE_TIMEOUT: Duration = Duration::from_secs(30);

pub fn run(app: impl DesktopApp + 'static, options: RunOptions) -> Result<(), String> {
    if !options.width.is_finite()
        || !options.height.is_finite()
        || options.width <= 0.0
        || options.height <= 0.0
    {
        return Err("Window dimensions must be finite and positive".into());
    }
    let event_loop = EventLoop::<AccessibilityEvent>::with_user_event()
        .build()
        .map_err(|e| e.to_string())?;
    let proxy = event_loop.create_proxy();
    let smoke_deadline = options.smoke_test.then(|| Instant::now() + SMOKE_TIMEOUT);
    let mut host = Host {
        app,
        options,
        proxy,
        window: None,
        renderer: None,
        adapter: None,
        accessible_nodes: HashMap::new(),
        clipboard: None,
        modifiers: Modifiers::default(),
        cursor: (0.0, 0.0),
        ime: crate::ime::ImeSession::default(),
        focused: true,
        occluded: false,
        wake_at: None,
        retry_at: None,
        smoke_deadline,
        smoke_resize: None,
        smoke_resized: false,
        frames: 0,
        recoveries: 0,
        failure: None,
    };
    event_loop.run_app(&mut host).map_err(|e| e.to_string())?;
    host.failure.map_or(Ok(()), Err)
}

struct Host<A> {
    app: A,
    options: RunOptions,
    proxy: EventLoopProxy<AccessibilityEvent>,
    window: Option<Arc<Window>>,
    renderer: Option<GpuRenderer>,
    adapter: Option<Adapter>,
    accessible_nodes: HashMap<NodeId, Node>,
    clipboard: Option<arboard::Clipboard>,
    modifiers: Modifiers,
    cursor: (f64, f64),
    ime: crate::ime::ImeSession,
    focused: bool,
    occluded: bool,
    wake_at: Option<Instant>,
    retry_at: Option<Instant>,
    smoke_deadline: Option<Instant>,
    smoke_resize: Option<winit::dpi::PhysicalSize<u32>>,
    smoke_resized: bool,
    frames: u64,
    recoveries: u8,
    failure: Option<String>,
}

impl<A: DesktopApp> Host<A> {
    fn fail(&mut self, event_loop: &ActiveEventLoop, error: impl ToString) {
        self.failure = Some(error.to_string());
        event_loop.exit();
    }

    fn redraw(&self) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    fn pointer(&self) -> Point {
        let scale = self.window.as_ref().map_or(1.0, |w| w.scale_factor());
        Point::new(
            (self.cursor.0 / scale) as f32,
            (self.cursor.1 / scale) as f32,
        )
    }

    fn dispatch(&mut self, event_loop: &ActiveEventLoop, event: PlatformEvent) {
        match self.app.event(event) {
            Ok(response) => self.effects(event_loop, response),
            Err(error) => self.fail(event_loop, error),
        }
        self.sync_ime(event_loop);
    }

    fn sync_ime(&mut self, event_loop: &ActiveEventLoop) {
        let allowed = self.focused && self.app.ime_cursor().is_some();
        let changed = self.ime.set_target(self.app.ime_target(), allowed);
        let cancelled = self
            .ime
            .cancelled_by_editor(self.app.ime_composition_active());
        if changed || cancelled {
            if let Some(window) = &self.window {
                window.set_ime_allowed(false);
                if allowed {
                    window.set_ime_allowed(true);
                }
            }
            match self.app.event(PlatformEvent::ImeCancel) {
                Ok(response) => self.effects(event_loop, response),
                Err(error) => self.fail(event_loop, error),
            }
        }
    }

    fn effects(&mut self, event_loop: &ActiveEventLoop, response: EventResponse) {
        if response.redraw {
            self.redraw();
        }
        let mut requests: VecDeque<_> = response.clipboard.into();
        // A buggy app must not recursively request paste forever.
        for _ in 0..16 {
            let Some(request) = requests.pop_front() else {
                return;
            };
            if self.clipboard.is_none() {
                match arboard::Clipboard::new() {
                    Ok(clipboard) => self.clipboard = Some(clipboard),
                    Err(error) => {
                        eprintln!("Clipboard unavailable: {error}");
                        continue;
                    }
                }
            }
            let clipboard = self.clipboard.as_mut().expect("clipboard initialized");
            match request {
                ClipboardRequest::Copy(text) => {
                    if let Err(error) = clipboard.set_text(text) {
                        eprintln!("Clipboard copy failed: {error}");
                    }
                }
                ClipboardRequest::Paste => match clipboard.get_text() {
                    Ok(text) => match self.app.event(PlatformEvent::Paste(text)) {
                        Ok(response) => {
                            if response.redraw {
                                self.redraw();
                            }
                            requests.extend(response.clipboard);
                        }
                        Err(error) => {
                            self.fail(event_loop, error);
                            return;
                        }
                    },
                    Err(error) => eprintln!("Clipboard paste failed: {error}"),
                },
            }
        }
        if !requests.is_empty() {
            self.fail(
                event_loop,
                "Application issued a recursive clipboard request",
            );
        }
    }

    fn initialize_renderer(&mut self, event_loop: &ActiveEventLoop) {
        let Some(window) = self.window.clone() else {
            return;
        };
        match pollster::block_on(GpuRenderer::new(window)) {
            Ok(renderer) => {
                let info = renderer.adapter_info();
                eprintln!(
                    "GPU adapter={:?} backend={:?} type={:?}",
                    info.name, info.backend, info.device_type
                );
                self.renderer = Some(renderer);
                self.redraw();
            }
            Err(error) => self.fail(event_loop, error),
        }
    }

    fn render(&mut self, event_loop: &ActiveEventLoop) {
        let Some(window) = self.window.clone() else {
            return;
        };
        let physical = window.inner_size();
        if self.occluded || physical.width == 0 || physical.height == 0 {
            return;
        }
        let scale = window.scale_factor();
        let logical = physical.to_logical::<f32>(scale);
        if let Err(error) = self
            .app
            .update(Size::new(logical.width, logical.height), scale as f32)
        {
            self.fail(event_loop, error);
            return;
        }
        self.publish_accessibility(scale as f32);
        self.sync_ime(event_loop);
        let caret = self.app.ime_cursor();
        if let Some(caret) = caret {
            window.set_ime_cursor_area(
                LogicalPosition::new(f64::from(caret.x), f64::from(caret.y)),
                LogicalSize::new(f64::from(caret.width.max(1.0)), f64::from(caret.height)),
            );
        }
        let Some(renderer) = &mut self.renderer else {
            return;
        };
        match renderer.render(self.app.scene(), scale as f32) {
            Ok(RenderOutcome::Presented { .. }) => {
                self.frames += 1;
                self.recoveries = 0;
                self.retry_at = None;
                let stats = renderer.stats();
                window.set_title(&format!(
                    "{} | frames={} glyphs={} missing={}",
                    self.app.title(),
                    self.frames,
                    stats.glyphs,
                    stats.missing_glyphs
                ));
                if self.options.smoke_test {
                    eprintln!(
                        "FRAME {} scale={} rectangles={} text_runs={} glyphs={} missing={} draw_calls={}",
                        self.frames,
                        scale,
                        stats.rectangles,
                        stats.text_runs,
                        stats.glyphs,
                        stats.missing_glyphs,
                        stats.draw_calls
                    );
                    if self.frames == 1 {
                        let target = winit::dpi::PhysicalSize::new(
                            physical.width.saturating_sub(120).max(400),
                            physical.height.saturating_sub(80).max(300),
                        );
                        self.smoke_resize = Some(target);
                        if let Some(actual) = window.request_inner_size(target) {
                            self.smoke_resized = actual == target;
                            if let Err(error) = self
                                .renderer
                                .as_mut()
                                .expect("renderer exists")
                                .resize(actual)
                            {
                                self.fail(event_loop, error);
                                return;
                            }
                        }
                        self.redraw();
                    } else if self.frames >= 3 && self.smoke_resized {
                        eprintln!("SMOKE PASS: hardware text frame, native resize, redraw");
                        event_loop.exit();
                    } else {
                        self.retry_at = Some(Instant::now() + RETRY);
                    }
                }
            }
            Ok(RenderOutcome::Skipped(reason)) => {
                if matches!(reason, SkipReason::Timeout | SkipReason::Reconfigured) {
                    self.retry_at = Some(Instant::now() + RETRY);
                }
            }
            Ok(RenderOutcome::RecoveryRequired(reason)) => {
                if self.recoveries >= 3 {
                    self.fail(
                        event_loop,
                        format!("GPU recovery limit reached: {reason:?}"),
                    );
                    return;
                }
                self.recoveries += 1;
                self.renderer = None;
                self.initialize_renderer(event_loop);
            }
            Err(error) => {
                self.fail(event_loop, error);
                return;
            }
        }
        self.wake_at = self
            .app
            .wake_after()
            .map(|delay| Instant::now() + delay.max(Duration::from_millis(1)));
    }

    fn publish_accessibility(&mut self, scale: f32) {
        let Some(adapter) = &mut self.adapter else {
            return;
        };
        let app = &mut self.app;
        let previous = &mut self.accessible_nodes;
        adapter.update_if_active(|| {
            let mut update = app.accessibility(scale);
            let complete: HashMap<_, _> = update.nodes.iter().cloned().collect();
            update
                .nodes
                .retain(|(id, node)| previous.get(id) != Some(node));
            *previous = complete;
            update
        });
    }
}

impl<A: DesktopApp> ApplicationHandler<AccessibilityEvent> for Host<A> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attributes = Window::default_attributes()
            .with_title(self.app.title())
            .with_inner_size(LogicalSize::new(self.options.width, self.options.height))
            .with_min_inner_size(LogicalSize::new(480.0, 320.0))
            .with_visible(false);
        let window = match event_loop.create_window(attributes) {
            Ok(window) => Arc::new(window),
            Err(error) => {
                self.fail(event_loop, error);
                return;
            }
        };
        // AccessKit requires creating its adapter before the native window becomes visible.
        self.adapter = Some(Adapter::with_event_loop_proxy(
            event_loop,
            &window,
            self.proxy.clone(),
        ));
        self.accessible_nodes.clear();
        window.set_visible(true);
        self.window = Some(window);
        self.initialize_renderer(event_loop);
    }

    fn suspended(&mut self, event_loop: &ActiveEventLoop) {
        self.dispatch(
            event_loop,
            PlatformEvent::Input(InputEvent::WindowFocused(false)),
        );
        self.dispatch(event_loop, PlatformEvent::ImeCancel);
        self.ime = crate::ime::ImeSession::default();
        self.wake_at = None;
        self.retry_at = None;
        self.adapter = None;
        self.renderer = None;
        self.window = None;
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: AccessibilityEvent) {
        if self.window.as_ref().map(|w| w.id()) != Some(event.window_id) {
            return;
        }
        match event.window_event {
            AccessibilityWindowEvent::InitialTreeRequested => {
                self.accessible_nodes.clear();
                self.redraw();
            }
            AccessibilityWindowEvent::ActionRequested(request)
                if request.target_tree == TreeId::ROOT =>
            {
                self.dispatch(event_loop, PlatformEvent::Accessibility(request));
            }
            AccessibilityWindowEvent::AccessibilityDeactivated => self.accessible_nodes.clear(),
            _ => {}
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        let Some(window) = self.window.clone().filter(|w| w.id() == id) else {
            return;
        };
        if let Some(adapter) = &mut self.adapter {
            adapter.process_event(&window, &event);
        }
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::RedrawRequested => self.render(event_loop),
            WindowEvent::Resized(size) => {
                if self.smoke_resize == Some(size) {
                    self.smoke_resized = true;
                }
                if let Some(renderer) = &mut self.renderer
                    && let Err(error) = renderer.resize(size)
                {
                    self.fail(event_loop, error);
                    return;
                }
                self.redraw();
            }
            WindowEvent::ScaleFactorChanged { .. } => {
                if let Some(renderer) = &mut self.renderer
                    && let Err(error) = renderer.resize(window.inner_size())
                {
                    self.fail(event_loop, error);
                    return;
                }
                self.redraw();
            }
            WindowEvent::Occluded(value) => {
                self.occluded = value;
                if !value {
                    self.redraw();
                }
            }
            WindowEvent::Focused(value) => {
                self.focused = value;
                if !value {
                    self.modifiers = Modifiers::default();
                    self.ime.disabled();
                    self.dispatch(event_loop, PlatformEvent::ImeCancel);
                }
                self.dispatch(
                    event_loop,
                    PlatformEvent::Input(InputEvent::WindowFocused(value)),
                );
                self.redraw();
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                let value = modifiers.state();
                self.modifiers = Modifiers {
                    shift: value.shift_key(),
                    control: value.control_key(),
                    alt: value.alt_key(),
                    super_key: value.super_key(),
                };
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = (position.x, position.y);
                self.dispatch(
                    event_loop,
                    PlatformEvent::Input(InputEvent::PointerMoved {
                        position: self.pointer(),
                    }),
                );
            }
            WindowEvent::CursorLeft { .. } => {
                self.dispatch(event_loop, PlatformEvent::Input(InputEvent::PointerLeft))
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let position = self.pointer();
                let button = input::button(button);
                let modifiers = self.modifiers;
                let event = if state == ElementState::Pressed {
                    InputEvent::PointerDown {
                        position,
                        button,
                        modifiers,
                    }
                } else {
                    InputEvent::PointerUp {
                        position,
                        button,
                        modifiers,
                    }
                };
                self.dispatch(event_loop, PlatformEvent::Input(event));
            }
            WindowEvent::MouseWheel { delta, .. } => self.dispatch(
                event_loop,
                PlatformEvent::Input(InputEvent::Wheel {
                    position: self.pointer(),
                    delta: input::wheel(delta, window.scale_factor()),
                    modifiers: self.modifiers,
                }),
            ),
            WindowEvent::KeyboardInput {
                event,
                is_synthetic: false,
                ..
            } => {
                let key = input::key(&event.logical_key);
                let modifiers = self.modifiers;
                if event.state == ElementState::Pressed {
                    self.dispatch(
                        event_loop,
                        PlatformEvent::Input(InputEvent::KeyDown {
                            key,
                            modifiers,
                            repeat: event.repeat,
                        }),
                    );
                    // Ctrl+Alt can represent AltGr; printable text then comes from the OS keyboard layout.
                    let text_allowed = (!modifiers.control && !modifiers.alt
                        || modifiers.control && modifiers.alt)
                        && !modifiers.super_key;
                    if text_allowed
                        && !self.ime.composing
                        && let Some(text) = event.text
                    {
                        let text: String = text.chars().filter(|ch| !ch.is_control()).collect();
                        if !text.is_empty() {
                            self.dispatch(event_loop, PlatformEvent::Text(text));
                        }
                    }
                } else {
                    self.dispatch(
                        event_loop,
                        PlatformEvent::Input(InputEvent::KeyUp { key, modifiers }),
                    );
                }
            }
            WindowEvent::Ime(Ime::Preedit(text, selection)) => {
                if self.ime.preedit(!text.is_empty()) {
                    self.dispatch(event_loop, PlatformEvent::ImePreedit(text, selection));
                }
            }
            WindowEvent::Ime(Ime::Commit(text)) => {
                if self.ime.commit() {
                    self.dispatch(event_loop, PlatformEvent::ImeCommit(text));
                }
            }
            WindowEvent::Ime(Ime::Enabled) => self.ime.enabled(),
            WindowEvent::Ime(Ime::Disabled) => {
                self.ime.disabled();
                self.dispatch(event_loop, PlatformEvent::ImeCancel);
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let now = Instant::now();
        if self.smoke_deadline.is_some_and(|at| at <= now) {
            self.fail(
                event_loop,
                "Native smoke timed out before resize and three presented frames",
            );
            return;
        }
        if self.wake_at.is_some_and(|at| at <= now) {
            self.wake_at = None;
            self.dispatch(event_loop, PlatformEvent::Tick);
            self.wake_at = self
                .app
                .wake_after()
                .map(|delay| now + delay.max(Duration::from_millis(1)));
        }
        if self.retry_at.is_some_and(|at| at <= now) {
            self.retry_at = None;
            self.redraw();
        }
        let next = [self.wake_at, self.retry_at, self.smoke_deadline]
            .into_iter()
            .flatten()
            .min();
        event_loop.set_control_flow(next.map_or(ControlFlow::Wait, ControlFlow::WaitUntil));
    }
}
