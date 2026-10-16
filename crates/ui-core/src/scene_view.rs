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
        self.append_transformed_view(source, offset, 1.0, clip)
    }

    /// Append a uniformly scaled and translated view atomically. The target
    /// clip is in destination coordinates; bitmap source pixels never scale.
    /// Transformed text must retain a valid font size (at most 512 logical px).
    /// Rotation, skew and isolated group opacity are not part of this transform.
    pub fn append_transformed_view(
        &mut self,
        source: &Scene,
        offset: Point,
        scale: f32,
        clip: Rect,
    ) -> Result<(), &'static str> {
        if !offset.is_finite() || !clip.is_valid() || !scale.is_finite() || scale <= 0.0 {
            return Err("Invalid scene view transform");
        }
        let translate = |rect: Rect| {
            let result = Rect::new(
                rect.x * scale + offset.x,
                rect.y * scale + offset.y,
                rect.width * scale,
                rect.height * scale,
            );
            result
                .is_valid()
                .then_some(result)
                .filter(|result| {
                    (rect.width == 0.0 || result.width > 0.0)
                        && (rect.height == 0.0 || result.height > 0.0)
                })
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
                    if [item.color.r, item.color.g, item.color.b, item.color.a]
                        .into_iter()
                        .any(|channel| !channel.is_finite() || !(0.0..=1.0).contains(&channel))
                    {
                        return Err("Invalid scene rectangle color");
                    }
                    if let Some(bounds) = translate(item.bounds)?.intersection(clip) {
                        view.fill(bounds, item.color);
                    }
                }
                DrawCommand::RoundedRectangle(index) => {
                    let item = source
                        .rounded_rectangles
                        .get(index)
                        .ok_or("Invalid rounded rectangle reference")?;
                    if !item.is_valid() {
                        return Err("Invalid scene rounded rectangle");
                    }
                    let bounds = translate(item.bounds)?;
                    if let Some(clip) = translate(item.clip)?.intersection(clip) {
                        if let Some(width) = item.stroke_width {
                            view.rounded_stroke(
                                bounds,
                                clip,
                                item.radius * scale,
                                width * scale,
                                item.color,
                            );
                        } else {
                            view.rounded_fill(bounds, clip, item.radius * scale, item.color);
                        }
                    }
                }
                DrawCommand::StrokePath(index) => {
                    let item = source
                        .stroke_paths
                        .get(index)
                        .ok_or("Invalid stroke reference")?;
                    if !item.is_valid() {
                        return Err("Invalid scene stroke");
                    }
                    if let Some(clip) = translate(item.clip)?.intersection(clip) {
                        let segments: Vec<_> = item
                            .segments
                            .iter()
                            .map(|s| StrokeSegment {
                                start: Point::new(
                                    s.start.x * scale + offset.x,
                                    s.start.y * scale + offset.y,
                                ),
                                end: Point::new(
                                    s.end.x * scale + offset.x,
                                    s.end.y * scale + offset.y,
                                ),
                            })
                            .collect();
                        let translated = StrokePath {
                            segments,
                            clip,
                            width: item.width * scale,
                            color: item.color,
                        };
                        // Finite endpoints can still overflow when stroke caps
                        // expand their bounds. Do not silently drop such a path.
                        if !translated.is_valid() {
                            return Err("Scene view stroke overflow");
                        }
                        view.stroke_path(&translated.segments, clip, translated.width, item.color);
                    }
                }
                DrawCommand::Text(index) => {
                    let item = source.texts.get(index).ok_or("Invalid text reference")?;
                    let mut item = item.clone();
                    let mut spans = source.text_spans(index).to_vec();
                    for span in &mut spans {
                        if let Some(font) = &mut span.font {
                            font.size *= scale;
                        }
                        if let Some(size) = &mut span.font_size {
                            *size *= scale;
                        }
                    }
                    item.bounds = translate(item.bounds)?;
                    item.font_size *= scale;
                    if !item.is_valid() || !crate::valid_text_spans(&item.text, &spans) {
                        return Err("Scene view text metrics or color are invalid");
                    }
                    if let Some(clip) = translate(item.clip)?.intersection(clip) {
                        item.clip = clip;
                        view.styled_text(item, spans)?;
                    }
                }
                DrawCommand::Image(index) => {
                    let mut item = source
                        .images
                        .get(index)
                        .ok_or("Invalid image reference")?
                        .clone();
                    if !item.is_valid() {
                        return Err("Invalid scene image");
                    }
                    item.bounds = translate(item.bounds)?;
                    if let Some(clip) = translate(item.clip)?.intersection(clip) {
                        item.clip = clip;
                        view.image(item);
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
    fn inline_ranges_survive_append_transform_and_clear() {
        let bounds = Rect::new(0.0, 0.0, 100.0, 30.0);
        let mut source = Scene::default();
        source
            .styled_text(
                TextRun::new("Hello", bounds, Color::rgb(0, 0, 0), 14.0),
                vec![crate::TextSpan {
                    font: Some(crate::FontDescriptor {
                        size: 16.0,
                        ..Default::default()
                    }),
                    range: 0..5,
                    font_size: Some(18.0),
                    weight: Some(700),
                    color: Some(Color::rgb(10, 100, 200)),
                }],
            )
            .unwrap();
        let mut target = Scene::default();
        target.text(TextRun::new("Before", bounds, Color::rgb(0, 0, 0), 14.0));
        target
            .append_transformed_view(
                &source,
                Point::new(3.0, 4.0),
                2.0,
                Rect::new(0.0, 0.0, 500.0, 100.0),
            )
            .unwrap();
        assert!(target.text_spans(0).is_empty());
        assert_eq!(target.text_spans(1)[0].font_size, Some(36.0));
        assert_eq!(target.text_spans(1)[0].font.as_ref().unwrap().size, 32.0);
        assert_eq!(source.text_spans(0)[0].font.as_ref().unwrap().size, 16.0);
        assert_eq!(source.text_spans(0)[0].font_size, Some(18.0));
        let saved = target.clone();
        source.texts[0].text = "H".into();
        assert!(target.append_view(&source, Point::ZERO, bounds).is_err());
        assert_eq!(target, saved);
        target.clear();
        assert!(target.text_spans(1).is_empty());
    }

    #[test]
    fn scaled_text_limit_and_corrupt_source_metrics_fail_atomically() {
        let bounds = Rect::new(0.0, 0.0, 10.0, 10.0);
        let mut source = Scene::default();
        source.fill(bounds, Color::rgb(1, 2, 3));
        source.text(TextRun::new("text", bounds, Color::rgb(0, 0, 0), 512.0));
        let mut target = source.clone();
        let original = target.clone();
        assert!(
            target
                .append_transformed_view(&source, Point::ZERO, 2.0, bounds)
                .is_err()
        );
        assert_eq!(target, original);
        source.texts[0].font_size = 12.0;
        source.rounded_fill(bounds, bounds, 2.0, Color::rgb(1, 2, 3));
        source.rounded_rectangles[0].radius = f32::NAN;
        assert!(target.append_view(&source, Point::ZERO, bounds).is_err());
        assert_eq!(target, original);
        source.rounded_rectangles[0].radius = 2.0;
        source.rectangles[0].color.a = f32::INFINITY;
        assert!(target.append_view(&source, Point::ZERO, bounds).is_err());
        assert_eq!(target, original);
    }

    #[test]
    fn scaling_transforms_geometry_metrics_and_clips_without_changing_bitmap_source() {
        use crate::{Bitmap, ImagePrimitive};
        let bounds = Rect::new(2.0, 3.0, 10.0, 8.0);
        let mut source = Scene::default();
        source.rounded_stroke(bounds, bounds, 2.0, 1.0, Color::rgb(0, 0, 0));
        source.text(TextRun::new("text", bounds, Color::rgb(0, 0, 0), 10.0));
        source.stroke_path(
            &[StrokeSegment {
                start: Point::new(2.0, 3.0),
                end: Point::new(5.0, 6.0),
            }],
            bounds,
            1.0,
            Color::rgb(0, 0, 0),
        );
        let bitmap = Bitmap::new_rgba8(1, 1, vec![255; 4]).unwrap();
        source.image(ImagePrimitive::new(bitmap.clone(), bounds));
        let mut target = Scene::default();
        target
            .append_transformed_view(
                &source,
                Point::new(10.0, 20.0),
                2.0,
                Rect::new(16.0, 26.0, 100.0, 100.0),
            )
            .unwrap();
        assert_eq!(target.commands(), source.commands());
        assert_eq!(target.rounded_rectangles[0].radius, 4.0);
        assert_eq!(target.rounded_rectangles[0].stroke_width, Some(2.0));
        assert_eq!(target.texts[0].font_size, 20.0);
        assert_eq!(target.stroke_paths[0].width, 2.0);
        assert_eq!(target.images[0].bounds, Rect::new(14.0, 26.0, 20.0, 16.0));
        assert_eq!(target.images[0].clip, Rect::new(16.0, 26.0, 18.0, 16.0));
        assert_eq!(target.images[0].source, source.images[0].source);
        assert_eq!(target.images[0].bitmap, bitmap);
        let original = target.clone();
        for scale in [0.0, -1.0, f32::NAN, f32::MAX] {
            assert!(
                target
                    .append_transformed_view(&source, Point::ZERO, scale, bounds)
                    .is_err()
            );
            assert_eq!(target, original);
        }
        source.images.clear();
        assert!(target.append_view(&source, Point::ZERO, bounds).is_err());
        assert_eq!(target, original);
    }

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
