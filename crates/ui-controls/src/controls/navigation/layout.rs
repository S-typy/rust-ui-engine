use super::*;
use rust_desktop_ui_core::{GridPlacement, GridTrack};

impl Controls {
    pub(in crate::controls) fn sync_navigations(&mut self) -> Result<(), ControlError> {
        self.prune_navigations();
        let owners: Vec<_> = self.navigations.entries.keys().copied().collect();
        for owner in owners {
            self.check_navigation(owner)?;
            let focused = self.runtime.focused();
            let remembered = self.navigations.entries[&owner].pages.iter().find_map(|p| {
                focused
                    .filter(|id| self.flyout_descendant(*id, p.host))
                    .map(|id| (p.route.clone(), id))
            });
            if let Some((route, id)) = remembered {
                self.navigations
                    .entries
                    .get_mut(&owner)
                    .unwrap()
                    .pages
                    .iter_mut()
                    .find(|p| p.route == route)
                    .unwrap()
                    .focus = Some(id);
            }
            self.layout_navigation(owner)?;
        }
        if let Some(id) = self.navigations.pointer
            && (!self.is_enabled(id) || !self.window_focused || self.runtime.captured() != Some(id))
        {
            self.navigations.pointer = None;
            self.pressed = None;
            self.runtime.apply(Mutation::CapturePointer(None))?;
        }
        Ok(())
    }
    pub(super) fn present_navigation(&mut self, owner: WidgetId) -> Result<(), ControlError> {
        let e = &self.navigations.entries[&owner];
        let parts = e.parts;
        let caption = e.caption;
        let label = e.model.current_page().label.clone();
        let caption_text = e
            .error
            .as_ref()
            .map_or_else(|| label.clone(), |error| format!("{label} — {error}"));
        let route = e.model.route().to_owned();
        let back = e.model.back_route().is_some();
        let forward = e.model.forward_route().is_some();
        let pages: Vec<_> = e
            .pages
            .iter()
            .map(|p| {
                (
                    p.selector,
                    p.route.clone(),
                    e.model
                        .pages()
                        .iter()
                        .find(|d| d.route == p.route)
                        .unwrap()
                        .enabled,
                )
            })
            .collect();
        self.set_label(caption, caption_text)?;
        self.set_automation_name(parts.frame, &label)?;
        // The direct fields avoid implicit editor acceptance while chrome changes.
        for (id, enabled) in [(parts.back_button, back), (parts.forward_button, forward)] {
            self.controls.get_mut(&id).unwrap().enabled = enabled;
            self.edit_props(id, |p| p.enabled = enabled)?;
        }
        for (id, value, enabled) in pages {
            let control = self.controls.get_mut(&id).unwrap();
            control.enabled = enabled;
            control.checked = value == route;
            self.edit_props(id, |p| {
                p.enabled = enabled;
                p.tab_stop = value == route;
            })?;
        }
        self.layout_navigation(owner)
    }
    fn layout_navigation(&mut self, owner: WidgetId) -> Result<(), ControlError> {
        let e = &self.navigations.entries[&owner];
        let settings = e.settings;
        let compact = settings.view == NavigationView::Hamburger && !e.hamburger_open;
        let hidden = settings.view == NavigationView::Frame;
        let width = if hidden {
            0.0
        } else if compact {
            52.0
        } else {
            settings.pane_width
        };
        let width = self
            .runtime
            .bounds(owner)
            .map_or(width, |r| width.min((r.width - 80.0).max(0.0)));
        let p = e.parts;
        let toolbar = e.toolbar;
        let sidebar = e.sidebar;
        let caption = e.caption;
        let viewer = e.viewer;
        let office_viewer = e.office_viewer;
        let sections = e.sections.clone();
        let pages = e.pages.clone();
        let model = e.model.clone();
        let expanded = e.expanded_section.clone();
        let row_height = self.theme.tokens().control_height.max(32.0);
        self.edit_props(owner, |props| {
            props.style.grid_columns = vec![GridTrack::Px(width), GridTrack::Fr(1.0)];
            props.style.grid_rows = vec![GridTrack::Auto, GridTrack::Fr(1.0)];
            props.clip = true;
        })?;
        self.edit_props(toolbar, |props| {
            props.style.grid_row = placement(1);
            props.style.grid_column = GridPlacement {
                start: Some(1),
                span: 2,
            };
            props.style.height = Length::Px(row_height + 8.0);
            props.style.padding = Edges::all(4.0);
            props.clip = true;
        })?;
        self.edit_props(sidebar, |props| {
            props.collapsed = hidden;
            props.style.grid_row = placement(2);
            props.style.grid_column = placement(1);
            props.style.grid_columns = vec![GridTrack::Fr(1.0)];
            props.style.grid_rows = vec![GridTrack::Fr(1.0), GridTrack::Auto];
            props.clip = true;
        })?;
        self.edit_props(p.frame, |props| {
            props.style.grid_row = placement(2);
            props.style.grid_column = placement(2);
            props.clip = true;
        })?;
        self.edit_props(viewer.viewport, |props| {
            props.style.grid_row = placement(1);
            props.style.grid_column = placement(1);
            props.style.height = Length::Percent(1.0);
        })?;
        self.edit_props(office_viewer.viewport, |props| {
            props.collapsed = settings.view != NavigationView::Office;
            props.style.grid_row = placement(2);
            props.style.grid_column = placement(1);
            props.style.height =
                Length::Px((sections.len() as f32 * (row_height + 2.0) + 12.0).min(180.0));
        })?;
        self.edit_props(p.menu_button, |props| {
            props.collapsed = settings.view != NavigationView::Hamburger;
            props.style.width = Length::Px(40.0);
            props.style.height = Length::Px(row_height);
        })?;
        for id in [p.back_button, p.forward_button] {
            self.edit_props(id, |props| {
                props.collapsed = !settings.show_history;
                props.style.width = Length::Px(84.0);
                props.style.height = Length::Px(row_height);
            })?;
        }
        self.edit_props(caption, |props| {
            props.style.height = Length::Px(row_height);
            props.style.min_size.width = 0.0;
            props.style.flex_shrink = 1.0;
        })?;
        let mut extent = 12.0;
        for section in sections {
            let office = settings.view == NavigationView::Office;
            let section_visible = !office || section.id == expanded;
            self.edit_props(section.host, |props| {
                props.collapsed = !section_visible;
                props.style.width = Length::Percent(1.0);
            })?;
            let header_visible = !compact && !office;
            self.edit_props(section.header, |props| {
                props.collapsed = !header_visible;
                props.style.height = Length::Px(row_height);
                props.tab_stop = settings.view == NavigationView::Accordion;
            })?;
            self.edit_props(section.office, |props| {
                props.style.height = Length::Px(row_height);
            })?;
            self.controls.get_mut(&section.office).unwrap().checked = section.id == expanded;
            if section_visible && header_visible {
                extent += row_height + 2.0;
            }
            for page in pages.iter().filter(|p| {
                model
                    .pages()
                    .iter()
                    .any(|d| d.route == p.route && d.section == section.id)
            }) {
                let definition = model
                    .pages()
                    .iter()
                    .find(|d| d.route == page.route)
                    .unwrap();
                let visible = section_visible
                    && (settings.view != NavigationView::Accordion || section.id == expanded);
                self.edit_props(page.selector, |props| {
                    props.collapsed = !visible;
                    props.style.height = Length::Px(row_height);
                    props.style.width = Length::Percent(1.0);
                })?;
                let control = self.controls.get_mut(&page.selector).unwrap();
                control.button_content_layout = if compact {
                    ButtonContentLayout::IconOnly
                } else {
                    ButtonContentLayout::Leading
                };
                control.button_icon = definition
                    .icon
                    .or(compact.then_some(IconSymbol::FolderOpen))
                    .map(ButtonIcon::new);
                let available = (width
                    - if control.button_icon.is_some() {
                        52.0
                    } else {
                        24.0
                    })
                .max(1.0);
                let mut run = TextRun::new(
                    &definition.label,
                    Rect::new(0.0, 0.0, available, row_height),
                    Color::rgb(0, 0, 0),
                    self.theme.tokens().font_size,
                );
                run.wrap = false;
                let fitted = self
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
                self.set_label(page.selector, fitted)?;
                if visible {
                    extent += row_height + 2.0;
                }
            }
            if section_visible {
                extent += 4.0;
            }
        }
        self.set_scroll_extent(viewer.viewport, extent)?;
        self.set_scroll_extent(
            office_viewer.viewport,
            model.sections().len() as f32 * (row_height + 2.0) + 12.0,
        )?;
        self.scene_dirty = true;
        Ok(())
    }
}
fn placement(start: u16) -> GridPlacement {
    GridPlacement {
        start: Some(start),
        span: 1,
    }
}
