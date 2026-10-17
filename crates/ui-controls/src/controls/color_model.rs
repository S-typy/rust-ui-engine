//! Exact color/brush values, bounded settings and stable gradient-stop identity.
use super::*;
pub use rust_desktop_ui_core::{BrushValue, GradientStop, LinearGradient, Srgba8};

#[derive(Clone, Debug, PartialEq)]
pub struct ColorEditorSettings {
    pub allow_empty: bool,
    pub is_read_only: bool,
    pub palette: Vec<Srgba8>,
    pub recent_limit: usize,
    pub popup_size: Size,
}
impl Default for ColorEditorSettings {
    fn default() -> Self {
        Self {
            allow_empty: true,
            is_read_only: false,
            palette: [
                0x000000, 0x444444, 0x888888, 0xffffff, 0xe53935, 0xfb8c00, 0xfdd835, 0x43a047,
                0x00897b, 0x00acc1, 0x1e88e5, 0x3949ab, 0x8e24aa, 0xd81b60, 0x795548, 0x607d8b,
            ]
            .map(|n| Srgba8::rgb((n >> 16) as u8, (n >> 8) as u8, n as u8))
            .to_vec(),
            recent_limit: 16,
            popup_size: Size::new(520.0, 640.0),
        }
    }
}
impl ColorEditorSettings {
    pub fn validate(&self) -> Result<(), ControlError> {
        if self.palette.len() > 128
            || self.recent_limit > 32
            || !self.popup_size.is_valid()
            || !(320.0..=1024.0).contains(&self.popup_size.width)
            || !(240.0..=1024.0).contains(&self.popup_size.height)
        {
            return Err(error(
                "Color editor requires palette <=128, recent <=32 and popup width 320..1024/height 240..1024",
            ));
        }
        Ok(())
    }
    pub fn validate_value(&self, value: Option<&BrushValue>) -> Result<(), ControlError> {
        if value.is_none() && !self.allow_empty {
            Err(error("A color or brush is required"))
        } else {
            Ok(())
        }
    }
}

/// Replacing a dirty value requires an explicit caller decision.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ColorReplacePolicy {
    #[default]
    RejectDirty,
    CancelPending,
}

/// Identity belongs to a pending session; moving a stop preserves this key.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct GradientStopId(pub(crate) u64);
impl GradientStopId {
    pub fn get(self) -> u64 {
        self.0
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PendingGradientStop {
    pub id: GradientStopId,
    pub offset: f64,
    pub color: Srgba8,
}

#[derive(Clone)]
pub(super) struct Draft {
    pub value: Option<BrushValue>,
    pub stops: Vec<PendingGradientStop>,
    pub selected: Option<GradientStopId>,
    next: u64,
}
impl Draft {
    pub fn new(value: Option<BrushValue>) -> Self {
        let mut draft = Self {
            value: None,
            stops: Vec::new(),
            selected: None,
            next: 1,
        };
        draft.replace(value).expect("fresh bounded gradient IDs");
        draft
    }
    pub fn reset(&mut self, value: Option<BrushValue>) {
        self.value = value;
        self.stops.clear();
        self.selected = None;
    }
    pub fn replace(&mut self, value: Option<BrushValue>) -> Result<(), ControlError> {
        let count = match &value {
            Some(BrushValue::Linear(g)) => g.stops().len() as u64,
            _ => 0,
        };
        self.next
            .checked_add(count)
            .ok_or_else(|| error("Gradient stop identity budget exhausted"))?;
        self.stops.clear();
        if let Some(BrushValue::Linear(g)) = &value {
            for stop in g.stops() {
                self.stops.push(PendingGradientStop {
                    id: GradientStopId(self.next),
                    offset: stop.offset,
                    color: stop.color,
                });
                self.next += 1;
            }
        }
        self.selected = self.stops.first().map(|s| s.id);
        self.value = value;
        Ok(())
    }
    pub fn color(&self) -> Srgba8 {
        match &self.value {
            Some(BrushValue::Solid(color)) => *color,
            Some(BrushValue::Linear(_)) => self
                .stops
                .iter()
                .find(|s| Some(s.id) == self.selected)
                .map_or(Srgba8::BLACK, |s| s.color),
            None => Srgba8::BLACK,
        }
    }
    pub fn set_color(&mut self, color: Srgba8) {
        if let Some(BrushValue::Linear(g)) = &self.value {
            let angle = g.angle();
            if let Some(stop) = self.stops.iter_mut().find(|s| Some(s.id) == self.selected) {
                stop.color = color;
            }
            self.rebuild(angle);
        } else {
            self.value = Some(BrushValue::Solid(color));
        }
    }
    fn rebuild(&mut self, angle: f64) {
        self.value = Some(BrushValue::Linear(
            LinearGradient::new(
                angle,
                self.stops
                    .iter()
                    .map(|s| GradientStop::new(s.offset, s.color))
                    .collect(),
            )
            .unwrap(),
        ));
    }
    pub fn select(&mut self, id: GradientStopId) -> Result<(), ControlError> {
        if !self.stops.iter().any(|s| s.id == id) {
            return Err(error("Unknown gradient stop"));
        }
        self.selected = Some(id);
        Ok(())
    }
    pub fn update_stop(
        &mut self,
        id: GradientStopId,
        offset: f64,
        color: Srgba8,
    ) -> Result<(), ControlError> {
        if !offset.is_finite() || !(0.0..=1.0).contains(&offset) {
            return Err(error("Gradient offset requires a finite number in 0..1"));
        }
        let Some(BrushValue::Linear(g)) = &self.value else {
            return Err(error("Not a linear gradient"));
        };
        let angle = g.angle();
        let stop = self
            .stops
            .iter_mut()
            .find(|s| s.id == id)
            .ok_or_else(|| error("Unknown gradient stop"))?;
        stop.offset = offset;
        stop.color = color;
        self.stops.sort_by(|a, b| a.offset.total_cmp(&b.offset));
        self.rebuild(angle);
        Ok(())
    }
    pub fn add(&mut self, offset: f64, color: Srgba8) -> Result<GradientStopId, ControlError> {
        let Some(BrushValue::Linear(g)) = &self.value else {
            return Err(error("Not a linear gradient"));
        };
        if self.stops.len() >= 32
            || !offset.is_finite()
            || !(0.0..=1.0).contains(&offset)
            || self.next == u64::MAX
        {
            return Err(error(
                "Gradient requires at most 32 stops and finite offsets in 0..1",
            ));
        }
        let angle = g.angle();
        let id = GradientStopId(self.next);
        self.next += 1;
        self.stops.push(PendingGradientStop { id, offset, color });
        self.stops.sort_by(|a, b| a.offset.total_cmp(&b.offset));
        self.selected = Some(id);
        self.rebuild(angle);
        Ok(id)
    }
    pub fn remove(&mut self, id: GradientStopId) -> Result<(), ControlError> {
        let Some(BrushValue::Linear(g)) = &self.value else {
            return Err(error("Not a linear gradient"));
        };
        let index = self
            .stops
            .iter()
            .position(|s| s.id == id)
            .ok_or_else(|| error("Unknown gradient stop"))?;
        if self.stops.len() <= 2 {
            return Err(error("A gradient requires at least two stops"));
        }
        let angle = g.angle();
        self.stops.remove(index);
        if self.selected == Some(id) {
            self.selected = Some(self.stops[index.min(self.stops.len() - 1)].id);
        }
        self.rebuild(angle);
        Ok(())
    }
    pub fn angle(&mut self, angle: f64) -> Result<(), ControlError> {
        if !angle.is_finite() || !(0.0..=360.0).contains(&angle) {
            return Err(error("Gradient angle requires a finite number in 0..360"));
        }
        if !matches!(self.value, Some(BrushValue::Linear(_))) {
            return Err(error("Not a linear gradient"));
        }
        self.rebuild(angle);
        Ok(())
    }
}
// Preserve f64 round trips without expanding subnormal values beyond editor limits.
pub(super) fn compact_number(value: f64) -> String {
    let text = value.to_string();
    if text.len() <= 24 {
        text
    } else {
        format!("{value:e}")
    }
}
pub(super) fn error(message: impl Into<String>) -> ControlError {
    ControlError::Invalid(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stop_identity_exhaustion_fails_before_reset_or_reuse() {
        let mut draft = Draft::new(Some(BrushValue::Linear(LinearGradient::default())));
        let old = draft.stops.clone();
        let value = draft.value.clone();
        draft.next = u64::MAX - 1;
        assert!(
            draft
                .replace(Some(BrushValue::Linear(LinearGradient::default())))
                .is_err()
        );
        assert_eq!(draft.stops, old);
        assert_eq!(draft.value, value);
        draft.next = u64::MAX;
        assert!(draft.add(0.5, Srgba8::BLACK).is_err());
        assert_eq!(draft.stops, old);
    }
}
