use super::*;
use crate::spatial::*;

#[derive(Clone, Debug)]
pub enum SpatialCanvasModel {
    Chart3D(Chart3D),
    Map(Map),
    Diagram(Diagram),
}
impl SpatialCanvasModel {
    pub fn summary(&self) -> String {
        match self {
            Self::Chart3D(m) => m.summary(),
            Self::Map(m) => m.summary(),
            Self::Diagram(m) => m.summary(),
        }
    }
    pub fn table(&self) -> SpatialTable {
        match self {
            Self::Chart3D(m) => m.table(),
            Self::Map(m) => m.table(),
            Self::Diagram(m) => m.table(),
        }
    }
    fn paint(
        &self,
        scene: &mut Scene,
        b: Rect,
        clip: Rect,
        p: SpatialPalette,
    ) -> Result<(), SpatialError> {
        match self {
            Self::Chart3D(m) => m.paint(scene, b, clip, p),
            Self::Map(m) => m.paint(scene, b, clip, p),
            Self::Diagram(m) => m.paint(scene, b, clip, p),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpatialChangeKind {
    Selection,
    View,
    Document,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpatialCanvasChange {
    pub id: WidgetId,
    pub kind: SpatialChangeKind,
    pub summary: String,
}
pub(super) struct SpatialState {
    model: SpatialCanvasModel,
    summary: String,
    event: Option<SpatialCanvasChange>,
}
pub(super) struct SpatialPress {
    owner: WidgetId,
    start: Point,
    last: Point,
    button: PointerButton,
    initial: SpatialCanvasModel,
    hit: Option<DiagramHit>,
    moved: bool,
}
fn error(e: impl std::fmt::Display) -> ControlError {
    ControlError::Invalid(e.to_string())
}

impl Controls {
    pub fn spatial_canvas(
        &mut self,
        parent: WidgetId,
        model: SpatialCanvasModel,
        mut style: LayoutStyle,
    ) -> Result<WidgetId, ControlError> {
        if style.width == Length::Auto {
            style.width = Length::Px(640.0);
        }
        if style.height == Length::Auto {
            style.height = Length::Px(420.0);
        }
        style.padding = Edges::all(0.0);
        let summary = model.summary();
        self.preflight_new_editor_text(&summary, &style, false)?;
        let id = self.transparent_panel(parent, style)?;
        self.set_role(id, Role::Group);
        self.edit_props(id, |p| p.focusable = true)?;
        self.controls.get_mut(&id).unwrap().label = match model {
            SpatialCanvasModel::Chart3D(_) => "3D chart",
            SpatialCanvasModel::Map(_) => "Map",
            SpatialCanvasModel::Diagram(_) => "Diagram",
        }
        .into();
        self.spatial_canvases.insert(
            id,
            SpatialState {
                model,
                summary,
                event: None,
            },
        );
        self.scene_dirty = true;
        Ok(id)
    }
    pub fn chart_3d(
        &mut self,
        parent: WidgetId,
        model: Chart3D,
        style: LayoutStyle,
    ) -> Result<WidgetId, ControlError> {
        self.spatial_canvas(parent, SpatialCanvasModel::Chart3D(model), style)
    }
    pub fn map_view(
        &mut self,
        parent: WidgetId,
        model: Map,
        style: LayoutStyle,
    ) -> Result<WidgetId, ControlError> {
        self.spatial_canvas(parent, SpatialCanvasModel::Map(model), style)
    }
    pub fn diagram(
        &mut self,
        parent: WidgetId,
        model: Diagram,
        style: LayoutStyle,
    ) -> Result<WidgetId, ControlError> {
        self.spatial_canvas(parent, SpatialCanvasModel::Diagram(model), style)
    }
    pub fn spatial_model(&self, id: WidgetId) -> Option<&SpatialCanvasModel> {
        self.spatial_canvases.get(&id).map(|s| &s.model)
    }
    pub fn spatial_table(&self, id: WidgetId) -> Option<SpatialTable> {
        self.spatial_model(id).map(SpatialCanvasModel::table)
    }
    /// Requests follow the arranged viewport and do not cancel pointer capture.
    pub fn map_tile_requests(&mut self, id: WidgetId) -> Result<Vec<MapTileRequest>, ControlError> {
        let Some(bounds) = self.runtime.bounds(id) else {
            return Ok(Vec::new());
        };
        let mut model = self
            .spatial_model(id)
            .ok_or_else(|| error("Not a Map"))?
            .clone();
        let SpatialCanvasModel::Map(map) = &mut model else {
            return Err(error("Not a Map"));
        };
        let requests = map.request_tiles(bounds).map_err(error)?;
        if !requests.is_empty() {
            self.publish_spatial(id, model, None)?;
        }
        Ok(requests)
    }
    /// A provider completion is data arrival, not a replacement of the user's
    /// view or selection. Stale tokens fail before accepted state changes.
    pub fn complete_map_tile(
        &mut self,
        id: WidgetId,
        request: &MapTileRequest,
        result: MapTileResult,
    ) -> Result<(), ControlError> {
        let mut model = self
            .spatial_model(id)
            .ok_or_else(|| error("Not a Map"))?
            .clone();
        let SpatialCanvasModel::Map(map) = &mut model else {
            return Err(error("Not a Map"));
        };
        map.complete_tile(request, result).map_err(error)?;
        self.publish_spatial(id, model, None)
    }
    pub fn spatial_semantic_value(&self, id: WidgetId) -> Option<&str> {
        self.spatial_canvases.get(&id).map(|s| s.summary.as_str())
    }
    /// Atomically replaces accepted state after validation by the model's public
    /// mutators. A failed closure leaves the live control and gesture unchanged.
    pub fn edit_spatial(
        &mut self,
        id: WidgetId,
        edit: impl FnOnce(&mut SpatialCanvasModel) -> Result<(), SpatialError>,
    ) -> Result<(), ControlError> {
        let mut model = self
            .spatial_model(id)
            .ok_or_else(|| error("Not a spatial canvas"))?
            .clone();
        edit(&mut model).map_err(error)?;
        if self.spatial_press.as_ref().is_some_and(|p| p.owner == id) {
            self.cancel_spatial_interaction()?;
        }
        self.publish_spatial(id, model, None)
    }
    fn publish_spatial(
        &mut self,
        id: WidgetId,
        model: SpatialCanvasModel,
        kind: Option<SpatialChangeKind>,
    ) -> Result<(), ControlError> {
        let summary = model.summary();
        self.measure_text(&summary, 32768.0, false)?;
        let state = self
            .spatial_canvases
            .get_mut(&id)
            .ok_or_else(|| error("Not a spatial canvas"))?;
        state.model = model;
        state.summary = summary.clone();
        state.event = kind.map(|kind| SpatialCanvasChange { id, kind, summary });
        self.scene_dirty = true;
        Ok(())
    }
    pub fn take_spatial_changes(&mut self) -> Vec<SpatialCanvasChange> {
        self.spatial_canvases
            .values_mut()
            .filter_map(|s| s.event.take())
            .collect()
    }
    pub(super) fn paint_spatial(
        &mut self,
        id: WidgetId,
        b: Rect,
        clip: Rect,
    ) -> Result<bool, ControlError> {
        let Some(state) = self.spatial_canvases.get(&id) else {
            return Ok(false);
        };
        if b.width > 0.0 && b.height > 0.0 {
            state
                .model
                .paint(
                    &mut self.scene,
                    b,
                    clip,
                    SpatialPalette::from_theme(self.theme),
                )
                .map_err(error)?;
            if self.runtime.focused() == Some(id) {
                self.scene
                    .rounded_stroke(b, clip, 6.0, 2.0, self.theme.tokens().focus);
            }
        }
        Ok(true)
    }
    pub(super) fn cancel_spatial_interaction(&mut self) -> Result<(), ControlError> {
        if let Some(p) = self.spatial_press.take() {
            if self.runtime.tree().node(p.owner).is_some()
                && self.spatial_canvases.contains_key(&p.owner)
            {
                // Map cancellation keeps request generations monotonic, even if
                // a transport completed during the pan gesture.
                let model =
                    if let (SpatialCanvasModel::Map(current), SpatialCanvasModel::Map(original)) =
                        (self.spatial_model(p.owner).unwrap(), &p.initial)
                    {
                        let mut current = current.clone();
                        current
                            .set_view(original.center(), original.zoom())
                            .map_err(error)?;
                        SpatialCanvasModel::Map(current)
                    } else {
                        p.initial
                    };
                self.publish_spatial(p.owner, model, None)?;
            }
            if self.runtime.captured() == Some(p.owner) {
                self.runtime.apply(Mutation::CapturePointer(None))?;
            }
        }
        Ok(())
    }
    pub(super) fn sync_spatial(&mut self) -> Result<(), ControlError> {
        if self
            .spatial_press
            .as_ref()
            .is_some_and(|p| !self.is_enabled(p.owner) || self.runtime.captured() != Some(p.owner))
        {
            self.cancel_spatial_interaction()?;
        }
        self.spatial_canvases
            .retain(|id, _| self.runtime.tree().node(*id).is_some());
        Ok(())
    }
    pub(super) fn spatial_before_input(
        &mut self,
        event: &InputEvent,
    ) -> Result<bool, ControlError> {
        if matches!(
            event,
            InputEvent::WindowFocused(false)
                | InputEvent::KeyDown {
                    key: Key::Escape,
                    ..
                }
        ) && self.spatial_press.is_some()
        {
            self.cancel_spatial_interaction()?;
            return Ok(matches!(event, InputEvent::KeyDown { .. }));
        }
        self.sync_spatial()?;
        if self.popup.is_some() && matches!(event, InputEvent::KeyDown { .. }) {
            return Ok(false);
        }
        if let InputEvent::PointerDown { position, .. } = event
            && self.popup.is_some()
            && self
                .runtime
                .hit_test(*position)
                .is_some_and(|id| self.spatial_canvases.contains_key(&id))
        {
            self.popup_pointer_down(*position)?;
            self.update(self.viewport)?;
        }
        if self.spatial_press.is_some() {
            match event {
                InputEvent::PointerMoved { position } => {
                    self.move_spatial_pointer(*position)?;
                    return Ok(true);
                }
                InputEvent::PointerUp {
                    position, button, ..
                } if self
                    .spatial_press
                    .as_ref()
                    .is_some_and(|p| p.button == *button) =>
                {
                    self.finish_spatial_pointer(*position)?;
                    return Ok(true);
                }
                InputEvent::PointerDown { .. } => return Ok(true),
                InputEvent::PointerUp { .. } => return Ok(true),
                InputEvent::KeyDown { .. } | InputEvent::Wheel { .. } => {
                    self.cancel_spatial_interaction()?
                }
                _ => {}
            }
        }
        match event {
            InputEvent::PointerDown {
                position,
                button,
                modifiers,
            } if matches!(
                button,
                PointerButton::Primary | PointerButton::Middle | PointerButton::Secondary
            ) =>
            {
                let Some(id) = self
                    .runtime
                    .hit_test(*position)
                    .filter(|id| self.spatial_canvases.contains_key(id) && self.is_enabled(*id))
                else {
                    return Ok(false);
                };
                if !self.try_popup_focus(id)? {
                    return Ok(false);
                }
                self.synchronize_editor_focus()?;
                let bounds = self.runtime.bounds(id).unwrap();
                let mut model = self.spatial_model(id).unwrap().clone();
                let mut hit = None;
                if let SpatialCanvasModel::Diagram(d) = &mut model
                    && *button == PointerButton::Primary
                {
                    hit = d.hit_test(*position, bounds);
                    match hit {
                        Some(DiagramHit::Node(node) | DiagramHit::Resize(node)) => {
                            let mut ids = d.selected().clone();
                            if modifiers.shift {
                                if !ids.insert(node) {
                                    ids.remove(&node);
                                }
                            } else if !ids.contains(&node) {
                                ids.clear();
                                ids.insert(node);
                            }
                            d.select(ids).map_err(error)?;
                        }
                        _ if !modifiers.shift => d.select([]).map_err(error)?,
                        _ => {}
                    }
                    self.publish_spatial(id, model.clone(), Some(SpatialChangeKind::Selection))?;
                }
                self.runtime.apply(Mutation::CapturePointer(Some(id)))?;
                self.spatial_press = Some(SpatialPress {
                    owner: id,
                    start: *position,
                    last: *position,
                    button: *button,
                    initial: model,
                    hit,
                    moved: false,
                });
                Ok(true)
            }
            InputEvent::Wheel {
                position, delta, ..
            } => {
                let Some(id) = self
                    .runtime
                    .hit_test(*position)
                    .filter(|id| self.spatial_canvases.contains_key(id) && self.is_enabled(*id))
                else {
                    return Ok(false);
                };
                if !delta.is_finite() {
                    return Ok(true);
                }
                let b = self.runtime.bounds(id).unwrap();
                let mut m = self.spatial_model(id).unwrap().clone();
                let amount = (-delta.y / 240.0).clamp(-2.0, 2.0);
                match &mut m {
                    SpatialCanvasModel::Chart3D(c) => {
                        c.zoom(f64::from(amount).exp()).map_err(error)?
                    }
                    SpatialCanvasModel::Map(map) => map
                        .zoom_at(f64::from(-amount), *position, b)
                        .map_err(error)?,
                    SpatialCanvasModel::Diagram(d) => {
                        d.zoom_at((-amount).exp(), *position, b).map_err(error)?
                    }
                }
                self.publish_spatial(id, m, Some(SpatialChangeKind::View))?;
                Ok(true)
            }
            InputEvent::KeyDown { key, modifiers, .. }
                if !modifiers.alt && !modifiers.super_key =>
            {
                let Some(id) = self
                    .runtime
                    .focused()
                    .filter(|id| self.spatial_canvases.contains_key(id) && self.is_enabled(*id))
                else {
                    return Ok(false);
                };
                let b = self.runtime.bounds(id).unwrap();
                let mut model = self.spatial_model(id).unwrap().clone();
                let changed = match &mut model {
                    SpatialCanvasModel::Chart3D(c) => chart_key(c, key, *modifiers)?,
                    SpatialCanvasModel::Map(m) => map_key(m, key, *modifiers, b)?,
                    SpatialCanvasModel::Diagram(d) => diagram_key(d, key, *modifiers, b)?,
                };
                if let Some(kind) = changed {
                    self.publish_spatial(id, model, Some(kind))?;
                    Ok(true)
                } else {
                    Ok(false)
                }
            }
            _ => Ok(false),
        }
    }
    fn move_spatial_pointer(&mut self, position: Point) -> Result<(), ControlError> {
        if !position.is_finite() {
            return Ok(());
        }
        let p = self.spatial_press.as_ref().unwrap();
        let id = p.owner;
        let delta = Point::new(position.x - p.start.x, position.y - p.start.y);
        if delta.x.hypot(delta.y) < 3.0 && !p.moved {
            return Ok(());
        }
        let mut model = p.initial.clone();
        let mut kind = SpatialChangeKind::View;
        match &mut model {
            SpatialCanvasModel::Chart3D(c) => {
                if p.button == PointerButton::Secondary || p.button == PointerButton::Middle {
                    c.orbit(f64::from(delta.x) * 0.01, f64::from(delta.y) * 0.01)
                        .map_err(error)?;
                } else {
                    return Ok(());
                }
            }
            SpatialCanvasModel::Map(_) => {
                model = self.spatial_model(id).unwrap().clone();
                if let SpatialCanvasModel::Map(current) = &mut model {
                    current
                        .pan(Point::new(position.x - p.last.x, position.y - p.last.y))
                        .map_err(error)?;
                }
            }
            SpatialCanvasModel::Diagram(d) => {
                if p.button != PointerButton::Primary || p.hit.is_none() {
                    let pan = d.pan_offset();
                    d.set_view(Point::new(pan.x + delta.x, pan.y + delta.y), d.zoom())
                        .map_err(error)?;
                } else if !d.is_read_only() {
                    let z = d.zoom();
                    match p.hit {
                        Some(DiagramHit::Node(_)) => {
                            if d.selected()
                                .iter()
                                .any(|id| d.graph().node(*id).is_some_and(|n| n.locked))
                            {
                                return Ok(());
                            }
                            d.edit(DiagramEdit::Move {
                                nodes: d.selected().iter().copied().collect(),
                                delta: Point::new(delta.x / z, delta.y / z),
                            })
                            .map_err(error)?;
                            kind = SpatialChangeKind::Document;
                        }
                        Some(DiagramHit::Resize(node)) => {
                            let r = d.graph().node(node).unwrap().bounds;
                            d.edit(DiagramEdit::Resize {
                                node,
                                bounds: DiagramRect::new(
                                    r.x,
                                    r.y,
                                    (r.width + delta.x / z).max(32.0),
                                    (r.height + delta.y / z).max(24.0),
                                ),
                            })
                            .map_err(error)?;
                            kind = SpatialChangeKind::Document;
                        }
                        _ => {}
                    }
                }
            }
        }
        self.publish_spatial(id, model, Some(kind))?;
        let p = self.spatial_press.as_mut().unwrap();
        p.last = position;
        p.moved = true;
        Ok(())
    }
    fn finish_spatial_pointer(&mut self, position: Point) -> Result<(), ControlError> {
        let p = self.spatial_press.take().unwrap();
        if self.runtime.captured() == Some(p.owner) {
            self.runtime.apply(Mutation::CapturePointer(None))?;
        }
        if !self.is_enabled(p.owner)
            || !self
                .runtime
                .bounds(p.owner)
                .zip(self.runtime.clip(p.owner))
                .and_then(|(b, c)| b.intersection(c))
                .is_some_and(|b| b.contains(position.x, position.y))
        {
            return Ok(());
        }
        if p.moved || p.button != PointerButton::Primary {
            return Ok(());
        }
        let b = self.runtime.bounds(p.owner).unwrap();
        let mut model = self.spatial_model(p.owner).unwrap().clone();
        let mut kind = SpatialChangeKind::Selection;
        match &mut model {
            SpatialCanvasModel::Chart3D(c) => {
                let g = c.geometry(b).map_err(error)?;
                let hit = g.hit_test(position);
                if hit == g.hit_test(p.start) {
                    c.select(hit).map_err(error)?;
                }
            }
            SpatialCanvasModel::Map(m) => {
                let hit = m.hit_test(position, b);
                if hit == m.hit_test(p.start, b) {
                    m.select(hit).map_err(error)?;
                }
            }
            SpatialCanvasModel::Diagram(d) => {
                if d.tool() != DiagramTool::Select && !d.is_read_only() && p.hit.is_none() {
                    let world = d.world_point(position, b);
                    let id = d
                        .graph()
                        .nodes
                        .iter()
                        .map(|n| n.id)
                        .max()
                        .unwrap_or(0)
                        .checked_add(1)
                        .ok_or_else(|| error("Diagram identity exhausted"))?;
                    let shape = match d.tool() {
                        DiagramTool::Ellipse => DiagramShape::Ellipse,
                        DiagramTool::Rectangle => DiagramShape::Rectangle,
                        _ => DiagramShape::RoundedRectangle,
                    };
                    d.edit(DiagramEdit::Insert(DiagramNode {
                        id,
                        label: format!("Shape {id}"),
                        shape,
                        bounds: DiagramRect::new(world.x, world.y, 140.0, 64.0),
                        locked: false,
                    }))
                    .map_err(error)?;
                    d.select([id]).map_err(error)?;
                    kind = SpatialChangeKind::Document;
                }
            }
        }
        self.publish_spatial(p.owner, model, Some(kind))
    }
}
fn next_key<T: Copy + PartialEq>(keys: &[T], selected: Option<T>, key: &Key) -> Option<Option<T>> {
    let index = selected.and_then(|s| keys.iter().position(|v| *v == s));
    match key {
        Key::Home => Some(keys.first().copied()),
        Key::End => Some(keys.last().copied()),
        Key::ArrowRight | Key::ArrowDown => Some(
            keys.get(
                index
                    .map(|i| (i + 1).min(keys.len().saturating_sub(1)))
                    .unwrap_or(0),
            )
            .copied(),
        ),
        Key::ArrowLeft | Key::ArrowUp => {
            Some(keys.get(index.unwrap_or(0).saturating_sub(1)).copied())
        }
        Key::Escape => Some(None),
        _ => None,
    }
}
fn direction(key: &Key) -> Option<Point> {
    match key {
        Key::ArrowLeft => Some(Point::new(-1.0, 0.0)),
        Key::ArrowRight => Some(Point::new(1.0, 0.0)),
        Key::ArrowUp => Some(Point::new(0.0, -1.0)),
        Key::ArrowDown => Some(Point::new(0.0, 1.0)),
        _ => None,
    }
}
fn chart_key(
    c: &mut Chart3D,
    key: &Key,
    m: rust_desktop_ui_core::Modifiers,
) -> Result<Option<SpatialChangeKind>, ControlError> {
    if m.control
        && let Some(d) = direction(key)
    {
        c.orbit(f64::from(d.x) * 0.1, f64::from(d.y) * 0.1)
            .map_err(error)?;
        return Ok(Some(SpatialChangeKind::View));
    }
    if matches!(key, Key::PageUp | Key::PageDown) {
        c.zoom(if *key == Key::PageUp { 0.85 } else { 1.15 })
            .map_err(error)?;
        return Ok(Some(SpatialChangeKind::View));
    }
    if !m.control {
        let keys: Vec<_> = c
            .data()
            .series()
            .iter()
            .flat_map(|s| {
                s.points
                    .iter()
                    .filter(|p| p.y.is_some())
                    .map(|p| crate::ChartPointKey {
                        series: s.id,
                        point: p.id,
                    })
            })
            .collect();
        if let Some(key) = next_key(&keys, c.selected(), key) {
            c.select(key).map_err(error)?;
            return Ok(Some(SpatialChangeKind::Selection));
        }
    }
    Ok(None)
}
fn map_key(
    map: &mut Map,
    key: &Key,
    m: rust_desktop_ui_core::Modifiers,
    b: Rect,
) -> Result<Option<SpatialChangeKind>, ControlError> {
    if m.control
        && let Some(d) = direction(key)
    {
        map.pan(Point::new(-d.x * 40.0, -d.y * 40.0))
            .map_err(error)?;
        return Ok(Some(SpatialChangeKind::View));
    }
    if matches!(key, Key::PageUp | Key::PageDown) {
        map.zoom_at(
            if *key == Key::PageUp { 1.0 } else { -1.0 },
            Point::new(b.x + b.width * 0.5, b.y + b.height * 0.5),
            b,
        )
        .map_err(error)?;
        return Ok(Some(SpatialChangeKind::View));
    }
    if !m.control {
        let keys: Vec<_> = map.markers().iter().map(|m| m.id).collect();
        if let Some(key) = next_key(&keys, map.selected(), key) {
            map.select(key).map_err(error)?;
            return Ok(Some(SpatialChangeKind::Selection));
        }
    }
    Ok(None)
}
fn diagram_key(
    d: &mut Diagram,
    key: &Key,
    m: rust_desktop_ui_core::Modifiers,
    b: Rect,
) -> Result<Option<SpatialChangeKind>, ControlError> {
    if m.control {
        if let Key::Character(c) = key {
            if c.eq_ignore_ascii_case("z") && !d.is_read_only() {
                if m.shift {
                    d.redo().map_err(error)?;
                } else {
                    d.undo().map_err(error)?;
                }
                return Ok(Some(SpatialChangeKind::Document));
            }
            if c.eq_ignore_ascii_case("y") && !d.is_read_only() {
                d.redo().map_err(error)?;
                return Ok(Some(SpatialChangeKind::Document));
            }
            if c.eq_ignore_ascii_case("a") {
                d.select(d.graph().nodes.iter().map(|n| n.id).collect::<Vec<_>>())
                    .map_err(error)?;
                return Ok(Some(SpatialChangeKind::Selection));
            }
        }
        if let Some(delta) = direction(key) {
            let pan = d.pan_offset();
            d.set_view(
                Point::new(pan.x - delta.x * 40.0, pan.y - delta.y * 40.0),
                d.zoom(),
            )
            .map_err(error)?;
            return Ok(Some(SpatialChangeKind::View));
        }
    }
    if matches!(key, Key::PageUp | Key::PageDown) {
        d.zoom_at(
            if *key == Key::PageUp { 1.2 } else { 1.0 / 1.2 },
            Point::new(b.x + b.width * 0.5, b.y + b.height * 0.5),
            b,
        )
        .map_err(error)?;
        return Ok(Some(SpatialChangeKind::View));
    }
    if *key == Key::Escape {
        d.select([]).map_err(error)?;
        return Ok(Some(SpatialChangeKind::Selection));
    }
    if d.selected().is_empty() || matches!(key, Key::Home | Key::End) {
        let keys: Vec<_> = d.graph().nodes.iter().map(|n| n.id).collect();
        if let Some(key) = next_key(&keys, None, key) {
            d.select(key).map_err(error)?;
            return Ok(Some(SpatialChangeKind::Selection));
        }
    }
    if !d.is_read_only()
        && !d
            .selected()
            .iter()
            .any(|id| d.graph().node(*id).is_some_and(|n| n.locked))
    {
        if *key == Key::Delete {
            d.edit(DiagramEdit::RemoveNodes(
                d.selected().iter().copied().collect(),
            ))
            .map_err(error)?;
            return Ok(Some(SpatialChangeKind::Document));
        }
        if let Some(delta) = direction(key) {
            if m.shift && d.selected().len() == 1 {
                let id = *d.selected().first().unwrap();
                let r = d.graph().node(id).unwrap().bounds;
                d.edit(DiagramEdit::Resize {
                    node: id,
                    bounds: DiagramRect::new(
                        r.x,
                        r.y,
                        (r.width + delta.x * 4.0).max(32.0),
                        (r.height + delta.y * 4.0).max(24.0),
                    ),
                })
                .map_err(error)?;
            } else {
                d.edit(DiagramEdit::Move {
                    nodes: d.selected().iter().copied().collect(),
                    delta: Point::new(delta.x * 4.0, delta.y * 4.0),
                })
                .map_err(error)?;
            }
            return Ok(Some(SpatialChangeKind::Document));
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (Controls, WidgetId) {
        let mut ui = Controls::new(Theme::FluentLight).unwrap();
        let graph = DiagramGraph {
            nodes: vec![DiagramNode {
                id: 1,
                label: "First node".into(),
                shape: DiagramShape::RoundedRectangle,
                bounds: DiagramRect::new(40.0, 40.0, 120.0, 60.0),
                locked: false,
            }],
            connectors: vec![],
        };
        let id = ui
            .diagram(
                ui.root(),
                Diagram::new(graph).unwrap(),
                LayoutStyle {
                    width: Length::Px(600.0),
                    height: Length::Px(400.0),
                    ..Default::default()
                },
            )
            .unwrap();
        ui.update(Size::new(640.0, 480.0)).unwrap();
        (ui, id)
    }
    fn down(ui: &mut Controls, p: Point) {
        ui.dispatch(InputEvent::PointerDown {
            position: p,
            button: PointerButton::Primary,
            modifiers: Default::default(),
        })
        .unwrap();
    }
    fn up(ui: &mut Controls, p: Point) {
        ui.dispatch(InputEvent::PointerUp {
            position: p,
            button: PointerButton::Primary,
            modifiers: Default::default(),
        })
        .unwrap();
    }
    #[test]
    fn retained_drag_one_undo_escape_restore_and_disable_release() {
        let (mut ui, id) = fixture();
        let b = ui.runtime.bounds(id).unwrap();
        let start = Point::new(b.x + 80.0, b.y + 60.0);
        down(&mut ui, start);
        ui.dispatch(InputEvent::PointerMoved {
            position: Point::new(start.x + 20.0, start.y + 10.0),
        })
        .unwrap();
        ui.dispatch(InputEvent::PointerMoved {
            position: Point::new(start.x + 40.0, start.y + 20.0),
        })
        .unwrap();
        up(&mut ui, Point::new(start.x + 40.0, start.y + 20.0));
        ui.edit_spatial(id, |m| {
            if let SpatialCanvasModel::Diagram(d) = m {
                assert_eq!(d.graph().nodes[0].bounds.x, 80.0);
                d.undo()?;
                assert_eq!(d.graph().nodes[0].bounds.x, 40.0);
                assert!(!d.can_undo());
            }
            Ok(())
        })
        .unwrap();
        down(&mut ui, start);
        ui.dispatch(InputEvent::PointerMoved {
            position: Point::new(start.x + 25.0, start.y),
        })
        .unwrap();
        ui.dispatch(InputEvent::KeyDown {
            key: Key::Escape,
            modifiers: Default::default(),
            repeat: false,
        })
        .unwrap();
        assert!(ui.runtime.captured().is_none());
        let SpatialCanvasModel::Diagram(d) = ui.spatial_model(id).unwrap() else {
            panic!()
        };
        assert_eq!(d.graph().nodes[0].bounds.x, 40.0);
        down(&mut ui, start);
        ui.set_enabled(id, false).unwrap();
        ui.update(Size::new(640.0, 480.0)).unwrap();
        assert!(ui.runtime.captured().is_none());
    }
    #[test]
    fn semantics_and_gpu_scene_follow_keyboard_model_and_resize() {
        let (mut ui, id) = fixture();
        ui.perform_action(id, SemanticAction::Focus, None).unwrap();
        ui.dispatch(InputEvent::KeyDown {
            key: Key::Home,
            modifiers: Default::default(),
            repeat: false,
        })
        .unwrap();
        assert!(
            ui.spatial_semantic_value(id)
                .unwrap()
                .contains("First node")
        );
        ui.dispatch(InputEvent::KeyDown {
            key: Key::ArrowRight,
            modifiers: Default::default(),
            repeat: false,
        })
        .unwrap();
        ui.update(Size::new(320.0, 240.0)).unwrap();
        assert!(!ui.scene().rounded_rectangles.is_empty());
        assert!(
            ui.semantics()
                .iter()
                .find(|n| n.id == id)
                .unwrap()
                .value
                .as_ref()
                .unwrap()
                .contains("First node")
        );
        assert_eq!(ui.spatial_table(id).unwrap().rows.len(), 1);
        ui.remove(id).unwrap();
        assert!(ui.spatial_model(id).is_none());
    }
    #[test]
    fn tile_completion_does_not_end_pan_and_cancel_retains_accepted_resources() {
        let mut ui = Controls::new(Theme::FluentLight).unwrap();
        let map = Map::new(MapProvider::new("local", "Own fixtures").unwrap()).unwrap();
        let initial = map.center();
        let id = ui
            .map_view(
                ui.root(),
                map,
                LayoutStyle {
                    width: Length::Px(600.0),
                    height: Length::Px(400.0),
                    ..Default::default()
                },
            )
            .unwrap();
        ui.update(Size::new(640.0, 480.0)).unwrap();
        let requests = ui.map_tile_requests(id).unwrap();
        let bounds = ui.runtime.bounds(id).unwrap();
        let start = Point::new(bounds.x + 100.0, bounds.y + 80.0);
        down(&mut ui, start);
        ui.complete_map_tile(
            id,
            &requests[0],
            MapTileResult::Ready(
                rust_desktop_ui_core::Bitmap::new_rgba8(128, 128, vec![255; 128 * 128 * 4])
                    .unwrap(),
            ),
        )
        .unwrap();
        assert_eq!(ui.runtime.captured(), Some(id));
        ui.dispatch(InputEvent::PointerMoved {
            position: Point::new(start.x + 30.0, start.y + 10.0),
        })
        .unwrap();
        assert!(
            ui.complete_map_tile(id, &requests[1], MapTileResult::Missing("Late".into()))
                .is_err()
        );
        ui.dispatch(InputEvent::WindowFocused(false)).unwrap();
        assert!(ui.runtime.captured().is_none());
        let SpatialCanvasModel::Map(map) = ui.spatial_model(id).unwrap() else {
            panic!()
        };
        assert_eq!(map.center(), initial);
        assert_eq!(map.diagnostics().ready_tiles, 1);
        assert_eq!(map.diagnostics().pending_tiles, 0);
    }
    #[test]
    fn secondary_press_does_not_replace_active_transaction_and_tab_cancels_drag() {
        let (mut ui, id) = fixture();
        let b = ui.runtime.bounds(id).unwrap();
        let start = Point::new(b.x + 80.0, b.y + 60.0);
        down(&mut ui, start);
        ui.dispatch(InputEvent::PointerMoved {
            position: Point::new(start.x + 20.0, start.y),
        })
        .unwrap();
        ui.dispatch(InputEvent::PointerDown {
            position: start,
            button: PointerButton::Secondary,
            modifiers: Default::default(),
        })
        .unwrap();
        ui.dispatch(InputEvent::PointerUp {
            position: start,
            button: PointerButton::Secondary,
            modifiers: Default::default(),
        })
        .unwrap();
        assert_eq!(ui.runtime.captured(), Some(id));
        ui.dispatch(InputEvent::KeyDown {
            key: Key::Tab,
            modifiers: Default::default(),
            repeat: false,
        })
        .unwrap();
        assert!(ui.runtime.captured().is_none());
        let SpatialCanvasModel::Diagram(d) = ui.spatial_model(id).unwrap() else {
            panic!()
        };
        assert_eq!(d.graph().nodes[0].bounds.x, 40.0);
        assert!(!d.can_undo());
    }
}
