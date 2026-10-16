use crate::{DecodeOptions, ImageError, ImageFormat, decode};

#[test]
fn oversized_xml_names_are_rejected_before_diagnostic_expansion() {
    for element in [
        format!("<{} />", "a".repeat(5000)),
        format!("<rect {}='value' />", "a".repeat(5000)),
    ] {
        let source = format!("<svg width='8' height='8'>{element}</svg>");
        assert!(matches!(
            decode(
                source.as_bytes(),
                ImageFormat::Svg,
                DecodeOptions::default()
            ),
            Err(ImageError::Limit(_))
        ));
    }
}

fn svg(body: &str) -> String {
    format!("<svg width='8' height='8' xmlns='http://www.w3.org/2000/svg'>{body}</svg>")
}
fn result(text: &str) -> Result<crate::ImageAsset, ImageError> {
    decode(text.as_bytes(), ImageFormat::Svg, DecodeOptions::default())
}

#[test]
fn inline_css_and_repeated_clip_uses_cannot_bypass_actual_layer_budget() {
    for property in [
        "style='opacity:.9'",
        "opacity='.9'",
        "style='isolation:isolate'",
        "style='mix-blend-mode:multiply'",
    ] {
        let text = svg(&format!(
            "{}<rect width='8' height='8' fill='red'/>{}",
            format!("<g {property}>").repeat(10),
            "</g>".repeat(10)
        ));
        assert!(
            matches!(result(&text), Err(ImageError::Limit(_))),
            "{property}"
        );
    }
    let text = svg(&format!(
        "<defs><clipPath id='c'><rect width='8' height='8'/></clipPath></defs>{}<rect width='8' height='8' fill='red'/>{}",
        "<g clip-path='url(#c)'>".repeat(10),
        "</g>".repeat(10)
    ));
    assert!(matches!(result(&text), Err(ImageError::Limit(_))));
}

#[test]
fn large_isolated_layer_is_rejected_before_pixmap_allocation() {
    // Output is within16MiB, but the isolated geometry reaches resvg's5x-axis
    // scratch bound (400MiB). Preflight rejects before the first Pixmap::new.
    let text = "<svg width='2048' height='2048'><g opacity='.5'><rect x='-9000' y='-9000' width='40000' height='40000'/></g></svg>";
    assert_eq!(
        result(text).unwrap_err(),
        ImageError::Limit("SVG aggregate compositing allocations exceed 128 MiB")
    );
}

#[test]
fn cycles_missing_duplicate_and_long_resource_references_fail_before_normalization() {
    for body in [
        "<defs><linearGradient id='a' href='#b'/><linearGradient id='b' href='#a'/></defs><rect fill='url(#a)'/>",
        "<defs><mask id='m'><rect mask='url(#m)'/></mask></defs><rect mask='url(#m)'/>",
        "<defs><linearGradient id='same'/><linearGradient id='same'/></defs>",
        "<rect fill='url(#absent)'/>",
    ] {
        assert!(result(&svg(body)).is_err(), "{body}");
    }
    let mut body = String::new();
    for i in 0..66 {
        body.push_str(&format!("<linearGradient id='g{i}' href='#g{}'/>", i + 1));
    }
    body.push_str("<linearGradient id='g66'/>");
    assert!(matches!(result(&svg(&body)), Err(ImageError::Limit(_))));
}

#[test]
fn reference_expansion_is_bounded_before_usvg_can_duplicate_subtrees() {
    // Acyclic but exponentially referenced defs: two child paths refer to
    // the previous mask. Eight small levels are harmless; fifteen exceed cap.
    let mut body = "<defs><mask id='m0'><rect width='8' height='8'/></mask>".to_owned();
    for i in 1..15 {
        body.push_str(&format!(
            "<mask id='m{i}'><rect mask='url(#m{})'/><rect mask='url(#m{})'/></mask>",
            i - 1,
            i - 1
        ));
    }
    body.push_str("</defs><rect width='8' height='8' mask='url(#m14)'/>");
    assert!(matches!(result(&svg(&body)), Err(ImageError::Limit(_))));
}

#[test]
fn svg_raster_overdraw_and_dash_expansion_have_work_limits() {
    let text = format!(
        "<svg width='2048' height='2048'>{}</svg>",
        "<rect width='2048' height='2048'/>".repeat(20)
    );
    assert_eq!(
        result(&text).unwrap_err(),
        ImageError::Limit("SVG raster work exceeds 128 Mi pixels")
    );
    let text = svg("<path d='M0 0L8 8' stroke='red' stroke-dasharray='.000001 .000001'/>");
    assert!(matches!(result(&text), Err(ImageError::Limit(_))));
}

#[test]
fn ordinary_gradient_clip_mask_and_group_opacity_are_preserved() {
    let text = svg(
        "<defs><linearGradient id='g'><stop offset='0' stop-color='red'/><stop offset='1' stop-color='blue'/></linearGradient><clipPath id='c'><rect width='4' height='8'/></clipPath><mask id='m'><rect width='8' height='8' fill='white'/></mask></defs><g opacity='.5' clip-path='url(#c)' mask='url(#m)'><rect width='8' height='8' fill='url(#g)'/></g>",
    );
    let asset = result(&text).unwrap();
    let rgba = asset.bitmap().pixels();
    assert!((127..=128).contains(&rgba[(3 * 8 + 2) * 4 + 3]));
    assert_eq!(rgba[(3 * 8 + 6) * 4 + 3], 0);
    assert!(rgba[(3 * 8 + 1) * 4] > rgba[(3 * 8 + 3) * 4]);
}

#[test]
fn inline_css_obfuscation_or_unsupported_properties_are_explicit_errors() {
    for style in [
        "fill:u/**/rl(https://example.invalid/a)",
        "filter:url(#x)",
        "fill:var(--x)",
        "fill:URL('https://example.invalid/a')",
        "--x:red",
        "fill: red;@import: x",
    ] {
        assert!(
            result(&svg(&format!(
                "<rect width='8' height='8' style=\"{style}\"/>"
            )))
            .is_err(),
            "{style}"
        );
    }
    assert!(result("<svg xmlns='urn:foreign' width='8' height='8'/>").is_err());
}
