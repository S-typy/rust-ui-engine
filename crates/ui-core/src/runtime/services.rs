use super::*;
use crate::{
    DragCancelReason, DragEvent, DragId, DragPayload, DragPhase, DragSession, DropEffect,
    DropEffects, KeyChord, ShortcutEvent,
};

pub(super) type DropHandler = Box<dyn FnMut(&DragEvent, &DragPayload) -> Option<DropEffect>>;
type ShortcutHandler = Box<dyn FnMut(&ShortcutEvent, &mut EventContext)>;
pub(super) struct ShortcutBinding {
    owner: WidgetId,
    chord: KeyChord,
    allow_repeat: bool,
    handler: ShortcutHandler,
}

impl UiRuntime {
    /// Set navigation/shortcut policy for an existing subtree. Detached scopes
    /// remain registered but cannot handle input until attached again.
    pub fn set_focus_scope(
        &mut self,
        owner: WidgetId,
        scope: Option<FocusScope>,
    ) -> Result<(), RuntimeError> {
        if !self.tree.contains(owner) {
            return Err(TreeError::InvalidId(owner).into());
        }
        if let Some(scope) = scope {
            self.scopes.insert(owner, scope);
        } else {
            self.scopes.remove(&owner);
        }
        Ok(())
    }

    /// Register or replace one logical chord on a subtree. The closest eligible
    /// ancestor of focus wins. Ordinary routed handlers can suppress shortcuts
    /// using prevent_default; a matched shortcut consumes the key even when
    /// repeated invocation is disabled, preventing fall-through to an outer scope.
    pub fn on_shortcut(
        &mut self,
        owner: WidgetId,
        chord: KeyChord,
        allow_repeat: bool,
        handler: impl FnMut(&ShortcutEvent, &mut EventContext) + 'static,
    ) -> Result<(), RuntimeError> {
        if !self.tree.contains(owner) {
            return Err(TreeError::InvalidId(owner).into());
        }
        self.remove_shortcut(owner, &chord);
        self.shortcuts.push(ShortcutBinding {
            owner,
            chord,
            allow_repeat,
            handler: Box::new(handler),
        });
        Ok(())
    }

    pub fn remove_shortcut(&mut self, owner: WidgetId, chord: &KeyChord) {
        self.shortcuts.retain(|entry| {
            entry.owner != owner || !entry.chord.matches(&chord.key, chord.modifiers)
        });
    }

    /// Whether a key-down belongs to a registered shortcut in the current focus
    /// route. Hosts can route it before control-specific keyboard preprocessing;
    /// this query never invokes handlers or consumes the input itself.
    pub fn has_shortcut(&self, input: &InputEvent) -> bool {
        let InputEvent::KeyDown { key, modifiers, .. } = input else {
            return false;
        };
        if !self.window_focused {
            return false;
        }
        let mut current = self.focus.or(Some(self.tree.root()));
        while let Some(owner) = current {
            if self.effective_enabled(owner)
                && self
                    .shortcuts
                    .iter()
                    .any(|entry| entry.owner == owner && entry.chord.matches(key, *modifiers))
            {
                return true;
            }
            if self
                .scopes
                .get(&owner)
                .is_some_and(|scope| scope.isolate_shortcuts)
            {
                break;
            }
            current = self.tree.parent(owner);
        }
        false
    }

    /// Install one drop handler. Enter/Over negotiate an allowed effect; Over
    /// is authoritative. Leave/Cancelled clean up feedback; only Dropped should
    /// modify application data. The nearest registered hit ancestor owns the
    /// target, including a rejected target; rejection does not fall through.
    pub fn on_drop_target(
        &mut self,
        target: WidgetId,
        handler: impl FnMut(&DragEvent, &DragPayload) -> Option<DropEffect> + 'static,
    ) -> Result<(), RuntimeError> {
        if !self.tree.contains(target) {
            return Err(TreeError::InvalidId(target).into());
        }
        self.drop_handlers.insert(target, Box::new(handler));
        Ok(())
    }

    pub fn remove_drop_target(&mut self, target: WidgetId) {
        if self
            .drag
            .active()
            .is_some_and(|session| session.target == Some(target))
        {
            self.cancel_active_drag(DragCancelReason::TargetUnavailable);
        }
        self.drop_handlers.remove(&target);
    }

    /// Begin after an application-specific drag threshold. Capture remains on
    /// source while hit testing independently negotiates the actual drop target.
    pub fn begin_drag(
        &mut self,
        source: WidgetId,
        payload: DragPayload,
        effects: DropEffects,
        position: Point,
    ) -> Result<DragId, RuntimeError> {
        if !self.window_focused
            || !self.effective_enabled(source)
            || self.capture.is_some_and(|id| id != source)
        {
            return Err(RuntimeError::IneligibleCapture(source));
        }
        let id = self.drag.begin(source, payload, effects, position)?;
        self.capture = Some(source);
        self.pointer = Some(position);
        Ok(id)
    }

    pub fn drag_session(&self) -> Option<&DragSession> {
        self.drag.active()
    }
    /// Events include exactly one terminal result per accepted begin. Consumers
    /// can use them to update a cursor, source feedback or commit a source move.
    pub fn take_drag_events(&mut self) -> Vec<DragEvent> {
        self.drag.take_events()
    }
    pub fn cancel_drag(&mut self, id: DragId) -> Result<(), RuntimeError> {
        if self.drag.active().is_none_or(|session| session.id != id) {
            return Err(DragError::StaleSession.into());
        }
        self.cancel_active_drag(DragCancelReason::Explicit);
        Ok(())
    }

    /// Preorder considered by Tab navigation, restricted to the nearest active
    /// cycling focus scope. Composite controls can use the same boundary when
    /// temporarily adjusting their tab stops. The list includes nonfocusable
    /// nodes; eligibility still depends on visibility, enabled and tab-stop state.
    pub fn navigation_order(&self) -> Vec<WidgetId> {
        let mut current = self.focus;
        while let Some(owner) = current {
            if self.scopes.get(&owner).is_some_and(|scope| scope.cycle_tab) {
                return self.tree.subtree_preorder(owner);
            }
            current = self.tree.parent(owner);
        }
        self.tree.preorder()
    }

    pub(super) fn dispatch_shortcut(
        &mut self,
        input: &InputEvent,
        context: &mut EventContext,
    ) -> Option<WidgetId> {
        let InputEvent::KeyDown {
            key,
            modifiers,
            repeat,
        } = input
        else {
            return None;
        };
        if !self.window_focused {
            return None;
        }
        // Escape belongs to the active drag, even in an isolated keyboard scope.
        if *key == Key::Escape && self.drag.active().is_some() {
            context.prevent_default();
            self.cancel_active_drag(DragCancelReason::Escape);
            return None;
        }
        let mut current = self.focus.or(Some(self.tree.root()));
        while let Some(owner) = current {
            if self.effective_enabled(owner)
                && let Some(entry) = self
                    .shortcuts
                    .iter_mut()
                    .find(|entry| entry.owner == owner && entry.chord.matches(key, *modifiers))
            {
                context.prevent_default();
                if !repeat || entry.allow_repeat {
                    (entry.handler)(
                        &ShortcutEvent {
                            owner,
                            focused: self.focus,
                            chord: KeyChord::new(key.clone(), *modifiers),
                            repeat: *repeat,
                        },
                        context,
                    );
                    return Some(owner);
                }
                return None;
            }
            if self
                .scopes
                .get(&owner)
                .is_some_and(|scope| scope.isolate_shortcuts)
            {
                break;
            }
            current = self.tree.parent(owner);
        }
        None
    }

    pub(super) fn prune_services(&mut self) {
        self.scopes.retain(|id, _| self.tree.contains(*id));
        self.shortcuts
            .retain(|entry| self.tree.contains(entry.owner));
        self.drop_handlers.retain(|id, _| self.tree.contains(*id));
    }

    fn notify_drop(&mut self, event: &DragEvent, payload: &DragPayload) -> Option<DropEffect> {
        event
            .target
            .and_then(|target| self.drop_handlers.get_mut(&target))
            .and_then(|handler| handler(event, payload))
    }

    fn cancel_active_drag(&mut self, reason: DragCancelReason) {
        let Some(session) = self.drag.active().cloned() else {
            return;
        };
        let event = self
            .drag
            .cancel(session.id, reason)
            .expect("active session");
        self.notify_drop(&event, &session.payload);
        if self.capture == Some(session.source) {
            self.capture = None;
        }
    }

    pub(super) fn clean_drag_state(&mut self) {
        let Some(session) = self.drag.active() else {
            return;
        };
        let reason = if !self.window_focused {
            Some(DragCancelReason::WindowBlur)
        } else if !self.effective_enabled(session.source) {
            Some(DragCancelReason::SourceUnavailable)
        } else if session.target.is_some_and(|id| !self.effective_enabled(id)) {
            Some(DragCancelReason::TargetUnavailable)
        } else if self.capture != Some(session.source) {
            Some(DragCancelReason::CaptureChanged)
        } else {
            None
        };
        if let Some(reason) = reason {
            self.cancel_active_drag(reason);
        }
    }

    pub(super) fn dispatch_drag(&mut self, input: &InputEvent) {
        if self.drag.active().is_none() {
            return;
        }
        match input {
            InputEvent::WindowFocused(false) => {
                self.cancel_active_drag(DragCancelReason::WindowBlur)
            }
            InputEvent::KeyDown {
                key: Key::Escape, ..
            } => self.cancel_active_drag(DragCancelReason::Escape),
            InputEvent::PointerLeft => self.update_drag_target(None),
            InputEvent::PointerMoved { position }
            | InputEvent::PointerUp {
                position,
                button: PointerButton::Primary,
                ..
            } => {
                self.update_drag_target(Some(*position));
                if matches!(input, InputEvent::PointerUp { .. })
                    && let Some(session) = self.drag.active().cloned()
                {
                    let event = self.drag.finish(session.id).expect("active session");
                    self.notify_drop(&event, &session.payload);
                    if self.capture == Some(session.source) {
                        self.capture = None;
                    }
                }
            }
            _ => {}
        }
    }

    pub(super) fn prepare_drag_dispatch(&mut self, input: &InputEvent) -> Result<(), RuntimeError> {
        if self.drag.active().is_none()
            || !matches!(
                input,
                InputEvent::PointerMoved { .. }
                    | InputEvent::PointerUp {
                        button: PointerButton::Primary,
                        ..
                    }
            )
        {
            return Ok(());
        }
        // Routed callbacks have already committed their deferred mutations.
        // Resolve transforms now, but never guess new layout without an engine.
        if self
            .tree
            .pending_dirty()
            .intersects(DirtyFlags::MEASURE | DirtyFlags::LAYOUT)
        {
            self.cancel_active_drag(DragCancelReason::GeometryUnavailable);
        } else if self.geometry_dirty
            && let Err(error) = self.resolve_geometry()
        {
            self.cancel_active_drag(DragCancelReason::GeometryUnavailable);
            return Err(error);
        }
        Ok(())
    }

    pub(super) fn refresh_drag_geometry(&mut self) {
        if self.drag.active().is_some() {
            self.update_drag_target(self.pointer);
        }
    }

    fn update_drag_target(&mut self, position: Option<Point>) {
        let Some(session) = self.drag.active().cloned() else {
            return;
        };
        let mut target = position.and_then(|point| self.hit_test(point));
        while let Some(id) = target {
            if self.drop_handlers.contains_key(&id) {
                break;
            }
            target = self.tree.parent(id);
        }
        let position = position.unwrap_or(session.position);
        if target != session.target && session.target.is_some() {
            let mut event = session.event(DragPhase::Leave);
            event.position = position;
            self.notify_drop(&event, &session.payload);
        }
        let mut event = session.event(DragPhase::Over);
        event.target = target;
        event.position = position;
        if target != session.target && target.is_some() {
            event.effect = None;
            event.phase = DragPhase::Enter;
            self.notify_drop(&event, &session.payload);
            event.phase = DragPhase::Over;
        }
        let effect = self
            .notify_drop(&event, &session.payload)
            .filter(|effect| session.allowed.contains(*effect));
        self.drag
            .hover(session.id, target, position, effect)
            .expect("validated input and negotiated effect");
    }
}
