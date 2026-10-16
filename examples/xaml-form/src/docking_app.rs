//! Several native windows compose the same long-lived panel documents.
mod drag;
mod input;
pub(crate) mod storage;
#[cfg(test)]
mod tests;

use crate::{
    accessibility::Accessibility,
    form::Form,
    form_chrome::FormChrome,
    view::MountedView,
    view_ribbon::{MountedRibbon, build_ribbon},
};
use rust_desktop_ui_controls::*;
use rust_desktop_ui_core::{
    Axis, InputEvent, LayoutKind, LayoutStyle, Length, Mutation, Point, Rect, Scene, Size, WidgetId,
};
use rust_desktop_ui_platform_winit::{
    self as platform, DesktopApp, DesktopApplication, DesktopPointer, EventResponse, PlatformEvent,
    RunOptions, WindowGeometry, WindowKey, WindowSpec, accesskit,
};
use rust_desktop_ui_xaml::{self as xaml, DockNodeDefinition, ElementKind, WindowDefinition};
use std::{
    cell::RefCell,
    collections::{BTreeMap, HashMap},
    rc::Rc,
    time::Duration,
};

fn err(error: impl std::fmt::Display) -> String {
    error.to_string()
}

struct Workspace {
    layout: DockLayout,
    initial: DockSnapshot,
    panes: BTreeMap<DockPaneId, Form>,
    active: DockPaneId,
    focused: Option<(DockWindowId, DockPaneId)>,
    theme: Theme,
    alive: bool,
    diagnostics: bool,
    native_bounds: HashMap<DockWindowId, Rect>,
    native_geometry: HashMap<DockWindowId, WindowGeometry>,
    drop_targets: HashMap<DockWindowId, DockDropTargets>,
    drag: Option<drag::DragSession>,
    layout_file: Option<std::path::PathBuf>,
    layout_status: String,
    placement_revision: u64,
}

impl Workspace {
    fn reconcile_focus(&mut self) -> Result<(), String> {
        if self.layout.is_closed(&self.active)
            && let Some(pane) = self
                .layout
                .panes()
                .iter()
                .find(|pane| !self.layout.is_closed(&pane.id))
        {
            self.active = pane.id.clone();
        }
        let Some((_, pane)) = self.focused.clone() else {
            return Ok(());
        };
        let visible = self.layout.window_for(&pane).and_then(|window| {
            let geometry = self
                .layout
                .arrange(window, Rect::new(0.0, 0.0, 1200.0, 800.0))
                .ok()?;
            geometry
                .groups
                .iter()
                .any(|group| group.active == pane)
                .then_some(window)
        });
        if let Some(window) = visible {
            // Moving a still-active pane never destroys or blurs its editor.
            self.focused = Some((window, pane));
        } else {
            if let Some(form) = self.panes.get_mut(&pane) {
                form.event(PlatformEvent::Input(InputEvent::WindowFocused(false)))?;
                form.event(PlatformEvent::ImeCancel)?;
            }
            self.focused = None;
        }
        Ok(())
    }

    fn focus_pane(&mut self, window: DockWindowId, pane: Option<DockPaneId>) -> Result<(), String> {
        if self.focused.as_ref().map(|(_, old)| old) != pane.as_ref() {
            if let Some((_, old)) = self.focused.take()
                && let Some(form) = self.panes.get_mut(&old)
            {
                form.event(PlatformEvent::Input(InputEvent::WindowFocused(false)))?;
                form.event(PlatformEvent::ImeCancel)?;
            }
            if let Some(pane) = pane {
                self.active = pane.clone();
                self.focused = Some((window, pane));
            }
        }
        Ok(())
    }

    fn request(&mut self, window: DockWindowId, request: DockRequest) -> Result<(), String> {
        let result = (|| -> Result<(), String> {
            match request {
                DockRequest::Activate(pane) => {
                    self.layout.activate(&pane).map_err(err)?;
                    self.active = pane;
                }
                DockRequest::Close(pane) => {
                    self.layout.close(&pane).map_err(err)?;
                }
                DockRequest::Float(pane) => {
                    let origin = self
                        .native_bounds
                        .get(&window)
                        .copied()
                        .unwrap_or(Rect::new(100.0, 100.0, 0.0, 0.0));
                    self.layout
                        .float_pane(
                            &pane,
                            Rect::new(origin.x + 60.0, origin.y + 60.0, 640.0, 440.0),
                        )
                        .map_err(err)?;
                    self.active = pane;
                }
                DockRequest::RedockWindow(window) => {
                    self.layout.redock_window(window).map_err(err)?;
                }
                DockRequest::Move { pane, placement } => {
                    self.layout.move_pane(&pane, placement).map_err(err)?;
                    self.active = pane;
                }
                DockRequest::Resize { split, ratio } => {
                    self.layout.resize(split, ratio).map_err(err)?;
                }
            }
            Ok(())
        })();
        if let Err(error) = result {
            // Limits and stale gestures may reject a transition. Keep the
            // retained documents and native windows alive in that case.
            eprintln!("Docking action rejected: {error}");
            return Ok(());
        }
        self.reconcile_focus()
    }

    fn command(&mut self, window: DockWindowId, command: &str) -> Result<(), String> {
        match command {
            "DockFloat" if !self.layout.is_closed(&self.active) => {
                self.request(window, DockRequest::Float(self.active.clone()))?
            }
            "DockBack" => {
                if let Some(owner) = self.layout.window_for(&self.active)
                    && owner != DockWindowId::MAIN
                {
                    self.request(window, DockRequest::RedockWindow(owner))?;
                }
            }
            "DockReset" => {
                self.layout
                    .restore_snapshot(self.initial.clone())
                    .map_err(err)?;
                self.drag = None;
                self.drop_targets.clear();
                self.placement_revision = self.placement_revision.wrapping_add(1);
            }
            "DockSaveLayout" => self.layout_command(false)?,
            "DockLoadLayout" => self.layout_command(true)?,
            "DockRestore" => {
                for pane in self.layout.snapshot().closed {
                    // Restoring many panes must not build an unbounded chain
                    // of splits. Append tabs to an existing main group.
                    let geometry = self
                        .layout
                        .arrange(DockWindowId::MAIN, Rect::new(0.0, 0.0, 1200.0, 800.0))
                        .map_err(err)?;
                    let placement = geometry.groups.first().map_or(
                        DockPlacement::Root {
                            window: DockWindowId::MAIN,
                            edge: DockEdge::Right,
                        },
                        |group| DockPlacement::Tab {
                            group: group.id,
                            index: group.panes.len(),
                        },
                    );
                    if let Err(error) = self.layout.restore(&pane, placement) {
                        eprintln!("Docking restore rejected: {error}");
                        break;
                    }
                }
            }
            "Light" | "Dark" => {
                self.theme = if command == "Light" {
                    Theme::FluentLight
                } else {
                    Theme::FluentDark
                };
                for form in self.panes.values_mut() {
                    form.ui.set_theme(self.theme).map_err(err)?;
                }
            }
            _ => {}
        }
        self.reconcile_focus()
    }
}

pub(crate) struct DockApplication {
    state: Rc<RefCell<Workspace>>,
    windows: BTreeMap<WindowKey, DockWindow>,
    definition: WindowDefinition,
    ribbon: Option<xaml::RibbonDefinition>,
    options: RunOptions,
    gap: f32,
}

impl DockApplication {
    pub(crate) fn new(
        definition: WindowDefinition,
        options: RunOptions,
        diagnostics: bool,
        float_pane: Option<&str>,
    ) -> Result<Self, String> {
        xaml::validate_window_chrome(&definition)?;
        xaml::validate_window_docking(&definition)?;
        if definition.padding != [0.0; 4] {
            return Err("Docking shell requires Window Padding=0".into());
        }
        validate_shell_element(&definition.content)?;
        let gap = if let ElementKind::StackPanel(panel) = &definition.content.kind {
            if panel.padding != [0.0; 4]
                || panel.surface != xaml::SurfaceKindDefinition::Transparent
                || !panel.spacing.is_finite()
                || !(0.0..=256.0).contains(&panel.spacing)
            {
                return Err("Docking shell requires transparent StackPanel, Padding=0 and Spacing in [0, 256]".into());
            }
            for child in &panel.children {
                validate_shell_element(child)?;
            }
            panel.spacing
        } else {
            0.0
        };
        let (ribbon, docking) = match &definition.content.kind {
            ElementKind::DockWorkspace(dock) => (None, dock),
            ElementKind::StackPanel(panel) if panel.orientation == xaml::Orientation::Vertical => {
                match panel.children.as_slice() {
                    [dock] => match &dock.kind {
                        ElementKind::DockWorkspace(dock) => (None, dock),
                        _ => return Err("Docking requires a DockWorkspace".into()),
                    },
                    [ribbon, dock] => match (&ribbon.kind, &dock.kind) {
                        (ElementKind::Ribbon(ribbon), ElementKind::DockWorkspace(dock)) => {
                            (Some(ribbon.clone()), dock)
                        }
                        _ => {
                            return Err(
                                "Docking shell requires Ribbon followed by DockWorkspace".into()
                            );
                        }
                    },
                    _ => return Err("Docking shell supports Ribbon and DockWorkspace only".into()),
                }
            }
            _ => return Err("Docking requires a DockWorkspace or vertical shell".into()),
        };
        let mut counter = 0u64;
        let root = convert_node(&docking.root, &mut counter)?;
        let mut catalog = Vec::new();
        let mut panes = BTreeMap::new();
        for (index, pane) in docking.panes().iter().enumerate() {
            let id = DockPaneId::new(&pane.id).map_err(err)?;
            catalog.push(DockPane {
                id: id.clone(),
                title: pane.header.clone(),
                closable: pane.can_close,
                min_size: Size::new(pane.min_width, pane.min_height),
            });
            let mut form = Form::new(
                WindowDefinition {
                    title: pane.header.clone(),
                    width: 640.0,
                    height: 440.0,
                    theme: definition.theme,
                    density: definition.density,
                    accent_color: definition.accent_color,
                    title_bar: xaml::TitleBar::Native,
                    padding: [0.0; 4],
                    content: (*pane.content).clone(),
                },
                diagnostics,
            )?;
            form.accessible = Accessibility::with_base(((index as u64) + 1) * 1_000_000);
            panes.insert(id, form);
        }
        let mut layout = DockLayout::new(catalog, root).map_err(err)?;
        let initial = layout.snapshot();
        let active = DockPaneId::new("Document")
            .ok()
            .filter(|id| layout.pane(id).is_some())
            .unwrap_or_else(|| layout.panes()[0].id.clone());
        if let Some(pane) = float_pane {
            layout
                .float_pane(
                    &DockPaneId::new(pane).map_err(err)?,
                    Rect::new(180.0, 160.0, 640.0, 440.0),
                )
                .map_err(err)?;
        }
        let theme = crate::visual_model::theme(
            definition.theme,
            definition.density,
            definition.accent_color,
        )?;
        Ok(Self {
            state: Rc::new(RefCell::new(Workspace {
                layout,
                initial,
                panes,
                active,
                focused: None,
                theme,
                alive: true,
                diagnostics,
                native_bounds: HashMap::new(),
                native_geometry: HashMap::new(),
                drop_targets: HashMap::new(),
                drag: None,
                layout_file: None,
                layout_status: "Расположение сохраняется только по команде «Сохранить».".into(),
                placement_revision: 0,
            })),
            windows: BTreeMap::new(),
            definition,
            ribbon,
            options,
            gap,
        })
    }

    pub(crate) fn set_layout_file(&mut self, path: std::path::PathBuf) {
        self.state.borrow_mut().layout_file = Some(path);
    }
}

fn validate_shell_element(element: &xaml::ElementDefinition) -> Result<(), String> {
    let mut common = element.common.clone();
    common.name = None;
    if common != xaml::CommonProperties::default() {
        return Err("Docking shell elements support x:Name with automatic Width/Height, zero Margin, and default enabled/accessibility/tooltip properties".into());
    }
    Ok(())
}

fn convert_node(node: &DockNodeDefinition, counter: &mut u64) -> Result<DockNode, String> {
    *counter += 1;
    let id = *counter;
    match node {
        DockNodeDefinition::Group {
            selected_pane,
            panes,
        } => {
            let ids: Vec<_> = panes
                .iter()
                .map(|p| DockPaneId::new(&p.id))
                .collect::<Result<_, _>>()
                .map_err(err)?;
            let active = selected_pane
                .as_deref()
                .map(DockPaneId::new)
                .transpose()
                .map_err(err)?
                .unwrap_or_else(|| ids[0].clone());
            Ok(DockNode::Tabs {
                id: DockGroupId(id),
                panes: ids,
                active,
            })
        }
        DockNodeDefinition::Split {
            orientation,
            ratio,
            first,
            second,
        } => Ok(DockNode::Split {
            id: DockSplitId(id),
            axis: match orientation {
                xaml::Orientation::Horizontal => Axis::Horizontal,
                xaml::Orientation::Vertical => Axis::Vertical,
            },
            ratio: *ratio,
            first: Box::new(convert_node(first, counter)?),
            second: Box::new(convert_node(second, counter)?),
        }),
    }
}

impl DesktopApplication for DockApplication {
    fn windows(&mut self) -> Result<Vec<WindowSpec>, String> {
        let state = self.state.borrow();
        if !state.alive {
            return Ok(Vec::new());
        }
        let floating = state.layout.floating().to_vec();
        let placement_revision = state.placement_revision;
        drop(state);
        let mut specs = vec![WindowSpec::new(WindowKey::MAIN, self.options.clone())];
        for float in floating {
            let mut spec = WindowSpec::new(
                WindowKey(float.id.0),
                RunOptions {
                    width: f64::from(float.bounds.width),
                    height: f64::from(float.bounds.height),
                    min_size: Some(Size::new(240.0, 160.0)),
                    ..self.options.clone()
                },
            );
            spec.position = Some(Point::new(float.bounds.x, float.bounds.y));
            spec.owner = Some(WindowKey::MAIN);
            spec.placement_revision = placement_revision;
            specs.push(spec);
        }
        for spec in &specs {
            if !self.windows.contains_key(&spec.key) {
                let main = spec.key == WindowKey::MAIN;
                self.windows.insert(
                    spec.key,
                    DockWindow::new(
                        self.state.clone(),
                        DockWindowId(spec.key.0),
                        if main {
                            &self.definition.title
                        } else {
                            "Плавающая панель · Rust UI"
                        },
                        if main { self.ribbon.clone() } else { None },
                        if main {
                            self.definition.title_bar
                        } else {
                            xaml::TitleBar::Native
                        },
                        if main { self.gap } else { 0.0 },
                    )?,
                );
            }
        }
        Ok(specs)
    }
    fn window(&self, key: WindowKey) -> Option<&dyn DesktopApp> {
        self.windows.get(&key).map(|view| view as &dyn DesktopApp)
    }
    fn window_mut(&mut self, key: WindowKey) -> Option<&mut (dyn DesktopApp + '_)> {
        self.windows
            .get_mut(&key)
            .map(|view| view as &mut dyn DesktopApp)
    }
    fn close_requested(&mut self, key: WindowKey) -> Result<(), String> {
        let mut state = self.state.borrow_mut();
        state.drag = None;
        state.drop_targets.remove(&DockWindowId(key.0));
        if key == WindowKey::MAIN {
            state.alive = false;
        } else if state.layout.root(DockWindowId(key.0)).is_some() {
            state.request(
                DockWindowId(key.0),
                DockRequest::RedockWindow(DockWindowId(key.0)),
            )?;
        }
        Ok(())
    }
    fn window_closed(&mut self, key: WindowKey) {
        self.windows.remove(&key);
        let mut state = self.state.borrow_mut();
        let window = DockWindowId(key.0);
        state.native_bounds.remove(&window);
        state.native_geometry.remove(&window);
        state.drop_targets.remove(&window);
        if state.drag.as_ref().is_some_and(|drag| {
            drag.source == window
                || drag
                    .target
                    .as_ref()
                    .is_some_and(|(target, _)| *target == window)
        }) {
            state.drag = None;
        }
    }
}

struct DockWindow {
    state: Rc<RefCell<Workspace>>,
    id: DockWindowId,
    title: String,
    ui: Controls,
    ribbon: Option<MountedRibbon>,
    chrome: FormChrome,
    dock: DockView,
    scene: Scene,
    geometry: Option<DockGeometry>,
    accessible: Accessibility,
    action_owners: HashMap<accesskit::NodeId, DockPaneId>,
    hovered_pane: Option<DockPaneId>,
    suppress_text: bool,
    native_focused: bool,
    last_diagnostic: String,
    gap: f32,
    desktop_pointer: Option<DesktopPointer>,
    cross_drag_started: bool,
    layout_status: WidgetId,
    observed_revision: u64,
    applied_placement_revision: u64,
    layout_pending: bool,
}

impl DockWindow {
    fn new(
        state: Rc<RefCell<Workspace>>,
        id: DockWindowId,
        title: &str,
        ribbon_definition: Option<xaml::RibbonDefinition>,
        title_bar: xaml::TitleBar,
        gap: f32,
    ) -> Result<Self, String> {
        let mut ui = Controls::new(state.borrow().theme).map_err(err)?;
        ui.runtime
            .apply(Mutation::SetStyle {
                node: ui.root(),
                style: LayoutStyle {
                    kind: LayoutKind::Overlay,
                    ..Default::default()
                },
            })
            .map_err(err)?;
        for (command, label) in [
            ("DockFloat", "Отдельное окно"),
            ("DockBack", "Закрепить"),
            ("DockReset", "Сбросить расположение"),
            ("DockRestore", "Показать панели"),
            ("DockSaveLayout", "Сохранить"),
            ("DockLoadLayout", "Восстановить"),
            ("Light", "Светлая тема"),
            ("Dark", "Тёмная тема"),
        ] {
            ui.commands_mut().register(command, label);
        }
        ui.update(Size::new(1200.0, 800.0)).map_err(err)?;
        let ribbon = if let Some(definition) = ribbon_definition {
            let mut ribbon = build_ribbon(&ui, &definition)?;
            let height = ribbon.preflight(&mut ui, 1200.0).map_err(err)?;
            let wrapper = ui
                .transparent_panel(
                    ui.root(),
                    LayoutStyle {
                        kind: LayoutKind::Stack(Axis::Vertical),
                        height: Length::Px(height),
                        ..Default::default()
                    },
                )
                .map_err(err)?;
            ribbon
                .mount_with_width(&mut ui, wrapper, 1200.0)
                .map_err(err)?;
            Some(MountedRibbon { wrapper, ribbon })
        } else {
            None
        };
        let root = ui.root();
        let dock = DockView::new(&mut ui, root, id).map_err(err)?;
        let layout_status = ui.label(root, "", LayoutStyle::default()).map_err(err)?;
        ui.set_text_style(
            layout_status,
            TextStyle {
                font_size: Some(12.0),
                tone: TextTone::Secondary,
                wrap: false,
                ..Default::default()
            },
        )
        .map_err(err)?;
        Ok(Self {
            state,
            id,
            title: title.into(),
            ui,
            ribbon,
            chrome: FormChrome::new(title_bar),
            dock,
            scene: Scene::default(),
            geometry: None,
            accessible: Accessibility::default(),
            action_owners: HashMap::new(),
            hovered_pane: None,
            suppress_text: false,
            native_focused: false,
            last_diagnostic: String::new(),
            gap,
            desktop_pointer: None,
            cross_drag_started: false,
            layout_status,
            observed_revision: 0,
            applied_placement_revision: 0,
            layout_pending: false,
        })
    }

    fn focused_pane(&self) -> Option<DockPaneId> {
        self.state
            .borrow()
            .focused
            .as_ref()
            .filter(|(window, _)| *window == self.id)
            .map(|(_, pane)| pane.clone())
    }
    fn pane_bounds(&self, pane: &DockPaneId) -> Option<Rect> {
        self.geometry
            .as_ref()?
            .groups
            .iter()
            .find(|group| &group.active == pane)
            .map(|group| group.content)
    }

    fn compose_accessibility(&mut self, scale: f32) -> accesskit::TreeUpdate {
        let main = self
            .accessible
            .tree(&self.ui, &MountedView::default(), &self.title, scale);
        let focused = self.focused_pane();
        let mut panes = Vec::new();
        let mut state = self.state.borrow_mut();
        for group in self.geometry.iter().flat_map(|geometry| &geometry.groups) {
            let Some(form) = state.panes.get_mut(&group.active) else {
                continue;
            };
            panes.push(crate::docking_accessibility::PaneAccessibility {
                id: group.active.clone(),
                root: form.accessible.root_id(),
                offset: Point::new(group.content.x * scale, group.content.y * scale),
                clip: Rect::new(
                    group.content.x * scale,
                    group.content.y * scale,
                    group.content.width * scale,
                    group.content.height * scale,
                ),
                tree: form.accessibility(scale),
            });
        }
        let (tree, owners) =
            crate::docking_accessibility::merge_accessibility(main, panes, focused.as_ref());
        self.action_owners = owners;
        tree
    }
}

impl DesktopApp for DockWindow {
    fn update(&mut self, viewport: Size, scale: f32) -> Result<(), String> {
        self.sync_layout_revision()?;
        if self.cross_drag_started
            && self
                .state
                .borrow()
                .drag
                .as_ref()
                .is_none_or(|drag| drag.source != self.id)
        {
            self.cancel_dock_drag()?;
        }
        let mut state = self.state.borrow_mut();
        if self.id != DockWindowId::MAIN && state.layout.root(self.id).is_none() {
            return Ok(());
        }
        self.ui.set_theme(state.theme).map_err(err)?;
        self.ui
            .commands_mut()
            .set_enabled(&"DockFloat".into(), !state.layout.is_closed(&state.active));
        self.ui.commands_mut().set_enabled(
            &"DockBack".into(),
            state
                .layout
                .window_for(&state.active)
                .is_some_and(|window| window != DockWindowId::MAIN),
        );
        self.ui.commands_mut().set_enabled(
            &"DockRestore".into(),
            !state.layout.snapshot().closed.is_empty(),
        );
        for command in ["DockSaveLayout", "DockLoadLayout"] {
            self.ui
                .commands_mut()
                .set_enabled(&command.into(), state.layout_file.is_some());
        }
        let footer_height = 26.0_f32.min(viewport.height);
        self.ui
            .set_label(self.layout_status, &state.layout_status)
            .map_err(err)?;
        self.ui
            .runtime
            .apply(Mutation::SetStyle {
                node: self.layout_status,
                style: LayoutStyle {
                    offset: Point::new(12.0, (viewport.height - footer_height).max(0.0)),
                    width: Length::Px((viewport.width - 24.0).max(0.0)),
                    height: Length::Px(footer_height),
                    ..Default::default()
                },
            })
            .map_err(err)?;
        self.ui.update(viewport).map_err(err)?;
        let mut top = 0.0;
        if let Some(mounted) = &mut self.ribbon {
            self.chrome
                .configure(&mut mounted.ribbon, &self.title, viewport)?;
            top = mounted
                .ribbon
                .preflight(&mut self.ui, viewport.width)
                .map_err(err)?;
            self.ui
                .runtime
                .apply(Mutation::SetStyle {
                    node: mounted.wrapper,
                    style: LayoutStyle {
                        kind: LayoutKind::Stack(Axis::Vertical),
                        height: Length::Px(top),
                        ..Default::default()
                    },
                })
                .map_err(err)?;
            self.ui.update(viewport).map_err(err)?;
            mounted.update(&mut self.ui)?;
        }
        let content_top = if self.ribbon.is_some() {
            top + self.gap
        } else {
            8.0
        };
        let bounds = Rect::new(
            8.0_f32.min(viewport.width),
            content_top.min(viewport.height),
            (viewport.width - 16.0).max(0.0),
            (viewport.height - content_top - footer_height - 8.0).max(0.0),
        );
        let geometry = self
            .dock
            .update(&mut self.ui, &state.layout, bounds)
            .map_err(err)?;
        let popup_open = self.ui.popup_open()
            || geometry
                .groups
                .iter()
                .any(|group| state.panes[&group.active].ui.popup_open());
        if !popup_open && let Some(targets) = self.dock.drop_targets() {
            state.drop_targets.insert(self.id, targets);
        } else {
            state.drop_targets.remove(&self.id);
        }
        state.refresh_drag_target();
        let preview = state
            .drag
            .as_ref()
            .and_then(|drag| drag.target.as_ref())
            .filter(|(window, _)| *window == self.id)
            .map(|(_, preview)| *preview);
        if (state.drag.is_some() || self.dock.tab_drag().is_none())
            && self.dock.set_external_preview(preview).is_err()
        {
            self.dock.set_external_preview(None).map_err(err)?;
        }
        self.ui.update(viewport).map_err(err)?;
        self.scene = self.ui.base_scene().clone();
        for group in &geometry.groups {
            if group.content.width <= 0.0 || group.content.height <= 0.0 {
                continue;
            }
            let form = state
                .panes
                .get_mut(&group.active)
                .ok_or("Missing pane document")?;
            form.update(Size::new(group.content.width, group.content.height), scale)?;
            self.scene
                .append_view(
                    form.scene(),
                    Point::new(group.content.x, group.content.y),
                    group.content,
                )
                .map_err(err)?;
        }
        self.scene.append(self.ui.overlay_scene());
        self.scene.append(self.dock.overlay_scene());
        if state.diagnostics {
            use crate::popup_diagnostics::{rectangle, string};
            let groups = geometry.groups.iter().map(|group| format!("{{\"bounds\":{},\"content\":{},\"header\":{},\"active\":{},\"panes\":[{}]}}", rectangle(group.bounds), rectangle(group.content), rectangle(group.tab_strip), string(group.active.as_str()), group.panes.iter().map(|p| string(p.as_str())).collect::<Vec<_>>().join(","))).collect::<Vec<_>>().join(",");
            let editors = state
                .panes
                .iter()
                .map(|(id, form)| {
                    format!(
                        "{{\"pane\":{},\"ids\":[{}]}}",
                        string(id.as_str()),
                        form.view
                            .editors
                            .iter()
                            .map(|widget| string(&format!("{widget:?}")))
                            .collect::<Vec<_>>()
                            .join(",")
                    )
                })
                .collect::<Vec<_>>()
                .join(",");
            let diagnostic = format!(
                "dock-state {{\"window\":{},\"viewport\":[{},{}],\"floating\":{},\"groups\":[{}],\"editors\":[{}]}}",
                self.id.0,
                viewport.width,
                viewport.height,
                state.layout.floating().len(),
                groups,
                editors
            );
            if diagnostic != self.last_diagnostic {
                println!("{diagnostic}");
                self.last_diagnostic = diagnostic;
            }
        }
        self.geometry = Some(geometry);
        self.layout_pending = false;
        Ok(())
    }
    fn scene(&self) -> &Scene {
        &self.scene
    }
    fn title(&self) -> String {
        self.title.clone()
    }
    fn title_bar(&self) -> Option<platform::TitleBar> {
        self.chrome.title_bar(&self.ui, self.ribbon.as_ref())
    }
    fn window_chrome_changed(&mut self, chrome: platform::WindowChrome) {
        self.chrome
            .set_native(chrome, self.state.borrow().diagnostics);
    }
    fn window_geometry_changed(&mut self, position: Point, size: Size, _scale: f32) {
        let mut state = self.state.borrow_mut();
        let bounds = Rect::new(position.x, position.y, size.width, size.height);
        state.native_bounds.insert(self.id, bounds);
        if self.id != DockWindowId::MAIN
            && self.applied_placement_revision == state.placement_revision
        {
            let _ = state.layout.update_floating_bounds(self.id, bounds);
        }
    }
    fn event(&mut self, event: PlatformEvent) -> Result<EventResponse, String> {
        self.handle_event(event)
    }
    fn desktop_geometry_changed(&mut self, geometry: WindowGeometry) {
        let mut state = self.state.borrow_mut();
        if state.native_geometry.get(&self.id).is_none_or(|old| {
            old.client_size != geometry.client_size || old.scale != geometry.scale
        }) {
            state.drop_targets.remove(&self.id);
        }
        state.native_geometry.insert(self.id, geometry);
        state.refresh_drag_target();
    }
    fn desktop_pointer_changed(&mut self, pointer: DesktopPointer) {
        self.desktop_pointer = Some(pointer);
    }
    fn window_placement_applied(&mut self, revision: u64) {
        self.applied_placement_revision = revision;
    }
    fn pointer_capture_lost(&mut self) -> Result<EventResponse, String> {
        self.sync_layout_revision()?;
        self.cancel_dock_drag()?;
        self.ui.cancel_pointer_interaction().map_err(err)?;
        let mut state = self.state.borrow_mut();
        let panes = state.layout.window_panes(self.id).unwrap_or_default();
        for pane in panes {
            state
                .panes
                .get_mut(&pane)
                .expect("catalog pane")
                .ui
                .cancel_pointer_interaction()
                .map_err(err)?;
        }
        Ok(EventResponse {
            redraw: true,
            ..Default::default()
        })
    }
    fn ime_target(&self) -> Option<WidgetId> {
        let pane = self.focused_pane()?;
        self.state.borrow().panes.get(&pane)?.ime_target()
    }
    fn ime_cursor(&self) -> Option<Rect> {
        let pane = self.focused_pane()?;
        let bounds = self.pane_bounds(&pane)?;
        let caret = self.state.borrow().panes.get(&pane)?.ime_cursor()?;
        Rect::new(
            caret.x + bounds.x,
            caret.y + bounds.y,
            caret.width,
            caret.height,
        )
        .intersection(bounds)
    }
    fn ime_composition_active(&self) -> Option<bool> {
        let pane = self.focused_pane()?;
        self.state
            .borrow()
            .panes
            .get(&pane)?
            .ime_composition_active()
    }
    fn wake_after(&self) -> Option<Duration> {
        let state = self.state.borrow();
        self.geometry
            .iter()
            .flat_map(|g| &g.groups)
            .filter_map(|g| state.panes.get(&g.active).and_then(DesktopApp::wake_after))
            .chain(
                self.ui
                    .tooltip_deadline()
                    .map(|deadline| deadline.saturating_duration_since(std::time::Instant::now())),
            )
            .min()
    }
    fn accessibility(&mut self, scale: f32) -> accesskit::TreeUpdate {
        self.compose_accessibility(scale)
    }
}
