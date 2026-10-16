//! Application-owned intrinsic sizing for leaf controls.
use super::*;
use rust_desktop_ui_core::LayoutError;
use rust_desktop_ui_layout::MeasureInput;

impl Controls {
    /// Attach intrinsic content measurement to a leaf control. The callback may
    /// run repeatedly with different available-space constraints. It must be
    /// deterministic for the current content, finite and nonnegative.
    pub fn set_measure(
        &mut self,
        id: WidgetId,
        measure: impl FnMut(MeasureInput) -> Result<Size, LayoutError> + 'static,
    ) -> Result<(), ControlError> {
        let node = self
            .runtime
            .tree()
            .node(id)
            .ok_or_else(|| ControlError::Invalid("Unknown control".into()))?;
        if node.props.style.kind != LayoutKind::Leaf
            || !self
                .runtime
                .tree()
                .children(id)
                .unwrap_or_default()
                .is_empty()
        {
            return Err(ControlError::Invalid(
                "Custom measurement requires a leaf layout node".into(),
            ));
        }
        self.layout
            .set_measure(id, measure)
            .map_err(RuntimeError::from)?;
        self.runtime.invalidate_measure(id)?;
        Ok(())
    }
    /// Remove intrinsic measurement without recreating the retained control.
    pub fn remove_measure(&mut self, id: WidgetId) -> Result<bool, ControlError> {
        if !self.runtime.tree().contains(id) {
            return Err(ControlError::Invalid("Unknown control".into()));
        }
        let removed = self.layout.remove_measure(id);
        if removed {
            self.runtime.invalidate_measure(id)?;
        }
        Ok(removed)
    }
    /// Notify layout after the callback's externally owned content has changed.
    pub fn invalidate_measure(&mut self, id: WidgetId) -> Result<(), ControlError> {
        self.runtime.invalidate_measure(id)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::Cell, rc::Rc};
    #[test]
    fn leaf_measure_tracks_external_content_and_unsubscribes_on_removal() {
        let mut ui = Controls::new(Theme::Light).unwrap();
        let leaf = ui
            .panel(
                ui.root(),
                LayoutStyle {
                    kind: LayoutKind::Leaf,
                    horizontal_alignment: Some(rust_desktop_ui_core::Align::Start),
                    ..Default::default()
                },
            )
            .unwrap();
        // Panel construction selects a panel kind; explicitly opt the empty
        // retained control into leaf measurement before registration.
        ui.edit_props(leaf, |p| p.style.kind = LayoutKind::Leaf)
            .unwrap();
        let size = Rc::new(Cell::new(Size::new(60.0, 24.0)));
        let measured = size.clone();
        ui.set_measure(leaf, move |_| Ok(measured.get())).unwrap();
        ui.update(Size::new(300.0, 200.0)).unwrap();
        assert_eq!(ui.runtime.layout_bounds(leaf).unwrap().width, 60.0);
        size.set(Size::new(100.0, 30.0));
        ui.invalidate_measure(leaf).unwrap();
        ui.update(Size::new(300.0, 200.0)).unwrap();
        assert_eq!(ui.runtime.layout_bounds(leaf).unwrap().width, 100.0);
        assert_eq!(ui.update(Size::new(300.0, 200.0)).unwrap().layout_passes, 0);
        ui.remove(leaf).unwrap();
        assert_eq!(Rc::strong_count(&size), 1);
        assert!(ui.invalidate_measure(leaf).is_err());
    }
}
