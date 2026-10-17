use super::*;
use crate::barcode::{BarcodeGeometry, BarcodeGeometryOptions, BarcodeSymbol, BarcodeSymbology};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BarcodeSettings {
    pub symbology: BarcodeSymbology,
    /// Logical Scene units per module; export/printing uses separate dot geometry.
    pub module_size: u16,
    pub bar_height: u32,
    pub show_text: bool,
}
impl Default for BarcodeSettings {
    fn default() -> Self {
        Self {
            symbology: BarcodeSymbology::Ean13,
            module_size: 2,
            bar_height: 96,
            show_text: true,
        }
    }
}
impl BarcodeSettings {
    pub fn validate_value(self, value: &str) -> Result<BarcodeSymbol, ControlError> {
        if !(1..=16).contains(&self.module_size) || !(16..=1024).contains(&self.bar_height) {
            return Err(ControlError::Invalid(
                "Barcode requires ModuleSize 1..16 and BarHeight 16..1024".into(),
            ));
        }
        let symbol = BarcodeSymbol::encode(self.symbology, value).map_err(barcode_error)?;
        self.geometry(&symbol)?;
        Ok(symbol)
    }
    fn geometry(self, symbol: &BarcodeSymbol) -> Result<BarcodeGeometry, ControlError> {
        BarcodeGeometry::new(
            symbol,
            BarcodeGeometryOptions {
                module_size: self.module_size,
                bar_height: self.bar_height,
                ..Default::default()
            },
        )
        .map_err(barcode_error)
    }
}
fn barcode_error(e: crate::barcode::BarcodeError) -> ControlError {
    ControlError::Invalid(e.to_string())
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BarcodeDisplayStatus {
    Ready,
    InsufficientSpace,
    Clipped,
}
pub(super) struct BarcodeState {
    settings: BarcodeSettings,
    symbol: BarcodeSymbol,
    geometry: BarcodeGeometry,
    owned_width: Option<f32>,
    owned_height: Option<f32>,
}
impl BarcodeState {
    fn height(&self) -> f32 {
        self.geometry.height() as f32 + if self.settings.show_text { 28.0 } else { 0.0 }
    }
    fn bounds(&self, owner: Rect) -> Rect {
        Rect::new(
            owner.x + (owner.width - self.geometry.width() as f32).max(0.0) * 0.5,
            owner.y + (owner.height - self.height()).max(0.0) * 0.5,
            self.geometry.width() as f32,
            self.height(),
        )
    }
    fn status(&self, owner: Rect, clip: Rect) -> BarcodeDisplayStatus {
        let bounds = self.bounds(owner);
        if owner.width + 0.001 < bounds.width || owner.height + 0.001 < bounds.height {
            BarcodeDisplayStatus::InsufficientSpace
        } else if bounds.x < clip.x
            || bounds.y < clip.y
            || bounds.x + bounds.width > clip.x + clip.width
            || bounds.y + bounds.height > clip.y + clip.height
        {
            BarcodeDisplayStatus::Clipped
        } else {
            BarcodeDisplayStatus::Ready
        }
    }
}
impl Controls {
    pub fn barcode(
        &mut self,
        parent: WidgetId,
        value: impl Into<String>,
        settings: BarcodeSettings,
        mut style: LayoutStyle,
    ) -> Result<WidgetId, ControlError> {
        let symbol = settings.validate_value(&value.into())?;
        let geometry = settings.geometry(&symbol)?;
        let mut state = BarcodeState {
            settings,
            symbol,
            geometry,
            owned_width: None,
            owned_height: None,
        };
        let label = format!("{} · {}", settings.symbology.name(), state.symbol.data());
        self.preflight_new_editor_text(&label, &style, false)?;
        state.owned_width = (style.width == Length::Auto).then_some(state.geometry.width() as f32);
        state.owned_height = (style.height == Length::Auto).then_some(state.height());
        if let Some(width) = state.owned_width {
            style.width = Length::Px(width);
        }
        if let Some(height) = state.owned_height {
            style.height = Length::Px(height);
        }
        style.padding = Edges::all(0.0);
        let id = self.transparent_panel(parent, style)?;
        self.set_role(id, Role::Image);
        self.controls.get_mut(&id).unwrap().label = label;
        self.barcodes.insert(id, state);
        Ok(id)
    }
    pub fn barcode_settings(&self, id: WidgetId) -> Option<BarcodeSettings> {
        self.barcodes.get(&id).map(|s| s.settings)
    }
    pub fn barcode_value(&self, id: WidgetId) -> Option<&str> {
        self.barcodes.get(&id).map(|s| s.symbol.data())
    }
    pub fn barcode_symbol(&self, id: WidgetId) -> Option<&BarcodeSymbol> {
        self.barcodes.get(&id).map(|s| &s.symbol)
    }
    pub fn barcode_geometry(&self, id: WidgetId) -> Option<&BarcodeGeometry> {
        self.barcodes.get(&id).map(|s| &s.geometry)
    }
    pub fn barcode_display_status(&self, id: WidgetId) -> Option<BarcodeDisplayStatus> {
        Some(
            self.barcodes
                .get(&id)?
                .status(self.runtime.bounds(id)?, self.runtime.clip(id)?),
        )
    }
    /// Replace format/settings and data together. Failure preserves data and layout.
    pub fn set_barcode(
        &mut self,
        id: WidgetId,
        value: &str,
        settings: BarcodeSettings,
    ) -> Result<(), ControlError> {
        let old = self
            .barcodes
            .get(&id)
            .ok_or_else(|| ControlError::Invalid("Not a Barcode".into()))?;
        let symbol = settings.validate_value(value)?;
        let geometry = settings.geometry(&symbol)?;
        let (width, height) = (old.owned_width, old.owned_height);
        let mut state = BarcodeState {
            settings,
            symbol,
            geometry,
            owned_width: None,
            owned_height: None,
        };
        let label = format!("{} · {}", settings.symbology.name(), state.symbol.data());
        let props = self
            .runtime
            .tree()
            .node(id)
            .ok_or_else(|| ControlError::Invalid("Removed Barcode".into()))?
            .props
            .clone();
        self.preflight_new_editor_text(&label, &props.style, false)?;
        state.owned_width = width
            .filter(|v| props.style.width == Length::Px(*v))
            .map(|_| state.geometry.width() as f32);
        state.owned_height = height
            .filter(|v| props.style.height == Length::Px(*v))
            .map(|_| state.height());
        self.edit_props(id, |p| {
            if let Some(width) = state.owned_width {
                p.style.width = Length::Px(width);
            }
            if let Some(height) = state.owned_height {
                p.style.height = Length::Px(height);
            }
        })?;
        self.controls.get_mut(&id).unwrap().label = label;
        self.barcodes.insert(id, state);
        self.scene_dirty = true;
        Ok(())
    }
    pub fn set_barcode_value(&mut self, id: WidgetId, value: &str) -> Result<(), ControlError> {
        let settings = self
            .barcode_settings(id)
            .ok_or_else(|| ControlError::Invalid("Not a Barcode".into()))?;
        self.set_barcode(id, value, settings)
    }
    pub(super) fn retain_barcodes(&mut self) {
        self.barcodes
            .retain(|id, _| self.runtime.tree().node(*id).is_some());
    }
    pub(super) fn paint_barcode(
        &mut self,
        id: WidgetId,
        bounds: Rect,
        clip: Rect,
    ) -> Result<bool, ControlError> {
        let Some(state) = self.barcodes.get(&id) else {
            return Ok(false);
        };
        if state.status(bounds, clip) != BarcodeDisplayStatus::Ready {
            let mut run = TextRun::new(
                "Недостаточно места для штрихкода",
                bounds,
                self.theme.tokens().muted,
                12.0,
            );
            run.clip = clip;
            run.wrap = true;
            self.scene.text(run);
            return Ok(true);
        }
        let area = state.bounds(bounds);
        state
            .geometry
            .append_scene(&mut self.scene, Point::new(area.x, area.y), 1.0, clip)
            .map_err(barcode_error)?;
        if state.settings.show_text {
            let mut run = TextRun::new(
                state.symbol.data(),
                Rect::new(
                    area.x,
                    area.y + state.geometry.height() as f32 + 4.0,
                    area.width,
                    24.0,
                ),
                self.theme.tokens().text,
                14.0,
            );
            run.clip = clip;
            run.wrap = false;
            run.align = TextAlign::Center;
            self.scene.text(run);
        }
        Ok(true)
    }
}
