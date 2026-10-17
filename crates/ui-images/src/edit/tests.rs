use super::*;
use crate::{DecodeOptions, ImageFormat, ImageSource};
fn bitmap() -> Bitmap {
    Bitmap::new_rgba8(
        3,
        2,
        (0..6)
            .flat_map(|v| {
                [
                    v * 10 + 1,
                    v * 10 + 2,
                    v * 10 + 3,
                    if v == 0 { 0 } else { v * 40 },
                ]
            })
            .collect(),
    )
    .unwrap()
}
#[test]
fn quarter_turns_and_integer_crops_preserve_every_rgba_byte() {
    let b = bitmap();
    let r = rotate_bitmap(&b, QuarterTurn::Clockwise).unwrap();
    assert_eq!((r.width(), r.height()), (2, 3));
    assert_eq!(&r.pixels()[..4], &b.pixels()[12..16]);
    let mut back = r;
    for _ in 0..3 {
        back = rotate_bitmap(&back, QuarterTurn::Clockwise).unwrap();
    }
    assert_eq!(back.pixels(), b.pixels());
    let c = crop_bitmap(
        &b,
        PixelCrop {
            x: 1,
            y: 0,
            width: 2,
            height: 2,
        },
    )
    .unwrap();
    assert_eq!(
        c.pixels(),
        [&b.pixels()[4..12], &b.pixels()[16..24]].concat()
    );
    assert!(
        crop_bitmap(
            &b,
            PixelCrop {
                x: u32::MAX,
                y: 0,
                width: 2,
                height: 1
            }
        )
        .is_err()
    );
}
#[test]
fn png_roundtrip_preserves_hidden_rgb_and_alpha() {
    let b = bitmap();
    let bytes = encode_bitmap_png(&b).unwrap();
    let result = ImageSource::from_bytes(bytes, ImageFormat::Png)
        .unwrap()
        .decode(DecodeOptions::default())
        .unwrap();
    assert_eq!(result.bitmap().pixels(), b.pixels());
}
#[test]
fn draft_history_cancel_apply_and_null_are_distinct() {
    let b = bitmap();
    let mut s = ImageEditSession::new(Some(b.clone()), Default::default()).unwrap();
    s.rotate(QuarterTurn::Clockwise).unwrap();
    let rotated = s.draft().unwrap().clone();
    assert_eq!(s.accepted(), Some(&b));
    assert!(s.undo());
    assert_eq!(s.draft(), Some(&b));
    assert!(s.redo());
    assert_eq!(s.draft(), Some(&rotated));
    s.cancel();
    assert!(!s.is_dirty());
    s.set_draft(None).unwrap();
    assert!(s.is_dirty());
    assert_eq!(s.accepted(), Some(&b));
    assert!(s.apply());
    assert!(s.accepted().is_none());
    assert!(!s.apply());
}
#[test]
fn rejected_operation_preserves_history_and_external_conflicts() {
    let b = bitmap();
    let mut s = ImageEditSession::new(
        Some(b.clone()),
        ImageEditLimits {
            retained_bytes: 48,
            ..Default::default()
        },
    )
    .unwrap();
    s.rotate(QuarterTurn::Clockwise).unwrap();
    let before = s.draft().unwrap().clone();
    assert!(s.rotate(QuarterTurn::Clockwise).is_err());
    assert_eq!(s.draft(), Some(&before));
    assert!(s.can_undo());
    assert!(s.replace(None, false).is_err());
    assert!(
        s.crop(PixelCrop {
            x: 0,
            y: 0,
            width: 0,
            height: 1
        })
        .is_err()
    );
    assert_eq!(s.draft(), Some(&before));
    s.replace(None, true).unwrap();
    assert!(s.accepted().is_none());
    assert!(!s.can_undo());
}
