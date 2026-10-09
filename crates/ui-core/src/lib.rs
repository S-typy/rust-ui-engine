//! Render-backend-independent model for the first UI prototype.
//! All coordinates are logical pixels. The native shell applies DPI scaling.

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
}

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

#[derive(Debug, Clone, Copy)]
pub struct SolidRect {
    pub bounds: Rect,
    pub color: Color,
}

#[derive(Default)]
pub struct Scene {
    pub rectangles: Vec<SolidRect>,
}

impl Scene {
    pub fn fill(&mut self, bounds: Rect, color: Color) {
        if bounds.width > 0.0 && bounds.height > 0.0 {
            self.rectangles.push(SolidRect { bounds, color });
        }
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
}
