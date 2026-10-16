use super::*;

fn source(window: &str, image: &str) -> String {
    format!("<Window xmlns='urn:rust-ui-engine:ui' {window}><Image {image}/></Window>")
}

#[test]
fn image_and_theme_properties_survive_compilation() {
    let xml = source(
        "Theme='HighContrastDark' Density='Touch' AccentColor='#FFEEAA'",
        "Source='brand.logo-1' Stretch='UniformToFill' Sampling='Nearest' Opacity='0.25' Foreground='#FF123456'",
    );
    let window = parse(&xml).unwrap();
    assert_eq!(window.theme, Theme::HighContrastDark);
    assert_eq!(window.density, ThemeDensity::Touch);
    assert_eq!(window.accent_color, Some([255, 238, 170]));
    let ElementKind::Image(image) = window.content.kind else {
        panic!("Image")
    };
    assert_eq!(image.source, "brand.logo-1");
    assert_eq!(image.stretch, ImageStretchDefinition::UniformToFill);
    assert_eq!(image.sampling, ImageSamplingDefinition::Nearest);
    assert_eq!(image.opacity, 0.25);
    assert_eq!(image.foreground, Some([18, 52, 86, 255]));
    let generated = compile(&xml).unwrap();
    for expected in [
        "Theme::HighContrastDark",
        "ThemeDensity::Touch",
        "ImageDefinition",
        "brand.logo-1",
        "ImageStretchDefinition::UniformToFill",
        "ImageSamplingDefinition::Nearest",
    ] {
        assert!(generated.contains(expected), "{expected}");
    }
}

#[test]
fn image_profile_rejects_ambiguous_resources_and_invalid_presentation() {
    for properties in [
        "",
        "Source=''",
        "Source='https://example.com/a.svg'",
        "Source='../a.png'",
        "Source='a' Opacity='NaN'",
        "Source='a' Opacity='1.01'",
        "Source='a' Opacity='-0.1'",
        "Source='a' Sampling='Cubic'",
        "Source='a' Stretch='Cover'",
        "Source='a' Foreground='#80112233'",
    ] {
        assert!(parse(&source("", properties)).is_err(), "{properties}");
    }
    for properties in [
        "Theme='Unknown'",
        "Density='Small'",
        "AccentColor='#80112233'",
    ] {
        assert!(
            parse(&source(properties, "Source='a'")).is_err(),
            "{properties}"
        );
    }
    assert!(
        parse(
            "<Window xmlns='urn:rust-ui-engine:ui'><Image Source='a'><TextBlock/></Image></Window>"
        )
        .is_err()
    );
}

#[test]
fn programmatic_image_validation_matches_parser_and_legacy_defaults_remain() {
    let window = parse("<Window xmlns='urn:rust-ui-engine:ui'><TextBox/></Window>").unwrap();
    assert_eq!(window.density, ThemeDensity::Comfortable);
    assert_eq!(window.accent_color, None);
    let mut image = ImageDefinition {
        source: "a".into(),
        ..Default::default()
    };
    assert!(validate_image(&image).is_ok());
    image.opacity = f32::INFINITY;
    assert!(validate_image(&image).is_err());
}
