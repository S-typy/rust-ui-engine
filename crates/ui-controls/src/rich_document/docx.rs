//! Explicit, bounded WordprocessingML profile. No package content is extracted
//! to disk, fetched from a network, or executed.
use super::*;
use roxmltree::{Document, Node};
use std::io::{Cursor, Read, Write};
use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const PACKAGE_LIMIT: usize = 64 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocumentDiagnostic {
    pub part: String,
    pub message: String,
}
#[derive(Clone, Debug)]
pub struct DocxImport {
    pub document: RichDocument,
    pub diagnostics: Vec<DocumentDiagnostic>,
}
#[derive(Clone, Debug)]
pub struct DocxExport {
    pub bytes: Vec<u8>,
    pub diagnostics: Vec<DocumentDiagnostic>,
}

impl RichDocument {
    /// Imports the documented paragraph/run/table/block-image profile. Unknown
    /// active document content fails explicitly. Optional metadata is diagnosed.
    pub fn from_docx(bytes: &[u8]) -> Result<DocxImport, DocumentError> {
        let parts = read_package(bytes)?;
        let types = parse_xml(part(&parts, "[Content_Types].xml")?)?;
        if !types.root_element().has_tag_name((
            "http://schemas.openxmlformats.org/package/2006/content-types",
            "Types",
        )) {
            return Err(DocumentError::Codec("content-types namespace/root".into()));
        }
        let root_rels = parse_xml(part(&parts, "_rels/.rels")?)?;
        if !root_rels.root_element().has_tag_name((
            "http://schemas.openxmlformats.org/package/2006/relationships",
            "Relationships",
        )) {
            return Err(DocumentError::Codec("relationships namespace/root".into()));
        }
        let main = root_rels
            .descendants()
            .find(|n| {
                n.has_tag_name("Relationship")
                    && n.attribute("Type") == Some(&format!("{R}/officeDocument"))
            })
            .ok_or_else(|| DocumentError::Codec("officeDocument relationship missing".into()))?;
        if main.attribute("TargetMode") == Some("External") {
            return Err(DocumentError::Unsupported("external main document".into()));
        }
        let main_name = resolve_part("", main.attribute("Target").unwrap_or(""))?;
        let main_content_type = types
            .root_element()
            .children()
            .find(|n| {
                n.tag_name().name() == "Override"
                    && n.attribute("PartName") == Some(&format!("/{main_name}"))
            })
            .and_then(|n| n.attribute("ContentType"));
        if main_content_type
            != Some(
                "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml",
            )
        {
            return Err(DocumentError::Unsupported(
                "main document content type".into(),
            ));
        }
        let main_xml = parse_xml(part(&parts, &main_name)?)?;
        let root = main_xml.root_element();
        if !root.has_tag_name((W, "document")) {
            return Err(DocumentError::Unsupported(
                "WordprocessingML namespace or root".into(),
            ));
        }
        let body = root
            .children()
            .find(|n| n.has_tag_name((W, "body")))
            .ok_or_else(|| DocumentError::Codec("document body missing".into()))?;
        let mut result = DocxImport {
            document: RichDocument {
                blocks: Vec::new(),
                ..Default::default()
            },
            diagnostics: Vec::new(),
        };
        if body
            .children()
            .filter(|n| n.has_tag_name((W, "sectPr")))
            .count()
            > 1
        {
            return Err(DocumentError::Unsupported("multiple sections".into()));
        }
        if body.descendants().any(|n| {
            matches!(
                n.tag_name().name(),
                "bookmarkStart" | "bookmarkEnd" | "proofErr" | "lang"
            )
        }) {
            result.diagnostics.push(DocumentDiagnostic {
                part: main_name.clone(),
                message: "Bookmark, proofing and language metadata are not retained".into(),
            });
        }
        let base = main_name.rsplit_once('/').map_or("", |(a, _)| a);
        let file = main_name.rsplit('/').next().unwrap_or("");
        let relationships_name = if base.is_empty() {
            format!("_rels/{file}.rels")
        } else {
            format!("{base}/_rels/{file}.rels")
        };
        let mut relations = BTreeMap::new();
        let mut style_part = None;
        if let Some(xml) = parts.get(&relationships_name) {
            let rels = parse_xml(xml)?;
            for rel in rels.root_element().children().filter(Node::is_element) {
                let id = rel
                    .attribute("Id")
                    .ok_or_else(|| DocumentError::Codec("relationship ID missing".into()))?;
                if relations.contains_key(id) {
                    return Err(DocumentError::Codec("duplicate relationship ID".into()));
                }
                let target = rel.attribute("Target").unwrap_or("");
                let kind = rel.attribute("Type").unwrap_or("");
                if rel.attribute("TargetMode") == Some("External") {
                    result.diagnostics.push(DocumentDiagnostic {
                        part: relationships_name.clone(),
                        message: format!("External relationship {id} is not resolved"),
                    });
                    continue;
                }
                let resolved = resolve_part(base, target)?;
                if kind == format!("{R}/styles") {
                    style_part = Some(resolved.clone());
                }
                relations.insert(id.to_owned(), (kind.to_owned(), resolved));
            }
        }
        let mut defaults = RunStyle::default();
        if let Some(style_part) = style_part {
            let styles_xml = parse_xml(part(&parts, &style_part)?)?;
            if let Some(properties) = styles_xml
                .descendants()
                .find(|n| n.has_tag_name((W, "rPrDefault")))
                .and_then(|n| child(n, "rPr"))
            {
                defaults = read_run_style(properties, &defaults)?;
            }
            let style_nodes: BTreeMap<_, _> = styles_xml
                .root_element()
                .children()
                .filter(|n| n.has_tag_name((W, "style")))
                .filter_map(|n| attr(n, "styleId").map(|key| (key.to_owned(), n)))
                .collect();
            if let Some((key, _)) = style_nodes.iter().find(|(_, n)| {
                attr(**n, "type") == Some("paragraph")
                    && attr(**n, "default").is_some_and(|v| matches!(v, "1" | "true" | "on"))
            }) {
                defaults = resolve_style(key, &style_nodes, &defaults, &mut Vec::new())?;
            }
            let referenced: Vec<_> = body
                .descendants()
                .filter(|n| n.has_tag_name((W, "pStyle")))
                .map(|n| value(n).to_owned())
                .collect();
            for (key, node) in &style_nodes {
                if !referenced.contains(key) {
                    continue;
                }
                if attr(*node, "type") == Some("paragraph")
                    && valid_key(key).is_ok()
                    && result.document.styles.len() < 128
                {
                    let style = resolve_style(key, &style_nodes, &defaults, &mut Vec::new())?;
                    result.document.styles.insert(key.clone(), style);
                }
            }
            result.diagnostics.push(DocumentDiagnostic{part:style_part,message:"Referenced paragraph run styles resolved; style paragraph layout, theme, latent styles and unused style metadata are not retained".into()});
        }
        for node in body.children().filter(Node::is_element) {
            match node.tag_name().name() {
                "p" if node.tag_name().namespace() == Some(W) => {
                    if node.descendants().any(|n| n.has_tag_name((W, "drawing"))) {
                        for text in node.descendants().filter(|n| n.has_tag_name((W, "t"))) {
                            if !scalar_text(text)?.is_empty() {
                                return Err(DocumentError::Unsupported(
                                    "inline image mixed with text".into(),
                                ));
                            }
                        }
                        let image = read_image(node, &relations, &parts, &mut result.document)?;
                        result.document.blocks.push(RichBlock::Image(image));
                    } else {
                        result
                            .document
                            .blocks
                            .push(RichBlock::Paragraph(read_paragraph(
                                node,
                                &defaults,
                                &result.document.styles,
                            )?));
                    }
                }
                "tbl" if node.tag_name().namespace() == Some(W) => {
                    let table = read_table(node, &defaults, &result.document.styles)?;
                    result.document.blocks.push(RichBlock::Table(table));
                    result.diagnostics.push(DocumentDiagnostic{part:main_name.clone(),message:"Table imported as fixed columns and indivisible rows; original border, shading, row-height and table-style metadata are not retained".into()});
                }
                "sectPr" if node.tag_name().namespace() == Some(W) => {
                    read_page(node, &mut result.document.page, &mut result.diagnostics)?
                }
                name => return Err(DocumentError::Unsupported(format!("body element {name}"))),
            }
        }
        if result.document.blocks.is_empty() {
            result
                .document
                .blocks
                .push(RichBlock::Paragraph(Paragraph::default()));
        }
        result.document.validate()?;
        Ok(result)
    }

    /// Writes a ZIP/OPC DOCX with explicit typography and embedded PNG/JPEG.
    /// OOXML uses integral twips/half-points/EMUs; rounding is reported.
    pub fn to_docx(&self) -> Result<DocxExport, DocumentError> {
        self.validate()?;
        // Paragraph validation already rejects CR/NUL. Image alt text permits
        // XML-valid CR/LF/TAB, which xml() preserves using character references.
        if self.plain_text().chars().any(|c| {
            (c < ' ' && !matches!(c, '\t' | '\n' | '\r')) || matches!(c, '\u{fffe}' | '\u{ffff}')
        }) {
            return Err(DocumentError::Unsupported(
                "text contains a character forbidden by XML 1.0".into(),
            ));
        }
        let mut diagnostics = Vec::new();
        if self.media.keys().any(|key| {
            !self
                .blocks
                .iter()
                .any(|b| matches!(b,RichBlock::Image(i)if &i.media==key))
        }) {
            diagnostics.push(DocumentDiagnostic {
                part: "word/media".into(),
                message: "Unreferenced image resources are not included in DOCX".into(),
            });
        }
        if self.blocks.iter().any(|b| match b {
            RichBlock::Paragraph(p) => rounded_font(p),
            RichBlock::Table(t) => t.rows.iter().flatten().any(rounded_font),
            _ => false,
        }) {
            diagnostics.push(DocumentDiagnostic {
                part: "word/document.xml".into(),
                message: "Font sizes rounded to the nearest half point".into(),
            });
        }
        let mut document = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?><w:document xmlns:w=\"{W}\" xmlns:r=\"{R}\" xmlns:wp=\"http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing\" xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:pic=\"http://schemas.openxmlformats.org/drawingml/2006/picture\"><w:body>"
        );
        let mut relationships = format!(
            "<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"styles\" Type=\"{R}/styles\" Target=\"styles.xml\"/>"
        );
        let mut media_parts = Vec::new();
        let mut image_no = 0;
        for block in &self.blocks {
            match block {
                RichBlock::Paragraph(p) => write_paragraph(&mut document, p),
                RichBlock::Table(table) => {
                    document.push_str("<w:tbl><w:tblPr><w:tblW w:w=\"0\" w:type=\"auto\"/><w:tblLayout w:type=\"fixed\"/></w:tblPr><w:tblGrid>");
                    for width in &table.columns {
                        document.push_str(&format!(
                            "<w:gridCol w:w=\"{}\"/>",
                            (width * 15.0).round() as u32
                        ));
                    }
                    document.push_str("</w:tblGrid>");
                    for row in &table.rows {
                        document.push_str("<w:tr><w:trPr><w:cantSplit/></w:trPr>");
                        for (p, width) in row.iter().zip(&table.columns) {
                            document.push_str(&format!(
                                "<w:tc><w:tcPr><w:tcW w:w=\"{}\" w:type=\"dxa\"/></w:tcPr>",
                                (width * 15.0).round() as u32
                            ));
                            write_paragraph(&mut document, p);
                            document.push_str("</w:tc>");
                        }
                        document.push_str("</w:tr>");
                    }
                    document.push_str("</w:tbl>");
                }
                RichBlock::Image(image) => {
                    image_no += 1;
                    let media = &self.media[&image.media];
                    let ext = if media.content_type == "image/png" {
                        "png"
                    } else {
                        "jpg"
                    };
                    let name = format!("media/image{image_no}.{ext}");
                    relationships.push_str(&format!("<Relationship Id=\"image{image_no}\" Type=\"{R}/image\" Target=\"{name}\"/>"));
                    media_parts.push((format!("word/{name}"), media.bytes.as_slice()));
                    let cx = (image.width * 9525.0).round() as u64;
                    let cy = (image.height * 9525.0).round() as u64;
                    document.push_str(&format!("<w:p><w:r><w:drawing><wp:inline><wp:extent cx=\"{cx}\" cy=\"{cy}\"/><wp:docPr id=\"{image_no}\" name=\"{}\" descr=\"{}\"/><a:graphic><a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/picture\"><pic:pic><pic:nvPicPr><pic:cNvPr id=\"0\" name=\"{}\"/><pic:cNvPicPr/></pic:nvPicPr><pic:blipFill><a:blip r:embed=\"image{image_no}\"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill><pic:spPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"{cx}\" cy=\"{cy}\"/></a:xfrm><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></pic:spPr></pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p>",xml(&image.media),xml(&image.alt),xml(&image.media)));
                }
            }
        }
        let margins = self.page.margins().map(|v| (v * 15.0).round() as u32);
        document.push_str(&format!("<w:sectPr><w:pgSz w:w=\"{}\" w:h=\"{}\"/><w:pgMar w:top=\"{}\" w:right=\"{}\" w:bottom=\"{}\" w:left=\"{}\"/></w:sectPr></w:body></w:document>",(self.page.width*15.0).round() as u32,(self.page.height*15.0).round() as u32,margins[0],margins[1],margins[2],margins[3]));
        relationships.push_str("</Relationships>");
        let mut styles = format!("<w:styles xmlns:w=\"{W}\">");
        for (key, style) in &self.styles {
            styles.push_str(&format!(
                "<w:style w:type=\"paragraph\" w:styleId=\"{}\"><w:name w:val=\"{}\"/>",
                xml(key),
                xml(key)
            ));
            write_run_style(&mut styles, style);
            styles.push_str("</w:style>");
        }
        styles.push_str("</w:styles>");
        let content_types = "<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/><Default Extension=\"png\" ContentType=\"image/png\"/><Default Extension=\"jpg\" ContentType=\"image/jpeg\"/><Override PartName=\"/word/document.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml\"/><Override PartName=\"/word/styles.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml\"/></Types>";
        let root_rels = format!(
            "<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"document\" Type=\"{R}/officeDocument\" Target=\"word/document.xml\"/></Relationships>"
        );
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        for (name, bytes) in [
            ("[Content_Types].xml", content_types.as_bytes()),
            ("_rels/.rels", root_rels.as_bytes()),
            ("word/document.xml", document.as_bytes()),
            ("word/styles.xml", styles.as_bytes()),
            ("word/_rels/document.xml.rels", relationships.as_bytes()),
        ] {
            write_part(&mut writer, name, bytes)?;
        }
        for (name, bytes) in media_parts {
            write_part(&mut writer, &name, bytes)?;
        }
        let bytes = writer.finish().map_err(codec)?.into_inner();
        Ok(DocxExport { bytes, diagnostics })
    }
}
fn rounded_font(p: &Paragraph) -> bool {
    p.runs
        .iter()
        .any(|r| (r.style.size * 1.5 - (r.style.size * 1.5).round()).abs() > 0.0001)
}
fn codec(e: impl fmt::Display) -> DocumentError {
    DocumentError::Codec(e.to_string())
}
fn xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
        .replace('\r', "&#13;")
        .replace('\n', "&#10;")
        .replace('\t', "&#9;")
}
fn write_part(
    writer: &mut ZipWriter<Cursor<Vec<u8>>>,
    name: &str,
    bytes: &[u8],
) -> Result<(), DocumentError> {
    writer
        .start_file(
            name,
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated),
        )
        .map_err(codec)?;
    writer.write_all(bytes).map_err(codec)
}
fn part<'a>(parts: &'a BTreeMap<String, Vec<u8>>, name: &str) -> Result<&'a [u8], DocumentError> {
    parts
        .get(name)
        .map(Vec::as_slice)
        .ok_or_else(|| DocumentError::Codec(format!("missing part {name}")))
}
fn parse_xml(bytes: &[u8]) -> Result<Document<'_>, DocumentError> {
    if bytes.len() > 16 * 1024 * 1024 {
        return Err(DocumentError::Limit("XML part"));
    }
    Document::parse(std::str::from_utf8(bytes).map_err(codec)?).map_err(codec)
}
fn read_package(bytes: &[u8]) -> Result<BTreeMap<String, Vec<u8>>, DocumentError> {
    if bytes.len() > PACKAGE_LIMIT {
        return Err(DocumentError::Limit("ZIP input"));
    }
    rust_desktop_ui_data::office_package::validate_zip_directory(
        bytes,
        512,
        MAX_DOCUMENT_MEDIA_BYTES,
        PACKAGE_LIMIT,
    )
    .map_err(codec)?;
    let mut zip = ZipArchive::new(Cursor::new(bytes)).map_err(codec)?;
    if zip.len() > 512 {
        return Err(DocumentError::Limit("512 ZIP entries"));
    }
    let mut result = BTreeMap::new();
    let mut total = 0usize;
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index).map_err(codec)?;
        let name = entry.name().to_owned();
        if name.ends_with('/') {
            continue;
        }
        if name.to_ascii_lowercase().ends_with("vbaproject.bin") {
            return Err(DocumentError::Unsupported("macro-enabled package".into()));
        }
        if name.starts_with('/')
            || name.contains(['\\', ':', '\0'])
            || name
                .split('/')
                .any(|p| p.is_empty() || p == ".." || p == ".")
            || result.contains_key(&name)
        {
            return Err(DocumentError::Codec(
                "unsafe or duplicate package path".into(),
            ));
        }
        let size = usize::try_from(entry.size()).map_err(codec)?;
        total = total.saturating_add(size);
        if size > MAX_DOCUMENT_MEDIA_BYTES || total > PACKAGE_LIMIT {
            return Err(DocumentError::Limit("ZIP expanded size"));
        }
        let mut data = Vec::with_capacity(size);
        (&mut entry)
            .take(size as u64 + 1)
            .read_to_end(&mut data)
            .map_err(codec)?;
        if data.len() != size {
            return Err(DocumentError::Codec("ZIP size mismatch".into()));
        }
        result.insert(name, data);
    }
    Ok(result)
}
fn resolve_part(base: &str, target: &str) -> Result<String, DocumentError> {
    if target.is_empty() || target.contains(['\\', ':', '\0', '?', '#']) {
        return Err(DocumentError::Codec("invalid part target".into()));
    }
    let mut parts: Vec<&str> = if target.starts_with('/') {
        Vec::new()
    } else {
        base.split('/').filter(|s| !s.is_empty()).collect()
    };
    for p in target.trim_start_matches('/').split('/') {
        match p {
            "" | "." => {}
            ".." => {
                if parts.pop().is_none() {
                    return Err(DocumentError::Codec("part target escapes package".into()));
                }
            }
            _ => parts.push(p),
        }
    }
    Ok(parts.join("/"))
}
fn attr<'a>(node: Node<'a, '_>, name: &str) -> Option<&'a str> {
    node.attribute((W, name))
}
fn child<'a, 'i>(node: Node<'a, 'i>, name: &str) -> Option<Node<'a, 'i>> {
    node.children().find(|n| n.has_tag_name((W, name)))
}
fn value<'a>(node: Node<'a, '_>) -> &'a str {
    attr(node, "val").unwrap_or("")
}
fn on(node: Node<'_, '_>) -> bool {
    !matches!(value(node), "0" | "false" | "off")
}
fn number(node: Node<'_, '_>, name: &str) -> Result<f32, DocumentError> {
    attr(node, name)
        .ok_or_else(|| DocumentError::Codec(format!("{name} missing")))?
        .parse::<f32>()
        .map_err(codec)
}
fn read_run_style(node: Node<'_, '_>, base: &RunStyle) -> Result<RunStyle, DocumentError> {
    let mut s = base.clone();
    for p in node.children().filter(Node::is_element) {
        require_word_namespace(p)?;
        match p.tag_name().name() {
            "b" => s.bold = on(p),
            "i" => s.italic = on(p),
            "u" => {
                if !matches!(value(p), "single" | "none" | "") {
                    return Err(DocumentError::Unsupported("underline style".into()));
                }
                s.underline = value(p) != "none";
            }
            "sz" => s.size = number(p, "val")? / 1.5,
            "rFonts" => {
                if let Some(family) = attr(p, "ascii").or_else(|| attr(p, "hAnsi")) {
                    s.family = family.into();
                }
                if ["hAnsi", "eastAsia", "cs"]
                    .iter()
                    .filter_map(|name| attr(p, name))
                    .any(|family| family != s.family)
                {
                    return Err(DocumentError::Unsupported(
                        "different font families per script".into(),
                    ));
                }
            }
            "color" => {
                let color = value(p);
                if color != "auto" {
                    if color.len() != 6 || !color.is_ascii() {
                        return Err(DocumentError::Codec("RGB color".into()));
                    }
                    for (i, channel) in s.color.iter_mut().enumerate() {
                        *channel =
                            u8::from_str_radix(&color[i * 2..i * 2 + 2], 16).map_err(codec)?;
                    }
                }
            }
            "lang" | "szCs" | "bCs" | "iCs" => {}
            name => return Err(DocumentError::Unsupported(format!("run property {name}"))),
        }
    }
    if child(node, "szCs")
        .map(|p| number(p, "val"))
        .transpose()?
        .is_some_and(|size| (size / 1.5 - s.size).abs() > 0.01)
        || child(node, "bCs").is_some_and(|p| on(p) != s.bold)
        || child(node, "iCs").is_some_and(|p| on(p) != s.italic)
    {
        return Err(DocumentError::Unsupported(
            "different typography per complex script".into(),
        ));
    }
    s.validate()?;
    Ok(s)
}
fn resolve_style(
    key: &str,
    nodes: &BTreeMap<String, Node<'_, '_>>,
    base: &RunStyle,
    seen: &mut Vec<String>,
) -> Result<RunStyle, DocumentError> {
    if seen.len() > 16 || seen.iter().any(|s| s == key) {
        return Err(DocumentError::Codec("cyclic/deep style inheritance".into()));
    }
    let node = nodes
        .get(key)
        .ok_or_else(|| DocumentError::Codec(format!("unknown inherited style {key}")))?;
    seen.push(key.into());
    let inherited = if let Some(parent) = child(*node, "basedOn") {
        resolve_style(value(parent), nodes, base, seen)?
    } else {
        base.clone()
    };
    let result = if let Some(properties) = child(*node, "rPr") {
        read_run_style(properties, &inherited)?
    } else {
        inherited
    };
    seen.pop();
    Ok(result)
}
fn read_paragraph(
    node: Node<'_, '_>,
    defaults: &RunStyle,
    styles: &BTreeMap<String, RunStyle>,
) -> Result<Paragraph, DocumentError> {
    let mut p = Paragraph::default();
    let mut base = defaults.clone();
    if let Some(properties) = child(node, "pPr") {
        for n in properties.children().filter(Node::is_element) {
            require_word_namespace(n)?;
            match n.tag_name().name() {
                "pStyle" => {
                    let key = value(n);
                    base = styles.get(key).cloned().ok_or_else(|| {
                        DocumentError::Unsupported(format!("paragraph style {key}"))
                    })?;
                    p.style = Some(key.into());
                }
                "jc" => {
                    p.alignment = match value(n) {
                        "left" | "start" => ParagraphAlignment::Start,
                        "center" => ParagraphAlignment::Center,
                        "right" | "end" => ParagraphAlignment::End,
                        _ => return Err(DocumentError::Unsupported("paragraph alignment".into())),
                    }
                }
                "spacing" => {
                    if attr(n, "before").is_some_and(|v| v != "0") || attr(n, "line").is_some() {
                        return Err(DocumentError::Unsupported(
                            "paragraph before/line spacing".into(),
                        ));
                    }
                    if attr(n, "after").is_some() {
                        p.space_after = number(n, "after")? / 15.0;
                    }
                }
                "pageBreakBefore" => p.page_break_before = on(n),
                "widowControl" | "keepNext" | "keepLines" => {
                    return Err(DocumentError::Unsupported(
                        "paragraph keep/widow controls".into(),
                    ));
                }
                name => {
                    return Err(DocumentError::Unsupported(format!(
                        "paragraph property {name}"
                    )));
                }
            }
        }
    }
    for run in node.children().filter(Node::is_element) {
        require_word_namespace(run)?;
        match run.tag_name().name() {
            "pPr" => {}
            "r" => {
                let style = child(run, "rPr")
                    .map(|n| read_run_style(n, &base))
                    .transpose()?
                    .unwrap_or_else(|| base.clone());
                let mut text = String::new();
                for n in run.children().filter(Node::is_element) {
                    require_word_namespace(n)?;
                    match n.tag_name().name() {
                        "rPr" => {}
                        "t" => text.push_str(&scalar_text(n)?),
                        "tab" => text.push('\t'),
                        "br" | "cr" => {
                            if attr(n, "type").is_some_and(|v| v != "textWrapping") {
                                return Err(DocumentError::Unsupported(
                                    "run page/column break".into(),
                                ));
                            }
                            text.push('\n');
                        }
                        name => {
                            return Err(DocumentError::Unsupported(format!("run element {name}")));
                        }
                    }
                }
                if !text.is_empty() {
                    p.runs.push(RichRun { text, style });
                }
            }
            "bookmarkStart" | "bookmarkEnd" | "proofErr" => {}
            name => {
                return Err(DocumentError::Unsupported(format!(
                    "paragraph element {name}"
                )));
            }
        }
    }
    p.runs = merge_runs(p.runs);
    p.validate()?;
    Ok(p)
}
fn scalar_text(node: Node<'_, '_>) -> Result<String, DocumentError> {
    let mut text = String::new();
    for child in node.children() {
        if child.is_element() {
            return Err(DocumentError::Unsupported(format!(
                "{} must contain scalar text, not child elements",
                node.tag_name().name()
            )));
        }
        if child.is_text() {
            text.push_str(child.text().unwrap_or(""));
        }
    }
    Ok(text)
}
fn read_table(
    node: Node<'_, '_>,
    defaults: &RunStyle,
    styles: &BTreeMap<String, RunStyle>,
) -> Result<RichTable, DocumentError> {
    allowed_word_children(node, &["tblPr", "tblGrid", "tr"])?;
    let grid = child(node, "tblGrid")
        .ok_or_else(|| DocumentError::Unsupported("table without explicit grid".into()))?;
    let columns = grid
        .children()
        .filter(|n| n.has_tag_name((W, "gridCol")))
        .map(|n| number(n, "w").map(|v| v / 15.0))
        .collect::<Result<Vec<_>, _>>()?;
    allowed_word_children(grid, &["gridCol"])?;
    if node
        .descendants()
        .any(|n| n.has_tag_name((W, "gridSpan")) || n.has_tag_name((W, "vMerge")))
    {
        return Err(DocumentError::Unsupported("merged table cells".into()));
    }
    let mut rows = Vec::new();
    for row in node.children().filter(|n| n.has_tag_name((W, "tr"))) {
        allowed_word_children(row, &["trPr", "tc"])?;
        let mut cells = Vec::new();
        for cell in row.children().filter(|n| n.has_tag_name((W, "tc"))) {
            allowed_word_children(cell, &["tcPr", "p"])?;
            let paragraphs: Vec<_> = cell
                .children()
                .filter(|n| n.has_tag_name((W, "p")))
                .collect();
            if paragraphs.len() != 1 || child(cell, "tbl").is_some() {
                return Err(DocumentError::Unsupported(
                    "multi-paragraph/nested table cell".into(),
                ));
            }
            cells.push(read_paragraph(paragraphs[0], defaults, styles)?);
        }
        rows.push(cells);
    }
    Ok(RichTable { columns, rows })
}
fn read_page(
    node: Node<'_, '_>,
    page: &mut PageSettings,
    diagnostics: &mut Vec<DocumentDiagnostic>,
) -> Result<(), DocumentError> {
    for n in node.children().filter(Node::is_element) {
        require_word_namespace(n)?;
        match n.tag_name().name() {
            "pgSz" => {
                page.width = number(n, "w")? / 15.0;
                page.height = number(n, "h")? / 15.0;
            }
            "pgMar" => {
                let margins = [
                    number(n, "top")?,
                    number(n, "right")?,
                    number(n, "bottom")?,
                    number(n, "left")?,
                ];
                page.margin = margins[0] / 15.0;
                page.margins = if margins.iter().any(|v| (*v - margins[0]).abs() > 0.01) {
                    Some(margins.map(|v| v / 15.0))
                } else {
                    None
                };
            }
            "cols" if attr(n, "num").is_none_or(|v| v == "1") => {}
            "docGrid" => diagnostics.push(DocumentDiagnostic {
                part: "word/document.xml".into(),
                message: "Document grid metadata is not retained".into(),
            }),
            name => {
                return Err(DocumentError::Unsupported(format!(
                    "section property {name}"
                )));
            }
        }
    }
    page.validate()
}
fn read_image(
    node: Node<'_, '_>,
    relations: &BTreeMap<String, (String, String)>,
    parts: &BTreeMap<String, Vec<u8>>,
    document: &mut RichDocument,
) -> Result<RichImage, DocumentError> {
    for n in node.descendants().filter(Node::is_element) {
        let allowed = match n.tag_name().namespace() {
            Some(W) => matches!(n.tag_name().name(), "p" | "r" | "drawing"),
            Some("http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing") => {
                matches!(
                    n.tag_name().name(),
                    "inline" | "extent" | "docPr" | "cNvGraphicFramePr" | "effectExtent"
                )
            }
            Some("http://schemas.openxmlformats.org/drawingml/2006/main") => matches!(
                n.tag_name().name(),
                "graphic"
                    | "graphicData"
                    | "blip"
                    | "stretch"
                    | "fillRect"
                    | "xfrm"
                    | "off"
                    | "ext"
                    | "prstGeom"
                    | "avLst"
                    | "graphicFrameLocks"
            ),
            Some("http://schemas.openxmlformats.org/drawingml/2006/picture") => matches!(
                n.tag_name().name(),
                "pic" | "nvPicPr" | "cNvPr" | "cNvPicPr" | "blipFill" | "spPr"
            ),
            _ => false,
        };
        if !allowed {
            return Err(DocumentError::Unsupported(format!(
                "image markup {}",
                n.tag_name().name()
            )));
        }
        if n.tag_name().name() == "prstGeom" && n.attribute("prst") != Some("rect") {
            return Err(DocumentError::Unsupported(
                "nonrectangular image geometry".into(),
            ));
        }
        if n.tag_name().name() == "off"
            && ["x", "y"]
                .iter()
                .any(|key| n.attribute(*key).is_some_and(|v| v != "0"))
        {
            return Err(DocumentError::Unsupported("image transform offset".into()));
        }
        if n.tag_name().namespace() == Some(W)
            && !matches!(n.tag_name().name(), "p" | "r" | "drawing")
        {
            return Err(DocumentError::Unsupported(format!(
                "image paragraph property {}",
                n.tag_name().name()
            )));
        }
        if n.tag_name().name() == "srcRect"
            || (n.tag_name().name() == "xfrm"
                && ["rot", "flipH", "flipV"]
                    .iter()
                    .any(|a| n.attribute(*a).is_some_and(|v| !matches!(v, "0" | "false"))))
        {
            return Err(DocumentError::Unsupported(
                "image crop, rotation or reflection".into(),
            ));
        }
        if matches!(
            n.tag_name().name(),
            "effectLst" | "effectDag" | "duotone" | "grayscl" | "alphaMod"
        ) {
            return Err(DocumentError::Unsupported(
                "image color/effect transforms".into(),
            ));
        }
    }
    let inline: Vec<_> = node
        .descendants()
        .filter(|n| n.tag_name().name() == "inline")
        .collect();
    if inline.len() != 1 || node.descendants().any(|n| n.tag_name().name() == "anchor") {
        return Err(DocumentError::Unsupported(
            "floating/multiple images".into(),
        ));
    }
    let extent = inline[0]
        .children()
        .find(|n| n.tag_name().name() == "extent")
        .ok_or_else(|| DocumentError::Codec("image extent missing".into()))?;
    let width = extent
        .attribute("cx")
        .unwrap_or("")
        .parse::<f32>()
        .map_err(codec)?
        / 9525.0;
    let height = extent
        .attribute("cy")
        .unwrap_or("")
        .parse::<f32>()
        .map_err(codec)?
        / 9525.0;
    let blip = inline[0]
        .descendants()
        .find(|n| n.tag_name().name() == "blip")
        .ok_or_else(|| DocumentError::Codec("image blip missing".into()))?;
    let id = blip
        .attribute((R, "embed"))
        .ok_or_else(|| DocumentError::Unsupported("linked image".into()))?;
    let (kind, path) = relations
        .get(id)
        .ok_or_else(|| DocumentError::Codec("image relationship missing".into()))?;
    if kind != &format!("{R}/image") {
        return Err(DocumentError::Codec("image relationship type".into()));
    }
    let bytes = part(parts, path)?.to_vec();
    let format = rust_desktop_ui_images::detect_format(&bytes).map_err(codec)?;
    let content_type = match format {
        rust_desktop_ui_images::ImageFormat::Png => "image/png",
        rust_desktop_ui_images::ImageFormat::Jpeg => "image/jpeg",
        _ => return Err(DocumentError::Unsupported("DOCX image format".into())),
    };
    let properties = inline[0]
        .children()
        .find(|n| n.tag_name().name() == "docPr");
    let alt = properties
        .and_then(|n| n.attribute("descr"))
        .unwrap_or("")
        .to_owned();
    let suggested = properties.and_then(|n| n.attribute("name")).unwrap_or("");
    let key = if valid_key(suggested).is_ok()
        && document
            .media
            .get(suggested)
            .is_none_or(|m| m.bytes == bytes && m.content_type == content_type)
    {
        suggested.to_owned()
    } else {
        format!("image{}", document.media.len() + 1)
    };
    document.media.insert(
        key.clone(),
        DocumentMedia {
            content_type: content_type.into(),
            bytes,
        },
    );
    Ok(RichImage {
        media: key,
        width,
        height,
        alt,
    })
}
fn require_word_namespace(node: Node<'_, '_>) -> Result<(), DocumentError> {
    if node.tag_name().namespace() != Some(W) {
        return Err(DocumentError::Unsupported(
            "foreign markup namespace".into(),
        ));
    }
    Ok(())
}
fn allowed_word_children(node: Node<'_, '_>, names: &[&str]) -> Result<(), DocumentError> {
    for child in node.children().filter(Node::is_element) {
        require_word_namespace(child)?;
        if !names.contains(&child.tag_name().name()) {
            return Err(DocumentError::Unsupported(format!(
                "{} child {}",
                node.tag_name().name(),
                child.tag_name().name()
            )));
        }
    }
    Ok(())
}
fn write_run_style(out: &mut String, s: &RunStyle) {
    out.push_str(&format!("<w:rPr><w:rFonts w:ascii=\"{}\" w:hAnsi=\"{}\"/><w:sz w:val=\"{}\"/><w:b w:val=\"{}\"/><w:i w:val=\"{}\"/><w:u w:val=\"{}\"/><w:color w:val=\"{:02X}{:02X}{:02X}\"/></w:rPr>",xml(&s.family),xml(&s.family),(s.size*1.5).round()as u32,u8::from(s.bold),u8::from(s.italic),if s.underline{"single"}else{"none"},s.color[0],s.color[1],s.color[2]));
}
fn write_paragraph(out: &mut String, p: &Paragraph) {
    out.push_str("<w:p><w:pPr>");
    if let Some(style) = &p.style {
        out.push_str(&format!("<w:pStyle w:val=\"{}\"/>", xml(style)));
    }
    out.push_str(&format!(
        "<w:jc w:val=\"{}\"/><w:spacing w:after=\"{}\"/>",
        match p.alignment {
            ParagraphAlignment::Start => "left",
            ParagraphAlignment::Center => "center",
            ParagraphAlignment::End => "right",
        },
        (p.space_after * 15.0).round() as u32
    ));
    if p.page_break_before {
        out.push_str("<w:pageBreakBefore/>");
    }
    out.push_str("</w:pPr>");
    for run in &p.runs {
        out.push_str("<w:r>");
        write_run_style(out, &run.style);
        let mut text = String::new();
        for c in run.text.chars() {
            match c {
                '\n' | '\t' => {
                    if !text.is_empty() {
                        out.push_str(&format!("<w:t xml:space=\"preserve\">{}</w:t>", xml(&text)));
                        text.clear();
                    }
                    out.push_str(if c == '\n' { "<w:br/>" } else { "<w:tab/>" });
                }
                _ => text.push(c),
            }
        }
        if !text.is_empty() {
            out.push_str(&format!("<w:t xml:space=\"preserve\">{}</w:t>", xml(&text)));
        }
        out.push_str("</w:r>");
    }
    out.push_str("</w:p>");
}
