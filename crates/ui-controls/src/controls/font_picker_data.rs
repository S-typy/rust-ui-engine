//! Strict binding representation shared by FontPicker and application models.
use super::*;
use crate::DataValue;

pub fn font_descriptor_to_value(font: &FontDescriptor) -> DataValue {
    let (style, angle) = match font.slant {
        FontSlant::Normal => ("Normal", 0.0),
        FontSlant::Italic => ("Italic", 0.0),
        FontSlant::Oblique(angle) => ("Oblique", angle),
    };
    DataValue::object([
        ("family", font.family.clone().into()),
        ("size", DataValue::Number(f64::from(font.size))),
        ("weight", DataValue::Number(f64::from(font.weight))),
        ("width", DataValue::Number(f64::from(font.width))),
        ("style", style.into()),
        ("angle", DataValue::Number(f64::from(angle))),
    ])
}
pub fn font_descriptor_from_value(value: &DataValue) -> Result<FontDescriptor, String> {
    let DataValue::Object(fields) = value else {
        return Err("Font Value requires a descriptor object".into());
    };
    if fields.len() != 6
        || fields.keys().any(|key| {
            !matches!(
                key.as_str(),
                "family" | "size" | "weight" | "width" | "style" | "angle"
            )
        })
    {
        return Err("Font Value requires exactly family,size,weight,width,style,angle".into());
    }
    let string = |key: &str| match fields.get(key) {
        Some(DataValue::String(s)) => Ok(s.clone()),
        _ => Err(format!("Font {key} requires text")),
    };
    let number = |key: &str| match fields.get(key) {
        Some(DataValue::Number(n)) if n.is_finite() => Ok(*n),
        _ => Err(format!("Font {key} requires a finite number")),
    };
    let weight = number("weight")?;
    if weight.fract() != 0.0 || !(1.0..=1000.0).contains(&weight) {
        return Err("Font weight must be an integer in 1..1000".into());
    }
    let angle = number("angle")?;
    let size = number("size")?;
    let width = number("width")?;
    if !(1.0..=512.0).contains(&size)
        || !(25.0..=400.0).contains(&width)
        || !(-45.0..=45.0).contains(&angle)
    {
        return Err("Invalid font descriptor bounds".into());
    }
    let slant = match string("style")?.as_str() {
        "Normal" if angle == 0.0 => FontSlant::Normal,
        "Italic" if angle == 0.0 => FontSlant::Italic,
        "Oblique" => FontSlant::Oblique(angle as f32),
        _ => return Err("Font style requires Normal/Italic with zero angle, or Oblique".into()),
    };
    let font = FontDescriptor {
        family: string("family")?,
        size: size as f32,
        weight: weight as u16,
        width: width as f32,
        slant,
    };
    if !font.is_valid() || font.size < 1.0 {
        return Err("Invalid font descriptor bounds".into());
    }
    Ok(font)
}
