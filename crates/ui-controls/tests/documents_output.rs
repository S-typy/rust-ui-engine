use rust_desktop_ui_controls::{pdf::*, printing::*, reports::*};
use rust_desktop_ui_core::{pages::*, *};
use rust_desktop_ui_data::*;
use rust_desktop_ui_text::TextEngine;
use std::collections::BTreeMap;

fn page() -> PrintDocument {
    let mut scene = Scene::default();
    scene.fill(Rect::new(0.0, 0.0, 400.0, 500.0), Color::rgb(255, 255, 255));
    scene.text(TextRun::new(
        "Привет, PDF! Café",
        Rect::new(24.0, 24.0, 350.0, 32.0),
        Color::rgb(0, 0, 0),
        20.0,
    ));
    scene.fill(Rect::new(30.0, 90.0, 150.0, 30.0), Color::rgb(10, 80, 150));
    PrintDocument::new(
        "Проверка",
        vec![PrintPage {
            size: Size::new(400.0, 500.0),
            scene,
        }],
    )
    .unwrap()
}
#[test]
fn pdf_export_reopens_with_unicode_search_and_visible_images() {
    let pdf = export_pdf(&page(), &mut TextEngine::new(), || false).unwrap();
    let opened = PdfDocument::open(&pdf).unwrap();
    assert_eq!(opened.pages().len(), 1);
    assert!(
        opened.diagnostics().is_empty(),
        "{:?}",
        opened.diagnostics()
    );
    assert!(!opened.pages()[0].content.scene.images.is_empty());
    assert_eq!(opened.search("Привет").unwrap().len(), 1);
    assert_eq!(opened.search("Café").unwrap().len(), 1);
    assert!((opened.pages()[0].content.size.width - 400.0).abs() < 0.01);
}
#[test]
fn annotations_are_transactional_and_survive_save() {
    let bytes = export_pdf(&page(), &mut TextEngine::new(), || false).unwrap();
    let mut pdf = PdfDocument::open(&bytes).unwrap();
    let revision = pdf.revision();
    assert!(
        pdf.add_annotation(0, Rect::new(-1.0, 0.0, 10.0, 10.0), "bad")
            .is_err()
    );
    assert_eq!(pdf.revision(), revision);
    pdf.add_annotation(0, Rect::new(30.0, 40.0, 20.0, 20.0), "Заметка")
        .unwrap();
    let mut loaded = PdfDocument::open(&pdf.save().unwrap()).unwrap();
    assert_eq!(loaded.annotations()[0].text, "Заметка");
    assert!((loaded.annotations()[0].bounds.y - 40.0).abs() < 0.01);
    loaded.remove_annotation(0).unwrap();
    assert!(
        PdfDocument::open(&loaded.save().unwrap())
            .unwrap()
            .annotations()
            .is_empty()
    );
}
#[test]
fn pdf_rejects_invalid_limits_and_cancellation() {
    assert!(PdfDocument::open(b"not a PDF").is_err());
    assert!(matches!(
        PdfDocument::open_with_cancel(&[], || true),
        Err(PdfError::Cancelled)
    ));
    assert!(matches!(
        export_pdf(&page(), &mut TextEngine::new(), || true),
        Err(PdfError::Cancelled)
    ));
    let mut scene = Scene::default();
    let mut text = TextRun::new(
        "secret",
        Rect::new(0.0, 0.0, 50.0, 50.0),
        Color::rgb(0, 0, 0),
        12.0,
    );
    text.sensitive = true;
    scene.text(text);
    assert!(
        PrintDocument::new(
            "protected",
            vec![PrintPage {
                size: Size::new(100.0, 100.0),
                scene
            }]
        )
        .is_err()
    );
}
fn data() -> DataTable {
    DataTable::new(
        TableSchema::new(vec![
            FieldSchema::new("group", FieldType::Text),
            FieldSchema::new("value", FieldType::Integer),
        ])
        .unwrap(),
        (0..80)
            .map(|i| {
                DataRow::new(
                    i,
                    vec![
                        CellValue::Text(if i < 40 { "A" } else { "B" }.into()),
                        CellValue::Integer(i),
                    ],
                )
            })
            .collect(),
    )
    .unwrap()
}
fn item(x: f32, value: ReportValue) -> ReportItem {
    ReportItem::Text {
        x,
        y: 0.0,
        width: 160.0,
        font_size: 14.0,
        bold: false,
        color: Color::rgb(0, 0, 0),
        value,
    }
}
fn definition() -> ReportDefinition {
    ReportDefinition {
        title: "Test report".into(),
        page_size: Size::new(440.0, 400.0),
        margins: [20.0; 4],
        query: QueryRequest {
            group_by: vec![QueryExpression::field("group")],
            aggregates: vec![Aggregate {
                function: AggregateFunction::Sum,
                expression: Some(QueryExpression::field("value")),
            }],
            ..Default::default()
        },
        group_header: ReportBand {
            height: 26.0,
            items: vec![item(0.0, ReportValue::GroupKey(0))],
        },
        detail: ReportBand {
            height: 24.0,
            items: vec![item(
                0.0,
                ReportValue::Expression(QueryExpression::field("value")),
            )],
        },
        group_footer: ReportBand {
            height: 24.0,
            items: vec![item(0.0, ReportValue::GroupTotal(0))],
        },
        report_footer: ReportBand {
            height: 24.0,
            items: vec![item(0.0, ReportValue::Total(0))],
        },
        page_footer: ReportBand {
            height: 24.0,
            items: vec![
                item(0.0, ReportValue::PageNumber),
                item(180.0, ReportValue::PageCount),
            ],
        },
        ..Default::default()
    }
}
#[test]
fn grouped_reports_preserve_exact_totals_and_have_deterministic_pages() {
    let definition = definition();
    let table = data();
    let params = BTreeMap::new();
    let mut engine = TextEngine::new();
    let a = ReportEngine::build(&definition, &table, &params, &mut engine, || false).unwrap();
    let b = ReportEngine::build(&definition, &table, &params, &mut engine, || false).unwrap();
    assert_eq!(a.totals, vec![CellValue::Integer(3160)]);
    assert_eq!(a.groups[0].aggregates, vec![CellValue::Integer(780)]);
    assert!(a.document.pages().len() > 2);
    assert_eq!(a.document.pages().len(), b.document.pages().len());
    for (x, y) in a.document.pages().iter().zip(b.document.pages()) {
        assert_eq!(x.scene, y.scene);
        assert!(
            x.scene
                .texts
                .iter()
                .any(|r| r.text == a.document.pages().len().to_string())
        );
        assert!(!x.scene.texts.iter().any(|r| r.text == "888"));
    }
}
#[test]
fn report_parameter_filter_is_typed_and_invalid_expressions_report_location() {
    let mut d = definition();
    d.parameters = vec![FieldSchema::new("minimum", FieldType::Integer)];
    d.parameter_filters = vec![ReportParameterFilter {
        parameter: "minimum".into(),
        field: "value".into(),
        operator: ComparisonOperator::GreaterOrEqual,
    }];
    let params = BTreeMap::from([("minimum".into(), CellValue::Integer(70))]);
    let r = ReportEngine::build(&d, &data(), &params, &mut TextEngine::new(), || false).unwrap();
    assert_eq!(r.row_keys.len(), 10);
    d.detail.items = vec![item(
        0.0,
        ReportValue::Expression(QueryExpression::field("missing")),
    )];
    let e =
        ReportEngine::build(&d, &data(), &params, &mut TextEngine::new(), || false).unwrap_err();
    assert_eq!(e.band, "detail");
    assert_eq!(e.item, Some(0));
}
#[test]
fn cancelled_and_oversized_reports_do_not_return_partial_output() {
    assert!(
        ReportEngine::build(
            &definition(),
            &data(),
            &BTreeMap::new(),
            &mut TextEngine::new(),
            || true
        )
        .is_err()
    );
    let mut d = definition();
    d.detail.height = 390.0;
    assert!(
        ReportEngine::build(
            &d,
            &data(),
            &BTreeMap::new(),
            &mut TextEngine::new(),
            || false
        )
        .is_err()
    );
}

fn external_pdf() -> Vec<u8> {
    use lopdf::{Document, Object, Stream, dictionary};
    let mut d = Document::with_version("1.7");
    let pages = d.new_object_id();
    let font=d.add_object(dictionary!{"Type"=>"Font","Subtype"=>"Type1","BaseFont"=>"Helvetica","Encoding"=>"WinAnsiEncoding"});
    let content = d.add_object(Stream::new(
        lopdf::Dictionary::new(),
        b"BT /F1 16 Tf 1 0 0 1 30 270 Tm (External PDF fixture) Tj ET\n".to_vec(),
    ));
    let field=d.add_object(dictionary!{"Type"=>"Annot","Subtype"=>"Widget","FT"=>"Tx","T"=>lopdf::text_string("Customer"),
        "V"=>lopdf::text_string("Initial"),"Rect"=>vec![30.into(),100.into(),200.into(),130.into()]});
    let page=d.add_object(dictionary!{"Type"=>"Page","Parent"=>pages,"MediaBox"=>vec![0.into(),0.into(),300.into(),300.into()],
        "Resources"=>dictionary!{"Font"=>dictionary!{"F1"=>font}},"Contents"=>content,"Annots"=>vec![Object::Reference(field)]});
    d.objects.insert(
        pages,
        dictionary! {"Type"=>"Pages","Kids"=>vec![Object::Reference(page)],"Count"=>1}.into(),
    );
    let outlines = d.new_object_id();
    let bookmark=d.add_object(dictionary!{"Title"=>lopdf::text_string("Start"),"Parent"=>outlines,"Dest"=>vec![Object::Reference(page),Object::Name(b"Fit".to_vec())]});
    d.objects.insert(
        outlines,
        dictionary! {"Type"=>"Outlines","First"=>bookmark,"Last"=>bookmark,"Count"=>1}.into(),
    );
    let root = d.add_object(
        dictionary! {"Type"=>"Catalog","Pages"=>pages,"Outlines"=>outlines,
        "AcroForm"=>dictionary!{"Fields"=>vec![Object::Reference(field)]}},
    );
    d.trailer.set("Root", root);
    let mut bytes = Vec::new();
    d.save_to(&mut bytes).unwrap();
    bytes
}

#[test]
fn external_text_fields_and_bookmarks_preserve_original_objects() {
    let mut d = PdfDocument::open(&external_pdf()).unwrap();
    assert_eq!(d.search("External PDF").unwrap().len(), 1);
    assert_eq!(d.fields()[0].name, "Customer");
    assert_eq!(d.bookmarks()[0].page, 0);
    d.set_field(0, "Клиент").unwrap();
    let bytes = d.save().unwrap();
    let loaded = PdfDocument::open(&bytes).unwrap();
    assert_eq!(loaded.fields()[0].value, "Клиент");
    assert_eq!(loaded.search("External PDF").unwrap().len(), 1);
    let raw = lopdf::Document::load_mem(&bytes).unwrap();
    assert!(
        raw.objects
            .values()
            .filter_map(|o| o.as_dict().ok())
            .any(|d| d.has(b"AP"))
    );
}

#[test]
fn corrupt_page_command_is_rejected_before_export() {
    let mut scene = Scene::default();
    scene.fill(Rect::new(0.0, 0.0, 10.0, 10.0), Color::rgb(1, 2, 3));
    scene.rectangles.clear();
    assert!(
        PrintDocument::new(
            "bad",
            vec![PrintPage {
                size: Size::new(100.0, 100.0),
                scene
            }]
        )
        .is_err()
    );
}

#[test]
fn pdf_annotation_uses_media_origin_and_crop_is_diagnosed() {
    use lopdf::Object;
    let mut raw = lopdf::Document::load_mem(&external_pdf()).unwrap();
    let id = *raw.get_pages().values().next().unwrap();
    let d = raw.get_object_mut(id).unwrap().as_dict_mut().unwrap();
    d.set(
        "MediaBox",
        vec![Object::Integer(10), 20.into(), 310.into(), 320.into()],
    );
    d.set(
        "CropBox",
        vec![Object::Integer(20), 30.into(), 300.into(), 310.into()],
    );
    let mut bytes = Vec::new();
    raw.save_to(&mut bytes).unwrap();
    let mut pdf = PdfDocument::open(&bytes).unwrap();
    assert!(
        pdf.diagnostics()
            .iter()
            .any(|d| d.message.contains("CropBox"))
    );
    pdf.add_annotation(0, Rect::new(30.0, 40.0, 20.0, 20.0), "Origin")
        .unwrap();
    let reparsed = PdfDocument::open(&pdf.save().unwrap()).unwrap();
    let bounds = reparsed.annotations()[0].bounds;
    assert!((bounds.x - 30.0).abs() < 0.01);
    assert!((bounds.y - 40.0).abs() < 0.01);
}

#[test]
fn large_report_checks_mid_build_cancellation_and_totals() {
    let original = data();
    let table = DataTable::new(
        original.schema().clone(),
        (0..5000)
            .map(|i| {
                DataRow::new(
                    i,
                    vec![CellValue::Text("Group".into()), CellValue::Integer(i)],
                )
            })
            .collect(),
    )
    .unwrap();
    let mut d = definition();
    d.page_size = Size::new(794.0, 1123.0);
    let built = ReportEngine::build(&d, &table, &BTreeMap::new(), &mut TextEngine::new(), || {
        false
    })
    .unwrap();
    assert_eq!(built.row_keys.len(), 5000);
    assert_eq!(built.totals[0], CellValue::Integer(12_497_500));
    assert!(built.document.pages().len() > 100);
    let mut polls = 0;
    let error = ReportEngine::build(&d, &table, &BTreeMap::new(), &mut TextEngine::new(), || {
        polls += 1;
        polls > 100
    })
    .unwrap_err();
    assert!(error.message.contains("cancelled"));
}

#[test]
fn pdf_cycles_and_compressed_content_budgets_fail_without_partial_document() {
    use lopdf::{Object, Stream};
    let bytes = external_pdf();
    let mut raw = lopdf::Document::load_mem(&bytes).unwrap();
    let root = raw
        .catalog()
        .unwrap()
        .get(b"Pages")
        .unwrap()
        .as_reference()
        .unwrap();
    raw.get_object_mut(root)
        .unwrap()
        .as_dict_mut()
        .unwrap()
        .set("Kids", vec![Object::Reference(root)]);
    let mut cyclic = Vec::new();
    raw.save_to(&mut cyclic).unwrap();
    assert!(PdfDocument::open(&cyclic).is_err());

    let mut raw = lopdf::Document::load_mem(&bytes).unwrap();
    let page = *raw.get_pages().values().next().unwrap();
    let mut stream = Stream::new(lopdf::Dictionary::new(), vec![b' '; 4 * 1024 * 1024 + 1]);
    stream.compress().unwrap();
    let content = raw.add_object(stream);
    raw.get_object_mut(page)
        .unwrap()
        .as_dict_mut()
        .unwrap()
        .set("Contents", content);
    let mut excessive = Vec::new();
    raw.save_to(&mut excessive).unwrap();
    assert!(excessive.len() < 100_000);
    assert!(PdfDocument::open(&excessive).is_err());
}

#[test]
fn pdf_jpeg_image_is_decoded_with_declared_dimensions() {
    use lopdf::{Stream, dictionary};
    let mut bytes = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, 100)
        .encode(
            &[240, 20, 10, 240, 20, 10],
            2,
            1,
            image::ExtendedColorType::Rgb8,
        )
        .unwrap();
    let mut raw = lopdf::Document::load_mem(&external_pdf()).unwrap();
    let image = raw.add_object(Stream::new(
        dictionary! {
            "Type"=>"XObject","Subtype"=>"Image","Width"=>2,"Height"=>1,
            "BitsPerComponent"=>8,"ColorSpace"=>"DeviceRGB","Filter"=>"DCTDecode"
        },
        bytes,
    ));
    let content = raw.add_object(Stream::new(
        lopdf::Dictionary::new(),
        b"q 50 0 0 25 10 10 cm /Im Do Q".to_vec(),
    ));
    let page = *raw.get_pages().values().next().unwrap();
    let d = raw.get_object_mut(page).unwrap().as_dict_mut().unwrap();
    d.set("Contents", content);
    d.set(
        "Resources",
        dictionary! {"XObject"=>dictionary!{"Im"=>image}},
    );
    let mut bytes = Vec::new();
    raw.save_to(&mut bytes).unwrap();
    let loaded = PdfDocument::open(&bytes).unwrap();
    let bitmap = &loaded.pages()[0].content.scene.images[0].bitmap;
    assert_eq!((bitmap.width(), bitmap.height()), (2, 1));
    assert!(bitmap.pixels()[0] > 220 && bitmap.pixels()[1] < 40);
}
