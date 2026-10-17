//! Retained in-window toast cards. Showing and pointer actions preserve focus.
use super::notifications_model::*;
use super::*;
use std::{collections::VecDeque, time::Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NotificationCenter {
    pub container: WidgetId,
}
#[derive(Clone, Debug)]
pub struct NotificationParts {
    pub card: WidgetId,
    pub title: WidgetId,
    pub message: WidgetId,
    pub dismiss: WidgetId,
    pub actions: Vec<WidgetId>,
}
struct Item {
    id: NotificationId,
    spec: NotificationSpec,
    parts: NotificationParts,
    parents: Vec<(WidgetId, WidgetId)>,
    deadline: Option<Instant>,
    invoking: bool,
}
struct Entry {
    layer: WidgetId,
    settings: NotificationCenterSettings,
    next: u64,
    queue: VecDeque<Item>,
    history: VecDeque<NotificationRecord>,
}
#[derive(Default)]
pub(super) struct Notifications {
    entries: HashMap<WidgetId, Entry>,
}

impl Controls {
    /// Create a logical owner in the form. Cards are an overlay in the lower
    /// right client corner and consume no space in the owner's layout.
    pub fn notification_center(
        &mut self,
        parent: WidgetId,
        settings: NotificationCenterSettings,
        style: LayoutStyle,
    ) -> Result<NotificationCenter, ControlError> {
        settings.validate()?;
        if self.notifications.entries.len() >= 16 {
            return Err(invalid("At most 16 notification centers are supported"));
        }
        let container = self.transparent_panel(parent, style)?;
        let result = self.transparent_panel(
            self.overlay,
            LayoutStyle {
                kind: LayoutKind::Overlay,
                ..Default::default()
            },
        );
        let layer = match result {
            Ok(layer) => layer,
            Err(e) => {
                self.remove(container)?;
                return Err(e);
            }
        };
        self.edit_props(layer, |p| {
            p.visible = false;
            p.clip = false;
            p.z_index = 1100;
            p.hit_test = false;
        })?;
        self.notifications.entries.insert(
            container,
            Entry {
                layer,
                settings,
                next: 1,
                queue: VecDeque::new(),
                history: VecDeque::new(),
            },
        );
        Ok(NotificationCenter { container })
    }
    fn check_notification_parts(&self, owner: WidgetId) -> Result<(), ControlError> {
        let e = self
            .notifications
            .entries
            .get(&owner)
            .ok_or_else(|| invalid("Unknown notification center"))?;
        if !self.runtime.tree().contains(owner)
            || self.runtime.tree().parent(e.layer) != Some(self.overlay)
            || e.queue.iter().any(|item| {
                item.parents
                    .iter()
                    .any(|(id, parent)| self.runtime.tree().parent(*id) != Some(*parent))
            })
        {
            return Err(invalid(
                "Notification required parts were removed or reparented",
            ));
        }
        Ok(())
    }
    pub fn post_notification(
        &mut self,
        owner: WidgetId,
        spec: NotificationSpec,
    ) -> Result<NotificationId, ControlError> {
        self.check_notification_parts(owner)?;
        if !self.effective_enabled(owner) {
            return Err(invalid("Notification owner is unavailable"));
        }
        spec.validate()?;
        let entry = &self.notifications.entries[&owner];
        if entry.queue.len() >= MAX_PENDING_NOTIFICATIONS {
            return Err(invalid("Notification queue is full"));
        }
        let next = entry
            .next
            .checked_add(1)
            .ok_or_else(|| invalid("Notification identity exhausted"))?;
        let id = NotificationId {
            owner,
            serial: entry.next,
        };
        // Validate the narrowest supported presentation before creating widgets.
        // A label reserves eight pixels on each side in addition to the card
        // padding. Measure the same content width that its TextRun will use.
        if self.measure_text(&spec.title, 162.0, true)?.height > 60.0
            || self.measure_text(&spec.message, 200.0, true)?.height > 160.0
        {
            return Err(invalid(
                "Notification text needs more than the bounded card height",
            ));
        }
        for action in &spec.actions {
            // Actions remain single-line buttons when the viewport shrinks to
            // the minimum 240-pixel card. Measure the full button, including
            // its own padding, without first capping its natural width.
            // Standard themes use at most 15px text; accepting in compact
            // density must also remain valid after returning to comfortable.
            let required = self.measure_button_content(
                &action.label,
                &ButtonStyle {
                    font_size: Some(self.theme.tokens().font_size.max(15.0)),
                    ..Default::default()
                },
                None,
                ButtonContentLayout::Leading,
                16_384.0,
            )?;
            if required.width > 216.0 || required.height > 32.0 {
                return Err(invalid(
                    "Notification action label must fit a 216 by 32 button including padding",
                ));
            }
            if !self.commands.contains(&action.command) {
                return Err(invalid("Unknown notification action command"));
            }
        }
        let layer = self.notifications.entries[&owner].layer;
        let card = self.panel(
            layer,
            LayoutStyle {
                kind: LayoutKind::Flex(Axis::Vertical),
                padding: Edges::all(12.0),
                gap: 6.0,
                ..Default::default()
            },
        )?;
        let staged = (|| {
            self.controls.get_mut(&card).unwrap().popup_surface = true;
            self.edit_props(card, |p| {
                p.visible = false;
                p.clip = true;
            })?;
            self.set_role(card, Role::Group);
            self.set_automation_name(card, format!("{:?}: {}", spec.severity, spec.title))?;
            let top = self.transparent_panel(
                card,
                LayoutStyle {
                    kind: LayoutKind::Flex(Axis::Horizontal),
                    gap: 6.0,
                    ..row(36.0)
                },
            )?;
            let title = self.label(
                top,
                &spec.title,
                LayoutStyle {
                    width: Length::Px(0.0),
                    flex_grow: 1.0,
                    ..row(36.0)
                },
            )?;
            let dismiss = self.button(
                top,
                "",
                None,
                LayoutStyle {
                    width: Length::Px(32.0),
                    ..row(32.0)
                },
            )?;
            self.set_button_icon(dismiss, Some(ButtonIcon::new(IconSymbol::Close)))?;
            self.set_automation_name(dismiss, "Dismiss notification")?;
            let message = self.label(card, &spec.message, row(40.0))?;
            let actions_row = self.transparent_panel(
                card,
                LayoutStyle {
                    kind: LayoutKind::Flex(Axis::Vertical),
                    gap: 4.0,
                    ..row((spec.actions.len() * 36) as f32)
                },
            )?;
            let mut actions = Vec::new();
            self.edit_props(actions_row, |p| p.collapsed = spec.actions.is_empty())?;
            for action in &spec.actions {
                actions.push(self.button(actions_row, &action.label, None, row(32.0))?);
            }
            let mut parents = Vec::new();
            let mut stack = vec![card];
            while let Some(node) = stack.pop() {
                parents.push((node, self.runtime.tree().parent(node).unwrap()));
                stack.extend(
                    self.runtime
                        .tree()
                        .children(node)
                        .unwrap_or(&[])
                        .iter()
                        .copied(),
                );
            }
            Ok::<_, ControlError>((
                NotificationParts {
                    card,
                    title,
                    message,
                    dismiss,
                    actions,
                },
                parents,
            ))
        })();
        let (parts, parents) = match staged {
            Ok(value) => value,
            Err(e) => {
                self.remove(card)?;
                return Err(e);
            }
        };
        let entry = self.notifications.entries.get_mut(&owner).unwrap();
        entry.next = next;
        entry.queue.push_back(Item {
            id,
            spec,
            parts,
            parents,
            deadline: None,
            invoking: false,
        });
        self.scene_dirty = true;
        Ok(id)
    }
    pub fn notification_parts(&self, id: NotificationId) -> Option<&NotificationParts> {
        Some(
            &self
                .notifications
                .entries
                .get(&id.owner)?
                .queue
                .iter()
                .find(|i| i.id == id)?
                .parts,
        )
    }
    pub fn notification_is_pending(&self, id: NotificationId) -> bool {
        self.notification_parts(id).is_some()
    }
    pub fn notification_history(&self, owner: WidgetId) -> Option<&VecDeque<NotificationRecord>> {
        Some(&self.notifications.entries.get(&owner)?.history)
    }
    pub fn notification_message(&self, widget: WidgetId) -> bool {
        self.notifications
            .entries
            .values()
            .any(|e| e.queue.iter().any(|i| i.parts.message == widget))
    }
    pub fn dismiss_notification(&mut self, id: NotificationId) -> Result<bool, ControlError> {
        if !self.notifications.entries.contains_key(&id.owner) {
            return Ok(false);
        }
        self.check_notification_parts(id.owner)?;
        self.finish_notification(id, NotificationCloseReason::Dismissed)
    }
    fn finish_notification(
        &mut self,
        id: NotificationId,
        reason: NotificationCloseReason,
    ) -> Result<bool, ControlError> {
        let Some(e) = self.notifications.entries.get_mut(&id.owner) else {
            return Ok(false);
        };
        let Some(index) = e.queue.iter().position(|i| i.id == id) else {
            return Ok(false);
        };
        let item = e.queue.remove(index).unwrap();
        if e.history.len() == MAX_NOTIFICATION_HISTORY {
            e.history.pop_front();
        }
        e.history.push_back(NotificationRecord {
            id,
            notification: item.spec,
            reason,
        });
        self.remove(item.parts.card)?;
        self.scene_dirty = true;
        Ok(true)
    }
    pub(super) fn sync_notifications(&mut self) -> Result<(), ControlError> {
        self.sync_notifications_at(Instant::now())
    }
    fn sync_notifications_at(&mut self, now: Instant) -> Result<(), ControlError> {
        let owners: Vec<_> = self.notifications.entries.keys().copied().collect();
        for owner in owners {
            self.check_notification_parts(owner)?;
            if !self.effective_enabled(owner) {
                let ids: Vec<_> = self.notifications.entries[&owner]
                    .queue
                    .iter()
                    .map(|i| i.id)
                    .collect();
                for id in ids {
                    self.finish_notification(id, NotificationCloseReason::OwnerUnavailable)?;
                }
            }
            let e = &self.notifications.entries[&owner];
            let layer = e.layer;
            let width = e.settings.width.min((self.viewport.width - 24.0).max(0.0));
            let max_visible = e.settings.max_visible;
            let items: Vec<_> = e
                .queue
                .iter()
                .map(|i| (i.id, i.spec.clone(), i.parts.clone()))
                .collect();
            let mut y = 0.0;
            let mut visible = 0;
            let mut blocked = false;
            let available = (self.viewport.height - 24.0).max(0.0);
            for (id, spec, parts) in items {
                let message_height = self
                    .measure_text(&spec.message, (width - 40.0).max(1.0), true)?
                    .height
                    .max(20.0);
                let title_height = self
                    .measure_text(&spec.title, (width - 78.0).max(1.0), true)?
                    .height
                    .max(32.0);
                let height = 24.0
                    + title_height
                    + 6.0
                    + message_height
                    + if spec.actions.is_empty() {
                        0.0
                    } else {
                        6.0 + (spec.actions.len() * 36) as f32
                    };
                let show =
                    !blocked && width >= 240.0 && visible < max_visible && y + height <= available;
                blocked |= !show;
                self.edit_props(parts.card, |p| {
                    p.visible = show;
                    p.style.width = Length::Px(width);
                    p.style.height = Length::Px(height);
                    p.style.offset = Point::new(0.0, y);
                })?;
                self.edit_props(parts.message, |p| {
                    p.style.height = Length::Px(message_height)
                })?;
                let title_parent = self.runtime.tree().parent(parts.title).unwrap();
                self.edit_props(title_parent, |p| p.style.height = Length::Px(title_height))?;
                self.edit_props(parts.title, |p| p.style.height = Length::Px(title_height))?;
                for (button, action) in parts.actions.iter().zip(&spec.actions) {
                    self.set_enabled(*button, self.commands.enabled(&action.command))?;
                }
                if show {
                    visible += 1;
                    y += height + 8.0;
                    let item = self
                        .notifications
                        .entries
                        .get_mut(&owner)
                        .unwrap()
                        .queue
                        .iter_mut()
                        .find(|i| i.id == id)
                        .unwrap();
                    if item.deadline.is_none() {
                        item.deadline = spec.timeout.and_then(|duration| now.checked_add(duration));
                    }
                }
            }
            let height = (y - 8.0).max(0.0);
            let offset = Point::new(
                (self.viewport.width - width - 12.0).max(0.0),
                (self.viewport.height - height - 12.0).max(0.0),
            );
            self.edit_props(layer, |p| {
                p.visible = visible > 0;
                p.style.width = Length::Px(width);
                p.style.height = Length::Px(height);
                p.style.offset = offset;
            })?;
        }
        Ok(())
    }
    pub fn notification_deadline(&self) -> Option<Instant> {
        self.notifications
            .entries
            .iter()
            .filter(|(owner, _)| self.effective_enabled(**owner))
            .flat_map(|(_, e)| e.queue.iter().filter_map(|i| i.deadline))
            .min()
    }
    pub(super) fn notification_tick(&mut self, now: Instant) -> Result<bool, ControlError> {
        self.sync_notifications_at(now)?;
        let expired: Vec<_> = self
            .notifications
            .entries
            .values()
            .flat_map(|e| e.queue.iter())
            .filter(|i| i.deadline.is_some_and(|d| now >= d))
            .map(|i| i.id)
            .collect();
        let changed = !expired.is_empty();
        for id in expired {
            self.finish_notification(id, NotificationCloseReason::Expired)?;
        }
        if changed {
            self.sync_notifications_at(now)?;
        }
        Ok(changed)
    }
    fn notification_action(&self, button: WidgetId) -> Option<(NotificationId, Option<CommandId>)> {
        self.notifications
            .entries
            .values()
            .flat_map(|e| e.queue.iter())
            .find_map(|i| {
                if i.parts.dismiss == button {
                    Some((i.id, None))
                } else {
                    i.parts
                        .actions
                        .iter()
                        .position(|id| *id == button)
                        .map(|n| (i.id, Some(i.spec.actions[n].command.clone())))
                }
            })
    }
    pub(super) fn activate_notification(&mut self, button: WidgetId) -> Result<bool, ControlError> {
        let Some((id, command)) = self.notification_action(button) else {
            return Ok(false);
        };
        self.check_notification_parts(id.owner)?;
        if !self.effective_enabled(id.owner) || !self.activation_available(button) {
            return Ok(true);
        }
        let item = self
            .notifications
            .entries
            .get_mut(&id.owner)
            .unwrap()
            .queue
            .iter_mut()
            .find(|i| i.id == id)
            .unwrap();
        if item.invoking {
            return Ok(true);
        }
        item.invoking = true;
        if let Some(command) = command {
            if !self.commands.activate(&command) {
                self.notifications
                    .entries
                    .get_mut(&id.owner)
                    .unwrap()
                    .queue
                    .iter_mut()
                    .find(|i| i.id == id)
                    .unwrap()
                    .invoking = false;
                return Ok(true);
            }
            self.finish_notification(id, NotificationCloseReason::Action(command))?;
        } else {
            self.finish_notification(id, NotificationCloseReason::Dismissed)?;
        }
        Ok(true)
    }
    pub(super) fn notification_before_input(
        &mut self,
        event: &InputEvent,
    ) -> Result<bool, ControlError> {
        let pointer = match event {
            InputEvent::PointerDown {
                position,
                button: PointerButton::Primary,
                ..
            } => Some((*position, true)),
            InputEvent::PointerUp {
                position,
                button: PointerButton::Primary,
                ..
            } => Some((*position, false)),
            _ => None,
        };
        let Some((point, down)) = pointer else {
            return Ok(false);
        };
        let hit = self.runtime.hit_test(point);
        let target = if down {
            hit
        } else {
            self.pressed.filter(|_| !self.keyboard_pressed)
        };
        let Some(target) = target.filter(|id| {
            self.notification_action(*id).is_some()
                || (down
                    && self.notifications.entries.values().any(|e| {
                        e.queue
                            .iter()
                            .any(|item| self.flyout_descendant(*id, item.parts.card))
                    }))
        }) else {
            return Ok(false);
        };
        let action = self.notification_action(target).is_some();
        let previous_focus = self.runtime.focused();
        let report = self.runtime.dispatch(event.clone());
        // Runtime pointer defaults focus the hit button. Restore before any
        // editor focus synchronization; explicit focus to another control wins.
        if down && (self.runtime.focused() == Some(target) || self.runtime.focused().is_none()) {
            if let Some(previous) = previous_focus
                .filter(|id| self.runtime.tree().contains(*id) && self.effective_enabled(*id))
            {
                self.runtime.apply(Mutation::Focus(Some(previous)))?;
            } else {
                self.runtime.apply(Mutation::Focus(None))?;
            }
        }
        self.prune_removed_controls()?;
        if let Some(error) = report.errors.into_iter().next() {
            return Err(error.into());
        }
        if down {
            if action && !report.default_prevented && self.activation_available(target) {
                self.pressed = Some(target);
                self.keyboard_pressed = false;
                self.runtime.apply(Mutation::CapturePointer(Some(target)))?;
            }
        } else {
            self.pressed = None;
            self.runtime.apply(Mutation::CapturePointer(None))?;
            if !report.default_prevented && hit == Some(target) {
                self.activate_notification(target)?;
            }
        }
        self.scene_dirty = true;
        Ok(true)
    }
    pub(super) fn preflight_notification_removal(&self, id: WidgetId) -> Result<(), ControlError> {
        if self.notifications.entries.iter().any(|(owner, e)| {
            !self.flyout_descendant(*owner, id)
                && (id == e.layer
                    || e.queue
                        .iter()
                        .any(|item| item.parents.iter().any(|(part, _)| *part == id)))
        }) {
            Err(invalid(
                "Remove the notification center, not its owned card parts",
            ))
        } else {
            Ok(())
        }
    }
    pub(super) fn prune_notifications(&mut self) -> Result<(), ControlError> {
        let stale: Vec<_> = self
            .notifications
            .entries
            .keys()
            .copied()
            .filter(|id| !self.runtime.tree().contains(*id))
            .collect();
        for id in stale {
            let e = self.notifications.entries.remove(&id).unwrap();
            if self.runtime.tree().contains(e.layer) {
                self.remove(e.layer)?;
            }
        }
        Ok(())
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
