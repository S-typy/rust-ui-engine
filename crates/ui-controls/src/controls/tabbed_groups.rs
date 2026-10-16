//! Retained pages sharing the same tab model as `TabControl`.
use super::*;

/// Caption and availability of one retained page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GroupTab {
    pub header: String,
    pub icon: Option<IconSymbol>,
    pub enabled: bool,
}

impl GroupTab {
    pub fn new(header: impl Into<String>) -> Self {
        Self {
            header: header.into(),
            icon: None,
            enabled: true,
        }
    }
}

/// Pages and headers remain alive when selection changes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TabbedGroup {
    pub container: WidgetId,
    pub headers: Vec<WidgetId>,
    pub contents: Vec<WidgetId>,
}

impl Controls {
    /// Creates 1..=64 pages. The initial selection must be locally enabled.
    pub fn tabbed_group(
        &mut self,
        parent: WidgetId,
        definitions: Vec<GroupTab>,
        selected_index: usize,
        style: LayoutStyle,
    ) -> Result<TabbedGroup, ControlError> {
        self.add_tabbed_group(parent, definitions, selected_index, style, false)
    }

    pub(super) fn add_tabbed_group(
        &mut self,
        parent: WidgetId,
        definitions: Vec<GroupTab>,
        selected_index: usize,
        mut style: LayoutStyle,
        legacy_layout: bool,
    ) -> Result<TabbedGroup, ControlError> {
        if !(1..=64).contains(&definitions.len()) {
            return Err(ControlError::Invalid(
                "TabbedGroup requires 1..=64 pages".into(),
            ));
        }
        if definitions
            .iter()
            .any(|tab| tab.header.trim().is_empty() || tab.header.len() > 4096)
        {
            return Err(ControlError::Invalid(
                "Tab Header must be nonempty and at most 4096 UTF-8 bytes".into(),
            ));
        }
        if !definitions
            .get(selected_index)
            .is_some_and(|tab| tab.enabled)
        {
            return Err(ControlError::Invalid(
                "Selected tab must exist and be enabled".into(),
            ));
        }
        style.kind = LayoutKind::Flex(Axis::Vertical);
        if !legacy_layout {
            style.padding = Edges::default();
            style.gap = 0.0;
        }
        let container = self.panel(parent, style)?;
        if !legacy_layout {
            self.set_role(container, Role::Group);
            self.set_surface(container, SurfaceKind::Transparent)?;
        }
        let header_height = if legacy_layout {
            self.theme.tokens().control_height
        } else {
            48.0
        };
        let header = self.panel(
            container,
            LayoutStyle {
                kind: LayoutKind::Flex(Axis::Horizontal),
                height: Length::Px(header_height),
                flex_shrink: 0.0,
                gap: if legacy_layout { 4.0 } else { 0.0 },
                ..Default::default()
            },
        )?;
        self.set_role(header, Role::TabList);
        if !legacy_layout {
            self.set_surface(header, SurfaceKind::Transparent)?;
        }
        let content = self.panel(
            container,
            LayoutStyle {
                kind: if legacy_layout {
                    LayoutKind::Overlay
                } else {
                    LayoutKind::Flex(Axis::Vertical)
                },
                height: if legacy_layout {
                    Length::Px(0.0)
                } else {
                    Length::Auto
                },
                flex_grow: 1.0,
                flex_shrink: 1.0,
                ..Default::default()
            },
        )?;
        if !legacy_layout {
            self.set_surface(content, SurfaceKind::Transparent)?;
        }
        let mut tabs = Vec::with_capacity(definitions.len());
        let mut pages = Vec::with_capacity(definitions.len());
        let mut icons = Vec::with_capacity(definitions.len());
        for (index, definition) in definitions.into_iter().enumerate() {
            let tab = self.add(
                header,
                Kind::Tab {
                    group: container,
                    index,
                },
                definition.header.clone(),
                None,
                LayoutStyle {
                    height: if legacy_layout {
                        Length::Auto
                    } else {
                        Length::Px(header_height)
                    },
                    flex_grow: 1.0,
                    ..Default::default()
                },
            )?;
            self.controls.get_mut(&tab).unwrap().enabled = definition.enabled;
            self.edit_props(tab, |props| {
                props.enabled = definition.enabled;
                props.tab_stop = index == selected_index;
            })?;
            let page = self.panel(
                content,
                LayoutStyle {
                    kind: LayoutKind::Stack(Axis::Vertical),
                    padding: Edges::all(if legacy_layout { 8.0 } else { 16.0 }),
                    gap: if legacy_layout { 6.0 } else { 0.0 },
                    flex_grow: if legacy_layout { 0.0 } else { 1.0 },
                    flex_shrink: 1.0,
                    ..Default::default()
                },
            )?;
            if !legacy_layout {
                self.set_surface(page, SurfaceKind::Transparent)?;
                self.set_role(page, Role::Group);
                self.controls.get_mut(&page).unwrap().label = definition.header;
            }
            self.edit_props(page, |props| props.collapsed = index != selected_index)?;
            tabs.push(tab);
            pages.push(page);
            icons.push(definition.icon);
        }
        self.tabs.insert(
            container,
            Tabs {
                header,
                content,
                tabs: tabs.clone(),
                pages: pages.clone(),
                icons,
                selected: selected_index,
                legacy_layout,
            },
        );
        Ok(TabbedGroup {
            container,
            headers: tabs,
            contents: pages,
        })
    }

    pub(crate) fn tab_owner(&self, id: WidgetId) -> Option<WidgetId> {
        if self.tabs.contains_key(&id) && self.runtime.tree().node(id).is_some() {
            return Some(id);
        }
        match self.controls.get(&id)?.kind {
            Kind::Tab { group, .. } if self.tabs.contains_key(&group) => Some(group),
            _ => None,
        }
    }

    pub fn selected_tab(&self, id: WidgetId) -> Option<usize> {
        Some(self.tabs.get(&self.tab_owner(id)?)?.selected)
    }

    /// Updates the retained page without committing ordinary editor drafts.
    /// Container and header aliases refer to the same selected index.
    pub fn set_selected_tab(&mut self, id: WidgetId, selected: usize) -> Result<(), ControlError> {
        let owner = self
            .tab_owner(id)
            .ok_or_else(|| ControlError::Invalid("Unknown tab control".into()))?;
        let state = &self.tabs[&owner];
        let target = state
            .tabs
            .get(selected)
            .copied()
            .filter(|tab| {
                self.controls
                    .get(tab)
                    .is_some_and(|control| control.enabled)
            })
            .ok_or_else(|| {
                ControlError::Invalid("Selected tab must exist and be enabled".into())
            })?;
        if state.selected == selected {
            return Ok(());
        }
        let previous_page = state.pages[state.selected];
        let next_page = state.pages[selected];
        let focus_within = self.runtime.focused().is_some_and(|focused| {
            self.tab_descendant(focused, previous_page)
                || state.tabs.contains(&focused)
                || self.rich.popup.as_ref().is_some_and(|popup| {
                    self.tab_descendant(popup.owner, previous_page)
                        && self.tab_descendant(focused, popup.surface)
                })
                || (self.group_popup_focused()
                    && self
                        .popup_anchor()
                        .is_some_and(|anchor| self.tab_descendant(anchor, previous_page)))
        });
        let capture_within = self.runtime.captured().is_some_and(|captured| {
            self.tab_descendant(captured, previous_page)
                || (state.tabs.contains(&captured) && captured != target)
        });
        let cancel_press = self.pressed.is_some_and(|pressed| {
            self.tab_descendant(pressed, previous_page)
                || (state.tabs.contains(&pressed) && pressed != target)
        });
        self.edit_props(previous_page, |props| props.collapsed = true)?;
        self.edit_props(next_page, |props| props.collapsed = false)?;
        self.tabs.get_mut(&owner).unwrap().selected = selected;
        if capture_within {
            self.scroll_drag = None;
            self.runtime.apply(Mutation::CapturePointer(None))?;
        }
        if self
            .rich
            .popup
            .as_ref()
            .is_some_and(|popup| self.tab_descendant(popup.owner, previous_page))
        {
            self.close_rich_popup(false)?;
        }
        self.sync_popup_availability()?;
        self.synchronize_editor_focus()?;
        self.cancel_inactive_preedit();
        if cancel_press {
            self.pressed = None;
            self.keyboard_pressed = false;
        }
        if self
            .selecting
            .is_some_and(|selecting| self.tab_descendant(selecting, previous_page))
        {
            self.selecting = None;
        }
        self.tooltip_sync();
        self.refresh_tab_stops()?;
        if focus_within && self.is_enabled(target) && self.window_focused {
            self.runtime.apply(Mutation::Focus(Some(target)))?;
        }
        self.scene_dirty = true;
        Ok(())
    }

    fn tab_descendant(&self, mut id: WidgetId, ancestor: WidgetId) -> bool {
        loop {
            if id == ancestor {
                return true;
            }
            let Some(parent) = self.runtime.tree().parent(id) else {
                return false;
            };
            id = parent;
        }
    }

    pub(super) fn preflight_tab_removal(&self, id: WidgetId) -> Result<(), ControlError> {
        if self.tabs.values().any(|state| {
            state.header == id
                || state.content == id
                || state.tabs.contains(&id)
                || state.pages.contains(&id)
        }) {
            return Err(ControlError::Invalid("Required tab parts cannot be removed individually; remove their container or ancestor".into()));
        }
        Ok(())
    }

    pub(super) fn refresh_tab_stops(&mut self) -> Result<(), ControlError> {
        let mut updates = Vec::new();
        for state in self.tabs.values() {
            let selected = state.tabs[state.selected];
            let representative = if self.is_enabled(selected) {
                Some(selected)
            } else {
                state.tabs.iter().copied().find(|&tab| self.is_enabled(tab))
            };
            for &tab in &state.tabs {
                if let Some(node) = self.runtime.tree().node(tab)
                    && node.props.tab_stop != (Some(tab) == representative)
                {
                    updates.push((tab, Some(tab) == representative));
                }
            }
        }
        for (tab, tab_stop) in updates {
            self.edit_props(tab, |props| props.tab_stop = tab_stop)?;
        }
        Ok(())
    }

    pub(super) fn tab_key(&mut self, id: WidgetId, key: Key) -> Result<(), ControlError> {
        let Some(owner) = self.tab_owner(id) else {
            return Ok(());
        };
        let state = &self.tabs[&owner];
        let available: Vec<_> = state
            .tabs
            .iter()
            .copied()
            .filter(|&tab| self.is_enabled(tab))
            .collect();
        let Some(current) = available.iter().position(|&tab| tab == id) else {
            return Ok(());
        };
        let next = match key {
            Key::ArrowLeft => (current + available.len() - 1) % available.len(),
            Key::ArrowRight => (current + 1) % available.len(),
            Key::Home => 0,
            Key::End => available.len() - 1,
            _ => return Ok(()),
        };
        let target = available[next];
        self.runtime.apply(Mutation::Focus(Some(target)))?;
        self.activate(target)
    }

    pub(super) fn prepare_tab_exit(&mut self, event: &InputEvent) -> Result<bool, ControlError> {
        let InputEvent::KeyDown {
            key: Key::Tab,
            modifiers,
            ..
        } = event
        else {
            return Ok(false);
        };
        if modifiers.control || modifiers.alt || modifiers.super_key {
            return Ok(false);
        }
        let Some(owner) = self
            .runtime
            .focused()
            .and_then(|focused| self.tab_owner(focused))
        else {
            return Ok(false);
        };
        let outside = self.runtime.navigation_order().into_iter().any(|id| {
            self.tab_owner(id) != Some(owner)
                && self.is_enabled(id)
                && self
                    .runtime
                    .tree()
                    .node(id)
                    .is_some_and(|node| node.props.focusable && node.props.tab_stop)
        });
        if !outside {
            return Ok(false);
        }
        let headers = self.tabs[&owner].tabs.clone();
        for header in headers {
            if self
                .runtime
                .tree()
                .node(header)
                .is_some_and(|node| node.props.tab_stop)
            {
                self.edit_props(header, |props| props.tab_stop = false)?;
            }
        }
        Ok(true)
    }

    pub(super) fn paint_tabbed_group(
        &mut self,
        id: WidgetId,
        bounds: Rect,
        clip: Rect,
    ) -> Result<bool, ControlError> {
        let chrome = self.theme.chrome();
        let tokens = self.theme.tokens();
        if let Some(state) = self.tabs.get(&id) {
            if state.legacy_layout {
                return Ok(false);
            }
            let radius = if self.theme.is_fluent() { 8.0 } else { 0.0 };
            editor_fill(&mut self.scene, bounds, clip, radius, chrome.card_surface);
            shape_border(
                &mut self.scene,
                bounds,
                clip,
                radius,
                1.0,
                chrome.surface_border,
            );
            return Ok(true);
        }
        let Some(owner) = self.tab_owner(id) else {
            return Ok(false);
        };
        if self.tabs[&owner].legacy_layout {
            return Ok(false);
        }
        let clip = bounds
            .intersection(clip)
            .unwrap_or(Rect::new(0.0, 0.0, 0.0, 0.0));
        let state = &self.tabs[&owner];
        let Some(index) = state.tabs.iter().position(|&tab| tab == id) else {
            return Ok(false);
        };
        let selected = index == state.selected;
        let icon = state.icons[index];
        let enabled = self.is_enabled(id);
        if enabled && (self.runtime.hovered() == Some(id) || self.pressed == Some(id)) {
            let fill = if self.pressed == Some(id) {
                chrome.subtle_pressed
            } else {
                chrome.subtle_hover
            };
            editor_fill(
                &mut self.scene,
                Rect::new(
                    bounds.x + 3.0,
                    bounds.y + 3.0,
                    (bounds.width - 6.0).max(0.0),
                    (bounds.height - 6.0).max(0.0),
                ),
                clip,
                5.0,
                fill,
            );
        }
        fill_clipped(
            &mut self.scene,
            Rect::new(bounds.x, bounds.y + bounds.height - 1.0, bounds.width, 1.0),
            clip,
            chrome.divider,
        );
        if selected {
            let inset = 16.0_f32.min(bounds.width / 3.0);
            editor_fill(
                &mut self.scene,
                Rect::new(
                    bounds.x + inset,
                    bounds.y + bounds.height - 3.0,
                    (bounds.width - inset * 2.0).max(0.0),
                    3.0,
                ),
                clip,
                1.5,
                if enabled {
                    tokens.focus
                } else {
                    chrome.disabled_text
                },
            );
        }
        if enabled && self.runtime.focused() == Some(id) {
            shape_border(
                &mut self.scene,
                Rect::new(
                    bounds.x + 3.0,
                    bounds.y + 3.0,
                    (bounds.width - 6.0).max(0.0),
                    (bounds.height - 6.0).max(0.0),
                ),
                clip,
                4.0,
                2.0,
                tokens.focus,
            );
        }
        let color = if enabled {
            tokens.text
        } else {
            chrome.disabled_text
        };
        let icon_space = if icon.is_some() { 24.0 } else { 0.0 };
        if let Some(icon) = icon {
            crate::primitives::paint_icon(
                &mut self.scene,
                icon,
                Rect::new(
                    bounds.x + 16.0,
                    bounds.y + (bounds.height - 16.0) / 2.0,
                    16.0,
                    16.0,
                ),
                clip,
                color,
            );
        }
        let text_bounds = Rect::new(
            bounds.x + 16.0 + icon_space,
            bounds.y,
            (bounds.width - 32.0 - icon_space).max(0.0),
            bounds.height,
        );
        let mut run = TextRun {
            bounds: text_bounds,
            clip: text_bounds
                .intersection(clip)
                .unwrap_or(Rect::new(0.0, 0.0, 0.0, 0.0)),
            text: self.controls[&id]
                .label
                .chars()
                .map(|c| if single_line_separator(c) { ' ' } else { c })
                .collect(),
            font_size: tokens.font_size,
            color,
            weight: if selected { 600 } else { 400 },
            align: TextAlign::Center,
            wrap: false,
        };
        let prepared = self
            .engine
            .prepare(&run, 1.0)
            .map_err(|e| ControlError::Text(e.to_string()))?;
        run.bounds.y += ((bounds.height - prepared.height) / 2.0).max(0.0);
        self.scene.text(run);
        Ok(true)
    }
}
