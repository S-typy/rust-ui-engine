//! Bounded PDF object loading and an explicit axis-aligned page interpreter.
//! Original objects are preserved on save. Unsupported painting operations are
//! diagnosed; JavaScript, launch actions and external resources are never run.
mod render;
use lopdf::{Document, Object, ObjectId, dictionary};
use rust_desktop_ui_core::{
    Rect,
    pages::{PrintDocument, PrintPage},
};
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PdfError {
    Invalid(String),
    Limit(&'static str),
    Unsupported(String),
    Cancelled,
}
impl std::fmt::Display for PdfError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid(s) => write!(f, "Invalid PDF: {s}"),
            Self::Limit(s) => write!(f, "PDF limit: {s}"),
            Self::Unsupported(s) => write!(f, "Unsupported PDF: {s}"),
            Self::Cancelled => f.write_str("PDF operation cancelled"),
        }
    }
}
impl std::error::Error for PdfError {}
pub(crate) fn invalid(e: impl ToString) -> PdfError {
    PdfError::Invalid(e.to_string())
}
#[derive(Clone, Debug)]
pub struct PdfDiagnostic {
    pub page: Option<usize>,
    pub message: String,
}
#[derive(Clone, Debug)]
pub struct PdfText {
    pub bounds: Rect,
    pub text: String,
}
#[derive(Clone, Debug)]
pub struct PdfPage {
    pub content: PrintPage,
    pub text: Vec<PdfText>,
}
#[derive(Clone, Debug)]
pub struct PdfField {
    pub name: String,
    pub value: String,
    pub page: usize,
    pub bounds: Rect,
    id: ObjectId,
}
#[derive(Clone, Debug)]
pub struct PdfAnnotation {
    pub page: usize,
    pub bounds: Rect,
    pub text: String,
    id: ObjectId,
}
#[derive(Clone, Debug)]
pub struct PdfMatch {
    pub page: usize,
    pub item: usize,
    pub byte_range: std::ops::Range<usize>,
    pub bounds: Rect,
}
#[derive(Clone, Debug)]
pub struct PdfBookmark {
    pub title: String,
    pub page: usize,
}
#[derive(Clone, Debug)]
pub struct PdfDocument {
    raw: Document,
    pages: Vec<PdfPage>,
    page_ids: Vec<ObjectId>,
    diagnostics: Vec<PdfDiagnostic>,
    fields: Vec<PdfField>,
    annotations: Vec<PdfAnnotation>,
    bookmarks: Vec<PdfBookmark>,
    revision: u64,
}
impl PdfDocument {
    pub fn open(bytes: &[u8]) -> Result<Self, PdfError> {
        Self::open_with_cancel(bytes, || false)
    }
    pub fn open_with_cancel(
        bytes: &[u8],
        mut cancelled: impl FnMut() -> bool,
    ) -> Result<Self, PdfError> {
        if bytes.len() > 16 * 1024 * 1024 {
            return Err(PdfError::Limit("16 MiB input"));
        }
        if cancelled() {
            return Err(PdfError::Cancelled);
        }
        let raw = Document::load_mem_with_options(
            bytes,
            lopdf::LoadOptions {
                strict: true,
                max_decompressed_size: Some(4 * 1024 * 1024),
                ..Default::default()
            },
        )
        .map_err(invalid)?;
        if raw.is_encrypted() {
            return Err(PdfError::Unsupported("encrypted document".into()));
        }
        if raw.objects.len() > 20_000 {
            return Err(PdfError::Limit("20000 objects"));
        }
        let page_ids = page_ids(&raw)?;
        let mut result = Self {
            raw,
            pages: Vec::new(),
            page_ids,
            diagnostics: Vec::new(),
            fields: Vec::new(),
            annotations: Vec::new(),
            bookmarks: Vec::new(),
            revision: 0,
        };
        let mut decoded = 0usize;
        for (index, id) in result.page_ids.clone().into_iter().enumerate() {
            if cancelled() {
                return Err(PdfError::Cancelled);
            }
            let (page, warnings, bytes) = render::page(&result.raw, id, &mut cancelled)?;
            decoded = decoded
                .checked_add(bytes)
                .ok_or(PdfError::Limit("decoded bytes"))?;
            if decoded > 128 * 1024 * 1024 {
                return Err(PdfError::Limit("128 MiB decoded content"));
            }
            result
                .diagnostics
                .extend(warnings.into_iter().map(|message| PdfDiagnostic {
                    page: Some(index),
                    message,
                }));
            result.pages.push(page);
            result.read_annotations(index, id)?;
        }
        result.read_bookmarks()?;
        result.print_document()?;
        Ok(result)
    }
    pub fn pages(&self) -> &[PdfPage] {
        &self.pages
    }
    pub fn diagnostics(&self) -> &[PdfDiagnostic] {
        &self.diagnostics
    }
    pub fn fields(&self) -> &[PdfField] {
        &self.fields
    }
    pub fn annotations(&self) -> &[PdfAnnotation] {
        &self.annotations
    }
    pub fn bookmarks(&self) -> &[PdfBookmark] {
        &self.bookmarks
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn print_document(&self) -> Result<PrintDocument, PdfError> {
        let mut pages = self
            .pages
            .iter()
            .map(|p| p.content.clone())
            .collect::<Vec<_>>();
        for field in &self.fields {
            let scene = &mut pages[field.page].scene;
            scene.fill(
                field.bounds,
                rust_desktop_ui_core::Color::rgb(255, 255, 255),
            );
            scene.text(rust_desktop_ui_core::TextRun::new(
                &field.value,
                field.bounds,
                rust_desktop_ui_core::Color::rgb(0, 0, 0),
                14.0,
            ));
        }
        PrintDocument::new("PDF", pages).map_err(invalid)
    }
    pub fn search(&self, needle: &str) -> Result<Vec<PdfMatch>, PdfError> {
        if needle.is_empty() || needle.len() > 4096 {
            return Err(invalid("search length must be 1..4096 bytes"));
        }
        let mut hits = Vec::new();
        for (page, p) in self.pages.iter().enumerate() {
            for (item, text) in p.text.iter().enumerate() {
                for (start, _) in text.text.match_indices(needle) {
                    if hits.len() == 10_000 {
                        return Err(PdfError::Limit("10000 search results"));
                    }
                    hits.push(PdfMatch {
                        page,
                        item,
                        byte_range: start..start + needle.len(),
                        bounds: text.bounds,
                    });
                }
            }
        }
        Ok(hits)
    }
    /// Edit a flat, writable text widget. Save regenerates this widget's appearance.
    pub fn set_field(&mut self, index: usize, value: &str) -> Result<(), PdfError> {
        if value.len() > 4096 || value.chars().any(|c| c == '\0') {
            return Err(PdfError::Limit("4096-byte field"));
        }
        let field = self
            .fields
            .get(index)
            .ok_or_else(|| invalid("unknown field"))?;
        let mut raw = self.raw.clone();
        let dict = raw
            .get_object_mut(field.id)
            .and_then(Object::as_dict_mut)
            .map_err(invalid)?;
        dict.set("V", lopdf::text_string(value));
        // An explicit appearance is essential: viewers may ignore NeedAppearances.
        let bounds = field.bounds;
        let id = field.id;
        let appearance = crate::printing::field_appearance(&mut raw, value, bounds)?;
        let dict = raw
            .get_object_mut(id)
            .and_then(Object::as_dict_mut)
            .map_err(invalid)?;
        dict.set("AP", dictionary! {"N"=>appearance});
        let revision = self
            .revision
            .checked_add(1)
            .ok_or(PdfError::Limit("revision"))?;
        self.raw = raw;
        self.fields[index].value = value.into();
        self.revision = revision;
        Ok(())
    }
    pub fn add_annotation(
        &mut self,
        page: usize,
        bounds: Rect,
        text: &str,
    ) -> Result<(), PdfError> {
        let p = self
            .pages
            .get(page)
            .ok_or_else(|| invalid("unknown page"))?;
        if !bounds.is_valid()
            || bounds.width <= 0.0
            || bounds.height <= 0.0
            || bounds.x < 0.0
            || bounds.y < 0.0
            || bounds.x + bounds.width > p.content.size.width
            || bounds.y + bounds.height > p.content.size.height
            || text.len() > 4096
            || text.contains('\0')
            || self.annotations.len() >= 1024
        {
            return Err(invalid("annotation bounds, text or capacity"));
        }
        let revision = self
            .revision
            .checked_add(1)
            .ok_or(PdfError::Limit("revision"))?;
        let h = p.content.size.height;
        let (ox, oy) = self.page_origin(page)?;
        let mut raw = self.raw.clone();
        let id=raw.add_object(dictionary!{"Type"=>"Annot","Subtype"=>"Text",
            "Rect"=>vec![real(bounds.x+ox),real(h-bounds.y-bounds.height+oy),real(bounds.x+bounds.width+ox),real(h-bounds.y+oy)],
            "Contents"=>lopdf::text_string(text),"F"=>4});
        let page_id = self.page_ids[page];
        let mut annotations = raw
            .get_dictionary(page_id)
            .map_err(invalid)?
            .get(b"Annots")
            .ok()
            .map(|o| resolve(&raw, o))
            .transpose()?
            .and_then(|o| o.as_array().ok())
            .cloned()
            .unwrap_or_default();
        annotations.push(id.into());
        raw.get_object_mut(page_id)
            .and_then(Object::as_dict_mut)
            .map_err(invalid)?
            .set("Annots", annotations);
        self.raw = raw;
        self.revision = revision;
        self.annotations.push(PdfAnnotation {
            page,
            bounds,
            text: text.into(),
            id,
        });
        Ok(())
    }
    pub fn remove_annotation(&mut self, index: usize) -> Result<(), PdfError> {
        let a = self
            .annotations
            .get(index)
            .ok_or_else(|| invalid("unknown annotation"))?;
        let revision = self
            .revision
            .checked_add(1)
            .ok_or(PdfError::Limit("revision"))?;
        let mut raw = self.raw.clone();
        let page_id = self.page_ids[a.page];
        let mut annotations = resolve(
            &raw,
            raw.get_dictionary(page_id)
                .map_err(invalid)?
                .get(b"Annots")
                .map_err(invalid)?,
        )?
        .as_array()
        .map_err(invalid)?
        .clone();
        annotations.retain(|o| o.as_reference().ok() != Some(a.id));
        raw.get_object_mut(page_id)
            .and_then(Object::as_dict_mut)
            .map_err(invalid)?
            .set("Annots", annotations);
        raw.objects.remove(&a.id);
        self.raw = raw;
        self.annotations.remove(index);
        self.revision = revision;
        Ok(())
    }
    pub fn save(&self) -> Result<Vec<u8>, PdfError> {
        let mut bytes = Vec::new();
        self.raw.clone().save_to(&mut bytes).map_err(invalid)?;
        if bytes.len() > 64 * 1024 * 1024 {
            return Err(PdfError::Limit("64 MiB output"));
        }
        Ok(bytes)
    }
    fn read_annotations(&mut self, page: usize, id: ObjectId) -> Result<(), PdfError> {
        let (ox, oy) = self.page_origin(page)?;
        let d = self.raw.get_dictionary(id).map_err(invalid)?;
        let Some(array) = d.get(b"Annots").ok() else {
            return Ok(());
        };
        let array = resolve(&self.raw, array)?.as_array().map_err(invalid)?;
        if array.len() > 1024 {
            return Err(PdfError::Limit("1024 annotations per page"));
        }
        for object in array {
            let id = object.as_reference().map_err(invalid)?;
            let d = resolve(&self.raw, object)?.as_dict().map_err(invalid)?;
            let r = resolve(&self.raw, d.get(b"Rect").map_err(invalid)?)?
                .as_array()
                .map_err(invalid)?;
            if r.len() != 4 {
                return Err(invalid("annotation rectangle"));
            }
            let v = r.iter().map(number).collect::<Result<Vec<_>, _>>()?;
            let h = self.pages[page].content.size.height;
            let bounds = Rect::new(
                v[0] / 0.75 - ox,
                h - v[3] / 0.75 + oy,
                (v[2] - v[0]) / 0.75,
                (v[3] - v[1]) / 0.75,
            );
            if !bounds.is_valid() {
                return Err(invalid("annotation rectangle"));
            }
            let subtype = d
                .get(b"Subtype")
                .and_then(Object::as_name)
                .unwrap_or_default();
            if subtype == b"Widget"
                && d.get(b"FT").and_then(Object::as_name).ok() == Some(b"Tx".as_slice())
                && d.get(b"Ff").and_then(Object::as_i64).unwrap_or(0) & 1 == 0
                && !d.has(b"Parent")
            {
                let name = d
                    .get(b"T")
                    .ok()
                    .map(lopdf::decode_text_string)
                    .transpose()
                    .map_err(invalid)?
                    .unwrap_or_default();
                let value = d
                    .get(b"V")
                    .ok()
                    .map(lopdf::decode_text_string)
                    .transpose()
                    .map_err(invalid)?
                    .unwrap_or_default();
                if name.len() > 4096 || value.len() > 4096 || value.contains('\0') {
                    return Err(PdfError::Limit("4096-byte field name/value"));
                }
                self.fields.push(PdfField {
                    name,
                    value,
                    page,
                    bounds,
                    id,
                });
            } else if subtype == b"Text" {
                let text = d
                    .get(b"Contents")
                    .ok()
                    .map(lopdf::decode_text_string)
                    .transpose()
                    .map_err(invalid)?
                    .unwrap_or_default();
                self.annotations.push(PdfAnnotation {
                    page,
                    bounds,
                    text,
                    id,
                });
            } else {
                self.diagnostics.push(PdfDiagnostic {
                    page: Some(page),
                    message: "Unsupported annotation or field preserved without interaction".into(),
                });
            }
        }
        Ok(())
    }
    fn page_origin(&self, page: usize) -> Result<(f32, f32), PdfError> {
        let media = inherited(&self.raw, self.page_ids[page], b"MediaBox")?
            .ok_or_else(|| invalid("MediaBox missing"))?
            .as_array()
            .map_err(invalid)?;
        Ok((number(&media[0])? / 0.75, number(&media[1])? / 0.75))
    }
    fn read_bookmarks(&mut self) -> Result<(), PdfError> {
        let root = self.raw.catalog().map_err(invalid)?;
        let Some(outline) = root.get(b"Outlines").ok() else {
            return Ok(());
        };
        let outline = resolve(&self.raw, outline)?.as_dict().map_err(invalid)?;
        let mut pending = outline
            .get(b"First")
            .ok()
            .cloned()
            .into_iter()
            .collect::<Vec<_>>();
        let mut seen = BTreeSet::new();
        while let Some(object) = pending.pop() {
            let id = object.as_reference().map_err(invalid)?;
            if !seen.insert(id) || seen.len() > 1024 {
                return Err(invalid("cyclic or excessive outlines"));
            }
            let d = resolve(&self.raw, &object)?.as_dict().map_err(invalid)?;
            for k in [b"Next".as_slice(), b"First".as_slice()] {
                if let Ok(o) = d.get(k) {
                    pending.push(o.clone());
                }
            }
            let destination = d
                .get(b"Dest")
                .ok()
                .or_else(|| {
                    // A local GoTo is just a page destination. Other actions,
                    // including remote files, launch and scripts, stay inert.
                    let action = resolve(&self.raw, d.get(b"A").ok()?).ok()?.as_dict().ok()?;
                    if action.get(b"S").and_then(Object::as_name).ok() == Some(b"GoTo") {
                        action.get(b"D").ok()
                    } else {
                        None
                    }
                })
                .and_then(|o| resolve(&self.raw, o).ok())
                .and_then(|o| o.as_array().ok())
                .and_then(|a| a.first())
                .and_then(|o| o.as_reference().ok());
            if let Some(page) =
                destination.and_then(|id| self.page_ids.iter().position(|p| *p == id))
            {
                let title = lopdf::decode_text_string(d.get(b"Title").map_err(invalid)?)
                    .map_err(invalid)?;
                self.bookmarks.push(PdfBookmark { title, page });
            } else {
                self.diagnostics.push(PdfDiagnostic {
                    page: None,
                    message: "Named or unsupported action bookmark destination is not interactive"
                        .into(),
                });
            }
        }
        Ok(())
    }
}
fn real(v: f32) -> Object {
    Object::Real(v * 0.75)
}
fn number(o: &Object) -> Result<f32, PdfError> {
    let n = match o {
        Object::Integer(v) => *v as f32,
        Object::Real(v) => *v,
        _ => return Err(invalid("expected numeric operand")),
    };
    if !n.is_finite() || n.abs() > 10_000_000.0 {
        return Err(invalid("nonfinite or excessive coordinate"));
    }
    Ok(n)
}
fn resolve<'a>(doc: &'a Document, mut o: &'a Object) -> Result<&'a Object, PdfError> {
    let mut seen = BTreeSet::new();
    while let Object::Reference(id) = o {
        if seen.len() >= 64 || !seen.insert(*id) {
            return Err(invalid("cyclic object reference"));
        }
        o = doc.get_object(*id).map_err(invalid)?;
    }
    Ok(o)
}
fn inherited<'a>(
    doc: &'a Document,
    id: ObjectId,
    key: &[u8],
) -> Result<Option<&'a Object>, PdfError> {
    let mut current = id;
    let mut seen = BTreeSet::new();
    loop {
        if seen.len() >= 64 || !seen.insert(current) {
            return Err(invalid("cyclic page inheritance"));
        }
        let d = doc.get_dictionary(current).map_err(invalid)?;
        if let Ok(o) = d.get(key) {
            return Ok(Some(resolve(doc, o)?));
        }
        match d.get(b"Parent").and_then(Object::as_reference) {
            Ok(id) => current = id,
            Err(_) => return Ok(None),
        }
    }
}
fn page_ids(doc: &Document) -> Result<Vec<ObjectId>, PdfError> {
    let root = doc
        .catalog()
        .map_err(invalid)?
        .get(b"Pages")
        .and_then(Object::as_reference)
        .map_err(invalid)?;
    let mut pending = vec![(root, 0)];
    let mut seen = BTreeSet::new();
    let mut pages = Vec::new();
    while let Some((id, depth)) = pending.pop() {
        if depth > 64 || seen.len() > 4096 || !seen.insert(id) {
            return Err(invalid("cyclic, shared or excessive page tree"));
        }
        let d = doc.get_dictionary(id).map_err(invalid)?;
        match d.get(b"Type").and_then(Object::as_name).map_err(invalid)? {
            b"Page" => {
                pages.push(id);
                if pages.len() > 512 {
                    return Err(PdfError::Limit("512 pages"));
                }
            }
            b"Pages" => {
                let kids = resolve(doc, d.get(b"Kids").map_err(invalid)?)?
                    .as_array()
                    .map_err(invalid)?;
                if kids.len() > 4096 {
                    return Err(PdfError::Limit("page tree width"));
                }
                for kid in kids.iter().rev() {
                    pending.push((kid.as_reference().map_err(invalid)?, depth + 1));
                }
            }
            _ => return Err(invalid("page tree node type")),
        }
    }
    if pages.is_empty() {
        return Err(invalid("empty page tree"));
    }
    Ok(pages)
}
