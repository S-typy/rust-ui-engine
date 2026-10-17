//! Retained guarded workflows. Applications resolve requests outside UI borrows.
use super::wizard_model::{WizardSession, invalid};
use super::*;
use rust_desktop_ui_core::{GridPlacement, GridTrack, Modifiers};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WizardSettings {
    pub show_progress: bool,
    pub allow_cancel: bool,
    pub read_only: bool,
}
impl Default for WizardSettings {
    fn default() -> Self {
        Self {
            show_progress: true,
            allow_cancel: true,
            read_only: false,
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub struct Wizard {
    pub container: WidgetId,
    pub progress: StepProgress,
    pub status: WidgetId,
    pub back: WidgetId,
    pub next: WidgetId,
    pub finish: WidgetId,
    pub cancel: WidgetId,
}
struct Entry {
    parts: Wizard,
    label: String,
    tabs: TabbedGroup,
    session: WizardSession,
    settings: WizardSettings,
    context_version: u64,
    focus: HashMap<String, WidgetId>,
    parents: Vec<(WidgetId, WidgetId)>,
}
#[derive(Default)]
pub(super) struct Wizards {
    entries: HashMap<WidgetId, Entry>,
    switching: Option<WidgetId>,
}

impl Controls {
    pub fn wizard(
        &mut self,
        parent: WidgetId,
        label: impl Into<String>,
        model: WizardModel,
        settings: WizardSettings,
        mut style: LayoutStyle,
    ) -> Result<Wizard, ControlError> {
        if self.wizards.entries.len() >= 32 {
            return Err(invalid("At most 32 Wizard controls are supported"));
        }
        let label = label.into();
        self.measure_text(&label, 4096.0, false)?;
        let session = WizardSession::new(model);
        let progress = progress(
            &session.model,
            &session.history,
            &session.status,
            session.result,
        );
        self.prepare_step_progress(&label, &progress)?;
        self.measure_text(
            &status(&session.model, &session.history, &session.status),
            4096.0,
            false,
        )?;
        style.kind = LayoutKind::Grid;
        style.grid_columns = vec![GridTrack::Fr(1.0)];
        style.grid_rows = vec![
            GridTrack::Auto,
            GridTrack::Fr(1.0),
            GridTrack::Auto,
            GridTrack::Auto,
        ];
        style.gap = 8.0;
        let container = self.transparent_panel(parent, style)?;
        let result = (|| {
            self.set_role(container, Role::Group);
            self.set_automation_name(container, &label)?;
            let progress =
                self.step_progress(container, &label, progress, row(1, Length::Px(76.0)))?;
            let selected = session
                .model
                .steps()
                .iter()
                .position(|s| s.id == session.current())
                .unwrap();
            let tabs = self.tabbed_group(
                container,
                session
                    .model
                    .steps()
                    .iter()
                    .map(|s| GroupTab::new(&s.title))
                    .collect(),
                selected,
                row(2, Length::Percent(1.0)),
            )?;
            let tab_header = self.tabs[&tabs.container].header;
            self.edit_props(tab_header, |p| {
                p.visible = false;
                p.collapsed = true;
            })?;
            for header in &tabs.headers {
                self.edit_props(*header, |p| {
                    p.focusable = false;
                    p.tab_stop = false;
                })?;
            }
            let message = self.label(container, "", row(3, Length::Auto))?;
            self.set_role(message, Role::StatusBar);
            self.edit_props(message, |p| {
                p.focusable = true;
                p.tab_stop = false;
            })?;
            let actions = self.transparent_panel(
                container,
                LayoutStyle {
                    kind: LayoutKind::Wrap(Axis::Horizontal),
                    gap: 8.0,
                    ..row(4, Length::Auto)
                },
            )?;
            let back = self.button(actions, "Назад", None, button())?;
            let next = self.button(actions, "Далее", None, button())?;
            let finish = self.button(actions, "Завершить", None, button())?;
            let cancel = self.button(actions, "Отмена", None, button())?;
            let parts = Wizard {
                container,
                progress,
                status: message,
                back,
                next,
                finish,
                cancel,
            };
            let parents = self
                .runtime
                .tree()
                .preorder()
                .into_iter()
                .filter(|node| *node != container && self.flyout_descendant(*node, container))
                .filter_map(|node| {
                    self.runtime
                        .tree()
                        .parent(node)
                        .map(|parent| (node, parent))
                })
                .collect();
            self.wizards.entries.insert(
                container,
                Entry {
                    parts,
                    label,
                    tabs,
                    session,
                    settings,
                    context_version: 0,
                    focus: HashMap::new(),
                    parents,
                },
            );
            self.own_step_progress(progress.container, container);
            self.refresh_wizard(container)?;
            Ok(parts)
        })();
        if result.is_err() {
            self.remove(container)?;
        }
        result
    }
    pub fn wizard_parts(&self, id: WidgetId) -> Option<Wizard> {
        self.wizards.entries.get(&id).map(|e| e.parts)
    }
    pub fn wizard_model(&self, id: WidgetId) -> Option<&WizardModel> {
        self.wizards.entries.get(&id).map(|e| &e.session.model)
    }
    pub fn wizard_current_step(&self, id: WidgetId) -> Option<&str> {
        self.wizards.entries.get(&id).map(|e| e.session.current())
    }
    pub fn wizard_status(&self, id: WidgetId) -> Option<&WizardStatus> {
        self.wizards.entries.get(&id).map(|e| &e.session.status)
    }
    pub fn wizard_result(&self, id: WidgetId) -> Option<WizardResult> {
        self.wizards.entries.get(&id)?.session.result
    }
    pub fn wizard_history(&self, id: WidgetId) -> Option<&[String]> {
        self.wizards
            .entries
            .get(&id)
            .map(|e| e.session.history.as_slice())
    }
    pub fn wizard_settings(&self, id: WidgetId) -> Option<WizardSettings> {
        self.wizards.entries.get(&id).map(|e| e.settings)
    }
    pub fn wizard_page(&self, id: WidgetId, step: &str) -> Option<WidgetId> {
        let e = self.wizards.entries.get(&id)?;
        e.session
            .model
            .steps()
            .iter()
            .position(|s| s.id == step)
            .map(|index| e.tabs.contents[index])
    }
    pub fn set_wizard_read_only(
        &mut self,
        id: WidgetId,
        read_only: bool,
    ) -> Result<(), ControlError> {
        self.check_wizard_parts(id)?;
        let entry = self.wizards.entries.get_mut(&id).unwrap();
        entry.settings.read_only = read_only;
        if read_only {
            entry.session.cancel_pending();
        }
        self.refresh_wizard(id)
    }
    /// Call when application-owned data used by guards changes. Different
    /// versions cancel pending requests without changing retained content.
    pub fn set_wizard_context_version(
        &mut self,
        id: WidgetId,
        version: u64,
    ) -> Result<(), ControlError> {
        self.check_wizard_parts(id)?;
        let entry = self.wizards.entries.get_mut(&id).unwrap();
        if entry.context_version != version {
            entry.context_version = version;
            entry.session.cancel_pending();
        }
        self.refresh_wizard(id)
    }
    pub fn request_wizard(
        &mut self,
        id: WidgetId,
        intent: WizardIntent,
    ) -> Result<(), ControlError> {
        self.check_wizard_parts(id)?;
        if !self.wizard_available(id)
            || intent == WizardIntent::Cancel && !self.wizards.entries[&id].settings.allow_cancel
        {
            return Err(invalid("Wizard action is unavailable"));
        }
        self.check_wizard_composition(id)?;
        let e = self.wizards.entries.get_mut(&id).unwrap();
        e.session.request(id, intent, e.context_version)?;
        self.refresh_wizard(id)
    }
    pub fn drain_wizard_requests(&mut self) -> Vec<WizardRequest> {
        self.wizards
            .entries
            .values_mut()
            .filter_map(|e| e.session.take_request())
            .collect()
    }
    pub fn drain_wizard_events(&mut self) -> Vec<WizardEvent> {
        self.wizards
            .entries
            .iter_mut()
            .flat_map(|(&owner, e)| {
                std::mem::take(&mut e.session.events)
                    .into_iter()
                    .map(move |kind| WizardEvent { owner, kind })
            })
            .collect()
    }
    pub fn complete_wizard(
        &mut self,
        request: &WizardRequest,
        decision: WizardDecision,
    ) -> Result<WizardCompletion, ControlError> {
        let owner = request.owner();
        let Some(entry) = self.wizards.entries.get(&owner) else {
            return Ok(WizardCompletion::Stale);
        };
        if !entry.session.matches(request) {
            return Ok(WizardCompletion::Stale);
        }
        if !self.wizard_available(owner) {
            self.wizards
                .entries
                .get_mut(&owner)
                .unwrap()
                .session
                .cancel_pending();
            return Ok(WizardCompletion::Stale);
        }
        self.check_wizard_parts(owner)?;
        self.check_wizard_composition(owner)?;
        let entry = &self.wizards.entries[&owner];
        entry.session.preflight_complete(request, &decision)?;
        let mut history = entry.session.history.clone();
        let mut result = entry.session.result;
        let next_status = match &decision {
            WizardDecision::Step(target) => {
                if request.intent() == WizardIntent::Back {
                    history.pop();
                } else {
                    history.push(target.clone());
                }
                WizardStatus::Ready
            }
            WizardDecision::Reject(message) => WizardStatus::Error(message.clone()),
            WizardDecision::Finish => {
                result = Some(WizardResult::Finished);
                WizardStatus::Finished
            }
            WizardDecision::Cancel => {
                result = Some(WizardResult::Cancelled);
                WizardStatus::Cancelled
            }
        };
        let presentation = progress(&entry.session.model, &history, &next_status, result);
        let label = entry.label.clone();
        let message = status(&entry.session.model, &history, &next_status);
        self.prepare_step_progress(&label, &presentation)?;
        self.measure_text(&message, 4096.0, false)?;
        if let WizardDecision::Step(target) = &decision {
            self.switch_wizard_page(owner, target)?;
        }
        let progress_id = self.wizards.entries[&owner].parts.progress.container;
        self.apply_step_progress_value(progress_id, presentation)?;
        let completion = self
            .wizards
            .entries
            .get_mut(&owner)
            .unwrap()
            .session
            .publish(request, decision);
        self.refresh_wizard(owner)?;
        // A validation result must not blur an ordinary editor: its configured
        // LostFocus policy could otherwise accept the draft during rejection
        // or cancellation. The retained status remains separately accessible.
        Ok(completion)
    }
    /// Explicitly restart the same retained pages. Drafts, selection, undo and
    /// application data remain; a reset does not recreate or clear documents.
    pub fn reset_wizard(&mut self, id: WidgetId) -> Result<(), ControlError> {
        self.check_wizard_parts(id)?;
        self.check_wizard_composition(id)?;
        let entry = &self.wizards.entries[&id];
        entry.session.preflight_event()?;
        let start = entry.session.model.start().to_owned();
        let presentation = progress(
            &entry.session.model,
            std::slice::from_ref(&start),
            &WizardStatus::Ready,
            None,
        );
        let label = entry.label.clone();
        self.prepare_step_progress(&label, &presentation)?;
        self.switch_wizard_page(id, &start)?;
        self.apply_step_progress_value(
            self.wizards.entries[&id].parts.progress.container,
            presentation,
        )?;
        self.wizards.entries.get_mut(&id).unwrap().session.reset();
        self.refresh_wizard(id)
    }
    fn wizard_available(&self, id: WidgetId) -> bool {
        self.wizards.entries.get(&id).is_some_and(|e| {
            !e.settings.read_only
                && self.window_focused
                && self.is_enabled(id)
                && self.runtime.tree().is_attached(id)
        })
    }
    fn check_wizard_composition(&self, id: WidgetId) -> Result<(), ControlError> {
        if self.runtime.focused().is_some_and(|focused| {
            (self.flyout_descendant(focused, id)
                || self.flyout_focus_anchor_in(id).is_some()
                || self.rich.popup.as_ref().is_some_and(|popup| {
                    self.flyout_descendant(popup.owner, id)
                        && self.flyout_descendant(focused, popup.surface)
                }))
                && self
                    .document(focused)
                    .is_some_and(|d| d.composition().is_some())
        }) {
            return Err(invalid(
                "Finish composition before changing the wizard step",
            ));
        }
        Ok(())
    }
    fn check_wizard_parts(&self, id: WidgetId) -> Result<(), ControlError> {
        let e = self
            .wizards
            .entries
            .get(&id)
            .ok_or_else(|| invalid("Unknown Wizard"))?;
        if e.parents
            .iter()
            .any(|(node, parent)| self.runtime.tree().parent(*node) != Some(*parent))
        {
            return Err(invalid(
                "Wizard required parts must remain in their owned hosts",
            ));
        }
        if [
            e.parts.status,
            e.parts.back,
            e.parts.next,
            e.parts.finish,
            e.parts.cancel,
            e.tabs.container,
        ]
        .iter()
        .any(|id| !self.runtime.tree().contains(*id))
        {
            return Err(invalid("Wizard required content was removed"));
        }
        let Some(tabs) = self.tabs.get(&e.tabs.container) else {
            return Err(invalid("Wizard tab host was removed"));
        };
        if tabs.tabs != e.tabs.headers
            || tabs.pages != e.tabs.contents
            || tabs.selected
                != e.session
                    .model
                    .steps()
                    .iter()
                    .position(|s| s.id == e.session.current())
                    .unwrap()
        {
            return Err(invalid("Wizard owns its retained page order and selection"));
        }
        if self
            .runtime
            .tree()
            .node(tabs.header)
            .is_none_or(|node| !node.props.collapsed)
            || tabs
                .tabs
                .iter()
                .any(|id| self.controls.get(id).is_none_or(|control| !control.enabled))
        {
            return Err(invalid("Wizard owns its hidden tab headers"));
        }
        self.check_step_progress_parts(e.parts.progress.container)
    }
    pub(super) fn check_wizard_tab_transition(&self, tabs: WidgetId) -> Result<(), ControlError> {
        if self
            .wizards
            .entries
            .iter()
            .any(|(&owner, e)| e.tabs.container == tabs && self.wizards.switching != Some(owner))
        {
            Err(invalid(
                "Complete a Wizard request instead of selecting or reordering its tab host",
            ))
        } else {
            Ok(())
        }
    }
    pub(super) fn wizard_switches_tab_focus(&self, tabs: WidgetId) -> bool {
        self.wizards
            .switching
            .is_some_and(|owner| self.wizards.entries[&owner].tabs.container == tabs)
    }
    fn switch_wizard_page(&mut self, id: WidgetId, target: &str) -> Result<(), ControlError> {
        let e = &self.wizards.entries[&id];
        let old = e.session.current().to_owned();
        let tab = e.tabs.container;
        let index = e
            .session
            .model
            .steps()
            .iter()
            .position(|s| s.id == target)
            .ok_or_else(|| invalid("Unknown wizard target"))?;
        let page = e.tabs.contents[index];
        let old_page = self.wizard_page(id, &old).unwrap();
        if let Some(focused) = self
            .runtime
            .focused()
            .filter(|id| self.flyout_descendant(*id, old_page))
        {
            self.wizards
                .entries
                .get_mut(&id)
                .unwrap()
                .focus
                .insert(old, focused);
        }
        self.wizards.switching = Some(id);
        let result = self.set_selected_tab(tab, index);
        self.wizards.switching = None;
        result?;
        let saved = self.wizards.entries[&id]
            .focus
            .get(target)
            .copied()
            .filter(|id| {
                self.runtime
                    .tree()
                    .node(*id)
                    .is_some_and(|node| node.props.focusable)
                    && self.is_enabled(*id)
                    && self.flyout_descendant(*id, page)
            });
        let focus = saved
            .or_else(|| {
                self.runtime.navigation_order().into_iter().find(|id| {
                    self.flyout_descendant(*id, page)
                        && self.is_enabled(*id)
                        && self
                            .runtime
                            .tree()
                            .node(*id)
                            .is_some_and(|node| node.props.focusable && node.props.tab_stop)
                })
            })
            .unwrap_or(self.wizards.entries[&id].parts.status);
        self.runtime.apply(Mutation::Focus(Some(focus)))?;
        Ok(())
    }
    fn refresh_wizard(&mut self, id: WidgetId) -> Result<(), ControlError> {
        let e = &self.wizards.entries[&id];
        let p = e.parts;
        let settings = e.settings;
        let final_step = e.session.model.step(e.session.current()).unwrap().is_final;
        let message = status(&e.session.model, &e.session.history, &e.session.status);
        let enabled = [
            WizardIntent::Back,
            WizardIntent::Next,
            WizardIntent::Finish,
            WizardIntent::Cancel,
        ]
        .map(|intent| {
            !settings.read_only
                && e.session.can_request(intent)
                && (intent != WizardIntent::Cancel || settings.allow_cancel)
        });
        self.set_label(p.status, &message)?;
        self.set_automation_name(
            p.container,
            format!("{}. {message}", self.wizards.entries[&id].label),
        )?;
        for (button, enabled) in [p.back, p.next, p.finish, p.cancel]
            .into_iter()
            .zip(enabled)
        {
            self.set_enabled(button, enabled)?;
        }
        for (node, visible) in [
            (p.next, !final_step),
            (p.finish, final_step),
            (p.cancel, settings.allow_cancel),
            (p.progress.container, settings.show_progress),
        ] {
            self.edit_props(node, |p| {
                p.visible = visible;
                p.collapsed = !visible;
            })?;
        }
        Ok(())
    }
    pub(super) fn invalidate_wizard_editor(&mut self, editor: WidgetId) {
        let ids: Vec<_> = self
            .wizards
            .entries
            .iter()
            .filter_map(|(&id, e)| {
                self.wizard_page(id, e.session.current())
                    .filter(|page| {
                        self.flyout_descendant(editor, *page)
                            || (self.runtime.focused() == Some(editor)
                                && self.flyout_focus_anchor_in(*page).is_some())
                    })
                    .map(|_| id)
            })
            .collect();
        for id in ids {
            self.wizards
                .entries
                .get_mut(&id)
                .unwrap()
                .session
                .cancel_pending();
        }
    }
    pub(super) fn sync_wizards(&mut self) -> Result<(), ControlError> {
        self.wizards
            .entries
            .retain(|id, _| self.runtime.tree().contains(*id));
        self.prune_step_progresses();
        for id in self.wizards.entries.keys().copied().collect::<Vec<_>>() {
            self.check_wizard_parts(id)?;
            if !self.wizard_available(id) {
                self.wizards
                    .entries
                    .get_mut(&id)
                    .unwrap()
                    .session
                    .cancel_pending();
            }
            self.refresh_wizard(id)?;
        }
        Ok(())
    }
    pub(super) fn preflight_wizard_removal(&self, id: WidgetId) -> Result<(), ControlError> {
        if self.wizards.entries.values().any(|e| {
            [
                e.parts.status,
                e.parts.back,
                e.parts.next,
                e.parts.finish,
                e.parts.cancel,
                e.tabs.container,
            ]
            .contains(&id)
        }) {
            return Err(invalid("Remove the Wizard owner, not its required content"));
        }
        Ok(())
    }
    pub(super) fn activate_wizard(&mut self, button: WidgetId) -> Result<bool, ControlError> {
        let action = self.wizards.entries.iter().find_map(|(&id, e)| {
            [
                (e.parts.back, WizardIntent::Back),
                (e.parts.next, WizardIntent::Next),
                (e.parts.finish, WizardIntent::Finish),
                (e.parts.cancel, WizardIntent::Cancel),
            ]
            .into_iter()
            .find_map(|(part, intent)| (part == button).then_some((id, intent)))
        });
        if let Some((id, intent)) = action {
            if self.wizard_available(id) {
                self.request_wizard(id, intent)?;
            }
            return Ok(true);
        }
        Ok(false)
    }
    pub(super) fn wizard_before_input(&mut self, event: &InputEvent) -> Result<bool, ControlError> {
        if matches!(event, InputEvent::WindowFocused(false)) {
            for e in self.wizards.entries.values_mut() {
                e.session.cancel_pending();
            }
            return Ok(false);
        }
        if let InputEvent::PointerUp {
            position,
            button: PointerButton::Primary,
            ..
        } = event
            && let Some(target) = self.pressed.filter(|_| !self.keyboard_pressed)
            && self.wizards.entries.values().any(|e| {
                [e.parts.back, e.parts.next, e.parts.finish, e.parts.cancel].contains(&target)
            })
        {
            let hit = self.runtime.hit_test(*position);
            let report = self.runtime.dispatch(event.clone());
            self.prune_removed_controls()?;
            self.pressed = None;
            self.runtime.apply(Mutation::CapturePointer(None))?;
            if let Some(error) = report.errors.into_iter().next() {
                return Err(error.into());
            }
            if !report.default_prevented && hit == Some(target) && self.activation_available(target)
            {
                self.activate_wizard(target)?;
            }
            self.scene_dirty = true;
            return Ok(true);
        }
        if let InputEvent::PointerDown {
            position,
            button: PointerButton::Primary,
            ..
        } = event
            && let Some(target) = self.runtime.hit_test(*position)
        {
            if let Some(owner) = self.wizards.entries.iter().find_map(|(&owner, e)| {
                [e.parts.back, e.parts.next, e.parts.finish, e.parts.cancel]
                    .contains(&target)
                    .then_some(owner)
            }) && self.activation_available(target)
            {
                // Footer actions request a guard; they must not accept an
                // editor merely because the pointer moves focus to chrome.
                if self.check_wizard_composition(owner).is_err() {
                    return Ok(true);
                }
                let step = self.wizards.entries[&owner].session.current().to_owned();
                let page = self.wizard_page(owner, &step).unwrap();
                if let Some(focus) = self
                    .runtime
                    .focused()
                    .filter(|id| self.flyout_descendant(*id, page))
                    .or_else(|| self.flyout_focus_anchor_in(page))
                {
                    self.wizards
                        .entries
                        .get_mut(&owner)
                        .unwrap()
                        .focus
                        .insert(step, focus);
                    self.observed_editor_focus = None;
                }
                let report = self.runtime.dispatch(event.clone());
                self.prune_removed_controls()?;
                if let Some(error) = report.errors.into_iter().next() {
                    return Err(error.into());
                }
                if !report.default_prevented && self.activation_available(target) {
                    self.pressed = Some(target);
                    self.keyboard_pressed = false;
                    self.runtime.apply(Mutation::CapturePointer(Some(target)))?;
                }
                self.scene_dirty = true;
                return Ok(true);
            }
            self.invalidate_wizard_editor(target);
        }
        let focused = self.runtime.focused();
        let owner = focused.and_then(|focused| {
            self.wizards.entries.keys().copied().find(|id| {
                self.flyout_descendant(focused, *id) || self.flyout_focus_anchor_in(*id).is_some()
            })
        });
        let Some(owner) = owner else {
            return Ok(false);
        };
        if let InputEvent::KeyDown {
            key,
            modifiers,
            repeat,
        } = event
        {
            let intent = if *key == Key::Escape && *modifiers == Modifiers::default() {
                Some(WizardIntent::Cancel)
            } else if modifiers.alt
                && !modifiers.control
                && !modifiers.shift
                && !modifiers.super_key
            {
                match key {
                    Key::ArrowRight => Some(WizardIntent::Next),
                    Key::ArrowLeft => Some(WizardIntent::Back),
                    _ => None,
                }
            } else if *key == Key::Enter
                && modifiers.control
                && !modifiers.alt
                && !modifiers.shift
                && !modifiers.super_key
            {
                Some(WizardIntent::Finish)
            } else {
                None
            };
            if let Some(intent) = intent {
                if focused
                    .is_some_and(|id| self.document(id).is_some_and(|d| d.composition().is_some()))
                {
                    if *key == Key::Escape && !repeat {
                        self.cancel_preedit();
                    }
                    return Ok(*key == Key::Escape);
                }
                if !repeat
                    && self.wizard_available(owner)
                    && self.wizards.entries[&owner].session.can_request(intent)
                    && (intent != WizardIntent::Cancel
                        || self.wizards.entries[&owner].settings.allow_cancel)
                {
                    self.request_wizard(owner, intent)?;
                }
                return Ok(true);
            }
        }
        if matches!(event, InputEvent::KeyDown { .. })
            && let Some(id) = focused
        {
            self.invalidate_wizard_editor(id);
        }
        Ok(false)
    }
}
fn row(index: u16, height: Length) -> LayoutStyle {
    LayoutStyle {
        width: Length::Percent(1.0),
        height,
        grid_row: GridPlacement {
            start: Some(index),
            span: 1,
        },
        ..Default::default()
    }
}
fn button() -> LayoutStyle {
    LayoutStyle {
        width: Length::Px(120.0),
        height: Length::Px(36.0),
        flex_shrink: 0.0,
        ..Default::default()
    }
}
fn progress(
    model: &WizardModel,
    history: &[String],
    status: &WizardStatus,
    result: Option<WizardResult>,
) -> StepProgressValue {
    let current = history.last().unwrap();
    StepProgressValue {
        current: current.clone(),
        steps: model
            .steps()
            .iter()
            .map(|s| StepProgressItem {
                id: s.id.clone(),
                title: s.title.clone(),
                state: if &s.id == current {
                    if matches!(status, WizardStatus::Error(_)) {
                        StepState::Error
                    } else if result == Some(WizardResult::Finished) {
                        StepState::Completed
                    } else {
                        StepState::Current
                    }
                } else if history.contains(&s.id) {
                    StepState::Completed
                } else if result.is_some() {
                    StepState::Skipped
                } else {
                    StepState::Upcoming
                },
            })
            .collect(),
    }
}
fn status(model: &WizardModel, history: &[String], state: &WizardStatus) -> String {
    let current = history.last().unwrap();
    let index = model.steps().iter().position(|s| &s.id == current).unwrap();
    let suffix = match state {
        WizardStatus::Ready => "Готов к переходу",
        WizardStatus::Pending(_) => "Ожидание проверки",
        WizardStatus::Error(message) => message,
        WizardStatus::Finished => "Процесс завершён",
        WizardStatus::Cancelled => "Процесс отменён",
    };
    format!(
        "Шаг {} из {}: {}. {suffix}",
        index + 1,
        model.steps().len(),
        model.steps()[index].title
    )
}
