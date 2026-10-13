//! Small renderer-independent stroke unions; not a general vector path format.
use crate::{Color, Point, Rect};

/// Fixed per-path work bound, shared with renderer implementations.
pub const MAX_STROKE_SEGMENTS: usize = 16;

/// One straight segment in absolute logical pixels. Equal endpoints form a dot.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StrokeSegment {
    pub start: Point,
    pub end: Point,
}

/// Union of up to sixteen equally colored and equally wide round-ended segments.
/// Each pixel receives a single alpha contribution even at joins/crossings.
/// Disconnected segments are permitted. There are no fills, curves or dash rules.
#[derive(Clone, Debug, PartialEq)]
pub struct StrokePath {
    pub segments: Vec<StrokeSegment>,
    pub clip: Rect,
    pub width: f32,
    pub color: Color,
}

impl StrokePath {
    /// Stroke extent including round caps, before clipping and device AA padding.
    /// Invalid, overflowing or unrepresentable geometry returns `None`.
    pub fn bounds(&self) -> Option<Rect> {
        if self.segments.is_empty()
            || self.segments.len() > MAX_STROKE_SEGMENTS
            || !self.width.is_finite()
            || self.width <= 0.0
        {
            return None;
        }
        let mut left = f64::INFINITY;
        let mut top = f64::INFINITY;
        let mut right = f64::NEG_INFINITY;
        let mut bottom = f64::NEG_INFINITY;
        for segment in &self.segments {
            for point in [segment.start, segment.end] {
                if !point.is_finite() {
                    return None;
                }
                left = left.min(f64::from(point.x));
                top = top.min(f64::from(point.y));
                right = right.max(f64::from(point.x));
                bottom = bottom.max(f64::from(point.y));
            }
        }
        let radius = f64::from(self.width) * 0.5;
        let bounds = Rect::new(
            (left - radius) as f32,
            (top - radius) as f32,
            (right - left + f64::from(self.width)) as f32,
            (bottom - top + f64::from(self.width)) as f32,
        );
        (bounds.is_valid() && bounds.width > 0.0 && bounds.height > 0.0).then_some(bounds)
    }

    pub fn is_valid(&self) -> bool {
        self.bounds().is_some()
            && self.clip.is_valid()
            && [self.color.r, self.color.g, self.color.b, self.color.a]
                .into_iter()
                .all(|channel| channel.is_finite() && (0.0..=1.0).contains(&channel))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DrawCommand, Scene, TextRun};

    fn segment() -> StrokeSegment {
        StrokeSegment {
            start: Point::new(10.0, 20.0),
            end: Point::new(30.0, 40.0),
        }
    }

    #[test]
    fn stroke_extents_include_caps_and_clipping_preserves_endpoints() {
        let clip = Rect::new(15.0, 25.0, 10.0, 10.0);
        let mut scene = Scene::default();
        scene.stroke_path(&[segment()], clip, 4.0, Color::rgb(10, 20, 30));
        let path = &scene.stroke_paths[0];
        assert_eq!(path.segments, [segment()]);
        assert_eq!(path.clip, clip);
        assert_eq!(path.bounds(), Some(Rect::new(8.0, 18.0, 24.0, 24.0)));
        let point = StrokeSegment {
            start: Point::new(5.0, 5.0),
            end: Point::new(5.0, 5.0),
        };
        scene.stroke_path(
            &[point],
            Rect::new(0.0, 0.0, 10.0, 10.0),
            2.0,
            Color::rgb(0, 0, 0),
        );
        assert_eq!(
            scene.stroke_paths[1].bounds(),
            Some(Rect::new(4.0, 4.0, 2.0, 2.0))
        );
    }

    #[test]
    fn stroke_scene_append_and_clear_preserve_interleaved_painter_order() {
        let clip = Rect::new(0.0, 0.0, 100.0, 100.0);
        let color = Color::rgb(0, 0, 0);
        let mut scene = Scene::default();
        scene.fill(clip, color);
        scene.stroke_path(&[segment()], clip, 2.0, color);
        let mut overlay = Scene::default();
        overlay.text(TextRun::new("Caption", clip, color, 14.0));
        overlay.stroke_path(&[segment()], clip, 4.0, color);
        overlay.rounded_fill(clip, clip, 4.0, color);
        scene.append(&overlay);
        assert_eq!(
            scene.commands(),
            [
                DrawCommand::Rectangle(0),
                DrawCommand::StrokePath(0),
                DrawCommand::Text(0),
                DrawCommand::StrokePath(1),
                DrawCommand::RoundedRectangle(0)
            ]
        );
        assert_eq!(scene.stroke_paths[1].width, 4.0);
        scene.clear();
        assert!(scene.stroke_paths.is_empty() && scene.commands().is_empty());
        scene.stroke_path(&[segment()], clip, 2.0, color);
        assert_eq!(scene.commands(), [DrawCommand::StrokePath(0)]);
    }

    #[test]
    fn invalid_unbounded_or_fully_clipped_strokes_are_rejected() {
        let clip = Rect::new(0.0, 0.0, 100.0, 100.0);
        let color = Color::rgb(0, 0, 0);
        let mut scene = Scene::default();
        for width in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            scene.stroke_path(&[segment()], clip, width, color);
        }
        scene.stroke_path(&[], clip, 2.0, color);
        scene.stroke_path(&[segment(); MAX_STROKE_SEGMENTS + 1], clip, 2.0, color);
        for x in [f32::NAN, f32::INFINITY] {
            scene.stroke_path(
                &[StrokeSegment {
                    start: Point::new(x, 1.0),
                    ..segment()
                }],
                clip,
                2.0,
                color,
            );
        }
        scene.stroke_path(
            &[StrokeSegment {
                start: Point::new(-f32::MAX, 0.0),
                end: Point::new(f32::MAX, 0.0),
            }],
            clip,
            2.0,
            color,
        );
        scene.stroke_path(
            &[segment()],
            Rect::new(200.0, 200.0, 10.0, 10.0),
            2.0,
            color,
        );
        scene.stroke_path(&[segment()], clip, 2.0, Color { a: 1.1, ..color });
        assert!(scene.stroke_paths.is_empty() && scene.commands().is_empty());
        scene.stroke_path(&[segment(); MAX_STROKE_SEGMENTS], clip, 2.0, color);
        assert_eq!(scene.stroke_paths.len(), 1);
    }
}
