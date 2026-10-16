use super::*;
use rust_desktop_ui_core::{ImagePrimitive, ImageSampling};
use rust_desktop_ui_images::{
    DecodeOptions, ImageAsset, ImageCache, ImageCacheStats, ImageError, ImageFormat, ImageSource,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ImageStretch {
    None,
    Fill,
    #[default]
    Uniform,
    UniformToFill,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ImageSettings {
    pub stretch: ImageStretch,
    pub sampling: ImageSampling,
    pub opacity: f32,
    /// Opaque SVG currentColor override. `None` follows the theme text color.
    pub current_color: Option<Color>,
}
impl Default for ImageSettings {
    fn default() -> Self {
        Self {
            stretch: ImageStretch::Uniform,
            sampling: ImageSampling::Linear,
            opacity: 1.0,
            current_color: None,
        }
    }
}
impl ImageSettings {
    pub fn validate(self) -> Result<(), ControlError> {
        if !self.opacity.is_finite()
            || !(0.0..=1.0).contains(&self.opacity)
            || self.current_color.is_some_and(|color| {
                color.a != 1.0
                    || [color.r, color.g, color.b]
                        .into_iter()
                        .any(|v| !v.is_finite() || !(0.0..=1.0).contains(&v))
            })
        {
            return Err(ControlError::Invalid(
                "Image opacity must be finite in [0,1]; currentColor must be opaque valid RGB"
                    .into(),
            ));
        }
        Ok(())
    }
}
#[derive(Clone)]
enum Input {
    Source(ImageSource),
    Resource(String),
}
struct ImageControl {
    input: Input,
    settings: ImageSettings,
    asset: Option<ImageAsset>,
    error: Option<ImageError>,
    attempted: Option<(Option<u64>, DecodeOptions)>,
    budget_generation: Option<u64>,
    auto_width: bool,
    auto_height: bool,
    applied_width: Length,
    applied_height: Length,
}
pub(super) struct ImageControls {
    controls: HashMap<WidgetId, ImageControl>,
    resources: HashMap<String, ImageSource>,
    cache: ImageCache,
    scale: f32,
    live_generation: u64,
}
impl Default for ImageControls {
    fn default() -> Self {
        Self {
            controls: HashMap::new(),
            resources: HashMap::new(),
            cache: ImageCache::default(),
            scale: 1.0,
            live_generation: 0,
        }
    }
}
const MAX_IMAGES: usize = 4096;
const MAX_RESOURCES: usize = 256;
const MAX_RETAINED_BYTES: usize = 64 * 1024 * 1024;
fn resource_name(name: &str) -> Result<(), ControlError> {
    if name.is_empty()
        || name.len() > 256
        || !name
            .bytes()
            .all(|v| v.is_ascii_alphanumeric() || matches!(v, b'_' | b'.' | b'-'))
    {
        return Err(ControlError::Invalid(
            "Image resource name must contain 1..=256 ASCII letters, digits, '_', '.' or '-'"
                .into(),
        ));
    }
    Ok(())
}
fn source_budget<'a>(sources: impl Iterator<Item = &'a ImageSource>) -> usize {
    let mut identities = HashSet::new();
    sources
        .filter(|source| identities.insert(source.id()))
        .map(|source| source.bytes().len())
        .sum()
}
impl Controls {
    pub fn image(
        &mut self,
        parent: WidgetId,
        source: ImageSource,
        settings: ImageSettings,
        style: LayoutStyle,
        alt: impl Into<String>,
    ) -> Result<WidgetId, ControlError> {
        self.add_image(parent, Input::Source(source), settings, style, alt.into())
    }
    /// Resource names are explicit application keys, not paths or network URLs.
    /// Missing sources produce a noninteractive placeholder and `image_error`.
    pub fn image_from_resource(
        &mut self,
        parent: WidgetId,
        name: impl Into<String>,
        settings: ImageSettings,
        style: LayoutStyle,
        alt: impl Into<String>,
    ) -> Result<WidgetId, ControlError> {
        let name = name.into();
        resource_name(&name)?;
        self.add_image(parent, Input::Resource(name), settings, style, alt.into())
    }
    fn add_image(
        &mut self,
        parent: WidgetId,
        input: Input,
        settings: ImageSettings,
        style: LayoutStyle,
        alt: String,
    ) -> Result<WidgetId, ControlError> {
        settings.validate()?;
        self.prune_removed_controls()?;
        if self.images.controls.len() >= MAX_IMAGES {
            return Err(ControlError::Invalid(
                "Image control count exceeds 4096".into(),
            ));
        }
        self.check_image_source_budget(None, &input)?;
        let (auto_width, auto_height) = (style.width == Length::Auto, style.height == Length::Auto);
        let (applied_width, applied_height) = (style.width, style.height);
        let id = self.add(parent, Kind::Panel, alt, None, style)?;
        let control = self.controls.get_mut(&id).expect("new image");
        control.transparent_panel = true;
        control.role = Some(Role::Image);
        self.edit_props(id, |props| {
            props.hit_test = false;
            props.focusable = false;
        })?;
        self.images.controls.insert(
            id,
            ImageControl {
                input,
                settings,
                asset: None,
                error: None,
                attempted: None,
                budget_generation: None,
                auto_width,
                auto_height,
                applied_width,
                applied_height,
            },
        );
        self.refresh_image(id)?;
        self.scene_dirty = true;
        Ok(id)
    }
    /// Registry retains at most 256 distinct names and 64 MiB encoded bytes.
    /// Replacing a resource updates all its consumers on the next update.
    pub fn register_image_source(
        &mut self,
        name: impl Into<String>,
        source: ImageSource,
    ) -> Result<(), ControlError> {
        let name = name.into();
        resource_name(&name)?;
        if !self.images.resources.contains_key(&name)
            && self.images.resources.len() >= MAX_RESOURCES
        {
            return Err(ControlError::Invalid(
                "Image resources exceed 256 names".into(),
            ));
        }
        let bytes = source_budget(
            self.images
                .resources
                .iter()
                .filter(|(key, _)| **key != name)
                .map(|(_, source)| source)
                .chain(std::iter::once(&source)),
        );
        if bytes > MAX_RETAINED_BYTES {
            return Err(ControlError::Invalid(
                "Image resource encoded bytes exceed 64 MiB".into(),
            ));
        }
        self.images.resources.insert(name, source);
        self.scene_dirty = true;
        Ok(())
    }
    pub fn unregister_image_source(&mut self, name: &str) -> bool {
        let removed = self.images.resources.remove(name).is_some();
        self.scene_dirty |= removed;
        removed
    }
    pub fn image_source(&self, name: &str) -> Option<&ImageSource> {
        self.images.resources.get(name)
    }
    /// Resolve intrinsic logical dimensions through the same bounded cache used
    /// by image controls. Missing/invalid assets return a decoding diagnostic.
    pub fn image_resource_size(&mut self, name: &str) -> Result<Size, ImageError> {
        self.image_resource_asset(name)
            .map(|asset| asset.natural_size())
    }
    /// Decode a registered image through the shared bounded cache. The immutable
    /// bitmap can be shared by a tooltip or another content presenter.
    pub fn image_resource_asset(&mut self, name: &str) -> Result<ImageAsset, ImageError> {
        let source = self
            .images
            .resources
            .get(name)
            .ok_or_else(|| ImageError::Invalid("Image resource is not registered".into()))?;
        let options = if source.format() == ImageFormat::Svg {
            let color = self.theme.tokens().text;
            DecodeOptions {
                svg_scale: self.images.scale,
                current_color: [color.r, color.g, color.b].map(|v| (v * 255.0).round() as u8),
            }
        } else {
            DecodeOptions::default()
        };
        self.images.cache.load(source, options)
    }
    fn require_image(&self, id: WidgetId) -> Result<(), ControlError> {
        if self.runtime.tree().contains(id) && self.images.controls.contains_key(&id) {
            Ok(())
        } else {
            Err(ControlError::Invalid("Unknown image control".into()))
        }
    }
    fn check_image_source_budget(
        &self,
        replacing: Option<WidgetId>,
        proposed: &Input,
    ) -> Result<(), ControlError> {
        let existing = self
            .images
            .controls
            .iter()
            .filter(|(id, _)| Some(**id) != replacing)
            .filter_map(|(_, image)| {
                if let Input::Source(source) = &image.input {
                    Some(source)
                } else {
                    None
                }
            });
        let proposed = if let Input::Source(source) = proposed {
            Some(source)
        } else {
            None
        };
        if source_budget(existing.chain(proposed)) > MAX_RETAINED_BYTES {
            return Err(ControlError::Invalid(
                "Image controls retain more than 64 MiB encoded sources".into(),
            ));
        }
        Ok(())
    }
    /// Decode failure records a diagnostic and keeps the last good asset.
    /// Invalid ID/settings/resource budgets return an error without changes.
    pub fn set_image_source(
        &mut self,
        id: WidgetId,
        source: ImageSource,
    ) -> Result<(), ControlError> {
        self.set_image_input(id, Input::Source(source))
    }
    pub fn set_image_resource(
        &mut self,
        id: WidgetId,
        name: impl Into<String>,
    ) -> Result<(), ControlError> {
        let name = name.into();
        resource_name(&name)?;
        self.set_image_input(id, Input::Resource(name))
    }
    fn set_image_input(&mut self, id: WidgetId, input: Input) -> Result<(), ControlError> {
        self.require_image(id)?;
        self.check_image_source_budget(Some(id), &input)?;
        self.images.controls.get_mut(&id).unwrap().input = input;
        self.refresh_image(id)?;
        self.scene_dirty = true;
        Ok(())
    }
    pub fn set_image_settings(
        &mut self,
        id: WidgetId,
        settings: ImageSettings,
    ) -> Result<(), ControlError> {
        self.require_image(id)?;
        settings.validate()?;
        self.images.controls.get_mut(&id).unwrap().settings = settings;
        self.refresh_image(id)?;
        self.scene_dirty = true;
        Ok(())
    }
    pub fn image_settings(&self, id: WidgetId) -> Option<ImageSettings> {
        self.require_image(id).ok()?;
        Some(self.images.controls[&id].settings)
    }
    pub fn image_asset(&self, id: WidgetId) -> Option<&ImageAsset> {
        self.require_image(id).ok()?;
        self.images.controls[&id].asset.as_ref()
    }
    pub fn image_error(&self, id: WidgetId) -> Option<&ImageError> {
        self.require_image(id).ok()?;
        self.images.controls[&id].error.as_ref()
    }
    pub fn image_cache_stats(&self) -> ImageCacheStats {
        self.images.cache.stats()
    }
    /// Changes SVG raster resolution while preserving logical intrinsic size.
    pub fn set_image_scale(&mut self, scale: f32) -> Result<(), ControlError> {
        DecodeOptions {
            svg_scale: scale,
            ..Default::default()
        }
        .validate()
        .map_err(|e| ControlError::Invalid(e.to_string()))?;
        if self.images.scale != scale {
            self.images.scale = scale;
            self.scene_dirty = true;
        }
        Ok(())
    }
    pub(super) fn retain_images(&mut self) {
        let mut released = false;
        self.images.controls.retain(|id, image| {
            let keep = self.controls.contains_key(id);
            released |= !keep && image.asset.is_some();
            keep
        });
        if released {
            self.images.live_generation = self.images.live_generation.wrapping_add(1);
        }
    }
    pub(super) fn refresh_images(&mut self) -> Result<(), ControlError> {
        let ids: Vec<_> = self.images.controls.keys().copied().collect();
        for id in ids {
            self.refresh_image(id)?;
        }
        Ok(())
    }
    fn refresh_image(&mut self, id: WidgetId) -> Result<(), ControlError> {
        let image = &self.images.controls[&id];
        let source = match &image.input {
            Input::Source(source) => Some(source.clone()),
            Input::Resource(name) => self.images.resources.get(name).cloned(),
        };
        let color = image
            .settings
            .current_color
            .unwrap_or(self.theme.tokens().text);
        let options = if source
            .as_ref()
            .is_some_and(|source| source.format() == ImageFormat::Svg)
        {
            DecodeOptions {
                svg_scale: self.images.scale,
                current_color: [color.r, color.g, color.b].map(|v| (v * 255.0).round() as u8),
            }
        } else {
            DecodeOptions::default()
        };
        let key = (source.as_ref().map(ImageSource::id), options);
        if image.attempted != Some(key)
            || image
                .budget_generation
                .is_some_and(|generation| generation != self.images.live_generation)
        {
            let result = match source {
                Some(source) => self.images.cache.load(&source, options),
                None => Err(ImageError::Invalid(
                    "Image resource is not registered".into(),
                )),
            };
            let result = result.and_then(|asset| {
                let mut identities = HashSet::new();
                identities.insert(asset.bitmap().id());
                let bytes = asset.bitmap().pixels().len()
                    + self
                        .images
                        .controls
                        .iter()
                        .filter(|(other, _)| **other != id)
                        .filter_map(|(_, image)| image.asset.as_ref())
                        .filter(|asset| identities.insert(asset.bitmap().id()))
                        .map(|asset| asset.bitmap().pixels().len())
                        .sum::<usize>();
                if bytes > MAX_RETAINED_BYTES {
                    Err(ImageError::Limit("live image assets exceed 64 MiB"))
                } else {
                    Ok(asset)
                }
            });
            let image = self.images.controls.get_mut(&id).unwrap();
            image.attempted = Some(key);
            image.budget_generation = if matches!(
                &result,
                Err(ImageError::Limit("live image assets exceed 64 MiB"))
            ) {
                Some(self.images.live_generation)
            } else {
                None
            };
            match result {
                Ok(asset) => {
                    if image
                        .asset
                        .as_ref()
                        .is_none_or(|previous| previous.bitmap() != asset.bitmap())
                    {
                        self.images.live_generation = self.images.live_generation.wrapping_add(1);
                    }
                    image.asset = Some(asset);
                    image.error = None;
                }
                Err(error) => {
                    image.error = Some(error);
                }
            }
            self.scene_dirty = true;
        }
        let image = self.images.controls.get_mut(&id).unwrap();
        let mut style = self
            .runtime
            .tree()
            .node(id)
            .expect("live image")
            .props
            .style
            .clone();
        // An externally changed explicit axis becomes application-owned. Values
        // equal to the last generated size have no distinguishable provenance.
        image.auto_width =
            style.width == Length::Auto || (image.auto_width && style.width == image.applied_width);
        image.auto_height = style.height == Length::Auto
            || (image.auto_height && style.height == image.applied_height);
        let natural = image
            .asset
            .as_ref()
            .map_or(Size::new(32.0, 32.0), ImageAsset::natural_size);
        let mut desired = natural;
        if image.auto_width
            && !image.auto_height
            && let Length::Px(height) = style.height
        {
            desired.width = (f64::from(height) * f64::from(natural.width)
                / f64::from(natural.height))
            .min(f64::from(f32::MAX)) as f32;
        }
        if image.auto_height
            && !image.auto_width
            && let Length::Px(width) = style.width
        {
            desired.height = (f64::from(width) * f64::from(natural.height)
                / f64::from(natural.width))
            .min(f64::from(f32::MAX)) as f32;
        }
        if image.auto_width {
            style.width = Length::Px(desired.width);
        }
        if image.auto_height {
            style.height = Length::Px(desired.height);
        }
        image.applied_width = style.width;
        image.applied_height = style.height;
        if style != self.runtime.tree().node(id).unwrap().props.style {
            self.runtime.apply(Mutation::SetStyle { node: id, style })?;
            self.scene_dirty = true;
        }
        Ok(())
    }
    pub(super) fn paint_image(&mut self, id: WidgetId, bounds: Rect, clip: Rect) -> bool {
        let Some(image) = self.images.controls.get(&id) else {
            return false;
        };
        let Some(clip) = clip.intersection(bounds) else {
            return true;
        };
        if let Some(asset) = &image.asset {
            let natural = asset.natural_size();
            let (width, height) = match image.settings.stretch {
                ImageStretch::None => (natural.width, natural.height),
                ImageStretch::Fill => (bounds.width, bounds.height),
                ImageStretch::Uniform | ImageStretch::UniformToFill => {
                    let x = bounds.width / natural.width;
                    let y = bounds.height / natural.height;
                    let scale = if image.settings.stretch == ImageStretch::Uniform {
                        x.min(y)
                    } else {
                        x.max(y)
                    };
                    (natural.width * scale, natural.height * scale)
                }
            };
            let rect = Rect::new(
                bounds.x + (bounds.width - width) / 2.0,
                bounds.y + (bounds.height - height) / 2.0,
                width,
                height,
            );
            let mut primitive = ImagePrimitive::new(asset.bitmap().clone(), rect);
            primitive.clip = clip;
            primitive.opacity = image.settings.opacity;
            primitive.sampling = image.settings.sampling;
            self.scene.image(primitive);
        } else {
            let mut color = self.theme.tokens().muted;
            color.a *= image.settings.opacity;
            self.scene.rounded_stroke(bounds, clip, 3.0, 1.0, color);
            let icon = Rect::new(
                bounds.x + bounds.width * 0.25,
                bounds.y + bounds.height * 0.25,
                bounds.width * 0.5,
                bounds.height * 0.5,
            );
            self.scene.rounded_stroke(icon, clip, 1.0, 1.0, color);
        }
        true
    }
}

#[cfg(test)]
mod tests;
