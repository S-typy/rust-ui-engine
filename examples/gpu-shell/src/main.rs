//! Prototype shell: GPU-drawn shapes and a 100k-row virtualized scene.
//! No text editor or real Ribbon/TreeGrid yet.

use std::sync::Arc;
use rust_desktop_ui_core::DemoState;
use rust_desktop_ui_render_wgpu::GpuRenderer;
use winit::{
    application::ApplicationHandler,
    event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::{Key, NamedKey},
    window::{Window, WindowId},
};

#[derive(Default)]
struct App {
    window: Option<Arc<Window>>,
    renderer: Option<GpuRenderer>,
    ui: DemoState,
    cursor_logical: (f32, f32),
}

impl App {
    fn request_redraw(&self) {
        if let Some(window) = &self.window { window.request_redraw(); }
    }

    fn logical_height(&self) -> f32 {
        self.window.as_ref().map(|w| w.inner_size().height as f32 / w.scale_factor() as f32)
            .unwrap_or(0.0)
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() { return; }
        let attrs = Window::default_attributes()
            .with_title("Rust Desktop UI - GPU prototype")
            .with_inner_size(winit::dpi::LogicalSize::new(1250.0, 800.0));
        let window = Arc::new(event_loop.create_window(attrs).expect("could not create window"));
        let gpu = match pollster::block_on(GpuRenderer::new(window.clone())) {
            Ok(gpu) => gpu,
            Err(error) => { eprintln!("Could not initialize GPU: {error}"); event_loop.exit(); return; }
        };
        self.window = Some(window);
        self.renderer = Some(gpu);
        self.request_redraw();
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, window_id: WindowId, event: WindowEvent) {
        let Some(window) = self.window.as_ref() else { return; };
        if window.id() != window_id { return; }
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(renderer) = self.renderer.as_mut() { renderer.resize(size); }
                self.request_redraw();
            }
            WindowEvent::ScaleFactorChanged { .. } => self.request_redraw(),
            WindowEvent::CursorMoved { position, .. } => {
                let scale = window.scale_factor() as f32;
                self.cursor_logical = (position.x as f32 / scale, position.y as f32 / scale);
            }
            WindowEvent::MouseInput { state: ElementState::Pressed, button: MouseButton::Left, .. } => {
                self.ui.click(self.cursor_logical.0, self.cursor_logical.1, self.logical_height());
                self.request_redraw();
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let rows = match delta {
                    MouseScrollDelta::LineDelta(_, y) => (-y * 3.0).round() as i32,
                    MouseScrollDelta::PixelDelta(pos) => (-pos.y / 28.0).round() as i32,
                };
                self.ui.scroll(rows, self.logical_height());
                self.request_redraw();
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed && event.logical_key == Key::Named(NamedKey::Escape) => {
                event_loop.exit();
            }
            WindowEvent::RedrawRequested => {
                let scale = window.scale_factor() as f32;
                let physical = window.inner_size();
                if physical.width == 0 || physical.height == 0 { return; }
                let (w, h) = (physical.width as f32 / scale, physical.height as f32 / scale);
                let scene = self.ui.build_scene(w, h);
                if let Some(renderer) = self.renderer.as_mut() {
                    if let Err(error) = renderer.render(&scene, scale) {
                        eprintln!("GPU render failure: {error}");
                        event_loop.exit();
                    }
                }
                window.set_title(&format!(
                    "Rust Desktop UI | GPU prototype | 100,000 rows | first row {} | visible {}",
                    self.ui.first_row, self.ui.visible_rows(h),
                ));
            }
            _ => {}
        }
    }
}

fn main() {
    let event_loop = EventLoop::new().expect("could not create event loop");
    let mut app = App::default();
    event_loop.run_app(&mut app).expect("event loop failure");
}
