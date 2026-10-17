//! A retained wait surface with explicit generation-bound progress and completion.
use super::notifications_model::invalid;
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LoadingOverlay {
    pub container: WidgetId,
    pub content: WidgetId,
    pub overlay: WidgetId,
    pub message: WidgetId,
    pub progress: WidgetId,
    pub cancel: WidgetId,
}
/// Only the session returned by `begin_loading` can publish later progress.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LoadingToken {
    owner: WidgetId,
    generation: u64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LoadingResult {
    Succeeded,
    Failed(String),
    Cancelled,
}
#[derive(Clone, Debug, PartialEq)]
pub enum LoadingStatus {
    Idle,
    Running {
        message: String,
        progress: Option<f64>,
    },
    Finished(LoadingResult),
}
struct Entry {
    parts: LoadingOverlay,
    card: WidgetId,
    parents: Vec<(WidgetId, WidgetId)>,
    generation: u64,
    status: LoadingStatus,
    completion: Option<LoadingResult>,
    focus: Option<WidgetId>,
    content_enabled: bool,
}
#[derive(Default)]
pub(super) struct Loadings {
    entries: HashMap<WidgetId, Entry>,
}
impl Controls {
    pub fn loading_overlay(
        &mut self,
        parent: WidgetId,
        label: impl Into<String>,
        mut style: LayoutStyle,
    ) -> Result<LoadingOverlay, ControlError> {
        if self.loadings.entries.len() >= 64 {
            return Err(invalid("At most 64 loading overlays are supported"));
        }
        let label = label.into();
        self.measure_text(&label, 600.0, false)?;
        style.kind = LayoutKind::Overlay;
        let container = self.transparent_panel(parent, style)?;
        let result = (|| {
            let fill = LayoutStyle {
                width: Length::Percent(1.0),
                height: Length::Percent(1.0),
                ..Default::default()
            };
            let content = self.transparent_panel(container, fill.clone())?;
            let overlay = self.panel(
                container,
                LayoutStyle {
                    kind: LayoutKind::Overlay,
                    ..fill
                },
            )?;
            self.edit_props(overlay, |p| {
                p.visible = false;
                p.z_index = 20;
            })?;
            self.set_role(overlay, Role::Group);
            self.set_automation_name(overlay, label)?;
            let card = self.panel(
                overlay,
                LayoutStyle {
                    kind: LayoutKind::Flex(Axis::Vertical),
                    padding: Edges::all(12.0),
                    gap: 8.0,
                    ..Default::default()
                },
            )?;
            self.controls.get_mut(&card).unwrap().popup_surface = true;
            let message = self.label(card, "", row(40.0))?;
            let progress = self.progress_bar(
                card,
                ProgressBarSettings {
                    minimum: 0.0,
                    maximum: 1.0,
                    value: 0.0,
                },
                row(20.0),
            )?;
            self.set_automation_name(progress, "Loading progress")?;
            let cancel = self.button(card, "Cancel", None, row(36.0))?;
            let parts = LoadingOverlay {
                container,
                content,
                overlay,
                message,
                progress,
                cancel,
            };
            let mut parents = Vec::new();
            let mut stack = vec![content, overlay];
            while let Some(id) = stack.pop() {
                parents.push((id, self.runtime.tree().parent(id).unwrap()));
                stack.extend(
                    self.runtime
                        .tree()
                        .children(id)
                        .unwrap_or(&[])
                        .iter()
                        .copied(),
                );
            }
            Ok::<_, ControlError>((parts, card, parents))
        })();
        let (parts, card, parents) = match result {
            Ok(parts) => parts,
            Err(e) => {
                self.remove(container)?;
                return Err(e);
            }
        };
        self.loadings.entries.insert(
            container,
            Entry {
                parts,
                card,
                parents,
                generation: 0,
                status: LoadingStatus::Idle,
                completion: None,
                focus: None,
                content_enabled: true,
            },
        );
        Ok(parts)
    }
    fn check_loading(&self, id: WidgetId) -> Result<(), ControlError> {
        let e = self
            .loadings
            .entries
            .get(&id)
            .ok_or_else(|| invalid("Unknown LoadingOverlay"))?;
        if !self.runtime.tree().contains(id)
            || e.parents
                .iter()
                .any(|(id, parent)| self.runtime.tree().parent(*id) != Some(*parent))
        {
            return Err(invalid(
                "Loading overlay required parts were removed or reparented",
            ));
        }
        if self
            .progress_settings(e.parts.progress)
            .is_none_or(|s| s.minimum != 0.0 || s.maximum != 1.0)
        {
            return Err(invalid("Loading progress recipe was replaced"));
        }
        Ok(())
    }
    fn validate_loading_message(&mut self, message: &str) -> Result<(), ControlError> {
        if message.len() > 1024 || self.measure_text(message, 216.0, true)?.height > 80.0 {
            return Err(invalid("Loading message exceeds the supported card height"));
        }
        Ok(())
    }
    pub fn begin_loading(
        &mut self,
        id: WidgetId,
        message: impl Into<String>,
        cancellable: bool,
    ) -> Result<LoadingToken, ControlError> {
        self.check_loading(id)?;
        let message = message.into();
        self.validate_loading_message(&message)?;
        let e = &self.loadings.entries[&id];
        if matches!(e.status, LoadingStatus::Running { .. }) || e.completion.is_some() {
            return Err(invalid(
                "Finish loading and take its result before beginning another session",
            ));
        }
        if !self.effective_enabled(id)
            || !self
                .runtime
                .bounds(id)
                .is_some_and(|b| b.width >= 240.0 && b.height >= 176.0)
        {
            return Err(invalid(
                "LoadingOverlay requires an available laid-out area of at least 240 by 176",
            ));
        }
        let generation = e
            .generation
            .checked_add(1)
            .ok_or_else(|| invalid("Loading identity exhausted"))?;
        let parts = e.parts;
        let bounds = self.runtime.bounds(id).unwrap();
        let message_height = self
            .measure_text(&message, (bounds.width - 48.0).clamp(1.0, 396.0), true)?
            .height
            .max(20.0);
        if bounds.height < message_height + 120.0 {
            return Err(invalid(
                "Loading message does not fit the current owner area",
            ));
        }
        let focus = self
            .runtime
            .focused()
            .filter(|f| self.flyout_descendant(*f, parts.content));
        let content_enabled = self.controls[&parts.content].enabled;
        self.set_label(parts.message, &message)?;
        self.set_progress_indeterminate(parts.progress, true)?;
        self.set_enabled(parts.cancel, cancellable)?;
        if focus.is_some() {
            self.cancel_preedit();
            self.observed_editor_focus = None;
        }
        self.set_enabled(parts.content, false)?;
        self.edit_props(parts.overlay, |p| p.visible = true)?;
        let e = self.loadings.entries.get_mut(&id).unwrap();
        e.generation = generation;
        e.status = LoadingStatus::Running {
            message,
            progress: None,
        };
        e.focus = focus;
        e.content_enabled = content_enabled;
        // Wait input is explicit; keep a recoverable content focus and never
        // accept an editor merely because this temporary surface appeared.
        if cancellable && focus.is_some() {
            self.try_popup_focus(parts.cancel)?;
        }
        Ok(LoadingToken {
            owner: id,
            generation,
        })
    }
    pub fn update_loading(
        &mut self,
        token: LoadingToken,
        message: impl Into<String>,
        progress: Option<f64>,
    ) -> Result<bool, ControlError> {
        if !self.loading_token_live(token) {
            return Ok(false);
        }
        self.check_loading(token.owner)?;
        if !self.effective_enabled(token.owner) {
            self.finish_loading_inner(token, LoadingResult::Cancelled, false)?;
            return Ok(false);
        }
        let message = message.into();
        self.validate_loading_message(&message)?;
        if progress.is_some_and(|p| !p.is_finite() || !(0.0..=1.0).contains(&p)) {
            return Err(invalid("Loading progress must be in [0,1]"));
        }
        let parts = self.loadings.entries[&token.owner].parts;
        if let Some(bounds) = self.runtime.bounds(token.owner) {
            let height = self
                .measure_text(&message, (bounds.width - 48.0).clamp(1.0, 396.0), true)?
                .height
                .max(20.0);
            if bounds.height < height + 120.0 {
                return Err(invalid(
                    "Loading message does not fit the current owner area",
                ));
            }
        }
        self.set_label(parts.message, &message)?;
        self.set_progress_indeterminate(parts.progress, progress.is_none())?;
        if let Some(value) = progress {
            self.set_progress_value(parts.progress, value)?;
        }
        self.loadings.entries.get_mut(&token.owner).unwrap().status =
            LoadingStatus::Running { message, progress };
        Ok(true)
    }
    fn loading_token_live(&self, token: LoadingToken) -> bool {
        self.loadings.entries.get(&token.owner).is_some_and(|e| {
            e.generation == token.generation && matches!(e.status, LoadingStatus::Running { .. })
        })
    }
    pub fn finish_loading(
        &mut self,
        token: LoadingToken,
        result: LoadingResult,
    ) -> Result<bool, ControlError> {
        if !self.loading_token_live(token) {
            return Ok(false);
        }
        if !self.effective_enabled(token.owner) {
            return self.finish_loading_inner(token, LoadingResult::Cancelled, false);
        }
        if let LoadingResult::Failed(message) = &result {
            self.validate_loading_message(message)?;
        }
        self.finish_loading_inner(token, result, true)
    }
    fn finish_loading_inner(
        &mut self,
        token: LoadingToken,
        result: LoadingResult,
        restore: bool,
    ) -> Result<bool, ControlError> {
        if !self.loading_token_live(token) {
            return Ok(false);
        }
        self.check_loading(token.owner)?;
        let e = &self.loadings.entries[&token.owner];
        let parts = e.parts;
        let previous = e.focus;
        let enabled = e.content_enabled;
        let owned_focus = self
            .runtime
            .focused()
            .is_none_or(|id| self.flyout_descendant(id, parts.overlay));
        self.set_progress_indeterminate(parts.progress, false)?;
        self.edit_props(parts.overlay, |p| p.visible = false)?;
        self.set_enabled(parts.content, enabled)?;
        let e = self.loadings.entries.get_mut(&token.owner).unwrap();
        e.status = LoadingStatus::Finished(result.clone());
        e.completion = Some(result);
        e.focus = None;
        if restore
            && owned_focus
            && let Some(focus) = previous.filter(|id| {
                self.effective_enabled(*id)
                    && self.flyout_descendant(*id, parts.content)
                    && self
                        .runtime
                        .tree()
                        .node(*id)
                        .is_some_and(|node| node.props.focusable)
            })
        {
            self.try_popup_focus(focus)?;
        }
        Ok(true)
    }
    pub fn loading_status(&self, id: WidgetId) -> Option<&LoadingStatus> {
        Some(&self.loadings.entries.get(&id)?.status)
    }
    pub fn take_loading_result(&mut self, id: WidgetId) -> Option<LoadingResult> {
        self.loadings.entries.get_mut(&id)?.completion.take()
    }
    pub fn loading_parts(&self, id: WidgetId) -> Option<LoadingOverlay> {
        Some(self.loadings.entries.get(&id)?.parts)
    }
    pub(super) fn activate_loading(&mut self, button: WidgetId) -> Result<bool, ControlError> {
        let token = self.loadings.entries.iter().find_map(|(&id, e)| {
            (e.parts.cancel == button && matches!(e.status, LoadingStatus::Running { .. }))
                .then_some(LoadingToken {
                    owner: id,
                    generation: e.generation,
                })
        });
        let Some(token) = token else {
            return Ok(false);
        };
        if self.activation_available(button) {
            self.finish_loading(token, LoadingResult::Cancelled)?;
        }
        Ok(true)
    }
    pub(super) fn sync_loadings(&mut self) -> Result<(), ControlError> {
        let owners: Vec<_> = self.loadings.entries.keys().copied().collect();
        for owner in owners {
            self.check_loading(owner)?;
            let e = &self.loadings.entries[&owner];
            if !matches!(e.status, LoadingStatus::Running { .. }) {
                continue;
            }
            let token = LoadingToken {
                owner,
                generation: e.generation,
            };
            let card = e.card;
            let message_widget = e.parts.message;
            let message = match &e.status {
                LoadingStatus::Running { message, .. } => message.clone(),
                _ => unreachable!(),
            };
            let bounds = self
                .runtime
                .bounds(owner)
                .unwrap_or(Rect::new(0.0, 0.0, 0.0, 0.0));
            if !self.effective_enabled(owner) || bounds.width < 240.0 || bounds.height < 176.0 {
                self.finish_loading_inner(token, LoadingResult::Cancelled, false)?;
                continue;
            }
            let width = (bounds.width - 24.0).min(420.0);
            let message_height = self
                .measure_text(&message, (width - 24.0).max(1.0), true)?
                .height
                .max(20.0);
            let height = message_height + 96.0;
            if height + 24.0 > bounds.height {
                self.finish_loading_inner(token, LoadingResult::Cancelled, false)?;
                continue;
            }
            self.edit_props(message_widget, |p| {
                p.style.height = Length::Px(message_height)
            })?;
            self.edit_props(card, |p| {
                p.style.width = Length::Px(width);
                p.style.height = Length::Px(height);
                p.style.offset =
                    Point::new((bounds.width - width) / 2.0, (bounds.height - height) / 2.0);
            })?;
        }
        Ok(())
    }
    pub(super) fn preflight_loading_removal(&self, id: WidgetId) -> Result<(), ControlError> {
        if self.loadings.entries.iter().any(|(owner, e)| {
            !self.flyout_descendant(*owner, id) && e.parents.iter().any(|(part, _)| *part == id)
        }) {
            Err(invalid(
                "Remove the LoadingOverlay owner, not its required parts",
            ))
        } else {
            Ok(())
        }
    }
    pub(super) fn prune_loadings(&mut self) {
        self.loadings
            .entries
            .retain(|id, _| self.runtime.tree().contains(*id));
    }
}
fn row(height: f32) -> LayoutStyle {
    LayoutStyle {
        width: Length::Percent(1.0),
        height: Length::Px(height),
        flex_shrink: 0.0,
        ..Default::default()
    }
}
