//! Bounded Form XObject expansion with lexical resource names.
use super::*;
use lopdf::{
    Dictionary,
    content::{Content, Operation},
};
use std::collections::BTreeMap;

pub(super) struct Expanded {
    pub operations: Vec<Operation>,
    pub resources: Dictionary,
    pub decoded: usize,
    aliases: BTreeMap<(Vec<u8>, usize), Vec<u8>>,
    active: Vec<usize>,
    forms: usize,
}

pub(super) fn expand(
    doc: &Document,
    bytes: &[u8],
    resources: Option<&Dictionary>,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<Expanded, PdfError> {
    let mut result = Expanded {
        operations: Vec::new(),
        resources: Dictionary::new(),
        decoded: 0,
        aliases: BTreeMap::new(),
        active: Vec::new(),
        forms: 0,
    };
    result.stream(doc, bytes, resources, cancelled)?;
    Ok(result)
}

impl Expanded {
    fn push(&mut self, op: Operation) -> Result<(), PdfError> {
        if self.operations.len() >= 50_000 {
            return Err(PdfError::Limit("50000 expanded operations/page"));
        }
        self.operations.push(op);
        Ok(())
    }
    fn alias(&mut self, category: &[u8], value: &Object) -> Result<Object, PdfError> {
        // Immutable input-object identity is used only as a local cache key.
        let key = (category.to_vec(), value as *const Object as usize);
        if let Some(name) = self.aliases.get(&key) {
            return Ok(Object::Name(name.clone()));
        }
        if self.aliases.len() >= 4096 {
            return Err(PdfError::Limit("4096 used page resources"));
        }
        let name = format!("R{}", self.aliases.len()).into_bytes();
        if !self.resources.has(category) {
            self.resources.set(category.to_vec(), Dictionary::new());
        }
        self.resources
            .get_mut(category)
            .map_err(invalid)?
            .as_dict_mut()
            .map_err(invalid)?
            .set(name.clone(), value.clone());
        self.aliases.insert(key, name.clone());
        Ok(Object::Name(name))
    }
    fn stream(
        &mut self,
        doc: &Document,
        bytes: &[u8],
        resources: Option<&Dictionary>,
        cancelled: &mut impl FnMut() -> bool,
    ) -> Result<(), PdfError> {
        if cancelled() {
            return Err(PdfError::Cancelled);
        }
        self.decoded = self
            .decoded
            .checked_add(bytes.len())
            .ok_or(PdfError::Limit("decoded page"))?;
        if self.decoded > 32 * 1024 * 1024 {
            return Err(PdfError::Limit("32 MiB decoded page"));
        }
        let content = Content::decode(bytes).map_err(invalid)?;
        if content.operations.len() > 50_000 {
            return Err(PdfError::Limit("50000 content operations"));
        }
        let mut graphics_depth = 0usize;
        let mut marked_depth = 0usize;
        let mut pending_path = false;
        for (index, mut op) in content.operations.into_iter().enumerate() {
            if index % 128 == 0 && cancelled() {
                return Err(PdfError::Cancelled);
            }
            match op.operator.as_str() {
                "q" => {
                    graphics_depth += 1;
                    if graphics_depth > 64 {
                        return Err(PdfError::Limit("graphics stack"));
                    }
                }
                "Q" => {
                    graphics_depth = graphics_depth
                        .checked_sub(1)
                        .ok_or_else(|| invalid("graphics state crosses content boundary"))?;
                }
                "BDC" | "BMC" => {
                    marked_depth += 1;
                    if marked_depth > 64 {
                        return Err(PdfError::Limit("marked content depth"));
                    }
                    if op.operator == "BDC"
                        && op.operands.len() == 2
                        && matches!(op.operands[1], Object::Name(_))
                    {
                        let properties = resolve(
                            doc,
                            resource(doc, resources, b"Properties", &op.operands[1])?,
                        )?
                        .as_dict()
                        .map_err(invalid)?;
                        let mut selected = Dictionary::new();
                        if let Ok(text) = properties.get(b"ActualText") {
                            let text = resolve(doc, text)?;
                            let bytes = text.as_str().map_err(invalid)?;
                            if bytes.len() > 1024 * 1024 {
                                return Err(PdfError::Limit("ActualText property"));
                            }
                            self.decoded += bytes.len();
                            if self.decoded > 32 * 1024 * 1024 {
                                return Err(PdfError::Limit("32 MiB decoded page"));
                            }
                            selected.set("ActualText", text.clone());
                        }
                        op.operands[1] = Object::Dictionary(selected);
                    }
                }
                "EMC" => {
                    marked_depth = marked_depth
                        .checked_sub(1)
                        .ok_or_else(|| invalid("marked content crosses content boundary"))?;
                }
                "m" | "re" => pending_path = true,
                "n" | "f" | "F" | "f*" | "S" | "s" | "B" | "B*" | "b" | "b*" => {
                    pending_path = false
                }
                "Tf" | "gs" => {
                    let count = if op.operator == "Tf" { 2 } else { 1 };
                    if op.operands.len() != count {
                        return Err(invalid("resource operator operand count"));
                    }
                    let category = if op.operator == "Tf" {
                        b"Font".as_slice()
                    } else {
                        b"ExtGState".as_slice()
                    };
                    op.operands[0] = self.alias(
                        category,
                        resource(doc, resources, category, &op.operands[0])?,
                    )?;
                }
                "Do" => {
                    if op.operands.len() != 1 {
                        return Err(invalid("Do operand count"));
                    }
                    let value = resource(doc, resources, b"XObject", &op.operands[0])?;
                    let stream = resolve(doc, value)?.as_stream().map_err(invalid)?;
                    if stream.dict.get(b"Subtype").and_then(Object::as_name).ok() == Some(b"Form") {
                        if pending_path {
                            return Err(PdfError::Unsupported(
                                "Form invocation inside an unfinished path".into(),
                            ));
                        }
                        self.form(doc, stream, resources, cancelled)?;
                        continue;
                    }
                    op.operands[0] = self.alias(b"XObject", value)?;
                }
                _ => {}
            }
            self.push(op)?;
        }
        if graphics_depth != 0 || marked_depth != 0 {
            return Err(invalid("unbalanced content stream"));
        }
        Ok(())
    }
    fn form(
        &mut self,
        doc: &Document,
        form: &lopdf::Stream,
        parent: Option<&Dictionary>,
        cancelled: &mut impl FnMut() -> bool,
    ) -> Result<(), PdfError> {
        let identity = form as *const lopdf::Stream as usize;
        if self.active.contains(&identity) {
            return Err(invalid("cyclic Form XObject"));
        }
        if self.active.len() >= 16 || self.forms >= 4096 {
            return Err(PdfError::Limit("Form depth/count"));
        }
        self.forms += 1;
        if form.dict.has(b"Group") {
            return Err(PdfError::Unsupported("Form transparency group".into()));
        }
        if let Ok(kind) = form.dict.get(b"FormType")
            && kind.as_i64().map_err(invalid)? != 1
        {
            return Err(PdfError::Unsupported("FormType".into()));
        }
        let bbox = resolve(doc, form.dict.get(b"BBox").map_err(invalid)?)?
            .as_array()
            .map_err(invalid)?;
        if bbox.len() != 4 {
            return Err(invalid("Form BBox"));
        }
        let b = bbox.iter().map(number).collect::<Result<Vec<_>, _>>()?;
        if b[2] <= b[0] || b[3] <= b[1] {
            return Err(invalid("Form BBox extent"));
        }
        let matrix = match form.dict.get(b"Matrix") {
            Ok(value) => {
                let values = resolve(doc, value)?.as_array().map_err(invalid)?;
                if values.len() != 6 {
                    return Err(invalid("Form matrix"));
                }
                values.clone()
            }
            Err(_) => vec![1.into(), 0.into(), 0.into(), 1.into(), 0.into(), 0.into()],
        };
        if matrix.len() != 6 {
            return Err(invalid("Form matrix"));
        }
        let m = matrix.iter().map(number).collect::<Result<Vec<_>, _>>()?;
        if m[1] != 0.0 || m[2] != 0.0 {
            return Err(PdfError::Unsupported("rotated/sheared Form matrix".into()));
        }
        let resources = match form.dict.get(b"Resources") {
            Ok(value) => Some(resolve(doc, value)?.as_dict().map_err(invalid)?),
            Err(_) => parent,
        };
        let bytes = form
            .decompressed_content_with_limit(4 * 1024 * 1024)
            .map_err(invalid)?;
        self.active.push(identity);
        self.push(Operation::new("q", vec![]))?;
        self.push(Operation::new("cm", matrix))?;
        self.push(Operation::new(
            "re",
            vec![
                Object::Real(b[0]),
                Object::Real(b[1]),
                Object::Real(b[2] - b[0]),
                Object::Real(b[3] - b[1]),
            ],
        ))?;
        self.push(Operation::new("W", vec![]))?;
        self.push(Operation::new("n", vec![]))?;
        self.stream(doc, &bytes, resources, cancelled)?;
        self.push(Operation::new("n", vec![]))?;
        self.push(Operation::new("Q", vec![]))?;
        self.active.pop();
        Ok(())
    }
}

fn resource<'a>(
    doc: &'a Document,
    resources: Option<&'a Dictionary>,
    category: &[u8],
    name: &Object,
) -> Result<&'a Object, PdfError> {
    let name = name.as_name().map_err(invalid)?;
    let table = resources
        .ok_or_else(|| invalid("content resources missing"))?
        .get(category)
        .map_err(invalid)?;
    resolve(doc, table)?
        .as_dict()
        .map_err(invalid)?
        .get(name)
        .map_err(invalid)
}
