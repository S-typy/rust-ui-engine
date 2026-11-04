//! Bounded closed polygon contours shared by views, charts and page output.
use crate::{Color, DrawCommand, Point, Rect, Scene};

/// Maximum total number of edges, including the closing edge of each contour.
pub const MAX_FILL_EDGES: usize = 256;

/// Determines the interior of overlapping or nested closed contours.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FillRule {
    #[default]
    NonZero,
    EvenOdd,
}

/// One fill operation. Contours close implicitly and contain at least three
/// finite points. Overlapping contours contribute alpha once, after applying
/// the winding rule. Curves must be flattened by the producer within the bound.
#[derive(Clone, Debug, PartialEq)]
pub struct FillPath {
    pub contours: Vec<Vec<Point>>,
    pub clip: Rect,
    pub rule: FillRule,
    pub color: Color,
}

impl FillPath {
    pub fn edges(&self) -> impl Iterator<Item = (Point, Point)> + '_ {
        self.contours.iter().flat_map(|points| {
            points
                .iter()
                .copied()
                .zip(points.iter().copied().cycle().skip(1))
        })
    }

    pub fn edge_count(&self) -> Option<usize> {
        self.contours
            .iter()
            .try_fold(0usize, |count, points| count.checked_add(points.len()))
    }

    /// Unclipped extent; rejects invalid or overflowing geometry.
    pub fn bounds(&self) -> Option<Rect> {
        if self.contours.is_empty()
            || self.contours.iter().any(|points| points.len() < 3)
            || self.edge_count()? > MAX_FILL_EDGES
        {
            return None;
        }
        let (mut left, mut top, mut right, mut bottom) = (
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        );
        for point in self.contours.iter().flatten() {
            if !point.is_finite() {
                return None;
            }
            left = left.min(f64::from(point.x));
            top = top.min(f64::from(point.y));
            right = right.max(f64::from(point.x));
            bottom = bottom.max(f64::from(point.y));
        }
        let rect = Rect::new(
            left as f32,
            top as f32,
            (right - left) as f32,
            (bottom - top) as f32,
        );
        (rect.is_valid() && rect.width > 0.0 && rect.height > 0.0).then_some(rect)
    }

    pub fn is_valid(&self) -> bool {
        self.bounds().is_some()
            && self.clip.is_valid()
            && [self.color.r, self.color.g, self.color.b, self.color.a]
                .into_iter()
                .all(|c| c.is_finite() && (0.0..=1.0).contains(&c))
    }

    /// Geometric hit test using the same winding convention as rendering.
    /// Edges use a half-open crossing convention; clipping is applied first.
    pub fn contains(&self, point: Point) -> bool {
        if !self.is_valid() || !point.is_finite() || !self.clip.contains(point.x, point.y) {
            return false;
        }
        let mut winding = 0i32;
        for (a, b) in self.edges() {
            let cross = (f64::from(b.x) - f64::from(a.x)) * (f64::from(point.y) - f64::from(a.y))
                - (f64::from(b.y) - f64::from(a.y)) * (f64::from(point.x) - f64::from(a.x));
            if a.y <= point.y && b.y > point.y && cross > 0.0 {
                winding += 1;
            }
            if a.y > point.y && b.y <= point.y && cross < 0.0 {
                winding -= 1;
            }
        }
        match self.rule {
            FillRule::NonZero => winding != 0,
            FillRule::EvenOdd => winding % 2 != 0,
        }
    }
}

impl Scene {
    /// Append one validated polygon fill. Failure leaves this scene unchanged;
    /// a valid path outside its clip is a successful no-op.
    pub fn fill_path(&mut self, path: FillPath) -> Result<(), &'static str> {
        if !path.is_valid() {
            return Err("Invalid fill path");
        }
        if path.bounds().unwrap().intersection(path.clip).is_some() {
            self.commands
                .push(DrawCommand::FillPath(self.fill_paths.len()));
            self.fill_paths.push(path);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(x: f32, size: f32) -> Vec<Point> {
        vec![
            Point::new(x, x),
            Point::new(x + size, x),
            Point::new(x + size, x + size),
            Point::new(x, x + size),
        ]
    }
    fn path() -> FillPath {
        FillPath {
            contours: vec![square(0.0, 20.0), square(5.0, 10.0)],
            clip: Rect::new(0.0, 0.0, 40.0, 40.0),
            rule: FillRule::NonZero,
            color: Color::rgb(30, 60, 90),
        }
    }

    #[test]
    fn winding_distinguishes_holes_from_same_direction_overlap() {
        let mut path = path();
        assert!(path.contains(Point::new(10.0, 10.0)));
        path.rule = FillRule::EvenOdd;
        assert!(!path.contains(Point::new(10.0, 10.0)));
        assert!(path.contains(Point::new(2.0, 2.0)));
        path.rule = FillRule::NonZero;
        path.contours[1].reverse();
        assert!(!path.contains(Point::new(10.0, 10.0)));
        for contour in &mut path.contours {
            contour.reverse();
        }
        assert!(!path.contains(Point::new(10.0, 10.0)));
        assert!(path.contains(Point::new(2.0, 2.0)));
        assert!(!path.contains(Point::new(25.0, 10.0)));
    }

    #[test]
    fn invalid_fill_is_atomic_and_fully_clipped_fill_is_empty() {
        let mut scene = Scene::default();
        scene.fill_path(path()).unwrap();
        let saved = scene.clone();
        let mut invalid = path();
        invalid.contours[0][0].x = f32::NAN;
        assert!(scene.fill_path(invalid).is_err());
        assert_eq!(scene, saved);
        let mut clipped = path();
        clipped.clip = Rect::new(30.0, 30.0, 5.0, 5.0);
        scene.fill_path(clipped).unwrap();
        assert_eq!(scene, saved);
        let mut oversized = path();
        oversized.contours = vec![square(0.0, 20.0); MAX_FILL_EDGES / 4 + 1];
        assert!(scene.fill_path(oversized).is_err());
        assert_eq!(scene, saved);
    }

    #[test]
    fn retained_and_page_transforms_preserve_winding_clips_and_order() {
        let mut source = Scene::default();
        source.fill_path(path()).unwrap();
        source.fill(Rect::new(1.0, 1.0, 2.0, 2.0), Color::rgb(1, 2, 3));
        let offset = Point::new(10.0, 15.0);
        let clip = Rect::new(12.0, 17.0, 24.0, 26.0);
        let mut target = Scene::default();
        target
            .append_transformed_view(&source, offset, 2.0, clip)
            .unwrap();
        let paged = crate::pages::transform_page(&source, 2.0, offset, clip).unwrap();
        assert_eq!(target, paged);
        assert_eq!(target.fill_paths[0].clip, clip);
        assert_eq!(target.fill_paths[0].contours[1][0], Point::new(20.0, 25.0));
        assert_eq!(
            target.commands(),
            [DrawCommand::FillPath(0), DrawCommand::Rectangle(0)]
        );
        target.append(&source);
        assert_eq!(target.commands()[2], DrawCommand::FillPath(1));
        let saved = target.clone();
        assert!(
            target
                .append_transformed_view(&source, offset, f32::MAX, clip)
                .is_err()
        );
        assert_eq!(target, saved);
        target.clear();
        assert!(target.fill_paths.is_empty() && target.commands().is_empty());
    }
}
