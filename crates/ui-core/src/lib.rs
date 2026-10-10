//! Platform-independent retained UI foundation and rectangle scene.
//! All coordinates are logical pixels. The native shell applies DPI scaling.

mod arena;
mod dirty;
mod input;
mod model;
mod runtime;
mod scene_text;
mod tree;

pub use arena::WidgetId;
pub use dirty::DirtyFlags;
pub use input::*;
pub use model::*;
pub use runtime::*;
pub use scene_text::*;
pub use tree::{Node, TreeError, UiTree};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn contains(self, x: f32, y: f32) -> bool {
        x >= self.x && y >= self.y && x < self.x + self.width && y < self.y + self.height
    }

    pub fn is_valid(self) -> bool {
        [
            self.x,
            self.y,
            self.width,
            self.height,
            self.x + self.width,
            self.y + self.height,
        ]
        .into_iter()
        .all(f32::is_finite)
            && self.width >= 0.0
            && self.height >= 0.0
    }

    pub fn intersection(self, other: Self) -> Option<Self> {
        if !self.is_valid() || !other.is_valid() {
            return None;
        }
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        let right = (self.x + self.width).min(other.x + other.width);
        let bottom = (self.y + self.height).min(other.y + other.height);
        (right > x && bottom > y).then(|| Self::new(x, y, right - x, bottom - y))
    }
}

/// Straight-alpha color: RGB channels are sRGB-encoded in `0..=1`; alpha is
/// linear coverage in `0..=1`. Backends must decode RGB before linear blending.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self {
            r: r as f32 / 255.0,
            g: g as f32 / 255.0,
            b: b as f32 / 255.0,
            a: 1.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SolidRect {
    pub bounds: Rect,
    pub color: Color,
}

/// A uniformly rounded fill with a rectangular ancestor clip, in logical pixels.
/// The clip never changes the original corner geometry. This is not a rounded
/// clip for descendants; compose nested fills to draw borders and focus states.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RoundedRect {
    pub bounds: Rect,
    pub clip: Rect,
    pub radius: f32,
    pub color: Color,
}

impl RoundedRect {
    pub fn is_valid(self) -> bool {
        self.bounds.is_valid()
            && self.clip.is_valid()
            && self.radius.is_finite()
            && self.radius >= 0.0
            && self.radius <= self.bounds.width.min(self.bounds.height) * 0.5
            && [self.color.r, self.color.g, self.color.b, self.color.a]
                .into_iter()
                .all(|channel| channel.is_finite() && (0.0..=1.0).contains(&channel))
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Scene {
    pub rectangles: Vec<SolidRect>,
    pub rounded_rectangles: Vec<RoundedRect>,
    pub texts: Vec<TextRun>,
    commands: Vec<DrawCommand>,
}

impl Scene {
    /// Add an antialiased rounded fill in painter order. Oversized radii are
    /// clamped to half the smaller dimension; negative/non-finite radii, invalid
    /// colors/geometry and entirely clipped fills are ignored.
    pub fn rounded_fill(&mut self, bounds: Rect, clip: Rect, radius: f32, color: Color) {
        if !radius.is_finite() || radius < 0.0 {
            return;
        }
        let rectangle = RoundedRect {
            bounds,
            clip,
            radius: radius.min(bounds.width.min(bounds.height) * 0.5),
            color,
        };
        if rectangle.is_valid() && bounds.intersection(clip).is_some() {
            self.commands
                .push(DrawCommand::RoundedRectangle(self.rounded_rectangles.len()));
            self.rounded_rectangles.push(rectangle);
        }
    }

    pub fn fill(&mut self, bounds: Rect, color: Color) {
        if bounds.is_valid() && bounds.width > 0.0 && bounds.height > 0.0 {
            self.commands
                .push(DrawCommand::Rectangle(self.rectangles.len()));
            self.rectangles.push(SolidRect { bounds, color });
        }
    }

    /// Add a text command, preserving its order relative to backgrounds/overlays.
    pub fn text(&mut self, run: TextRun) {
        if run.is_valid()
            && !run.text.is_empty()
            && run.bounds.width > 0.0
            && run.bounds.height > 0.0
        {
            self.commands.push(DrawCommand::Text(self.texts.len()));
            self.texts.push(run);
        }
    }

    pub fn commands(&self) -> &[DrawCommand] {
        &self.commands
    }

    /// Clear all typed arrays and their painter order together.
    pub fn clear(&mut self) {
        self.rectangles.clear();
        self.rounded_rectangles.clear();
        self.texts.clear();
        self.commands.clear();
    }

    pub fn append(&mut self, other: &Scene) {
        let rectangles = self.rectangles.len();
        let rounded_rectangles = self.rounded_rectangles.len();
        let texts = self.texts.len();
        self.rectangles.extend_from_slice(&other.rectangles);
        self.rounded_rectangles
            .extend_from_slice(&other.rounded_rectangles);
        self.texts.extend_from_slice(&other.texts);
        self.commands
            .extend(other.commands.iter().map(|command| match command {
                DrawCommand::Rectangle(index) => DrawCommand::Rectangle(rectangles + index),
                DrawCommand::RoundedRectangle(index) => {
                    DrawCommand::RoundedRectangle(rounded_rectangles + index)
                }
                DrawCommand::Text(index) => DrawCommand::Text(texts + index),
            }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn geometry_is_half_open() {
        let rect = Rect::new(10.0, 20.0, 50.0, 10.0);
        assert!(rect.contains(10.0, 20.0));
        assert!(!rect.contains(60.0, 20.0));
        assert!(!rect.contains(10.0, 30.0));
        assert!(!rect.contains(9.0, 25.0));
    }

    #[test]
    fn empty_rectangles_do_not_enter_the_scene() {
        let mut scene = Scene::default();
        for width in [0.0, -1.0, f32::NAN] {
            scene.fill(Rect::new(0.0, 0.0, width, 10.0), Color::rgb(0, 0, 0));
        }
        assert!(scene.rectangles.is_empty());
        scene.fill(Rect::new(1.0, 2.0, 3.0, 4.0), Color::rgb(255, 0, 0));
        assert_eq!(scene.rectangles.len(), 1);
        assert_eq!(scene.rectangles[0].color, Color::rgb(255, 0, 0));
    }

    #[test]
    fn composing_scenes_preserves_background_text_and_overlay_order() {
        let bounds = Rect::new(0.0, 0.0, 100.0, 30.0);
        let color = Color::rgb(20, 30, 40);
        let mut content = Scene::default();
        content.fill(bounds, color);
        content.text(TextRun::new("Текст", bounds, color, 14.0));
        let mut popup = Scene::default();
        popup.fill(bounds, color);
        popup.text(TextRun::new("Меню", bounds, color, 14.0));
        content.append(&popup);
        assert_eq!(
            content.commands(),
            &[
                DrawCommand::Rectangle(0),
                DrawCommand::Text(0),
                DrawCommand::Rectangle(1),
                DrawCommand::Text(1)
            ]
        );
        assert_eq!(content.texts[1].text, "Меню");
        content.clear();
        content.text(TextRun::new("Новое", bounds, color, 14.0));
        assert_eq!(content.commands(), &[DrawCommand::Text(0)]);
        assert!(content.rectangles.is_empty());
    }

    #[test]
    fn rounded_fills_keep_original_corners_and_compose_in_painter_order() {
        let bounds = Rect::new(10.0, 20.0, 100.0, 40.0);
        let clip = Rect::new(20.0, 0.0, 200.0, 200.0);
        let color = Color::rgb(30, 40, 50);
        let mut scene = Scene::default();
        scene.rounded_fill(bounds, clip, 1000.0, color);
        assert_eq!(scene.rounded_rectangles[0].bounds, bounds);
        assert_eq!(scene.rounded_rectangles[0].clip, clip);
        assert_eq!(scene.rounded_rectangles[0].radius, 20.0);
        scene.text(TextRun::new("Text", bounds, color, 14.0));
        let mut overlay = Scene::default();
        overlay.fill(bounds, color);
        overlay.rounded_fill(bounds, bounds, 4.0, color);
        scene.append(&overlay);
        assert_eq!(
            scene.commands(),
            [
                DrawCommand::RoundedRectangle(0),
                DrawCommand::Text(0),
                DrawCommand::Rectangle(0),
                DrawCommand::RoundedRectangle(1),
            ]
        );
        scene.clear();
        assert!(scene.rounded_rectangles.is_empty());
        assert!(scene.commands().is_empty());
        scene.rounded_fill(bounds, bounds, 0.0, color);
        assert_eq!(scene.commands(), [DrawCommand::RoundedRectangle(0)]);
    }

    #[test]
    fn rounded_fill_rejects_invalid_and_fully_clipped_input() {
        let bounds = Rect::new(0.0, 0.0, 30.0, 20.0);
        let mut scene = Scene::default();
        for radius in [-1.0, f32::NAN, f32::INFINITY] {
            scene.rounded_fill(bounds, bounds, radius, Color::rgb(0, 0, 0));
        }
        scene.rounded_fill(
            bounds,
            Rect::new(40.0, 0.0, 20.0, 20.0),
            4.0,
            Color::rgb(0, 0, 0),
        );
        scene.rounded_fill(
            bounds,
            bounds,
            4.0,
            Color {
                r: f32::NAN,
                ..Color::rgb(0, 0, 0)
            },
        );
        scene.rounded_fill(
            Rect::new(0.0, 0.0, -1.0, 20.0),
            bounds,
            4.0,
            Color::rgb(0, 0, 0),
        );
        assert!(scene.rounded_rectangles.is_empty());
        assert!(scene.commands().is_empty());
    }
}
