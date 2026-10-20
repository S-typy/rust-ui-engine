use super::*;
use roxmltree::{Document, Node};
use std::io::{Cursor, Read, Write};

const NS: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const PKG: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const MAX_PACKAGE: usize = 64 * 1024 * 1024;
const MAX_PART: usize = 16 * 1024 * 1024;
#[cfg(test)]
mod validation_tests;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkbookDiagnostic {
    pub part: String,
    pub message: String,
}
#[derive(Clone, Debug)]
pub struct WorkbookImport {
    pub workbook: Workbook,
    pub diagnostics: Vec<WorkbookDiagnostic>,
}
impl Workbook {
    /// Write an unencrypted, macro-free XLSX package. No filesystem I/O occurs.
    /// Decimal and integers beyond exact spreadsheet precision are rejected.
    pub fn to_xlsx(&self) -> Result<Vec<u8>, DataError> {
        let mut parts = BTreeMap::new();
        let mut styles = vec![CellStyle::default()];
        for sheet in &self.sheets {
            for cell in sheet.cells.values() {
                let style = effective_style(cell);
                if !styles.contains(&style) {
                    if styles.len() >= 1024 {
                        return Err(invalid("XLSX style capacity exceeded"));
                    }
                    styles.push(style);
                }
            }
        }
        parts.insert("xl/styles.xml".into(), write_styles(&styles).into_bytes());
        let mut sheets = String::new();
        let mut rels = String::new();
        let mut content = String::new();
        for (index, sheet) in self.sheets.iter().enumerate() {
            let n = index + 1;
            sheets.push_str(&format!(
                "<sheet name=\"{}\" sheetId=\"{n}\" r:id=\"sheet{n}\"/>",
                escape(&sheet.name)
            ));
            rels.push_str(&format!("<Relationship Id=\"sheet{n}\" Type=\"{REL}/worksheet\" Target=\"worksheets/sheet{n}.xml\"/>"));
            content.push_str(&format!("<Override PartName=\"/xl/worksheets/sheet{n}.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml\"/>"));
            let mut data = String::new();
            let mut open_row = None;
            for (address, cell) in &sheet.cells {
                if matches!(
                    cell.input,
                    CellInput::Error(FormulaError::Cycle | FormulaError::Limit)
                ) {
                    return Err(invalid(
                        "internal cycle/limit literals have no portable XLSX error type",
                    ));
                }
                if open_row != Some(address.row) {
                    if open_row.is_some() {
                        data.push_str("</row>");
                    }
                    data.push_str(&format!("<row r=\"{}\">", address.row + 1));
                    open_row = Some(address.row);
                }
                let style = styles
                    .iter()
                    .position(|s| *s == effective_style(cell))
                    .unwrap();
                let result = self.value(index, *address)?;
                let (mut kind, value) = value_xml(&result)?;
                if matches!(cell.input, CellInput::Formula(_))
                    && matches!(result, WorkbookValue::Scalar(CellValue::Text(_)))
                {
                    kind = " t=\"str\"".into();
                }
                data.push_str(&format!("<c r=\"{}\" s=\"{style}\"{kind}>", address.a1()));
                if let CellInput::Formula(text) = &cell.input {
                    data.push_str(&format!(
                        "<f>{}</f>",
                        escape(text.strip_prefix('=').unwrap_or(text))
                    ));
                }
                if matches!(result, WorkbookValue::Scalar(CellValue::Text(_)))
                    && !matches!(cell.input, CellInput::Formula(_))
                {
                    data.push_str(&format!("<is><t xml:space=\"preserve\">{value}</t></is>"));
                } else if !value.is_empty() {
                    data.push_str(&format!("<v>{value}</v>"));
                }
                data.push_str("</c>");
            }
            if open_row.is_some() {
                data.push_str("</row>");
            }
            let merges = if sheet.merges.is_empty() {
                String::new()
            } else {
                format!(
                    "<mergeCells count=\"{}\">{}</mergeCells>",
                    sheet.merges.len(),
                    sheet
                        .merges
                        .iter()
                        .map(|r| format!("<mergeCell ref=\"{}:{}\"/>", r.start.a1(), r.end.a1()))
                        .collect::<String>()
                )
            };
            let xml = format!(
                "<?xml version=\"1.0\" encoding=\"UTF-8\"?><worksheet xmlns=\"{NS}\"><sheetData>{data}</sheetData>{merges}</worksheet>"
            );
            parts.insert(format!("xl/worksheets/sheet{n}.xml"), xml.into_bytes());
        }
        parts.insert("xl/workbook.xml".into(),format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?><workbook xmlns=\"{NS}\" xmlns:r=\"{REL}\"><sheets>{sheets}</sheets><calcPr calcId=\"0\" fullCalcOnLoad=\"1\"/></workbook>").into_bytes());
        parts.insert("xl/_rels/workbook.xml.rels".into(),format!("<Relationships xmlns=\"{PKG}\">{rels}<Relationship Id=\"styles\" Type=\"{REL}/styles\" Target=\"styles.xml\"/></Relationships>").into_bytes());
        parts.insert("_rels/.rels".into(),format!("<Relationships xmlns=\"{PKG}\"><Relationship Id=\"workbook\" Type=\"{REL}/officeDocument\" Target=\"xl/workbook.xml\"/></Relationships>").into_bytes());
        parts.insert("[Content_Types].xml".into(),format!("<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/><Override PartName=\"/xl/workbook.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml\"/><Override PartName=\"/xl/styles.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml\"/>{content}</Types>").into_bytes());
        write_package(&parts)
    }
    /// Read a bounded package. Cached formula values are discarded and recalculated.
    /// Unsupported visual/package features are returned as explicit diagnostics.
    pub fn from_xlsx(bytes: &[u8]) -> Result<WorkbookImport, DataError> {
        read_workbook(bytes)
    }
}
fn effective_style(cell: &WorkbookCell) -> CellStyle {
    let mut s = cell.style.clone();
    if s.number_format.is_empty() {
        match cell.input {
            CellInput::Value(CellValue::Date(_)) => s.number_format = "dd.mm.yyyy".into(),
            CellInput::Value(CellValue::Time(_)) => s.number_format = "hh:mm:ss".into(),
            _ => {}
        }
    }
    s
}
fn value_xml(value: &WorkbookValue) -> Result<(String, String), DataError> {
    Ok(match value {
        WorkbookValue::Error(e) => (
            " t=\"e\"".into(),
            match e {
                FormulaError::Cycle => "#REF!",
                FormulaError::Limit => "#NUM!",
                _ => e.display(),
            }
            .into(),
        ),
        WorkbookValue::Scalar(v) => match v {
            CellValue::Null => (String::new(), String::new()),
            CellValue::Bool(b) => (" t=\"b\"".into(), u8::from(*b).to_string()),
            CellValue::Text(t) => (" t=\"inlineStr\"".into(), escape(&encode_string(t))),
            CellValue::Integer(n) => {
                if n.unsigned_abs() > 999_999_999_999_999 {
                    return Err(invalid(
                        "XLSX integer exceeds 15-digit interchange precision; store it as text",
                    ));
                }
                (String::new(), n.to_string())
            }
            CellValue::Real(n) => (String::new(), n.to_string()),
            CellValue::Date(d) => (" t=\"d\"".into(), d.to_string()),
            CellValue::Time(t) => (" t=\"d\"".into(), t.to_string()),
            CellValue::Decimal(_) => {
                return Err(invalid(
                    "exact decimal XLSX storage is not supported; convert explicitly to number or text",
                ));
            }
            CellValue::Binary(_) => return Err(invalid("binary XLSX cell")),
        },
    })
}
fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
        // References survive XML line-ending and attribute normalization.
        .replace('\r', "&#13;")
        .replace('\n', "&#10;")
        .replace('\t', "&#9;")
}
fn write_styles(styles: &[CellStyle]) -> String {
    let mut numbers = String::new();
    let mut fonts = String::new();
    let mut fills = String::from(
        "<fill><patternFill patternType=\"none\"/></fill><fill><patternFill patternType=\"gray125\"/></fill>",
    );
    let mut xfs = String::new();
    for (index, style) in styles.iter().enumerate() {
        numbers.push_str(&format!(
            "<numFmt numFmtId=\"{}\" formatCode=\"{}\"/>",
            164 + index,
            escape(if style.number_format.is_empty() {
                "General"
            } else {
                &style.number_format
            })
        ));
        fonts.push_str(&format!(
            "<font><sz val=\"11\"/><name val=\"Calibri\"/>{}{}{}</font>",
            if style.bold { "<b/>" } else { "" },
            if style.italic { "<i/>" } else { "" },
            style
                .foreground
                .map(|c| format!("<color rgb=\"FF{c:06X}\"/>"))
                .unwrap_or_default()
        ));
        fills.push_str(&style.background.map(|c|format!("<fill><patternFill patternType=\"solid\"><fgColor rgb=\"FF{c:06X}\"/><bgColor indexed=\"64\"/></patternFill></fill>")).unwrap_or_else(||"<fill><patternFill patternType=\"none\"/></fill>".into()));
        xfs.push_str(&format!("<xf numFmtId=\"{}\" fontId=\"{index}\" fillId=\"{}\" borderId=\"0\" xfId=\"0\" applyNumberFormat=\"1\" applyFont=\"1\" applyFill=\"1\" applyAlignment=\"1\"><alignment wrapText=\"{}\"/></xf>",164+index,index+2,u8::from(style.wrap)));
    }
    format!(
        "<styleSheet xmlns=\"{NS}\"><numFmts count=\"{}\">{numbers}</numFmts><fonts count=\"{}\">{fonts}</fonts><fills count=\"{}\">{fills}</fills><borders count=\"1\"><border><left/><right/><top/><bottom/><diagonal/></border></borders><cellStyleXfs count=\"1\"><xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"0\"/></cellStyleXfs><cellXfs count=\"{}\">{xfs}</cellXfs><cellStyles count=\"1\"><cellStyle name=\"Normal\" xfId=\"0\" builtinId=\"0\"/></cellStyles></styleSheet>",
        styles.len(),
        styles.len(),
        styles.len() + 2,
        styles.len()
    )
}
fn package_error(error: impl std::fmt::Display) -> DataError {
    DataError::Adapter(format!("XLSX package: {error}"))
}
fn write_package(parts: &BTreeMap<String, Vec<u8>>) -> Result<Vec<u8>, DataError> {
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    let mut total = 0;
    for (name, data) in parts {
        total += data.len();
        if data.len() > MAX_PART || total > MAX_PACKAGE {
            return Err(invalid("XLSX package capacity exceeded"));
        }
        zip.start_file(name, options).map_err(package_error)?;
        zip.write_all(data).map_err(package_error)?;
    }
    Ok(zip.finish().map_err(package_error)?.into_inner())
}
fn read_package(bytes: &[u8]) -> Result<BTreeMap<String, Vec<u8>>, DataError> {
    if bytes.len() > MAX_PACKAGE {
        return Err(invalid("XLSX compressed package capacity exceeded"));
    }
    crate::office_package::validate_zip_directory(bytes, 1024, MAX_PART, MAX_PACKAGE)?;
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).map_err(package_error)?;
    if zip.len() > 1024 {
        return Err(invalid("XLSX package part count exceeded"));
    }
    let mut parts = BTreeMap::new();
    let mut total = 0usize;
    for index in 0..zip.len() {
        let file = zip.by_index(index).map_err(package_error)?;
        let name = file.name().to_owned();
        if file.is_dir() {
            continue;
        }
        if name.starts_with('/')
            || name.contains('\\')
            || name
                .split('/')
                .any(|s| s.is_empty() || s == ".." || s == ".")
            || name.contains(':')
            || file.encrypted()
            || parts.contains_key(&name)
        {
            return Err(invalid("unsafe, encrypted or duplicate XLSX part"));
        }
        if file.size() > MAX_PART as u64 {
            return Err(invalid("XLSX expanded part capacity exceeded"));
        }
        let mut content = Vec::new();
        file.take((MAX_PART + 1) as u64)
            .read_to_end(&mut content)
            .map_err(package_error)?;
        total = total
            .checked_add(content.len())
            .ok_or_else(|| invalid("XLSX size overflow"))?;
        if content.len() > MAX_PART || total > MAX_PACKAGE {
            return Err(invalid("XLSX expanded package capacity exceeded"));
        }
        parts.insert(name, content);
    }
    Ok(parts)
}
fn xml<'a>(parts: &'a BTreeMap<String, Vec<u8>>, name: &str) -> Result<Document<'a>, DataError> {
    let bytes = parts
        .get(name)
        .ok_or_else(|| invalid(format!("missing XLSX part: {name}")))?;
    let text = std::str::from_utf8(bytes).map_err(package_error)?;
    Document::parse_with_options(
        text,
        roxmltree::ParsingOptions {
            allow_dtd: false,
            nodes_limit: 262_144,
            ..Default::default()
        },
    )
    .map_err(package_error)
}
fn children<'a, 'i>(node: Node<'a, 'i>, name: &'a str) -> impl Iterator<Item = Node<'a, 'i>> {
    node.children().filter(move |n| {
        n.is_element() && n.tag_name().namespace() == Some(NS) && n.tag_name().name() == name
    })
}
fn child<'a, 'i>(node: Node<'a, 'i>, name: &str) -> Option<Node<'a, 'i>> {
    node.children().find(|n| {
        n.is_element() && n.tag_name().namespace() == Some(NS) && n.tag_name().name() == name
    })
}
fn attr<'a, 'i>(node: Node<'a, 'i>, name: &str) -> Result<&'a str, DataError> {
    node.attribute(name)
        .ok_or_else(|| invalid(format!("missing XLSX {name} attribute")))
}
fn warning(diagnostics: &mut Vec<WorkbookDiagnostic>, part: &str, message: impl Into<String>) {
    if diagnostics.len() < 1024 {
        diagnostics.push(WorkbookDiagnostic {
            part: part.into(),
            message: message.into(),
        });
    }
}
fn singletons(node: Node<'_, '_>, names: &[&str]) -> Result<(), DataError> {
    let mut seen = BTreeSet::new();
    for element in node.children().filter(|n| n.is_element()) {
        if element.tag_name().namespace() == Some(NS)
            && names.contains(&element.tag_name().name())
            && !seen.insert(element.tag_name().name())
        {
            return Err(invalid(format!(
                "duplicate XLSX {} element",
                element.tag_name().name()
            )));
        }
    }
    Ok(())
}
fn attribute_diagnostics(
    node: Node<'_, '_>,
    allowed: &[(&str, Option<&str>)],
    path: &str,
    diagnostics: &mut Vec<WorkbookDiagnostic>,
) {
    for attribute in node.attributes() {
        if !allowed.contains(&(attribute.name(), attribute.namespace())) {
            warning(
                diagnostics,
                path,
                format!(
                    "{} attribute {} is not retained",
                    node.tag_name().name(),
                    attribute.name()
                ),
            );
        }
    }
}
fn resolve(base: &str, target: &str) -> Result<String, DataError> {
    if target.contains(['\\', ':', '?', '#', '%']) {
        return Err(invalid("unsupported XLSX relationship target"));
    }
    let mut segments: Vec<&str> = if target.starts_with('/') {
        Vec::new()
    } else {
        base.rsplit_once('/')
            .map(|(dir, _)| dir.split('/').collect())
            .unwrap_or_default()
    };
    for segment in target.trim_start_matches('/').split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                if segments.pop().is_none() {
                    return Err(invalid("XLSX relationship escapes package"));
                }
            }
            _ => segments.push(segment),
        }
    }
    if segments.is_empty() {
        return Err(invalid("empty XLSX relationship target"));
    }
    Ok(segments.join("/"))
}
fn relationships(
    parts: &BTreeMap<String, Vec<u8>>,
    path: &str,
    base: &str,
    diagnostics: &mut Vec<WorkbookDiagnostic>,
) -> Result<BTreeMap<String, (String, String)>, DataError> {
    let document = xml(parts, path)?;
    let root = document.root_element();
    if !root.has_tag_name((PKG, "Relationships")) {
        return Err(invalid("invalid XLSX relationships root"));
    }
    let mut result = BTreeMap::new();
    let mut seen = BTreeSet::new();
    for node in root.children().filter(|n| n.is_element()) {
        if !node.has_tag_name((PKG, "Relationship")) {
            return Err(invalid("invalid XLSX relationship"));
        }
        let key = attr(node, "Id")?.to_owned();
        if key.is_empty() || !seen.insert(key.clone()) {
            return Err(invalid("duplicate or empty XLSX relationship id"));
        }
        let target = attr(node, "Target")?;
        let kind = attr(node, "Type")?.to_owned();
        let mode = node.attribute("TargetMode").unwrap_or("Internal");
        if !matches!(mode, "Internal" | "External") {
            return Err(invalid("unknown XLSX relationship TargetMode"));
        }
        if mode == "External" {
            warning(
                diagnostics,
                path,
                "external relationship ignored; no network access",
            );
            continue;
        }
        let target = resolve(base, target)?;
        result.insert(key, (kind, target));
    }
    Ok(result)
}
fn read_workbook(bytes: &[u8]) -> Result<WorkbookImport, DataError> {
    let parts = read_package(bytes)?;
    let mut diagnostics = Vec::new();
    let types = xml(&parts, "[Content_Types].xml")?;
    if !types.root_element().has_tag_name((
        "http://schemas.openxmlformats.org/package/2006/content-types",
        "Types",
    )) {
        return Err(invalid("invalid content types root"));
    }
    if types.descendants().any(|n| {
        n.attribute("ContentType")
            .is_some_and(|t| t.contains("macroEnabled") || t.contains("vbaProject"))
    }) {
        return Err(invalid(
            "macro-enabled workbooks are outside the XLSX profile",
        ));
    }
    let roots = relationships(&parts, "_rels/.rels", "", &mut diagnostics)?;
    let office: Vec<_> = roots
        .values()
        .filter(|(kind, _)| kind == &format!("{REL}/officeDocument"))
        .collect();
    if office.len() != 1 {
        return Err(invalid("expected one XLSX office document relationship"));
    }
    let book_path = &office[0].1;
    let part_name = format!("/{book_path}");
    let overrides: Vec<_> = types
        .root_element()
        .children()
        .filter(|n| {
            n.is_element()
                && n.tag_name().name() == "Override"
                && n.attribute("PartName") == Some(part_name.as_str())
        })
        .collect();
    if overrides.len() > 1 {
        return Err(invalid("duplicate workbook content type"));
    }
    let declared = overrides
        .first()
        .and_then(|n| n.attribute("ContentType"))
        .or_else(|| {
            types
                .root_element()
                .children()
                .find(|n| {
                    n.is_element()
                        && n.tag_name().name() == "Default"
                        && n.attribute("Extension")
                            == book_path.rsplit_once('.').map(|(_, extension)| extension)
                })
                .and_then(|n| n.attribute("ContentType"))
        });
    if declared
        != Some("application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml")
    {
        return Err(invalid("missing or unsupported workbook content type"));
    }
    let document = xml(&parts, book_path)?;
    let root = document.root_element();
    if !root.has_tag_name((NS, "workbook")) {
        return Err(invalid("unsupported workbook namespace or root"));
    }
    singletons(
        root,
        &["sheets", "workbookPr", "bookViews", "calcPr", "fileVersion"],
    )?;
    attribute_diagnostics(root, &[], book_path, &mut diagnostics);
    let (dir, file) = book_path.rsplit_once('/').unwrap_or(("", book_path));
    let rel_path = if dir.is_empty() {
        format!("_rels/{file}.rels")
    } else {
        format!("{dir}/_rels/{file}.rels")
    };
    let rels = relationships(&parts, &rel_path, book_path, &mut diagnostics)?;
    let mut used = BTreeSet::from([
        "[Content_Types].xml".to_owned(),
        "_rels/.rels".to_owned(),
        book_path.clone(),
        rel_path,
    ]);
    let mut styles = vec![CellStyle::default()];
    let mut shared = Vec::new();
    let mut loaded_styles = false;
    let mut loaded_strings = false;
    for (kind, target) in rels.values() {
        if kind == &format!("{REL}/styles") {
            if loaded_styles {
                return Err(invalid("multiple XLSX styles relationships"));
            }
            loaded_styles = true;
            styles = read_styles(&parts, target, &mut diagnostics)?;
            used.insert(target.clone());
        } else if kind == &format!("{REL}/sharedStrings") {
            if loaded_strings {
                return Err(invalid("multiple XLSX sharedStrings relationships"));
            }
            loaded_strings = true;
            shared = read_strings(&parts, target, &mut diagnostics)?;
            used.insert(target.clone());
        }
    }
    let calendar = child(root, "workbookPr")
        .and_then(|n| n.attribute("date1904"))
        .unwrap_or("0");
    if !matches!(calendar, "0" | "1" | "false" | "true") {
        return Err(invalid("invalid XLSX date1904 value"));
    }
    let date1904 = matches!(calendar, "1" | "true");
    for node in root.children().filter(|n| n.is_element()) {
        if node.has_tag_name((NS, "workbookPr")) {
            attribute_diagnostics(node, &[("date1904", None)], book_path, &mut diagnostics);
            if node.children().any(|n| n.is_element()) {
                warning(
                    &mut diagnostics,
                    book_path,
                    "workbookPr child elements are not retained",
                );
            }
        } else if node.has_tag_name((NS, "calcPr")) {
            attribute_diagnostics(
                node,
                &[("calcId", None), ("fullCalcOnLoad", None)],
                book_path,
                &mut diagnostics,
            );
            if node
                .attribute("fullCalcOnLoad")
                .is_some_and(|v| !matches!(v, "1" | "true"))
                || node.children().any(|n| n.is_element())
            {
                warning(
                    &mut diagnostics,
                    book_path,
                    "calculation settings replaced by full recalculation",
                );
            }
        } else if !node.has_tag_name((NS, "sheets")) {
            warning(
                &mut diagnostics,
                book_path,
                format!("unsupported workbook element {}", node.tag_name().name()),
            );
        }
    }
    let sheets_node = child(root, "sheets").ok_or_else(|| invalid("XLSX sheets missing"))?;
    attribute_diagnostics(sheets_node, &[], book_path, &mut diagnostics);
    for node in sheets_node
        .children()
        .filter(|n| n.is_element() && !n.has_tag_name((NS, "sheet")))
    {
        warning(
            &mut diagnostics,
            book_path,
            format!("sheets child {} is not retained", node.tag_name().name()),
        );
    }
    let mut workbook = None;
    let mut edits = Vec::new();
    let mut sheet_ids = BTreeSet::new();
    for (index, sheet) in children(sheets_node, "sheet").enumerate() {
        if index >= MAX_SHEETS {
            return Err(invalid("XLSX sheet capacity exceeded"));
        }
        let name = attr(sheet, "name")?;
        attribute_diagnostics(
            sheet,
            &[
                ("name", None),
                ("sheetId", None),
                ("state", None),
                ("id", Some(REL)),
            ],
            book_path,
            &mut diagnostics,
        );
        if sheet.children().any(|n| n.is_element()) {
            warning(
                &mut diagnostics,
                book_path,
                "sheet child elements are not retained",
            );
        }
        validate_name(name)?;
        if !sheet_ids.insert(attr(sheet, "sheetId")?.to_owned()) {
            return Err(invalid("duplicate XLSX sheet id"));
        }
        if index == 0 {
            workbook = Some(Workbook::new(name)?);
        } else {
            edits.push(WorkbookEdit::AddSheet { name: name.into() });
        }
        if sheet.attribute("state").is_some_and(|s| s != "visible") {
            warning(
                &mut diagnostics,
                book_path,
                format!("sheet {name}: visibility state is not represented"),
            );
        }
        let rid = sheet
            .attribute((REL, "id"))
            .ok_or_else(|| invalid("worksheet relationship missing"))?;
        let (kind, path) = rels
            .get(rid)
            .ok_or_else(|| invalid("worksheet target missing"))?;
        if kind != &format!("{REL}/worksheet") {
            return Err(invalid("unsupported non-worksheet sheet"));
        }
        if !used.insert(path.clone()) {
            return Err(invalid("worksheet part reused"));
        }
        read_sheet(
            &parts,
            path,
            index,
            &styles,
            &shared,
            date1904,
            &mut edits,
            &mut diagnostics,
        )?;
    }
    let mut workbook = workbook.ok_or_else(|| invalid("XLSX workbook has no sheets"))?;
    workbook.apply(&edits)?;
    for path in parts.keys() {
        if !used.contains(path) {
            warning(
                &mut diagnostics,
                path,
                "part is outside the supported workbook profile and is not retained",
            );
        }
    }
    Ok(WorkbookImport {
        workbook,
        diagnostics,
    })
}
fn read_strings(
    parts: &BTreeMap<String, Vec<u8>>,
    path: &str,
    diagnostics: &mut Vec<WorkbookDiagnostic>,
) -> Result<Vec<String>, DataError> {
    let doc = xml(parts, path)?;
    if !doc.root_element().has_tag_name((NS, "sst")) {
        return Err(invalid("invalid shared strings root"));
    }
    let mut result = Vec::new();
    for item in children(doc.root_element(), "si") {
        if result.len() >= MAX_CELLS {
            return Err(invalid("shared string capacity exceeded"));
        }
        result.push(read_text(item, path, diagnostics)?);
    }
    Ok(result)
}
fn read_text(
    node: Node<'_, '_>,
    path: &str,
    diagnostics: &mut Vec<WorkbookDiagnostic>,
) -> Result<String, DataError> {
    let mut text = String::new();
    singletons(node, &["t"])?;
    if child(node, "t").is_some() && child(node, "r").is_some() {
        return Err(invalid("conflicting plain and rich XLSX string"));
    }
    for item in node.children().filter(|n| n.is_element()) {
        if item.has_tag_name((NS, "t")) {
            append_text(item, &mut text)?;
        } else if item.has_tag_name((NS, "r")) {
            warning(diagnostics, path, "rich cell text flattened to plain text");
            singletons(item, &["t", "rPr"])?;
            if let Some(t) = child(item, "t") {
                append_text(t, &mut text)?;
            }
            for extra in item.children().filter(|n| n.is_element()) {
                if !extra.has_tag_name((NS, "t")) && !extra.has_tag_name((NS, "rPr")) {
                    return Err(invalid("unsupported rich XLSX text element"));
                }
            }
        } else {
            warning(
                diagnostics,
                path,
                format!("unsupported string element {}", item.tag_name().name()),
            );
        }
        if text.len() > MAX_TEXT_BYTES {
            return Err(invalid("XLSX string capacity exceeded"));
        }
    }
    decode_string(&text)
}
fn append_text(node: Node<'_, '_>, output: &mut String) -> Result<(), DataError> {
    for content in node.children() {
        if content.is_element() {
            return Err(invalid("nested XML markup in XLSX text"));
        }
        if content.is_text() {
            let text = content.text().unwrap_or("");
            if output.len() + text.len() > MAX_TEXT_BYTES {
                return Err(invalid("XLSX string capacity exceeded"));
            }
            output.push_str(text);
        }
    }
    Ok(())
}
fn escaped_unit(text: &str) -> Option<u16> {
    let b = text.as_bytes();
    if b.len() >= 7
        && b[0] == b'_'
        && b[1] == b'x'
        && b[6] == b'_'
        && b[2..6].iter().all(u8::is_ascii_hexdigit)
    {
        u16::from_str_radix(&text[2..6], 16).ok()
    } else {
        None
    }
}
fn encode_string(text: &str) -> String {
    let mut output = String::new();
    let mut index = 0;
    while index < text.len() {
        let tail = &text[index..];
        if escaped_unit(tail).is_some() {
            output.push_str("_x005F_");
            index += 1;
        } else {
            let c = tail.chars().next().unwrap();
            output.push(c);
            index += c.len_utf8();
        }
    }
    output
}
fn decode_string(text: &str) -> Result<String, DataError> {
    let mut output = String::new();
    let mut index = 0;
    while index < text.len() {
        let tail = &text[index..];
        if let Some(first) = escaped_unit(tail) {
            index += 7;
            let c = if (0xd800..=0xdbff).contains(&first) {
                let second = escaped_unit(&text[index..])
                    .filter(|n| (0xdc00..=0xdfff).contains(n))
                    .ok_or_else(|| invalid("invalid XLSX string surrogate pair"))?;
                index += 7;
                char::from_u32(
                    0x10000 + ((u32::from(first) - 0xd800) << 10) + (u32::from(second) - 0xdc00),
                )
                .ok_or_else(|| invalid("invalid XLSX Unicode"))?
            } else {
                char::from_u32(u32::from(first)).ok_or_else(|| invalid("invalid XLSX Unicode"))?
            };
            output.push(c);
        } else {
            let c = tail.chars().next().unwrap();
            output.push(c);
            index += c.len_utf8();
        }
    }
    if !valid_xml(&output) {
        return Err(invalid(
            "encoded control character is outside workbook text profile",
        ));
    }
    Ok(output)
}
#[allow(clippy::too_many_arguments)]
fn read_sheet(
    parts: &BTreeMap<String, Vec<u8>>,
    path: &str,
    sheet: usize,
    styles: &[CellStyle],
    shared: &[String],
    date1904: bool,
    edits: &mut Vec<WorkbookEdit>,
    diagnostics: &mut Vec<WorkbookDiagnostic>,
) -> Result<(), DataError> {
    let doc = xml(parts, path)?;
    let root = doc.root_element();
    if !root.has_tag_name((NS, "worksheet")) {
        return Err(invalid("invalid worksheet root"));
    }
    singletons(
        root,
        &[
            "sheetData",
            "dimension",
            "sheetViews",
            "sheetFormatPr",
            "pageMargins",
            "mergeCells",
        ],
    )?;
    attribute_diagnostics(root, &[], path, diagnostics);
    for node in root.children().filter(|n| n.is_element()) {
        if !node.has_tag_name((NS, "sheetData"))
            && !node.has_tag_name((NS, "dimension"))
            && !node.has_tag_name((NS, "mergeCells"))
        {
            warning(
                diagnostics,
                path,
                format!("unsupported worksheet element {}", node.tag_name().name()),
            );
        }
    }
    let data = child(root, "sheetData").ok_or_else(|| invalid("worksheet sheetData missing"))?;
    attribute_diagnostics(data, &[], path, diagnostics);
    for node in data
        .children()
        .filter(|n| n.is_element() && !n.has_tag_name((NS, "row")))
    {
        warning(
            diagnostics,
            path,
            format!("sheetData child {} is not retained", node.tag_name().name()),
        );
    }
    let mut seen = BTreeSet::new();
    let mut seen_rows = BTreeSet::new();
    for row in children(data, "row") {
        attribute_diagnostics(row, &[("r", None), ("spans", None)], path, diagnostics);
        let row_index = row
            .attribute("r")
            .map(|value| value.parse::<u32>().map_err(package_error))
            .transpose()?;
        if row_index.is_some_and(|index| index == 0 || index > MAX_ROWS || !seen_rows.insert(index))
        {
            return Err(invalid("invalid or duplicate worksheet row"));
        }
        for node in row
            .children()
            .filter(|n| n.is_element() && !n.has_tag_name((NS, "c")))
        {
            warning(
                diagnostics,
                path,
                format!("row child {} is not retained", node.tag_name().name()),
            );
        }
        for node in children(row, "c") {
            if edits.len() >= MAX_CELLS {
                return Err(invalid("XLSX cell capacity exceeded"));
            }
            let address = CellAddress::parse(attr(node, "r")?)?;
            if row_index.is_some_and(|row| address.row() + 1 != row) {
                return Err(invalid("cell address disagrees with its worksheet row"));
            }
            if !seen.insert(address) {
                return Err(invalid("duplicate worksheet cell"));
            }
            singletons(node, &["f", "v", "is"])?;
            attribute_diagnostics(
                node,
                &[("r", None), ("s", None), ("t", None)],
                path,
                diagnostics,
            );
            for element in node.children().filter(|n| n.is_element()) {
                if !element.has_tag_name((NS, "v"))
                    && !element.has_tag_name((NS, "f"))
                    && !element.has_tag_name((NS, "is"))
                {
                    warning(
                        diagnostics,
                        path,
                        format!("cell child {} is not retained", element.tag_name().name()),
                    );
                }
                if element.has_tag_name((NS, "v")) || element.has_tag_name((NS, "f")) {
                    if element.children().any(|n| n.is_element()) {
                        return Err(invalid("XLSX cell value/formula must contain text only"));
                    }
                    let allowed = if element.has_tag_name((NS, "f")) {
                        &[("t", None), ("ref", None)][..]
                    } else {
                        &[][..]
                    };
                    attribute_diagnostics(element, allowed, path, diagnostics);
                }
            }
            let inline = node.attribute("t") == Some("inlineStr");
            if inline && (child(node, "v").is_some() || child(node, "f").is_some())
                || !inline && child(node, "is").is_some()
            {
                return Err(invalid("conflicting XLSX cell encodings"));
            }
            let style_index: usize = node
                .attribute("s")
                .unwrap_or("0")
                .parse()
                .map_err(package_error)?;
            let style = styles
                .get(style_index)
                .ok_or_else(|| invalid("invalid cell style index"))?
                .clone();
            let raw = child(node, "v").and_then(|v| v.text()).unwrap_or("");
            let input = if let Some(f) = child(node, "f") {
                if f.attribute("t").is_some_and(|t| t != "normal") || f.attribute("ref").is_some() {
                    return Err(invalid(
                        "shared, array and data-table formulas are outside the current profile",
                    ));
                }
                CellInput::Formula(format!("={}", f.text().unwrap_or("")))
            } else {
                match node.attribute("t").unwrap_or("n") {
                    "inlineStr" => CellInput::Value(CellValue::Text(read_text(
                        child(node, "is").ok_or_else(|| invalid("inline string missing"))?,
                        path,
                        diagnostics,
                    )?)),
                    "s" => {
                        let index: usize = raw.parse().map_err(package_error)?;
                        CellInput::Value(CellValue::Text(
                            shared
                                .get(index)
                                .ok_or_else(|| invalid("invalid shared string index"))?
                                .clone(),
                        ))
                    }
                    "str" => CellInput::Value(CellValue::Text(decode_string(raw)?)),
                    "b" => CellInput::Value(CellValue::Bool(match raw {
                        "1" | "true" => true,
                        "0" | "false" => false,
                        _ => return Err(invalid("invalid boolean cell")),
                    })),
                    "e" => CellInput::Error(match raw {
                        "#DIV/0!" => FormulaError::DivisionByZero,
                        "#VALUE!" => FormulaError::Value,
                        "#REF!" => FormulaError::Reference,
                        "#NAME?" => FormulaError::Name,
                        "#NUM!" => FormulaError::Number,
                        _ => {
                            warning(
                                diagnostics,
                                path,
                                format!("error {raw} represented as #VALUE!"),
                            );
                            FormulaError::Value
                        }
                    }),
                    "d" => {
                        let value = if let Ok(date) = raw.parse::<crate::DateValue>() {
                            CellValue::Date(date)
                        } else if let Ok(time) = raw.parse::<crate::TimeValue>() {
                            CellValue::Time(time)
                        } else {
                            warning(
                                diagnostics,
                                path,
                                "date-time cell preserved as ISO text; combined timestamp not represented",
                            );
                            CellValue::Text(raw.into())
                        };
                        CellInput::Value(value)
                    }
                    "n" => {
                        let value = if raw.is_empty() {
                            CellValue::Null
                        } else {
                            let n: f64 = raw.parse().map_err(package_error)?;
                            if !n.is_finite() {
                                return Err(invalid("nonfinite XLSX number"));
                            }
                            if let Some(date) = serial_value(n, &style.number_format, date1904)? {
                                date
                            } else if let Ok(integer) = raw.parse::<i64>() {
                                CellValue::Integer(integer)
                            } else {
                                CellValue::Real(n)
                            }
                        };
                        CellInput::Value(value)
                    }
                    _ => return Err(invalid("unsupported XLSX cell type")),
                }
            };
            edits.push(WorkbookEdit::Set {
                sheet,
                address,
                cell: WorkbookCell { input, style },
            });
        }
    }
    if let Some(merges) = child(root, "mergeCells") {
        let mut count = 0usize;
        attribute_diagnostics(merges, &[("count", None)], path, diagnostics);
        for node in merges.children().filter(|n| n.is_element()) {
            if !node.has_tag_name((NS, "mergeCell")) || count >= 1024 {
                return Err(invalid("invalid merged cells"));
            }
            attribute_diagnostics(node, &[("ref", None)], path, diagnostics);
            let (first, last) = attr(node, "ref")?
                .split_once(':')
                .ok_or_else(|| invalid("invalid merge reference"))?;
            edits.push(WorkbookEdit::Merge {
                sheet,
                range: CellRange::new(CellAddress::parse(first)?, CellAddress::parse(last)?)?,
            });
            count += 1;
        }
        if let Some(declared) = merges.attribute("count")
            && declared.parse::<usize>().map_err(package_error)? != count
        {
            return Err(invalid("merge count mismatch"));
        }
    }
    Ok(())
}
fn style_children_diagnostics(
    node: Node<'_, '_>,
    allowed: &[&str],
    path: &str,
    diagnostics: &mut Vec<WorkbookDiagnostic>,
) {
    for item in node.children().filter(|n| n.is_element()) {
        if item.tag_name().namespace() != Some(NS) || !allowed.contains(&item.tag_name().name()) {
            warning(
                diagnostics,
                path,
                format!(
                    "{} child {} is not retained",
                    node.tag_name().name(),
                    item.tag_name().name()
                ),
            );
        }
    }
}
fn empty_style_metadata(node: Node<'_, '_>) -> bool {
    // These are the inert Normal/empty-border records emitted by this codec.
    // Other metadata must be reported rather than silently flattened.
    let items: Vec<_> = node.children().filter(|n| n.is_element()).collect();
    match node.tag_name().name() {
        "borders" => items.iter().all(|border| {
            border.has_tag_name((NS, "border"))
                && border.attributes().len() == 0
                && border.children().filter(|n| n.is_element()).all(|side| {
                    side.tag_name().namespace() == Some(NS)
                        && matches!(
                            side.tag_name().name(),
                            "left"
                                | "right"
                                | "top"
                                | "bottom"
                                | "diagonal"
                                | "vertical"
                                | "horizontal"
                        )
                        && side.attributes().len() == 0
                        && !side.children().any(|n| n.is_element())
                })
        }),
        "cellStyleXfs" => {
            items.len() == 1
                && items[0].has_tag_name((NS, "xf"))
                && !items[0].children().any(|n| n.is_element())
                && items[0].attributes().all(|a| {
                    a.namespace().is_none()
                        && matches!(
                            a.name(),
                            "numFmtId" | "fontId" | "fillId" | "borderId" | "xfId"
                        )
                        && a.value() == "0"
                })
        }
        "cellStyles" => {
            items.len() == 1
                && items[0].has_tag_name((NS, "cellStyle"))
                && !items[0].children().any(|n| n.is_element())
                && items[0].attributes().all(|a| {
                    a.namespace().is_none()
                        && matches!(
                            (a.name(), a.value()),
                            ("name", "Normal") | ("xfId" | "builtinId", "0")
                        )
                })
        }
        _ => false,
    }
}
fn read_styles(
    parts: &BTreeMap<String, Vec<u8>>,
    path: &str,
    diagnostics: &mut Vec<WorkbookDiagnostic>,
) -> Result<Vec<CellStyle>, DataError> {
    let doc = xml(parts, path)?;
    let root = doc.root_element();
    if !root.has_tag_name((NS, "styleSheet")) {
        return Err(invalid("invalid styles root"));
    }
    attribute_diagnostics(root, &[], path, diagnostics);
    singletons(
        root,
        &[
            "numFmts",
            "fonts",
            "fills",
            "borders",
            "cellStyleXfs",
            "cellXfs",
            "cellStyles",
            "dxfs",
            "tableStyles",
            "colors",
            "extLst",
        ],
    )?;
    for node in root.children().filter(|n| n.is_element()) {
        attribute_diagnostics(node, &[("count", None)], path, diagnostics);
        if node.tag_name().namespace() != Some(NS) {
            warning(
                diagnostics,
                path,
                format!(
                    "foreign style element {} is not retained",
                    node.tag_name().name()
                ),
            );
            continue;
        }
        match node.tag_name().name() {
            "numFmts" => style_children_diagnostics(node, &["numFmt"], path, diagnostics),
            "fonts" => style_children_diagnostics(node, &["font"], path, diagnostics),
            "fills" => style_children_diagnostics(node, &["fill"], path, diagnostics),
            "cellXfs" => style_children_diagnostics(node, &["xf"], path, diagnostics),
            "borders" | "cellStyleXfs" | "cellStyles" if empty_style_metadata(node) => {}
            _ => warning(
                diagnostics,
                path,
                format!("style {} is not retained", node.tag_name().name()),
            ),
        }
    }
    let mut formats = BTreeMap::new();
    if let Some(node) = child(root, "numFmts") {
        for f in children(node, "numFmt") {
            attribute_diagnostics(
                f,
                &[("numFmtId", None), ("formatCode", None)],
                path,
                diagnostics,
            );
            style_children_diagnostics(f, &[], path, diagnostics);
            let id: u32 = attr(f, "numFmtId")?.parse().map_err(package_error)?;
            if formats
                .insert(id, attr(f, "formatCode")?.to_owned())
                .is_some()
            {
                return Err(invalid("duplicate number format"));
            }
        }
    }
    let fonts: Vec<_> = child(root, "fonts")
        .map(|n| children(n, "font").collect())
        .unwrap_or_default();
    let fills: Vec<_> = child(root, "fills")
        .map(|n| children(n, "fill").collect())
        .unwrap_or_default();
    let Some(xfs) = child(root, "cellXfs") else {
        return Ok(vec![CellStyle::default()]);
    };
    let mut result = Vec::new();
    for xf in children(xfs, "xf") {
        if result.len() >= 1024 {
            return Err(invalid("XLSX style capacity exceeded"));
        }
        singletons(xf, &["alignment", "protection", "extLst"])?;
        attribute_diagnostics(
            xf,
            &[
                ("numFmtId", None),
                ("fontId", None),
                ("fillId", None),
                ("borderId", None),
                ("xfId", None),
                ("applyNumberFormat", None),
                ("applyFont", None),
                ("applyFill", None),
                ("applyAlignment", None),
            ],
            path,
            diagnostics,
        );
        style_children_diagnostics(xf, &["alignment"], path, diagnostics);
        if xf.attribute("xfId").is_some_and(|id| id != "0") {
            warning(
                diagnostics,
                path,
                "cell xfId style inheritance is not represented",
            );
        }
        for flag in [
            "applyNumberFormat",
            "applyFont",
            "applyFill",
            "applyAlignment",
        ] {
            if xf
                .attribute(flag)
                .is_some_and(|value| !matches!(value, "1" | "true"))
            {
                warning(
                    diagnostics,
                    path,
                    format!("cell {flag} flag is not represented; direct formatting retained"),
                );
            }
        }
        let mut style = CellStyle::default();
        let number: u32 = xf
            .attribute("numFmtId")
            .unwrap_or("0")
            .parse()
            .map_err(package_error)?;
        style.number_format = if let Some(format) = formats.get(&number) {
            if format == "General" {
                String::new()
            } else {
                format.clone()
            }
        } else {
            match number {
                0 => "",
                1 => "0",
                2 => "0.00",
                9 => "0%",
                10 => "0.00%",
                14 => "mm-dd-yy",
                20 => "hh:mm",
                21 => "hh:mm:ss",
                22 => "m/d/yy h:mm",
                _ => {
                    warning(
                        diagnostics,
                        path,
                        format!("built-in number format {number} not represented"),
                    );
                    ""
                }
            }
            .into()
        };
        let font: usize = xf
            .attribute("fontId")
            .unwrap_or("0")
            .parse()
            .map_err(package_error)?;
        if let Some(font) = fonts.get(font) {
            attribute_diagnostics(*font, &[], path, diagnostics);
            singletons(*font, &["b", "i", "color", "sz", "name"])?;
            style.bold = child(*font, "b")
                .is_some_and(|n| !matches!(n.attribute("val"), Some("0" | "false")));
            style.italic = child(*font, "i")
                .is_some_and(|n| !matches!(n.attribute("val"), Some("0" | "false")));
            if let Some(color) = child(*font, "color") {
                style.foreground = read_color(color, path, diagnostics)?;
            }
            for node in font.children().filter(|n| n.is_element()) {
                let supported = (node.tag_name().namespace() == Some(NS)
                    && matches!(node.tag_name().name(), "b" | "i" | "color"))
                    || (node.has_tag_name((NS, "sz")) && node.attribute("val") == Some("11"))
                    || (node.has_tag_name((NS, "name"))
                        && node.attribute("val") == Some("Calibri"));
                if !supported {
                    warning(
                        diagnostics,
                        path,
                        format!("font {} is not represented", node.tag_name().name()),
                    );
                } else {
                    let allowed = if node.has_tag_name((NS, "color")) {
                        &[("rgb", None)][..]
                    } else {
                        &[("val", None)][..]
                    };
                    attribute_diagnostics(node, allowed, path, diagnostics);
                    style_children_diagnostics(node, &[], path, diagnostics);
                }
            }
        } else if font != 0 || !fonts.is_empty() {
            return Err(invalid("font index outside styles"));
        }
        let fill: usize = xf
            .attribute("fillId")
            .unwrap_or("0")
            .parse()
            .map_err(package_error)?;
        if let Some(fill) = fills.get(fill) {
            attribute_diagnostics(*fill, &[], path, diagnostics);
            singletons(*fill, &["patternFill", "gradientFill"])?;
            style_children_diagnostics(*fill, &["patternFill"], path, diagnostics);
            if let Some(pattern) = child(*fill, "patternFill") {
                attribute_diagnostics(pattern, &[("patternType", None)], path, diagnostics);
                singletons(pattern, &["fgColor", "bgColor"])?;
                style_children_diagnostics(pattern, &["fgColor", "bgColor"], path, diagnostics);
                for color in pattern.children().filter(|n| n.is_element()) {
                    if color.has_tag_name((NS, "fgColor")) {
                        attribute_diagnostics(color, &[("rgb", None)], path, diagnostics);
                        style_children_diagnostics(color, &[], path, diagnostics);
                    } else if color.has_tag_name((NS, "bgColor"))
                        && !(color.attributes().len() == 1
                            && color.attribute("indexed") == Some("64")
                            && !color.children().any(|n| n.is_element()))
                    {
                        warning(diagnostics, path, "patternFill bgColor is not retained");
                    }
                }
                match pattern.attribute("patternType").unwrap_or("none") {
                    "solid" => {
                        if let Some(color) = child(pattern, "fgColor") {
                            style.background = read_color(color, path, diagnostics)?;
                        }
                    }
                    "none" | "gray125" => {}
                    _ => warning(diagnostics, path, "pattern fill is not represented"),
                }
            } else {
                warning(diagnostics, path, "gradient fill is not represented");
            }
        } else if fill != 0 || !fills.is_empty() {
            return Err(invalid("fill index outside styles"));
        }
        if xf.attribute("borderId").is_some_and(|n| n != "0") {
            warning(diagnostics, path, "cell border is not represented");
        }
        if let Some(alignment) = child(xf, "alignment") {
            style_children_diagnostics(alignment, &[], path, diagnostics);
            style.wrap = matches!(alignment.attribute("wrapText"), Some("1" | "true"));
            for a in alignment.attributes() {
                if a.name() != "wrapText" {
                    warning(
                        diagnostics,
                        path,
                        format!("alignment {} is not represented", a.name()),
                    );
                }
            }
        }
        result.push(style);
    }
    if result.is_empty() {
        return Err(invalid("XLSX cell styles empty"));
    }
    Ok(result)
}
fn read_color(
    node: Node<'_, '_>,
    path: &str,
    diagnostics: &mut Vec<WorkbookDiagnostic>,
) -> Result<Option<u32>, DataError> {
    if let Some(rgb) = node.attribute("rgb") {
        if rgb.len() != 8 || !rgb.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(invalid("invalid XLSX ARGB color"));
        }
        if !rgb[..2].eq_ignore_ascii_case("FF") {
            warning(diagnostics, path, "color alpha not represented");
        }
        return Ok(Some(
            u32::from_str_radix(&rgb[2..], 16).map_err(package_error)?,
        ));
    }
    warning(diagnostics, path, "theme/indexed color not represented");
    Ok(None)
}
fn serial_value(n: f64, format: &str, date1904: bool) -> Result<Option<CellValue>, DataError> {
    let lower = format_tokens(format)?;
    let has_date = lower.contains('y') || lower.contains('d');
    let has_time = lower.contains('h') || lower.contains('s');
    if !has_date && !has_time {
        return Ok(None);
    }
    if has_date && has_time {
        return Err(invalid(
            "combined numeric date-time outside XLSX value profile",
        ));
    }
    if has_time {
        if !(0.0..1.0).contains(&n) {
            return Err(invalid("time serial outside a single day"));
        }
        let nanos = (n * 86_400_000_000_000.0).round() as u64;
        if nanos >= 86_400_000_000_000 {
            return Err(invalid("rounded time outside a day"));
        }
        let seconds = (nanos / 1_000_000_000) as u32;
        return Ok(Some(CellValue::Time(crate::TimeValue::new(
            (seconds / 3600) as u8,
            ((seconds / 60) % 60) as u8,
            (seconds % 60) as u8,
            (nanos % 1_000_000_000) as u32,
        )?)));
    }
    if n.fract() != 0.0 || !(0.0..=3_000_000.0).contains(&n) {
        return Err(invalid("invalid date serial"));
    }
    let mut days = n as i64;
    if !date1904 {
        if days == 60 {
            return Err(invalid("fictional 1900-02-29 is not a valid date"));
        }
        if days > 60 {
            days -= 1;
        }
    }
    let mut date = crate::DateValue::new(
        if date1904 { 1904 } else { 1899 },
        if date1904 { 1 } else { 12 },
        if date1904 { 1 } else { 31 },
    )?;
    // Bounded civil conversion; fewer than 100,000 month steps.
    let mut year = date.year();
    let mut month = date.month();
    let mut day = date.day();
    while days > 0 {
        let month_days = (28..=31)
            .rev()
            .find(|d| crate::DateValue::new(year, month, *d).is_ok())
            .unwrap();
        let left = i64::from(month_days - day);
        if days <= left {
            day += days as u8;
            break;
        }
        days -= left + 1;
        day = 1;
        if month == 12 {
            year = year
                .checked_add(1)
                .ok_or_else(|| invalid("date overflow"))?;
            month = 1;
        } else {
            month += 1;
        }
        if year > 9999 {
            return Err(invalid("date exceeds supported years"));
        }
    }
    date = crate::DateValue::new(year, month, day)?;
    Ok(Some(CellValue::Date(date)))
}

fn format_tokens(format: &str) -> Result<String, DataError> {
    let mut tokens = String::new();
    let mut chars = format.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                let mut closed = false;
                for c in chars.by_ref() {
                    if c == '"' {
                        closed = true;
                        break;
                    }
                }
                if !closed {
                    return Err(invalid("unterminated number format literal"));
                }
            }
            '\\' | '_' | '*' => {
                chars
                    .next()
                    .ok_or_else(|| invalid("truncated number format escape"))?;
            }
            '[' => {
                let mut directive = String::new();
                let mut closed = false;
                for c in chars.by_ref() {
                    if c == ']' {
                        closed = true;
                        break;
                    }
                    directive.push(c);
                }
                if !closed {
                    return Err(invalid("unterminated number format directive"));
                }
                if matches!(
                    directive.to_ascii_lowercase().as_str(),
                    "h" | "hh" | "m" | "mm" | "s" | "ss"
                ) {
                    return Err(invalid(
                        "elapsed duration format is outside time-of-day profile",
                    ));
                }
            }
            ';' => break,
            _ => tokens.extend(c.to_lowercase()),
        }
    }
    Ok(tokens)
}

#[cfg(test)]
pub(super) fn test_package(parts: &[(&str, &str)]) -> Vec<u8> {
    write_package(
        &parts
            .iter()
            .map(|(n, s)| (n.to_string(), s.as_bytes().to_vec()))
            .collect(),
    )
    .unwrap()
}

#[cfg(test)]
mod serial_tests {
    use super::*;
    #[test]
    fn formats_ignore_literals_colors_and_escaped_letters() {
        for format in ["0 \"day\"", "[Red]0.00", "0.00 \"USD\"", "0 \\d"] {
            assert_eq!(serial_value(5.0, format, false).unwrap(), None, "{format}");
        }
        assert_eq!(
            serial_value(1.0, "[$-ru-RU]dd.mm.yyyy", false).unwrap(),
            Some(CellValue::Date(crate::DateValue::new(1900, 1, 1).unwrap()))
        );
        assert!(serial_value(0.5, "[h]:mm", false).is_err());
    }
    #[test]
    fn numeric_time_preserves_subsecond_and_leap_bug_is_explicit() {
        assert_eq!(
            serial_value(0.125 / 86400.0, "hh:mm:ss.000", false).unwrap(),
            Some(CellValue::Time(
                crate::TimeValue::new(0, 0, 0, 125_000_000).unwrap()
            ))
        );
        assert!(serial_value(60.0, "yyyy-mm-dd", false).is_err());
        assert_eq!(
            serial_value(0.0, "yyyy-mm-dd", true).unwrap(),
            Some(CellValue::Date(crate::DateValue::new(1904, 1, 1).unwrap()))
        );
    }
}
