use super::*;

fn source(color: &str, width: u32, height: u32) -> ImageSource {
    ImageSource::from_bytes(format!("<svg xmlns='http://www.w3.org/2000/svg' width='{width}' height='{height}'><rect width='{width}' height='{height}' fill='{color}'/></svg>").into_bytes(), ImageFormat::Svg).unwrap()
}
fn frame(ui: &mut Controls) {
    ui.update(Size::new(600.0, 480.0)).unwrap();
}
fn fixed() -> LayoutStyle {
    LayoutStyle {
        width: Length::Px(100.0),
        height: Length::Px(100.0),
        flex_shrink: 0.0,
        ..Default::default()
    }
}

#[test]
fn image_is_noninteractive_accessible_and_shares_cached_assets() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let source = source("red", 20, 10);
    let first = ui
        .image(
            ui.root(),
            source.clone(),
            ImageSettings::default(),
            fixed(),
            "Diagram",
        )
        .unwrap();
    let second = ui
        .image(
            ui.root(),
            source,
            ImageSettings::default(),
            fixed(),
            "Same diagram",
        )
        .unwrap();
    frame(&mut ui);
    assert_eq!(
        ui.image_asset(first).unwrap().bitmap(),
        ui.image_asset(second).unwrap().bitmap()
    );
    assert_eq!(ui.image_cache_stats().entries, 1);
    assert!(ui.image_cache_stats().hits > 0);
    let node = ui.runtime.tree().node(first).unwrap();
    assert!(!node.props.hit_test);
    assert!(!node.props.focusable);
    let semantic = ui
        .semantics()
        .into_iter()
        .find(|node| node.id == first)
        .unwrap();
    assert_eq!(semantic.role, Role::Image);
    assert_eq!(semantic.label, "Diagram");
    assert_eq!(ui.scene().images.len(), 2);
}

#[test]
fn stretch_preserves_source_mapping_and_clips_uniform_to_fill() {
    for (stretch, width, height) in [
        (ImageStretch::None, 20.0, 10.0),
        (ImageStretch::Fill, 100.0, 100.0),
        (ImageStretch::Uniform, 100.0, 50.0),
        (ImageStretch::UniformToFill, 200.0, 100.0),
    ] {
        let mut ui = Controls::new(Theme::FluentLight).unwrap();
        let id = ui
            .image(
                ui.root(),
                source("red", 20, 10),
                ImageSettings {
                    stretch,
                    sampling: ImageSampling::Nearest,
                    opacity: 0.4,
                    ..Default::default()
                },
                fixed(),
                "Image",
            )
            .unwrap();
        frame(&mut ui);
        let primitive = &ui.scene().images[0];
        let bounds = ui.runtime.bounds(id).unwrap();
        assert_eq!(
            (primitive.bounds.width, primitive.bounds.height),
            (width, height)
        );
        assert_eq!(primitive.bounds.x, bounds.x + (bounds.width - width) / 2.0);
        assert_eq!(primitive.source, Rect::new(0.0, 0.0, 20.0, 10.0));
        assert_eq!(primitive.clip, bounds);
        assert_eq!(primitive.opacity, 0.4);
        assert_eq!(primitive.sampling, ImageSampling::Nearest);
    }
}

#[test]
fn missing_invalid_and_late_resources_have_recoverable_diagnostics() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let id = ui
        .image_from_resource(
            ui.root(),
            "Logo",
            ImageSettings::default(),
            LayoutStyle::default(),
            "Company",
        )
        .unwrap();
    frame(&mut ui);
    assert!(ui.image_error(id).is_some());
    assert!(ui.scene().images.is_empty());
    assert_eq!(size(&ui, id), Size::new(32.0, 32.0));
    ui.register_image_source("Logo", source("red", 20, 10))
        .unwrap();
    frame(&mut ui);
    assert!(ui.image_error(id).is_none());
    assert_eq!(size(&ui, id), Size::new(20.0, 10.0));
    let bitmap = ui.image_asset(id).unwrap().bitmap().clone();
    ui.register_image_source(
        "Logo",
        ImageSource::from_bytes(b"broken".to_vec(), ImageFormat::Svg).unwrap(),
    )
    .unwrap();
    frame(&mut ui);
    assert!(ui.image_error(id).is_some());
    assert_eq!(ui.image_asset(id).unwrap().bitmap(), &bitmap);
    assert!(ui.unregister_image_source("Logo"));
    frame(&mut ui);
    assert!(ui.image_error(id).is_some());
    assert_eq!(ui.image_asset(id).unwrap().bitmap(), &bitmap);
    ui.register_image_source("Logo", source("blue", 40, 30))
        .unwrap();
    frame(&mut ui);
    assert!(ui.image_error(id).is_none());
    assert_ne!(ui.image_asset(id).unwrap().bitmap(), &bitmap);
    assert_eq!(size(&ui, id), Size::new(40.0, 30.0));
}

#[test]
fn svg_theme_and_dpi_follow_without_changing_explicit_or_intrinsic_size() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let source = source("currentColor", 20, 10);
    let intrinsic = ui
        .image(
            ui.root(),
            source.clone(),
            ImageSettings::default(),
            LayoutStyle::default(),
            "Intrinsic",
        )
        .unwrap();
    let explicit = ui
        .image(
            ui.root(),
            source,
            ImageSettings::default(),
            fixed(),
            "Explicit",
        )
        .unwrap();
    frame(&mut ui);
    let before = ui.image_asset(intrinsic).unwrap().bitmap().clone();
    ui.set_theme(Theme::FluentDark).unwrap();
    frame(&mut ui);
    let dark = ui.image_asset(intrinsic).unwrap().bitmap().clone();
    assert_ne!(before, dark);
    assert_ne!(before.pixels(), dark.pixels());
    ui.set_image_scale(2.0).unwrap();
    frame(&mut ui);
    assert_eq!(ui.image_asset(intrinsic).unwrap().bitmap().width(), 40);
    assert_eq!(size(&ui, intrinsic), Size::new(20.0, 10.0));
    assert_eq!(size(&ui, explicit), Size::new(100.0, 100.0));
    assert_eq!(
        ui.image_asset(intrinsic).unwrap().bitmap(),
        ui.image_asset(explicit).unwrap().bitmap()
    );
    ui.set_image_settings(
        intrinsic,
        ImageSettings {
            current_color: Some(Color::rgb(20, 30, 40)),
            ..Default::default()
        },
    )
    .unwrap();
    frame(&mut ui);
    let explicit_color = ui.image_asset(intrinsic).unwrap().bitmap().clone();
    ui.set_theme(Theme::FluentLight).unwrap();
    frame(&mut ui);
    assert_eq!(ui.image_asset(intrinsic).unwrap().bitmap(), &explicit_color);
}

#[test]
fn invalid_settings_are_atomic_and_one_auto_axis_keeps_aspect_ratio() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let id = ui
        .image(
            ui.root(),
            source("red", 20, 10),
            ImageSettings::default(),
            LayoutStyle {
                height: Length::Px(40.0),
                ..Default::default()
            },
            "Image",
        )
        .unwrap();
    frame(&mut ui);
    assert_eq!(size(&ui, id), Size::new(80.0, 40.0));
    let bitmap = ui.image_asset(id).unwrap().bitmap().clone();
    for opacity in [-1.0, 1.1, f32::NAN] {
        assert!(
            ui.set_image_settings(
                id,
                ImageSettings {
                    opacity,
                    ..Default::default()
                }
            )
            .is_err()
        );
    }
    assert!(ui.set_image_scale(f32::INFINITY).is_err());
    assert!(
        ui.set_image_settings(
            id,
            ImageSettings {
                current_color: Some(Color {
                    a: 0.5,
                    ..Color::rgb(1, 2, 3)
                }),
                ..Default::default()
            }
        )
        .is_err()
    );
    assert_eq!(ui.image_asset(id).unwrap().bitmap(), &bitmap);
    assert_eq!(ui.image_settings(id), Some(ImageSettings::default()));
    let mut style = ui.runtime.tree().node(id).unwrap().props.style.clone();
    style.width = Length::Px(77.0);
    ui.runtime
        .apply(Mutation::SetStyle { node: id, style })
        .unwrap();
    ui.set_image_source(id, source("blue", 90, 10)).unwrap();
    frame(&mut ui);
    assert_eq!(size(&ui, id), Size::new(77.0, 40.0));
}

#[test]
fn detached_image_retains_asset_but_removed_image_cannot_affect_reused_id() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let id = ui
        .image(
            ui.root(),
            source("red", 20, 10),
            ImageSettings::default(),
            fixed(),
            "Image",
        )
        .unwrap();
    let bitmap = ui.image_asset(id).unwrap().bitmap().clone();
    ui.runtime.apply(Mutation::Detach(id)).unwrap();
    frame(&mut ui);
    assert_eq!(ui.image_asset(id).unwrap().bitmap(), &bitmap);
    assert!(ui.scene().images.is_empty());
    ui.runtime.apply(Mutation::Remove(id)).unwrap();
    frame(&mut ui);
    assert!(ui.image_asset(id).is_none());
    assert!(ui.images.controls.is_empty());
    let replacement = ui
        .image(
            ui.root(),
            source("blue", 20, 10),
            ImageSettings::default(),
            fixed(),
            "Replacement",
        )
        .unwrap();
    assert_ne!(id, replacement);
    assert!(ui.set_image_source(id, source("green", 1, 1)).is_err());
    assert_eq!(
        ui.image_asset(replacement).unwrap().bitmap().pixels()[2],
        255
    );
}

#[test]
fn named_resource_registry_has_bounded_count_and_intrinsic_queries_share_cache() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let source = source("red", 20, 10);
    for i in 0..MAX_RESOURCES {
        ui.register_image_source(format!("Image{i}"), source.clone())
            .unwrap();
    }
    assert!(
        ui.register_image_source("Overflow", source.clone())
            .is_err()
    );
    assert!(
        ui.register_image_source("bad/name", source.clone())
            .is_err()
    );
    assert!(ui.register_image_source("", source.clone()).is_err());
    assert!(ui.image_resource_size("Missing").is_err());
    assert_eq!(
        ui.image_resource_size("Image0").unwrap(),
        Size::new(20.0, 10.0)
    );
    let before = ui.image_cache_stats();
    ui.image_from_resource(
        ui.root(),
        "Image1",
        ImageSettings::default(),
        fixed(),
        "Image",
    )
    .unwrap();
    assert_eq!(ui.image_cache_stats().hits, before.hits + 1);
    assert!(ui.unregister_image_source("Image0"));
    ui.register_image_source("Replacement", source).unwrap();
}

fn size(ui: &Controls, id: WidgetId) -> Size {
    let rect = ui.runtime.bounds(id).unwrap();
    Size::new(rect.width, rect.height)
}

#[test]
fn live_asset_budget_failure_stays_idle_until_an_allocation_is_released() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let mut ids = Vec::new();
    for _ in 0..5 {
        ids.push(
            ui.image(
                ui.root(),
                source("red", 2048, 2048),
                ImageSettings::default(),
                fixed(),
                "Image",
            )
            .unwrap(),
        );
    }
    assert!(ui.image_asset(ids[3]).is_some());
    assert!(ui.image_asset(ids[4]).is_none());
    assert!(matches!(
        ui.image_error(ids[4]),
        Some(ImageError::Limit("live image assets exceed 64 MiB"))
    ));
    let before = ui.image_cache_stats();
    frame(&mut ui);
    frame(&mut ui);
    assert_eq!(
        ui.image_cache_stats(),
        before,
        "failed quota must not decode/cache-load every idle frame"
    );
    ui.remove(ids[0]).unwrap();
    frame(&mut ui);
    assert!(ui.image_asset(ids[4]).is_some());
    assert!(ui.image_error(ids[4]).is_none());
}
