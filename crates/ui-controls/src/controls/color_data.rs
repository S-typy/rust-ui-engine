//! Strict nullable binding representations; transparent black is not null.
use super::*;
use crate::DataValue;

pub fn color_to_value(color: Option<Srgba8>) -> DataValue {
    color.map_or(DataValue::Null, |c| {
        DataValue::object([
            ("r", DataValue::Number(f64::from(c.r))),
            ("g", DataValue::Number(f64::from(c.g))),
            ("b", DataValue::Number(f64::from(c.b))),
            ("a", DataValue::Number(f64::from(c.a))),
        ])
    })
}
pub fn color_from_value(value: &DataValue) -> Result<Option<Srgba8>, String> {
    if *value == DataValue::Null {
        return Ok(None);
    }
    let DataValue::Object(fields) = value else {
        return Err("Color requires null or {r,g,b,a}".into());
    };
    if fields.len() != 4 {
        return Err("Color requires exactly r,g,b,a channels".into());
    }
    let channel = |key| match fields.get(key) {
        Some(DataValue::Number(n))
            if n.is_finite() && n.fract() == 0.0 && (0.0..=255.0).contains(n) =>
        {
            Ok(*n as u8)
        }
        _ => Err(format!("Color channel {key} requires an integer in 0..255")),
    };
    Ok(Some(Srgba8::new(
        channel("r")?,
        channel("g")?,
        channel("b")?,
        channel("a")?,
    )))
}
pub fn brush_to_value(value: Option<&BrushValue>) -> DataValue {
    match value {
        None => DataValue::Null,
        Some(BrushValue::Solid(c)) => DataValue::object([
            ("kind", "Solid".into()),
            ("color", color_to_value(Some(*c))),
        ]),
        Some(BrushValue::Linear(g)) => DataValue::object([
            ("kind", "Linear".into()),
            ("angle", DataValue::Number(g.angle())),
            (
                "stops",
                DataValue::List(
                    g.stops()
                        .iter()
                        .map(|s| {
                            DataValue::object([
                                ("offset", DataValue::Number(s.offset)),
                                ("color", color_to_value(Some(s.color))),
                            ])
                        })
                        .collect(),
                ),
            ),
        ]),
    }
}
pub fn brush_from_value(value: &DataValue) -> Result<Option<BrushValue>, String> {
    if *value == DataValue::Null {
        return Ok(None);
    }
    let DataValue::Object(fields) = value else {
        return Err("Brush requires null or a typed object".into());
    };
    match fields.get("kind") {
        Some(DataValue::String(kind)) if kind == "Solid" && fields.len() == 2 => {
            let color = color_from_value(fields.get("color").ok_or("Solid brush requires color")?)?
                .ok_or("Solid brush color cannot be null")?;
            Ok(Some(BrushValue::Solid(color)))
        }
        Some(DataValue::String(kind)) if kind == "Linear" && fields.len() == 3 => {
            let Some(DataValue::Number(angle)) = fields.get("angle") else {
                return Err("Linear brush requires angle".into());
            };
            let Some(DataValue::List(stops)) = fields.get("stops") else {
                return Err("Linear brush requires stops".into());
            };
            if !(2..=32).contains(&stops.len()) {
                return Err("Gradient requires 2..32 stops".into());
            }
            let stops = stops
                .iter()
                .map(|stop| {
                    let DataValue::Object(s) = stop else {
                        return Err("Stop requires offset and color".into());
                    };
                    if s.len() != 2 {
                        return Err("Stop requires exactly offset and color".into());
                    }
                    let Some(DataValue::Number(offset)) = s.get("offset") else {
                        return Err("Stop requires offset".into());
                    };
                    let color = color_from_value(s.get("color").ok_or("Stop requires color")?)?
                        .ok_or("Stop color cannot be null")?;
                    Ok(GradientStop::new(*offset, color))
                })
                .collect::<Result<Vec<_>, String>>()?;
            Ok(Some(BrushValue::Linear(
                LinearGradient::new(*angle, stops).map_err(|e| e.to_string())?,
            )))
        }
        _ => Err("Brush requires Solid/color or Linear/angle/stops fields".into()),
    }
}
