//! Shared page export. PDF text uses shaped glyph images with ActualText;
//! no system font binaries are embedded. The page UI still renders through Scene.
use crate::pdf::{PdfError, invalid};
use lopdf::{
    Dictionary, Document, Object, ObjectId, Stream,
    content::{Content, Operation},
    dictionary,
};
use rust_desktop_ui_core::{
    pages::{PrintDocument, PrintPage},
    *,
};
use rust_desktop_ui_text::TextEngine;

pub fn from_rich_document(
    document: &crate::rich_document::RichDocument,
    engine: &mut TextEngine,
    title: &str,
) -> Result<PrintDocument, PdfError> {
    let layout = document.paginate(engine).map_err(invalid)?;
    PrintDocument::new(
        title,
        layout
            .pages
            .into_iter()
            .map(|page| PrintPage {
                size: Size::new(document.page.width, document.page.height),
                scene: page.scene,
            })
            .collect(),
    )
    .map_err(invalid)
}
/// Replace text with the same shaped glyph bitmaps used by the GPU renderer.
/// This prepares a printer transport; it is not an alternative UI renderer.
pub fn prepare_for_printer(
    source: &PrintDocument,
    engine: &mut TextEngine,
    scale: f32,
    mut cancelled: impl FnMut() -> bool,
) -> Result<PrintDocument, PdfError> {
    if !scale.is_finite() || !(1.0..=4.0).contains(&scale) {
        return Err(invalid("print glyph scale must be 1..4"));
    }
    let mut pages = Vec::new();
    for page in source.pages() {
        if cancelled() {
            return Err(PdfError::Cancelled);
        }
        let mut scene = Scene::default();
        for command in page.scene.commands() {
            match *command {
                DrawCommand::Text(i) => {
                    let run = &page.scene.texts[i];
                    let prepared = engine
                        .prepare_styled(run, page.scene.text_spans(i), scale)
                        .map_err(invalid)?;
                    for g in &prepared.glyphs {
                        let mut pixels = g.bitmap.rgba.clone();
                        for p in pixels.chunks_exact_mut(4) {
                            p[0] = (f32::from(p[0]) * g.tint.r).round() as u8;
                            p[1] = (f32::from(p[1]) * g.tint.g).round() as u8;
                            p[2] = (f32::from(p[2]) * g.tint.b).round() as u8;
                            p[3] = (f32::from(p[3]) * g.tint.a).round() as u8;
                        }
                        let bitmap = Bitmap::new_rgba8(g.bitmap.width, g.bitmap.height, pixels)
                            .map_err(invalid)?;
                        let mut image = ImagePrimitive::new(
                            bitmap,
                            Rect::new(
                                g.position.x / scale,
                                g.position.y / scale,
                                g.bitmap.width as f32 / scale,
                                g.bitmap.height as f32 / scale,
                            ),
                        );
                        image.clip = Rect::new(
                            g.clip.x / scale,
                            g.clip.y / scale,
                            g.clip.width / scale,
                            g.clip.height / scale,
                        );
                        scene.image(image);
                    }
                }
                DrawCommand::Rectangle(i) => {
                    let r = &page.scene.rectangles[i];
                    scene.fill(r.bounds, r.color);
                }
                DrawCommand::RoundedRectangle(i) => {
                    let r = &page.scene.rounded_rectangles[i];
                    if let Some(w) = r.stroke_width {
                        scene.rounded_stroke(r.bounds, r.clip, r.radius, w, r.color);
                    } else {
                        scene.rounded_fill(r.bounds, r.clip, r.radius, r.color);
                    }
                }
                DrawCommand::StrokePath(i) => {
                    let p = &page.scene.stroke_paths[i];
                    scene.stroke_path(&p.segments, p.clip, p.width, p.color);
                }
                DrawCommand::Image(i) => scene.image(page.scene.images[i].clone()),
            }
            if cancelled() {
                return Err(PdfError::Cancelled);
            }
        }
        pages.push(PrintPage {
            size: page.size,
            scene,
        });
    }
    PrintDocument::new(source.title(), pages).map_err(invalid)
}
pub fn export_pdf(
    source: &PrintDocument,
    engine: &mut TextEngine,
    mut cancelled: impl FnMut() -> bool,
) -> Result<Vec<u8>, PdfError> {
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let mut kids = Vec::new();
    let mut budget = 0;
    for page in source.pages() {
        if cancelled() {
            return Err(PdfError::Cancelled);
        }
        let (content, resources) =
            page_content(&mut doc, page, engine, &mut budget, &mut cancelled)?;
        let stream = doc.add_object(Stream::new(Dictionary::new(), content));
        let id = doc.add_object(dictionary! {"Type"=>"Page","Parent"=>pages_id,
        "MediaBox"=>vec![0.into(),0.into(),real(page.size.width),real(page.size.height)],
        "Contents"=>stream,"Resources"=>resources});
        kids.push(id.into());
    }
    doc.objects.insert(
        pages_id,
        dictionary! {"Type"=>"Pages","Kids"=>kids,"Count"=>source.pages().len() as i64}.into(),
    );
    let root = doc.add_object(dictionary! {"Type"=>"Catalog","Pages"=>pages_id});
    let info=doc.add_object(dictionary!{"Title"=>lopdf::text_string(source.title()),"Producer"=>lopdf::text_string("Rust UI Engine")});
    doc.trailer.set("Root", root);
    doc.trailer.set("Info", info);
    doc.compress();
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).map_err(invalid)?;
    if bytes.len() > 64 * 1024 * 1024 {
        return Err(PdfError::Limit("64 MiB exported PDF"));
    }
    Ok(bytes)
}
pub(crate) fn field_appearance(
    doc: &mut Document,
    value: &str,
    bounds: Rect,
) -> Result<ObjectId, PdfError> {
    let mut scene = Scene::default();
    scene.fill(
        Rect::new(0.0, 0.0, bounds.width, bounds.height),
        Color::rgb(255, 255, 255),
    );
    scene.text(TextRun::new(
        value,
        Rect::new(
            3.0,
            2.0,
            (bounds.width - 6.0).max(1.0),
            (bounds.height - 4.0).max(1.0),
        ),
        Color::rgb(0, 0, 0),
        14.0,
    ));
    let page = PrintPage {
        size: Size::new(bounds.width, bounds.height),
        scene,
    };
    let (content, resources) =
        page_content(doc, &page, &mut TextEngine::new(), &mut 0, &mut || false)?;
    Ok(doc.add_object(Stream::new(dictionary!{"Type"=>"XObject","Subtype"=>"Form","FormType"=>1,
        "BBox"=>vec![0.into(),0.into(),real(bounds.width),real(bounds.height)],"Resources"=>resources},content)))
}
fn real(n: f32) -> Object {
    Object::Real(n * 0.75)
}
fn raw(n: f32) -> Object {
    Object::Real(n)
}
fn op(name: &str, args: Vec<Object>) -> Operation {
    Operation::new(name, args)
}
fn rectangle(r: Rect, h: f32) -> Operation {
    op(
        "re",
        vec![
            real(r.x),
            real(h - r.y - r.height),
            real(r.width),
            real(r.height),
        ],
    )
}
fn color(c: Color, stroke: bool) -> Result<Operation, PdfError> {
    if c.a != 1.0 {
        return Err(PdfError::Unsupported(
            "transparent vector fill in PDF export".into(),
        ));
    }
    Ok(op(
        if stroke { "RG" } else { "rg" },
        vec![raw(c.r), raw(c.g), raw(c.b)],
    ))
}
fn page_content(
    doc: &mut Document,
    page: &PrintPage,
    engine: &mut TextEngine,
    budget: &mut usize,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<(Vec<u8>, Dictionary), PdfError> {
    let mut operations = Vec::new();
    let mut objects = Dictionary::new();
    let h = page.size.height;
    for command in page.scene.commands() {
        if cancelled() {
            return Err(PdfError::Cancelled);
        }
        match *command {
            DrawCommand::Rectangle(i) => {
                let r = &page.scene.rectangles[i];
                operations.push(color(r.color, false)?);
                operations.push(rectangle(r.bounds, h));
                operations.push(op("f", vec![]));
            }
            DrawCommand::RoundedRectangle(i) => {
                let r = &page.scene.rounded_rectangles[i];
                if r.radius != 0.0 {
                    return Err(PdfError::Unsupported(
                        "rounded vector rectangle export".into(),
                    ));
                }
                operations.extend([
                    op("q", vec![]),
                    rectangle(r.clip, h),
                    op("W", vec![]),
                    op("n", vec![]),
                ]);
                operations.push(color(r.color, r.stroke_width.is_some())?);
                if let Some(w) = r.stroke_width {
                    operations.push(op("w", vec![real(w)]));
                }
                operations.push(rectangle(r.bounds, h));
                operations.push(op(if r.stroke_width.is_some() { "S" } else { "f" }, vec![]));
                operations.push(op("Q", vec![]));
            }
            DrawCommand::StrokePath(i) => {
                let p = &page.scene.stroke_paths[i];
                operations.extend([
                    op("q", vec![]),
                    rectangle(p.clip, h),
                    op("W", vec![]),
                    op("n", vec![]),
                    color(p.color, true)?,
                    op("w", vec![real(p.width)]),
                ]);
                for s in &p.segments {
                    operations.extend([
                        op("m", vec![real(s.start.x), real(h - s.start.y)]),
                        op("l", vec![real(s.end.x), real(h - s.end.y)]),
                    ]);
                }
                operations.extend([op("S", vec![]), op("Q", vec![])]);
            }
            DrawCommand::Image(i) => emit_image(
                doc,
                &page.scene.images[i],
                h,
                &mut operations,
                &mut objects,
                budget,
            )?,
            DrawCommand::Text(i) => {
                let run = &page.scene.texts[i];
                operations.push(op(
                    "BDC",
                    vec![
                        Object::Name(b"Span".to_vec()),
                        dictionary! {"ActualText"=>lopdf::text_string(&run.text)}.into(),
                    ],
                ));
                let one = PrintDocument::new(
                    "text",
                    vec![PrintPage {
                        size: page.size,
                        scene: {
                            let mut s = Scene::default();
                            s.styled_text(run.clone(), page.scene.text_spans(i).to_vec())
                                .map_err(invalid)?;
                            s
                        },
                    }],
                )
                .map_err(invalid)?;
                let shaped = prepare_for_printer(&one, engine, 2.0, &mut *cancelled)?;
                for image in &shaped.pages()[0].scene.images {
                    emit_image(doc, image, h, &mut operations, &mut objects, budget)?;
                }
                operations.push(op("EMC", vec![]));
            }
        }
        if operations.len() > 200_000 {
            return Err(PdfError::Limit("PDF page operations"));
        }
    }
    Ok((
        Content { operations }.encode().map_err(invalid)?,
        dictionary! {"XObject"=>objects},
    ))
}
fn emit_image(
    doc: &mut Document,
    image: &ImagePrimitive,
    h: f32,
    operations: &mut Vec<Operation>,
    objects: &mut Dictionary,
    budget: &mut usize,
) -> Result<(), PdfError> {
    let b = &image.bitmap;
    if image.source != Rect::new(0.0, 0.0, b.width() as f32, b.height() as f32) {
        return Err(PdfError::Unsupported("cropped source image export".into()));
    }
    *budget = budget
        .checked_add(b.pixels().len())
        .ok_or(PdfError::Limit("image bytes"))?;
    if *budget > 128 * 1024 * 1024 || doc.objects.len() > 19_000 {
        return Err(PdfError::Limit("PDF image/object budget"));
    }
    let mut rgb = Vec::with_capacity(b.pixels().len() / 4 * 3);
    let mut alpha = Vec::with_capacity(b.pixels().len() / 4);
    for p in b.pixels().chunks_exact(4) {
        rgb.extend_from_slice(&p[..3]);
        alpha.push((f32::from(p[3]) * image.opacity).round() as u8);
    }
    let mask = doc.add_object(Stream::new(
        dictionary! {"Type"=>"XObject","Subtype"=>"Image","Width"=>b.width() as i64,
        "Height"=>b.height() as i64,"ColorSpace"=>"DeviceGray","BitsPerComponent"=>8},
        alpha,
    ));
    let id = doc.add_object(Stream::new(
        dictionary! {"Type"=>"XObject","Subtype"=>"Image","Width"=>b.width() as i64,
        "Height"=>b.height() as i64,"ColorSpace"=>"DeviceRGB","BitsPerComponent"=>8,"SMask"=>mask},
        rgb,
    ));
    let name = format!("I{}", objects.len());
    objects.set(name.clone(), id);
    let r = image.bounds;
    operations.extend([
        op("q", vec![]),
        rectangle(image.clip, h),
        op("W", vec![]),
        op("n", vec![]),
        op(
            "cm",
            vec![
                real(r.width),
                0.into(),
                0.into(),
                real(r.height),
                real(r.x),
                real(h - r.y - r.height),
            ],
        ),
        op("Do", vec![Object::Name(name.into_bytes())]),
        op("Q", vec![]),
    ]);
    Ok(())
}
