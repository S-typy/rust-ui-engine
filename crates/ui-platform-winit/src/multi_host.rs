use crate::{
    ClipboardRequest, DesktopApp, DesktopApplication, EventResponse, PlatformEvent, WindowChrome,
    WindowKey, WindowSpec, chrome::NativeChrome, input, multi::validate_specs,
};
use accesskit::{Node, NodeId, TreeId};
use accesskit_winit::{
    Adapter, Event as AccessibilityEvent, WindowEvent as AccessibilityWindowEvent,
};
use rust_desktop_ui_core::{InputEvent, Modifiers, Point, Size};
use rust_desktop_ui_render_wgpu::{GpuRenderer, RenderOutcome, SkipReason};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::Arc,
    time::{Duration, Instant},
};
use winit::{
    application::ApplicationHandler,
    dpi::{LogicalPosition, LogicalSize},
    event::{ElementState, Ime, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy},
    window::{Window, WindowAttributes, WindowId},
};

const RETRY: Duration = Duration::from_millis(16);
const SMOKE_TIMEOUT: Duration = Duration::from_secs(30);

pub(crate) fn run(app: impl DesktopApplication + 'static) -> Result<(), String> {
    let event_loop = EventLoop::<AccessibilityEvent>::with_user_event()
        .build()
        .map_err(|error| error.to_string())?;
    let mut host = MultiHost {
        app,
        proxy: event_loop.create_proxy(),
        windows: HashMap::new(),
        native_keys: HashMap::new(),
        clipboard: None,
        failure: None,
        suspended: true,
    };
    event_loop
        .run_app(&mut host)
        .map_err(|error| error.to_string())?;
    host.failure.map_or(Ok(()), Err)
}

struct MultiHost<A> {
    app: A,
    proxy: EventLoopProxy<AccessibilityEvent>,
    windows: HashMap<WindowKey, WindowState>,
    native_keys: HashMap<WindowId, WindowKey>,
    clipboard: Option<arboard::Clipboard>,
    failure: Option<String>,
    suspended: bool,
}

// Drop GPU surfaces and native adapters before their corresponding Window.
struct WindowState {
    renderer: Option<GpuRenderer>,
    adapter: Adapter,
    chrome: Option<NativeChrome>,
    window: Arc<Window>,
    spec: WindowSpec,
    accessible_nodes: HashMap<NodeId, Node>,
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
    smoke_done: bool,
    frames: u64,
    recoveries: u8,
}

/// A retiring view is still available here. The blur observes an active
/// composition before cancellation; focus loss must not be sent twice.
fn retire_view(app: &mut dyn DesktopApp, focused: bool) -> Result<Vec<EventResponse>, String> {
    let mut responses = Vec::with_capacity(3);
    if focused {
        responses.push(app.event(PlatformEvent::Input(InputEvent::WindowFocused(false)))?);
    }
    responses.push(app.event(PlatformEvent::ImeCancel)?);
    responses.push(app.event(PlatformEvent::Input(InputEvent::PointerLeft))?);
    Ok(responses)
}

fn with_view_by_key<T>(
    application: &mut impl DesktopApplication,
    key: WindowKey,
    operation: impl FnOnce(&mut dyn DesktopApp) -> Result<T, String>,
) -> Result<T, String> {
    let view = application
        .window_mut(key)
        .ok_or("Retained window view is absent")?;
    operation(view)
}

impl<A: DesktopApplication> MultiHost<A> {
    fn fail(&mut self, event_loop: &ActiveEventLoop, error: impl ToString) {
        self.failure = Some(error.to_string());
        event_loop.exit();
    }

    fn redraw_all(&self) {
        for state in self.windows.values() {
            state.window.request_redraw();
        }
    }

    fn with_view(
        &mut self,
        key: WindowKey,
        operation: impl FnOnce(
            &mut WindowState,
            &mut dyn DesktopApp,
            &mut Option<arboard::Clipboard>,
        ) -> Result<bool, String>,
    ) -> Result<(), String> {
        let state = self
            .windows
            .get_mut(&key)
            .ok_or("Native window is absent")?;
        let redraw = with_view_by_key(&mut self.app, key, |app| {
            operation(state, app, &mut self.clipboard)
        })?;
        if redraw {
            self.redraw_all();
        }
        Ok(())
    }

    fn reconcile(&mut self, event_loop: &ActiveEventLoop) -> Result<(), String> {
        if self.suspended {
            return Ok(());
        }
        let specs = self.app.windows()?;
        validate_specs(&specs)?;
        // Validate the complete requested transition before creating or retiring anything.
        for spec in &specs {
            if self.app.window(spec.key).is_none() {
                return Err(format!("Missing view for native window {:?}", spec.key));
            }
            if let Some(state) = self.windows.get(&spec.key)
                && state.spec.owner != spec.owner
            {
                return Err("Changing a live native window owner requires a new WindowKey".into());
            }
        }
        let wanted: HashSet<_> = specs.iter().map(|spec| spec.key).collect();
        let mut retiring: Vec<_> = self
            .windows
            .keys()
            .copied()
            .filter(|key| !wanted.contains(key))
            .collect();
        let removed = !retiring.is_empty();
        for key in &retiring {
            if self.app.window(*key).is_none() {
                return Err("A retiring view must remain available until window_closed".into());
            }
        }
        // Owned windows are destroyed before their owners, including main-window shutdown.
        retiring.sort_by_key(|key| {
            let mut depth = 0;
            let mut owner = self.windows.get(key).and_then(|state| state.spec.owner);
            while let Some(key) = owner {
                depth += 1;
                owner = self.windows.get(&key).and_then(|state| state.spec.owner);
            }
            std::cmp::Reverse(depth)
        });
        for key in retiring {
            let mut state = self.windows.remove(&key).expect("retiring window exists");
            let app = self
                .app
                .window_mut(key)
                .ok_or("Retiring view disappeared")?;
            for response in retire_view(app, state.focused)? {
                state.effects(app, &mut self.clipboard, response)?;
            }
            state.window.set_ime_allowed(false);
            self.native_keys.remove(&state.window.id());
            drop(state);
            self.app.window_closed(key);
        }
        if specs.is_empty() {
            event_loop.exit();
            return Ok(());
        }
        let mut pending: Vec<_> = specs
            .into_iter()
            .filter(|spec| !self.windows.contains_key(&spec.key))
            .collect();
        let changed = removed || !pending.is_empty();
        while !pending.is_empty() {
            let index = pending
                .iter()
                .position(|spec| spec.owner.is_none_or(|key| self.windows.contains_key(&key)))
                .ok_or("Cannot resolve native window ownership")?;
            let spec = pending.remove(index);
            let mut attributes = Window::default_attributes()
                .with_inner_size(LogicalSize::new(spec.options.width, spec.options.height))
                .with_visible(false)
                .with_active(spec.activate);
            if let Some(size) = spec.options.min_size {
                attributes =
                    attributes.with_min_inner_size(LogicalSize::new(size.width, size.height));
            }
            if let Some(position) = spec.position {
                attributes = attributes.with_position(LogicalPosition::new(position.x, position.y));
            }
            if let Some(owner) = spec.owner {
                attributes = owned_attributes(attributes, &self.windows[&owner].window)?;
            }
            let app = self
                .app
                .window_mut(spec.key)
                .ok_or("New window view disappeared")?;
            attributes = attributes.with_title(app.title());
            let window = Arc::new(
                event_loop
                    .create_window(attributes)
                    .map_err(|e| e.to_string())?,
            );
            let chrome = if let Some(bar) = app.title_bar() {
                match NativeChrome::attach(window.clone(), bar, spec.options.min_size) {
                    Ok(chrome) => chrome,
                    Err(error) => {
                        eprintln!("Using native title bar: {error}");
                        None
                    }
                }
            } else {
                None
            };
            let adapter = Adapter::with_event_loop_proxy(event_loop, &window, self.proxy.clone());
            let mut state = WindowState {
                renderer: None,
                adapter,
                chrome,
                window,
                accessible_nodes: HashMap::new(),
                modifiers: Modifiers::default(),
                cursor: (0.0, 0.0),
                ime: crate::ime::ImeSession::default(),
                focused: false,
                occluded: false,
                wake_at: None,
                retry_at: None,
                smoke_deadline: spec
                    .options
                    .smoke_test
                    .then(|| Instant::now() + SMOKE_TIMEOUT),
                smoke_resize: None,
                smoke_resized: false,
                smoke_done: false,
                frames: 0,
                recoveries: 0,
                spec,
            };
            state.initialize_renderer()?;
            state.window.set_visible(true);
            state.focused = state.window.has_focus();
            state.dispatch(
                app,
                &mut self.clipboard,
                PlatformEvent::Input(InputEvent::WindowFocused(state.focused)),
            )?;
            state.geometry_changed(app);
            self.native_keys.insert(state.window.id(), state.spec.key);
            self.windows.insert(state.spec.key, state);
        }
        if changed {
            self.redraw_all();
        }
        Ok(())
    }
}

fn owned_attributes(
    attributes: WindowAttributes,
    owner: &Window,
) -> Result<WindowAttributes, String> {
    #[cfg(target_os = "windows")]
    {
        use winit::{
            platform::windows::WindowAttributesExtWindows,
            raw_window_handle::{HasWindowHandle, RawWindowHandle},
        };
        let handle = owner.window_handle().map_err(|error| error.to_string())?;
        let RawWindowHandle::Win32(handle) = handle.as_raw() else {
            return Err("Expected a Win32 owner window".into());
        };
        Ok(attributes.with_owner_window(handle.hwnd.get()))
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = owner;
        Ok(attributes)
    }
}

impl WindowState {
    fn initialize_renderer(&mut self) -> Result<(), String> {
        let renderer = pollster::block_on(GpuRenderer::new(self.window.clone()))
            .map_err(|error| error.to_string())?;
        let info = renderer.adapter_info();
        eprintln!(
            "WINDOW {:?} GPU adapter={:?} backend={:?} type={:?}",
            self.spec.key, info.name, info.backend, info.device_type
        );
        self.renderer = Some(renderer);
        self.window.request_redraw();
        Ok(())
    }

    fn pointer(&self) -> Point {
        let scale = self.window.scale_factor();
        Point::new(
            (self.cursor.0 / scale) as f32,
            (self.cursor.1 / scale) as f32,
        )
    }

    fn geometry_changed(&self, app: &mut dyn DesktopApp) {
        let Ok(position) = self.window.outer_position() else {
            return;
        };
        let scale = self.window.scale_factor();
        let position = position.to_logical::<f32>(scale);
        let size = self.window.inner_size().to_logical::<f32>(scale);
        if size.width <= 0.0 || size.height <= 0.0 {
            return;
        }
        app.window_geometry_changed(
            Point::new(position.x, position.y),
            Size::new(size.width, size.height),
            scale as f32,
        );
    }

    fn effects(
        &mut self,
        app: &mut dyn DesktopApp,
        clipboard: &mut Option<arboard::Clipboard>,
        response: EventResponse,
    ) -> Result<bool, String> {
        let mut redraw = response.redraw;
        let mut requests: VecDeque<_> = response.clipboard.into();
        for _ in 0..16 {
            let Some(request) = requests.pop_front() else {
                return Ok(redraw);
            };
            if clipboard.is_none() {
                match arboard::Clipboard::new() {
                    Ok(value) => *clipboard = Some(value),
                    Err(error) => {
                        eprintln!("Clipboard unavailable: {error}");
                        continue;
                    }
                }
            }
            let clipboard = clipboard.as_mut().expect("clipboard initialized");
            match request {
                ClipboardRequest::Copy(text) => {
                    if let Err(error) = clipboard.set_text(text) {
                        eprintln!("Clipboard copy failed: {error}");
                    }
                }
                ClipboardRequest::Paste => match clipboard.get_text() {
                    Ok(text) => {
                        // Paste always goes back to the requesting view, never the newest window.
                        let response = app.event(PlatformEvent::Paste(text))?;
                        redraw |= response.redraw;
                        requests.extend(response.clipboard);
                    }
                    Err(error) => eprintln!("Clipboard paste failed: {error}"),
                },
            }
        }
        if !requests.is_empty() {
            return Err("Application issued a recursive clipboard request".into());
        }
        Ok(redraw)
    }

    fn sync_ime(
        &mut self,
        app: &mut dyn DesktopApp,
        clipboard: &mut Option<arboard::Clipboard>,
    ) -> Result<bool, String> {
        let allowed = self.focused && app.ime_cursor().is_some();
        let changed = self.ime.set_target(app.ime_target(), allowed);
        let cancelled = self.ime.cancelled_by_editor(app.ime_composition_active());
        let mut redraw = false;
        if changed || cancelled {
            self.window.set_ime_allowed(false);
            if allowed {
                self.window.set_ime_allowed(true)
            }
            let response = app.event(PlatformEvent::ImeCancel)?;
            redraw = self.effects(app, clipboard, response)?;
        }
        if let Some(chrome) = &self.chrome {
            chrome.update(app.title_bar())
        }
        Ok(redraw)
    }

    fn dispatch(
        &mut self,
        app: &mut dyn DesktopApp,
        clipboard: &mut Option<arboard::Clipboard>,
        event: PlatformEvent,
    ) -> Result<bool, String> {
        let response = app.event(event)?;
        let redraw = self.effects(app, clipboard, response)?;
        Ok(self.sync_ime(app, clipboard)? || redraw)
    }

    fn render(
        &mut self,
        app: &mut dyn DesktopApp,
        clipboard: &mut Option<arboard::Clipboard>,
    ) -> Result<bool, String> {
        let physical = self.window.inner_size();
        if self.occluded || physical.width == 0 || physical.height == 0 {
            return Ok(false);
        }
        let scale = self.window.scale_factor();
        let logical = physical.to_logical::<f32>(scale);
        let chrome = self.chrome.as_ref().map_or(
            WindowChrome {
                maximized: self.window.is_maximized(),
                focused: self.focused,
                ..WindowChrome::default()
            },
            |chrome| chrome.state(&self.window, self.focused),
        );
        app.window_chrome_changed(chrome);
        app.update(Size::new(logical.width, logical.height), scale as f32)?;
        let previous = &mut self.accessible_nodes;
        self.adapter.update_if_active(|| {
            let mut update = app.accessibility(scale as f32);
            let complete = update.nodes.iter().cloned().collect();
            update
                .nodes
                .retain(|(id, node)| previous.get(id) != Some(node));
            *previous = complete;
            update
        });
        let redraw = self.sync_ime(app, clipboard)?;
        if let Some(caret) = app.ime_cursor() {
            self.window.set_ime_cursor_area(
                LogicalPosition::new(f64::from(caret.x), f64::from(caret.y)),
                LogicalSize::new(f64::from(caret.width.max(1.0)), f64::from(caret.height)),
            );
        }
        let decorated = self.chrome.as_ref().map(|chrome| {
            let mut scene = app.scene().clone();
            chrome.paint(&mut scene, self.focused);
            scene
        });
        let scene = decorated.as_ref().unwrap_or_else(|| app.scene());
        let renderer = self
            .renderer
            .as_mut()
            .ok_or("Native window renderer is absent")?;
        match renderer
            .render(scene, scale as f32)
            .map_err(|error| error.to_string())?
        {
            RenderOutcome::Presented { .. } => {
                self.frames += 1;
                self.recoveries = 0;
                self.retry_at = None;
                let stats = renderer.stats();
                let title = if self.spec.options.show_render_stats {
                    format!(
                        "{} | frames={} glyphs={} missing={}",
                        app.title(),
                        self.frames,
                        stats.glyphs,
                        stats.missing_glyphs
                    )
                } else {
                    app.title()
                };
                self.window.set_title(&title);
                if self.spec.options.smoke_test && !self.smoke_done {
                    eprintln!(
                        "WINDOW {:?} FRAME {} scale={} rectangles={} text_runs={} glyphs={} missing={} draw_calls={}",
                        self.spec.key,
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
                            || self.window.request_inner_size(target),
                            |chrome| chrome.request_inner_size(target),
                        );
                        if let Some(actual) = actual {
                            self.smoke_resized = actual == target;
                            self.renderer
                                .as_mut()
                                .expect("renderer initialized")
                                .resize(actual)
                                .map_err(|error| error.to_string())?;
                        }
                        self.window.request_redraw();
                    } else if self.frames >= 3 && self.smoke_resized {
                        self.smoke_done = true;
                        self.smoke_deadline = None;
                        eprintln!(
                            "WINDOW {:?} SMOKE PASS: GPU frame, native resize, redraw",
                            self.spec.key
                        );
                    } else {
                        self.retry_at = Some(Instant::now() + RETRY)
                    }
                }
            }
            RenderOutcome::Skipped(reason) => {
                if matches!(reason, SkipReason::Timeout | SkipReason::Reconfigured) {
                    self.retry_at = Some(Instant::now() + RETRY);
                }
            }
            RenderOutcome::RecoveryRequired(reason) => {
                if self.recoveries >= 3 {
                    return Err(format!("GPU recovery limit reached: {reason:?}"));
                }
                self.recoveries += 1;
                self.renderer = None;
                self.initialize_renderer()?;
            }
        }
        self.wake_at = app
            .wake_after()
            .map(|delay| Instant::now() + delay.max(Duration::from_millis(1)));
        Ok(redraw)
    }

    fn event(
        &mut self,
        app: &mut dyn DesktopApp,
        clipboard: &mut Option<arboard::Clipboard>,
        event: WindowEvent,
    ) -> Result<bool, String> {
        self.adapter.process_event(&self.window, &event);
        match event {
            WindowEvent::RedrawRequested => self.render(app, clipboard),
            WindowEvent::Moved(_) => {
                self.geometry_changed(app);
                Ok(false)
            }
            WindowEvent::Resized(size) => {
                if self.smoke_resize == Some(size) {
                    self.smoke_resized = true
                }
                if let Some(renderer) = &mut self.renderer {
                    renderer.resize(size).map_err(|error| error.to_string())?
                }
                self.geometry_changed(app);
                self.window.request_redraw();
                Ok(false)
            }
            WindowEvent::ScaleFactorChanged { .. } => {
                if let Some(renderer) = &mut self.renderer {
                    renderer
                        .resize(self.window.inner_size())
                        .map_err(|error| error.to_string())?
                }
                self.geometry_changed(app);
                self.window.request_redraw();
                Ok(false)
            }
            WindowEvent::Occluded(value) => {
                self.occluded = value;
                if !value {
                    self.window.request_redraw()
                }
                Ok(false)
            }
            WindowEvent::Focused(value) => {
                if self.focused == value {
                    return Ok(false);
                }
                self.focused = value;
                if !value {
                    self.modifiers = Modifiers::default();
                    self.ime.disabled();
                }
                // Focus loss observes composition first, then cancels it. Synchronize
                // IME only after both events so a target reset cannot precede blur.
                let response = app.event(PlatformEvent::Input(InputEvent::WindowFocused(value)))?;
                let mut redraw = self.effects(app, clipboard, response)?;
                if !value {
                    let response = app.event(PlatformEvent::ImeCancel)?;
                    redraw |= self.effects(app, clipboard, response)?;
                    let response = app.event(PlatformEvent::Input(InputEvent::PointerLeft))?;
                    redraw |= self.effects(app, clipboard, response)?;
                }
                redraw |= self.sync_ime(app, clipboard)?;
                self.window.request_redraw();
                Ok(redraw)
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                let value = modifiers.state();
                self.modifiers = Modifiers {
                    shift: value.shift_key(),
                    control: value.control_key(),
                    alt: value.alt_key(),
                    super_key: value.super_key(),
                };
                Ok(false)
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = (position.x, position.y);
                self.dispatch(
                    app,
                    clipboard,
                    PlatformEvent::Input(InputEvent::PointerMoved {
                        position: self.pointer(),
                    }),
                )
            }
            WindowEvent::CursorLeft { .. } => self.dispatch(
                app,
                clipboard,
                PlatformEvent::Input(InputEvent::PointerLeft),
            ),
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
                self.dispatch(app, clipboard, PlatformEvent::Input(event))
            }
            WindowEvent::MouseWheel { delta, .. } => self.dispatch(
                app,
                clipboard,
                PlatformEvent::Input(InputEvent::Wheel {
                    position: self.pointer(),
                    delta: input::wheel(delta, self.window.scale_factor()),
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
                    let mut redraw = self.dispatch(
                        app,
                        clipboard,
                        PlatformEvent::Input(InputEvent::KeyDown {
                            key,
                            modifiers,
                            repeat: event.repeat,
                        }),
                    )?;
                    let text_allowed = (!modifiers.control && !modifiers.alt
                        || modifiers.control && modifiers.alt)
                        && !modifiers.super_key;
                    if text_allowed
                        && !self.ime.composing
                        && let Some(text) = event.text
                    {
                        let text: String = text.chars().filter(|ch| !ch.is_control()).collect();
                        if !text.is_empty() {
                            redraw |= self.dispatch(app, clipboard, PlatformEvent::Text(text))?
                        }
                    }
                    Ok(redraw)
                } else {
                    self.dispatch(
                        app,
                        clipboard,
                        PlatformEvent::Input(InputEvent::KeyUp { key, modifiers }),
                    )
                }
            }
            WindowEvent::Ime(Ime::Preedit(text, selection))
                if self.ime.preedit(!text.is_empty()) =>
            {
                self.dispatch(app, clipboard, PlatformEvent::ImePreedit(text, selection))
            }
            WindowEvent::Ime(Ime::Commit(text)) if self.ime.commit() => {
                self.dispatch(app, clipboard, PlatformEvent::ImeCommit(text))
            }
            WindowEvent::Ime(Ime::Enabled) => {
                self.ime.enabled();
                Ok(false)
            }
            WindowEvent::Ime(Ime::Disabled) => {
                self.ime.disabled();
                self.dispatch(app, clipboard, PlatformEvent::ImeCancel)
            }
            _ => Ok(false),
        }
    }
}

impl<A: DesktopApplication> ApplicationHandler<AccessibilityEvent> for MultiHost<A> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        self.suspended = false;
        if let Err(error) = self.reconcile(event_loop) {
            self.fail(event_loop, error)
        }
    }

    fn suspended(&mut self, event_loop: &ActiveEventLoop) {
        self.suspended = true;
        for (key, state) in &mut self.windows {
            let Some(app) = self.app.window_mut(*key) else {
                self.fail(event_loop, "Suspended window view is absent");
                return;
            };
            let result = retire_view(app, state.focused).and_then(|responses| {
                for response in responses {
                    state.effects(app, &mut self.clipboard, response)?;
                }
                Ok(())
            });
            if let Err(error) = result {
                self.fail(event_loop, error);
                return;
            }
        }
        self.native_keys.clear();
        self.windows.clear();
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: AccessibilityEvent) {
        let Some(key) = self.native_keys.get(&event.window_id).copied() else {
            return;
        };
        let result = self.with_view(key, |state, app, clipboard| match event.window_event {
            AccessibilityWindowEvent::InitialTreeRequested => {
                state.accessible_nodes.clear();
                state.window.request_redraw();
                Ok(false)
            }
            AccessibilityWindowEvent::ActionRequested(request)
                if request.target_tree == TreeId::ROOT =>
            {
                state.dispatch(app, clipboard, PlatformEvent::Accessibility(request))
            }
            AccessibilityWindowEvent::AccessibilityDeactivated => {
                state.accessible_nodes.clear();
                Ok(false)
            }
            _ => Ok(false),
        });
        if let Err(error) = result {
            self.fail(event_loop, error)
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        let Some(key) = self.native_keys.get(&id).copied() else {
            return;
        };
        let result = if matches!(event, WindowEvent::CloseRequested) {
            self.app
                .close_requested(key)
                .and_then(|()| self.reconcile(event_loop))
        } else {
            self.with_view(key, |state, app, clipboard| {
                state.event(app, clipboard, event)
            })
        };
        if let Err(error) = result {
            self.fail(event_loop, error)
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if let Err(error) = self.reconcile(event_loop) {
            self.fail(event_loop, error);
            return;
        }
        let now = Instant::now();
        let keys: Vec<_> = self.windows.keys().copied().collect();
        for key in keys {
            let result = self.with_view(key, |state, app, clipboard| {
                if state.smoke_deadline.is_some_and(|at| at <= now) {
                    return Err(format!("Native smoke timed out for {:?}", key));
                }
                let mut redraw = false;
                if state.wake_at.is_some_and(|at| at <= now) {
                    state.wake_at = None;
                    redraw = state.dispatch(app, clipboard, PlatformEvent::Tick)?;
                    state.wake_at = app
                        .wake_after()
                        .map(|delay| now + delay.max(Duration::from_millis(1)));
                }
                if state.retry_at.is_some_and(|at| at <= now) {
                    state.retry_at = None;
                    state.window.request_redraw();
                }
                Ok(redraw)
            });
            if let Err(error) = result {
                self.fail(event_loop, error);
                return;
            }
        }
        if let Err(error) = self.reconcile(event_loop) {
            self.fail(event_loop, error);
            return;
        }
        let smoke: Vec<_> = self
            .windows
            .values()
            .filter(|state| state.spec.options.smoke_test)
            .collect();
        if !smoke.is_empty() && smoke.iter().all(|state| state.smoke_done) {
            eprintln!("MULTIWINDOW SMOKE PASS: {} native GPU windows", smoke.len());
            event_loop.exit();
        }
        let next = self
            .windows
            .values()
            .flat_map(|state| [state.wake_at, state.retry_at, state.smoke_deadline])
            .flatten()
            .min();
        event_loop.set_control_flow(next.map_or(ControlFlow::Wait, ControlFlow::WaitUntil));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_desktop_ui_core::Scene;

    #[derive(Default)]
    struct ViewProbe {
        scene: Scene,
        events: Vec<&'static str>,
        composing: bool,
        observed: bool,
    }
    impl DesktopApp for ViewProbe {
        fn update(&mut self, _: Size, _: f32) -> Result<(), String> {
            Ok(())
        }
        fn scene(&self) -> &Scene {
            &self.scene
        }
        fn title(&self) -> String {
            "probe".into()
        }
        fn event(&mut self, event: PlatformEvent) -> Result<EventResponse, String> {
            match event {
                PlatformEvent::Input(InputEvent::WindowFocused(false)) => {
                    self.observed = self.composing;
                    self.events.push("blur")
                }
                PlatformEvent::ImeCancel => {
                    self.composing = false;
                    self.events.push("cancel")
                }
                PlatformEvent::Input(InputEvent::PointerLeft) => self.events.push("leave"),
                _ => return Err("Unexpected retirement event".into()),
            }
            Ok(EventResponse::default())
        }
    }

    #[test]
    fn retiring_view_observes_composition_before_cancelling_and_clears_pointer() {
        let mut view = ViewProbe {
            composing: true,
            ..ViewProbe::default()
        };
        retire_view(&mut view, true).unwrap();
        assert!(view.observed);
        assert!(!view.composing);
        assert_eq!(view.events, ["blur", "cancel", "leave"]);
    }

    #[test]
    fn retiring_unfocused_view_does_not_repeat_focus_loss_or_touch_other_view() {
        let mut first = ViewProbe {
            composing: true,
            ..ViewProbe::default()
        };
        let second = ViewProbe {
            composing: true,
            ..ViewProbe::default()
        };
        retire_view(&mut first, false).unwrap();
        assert_eq!(first.events, ["cancel", "leave"]);
        assert!(second.events.is_empty());
        assert!(second.composing);
    }

    struct ApplicationProbe {
        main: ViewProbe,
        floating: ViewProbe,
    }

    impl DesktopApplication for ApplicationProbe {
        fn windows(&mut self) -> Result<Vec<WindowSpec>, String> {
            Ok(vec![])
        }
        fn window(&self, key: WindowKey) -> Option<&dyn DesktopApp> {
            match key.0 {
                0 => Some(&self.main),
                7 => Some(&self.floating),
                _ => None,
            }
        }
        fn window_mut(&mut self, key: WindowKey) -> Option<&mut (dyn DesktopApp + '_)> {
            match key.0 {
                0 => Some(&mut self.main),
                7 => Some(&mut self.floating),
                _ => None,
            }
        }
        fn close_requested(&mut self, _: WindowKey) -> Result<(), String> {
            Ok(())
        }
    }

    #[test]
    fn view_routing_keeps_window_state_separate_and_rejects_stale_keys() {
        let mut application = ApplicationProbe {
            main: ViewProbe {
                composing: true,
                ..ViewProbe::default()
            },
            floating: ViewProbe {
                composing: true,
                ..ViewProbe::default()
            },
        };
        with_view_by_key(&mut application, WindowKey(7), |view| {
            view.event(PlatformEvent::ImeCancel)
        })
        .unwrap();
        assert!(application.main.composing);
        assert!(!application.floating.composing);
        assert!(application.main.events.is_empty());
        assert_eq!(application.floating.events, ["cancel"]);
        assert!(
            with_view_by_key(&mut application, WindowKey(99), |view| {
                view.event(PlatformEvent::ImeCancel)
            })
            .is_err()
        );
        assert!(application.main.composing);
        assert_eq!(application.floating.events, ["cancel"]);
    }
}
