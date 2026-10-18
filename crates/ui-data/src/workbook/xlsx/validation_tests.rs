use super::*;

fn package(sheet_body: &str, workbook_extra: &str) -> Vec<u8> {
    let types = "<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Override PartName=\"/xl/workbook.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml\"/></Types>";
    let roots = format!(
        "<Relationships xmlns=\"{PKG}\"><Relationship Id=\"book\" Type=\"{REL}/officeDocument\" Target=\"xl/workbook.xml\"/></Relationships>"
    );
    let book = format!(
        "<workbook xmlns=\"{NS}\" xmlns:r=\"{REL}\"><sheets><sheet name=\"Data\" sheetId=\"1\" r:id=\"one\"/></sheets>{workbook_extra}</workbook>"
    );
    let rels = format!(
        "<Relationships xmlns=\"{PKG}\"><Relationship Id=\"one\" Type=\"{REL}/worksheet\" Target=\"worksheets/sheet1.xml\"/></Relationships>"
    );
    let sheet = format!("<worksheet xmlns=\"{NS}\">{sheet_body}</worksheet>");
    test_package(&[
        ("[Content_Types].xml", types),
        ("_rels/.rels", &roots),
        ("xl/workbook.xml", &book),
        ("xl/_rels/workbook.xml.rels", &rels),
        ("xl/worksheets/sheet1.xml", &sheet),
    ])
}

#[test]
fn external_relationships_still_participate_in_unique_identity_validation() {
    for entries in [
        "<Relationship Id=\"same\" Type=\"external\" Target=\"https://example.invalid\" TargetMode=\"External\"/><Relationship Id=\"same\" Type=\"internal\" Target=\"part.xml\"/>",
        "<Relationship Id=\"same\" Type=\"internal\" Target=\"part.xml\"/><Relationship Id=\"same\" Type=\"external\" Target=\"https://example.invalid\" TargetMode=\"External\"/>",
        "<Relationship Id=\"one\" Type=\"type\" Target=\"part.xml\" TargetMode=\"Unknown\"/>",
    ] {
        let xml = format!("<Relationships xmlns=\"{PKG}\">{entries}</Relationships>");
        let parts = BTreeMap::from([("rels".into(), xml.into_bytes())]);
        assert!(relationships(&parts, "rels", "", &mut Vec::new()).is_err());
    }
}

#[test]
fn duplicate_singletons_and_conflicting_cell_encodings_reject_the_whole_import() {
    for body in [
        "<sheetData/><sheetData/>",
        "<sheetData><row r=\"1\"><c r=\"A1\"><v>1</v><v>2</v></c></row></sheetData>",
        "<sheetData><row r=\"1\"><c r=\"A1\"><f>1</f><f>2</f></c></row></sheetData>",
        "<sheetData><row r=\"1\"><c r=\"A1\" t=\"inlineStr\"><is><t>one</t></is><v>2</v></c></row></sheetData>",
        "<sheetData><row r=\"1\"><c r=\"A1\"><v>2<extra/></v></c></row></sheetData>",
    ] {
        assert!(Workbook::from_xlsx(&package(body, "")).is_err(), "{body}");
    }
    assert!(Workbook::from_xlsx(&package("<sheetData/>", "<workbookPr/><workbookPr/>")).is_err());
    assert!(
        Workbook::from_xlsx(&package("<sheetData/>", "<workbookPr date1904=\"maybe\"/>")).is_err()
    );
}

#[test]
fn row_identity_and_cell_address_must_agree() {
    for body in [
        "<sheetData><row r=\"1\"><c r=\"A2\"><v>2</v></c></row></sheetData>",
        "<sheetData><row r=\"1\"/><row r=\"1\"/></sheetData>",
        "<sheetData><row r=\"0\"/></sheetData>",
    ] {
        assert!(Workbook::from_xlsx(&package(body, "")).is_err(), "{body}");
    }
}

#[test]
fn omitted_visual_and_metadata_features_are_reported_without_losing_cell_value() {
    let bytes = package(
        "<sheetViews><sheetView workbookViewId=\"0\"><pane xSplit=\"1\" state=\"frozen\"/></sheetView></sheetViews><sheetData><row r=\"1\" hidden=\"1\" customHeight=\"1\" ht=\"40\"><c r=\"A1\" vm=\"4\"><v>42</v><custom/></c></row></sheetData>",
        "<workbookPr date1904=\"false\" defaultThemeVersion=\"1\"/><calcPr calcMode=\"manual\"/><bookViews><workbookView/></bookViews>",
    );
    let result = Workbook::from_xlsx(&bytes).unwrap();
    assert_eq!(
        result
            .workbook
            .value(0, CellAddress::parse("A1").unwrap())
            .unwrap(),
        WorkbookValue::Scalar(CellValue::Integer(42))
    );
    for feature in [
        "sheetViews",
        "hidden",
        "customHeight",
        "ht",
        "vm",
        "custom",
        "defaultThemeVersion",
        "calcMode",
        "bookViews",
    ] {
        assert!(
            result
                .diagnostics
                .iter()
                .any(|d| d.message.contains(feature)),
            "missing diagnostic for {feature}"
        );
    }
}

fn package_with_styles(styles: &str) -> Vec<u8> {
    let mut parts = read_package(&package(
        "<sheetData><row r=\"1\"><c r=\"A1\" s=\"0\"><v>42</v></c></row></sheetData>",
        "",
    ))
    .unwrap();
    let path = "xl/_rels/workbook.xml.rels";
    let rels = String::from_utf8(parts[path].clone()).unwrap().replace(
        "</Relationships>",
        &format!("<Relationship Id=\"styles\" Type=\"{REL}/styles\" Target=\"styles.xml\"/></Relationships>"),
    );
    parts.insert(path.into(), rels.into_bytes());
    parts.insert("xl/styles.xml".into(), styles.as_bytes().to_vec());
    write_package(&parts).unwrap()
}

#[test]
fn unsupported_style_metadata_is_diagnosed_while_direct_style_and_value_survive() {
    let styles = format!(
        "<styleSheet xmlns=\"{NS}\"><numFmts count=\"1\"><numFmt numFmtId=\"164\" formatCode=\"0.00\"/></numFmts><fonts count=\"1\"><font><b/><i/><color rgb=\"FF1266AA\"/></font></fonts><fills count=\"1\"><fill><patternFill patternType=\"none\"/></fill></fills><cellStyleXfs count=\"1\"><xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"0\"><alignment textRotation=\"90\"/></xf></cellStyleXfs><cellXfs count=\"1\"><xf numFmtId=\"164\" fontId=\"0\" fillId=\"0\" xfId=\"0\" quotePrefix=\"1\" applyFont=\"0\"><alignment wrapText=\"1\" textRotation=\"45\"/><protection locked=\"0\"/></xf></cellXfs><dxfs count=\"1\"><dxf><font><b/></font></dxf></dxfs></styleSheet>"
    );
    let imported = Workbook::from_xlsx(&package_with_styles(&styles)).unwrap();
    let address = CellAddress::parse("A1").unwrap();
    assert_eq!(
        imported.workbook.value(0, address).unwrap(),
        WorkbookValue::Scalar(CellValue::Integer(42))
    );
    let style = &imported.workbook.sheets()[0].cell(address).unwrap().style;
    assert!(style.bold && style.italic && style.wrap);
    assert_eq!(style.foreground, Some(0x1266aa));
    assert_eq!(style.number_format, "0.00");
    for feature in [
        "protection",
        "quotePrefix",
        "applyFont",
        "cellStyleXfs",
        "textRotation",
        "dxfs",
    ] {
        assert!(
            imported
                .diagnostics
                .iter()
                .any(|d| d.part == "xl/styles.xml" && d.message.contains(feature)),
            "missing diagnostic for {feature}: {:?}",
            imported.diagnostics
        );
    }
}

#[test]
fn foreign_style_elements_and_color_modifiers_are_not_silently_treated_as_supported() {
    let styles = format!(
        "<styleSheet xmlns=\"{NS}\" xmlns:x=\"urn:foreign\"><fonts count=\"1\"><font><x:b/><color rgb=\"FF1266AA\" tint=\"0.5\"/></font></fonts><cellXfs count=\"1\"><xf fontId=\"0\"><x:alignment wrapText=\"1\"/></xf></cellXfs></styleSheet>"
    );
    let imported = Workbook::from_xlsx(&package_with_styles(&styles)).unwrap();
    let style = &imported.workbook.sheets()[0]
        .cell(CellAddress::parse("A1").unwrap())
        .unwrap()
        .style;
    assert!(!style.bold && !style.wrap);
    assert_eq!(style.foreground, Some(0x1266aa));
    for feature in ["font b", "tint", "alignment"] {
        assert!(
            imported
                .diagnostics
                .iter()
                .any(|d| d.message.contains(feature)),
            "{feature}"
        );
    }
}

#[test]
fn duplicate_style_tables_and_duplicate_cell_alignment_reject_import() {
    for content in [
        "<cellXfs><xf/></cellXfs><cellXfs><xf/></cellXfs>",
        "<cellXfs><xf><alignment/><alignment/></xf></cellXfs>",
    ] {
        let styles = format!("<styleSheet xmlns=\"{NS}\">{content}</styleSheet>");
        assert!(Workbook::from_xlsx(&package_with_styles(&styles)).is_err());
    }
}
