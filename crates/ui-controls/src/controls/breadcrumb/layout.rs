use super::*;
use rust_desktop_ui_core::{GridPlacement, GridTrack};

impl Controls {
    pub(super) fn show_breadcrumb_editor(
        &mut self,
        owner: WidgetId,
        editing: bool,
    ) -> Result<(), ControlError> {
        let e = &self.breadcrumbs.entries[&owner];
        let editor = e.parts.editor;
        let bar = e.bar;
        self.edit_props(editor, |p| {
            p.visible = editing;
            p.collapsed = !editing;
        })?;
        self.edit_props(bar, |p| {
            p.visible = !editing;
            p.collapsed = editing;
        })
    }
    pub(super) fn apply_breadcrumb_availability(
        &mut self,
        owner: WidgetId,
    ) -> Result<(), ControlError> {
        let e = &self.breadcrumbs.entries[&owner];
        let p = e.parts;
        let settings = e.session.settings;
        let segments = e.segments.clone();
        let history = !e.session.history.is_empty();
        for id in [p.overflow_button, p.history_button, p.edit_button]
            .into_iter()
            .chain(segments.iter().flat_map(|s| [s.button, s.children]))
        {
            let enabled = !settings.read_only
                && (id != p.edit_button || settings.editable)
                && (id != p.history_button || history);
            self.set_enabled(id, enabled)?;
        }
        Ok(())
    }
    pub(in crate::controls) fn sync_breadcrumbs(&mut self) -> Result<(), ControlError> {
        self.prune_breadcrumbs()?;
        let owners: Vec<_> = self.breadcrumbs.entries.keys().copied().collect();
        for owner in owners {
            self.check_breadcrumb_parts(owner)?;
            let p = self.breadcrumbs.entries[&owner].parts;
            let draft = self.document(p.editor).unwrap().text().to_owned();
            let changed = self.breadcrumbs.entries[&owner].draft != draft;
            let composing = self.document(p.editor).unwrap().composition().is_some();
            let lost_popup =
                self.breadcrumbs.entries[&owner].was_open && !self.flyout_is_open(p.popup);
            let outside = self.runtime.focused().is_none_or(|focused| {
                !self.flyout_descendant(focused, owner)
                    && !self.flyout_descendant(focused, p.popup.container)
            });
            if changed || composing || lost_popup || outside || !self.breadcrumb_active(owner) {
                self.breadcrumbs
                    .entries
                    .get_mut(&owner)
                    .unwrap()
                    .session
                    .cancel();
                self.close_breadcrumb_popup(owner, false)?;
            }
            self.breadcrumbs.entries.get_mut(&owner).unwrap().draft = draft;
        }
        Ok(())
    }
    pub(in crate::controls) fn sync_breadcrumb_layout(&mut self) -> Result<(), ControlError> {
        let owners: Vec<_> = self.breadcrumbs.entries.keys().copied().collect();
        for owner in owners {
            self.check_breadcrumb_parts(owner)?;
            let e = &self.breadcrumbs.entries[&owner];
            if e.editing {
                continue;
            }
            let Some(bounds) = self.runtime.bounds(owner) else {
                continue;
            };
            let parts = e.parts;
            let bar = e.bar;
            let segments = e.segments.clone();
            let path = e.session.path.clone();
            let settings = e.session.settings;
            let button_width = self.theme.tokens().control_height;
            let overflow_needed = path.segments.len() > 1;
            let compact = bounds.width < button_width * 3.0 + 32.0;
            let show_history = !compact && settings.history_limit > 0;
            let show_edit = !compact && settings.editable;
            let reserve = button_width
                * (f32::from(overflow_needed || compact)
                    + f32::from(show_history)
                    + f32::from(show_edit));
            let mut available = (bounds.width - reserve).max(0.0);
            let mut widths = vec![0.0; segments.len()];
            let mut child_widths = vec![0.0; segments.len()];
            // Preserve the nearest levels and stable IDs. The overflow list
            // exposes every full label, including a single truncated segment.
            for i in (0..segments.len()).rev().take(48) {
                let desired = (self
                    .measure_text(&path.segments[i].label, 4096.0, false)?
                    .width
                    + 20.0)
                    .clamp(40.0, 240.0);
                let child = if path.segments[i].has_children {
                    24.0
                } else {
                    0.0
                };
                if desired + child <= available {
                    widths[i] = desired;
                    child_widths[i] = child;
                    available -= desired + child;
                } else if i == segments.len() - 1 && available >= 40.0 {
                    widths[i] = (available - child).max(24.0);
                    child_widths[i] = child.min((available - widths[i]).max(0.0));
                    break;
                } else {
                    break;
                }
            }
            let hidden: Vec<_> = segments
                .iter()
                .zip(&widths)
                .filter_map(|(s, w)| (*w == 0.0).then_some(s.key.clone()))
                .collect();
            let focused = self.runtime.focused();
            let focused_hidden = segments
                .iter()
                .find(|s| {
                    (focused == Some(s.button) || focused == Some(s.children))
                        && hidden.contains(&s.key)
                })
                .map(|s| s.key.clone());
            let restoring = self.breadcrumbs.entries[&owner]
                .hidden_focus
                .clone()
                .filter(|key| !hidden.contains(key) && focused == Some(parts.overflow_button));
            let mut grid = Vec::new();
            let mut placements = Vec::new();
            if overflow_needed || compact || !hidden.is_empty() {
                grid.push(GridTrack::Px(button_width.min(bounds.width.max(0.0))));
                placements.push((parts.overflow_button, grid.len() as u16));
            }
            for (i, s) in segments.iter().enumerate() {
                if widths[i] > 0.0 {
                    grid.push(GridTrack::Px(widths[i]));
                    placements.push((s.button, grid.len() as u16));
                    let mut run = TextRun::new(
                        &path.segments[i].label,
                        Rect::new(0.0, 0.0, (widths[i] - 20.0).max(1.0), button_width),
                        Color::rgb(0, 0, 0),
                        self.theme.tokens().font_size,
                    );
                    run.wrap = false;
                    let text = self
                        .engine
                        .fit_text(
                            &run,
                            &[],
                            rust_desktop_ui_core::TextTrimming::CharacterEllipsis,
                            Some(1),
                        )
                        .map_err(|e| ControlError::Text(e.to_string()))?
                        .run
                        .text;
                    self.set_label(s.button, text)?;
                    if child_widths[i] > 0.0 {
                        grid.push(GridTrack::Px(child_widths[i]));
                        placements.push((s.children, grid.len() as u16));
                    }
                }
            }
            grid.push(GridTrack::Fr(1.0));
            if show_history {
                grid.push(GridTrack::Px(button_width));
                placements.push((parts.history_button, grid.len() as u16));
            }
            if show_edit {
                grid.push(GridTrack::Px(button_width));
                placements.push((parts.edit_button, grid.len() as u16));
            }
            self.edit_props(bar, |p| {
                p.style.grid_columns = grid;
                p.style.grid_rows = vec![GridTrack::Fr(1.0)];
                p.clip = true;
            })?;
            for id in [
                parts.overflow_button,
                parts.history_button,
                parts.edit_button,
            ]
            .into_iter()
            .chain(segments.iter().flat_map(|s| [s.button, s.children]))
            {
                let position = placements
                    .iter()
                    .find_map(|(candidate, column)| (*candidate == id).then_some(*column));
                self.edit_props(id, |p| {
                    p.visible = position.is_some();
                    p.collapsed = position.is_none();
                    if let Some(column) = position {
                        // Both axes are explicit: the permanent history/edit
                        // buttons precede segments in retained child order.
                        // Auto row placement would move later segments below
                        // the single visible row after those trailing columns.
                        p.style.grid_row = GridPlacement {
                            start: Some(1),
                            span: 1,
                        };
                        p.style.grid_column = GridPlacement {
                            start: Some(column),
                            span: 1,
                        };
                    }
                })?;
            }
            self.breadcrumbs.entries.get_mut(&owner).unwrap().hidden = hidden;
            if let Some(key) = focused_hidden {
                self.breadcrumbs
                    .entries
                    .get_mut(&owner)
                    .unwrap()
                    .hidden_focus = Some(key);
                self.runtime
                    .apply(Mutation::Focus(Some(parts.overflow_button)))?;
            } else if let Some(key) = restoring
                && let Some(id) = self.breadcrumb_segment_button(owner, &key)
            {
                self.breadcrumbs
                    .entries
                    .get_mut(&owner)
                    .unwrap()
                    .hidden_focus = None;
                self.runtime.apply(Mutation::Focus(Some(id)))?;
            }
        }
        Ok(())
    }
}
