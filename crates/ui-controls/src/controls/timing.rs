//! One host clock for caret, hover, held-button repeat and visual transitions.
use super::*;
use std::time::{Duration, Instant};

impl Controls {
    pub(super) fn sync_caret_clock(&mut self) {
        let owner = self.runtime.focused().filter(|id| {
            self.window_focused && (self.document(*id).is_some() || self.is_password_box(*id))
        });
        if self.caret_clock.map(|(id, _)| id) != owner {
            self.caret_clock = owner.map(|id| (id, Instant::now() + Duration::from_millis(500)));
        }
    }

    /// Earliest deadline for this retained view. The host may sleep when None.
    pub fn next_deadline(&self) -> Option<Instant> {
        [
            self.caret_clock.map(|(_, deadline)| deadline),
            self.tooltip_deadline(),
            self.repeat_deadline(),
            self.progress_deadline(),
            self.expander_deadline(),
            self.password_deadline(),
        ]
        .into_iter()
        .flatten()
        .min()
    }

    /// Advances independent timers. Frequent animation ticks do not accelerate the caret.
    pub fn tick(&mut self, now: Instant) -> Result<bool, ControlError> {
        self.sync_caret_clock();
        let mut changed = false;
        if let Some((id, deadline)) = self.caret_clock
            && now >= deadline
        {
            changed |= self.blink_caret();
            self.caret_clock = Some((id, now + Duration::from_millis(500)));
        }
        changed |= self.tooltip_tick(now)?;
        changed |= self.repeat_tick(now)?;
        changed |= self.progress_tick(now);
        changed |= self.expander_tick(now)?;
        changed |= self.password_tick(now)?;
        Ok(changed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn animation_ticks_do_not_accelerate_caret_and_blur_stops_all_deadlines() {
        let mut ui = Controls::new(Theme::FluentLight).unwrap();
        let editor = ui
            .edit_box(
                ui.root(),
                "Editor",
                "draft",
                LayoutStyle {
                    height: Length::Px(36.0),
                    ..Default::default()
                },
            )
            .unwrap();
        ui.busy_indicator(
            ui.root(),
            "Working",
            LayoutStyle {
                height: Length::Px(20.0),
                ..Default::default()
            },
        )
        .unwrap();
        ui.runtime.apply(Mutation::Focus(Some(editor))).unwrap();
        ui.update(Size::new(320.0, 140.0)).unwrap();
        let deadline = ui.caret_clock.unwrap().1;
        let initially_visible = ui.caret_visible;
        for milliseconds in (16..500).step_by(16) {
            ui.tick(deadline - Duration::from_millis(500 - milliseconds))
                .unwrap();
            assert_eq!(ui.caret_visible, initially_visible);
        }
        ui.tick(deadline).unwrap();
        assert_ne!(ui.caret_visible, initially_visible);
        ui.tick(deadline + Duration::from_millis(16)).unwrap();
        assert_ne!(ui.caret_visible, initially_visible);
        ui.dispatch(InputEvent::WindowFocused(false)).unwrap();
        ui.tick(deadline + Duration::from_secs(2)).unwrap();
        assert!(ui.next_deadline().is_none());
    }
}
