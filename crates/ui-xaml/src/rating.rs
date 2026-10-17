//! Nullable score literals and bindings for Rating.
use crate::*;

#[derive(Clone, Debug, PartialEq)]
pub struct RatingDefinition {
    pub value: Option<f64>,
    pub maximum: u8,
    pub step: f64,
    pub allow_empty: bool,
    pub is_read_only: bool,
    pub glyph_size: f32,
    pub spacing: f32,
}
impl Default for RatingDefinition {
    fn default() -> Self {
        Self {
            value: None,
            maximum: 5,
            step: 1.0,
            allow_empty: true,
            is_read_only: false,
            glyph_size: 24.0,
            spacing: 4.0,
        }
    }
}
pub fn validate_rating(rating: &RatingDefinition, common: &CommonProperties) -> Result<(), String> {
    if !(1..=20).contains(&rating.maximum)
        || !rating.step.is_finite()
        || !(0.01..=1.0).contains(&rating.step)
        || !rating.glyph_size.is_finite()
        || !(8.0..=64.0).contains(&rating.glyph_size)
        || !rating.spacing.is_finite()
        || !(0.0..=32.0).contains(&rating.spacing)
        || rating
            .value
            .is_some_and(|v| !v.is_finite() || !(0.0..=f64::from(rating.maximum)).contains(&v))
        || rating.value.is_none() && !rating.allow_empty && !common.bindings.contains_key("Value")
    {
        return Err("Rating requires Maximum 1..20, Step 0.01..1, GlyphSize 8..64, Spacing 0..32 and a finite Value within 0..Maximum; empty Value requires AllowEmpty".into());
    }
    if common.bindings.keys().any(|p| {
        !crate::badges::binding_property(p)
            && !matches!(
                p.as_str(),
                "Value" | "IsReadOnly" | "IsEnabled" | "Visibility" | "Width" | "Height"
            )
    }) {
        return Err("Rating supports Value, IsReadOnly and common layout bindings".into());
    }
    if let Some(value) = common
        .bindings
        .get("Value")
        .and_then(|b| b.fallback_value.as_ref())
    {
        let parsed = if value == "null" {
            None
        } else {
            Some(
                value
                    .parse::<f64>()
                    .map_err(|_| "Rating fallback requires a number or null")?,
            )
        };
        if parsed.is_none() && !rating.allow_empty
            || parsed
                .is_some_and(|v| !v.is_finite() || !(0.0..=f64::from(rating.maximum)).contains(&v))
        {
            return Err("Rating fallback is outside the allowed value range".into());
        }
    }
    Ok(())
}
pub(crate) fn property(
    document: &Document<'_>,
    attr: Attribute<'_, '_>,
    rating: &mut RatingDefinition,
) -> Result<bool, Diagnostic> {
    let number = || {
        attr.value()
            .parse::<f64>()
            .map_err(|_| at_attribute(document, attr, "Rating requires a finite number"))
    };
    match attr.name() {
        "Value" => {
            rating.value = if matches!(attr.value(), "" | "null") {
                None
            } else {
                Some(number()?)
            }
        }
        "Maximum" => {
            rating.maximum = attr
                .value()
                .parse()
                .map_err(|_| at_attribute(document, attr, "Maximum requires an integer in 1..20"))?
        }
        "Step" => rating.step = number()?,
        "AllowEmpty" => rating.allow_empty = boolean(document, attr)?,
        "IsReadOnly" => rating.is_read_only = boolean(document, attr)?,
        "GlyphSize" => rating.glyph_size = number()? as f32,
        "Spacing" => rating.spacing = number()? as f32,
        _ => return Ok(false),
    }
    Ok(true)
}
pub(crate) fn code(r: &RatingDefinition) -> String {
    format!(
        "rust_desktop_ui_xaml::RatingDefinition {{ value:{:?},maximum:{},step:{:?},allow_empty:{},is_read_only:{},glyph_size:{:?},spacing:{:?} }}",
        r.value, r.maximum, r.step, r.allow_empty, r.is_read_only, r.glyph_size, r.spacing
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    fn window(body: &str) -> Result<WindowDefinition, Diagnostic> {
        parse(&format!(
            "<Window xmlns='urn:rust-ui-engine:ui'>{body}</Window>"
        ))
    }
    #[test]
    fn empty_zero_fraction_and_twoway_value_are_distinct() {
        for (value, expected) in [("null", None), ("0", Some(0.0)), ("2.345", Some(2.345))] {
            let w = window(&format!("<Rating Value='{value}' Step='0.5'/>")).unwrap();
            let ElementKind::Rating(r) = w.content.kind else {
                panic!()
            };
            assert_eq!(r.value, expected);
            assert!(code(&r).contains("RatingDefinition"));
        }
        let w = window(
            "<Rating Value='{Binding Score}' AllowEmpty='False' IsReadOnly='{Binding ReadOnly}'/>",
        )
        .unwrap();
        let b = &w.content.common.bindings["Value"];
        assert_eq!(b.mode, BindingMode::TwoWay);
        assert_eq!(
            b.update_source_trigger,
            UpdateSourceTrigger::PropertyChanged
        );
    }
    #[test]
    fn invalid_literals_children_and_fallbacks_are_diagnostics() {
        for attr in [
            "Maximum='0'",
            "Maximum='21'",
            "Value='NaN'",
            "Value='Infinity'",
            "Value='6'",
            "Value='-1'",
            "Step='0'",
            "Step='1.1'",
            "Step='NaN'",
            "GlyphSize='7'",
            "GlyphSize='Infinity'",
            "Spacing='-1'",
            "AllowEmpty='False'",
            "Value='{Binding Score, FallbackValue=NaN}'",
            "Text='wrong'",
        ] {
            assert!(window(&format!("<Rating {attr}/>")).is_err(), "{attr}");
        }
        assert!(window("<Rating><TextBlock Text='bad'/></Rating>").is_err());
    }
}
