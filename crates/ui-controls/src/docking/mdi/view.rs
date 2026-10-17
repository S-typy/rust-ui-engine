use super::*;
use crate::{
    ButtonChrome, ButtonContentLayout, ButtonIcon, ControlError, Controls, IconSymbol, Role,
};
use rust_desktop_ui_core::{
    InputEvent, Key, LayoutKind, LayoutStyle, Length, Modifiers, Mutation, PointerButton, Scene,
    WidgetId,
};
use std::collections::BTreeMap;
#[derive(Clone, Debug, PartialEq)]
pub enum DockMdiRequest {
    Activate(DockPaneId),
    Close(DockPaneId),
    Float(DockPaneId),
}
struct Chrome {
    frame: WidgetId,
    title: WidgetId,
    close: WidgetId,
    maximize: WidgetId,
    float: WidgetId,
}
struct Gesture {
    pane: DockPaneId,
    widget: WidgetId,
    start: Point,
    original: Rect,
    resize: bool,
    revision: u64,
    candidate: Rect,
}
/// Retained chrome, independent of document arenas and native window creation.
pub struct DockMdiView {
    root: WidgetId,
    window: DockWindowId,
    chrome: BTreeMap<DockPaneId, Chrome>,
    geometry: Option<DockMdiGeometry>,
    gesture: Option<Gesture>,
    overlay: Scene,
}
fn error(e: impl std::fmt::Display) -> ControlError {
    ControlError::Invalid(e.to_string())
}
fn edit_props(
    ui: &mut Controls,
    id: WidgetId,
    edit: impl FnOnce(&mut rust_desktop_ui_core::NodeProps),
) -> Result<(), ControlError> {
    let mut props = ui
        .runtime
        .tree()
        .node(id)
        .ok_or_else(|| error("Removed MDI part"))?
        .props
        .clone();
    edit(&mut props);
    ui.runtime.apply(Mutation::SetProps { node: id, props })?;
    Ok(())
}
fn bounds(ui: &mut Controls, id: WidgetId, r: Rect) -> Result<(), ControlError> {
    edit_props(ui, id, |p| {
        p.style.offset = Point::new(r.x, r.y);
        p.style.width = Length::Px(r.width);
        p.style.height = Length::Px(r.height);
        p.style.flex_shrink = 0.;
    })
}
fn icon(
    ui: &mut Controls,
    parent: WidgetId,
    symbol: IconSymbol,
    label: &str,
) -> Result<WidgetId, ControlError> {
    let id = ui.button(parent, "", None, LayoutStyle::default())?;
    ui.set_button_icon(
        id,
        Some(ButtonIcon {
            symbol,
            size: 14.,
            spacing: 0.,
        }),
    )?;
    ui.set_button_content_layout(id, ButtonContentLayout::IconOnly)?;
    ui.set_button_chrome(id, ButtonChrome::Flat)?;
    ui.set_automation_name(id, label)?;
    Ok(id)
}
impl DockMdiView {
    pub fn new(
        ui: &mut Controls,
        parent: WidgetId,
        window: DockWindowId,
    ) -> Result<Self, ControlError> {
        let root = ui.transparent_panel(
            parent,
            LayoutStyle {
                kind: LayoutKind::Overlay,
                ..Default::default()
            },
        )?;
        Ok(Self {
            root,
            window,
            chrome: BTreeMap::new(),
            geometry: None,
            gesture: None,
            overlay: Scene::default(),
        })
    }
    pub fn root(&self) -> WidgetId {
        self.root
    }
    pub fn title_widget(&self, id: &DockPaneId) -> Option<WidgetId> {
        Some(self.chrome.get(id)?.title)
    }
    pub fn geometry(&self) -> Option<&DockMdiGeometry> {
        self.geometry.as_ref()
    }
    pub fn overlay_scene(&self) -> &Scene {
        &self.overlay
    }
    pub fn is_interacting(&self) -> bool {
        self.gesture.is_some()
    }
    fn check_parts(&self, ui: &Controls) -> Result<(), ControlError> {
        if ui.runtime.tree().node(self.root).is_none() {
            return Err(error("MDI root was removed"));
        }
        for part in self.chrome.values() {
            for (child, parent) in [
                (part.frame, self.root),
                (part.title, part.frame),
                (part.close, part.frame),
                (part.maximize, part.frame),
                (part.float, part.frame),
            ] {
                if ui.runtime.tree().parent(child) != Some(parent) {
                    return Err(error("MDI required part was removed or reparented"));
                }
            }
        }
        Ok(())
    }
    pub fn update(
        &mut self,
        ui: &mut Controls,
        layout: &DockLayout,
        mdi: &DockMdiLayout,
        area: Rect,
    ) -> Result<DockMdiGeometry, ControlError> {
        self.check_parts(ui)?;
        mdi.validate_layout(layout, self.window).map_err(error)?;
        let geometry = mdi.arrange(self.window, area).map_err(error)?;
        for frame in &geometry.frames {
            let p = layout
                .pane(&frame.pane)
                .ok_or_else(|| error("MDI pane is absent"))?;
            ui.measure_text(&p.title, 4096., false)?;
        }
        bounds(ui, self.root, area)?;
        for frame in &geometry.frames {
            if !self.chrome.contains_key(&frame.pane) {
                let parent = ui.panel(
                    self.root,
                    LayoutStyle {
                        kind: LayoutKind::Overlay,
                        ..Default::default()
                    },
                )?;
                ui.set_role(parent, Role::Group);
                let title = ui.toggle(
                    parent,
                    &layout.pane(&frame.pane).unwrap().title,
                    None,
                    false,
                    LayoutStyle::default(),
                )?;
                ui.set_role(title, Role::Tab);
                ui.set_button_chrome(title, ButtonChrome::Tab)?;
                let close = icon(ui, parent, IconSymbol::Close, "Закрыть документ")?;
                let maximize = icon(ui, parent, IconSymbol::Add, "Развернуть документ")?;
                let float = icon(ui, parent, IconSymbol::Copy, "Документ в отдельном окне")?;
                self.chrome.insert(
                    frame.pane.clone(),
                    Chrome {
                        frame: parent,
                        title,
                        close,
                        maximize,
                        float,
                    },
                );
            }
        }
        for (id, p) in &self.chrome {
            edit_props(ui, p.frame, |props| {
                props.visible = geometry.frames.iter().any(|f| &f.pane == id)
            })?;
        }
        // Reparenting to the same root only reorders the retained chrome nodes.
        for f in &geometry.frames {
            let p = &self.chrome[&f.pane];
            ui.runtime.apply(Mutation::Reparent {
                node: p.frame,
                parent: self.root,
            })?;
            bounds(
                ui,
                p.frame,
                Rect::new(
                    f.bounds.x - area.x,
                    f.bounds.y - area.y,
                    f.bounds.width,
                    f.bounds.height,
                ),
            )?;
            bounds(
                ui,
                p.title,
                Rect::new(0., 0., (f.bounds.width - 96.).max(0.), f.header.height),
            )?;
            for (id, x) in [(p.float, 96.), (p.maximize, 64.), (p.close, 32.)] {
                bounds(
                    ui,
                    id,
                    Rect::new(
                        (f.bounds.width - x).max(0.),
                        0.,
                        32f32.min(f.bounds.width),
                        f.header.height,
                    ),
                )?;
            }
            ui.set_checked(p.title, mdi.order().last() == Some(&f.pane))?;
            ui.set_label(p.title, &layout.pane(&f.pane).unwrap().title)?;
            ui.set_enabled(p.close, layout.pane(&f.pane).unwrap().closable)?;
        }
        if self
            .gesture
            .as_ref()
            .is_some_and(|g| g.revision != mdi.revision())
        {
            self.cancel(ui)?;
        }
        self.geometry = Some(geometry.clone());
        Ok(geometry)
    }
    pub fn cancel(&mut self, ui: &mut Controls) -> Result<(), ControlError> {
        if let Some(g) = self.gesture.take()
            && ui.runtime.captured() == Some(g.widget)
        {
            ui.runtime.apply(Mutation::CapturePointer(None))?;
        }
        self.overlay = Scene::default();
        Ok(())
    }
    pub fn handle_input(
        &mut self,
        ui: &mut Controls,
        event: &InputEvent,
        layout: &DockLayout,
        mdi: &mut DockMdiLayout,
    ) -> Result<(bool, Vec<DockMdiRequest>), ControlError> {
        if matches!(
            event,
            InputEvent::WindowFocused(false)
                | InputEvent::KeyDown {
                    key: Key::Escape,
                    ..
                }
        ) {
            let consumed = self.gesture.is_some();
            self.cancel(ui)?;
            return Ok((consumed, Vec::new()));
        }
        self.check_parts(ui)?;
        if let Err(reason) = mdi.validate_layout(layout, self.window) {
            self.cancel(ui)?;
            return Err(error(reason));
        }
        if self.gesture.as_ref().is_some_and(|g| {
            g.revision != mdi.revision() || ui.runtime.captured() != Some(g.widget)
        }) {
            self.cancel(ui)?;
            return Ok((false, Vec::new()));
        }
        match event {
            InputEvent::PointerDown {
                position,
                button: PointerButton::Primary,
                modifiers,
            } if *modifiers == Modifiers::default() => {
                let Some(f) = self
                    .geometry
                    .as_ref()
                    .and_then(|g| g.frame_at(*position))
                    .cloned()
                else {
                    return Ok((false, Vec::new()));
                };
                let resize = position.x >= f.bounds.x + f.bounds.width - 8.
                    && position.y >= f.bounds.y + f.bounds.height - 8.;
                let title = f.header.contains(position.x, position.y)
                    && position.x < f.header.x + (f.header.width - 96.).max(0.);
                if (resize || title) && mdi.maximized().is_none() {
                    mdi.activate(&f.pane).map_err(error)?;
                    let widget = self.chrome[&f.pane].title;
                    ui.runtime.apply(Mutation::Focus(Some(widget)))?;
                    ui.runtime.apply(Mutation::CapturePointer(Some(widget)))?;
                    let origin = self.geometry.as_ref().unwrap().bounds;
                    let original = Rect::new(
                        f.bounds.x - origin.x,
                        f.bounds.y - origin.y,
                        f.bounds.width,
                        f.bounds.height,
                    );
                    self.gesture = Some(Gesture {
                        pane: f.pane.clone(),
                        widget,
                        start: *position,
                        original,
                        resize,
                        revision: mdi.revision(),
                        candidate: original,
                    });
                    return Ok((true, vec![DockMdiRequest::Activate(f.pane)]));
                }
            }
            InputEvent::PointerMoved { position } => {
                if let Some(g) = &mut self.gesture {
                    let dx = position.x - g.start.x;
                    let dy = position.y - g.start.y;
                    let min = layout.pane(&g.pane).unwrap().min_size;
                    let mut r = g.original;
                    if g.resize {
                        r.width = (r.width + dx).clamp(min.width.max(120.), 32768. - r.x);
                        r.height = (r.height + dy)
                            .clamp(min.height + DOCK_MDI_HEADER_HEIGHT, 32768. - r.y);
                    } else {
                        r.x = (r.x + dx).clamp(0., 32768. - r.width);
                        r.y = (r.y + dy).clamp(0., 32768. - r.height);
                    }
                    g.candidate = r;
                    self.overlay = Scene::default();
                    if let Some(geo) = &self.geometry {
                        let preview =
                            Rect::new(geo.bounds.x + r.x, geo.bounds.y + r.y, r.width, r.height);
                        let clip = geo.bounds;
                        let color = ui.theme().tokens().focus;
                        for edge in [
                            Rect::new(preview.x, preview.y, preview.width, 2.),
                            Rect::new(
                                preview.x,
                                preview.y + preview.height - 2.,
                                preview.width,
                                2.,
                            ),
                            Rect::new(preview.x, preview.y, 2., preview.height),
                            Rect::new(
                                preview.x + preview.width - 2.,
                                preview.y,
                                2.,
                                preview.height,
                            ),
                        ] {
                            if let Some(rect) = edge.intersection(clip) {
                                self.overlay.fill(rect, color);
                            }
                        }
                    }
                    return Ok((true, Vec::new()));
                }
            }
            InputEvent::PointerUp {
                button: PointerButton::Primary,
                ..
            } => {
                if let Some(g) = self.gesture.take() {
                    ui.runtime.apply(Mutation::CapturePointer(None))?;
                    self.overlay = Scene::default();
                    if g.candidate != g.original {
                        mdi.set_bounds(layout, &g.pane, g.candidate)
                            .map_err(error)?;
                    }
                    return Ok((true, Vec::new()));
                }
            }
            _ => {}
        }
        Ok((false, Vec::new()))
    }
    pub fn process_activated(
        &mut self,
        ui: &Controls,
        ids: &[WidgetId],
        mdi: &mut DockMdiLayout,
    ) -> Result<Vec<DockMdiRequest>, ControlError> {
        self.check_parts(ui)?;
        let mut requests = Vec::new();
        for id in ids {
            for (key, p) in &self.chrome {
                if *id == p.title {
                    mdi.activate(key).map_err(error)?;
                    requests.push(DockMdiRequest::Activate(key.clone()));
                } else if *id == p.close {
                    requests.push(DockMdiRequest::Close(key.clone()));
                } else if *id == p.float {
                    requests.push(DockMdiRequest::Float(key.clone()));
                } else if *id == p.maximize {
                    mdi.toggle_maximize(key).map_err(error)?;
                    requests.push(DockMdiRequest::Activate(key.clone()));
                }
            }
        }
        Ok(requests)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn changed_pane_minimum_rejects_input_before_capture_or_geometry_mutation() {
        let layout = super::super::tests::layout();
        let mut mdi = DockMdiLayout::new();
        mdi.synchronize(&layout, DockWindowId::MAIN).unwrap();
        let mut ui = Controls::new(crate::Theme::FluentLight).unwrap();
        let root = ui.root();
        let mut view = DockMdiView::new(&mut ui, root, DockWindowId::MAIN).unwrap();
        view.update(&mut ui, &layout, &mdi, Rect::new(0., 0., 900., 600.))
            .unwrap();
        ui.update(Size::new(900., 600.)).unwrap();
        let before = mdi.revision();
        let (consumed, _) = view
            .handle_input(
                &mut ui,
                &InputEvent::PointerDown {
                    position: Point::new(80., 64.),
                    button: PointerButton::Primary,
                    modifiers: Modifiers::default(),
                },
                &layout,
                &mut mdi,
            )
            .unwrap();
        assert!(consumed);
        assert!(ui.runtime.captured().is_some());
        let mut panes = layout.panes().to_vec();
        panes[0].min_size.height = 32768.;
        let changed = DockLayout::from_snapshot(panes, layout.snapshot()).unwrap();
        assert!(mdi.synchronize(&changed, DockWindowId::MAIN).is_err());
        assert_eq!(mdi.revision(), before);
        assert!(
            view.handle_input(
                &mut ui,
                &InputEvent::KeyDown {
                    key: Key::Escape,
                    modifiers: Modifiers::default(),
                    repeat: false
                },
                &changed,
                &mut mdi
            )
            .unwrap()
            .0
        );
        assert_eq!(ui.runtime.captured(), None);
        assert!(
            view.handle_input(
                &mut ui,
                &InputEvent::PointerDown {
                    position: Point::new(100., 100.),
                    button: PointerButton::Primary,
                    modifiers: Modifiers::default()
                },
                &changed,
                &mut mdi
            )
            .is_err()
        );
        assert_eq!(mdi.revision(), before);
        assert_eq!(ui.runtime.captured(), None);
    }
    #[test]
    fn maximize_emits_activation_for_the_visible_document_and_retains_title_identity() {
        let layout = super::super::tests::layout();
        let mut mdi = DockMdiLayout::new();
        mdi.synchronize(&layout, DockWindowId::MAIN).unwrap();
        let mut ui = Controls::new(crate::Theme::FluentLight).unwrap();
        let root = ui.root();
        let mut view = DockMdiView::new(&mut ui, root, DockWindowId::MAIN).unwrap();
        view.update(&mut ui, &layout, &mdi, Rect::new(0., 0., 900., 600.))
            .unwrap();
        let key = DockPaneId::new("B").unwrap();
        let title = view.title_widget(&key).unwrap();
        let maximize = view.chrome[&key].maximize;
        assert_eq!(
            view.process_activated(&ui, &[maximize], &mut mdi).unwrap(),
            vec![DockMdiRequest::Activate(key.clone())]
        );
        view.update(&mut ui, &layout, &mdi, Rect::new(0., 0., 900., 600.))
            .unwrap();
        ui.update(Size::new(900., 600.)).unwrap();
        assert_eq!(view.title_widget(&key), Some(title));
        assert!(
            ui.runtime
                .bounds(title)
                .is_some_and(|r| r.width > 0. && r.height > 0.)
        );
        assert_eq!(view.geometry().unwrap().frames.len(), 1);
    }
}
