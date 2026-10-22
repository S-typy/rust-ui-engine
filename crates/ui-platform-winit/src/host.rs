use crate::{
    AccessibilitySnapshot, AppWaker, ClipboardRequest, DesktopApp, EventResponse, PlatformEvent,
    RunOptions, WindowChrome, chrome::NativeChrome, desktop, input, wake::HostEvent,
};
use accesskit_winit::{Adapter, WindowEvent as AccessibilityWindowEvent};
use rust_desktop_ui_core::{InputEvent, Modifiers, Point, Size};
use rust_desktop_ui_render_wgpu::{GpuRenderer, RenderOutcome, SkipReason};
use std::{
    collections::VecDeque,
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

/// Preserve the application's composition state until its focus-loss policy
/// has run. The dispatcher may synchronize native IME after each event.
fn dispatch_focus_transition(focused: bool, mut dispatch: impl FnMut(PlatformEvent)) {
    dispatch(PlatformEvent::Input(InputEvent::WindowFocused(focused)));
    if !focused {
        dispatch(PlatformEvent::ImeCancel);
    }
}

pub fn run(app: impl DesktopApp + 'static, options: RunOptions) -> Result<(), String> {
    if !options.width.is_finite()
        || !options.height.is_finite()
        || options.width <= 0.0
        || options.height <= 0.0
    {
        return Err("Window dimensions must be finite and positive".into());
    }
    if options.min_size.is_some_and(|size| !size.is_valid()) {
        return Err("Minimum window dimensions must be finite and nonnegative".into());
    }
    if options.smoke_device_recovery && !options.smoke_test {
        return Err("Device recovery diagnostic requires smoke_test".into());
    }
    let event_loop = EventLoop::<HostEvent>::with_user_event()
        .build()
        .map_err(|e| e.to_string())?;
    let proxy = event_loop.create_proxy();
    let smoke_deadline = options.smoke_test.then(|| Instant::now() + SMOKE_TIMEOUT);
    let mut host = Host {
        app,
        options,
        proxy,
        window: None,
        app_waker: None,
        services: None,
        chrome: None,
        capture: None,
        renderer: None,
        adapter: None,
        accessible_nodes: AccessibilitySnapshot::default(),
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
        recovery_requested: false,
        recovery_observed: false,
        failure: None,
    };
    event_loop.run_app(&mut host).map_err(|e| e.to_string())?;
    host.failure.take().map_or(Ok(()), Err)
}

struct Host<A> {
    app: A,
    options: RunOptions,
    proxy: EventLoopProxy<HostEvent>,
    app_waker: Option<AppWaker>,
    services: Option<crate::services::NativeServices>,
    window: Option<Arc<Window>>,
    chrome: Option<NativeChrome>,
    capture: Option<desktop::CaptureObserver>,
    renderer: Option<GpuRenderer>,
    adapter: Option<Adapter>,
    accessible_nodes: AccessibilitySnapshot,
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
    recovery_requested: bool,
    recovery_observed: bool,
    failure: Option<String>,
}

impl<A> Drop for Host<A> {
    fn drop(&mut self) {
        if let Some(waker) = &self.app_waker {
            waker.deactivate();
        }
        if let Some(services) = &mut self.services {
            services.close();
        }
    }
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

    fn geometry_changed(&mut self) {
        let Some(window) = &self.window else { return };
        desktop::notify_geometry(window, &mut self.app);
    }

    fn drain_capture_loss(&mut self, event_loop: &ActiveEventLoop) {
        if self
            .capture
            .as_ref()
            .is_some_and(|capture| capture.take_lost())
        {
            match self.app.pointer_capture_lost() {
                Ok(response) => self.effects(event_loop, response),
                Err(error) => self.fail(event_loop, error),
            }
            self.sync_ime(event_loop);
        }
    }

    fn dispatch(&mut self, event_loop: &ActiveEventLoop, event: PlatformEvent) {
        let protected_before = self.app.ime_is_password();
        match self.app.event(event) {
            Ok(response) => self.effects(event_loop, response),
            Err(error) => self.fail(event_loop, error),
        }
        self.sync_ime(event_loop);
        // Publish protected focus before the next queued key, not at the next
        // GPU frame. Screen-reader keyboard echo must see the password role.
        if protected_before != self.app.ime_is_password()
            && let Some(window) = &self.window
            && let Err(error) = self.publish_accessibility(window.scale_factor() as f32)
        {
            self.fail(event_loop, error);
        }
        if let Some(direction) = self.app.take_resize_request()
            && let Some(window) = &self.window
            && let Err(error) = crate::begin_resize(window, direction)
        {
            eprintln!("Native resize unavailable: {error}");
        }
    }

    fn sync_ime(&mut self, event_loop: &ActiveEventLoop) {
        if let Some(window) = &self.window {
            window.set_ime_purpose(if self.app.ime_is_password() {
                winit::window::ImePurpose::Password
            } else {
                winit::window::ImePurpose::Normal
            });
        }
        let allowed = self.focused
            && crate::ime::composition_allowed(self.app.ime_is_password())
            && crate::ime::valid_cursor(self.app.ime_target(), self.app.ime_cursor()).is_some();
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
        // Popup and layout changes must reach native hit testing before the next
        // queued mouse message, not only when a later redraw happens to arrive.
        if let Some(chrome) = &self.chrome {
            chrome.update(self.app.title_bar());
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
        let chrome = self.chrome.as_ref().map_or(
            WindowChrome {
                maximized: window.is_maximized(),
                focused: self.focused,
                ..WindowChrome::default()
            },
            |chrome| chrome.state(&window, self.focused),
        );
        self.app.window_chrome_changed(chrome);
        if let Err(error) = self
            .app
            .update(Size::new(logical.width, logical.height), scale as f32)
        {
            self.fail(event_loop, error);
            return;
        }
        if let Err(error) = self.publish_accessibility(scale as f32) {
            self.fail(event_loop, error);
            return;
        }
        self.sync_ime(event_loop);
        let caret = crate::ime::valid_cursor(self.app.ime_target(), self.app.ime_cursor());
        if let Some(caret) = caret {
            window.set_ime_cursor_area(
                LogicalPosition::new(f64::from(caret.x), f64::from(caret.y)),
                LogicalSize::new(f64::from(caret.width.max(1.0)), f64::from(caret.height)),
            );
        }
        let Some(renderer) = &mut self.renderer else {
            return;
        };
        // Opaque GPU swapchains cover DWM's caption glyph pixels. Paint our own
        // vectors at the native button bounds while Windows retains hit testing,
        // accessibility and all system actions. Ordinary windows avoid the copy.
        let decorated_scene = self.chrome.as_ref().map(|chrome| {
            let mut scene = self.app.scene().clone();
            chrome.paint(&mut scene, self.focused);
            scene
        });
        let scene = decorated_scene.as_ref().unwrap_or_else(|| self.app.scene());
        match renderer.render(scene, scale as f32) {
            Ok(RenderOutcome::Presented { .. }) => {
                self.frames += 1;
                self.recoveries = 0;
                self.retry_at = None;
                let stats = renderer.stats();
                self.app.frame_presented(stats);
                let title = if self.options.show_render_stats {
                    format!(
                        "{} | frames={} glyphs={} missing={}",
                        self.app.title(),
                        self.frames,
                        stats.glyphs,
                        stats.missing_glyphs
                    )
                } else {
                    self.app.title()
                };
                window.set_title(&title);
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
                        let actual = self.chrome.as_ref().map_or_else(
                            || window.request_inner_size(target),
                            |chrome| chrome.request_inner_size(target),
                        );
                        if let Some(actual) = actual {
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
                        if self.options.smoke_device_recovery && !self.recovery_requested {
                            self.recovery_requested = true;
                            self.renderer
                                .as_mut()
                                .expect("renderer exists")
                                .request_device_recovery();
                            eprintln!(
                                "RECOVERY: controlled device destruction requested after resize"
                            );
                            self.redraw();
                        } else if !self.options.smoke_device_recovery || self.recovery_observed {
                            eprintln!(
                                "SMOKE PASS: hardware frame, native resize, redraw; recovery={} images={} uploads={}",
                                self.recovery_observed, stats.images, stats.uploaded_images
                            );
                            event_loop.exit();
                        } else {
                            self.fail(
                                event_loop,
                                "Recovery diagnostic did not traverse renderer recreation",
                            );
                        }
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
                if self.recovery_requested
                    && reason == rust_desktop_ui_render_wgpu::RecoveryReason::DeviceLost
                {
                    self.recovery_observed = true;
                    eprintln!("RECOVERY: ordinary host renderer recreation");
                }
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

    fn publish_accessibility(&mut self, scale: f32) -> Result<(), String> {
        let Some(adapter) = &mut self.adapter else {
            return Ok(());
        };
        let app = &mut self.app;
        let previous = &mut self.accessible_nodes;
        let mut error = None;
        adapter.update_if_active(|| match previous.update(app.accessibility(scale)) {
            Ok(update) => update,
            Err(message) => {
                error = Some(message);
                previous.unchanged()
            }
        });
        error.map_or(Ok(()), Err)
    }
}

impl<A: DesktopApp> ApplicationHandler<HostEvent> for Host<A> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let mut attributes = Window::default_attributes()
            .with_active(self.options.activate)
            .with_title(self.app.title())
            .with_inner_size(LogicalSize::new(self.options.width, self.options.height))
            .with_visible(false);
        if let Some(size) = self.options.min_size {
            attributes = attributes.with_min_inner_size(LogicalSize::new(size.width, size.height));
        }
        let window = match event_loop.create_window(attributes) {
            Ok(window) => Arc::new(window),
            Err(error) => {
                self.fail(event_loop, error);
                return;
            }
        };
        if let Some(bar) = self.app.title_bar() {
            match NativeChrome::attach(window.clone(), bar, self.options.min_size) {
                Ok(chrome) => self.chrome = chrome,
                Err(error) => eprintln!("Using native title bar: {error}"),
            }
        }
        match desktop::CaptureObserver::attach(window.clone()) {
            Ok(capture) => self.capture = Some(capture),
            Err(error) => {
                self.fail(event_loop, error);
                return;
            }
        }
        // AccessKit requires creating its adapter before the native window becomes visible.
        let waker = AppWaker::for_window(self.proxy.clone(), window.id());
        self.adapter = Some(crate::wake::accessibility_adapter(
            event_loop,
            &window,
            self.proxy.clone(),
            &waker,
        ));
        self.accessible_nodes.clear();
        self.app.set_waker(Some(waker.clone()));
        let services = crate::services::NativeServices::new(window.clone(), waker.clone());
        self.app.platform_services_changed(Some(services.handle()));
        self.services = Some(services);
        self.app_waker = Some(waker);
        window.set_visible(true);
        self.focused = window.has_focus();
        self.window = Some(window);
        dispatch_focus_transition(self.focused, |event| self.dispatch(event_loop, event));
        self.app.window_placement_applied(0);
        self.geometry_changed();
        self.initialize_renderer(event_loop);
    }

    fn suspended(&mut self, event_loop: &ActiveEventLoop) {
        if let Some(waker) = self.app_waker.take() {
            waker.deactivate();
        }
        if let Some(mut services) = self.services.take() {
            services.close();
        }
        self.app.platform_services_changed(None);
        self.app.set_waker(None);
        self.focused = false;
        self.modifiers = Modifiers::default();
        self.occluded = false;
        dispatch_focus_transition(false, |event| self.dispatch(event_loop, event));
        self.ime = crate::ime::ImeSession::default();
        self.wake_at = None;
        self.retry_at = None;
        self.adapter = None;
        self.renderer = None;
        self.chrome = None;
        self.capture = None;
        self.window = None;
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: HostEvent) {
        let (event, token) = match event {
            HostEvent::Accessibility(event, token) => (event, token),
            HostEvent::Wake(id, token) => {
                if self.window.as_ref().map(|window| window.id()) == Some(id)
                    && self
                        .app_waker
                        .as_ref()
                        .is_some_and(|waker| waker.acknowledge(&token))
                {
                    self.dispatch(event_loop, PlatformEvent::Tick);
                    self.redraw();
                }
                return;
            }
        };
        self.drain_capture_loss(event_loop);
        if self.failure.is_some() {
            return;
        }
        if self.window.as_ref().map(|w| w.id()) != Some(event.window_id)
            || !self
                .app_waker
                .as_ref()
                .is_some_and(|waker| waker.matches(&token))
        {
            return;
        }
        match event.window_event {
            AccessibilityWindowEvent::InitialTreeRequested => {
                self.accessible_nodes.clear();
                self.redraw();
            }
            AccessibilityWindowEvent::ActionRequested(request)
                if self.accessible_nodes.permits(&request) =>
            {
                if request.action == accesskit::Action::Focus {
                    if !self
                        .window
                        .as_ref()
                        .is_some_and(|w| desktop::accessibility_focus(w))
                    {
                        return;
                    }
                    if !self.focused {
                        self.focused = true;
                        self.dispatch(
                            event_loop,
                            PlatformEvent::Input(InputEvent::WindowFocused(true)),
                        );
                    }
                }
                self.dispatch(event_loop, PlatformEvent::Accessibility(request));
            }
            AccessibilityWindowEvent::AccessibilityDeactivated => self.accessible_nodes.clear(),
            _ => {}
        }
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(waker) = self.app_waker.take() {
            waker.deactivate();
        }
        if let Some(mut services) = self.services.take() {
            services.close();
        }
        self.app.platform_services_changed(None);
        self.app.set_waker(None);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        self.drain_capture_loss(event_loop);
        if self.failure.is_some() {
            return;
        }
        let Some(window) = self.window.clone().filter(|w| w.id() == id) else {
            return;
        };
        let pointer_position = match &event {
            WindowEvent::CursorMoved { position, .. } => Some((position.x, position.y)),
            WindowEvent::MouseInput { .. } | WindowEvent::MouseWheel { .. } => Some(
                self.capture
                    .as_ref()
                    .and_then(|capture| capture.pointer_position(&event, &window))
                    .unwrap_or(self.cursor),
            ),
            _ => None,
        };
        if let Some(context) = pointer_position.and_then(|position| {
            desktop::pointer(
                &window,
                position,
                std::iter::once((crate::WindowKey::MAIN, window.as_ref())),
            )
        }) {
            self.app.desktop_pointer_changed(context);
        }
        if let Some(position) = pointer_position {
            self.cursor = position;
        }
        if let Some(adapter) = &mut self.adapter {
            adapter.process_event(&window, &event);
        }
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::RedrawRequested => self.render(event_loop),
            WindowEvent::Moved(_) => self.geometry_changed(),
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
                self.geometry_changed();
                self.redraw();
            }
            WindowEvent::ScaleFactorChanged { .. } => {
                if let Some(renderer) = &mut self.renderer
                    && let Err(error) = renderer.resize(window.inner_size())
                {
                    self.fail(event_loop, error);
                    return;
                }
                self.geometry_changed();
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
                }
                dispatch_focus_transition(value, |event| self.dispatch(event_loop, event));
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
        // Snapshot a bounded turn; callbacks may enqueue work for a later wake.
        for _ in 0..64 {
            let Some(completion) = self.services.as_mut().and_then(|v| v.next()) else {
                break;
            };
            match self.app.platform_service_completed(completion) {
                Ok(response) => self.effects(event_loop, response),
                Err(error) => {
                    self.fail(event_loop, error);
                    return;
                }
            }
        }
        for _ in 0..128 {
            let Some(event) = self.services.as_mut().and_then(|v| v.event()) else {
                break;
            };
            match self.app.platform_service_event(event) {
                Ok(response) => self.effects(event_loop, response),
                Err(error) => {
                    self.fail(event_loop, error);
                    return;
                }
            }
        }
        self.drain_capture_loss(event_loop);
        if self.failure.is_some() {
            return;
        }
        if let Some(window) = self.window.clone()
            && let Some(request) = self.app.take_dialog_request()
        {
            // A malformed request must not accept an editor draft through blur.
            let request = crate::dialogs::validate_request(request);
            let opened = request.is_ok();
            let completion = match request {
                Err(completion) => completion,
                Ok(request) => {
                    // Preserve the editor's blur policy before cancelling its IME.
                    let was_focused = crate::modality::suspend_input(
                        &mut self.focused,
                        &mut self.modifiers,
                        &mut self.ime,
                    );
                    if was_focused {
                        dispatch_focus_transition(false, |event| self.dispatch(event_loop, event));
                    } else {
                        self.dispatch(event_loop, PlatformEvent::ImeCancel);
                    }
                    window.set_ime_allowed(false);
                    if self.failure.is_some() {
                        return;
                    }
                    crate::dialogs::show(&window, request)
                }
            };
            match self.app.dialog_completed(completion) {
                Ok(response) => self.effects(event_loop, response),
                Err(error) => {
                    self.fail(event_loop, error);
                    return;
                }
            }
            if opened {
                self.focused = window.has_focus();
                if self.focused {
                    self.dispatch(
                        event_loop,
                        PlatformEvent::Input(InputEvent::WindowFocused(true)),
                    );
                }
            }
            window.request_redraw();
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A windowless application probe whose blur policy accepts only a draft
    /// that has no active composition. This observes the production dispatcher
    /// rather than reconstructing its event sequence in the test.
    struct FocusProbe {
        composing: bool,
        focused: bool,
        draft: &'static str,
        accepted: &'static str,
        composition_seen_on_blur: Option<bool>,
        cancellations: usize,
    }

    impl FocusProbe {
        fn new(composing: bool) -> Self {
            Self {
                composing,
                focused: true,
                draft: "edited value",
                accepted: "initial value",
                composition_seen_on_blur: None,
                cancellations: 0,
            }
        }

        fn dispatch(&mut self, event: PlatformEvent) {
            match event {
                PlatformEvent::Input(InputEvent::WindowFocused(focused)) => {
                    self.focused = focused;
                    if !focused {
                        self.composition_seen_on_blur = Some(self.composing);
                        if !self.composing {
                            self.accepted = self.draft;
                        }
                    }
                }
                PlatformEvent::ImeCancel => {
                    self.composing = false;
                    self.cancellations += 1;
                }
                _ => panic!("unexpected event in focus transition"),
            }
        }
    }

    #[test]
    fn focus_loss_exposes_composition_before_cancellation_and_preserves_accepted_value() {
        let mut app = FocusProbe::new(true);
        dispatch_focus_transition(false, |event| app.dispatch(event));
        assert_eq!(app.composition_seen_on_blur, Some(true));
        assert_eq!(app.accepted, "initial value");
        assert_eq!(app.draft, "edited value");
        assert!(!app.focused);
        assert!(!app.composing);
        assert_eq!(app.cancellations, 1);
    }

    #[test]
    fn focus_loss_leaves_noncomposing_draft_acceptance_to_the_application() {
        let mut app = FocusProbe::new(false);
        dispatch_focus_transition(false, |event| app.dispatch(event));
        assert_eq!(app.composition_seen_on_blur, Some(false));
        assert_eq!(app.accepted, "edited value");
        assert!(!app.focused);
        assert!(!app.composing);
        assert_eq!(app.cancellations, 1);
    }

    #[test]
    fn focus_restore_does_not_cancel_composition_or_accept_the_draft() {
        let mut app = FocusProbe::new(true);
        app.focused = false;
        dispatch_focus_transition(true, |event| app.dispatch(event));
        assert!(app.focused);
        assert!(app.composing);
        assert_eq!(app.accepted, "initial value");
        assert_eq!(app.composition_seen_on_blur, None);
        assert_eq!(app.cancellations, 0);
    }
}
