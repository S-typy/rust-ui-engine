//! Edge rails and transient retained-pane chrome.
use super::*;

pub(super) struct RailChrome {
    pub root: WidgetId,
    next: WidgetId,
}
pub(super) struct AutoHideChrome {
    root: WidgetId,
    title: WidgetId,
    pin: WidgetId,
    close: WidgetId,
    float: WidgetId,
    caption_bounds: Rect,
}

impl DockView {
    pub(super) fn prepare_auto_hide_rails(
        &mut self,
        ui: &mut Controls,
        geometry: &DockGeometry,
    ) -> Result<(), ControlError> {
        for rail in &geometry.auto_hide_rails {
            if let std::collections::hash_map::Entry::Vacant(entry) = self.rails.entry(rail.edge) {
                let root = ui.transparent_panel(
                    self.root,
                    LayoutStyle {
                        kind: LayoutKind::Overlay,
                        ..Default::default()
                    },
                )?;
                ui.set_role(root, Role::TabList);
                ui.set_automation_name(root, "Автоматически скрываемые панели")?;
                let next = icon_button(
                    ui,
                    root,
                    IconSymbol::ChevronDown,
                    "Следующая скрываемая панель",
                )?;
                entry.insert(RailChrome { root, next });
            }
        }
        Ok(())
    }
    /// Paint after pinned pane contents and before the revealed pane's contents.
    /// Header controls are part of Controls::overlay_scene.
    pub fn auto_hide_background_scene(&self) -> &Scene {
        &self.auto_hide_background
    }
    pub fn auto_hide_caption_widget(&self) -> Option<WidgetId> {
        self.geometry.as_ref()?.auto_hide_panel.as_ref()?;
        self.auto_hide_chrome.as_ref().map(|chrome| chrome.title)
    }
    pub(super) fn update_auto_hide(
        &mut self,
        ui: &mut Controls,
        layout: &DockLayout,
        geometry: &DockGeometry,
    ) -> Result<(), ControlError> {
        for (edge, chrome) in &self.rails {
            if !geometry
                .auto_hide_rails
                .iter()
                .any(|rail| &rail.edge == edge)
            {
                set_visible(ui, chrome.root, false)?;
            }
        }
        for rail in &geometry.auto_hide_rails {
            let chrome = &self.rails[&rail.edge];
            set_bounds(
                ui,
                chrome.root,
                rail.bounds,
                Point::new(geometry.bounds.x, geometry.bounds.y),
            )?;
            set_visible(ui, chrome.root, true)?;
            let horizontal = matches!(rail.edge, DockEdge::Top | DockEdge::Bottom);
            let length = if horizontal {
                rail.bounds.width
            } else {
                rail.bounds.height
            };
            let ideal = if horizontal { 120.0 } else { 36.0 };
            let overflow = length < ideal * rail.panes.len() as f32;
            let next = overflow && rail.panes.len() > 1 && length >= 48.0;
            let available = (length - if next { 28.0 } else { 0.0 }).max(0.0);
            let count = ((available / ideal).floor().max(1.0) as usize).min(rail.panes.len());
            let active = geometry
                .auto_hide_panel
                .as_ref()
                .and_then(|panel| rail.panes.iter().position(|pane| pane == &panel.pane))
                .or_else(|| {
                    rail.panes
                        .iter()
                        .position(|pane| ui.runtime.focused() == self.tabs.get(pane).copied())
                })
                .unwrap_or(0);
            let start = active.saturating_sub(count.saturating_sub(1));
            let end = (start + count).min(rail.panes.len());
            set_visible(ui, chrome.next, next)?;
            if next {
                let bounds = if horizontal {
                    Rect::new(length - 28.0, 0.0, 28.0, rail.bounds.height)
                } else {
                    Rect::new(0.0, length - 28.0, rail.bounds.width, 28.0)
                };
                set_bounds(ui, chrome.next, bounds, Point::ZERO)?;
                self.actions
                    .insert(chrome.next, Action::NextRail(rail.edge));
            }
            let mut offset = 0.0;
            for (index, pane) in rail.panes.iter().enumerate() {
                let widget = self.tabs[pane];
                if ui.runtime.tree().parent(widget) != Some(chrome.root) {
                    ui.runtime.apply(Mutation::Reparent {
                        node: widget,
                        parent: chrome.root,
                    })?;
                }
                let visible = (start..end).contains(&index) && available > 0.0;
                set_visible(ui, widget, visible)?;
                if !visible {
                    continue;
                }
                let length = ideal.min(available / count as f32);
                let bounds = if horizontal {
                    Rect::new(offset, 0.0, length, rail.bounds.height)
                } else {
                    Rect::new(0.0, offset, rail.bounds.width, length)
                };
                let selected = geometry
                    .auto_hide_panel
                    .as_ref()
                    .is_some_and(|panel| &panel.pane == pane);
                let definition = layout.pane(pane).expect("validated pane");
                let title = if definition.title.is_empty() {
                    pane.as_str()
                } else {
                    &definition.title
                };
                let label = if horizontal {
                    fit_title(ui, title, (bounds.width - 16.0).max(0.0), selected)?
                } else {
                    let document = TextDocument::new(title);
                    title[..document.grapheme_boundary(1).unwrap_or(title.len())].to_owned()
                };
                ui.set_label(widget, label)?;
                ui.set_checked(widget, selected)?;
                set_bounds(ui, widget, bounds, Point::ZERO)?;
                self.tab_bounds.insert(
                    pane.clone(),
                    Rect::new(
                        rail.bounds.x + bounds.x,
                        rail.bounds.y + bounds.y,
                        bounds.width,
                        bounds.height,
                    ),
                );
                self.actions
                    .insert(widget, Action::Request(DockRequest::Activate(pane.clone())));
                offset += length;
            }
        }
        self.auto_hide_background.clear();
        let Some(panel) = &geometry.auto_hide_panel else {
            if let Some(chrome) = &self.auto_hide_chrome {
                set_visible(ui, chrome.root, false)?;
            }
            return Ok(());
        };
        if self.auto_hide_chrome.is_none() {
            let root = ui.transparent_panel(
                self.root,
                LayoutStyle {
                    kind: LayoutKind::Overlay,
                    ..Default::default()
                },
            )?;
            ui.set_overlay_surface(root)?;
            edit_props(ui, root, |props| {
                props.z_index = 500;
                props.clip = true;
            })?;
            ui.set_role(root, Role::Group);
            ui.set_automation_name(root, "Раскрытая панель")?;
            let title = ui.button(root, "", None, LayoutStyle::default())?;
            ui.set_button_chrome(title, ButtonChrome::Flat)?;
            ui.set_button_style(
                title,
                ButtonStyle {
                    padding: Some(Edges::all(6.0)),
                    ..Default::default()
                },
            )?;
            let pin = icon_button(ui, root, IconSymbol::Pin, "Закрепить панель")?;
            let close = icon_button(ui, root, IconSymbol::Close, "Закрыть панель")?;
            let float = icon_button(ui, root, IconSymbol::Copy, "Отделить панель в окно")?;
            self.auto_hide_chrome = Some(AutoHideChrome {
                root,
                title,
                pin,
                close,
                float,
                caption_bounds: Rect::new(0.0, 0.0, 0.0, 0.0),
            });
        }
        let chrome = self.auto_hide_chrome.as_mut().expect("created chrome");
        set_visible(ui, chrome.root, true)?;
        set_bounds(
            ui,
            chrome.root,
            panel.header,
            Point::new(geometry.bounds.x, geometry.bounds.y),
        )?;
        let pane = layout.pane(&panel.pane).expect("validated pane");
        let mut right = panel.header.width;
        for (widget, visible, request) in [
            (
                chrome.close,
                pane.closable,
                DockRequest::Close(panel.pane.clone()),
            ),
            (
                chrome.pin,
                true,
                DockRequest::SetAutoHide {
                    pane: panel.pane.clone(),
                    settings: None,
                },
            ),
            (chrome.float, true, DockRequest::Float(panel.pane.clone())),
        ] {
            let visible = visible && right >= 52.0;
            set_visible(ui, widget, visible)?;
            if visible {
                right -= 26.0;
                set_bounds(
                    ui,
                    widget,
                    Rect::new(right, 0.0, 26.0, panel.header.height),
                    Point::ZERO,
                )?;
                self.actions.insert(widget, Action::Request(request));
            }
        }
        let label = fit_title(ui, &pane.title, (right - 12.0).max(0.0), true)?;
        ui.set_label(chrome.title, label)?;
        ui.set_automation_name(chrome.title, &pane.title)?;
        set_bounds(
            ui,
            chrome.title,
            Rect::new(0.0, 0.0, right, panel.header.height),
            Point::ZERO,
        )?;
        chrome.caption_bounds =
            Rect::new(panel.header.x, panel.header.y, right, panel.header.height);
        self.actions.insert(
            chrome.title,
            Action::Request(DockRequest::Activate(panel.pane.clone())),
        );
        let tokens = ui.theme().chrome();
        self.auto_hide_background
            .fill(panel.bounds, tokens.card_surface);
        self.auto_hide_background.rounded_stroke(
            panel.bounds,
            panel.bounds,
            0.0,
            1.0,
            tokens.surface_border,
        );
        let mut header = Scene::default();
        header.fill(panel.header, tokens.ribbon_surface);
        header.fill(
            Rect::new(
                panel.header.x,
                panel.header.y + panel.header.height - 1.0,
                panel.header.width,
                1.0,
            ),
            tokens.divider,
        );
        ui.set_custom_scene(chrome.root, header)?;
        Ok(())
    }
    pub(super) fn drag_handle_at(&self, position: Point) -> Option<(DockPaneId, WidgetId)> {
        if let Some(panel) = self
            .geometry
            .as_ref()
            .and_then(|geometry| geometry.auto_hide_panel.as_ref())
            && panel.bounds.contains(position.x, position.y)
        {
            let chrome = self.auto_hide_chrome.as_ref()?;
            return chrome
                .caption_bounds
                .contains(position.x, position.y)
                .then_some((panel.pane.clone(), chrome.title));
        }
        self.tab_bounds
            .iter()
            .find(|(_, bounds)| bounds.contains(position.x, position.y))
            .map(|(pane, _)| (pane.clone(), self.tabs[pane]))
    }
    pub(super) fn next_rail_request(&self, edge: DockEdge) -> Option<DockRequest> {
        let geometry = self.geometry.as_ref()?;
        let rail = geometry
            .auto_hide_rails
            .iter()
            .find(|rail| rail.edge == edge)?;
        let current = geometry
            .auto_hide_panel
            .as_ref()
            .and_then(|panel| rail.panes.iter().position(|pane| pane == &panel.pane));
        let next = current.map_or(0, |index| (index + 1) % rail.panes.len());
        Some(DockRequest::Activate(rail.panes[next].clone()))
    }
    pub(super) fn auto_hide_input(
        &mut self,
        ui: &mut Controls,
        event: &InputEvent,
        layout: &DockLayout,
    ) -> Result<Option<(bool, Vec<DockRequest>)>, ControlError> {
        if layout.revealed_auto_hide(self.window).is_some() {
            let dismiss = match event {
                InputEvent::WindowFocused(false) => Some(false),
                InputEvent::KeyDown {
                    key: Key::Escape, ..
                } if self.gesture.is_none() => Some(true),
                InputEvent::PointerDown { position, .. } if self.gesture.is_none() => {
                    self.geometry.as_ref().and_then(|geometry| {
                        let inside = geometry
                            .auto_hide_panel
                            .as_ref()
                            .is_some_and(|panel| panel.bounds.contains(position.x, position.y))
                            || geometry
                                .auto_hide_rails
                                .iter()
                                .any(|rail| rail.bounds.contains(position.x, position.y));
                        (!inside).then_some(false)
                    })
                }
                _ => None,
            };
            if let Some(restore_focus) = dismiss {
                if restore_focus
                    && ui.runtime.focused().is_some_and(|id| {
                        ui.document(id)
                            .is_some_and(|document| document.composition().is_some())
                    })
                {
                    ui.cancel_preedit();
                    return Ok(Some((true, Vec::new())));
                }
                let mut closed = layout.clone();
                closed.dismiss_auto_hide(self.window).map_err(model_error)?;
                let (consumed, mut requests) = if restore_focus {
                    (true, Vec::new())
                } else {
                    self.handle_input(ui, event, &closed)?
                };
                requests.insert(
                    0,
                    DockRequest::DismissAutoHide {
                        window: self.window,
                        restore_focus,
                    },
                );
                return Ok(Some((consumed, requests)));
            }
        }
        if let InputEvent::KeyDown {
            key,
            modifiers,
            repeat,
        } = event
            && *modifiers == Modifiers::default()
            && let Some(focused) = ui.runtime.focused()
            && let Some((pane, _)) = self.tabs.iter().find(|(_, widget)| **widget == focused)
            && let Some(rail) = self.geometry.as_ref().and_then(|geometry| {
                geometry
                    .auto_hide_rails
                    .iter()
                    .find(|rail| rail.panes.contains(pane))
            })
        {
            let index = rail
                .panes
                .iter()
                .position(|id| id == pane)
                .expect("known pane");
            let next = match key {
                Key::ArrowLeft | Key::ArrowUp => {
                    Some((index + rail.panes.len() - 1) % rail.panes.len())
                }
                Key::ArrowRight | Key::ArrowDown => Some((index + 1) % rail.panes.len()),
                Key::Home => Some(0),
                Key::End => Some(rail.panes.len() - 1),
                Key::Enter | Key::Space if !repeat => Some(index),
                _ => None,
            };
            if let Some(index) = next {
                let pane = rail.panes[index].clone();
                self.pending_tab_focus = Some(pane.clone());
                return Ok(Some((true, vec![DockRequest::Activate(pane)])));
            }
        }
        Ok(None)
    }
}

pub(super) fn nearest_edge(group: Rect, bounds: Rect) -> DockEdge {
    [
        (DockEdge::Left, (group.x - bounds.x).abs()),
        (
            DockEdge::Right,
            (bounds.x + bounds.width - group.x - group.width).abs(),
        ),
        (DockEdge::Top, (group.y - bounds.y).abs()),
        (
            DockEdge::Bottom,
            (bounds.y + bounds.height - group.y - group.height).abs(),
        ),
    ]
    .into_iter()
    .min_by(|a, b| a.1.total_cmp(&b.1))
    .map_or(DockEdge::Left, |item| item.0)
}
