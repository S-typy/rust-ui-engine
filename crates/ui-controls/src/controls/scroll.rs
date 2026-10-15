//! Two-axis scrolling with overlay scroll bars and retained pointer capture.
use super::*;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ScrollBarVisibility {
    #[default]
    Auto,
    /// Scrolling remains enabled, but the bar is never painted or hit tested.
    Hidden,
    /// No scrolling on this axis; content fills the viewport dimension.
    Disabled,
    /// The bar is visible even when its thumb covers the complete track.
    Visible,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ScrollSettings {
    pub horizontal: ScrollBarVisibility,
    pub vertical: ScrollBarVisibility,
}

impl Default for ScrollSettings {
    fn default() -> Self {
        Self {
            horizontal: ScrollBarVisibility::Disabled,
            vertical: ScrollBarVisibility::Auto,
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct ScrollDrag {
    id: WidgetId,
    horizontal: bool,
    grab: f32,
}

#[derive(Clone, Copy)]
struct ScrollBar {
    track: Rect,
    thumb: Rect,
    horizontal: bool,
    maximum: f32,
}

impl Controls {
    pub(super) fn scroll_wheel_from(
        &mut self,
        id: WidgetId,
        delta: Point,
    ) -> Result<(), ControlError> {
        if !delta.is_finite() {
            return Ok(());
        }
        let mut next = self.scroll_ancestor(id);
        while let Some(id) = next {
            if self.is_enabled(id) {
                let current = self.scroll_offset(id).unwrap_or(Point::ZERO);
                // Widen the sum so finite f32 wheel deltas cannot overflow.
                let add = |a: f32, b: f32| {
                    (f64::from(a) + f64::from(b)).clamp(-f64::from(f32::MAX), f64::from(f32::MAX))
                        as f32
                };
                self.scroll_to_point(
                    id,
                    Point::new(add(current.x, delta.x), add(current.y, delta.y)),
                )?;
                if self.scroll_offset(id) != Some(current) {
                    break;
                }
            }
            next = self
                .runtime
                .tree()
                .parent(id)
                .and_then(|parent| self.scroll_ancestor(parent));
        }
        Ok(())
    }

    /// Current logical offset of a live scroll viewer.
    pub fn scroll_offset(&self, id: WidgetId) -> Option<Point> {
        if !matches!(self.controls.get(&id)?.kind, Kind::Scroll { .. }) {
            return None;
        }
        Some(self.runtime.tree().node(id)?.props.scroll)
    }

    /// Select enabled axes and overlay-bar visibility without replacing content.
    pub fn configure_scroll_view(
        &mut self,
        id: WidgetId,
        settings: ScrollSettings,
    ) -> Result<(), ControlError> {
        let Some(Control {
            kind:
                Kind::Scroll {
                    extent,
                    settings: old,
                    ..
                },
            ..
        }) = self.controls.get_mut(&id)
        else {
            return Err(ControlError::Invalid("Unknown scroll viewer".into()));
        };
        if *old == settings {
            return Ok(());
        }
        *old = settings;
        let extent = *extent;
        self.set_scroll_extent_size(id, extent)?;
        self.scroll_to_point(id, self.scroll_offset(id).unwrap_or(Point::ZERO))?;
        self.scene_dirty = true;
        Ok(())
    }

    /// Set the content's measured extent. Disabled axes still fill the viewport.
    pub fn set_scroll_extent_size(&mut self, id: WidgetId, size: Size) -> Result<(), ControlError> {
        if !size.is_valid() {
            return Err(ControlError::Invalid("Invalid scroll extent".into()));
        }
        let Some(Control {
            kind:
                Kind::Scroll {
                    content,
                    extent,
                    settings,
                },
            ..
        }) = self.controls.get_mut(&id)
        else {
            return Err(ControlError::Invalid("Unknown scroll viewer".into()));
        };
        let extent_changed = *extent != size;
        *extent = size;
        let (content, settings) = (*content, *settings);
        let width = if settings.horizontal == ScrollBarVisibility::Disabled {
            Length::Percent(1.0)
        } else {
            Length::Px(size.width)
        };
        let height = if settings.vertical == ScrollBarVisibility::Disabled {
            Length::Percent(1.0)
        } else {
            Length::Px(size.height)
        };
        if !extent_changed
            && self.runtime.tree().node(content).is_some_and(|node| {
                node.props.style.width == width && node.props.style.height == height
            })
        {
            return Ok(());
        }
        self.edit_props(content, |props| {
            props.style.width = width;
            props.style.height = height;
        })?;
        self.scene_dirty = true;
        Ok(())
    }

    /// Publish a measured overflow extent without changing the retained content
    /// size. Layout adapters use this when content follows viewport width and
    /// overflowing descendants determine the scrollable area.
    pub fn set_scroll_measured_extent(
        &mut self,
        id: WidgetId,
        size: Size,
    ) -> Result<(), ControlError> {
        if !size.is_valid() {
            return Err(ControlError::Invalid("Invalid scroll extent".into()));
        }
        let Some(Control {
            kind: Kind::Scroll { extent, .. },
            ..
        }) = self.controls.get_mut(&id)
        else {
            return Err(ControlError::Invalid("Unknown scroll viewer".into()));
        };
        if *extent != size {
            *extent = size;
            self.scene_dirty = true;
        }
        self.scroll_to_point(id, self.scroll_offset(id).unwrap_or(Point::ZERO))
    }

    /// Scroll both axes atomically, clamping to current viewport and enabled axes.
    pub fn scroll_to_point(&mut self, id: WidgetId, desired: Point) -> Result<(), ControlError> {
        if !desired.is_finite() {
            return Err(ControlError::Invalid("Scroll must be finite".into()));
        }
        let Some(Control {
            kind: Kind::Scroll {
                extent, settings, ..
            },
            ..
        }) = self.controls.get(&id)
        else {
            return Ok(());
        };
        let viewport = self
            .runtime
            .bounds(id)
            .unwrap_or(Rect::new(0.0, 0.0, 0.0, 0.0));
        let clamp = |value: f32, extent: f32, viewport: f32, setting| {
            if setting == ScrollBarVisibility::Disabled {
                0.0
            } else {
                value.clamp(0.0, (extent - viewport).max(0.0))
            }
        };
        let next = Point::new(
            clamp(desired.x, extent.width, viewport.width, settings.horizontal),
            clamp(desired.y, extent.height, viewport.height, settings.vertical),
        );
        self.edit_props(id, |props| props.scroll = next)
    }

    pub(super) fn scroll_key(&mut self, id: WidgetId, key: Key) -> Result<(), ControlError> {
        let Some(current) = self.scroll_offset(id) else {
            return Ok(());
        };
        let page = self.runtime.bounds(id).map_or(0.0, |r| r.height * 0.8);
        let desired = match key {
            Key::ArrowDown => Point::new(current.x, current.y + 40.0),
            Key::ArrowUp => Point::new(current.x, current.y - 40.0),
            Key::ArrowRight => Point::new(current.x + 40.0, current.y),
            Key::ArrowLeft => Point::new(current.x - 40.0, current.y),
            Key::Home => Point::ZERO,
            Key::End => Point::new(f32::MAX, f32::MAX),
            Key::PageDown => Point::new(current.x, current.y + page),
            Key::PageUp => Point::new(current.x, current.y - page),
            _ => return Ok(()),
        };
        self.scroll_to_point(id, desired)
    }

    fn scroll_bars(&self, id: WidgetId) -> Vec<ScrollBar> {
        let Some(Control {
            kind: Kind::Scroll {
                extent, settings, ..
            },
            ..
        }) = self.controls.get(&id)
        else {
            return Vec::new();
        };
        let Some(bounds) = self.runtime.bounds(id) else {
            return Vec::new();
        };
        let Some(clip) = self.runtime.clip(id) else {
            return Vec::new();
        };
        if bounds.intersection(clip).is_none() || bounds.width < 12.0 || bounds.height < 12.0 {
            return Vec::new();
        }
        let offset = self.scroll_offset(id).unwrap_or(Point::ZERO);
        let visible = |mode, overflows| {
            mode == ScrollBarVisibility::Visible || mode == ScrollBarVisibility::Auto && overflows
        };
        let horizontal = visible(settings.horizontal, extent.width > bounds.width);
        let vertical = visible(settings.vertical, extent.height > bounds.height);
        let mut result = Vec::new();
        for is_horizontal in [false, true] {
            if !(if is_horizontal { horizontal } else { vertical }) {
                continue;
            }
            let track = if is_horizontal {
                Rect::new(
                    bounds.x + 2.0,
                    bounds.y + bounds.height - 10.0,
                    (bounds.width - 4.0 - if vertical { 10.0 } else { 0.0 }).max(0.0),
                    8.0,
                )
            } else {
                Rect::new(
                    bounds.x + bounds.width - 10.0,
                    bounds.y + 2.0,
                    8.0,
                    (bounds.height - 4.0 - if horizontal { 10.0 } else { 0.0 }).max(0.0),
                )
            };
            let (viewport, total, current, length) = if is_horizontal {
                (bounds.width, extent.width, offset.x, track.width)
            } else {
                (bounds.height, extent.height, offset.y, track.height)
            };
            let maximum = (total - viewport).max(0.0);
            let thumb_length = if maximum == 0.0 {
                length
            } else {
                (length * viewport / total).max(20.0).min(length)
            };
            let start = if maximum == 0.0 {
                0.0
            } else {
                (length - thumb_length) * (current / maximum)
            };
            let thumb = if is_horizontal {
                Rect::new(track.x + start, track.y + 1.0, thumb_length, 6.0)
            } else {
                Rect::new(track.x + 1.0, track.y + start, 6.0, thumb_length)
            };
            result.push(ScrollBar {
                track,
                thumb,
                horizontal: is_horizontal,
                maximum,
            });
        }
        result
    }

    pub(super) fn paint_scroll_bars(&mut self, id: WidgetId) {
        let Some(clip) = self.runtime.clip(id) else {
            return;
        };
        let enabled = self.is_enabled(id);
        for bar in self.scroll_bars(id) {
            editor_fill(
                &mut self.scene,
                bar.track,
                clip,
                4.0,
                self.theme.chrome().subtle_hover,
            );
            editor_fill(
                &mut self.scene,
                bar.thumb,
                clip,
                3.0,
                if enabled {
                    self.theme.tokens().muted
                } else {
                    self.theme.tokens().border
                },
            );
        }
    }

    fn scroll_bar_at(&self, point: Point) -> Option<(WidgetId, ScrollBar)> {
        // Start from the topmost runtime hit. A popup or overlapping sibling must
        // prevent a bar underneath from intercepting the pointer.
        let mut id = self.runtime.hit_test(point)?;
        loop {
            if self.is_enabled(id)
                && self
                    .runtime
                    .clip(id)
                    .is_some_and(|clip| clip.contains(point.x, point.y))
                && let Some(bar) = self
                    .scroll_bars(id)
                    .into_iter()
                    .find(|bar| bar.track.contains(point.x, point.y))
            {
                return Some((id, bar));
            }
            id = self.runtime.tree().parent(id)?;
        }
    }

    fn scroll_drag_to(&mut self, drag: ScrollDrag, point: Point) -> Result<(), ControlError> {
        let Some(bar) = self
            .scroll_bars(drag.id)
            .into_iter()
            .find(|bar| bar.horizontal == drag.horizontal)
        else {
            return Ok(());
        };
        let (position, start, track, thumb) = if bar.horizontal {
            (point.x, bar.track.x, bar.track.width, bar.thumb.width)
        } else {
            (point.y, bar.track.y, bar.track.height, bar.thumb.height)
        };
        let fraction = if track > thumb {
            ((position - start - drag.grab) / (track - thumb)).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let mut desired = self.scroll_offset(drag.id).unwrap_or(Point::ZERO);
        if bar.horizontal {
            desired.x = fraction * bar.maximum;
        } else {
            desired.y = fraction * bar.maximum;
        }
        self.scroll_to_point(drag.id, desired)
    }

    pub(super) fn scroll_before_input(&mut self, event: &InputEvent) -> Result<bool, ControlError> {
        if self
            .scroll_drag
            .is_some_and(|drag| !self.is_enabled(drag.id) || self.scroll_offset(drag.id).is_none())
        {
            self.scroll_drag = None;
            self.runtime.apply(Mutation::CapturePointer(None))?;
        }
        if let Some(drag) = self.scroll_drag {
            match event {
                InputEvent::PointerMoved { position } => {
                    self.scroll_drag_to(drag, *position)?;
                    return Ok(true);
                }
                InputEvent::PointerUp {
                    button: PointerButton::Primary,
                    ..
                } => {
                    self.scroll_drag = None;
                    self.runtime.apply(Mutation::CapturePointer(None))?;
                    self.scene_dirty = true;
                    return Ok(true);
                }
                InputEvent::KeyDown {
                    key: Key::Escape, ..
                }
                | InputEvent::WindowFocused(false) => {
                    self.scroll_drag = None;
                    self.runtime.apply(Mutation::CapturePointer(None))?;
                }
                _ => {}
            }
        }
        if let InputEvent::PointerDown {
            position,
            button: PointerButton::Primary,
            ..
        } = event
            && let Some((id, bar)) = self.scroll_bar_at(*position)
        {
            self.popup_pointer_down(*position)?;
            self.runtime.apply(Mutation::Focus(Some(id)))?;
            self.synchronize_editor_focus()?;
            let grab = if bar.thumb.contains(position.x, position.y) {
                if bar.horizontal {
                    position.x - bar.thumb.x
                } else {
                    position.y - bar.thumb.y
                }
            } else if bar.horizontal {
                bar.thumb.width * 0.5
            } else {
                bar.thumb.height * 0.5
            };
            let drag = ScrollDrag {
                id,
                horizontal: bar.horizontal,
                grab,
            };
            self.scroll_drag = Some(drag);
            self.runtime.apply(Mutation::CapturePointer(Some(id)))?;
            self.scroll_drag_to(drag, *position)?;
            return Ok(true);
        }
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn viewer(settings: ScrollSettings) -> (Controls, ScrollViewer) {
        let mut ui = Controls::new(Theme::Light).unwrap();
        let view = ui
            .scroll_view(
                ui.root(),
                LayoutStyle {
                    width: Length::Px(200.0),
                    height: Length::Px(100.0),
                    ..Default::default()
                },
                400.0,
            )
            .unwrap();
        ui.configure_scroll_view(view.viewport, settings).unwrap();
        ui.set_scroll_extent_size(view.viewport, Size::new(600.0, 400.0))
            .unwrap();
        ui.update(Size::new(400.0, 300.0)).unwrap();
        (ui, view)
    }

    #[test]
    fn two_axis_offsets_clamp_and_disabled_axis_resets_without_recreating_content() {
        let (mut ui, view) = viewer(ScrollSettings {
            horizontal: ScrollBarVisibility::Auto,
            vertical: ScrollBarVisibility::Hidden,
        });
        ui.scroll_to_point(view.viewport, Point::new(1000.0, 1000.0))
            .unwrap();
        assert_eq!(
            ui.scroll_offset(view.viewport),
            Some(Point::new(400.0, 300.0))
        );
        ui.configure_scroll_view(
            view.viewport,
            ScrollSettings {
                horizontal: ScrollBarVisibility::Disabled,
                vertical: ScrollBarVisibility::Hidden,
            },
        )
        .unwrap();
        assert_eq!(
            ui.scroll_offset(view.viewport),
            Some(Point::new(0.0, 300.0))
        );
        assert!(ui.runtime.tree().node(view.content).is_some());
        assert!(ui.scroll_bars(view.viewport).is_empty());
        assert!(
            ui.set_scroll_extent_size(view.viewport, Size::new(f32::NAN, 100.0))
                .is_err()
        );
        assert!(
            ui.scroll_to_point(view.viewport, Point::new(f32::INFINITY, 0.0))
                .is_err()
        );
    }

    #[test]
    fn overlay_thumb_drag_and_cancel_preserve_final_offset() {
        let (mut ui, view) = viewer(ScrollSettings {
            horizontal: ScrollBarVisibility::Auto,
            vertical: ScrollBarVisibility::Auto,
        });
        let bar = ui
            .scroll_bars(view.viewport)
            .into_iter()
            .find(|bar| bar.horizontal)
            .unwrap();
        let start = Point::new(bar.thumb.x + 3.0, bar.thumb.y + 3.0);
        ui.dispatch(InputEvent::PointerDown {
            position: start,
            button: PointerButton::Primary,
            modifiers: Default::default(),
        })
        .unwrap();
        ui.dispatch(InputEvent::PointerMoved {
            position: Point::new(start.x + 60.0, start.y),
        })
        .unwrap();
        let moved = ui.scroll_offset(view.viewport).unwrap();
        assert!(moved.x > 0.0);
        ui.cancel_pointer_interaction().unwrap();
        ui.dispatch(InputEvent::PointerMoved {
            position: Point::new(start.x + 90.0, start.y),
        })
        .unwrap();
        assert_eq!(ui.scroll_offset(view.viewport), Some(moved));
        assert!(ui.scroll_drag.is_none());
    }

    #[test]
    fn hidden_bars_scroll_and_visible_bars_remain_without_overflow() {
        let (mut ui, view) = viewer(ScrollSettings {
            horizontal: ScrollBarVisibility::Hidden,
            vertical: ScrollBarVisibility::Visible,
        });
        assert_eq!(ui.scroll_bars(view.viewport).len(), 1);
        ui.scroll_to_point(view.viewport, Point::new(120.0, 80.0))
            .unwrap();
        ui.set_scroll_extent_size(view.viewport, Size::new(100.0, 40.0))
            .unwrap();
        ui.update(Size::new(400.0, 300.0)).unwrap();
        assert_eq!(ui.scroll_offset(view.viewport), Some(Point::ZERO));
        let bar = ui.scroll_bars(view.viewport)[0];
        assert_eq!(bar.maximum, 0.0);
        assert_eq!(bar.thumb.height, bar.track.height);
    }

    #[test]
    fn wheel_moves_both_axes_without_layout_and_arrow_keys_use_physical_axes() {
        let (mut ui, view) = viewer(ScrollSettings {
            horizontal: ScrollBarVisibility::Auto,
            vertical: ScrollBarVisibility::Auto,
        });
        let passes = ui.stats().layout_passes;
        let bounds = ui.runtime.bounds(view.viewport).unwrap();
        ui.dispatch(InputEvent::Wheel {
            position: Point::new(bounds.x + 20.0, bounds.y + 20.0),
            delta: Point::new(30.0, 50.0),
            modifiers: Default::default(),
        })
        .unwrap();
        ui.update(Size::new(400.0, 300.0)).unwrap();
        assert_eq!(
            ui.scroll_offset(view.viewport),
            Some(Point::new(30.0, 50.0))
        );
        assert_eq!(ui.stats().layout_passes, passes);
        ui.perform_action(view.viewport, SemanticAction::Focus, None)
            .unwrap();
        ui.dispatch(InputEvent::KeyDown {
            key: Key::ArrowRight,
            modifiers: Default::default(),
            repeat: false,
        })
        .unwrap();
        assert_eq!(
            ui.scroll_offset(view.viewport),
            Some(Point::new(70.0, 50.0))
        );
    }

    #[test]
    fn disabled_scroll_viewer_ignores_wheel_and_inner_limit_chains_to_parent() {
        let (mut ui, outer) = viewer(ScrollSettings::default());
        let inner = ui
            .scroll_view(
                outer.content,
                LayoutStyle {
                    width: Length::Px(160.0),
                    height: Length::Px(60.0),
                    ..Default::default()
                },
                60.0,
            )
            .unwrap();
        ui.update(Size::new(400.0, 300.0)).unwrap();
        ui.scroll_wheel_from(inner.content, Point::new(0.0, 40.0))
            .unwrap();
        assert_eq!(ui.scroll_offset(inner.viewport), Some(Point::ZERO));
        assert_eq!(
            ui.scroll_offset(outer.viewport),
            Some(Point::new(0.0, 40.0))
        );
        ui.set_enabled(outer.viewport, false).unwrap();
        ui.scroll_wheel_from(inner.content, Point::new(0.0, 40.0))
            .unwrap();
        assert_eq!(
            ui.scroll_offset(outer.viewport),
            Some(Point::new(0.0, 40.0))
        );
    }
}
