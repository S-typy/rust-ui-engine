//! Opt-in bounded, event-loop-driven repeat policy for command buttons.
use super::*;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RepeatButtonSettings {
    pub delay: Duration,
    pub interval: Duration,
}
impl Default for RepeatButtonSettings {
    fn default() -> Self {
        Self {
            delay: Duration::from_millis(400),
            interval: Duration::from_millis(80),
        }
    }
}
#[derive(Clone, Copy)]
struct HeldRepeat {
    id: WidgetId,
    deadline: Instant,
}
#[derive(Default)]
pub(super) struct RepeatState {
    settings: HashMap<WidgetId, RepeatButtonSettings>,
    held: Option<HeldRepeat>,
    consumed_release: Option<WidgetId>,
}
impl Controls {
    /// Enables timer repetition while pointer/Space is held over an available
    /// plain button. Enter and semantic activation remain single invocations.
    pub fn set_repeat_button(
        &mut self,
        id: WidgetId,
        settings: Option<RepeatButtonSettings>,
    ) -> Result<(), ControlError> {
        if !self.runtime.tree().contains(id)
            || self.flyout_expanded(id).is_some()
            || !self.controls.get(&id).is_some_and(|c| {
                matches!(c.kind, Kind::Button) && c.menu.is_none() && c.dropdown_target.is_none()
            })
        {
            return Err(ControlError::Invalid(
                "Repeat requires a plain live Button".into(),
            ));
        }
        if let Some(value) = settings {
            if !(Duration::from_millis(1)..=Duration::from_secs(60)).contains(&value.delay)
                || !(Duration::from_millis(16)..=Duration::from_secs(60)).contains(&value.interval)
            {
                return Err(ControlError::Invalid(
                    "Repeat delay must be 1..60000ms and interval 16..60000ms".into(),
                ));
            }
            if !self.repeats.settings.contains_key(&id) && self.repeats.settings.len() >= 4096 {
                return Err(ControlError::Invalid(
                    "At most 4096 repeat buttons are supported".into(),
                ));
            }
            self.repeats.settings.insert(id, value);
        } else {
            self.repeats.settings.remove(&id);
        }
        if self.repeats.held.is_some_and(|held| held.id == id) {
            self.repeats.held = None;
        }
        Ok(())
    }
    pub fn repeat_button(&self, id: WidgetId) -> Option<RepeatButtonSettings> {
        self.runtime.tree().node(id)?;
        self.repeats.settings.get(&id).copied()
    }
    /// A host should combine this deadline with other one-shot UI deadlines.
    pub fn repeat_deadline(&self) -> Option<Instant> {
        let held = self.repeats.held?;
        self.repeat_eligible(held.id).then_some(held.deadline)
    }
    /// Executes at most one repetition per tick. Missed intervals are discarded
    /// rather than replayed in a burst. A release after a timer activation does
    /// not add another invocation.
    pub fn repeat_tick(&mut self, now: Instant) -> Result<bool, ControlError> {
        let Some(mut held) = self.repeats.held else {
            return Ok(false);
        };
        if !self.repeat_eligible(held.id) {
            self.repeats.held = None;
            return Ok(false);
        }
        if now < held.deadline {
            return Ok(false);
        }
        let settings = self.repeats.settings[&held.id];
        held.deadline = now + settings.interval;
        self.repeats.held = Some(held);
        self.repeats.consumed_release = Some(held.id);
        self.activate(held.id)?;
        Ok(true)
    }
    fn repeat_eligible(&self, id: WidgetId) -> bool {
        self.window_focused
            && self.pressed == Some(id)
            && self.is_enabled(id)
            && self.repeats.settings.contains_key(&id)
            && if self.keyboard_pressed {
                self.runtime.focused() == Some(id)
            } else {
                self.runtime.captured() == Some(id) && self.runtime.hovered() == Some(id)
            }
    }
    pub(super) fn repeat_before_input(&mut self, event: &InputEvent) {
        let release = matches!(
            event,
            InputEvent::PointerUp {
                button: PointerButton::Primary,
                ..
            }
        ) && !self.keyboard_pressed
            || matches!(
                event,
                InputEvent::KeyUp {
                    key: Key::Space,
                    ..
                }
            ) && self.keyboard_pressed;
        if release {
            if self.repeats.consumed_release.is_some()
                && self.repeats.consumed_release == self.pressed
            {
                self.pressed = None;
            }
            self.repeats.held = None;
            self.repeats.consumed_release = None;
        } else if matches!(
            event,
            InputEvent::WindowFocused(false)
                | InputEvent::PointerLeft
                | InputEvent::KeyDown {
                    key: Key::Escape,
                    ..
                }
        ) {
            self.repeats.held = None;
        } else if matches!(
            event,
            InputEvent::PointerDown {
                button: PointerButton::Primary,
                ..
            } | InputEvent::KeyDown {
                key: Key::Space,
                repeat: false,
                ..
            }
        ) {
            self.repeats.consumed_release = None;
        }
    }
    pub(super) fn repeat_after_input(&mut self, _event: &InputEvent) {
        let candidate = self.pressed.filter(|id| self.repeat_eligible(*id));
        if candidate == self.repeats.held.map(|held| held.id) {
            return;
        }
        self.repeats.held = candidate.map(|id| HeldRepeat {
            id,
            deadline: Instant::now() + self.repeats.settings[&id].delay,
        });
    }
    pub(super) fn prune_repeat(&mut self) {
        self.repeats
            .settings
            .retain(|id, _| self.runtime.tree().contains(*id));
        if self
            .repeats
            .held
            .is_some_and(|held| !self.repeat_eligible(held.id))
        {
            self.repeats.held = None;
        }
        if self.repeats.consumed_release != self.pressed {
            self.repeats.consumed_release = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_desktop_ui_core::Modifiers;
    #[test]
    fn held_space_repeats_without_release_duplicate_and_cancel_stops_deadline() {
        let mut ui = Controls::new(Theme::FluentLight).unwrap();
        ui.commands_mut().register("run", "Run");
        let id = ui
            .button(ui.root(), "Run", Some("run".into()), LayoutStyle::default())
            .unwrap();
        ui.set_repeat_button(id, Some(RepeatButtonSettings::default()))
            .unwrap();
        ui.update(Size::new(400.0, 300.0)).unwrap();
        ui.runtime.apply(Mutation::Focus(Some(id))).unwrap();
        ui.dispatch(InputEvent::KeyDown {
            key: Key::Space,
            modifiers: Modifiers::default(),
            repeat: false,
        })
        .unwrap();
        let deadline = ui.repeat_deadline().unwrap();
        assert!(!ui.repeat_tick(deadline - Duration::from_millis(1)).unwrap());
        assert!(ui.repeat_tick(deadline).unwrap());
        assert!(ui.repeat_tick(deadline + Duration::from_secs(20)).unwrap());
        ui.dispatch(InputEvent::KeyUp {
            key: Key::Space,
            modifiers: Modifiers::default(),
        })
        .unwrap();
        assert_eq!(ui.drain_activated().len(), 2);
        assert!(ui.repeat_deadline().is_none());
        ui.dispatch(InputEvent::KeyDown {
            key: Key::Space,
            modifiers: Modifiers::default(),
            repeat: false,
        })
        .unwrap();
        ui.dispatch(InputEvent::WindowFocused(false)).unwrap();
        assert!(ui.repeat_deadline().is_none());
    }

    #[test]
    fn unrelated_release_preserves_pointer_or_space_hold_and_real_release_activates_once() {
        for pointer in [false, true] {
            for repeated in [false, true] {
                let mut ui = Controls::new(Theme::FluentLight).unwrap();
                let id = ui
                    .button(
                        ui.root(),
                        "Run",
                        None,
                        LayoutStyle {
                            width: Length::Px(140.0),
                            height: Length::Px(40.0),
                            ..Default::default()
                        },
                    )
                    .unwrap();
                ui.set_repeat_button(id, Some(RepeatButtonSettings::default()))
                    .unwrap();
                ui.update(Size::new(400.0, 300.0)).unwrap();
                ui.runtime.apply(Mutation::Focus(Some(id))).unwrap();
                let bounds = ui.runtime.bounds(id).unwrap();
                let position = Point::new(bounds.x + 8.0, bounds.y + 8.0);
                let key_release = InputEvent::KeyUp {
                    key: Key::Space,
                    modifiers: Modifiers::default(),
                };
                let pointer_release = InputEvent::PointerUp {
                    position,
                    button: PointerButton::Primary,
                    modifiers: Modifiers::default(),
                };
                if pointer {
                    ui.dispatch(InputEvent::PointerMoved { position }).unwrap();
                    ui.dispatch(InputEvent::PointerDown {
                        position,
                        button: PointerButton::Primary,
                        modifiers: Modifiers::default(),
                    })
                    .unwrap();
                } else {
                    ui.dispatch(InputEvent::KeyDown {
                        key: Key::Space,
                        modifiers: Modifiers::default(),
                        repeat: false,
                    })
                    .unwrap();
                }
                let deadline = ui.repeat_deadline().unwrap();
                if repeated {
                    assert!(ui.repeat_tick(deadline).unwrap());
                }
                let next_deadline = ui.repeat_deadline();
                ui.dispatch(if pointer {
                    key_release.clone()
                } else {
                    pointer_release.clone()
                })
                .unwrap();
                assert_eq!(ui.pressed, Some(id));
                assert_eq!(ui.repeat_deadline(), next_deadline);
                ui.dispatch(if pointer {
                    pointer_release
                } else {
                    key_release
                })
                .unwrap();
                assert_eq!(ui.drain_activated(), vec![id]);
                assert!(ui.repeat_deadline().is_none());
            }
        }
    }
}
