//! Bounded grouped samples. Preview and acceptance are separate interactions.
use super::*;

pub const MAX_GALLERY_ITEMS: usize = 256;
pub const MAX_GALLERY_GROUPS: usize = 32;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GalleryGroup {
    pub key: String,
    pub header: String,
}
impl GalleryGroup {
    pub fn new(key: impl Into<String>, header: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            header: header.into(),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GalleryItem {
    pub item: SelectionItem,
    /// Empty means the ungrouped section, preceding named groups.
    pub group: String,
    pub icon: Option<IconSymbol>,
    /// A registered image key, never a path or URL.
    pub image_resource: Option<String>,
}
impl GalleryItem {
    pub fn new(key: impl Into<String>, caption: impl Into<String>) -> Self {
        Self {
            item: SelectionItem::new(key, caption),
            group: String::new(),
            icon: None,
            image_resource: None,
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct GallerySettings {
    pub sample_width: f32,
    pub sample_height: f32,
    pub gap: f32,
    pub group_header_height: f32,
    pub is_read_only: bool,
}
impl Default for GallerySettings {
    fn default() -> Self {
        Self {
            sample_width: 112.0,
            sample_height: 96.0,
            gap: 8.0,
            group_header_height: 28.0,
            is_read_only: false,
        }
    }
}
impl GallerySettings {
    pub fn validate(&self) -> Result<(), ControlError> {
        for (value, min, max) in [
            (self.sample_width, 32.0, 1024.0),
            (self.sample_height, 32.0, 1024.0),
            (self.gap, 0.0, 64.0),
            (self.group_header_height, 16.0, 128.0),
        ] {
            if !value.is_finite() || !(min..=max).contains(&value) {
                return Err(invalid(
                    "Gallery dimensions exceed their finite logical-pixel bounds",
                ));
            }
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GalleryModel {
    groups: Vec<GalleryGroup>,
    items: Vec<GalleryItem>,
}
impl GalleryModel {
    pub fn new(groups: Vec<GalleryGroup>, items: Vec<GalleryItem>) -> Result<Self, ControlError> {
        let model = Self { groups, items };
        model.validate()?;
        Ok(model)
    }
    pub fn groups(&self) -> &[GalleryGroup] {
        &self.groups
    }
    pub fn items(&self) -> &[GalleryItem] {
        &self.items
    }
    pub(super) fn item(&self, key: &str) -> Option<&GalleryItem> {
        self.items.iter().find(|i| i.item.key == key)
    }
    pub fn validate(&self) -> Result<(), ControlError> {
        if self.items.len() > MAX_GALLERY_ITEMS || self.groups.len() > MAX_GALLERY_GROUPS {
            return Err(invalid("Gallery exceeds 256 samples or 32 groups"));
        }
        let mut groups = HashSet::new();
        let mut keys = HashSet::new();
        let mut bytes = 0usize;
        for group in &self.groups {
            validate_key(&group.key)?;
            if !groups.insert(group.key.as_str()) {
                return Err(invalid("Duplicate gallery group"));
            }
            bytes += group.key.len() + group.header.len();
            validate_text(&group.header)?;
        }
        for item in &self.items {
            validate_key(&item.item.key)?;
            if !keys.insert(item.item.key.as_str()) {
                return Err(invalid("Duplicate gallery sample key"));
            }
            if !item.group.is_empty() && !groups.contains(item.group.as_str()) {
                return Err(invalid("Gallery sample refers to an unknown group"));
            }
            if item.icon.is_some() && item.image_resource.is_some() {
                return Err(invalid(
                    "A gallery sample uses either Icon or ImageResource",
                ));
            }
            if item.item.columns.len() > 31 {
                return Err(invalid("Gallery supports at most 31 additional columns"));
            }
            bytes += item.item.key.len() + item.group.len();
            for value in std::iter::once(&item.item.text)
                .chain(item.item.columns.iter())
                .chain(item.image_resource.iter())
            {
                validate_text(value)?;
                bytes += value.len();
            }
            if item.image_resource.as_ref().is_some_and(String::is_empty) {
                return Err(invalid("Image resource key cannot be empty"));
            }
        }
        if bytes > 64 * 1024 {
            return Err(invalid("Gallery metadata exceeds 64 KiB"));
        }
        Ok(())
    }
    pub(super) fn order(&self) -> Vec<usize> {
        std::iter::once("")
            .chain(self.groups.iter().map(|g| g.key.as_str()))
            .flat_map(|group| {
                self.items
                    .iter()
                    .enumerate()
                    .filter_map(move |(i, item)| (item.group == group).then_some(i))
            })
            .collect()
    }
}
pub(super) fn validate_key(key: &str) -> Result<(), ControlError> {
    if key.is_empty()
        || key.len() > 128
        || !key
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"_.-/".contains(&c))
    {
        return Err(invalid(
            "Gallery keys require 1..128 ASCII identifier bytes",
        ));
    }
    Ok(())
}
pub(super) fn validate_text(text: &str) -> Result<(), ControlError> {
    if text.len() > 4096 || text.chars().any(char::is_control) {
        return Err(invalid(
            "Gallery captions require at most 4096 printable UTF-8 bytes",
        ));
    }
    Ok(())
}
pub(super) fn validate_selection(
    model: &GalleryModel,
    key: Option<&str>,
) -> Result<(), ControlError> {
    if let Some(key) = key
        && model.item(key).is_none_or(|item| !item.item.enabled)
    {
        return Err(invalid("Unknown or disabled gallery sample"));
    }
    Ok(())
}
pub(super) fn invalid(message: impl Into<String>) -> ControlError {
    ControlError::Invalid(message.into())
}
