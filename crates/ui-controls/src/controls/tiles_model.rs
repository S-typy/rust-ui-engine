//! Stable identities and bounded presentation data for adaptive tile collections.
use super::*;

pub const MAX_TILES: usize = 100_000;
pub const MAX_TILE_GROUPS: usize = 256;
/// Integer logical coordinates remain exactly representable by f32 below 2^24.
pub const MAX_TILE_EXTENT: f64 = 16_777_000.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TileSize {
    #[default]
    Small,
    Medium,
    Wide,
    Large,
}
impl TileSize {
    pub(super) fn span(self) -> (usize, usize) {
        match self {
            Self::Small => (1, 1),
            Self::Medium => (1, 2),
            Self::Wide => (2, 1),
            Self::Large => (2, 2),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TileGroup {
    pub key: String,
    pub header: String,
}
impl TileGroup {
    pub fn new(key: impl Into<String>, header: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            header: header.into(),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TileItem {
    pub item: SelectionItem,
    /// Empty means the implicit ungrouped section, preceding named groups.
    pub group: String,
    pub size: TileSize,
    pub icon: Option<IconSymbol>,
    pub command: Option<CommandId>,
}
impl TileItem {
    pub fn new(key: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            item: SelectionItem::new(key, text),
            group: String::new(),
            size: TileSize::Small,
            icon: None,
            command: None,
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct TileSettings {
    pub cell_width: f32,
    pub cell_height: f32,
    pub gap: f32,
    pub group_header_height: f32,
    pub overscan: f32,
    pub max_realized: usize,
    pub selection: SelectionMode,
    pub is_read_only: bool,
    pub allow_reorder: bool,
}
impl Default for TileSettings {
    fn default() -> Self {
        Self {
            cell_width: 144.0,
            cell_height: 88.0,
            gap: 8.0,
            group_header_height: 32.0,
            overscan: 120.0,
            max_realized: 512,
            selection: SelectionMode::Single,
            is_read_only: false,
            allow_reorder: true,
        }
    }
}
impl TileSettings {
    pub fn validate(&self) -> Result<(), ControlError> {
        for (value, low, high) in [
            (self.cell_width, 32.0, 1024.0),
            (self.cell_height, 24.0, 1024.0),
            (self.gap, 0.0, 64.0),
            (self.group_header_height, 16.0, 128.0),
            (self.overscan, 0.0, 4096.0),
        ] {
            if !value.is_finite() || !(low..=high).contains(&value) || value.fract() != 0.0 {
                return Err(tile_error(
                    "Tile dimensions must be finite integer logical pixels within their limits",
                ));
            }
        }
        if !(1..=4096).contains(&self.max_realized) {
            return Err(tile_error("Tile realization limit must be 1..=4096"));
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TileModel {
    groups: Vec<TileGroup>,
    items: Vec<TileItem>,
}
impl TileModel {
    pub fn new(groups: Vec<TileGroup>, items: Vec<TileItem>) -> Result<Self, ControlError> {
        let model = Self { groups, items };
        model.validate()?;
        Ok(model)
    }
    pub fn groups(&self) -> &[TileGroup] {
        &self.groups
    }
    pub fn items(&self) -> &[TileItem] {
        &self.items
    }
    pub fn validate(&self) -> Result<(), ControlError> {
        if self.groups.len() > MAX_TILE_GROUPS || self.items.len() > MAX_TILES {
            return Err(tile_error("Tile model exceeds its item or group limit"));
        }
        let mut groups = HashSet::new();
        let mut keys = HashSet::new();
        let mut bytes = 0usize;
        for group in &self.groups {
            if group.key.is_empty() || !groups.insert(group.key.as_str()) {
                return Err(tile_error("Tile group keys must be nonempty and unique"));
            }
            bytes = bytes
                .saturating_add(group.key.len())
                .saturating_add(group.header.len());
            validate_tile_text(&group.key)?;
            validate_tile_text(&group.header)?;
        }
        for tile in &self.items {
            let item = &tile.item;
            if item.key.is_empty()
                || !keys.insert(item.key.as_str())
                || (!tile.group.is_empty() && !groups.contains(tile.group.as_str()))
                || item.columns.len() > 31
            {
                return Err(tile_error(
                    "Tile keys must be unique, groups known, and columns bounded to 31",
                ));
            }
            for text in std::iter::once(&item.key)
                .chain(std::iter::once(&item.text))
                .chain(item.columns.iter())
                .chain(std::iter::once(&tile.group))
            {
                validate_tile_text(text)?;
                bytes = bytes.saturating_add(text.len());
            }
            if let Some(command) = &tile.command {
                validate_tile_text(&command.0)?;
                bytes = bytes.saturating_add(command.0.len());
            }
        }
        if bytes > 16 * 1024 * 1024 {
            return Err(tile_error("Tile metadata exceeds 16 MiB"));
        }
        Ok(())
    }
    pub(super) fn moved(
        &self,
        key: &str,
        group: &str,
        before: Option<&str>,
    ) -> Result<Self, ControlError> {
        if !group.is_empty() && !self.groups.iter().any(|g| g.key == group) {
            return Err(tile_error("Unknown tile destination group"));
        }
        let index = self
            .items
            .iter()
            .position(|t| t.item.key == key)
            .ok_or_else(|| tile_error("Unknown tile key"))?;
        if before == Some(key) {
            return Ok(self.clone());
        }
        if let Some(before) = before
            && !self
                .items
                .iter()
                .any(|t| t.item.key == before && t.group == group)
        {
            return Err(tile_error(
                "Tile insertion anchor must belong to its destination group",
            ));
        }
        let mut next = self.clone();
        let mut tile = next.items.remove(index);
        tile.group = group.into();
        let at = before.map_or_else(
            || {
                next.items
                    .iter()
                    .rposition(|t| t.group == group)
                    .map_or(next.items.len(), |i| i + 1)
            },
            |key| next.items.iter().position(|t| t.item.key == key).unwrap(),
        );
        next.items.insert(at, tile);
        Ok(next)
    }
}
pub(super) fn tile_error(message: impl Into<String>) -> ControlError {
    ControlError::Invalid(message.into())
}
pub(super) fn validate_tile_text(value: &str) -> Result<(), ControlError> {
    if value.len() > 4096 || value.contains('\0') {
        return Err(tile_error("Tile text exceeds 4096 bytes or contains NUL"));
    }
    Ok(())
}
