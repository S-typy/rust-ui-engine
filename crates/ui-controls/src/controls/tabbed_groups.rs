//! Retained pages sharing the same tab model as `TabControl`.
use super::*;

#[derive(Clone, Copy)]
struct TabDrag {
    tab: WidgetId,
    start: Point,
    moved: bool,
}

pub(super) struct Tabs {
    pub(super) header: WidgetId,
    pub(super) content: WidgetId,
    pub(super) icons: Vec<Option<IconSymbol>>,
    pub(super) tabs: Vec<WidgetId>,
    pub(super) pages: Vec<WidgetId>,
    pub(super) selected: usize,
    pub(super) legacy_layout: bool,
    strip: WidgetId,
    overflow: WidgetId,
    flyout: Flyout,
    overflow_viewer: ScrollViewer,
    force_order: bool,
    drag: Option<TabDrag>,
}

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
        let strip = self.transparent_panel(
            header,
            LayoutStyle {
                kind: LayoutKind::Flex(Axis::Horizontal),
                width: Length::Px(0.0),
                height: Length::Percent(1.0),
                flex_grow: 1.0,
                gap: 0.0,
                ..Default::default()
            },
        )?;
        let overflow = self.button(
            header,
            "More tabs",
            None,
            LayoutStyle {
                width: Length::Px(32.0),
                height: Length::Percent(1.0),
                flex_shrink: 0.0,
                ..Default::default()
            },
        )?;
        self.set_button_icon(overflow, Some(ButtonIcon::new(IconSymbol::ChevronDown)))?;
        self.set_button_content_layout(overflow, ButtonContentLayout::IconOnly)?;
        self.set_button_chrome(overflow, ButtonChrome::Flat)?;
        self.edit_props(overflow, |p| {
            p.visible = false;
            p.collapsed = true;
        })?;
        let flyout = self.flyout(overflow, Size::new(280.0, 240.0))?;
        self.set_button_flyout(overflow, flyout)?;
        let overflow_viewer = self.scroll_view(
            flyout.content,
            LayoutStyle {
                width: Length::Percent(1.0),
                height: Length::Percent(1.0),
                ..Default::default()
            },
            240.0,
        )?;
        self.set_role(overflow_viewer.content, Role::TabList);
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
                strip,
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
                strip,
                overflow,
                flyout,
                overflow_viewer,
                force_order: false,
                drag: None,
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
        let flyout = self.tabs[&owner].flyout;
        if self.flyout_is_open(flyout) {
            self.close_flyout(flyout)?;
        }
        self.sync_tab_headers()?;
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
                || state.strip == id
                || state.overflow == id
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
            .filter(|tab| {
                self.controls
                    .get(tab)
                    .is_some_and(|control| control.enabled)
            })
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
        self.select_tab_id(owner, target)?;
        self.runtime.apply(Mutation::Focus(Some(target)))?;
        Ok(())
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
        if self.button_content(id).is_some() {
            return Ok(true);
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

impl Controls {
    /// Stable selected header identity, independent of the current page order.
    pub fn selected_tab_id(&self, id: WidgetId) -> Option<WidgetId> {
        let state = self.tabs.get(&self.tab_owner(id)?)?;
        Some(state.tabs[state.selected])
    }

    /// Current order of retained header identities. Initial handle vectors are
    /// snapshots; this method reflects subsequent user and programmatic moves.
    pub fn tab_order(&self, id: WidgetId) -> Option<&[WidgetId]> {
        Some(&self.tabs.get(&self.tab_owner(id)?)?.tabs)
    }

    pub fn select_tab_id(&mut self, id: WidgetId, header: WidgetId) -> Result<(), ControlError> {
        let state = self
            .tabs
            .get(
                &self
                    .tab_owner(id)
                    .ok_or_else(|| ControlError::Invalid("Unknown tab control".into()))?,
            )
            .unwrap();
        let index = state
            .tabs
            .iter()
            .position(|tab| *tab == header)
            .ok_or_else(|| {
                ControlError::Invalid("Header does not belong to this tab control".into())
            })?;
        self.set_selected_tab(id, index)
    }

    /// Atomically reorders all pages. Every existing header must occur once.
    /// Selection follows its header, while pages, editor drafts and IDs survive.
    pub fn reorder_tabs(&mut self, id: WidgetId, order: &[WidgetId]) -> Result<(), ControlError> {
        let owner = self
            .tab_owner(id)
            .ok_or_else(|| ControlError::Invalid("Unknown tab control".into()))?;
        let state = &self.tabs[&owner];
        let unique: HashSet<_> = order.iter().copied().collect();
        if order.len() != state.tabs.len()
            || unique.len() != order.len()
            || state.tabs.iter().any(|tab| !unique.contains(tab))
        {
            return Err(ControlError::Invalid(
                "Tab order must contain every current header exactly once".into(),
            ));
        }
        if order == state.tabs {
            return Ok(());
        }
        let selected = state.tabs[state.selected];
        let pages: Vec<_> = order
            .iter()
            .map(|id| state.pages[state.tabs.iter().position(|tab| tab == id).unwrap()])
            .collect();
        let icons: Vec<_> = order
            .iter()
            .map(|id| state.icons[state.tabs.iter().position(|tab| tab == id).unwrap()])
            .collect();
        let state = self.tabs.get_mut(&owner).unwrap();
        state.tabs = order.to_vec();
        state.pages = pages;
        state.icons = icons;
        state.selected = order.iter().position(|tab| *tab == selected).unwrap();
        state.force_order = true;
        for (index, tab) in order.iter().enumerate() {
            self.controls.get_mut(tab).unwrap().kind = Kind::Tab {
                group: owner,
                index,
            };
        }
        self.sync_tab_headers()?;
        self.refresh_tab_stops()?;
        self.scene_dirty = true;
        Ok(())
    }

    /// Replaces the visual header only; the Tab retains label, selected state,
    /// focus, keyboard navigation and hit testing. Content must be noninteractive.
    pub fn set_tab_header_content(
        &mut self,
        header: WidgetId,
        content: Option<WidgetId>,
    ) -> Result<(), ControlError> {
        if !matches!(
            self.controls.get(&header).map(|c| &c.kind),
            Some(Kind::Tab { .. })
        ) {
            return Err(ControlError::Invalid(
                "Tab header template requires a tab header".into(),
            ));
        }
        self.set_button_content(header, content)
    }

    pub fn tab_overflow_button(&self, id: WidgetId) -> Option<WidgetId> {
        Some(self.tabs.get(&self.tab_owner(id)?)?.overflow)
    }

    pub(super) fn sync_tab_headers(&mut self) -> Result<(), ControlError> {
        let owners: Vec<_> = self.tabs.keys().copied().collect();
        for owner in owners {
            let Some(bounds) = self.runtime.bounds(self.tabs[&owner].header) else {
                continue;
            };
            let state = &self.tabs[&owner];
            let (
                tabs,
                pages,
                icons,
                selected,
                strip,
                overflow,
                flyout,
                viewer,
                force,
                legacy,
                header,
                content,
            ) = (
                state.tabs.clone(),
                state.pages.clone(),
                state.icons.clone(),
                state.selected,
                state.strip,
                state.overflow,
                state.flyout,
                state.overflow_viewer,
                state.force_order,
                state.legacy_layout,
                state.header,
                state.content,
            );
            let height = if legacy {
                self.theme.tokens().control_height
            } else {
                self.theme.tokens().control_height.max(48.0)
            };
            let mut widths = Vec::with_capacity(tabs.len());
            for (index, tab) in tabs.iter().enumerate() {
                let label = self.controls[tab]
                    .label
                    .chars()
                    .map(|c| if single_line_separator(c) { ' ' } else { c })
                    .collect::<String>();
                // Keep the strip stable when selection changes and reserve the
                // same semibold glyph advances used by the selected caption.
                let mut caption_width = 0.0_f32;
                for font_weight in [400, 600] {
                    caption_width = caption_width.max(
                        self.measure_styled_text(
                            &label,
                            4096.0,
                            TextStyle {
                                font_weight: Some(font_weight),
                                wrap: false,
                                ..Default::default()
                            },
                        )?
                        .width,
                    );
                }
                let width =
                    (caption_width.ceil() + 32.0 + if icons[index].is_some() { 24.0 } else { 0.0 })
                        .clamp(72.0, 280.0);
                let template_width = self
                    .button_content(*tab)
                    .and_then(|root| self.runtime.tree().node(root))
                    .and_then(|node| match node.props.style.width {
                        Length::Px(width) => Some(width + 16.0),
                        _ => None,
                    })
                    .unwrap_or(0.0);
                widths.push(width.max(template_width).min(4096.0));
            }
            let overflowed = widths.iter().sum::<f32>() > bounds.width;
            let available = (bounds.width - if overflowed { 32.0 } else { 0.0 }).max(0.0);
            let mut visible = vec![false; tabs.len()];
            let mut used = widths[selected].min(available);
            visible[selected] = true;
            for index in 0..tabs.len() {
                if index == selected {
                    continue;
                }
                if !overflowed || used + widths[index] <= available {
                    visible[index] = true;
                    used += widths[index];
                }
            }
            let rearrange = force
                || tabs.iter().enumerate().any(|(index, tab)| {
                    let target = if visible[index] {
                        strip
                    } else {
                        viewer.content
                    };
                    self.runtime.tree().parent(*tab) != Some(target)
                });
            let mut mutations = Vec::new();
            let mut overflow_count = 0;
            for (index, tab) in tabs.iter().copied().enumerate() {
                let target = if visible[index] {
                    strip
                } else {
                    overflow_count += 1;
                    viewer.content
                };
                if rearrange {
                    mutations.push(Mutation::Reparent {
                        node: tab,
                        parent: target,
                    });
                }
                let mut props = self.runtime.tree().node(tab).unwrap().props.clone();
                props.style.width = if visible[index] {
                    Length::Px(widths[index].min(available))
                } else {
                    Length::Percent(1.0)
                };
                props.style.height = Length::Px(height);
                props.style.flex_grow = 0.0;
                props.style.flex_shrink = 0.0;
                mutations.push(Mutation::SetProps { node: tab, props });
            }
            if force {
                for page in pages {
                    mutations.push(Mutation::Reparent {
                        node: page,
                        parent: content,
                    });
                }
            }
            self.runtime.apply(Mutation::Batch(mutations))?;
            self.edit_props(header, |p| p.style.height = Length::Px(height))?;
            self.edit_props(overflow, |p| {
                p.visible = overflowed;
                p.collapsed = !overflowed;
            })?;
            self.set_scroll_extent(
                viewer.viewport,
                (overflow_count as f32 * (height + 6.0) + 12.0).max(1.0),
            )?;
            self.set_flyout_placement(
                flyout,
                Size::new(
                    300.0,
                    (overflow_count as f32 * (height + 6.0) + 28.0).max(48.0),
                ),
                FlyoutPlacement::Below,
            )?;
            if !overflowed && self.flyout_is_open(flyout) {
                self.close_flyout(flyout)?;
            }
            self.tabs.get_mut(&owner).unwrap().force_order = false;
        }
        Ok(())
    }

    pub(super) fn cancel_tab_drag(&mut self) -> Result<(), ControlError> {
        let captured = self.runtime.captured();
        let mut release = false;
        for state in self.tabs.values_mut() {
            if let Some(drag) = state.drag.take() {
                release |= captured == Some(drag.tab);
            }
        }
        if release {
            self.runtime.apply(Mutation::CapturePointer(None))?;
        }
        self.scene_dirty = true;
        Ok(())
    }

    fn route_tab_input(&mut self, event: &InputEvent) -> Result<bool, ControlError> {
        let report = self.runtime.dispatch(event.clone());
        if let Some(error) = report.errors.into_iter().next() {
            return Err(error.into());
        }
        self.prune_removed_controls()?;
        self.synchronize_editor_focus()?;
        self.cancel_inactive_preedit();
        self.tooltip_after_input(event);
        self.scene_dirty = true;
        Ok(!report.default_prevented)
    }

    pub(super) fn tab_before_input(&mut self, event: &InputEvent) -> Result<bool, ControlError> {
        let drag = self
            .tabs
            .iter()
            .find_map(|(id, state)| state.drag.map(|drag| (*id, drag)));
        if let Some((owner, drag)) = drag {
            if !self.window_focused
                || !self.is_enabled(drag.tab)
                || self.runtime.captured() != Some(drag.tab)
            {
                self.cancel_tab_drag()?;
            } else {
                match event {
                    InputEvent::PointerMoved { position } => {
                        let allowed = self.route_tab_input(event)?;
                        if !self.tabs.contains_key(&owner)
                            || !self.is_enabled(drag.tab)
                            || self.runtime.captured() != Some(drag.tab)
                        {
                            self.cancel_tab_drag()?;
                            return Ok(true);
                        }
                        if allowed
                            && ((position.x - drag.start.x).abs()
                                + (position.y - drag.start.y).abs())
                                >= 6.0
                        {
                            self.tabs
                                .get_mut(&owner)
                                .unwrap()
                                .drag
                                .as_mut()
                                .unwrap()
                                .moved = true;
                            let mut order = self.tabs[&owner].tabs.clone();
                            let target = order
                                .iter()
                                .copied()
                                .filter(|id| {
                                    self.runtime.tree().parent(*id) == Some(self.tabs[&owner].strip)
                                })
                                .find(|id| {
                                    self.runtime
                                        .bounds(*id)
                                        .is_some_and(|r| r.contains(position.x, position.y))
                                });
                            if let Some(target) = target.filter(|id| *id != drag.tab) {
                                let from = order.iter().position(|id| *id == drag.tab).unwrap();
                                let to = order.iter().position(|id| *id == target).unwrap();
                                order.remove(from);
                                order.insert(to, drag.tab);
                                self.reorder_tabs(owner, &order)?;
                            }
                        }
                        return Ok(true);
                    }
                    InputEvent::PointerUp {
                        position,
                        button: PointerButton::Primary,
                        ..
                    } => {
                        let allowed = self.route_tab_input(event)?;
                        self.cancel_tab_drag()?;
                        if allowed
                            && !drag.moved
                            && self.runtime.hit_test(*position) == Some(drag.tab)
                        {
                            self.activate(drag.tab)?;
                        }
                        return Ok(true);
                    }
                    InputEvent::KeyDown {
                        key: Key::Escape, ..
                    }
                    | InputEvent::WindowFocused(false) => {
                        self.cancel_tab_drag()?;
                    }
                    _ => {}
                }
            }
        }
        match event {
            InputEvent::PointerDown {
                position,
                button: PointerButton::Primary,
                ..
            } => {
                let Some(tab) = self.runtime.hit_test(*position).filter(|id| {
                    matches!(
                        self.controls.get(id).map(|c| &c.kind),
                        Some(Kind::Tab { .. })
                    ) && self.is_enabled(*id)
                }) else {
                    return Ok(false);
                };
                let owner = self.tab_owner(tab).unwrap();
                // Overflow entries select normally; dragging only rearranges the visible strip.
                if self.runtime.tree().parent(tab) != Some(self.tabs[&owner].strip) {
                    return Ok(false);
                }
                self.popup_pointer_down(*position)?;
                if self.route_tab_input(event)? && self.is_enabled(tab) {
                    self.runtime.apply(Mutation::CapturePointer(Some(tab)))?;
                    self.tabs.get_mut(&owner).unwrap().drag = Some(TabDrag {
                        tab,
                        start: *position,
                        moved: false,
                    });
                }
                Ok(true)
            }
            InputEvent::KeyDown {
                key,
                modifiers,
                repeat: false,
            } if modifiers.control
                && modifiers.shift
                && !modifiers.alt
                && !modifiers.super_key
                && matches!(key, Key::ArrowLeft | Key::ArrowRight) =>
            {
                let Some(tab) = self.runtime.focused().filter(|id| {
                    matches!(
                        self.controls.get(id).map(|c| &c.kind),
                        Some(Kind::Tab { .. })
                    ) && self.is_enabled(*id)
                }) else {
                    return Ok(false);
                };
                if !self.route_tab_input(event)? {
                    return Ok(true);
                }
                let owner = self.tab_owner(tab).unwrap();
                let mut order = self.tabs[&owner].tabs.clone();
                let from = order.iter().position(|id| *id == tab).unwrap();
                let to = if *key == Key::ArrowLeft {
                    from.saturating_sub(1)
                } else {
                    (from + 1).min(order.len() - 1)
                };
                order.swap(from, to);
                self.reorder_tabs(owner, &order)?;
                Ok(true)
            }
            _ => Ok(false),
        }
    }
}

#[cfg(test)]
mod navigation_tests {
    use super::*;
    use rust_desktop_ui_core::Modifiers;
    fn fixture(width: f32) -> (Controls, TabbedGroup) {
        let mut ui = Controls::new(Theme::FluentLight).unwrap();
        let mut disabled = GroupTab::new("Unavailable");
        disabled.enabled = false;
        let group = ui
            .tabbed_group(
                ui.root(),
                vec![
                    GroupTab::new("First page"),
                    disabled,
                    GroupTab::new("Third page"),
                    GroupTab::new("Last page"),
                ],
                0,
                LayoutStyle {
                    width: Length::Px(width),
                    ..Default::default()
                },
            )
            .unwrap();
        (ui, group)
    }
    fn frame(ui: &mut Controls) {
        ui.update(Size::new(800.0, 500.0)).unwrap();
    }
    #[test]
    fn icon_tab_reserves_semibold_caption_width_before_and_after_selection() {
        let mut ui = Controls::new(Theme::FluentLight).unwrap();
        let caption = "Документ и диалоги";
        let mut first = GroupTab::new(caption);
        first.icon = Some(IconSymbol::Edit);
        let group = ui
            .tabbed_group(
                ui.root(),
                vec![first, GroupTab::new("Other")],
                0,
                LayoutStyle {
                    width: Length::Px(640.0),
                    ..Default::default()
                },
            )
            .unwrap();
        frame(&mut ui);
        let width = ui.runtime.bounds(group.headers[0]).unwrap().width;
        let painted = ui
            .measure_styled_text(
                caption,
                4096.0,
                TextStyle {
                    font_weight: Some(600),
                    wrap: false,
                    ..Default::default()
                },
            )
            .unwrap();
        assert!(width - 32.0 - 24.0 >= painted.width);
        ui.select_tab_id(group.container, group.headers[1]).unwrap();
        frame(&mut ui);
        assert_eq!(ui.runtime.bounds(group.headers[0]).unwrap().width, width);
    }
    #[test]
    fn overflow_moves_retained_headers_and_selection_is_revealed_without_losing_editor() {
        let (mut ui, group) = fixture(180.0);
        let editor = ui
            .edit_box(
                group.contents[0],
                "Draft",
                "original",
                LayoutStyle::default(),
            )
            .unwrap();
        frame(&mut ui);
        ui.runtime.apply(Mutation::Focus(Some(editor))).unwrap();
        ui.text_input(" draft").unwrap();
        let draft = ui.document(editor).unwrap().text().to_owned();
        let overflow = ui.tab_overflow_button(group.container).unwrap();
        assert!(ui.is_enabled(overflow));
        assert!(!ui.is_enabled(group.headers[3]));
        ui.activate(overflow).unwrap();
        frame(&mut ui);
        assert!(ui.is_enabled(group.headers[3]));
        ui.activate(group.headers[3]).unwrap();
        frame(&mut ui);
        assert_eq!(ui.selected_tab_id(group.container), Some(group.headers[3]));
        assert_eq!(
            ui.runtime.tree().parent(group.headers[3]),
            Some(ui.tabs[&group.container].strip)
        );
        assert!(!ui.flyout_is_open(ui.tabs[&group.container].flyout));
        ui.select_tab_id(group.container, group.headers[0]).unwrap();
        frame(&mut ui);
        assert_eq!(ui.document(editor).unwrap().text(), draft);
        assert!(
            ui.semantics()
                .iter()
                .any(|node| node.id == group.headers[0] && node.selected)
        );
    }
    #[test]
    fn reordered_headers_keep_selection_identity_page_draft_and_atomic_validation() {
        let (mut ui, group) = fixture(600.0);
        let editor = ui
            .edit_box(group.contents[0], "Draft", "kept", LayoutStyle::default())
            .unwrap();
        frame(&mut ui);
        let order = [
            group.headers[3],
            group.headers[2],
            group.headers[1],
            group.headers[0],
        ];
        ui.reorder_tabs(group.container, &order).unwrap();
        frame(&mut ui);
        assert_eq!(ui.selected_tab_id(group.container), Some(group.headers[0]));
        assert_eq!(ui.selected_tab(group.container), Some(3));
        assert_eq!(ui.tab_order(group.container), Some(order.as_slice()));
        assert_eq!(ui.document(editor).unwrap().text(), "kept");
        assert!(ui.runtime.bounds(editor).is_some());
        assert!(
            ui.reorder_tabs(group.container, &[group.headers[0]; 4])
                .is_err()
        );
        assert_eq!(ui.tab_order(group.container), Some(order.as_slice()));
        assert!(ui.select_tab_id(group.container, group.headers[1]).is_err());
        ui.select_tab_id(group.container, group.headers[2]).unwrap();
        assert_eq!(ui.selected_tab(group.container), Some(1));
    }
    #[test]
    fn keyboard_overflow_navigation_and_reorder_skip_disabled_and_preserve_focus() {
        let (mut ui, group) = fixture(200.0);
        frame(&mut ui);
        ui.runtime
            .apply(Mutation::Focus(Some(group.headers[0])))
            .unwrap();
        ui.dispatch(InputEvent::KeyDown {
            key: Key::ArrowRight,
            modifiers: Modifiers::default(),
            repeat: false,
        })
        .unwrap();
        frame(&mut ui);
        assert_eq!(ui.selected_tab_id(group.container), Some(group.headers[2]));
        assert_eq!(ui.runtime.focused(), Some(group.headers[2]));
        ui.dispatch(InputEvent::KeyDown {
            key: Key::ArrowRight,
            modifiers: Modifiers {
                control: true,
                shift: true,
                ..Default::default()
            },
            repeat: false,
        })
        .unwrap();
        frame(&mut ui);
        assert_eq!(ui.tab_order(group.container).unwrap()[3], group.headers[2]);
        assert_eq!(ui.selected_tab(group.container), Some(3));
        assert_eq!(ui.runtime.focused(), Some(group.headers[2]));
    }
    #[test]
    fn pointer_drag_reorders_without_activating_and_cancel_late_release_is_safe() {
        let (mut ui, group) = fixture(650.0);
        frame(&mut ui);
        let center = |r: Rect| Point::new(r.x + r.width / 2.0, r.y + r.height / 2.0);
        let from = center(ui.runtime.bounds(group.headers[0]).unwrap());
        let to = center(ui.runtime.bounds(group.headers[2]).unwrap());
        ui.dispatch(InputEvent::PointerDown {
            position: from,
            button: PointerButton::Primary,
            modifiers: Modifiers::default(),
        })
        .unwrap();
        ui.dispatch(InputEvent::PointerMoved { position: to })
            .unwrap();
        frame(&mut ui);
        ui.dispatch(InputEvent::PointerUp {
            position: to,
            button: PointerButton::Primary,
            modifiers: Modifiers::default(),
        })
        .unwrap();
        frame(&mut ui);
        assert_eq!(ui.tab_order(group.container).unwrap()[2], group.headers[0]);
        assert_eq!(ui.selected_tab_id(group.container), Some(group.headers[0]));
        assert!(ui.drain_activated().is_empty());
        let pos = center(ui.runtime.bounds(group.headers[0]).unwrap());
        ui.dispatch(InputEvent::PointerDown {
            position: pos,
            button: PointerButton::Primary,
            modifiers: Modifiers::default(),
        })
        .unwrap();
        ui.cancel_pointer_interaction().unwrap();
        ui.dispatch(InputEvent::PointerUp {
            position: pos,
            button: PointerButton::Primary,
            modifiers: Modifiers::default(),
        })
        .unwrap();
        assert!(ui.drain_activated().is_empty());
        assert!(ui.runtime.captured().is_none());
    }
    #[test]
    fn noninteractive_header_template_retains_semantic_identity_across_overflow() {
        let (mut ui, group) = fixture(600.0);
        let template = ui
            .label(ui.root(), "Template visual", LayoutStyle::default())
            .unwrap();
        ui.set_tab_header_content(group.headers[2], Some(template))
            .unwrap();
        frame(&mut ui);
        assert_eq!(ui.button_content(group.headers[2]), Some(template));
        assert!(
            ui.semantics().iter().any(|n| n.id == group.headers[2]
                && n.role == Role::Tab
                && n.label == "Third page")
        );
        ui.edit_props(group.container, |p| p.style.width = Length::Px(170.0))
            .unwrap();
        frame(&mut ui);
        ui.select_tab_id(group.container, group.headers[2]).unwrap();
        frame(&mut ui);
        assert_eq!(ui.button_content(group.headers[2]), Some(template));
        assert_eq!(ui.runtime.tree().parent(template), Some(group.headers[2]));
        ui.set_tab_header_content(group.headers[2], None).unwrap();
        assert!(ui.runtime.tree().contains(template));
        assert_eq!(ui.runtime.tree().parent(template), None);
    }
}
#[cfg(test)]
mod header_edge_tests {
    use super::*;
    use rust_desktop_ui_core::{EventPhase, Modifiers};
    #[test]
    fn header_order_recovers_after_overflow_and_preview_can_cancel_a_drag() {
        let mut ui = Controls::new(Theme::FluentLight).unwrap();
        let group = ui
            .tabbed_group(
                ui.root(),
                vec![
                    GroupTab::new("First"),
                    GroupTab::new("Second"),
                    GroupTab::new("Third"),
                ],
                2,
                LayoutStyle::default(),
            )
            .unwrap();
        for width in [500.0, 130.0, 500.0] {
            ui.update(Size::new(width, 300.0)).unwrap();
        }
        assert_eq!(
            ui.runtime
                .tree()
                .children(ui.tabs[&group.container].strip)
                .unwrap(),
            group.headers
        );
        ui.runtime
            .on(ui.root(), |event, context| {
                if event.phase == EventPhase::Capture {
                    context.prevent_default();
                }
            })
            .unwrap();
        let a = ui.runtime.bounds(group.headers[0]).unwrap();
        let b = ui.runtime.bounds(group.headers[2]).unwrap();
        ui.dispatch(InputEvent::PointerDown {
            position: Point::new(a.x + 10.0, a.y + 10.0),
            button: PointerButton::Primary,
            modifiers: Modifiers::default(),
        })
        .unwrap();
        ui.dispatch(InputEvent::PointerMoved {
            position: Point::new(b.x + 10.0, b.y + 10.0),
        })
        .unwrap();
        assert_eq!(ui.runtime.captured(), None);
        assert_eq!(
            ui.tab_order(group.container),
            Some(group.headers.as_slice())
        );
        assert_eq!(ui.selected_tab_id(group.container), Some(group.headers[2]));
    }
}
