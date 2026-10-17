//! Exact accepted-path binding shape; draft text is deliberately excluded.
use super::*;
use crate::DataValue;

pub fn breadcrumb_path_to_value(path: &BreadcrumbPath) -> DataValue {
    DataValue::object([
        ("text", path.text.clone().into()),
        (
            "segments",
            DataValue::List(
                path.segments
                    .iter()
                    .map(|segment| {
                        DataValue::object([
                            ("key", segment.key.clone().into()),
                            ("label", segment.label.clone().into()),
                            ("has_children", segment.has_children.into()),
                        ])
                    })
                    .collect(),
            ),
        ),
    ])
}

pub fn breadcrumb_path_from_value(value: &DataValue) -> Result<BreadcrumbPath, String> {
    let DataValue::Object(fields) = value else {
        return Err("Breadcrumb Value requires an object".into());
    };
    if fields.len() != 2 {
        return Err("Breadcrumb Value requires exactly text and segments".into());
    }
    let Some(DataValue::String(text)) = fields.get("text") else {
        return Err("Breadcrumb text requires a string".into());
    };
    let Some(DataValue::List(items)) = fields.get("segments") else {
        return Err("Breadcrumb segments requires a list".into());
    };
    if items.len() > MAX_BREADCRUMB_SEGMENTS {
        return Err("At most 64 breadcrumb segments are supported".into());
    }
    let mut segments = Vec::with_capacity(items.len());
    for item in items {
        let DataValue::Object(fields) = item else {
            return Err("Breadcrumb segment requires an object".into());
        };
        if fields.len() != 3 {
            return Err("Breadcrumb segment requires exactly key,label,has_children".into());
        }
        let Some(DataValue::String(key)) = fields.get("key") else {
            return Err("Breadcrumb key requires a string".into());
        };
        let Some(DataValue::String(label)) = fields.get("label") else {
            return Err("Breadcrumb label requires a string".into());
        };
        let Some(DataValue::Bool(has_children)) = fields.get("has_children") else {
            return Err("Breadcrumb has_children requires a boolean".into());
        };
        segments.push(BreadcrumbSegment {
            key: key.clone(),
            label: label.clone(),
            has_children: *has_children,
        });
    }
    let path = BreadcrumbPath {
        text: text.clone(),
        segments,
    };
    path.validate().map_err(|error| error.to_string())?;
    Ok(path)
}
