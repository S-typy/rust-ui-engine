//! Composition of independently retained views into one window scene.
use crate::{DrawCommand, Point, Rect, Scene, StrokePath, StrokeSegment};

impl Scene {
    /// Append a translated view, intersecting every primitive with a window-local
    /// clip. Text layout boxes and rounded geometry retain their original size.
    /// The source is unchanged; malformed references or overflowing geometry
    /// return an error without modifying the destination.
    pub fn append_view(
        &mut self,
        source: &Scene,
        offset: Point,
        clip: Rect,
    ) -> Result<(), &'static str> {
        if !offset.is_finite() || !clip.is_valid() {
            return Err("Invalid scene view transform");
        }
        let translate = |rect: Rect| {
            let result = Rect::new(
                rect.x + offset.x,
                rect.y + offset.y,
                rect.width,
                rect.height,
            );
            result
                .is_valid()
                .then_some(result)
                .ok_or("Scene view geometry overflow")
        };
        let mut view = Scene::default();
        for command in source.commands() {
            match *command {
                DrawCommand::Rectangle(index) => {
                    let item = source
                        .rectangles
                        .get(index)
                        .ok_or("Invalid rectangle reference")?;
                    if let Some(bounds) = translate(item.bounds)?.intersection(clip) {
                        view.fill(bounds, item.color);
                    }
                }
                DrawCommand::RoundedRectangle(index) => {
                    let item = source
                        .rounded_rectangles
                        .get(index)
                        .ok_or("Invalid rounded rectangle reference")?;
                    let bounds = translate(item.bounds)?;
                    if let Some(clip) = translate(item.clip)?.intersection(clip) {
                        if let Some(width) = item.stroke_width {
                            view.rounded_stroke(bounds, clip, item.radius, width, item.color);
                        } else {
                            view.rounded_fill(bounds, clip, item.radius, item.color);
                        }
                    }
                }
                DrawCommand::StrokePath(index) => {
                    let item = source
                        .stroke_paths
                        .get(index)
                        .ok_or("Invalid stroke reference")?;
                    if let Some(clip) = translate(item.clip)?.intersection(clip) {
                        let segments: Vec<_> = item
                            .segments
                            .iter()
                            .map(|s| StrokeSegment {
                                start: Point::new(s.start.x + offset.x, s.start.y + offset.y),
                                end: Point::new(s.end.x + offset.x, s.end.y + offset.y),
                            })
                            .collect();
                        let translated = StrokePath {
                            segments,
                            clip,
                            width: item.width,
                            color: item.color,
                        };
                        // Finite endpoints can still overflow when stroke caps
                        // expand their bounds. Do not silently drop such a path.
                        if translated.bounds().is_none() {
                            return Err("Scene view stroke overflow");
                        }
                        view.stroke_path(&translated.segments, clip, item.width, item.color);
                    }
                }
                DrawCommand::Text(index) => {
                    let item = source.texts.get(index).ok_or("Invalid text reference")?;
                    let mut item = item.clone();
                    item.bounds = translate(item.bounds)?;
                    if let Some(clip) = translate(item.clip)?.intersection(clip) {
                        item.clip = clip;
                        view.text(item);
                    }
                }
            }
        }
        self.append(&view);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Color, TextRun};

    #[test]
    fn composed_view_keeps_painter_order_and_unclipped_text_and_corner_geometry() {
        let bounds = Rect::new(0.0, 0.0, 100.0, 40.0);
        let mut source = Scene::default();
        source.fill(bounds, Color::rgb(0, 0, 0));
        source.rounded_fill(bounds, bounds, 8.0, Color::rgb(255, 255, 255));
        source.text(TextRun::new("document", bounds, Color::rgb(0, 0, 0), 14.0));
        source.stroke_path(
            &[StrokeSegment {
                start: Point::ZERO,
                end: Point::new(20.0, 20.0),
            }],
            bounds,
            2.0,
            Color::rgb(0, 0, 0),
        );
        let original = source.clone();
        let mut target = Scene::default();
        let clip = Rect::new(20.0, 40.0, 50.0, 20.0);
        target
            .append_view(&source, Point::new(10.0, 30.0), clip)
            .unwrap();
        assert_eq!(source, original);
        assert_eq!(target.commands(), source.commands());
        assert_eq!(target.rectangles[0].bounds, clip);
        assert_eq!(
            target.rounded_rectangles[0].bounds,
            Rect::new(10.0, 30.0, 100.0, 40.0)
        );
        assert_eq!(target.rounded_rectangles[0].clip, clip);
        assert_eq!(target.texts[0].bounds, target.rounded_rectangles[0].bounds);
        assert_eq!(target.texts[0].clip, clip);
        assert_eq!(
            target.stroke_paths[0].segments[0].start,
            Point::new(10.0, 30.0)
        );
    }

    #[test]
    fn invalid_view_is_atomic_and_fully_clipped_content_disappears() {
        let mut source = Scene::default();
        source.fill(Rect::new(0.0, 0.0, 10.0, 10.0), Color::rgb(0, 0, 0));
        let mut target = source.clone();
        let original = target.clone();
        assert!(
            target
                .append_view(
                    &source,
                    Point::new(f32::INFINITY, 0.0),
                    Rect::new(0.0, 0.0, 50.0, 50.0)
                )
                .is_err()
        );
        assert_eq!(target, original);
        target
            .append_view(
                &source,
                Point::new(100.0, 0.0),
                Rect::new(0.0, 0.0, 50.0, 50.0),
            )
            .unwrap();
        assert_eq!(target, original);
        source.rectangles.clear();
        assert!(
            target
                .append_view(&source, Point::ZERO, Rect::new(0.0, 0.0, 50.0, 50.0))
                .is_err()
        );
        assert_eq!(target, original);
    }

    #[test]
    fn translated_stroke_cap_overflow_is_rejected_atomically() {
        let extent = f32::MAX;
        let source_clip = Rect::new(0.0, 0.0, extent * 0.125, 1.0);
        let mut source = Scene::default();
        source.fill(source_clip, Color::rgb(12, 23, 34));
        source.stroke_path(
            &[StrokeSegment {
                start: Point::ZERO,
                end: Point::ZERO,
            }],
            source_clip,
            extent,
            Color::rgb(0, 0, 0),
        );
        assert_eq!(source.stroke_paths.len(), 1);
        let mut target = Scene::default();
        target.fill(Rect::new(0.0, 0.0, 2.0, 2.0), Color::rgb(255, 255, 255));
        let original = target.clone();
        let offset = Point::new(extent * 0.75, 0.0);
        let clip = Rect::new(offset.x, 0.0, extent * 0.125, 1.0);
        assert_eq!(
            target.append_view(&source, offset, clip),
            Err("Scene view stroke overflow")
        );
        assert_eq!(target, original);
    }
}
