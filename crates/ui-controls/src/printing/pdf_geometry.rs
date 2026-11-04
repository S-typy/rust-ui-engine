//! Native PDF paths for the bounded Scene shape vocabulary.
use super::*;

pub(super) fn ink(
    color: Color,
    stroke: bool,
    states: &mut Dictionary,
) -> Result<Vec<Operation>, PdfError> {
    let rgb = op(
        if stroke { "RG" } else { "rg" },
        vec![raw(color.r), raw(color.g), raw(color.b)],
    );
    // Every primitive is q/Q-scoped, so ordinary opaque pages need no ExtGState.
    if color.a == 1.0 {
        return Ok(vec![rgb]);
    }
    let name = format!("A{:08x}", color.a.to_bits());
    if !states.has(name.as_bytes()) {
        if states.len() >= 1024 {
            return Err(PdfError::Limit("1024 PDF opacity states/page"));
        }
        states.set(name.clone(), dictionary! {"Type"=>"ExtGState", "CA"=>raw(color.a), "ca"=>raw(color.a), "BM"=>"Normal", "AIS"=>false});
    }
    Ok(vec![op("gs", vec![Object::Name(name.into_bytes())]), rgb])
}

fn point(p: Point, h: f32) -> Vec<Object> {
    vec![real(p.x), real(h - p.y)]
}
fn curve(a: Point, b: Point, c: Point, h: f32) -> Operation {
    op("c", [point(a, h), point(b, h), point(c, h)].concat())
}
const K: f32 = 0.552_284_8;

pub(super) fn rounded(rect: Rect, radius: f32, h: f32, out: &mut Vec<Operation>) {
    if radius == 0.0 {
        out.push(rectangle(rect, h));
        return;
    }
    let (x, y, right, bottom, r) = (
        rect.x,
        rect.y,
        rect.x + rect.width,
        rect.y + rect.height,
        radius,
    );
    let k = r * K;
    out.extend([
        op("m", point(Point::new(x + r, y), h)),
        op("l", point(Point::new(right - r, y), h)),
        curve(
            Point::new(right - r + k, y),
            Point::new(right, y + r - k),
            Point::new(right, y + r),
            h,
        ),
        op("l", point(Point::new(right, bottom - r), h)),
        curve(
            Point::new(right, bottom - r + k),
            Point::new(right - r + k, bottom),
            Point::new(right - r, bottom),
            h,
        ),
        op("l", point(Point::new(x + r, bottom), h)),
        curve(
            Point::new(x + r - k, bottom),
            Point::new(x, bottom - r + k),
            Point::new(x, bottom - r),
            h,
        ),
        op("l", point(Point::new(x, y + r), h)),
        curve(
            Point::new(x, y + r - k),
            Point::new(x + r - k, y),
            Point::new(x + r, y),
            h,
        ),
        op("h", vec![]),
    ]);
}
pub(super) fn ellipse(rect: Rect, h: f32, out: &mut Vec<Operation>) {
    let (rx, ry) = (rect.width / 2.0, rect.height / 2.0);
    let (cx, cy) = (rect.x + rx, rect.y + ry);
    out.extend([
        op("m", point(Point::new(cx + rx, cy), h)),
        curve(
            Point::new(cx + rx, cy + ry * K),
            Point::new(cx + rx * K, cy + ry),
            Point::new(cx, cy + ry),
            h,
        ),
        curve(
            Point::new(cx - rx * K, cy + ry),
            Point::new(cx - rx, cy + ry * K),
            Point::new(cx - rx, cy),
            h,
        ),
        curve(
            Point::new(cx - rx, cy - ry * K),
            Point::new(cx - rx * K, cy - ry),
            Point::new(cx, cy - ry),
            h,
        ),
        curve(
            Point::new(cx + rx * K, cy - ry),
            Point::new(cx + rx, cy - ry * K),
            Point::new(cx + rx, cy),
            h,
        ),
        op("h", vec![]),
    ]);
}
pub(super) fn inset(rect: Rect, width: f32) -> Option<Rect> {
    (rect.width > 2.0 * width && rect.height > 2.0 * width).then(|| {
        Rect::new(
            rect.x + width,
            rect.y + width,
            rect.width - 2.0 * width,
            rect.height - 2.0 * width,
        )
    })
}
