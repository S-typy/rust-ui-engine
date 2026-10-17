//! Screen-space coordination while the source native window retains capture.
use super::*;
use rust_desktop_ui_core::{Key, PointerButton};

pub(super) struct DragSession {
    pub source: DockWindowId,
    pub pane: DockPaneId,
    pub pointer: DesktopPointer,
    pub target: Option<(DockWindowId, DockDropPreview)>,
}

impl Workspace {
    fn drag_target(&self, pointer: DesktopPointer) -> Option<(DockWindowId, DockDropPreview)> {
        let target = DockWindowId(pointer.hit_window?.0);
        if target != DockWindowId::MAIN && self.layout.root(target).is_none() {
            return None;
        }
        let geometry = self.native_geometry.get(&target)?;
        let position = geometry.screen_to_client(pointer.screen_position)?;
        if !Rect::new(
            0.0,
            0.0,
            geometry.client_size.width,
            geometry.client_size.height,
        )
        .contains(position.x, position.y)
        {
            return None;
        }
        let preview = self.drop_targets.get(&target)?.target_at(position)?;
        Some((target, preview))
    }

    pub(super) fn refresh_drag_target(&mut self) {
        let Some(drag) = self.drag.as_ref() else {
            return;
        };
        if self.layout.window_for(&drag.pane) != Some(drag.source) {
            self.drag = None;
            return;
        }
        let target = self.drag_target(drag.pointer);
        self.drag.as_mut().expect("live drag").target = target;
    }
}

impl DockWindow {
    pub(super) fn sync_layout_revision(&mut self) -> Result<(), String> {
        let mut state = self.state.borrow_mut();
        if self.observed_revision == state.placement_revision {
            return Ok(());
        }
        // Restore/reset has already replaced the model. Discard the old split
        // rollback instead of applying it over the restored ratio.
        let _ = self
            .dock
            .cancel_interaction(&mut self.ui, &state.layout)
            .map_err(err)?;
        self.ui.cancel_pointer_interaction().map_err(err)?;
        for pane in state.layout.window_panes(self.id).unwrap_or_default() {
            state
                .panes
                .get_mut(&pane)
                .expect("catalog pane")
                .ui
                .cancel_pointer_interaction()
                .map_err(err)?;
        }
        self.mdi.cancel(&mut self.ui).map_err(err)?;
        self.mdi_geometry = None;
        self.geometry = None;
        self.action_owners.clear();
        self.hovered_pane = None;
        self.cross_drag_started = false;
        self.desktop_pointer = None;
        self.layout_pending = true;
        self.observed_revision = state.placement_revision;
        Ok(())
    }

    pub(super) fn cancel_dock_drag(&mut self) -> Result<(), String> {
        let requests = self
            .dock
            .cancel_interaction(&mut self.ui, &self.state.borrow().layout)
            .map_err(err)?;
        let mut state = self.state.borrow_mut();
        if state
            .drag
            .as_ref()
            .is_some_and(|drag| drag.source == self.id)
        {
            state.drag = None;
        }
        for request in requests {
            state.request(self.id, request)?;
        }
        self.cross_drag_started = false;
        self.desktop_pointer = None;
        Ok(())
    }

    /// Returns true only when this coordinator completed/cancelled the event.
    /// Local drag processing still detects the threshold and owns capture.
    pub(super) fn route_desktop_drag(&mut self, input: &InputEvent) -> Result<bool, String> {
        if matches!(
            input,
            InputEvent::WindowFocused(false)
                | InputEvent::KeyDown {
                    key: Key::Escape,
                    ..
                }
        ) {
            let interacting = self.dock.is_interacting();
            self.cancel_dock_drag()?;
            return Ok(interacting && !matches!(input, InputEvent::WindowFocused(_)));
        }
        if self.cross_drag_started
            && self
                .state
                .borrow()
                .drag
                .as_ref()
                .is_none_or(|drag| drag.source != self.id)
        {
            self.cancel_dock_drag()?;
            return Ok(matches!(
                input,
                InputEvent::PointerMoved { .. } | InputEvent::PointerUp { .. }
            ));
        }
        let Some(tab) = self.dock.tab_drag() else {
            return Ok(false);
        };
        if self.ui.runtime.captured() != Some(tab.widget) {
            self.cancel_dock_drag()?;
            return Ok(false);
        }
        if !tab.dragging
            || !matches!(
                input,
                InputEvent::PointerUp {
                    button: PointerButton::Primary,
                    ..
                }
            )
        {
            return Ok(false);
        }
        let Some(pointer) = self.desktop_pointer.take() else {
            // On platforms without desktop hit testing, retain local behavior.
            return Ok(false);
        };
        let target = self.state.borrow().drag_target(pointer);
        let pane = self.dock.finish_tab_drag(&mut self.ui).map_err(err)?;
        self.cross_drag_started = false;
        let mut state = self.state.borrow_mut();
        state.drag = None;
        if let Some(pane) = pane {
            if let Some((_, preview)) = target {
                // The model validates the target again and applies one atomic
                // move. A stale target cannot destroy a retained document.
                state.request(
                    self.id,
                    DockRequest::Move {
                        pane,
                        placement: preview.placement,
                    },
                )?;
            } else if pointer.hit_window.is_none() {
                // Drop outside all application client areas creates a window.
                // A ribbon/status/menu area inside an owned window cancels.
                let scale = state.native_geometry.get(&self.id).map_or(1.0, |g| g.scale);
                let bounds = Rect::new(
                    pointer.screen_position.x / scale - 32.0,
                    pointer.screen_position.y / scale - 16.0,
                    640.0,
                    440.0,
                );
                match state.layout.float_pane(&pane, bounds) {
                    Ok(_) => {
                        state.active = pane;
                        state.reconcile_focus()?;
                    }
                    Err(error) => eprintln!("Docking float rejected: {error}"),
                }
            }
        }
        Ok(true)
    }

    pub(super) fn update_desktop_drag(&mut self, input: &InputEvent) -> Result<(), String> {
        if !matches!(input, InputEvent::PointerMoved { .. }) {
            return Ok(());
        }
        let Some(tab) = self.dock.tab_drag().filter(|tab| tab.dragging) else {
            return Ok(());
        };
        let Some(pointer) = self.desktop_pointer.take() else {
            return Ok(());
        };
        let mut state = self.state.borrow_mut();
        let target = state.drag_target(pointer);
        let source_preview = target
            .as_ref()
            .filter(|(window, _)| *window == self.id)
            .map(|(_, preview)| *preview);
        state.drag = Some(DragSession {
            source: self.id,
            pane: tab.pane,
            pointer,
            target,
        });
        self.cross_drag_started = true;
        self.dock
            .set_external_preview(source_preview)
            .map_err(err)?;
        Ok(())
    }
}

#[cfg(test)]
#[path = "drag_tests.rs"]
mod tests;
