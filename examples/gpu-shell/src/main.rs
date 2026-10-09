//! Native GPU shell and a shape-only demonstration, before text or controls exist.

mod demo;
mod retained;

use demo::DemoState;
use retained::RetainedDemo;
use rust_desktop_ui_core::{InputEvent, Key as UiKey, Modifiers, Point, PointerButton, Size};
use rust_desktop_ui_render_wgpu::{GpuRenderer, RenderOutcome, SkipReason};
use std::{
    process::ExitCode,
    sync::Arc,
    time::{Duration, Instant},
};
use winit::{
    application::ApplicationHandler,
    dpi::{LogicalSize, PhysicalSize},
    event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::{Key, NamedKey},
    window::{Window, WindowId},
};

const MAX_RECOVERIES: u8 = 3;
const RETRY_DELAY: Duration = Duration::from_millis(16);

#[derive(Default)]
struct WheelAccumulator {
    remainder: f64,
}

impl WheelAccumulator {
    fn rows(&mut self, delta: MouseScrollDelta, scale: f64) -> i32 {
        if !scale.is_finite() || scale <= 0.0 {
            return 0;
        }
        let amount = match delta {
            MouseScrollDelta::LineDelta(_, y) => -f64::from(y) * 3.0,
            // PixelDelta is physical pixels; demo row geometry uses logical pixels.
            MouseScrollDelta::PixelDelta(position) => -position.y / scale / 28.0,
        };
        if !amount.is_finite() {
            return 0;
        }
        let total = (self.remainder + amount).clamp(f64::from(i32::MIN), f64::from(i32::MAX));
        let rows = total.trunc() as i32;
        self.remainder = total - f64::from(rows);
        rows
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SmokeStage {
    Initial,
    ResizeRequested,
    InputApplied,
    Complete,
}

struct SmokeTest {
    stage: SmokeStage,
    deadline: Instant,
    resized: bool,
    resize_target: Option<PhysicalSize<u32>>,
}

struct App {
    renderer: Option<GpuRenderer>,
    window: Option<Arc<Window>>,
    ui: DemoState,
    retained: Option<RetainedDemo>,
    modifiers: Modifiers,
    wheel: WheelAccumulator,
    cursor_physical: (f64, f64),
    frames: u64,
    recovery_attempts: u8,
    retry_at: Option<Instant>,
    occluded: bool,
    failure: Option<String>,
    smoke: Option<SmokeTest>,
}

impl App {
    fn new(smoke: bool, rectangles: bool) -> Result<Self, String> {
        Ok(Self {
            renderer: None,
            window: None,
            ui: DemoState::default(),
            retained: if rectangles {
                None
            } else {
                Some(RetainedDemo::new().map_err(|error| error.to_string())?)
            },
            modifiers: Modifiers::default(),
            wheel: WheelAccumulator::default(),
            cursor_physical: (0.0, 0.0),
            frames: 0,
            recovery_attempts: 0,
            retry_at: None,
            occluded: false,
            failure: None,
            smoke: smoke.then(|| SmokeTest {
                stage: SmokeStage::Initial,
                deadline: Instant::now() + Duration::from_secs(20),
                resized: false,
                resize_target: None,
            }),
        })
    }

    fn fail(&mut self, event_loop: &ActiveEventLoop, message: impl ToString) {
        self.failure = Some(message.to_string());
        self.retry_at = None;
        event_loop.exit();
    }

    fn request_redraw(&self) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    fn dimensions(&self) -> (f32, f32) {
        self.window.as_ref().map_or((0.0, 0.0), |window| {
            let size = window.inner_size().to_logical::<f32>(window.scale_factor());
            (size.width, size.height)
        })
    }

    fn pointer(&self) -> Point {
        let scale = self
            .window
            .as_ref()
            .map_or(1.0, |window| window.scale_factor());
        Point::new(
            (self.cursor_physical.0 / scale) as f32,
            (self.cursor_physical.1 / scale) as f32,
        )
    }

    fn input(&mut self, event_loop: &ActiveEventLoop, input: InputEvent) {
        let (width, height) = self.dimensions();
        if let Some(demo) = &mut self.retained {
            let before = demo.runtime.stats();
            if let Err(error) = demo
                .prepare(Size::new(width, height))
                .and_then(|_| demo.input(input))
            {
                self.fail(event_loop, error);
                return;
            }
            let changed = demo.runtime.needs_update() || demo.runtime.stats() != before;
            self.title();
            if changed {
                self.request_redraw();
            }
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
                    "GPU adapter={:?} backend={:?} type={:?} driver={:?} driver_info={:?}",
                    info.name, info.backend, info.device_type, info.driver, info.driver_info
                );
                self.renderer = Some(renderer);
                self.request_redraw();
            }
            Err(error) => self.fail(event_loop, error),
        }
    }

    fn resize(&mut self, event_loop: &ActiveEventLoop, size: PhysicalSize<u32>) {
        eprintln!("resize physical={}x{}", size.width, size.height);
        if let Some(renderer) = &mut self.renderer
            && let Err(error) = renderer.resize(size)
        {
            self.fail(event_loop, error);
            return;
        }
        if self.retained.is_none() {
            self.ui.scroll(0, self.dimensions().1);
        }
        if let Some(smoke) = &mut self.smoke
            && smoke.stage == SmokeStage::ResizeRequested
            && smoke.resize_target == Some(size)
        {
            smoke.resized = true;
        }
        self.request_redraw();
    }

    fn click(&mut self, x: f32, y: f32, origin: &str) {
        let (width, height) = self.dimensions();
        self.ui.click(x, y, width, height);
        eprintln!(
            "input source={origin} click=({x},{y}) tab={} selected={:?}",
            self.ui.active_tab, self.ui.selected_row
        );
        self.request_redraw();
    }

    fn scroll(&mut self, delta: MouseScrollDelta, origin: &str) {
        let scale = self
            .window
            .as_ref()
            .map_or(1.0, |window| window.scale_factor());
        let rows = self.wheel.rows(delta, scale);
        if rows != 0 {
            self.ui.scroll(rows, self.dimensions().1);
            eprintln!(
                "input source={origin} wheel_rows={rows} first_row={}",
                self.ui.first_row
            );
            self.request_redraw();
        }
    }

    fn title(&self) {
        if let Some(window) = &self.window {
            let size = window.inner_size();
            if let Some(demo) = &self.retained {
                let stats = demo.runtime.stats();
                window.set_title(&format!(
                    "Rust UI Engine | mode=retained | frame={} | layout={} | paint={} | focus={} | hover={} | capture={} | activated={} | removed={} | nodes={} | scroll={:.0} | size={}x{}",
                    self.frames, stats.layout_passes, stats.paint_passes,
                    demo.name(demo.runtime.focused()), demo.name(demo.runtime.hovered()),
                    demo.name(demo.runtime.captured()), demo.activated, demo.removed,
                    demo.runtime.tree().len(), demo.scroll_y, size.width, size.height
                ));
                return;
            }
            let selected = self
                .ui
                .selected_row
                .map_or_else(|| "none".to_owned(), |row| row.to_string());
            window.set_title(&format!(
                "Rust UI Engine | frame={} | first_row={} | selected={} | tab={} | size={}x{}",
                self.frames,
                self.ui.first_row,
                selected,
                self.ui.active_tab,
                size.width,
                size.height
            ));
        }
    }

    fn smoke_stage(&mut self, stage: SmokeStage) {
        if let Some(smoke) = &mut self.smoke {
            smoke.stage = stage;
        }
    }

    fn advance_smoke(&mut self, event_loop: &ActiveEventLoop) {
        let Some(smoke) = &self.smoke else {
            return;
        };
        match smoke.stage {
            SmokeStage::Initial => {
                self.smoke_stage(SmokeStage::ResizeRequested);
                if let (Some(window), Some(smoke)) = (&self.window, &mut self.smoke) {
                    smoke.resize_target =
                        Some(LogicalSize::new(960.0, 640.0).to_physical(window.scale_factor()));
                }
                if let Some(window) = &self.window
                    && let Some(size) = window.request_inner_size(LogicalSize::new(960.0, 640.0))
                {
                    self.resize(event_loop, size);
                }
            }
            SmokeStage::ResizeRequested if smoke.resized => {
                self.smoke_stage(SmokeStage::InputApplied);
                if self.retained.is_some() {
                    let modifiers = Modifiers::default();
                    for input in [
                        // Smoke input is synthetic and may run in a background window.
                        InputEvent::WindowFocused(true),
                        InputEvent::PointerMoved {
                            position: Point::new(50.0, 40.0),
                        },
                        InputEvent::PointerDown {
                            position: Point::new(50.0, 40.0),
                            button: PointerButton::Primary,
                            modifiers,
                        },
                        InputEvent::PointerUp {
                            position: Point::new(600.0, 300.0),
                            button: PointerButton::Primary,
                            modifiers,
                        },
                        InputEvent::KeyDown {
                            key: UiKey::Tab,
                            modifiers,
                            repeat: false,
                        },
                        InputEvent::KeyDown {
                            key: UiKey::Enter,
                            modifiers,
                            repeat: false,
                        },
                        InputEvent::KeyDown {
                            key: UiKey::Tab,
                            modifiers,
                            repeat: false,
                        },
                        InputEvent::KeyDown {
                            key: UiKey::Delete,
                            modifiers,
                            repeat: false,
                        },
                        InputEvent::Wheel {
                            position: Point::new(400.0, 200.0),
                            delta: Point::new(0.0, -84.0),
                            modifiers,
                        },
                    ] {
                        self.input(event_loop, input);
                        if self.failure.is_some() {
                            return;
                        }
                    }
                } else {
                    self.click(150.0, 20.0, "synthetic");
                    self.click(400.0, 242.0, "synthetic");
                    self.scroll(MouseScrollDelta::LineDelta(0.0, -4.0), "synthetic");
                }
            }
            SmokeStage::InputApplied => {
                if let Some(demo) = &self.retained {
                    if demo.activated != 2
                        || demo.removed != 1
                        || demo.scroll_y != 84.0
                        || demo.runtime.focused().is_some()
                        || demo.runtime.captured().is_some()
                    {
                        self.fail(
                            event_loop,
                            "Retained smoke focus/capture/activation/removal/scroll mismatch",
                        );
                        return;
                    }
                    eprintln!(
                        "SMOKE PASS mode=retained frames={} resize=native input=synthetic activated=2 removed=1 scroll=84 layout={} paint={}",
                        self.frames,
                        demo.runtime.stats().layout_passes,
                        demo.runtime.stats().paint_passes
                    );
                    self.smoke_stage(SmokeStage::Complete);
                    event_loop.exit();
                    return;
                }
                if self.ui.active_tab != 1
                    || self.ui.selected_row != Some(2)
                    || self.ui.first_row != 12
                {
                    self.fail(
                        event_loop,
                        "Smoke input state did not match the expected selection and scroll",
                    );
                    return;
                }
                self.smoke_stage(SmokeStage::Complete);
                eprintln!(
                    "SMOKE PASS frames={} resize=native input=synthetic first_row=12 selected=2 tab=1",
                    self.frames
                );
                event_loop.exit();
            }
            _ => {}
        }
    }

    fn render(&mut self, event_loop: &ActiveEventLoop) {
        if self.occluded {
            return;
        }
        let Some(window) = self.window.as_ref() else {
            return;
        };
        let scale = window.scale_factor() as f32;
        let (width, height) = self.dimensions();
        let legacy_scene;
        let scene = if let Some(demo) = &mut self.retained {
            if let Err(error) = demo.prepare(Size::new(width, height)) {
                self.fail(event_loop, error);
                return;
            }
            demo.scene()
        } else {
            legacy_scene = self.ui.build_scene(width, height);
            &legacy_scene
        };
        let Some(renderer) = self.renderer.as_mut() else {
            return;
        };
        match renderer.render(scene, scale) {
            Ok(RenderOutcome::Presented { rectangles }) => {
                self.frames += 1;
                self.recovery_attempts = 0;
                self.retry_at = None;
                if self.frames == 1 {
                    eprintln!("first_frame presented rectangles={rectangles} scale_factor={scale}");
                }
                self.title();
                self.advance_smoke(event_loop);
            }
            Ok(RenderOutcome::Skipped(SkipReason::Timeout | SkipReason::Reconfigured)) => {
                self.retry_at = Some(Instant::now() + RETRY_DELAY);
            }
            Ok(RenderOutcome::Skipped(SkipReason::ZeroSized | SkipReason::Occluded)) => {
                self.retry_at = None
            }
            Ok(RenderOutcome::RecoveryRequired(reason)) => {
                if self.recovery_attempts >= MAX_RECOVERIES {
                    self.fail(
                        event_loop,
                        format!("GPU recovery failed after {MAX_RECOVERIES} attempts: {reason:?}"),
                    );
                    return;
                }
                self.recovery_attempts += 1;
                eprintln!(
                    "GPU recovery attempt={} reason={reason:?}",
                    self.recovery_attempts
                );
                // Drop old surface/device before requesting their replacements.
                self.renderer = None;
                self.initialize_renderer(event_loop);
            }
            Err(error) => self.fail(event_loop, error),
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        self.occluded = false;
        if self.window.is_none() {
            let attributes = Window::default_attributes()
                .with_title("Rust UI Engine | starting")
                .with_inner_size(LogicalSize::new(1250.0, 800.0));
            match event_loop.create_window(attributes) {
                Ok(window) => self.window = Some(Arc::new(window)),
                Err(error) => {
                    self.fail(event_loop, format!("Cannot create native window: {error}"));
                    return;
                }
            }
        }
        if self.renderer.is_none() {
            self.initialize_renderer(event_loop);
        }
        if let Some(window) = &self.window {
            self.input(event_loop, InputEvent::WindowFocused(window.has_focus()));
        }
    }

    fn suspended(&mut self, event_loop: &ActiveEventLoop) {
        // A platform suspension need not be preceded by a focus-loss event.
        self.input(event_loop, InputEvent::WindowFocused(false));
        self.modifiers = Modifiers::default();
        self.cursor_physical = (0.0, 0.0);
        self.renderer = None;
        self.window = None;
        self.retry_at = None;
        eprintln!("suspended: native surface released");
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(window) = self.window.clone() else {
            return;
        };
        if window.id() != window_id {
            return;
        }
        match event {
            WindowEvent::CloseRequested => {
                eprintln!("close_requested");
                event_loop.exit();
            }
            WindowEvent::Resized(size) => self.resize(event_loop, size),
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                eprintln!("scale_factor_changed={scale_factor}");
                self.resize(event_loop, window.inner_size());
            }
            WindowEvent::Occluded(occluded) => {
                self.occluded = occluded;
                eprintln!("occluded={occluded}");
                if !occluded {
                    self.request_redraw();
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor_physical = (position.x, position.y);
                self.input(
                    event_loop,
                    InputEvent::PointerMoved {
                        position: self.pointer(),
                    },
                );
            }
            WindowEvent::CursorLeft { .. } => self.input(event_loop, InputEvent::PointerLeft),
            WindowEvent::MouseInput { state, button, .. } => {
                if self.retained.is_some() {
                    let button = match button {
                        MouseButton::Left => PointerButton::Primary,
                        MouseButton::Right => PointerButton::Secondary,
                        MouseButton::Middle => PointerButton::Middle,
                        MouseButton::Back => PointerButton::Other(4),
                        MouseButton::Forward => PointerButton::Other(5),
                        MouseButton::Other(value) => PointerButton::Other(value),
                    };
                    let position = self.pointer();
                    let modifiers = self.modifiers;
                    let input = if state == ElementState::Pressed {
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
                    self.input(event_loop, input);
                } else if state == ElementState::Pressed && button == MouseButton::Left {
                    let position = self.pointer();
                    self.click(position.x, position.y, "window-event");
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                if self.retained.is_some() {
                    let delta = match delta {
                        MouseScrollDelta::LineDelta(x, y) => Point::new(x * 84.0, y * 84.0),
                        MouseScrollDelta::PixelDelta(delta) => Point::new(
                            (delta.x / window.scale_factor()) as f32,
                            (delta.y / window.scale_factor()) as f32,
                        ),
                    };
                    self.input(
                        event_loop,
                        InputEvent::Wheel {
                            position: self.pointer(),
                            delta,
                            modifiers: self.modifiers,
                        },
                    );
                } else {
                    self.scroll(delta, "window-event");
                }
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                let state = modifiers.state();
                self.modifiers = Modifiers {
                    shift: state.shift_key(),
                    control: state.control_key(),
                    alt: state.alt_key(),
                    super_key: state.super_key(),
                };
            }
            WindowEvent::Focused(focused) => {
                if !focused {
                    self.modifiers = Modifiers::default();
                }
                self.input(event_loop, InputEvent::WindowFocused(focused));
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if event.state == ElementState::Pressed
                    && event.logical_key == Key::Named(NamedKey::Escape)
                {
                    event_loop.exit();
                } else {
                    let key = normalize_key(event.logical_key);
                    let modifiers = self.modifiers;
                    let input = if event.state == ElementState::Pressed {
                        InputEvent::KeyDown {
                            key,
                            modifiers,
                            repeat: event.repeat,
                        }
                    } else {
                        InputEvent::KeyUp { key, modifiers }
                    };
                    self.input(event_loop, input);
                }
            }
            WindowEvent::RedrawRequested => self.render(event_loop),
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let now = Instant::now();
        if self
            .smoke
            .as_ref()
            .is_some_and(|smoke| now >= smoke.deadline && smoke.stage != SmokeStage::Complete)
        {
            self.fail(event_loop, "GPU smoke-test timed out before a presented frame, resize and input checks completed");
            return;
        }
        if self.retry_at.is_some_and(|retry| now >= retry) {
            self.retry_at = None;
            self.request_redraw();
        }
        let wakeup = match (
            self.retry_at,
            self.smoke.as_ref().map(|smoke| smoke.deadline),
        ) {
            (Some(retry), Some(deadline)) => Some(retry.min(deadline)),
            (retry, deadline) => retry.or(deadline),
        };
        event_loop.set_control_flow(wakeup.map_or(ControlFlow::Wait, ControlFlow::WaitUntil));
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        self.renderer = None;
        self.window = None;
        eprintln!("exit frames_presented={}", self.frames);
    }
}

fn normalize_key(key: Key) -> UiKey {
    match key {
        Key::Named(NamedKey::Tab) => UiKey::Tab,
        Key::Named(NamedKey::Enter) => UiKey::Enter,
        Key::Named(NamedKey::Space) => UiKey::Space,
        Key::Named(NamedKey::Escape) => UiKey::Escape,
        Key::Named(NamedKey::Delete) => UiKey::Delete,
        Key::Named(NamedKey::ArrowUp) => UiKey::ArrowUp,
        Key::Named(NamedKey::ArrowDown) => UiKey::ArrowDown,
        Key::Named(NamedKey::ArrowLeft) => UiKey::ArrowLeft,
        Key::Named(NamedKey::ArrowRight) => UiKey::ArrowRight,
        Key::Named(NamedKey::Home) => UiKey::Home,
        Key::Named(NamedKey::End) => UiKey::End,
        Key::Character(text) => UiKey::Character(text.to_string()),
        _ => UiKey::Other,
    }
}

fn run() -> Result<(), String> {
    let mut smoke = false;
    let mut rectangles = false;
    for argument in std::env::args().skip(1) {
        match argument.as_str() {
            "--smoke-test" => smoke = true,
            "--rectangles" => rectangles = true,
            "--help" | "-h" => {
                println!(
                    "gpu-shell [--rectangles] [--smoke-test]\nNative retained UI demo; --rectangles selects the original rectangle scene.\nSmoke mode tests presented frames, a native resize and synthetic input.\nSet WGPU_BACKEND=dx12, vulkan or metal to select a compiled native backend."
                );
                return Ok(());
            }
            _ => return Err(format!("Unknown argument: {argument}")),
        }
    }
    let event_loop =
        EventLoop::new().map_err(|error| format!("Cannot create event loop: {error}"))?;
    let mut app = App::new(smoke, rectangles)?;
    event_loop
        .run_app(&mut app)
        .map_err(|error| format!("Event loop failed: {error}"))?;
    if let Some(error) = app.failure {
        return Err(error);
    }
    if app
        .smoke
        .is_some_and(|smoke| smoke.stage != SmokeStage::Complete)
    {
        return Err("Smoke window closed before checks completed".into());
    }
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("ERROR: {error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use winit::dpi::PhysicalPosition;

    #[test]
    fn pixel_wheel_preserves_fractional_rows_and_dpi() {
        let mut wheel = WheelAccumulator::default();
        let delta = MouseScrollDelta::PixelDelta(PhysicalPosition::new(0.0, -14.0));
        for _ in 0..3 {
            assert_eq!(wheel.rows(delta, 2.0), 0);
        }
        assert_eq!(wheel.rows(delta, 2.0), 1);
        assert_eq!(wheel.rows(MouseScrollDelta::LineDelta(0.0, 1.0), 2.0), -3);
    }

    #[test]
    fn invalid_wheel_inputs_do_not_poison_the_accumulator() {
        let mut wheel = WheelAccumulator::default();
        assert_eq!(
            wheel.rows(MouseScrollDelta::LineDelta(0.0, f32::NAN), 1.0),
            0
        );
        assert_eq!(wheel.rows(MouseScrollDelta::LineDelta(0.0, 1.0), 0.0), 0);
        assert_eq!(wheel.rows(MouseScrollDelta::LineDelta(0.0, -1.0), 1.0), 3);
    }
}
