//! Host-driven progress animation; no background timer or hidden-window work.
use super::*;
use std::time::{Duration, Instant};

#[derive(Default)]
pub(super) struct ProgressAnimation {
    active: HashSet<WidgetId>,
    reduced_motion: bool,
    last_frame: Option<Instant>,
    phase: f32,
}

impl Controls {
    /// Indeterminate mode preserves the stored numeric value for a later return to determinate mode.
    pub fn set_progress_indeterminate(
        &mut self,
        id: WidgetId,
        indeterminate: bool,
    ) -> Result<(), ControlError> {
        if self.progress_settings(id).is_none() {
            return Err(ControlError::Invalid("Unknown ProgressBar".into()));
        }
        if indeterminate {
            self.progress_animation.active.insert(id);
        } else {
            self.progress_animation.active.remove(&id);
        }
        self.scene_dirty = true;
        Ok(())
    }

    pub fn progress_indeterminate(&self, id: WidgetId) -> Option<bool> {
        self.progress_settings(id)?;
        Some(self.progress_animation.active.contains(&id))
    }

    /// Reduced motion uses a stationary busy segment and stops animation deadlines.
    pub fn set_reduced_motion(&mut self, reduced: bool) {
        if self.progress_animation.reduced_motion != reduced {
            self.progress_animation.reduced_motion = reduced;
            self.progress_animation.last_frame = None;
            self.scene_dirty = true;
        }
    }

    pub fn reduced_motion(&self) -> bool {
        self.progress_animation.reduced_motion
    }

    pub fn busy_indicator(
        &mut self,
        parent: WidgetId,
        label: impl Into<String>,
        style: LayoutStyle,
    ) -> Result<WidgetId, ControlError> {
        let label = label.into();
        TextLimits::default().validate(&label).map_err(edit_error)?;
        let id = self.progress_bar(parent, ProgressBarSettings::default(), style)?;
        self.set_automation_name(id, label)?;
        self.set_progress_indeterminate(id, true)?;
        Ok(id)
    }

    fn progress_animating(&self) -> bool {
        self.window_focused
            && !self.reduced_motion()
            && self.progress_animation.active.iter().any(|id| {
                self.is_enabled(*id)
                    && self.runtime.bounds(*id).is_some_and(|bounds| {
                        self.runtime
                            .clip(*id)
                            .and_then(|clip| clip.intersection(bounds))
                            .is_some_and(|rect| rect.width > 0.0 && rect.height > 0.0)
                    })
            })
    }

    pub fn progress_deadline(&self) -> Option<Instant> {
        self.progress_animating().then(|| {
            self.progress_animation
                .last_frame
                .unwrap_or_else(Instant::now)
                + Duration::from_millis(16)
        })
    }

    /// Advance once at the host clock. Large gaps are bounded to prevent catch-up work.
    pub fn progress_tick(&mut self, now: Instant) -> bool {
        if !self.progress_animating() {
            self.progress_animation.last_frame = None;
            return false;
        }
        let elapsed = self
            .progress_animation
            .last_frame
            .map_or(Duration::from_millis(16), |last| {
                now.saturating_duration_since(last)
            });
        if elapsed < Duration::from_millis(16) {
            return false;
        }
        self.progress_animation.phase =
            (self.progress_animation.phase + elapsed.as_secs_f32().min(0.1) / 1.4) % 1.0;
        self.progress_animation.last_frame = Some(now);
        self.scene_dirty = true;
        true
    }

    pub(super) fn retain_progress_animations(&mut self) {
        self.progress_animation
            .active
            .retain(|id| self.runtime.tree().node(*id).is_some());
    }

    pub(super) fn paint_busy_progress(&mut self, id: WidgetId, bounds: Rect, clip: Rect) -> bool {
        if !self.progress_animation.active.contains(&id) {
            return false;
        }
        let Some(clip) = bounds.intersection(clip) else {
            return true;
        };
        let height = 4.0_f32.min(bounds.height);
        let track = Rect::new(
            bounds.x,
            bounds.y + (bounds.height - height) / 2.0,
            bounds.width,
            height,
        );
        editor_fill(
            &mut self.scene,
            track,
            clip,
            height / 2.0,
            self.theme.chrome().divider,
        );
        let phase = if self.reduced_motion() {
            0.5
        } else {
            self.progress_animation.phase
        };
        let left = (phase * 1.5 - 0.25) * track.width;
        let busy = Rect::new(track.x + left, track.y, track.width * 0.3, track.height);
        let color = if self.is_enabled(id) {
            self.theme.tokens().focus
        } else {
            self.theme.chrome().disabled_text
        };
        editor_fill(&mut self.scene, busy, clip, height / 2.0, color);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn busy_value_and_deadlines_follow_visibility_and_motion() {
        let mut ui = Controls::new(Theme::FluentLight).unwrap();
        let id = ui
            .busy_indicator(
                ui.root(),
                "Working",
                LayoutStyle {
                    width: Length::Px(200.0),
                    height: Length::Px(24.0),
                    ..Default::default()
                },
            )
            .unwrap();
        ui.set_progress_value(id, 37.0).unwrap();
        ui.update(Size::new(300.0, 100.0)).unwrap();
        assert_eq!(ui.progress_value(id), Some(37.0));
        assert!(ui.progress_deadline().is_some());
        let now = Instant::now();
        assert!(ui.progress_tick(now));
        assert!(!ui.progress_tick(now));
        ui.set_reduced_motion(true);
        assert!(ui.progress_deadline().is_none());
        assert!(!ui.progress_tick(now + Duration::from_secs(1)));
        ui.set_reduced_motion(false);
        ui.edit_props(id, |props| props.collapsed = true).unwrap();
        ui.update(Size::new(300.0, 100.0)).unwrap();
        assert!(ui.progress_deadline().is_none());
        ui.set_progress_indeterminate(id, false).unwrap();
        assert_eq!(ui.progress_value(id), Some(37.0));
    }
}
