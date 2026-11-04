use super::*;

#[test]
fn native_gdi_polygon_winding_clip_and_dc_restoration() {
    unsafe {
        let dc = Dc(CreateCompatibleDC(null_mut()));
        assert!(!dc.0.is_null());
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: 64,
                biHeight: -64,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                ..zeroed()
            },
            ..zeroed()
        };
        let mut pixels = null_mut();
        let bitmap = Gdi(CreateDIBSection(
            dc.0,
            &info,
            DIB_RGB_COLORS,
            &mut pixels,
            null_mut(),
            0,
        ));
        assert!(!bitmap.0.is_null());
        let old = SelectObject(dc.0, bitmap.0);
        let result = std::panic::catch_unwind(|| {
            let square = |x: f32, size: f32| {
                vec![
                    Point::new(x, x),
                    Point::new(x + size, x),
                    Point::new(x + size, x + size),
                    Point::new(x, x + size),
                ]
            };
            for rule in [FillRule::NonZero, FillRule::EvenOdd] {
                assert_ne!(PatBlt(dc.0, 0, 0, 64, 64, BLACKNESS), 0);
                let mut scene = Scene::default();
                scene
                    .fill_path(FillPath {
                        contours: vec![square(4.0, 48.0), square(16.0, 16.0)],
                        clip: Rect::new(8.0, 0.0, 48.0, 56.0),
                        rule,
                        color: Color::rgb(255, 0, 0),
                    })
                    .unwrap();
                let old_mode = GetPolyFillMode(dc.0);
                let old_brush = GetCurrentObject(dc.0, OBJ_BRUSH as u32);
                let old_pen = GetCurrentObject(dc.0, OBJ_PEN as u32);
                draw(dc.0, &scene, DrawCommand::FillPath(0), Point::new(1.0, 1.0)).unwrap();
                assert_eq!(GetPixel(dc.0, 6, 12), 0);
                assert_eq!(GetPixel(dc.0, 12, 12), 255);
                assert_eq!(
                    GetPixel(dc.0, 24, 24),
                    if rule == FillRule::NonZero { 255 } else { 0 }
                );
                assert_eq!(GetPolyFillMode(dc.0), old_mode);
                assert_eq!(GetCurrentObject(dc.0, OBJ_BRUSH as u32), old_brush);
                assert_eq!(GetCurrentObject(dc.0, OBJ_PEN as u32), old_pen);
            }
        });
        SelectObject(dc.0, old);
        if let Err(error) = result {
            std::panic::resume_unwind(error);
        }
    }
}
