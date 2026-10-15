//! Retained docking chrome and pointer/keyboard interaction.

use super::model::*;
use crate::{
    ButtonChrome, ButtonContentLayout, ButtonIcon, ButtonStyle, ControlError, Controls, IconSymbol,
    Role, TextStyle,
};
use rust_desktop_ui_core::{
    Axis, Color, Edges, InputEvent, Key, LayoutKind, LayoutStyle, Length, Modifiers, Mutation,
    Point, PointerButton, Rect, Scene, WidgetId,
};
use rust_desktop_ui_text::TextDocument;
use std::collections::{HashMap, HashSet};

/// A view request. The application applies it to its shared docking model and
/// creates or closes native windows only after a successful model mutation.
#[derive(Clone, Debug, PartialEq)]
pub enum DockRequest {
    Activate(DockPaneId),
    Close(DockPaneId),
    Float(DockPaneId),
    RedockWindow(DockWindowId),
    Move {
        pane: DockPaneId,
        placement: DockPlacement,
    },
    Resize {
        split: DockSplitId,
        ratio: f32,
    },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DockDropPreview {
    pub placement: DockPlacement,
    pub bounds: Rect,
}

/// A captured tab gesture. `dragging` becomes true only after the five-pixel
/// threshold; a simple press must not move a pane to another native window.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DockTabDrag {
    pub pane: DockPaneId,
    pub widget: WidgetId,
    pub dragging: bool,
}

/// Immutable window-local drop geometry, independent of a Controls arena.
/// Refresh this projection after layout changes and validate the live model
/// before committing a placement calculated from a previously stored snapshot.
#[derive(Clone, Debug)]
pub struct DockDropTargets {
    window: DockWindowId,
    bounds: Rect,
    groups: Vec<DockGroupGeometry>,
    tab_bounds: HashMap<DockPaneId, Rect>,
}

impl DockDropTargets {
    pub fn window(&self) -> DockWindowId {
        self.window
    }
    pub fn bounds(&self) -> Rect {
        self.bounds
    }
    /// Accepts any pane, including one owned by another native window.
    /// Positions and preview bounds use logical coordinates of this window.
    pub fn target_at(&self, position: Point) -> Option<DockDropPreview> {
        drop_target(
            self.window,
            self.bounds,
            &self.groups,
            &self.tab_bounds,
            position,
        )
    }
}

struct GroupChrome {
    header: WidgetId,
    close: WidgetId,
    float: WidgetId,
    redock: WidgetId,
    next: WidgetId,
}

#[derive(Clone)]
enum Action {
    Request(DockRequest),
    Next(DockGroupId),
}

enum Gesture {
    Tab {
        pane: DockPaneId,
        widget: WidgetId,
        start: Point,
        dragging: bool,
    },
    Split {
        id: DockSplitId,
        widget: WidgetId,
        original: f32,
        axis: Axis,
        area: Rect,
        grab: f32,
    },
}

impl Gesture {
    fn widget(&self) -> WidgetId {
        match self {
            Self::Tab { widget, .. } | Self::Split { widget, .. } => *widget,
        }
    }
}

/// One native window's docking chrome. Pane content is composed separately by
/// the application; changing its placement never recreates a content control.
pub struct DockView {
    root: WidgetId,
    window: DockWindowId,
    geometry: Option<DockGeometry>,
    groups: HashMap<DockGroupId, GroupChrome>,
    tabs: HashMap<DockPaneId, WidgetId>,
    splitters: HashMap<DockSplitId, WidgetId>,
    tab_bounds: HashMap<DockPaneId, Rect>,
    actions: HashMap<WidgetId, Action>,
    gesture: Option<Gesture>,
    preview: Option<DockDropPreview>,
    overlay: Scene,
    preview_accent: Color,
    pending_tab_focus: Option<DockPaneId>,
}

impl DockView {
    pub fn new(
        ui: &mut Controls,
        parent: WidgetId,
        window: DockWindowId,
    ) -> Result<Self, ControlError> {
        let root = ui.transparent_panel(
            parent,
            LayoutStyle {
                kind: LayoutKind::Overlay,
                width: Length::Px(0.0),
                height: Length::Px(0.0),
                ..Default::default()
            },
        )?;
        edit_props(ui, root, |props| {
            props.clip = true;
            props.hit_test = false;
        })?;
        ui.set_automation_name(root, "Рабочая область панелей")?;
        Ok(Self {
            root,
            window,
            geometry: None,
            groups: HashMap::new(),
            tabs: HashMap::new(),
            splitters: HashMap::new(),
            tab_bounds: HashMap::new(),
            actions: HashMap::new(),
            gesture: None,
            preview: None,
            overlay: Scene::default(),
            preview_accent: ui.theme().tokens().focus,
            pending_tab_focus: None,
        })
    }

    pub fn root(&self) -> WidgetId {
        self.root
    }
    pub fn window(&self) -> DockWindowId {
        self.window
    }
    pub fn geometry(&self) -> Option<&DockGeometry> {
        self.geometry.as_ref()
    }
    pub fn overlay_scene(&self) -> &Scene {
        &self.overlay
    }
    pub fn drop_preview(&self) -> Option<DockDropPreview> {
        self.preview
    }
    pub fn is_interacting(&self) -> bool {
        self.gesture.is_some()
    }
    /// Read after routing the current event through `handle_input`, which
    /// discards gestures whose capture was lost. Splitter gestures return None.
    pub fn tab_drag(&self) -> Option<DockTabDrag> {
        match &self.gesture {
            Some(Gesture::Tab {
                pane,
                widget,
                dragging,
                ..
            }) => Some(DockTabDrag {
                pane: pane.clone(),
                widget: *widget,
                dragging: *dragging,
            }),
            _ => None,
        }
    }
    /// Copy the latest geometry for a coordinator that cannot borrow this view
    /// while routing input through another native window.
    pub fn drop_targets(&self) -> Option<DockDropTargets> {
        let geometry = self.geometry.as_ref()?;
        Some(DockDropTargets {
            window: self.window,
            bounds: geometry.bounds,
            groups: geometry.groups.clone(),
            tab_bounds: self.tab_bounds.clone(),
        })
    }
    pub fn drop_target_at(&self, position: Point) -> Option<DockDropPreview> {
        let geometry = self.geometry.as_ref()?;
        drop_target(
            self.window,
            geometry.bounds,
            &geometry.groups,
            &self.tab_bounds,
            position,
        )
    }
    /// Replace only the painted drop preview, without focus, capture or model
    /// changes. `None` also hides a source window's local preview during a drag.
    /// Invalid or stale geometry is rejected without replacing the old preview.
    pub fn set_external_preview(
        &mut self,
        preview: Option<DockDropPreview>,
    ) -> Result<(), ControlError> {
        if preview.is_some_and(|preview| !self.valid_preview(preview)) {
            return Err(ControlError::Invalid(
                "Drop preview does not match this window's current geometry".into(),
            ));
        }
        self.preview = preview;
        self.paint_preview(self.preview_accent);
        Ok(())
    }
    /// End only a tab gesture, releasing its capture and clearing its preview.
    /// Returns the pane only after the drag threshold and while capture still
    /// belongs to this tab. Never emits Move/Float; the coordinator owns commit.
    /// A splitter gesture and another widget's capture are left untouched.
    pub fn finish_tab_drag(
        &mut self,
        ui: &mut Controls,
    ) -> Result<Option<DockPaneId>, ControlError> {
        if !matches!(self.gesture, Some(Gesture::Tab { .. })) {
            return Ok(None);
        }
        let Some(Gesture::Tab {
            pane,
            widget,
            dragging,
            ..
        }) = self.gesture.take()
        else {
            unreachable!("matched tab gesture");
        };
        let captured = ui.runtime.captured() == Some(widget);
        if captured {
            ui.runtime.apply(Mutation::CapturePointer(None))?;
        }
        self.pending_tab_focus = None;
        self.preview = None;
        self.overlay.clear();
        Ok((dragging && captured).then_some(pane))
    }
    pub fn tab_widget(&self, pane: &DockPaneId) -> Option<WidgetId> {
        self.tabs.get(pane).copied()
    }
    pub fn splitter_widget(&self, split: DockSplitId) -> Option<WidgetId> {
        self.splitters.get(&split).copied()
    }

    /// Bounds and returned geometry are absolute window-local logical pixels.
    /// The caller performs its normal Controls::update after this method.
    pub fn update(
        &mut self,
        ui: &mut Controls,
        layout: &DockLayout,
        bounds: Rect,
    ) -> Result<DockGeometry, ControlError> {
        let geometry = layout.arrange(self.window, bounds).map_err(model_error)?;
        set_bounds(ui, self.root, bounds, Point::ZERO)?;
        self.actions.clear();
        self.tab_bounds.clear();
        for group in &geometry.groups {
            if !self.groups.contains_key(&group.id) {
                let header = ui.transparent_panel(
                    self.root,
                    LayoutStyle {
                        kind: LayoutKind::Overlay,
                        ..Default::default()
                    },
                )?;
                ui.set_role(header, Role::TabList);
                ui.set_automation_name(header, "Вкладки панелей")?;
                let close = icon_button(ui, header, IconSymbol::Close, "Закрыть панель")?;
                let float = icon_button(ui, header, IconSymbol::Copy, "Отделить панель в окно")?;
                let redock = icon_button(
                    ui,
                    header,
                    IconSymbol::Pin,
                    "Вернуть окно в рабочую область",
                )?;
                let next = icon_button(ui, header, IconSymbol::ChevronDown, "Следующая вкладка")?;
                self.groups.insert(
                    group.id,
                    GroupChrome {
                        header,
                        close,
                        float,
                        redock,
                        next,
                    },
                );
            }
        }
        // Keep tab IDs alive even while a pane is closed or belongs to another
        // native window. Reparent before removing obsolete group headers.
        for pane in layout.panes() {
            let widget = if let Some(widget) = self.tabs.get(&pane.id) {
                *widget
            } else {
                let widget =
                    ui.toggle(self.root, &pane.title, None, false, LayoutStyle::default())?;
                ui.set_role(widget, Role::Tab);
                ui.set_button_chrome(widget, ButtonChrome::Tab)?;
                ui.set_button_style(
                    widget,
                    ButtonStyle {
                        padding: Some(Edges::all(8.0)),
                        corner_radius: Some(3.0),
                        ..Default::default()
                    },
                )?;
                self.tabs.insert(pane.id.clone(), widget);
                widget
            };
            let title = if pane.title.is_empty() {
                pane.id.as_str()
            } else {
                &pane.title
            };
            ui.set_automation_name(widget, title)?;
            ui.set_tooltip(widget, title)?;
            let group = geometry
                .groups
                .iter()
                .find(|group| group.panes.contains(&pane.id));
            let parent = group.map_or(self.root, |group| self.groups[&group.id].header);
            if ui.runtime.tree().parent(widget) != Some(parent) {
                ui.runtime.apply(Mutation::Reparent {
                    node: widget,
                    parent,
                })?;
            }
            if group.is_none() {
                set_visible(ui, widget, false)?;
            }
        }
        let live: HashSet<_> = geometry.groups.iter().map(|group| group.id).collect();
        let removed: Vec<_> = self
            .groups
            .keys()
            .copied()
            .filter(|group| !live.contains(group))
            .collect();
        for id in removed {
            ui.remove(self.groups.remove(&id).expect("known group").header)?;
        }
        for group in &geometry.groups {
            self.update_group(ui, layout, group, bounds)?;
        }
        let live_splits: HashSet<_> = geometry.splitters.iter().map(|split| split.id).collect();
        let removed: Vec<_> = self
            .splitters
            .keys()
            .copied()
            .filter(|split| !live_splits.contains(split))
            .collect();
        for id in removed {
            ui.remove(self.splitters.remove(&id).expect("known split"))?;
        }
        for split in &geometry.splitters {
            let widget = if let Some(widget) = self.splitters.get(&split.id) {
                *widget
            } else {
                let widget = ui.button(self.root, "", None, LayoutStyle::default())?;
                ui.set_role(widget, Role::Separator);
                ui.set_button_chrome(widget, ButtonChrome::Flat)?;
                ui.set_button_style(
                    widget,
                    ButtonStyle {
                        padding: Some(Edges::all(0.0)),
                        ..Default::default()
                    },
                )?;
                ui.set_automation_name(widget, "Изменить размер панелей")?;
                ui.set_tooltip(
                    widget,
                    "Перетащите разделитель или используйте клавиши со стрелками",
                )?;
                self.splitters.insert(split.id, widget);
                widget
            };
            set_bounds(ui, widget, split.bounds, Point::new(bounds.x, bounds.y))?;
            set_visible(
                ui,
                widget,
                split.bounds.width > 0.0 && split.bounds.height > 0.0,
            )?;
        }
        let mut scene = Scene::default();
        let chrome = ui.theme().chrome();
        for group in &geometry.groups {
            scene.rounded_fill(group.bounds, bounds, 5.0, chrome.card_surface);
            scene.rounded_stroke(group.bounds, bounds, 5.0, 1.0, chrome.surface_border);
            scene.fill(group.tab_strip, chrome.ribbon_surface);
            let y = group.tab_strip.y + group.tab_strip.height - 1.0;
            scene.fill(
                Rect::new(group.bounds.x, y, group.bounds.width, 1.0),
                chrome.divider,
            );
        }
        for split in &geometry.splitters {
            let b = split.bounds;
            let mark = match split.axis {
                Axis::Horizontal => Rect::new(
                    b.x + (b.width - 1.0).max(0.0) / 2.0,
                    b.y + b.height * 0.4,
                    1.0_f32.min(b.width),
                    b.height * 0.2,
                ),
                Axis::Vertical => Rect::new(
                    b.x + b.width * 0.4,
                    b.y + (b.height - 1.0).max(0.0) / 2.0,
                    b.width * 0.2,
                    1.0_f32.min(b.height),
                ),
            };
            scene.fill(mark, chrome.divider);
        }
        ui.set_custom_scene(self.root, scene)?;
        self.geometry = Some(geometry.clone());
        self.preview_accent = ui.theme().tokens().focus;
        if self
            .preview
            .is_some_and(|preview| !self.valid_preview(preview))
        {
            self.preview = None;
        }
        self.paint_preview(self.preview_accent);
        if let Some(pane) = self.pending_tab_focus.take()
            && self.tab_bounds.contains_key(&pane)
        {
            ui.runtime.apply(Mutation::Focus(Some(self.tabs[&pane])))?;
        }
        Ok(geometry)
    }

    fn update_group(
        &mut self,
        ui: &mut Controls,
        layout: &DockLayout,
        group: &DockGroupGeometry,
        bounds: Rect,
    ) -> Result<(), ControlError> {
        let chrome = &self.groups[&group.id];
        set_bounds(
            ui,
            chrome.header,
            group.tab_strip,
            Point::new(bounds.x, bounds.y),
        )?;
        let w = group.tab_strip.width;
        let h = group.tab_strip.height;
        let pane = layout.pane(&group.active).expect("validated active pane");
        let close = pane.closable && w >= 72.0;
        let float = w >= 112.0;
        let redock = self.window != DockWindowId::MAIN && w >= 164.0;
        let base = usize::from(close) + usize::from(float) + usize::from(redock);
        let desired: Vec<_> = group
            .panes
            .iter()
            .map(|id| {
                let pane = layout.pane(id).expect("validated pane");
                measure_title(ui, &pane.title, id == &group.active)
                    .map(|width| (width + 24.0).clamp(84.0, 180.0))
            })
            .collect::<Result<_, _>>()?;
        let overflow = desired.iter().sum::<f32>() > (w - 6.0 - base as f32 * 26.0).max(0.0);
        let next = overflow && group.panes.len() > 1 && w >= 72.0;
        let actions = [
            (
                chrome.close,
                close,
                Action::Request(DockRequest::Close(group.active.clone())),
            ),
            (
                chrome.float,
                float,
                Action::Request(DockRequest::Float(group.active.clone())),
            ),
            (
                chrome.redock,
                redock,
                Action::Request(DockRequest::RedockWindow(self.window)),
            ),
            (chrome.next, next, Action::Next(group.id)),
        ];
        let mut right = (w - 3.0).max(0.0);
        for (widget, visible, action) in actions {
            set_visible(ui, widget, visible && h > 0.0)?;
            if visible {
                right = (right - 26.0).max(0.0);
                set_bounds(
                    ui,
                    widget,
                    Rect::new(right, 2.0_f32.min(h), 26.0_f32.min(w), (h - 4.0).max(0.0)),
                    Point::ZERO,
                )?;
                self.actions.insert(widget, action);
            }
        }
        let available = (right - 3.0).max(0.0);
        let active = group
            .panes
            .iter()
            .position(|pane| pane == &group.active)
            .unwrap_or(0);
        let count = if overflow {
            (available / 84.0).floor().max(1.0) as usize
        } else {
            group.panes.len()
        }
        .min(group.panes.len());
        let start = if overflow {
            active.saturating_sub(count - 1)
        } else {
            0
        };
        let end = (start + count).min(group.panes.len());
        for (index, pane) in group.panes.iter().enumerate() {
            if index < start || index >= end {
                set_visible(ui, self.tabs[pane], false)?;
            }
        }
        let mut x = 3.0;
        for (index, desired_width) in desired.iter().enumerate().take(end).skip(start) {
            let pane = &group.panes[index];
            let width = if overflow {
                available / count as f32
            } else {
                *desired_width
            }
            .min((right - x).max(0.0));
            let widget = self.tabs[pane];
            let title = layout
                .pane(pane)
                .map(|pane| {
                    if pane.title.is_empty() {
                        pane.id.as_str()
                    } else {
                        &pane.title
                    }
                })
                .unwrap_or("");
            let label = fit_title(ui, title, (width - 16.0).max(0.0), pane == &group.active)?;
            ui.set_label(widget, label)?;
            ui.set_checked(widget, pane == &group.active)?;
            set_bounds(ui, widget, Rect::new(x, 0.0, width, h), Point::ZERO)?;
            set_visible(ui, widget, width > 0.0 && h > 0.0)?;
            if width > 0.0 && h > 0.0 {
                self.tab_bounds.insert(
                    pane.clone(),
                    Rect::new(group.tab_strip.x + x, group.tab_strip.y, width, h),
                );
                self.actions
                    .insert(widget, Action::Request(DockRequest::Activate(pane.clone())));
            }
            x += width;
        }
        Ok(())
    }

    /// Route before Controls::dispatch. A consumed event must not be dispatched
    /// again. Ordinary chrome buttons use Controls and process_activated.
    pub fn handle_input(
        &mut self,
        ui: &mut Controls,
        event: &InputEvent,
        layout: &DockLayout,
    ) -> Result<(bool, Vec<DockRequest>), ControlError> {
        if matches!(event, InputEvent::WindowFocused(false)) {
            let requests = self.cancel(ui, layout)?;
            return Ok((false, requests));
        }
        if matches!(
            event,
            InputEvent::KeyDown {
                key: Key::Escape,
                ..
            }
        ) && (self.gesture.is_some() || self.preview.is_some())
        {
            return Ok((true, self.cancel(ui, layout)?));
        }
        if self
            .gesture
            .as_ref()
            .is_some_and(|gesture| ui.runtime.captured() != Some(gesture.widget()))
        {
            return Ok((false, self.cancel(ui, layout)?));
        }
        match event {
            InputEvent::PointerDown {
                position,
                button: PointerButton::Primary,
                modifiers,
            } if *modifiers == Modifiers::default() => {
                if let Some((pane, widget)) = self
                    .tab_bounds
                    .iter()
                    .find(|(_, bounds)| bounds.contains(position.x, position.y))
                    .map(|(pane, _)| (pane.clone(), self.tabs[pane]))
                {
                    if !begin_capture(ui, widget)? {
                        return Ok((true, Vec::new()));
                    }
                    self.gesture = Some(Gesture::Tab {
                        pane: pane.clone(),
                        widget,
                        start: *position,
                        dragging: false,
                    });
                    return Ok((true, vec![DockRequest::Activate(pane)]));
                }
                if let Some(split) = self.geometry.as_ref().and_then(|geometry| {
                    geometry
                        .splitters
                        .iter()
                        .find(|split| split.bounds.contains(position.x, position.y))
                }) {
                    let widget = self.splitters[&split.id];
                    let (pointer, center) = match split.axis {
                        Axis::Horizontal => (position.x, split.bounds.x + split.bounds.width / 2.0),
                        Axis::Vertical => (position.y, split.bounds.y + split.bounds.height / 2.0),
                    };
                    if !begin_capture(ui, widget)? {
                        return Ok((true, Vec::new()));
                    }
                    self.gesture = Some(Gesture::Split {
                        id: split.id,
                        widget,
                        original: split.ratio,
                        axis: split.axis,
                        area: split.container,
                        grab: pointer - center,
                    });
                    return Ok((true, Vec::new()));
                }
            }
            InputEvent::PointerMoved { position } if self.gesture.is_some() => {
                if let Some(Gesture::Split {
                    id,
                    axis,
                    area,
                    grab,
                    ..
                }) = self.gesture
                {
                    let (pointer, origin, length) = match axis {
                        Axis::Horizontal => (position.x, area.x, area.width),
                        Axis::Vertical => (position.y, area.y, area.height),
                    };
                    let divider = DOCK_SPLITTER_WIDTH.min(length);
                    let available = length - divider;
                    let requests = if available > 0.0 {
                        vec![DockRequest::Resize {
                            split: id,
                            ratio: ((pointer - origin - divider / 2.0 - grab) / available)
                                .clamp(0.01, 0.99),
                        }]
                    } else {
                        Vec::new()
                    };
                    return Ok((true, requests));
                }
                if let Some(Gesture::Tab {
                    start, dragging, ..
                }) = &mut self.gesture
                {
                    *dragging |=
                        (position.x - start.x).powi(2) + (position.y - start.y).powi(2) >= 25.0;
                    if *dragging {
                        self.preview = self.drop_target_at(*position);
                        self.paint_preview(ui.theme().tokens().focus);
                    }
                }
                return Ok((true, Vec::new()));
            }
            InputEvent::PointerUp {
                position,
                button: PointerButton::Primary,
                ..
            } if self.gesture.is_some() => {
                let gesture = self.gesture.take().expect("matched gesture");
                ui.runtime.apply(Mutation::CapturePointer(None))?;
                self.overlay.clear();
                self.preview = None;
                let requests = match gesture {
                    Gesture::Tab {
                        pane,
                        dragging: true,
                        ..
                    } => {
                        if let Some(target) = self.drop_target_at(*position) {
                            vec![DockRequest::Move {
                                pane,
                                placement: target.placement,
                            }]
                        } else if self.geometry.as_ref().is_some_and(|geometry| {
                            !geometry.bounds.contains(position.x, position.y)
                        }) {
                            vec![DockRequest::Float(pane)]
                        } else {
                            Vec::new()
                        }
                    }
                    _ => Vec::new(),
                };
                return Ok((true, requests));
            }
            InputEvent::KeyDown {
                key,
                modifiers,
                repeat,
            } if *modifiers == Modifiers::default() => {
                let focused = ui.runtime.focused();
                if let Some(pane) = self.tabs.iter().find_map(|(pane, widget)| {
                    (Some(*widget) == focused && self.tab_bounds.contains_key(pane)).then_some(pane)
                }) {
                    if matches!(key, Key::Enter | Key::Space) {
                        return Ok((
                            true,
                            if *repeat {
                                Vec::new()
                            } else {
                                vec![DockRequest::Activate(pane.clone())]
                            },
                        ));
                    }
                    if let Some(group) = self.geometry.as_ref().and_then(|geometry| {
                        geometry
                            .groups
                            .iter()
                            .find(|group| group.panes.contains(pane))
                    }) {
                        let current = group.panes.iter().position(|id| id == pane).unwrap_or(0);
                        let next = match key {
                            Key::ArrowLeft => {
                                Some((current + group.panes.len() - 1) % group.panes.len())
                            }
                            Key::ArrowRight => Some((current + 1) % group.panes.len()),
                            Key::Home => Some(0),
                            Key::End => Some(group.panes.len() - 1),
                            _ => None,
                        };
                        if let Some(next) = next {
                            let pane = group.panes[next].clone();
                            self.pending_tab_focus = Some(pane.clone());
                            return Ok((true, vec![DockRequest::Activate(pane)]));
                        }
                    }
                }
                if let Some(split) = self.geometry.as_ref().and_then(|geometry| {
                    geometry
                        .splitters
                        .iter()
                        .find(|split| self.splitters.get(&split.id).copied() == focused)
                }) {
                    let direction = match (split.axis, key) {
                        (Axis::Horizontal, Key::ArrowLeft) | (Axis::Vertical, Key::ArrowUp) => -1.0,
                        (Axis::Horizontal, Key::ArrowRight) | (Axis::Vertical, Key::ArrowDown) => {
                            1.0
                        }
                        _ => 0.0,
                    };
                    if direction != 0.0 {
                        return Ok((
                            true,
                            vec![DockRequest::Resize {
                                split: split.id,
                                ratio: (split.ratio + direction * 0.02).clamp(0.01, 0.99),
                            }],
                        ));
                    }
                }
            }
            _ => {}
        }
        Ok((false, Vec::new()))
    }

    /// Convert the shell's drained button/accessibility activations into model requests.
    pub fn process_activated(&self, widgets: &[WidgetId], layout: &DockLayout) -> Vec<DockRequest> {
        widgets
            .iter()
            .filter_map(|widget| match self.actions.get(widget)? {
                Action::Request(request) => Some(request.clone()),
                Action::Next(group) => {
                    let group = layout
                        .arrange(self.window, self.geometry.as_ref()?.bounds)
                        .ok()?
                        .groups
                        .into_iter()
                        .find(|item| item.id == *group)?;
                    let active = group.panes.iter().position(|pane| pane == &group.active)?;
                    Some(DockRequest::Activate(
                        group.panes[(active + 1) % group.panes.len()].clone(),
                    ))
                }
            })
            .collect()
    }

    /// Cancel an unfinished gesture. Apply returned resize requests to restore
    /// the ratio from before the drag; tab preview never mutates the layout.
    pub fn cancel_interaction(
        &mut self,
        ui: &mut Controls,
        layout: &DockLayout,
    ) -> Result<Vec<DockRequest>, ControlError> {
        self.cancel(ui, layout)
    }

    fn cancel(
        &mut self,
        ui: &mut Controls,
        layout: &DockLayout,
    ) -> Result<Vec<DockRequest>, ControlError> {
        self.pending_tab_focus = None;
        let gesture = self.gesture.take();
        if gesture
            .as_ref()
            .is_some_and(|gesture| ui.runtime.captured() == Some(gesture.widget()))
        {
            ui.runtime.apply(Mutation::CapturePointer(None))?;
        }
        let requests = if let Some(Gesture::Split { id, original, .. }) = gesture
            && layout
                .arrange(
                    self.window,
                    self.geometry
                        .as_ref()
                        .map_or(Rect::new(0.0, 0.0, 0.0, 0.0), |geometry| geometry.bounds),
                )
                .is_ok_and(|geometry| geometry.splitters.iter().any(|split| split.id == id))
        {
            vec![DockRequest::Resize {
                split: id,
                ratio: original,
            }]
        } else {
            Vec::new()
        };
        self.overlay.clear();
        self.preview = None;
        Ok(requests)
    }

    fn valid_preview(&self, preview: DockDropPreview) -> bool {
        let Some(geometry) = &self.geometry else {
            return false;
        };
        if !preview.bounds.is_valid() || preview.bounds.width <= 0.0 || preview.bounds.height <= 0.0
        {
            return false;
        }
        match preview.placement {
            DockPlacement::Tab { group, index } => geometry.groups.iter().any(|candidate| {
                candidate.id == group
                    && index <= candidate.panes.len()
                    && preview.bounds == candidate.bounds
            }),
            DockPlacement::Split { group, edge } => geometry.groups.iter().any(|candidate| {
                candidate.id == group && preview.bounds == edge_half(candidate.bounds, edge)
            }),
            DockPlacement::Root { window, edge } => {
                window == self.window
                    && preview.bounds
                        == if geometry.groups.is_empty() {
                            geometry.bounds
                        } else {
                            edge_half(geometry.bounds, edge)
                        }
            }
        }
    }

    fn paint_preview(&mut self, accent: Color) {
        self.overlay.clear();
        if let Some(preview) = self.preview {
            self.overlay.rounded_fill(
                preview.bounds,
                preview.bounds,
                5.0,
                Color { a: 0.20, ..accent },
            );
            self.overlay
                .rounded_stroke(preview.bounds, preview.bounds, 5.0, 2.0, accent);
        }
    }
}

fn drop_target(
    window: DockWindowId,
    bounds: Rect,
    groups: &[DockGroupGeometry],
    tab_bounds: &HashMap<DockPaneId, Rect>,
    position: Point,
) -> Option<DockDropPreview> {
    if !position.is_finite()
        || bounds.width <= 0.0
        || bounds.height <= 0.0
        || !bounds.contains(position.x, position.y)
    {
        return None;
    }
    // An empty main workspace is a full-size target even away from its edges.
    if groups.is_empty() {
        return Some(DockDropPreview {
            placement: DockPlacement::Root {
                window,
                edge: DockEdge::Right,
            },
            bounds,
        });
    }
    let group = groups
        .iter()
        .find(|group| group.bounds.contains(position.x, position.y));
    // Tab insertion wins over an outer edge so reordering does not create splits.
    if let Some(group) = group.filter(|group| group.tab_strip.contains(position.x, position.y)) {
        let index = group
            .panes
            .iter()
            .enumerate()
            .find_map(|(index, pane)| {
                tab_bounds
                    .get(pane)
                    .filter(|rect| position.x < rect.x + rect.width / 2.0)
                    .map(|_| index)
            })
            .unwrap_or(group.panes.len());
        return Some(DockDropPreview {
            placement: DockPlacement::Tab {
                group: group.id,
                index,
            },
            bounds: group.bounds,
        });
    }
    if let Some(edge) = edge_at(bounds, position, 18.0) {
        return Some(DockDropPreview {
            placement: DockPlacement::Root { window, edge },
            bounds: edge_half(bounds, edge),
        });
    }
    let group = group?;
    if let Some(edge) = edge_at(group.content, position, 48.0) {
        Some(DockDropPreview {
            placement: DockPlacement::Split {
                group: group.id,
                edge,
            },
            bounds: edge_half(group.bounds, edge),
        })
    } else {
        Some(DockDropPreview {
            placement: DockPlacement::Tab {
                group: group.id,
                index: group.panes.len(),
            },
            bounds: group.bounds,
        })
    }
}

fn begin_capture(ui: &mut Controls, widget: WidgetId) -> Result<bool, ControlError> {
    if !ui.is_enabled(widget) {
        return Ok(false);
    }
    match ui.runtime.apply(Mutation::Focus(Some(widget))) {
        Ok(()) => {}
        Err(rust_desktop_ui_core::RuntimeError::IneligibleFocus(_)) => return Ok(false),
        Err(error) => return Err(error.into()),
    }
    match ui.runtime.apply(Mutation::CapturePointer(Some(widget))) {
        Ok(()) => Ok(true),
        Err(rust_desktop_ui_core::RuntimeError::IneligibleCapture(_)) => Ok(false),
        Err(error) => Err(error.into()),
    }
}

fn icon_button(
    ui: &mut Controls,
    parent: WidgetId,
    symbol: IconSymbol,
    label: &str,
) -> Result<WidgetId, ControlError> {
    let widget = ui.button(parent, "", None, LayoutStyle::default())?;
    ui.set_button_icon(
        widget,
        Some(ButtonIcon {
            symbol,
            size: 14.0,
            spacing: 0.0,
        }),
    )?;
    ui.set_button_content_layout(widget, ButtonContentLayout::IconOnly)?;
    ui.set_button_chrome(widget, ButtonChrome::Flat)?;
    ui.set_button_style(
        widget,
        ButtonStyle {
            padding: Some(Edges::all(2.0)),
            ..Default::default()
        },
    )?;
    ui.set_automation_name(widget, label)?;
    ui.set_tooltip(widget, label)?;
    Ok(widget)
}

fn edit_props(
    ui: &mut Controls,
    widget: WidgetId,
    edit: impl FnOnce(&mut rust_desktop_ui_core::NodeProps),
) -> Result<(), ControlError> {
    let mut props = ui
        .runtime
        .tree()
        .node(widget)
        .ok_or_else(|| ControlError::Invalid("Docking chrome node was removed".into()))?
        .props
        .clone();
    edit(&mut props);
    ui.runtime.apply(Mutation::SetProps {
        node: widget,
        props,
    })?;
    Ok(())
}

fn set_visible(ui: &mut Controls, widget: WidgetId, visible: bool) -> Result<(), ControlError> {
    edit_props(ui, widget, |props| props.visible = visible)
}

fn set_bounds(
    ui: &mut Controls,
    widget: WidgetId,
    bounds: Rect,
    origin: Point,
) -> Result<(), ControlError> {
    edit_props(ui, widget, |props| {
        props.style.width = Length::Px(bounds.width);
        props.style.height = Length::Px(bounds.height);
        props.style.offset = Point::new(bounds.x - origin.x, bounds.y - origin.y);
        props.style.flex_shrink = 0.0;
    })
}

fn measure_title(ui: &mut Controls, title: &str, selected: bool) -> Result<f32, ControlError> {
    ui.measure_styled_text(
        title,
        4096.0,
        TextStyle {
            font_weight: Some(if selected { 600 } else { 400 }),
            wrap: false,
            ..Default::default()
        },
    )
    .map(|size| size.width)
}

fn fit_title(
    ui: &mut Controls,
    title: &str,
    width: f32,
    selected: bool,
) -> Result<String, ControlError> {
    if measure_title(ui, title, selected)? <= width {
        return Ok(title.to_owned());
    }
    if measure_title(ui, "…", selected)? > width {
        return Ok(String::new());
    }
    let boundaries = TextDocument::new(title.to_owned()).grapheme_boundaries();
    let mut low = 0;
    let mut high = boundaries.len().saturating_sub(1);
    let mut result = "…".to_owned();
    while low <= high {
        let mid = low + (high - low) / 2;
        let candidate = format!("{}…", &title[..boundaries[mid]]);
        if measure_title(ui, &candidate, selected)? <= width {
            result = candidate;
            low = mid + 1;
        } else if mid == 0 {
            break;
        } else {
            high = mid - 1;
        }
    }
    Ok(result)
}

fn model_error(error: DockError) -> ControlError {
    ControlError::Invalid(error.to_string())
}

fn edge_at(bounds: Rect, point: Point, maximum: f32) -> Option<DockEdge> {
    let x = (bounds.width * 0.25).min(maximum);
    let y = (bounds.height * 0.25).min(maximum);
    if point.x < bounds.x + x {
        Some(DockEdge::Left)
    } else if point.x >= bounds.x + bounds.width - x {
        Some(DockEdge::Right)
    } else if point.y < bounds.y + y {
        Some(DockEdge::Top)
    } else if point.y >= bounds.y + bounds.height - y {
        Some(DockEdge::Bottom)
    } else {
        None
    }
}

fn edge_half(bounds: Rect, edge: DockEdge) -> Rect {
    match edge {
        DockEdge::Left => Rect::new(bounds.x, bounds.y, bounds.width / 2.0, bounds.height),
        DockEdge::Right => Rect::new(
            bounds.x + bounds.width / 2.0,
            bounds.y,
            bounds.width / 2.0,
            bounds.height,
        ),
        DockEdge::Top => Rect::new(bounds.x, bounds.y, bounds.width, bounds.height / 2.0),
        DockEdge::Bottom => Rect::new(
            bounds.x,
            bounds.y + bounds.height / 2.0,
            bounds.width,
            bounds.height / 2.0,
        ),
    }
}

#[cfg(test)]
#[path = "view_tests.rs"]
mod tests;
