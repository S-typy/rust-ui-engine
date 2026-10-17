//! Strict binding representation for tile records; grouping is owned by the collection.
use super::*;
use crate::DataValue;

pub fn tile_items_to_value(items: &[TileItem]) -> DataValue {
    DataValue::List(
        items
            .iter()
            .map(|t| {
                DataValue::object([
                    ("key", t.item.key.clone().into()),
                    ("text", t.item.text.clone().into()),
                    (
                        "columns",
                        DataValue::List(
                            t.item
                                .columns
                                .iter()
                                .cloned()
                                .map(DataValue::String)
                                .collect(),
                        ),
                    ),
                    ("enabled", t.item.enabled.into()),
                    ("group", t.group.clone().into()),
                    ("size", format!("{:?}", t.size).into()),
                    (
                        "command",
                        t.command
                            .as_ref()
                            .map_or(DataValue::Null, |v| v.0.clone().into()),
                    ),
                    (
                        "icon",
                        t.icon.map_or(DataValue::Null, |v| format!("{v:?}").into()),
                    ),
                ])
            })
            .collect(),
    )
}
pub fn tile_items_from_value(value: &DataValue) -> Result<Vec<TileItem>, String> {
    let rows = value.as_list().ok_or("Tile ItemsSource requires a list")?;
    if rows.len() > MAX_TILES {
        return Err("Tile ItemsSource exceeds 100000 records".into());
    }
    let text = |value: &DataValue| match value {
        DataValue::String(s) => Ok(s.clone()),
        _ => Err("Expected a string tile field".to_owned()),
    };
    let mut result = Vec::with_capacity(rows.len());
    for row in rows {
        let fields = row.as_object().ok_or("Tile record requires an object")?;
        if fields.keys().any(|key| {
            !matches!(
                key.as_str(),
                "key" | "text" | "columns" | "enabled" | "group" | "size" | "command" | "icon"
            )
        }) {
            return Err("Unknown tile record field".into());
        }
        let key = text(fields.get("key").ok_or("Tile record requires key")?)?;
        let mut tile = TileItem::new(
            key,
            text(fields.get("text").ok_or("Tile record requires text")?)?,
        );
        if let Some(value) = fields.get("columns") {
            tile.item.columns = value
                .as_list()
                .ok_or("Tile columns require a list")?
                .iter()
                .map(text)
                .collect::<Result<_, _>>()?;
        }
        if let Some(value) = fields.get("enabled") {
            tile.item.enabled = match value {
                DataValue::Bool(b) => *b,
                _ => return Err("Tile enabled requires a boolean".into()),
            };
        }
        if let Some(value) = fields.get("group") {
            tile.group = text(value)?;
        }
        if let Some(value) = fields.get("size") {
            tile.size = match text(value)?.as_str() {
                "Small" => TileSize::Small,
                "Medium" => TileSize::Medium,
                "Wide" => TileSize::Wide,
                "Large" => TileSize::Large,
                _ => return Err("Unknown tile size".into()),
            };
        }
        if let Some(value) = fields.get("command")
            && *value != DataValue::Null
        {
            tile.command = Some(CommandId(text(value)?));
        }
        if let Some(value) = fields.get("icon")
            && *value != DataValue::Null
        {
            tile.icon = Some(match text(value)?.as_str() {
                "Check" => IconSymbol::Check,
                "Close" => IconSymbol::Close,
                "Add" => IconSymbol::Add,
                "Remove" => IconSymbol::Remove,
                "ChevronDown" => IconSymbol::ChevronDown,
                "ChevronRight" => IconSymbol::ChevronRight,
                "Save" => IconSymbol::Save,
                "Pin" => IconSymbol::Pin,
                "FolderOpen" => IconSymbol::FolderOpen,
                "Copy" => IconSymbol::Copy,
                "Edit" => IconSymbol::Edit,
                "Eraser" => IconSymbol::Eraser,
                "Sun" => IconSymbol::Sun,
                "Moon" => IconSymbol::Moon,
                "Lock" => IconSymbol::Lock,
                "Eye" => IconSymbol::Eye,
                "Backspace" => IconSymbol::Backspace,
                _ => return Err("Unknown tile icon".into()),
            });
        }
        result.push(tile);
    }
    Ok(result)
}
