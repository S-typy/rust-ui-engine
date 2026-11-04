use super::*;

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
