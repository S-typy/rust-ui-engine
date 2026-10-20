//! Strict versioned report definitions. File I/O remains application-owned.
use super::*;
use crate::definition_codec::*;
use serde::{Deserialize, Serialize};

pub const MAX_REPORT_DEFINITION_BYTES: usize = 4 * 1024 * 1024;
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    version: u32,
    title: String,
    page: [f32; 2],
    margins: [f32; 4],
    parameters: Vec<FieldDto>,
    filters: Vec<(String, String, String)>,
    query: QueryDto,
    bands: Vec<Band>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Band {
    height: f32,
    items: Vec<Item>,
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", deny_unknown_fields)]
enum Item {
    Text {
        x: f32,
        y: f32,
        width: f32,
        font_size: f32,
        bold: bool,
        color: [f32; 4],
        value: Value,
    },
    Rectangle {
        bounds: [f32; 4],
        color: [f32; 4],
    },
    Image {
        bounds: [f32; 4],
        width: u32,
        height: u32,
        rgba: Vec<u8>,
    },
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", deny_unknown_fields)]
enum Value {
    Literal(String),
    Expression(String),
    Parameter(String),
    GroupKey(usize),
    GroupTotal(usize),
    Total(usize),
    PageNumber,
    PageCount,
}
fn color(c: Color) -> [f32; 4] {
    [c.r, c.g, c.b, c.a]
}
fn from_color(c: [f32; 4]) -> Color {
    Color {
        r: c[0],
        g: c[1],
        b: c[2],
        a: c[3],
    }
}
fn rect(r: Rect) -> [f32; 4] {
    [r.x, r.y, r.width, r.height]
}
fn from_rect(r: [f32; 4]) -> Rect {
    Rect::new(r[0], r[1], r[2], r[3])
}
impl Item {
    fn encode(item: &ReportItem) -> Result<Self, String> {
        Ok(match item {
            ReportItem::Text {
                x,
                y,
                width,
                font_size,
                bold,
                color: c,
                value,
            } => Self::Text {
                x: *x,
                y: *y,
                width: *width,
                font_size: *font_size,
                bold: *bold,
                color: color(*c),
                value: match value {
                    ReportValue::Literal(s) => Value::Literal(s.clone()),
                    ReportValue::Expression(e) => Value::Expression(
                        format_expression_text(e).map_err(crate::definition_codec::error)?,
                    ),
                    ReportValue::Parameter(s) => Value::Parameter(s.clone()),
                    ReportValue::GroupKey(n) => Value::GroupKey(*n),
                    ReportValue::GroupTotal(n) => Value::GroupTotal(*n),
                    ReportValue::Total(n) => Value::Total(*n),
                    ReportValue::PageNumber => Value::PageNumber,
                    ReportValue::PageCount => Value::PageCount,
                },
            },
            ReportItem::Rectangle { bounds, color: c } => Self::Rectangle {
                bounds: rect(*bounds),
                color: color(*c),
            },
            ReportItem::Image { bounds, bitmap } => {
                if bitmap.pixels().len() > 512 * 1024 {
                    return Err("Embedded report image exceeds 512 KiB".into());
                }
                Self::Image {
                    bounds: rect(*bounds),
                    width: bitmap.width(),
                    height: bitmap.height(),
                    rgba: bitmap.pixels().to_vec(),
                }
            }
        })
    }
    fn decode(self, schema: &TableSchema) -> Result<ReportItem, String> {
        Ok(match self {
            Self::Text {
                x,
                y,
                width,
                font_size,
                bold,
                color,
                value,
            } => ReportItem::Text {
                x,
                y,
                width,
                font_size,
                bold,
                color: from_color(color),
                value: match value {
                    Value::Literal(s) => ReportValue::Literal(s),
                    Value::Expression(s) => ReportValue::Expression(
                        parse_expression_text(schema, &s)
                            .map_err(crate::definition_codec::error)?,
                    ),
                    Value::Parameter(s) => ReportValue::Parameter(s),
                    Value::GroupKey(n) => ReportValue::GroupKey(n),
                    Value::GroupTotal(n) => ReportValue::GroupTotal(n),
                    Value::Total(n) => ReportValue::Total(n),
                    Value::PageNumber => ReportValue::PageNumber,
                    Value::PageCount => ReportValue::PageCount,
                },
            },
            Self::Rectangle { bounds, color } => ReportItem::Rectangle {
                bounds: from_rect(bounds),
                color: from_color(color),
            },
            Self::Image {
                bounds,
                width,
                height,
                rgba,
            } => {
                if rgba.len() > 512 * 1024 {
                    return Err("Embedded report image exceeds 512 KiB".into());
                }
                ReportItem::Image {
                    bounds: from_rect(bounds),
                    bitmap: Bitmap::new_rgba8(width, height, rgba)
                        .map_err(crate::definition_codec::error)?,
                }
            }
        })
    }
}
impl ReportDefinition {
    /// Serialize the complete supported definition, not evaluated pages.
    /// Validation against a data schema is performed by the designer/engine.
    pub fn to_json(&self) -> Result<String, ReportError> {
        let build = || -> Result<String, String> {
            let bands = [
                &self.report_header,
                &self.page_header,
                &self.group_header,
                &self.detail,
                &self.group_footer,
                &self.report_footer,
                &self.page_footer,
            ];
            if bands.iter().any(|b| b.items.len() > 256) {
                return Err("Report item limit exceeded".into());
            }
            let file = File {
                version: 1,
                title: self.title.clone(),
                page: [self.page_size.width, self.page_size.height],
                margins: self.margins,
                parameters: self.parameters.iter().map(FieldDto::encode).collect(),
                filters: self
                    .parameter_filters
                    .iter()
                    .map(|f| {
                        (
                            f.parameter.clone(),
                            f.field.clone(),
                            format!("{:?}", f.operator),
                        )
                    })
                    .collect(),
                query: QueryDto::encode(&self.query)?,
                bands: bands
                    .iter()
                    .map(|b| {
                        Ok(Band {
                            height: b.height,
                            items: b.items.iter().map(Item::encode).collect::<Result<_, _>>()?,
                        })
                    })
                    .collect::<Result<_, String>>()?,
            };
            let json =
                serde_json::to_string_pretty(&file).map_err(crate::definition_codec::error)?;
            if json.len() > MAX_REPORT_DEFINITION_BYTES {
                return Err("Report definition exceeds 4 MiB".into());
            }
            Ok(json)
        };
        build().map_err(|e| super::error("definition", e))
    }
    pub fn from_json(json: &str, schema: &TableSchema) -> Result<Self, ReportError> {
        let build = || -> Result<Self, String> {
            if json.len() > MAX_REPORT_DEFINITION_BYTES {
                return Err("Report definition exceeds 4 MiB".into());
            }
            let f: File = serde_json::from_str(json).map_err(crate::definition_codec::error)?;
            if f.version != 1
                || f.bands.len() != 7
                || f.parameters.len() > 64
                || f.filters.len() > 64
            {
                return Err("Unsupported report version or limits".into());
            }
            let mut bands = f
                .bands
                .into_iter()
                .map(|b| {
                    if b.items.len() > 256 {
                        return Err("Report item limit exceeded".into());
                    }
                    Ok(ReportBand {
                        height: b.height,
                        items: b
                            .items
                            .into_iter()
                            .map(|i| i.decode(schema))
                            .collect::<Result<_, _>>()?,
                    })
                })
                .collect::<Result<Vec<_>, String>>()?
                .into_iter();
            Ok(Self {
                title: f.title,
                page_size: Size::new(f.page[0], f.page[1]),
                margins: f.margins,
                parameters: f
                    .parameters
                    .into_iter()
                    .map(FieldDto::decode)
                    .collect::<Result<_, _>>()?,
                parameter_filters: f
                    .filters
                    .into_iter()
                    .map(|(parameter, field, op)| {
                        Ok(ReportParameterFilter {
                            parameter,
                            field,
                            operator: match op.as_str() {
                                "Equal" => ComparisonOperator::Equal,
                                "NotEqual" => ComparisonOperator::NotEqual,
                                "Less" => ComparisonOperator::Less,
                                "LessOrEqual" => ComparisonOperator::LessOrEqual,
                                "Greater" => ComparisonOperator::Greater,
                                "GreaterOrEqual" => ComparisonOperator::GreaterOrEqual,
                                _ => return Err("Unknown comparison".into()),
                            },
                        })
                    })
                    .collect::<Result<_, String>>()?,
                query: f.query.decode(schema)?,
                report_header: bands.next().unwrap(),
                page_header: bands.next().unwrap(),
                group_header: bands.next().unwrap(),
                detail: bands.next().unwrap(),
                group_footer: bands.next().unwrap(),
                report_footer: bands.next().unwrap(),
                page_footer: bands.next().unwrap(),
            })
        };
        build().map_err(|e| super::error("definition", e))
    }
}
