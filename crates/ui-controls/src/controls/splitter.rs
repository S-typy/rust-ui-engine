//! Retained two-pane layout with an independently focusable resize divider.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SplitPane {
    First,
    Second,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SplitterSettings {
    /// Horizontal places panes side by side; Vertical places them top to bottom.
    pub axis: Axis,
    /// Requested share of the available pane space. Constraints do not overwrite it.
    pub ratio: f32,
    pub first_min: f32,
    pub second_min: f32,
    pub first_max: f32,
    pub second_max: f32,
    pub divider_width: f32,
    pub collapsed: Option<SplitPane>,
}
impl Default for SplitterSettings {
    fn default() -> Self {
        Self {
            axis: Axis::Horizontal,
            ratio: 0.5,
            first_min: 0.0,
            second_min: 0.0,
            first_max: f32::INFINITY,
            second_max: f32::INFINITY,
            divider_width: 6.0,
            collapsed: None,
        }
    }
}
impl SplitterSettings {
    pub fn validate(&self) -> Result<(), ControlError> {
        if !self.ratio.is_finite()
            || !(0.0..=1.0).contains(&self.ratio)
            || !self.divider_width.is_finite()
            || !(1.0..=64.0).contains(&self.divider_width)
            || [self.first_min, self.second_min]
                .into_iter()
                .any(|v| !v.is_finite() || v < 0.0)
            || self.first_max.is_nan()
            || self.second_max.is_nan()
            || self.first_max < self.first_min
            || self.second_max < self.second_min
        {
            return Err(ControlError::Invalid(
                "Invalid splitter ratio, constraints or divider width".into(),
            ));
        }
        Ok(())
    }
}

/// Required hosts remain live when a pane is collapsed. Remove the container,
/// rather than removing either pane or the divider individually.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SplitContainer {
    pub container: WidgetId,
    pub first: WidgetId,
    pub second: WidgetId,
    pub divider: WidgetId,
}

pub(super) struct SplitterState {
    parts: SplitContainer,
    settings: SplitterSettings,
}
#[derive(Clone, Copy)]
pub(super) struct SplitterDrag {
    owner: WidgetId,
    ratio: f32,
    collapsed: Option<SplitPane>,
    start: f32,
    first: f32,
    available: f32,
}

fn pane_sizes(settings: SplitterSettings, total: f32) -> (f32, f32) {
    match settings.collapsed {
        Some(SplitPane::First) => return (0.0, total),
        Some(SplitPane::Second) => return (total, 0.0),
        None => {}
    }
    let minimum = f64::from(settings.first_min) + f64::from(settings.second_min);
    if f64::from(total) < minimum {
        let first = (f64::from(total) * f64::from(settings.first_min) / minimum) as f32;
        return (first, (total - first).max(0.0));
    }
    let low = settings.first_min.max(total - settings.second_max);
    let high = settings.first_max.min(total - settings.second_min);
    if low > high {
        return (
            settings.first_max.min(total),
            settings.second_max.min(total),
        );
    }
    let first = (total * settings.ratio).clamp(low, high);
    (first, (total - first).max(0.0))
}

impl Controls {
    pub fn split_container(
        &mut self,
        parent: WidgetId,
        settings: SplitterSettings,
        mut style: LayoutStyle,
    ) -> Result<SplitContainer, ControlError> {
        settings.validate()?;
        style.kind = LayoutKind::Flex(settings.axis);
        style.gap = 0.0;
        let container = self.panel(parent, style)?;
        self.set_surface(container, SurfaceKind::Transparent)?;
        let host_style = LayoutStyle {
            kind: LayoutKind::Stack(Axis::Vertical),
            flex_shrink: 0.0,
            ..Default::default()
        };
        let first = self.panel(container, host_style.clone())?;
        let divider = self.panel(
            container,
            LayoutStyle {
                flex_shrink: 0.0,
                ..Default::default()
            },
        )?;
        let second = self.panel(container, host_style)?;
        for host in [first, second] {
            self.set_surface(host, SurfaceKind::Transparent)?;
            self.edit_props(host, |p| p.clip = true)?;
        }
        self.edit_props(container, |p| p.clip = true)?;
        self.edit_props(divider, |p| {
            p.focusable = true;
            p.clip = true;
        })?;
        self.set_role(divider, Role::Splitter);
        self.set_automation_name(divider, "Resize panes")?;
        let parts = SplitContainer {
            container,
            first,
            second,
            divider,
        };
        self.splitters
            .insert(container, SplitterState { parts, settings });
        self.sync_splitters()?;
        Ok(parts)
    }

    pub(super) fn splitter_owner(&self, id: WidgetId) -> Option<WidgetId> {
        self.splitters.get(&id).map(|_| id).or_else(|| {
            self.splitters
                .iter()
                .find_map(|(&owner, s)| (s.parts.divider == id).then_some(owner))
        })
    }
    pub fn splitter_settings(&self, id: WidgetId) -> Option<SplitterSettings> {
        let owner = self.splitter_owner(id)?;
        self.runtime.tree().node(owner)?;
        Some(self.splitters[&owner].settings)
    }
    pub fn split_ratio(&self, id: WidgetId) -> Option<f32> {
        Some(self.splitter_settings(id)?.ratio)
    }
    pub fn set_split_ratio(&mut self, id: WidgetId, ratio: f32) -> Result<(), ControlError> {
        let mut settings = self
            .splitter_settings(id)
            .ok_or_else(|| ControlError::Invalid("Unknown splitter".into()))?;
        settings.ratio = ratio;
        self.configure_splitter(id, settings)
    }
    pub fn set_split_collapsed(
        &mut self,
        id: WidgetId,
        pane: Option<SplitPane>,
    ) -> Result<(), ControlError> {
        let mut settings = self
            .splitter_settings(id)
            .ok_or_else(|| ControlError::Invalid("Unknown splitter".into()))?;
        settings.collapsed = pane;
        self.configure_splitter(id, settings)
    }
    /// Validate all settings before changing layout or ending an active gesture.
    pub fn configure_splitter(
        &mut self,
        id: WidgetId,
        settings: SplitterSettings,
    ) -> Result<(), ControlError> {
        settings.validate()?;
        let owner = self
            .splitter_owner(id)
            .ok_or_else(|| ControlError::Invalid("Unknown splitter".into()))?;
        if self.splitters[&owner].settings == settings {
            return Ok(());
        }
        if self.splitter_drag.is_some_and(|d| d.owner == owner) {
            self.splitter_drag = None;
            self.runtime.apply(Mutation::CapturePointer(None))?;
        }
        self.splitters.get_mut(&owner).unwrap().settings = settings;
        self.sync_splitters()?;
        self.scene_dirty = true;
        Ok(())
    }

    pub(super) fn sync_splitters(&mut self) -> Result<(), ControlError> {
        if self.splitter_drag.is_some_and(|drag| {
            self.splitters.get(&drag.owner).is_none_or(|s| {
                !self.is_enabled(s.parts.divider)
                    || self.runtime.captured() != Some(s.parts.divider)
            })
        }) {
            return self.cancel_splitter_interaction();
        }
        let items: Vec<_> = self
            .splitters
            .values()
            .map(|s| (s.parts, s.settings))
            .collect();
        let tokens = self.theme.tokens();
        for (parts, settings) in items {
            let Some(container) = self.runtime.tree().node(parts.container) else {
                continue;
            };
            let padding = container.props.style.padding;
            let bounds = self
                .runtime
                .layout_bounds(parts.container)
                .unwrap_or(Rect::new(0.0, 0.0, 0.0, 0.0));
            let (length, cross) = match settings.axis {
                Axis::Horizontal => (
                    (bounds.width - padding.left - padding.right).max(0.0),
                    (bounds.height - padding.top - padding.bottom).max(0.0),
                ),
                Axis::Vertical => (
                    (bounds.height - padding.top - padding.bottom).max(0.0),
                    (bounds.width - padding.left - padding.right).max(0.0),
                ),
            };
            let divider = settings.divider_width.min(length);
            let (first, second) = pane_sizes(settings, (length - divider).max(0.0));
            self.edit_props(parts.container, |p| {
                p.style.kind = LayoutKind::Flex(settings.axis);
                p.style.gap = 0.0;
            })?;
            for (id, size, collapsed) in [
                (
                    parts.first,
                    first,
                    settings.collapsed == Some(SplitPane::First),
                ),
                (parts.divider, divider, false),
                (
                    parts.second,
                    second,
                    settings.collapsed == Some(SplitPane::Second),
                ),
            ] {
                if !self.runtime.tree().contains(id) {
                    continue;
                }
                self.edit_props(id, |p| {
                    p.collapsed = collapsed;
                    p.style.flex_shrink = 0.0;
                    match settings.axis {
                        Axis::Horizontal => {
                            p.style.width = Length::Px(size);
                            p.style.height = Length::Px(cross);
                        }
                        Axis::Vertical => {
                            p.style.width = Length::Px(cross);
                            p.style.height = Length::Px(size);
                        }
                    }
                })?;
            }
            let active = self.runtime.focused() == Some(parts.divider)
                || self
                    .splitter_drag
                    .is_some_and(|d| d.owner == parts.container);
            self.edit_props(parts.divider, |p| {
                p.paint.background = Some(if active { tokens.focus } else { tokens.border })
            })?;
        }
        Ok(())
    }
    pub(super) fn preflight_splitter_removal(&self, id: WidgetId) -> Result<(), ControlError> {
        if self
            .splitters
            .values()
            .any(|s| [s.parts.first, s.parts.second, s.parts.divider].contains(&id))
        {
            return Err(ControlError::Invalid(
                "Remove the SplitContainer, not an individual required host".into(),
            ));
        }
        Ok(())
    }
    pub(super) fn retain_splitters(&mut self) -> Result<(), ControlError> {
        self.splitters.retain(|_, s| {
            [
                s.parts.container,
                s.parts.first,
                s.parts.second,
                s.parts.divider,
            ]
            .into_iter()
            .all(|id| self.runtime.tree().contains(id))
        });
        if self
            .splitter_drag
            .is_some_and(|d| !self.splitters.contains_key(&d.owner))
        {
            self.splitter_drag = None;
            self.runtime.apply(Mutation::CapturePointer(None))?;
        }
        Ok(())
    }
    pub(super) fn cancel_splitter_interaction(&mut self) -> Result<(), ControlError> {
        if let Some(drag) = self.splitter_drag.take() {
            if let Some(splitter) = self.splitters.get_mut(&drag.owner) {
                splitter.settings.ratio = drag.ratio;
                splitter.settings.collapsed = drag.collapsed;
            }
            self.runtime.apply(Mutation::CapturePointer(None))?;
            self.sync_splitters()?;
            self.scene_dirty = true;
        }
        Ok(())
    }

    pub(super) fn splitter_before_input(
        &mut self,
        event: &InputEvent,
    ) -> Result<bool, ControlError> {
        if let Some(drag) = self.splitter_drag {
            let live = self.splitters.get(&drag.owner).is_some_and(|s| {
                self.is_enabled(s.parts.divider)
                    && self.runtime.bounds(s.parts.divider).is_some()
                    && self.runtime.captured() == Some(s.parts.divider)
            });
            if !live
                || matches!(
                    event,
                    InputEvent::WindowFocused(false)
                        | InputEvent::KeyDown {
                            key: Key::Escape,
                            ..
                        }
                )
            {
                self.cancel_splitter_interaction()?;
                return Ok(matches!(
                    event,
                    InputEvent::KeyDown {
                        key: Key::Escape,
                        ..
                    }
                ));
            }
            if let InputEvent::PointerMoved { position } = event {
                let settings = self.splitters[&drag.owner].settings;
                let coordinate = match settings.axis {
                    Axis::Horizontal => position.x,
                    Axis::Vertical => position.y,
                };
                if coordinate.is_finite() && drag.available > 0.0 {
                    let ratio = ((f64::from(drag.first) + f64::from(coordinate)
                        - f64::from(drag.start))
                        / f64::from(drag.available))
                    .clamp(0.0, 1.0) as f32;
                    let state = self.splitters.get_mut(&drag.owner).unwrap();
                    state.settings.ratio = ratio;
                    state.settings.collapsed = None;
                    self.sync_splitters()?;
                    self.scene_dirty = true;
                }
                return Ok(true);
            }
            if matches!(
                event,
                InputEvent::PointerUp {
                    button: PointerButton::Primary,
                    ..
                }
            ) {
                self.splitter_drag = None;
                self.runtime.apply(Mutation::CapturePointer(None))?;
                self.scene_dirty = true;
                return Ok(true);
            }
        }
        if let InputEvent::PointerDown {
            position,
            button: PointerButton::Primary,
            ..
        } = event
        {
            let Some(id) = self.runtime.hit_test(*position) else {
                return Ok(false);
            };
            let Some(owner) = self
                .splitter_owner(id)
                .filter(|owner| self.splitters[owner].parts.divider == id)
            else {
                return Ok(false);
            };
            if !self.is_enabled(id) {
                return Ok(false);
            }
            let state = &self.splitters[&owner];
            let parts = state.parts;
            let settings = state.settings;
            let first_bounds = self
                .runtime
                .layout_bounds(parts.first)
                .unwrap_or(Rect::new(0.0, 0.0, 0.0, 0.0));
            let second_bounds = self
                .runtime
                .layout_bounds(parts.second)
                .unwrap_or(Rect::new(0.0, 0.0, 0.0, 0.0));
            let (start, first, second) = match settings.axis {
                Axis::Horizontal => (position.x, first_bounds.width, second_bounds.width),
                Axis::Vertical => (position.y, first_bounds.height, second_bounds.height),
            };
            self.runtime.apply(Mutation::Focus(Some(id)))?;
            self.runtime.apply(Mutation::CapturePointer(Some(id)))?;
            self.splitter_drag = Some(SplitterDrag {
                owner,
                ratio: settings.ratio,
                collapsed: settings.collapsed,
                start,
                first,
                available: first + second,
            });
            self.scene_dirty = true;
            return Ok(true);
        }
        if let InputEvent::KeyDown { key, modifiers, .. } = event {
            let Some(id) = self.runtime.focused() else {
                return Ok(false);
            };
            let Some(owner) = self
                .splitter_owner(id)
                .filter(|owner| self.splitters[owner].parts.divider == id)
            else {
                return Ok(false);
            };
            if !self.is_enabled(id) || modifiers.alt || modifiers.super_key {
                return Ok(false);
            }
            let mut settings = self.splitters[&owner].settings;
            let step = if modifiers.control { 0.1 } else { 0.02 };
            match (settings.axis, key) {
                (Axis::Horizontal, Key::ArrowLeft) | (Axis::Vertical, Key::ArrowUp) => {
                    settings.ratio = (settings.ratio - step).max(0.0);
                    settings.collapsed = None;
                }
                (Axis::Horizontal, Key::ArrowRight) | (Axis::Vertical, Key::ArrowDown) => {
                    settings.ratio = (settings.ratio + step).min(1.0);
                    settings.collapsed = None;
                }
                (_, Key::Home) => {
                    settings.ratio = 0.0;
                    settings.collapsed = None;
                }
                (_, Key::End) => {
                    settings.ratio = 1.0;
                    settings.collapsed = None;
                }
                (_, Key::Enter) => {
                    settings.collapsed = if settings.collapsed.is_some() {
                        None
                    } else if modifiers.shift {
                        Some(SplitPane::Second)
                    } else {
                        Some(SplitPane::First)
                    }
                }
                _ => return Ok(false),
            }
            self.configure_splitter(owner, settings)?;
            return Ok(true);
        }
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn example(settings: SplitterSettings) -> (Controls, SplitContainer, WidgetId) {
        let mut ui = Controls::new(Theme::FluentLight).unwrap();
        let split = ui
            .split_container(
                ui.root(),
                settings,
                LayoutStyle {
                    width: Length::Px(400.0),
                    height: Length::Px(120.0),
                    ..Default::default()
                },
            )
            .unwrap();
        let editor = ui
            .edit_box(split.first, "Editor", "draft", LayoutStyle::default())
            .unwrap();
        ui.update(Size::new(600.0, 300.0)).unwrap();
        (ui, split, editor)
    }
    fn key(ui: &mut Controls, key: Key) {
        ui.dispatch(InputEvent::KeyDown {
            key,
            modifiers: Default::default(),
            repeat: false,
        })
        .unwrap();
    }
    #[test]
    fn ratio_constraints_resize_and_impossible_minimum_keep_ids() {
        let (mut ui, split, editor) = example(SplitterSettings {
            ratio: 0.1,
            first_min: 100.0,
            second_min: 80.0,
            ..Default::default()
        });
        assert_eq!(ui.runtime.layout_bounds(split.first).unwrap().width, 100.0);
        assert_eq!(ui.split_ratio(split.container), Some(0.1));
        ui.edit_props(split.container, |p| p.style.width = Length::Px(100.0))
            .unwrap();
        ui.update(Size::new(600.0, 300.0)).unwrap();
        let first = ui.runtime.layout_bounds(split.first).unwrap().width;
        let second = ui.runtime.layout_bounds(split.second).unwrap().width;
        assert!((first - 94.0 * 100.0 / 180.0).abs() < 0.001);
        assert!((first + second - 94.0).abs() < 0.001);
        ui.edit_props(split.container, |p| p.style.width = Length::Px(800.0))
            .unwrap();
        ui.update(Size::new(900.0, 300.0)).unwrap();
        assert_eq!(ui.runtime.layout_bounds(split.first).unwrap().width, 100.0);
        assert_eq!(ui.document(editor).unwrap().text(), "draft");
        assert_eq!(ui.runtime.tree().parent(editor), Some(split.first));
    }
    #[test]
    fn keyboard_collapse_restore_and_theme_keep_retained_children() {
        let (mut ui, split, editor) = example(SplitterSettings::default());
        ui.perform_action(split.divider, SemanticAction::Focus, None)
            .unwrap();
        key(&mut ui, Key::ArrowRight);
        assert_eq!(ui.split_ratio(split.container), Some(0.52));
        key(&mut ui, Key::Enter);
        ui.update(Size::new(600.0, 300.0)).unwrap();
        assert!(ui.runtime.bounds(editor).is_none());
        assert_eq!(ui.runtime.focused(), Some(split.divider));
        ui.set_theme(Theme::HighContrastDark).unwrap();
        key(&mut ui, Key::Enter);
        ui.update(Size::new(600.0, 300.0)).unwrap();
        assert!(ui.runtime.bounds(editor).is_some());
        assert_eq!(ui.document(editor).unwrap().text(), "draft");
        assert_eq!(ui.split_ratio(split.container), Some(0.52));
        assert!(ui.remove(split.first).is_err());
        ui.remove(split.container).unwrap();
        assert!(!ui.runtime.tree().contains(editor));
    }
    #[test]
    fn pointer_cancel_rolls_back_ratio_and_release_accepts_it() {
        let (mut ui, split, _) = example(SplitterSettings::default());
        let bounds = ui.runtime.bounds(split.divider).unwrap();
        let point = Point::new(bounds.x + 2.0, bounds.y + 10.0);
        let down = InputEvent::PointerDown {
            position: point,
            button: PointerButton::Primary,
            modifiers: Default::default(),
        };
        ui.dispatch(down.clone()).unwrap();
        ui.dispatch(InputEvent::PointerMoved {
            position: Point::new(point.x + 50.0, point.y),
        })
        .unwrap();
        assert!(ui.split_ratio(split.container).unwrap() > 0.6);
        key(&mut ui, Key::Escape);
        assert_eq!(ui.split_ratio(split.container), Some(0.5));
        assert!(ui.runtime.captured().is_none());
        ui.update(Size::new(600.0, 300.0)).unwrap();
        ui.dispatch(down).unwrap();
        ui.dispatch(InputEvent::PointerMoved {
            position: Point::new(point.x + 30.0, point.y),
        })
        .unwrap();
        let ratio = ui.split_ratio(split.container);
        ui.dispatch(InputEvent::PointerUp {
            position: point,
            button: PointerButton::Primary,
            modifiers: Default::default(),
        })
        .unwrap();
        assert_eq!(ui.split_ratio(split.container), ratio);
        assert!(ui.runtime.captured().is_none());
    }
    #[test]
    fn nested_splitter_settles_and_invalid_settings_are_atomic() {
        let (mut ui, outer, _) = example(SplitterSettings::default());
        let inner = ui
            .split_container(
                outer.second,
                SplitterSettings {
                    axis: Axis::Vertical,
                    ..Default::default()
                },
                LayoutStyle {
                    width: Length::Percent(1.0),
                    height: Length::Percent(1.0),
                    ..Default::default()
                },
            )
            .unwrap();
        ui.update(Size::new(600.0, 300.0)).unwrap();
        assert_eq!(ui.runtime.layout_bounds(inner.first).unwrap().width, 197.0);
        assert_eq!(ui.runtime.layout_bounds(inner.first).unwrap().height, 57.0);
        let old = ui.splitter_settings(inner.container).unwrap();
        assert!(
            ui.configure_splitter(
                inner.container,
                SplitterSettings {
                    ratio: f32::NAN,
                    ..old
                }
            )
            .is_err()
        );
        assert_eq!(ui.splitter_settings(inner.container), Some(old));
        ui.edit_props(outer.container, |p| p.style.width = Length::Px(600.0))
            .unwrap();
        ui.update(Size::new(700.0, 300.0)).unwrap();
        assert_eq!(ui.runtime.layout_bounds(inner.first).unwrap().width, 297.0);
        assert_eq!(ui.update(Size::new(700.0, 300.0)).unwrap().layout_passes, 0);
    }
    #[test]
    fn finite_maxima_leave_extra_space_and_disabled_capture_restores_ratio() {
        let (mut ui, split, _) = example(SplitterSettings {
            first_max: 120.0,
            second_max: 100.0,
            ..Default::default()
        });
        assert_eq!(ui.runtime.layout_bounds(split.first).unwrap().width, 120.0);
        assert_eq!(ui.runtime.layout_bounds(split.second).unwrap().width, 100.0);
        let bounds = ui.runtime.bounds(split.divider).unwrap();
        let point = Point::new(bounds.x + 2.0, bounds.y + 10.0);
        ui.dispatch(InputEvent::PointerDown {
            position: point,
            button: PointerButton::Primary,
            modifiers: Default::default(),
        })
        .unwrap();
        ui.dispatch(InputEvent::PointerMoved {
            position: Point::new(point.x + 20.0, point.y),
        })
        .unwrap();
        ui.set_enabled(split.container, false).unwrap();
        ui.update(Size::new(600.0, 300.0)).unwrap();
        assert_eq!(ui.split_ratio(split.container), Some(0.5));
        assert!(ui.runtime.captured().is_none());
    }

    #[test]
    fn one_update_settles_auto_text_nested_split_thumbs_and_flyout_anchor() {
        use rust_desktop_ui_core::{GridPlacement, GridTrack};
        let mut ui = Controls::new(Theme::FluentLight).unwrap();
        let viewport = Size::new(600.0, 400.0);
        let grid = ui
            .panel(
                ui.root(),
                LayoutStyle {
                    kind: LayoutKind::Grid,
                    width: Length::Px(600.0),
                    height: Length::Px(400.0),
                    grid_rows: vec![GridTrack::Auto, GridTrack::Fr(1.0)],
                    grid_columns: vec![GridTrack::Fr(1.0)],
                    ..Default::default()
                },
            )
            .unwrap();
        let title = ui
            .label(grid, "Short title", LayoutStyle::default())
            .unwrap();
        ui.set_text_auto_height(title, true).unwrap();
        let outer = ui
            .split_container(
                grid,
                SplitterSettings::default(),
                LayoutStyle {
                    grid_row: GridPlacement {
                        start: Some(2),
                        ..Default::default()
                    },
                    ..Default::default()
                },
            )
            .unwrap();
        ui.edit_props(outer.first, |props| {
            props.style.kind = LayoutKind::Grid;
            props.style.grid_columns = vec![GridTrack::Fr(1.0)];
            props.style.grid_rows = vec![GridTrack::Fr(1.0)];
        })
        .unwrap();
        let inner = ui
            .split_container(
                outer.first,
                SplitterSettings {
                    axis: Axis::Vertical,
                    ..Default::default()
                },
                LayoutStyle::default(),
            )
            .unwrap();
        ui.edit_props(inner.first, |props| {
            props.style.kind = LayoutKind::Grid;
            props.style.grid_columns = vec![GridTrack::Fr(1.0)];
            props.style.grid_rows = vec![GridTrack::Fr(1.0)];
        })
        .unwrap();
        let range = ui
            .range_slider(
                inner.first,
                RangeSliderSettings::default(),
                LayoutStyle::default(),
            )
            .unwrap();
        ui.edit_props(range, |props| {
            props.style.width = Length::Auto;
            props.style.height = Length::Auto;
        })
        .unwrap();
        let button = ui
            .button(inner.second, "Details", None, LayoutStyle::default())
            .unwrap();
        let flyout_size = Size::new(160.0, 60.0);
        let flyout = ui.flyout(button, flyout_size).unwrap();
        let editor = ui
            .edit_box(flyout.content, "Draft", "retained", LayoutStyle::default())
            .unwrap();
        ui.update(viewport).unwrap();
        let original_height = ui.runtime.bounds(title).unwrap().height;
        ui.open_flyout(flyout).unwrap();
        ui.set_label(
            title,
            "Long wrapped title changes the available split height. ".repeat(9),
        )
        .unwrap();
        ui.update(viewport).unwrap();

        assert!(ui.runtime.bounds(title).unwrap().height > original_height);
        let outer_bounds = ui.runtime.bounds(outer.container).unwrap();
        let inner_bounds = ui.runtime.bounds(inner.container).unwrap();
        assert_eq!(inner_bounds.height, outer_bounds.height);
        let pane = ui.runtime.bounds(inner.first).unwrap();
        assert!((pane.height - (inner_bounds.height - 6.0) * 0.5).abs() < 0.01);
        let (lower, upper) = ui.range_slider_thumbs(range).unwrap();
        for thumb in [lower, upper] {
            let bounds = ui.runtime.bounds(thumb).unwrap();
            assert!((bounds.y + bounds.height * 0.5 - (pane.y + pane.height * 0.5)).abs() < 0.01);
        }
        let anchor = ui
            .runtime
            .bounds(button)
            .unwrap()
            .intersection(ui.runtime.clip(button).unwrap())
            .unwrap();
        let expected = popup::place_overlay(anchor, flyout_size, viewport, false);
        assert_eq!(ui.runtime.bounds(flyout.container), Some(expected));
        assert_eq!(ui.document(editor).unwrap().text(), "retained");
        let stable: Vec<_> = [outer.first, inner.first, lower, upper, flyout.container]
            .into_iter()
            .map(|id| (id, ui.runtime.bounds(id)))
            .collect();
        assert_eq!(ui.update(viewport).unwrap().layout_passes, 0);
        for (id, bounds) in stable {
            assert_eq!(ui.runtime.bounds(id), bounds);
        }
    }

    #[test]
    fn cyclic_intrinsic_width_and_wrapped_height_return_a_bounded_error() {
        let mut ui = Controls::new(Theme::FluentLight).unwrap();
        let row = ui
            .panel(
                ui.root(),
                LayoutStyle {
                    kind: LayoutKind::Stack(Axis::Horizontal),
                    width: Length::Px(600.0),
                    height: Length::Px(300.0),
                    ..Default::default()
                },
            )
            .unwrap();
        let label = ui
            .label(
                row,
                "Wrapping text determines the next height",
                LayoutStyle {
                    height: Length::Px(20.0),
                    ..Default::default()
                },
            )
            .unwrap();
        ui.set_text_auto_height(label, true).unwrap();
        ui.set_measure(label, |input| {
            Ok(Size::new(
                if input.height.unwrap_or(0.0) > 60.0 {
                    500.0
                } else {
                    40.0
                },
                20.0,
            ))
        })
        .unwrap();
        let error = ui.update(Size::new(600.0, 300.0)).unwrap_err();
        assert!(
            matches!(error, ControlError::Runtime(RuntimeError::InvalidLayout(message))
            if message.contains("did not converge"))
        );
        // Removing the circular size rule recovers through the ordinary API.
        ui.remove_measure(label).unwrap();
        ui.edit_props(label, |props| props.style.width = Length::Px(200.0))
            .unwrap();
        ui.update(Size::new(600.0, 300.0)).unwrap();
    }
}
