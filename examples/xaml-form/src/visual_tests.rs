use crate::form::Form;
use rust_desktop_ui_core::Size;
use rust_desktop_ui_platform_winit::DesktopApp;

#[test]
fn compiled_visuals_mount_all_themes_and_density_with_images_and_error_isolation() {
    use rust_desktop_ui_xaml::{Theme, ThemeDensity};
    for theme in [
        Theme::Light,
        Theme::Dark,
        Theme::HighContrastLight,
        Theme::HighContrastDark,
    ] {
        for density in [
            ThemeDensity::Comfortable,
            ThemeDensity::Compact,
            ThemeDensity::Touch,
        ] {
            let mut definition = crate::generated_visuals::window_definition();
            definition.theme = theme;
            definition.density = density;
            let mut form = Form::new(definition, false).unwrap();
            form.update(Size::new(1180.0, 820.0), 1.5).unwrap();
            for name in [
                "Landscape",
                "Crop",
                "Compass",
                "RasterNearest",
                "RasterLinear",
            ] {
                let id = form.find_name(name).unwrap();
                assert!(
                    form.ui.image_error(id).is_none(),
                    "{name}: {:?}",
                    form.ui.image_error(id)
                );
                assert!(form.ui.image_asset(id).is_some(), "{name}");
            }
            assert!(
                form.ui
                    .image_error(form.find_name("Broken").unwrap())
                    .is_some()
            );
            assert_eq!(form.text(), "Привет, мир!");
            assert!(!form.ui.scene().images.is_empty());
        }
    }
}

#[test]
fn xaml_image_natural_size_dpi_and_live_current_color() {
    let definition=rust_desktop_ui_xaml::parse(r#"<Window xmlns="urn:rust-ui-engine:ui" xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"><StackPanel><Image x:Name="I" Source="Compass"/><TextBox x:Name="Message" Text="Draft"/></StackPanel></Window>"#).unwrap();
    let mut form = Form::new(definition, false).unwrap();
    form.update(Size::new(500.0, 400.0), 1.0).unwrap();
    let id = form.find_name("I").unwrap();
    let first = form.ui.image_asset(id).unwrap().bitmap().id();
    let size = form.ui.runtime.bounds(id).unwrap();
    assert_eq!((size.width, size.height), (64.0, 64.0));
    form.ui.commands_mut().activate(&"Dark".into());
    form.update(Size::new(500.0, 400.0), 2.0).unwrap();
    form.update(Size::new(500.0, 400.0), 2.0).unwrap();
    assert!(form.ui.theme().is_dark());
    let image = form.ui.image_asset(id).unwrap();
    assert_ne!(image.bitmap().id(), first);
    assert_eq!(image.bitmap().width(), 128);
    assert_eq!(form.ui.runtime.bounds(id).unwrap(), size);
    assert_eq!(form.text(), "Draft");
}

#[test]
fn incompatible_accent_switch_preserves_live_form_instead_of_closing_host() {
    let mut definition = crate::generated_visuals::window_definition();
    definition.theme = rust_desktop_ui_xaml::Theme::Dark;
    definition.accent_color = Some([255, 255, 0]);
    let mut form = Form::new(definition, false).unwrap();
    form.update(Size::new(1180.0, 820.0), 1.0).unwrap();
    let before = form.ui.theme();
    assert!(form.ui.commands_mut().activate(&"Light".into()));
    form.update(Size::new(1180.0, 820.0), 1.0).unwrap();
    assert_eq!(form.ui.theme(), before);
    assert_eq!(form.text(), "Привет, мир!");
    assert!(
        form.ui
            .image_asset(form.find_name("Compass").unwrap())
            .is_some()
    );
}
