use super::*;

#[test]
fn output_budgets_reject_opacity_excess_and_explosive_image_crop() {
    let mut scene = Scene::default();
    for index in 1..=1025 {
        scene.fill(
            Rect::new(0.0, 0.0, 1.0, 1.0),
            Color {
                a: index as f32 / 2048.0,
                ..Color::rgb(0, 0, 255)
            },
        );
    }
    let document = |scene| {
        PrintDocument::new(
            "budget",
            vec![PrintPage {
                size: Size::new(100.0, 100.0),
                scene,
            }],
        )
        .unwrap()
    };
    assert!(matches!(
        export_pdf(&document(scene), &mut TextEngine::new(), || false),
        Err(PdfError::Limit(_))
    ));
    let mut image = ImagePrimitive::new(
        Bitmap::new_rgba8(1, 1, vec![255; 4]).unwrap(),
        Rect::new(0.0, 0.0, 100.0, 100.0),
    );
    image.source = Rect::new(0.0, 0.0, 1e-20, 1e-20);
    let mut scene = Scene::default();
    scene.image(image);
    assert!(matches!(
        export_pdf(&document(scene), &mut TextEngine::new(), || false),
        Err(PdfError::Unsupported(_))
    ));
}

#[test]
fn polygon_export_preserves_contours_winding_and_clip_without_rasterization() {
    let mut scene = Scene::default();
    let path = FillPath {
        contours: vec![
            vec![
                Point::new(0.0, 0.0),
                Point::new(80.0, 0.0),
                Point::new(0.0, 80.0),
            ],
            vec![
                Point::new(10.0, 10.0),
                Point::new(20.0, 10.0),
                Point::new(10.0, 20.0),
            ],
        ],
        clip: Rect::new(5.0, 5.0, 60.0, 60.0),
        rule: FillRule::EvenOdd,
        color: Color::rgb(20, 40, 60),
    };
    scene.fill_path(path.clone()).unwrap();
    let source = PrintDocument::new(
        "Contours",
        vec![PrintPage {
            size: Size::new(100.0, 100.0),
            scene,
        }],
    )
    .unwrap();
    let mut engine = TextEngine::new();
    let prepared = prepare_for_printer(&source, &mut engine, 1.5, || false).unwrap();
    assert_eq!(prepared.pages()[0].scene.fill_paths, [path]);
    let bytes = export_pdf(&source, &mut engine, || false).unwrap();
    let doc = Document::load_mem(&bytes).unwrap();
    let page = doc.get_pages()[&1];
    let content = Content::decode(&doc.get_page_content(page)).unwrap();
    let ops = &content.operations;
    assert_eq!(ops.iter().filter(|o| o.operator == "m").count(), 2);
    assert_eq!(ops.iter().filter(|o| o.operator == "h").count(), 2);
    assert_eq!(ops.iter().filter(|o| o.operator == "f*").count(), 1);
    assert!(ops.iter().any(|o| o.operator == "W"));
    assert!(!ops.iter().any(|o| o.operator == "Do"));
    let points: Vec<_> = ops
        .iter()
        .filter(|o| o.operator == "m")
        .map(|o| {
            o.operands
                .iter()
                .map(|v| match v {
                    Object::Integer(n) => *n as f32,
                    Object::Real(n) => *n,
                    _ => panic!("Expected numeric coordinate"),
                })
                .collect::<Vec<_>>()
        })
        .collect();
    // Logical pixels at 96 DPI become PDF points at 72 DPI.
    assert_eq!(points[0], vec![0.0, 75.0]);
    assert_eq!(points[1], vec![7.5, 67.5]);
}
