use super::*;
use rust_desktop_ui_core::{Point, Rect, Scene};

#[test]
fn ean_checksum_canonical_data_and_exact_module_envelopes() {
    for (kind, short, full, modules, quiet) in [
        (
            BarcodeSymbology::Ean13,
            "400638133393",
            "4006381333931",
            95,
            (11, 7),
        ),
        (BarcodeSymbology::Ean8, "9638507", "96385074", 67, (7, 7)),
    ] {
        let a = BarcodeSymbol::encode(kind, short).unwrap();
        let b = BarcodeSymbol::encode(kind, full).unwrap();
        assert_eq!(a, b);
        assert_eq!(a.data(), full);
        assert_eq!(a.modules(), modules);
        assert_eq!(a.quiet_zones(), quiet);
        assert_eq!(a.bars().iter().filter(|bar| bar.guard).count(), 6);
        let mut invalid = full.to_owned();
        invalid.pop();
        invalid.push(if full.ends_with('0') { '1' } else { '0' });
        assert!(BarcodeSymbol::encode(kind, &invalid).is_err());
    }
    for text in [
        "",
        "40063813339",
        "40063813339311",
        "４00638133393",
        "40063813339 ",
        "-00638133393",
    ] {
        assert!(BarcodeSymbol::encode(BarcodeSymbology::Ean13, text).is_err());
    }
}
#[test]
fn code128_start_checksum_stop_and_character_boundaries() {
    let a = BarcodeSymbol::encode(BarcodeSymbology::Code128B, "AB").unwrap();
    assert_eq!(a.modules(), 57);
    assert_eq!(a.total_modules(), 77);
    assert_eq!(
        a.bars()[0],
        BarcodeBar {
            start: 0,
            width: 2,
            guard: false
        }
    );
    // Start B=104; A=33, B=34; (104+33+2*34)%103=102.
    let checksum = &a.bars()[9..12];
    assert_eq!(
        checksum.iter().map(|b| b.width).collect::<Vec<_>>(),
        [4, 1, 3]
    );
    assert_eq!(a.bars().last().unwrap().start, 55);
    assert_eq!(a.bars().last().unwrap().width, 2);
    let all = (32..=126).map(char::from).collect::<String>();
    assert!(BarcodeSymbol::encode(BarcodeSymbology::Code128B, &all).is_ok());
    assert!(BarcodeSymbol::encode(BarcodeSymbology::Code128B, &"x".repeat(128)).is_ok());
    for text in [
        String::new(),
        "x".repeat(129),
        "line\n".into(),
        "é".into(),
        "\u{7f}".into(),
    ] {
        assert!(BarcodeSymbol::encode(BarcodeSymbology::Code128B, &text).is_err());
    }
}
#[test]
fn geometry_quiet_zones_guard_height_scaling_and_rejection_are_exact() {
    let symbol = BarcodeSymbol::encode(BarcodeSymbology::Ean13, "590123412345").unwrap();
    for pitch in [1, 2, 3, 5, 8, 16] {
        let geometry = BarcodeGeometry::new(
            &symbol,
            BarcodeGeometryOptions {
                module_size: pitch,
                bar_height: 64,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(geometry.width(), 113 * u32::from(pitch));
        assert_eq!(geometry.height(), 64 + 5 * u32::from(pitch));
        assert_eq!(geometry.bars()[0].x, 11 * u32::from(pitch));
        let last = geometry.bars().last().unwrap();
        assert_eq!(geometry.width() - last.x - last.width, 7 * u32::from(pitch));
        for (module, bar) in symbol.bars().iter().zip(geometry.bars()) {
            assert_eq!(bar.x, (11 + u32::from(module.start)) * u32::from(pitch));
            assert_eq!(bar.width, u32::from(module.width) * u32::from(pitch));
            assert_eq!(
                bar.height,
                64 + if module.guard {
                    5 * u32::from(pitch)
                } else {
                    0
                }
            );
        }
    }
    for options in [
        BarcodeGeometryOptions {
            module_size: 0,
            ..Default::default()
        },
        BarcodeGeometryOptions {
            quiet_left: Some(10),
            ..Default::default()
        },
        BarcodeGeometryOptions {
            quiet_right: Some(6),
            ..Default::default()
        },
        BarcodeGeometryOptions {
            bar_height: u32::MAX,
            ..Default::default()
        },
        BarcodeGeometryOptions {
            module_size: u16::MAX,
            ..Default::default()
        },
    ] {
        assert!(BarcodeGeometry::new(&symbol, options).is_err());
    }
}
#[test]
fn scene_and_vector_export_share_full_geometry_and_partial_clips_are_atomic() {
    let symbol = BarcodeSymbol::encode(BarcodeSymbology::Ean8, "9638507").unwrap();
    let geometry = BarcodeGeometry::new(&symbol, Default::default()).unwrap();
    let mut scene = Scene::default();
    geometry
        .append_scene(
            &mut scene,
            Point::new(13.0, 7.0),
            1.5,
            Rect::new(0.0, 0.0, 1000.0, 500.0),
        )
        .unwrap();
    assert_eq!(scene.rectangles.len(), geometry.bars().len() + 1);
    for (bar, drawn) in geometry.bars().iter().zip(&scene.rectangles[1..]) {
        assert_eq!(
            drawn.bounds,
            Rect::new(
                13.0 + bar.x as f32 * 1.5,
                7.0,
                bar.width as f32 * 1.5,
                bar.height as f32 * 1.5
            )
        );
    }
    let old = scene.clone();
    assert!(
        geometry
            .append_scene(
                &mut scene,
                Point::new(0.0, 0.0),
                1.0,
                Rect::new(0.0, 0.0, 100.0, 100.0)
            )
            .is_err()
    );
    assert_eq!(scene, old);
    for origin in [Point::new(1e30, 0.0), Point::new(0.0, 1e30)] {
        assert!(
            geometry
                .append_scene(&mut scene, origin, 1.0, Rect::new(0.0, 0.0, 1e32, 1e32))
                .is_err()
        );
        assert_eq!(scene, old);
    }
    let svg = geometry.to_svg();
    assert_eq!(svg.matches("<rect ").count(), geometry.bars().len() + 1);
    assert!(svg.contains("fill=\"#fff\""));
    assert!(!svg.contains("<image"));
}
#[test]
fn printer_pitch_is_one_integral_grid_and_reports_actual_physical_size() {
    let symbol = BarcodeSymbol::encode(BarcodeSymbology::Code128B, "Print-128").unwrap();
    for dpi in [96, 203, 300, 600, 1200] {
        let layout = BarcodePrintLayout::new(
            &symbol,
            BarcodePrintOptions {
                dpi,
                ..Default::default()
            },
        )
        .unwrap();
        let pitch = u32::from(layout.geometry.module_size());
        assert!(
            layout
                .geometry
                .bars()
                .iter()
                .all(|r| r.x % pitch == 0 && r.width % pitch == 0)
        );
        assert_eq!(
            layout.actual_module_mm,
            f64::from(pitch) * 25.4 / f64::from(dpi)
        );
    }
    assert!(
        BarcodePrintLayout::new(
            &symbol,
            BarcodePrintOptions {
                dpi: 72,
                module_mm: 0.05,
                ..Default::default()
            }
        )
        .is_err()
    );
    assert!(
        BarcodePrintLayout::new(
            &symbol,
            BarcodePrintOptions {
                module_mm: f64::NAN,
                ..Default::default()
            }
        )
        .is_err()
    );
}
#[test]
fn raster_has_exact_black_runs_and_rejects_large_area_before_output() {
    let symbol = BarcodeSymbol::encode(BarcodeSymbology::Code128B, "Raster").unwrap();
    let geometry = BarcodeGeometry::new(&symbol, Default::default()).unwrap();
    let mut bytes = Vec::new();
    geometry.write_pgm(&mut bytes).unwrap();
    let header = format!("P5\n{} {}\n255\n", geometry.width(), geometry.height());
    let pixels = &bytes[header.len()..];
    assert_eq!(
        pixels.len(),
        geometry.width() as usize * geometry.height() as usize
    );
    for x in 0..geometry.width() {
        let dark = geometry
            .bars()
            .iter()
            .any(|r| x >= r.x && x < r.x + r.width);
        assert_eq!(pixels[x as usize], if dark { 0 } else { 255 });
    }
    let huge = BarcodeGeometry::new(
        &symbol,
        BarcodeGeometryOptions {
            module_size: 100,
            bar_height: 30_000,
            ..Default::default()
        },
    )
    .unwrap();
    let mut output = Vec::new();
    assert!(huge.write_pgm(&mut output).is_err());
    assert!(output.is_empty());
}
