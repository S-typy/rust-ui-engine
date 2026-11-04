use super::*;
use rust_desktop_ui_core::{
    FillPath, FillRule, ImagePrimitive, ImageSampling, Point, StrokeSegment, TextRun,
};

fn block_on<F: std::future::Future>(future: F) -> F::Output {
    struct Wake(std::thread::Thread);
    impl std::task::Wake for Wake {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }
    }
    let waker = std::task::Waker::from(Arc::new(Wake(std::thread::current())));
    let mut cx = std::task::Context::from_waker(&waker);
    let mut future = std::pin::pin!(future);
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    loop {
        if let std::task::Poll::Ready(value) = future.as_mut().poll(&mut cx) {
            return value;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "GPU initialization deadline"
        );
        std::thread::park_timeout(Duration::from_millis(50));
    }
}

#[test]
#[ignore = "requires a native hardware GPU"]
fn offscreen_gpu_shared_scene_alpha_clip_cache_and_stride() {
    let mut renderer = block_on(OffscreenRenderer::new()).unwrap();
    println!("offscreen hardware: {:?}", renderer.adapter_info());
    let clip = Rect::new(0.0, 0.0, 97.0, 96.0);
    let mut scene = Scene::default();
    let square = |x: f32, y: f32, s: f32| {
        vec![
            Point::new(x, y),
            Point::new(x + s, y),
            Point::new(x + s, y + s),
            Point::new(x, y + s),
        ]
    };
    scene.text(TextRun::new(
        "PDF",
        Rect::new(2.0, 0.0, 90.0, 20.0),
        Color::rgb(0, 0, 0),
        12.0,
    ));
    scene
        .fill_path(FillPath {
            contours: vec![square(8.0, 20.0, 32.0), square(16.0, 28.0, 16.0)],
            clip,
            rule: FillRule::EvenOdd,
            color: Color {
                a: 0.5,
                ..Color::rgb(255, 0, 0)
            },
        })
        .unwrap();
    scene.fill(Rect::new(60.0, 20.0, 20.0, 20.0), Color::rgb(0, 255, 0));
    scene.rounded_fill(
        Rect::new(60.0, 45.0, 20.0, 10.0),
        clip,
        5.0,
        Color::rgb(0, 0, 255),
    );
    scene.ellipse(
        Rect::new(12.0, 70.0, 20.0, 10.0),
        clip,
        None,
        Color::rgb(0, 0, 255),
    );
    scene.stroke_path(
        &[StrokeSegment {
            start: Point::new(50.0, 80.0),
            end: Point::new(80.0, 80.0),
        }],
        clip,
        2.0,
        Color::rgb(0, 0, 0),
    );
    let bitmap = Bitmap::new_rgba8(
        2,
        2,
        vec![255, 0, 0, 255, 0, 255, 0, 0, 0, 0, 255, 128, 0, 255, 0, 255],
    )
    .unwrap();
    let mut image = ImagePrimitive::new(bitmap, Rect::new(60.0, 58.0, 24.0, 16.0));
    image.sampling = ImageSampling::Nearest;
    scene.image(image);
    let saved = scene.clone();
    let mut last_id = None;
    for (index, scale) in [1.0_f32, 1.5, 2.0].into_iter().enumerate() {
        let size = [(97.0 * scale).ceil() as u32, (96.0 * scale).ceil() as u32];
        let bitmap = renderer.render(&scene, size, scale).unwrap();
        assert_eq!((bitmap.width(), bitmap.height()), (size[0], size[1]));
        assert_ne!(last_id, Some(bitmap.id()));
        last_id = Some(bitmap.id());
        let pixel = |x: f32, y: f32| {
            let offset = ((y * scale) as usize * size[0] as usize + (x * scale) as usize) * 4;
            &bitmap.pixels()[offset..offset + 4]
        };
        for (label, x, y, expected) in [
            ("hole", 24.0, 36.0, [255, 255, 255, 255]),
            ("linear alpha", 12.0, 24.0, [255, 188, 188, 255]),
            ("rectangle", 70.0, 30.0, [0, 255, 0, 255]),
            ("rounded center", 70.0, 50.0, [0, 0, 255, 255]),
            ("rounded corner", 60.0, 45.0, [255, 255, 255, 255]),
            ("ellipse", 22.0, 75.0, [0, 0, 255, 255]),
            ("stroke", 60.0, 80.0, [0, 0, 0, 255]),
            ("opaque image", 66.0, 61.0, [255, 0, 0, 255]),
            ("transparent image", 78.0, 61.0, [255, 255, 255, 255]),
            ("half image", 66.0, 70.0, [187, 187, 255, 255]),
        ] {
            let actual = pixel(x, y);
            assert!(
                actual
                    .iter()
                    .zip(expected)
                    .all(|(&a, b)| i16::from(a).abs_diff(b) <= 2),
                "{scale} {label}: {actual:?}"
            );
            println!("PASS {scale} {label}: {actual:?}");
        }
        assert!(bitmap.pixels().chunks_exact(4).all(|p| p[3] == 255));
        let stats = renderer.stats();
        assert_eq!(stats.fill_paths, 1);
        assert_eq!(stats.images, 1);
        assert!(stats.glyphs > 0);
        assert_eq!(stats.missing_glyphs, 0);
        assert_eq!(stats.uploaded_images, usize::from(index == 0));
        assert_eq!(scene, saved);
        assert!(renderer.render(&scene, [0, 1], scale).is_err());
    }
}
