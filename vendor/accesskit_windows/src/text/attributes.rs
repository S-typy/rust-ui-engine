// Copyright 2022 The AccessKit Authors. All rights reserved.
// Licensed under Apache-2.0 OR MIT; extracted from the existing text provider.
use super::*;

macro_rules! run_attributes {
    ($node:expr, $id:expr) => {
        match $id {
            UIA_CultureAttributeId => Ok(Variant::from(($node).language().map(LocaleName)).into()),
            UIA_FontNameAttributeId => {
                let mut buffer = StringBuffer::acquire();
                Ok(Variant::from(
                    ($node)
                        .font_family()
                        .map(|s| StrWrapper::new(s, &mut buffer)),
                )
                .into())
            }
            UIA_FontSizeAttributeId => {
                Ok(Variant::from(($node).font_size().map(|value| value as f64)).into())
            }
            UIA_FontWeightAttributeId => {
                Ok(Variant::from(($node).font_weight().map(|value| value as i32)).into())
            }
            UIA_IsItalicAttributeId => Ok(Variant::from(($node).is_italic()).into()),
            UIA_BackgroundColorAttributeId => Ok(Variant::from(($node).background_color()).into()),
            UIA_ForegroundColorAttributeId => Ok(Variant::from(($node).foreground_color()).into()),
            UIA_OverlineStyleAttributeId => {
                Ok(Variant::from(($node).overline().map(|d| d.style)).into())
            }
            UIA_OverlineColorAttributeId => {
                Ok(Variant::from(($node).overline().map(|d| d.color)).into())
            }
            UIA_StrikethroughStyleAttributeId => {
                Ok(Variant::from(($node).strikethrough().map(|d| d.style)).into())
            }
            UIA_StrikethroughColorAttributeId => {
                Ok(Variant::from(($node).strikethrough().map(|d| d.color)).into())
            }
            UIA_UnderlineStyleAttributeId => {
                Ok(Variant::from(($node).underline().map(|d| d.style)).into())
            }
            UIA_UnderlineColorAttributeId => {
                Ok(Variant::from(($node).underline().map(|d| d.color)).into())
            }
            UIA_HorizontalTextAlignmentAttributeId => {
                Ok(Variant::from(($node).text_align()).into())
            }
            UIA_IsSubscriptAttributeId => Ok(Variant::from(
                ($node)
                    .vertical_offset()
                    .map(|o| o == VerticalOffset::Subscript),
            )
            .into()),
            UIA_IsSuperscriptAttributeId => Ok(Variant::from(
                ($node)
                    .vertical_offset()
                    .map(|o| o == VerticalOffset::Superscript),
            )
            .into()),
            // TODO: implement more attributes
            _ => {
                let value = unsafe { UiaGetReservedNotSupportedValue() }.unwrap();
                Ok(value.into())
            }
        }
    };
}

pub(super) fn value(range: &Range, id: UIA_TEXTATTRIBUTE_ID) -> Result<VARIANT> {
    match id {
        UIA_IsReadOnlyAttributeId => {
            // TBD: do we ever want to support mixed read-only/editable text?
            let value = range.node().is_read_only();
            Ok(value.into())
        }
        UIA_CaretPositionAttributeId => {
            let mut value = CaretPosition_Unknown;
            if range.is_degenerate() {
                let pos = range.start();
                if pos.is_line_start() {
                    value = CaretPosition_BeginningOfLine;
                } else if pos.is_line_end() {
                    value = CaretPosition_EndOfLine;
                }
            }
            Ok(value.0.into())
        }
        _ => run_attributes!(range, id),
    }
}

pub(super) fn run_value(node: &NodeRef, id: UIA_TEXTATTRIBUTE_ID) -> Result<VARIANT> {
    run_attributes!(node, id)
}
