use super::spectrum::{Hsv, SpectrumPart, SpectrumRaster};
use super::*;

#[derive(Clone, Debug)]
pub(super) struct ColorSpectrum {
    pub(super) host: WidgetId,
    pub(super) plane: WidgetId,
    pub(super) strip: WidgetId,
    hsv: Hsv,
    raster: SpectrumRaster,
}

impl ColorSpectrum {
    fn part(&self, id: WidgetId) -> Option<SpectrumPart> {
        if id == self.plane {
            Some(SpectrumPart::Plane)
        } else if id == self.strip {
            Some(SpectrumPart::Hue)
        } else {
            None
        }
    }
}

impl Controls {
    pub(super) fn make_property_spectrum(
        &mut self,
        parent: WidgetId,
    ) -> Result<ColorSpectrum, ControlError> {
        let host = self.transparent_panel(parent, horizontal(152.))?;
        let plane = self.transparent_panel(host, grow())?;
        let strip = self.transparent_panel(
            host,
            LayoutStyle {
                width: Length::Px(26.),
                height: Length::Percent(1.),
                flex_shrink: 0.,
                ..Default::default()
            },
        )?;
        for part in [plane, strip] {
            self.edit_props(part, |props| props.focusable = true)?;
            self.set_role(part, Role::Group);
        }
        Ok(ColorSpectrum {
            host,
            plane,
            strip,
            hsv: Hsv::default(),
            raster: SpectrumRaster::default(),
        })
    }

    pub(super) fn sync_property_spectrum(&mut self, owner: WidgetId) -> Result<(), ControlError> {
        let entry = self.color_editors.entries.get_mut(&owner).unwrap();
        let Some(spectrum) = entry
            .popup
            .as_mut()
            .and_then(|popup| popup.spectrum.as_mut())
        else {
            return Ok(());
        };
        // Preserve sub-byte coordinates, including hue at zero saturation, when
        // the exact RGB fields still represent the same spectrum point.
        let color = entry.draft.color();
        if spectrum.hsv.color(color.a) != color {
            spectrum.hsv = Hsv::from_color(color, spectrum.hsv);
        }
        let (hsv, plane, strip) = (spectrum.hsv, spectrum.plane, spectrum.strip);
        self.set_automation_name(
            plane,
            format!(
                "Насыщенность {:.0} %, яркость {:.0} %. Стрелки изменяют цвет",
                hsv.s * 100.,
                hsv.v * 100.
            ),
        )?;
        self.set_automation_name(
            strip,
            format!("Цветовой тон {:.0}°. Стрелки изменяют тон", hsv.h * 360.),
        )
    }

    fn spectrum_owner(&self, widget: WidgetId) -> Option<(WidgetId, SpectrumPart)> {
        self.color_editors
            .entries
            .iter()
            .find_map(|(&owner, entry)| {
                let popup = entry.popup.as_ref()?;
                let part = popup.spectrum.as_ref()?.part(widget)?;
                (entry.pending
                    && popup.property_palette.as_ref()?.expanded
                    && self.flyout_is_open(popup.flyout))
                .then_some((owner, part))
            })
    }

    pub(super) fn paint_property_spectrum(
        &mut self,
        widget: WidgetId,
        bounds: Rect,
        clip: Rect,
    ) -> bool {
        let Some((owner, part)) = self.spectrum_owner(widget) else {
            return false;
        };
        let spectrum = self
            .color_editors
            .entries
            .get_mut(&owner)
            .unwrap()
            .popup
            .as_mut()
            .unwrap()
            .spectrum
            .as_mut()
            .unwrap();
        spectrum
            .raster
            .paint(&mut self.scene, part, spectrum.hsv, bounds, clip);
        let color = if self.runtime.focused() == Some(widget) {
            self.theme.tokens().focus
        } else {
            self.theme.tokens().border
        };
        shape_border(&mut self.scene, bounds, clip, 0., 1., color);
        true
    }

    fn set_property_hsv(&mut self, owner: WidgetId, hsv: Hsv) -> Result<(), ControlError> {
        let entry = self.color_editors.entries.get_mut(&owner).unwrap();
        entry.popup.as_mut().unwrap().spectrum.as_mut().unwrap().hsv = hsv;
        let color = hsv.color(entry.draft.color().a);
        if entry.draft.value == Some(BrushValue::Solid(color))
            && !self.color_popup_fields_dirty(&self.color_editors.entries[&owner])
        {
            self.sync_property_spectrum(owner)
        } else {
            self.set_color_draft(owner, color)
        }
    }

    pub(super) fn property_spectrum_input(
        &mut self,
        event: &InputEvent,
    ) -> Result<bool, ControlError> {
        let target = match event {
            InputEvent::PointerDown {
                position,
                button: PointerButton::Primary,
                ..
            } => self.runtime.hit_test(*position),
            InputEvent::PointerMoved { .. }
            | InputEvent::PointerUp {
                button: PointerButton::Primary,
                ..
            } => self.runtime.captured(),
            InputEvent::KeyDown { .. } => self.runtime.focused(),
            _ => None,
        };
        let Some((widget, owner, part)) = target.and_then(|widget| {
            self.spectrum_owner(widget)
                .map(|(owner, part)| (widget, owner, part))
        }) else {
            return Ok(false);
        };
        if !self.color_editable(owner) {
            self.cancel_color_popup(owner)?;
            return Ok(true);
        }
        let popup = self.color_popup(owner).unwrap();
        if self.color_field_ids(popup).iter().any(|&field| {
            self.document(field)
                .is_some_and(|d| d.composition().is_some())
        }) {
            return Ok(false);
        }
        let mut hsv = popup.spectrum.as_ref().unwrap().hsv;
        match event {
            InputEvent::PointerDown { position, .. }
            | InputEvent::PointerMoved { position }
            | InputEvent::PointerUp { position, .. } => {
                let Some(bounds) = self.runtime.bounds(widget) else {
                    return Ok(false);
                };
                if matches!(event, InputEvent::PointerDown { .. }) {
                    self.runtime.apply(Mutation::Focus(Some(widget)))?;
                    self.synchronize_editor_focus()?;
                    self.runtime.apply(Mutation::CapturePointer(Some(widget)))?;
                }
                if matches!(event, InputEvent::PointerUp { .. }) {
                    self.runtime.apply(Mutation::CapturePointer(None))?;
                }
                hsv = hsv.at(part, bounds, *position);
            }
            InputEvent::KeyDown {
                key,
                modifiers,
                repeat,
            } if !modifiers.alt && !modifiers.control && !modifiers.super_key => {
                let step = if modifiers.shift { 0.1 } else { 0.01 };
                let degree = if modifiers.shift {
                    10. / 360.
                } else {
                    1. / 360.
                };
                match (part, key) {
                    (SpectrumPart::Plane, Key::ArrowLeft) => hsv.s = (hsv.s - step).max(0.),
                    (SpectrumPart::Plane, Key::ArrowRight) => hsv.s = (hsv.s + step).min(1.),
                    (SpectrumPart::Plane, Key::ArrowDown) => hsv.v = (hsv.v - step).max(0.),
                    (SpectrumPart::Plane, Key::ArrowUp) => hsv.v = (hsv.v + step).min(1.),
                    (SpectrumPart::Hue, Key::ArrowDown | Key::ArrowRight) => {
                        hsv.h = (hsv.h + degree).min(1.)
                    }
                    (SpectrumPart::Hue, Key::ArrowUp | Key::ArrowLeft) => {
                        hsv.h = (hsv.h - degree).max(0.)
                    }
                    (SpectrumPart::Hue, Key::Home) => hsv.h = 0.,
                    (SpectrumPart::Hue, Key::End) => hsv.h = 1.,
                    (_, Key::Enter) if !modifiers.shift => {
                        if !repeat {
                            self.apply_color_popup(owner)?;
                        }
                        return Ok(true);
                    }
                    _ => return Ok(false),
                }
            }
            _ => return Ok(false),
        }
        self.set_property_hsv(owner, hsv)?;
        Ok(true)
    }
}
