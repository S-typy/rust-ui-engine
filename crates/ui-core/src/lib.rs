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
        Self { x, y, width, height }
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

const TOOLBAR_BOTTOM: f32 = 142.0;
const GRID_HEADER_BOTTOM: f32 = 185.0;
const ROW_HEIGHT: f32 = 28.0;
const LEFT_WIDTH: f32 = 242.0;

/// This is intentionally a *shape-only* smoke test, not a finished Ribbon or TreeGrid.
/// It tests GPU batching and visible-row-only scene generation before text/widgets exist.
#[derive(Debug, Default)]
pub struct DemoState {
    pub active_tab: usize,
    pub selected_row: Option<usize>,
    pub first_row: usize,
}

impl DemoState {
    pub const TOTAL_ROWS: usize = 100_000;

    pub fn visible_rows(&self, height: f32) -> usize {
        ((height - GRID_HEADER_BOTTOM).max(0.0) / ROW_HEIGHT).ceil() as usize
    }

    pub fn scroll(&mut self, delta_rows: i32, height: f32) {
        let maximum_first = Self::TOTAL_ROWS.saturating_sub(self.visible_rows(height));
        self.first_row = self.first_row
            .saturating_add_signed(delta_rows as isize)
            .min(maximum_first);
    }

    pub fn click(&mut self, x: f32, y: f32, height: f32) {
        if (8.0..41.0).contains(&y) {
            let tab = ((x - 14.0) / 116.0).floor() as isize;
            if (0..4).contains(&tab) {
                self.active_tab = tab as usize;
            }
        } else if y >= GRID_HEADER_BOTTOM && y < height && x >= LEFT_WIDTH {
            let visible_index = ((y - GRID_HEADER_BOTTOM) / ROW_HEIGHT) as usize;
            let row = self.first_row + visible_index;
            if row < Self::TOTAL_ROWS { self.selected_row = Some(row); }
        }
    }

    pub fn build_scene(&self, width: f32, height: f32) -> Scene {
        let mut scene = Scene::default();
        let width = width.max(1.0);
        let height = height.max(1.0);
        let navy = Color::rgb(36, 54, 77);
        let border = Color::rgb(215, 222, 231);
        let pale = Color::rgb(247, 249, 252);
        let blue = Color::rgb(42, 112, 206);

        scene.fill(Rect::new(0.0, 0.0, width, height), Color::rgb(255, 255, 255));
        // Tabs and grouped ribbon silhouette.
        scene.fill(Rect::new(0.0, 0.0, width, 42.0), Color::rgb(239, 244, 250));
        scene.fill(Rect::new(0.0, 41.0, width, 1.0), border);
        for i in 0..4 {
            let x = 14.0 + i as f32 * 116.0;
            scene.fill(Rect::new(x, 10.0, 101.0, 28.0), if i == self.active_tab { Color::rgb(255, 255, 255) } else { Color::rgb(230, 236, 245) });
            if i == self.active_tab { scene.fill(Rect::new(x, 37.0, 101.0, 3.0), blue); }
        }
        scene.fill(Rect::new(0.0, 42.0, width, 101.0), pale);
        for group in 0..5 {
            let x = 18.0 + group as f32 * 178.0;
            scene.fill(Rect::new(x, 56.0, 148.0, 67.0), Color::rgb(233, 238, 245));
            scene.fill(Rect::new(x + 9.0, 64.0, 43.0, 43.0), Color::rgb(208, 222, 239));
            scene.fill(Rect::new(x + 59.0, 64.0, 79.0, 16.0), Color::rgb(212, 222, 237));
            scene.fill(Rect::new(x + 59.0, 89.0, 59.0, 16.0), Color::rgb(212, 222, 237));
        }
        scene.fill(Rect::new(0.0, TOOLBAR_BOTTOM, width, 1.0), border);

        // Navigation panel and data header.
        scene.fill(Rect::new(0.0, 143.0, LEFT_WIDTH, height - 143.0), pale);
        scene.fill(Rect::new(LEFT_WIDTH - 1.0, 143.0, 1.0, height - 143.0), border);
        scene.fill(Rect::new(LEFT_WIDTH, 143.0, width - LEFT_WIDTH, GRID_HEADER_BOTTOM - 143.0), Color::rgb(237, 242, 248));
        scene.fill(Rect::new(LEFT_WIDTH, GRID_HEADER_BOTTOM - 1.0, width - LEFT_WIDTH, 1.0), border);
        for col in [0.44_f32, 0.66, 0.81, 0.94] {
            let cx = LEFT_WIDTH + (width - LEFT_WIDTH) * col;
            scene.fill(Rect::new(cx, 143.0, 1.0, height - 143.0), Color::rgb(228, 232, 238));
        }

        // Navigation placeholders, not actual author labels yet.
        for i in 0..15 {
            let y = 164.0 + i as f32 * 29.0;
            if y > height { break; }
            scene.fill(Rect::new(15.0 + (i % 3) as f32 * 8.0, y, 11.0, 11.0), Color::rgb(167, 187, 213));
            scene.fill(Rect::new(38.0 + (i % 3) as f32 * 8.0, y, 145.0 - (i % 4) as f32 * 15.0, 8.0), Color::rgb(215, 223, 233));
        }

        // Virtualization: only the visible rows are rendered; never TOTAL_ROWS rows.
        for visible in 0..self.visible_rows(height) {
            let absolute = self.first_row + visible;
            if absolute >= Self::TOTAL_ROWS { break; }
            let y = GRID_HEADER_BOTTOM + visible as f32 * ROW_HEIGHT;
            if y >= height { break; }
            if self.selected_row == Some(absolute) {
                scene.fill(Rect::new(LEFT_WIDTH + 1.0, y, width - LEFT_WIDTH - 1.0, ROW_HEIGHT), Color::rgb(214, 233, 252));
            } else if absolute % 2 == 1 {
                scene.fill(Rect::new(LEFT_WIDTH + 1.0, y, width - LEFT_WIDTH - 1.0, ROW_HEIGHT), Color::rgb(249, 251, 254));
            }
            let indent = ((absolute / 9) % 3) as f32 * 12.0;
            scene.fill(Rect::new(LEFT_WIDTH + 12.0 + indent, y + 9.0, 9.0, 9.0), navy);
            scene.fill(Rect::new(LEFT_WIDTH + 30.0 + indent, y + 10.0, 113.0 + (absolute % 7) as f32 * 15.0, 8.0), Color::rgb(166, 180, 198));
            scene.fill(Rect::new(LEFT_WIDTH + (width - LEFT_WIDTH) * 0.48, y + 10.0, 52.0, 8.0), Color::rgb(207, 215, 228));
            scene.fill(Rect::new(LEFT_WIDTH, y + ROW_HEIGHT - 1.0, width - LEFT_WIDTH, 1.0), Color::rgb(241, 244, 248));
        }
        scene
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn geometry_is_half_open() {
        let r = Rect::new(10.0, 20.0, 50.0, 10.0);
        assert!(r.contains(10.0, 20.0));
        assert!(!r.contains(60.0, 30.0));
    }

    #[test]
    fn virtual_scene_does_not_contain_hundred_thousand_rows() {
        let scene = DemoState::default().build_scene(1200.0, 800.0);
        assert!(scene.rectangles.len() < 1000);
    }

    #[test]
    fn scrolling_is_clamped() {
        let mut s = DemoState::default();
        s.scroll(-20, 800.0);
        assert_eq!(s.first_row, 0);
        s.scroll(2_000_000, 800.0);
        assert!(s.first_row < DemoState::TOTAL_ROWS);
    }

    #[test]
    fn clicking_data_row_changes_selection() {
        let mut s = DemoState::default();
        s.click(400.0, GRID_HEADER_BOTTOM + 2.0 * ROW_HEIGHT + 1.0, 800.0);
        assert_eq!(s.selected_row, Some(2));
    }
}
