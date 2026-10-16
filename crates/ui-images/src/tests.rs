use super::*;

fn source(color: &str) -> ImageSource {
    ImageSource::from_bytes(format!(r#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="10"><rect width="20" height="10" fill="{color}"/></svg>"#).into_bytes(), ImageFormat::Svg).unwrap()
}
#[test]
fn svg_scales_pixels_without_changing_intrinsic_size_and_unpremultiplies() {
    let colored = source("currentColor");
    let options = DecodeOptions {
        svg_scale: 2.0,
        current_color: [20, 40, 80],
    };
    let image = colored.decode(options).unwrap();
    assert_eq!(
        image.natural_size(),
        rust_desktop_ui_core::Size::new(20.0, 10.0)
    );
    assert_eq!((image.bitmap().width(), image.bitmap().height()), (40, 20));
    assert_eq!(&image.bitmap().pixels()[..4], &[20, 40, 80, 255]);
    let alpha = source("red");
    let text = String::from_utf8(alpha.bytes().to_vec())
        .unwrap()
        .replace("fill=", "fill-opacity=\"0.5\" fill=");
    let image = decode(text.as_bytes(), ImageFormat::Svg, DecodeOptions::default()).unwrap();
    assert_eq!(image.bitmap().pixels()[0], 255);
    assert!((127..=128).contains(&image.bitmap().pixels()[3]));
}
#[test]
fn png_jpeg_bmp_and_ico_are_real_decoders_and_bad_data_is_an_error() {
    let pixels = image::RgbaImage::from_pixel(3, 2, image::Rgba([20, 80, 160, 255]));
    for (kind, encoding) in [
        (ImageFormat::Png, image::ImageFormat::Png),
        (ImageFormat::Jpeg, image::ImageFormat::Jpeg),
        (ImageFormat::Bmp, image::ImageFormat::Bmp),
        (ImageFormat::Ico, image::ImageFormat::Ico),
    ] {
        let mut output = std::io::Cursor::new(Vec::new());
        let original = image::DynamicImage::ImageRgba8(pixels.clone());
        if kind == ImageFormat::Jpeg {
            image::DynamicImage::ImageRgb8(original.to_rgb8())
                .write_to(&mut output, encoding)
                .unwrap();
        } else {
            original.write_to(&mut output, encoding).unwrap();
        }
        let bytes = output.into_inner();
        assert_eq!(detect_format(&bytes).unwrap(), kind);
        let result = decode(&bytes, kind, DecodeOptions::default()).unwrap();
        assert_eq!((result.bitmap().width(), result.bitmap().height()), (3, 2));
        assert!(
            decode(
                &bytes[..10.min(bytes.len())],
                kind,
                DecodeOptions::default()
            )
            .is_err()
        );
    }
}
#[test]
fn svg_rejects_external_active_expensive_or_unsupported_content() {
    for body in [
        "<image href='file:///private.png'/>",
        "<use href='#a'/>",
        "<script>alert(1)</script>",
        "<text>not silently omitted</text>",
        "<filter id='blur'/>",
        "<g onclick='x()'/>",
        "<rect fill='url(https://example.invalid/x)'/>",
        "<rect style='fill:url(#x);stroke:url(file:///x)'/>",
        "<style>@import 'file:///x';</style>",
    ] {
        let svg = format!("<svg width='20' height='10'>{body}</svg>");
        assert!(
            decode(svg.as_bytes(), ImageFormat::Svg, DecodeOptions::default()).is_err(),
            "{body}"
        );
    }
    for text in [
        "<!DOCTYPE svg [<!ENTITY x 'a'>]><svg>&x;</svg>",
        "<svg width='9000' height='1'/>",
        "<svg width='4096' height='4096'/>",
    ] {
        assert!(decode(text.as_bytes(), ImageFormat::Svg, DecodeOptions::default()).is_err());
    }
    assert!(
        source("red")
            .decode(DecodeOptions {
                svg_scale: f32::NAN,
                ..Default::default()
            })
            .is_err()
    );
}
#[test]
fn cache_reuses_identity_keys_options_and_evicts_lru_without_invalidating_clones() {
    let a = source("red");
    let b = source("blue");
    let c = source("green");
    let mut cache = ImageCache::new(2, 8192).unwrap();
    let original = cache.load(&a, DecodeOptions::default()).unwrap();
    cache.load(&b, DecodeOptions::default()).unwrap();
    assert_eq!(
        cache
            .load(&a.clone(), DecodeOptions::default())
            .unwrap()
            .bitmap(),
        original.bitmap()
    );
    cache.load(&c, DecodeOptions::default()).unwrap();
    assert_eq!(cache.stats().entries, 2);
    assert_eq!(cache.stats().hits, 1);
    let blue = cache.load(&b, DecodeOptions::default()).unwrap();
    assert_ne!(blue.bitmap(), original.bitmap());
    assert_eq!(original.bitmap().pixels()[0], 255);
    let before = cache.stats().retained_bytes;
    let broken = ImageSource::from_bytes(b"bad".to_vec(), ImageFormat::Png).unwrap();
    assert!(cache.load(&broken, DecodeOptions::default()).is_err());
    assert_eq!(cache.stats().retained_bytes, before);
    cache.clear();
    assert_eq!(cache.stats().retained_bytes, 0);
}
