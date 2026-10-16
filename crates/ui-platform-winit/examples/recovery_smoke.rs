//! Passive Windows diagnostic for real single/multiple host GPU recreation.
use rust_desktop_ui_core::{Bitmap, Color, ImagePrimitive, Point, Rect, Scene, Size, TextRun};
use rust_desktop_ui_platform_winit::{
    DesktopApp, DesktopApplication, EventResponse, PlatformEvent, RendererStats, RunOptions,
    WindowKey, WindowSpec, run, run_multi,
};
use std::sync::{Arc, Mutex};

struct View {
    scene: Scene,
    original: Scene,
    stats: Arc<Mutex<Vec<RendererStats>>>,
    name: String,
}
impl View {
    fn new(name: &str, stats: Arc<Mutex<Vec<RendererStats>>>) -> Self {
        let mut scene = Scene::default();
        scene.fill(Rect::new(0.0, 0.0, 600.0, 400.0), Color::rgb(240, 244, 249));
        let bitmap = Bitmap::new_rgba8(
            2,
            2,
            vec![
                255, 40, 40, 255, 40, 180, 70, 255, 40, 100, 255, 255, 255, 255, 255, 100,
            ],
        )
        .unwrap();
        scene.image(ImagePrimitive::new(
            bitmap,
            Rect::new(24.0, 48.0, 160.0, 160.0),
        ));
        scene.text(TextRun::new(
            "Retained image and text survive GPU recreation",
            Rect::new(24.0, 12.0, 480.0, 28.0),
            Color::rgb(20, 30, 40),
            16.0,
        ));
        Self {
            original: scene.clone(),
            scene,
            stats,
            name: name.to_owned(),
        }
    }
}
impl DesktopApp for View {
    fn update(&mut self, _viewport: Size, _scale: f32) -> Result<(), String> {
        if self.scene != self.original {
            return Err("retained application scene changed".into());
        }
        Ok(())
    }
    fn scene(&self) -> &Scene {
        &self.scene
    }
    fn frame_presented(&mut self, stats: RendererStats) {
        self.stats.lock().unwrap().push(stats);
    }
    fn event(&mut self, _event: PlatformEvent) -> Result<EventResponse, String> {
        Ok(EventResponse::default())
    }
    fn title(&self) -> String {
        self.name.clone()
    }
}
fn options() -> RunOptions {
    RunOptions {
        width: 600.0,
        height: 400.0,
        min_size: None,
        smoke_test: true,
        smoke_device_recovery: true,
        activate: false,
        show_render_stats: true,
    }
}
struct Application {
    views: [View; 2],
}
impl DesktopApplication for Application {
    fn windows(&mut self) -> Result<Vec<WindowSpec>, String> {
        Ok((0..2)
            .map(|index| {
                let mut spec = WindowSpec::new(WindowKey(index), options());
                spec.position = Some(Point::new(
                    80.0 + index as f32 * 180.0,
                    80.0 + index as f32 * 120.0,
                ));
                if index == 1 {
                    spec.owner = Some(WindowKey::MAIN);
                }
                spec
            })
            .collect())
    }
    fn window(&self, key: WindowKey) -> Option<&dyn DesktopApp> {
        self.views.get(key.0 as usize).map(|v| v as &dyn DesktopApp)
    }
    fn window_mut(&mut self, key: WindowKey) -> Option<&mut (dyn DesktopApp + '_)> {
        self.views
            .get_mut(key.0 as usize)
            .map(|v| v as &mut dyn DesktopApp)
    }
    fn close_requested(&mut self, _key: WindowKey) -> Result<(), String> {
        Err("diagnostic interrupted by native close".into())
    }
}
fn main() -> Result<(), String> {
    let multiple = std::env::args().any(|arg| arg == "--multi");
    let counts: Vec<_> = (0..if multiple { 2 } else { 1 })
        .map(|_| Arc::new(Mutex::new(Vec::new())))
        .collect();
    if multiple {
        run_multi(Application {
            views: [
                View::new("Recovery main", counts[0].clone()),
                View::new("Recovery floating", counts[1].clone()),
            ],
        })?;
    } else {
        run(View::new("Recovery single", counts[0].clone()), options())?;
    }
    for (index, counter) in counts.iter().enumerate() {
        let frames = counter.lock().unwrap();
        if frames.len() < 4
            || frames
                .iter()
                .any(|s| s.images != 1 || s.glyphs == 0 || s.missing_glyphs != 0)
            || frames.iter().filter(|s| s.uploaded_images == 1).count() != 2
        {
            return Err(format!(
                "window {index}: missing retained-image/text/re-upload evidence: {frames:?}"
            ));
        }
        println!(
            "RECOVERY APP PASS: window={index} frames={} same retained Scene; two bitmap uploads; glyphs={}; missing=0",
            frames.len(),
            frames.last().unwrap().glyphs
        );
    }
    Ok(())
}
