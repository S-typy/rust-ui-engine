//! Route one native window's input to its shell or a persistent pane arena.
use super::*;
use rust_desktop_ui_core::{Key, Modifiers};
use std::time::Instant;

impl DockWindow {
    pub(super) fn handle_event(&mut self, event: PlatformEvent) -> Result<EventResponse, String> {
        self.sync_layout_revision()?;
        // A retiring floating HWND can deliver focus/IME events after its pane
        // has already been transferred. It must not blur the new owner's arena.
        if self.id != DockWindowId::MAIN && self.state.borrow().layout.root(self.id).is_none() {
            return Ok(EventResponse::default());
        }
        let mut response = EventResponse::default();
        if self.layout_pending {
            // Pointer and accessibility coordinates belong to the old layout.
            // Native lifecycle still follows the model's current pane owner.
            response.redraw = true;
            match event {
                PlatformEvent::Input(InputEvent::WindowFocused(focused)) => {
                    self.route_window_focus(focused, &mut response)?;
                }
                PlatformEvent::ImeCancel => {
                    self.deliver_focused_lifecycle(PlatformEvent::ImeCancel, &mut response)?;
                }
                _ => {}
            }
            return Ok(response);
        }
        match event {
            PlatformEvent::Input(input) => {
                response.redraw = true;
                self.route_input(input, &mut response)?;
            }
            PlatformEvent::Tick => {
                response.redraw |= self.ui.tick(Instant::now()).map_err(err)?;
                for pane in self.live_panes() {
                    self.deliver_pane(&pane, PlatformEvent::Tick, &mut response)?;
                }
            }
            PlatformEvent::Accessibility(request) => {
                self.route_accessibility(request, &mut response)?;
            }
            event => {
                let text_suppressed = self.suppress_text
                    || self
                        .ribbon
                        .as_ref()
                        .is_some_and(|mounted| mounted.ribbon.key_tips_visible())
                    || self.ui.popup_open();
                if (!text_suppressed || matches!(event, PlatformEvent::ImeCancel))
                    && let Some(pane) = self.live_focused_pane()
                {
                    self.deliver_pane(&pane, event, &mut response)?;
                }
            }
        }
        self.finish_shell_actions()?;
        response.clipboard.extend(
            self.ui
                .drain_clipboard_requests()
                .into_iter()
                .map(|request| match request {
                    rust_desktop_ui_controls::ClipboardRequest::Copy(text) => {
                        platform::ClipboardRequest::Copy(text)
                    }
                    rust_desktop_ui_controls::ClipboardRequest::Paste => {
                        platform::ClipboardRequest::Paste
                    }
                }),
        );
        Ok(response)
    }

    fn route_input(
        &mut self,
        input: InputEvent,
        response: &mut EventResponse,
    ) -> Result<(), String> {
        if self.route_desktop_drag(&input)? {
            return Ok(());
        }
        if !matches!(
            input,
            InputEvent::PointerMoved { .. } | InputEvent::WindowFocused(true)
        ) {
            self.ui.dismiss_tooltip();
        }
        if let InputEvent::WindowFocused(focused) = input {
            self.route_window_focus(focused, response)?;
            return Ok(());
        }
        if matches!(input, InputEvent::PointerLeft) {
            self.set_hovered_pane(None, response)?;
            self.ui.dispatch(input).map_err(err)?;
            return Ok(());
        }

        // A popup owns the complete click sequence. In particular, a click
        // outside it dismisses it without activating a tab or starting a drag.
        if let Some(pane) = self.live_panes().into_iter().find(|pane| {
            self.state
                .borrow()
                .panes
                .get(pane)
                .is_some_and(|form| form.ui.popup_open())
        }) {
            let bounds = self.pane_bounds(&pane).expect("live pane has geometry");
            let local = translated_input(&input, bounds);
            if let InputEvent::PointerDown { position, .. } = &local {
                self.suppress_text = false;
                let inside = self.state.borrow().panes[&pane]
                    .ui
                    .popup_contains_point(*position);
                if !inside {
                    self.state
                        .borrow_mut()
                        .panes
                        .get_mut(&pane)
                        .expect("live pane")
                        .ui
                        .close_popup()
                        .map_err(err)?;
                    return Ok(());
                }
            }
            self.deliver_pane(&pane, PlatformEvent::Input(local), response)?;
            return Ok(());
        }
        if self.ui.popup_open() {
            if let InputEvent::PointerDown { position, .. } = &input {
                self.suppress_text = false;
                if !self.ui.popup_contains_point(*position) {
                    self.ui.close_popup().map_err(err)?;
                    return Ok(());
                }
            }
            self.ui.dispatch(input).map_err(err)?;
            return Ok(());
        }

        // A content editor owns pointer capture even when the pointer crosses
        // a splitter, another panel, or the native client boundary.
        if pointer_position(&input).is_some()
            && let Some(pane) = self.live_panes().into_iter().find(|pane| {
                self.state
                    .borrow()
                    .panes
                    .get(pane)
                    .is_some_and(|form| form.ui.runtime.captured().is_some())
            })
        {
            let local = translated_input(&input, self.pane_bounds(&pane).expect("live geometry"));
            self.set_hovered_pane(Some(pane.clone()), response)?;
            self.deliver_pane(&pane, PlatformEvent::Input(local), response)?;
            return Ok(());
        }

        if matches!(input, InputEvent::PointerDown { .. }) {
            self.suppress_text = false;
        }
        if matches!(input, InputEvent::KeyDown { .. } | InputEvent::KeyUp { .. }) {
            self.suppress_text = self
                .ribbon
                .as_ref()
                .is_some_and(|mounted| mounted.ribbon.key_tips_visible());
        }
        let mut ribbon_consumed = false;
        if let Some(mounted) = &mut self.ribbon {
            ribbon_consumed = mounted
                .ribbon
                .handle_input(&mut self.ui, &input)
                .map_err(err)?;
        }
        if ribbon_consumed {
            self.suppress_text = true;
            self.state.borrow_mut().focus_pane(self.id, None)?;
            return Ok(());
        }

        if let InputEvent::KeyDown {
            key: Key::Tab,
            modifiers,
            repeat,
        } = &input
            && modifiers.control
            && !modifiers.alt
            && !modifiers.super_key
        {
            self.suppress_text = true;
            if !repeat && !self.dock.is_interacting() {
                let panes = self.live_panes();
                if !panes.is_empty() {
                    let current = self
                        .live_focused_pane()
                        .and_then(|pane| panes.iter().position(|id| id == &pane));
                    let index = match (current, modifiers.shift) {
                        (Some(index), false) => (index + 1) % panes.len(),
                        (Some(index), true) => (index + panes.len() - 1) % panes.len(),
                        (None, false) => 0,
                        (None, true) => panes.len() - 1,
                    };
                    let pane = panes[index].clone();
                    self.enter_pane(&pane, response)?;
                    self.deliver_pane(
                        &pane,
                        PlatformEvent::Input(InputEvent::KeyDown {
                            key: Key::Tab,
                            modifiers: Modifiers {
                                shift: modifiers.shift,
                                ..Default::default()
                            },
                            repeat: false,
                        }),
                        response,
                    )?;
                }
            }
            return Ok(());
        }

        // A pane composition owns the first Escape; the next one dismisses
        // auto-hide without rolling back the editor's retained draft.
        if matches!(
            input,
            InputEvent::KeyDown {
                key: Key::Escape,
                ..
            }
        ) && let Some(pane) = self.live_focused_pane()
            && self.state.borrow().layout.revealed_auto_hide(self.id) == Some(&pane)
            && self.state.borrow().panes[&pane].ime_composition_active() == Some(true)
        {
            self.deliver_pane(&pane, PlatformEvent::ImeCancel, response)?;
            return Ok(());
        }

        let (consumed, requests) = self
            .dock
            .handle_input(&mut self.ui, &input, &self.state.borrow().layout)
            .map_err(err)?;
        self.update_desktop_drag(&input)?;
        if consumed {
            if matches!(input, InputEvent::PointerDown { .. }) {
                self.state.borrow_mut().focus_pane(self.id, None)?;
                self.set_hovered_pane(None, response)?;
            }
            self.apply_requests(requests)?;
            return Ok(());
        }
        self.apply_requests(requests)?;
        if pointer_position(&input).is_some() && self.ui.runtime.captured().is_some() {
            self.set_hovered_pane(None, response)?;
            self.ui.dispatch(input).map_err(err)?;
            return Ok(());
        }
        if let Some(position) = pointer_position(&input) {
            let pane = self
                .geometry
                .as_ref()
                .and_then(|geometry| geometry.pane_at(position))
                .filter(|pane| self.pane_is_live(pane))
                .cloned();
            self.set_hovered_pane(pane.clone(), response)?;
            if let Some(pane) = pane {
                if matches!(input, InputEvent::PointerDown { .. }) {
                    self.enter_pane(&pane, response)?;
                }
                self.ui.dispatch(InputEvent::PointerLeft).map_err(err)?;
                let local =
                    translated_input(&input, self.pane_bounds(&pane).expect("live geometry"));
                self.deliver_pane(&pane, PlatformEvent::Input(local), response)?;
            } else {
                if matches!(input, InputEvent::PointerDown { .. }) {
                    self.state.borrow_mut().focus_pane(self.id, None)?;
                }
                self.ui.dispatch(input).map_err(err)?;
            }
        } else if let Some(pane) = self.live_focused_pane() {
            self.deliver_pane(&pane, PlatformEvent::Input(input), response)?;
        } else {
            self.ui.dispatch(input).map_err(err)?;
        }
        Ok(())
    }

    fn route_window_focus(
        &mut self,
        focused: bool,
        response: &mut EventResponse,
    ) -> Result<(), String> {
        let input = InputEvent::WindowFocused(focused);
        let was_focused = std::mem::replace(&mut self.native_focused, focused);
        self.suppress_text = false;
        if let Some(mounted) = &mut self.ribbon {
            mounted
                .ribbon
                .handle_input(&mut self.ui, &input)
                .map_err(err)?;
        }
        let (_, requests) = self
            .dock
            .handle_input(&mut self.ui, &input, &self.state.borrow().layout)
            .map_err(err)?;
        self.apply_requests(requests)?;
        self.ui.dispatch(input.clone()).map_err(err)?;
        // Initial NOACTIVATE false belongs to the shell only. A retained pane
        // newly moved here has not lost native focus in this window yet.
        if focused || was_focused {
            self.deliver_focused_lifecycle(PlatformEvent::Input(input), response)?;
            if !focused {
                self.deliver_focused_lifecycle(PlatformEvent::ImeCancel, response)?;
            }
        }
        if !focused {
            self.set_hovered_pane(None, response)?;
        }
        Ok(())
    }

    /// Focus and IME cancellation need ownership, not potentially stale bounds.
    fn deliver_focused_lifecycle(
        &mut self,
        event: PlatformEvent,
        response: &mut EventResponse,
    ) -> Result<(), String> {
        let mut state = self.state.borrow_mut();
        let pane = state.focused.as_ref().and_then(|(window, pane)| {
            (*window == self.id
                && state.layout.window_for(pane) == Some(self.id)
                && state.layout.pane_is_visible(pane))
            .then(|| pane.clone())
        });
        if let Some(pane) = pane
            && let Some(form) = state.panes.get_mut(&pane)
        {
            merge(response, form.event(event)?);
        }
        Ok(())
    }

    fn pane_is_live(&self, pane: &DockPaneId) -> bool {
        let state = self.state.borrow();
        state.layout.window_for(pane) == Some(self.id)
            && state.layout.pane_is_visible(pane)
            && self
                .pane_bounds(pane)
                .is_some_and(|bounds| bounds.width > 0.0 && bounds.height > 0.0)
            && state.panes.contains_key(pane)
    }

    fn live_panes(&self) -> Vec<DockPaneId> {
        self.geometry
            .iter()
            .flat_map(DockGeometry::visible_panes)
            .filter(|(pane, _)| self.pane_is_live(pane))
            .map(|(pane, _)| pane.clone())
            .collect()
    }

    fn live_focused_pane(&self) -> Option<DockPaneId> {
        self.focused_pane().filter(|pane| self.pane_is_live(pane))
    }

    fn deliver_pane(
        &mut self,
        pane: &DockPaneId,
        event: PlatformEvent,
        response: &mut EventResponse,
    ) -> Result<(), String> {
        if !self.pane_is_live(pane) {
            return Ok(());
        }
        if let Some(form) = self.state.borrow_mut().panes.get_mut(pane) {
            merge(response, form.event(event)?);
        }
        Ok(())
    }

    fn enter_pane(
        &mut self,
        pane: &DockPaneId,
        response: &mut EventResponse,
    ) -> Result<(), String> {
        if !self.pane_is_live(pane) {
            return Ok(());
        }
        if self
            .state
            .borrow()
            .layout
            .revealed_auto_hide(self.id)
            .is_some_and(|revealed| revealed != pane)
        {
            self.state.borrow_mut().request(
                self.id,
                DockRequest::DismissAutoHide {
                    window: self.id,
                    restore_focus: false,
                },
            )?;
        }
        self.state
            .borrow_mut()
            .focus_pane(self.id, Some(pane.clone()))?;
        self.ui.runtime.apply(Mutation::Focus(None)).map_err(err)?;
        self.deliver_pane(
            pane,
            PlatformEvent::Input(InputEvent::WindowFocused(true)),
            response,
        )
    }

    fn set_hovered_pane(
        &mut self,
        pane: Option<DockPaneId>,
        response: &mut EventResponse,
    ) -> Result<(), String> {
        if self.hovered_pane != pane {
            if let Some(old) = self.hovered_pane.take() {
                self.deliver_pane(
                    &old,
                    PlatformEvent::Input(InputEvent::PointerLeft),
                    response,
                )?;
            }
            self.hovered_pane = pane;
        }
        Ok(())
    }

    fn apply_requests(&mut self, requests: Vec<DockRequest>) -> Result<(), String> {
        let mut state = self.state.borrow_mut();
        for request in requests {
            let restore = matches!(
                request,
                DockRequest::DismissAutoHide {
                    restore_focus: true,
                    ..
                }
            );
            state.request(self.id, request)?;
            if restore
                && state
                    .focused
                    .as_ref()
                    .is_some_and(|(window, _)| *window == self.id)
            {
                self.ui.runtime.apply(Mutation::Focus(None)).map_err(err)?;
            }
        }
        Ok(())
    }

    fn finish_shell_actions(&mut self) -> Result<(), String> {
        let activated = if let Some(mounted) = &mut self.ribbon {
            mounted.ribbon.process(&mut self.ui).map_err(err)?
        } else {
            self.ui.drain_activated()
        };
        let requests = self
            .dock
            .process_activated(&activated, &self.state.borrow().layout);
        self.apply_requests(requests)?;
        let invocations = self.ui.commands_mut().drain_invocations();
        let mut state = self.state.borrow_mut();
        for invocation in invocations {
            state.command(self.id, &invocation.id.0)?;
        }
        Ok(())
    }

    fn route_accessibility(
        &mut self,
        request: accesskit::ActionRequest,
        response: &mut EventResponse,
    ) -> Result<(), String> {
        if request.target_tree != accesskit::TreeId::ROOT {
            return Ok(());
        }
        self.suppress_text = false;
        if let Some(mounted) = &mut self.ribbon {
            mounted.ribbon.cancel_key_tips(&mut self.ui);
        }
        if let Some(pane) = self.action_owners.get(&request.target_node).cloned() {
            if !self.pane_is_live(&pane) {
                return Ok(());
            }
            let supported = {
                let state = self.state.borrow();
                let form = &state.panes[&pane];
                supports_action(&form.ui, &form.accessible, &request)
            };
            if supported {
                self.ui.close_popup().map_err(err)?;
                self.enter_pane(&pane, response)?;
                self.deliver_pane(&pane, PlatformEvent::Accessibility(request), response)?;
            }
        } else if supports_action(&self.ui, &self.accessible, &request) {
            self.state.borrow_mut().focus_pane(self.id, None)?;
            self.accessible.action(&mut self.ui, request)?;
            response.redraw = true;
        }
        Ok(())
    }
}

fn supports_action(
    ui: &Controls,
    accessible: &Accessibility,
    request: &accesskit::ActionRequest,
) -> bool {
    let action = match request.action {
        accesskit::Action::Focus => SemanticAction::Focus,
        accesskit::Action::Click => SemanticAction::Activate,
        accesskit::Action::SetValue
            if matches!(request.data, Some(accesskit::ActionData::Value(_))) =>
        {
            SemanticAction::SetValue
        }
        accesskit::Action::Expand => SemanticAction::Expand,
        accesskit::Action::Collapse => SemanticAction::Collapse,
        _ => return false,
    };
    ui.semantics().iter().any(|node| {
        accessible.id(node.id) == Some(request.target_node)
            && node.actions.contains(&action)
            && ui
                .runtime
                .clip(node.id)
                .and_then(|clip| node.bounds.intersection(clip))
                .is_some_and(|visible| visible.width > 0.0 && visible.height > 0.0)
    })
}

fn merge(target: &mut EventResponse, source: EventResponse) {
    target.redraw |= source.redraw;
    target.clipboard.extend(source.clipboard);
}

fn pointer_position(event: &InputEvent) -> Option<Point> {
    match event {
        InputEvent::PointerMoved { position }
        | InputEvent::PointerDown { position, .. }
        | InputEvent::PointerUp { position, .. }
        | InputEvent::Wheel { position, .. } => Some(*position),
        _ => None,
    }
}

/// Translate positions only. Captured drags may intentionally extend outside
/// the pane; wheel deltas, modifiers and logical keyboard keys stay unchanged.
fn translated_input(event: &InputEvent, bounds: Rect) -> InputEvent {
    let mut event = event.clone();
    match &mut event {
        InputEvent::PointerMoved { position }
        | InputEvent::PointerDown { position, .. }
        | InputEvent::PointerUp { position, .. }
        | InputEvent::Wheel { position, .. } => {
            position.x -= bounds.x;
            position.y -= bounds.y;
        }
        _ => {}
    }
    event
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_desktop_ui_core::PointerButton;

    fn fixture() -> DockWindow {
        let definition = xaml::parse(r#"<Window xmlns="urn:rust-ui-engine:ui" xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml" Padding="0">
            <DockWorkspace><DockSplit><DockGroup><DockPane Id="A"><StackPanel Padding="12">
                <TextBox x:Name="Message" />
            </StackPanel></DockPane></DockGroup><DockGroup><DockPane Id="B"><StackPanel Padding="12">
                <TextBox x:Name="OtherEditor" />
            </StackPanel></DockPane></DockGroup></DockSplit></DockWorkspace>
        </Window>"#).unwrap();
        let application =
            DockApplication::new(definition, RunOptions::default(), false, None).unwrap();
        let mut window = DockWindow::new(
            application.state,
            DockWindowId::MAIN,
            "Test",
            None,
            xaml::TitleBar::Native,
            0.0,
        )
        .unwrap();
        window.update(Size::new(900.0, 500.0), 1.0).unwrap();
        window
    }

    fn pane(value: &str) -> DockPaneId {
        DockPaneId::new(value).unwrap()
    }

    fn editor_point(window: &DockWindow, pane: &DockPaneId) -> Point {
        let state = window.state.borrow();
        let form = &state.panes[pane];
        let local = form.ui.runtime.bounds(form.editor()).unwrap();
        let global = window.pane_bounds(pane).unwrap();
        Point::new(
            global.x + local.x + 12.0,
            global.y + local.y + local.height * 0.5,
        )
    }

    fn pointer(window: &mut DockWindow, point: Point, down: bool) {
        let event = if down {
            InputEvent::PointerDown {
                position: point,
                button: PointerButton::Primary,
                modifiers: Modifiers::default(),
            }
        } else {
            InputEvent::PointerUp {
                position: point,
                button: PointerButton::Primary,
                modifiers: Modifiers::default(),
            }
        };
        window.handle_event(PlatformEvent::Input(event)).unwrap();
    }

    fn click_editor(window: &mut DockWindow, pane: &DockPaneId) {
        let point = editor_point(window, pane);
        pointer(window, point, true);
        pointer(window, point, false);
    }

    #[test]
    fn editor_capture_and_ctrl_tab_keep_input_in_the_correct_arena() {
        let mut window = fixture();
        let a = pane("A");
        let b = pane("B");
        let point_a = editor_point(&window, &a);
        let point_b = editor_point(&window, &b);
        pointer(&mut window, point_a, true);
        window
            .handle_event(PlatformEvent::Input(InputEvent::PointerMoved {
                position: point_b,
            }))
            .unwrap();
        assert_eq!(window.live_focused_pane(), Some(a.clone()));
        assert!(
            window.state.borrow().panes[&a]
                .ui
                .runtime
                .captured()
                .is_some()
        );
        assert!(
            window.state.borrow().panes[&b]
                .ui
                .runtime
                .focused()
                .is_none()
        );
        pointer(&mut window, point_b, false);
        assert!(
            window.state.borrow().panes[&a]
                .ui
                .runtime
                .captured()
                .is_none()
        );
        window
            .handle_event(PlatformEvent::Text("draft".into()))
            .unwrap();
        window
            .handle_event(PlatformEvent::Input(InputEvent::KeyDown {
                key: Key::Tab,
                modifiers: Modifiers {
                    control: true,
                    ..Default::default()
                },
                repeat: false,
            }))
            .unwrap();
        assert_eq!(window.live_focused_pane(), Some(b.clone()));
        window
            .handle_event(PlatformEvent::Input(InputEvent::KeyDown {
                key: Key::Character("x".into()),
                modifiers: Modifiers::default(),
                repeat: false,
            }))
            .unwrap();
        window
            .handle_event(PlatformEvent::Text("second".into()))
            .unwrap();
        let state = window.state.borrow();
        assert_eq!(state.panes[&a].text(), "draft");
        assert_eq!(state.panes[&b].text(), "second");
        assert_eq!(
            state.panes[&a].ui.accepted_value(state.panes[&a].editor()),
            Some("draft")
        );
        drop(state);
        let modifiers = Modifiers {
            control: true,
            ..Default::default()
        };
        window
            .handle_event(PlatformEvent::Input(InputEvent::KeyDown {
                key: Key::Character("a".into()),
                modifiers,
                repeat: false,
            }))
            .unwrap();
        let result = window
            .handle_event(PlatformEvent::Input(InputEvent::KeyDown {
                key: Key::Character("c".into()),
                modifiers,
                repeat: false,
            }))
            .unwrap();
        assert!(result.clipboard.iter().any(|request| matches!(request, platform::ClipboardRequest::Copy(value) if value == "second")));
    }

    #[test]
    fn first_click_outside_a_pane_menu_only_closes_the_menu() {
        let mut window = fixture();
        let a = pane("A");
        let b = pane("B");
        click_editor(&mut window, &a);
        {
            let mut state = window.state.borrow_mut();
            let form = state.panes.get_mut(&a).unwrap();
            form.ui
                .popup_menu(form.editor(), &[MenuItem::new("Очистить", "Clear")])
                .unwrap();
        }
        let point_b = editor_point(&window, &b);
        pointer(&mut window, point_b, true);
        pointer(&mut window, point_b, false);
        assert!(!window.state.borrow().panes[&a].ui.popup_open());
        assert_eq!(window.live_focused_pane(), Some(a));
        assert!(
            window.state.borrow().panes[&b]
                .ui
                .runtime
                .focused()
                .is_none()
        );
        assert!(!window.dock.is_interacting());
        click_editor(&mut window, &b);
        assert_eq!(window.live_focused_pane(), Some(b));
    }

    #[test]
    fn moved_pane_ignores_old_window_text_blur_and_accessibility() {
        let mut window = fixture();
        let a = pane("A");
        click_editor(&mut window, &a);
        window
            .handle_event(PlatformEvent::Text("uncommitted".into()))
            .unwrap();
        window.accessibility(1.0);
        let (editor, target) = {
            let state = window.state.borrow();
            let form = &state.panes[&a];
            (form.editor(), form.accessible.id(form.editor()).unwrap())
        };
        window
            .state
            .borrow_mut()
            .request(DockWindowId::MAIN, DockRequest::Float(a.clone()))
            .unwrap();
        let owner = window.state.borrow().layout.window_for(&a).unwrap();
        assert_ne!(owner, DockWindowId::MAIN);
        window
            .handle_event(PlatformEvent::Input(InputEvent::WindowFocused(false)))
            .unwrap();
        window
            .handle_event(PlatformEvent::Text("wrong window".into()))
            .unwrap();
        window
            .handle_event(PlatformEvent::Accessibility(accesskit::ActionRequest {
                action: accesskit::Action::SetValue,
                target_tree: accesskit::TreeId::ROOT,
                target_node: target,
                data: Some(accesskit::ActionData::Value("wrong action".into())),
            }))
            .unwrap();
        let state = window.state.borrow();
        let form = &state.panes[&a];
        assert_eq!(form.editor(), editor);
        assert_eq!(form.ui.runtime.focused(), Some(editor));
        assert_eq!(form.text(), "uncommitted");
        assert_eq!(form.ui.accepted_value(editor), Some(""));
        assert_eq!(state.focused, Some((owner, a)));
    }

    #[test]
    fn new_floating_window_initial_unfocused_state_preserves_the_editor_session() {
        let mut main = fixture();
        let a = pane("A");
        main.handle_event(PlatformEvent::Input(InputEvent::WindowFocused(true)))
            .unwrap();
        click_editor(&mut main, &a);
        main.handle_event(PlatformEvent::Text("retained draft".into()))
            .unwrap();
        let editor = main.state.borrow().panes[&a].editor();
        main.state
            .borrow_mut()
            .request(DockWindowId::MAIN, DockRequest::Float(a.clone()))
            .unwrap();
        let owner = main.state.borrow().layout.window_for(&a).unwrap();
        let mut floating = DockWindow::new(
            main.state.clone(),
            owner,
            "Floating",
            None,
            xaml::TitleBar::Native,
            0.0,
        )
        .unwrap();
        floating.update(Size::new(640.0, 440.0), 1.0).unwrap();
        assert!(!floating.native_focused);
        floating
            .handle_event(PlatformEvent::Input(InputEvent::WindowFocused(false)))
            .unwrap();
        {
            let state = main.state.borrow();
            let form = &state.panes[&a];
            assert_eq!(form.editor(), editor);
            assert_eq!(form.ui.runtime.focused(), Some(editor));
            assert_eq!(form.text(), "retained draft");
            assert_eq!(form.ui.accepted_value(editor), Some(""));
            assert_eq!(state.focused, Some((owner, a.clone())));
        }
        floating
            .handle_event(PlatformEvent::Input(InputEvent::WindowFocused(true)))
            .unwrap();
        assert!(floating.native_focused);
        floating
            .handle_event(PlatformEvent::Input(InputEvent::WindowFocused(false)))
            .unwrap();
        assert!(!floating.native_focused);
        let state = main.state.borrow();
        let form = &state.panes[&a];
        assert_eq!(form.text(), "retained draft");
        assert_eq!(form.ui.accepted_value(editor), Some("retained draft"));
        assert_eq!(form.ui.runtime.focused(), None);
    }

    #[test]
    fn pending_restore_routes_focus_loss_without_using_old_geometry() {
        let mut window = fixture();
        let id = pane("A");
        window
            .handle_event(PlatformEvent::Input(InputEvent::WindowFocused(true)))
            .unwrap();
        click_editor(&mut window, &id);
        window
            .handle_event(PlatformEvent::Text("accepted on blur".into()))
            .unwrap();
        let editor = window.state.borrow().panes[&id].editor();
        window
            .state
            .borrow_mut()
            .command(DockWindowId::MAIN, "DockReset")
            .unwrap();
        window
            .handle_event(PlatformEvent::Input(InputEvent::WindowFocused(false)))
            .unwrap();
        assert!(window.layout_pending && window.geometry.is_none());
        assert!(!window.native_focused);
        let tab = window.dock.tab_widget(&id).unwrap();
        assert!(window.ui.runtime.apply(Mutation::Focus(Some(tab))).is_err());
        let state = window.state.borrow();
        let form = &state.panes[&id];
        assert_eq!(form.ui.accepted_value(editor), Some("accepted on blur"));
        assert_eq!(form.ui.runtime.focused(), None);
    }

    #[test]
    fn pending_restore_blocks_text_and_accessibility_but_delivers_ime_cancel() {
        let mut window = fixture();
        let id = pane("A");
        window
            .handle_event(PlatformEvent::Input(InputEvent::WindowFocused(true)))
            .unwrap();
        click_editor(&mut window, &id);
        window
            .handle_event(PlatformEvent::Text("draft".into()))
            .unwrap();
        window.accessibility(1.0);
        let (editor, target) = {
            let mut state = window.state.borrow_mut();
            let form = state.panes.get_mut(&id).unwrap();
            form.ui
                .ime_preedit("候補", Some((0, "候補".len())))
                .unwrap();
            (form.editor(), form.accessible.id(form.editor()).unwrap())
        };
        let old_point = editor_point(&window, &pane("B"));
        window
            .state
            .borrow_mut()
            .command(DockWindowId::MAIN, "DockReset")
            .unwrap();
        window
            .handle_event(PlatformEvent::Text("must not arrive".into()))
            .unwrap();
        pointer(&mut window, old_point, true);
        window
            .handle_event(PlatformEvent::Accessibility(accesskit::ActionRequest {
                action: accesskit::Action::SetValue,
                target_tree: accesskit::TreeId::ROOT,
                target_node: target,
                data: Some(accesskit::ActionData::Value("must not arrive".into())),
            }))
            .unwrap();
        assert!(window.layout_pending && window.geometry.is_none());
        assert!(
            window.state.borrow().panes[&id]
                .ui
                .document(editor)
                .unwrap()
                .composition()
                .is_some()
        );
        window.handle_event(PlatformEvent::ImeCancel).unwrap();
        let state = window.state.borrow();
        let form = &state.panes[&id];
        assert_eq!(form.text(), "draft");
        assert_eq!(form.ui.accepted_value(editor), Some(""));
        assert_eq!(form.ui.runtime.focused(), Some(editor));
        assert!(form.ui.document(editor).unwrap().composition().is_none());
    }

    #[test]
    fn restored_new_float_processes_initial_false_for_shell_without_committing_pane() {
        let mut main = fixture();
        let id = pane("A");
        main.handle_event(PlatformEvent::Input(InputEvent::WindowFocused(true)))
            .unwrap();
        click_editor(&mut main, &id);
        main.handle_event(PlatformEvent::Text("retained after restore".into()))
            .unwrap();
        let editor = main.state.borrow().panes[&id].editor();
        let owner = {
            let mut state = main.state.borrow_mut();
            state
                .request(DockWindowId::MAIN, DockRequest::Float(id.clone()))
                .unwrap();
            let snapshot = state.layout.snapshot();
            state.layout.restore_snapshot(snapshot).unwrap();
            state.placement_revision += 1;
            state.layout.window_for(&id).unwrap()
        };
        let mut floating = DockWindow::new(
            main.state.clone(),
            owner,
            "Restored",
            None,
            xaml::TitleBar::Native,
            0.0,
        )
        .unwrap();
        floating
            .handle_event(PlatformEvent::Input(InputEvent::WindowFocused(false)))
            .unwrap();
        assert!(floating.layout_pending && floating.geometry.is_none());
        {
            let state = main.state.borrow();
            let form = &state.panes[&id];
            assert_eq!(form.ui.runtime.focused(), Some(editor));
            assert_eq!(form.ui.accepted_value(editor), Some(""));
            assert_eq!(form.text(), "retained after restore");
        }
        floating.update(Size::new(640.0, 440.0), 1.0).unwrap();
        let tab = floating.dock.tab_widget(&id).unwrap();
        assert!(
            floating
                .ui
                .runtime
                .apply(Mutation::Focus(Some(tab)))
                .is_err()
        );
        floating
            .handle_event(PlatformEvent::Input(InputEvent::WindowFocused(true)))
            .unwrap();
        floating
            .handle_event(PlatformEvent::Input(InputEvent::WindowFocused(false)))
            .unwrap();
        assert_eq!(
            main.state.borrow().panes[&id].ui.accepted_value(editor),
            Some("retained after restore")
        );
    }

    #[test]
    fn retired_float_cannot_deliver_pending_epoch_blur_or_ime_cancel_to_moved_pane() {
        let mut main = fixture();
        let id = pane("A");
        main.handle_event(PlatformEvent::Input(InputEvent::WindowFocused(true)))
            .unwrap();
        click_editor(&mut main, &id);
        main.handle_event(PlatformEvent::Text("retained draft".into()))
            .unwrap();
        main.state
            .borrow_mut()
            .request(DockWindowId::MAIN, DockRequest::Float(id.clone()))
            .unwrap();
        let owner = main.state.borrow().layout.window_for(&id).unwrap();
        let mut floating = DockWindow::new(
            main.state.clone(),
            owner,
            "Retiring",
            None,
            xaml::TitleBar::Native,
            0.0,
        )
        .unwrap();
        floating.update(Size::new(640.0, 440.0), 1.0).unwrap();
        floating
            .handle_event(PlatformEvent::Input(InputEvent::WindowFocused(true)))
            .unwrap();
        let editor = {
            let mut state = main.state.borrow_mut();
            let form = state.panes.get_mut(&id).unwrap();
            form.ui
                .ime_preedit("候補", Some((0, "候補".len())))
                .unwrap();
            let editor = form.editor();
            state.layout.redock_window(owner).unwrap();
            state.reconcile_focus().unwrap();
            state.placement_revision += 1;
            editor
        };
        floating
            .handle_event(PlatformEvent::Input(InputEvent::WindowFocused(false)))
            .unwrap();
        floating.handle_event(PlatformEvent::ImeCancel).unwrap();
        let state = main.state.borrow();
        let form = &state.panes[&id];
        assert_eq!(state.focused, Some((DockWindowId::MAIN, id.clone())));
        assert_eq!(form.ui.runtime.focused(), Some(editor));
        assert_eq!(form.ui.accepted_value(editor), Some(""));
        assert_eq!(form.text(), "retained draft");
        assert!(form.ui.document(editor).unwrap().composition().is_some());
    }

    #[test]
    fn direct_accessibility_action_on_a_fully_clipped_editor_is_ignored() {
        let mut window = fixture();
        let a = pane("A");
        let editor = window.state.borrow().panes[&a].editor();
        {
            let mut state = window.state.borrow_mut();
            let form = state.panes.get_mut(&a).unwrap();
            let mut style = form
                .ui
                .runtime
                .tree()
                .node(editor)
                .unwrap()
                .props
                .style
                .clone();
            style.margin.top = 800.0;
            form.ui
                .runtime
                .apply(Mutation::SetStyle {
                    node: editor,
                    style,
                })
                .unwrap();
        }
        window.update(Size::new(900.0, 500.0), 1.0).unwrap();
        window.accessibility(1.0);
        let target = window.state.borrow().panes[&a]
            .accessible
            .id(editor)
            .unwrap();
        // Simulate an accessibility request queued before clipping changed.
        window.action_owners.insert(target, a.clone());
        window
            .handle_event(PlatformEvent::Accessibility(accesskit::ActionRequest {
                action: accesskit::Action::SetValue,
                target_tree: accesskit::TreeId::ROOT,
                target_node: target,
                data: Some(accesskit::ActionData::Value("invisible".into())),
            }))
            .unwrap();
        assert_eq!(window.state.borrow().panes[&a].text(), "");
        assert_eq!(window.state.borrow().focused, None);
    }

    #[test]
    fn pane_coordinates_preserve_capture_outside_bounds_and_wheel_delta() {
        let bounds = Rect::new(120.0, 80.0, 400.0, 300.0);
        let modifiers = Modifiers {
            control: true,
            shift: true,
            ..Default::default()
        };
        let down = InputEvent::PointerDown {
            position: Point::new(116.0, 90.0),
            button: PointerButton::Primary,
            modifiers,
        };
        assert_eq!(
            translated_input(&down, bounds),
            InputEvent::PointerDown {
                position: Point::new(-4.0, 10.0),
                button: PointerButton::Primary,
                modifiers
            }
        );
        let up = InputEvent::PointerUp {
            position: Point::new(800.0, -10.0),
            button: PointerButton::Middle,
            modifiers,
        };
        assert_eq!(
            pointer_position(&translated_input(&up, bounds)),
            Some(Point::new(680.0, -90.0))
        );
        let wheel = InputEvent::Wheel {
            position: Point::new(140.0, 120.0),
            delta: Point::new(-1.5, 28.0),
            modifiers,
        };
        assert_eq!(
            translated_input(&wheel, bounds),
            InputEvent::Wheel {
                position: Point::new(20.0, 40.0),
                delta: Point::new(-1.5, 28.0),
                modifiers
            }
        );
        let moved = InputEvent::PointerMoved {
            position: Point::new(130.0, 100.0),
        };
        assert_eq!(
            translated_input(&moved, bounds),
            InputEvent::PointerMoved {
                position: Point::new(10.0, 20.0)
            }
        );
        for input in [
            InputEvent::PointerLeft,
            InputEvent::WindowFocused(false),
            InputEvent::KeyDown {
                key: Key::Tab,
                modifiers,
                repeat: true,
            },
            InputEvent::KeyUp {
                key: Key::Character("ж".into()),
                modifiers,
            },
        ] {
            assert_eq!(translated_input(&input, bounds), input);
        }
    }
    #[test]
    fn auto_hide_retains_editor_and_escape_restores_pre_reveal_focus_after_ime() {
        let mut window = fixture();
        let a = pane("A");
        let b = pane("B");
        click_editor(&mut window, &a);
        window
            .handle_event(PlatformEvent::Text("document draft".into()))
            .unwrap();
        let (editor, accepted) = {
            let mut state = window.state.borrow_mut();
            let form = state.panes.get_mut(&b).unwrap();
            let editor = form.editor();
            let mut settings = form.ui.editor_settings(editor).unwrap().clone();
            settings.commit_on_focus_lost = false;
            form.ui.set_editor_settings(editor, settings).unwrap();
            (editor, form.ui.accepted_value(editor).unwrap().to_owned())
        };
        window
            .state
            .borrow_mut()
            .request(
                DockWindowId::MAIN,
                DockRequest::SetAutoHide {
                    pane: b.clone(),
                    settings: Some(DockAutoHideSettings {
                        edge: DockEdge::Right,
                        extent: 320.0,
                    }),
                },
            )
            .unwrap();
        window.update(Size::new(900.0, 500.0), 1.0).unwrap();
        let tab = window.dock.tab_widget(&b).unwrap();
        let bounds = window.ui.runtime.bounds(tab).unwrap();
        let point = Point::new(
            bounds.x + bounds.width / 2.0,
            bounds.y + bounds.height / 2.0,
        );
        pointer(&mut window, point, true);
        pointer(&mut window, point, false);
        window.update(Size::new(900.0, 500.0), 1.0).unwrap();
        click_editor(&mut window, &b);
        window
            .handle_event(PlatformEvent::Text("retained draft".into()))
            .unwrap();
        window
            .handle_event(PlatformEvent::ImePreedit("compose".into(), None))
            .unwrap();
        let escape = PlatformEvent::Input(InputEvent::KeyDown {
            key: Key::Escape,
            modifiers: Modifiers::default(),
            repeat: false,
        });
        window.handle_event(escape.clone()).unwrap();
        assert_eq!(
            window
                .state
                .borrow()
                .layout
                .revealed_auto_hide(DockWindowId::MAIN),
            Some(&b)
        );
        assert!(
            window.state.borrow().panes[&b]
                .ui
                .document(editor)
                .unwrap()
                .composition()
                .is_none()
        );
        window.handle_event(escape).unwrap();
        window.update(Size::new(900.0, 500.0), 1.0).unwrap();
        assert!(
            window
                .state
                .borrow()
                .layout
                .revealed_auto_hide(DockWindowId::MAIN)
                .is_none()
        );
        assert_eq!(window.live_focused_pane(), Some(a.clone()));
        {
            let state = window.state.borrow();
            let form = &state.panes[&b];
            assert_eq!(form.editor(), editor);
            assert_eq!(form.text(), "retained draft");
            assert_eq!(form.ui.accepted_value(editor), Some(accepted.as_str()));
        }
        window
            .state
            .borrow_mut()
            .request(
                DockWindowId::MAIN,
                DockRequest::SetAutoHide {
                    pane: b.clone(),
                    settings: None,
                },
            )
            .unwrap();
        window.update(Size::new(900.0, 500.0), 1.0).unwrap();
        assert_eq!(window.state.borrow().panes[&b].editor(), editor);
        assert!(window.pane_bounds(&b).is_some());
    }

    #[test]
    fn auto_hide_overlay_routes_clicks_above_pinned_content_and_outside_never_restores_old_focus() {
        let mut window = fixture();
        let a = pane("A");
        let b = pane("B");
        click_editor(&mut window, &a);
        window
            .state
            .borrow_mut()
            .request(
                DockWindowId::MAIN,
                DockRequest::SetAutoHide {
                    pane: b.clone(),
                    settings: Some(DockAutoHideSettings {
                        edge: DockEdge::Left,
                        extent: 320.0,
                    }),
                },
            )
            .unwrap();
        window
            .state
            .borrow_mut()
            .request(DockWindowId::MAIN, DockRequest::Activate(b.clone()))
            .unwrap();
        window.update(Size::new(900.0, 500.0), 1.0).unwrap();
        click_editor(&mut window, &b);
        window
            .handle_event(PlatformEvent::Text("overlay".into()))
            .unwrap();
        assert_eq!(window.state.borrow().panes[&a].text(), "");
        assert_eq!(window.state.borrow().panes[&b].text(), "overlay");
        let pinned = window.pane_bounds(&a).unwrap();
        let outside = Point::new(pinned.x + pinned.width - 30.0, pinned.y + 30.0);
        pointer(&mut window, outside, true);
        pointer(&mut window, outside, false);
        assert!(
            window
                .state
                .borrow()
                .layout
                .revealed_auto_hide(DockWindowId::MAIN)
                .is_none()
        );
        assert_eq!(window.live_focused_pane(), Some(a));
        assert!(window.state.borrow().auto_hide_return.is_empty());
    }
}
