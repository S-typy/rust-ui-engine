//! Bounded PDF subpaths, with device-space cubic flattening.
use super::{PdfError, invalid};
use rust_desktop_ui_core::{
    Color, FillPath, FillRule, MAX_FILL_EDGES, Point, Rect, Scene, StrokeSegment,
};

#[derive(Default)]
struct Subpath {
    points: Vec<Point>,
    closed: bool,
}

#[derive(Default)]
pub(super) struct Path {
    subpaths: Vec<Subpath>,
    points: usize,
    rectangle: Option<Rect>,
}

impl Path {
    pub(super) fn current(&self) -> Result<Point, PdfError> {
        let p = self
            .subpaths
            .last()
            .ok_or_else(|| invalid("path without move"))?;
        Ok(if p.closed {
            p.points[0]
        } else {
            *p.points.last().unwrap()
        })
    }

    pub(super) fn move_to(&mut self, point: Point) -> Result<(), PdfError> {
        self.reserve(point)?;
        self.rectangle = None;
        self.subpaths.push(Subpath {
            points: vec![point],
            closed: false,
        });
        Ok(())
    }

    fn reserve(&mut self, point: Point) -> Result<(), PdfError> {
        if !point.is_finite() || point.x.abs() > 10_000_000.0 || point.y.abs() > 10_000_000.0 {
            return Err(invalid("transformed path coordinate"));
        }
        if self.points >= MAX_FILL_EDGES {
            return Err(PdfError::Limit("256 path points"));
        }
        self.points += 1;
        Ok(())
    }

    pub(super) fn line_to(&mut self, point: Point) -> Result<(), PdfError> {
        let current = self.current()?;
        if self.subpaths.last().unwrap().closed {
            self.move_to(current)?;
        }
        if current != point {
            self.reserve(point)?;
            self.subpaths.last_mut().unwrap().points.push(point);
        }
        self.rectangle = None;
        Ok(())
    }

    pub(super) fn close(&mut self) {
        if let Some(path) = self.subpaths.last_mut() {
            path.closed = true;
        }
    }

    pub(super) fn rectangle(&mut self, points: [Point; 4]) -> Result<(), PdfError> {
        let single = self.subpaths.is_empty();
        self.move_to(points[0])?;
        for point in points.into_iter().skip(1) {
            self.line_to(point)?;
        }
        self.close();
        if single {
            let a = points[0];
            let b = points[2];
            self.rectangle = Some(Rect::new(
                a.x.min(b.x),
                a.y.min(b.y),
                (b.x - a.x).abs(),
                (b.y - a.y).abs(),
            ));
        }
        Ok(())
    }

    pub(super) fn clip_rect(&self) -> Result<Rect, PdfError> {
        self.rectangle
            .ok_or_else(|| PdfError::Unsupported("nonrectangular clipping".into()))
    }

    pub(super) fn curve_to(&mut self, a: Point, b: Point, end: Point) -> Result<(), PdfError> {
        for p in [a, b, end] {
            if !p.is_finite() || p.x.abs() > 10_000_000.0 || p.y.abs() > 10_000_000.0 {
                return Err(invalid("transformed curve coordinate"));
            }
        }
        self.flatten(self.current()?, a, b, end, 0)
    }

    fn flatten(
        &mut self,
        start: Point,
        a: Point,
        b: Point,
        end: Point,
        depth: u8,
    ) -> Result<(), PdfError> {
        // Distance to the finite chord also handles collinear overshoot and cusps.
        let distance = |p: Point| {
            let (x, y) = (f64::from(end.x - start.x), f64::from(end.y - start.y));
            let (px, py) = (f64::from(p.x - start.x), f64::from(p.y - start.y));
            let t = if x * x + y * y == 0.0 {
                0.0
            } else {
                ((px * x + py * y) / (x * x + y * y)).clamp(0.0, 1.0)
            };
            (px - t * x).hypot(py - t * y)
        };
        if distance(a).max(distance(b)) <= 0.125 {
            return self.line_to(end);
        }
        if depth == 16 {
            return Err(PdfError::Limit("cubic subdivision depth"));
        }
        let mid = |p: Point, q: Point| Point::new(p.x * 0.5 + q.x * 0.5, p.y * 0.5 + q.y * 0.5);
        let (sa, ab, be) = (mid(start, a), mid(a, b), mid(b, end));
        let (left, right) = (mid(sa, ab), mid(ab, be));
        let center = mid(left, right);
        self.flatten(start, sa, left, center, depth + 1)?;
        self.flatten(center, right, be, end, depth + 1)
    }

    pub(super) fn fill(
        &self,
        scene: &mut Scene,
        clip: Rect,
        color: Color,
        rule: FillRule,
    ) -> Result<(), PdfError> {
        if let Some(rect) = self.rectangle {
            if let Some(r) = rect.intersection(clip) {
                scene.fill(r, color);
            }
            return Ok(());
        }
        // Uniform rectangular rings are a normal border primitive. Keeping
        // them out of the bounded general-path buffer is essential for sheets.
        if rule == FillRule::EvenOdd
            && let Some((bounds, width)) = self.inset_border()
        {
            scene.rounded_stroke(bounds, clip, 0.0, width, color);
            return Ok(());
        }
        let contours: Vec<_> = self
            .subpaths
            .iter()
            .filter_map(|p| {
                let mut points = p.points.clone();
                if points.len() > 1 && points.last() == points.first() {
                    points.pop();
                }
                (points.len() >= 3).then_some(points)
            })
            .collect();
        if contours.is_empty() {
            return Ok(());
        }
        let path = FillPath {
            contours,
            clip,
            color,
            rule,
        };
        // Degenerate zero-extent fills paint nothing in PDF.
        if path.bounds().is_some() {
            scene.fill_path(path).map_err(invalid)?;
        }
        Ok(())
    }

    fn inset_border(&self) -> Option<(Rect, f32)> {
        fn rect(path: &Subpath) -> Option<Rect> {
            let mut p = path.points.as_slice();
            if p.len() == 5 && p.first() == p.last() {
                p = &p[..4];
            }
            if p.len() != 4 {
                return None;
            }
            let aligned = (p[0].y == p[1].y
                && p[1].x == p[2].x
                && p[2].y == p[3].y
                && p[3].x == p[0].x)
                || (p[0].x == p[1].x && p[1].y == p[2].y && p[2].x == p[3].x && p[3].y == p[0].y);
            if !aligned {
                return None;
            }
            Some(Rect::new(
                p[0].x.min(p[2].x),
                p[0].y.min(p[2].y),
                (p[0].x - p[2].x).abs(),
                (p[0].y - p[2].y).abs(),
            ))
        }
        if self.subpaths.len() != 2 {
            return None;
        }
        let (a, b) = (rect(&self.subpaths[0])?, rect(&self.subpaths[1])?);
        let (outer, inner) = if a.width > b.width { (a, b) } else { (b, a) };
        let widths = [
            inner.x - outer.x,
            inner.y - outer.y,
            outer.x + outer.width - inner.x - inner.width,
            outer.y + outer.height - inner.y - inner.height,
        ];
        let width = widths[0];
        (width > 0.0
            && inner.width > 0.0
            && inner.height > 0.0
            && widths.iter().all(|value| (*value - width).abs() <= 0.0001))
        .then_some((outer, width))
    }

    pub(super) fn segments(&self) -> Vec<StrokeSegment> {
        let mut segments = Vec::new();
        for path in &self.subpaths {
            segments.extend(path.points.windows(2).map(|p| StrokeSegment {
                start: p[0],
                end: p[1],
            }));
            if path.closed && path.points.len() > 1 && path.points.last() != path.points.first() {
                segments.push(StrokeSegment {
                    start: *path.points.last().unwrap(),
                    end: path.points[0],
                });
            }
        }
        segments
    }

    pub(super) fn has_joins(&self) -> bool {
        self.subpaths.iter().any(|p| p.points.len() > 2)
    }

    pub(super) fn has_open_ends(&self) -> bool {
        self.subpaths
            .iter()
            .any(|p| !p.closed && p.points.len() > 1)
    }

    pub(super) fn clear(&mut self) {
        *self = Self::default();
    }
}
