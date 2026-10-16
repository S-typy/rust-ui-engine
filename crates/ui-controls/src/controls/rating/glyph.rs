//! Own bounded star geometry; no font glyph or external artwork.
use super::*;
use rust_desktop_ui_core::StrokeSegment;
pub(super) fn geometry(bounds: Rect, settings: RatingSettings) -> (Rect, f32, f32) {
    let natural = f32::from(settings.maximum) * settings.glyph_size
        + f32::from(settings.maximum - 1) * settings.spacing;
    let scale = ((bounds.width - 64.0).max(0.0) / natural)
        .min((bounds.height - 8.0).max(0.0) / settings.glyph_size)
        .min(1.0);
    let size = settings.glyph_size * scale;
    let gap = settings.spacing * scale;
    let width = f32::from(settings.maximum) * size + f32::from(settings.maximum - 1) * gap;
    (
        Rect::new(
            bounds.x + 4.0,
            bounds.y + (bounds.height - size) * 0.5,
            width,
            size,
        ),
        size,
        gap,
    )
}
pub(super) fn paint(
    scene: &mut Scene,
    bounds: Rect,
    clip: Rect,
    fraction: f32,
    active: Color,
    outline: Color,
) {
    if bounds.width <= 0.0 || bounds.height <= 0.0 {
        return;
    }
    let mut points = [Point::ZERO; 10];
    for (index, point) in points.iter_mut().enumerate() {
        let angle = -std::f32::consts::FRAC_PI_2 + index as f32 * std::f32::consts::PI / 5.0;
        let radius = if index % 2 == 0 { 0.46 } else { 0.22 };
        *point = Point::new(
            bounds.x + bounds.width * (0.5 + radius * angle.cos()),
            bounds.y + bounds.height * (0.5 + radius * angle.sin()),
        );
    }
    let segments: Vec<_> = (0..10)
        .map(|i| StrokeSegment {
            start: points[i],
            end: points[(i + 1) % 10],
        })
        .collect();
    scene.stroke_path(&segments, clip, 1.25, outline);
    let Some(fill_clip) =
        Rect::new(bounds.x, bounds.y, bounds.width * fraction, bounds.height).intersection(clip)
    else {
        return;
    };
    if fraction <= 0.0 {
        return;
    }
    // Opaque, adjoining bands have no antialiasing seams between scanlines.
    // Split at every vertex, then inset each band's sides to stay inside the
    // polygon for its entire height. The outline covers the <=0.25px inset.
    // At most 256 regular bands plus ten vertex cuts, with two spans per band.
    for rect in fill_bands(&points, bounds) {
        if let Some(rect) = rect.intersection(fill_clip) {
            scene.fill(rect, active);
        }
    }
    scene.stroke_path(&segments, fill_clip, 1.25, active);
}

fn fill_bands(points: &[Point; 10], bounds: Rect) -> Vec<Rect> {
    let rows = (bounds.height * 4.0).ceil().clamp(1.0, 256.0) as usize;
    let mut cuts: Vec<_> = (0..=rows)
        .map(|row| bounds.y + bounds.height * row as f32 / rows as f32)
        .chain(points.iter().map(|point| point.y))
        .collect();
    cuts.sort_by(f32::total_cmp);
    cuts.dedup();
    let mut result = Vec::new();
    for ys in cuts.windows(2) {
        let mid = (ys[0] + ys[1]) * 0.5;
        let mut edges: Vec<_> = (0..10)
            .filter_map(|i| {
                let a = points[i];
                let b = points[(i + 1) % 10];
                ((a.y <= mid && b.y > mid) || (b.y <= mid && a.y > mid)).then(|| {
                    let at = |y| a.x + (y - a.y) / (b.y - a.y) * (b.x - a.x);
                    (at(mid), at(ys[0]), at(ys[1]))
                })
            })
            .collect();
        edges.sort_by(|a, b| a.0.total_cmp(&b.0));
        for pair in edges.chunks_exact(2) {
            let left = pair[0].1.max(pair[0].2);
            let right = pair[1].1.min(pair[1].2);
            if right > left {
                result.push(Rect::new(left, ys[0], right - left, ys[1] - ys[0]));
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn contains(points: &[Point; 10], point: Point) -> bool {
        let mut inside = false;
        for i in 0..10 {
            let a = points[i];
            let b = points[(i + 1) % 10];
            if (a.y > point.y) != (b.y > point.y)
                && point.x < (b.x - a.x) * (point.y - a.y) / (b.y - a.y) + a.x
            {
                inside = !inside;
            }
        }
        inside
    }

    #[test]
    fn rating_fill_is_solid_inside_polygon_and_fraction_clip() {
        for size in [8.0, 24.0, 36.0, 64.0] {
            for fraction in [0.0, 0.25, 0.5, 0.75, 1.0] {
                let bounds = Rect::new(13.25, 7.5, size, size);
                let mut scene = Scene::default();
                paint(
                    &mut scene,
                    bounds,
                    bounds,
                    fraction,
                    Color::rgb(0, 100, 255),
                    Color::rgb(100, 100, 100),
                );
                assert!(scene.rectangles.len() <= 532);
                let points: [Point; 10] =
                    std::array::from_fn(|i| scene.stroke_paths[0].segments[i].start);
                for rect in &scene.rectangles {
                    let r = rect.bounds;
                    assert!(r.x >= bounds.x && r.y >= bounds.y);
                    assert!(r.x + r.width <= bounds.x + bounds.width * fraction + 0.0001);
                    assert!(r.y + r.height <= bounds.y + bounds.height);
                    // Insets avoid ambiguity on the exact polygon boundary.
                    for x in [r.x + r.width * 0.001, r.x + r.width * 0.999] {
                        for y in [r.y + r.height * 0.001, r.y + r.height * 0.999] {
                            assert!(
                                contains(&points, Point::new(x, y)),
                                "fill outside polygon: {size} {fraction} {r:?}"
                            );
                        }
                    }
                }
                if fraction >= 0.75 {
                    // Every central sample is covered at 1x, 1.5x and 2x;
                    // unlike narrow antialiased strokes, bands have no gaps.
                    for scale in [1.0, 1.5, 2.0] {
                        for row in 0..(size * 0.24 * scale) as usize {
                            let y = bounds.y + size * 0.35 + (row as f32 + 0.5) / scale;
                            let x = bounds.x + size * 0.5;
                            assert!(scene.rectangles.iter().any(|r| r.bounds.contains(x, y)));
                        }
                    }
                }
            }
        }
    }
}
