//! PDF path semantics and bounded graphics-state regressions.
use lopdf::{Document, Object, Stream, dictionary};
use rust_desktop_ui_controls::pdf::{PdfDocument, PdfError};
use rust_desktop_ui_core::{Color, FillRule, ImageSampling, Point};

fn fixture(content: &str) -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let pages = doc.new_object_id();
    let image = doc.add_object(Stream::new(
        dictionary! {
            "Type"=>"XObject", "Subtype"=>"Image", "Width"=>1, "Height"=>1,
            "ColorSpace"=>"DeviceRGB", "BitsPerComponent"=>8
        },
        vec![255, 0, 0],
    ));
    let stream = doc.add_object(Stream::new(
        lopdf::Dictionary::new(),
        content.as_bytes().to_vec(),
    ));
    let page = doc.add_object(dictionary! {
        "Type"=>"Page", "Parent"=>pages, "MediaBox"=>vec![0.into(),0.into(),144.into(),144.into()],
        "Contents"=>stream, "Resources"=>dictionary!{
            "ExtGState"=>dictionary!{
                "Half"=>dictionary!{"ca"=>0.5, "CA"=>0.25, "BM"=>"Normal", "AIS"=>false},
                "Bad"=>dictionary!{"ca"=>2},
                "Blend"=>dictionary!{"BM"=>"Multiply"},
                "Round"=>dictionary!{"LW"=>3, "LC"=>1,"LJ"=>1}
            },
            "XObject"=>dictionary!{"Pixel"=>image}
        }
    });
    doc.objects.insert(
        pages,
        dictionary! {"Type"=>"Pages","Kids"=>vec![Object::Reference(page)],"Count"=>1}.into(),
    );
    let catalog = doc.add_object(dictionary! {"Type"=>"Catalog", "Pages"=>pages});
    doc.trailer.set("Root", catalog);
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).unwrap();
    bytes
}

#[test]
fn compound_paths_keep_winding_and_implicit_close() {
    let outer = "12 12 96 96 re 36 30 48 48 re";
    let odd = PdfDocument::open(&fixture(&format!("{outer} f*"))).unwrap();
    let filled = PdfDocument::open(&fixture(&format!("{outer} f"))).unwrap();
    let point = Point::new(80.0, 112.0);
    let path = &odd.pages()[0].content.scene.fill_paths[0];
    assert_eq!(path.rule, FillRule::EvenOdd);
    assert!(path.contains(Point::new(24.0, 160.0)));
    assert!(!path.contains(point));
    assert!(filled.pages()[0].content.scene.fill_paths[0].contains(point));
    let reversed = PdfDocument::open(&fixture("12 12 96 96 re 84 36 -48 48 re f")).unwrap();
    assert!(!reversed.pages()[0].content.scene.fill_paths[0].contains(point));
    let triangle = PdfDocument::open(&fixture("12 12 m 108 12 l 60 108 l f")).unwrap();
    assert!(triangle.pages()[0].content.scene.fill_paths[0].contains(point));
    assert!(odd.diagnostics().is_empty());
}

#[test]
fn cubic_abbreviations_match_explicit_control_points() {
    for (short, expanded) in [
        ("72 120 120 12 v", "12 12 72 120 120 12 c"),
        ("72 120 120 12 y", "72 120 120 12 120 12 c"),
    ] {
        let make = |path| PdfDocument::open(&fixture(&format!("12 12 m {path} h f"))).unwrap();
        let a = make(short);
        let b = make(expanded);
        let a = &a.pages()[0].content.scene.fill_paths[0];
        let b = &b.pages()[0].content.scene.fill_paths[0];
        assert_eq!(a.contours, b.contours);
        assert!(a.contours[0].len() > 4);
    }
    // Collinear controls beyond the endpoint must not collapse to a zero chord.
    let overshoot = PdfDocument::open(&fixture("12 12 m 120 12 120 72 12 12 c h f")).unwrap();
    assert!(
        overshoot.pages()[0].content.scene.fill_paths[0]
            .bounds()
            .unwrap()
            .width
            > 70.0
    );
}

#[test]
fn opacity_survives_color_changes_and_q_restore_and_applies_to_images() {
    let pdf=PdfDocument::open(&fixture("q /Half gs 1 0 0 rg 12 12 12 12 re f 0.5 g 36 12 12 12 re f /Round gs 1 0 0 RG 12 60 m 60 60 l S q 24 0 0 24 72 72 cm /Pixel Do Q Q 0 1 0 rg 60 12 12 12 re f")).unwrap();
    assert!(pdf.diagnostics().is_empty(), "{:?}", pdf.diagnostics());
    let scene = &pdf.pages()[0].content.scene;
    assert_eq!(
        scene.rectangles[1].color,
        Color {
            a: 0.5,
            ..Color::rgb(255, 0, 0)
        }
    );
    assert_eq!(scene.rectangles[2].color.a, 0.5);
    assert_eq!(scene.rectangles[3].color, Color::rgb(0, 255, 0));
    assert_eq!(scene.stroke_paths[0].color.a, 0.25);
    assert_eq!(scene.stroke_paths[0].width, 4.0);
    assert_eq!(scene.images[0].opacity, 0.5);
    assert_eq!(scene.images[0].sampling, ImageSampling::Nearest);
}

#[test]
fn clip_is_applied_after_paint_and_current_path_is_not_graphics_state() {
    let pdf = PdfDocument::open(&fixture(
        "12 12 24 24 re W 12 w 1 J 1 j S 0 1 0 rg 0 0 144 144 re f",
    ))
    .unwrap();
    let scene = &pdf.pages()[0].content.scene;
    assert_eq!(scene.stroke_paths[0].clip.width, 192.0);
    assert_eq!(scene.rectangles[1].bounds.width, 32.0);
    let pdf = PdfDocument::open(&fixture("12 12 m q 60 12 l Q 60 60 l f")).unwrap();
    assert_eq!(
        pdf.pages()[0].content.scene.fill_paths[0].contours[0].len(),
        3
    );
    let pdf = PdfDocument::open(&fixture("12 12 m 60 12 l 60 60 l s")).unwrap();
    assert_eq!(
        pdf.pages()[0].content.scene.stroke_paths[0].segments.len(),
        3
    );
}

#[test]
fn malformed_excessive_or_unsupported_graphics_fail_explicitly() {
    for content in [
        "1 1 l",
        "1 1 2 2 v",
        "/Bad gs",
        "3 J",
        "1.5 j",
        "-1 w",
        "0 M",
    ] {
        assert!(
            matches!(
                PdfDocument::open(&fixture(content)),
                Err(PdfError::Invalid(_))
            ),
            "{content}"
        );
    }
    for content in [
        "/Blend gs",
        "0 0 m 100 0 l 50 50 l h W n",
        "/Half gs 12 12 24 24 re B",
    ] {
        assert!(
            matches!(
                PdfDocument::open(&fixture(content)),
                Err(PdfError::Unsupported(_))
            ),
            "{content}"
        );
    }
    let content = (0..257).map(|i| format!("{i} 0 m ")).collect::<String>();
    assert!(matches!(
        PdfDocument::open(&fixture(&content)),
        Err(PdfError::Limit(_))
    ));
    let mut content = String::from("/Half gs /Round gs 12 12 m ");
    for n in 1..=17 {
        content.push_str(&format!("{} 24 l ", 12 + n));
    }
    content.push('S');
    assert!(matches!(
        PdfDocument::open(&fixture(&content)),
        Err(PdfError::Unsupported(_))
    ));
}

fn forms_fixture(cyclic: bool, content: &[u8]) -> Vec<u8> {
    let mut doc =
        Document::load_mem(&fixture("q /Half gs /Tile Do Q 0 1 0 rg 120 12 12 12 re f")).unwrap();
    let page = *doc.get_pages().values().next().unwrap();
    let tile = doc.new_object_id();
    let child=doc.add_object(Stream::new(dictionary!{
        "Type"=>"XObject", "Subtype"=>"Form", "BBox"=>vec![0.into(),0.into(),24.into(),24.into()],
        "Matrix"=>vec![2.into(),0.into(),0.into(),2.into(),12.into(),12.into()],
        "Resources"=>dictionary!{"ExtGState"=>dictionary!{"Half"=>dictionary!{"ca"=>0.25}}}
    },b"/Half gs 1 0 0 rg -12 -12 48 48 re f".to_vec()));
    doc.objects.insert(tile,Stream::new(dictionary!{
        "Type"=>"XObject", "Subtype"=>"Form", "BBox"=>vec![0.into(),0.into(),96.into(),96.into()],
        "Resources"=>dictionary!{"XObject"=>dictionary!{"Child"=>if cyclic {tile} else {child}},
            "ExtGState"=>dictionary!{"Half"=>dictionary!{"ca"=>0.75}}}
    },content.to_vec()).into());
    doc.get_object_mut(page)
        .unwrap()
        .as_dict_mut()
        .unwrap()
        .get_mut(b"Resources")
        .unwrap()
        .as_dict_mut()
        .unwrap()
        .get_mut(b"XObject")
        .unwrap()
        .as_dict_mut()
        .unwrap()
        .set("Tile", tile);
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).unwrap();
    bytes
}

#[test]
fn nested_forms_keep_local_resources_bbox_and_parent_state() {
    let pdf = PdfDocument::open(&forms_fixture(
        false,
        b"/Half gs /Child Do 0 0 1 rg 60 60 12 12 re f",
    ))
    .unwrap();
    assert!(pdf.diagnostics().is_empty(), "{:?}", pdf.diagnostics());
    let shapes = &pdf.pages()[0].content.scene.rectangles;
    assert_eq!(shapes.len(), 4);
    assert_eq!(
        shapes[1].color,
        Color {
            a: 0.25,
            ..Color::rgb(255, 0, 0)
        }
    );
    assert_eq!(shapes[1].bounds.width, 64.0);
    assert_eq!(shapes[1].bounds.height, 64.0);
    assert_eq!(shapes[1].bounds.x, 16.0);
    assert_eq!(
        shapes[2].color,
        Color {
            a: 0.75,
            ..Color::rgb(0, 0, 255)
        }
    );
    assert_eq!(shapes[3].color, Color::rgb(0, 255, 0));
    let saved = pdf.save().unwrap();
    let reopened = PdfDocument::open(&saved).unwrap();
    assert_eq!(reopened.pages()[0].content.scene.rectangles, *shapes);
}

#[test]
fn form_cycles_and_cross_boundary_graphics_state_are_rejected() {
    assert!(matches!(
        PdfDocument::open(&forms_fixture(true, b"/Child Do")),
        Err(PdfError::Invalid(_))
    ));
    for content in [b"Q q".as_slice(), b"q", b"EMC BMC", b"/Span BMC"] {
        assert!(matches!(
            PdfDocument::open(&forms_fixture(false, content)),
            Err(PdfError::Invalid(_))
        ));
    }
    let commands = "/Child Do ".repeat(4097);
    assert!(matches!(
        PdfDocument::open(&forms_fixture(false, commands.as_bytes())),
        Err(PdfError::Limit(_))
    ));
}

#[test]
fn named_actualtext_properties_are_scoped_and_amplification_is_bounded() {
    let make = |content: &str, text: &str| {
        let mut doc = Document::load_mem(&fixture(content)).unwrap();
        let id = *doc.get_pages().values().next().unwrap();
        doc.get_object_mut(id)
            .unwrap()
            .as_dict_mut()
            .unwrap()
            .get_mut(b"Resources")
            .unwrap()
            .as_dict_mut()
            .unwrap()
            .set(
                "Properties",
                dictionary! {"Title"=>dictionary!{"ActualText"=>lopdf::text_string(text)}},
            );
        let mut bytes = Vec::new();
        doc.save_to(&mut bytes).unwrap();
        bytes
    };
    let bytes = make("/Span /Title BDC 12 12 12 12 re f EMC", "Сводка");
    assert_eq!(
        PdfDocument::open(&bytes).unwrap().pages()[0].text[0].text,
        "Сводка"
    );
    let amplified = make(
        &"/Span /Title BDC EMC ".repeat(300),
        &"x".repeat(128 * 1024),
    );
    assert!(matches!(
        PdfDocument::open(&amplified),
        Err(PdfError::Limit(_))
    ));
}

#[test]
fn dense_uniform_borders_use_border_geometry_without_losing_clip_or_opacity() {
    let mut content = String::from("/Half gs 0 0 1 rg ");
    for _ in 0..600 {
        content.push_str("12 12 24 24 re 15 15 18 18 re f* ");
    }
    let doc = PdfDocument::open(&fixture(&content)).unwrap();
    let scene = &doc.pages()[0].content.scene;
    assert!(scene.fill_paths.is_empty());
    assert_eq!(scene.rounded_rectangles.len(), 600);
    let border = &scene.rounded_rectangles[0];
    assert_eq!(border.bounds.width, 32.0);
    assert_eq!(border.stroke_width, Some(4.0));
    assert_eq!(
        border.color,
        Color {
            a: 0.5,
            ..Color::rgb(0, 0, 255)
        }
    );
    assert_eq!(border.clip.width, 192.0);
    // A nonuniform hole must keep its general winding geometry.
    let doc = PdfDocument::open(&fixture("12 12 24 24 re 15 18 18 15 re f*")).unwrap();
    assert_eq!(doc.pages()[0].content.scene.fill_paths.len(), 1);
}
