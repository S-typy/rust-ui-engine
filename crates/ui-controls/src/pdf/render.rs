use super::*;
use lopdf::content::Content;
use rust_desktop_ui_core::{
    Bitmap, Color, ImagePrimitive, Point, Scene, Size, StrokeSegment, TextRun,
};
use rust_desktop_ui_text::TextEngine;

#[derive(Clone)]
struct State {
    matrix: [f32; 6],
    fill: Color,
    stroke: Color,
    width: f32,
    clip: Rect,
    text_x: f32,
    text_y: f32,
    line_x: f32,
    line_y: f32,
    font: Vec<u8>,
    size: f32,
    leading: f32,
}
impl State {
    fn point(&self, x: f32, y: f32, h: f32) -> Point {
        Point::new(
            (x * self.matrix[0] + self.matrix[4]) / 0.75,
            h - (y * self.matrix[3] + self.matrix[5]) / 0.75,
        )
    }
    fn rect(&self, x: f32, y: f32, w: f32, h: f32, page_height: f32) -> Rect {
        let a = self.point(x, y, page_height);
        let b = self.point(x + w, y + h, page_height);
        Rect::new(
            a.x.min(b.x),
            a.y.min(b.y),
            (a.x - b.x).abs(),
            (a.y - b.y).abs(),
        )
    }
}
pub(super) fn page(
    doc: &Document,
    id: ObjectId,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<(PdfPage, Vec<String>, usize), PdfError> {
    let media = inherited(doc, id, b"MediaBox")?
        .ok_or_else(|| invalid("page MediaBox missing"))?
        .as_array()
        .map_err(invalid)?;
    if media.len() != 4 {
        return Err(invalid("invalid MediaBox"));
    }
    let m = media.iter().map(number).collect::<Result<Vec<_>, _>>()?;
    let size = Size::new((m[2] - m[0]) / 0.75, (m[3] - m[1]) / 0.75);
    if !(1.0..=19200.0).contains(&size.width) || !(1.0..=19200.0).contains(&size.height) {
        return Err(invalid("page dimensions"));
    }
    if inherited(doc, id, b"Rotate")?
        .and_then(|o| o.as_i64().ok())
        .unwrap_or(0)
        % 360
        != 0
    {
        return Err(PdfError::Unsupported("rotated pages".into()));
    }
    let bounds = Rect::new(0.0, 0.0, size.width, size.height);
    let mut state = State {
        matrix: [1.0, 0.0, 0.0, 1.0, -m[0], -m[1]],
        fill: Color::rgb(0, 0, 0),
        stroke: Color::rgb(0, 0, 0),
        width: 1.0,
        clip: bounds,
        text_x: 0.0,
        text_y: 0.0,
        line_x: 0.0,
        line_y: 0.0,
        font: Vec::new(),
        size: 12.0,
        leading: 14.0,
    };
    let resources = inherited(doc, id, b"Resources")?.and_then(|o| o.as_dict().ok());
    let data = doc
        .get_page_content_with_limit(id, 4 * 1024 * 1024)
        .map_err(invalid)?;
    let mut decoded = data.len();
    let content = Content::decode(&data).map_err(invalid)?;
    if content.operations.len() > 50_000 {
        return Err(PdfError::Limit("50000 operations/page"));
    }
    let mut scene = Scene::default();
    scene.fill(bounds, Color::rgb(255, 255, 255));
    let mut warnings = BTreeSet::new();
    if let Some(crop) = inherited(doc, id, b"CropBox")?
        && crop != &Object::Array(media.clone())
    {
        warnings.insert("CropBox differs from MediaBox; the full media area is displayed".into());
    }
    if inherited(doc, id, b"UserUnit")?
        .map(number)
        .transpose()?
        .is_some_and(|unit| unit != 1.0)
    {
        return Err(PdfError::Unsupported("page UserUnit other than 1".into()));
    }
    let mut text = Vec::new();
    let mut stack = Vec::new();
    let mut path = Vec::new();
    let mut rectangle = None;
    let mut current = None;
    let mut start = None;
    let mut pending_clip = false;
    let mut actual: Vec<Option<(String, Option<Rect>)>> = Vec::new();
    let mut engine = TextEngine::new();
    for (index, op) in content.operations.iter().enumerate() {
        if index % 128 == 0 && cancelled() {
            return Err(PdfError::Cancelled);
        }
        let a = &op.operands;
        let numbers = |count: usize| -> Result<Vec<f32>, PdfError> {
            if a.len() != count {
                return Err(invalid(format!("{} operand count", op.operator)));
            }
            a.iter().map(number).collect()
        };
        match op.operator.as_str() {
            "q" => {
                if stack.len() >= 64 {
                    return Err(PdfError::Limit("graphics stack"));
                }
                stack.push(state.clone());
            }
            "Q" => {
                state = stack
                    .pop()
                    .ok_or_else(|| invalid("unbalanced graphics state"))?;
            }
            "cm" => {
                let v = numbers(6)?;
                if v[1] != 0.0 || v[2] != 0.0 {
                    return Err(PdfError::Unsupported(
                        "rotated/sheared content matrix".into(),
                    ));
                }
                state.matrix = [
                    state.matrix[0] * v[0],
                    0.0,
                    0.0,
                    state.matrix[3] * v[3],
                    state.matrix[0] * v[4] + state.matrix[4],
                    state.matrix[3] * v[5] + state.matrix[5],
                ];
                if state
                    .matrix
                    .iter()
                    .any(|n| !n.is_finite() || n.abs() > 10_000_000.0)
                {
                    return Err(invalid("matrix overflow"));
                }
            }
            "rg" | "RG" => {
                let v = numbers(3)?;
                if v.iter().any(|v| !(0.0..=1.0).contains(v)) {
                    return Err(invalid("color"));
                }
                let c = Color {
                    r: v[0],
                    g: v[1],
                    b: v[2],
                    a: 1.0,
                };
                if op.operator == "rg" {
                    state.fill = c
                } else {
                    state.stroke = c
                };
            }
            "g" | "G" => {
                let v = numbers(1)?[0];
                if !(0.0..=1.0).contains(&v) {
                    return Err(invalid("gray color"));
                }
                let c = Color {
                    r: v,
                    g: v,
                    b: v,
                    a: 1.0,
                };
                if op.operator == "g" {
                    state.fill = c
                } else {
                    state.stroke = c
                };
            }
            "w" => {
                state.width = numbers(1)?[0];
                if state.width < 0.0 {
                    return Err(invalid("stroke width"));
                }
            }
            "re" => {
                let v = numbers(4)?;
                let r = state.rect(v[0], v[1], v[2], v[3], size.height);
                if rectangle.is_some() || !path.is_empty() {
                    warnings.insert("Compound filled paths are not rendered".into());
                }
                rectangle = Some(r);
            }
            "m" => {
                let v = numbers(2)?;
                let p = state.point(v[0], v[1], size.height);
                current = Some(p);
                start = Some(p);
            }
            "l" => {
                let v = numbers(2)?;
                let p = state.point(v[0], v[1], size.height);
                let from = current.ok_or_else(|| invalid("line without move"))?;
                path.push(StrokeSegment {
                    start: from,
                    end: p,
                });
                current = Some(p);
            }
            "h" => {
                if let (Some(a), Some(b)) = (current, start) {
                    path.push(StrokeSegment { start: a, end: b });
                    current = start;
                }
            }
            "W" | "W*" => {
                pending_clip = true;
            }
            "n" | "f" | "f*" | "S" | "s" | "B" | "B*" => {
                if pending_clip {
                    let r = rectangle
                        .ok_or_else(|| PdfError::Unsupported("nonrectangular clipping".into()))?;
                    state.clip = state
                        .clip
                        .intersection(r)
                        .unwrap_or(Rect::new(0.0, 0.0, 0.0, 0.0));
                    pending_clip = false;
                }
                let fill = matches!(op.operator.as_str(), "f" | "f*" | "B" | "B*");
                let stroke = matches!(op.operator.as_str(), "S" | "s" | "B" | "B*");
                if let Some(r) = rectangle.take() {
                    if fill && let Some(r) = r.intersection(state.clip) {
                        scene.fill(r, state.fill);
                    }
                    if stroke {
                        scene.rounded_stroke(
                            r,
                            state.clip,
                            0.0,
                            state.width * state.matrix[0].abs() / 0.75,
                            state.stroke,
                        );
                    }
                } else if fill && !path.is_empty() {
                    warnings.insert("Nonrectangular filled path omitted".into());
                }
                if stroke {
                    for segments in path.chunks(rust_desktop_ui_core::MAX_STROKE_SEGMENTS) {
                        scene.stroke_path(
                            segments,
                            state.clip,
                            state.width * state.matrix[0].abs() / 0.75,
                            state.stroke,
                        );
                    }
                }
                path.clear();
                current = None;
                start = None;
            }
            "BT" => {
                state.text_x = 0.0;
                state.text_y = 0.0;
                state.line_x = 0.0;
                state.line_y = 0.0;
            }
            "ET" => {}
            "Tf" => {
                if a.len() != 2 {
                    return Err(invalid("Tf operands"));
                }
                state.font = a[0].as_name().map_err(invalid)?.to_vec();
                state.size = number(&a[1])?;
                if !(0.01..=384.0).contains(&state.size) {
                    return Err(invalid("font size"));
                }
            }
            "Tm" => {
                let v = numbers(6)?;
                if v[..4] != [1.0, 0.0, 0.0, 1.0] {
                    return Err(PdfError::Unsupported("scaled/rotated text matrix".into()));
                }
                state.text_x = v[4];
                state.text_y = v[5];
                state.line_x = v[4];
                state.line_y = v[5];
            }
            "Td" | "TD" => {
                let v = numbers(2)?;
                state.line_x += v[0];
                state.line_y += v[1];
                state.text_x = state.line_x;
                state.text_y = state.line_y;
                if op.operator == "TD" {
                    state.leading = -v[1];
                }
            }
            "TL" => state.leading = numbers(1)?[0],
            "T*" => {
                state.line_y -= state.leading;
                state.text_x = state.line_x;
                state.text_y = state.line_y;
            }
            "Tj" | "TJ" => {
                if a.len() != 1 {
                    return Err(invalid("text operand count"));
                }
                let items = if op.operator == "TJ" {
                    a[0].as_array().map_err(invalid)?.as_slice()
                } else {
                    a.as_slice()
                };
                let fonts = resources
                    .and_then(|d| d.get(b"Font").ok())
                    .map(|o| resolve(doc, o))
                    .transpose()?
                    .and_then(|o| o.as_dict().ok());
                let font = fonts
                    .ok_or_else(|| invalid("font resources missing"))?
                    .get(&state.font)
                    .map_err(invalid)?;
                let font = resolve(doc, font)?.as_dict().map_err(invalid)?;
                let encoding = font
                    .get_font_encoding_with_limit(doc, 1024 * 1024)
                    .map_err(invalid)?;
                warnings.insert("PDF fonts are shaped with system fallback; exact embedded-font metrics are not implemented".into());
                for item in items {
                    if let Object::String(bytes, _) = item {
                        let value = Document::decode_text(&encoding, bytes).map_err(invalid)?;
                        if value.len() > 1024 * 1024 {
                            return Err(PdfError::Limit("text item"));
                        }
                        let p = state.point(state.text_x, state.text_y, size.height);
                        let font_size = state.size * state.matrix[3].abs() / 0.75;
                        if !(0.01..=512.0).contains(&font_size) {
                            return Err(invalid("transformed font size"));
                        }
                        let mut run = TextRun::new(
                            value.clone(),
                            Rect::new(
                                p.x,
                                p.y - font_size,
                                (size.width - p.x).max(1.0),
                                font_size * 1.6,
                            ),
                            state.fill,
                            font_size,
                        );
                        run.clip = state.clip;
                        let prepared = engine.prepare(&run, 1.0).map_err(invalid)?;
                        let rect = Rect::new(
                            p.x,
                            p.y - font_size,
                            prepared.width.max(1.0),
                            prepared.height.max(1.0),
                        );
                        state.text_x += prepared.width * 0.75 / state.matrix[0].abs().max(0.001);
                        if actual.iter().all(Option::is_none) {
                            text.push(PdfText {
                                bounds: rect,
                                text: value,
                            });
                        }
                        scene.text(run);
                    } else {
                        state.text_x -= number(item)? / 1000.0 * state.size;
                    }
                }
            }
            "Do" => {
                if a.len() != 1 {
                    return Err(invalid("Do operands"));
                }
                let name = a[0].as_name().map_err(invalid)?;
                let objects = resources
                    .and_then(|d| d.get(b"XObject").ok())
                    .ok_or_else(|| invalid("XObject resources missing"))?;
                let object = resolve(doc, objects)?
                    .as_dict()
                    .map_err(invalid)?
                    .get(name)
                    .map_err(invalid)?;
                let stream = resolve(doc, object)?.as_stream().map_err(invalid)?;
                if stream.dict.get(b"Subtype").and_then(Object::as_name).ok()
                    != Some(b"Image".as_slice())
                {
                    warnings.insert("Form XObject omitted".into());
                    continue;
                }
                let bitmap = image(doc, stream)?;
                decoded += bitmap.pixels().len();
                if decoded > 32 * 1024 * 1024 {
                    return Err(PdfError::Limit("32 MiB decoded page"));
                }
                if state.matrix[0] < 0.0 || state.matrix[3] < 0.0 {
                    return Err(PdfError::Unsupported("mirrored image".into()));
                }
                let r = state.rect(0.0, 0.0, 1.0, 1.0, size.height);
                let mut image = ImagePrimitive::new(bitmap, r);
                image.clip = state.clip;
                scene.image(image);
                for (_, bounds) in actual.iter_mut().flatten() {
                    *bounds = Some(bounds.map_or(r, |b| {
                        Rect::new(
                            b.x.min(r.x),
                            b.y.min(r.y),
                            (b.x + b.width).max(r.x + r.width) - b.x.min(r.x),
                            (b.y + b.height).max(r.y + r.height) - b.y.min(r.y),
                        )
                    }));
                }
            }
            "BDC" => {
                if actual.len() >= 64 {
                    return Err(PdfError::Limit("marked content depth"));
                }
                let value = a
                    .get(1)
                    .and_then(|o| o.as_dict().ok())
                    .and_then(|d| d.get(b"ActualText").ok())
                    .map(lopdf::decode_text_string)
                    .transpose()
                    .map_err(invalid)?;
                actual.push(value.map(|t| (t, None)));
            }
            "BMC" => {
                if actual.len() >= 64 {
                    return Err(PdfError::Limit("marked content depth"));
                }
                actual.push(None);
            }
            "EMC" => {
                if let Some((value, rect)) = actual
                    .pop()
                    .ok_or_else(|| invalid("unbalanced marked content"))?
                {
                    text.push(PdfText {
                        text: value,
                        bounds: rect.unwrap_or(bounds),
                    });
                }
            }
            // Defaults only: these are common explicit no-op settings.
            "Tc" | "Tw" | "Ts" | "Tr" if numbers(1)?[0] == 0.0 => {}
            "Tz" if numbers(1)?[0] == 100.0 => {}
            "J" | "j" | "M" => {
                let _ = numbers(1)?;
            }
            other => {
                warnings.insert(format!("Unsupported painting operator {other}"));
            }
        }
        if scene.commands().len() > 50_000 || text.len() > 50_000 {
            return Err(PdfError::Limit("page primitive count"));
        }
    }
    if !stack.is_empty() || !actual.is_empty() {
        return Err(invalid("unbalanced page content"));
    }
    Ok((
        PdfPage {
            content: PrintPage { size, scene },
            text,
        },
        warnings.into_iter().collect(),
        decoded,
    ))
}
fn image(doc: &Document, stream: &lopdf::Stream) -> Result<Bitmap, PdfError> {
    let width = stream
        .dict
        .get(b"Width")
        .and_then(Object::as_i64)
        .map_err(invalid)?;
    let height = stream
        .dict
        .get(b"Height")
        .and_then(Object::as_i64)
        .map_err(invalid)?;
    if !(1..=4096).contains(&width)
        || !(1..=4096).contains(&height)
        || width * height > 8 * 1024 * 1024
    {
        return Err(PdfError::Limit("image dimensions"));
    }
    if stream
        .dict
        .get(b"BitsPerComponent")
        .and_then(Object::as_i64)
        .map_err(invalid)?
        != 8
    {
        return Err(PdfError::Unsupported("image bit depth".into()));
    }
    let channels = match stream
        .dict
        .get(b"ColorSpace")
        .and_then(Object::as_name)
        .map_err(invalid)?
    {
        b"DeviceRGB" => 3,
        b"DeviceGray" => 1,
        _ => return Err(PdfError::Unsupported("image color space".into())),
    };
    if stream.dict.has(b"Decode") || stream.dict.has(b"Mask") {
        return Err(PdfError::Unsupported("image decode/mask".into()));
    }
    let count = (width * height) as usize;
    let data = if stream.dict.get(b"Filter").and_then(Object::as_name).ok()
        == Some(b"DCTDecode".as_slice())
    {
        use image::ImageDecoder;
        let decoder = image::codecs::jpeg::JpegDecoder::new(std::io::Cursor::new(&stream.content))
            .map_err(invalid)?;
        if decoder.dimensions() != (width as u32, height as u32) {
            return Err(invalid(
                "JPEG dimensions disagree with PDF image dictionary",
            ));
        }
        let expected = if channels == 3 {
            image::ColorType::Rgb8
        } else {
            image::ColorType::L8
        };
        if decoder.color_type() != expected {
            return Err(PdfError::Unsupported("JPEG color conversion".into()));
        }
        let mut bytes = vec![0; count * channels];
        decoder.read_image(&mut bytes).map_err(invalid)?;
        bytes
    } else {
        stream
            .decompressed_content_with_limit(count * channels)
            .map_err(invalid)?
    };
    if data.len() != count * channels {
        return Err(invalid("image sample count"));
    }
    let alpha = if let Ok(o) = stream.dict.get(b"SMask") {
        let s = resolve(doc, o)?.as_stream().map_err(invalid)?;
        if s.dict
            .get(b"Width")
            .and_then(Object::as_i64)
            .map_err(invalid)?
            != width
            || s.dict
                .get(b"Height")
                .and_then(Object::as_i64)
                .map_err(invalid)?
                != height
            || s.dict.get(b"ColorSpace").and_then(Object::as_name).ok()
                != Some(b"DeviceGray".as_slice())
            || s.dict
                .get(b"BitsPerComponent")
                .and_then(Object::as_i64)
                .map_err(invalid)?
                != 8
            || s.dict.has(b"SMask")
        {
            return Err(invalid("soft mask profile"));
        }
        let bytes = s.decompressed_content_with_limit(count).map_err(invalid)?;
        if bytes.len() != count {
            return Err(invalid("mask length"));
        }
        Some(bytes)
    } else {
        None
    };
    let mut rgba = Vec::with_capacity(count * 4);
    for i in 0..count {
        let p = &data[i * channels..(i + 1) * channels];
        rgba.extend_from_slice(&[
            p[0],
            p[if channels == 3 { 1 } else { 0 }],
            p[if channels == 3 { 2 } else { 0 }],
            alpha.as_ref().map_or(255, |a| a[i]),
        ]);
    }
    Bitmap::new_rgba8(width as u32, height as u32, rgba).map_err(invalid)
}
