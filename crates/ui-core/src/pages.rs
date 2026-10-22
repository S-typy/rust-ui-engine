//! Immutable page snapshots shared by preview, export and platform printing.
//! Geometry uses logical pixels: 96 units equal one inch.
use crate::{Color, DrawCommand, Point, Rect, Scene, Size, StrokeSegment};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PageError(pub String);
impl std::fmt::Display for PageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for PageError {}
#[derive(Clone, Debug)]
pub struct PrintPage {
    pub size: Size,
    pub scene: Scene,
}
#[derive(Clone, Debug)]
pub struct PrintDocument {
    pages: Vec<PrintPage>,
    title: String,
}
impl PrintDocument {
    pub fn new(title: impl Into<String>, pages: Vec<PrintPage>) -> Result<Self, PageError> {
        let title = title.into();
        if title.len() > 1024 || title.contains('\0') || pages.is_empty() || pages.len() > 512 {
            return Err(PageError(
                "Invalid document title or page count (1..512)".into(),
            ));
        }
        let mut commands = 0;
        let mut text_bytes = 0;
        let mut image_bytes = 0;
        let mut images = std::collections::BTreeSet::new();
        for page in &pages {
            if !page.size.width.is_finite()
                || !page.size.height.is_finite()
                || !(1.0..=19200.0).contains(&page.size.width)
                || !(1.0..=19200.0).contains(&page.size.height)
            {
                return Err(PageError("Invalid page dimensions".into()));
            }
            commands += page.scene.commands().len();
            for command in page.scene.commands() {
                let valid = match *command {
                    DrawCommand::Rectangle(i) => i < page.scene.rectangles.len(),
                    DrawCommand::RoundedRectangle(i) => i < page.scene.rounded_rectangles.len(),
                    DrawCommand::Ellipse(i) => i < page.scene.ellipses.len(),
                    DrawCommand::StrokePath(i) => i < page.scene.stroke_paths.len(),
                    DrawCommand::Image(i) => i < page.scene.images.len(),
                    DrawCommand::Text(i) => page.scene.texts.get(i).is_some_and(|r| {
                        crate::valid_text_spans(&r.text, page.scene.text_spans(i))
                    }),
                };
                if !valid {
                    return Err(PageError("Invalid page command index or text spans".into()));
                }
            }
            if page
                .scene
                .rectangles
                .iter()
                .any(|r| !r.bounds.is_valid() || !valid_color(r.color))
                || page
                    .scene
                    .texts
                    .iter()
                    .any(|r| !r.is_valid() || r.sensitive)
                || page.scene.images.iter().any(|r| !r.is_valid())
                || page.scene.stroke_paths.iter().any(|r| !r.is_valid())
                || page.scene.ellipses.iter().any(|r| !r.is_valid())
                || page.scene.rounded_rectangles.iter().any(|r| !r.is_valid())
            {
                return Err(PageError("Invalid page geometry or protected text".into()));
            }
            text_bytes += page.scene.texts.iter().map(|t| t.text.len()).sum::<usize>();
            for image in &page.scene.images {
                if images.insert(image.bitmap.id()) {
                    image_bytes += image.bitmap.pixels().len();
                }
            }
        }
        if commands > 100_000 || text_bytes > 16 * 1024 * 1024 || image_bytes > 128 * 1024 * 1024 {
            return Err(PageError("Page document resource limit exceeded".into()));
        }
        Ok(Self { title, pages })
    }
    pub fn pages(&self) -> &[PrintPage] {
        &self.pages
    }
    pub fn title(&self) -> &str {
        &self.title
    }
}
fn valid_color(c: Color) -> bool {
    [c.r, c.g, c.b, c.a]
        .into_iter()
        .all(|v| v.is_finite() && (0.0..=1.0).contains(&v))
}
/// Scale all command types in painter order, preserving clips and text spans.
pub fn transform_page(
    source: &Scene,
    scale: f32,
    offset: Point,
    clip: Rect,
) -> Result<Scene, PageError> {
    if !scale.is_finite()
        || !(0.01..=8.0).contains(&scale)
        || !offset.x.is_finite()
        || !offset.y.is_finite()
        || !clip.is_valid()
    {
        return Err(PageError("Invalid page transform".into()));
    }
    let rect = |r: Rect| {
        Rect::new(
            offset.x + r.x * scale,
            offset.y + r.y * scale,
            r.width * scale,
            r.height * scale,
        )
    };
    let mut out = Scene::default();
    for command in source.commands() {
        match *command {
            DrawCommand::Rectangle(i) => {
                let r = &source.rectangles[i];
                if let Some(r) = rect(r.bounds).intersection(clip) {
                    out.fill(r, source.rectangles[i].color);
                }
            }
            DrawCommand::RoundedRectangle(i) => {
                let r = &source.rounded_rectangles[i];
                let Some(c) = rect(r.clip).intersection(clip) else {
                    continue;
                };
                if let Some(w) = r.stroke_width {
                    out.rounded_stroke(rect(r.bounds), c, r.radius * scale, w * scale, r.color);
                } else {
                    out.rounded_fill(rect(r.bounds), c, r.radius * scale, r.color);
                }
            }
            DrawCommand::Ellipse(i) => {
                let r = &source.ellipses[i];
                if let Some(c) = rect(r.clip).intersection(clip) {
                    out.ellipse(
                        rect(r.bounds),
                        c,
                        r.stroke_width.map(|w| w * scale),
                        r.color,
                    );
                }
            }
            DrawCommand::StrokePath(i) => {
                let p = &source.stroke_paths[i];
                let Some(c) = rect(p.clip).intersection(clip) else {
                    continue;
                };
                let point = |p: Point| Point::new(offset.x + p.x * scale, offset.y + p.y * scale);
                let segments = p
                    .segments
                    .iter()
                    .map(|s| StrokeSegment {
                        start: point(s.start),
                        end: point(s.end),
                    })
                    .collect::<Vec<_>>();
                out.stroke_path(&segments, c, p.width * scale, p.color);
            }
            DrawCommand::Text(i) => {
                let mut r = source.texts[i].clone();
                let Some(c) = rect(r.clip).intersection(clip) else {
                    continue;
                };
                r.bounds = rect(r.bounds);
                r.clip = c;
                r.font_size *= scale;
                if r.font_size > 512.0 {
                    return Err(PageError("Scaled font exceeds renderer limit".into()));
                }
                let mut spans = source.text_spans(i).to_vec();
                for s in &mut spans {
                    if let Some(f) = &mut s.font {
                        f.size *= scale;
                    }
                    if let Some(size) = &mut s.font_size {
                        *size *= scale;
                    }
                }
                out.styled_text(r, spans).map_err(|e| PageError(e.into()))?;
            }
            DrawCommand::Image(i) => {
                let mut image = source.images[i].clone();
                let Some(c) = rect(image.clip).intersection(clip) else {
                    continue;
                };
                image.bounds = rect(image.bounds);
                image.clip = c;
                out.image(image);
            }
        }
    }
    Ok(out)
}
