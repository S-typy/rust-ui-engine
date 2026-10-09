//! Retained scene construction and synchronous event routing.

use std::{collections::HashMap, error::Error, fmt};

use crate::{
    DirtyFlags, EventContext, EventPhase, InputEvent, Key, LayoutEngine, LayoutError,
    LayoutSnapshot, LayoutStyle, NodeProps, Paint, Point, PointerButton, Rect, RoutedEvent, Scene,
    Size, TreeError, UiTree, WidgetId,
};

/// An operation accepted from either the host or a deferred event callback.
#[derive(Debug, Clone)]
pub enum Mutation {
    Remove(WidgetId),
    Reparent { node: WidgetId, parent: WidgetId },
    SetProps { node: WidgetId, props: NodeProps },
    SetStyle { node: WidgetId, style: LayoutStyle },
    SetPaint { node: WidgetId, paint: Paint },
    Focus(Option<WidgetId>),
    CapturePointer(Option<WidgetId>),
}

#[derive(Debug)]
pub enum RuntimeError {
    Tree(TreeError),
    Layout(LayoutError),
    InvalidViewport,
    InvalidLayout(&'static str),
    InvalidInput(&'static str),
    IneligibleFocus(WidgetId),
    IneligibleCapture(WidgetId),
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Tree(error) => write!(f, "tree operation failed: {error}"),
            Self::Layout(error) => write!(f, "layout failed: {error}"),
            Self::InvalidViewport => {
                write!(f, "viewport dimensions must be finite and nonnegative")
            }
            Self::InvalidLayout(message) => write!(f, "invalid layout snapshot: {message}"),
            Self::InvalidInput(message) => write!(f, "invalid input: {message}"),
            Self::IneligibleFocus(id) => write!(f, "node {id:?} cannot receive focus"),
            Self::IneligibleCapture(id) => write!(f, "node {id:?} cannot capture the pointer"),
        }
    }
}

impl Error for RuntimeError {}

impl From<TreeError> for RuntimeError {
    fn from(value: TreeError) -> Self {
        Self::Tree(value)
    }
}

impl From<LayoutError> for RuntimeError {
    fn from(value: LayoutError) -> Self {
        Self::Layout(value)
    }
}

/// Work performed by `update`, excluding the renderer and platform event loop.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct FrameStats {
    pub layout_passes: u64,
    pub measured_nodes: u64,
    pub arranged_nodes: u64,
    pub paint_passes: u64,
    pub painted_nodes: u64,
    pub semantics_nodes: u64,
}

impl FrameStats {
    fn accumulate(&mut self, other: Self) {
        self.layout_passes += other.layout_passes;
        self.measured_nodes += other.measured_nodes;
        self.arranged_nodes += other.arranged_nodes;
        self.paint_passes += other.paint_passes;
        self.painted_nodes += other.painted_nodes;
        self.semantics_nodes += other.semantics_nodes;
    }
}

#[derive(Debug, Default)]
pub struct DispatchReport {
    pub target: Option<WidgetId>,
    pub deliveries: usize,
    pub default_prevented: bool,
    pub propagation_stopped: bool,
    /// Deferred operations run in FIFO order; a rejected operation does not
    /// prevent the following independent operations from being attempted.
    pub errors: Vec<RuntimeError>,
}

#[derive(Clone, Copy)]
struct VisualNode {
    id: WidgetId,
    bounds: Rect,
    clip: Rect,
    enabled: bool,
}

type Handler = Box<dyn FnMut(&RoutedEvent, &mut EventContext)>;

/// Owns the tree and its cached layout and rectangle scene.
///
/// Call `update` after mutations and before rendering or routing input against
/// newly laid out nodes. Input callbacks can only queue mutations, so a route
/// never observes a partially changed tree.
pub struct UiRuntime {
    tree: UiTree,
    snapshot: Option<LayoutSnapshot>,
    viewport: Option<Size>,
    visual: Vec<VisualNode>,
    geometry_dirty: bool,
    handlers: HashMap<WidgetId, Handler>,
    scene: Scene,
    focus: Option<WidgetId>,
    hover: Option<WidgetId>,
    capture: Option<WidgetId>,
    pointer: Option<Point>,
    window_focused: bool,
    stats: FrameStats,
}

impl UiRuntime {
    pub fn new(tree: UiTree) -> Self {
        Self {
            tree,
            snapshot: None,
            viewport: None,
            visual: Vec::new(),
            geometry_dirty: true,
            handlers: HashMap::new(),
            scene: Scene::default(),
            focus: None,
            hover: None,
            capture: None,
            pointer: None,
            window_focused: true,
            stats: FrameStats::default(),
        }
    }

    pub fn tree(&self) -> &UiTree {
        &self.tree
    }
    pub fn scene(&self) -> &Scene {
        &self.scene
    }
    pub fn focused(&self) -> Option<WidgetId> {
        self.focus
    }
    pub fn hovered(&self) -> Option<WidgetId> {
        self.hover
    }
    pub fn captured(&self) -> Option<WidgetId> {
        self.capture
    }
    pub fn stats(&self) -> FrameStats {
        self.stats
    }

    /// A changed viewport also requires an update, even when this is false.
    pub fn needs_update(&self) -> bool {
        self.snapshot.is_none() || self.geometry_dirty || !self.tree.pending_dirty().is_empty()
    }

    /// Last resolved global border box. Hidden nodes have no visual box.
    pub fn bounds(&self, id: WidgetId) -> Option<Rect> {
        self.visual
            .iter()
            .find(|node| node.id == id)
            .map(|node| node.bounds)
    }

    /// Last resolved ancestor clip in logical global coordinates.
    pub fn clip(&self, id: WidgetId) -> Option<Rect> {
        self.visual
            .iter()
            .find(|node| node.id == id)
            .map(|node| node.clip)
    }

    /// Hit testing and painting use the same stable sibling z-order and clip.
    pub fn hit_test(&self, point: Point) -> Option<WidgetId> {
        self.visual
            .iter()
            .rev()
            .find(|visual| {
                visual.enabled
                    && visual.bounds.contains(point.x, point.y)
                    && visual.clip.contains(point.x, point.y)
                    && self
                        .tree
                        .node(visual.id)
                        .is_some_and(|node| node.props.hit_test)
            })
            .map(|node| node.id)
    }

    pub fn on(
        &mut self,
        id: WidgetId,
        handler: impl FnMut(&RoutedEvent, &mut EventContext) + 'static,
    ) -> Result<(), RuntimeError> {
        if self.tree.node(id).is_none() {
            return Err(TreeError::InvalidId(id).into());
        }
        self.handlers.insert(id, Box::new(handler));
        Ok(())
    }

    pub fn insert(&mut self, parent: WidgetId, props: NodeProps) -> Result<WidgetId, RuntimeError> {
        let id = self.tree.insert(parent, props)?;
        self.geometry_dirty = true;
        Ok(id)
    }

    pub fn apply(&mut self, mutation: Mutation) -> Result<(), RuntimeError> {
        match mutation {
            Mutation::Remove(id) => {
                let removed = self.tree.remove(id)?;
                for id in removed {
                    self.handlers.remove(&id);
                }
                self.geometry_dirty = true;
            }
            Mutation::Reparent { node, parent } => {
                self.tree.reparent(node, parent)?;
                self.geometry_dirty = true;
            }
            Mutation::SetProps { node, props } => {
                let changed = self
                    .tree
                    .node(node)
                    .is_none_or(|current| current.props != props);
                self.tree.set_props(node, props)?;
                self.geometry_dirty |= changed;
            }
            Mutation::SetStyle { node, style } => {
                let changed = self
                    .tree
                    .node(node)
                    .is_none_or(|current| current.props.style != style);
                self.tree.set_style(node, style)?;
                self.geometry_dirty |= changed;
            }
            Mutation::SetPaint { node, paint } => self.tree.set_paint(node, paint)?,
            Mutation::Focus(id) => {
                if let Some(id) = id
                    && !self.focus_eligible(id)
                {
                    return Err(RuntimeError::IneligibleFocus(id));
                }
                self.set_focus(id);
            }
            Mutation::CapturePointer(id) => {
                if let Some(id) = id
                    && (!self.window_focused || !self.effective_enabled(id))
                {
                    return Err(RuntimeError::IneligibleCapture(id));
                }
                self.capture = id;
            }
        }
        self.clean_interaction_state();
        Ok(())
    }

    pub fn update(
        &mut self,
        engine: &mut impl LayoutEngine,
        viewport: Size,
    ) -> Result<FrameStats, RuntimeError> {
        if !viewport.width.is_finite()
            || !viewport.height.is_finite()
            || viewport.width < 0.0
            || viewport.height < 0.0
        {
            return Err(RuntimeError::InvalidViewport);
        }
        let mut delta = FrameStats::default();
        let layout_needed = self.snapshot.is_none()
            || self.viewport != Some(viewport)
            || self
                .tree
                .pending_dirty()
                .intersects(DirtyFlags::MEASURE | DirtyFlags::LAYOUT);
        if !layout_needed && !self.geometry_dirty && self.tree.pending_dirty().is_empty() {
            return Ok(delta);
        }
        if layout_needed {
            let snapshot = engine.compute(&self.tree, viewport)?;
            self.validate_snapshot(&snapshot, viewport)?;
            delta.measured_nodes = snapshot.measured_nodes as u64;
            delta.arranged_nodes = snapshot.arranged_nodes as u64;
            self.snapshot = Some(snapshot);
            self.viewport = Some(viewport);
            self.geometry_dirty = true;
            delta.layout_passes = 1;
        }
        self.clean_interaction_state();
        let paint_needed =
            self.geometry_dirty || self.tree.pending_dirty().intersects(DirtyFlags::PAINT);
        if paint_needed {
            if self.geometry_dirty {
                self.resolve_geometry()?;
            }
            self.refresh_hover();
            self.rebuild_scene();
            delta.paint_passes = 1;
            delta.painted_nodes = self.visual.len() as u64;
        }
        delta.semantics_nodes = self
            .tree
            .preorder()
            .into_iter()
            .filter(|id| self.tree.dirty(*id).intersects(DirtyFlags::SEMANTICS))
            .count() as u64;
        self.tree.clear_dirty();
        self.stats.accumulate(delta);
        Ok(delta)
    }

    pub fn dispatch(&mut self, input: InputEvent) -> DispatchReport {
        let mut report = DispatchReport::default();
        if !valid_input(&input) {
            report.errors.push(RuntimeError::InvalidInput(
                "coordinates and deltas must be finite",
            ));
            return report;
        }
        if input == InputEvent::WindowFocused(true) {
            self.window_focused = true;
        }
        self.clean_interaction_state();
        if self.geometry_dirty
            && let Err(error) = self.resolve_geometry()
        {
            report.errors.push(error);
            return report;
        }
        let previous_hover = self.hover;
        match &input {
            InputEvent::PointerMoved { position }
            | InputEvent::PointerDown { position, .. }
            | InputEvent::PointerUp { position, .. }
            | InputEvent::Wheel { position, .. } => self.pointer = Some(*position),
            InputEvent::PointerLeft => self.pointer = None,
            _ => {}
        }
        self.refresh_hover();
        let target = match &input {
            InputEvent::KeyDown { .. }
            | InputEvent::KeyUp { .. }
            | InputEvent::WindowFocused(_) => self.focus.or(Some(self.tree.root())),
            InputEvent::PointerLeft => self.capture.or(previous_hover),
            _ => self.capture.or(self.hover),
        };
        report.target = target;
        let mut context = EventContext::default();
        if let Some(target) = target {
            let mut ancestors = Vec::new();
            let mut parent = self.tree.parent(target);
            while let Some(id) = parent {
                ancestors.push(id);
                parent = self.tree.parent(id);
            }
            let route = ancestors
                .iter()
                .rev()
                .map(|id| (*id, EventPhase::Capture))
                .chain(std::iter::once((target, EventPhase::Target)))
                .chain(ancestors.iter().map(|id| (*id, EventPhase::Bubble)))
                .collect::<Vec<_>>();
            for (current_target, phase) in route {
                if context.propagation_stopped {
                    break;
                }
                if let Some(handler) = self.handlers.get_mut(&current_target) {
                    handler(
                        &RoutedEvent {
                            input: input.clone(),
                            target,
                            current_target,
                            phase,
                        },
                        &mut context,
                    );
                    report.deliveries += 1;
                }
            }
        }
        report.default_prevented = context.default_prevented;
        report.propagation_stopped = context.propagation_stopped;
        // Default actions are settled first; explicit callback commands then
        // take precedence. Neither can change the route already delivered.
        if !context.default_prevented {
            match &input {
                InputEvent::PointerDown {
                    button: PointerButton::Primary,
                    ..
                } => {
                    let mut candidate = target;
                    while let Some(id) = candidate {
                        if self.focus_eligible(id) {
                            break;
                        }
                        candidate = self.tree.parent(id);
                    }
                    self.set_focus(candidate);
                }
                InputEvent::KeyDown {
                    key: Key::Tab,
                    modifiers,
                    ..
                } if !modifiers.control && !modifiers.alt && !modifiers.super_key => {
                    self.advance_focus(modifiers.shift);
                }
                _ => {}
            }
        }
        for command in context.commands {
            if let Err(error) = self.apply(command) {
                report.errors.push(error);
            }
        }
        match input {
            InputEvent::PointerUp {
                button: PointerButton::Primary,
                ..
            } => self.capture = None,
            InputEvent::WindowFocused(focused) => {
                self.window_focused = focused;
                if !focused {
                    self.set_focus(None);
                    self.capture = None;
                    self.pointer = None;
                    self.set_hover(None);
                }
            }
            _ => {}
        }
        self.clean_interaction_state();
        report
    }

    fn effective_enabled(&self, id: WidgetId) -> bool {
        let mut current = Some(id);
        while let Some(id) = current {
            let Some(node) = self.tree.node(id) else {
                return false;
            };
            if !node.props.visible || !node.props.enabled {
                return false;
            }
            current = self.tree.parent(id);
        }
        true
    }

    fn focus_eligible(&self, id: WidgetId) -> bool {
        self.window_focused
            && self.effective_enabled(id)
            && self.tree.node(id).is_some_and(|node| node.props.focusable)
    }

    fn clean_interaction_state(&mut self) {
        if self.focus.is_some_and(|id| !self.focus_eligible(id)) {
            self.set_focus(None);
        }
        if self
            .capture
            .is_some_and(|id| !self.window_focused || !self.effective_enabled(id))
        {
            self.capture = None;
        }
        if self.hover.is_some_and(|id| !self.effective_enabled(id)) {
            self.set_hover(None);
        }
    }

    fn set_focus(&mut self, next: Option<WidgetId>) {
        if self.focus == next {
            return;
        }
        for id in self.focus.into_iter().chain(next) {
            let _ = self
                .tree
                .mark_dirty(id, DirtyFlags::PAINT | DirtyFlags::SEMANTICS);
        }
        self.focus = next;
    }

    fn set_hover(&mut self, next: Option<WidgetId>) {
        if self.hover == next {
            return;
        }
        for id in self.hover.into_iter().chain(next) {
            if self
                .tree
                .node(id)
                .is_some_and(|node| node.props.paint.hover_background.is_some())
            {
                let _ = self.tree.mark_dirty(id, DirtyFlags::PAINT);
            }
        }
        self.hover = next;
    }

    fn refresh_hover(&mut self) {
        let next = self.pointer.and_then(|position| self.hit_test(position));
        self.set_hover(next);
    }

    fn advance_focus(&mut self, backwards: bool) {
        let candidates = self
            .tree
            .preorder()
            .into_iter()
            .filter(|id| self.focus_eligible(*id))
            .collect::<Vec<_>>();
        if candidates.is_empty() {
            self.set_focus(None);
            return;
        }
        let next = match self
            .focus
            .and_then(|id| candidates.iter().position(|candidate| *candidate == id))
        {
            Some(index) if backwards => (index + candidates.len() - 1) % candidates.len(),
            Some(index) => (index + 1) % candidates.len(),
            None if backwards => candidates.len() - 1,
            None => 0,
        };
        self.set_focus(Some(candidates[next]));
    }

    fn validate_snapshot(
        &self,
        snapshot: &LayoutSnapshot,
        viewport: Size,
    ) -> Result<(), RuntimeError> {
        for id in self.tree.preorder() {
            let Some(rect) = snapshot.boxes.get(&id) else {
                return Err(RuntimeError::InvalidLayout("missing node border box"));
            };
            if !valid_rect(*rect) {
                return Err(RuntimeError::InvalidLayout(
                    "border boxes must be finite and nonnegative",
                ));
            }
        }
        if snapshot.boxes.get(&self.tree.root())
            != Some(&Rect::new(0.0, 0.0, viewport.width, viewport.height))
        {
            return Err(RuntimeError::InvalidLayout("root must fill the viewport"));
        }
        Ok(())
    }

    fn resolve_geometry(&mut self) -> Result<(), RuntimeError> {
        let (Some(snapshot), Some(viewport)) = (&self.snapshot, self.viewport) else {
            self.visual.clear();
            self.geometry_dirty = false;
            return Ok(());
        };
        let mut visual = Vec::with_capacity(self.visual.len());
        let viewport = Rect::new(0.0, 0.0, viewport.width, viewport.height);
        let mut stack = vec![(self.tree.root(), Point::default(), viewport, true)];
        while let Some((id, origin, clip, inherited_enabled)) = stack.pop() {
            let Some(node) = self.tree.node(id) else {
                continue;
            };
            if !node.props.visible {
                continue;
            }
            let Some(local) = snapshot.boxes.get(&id) else {
                continue;
            };
            let bounds = Rect::new(
                origin.x + local.x + node.props.translation.x,
                origin.y + local.y + node.props.translation.y,
                local.width,
                local.height,
            );
            if !valid_rect(bounds) {
                return Err(RuntimeError::InvalidLayout("global geometry overflow"));
            }
            let enabled = inherited_enabled && node.props.enabled;
            visual.push(VisualNode {
                id,
                bounds,
                clip,
                enabled,
            });
            let child_clip = if node.props.clip {
                intersection(clip, bounds)
            } else {
                clip
            };
            let child_origin = Point {
                x: bounds.x - node.props.scroll.x,
                y: bounds.y - node.props.scroll.y,
            };
            let mut children = self.tree.children(id).unwrap_or_default().to_vec();
            children.sort_by_key(|child| {
                self.tree
                    .node(*child)
                    .map(|node| node.props.z_index)
                    .unwrap_or_default()
            });
            for child in children.into_iter().rev() {
                stack.push((child, child_origin, child_clip, enabled));
            }
        }
        self.visual = visual;
        self.geometry_dirty = false;
        Ok(())
    }

    fn rebuild_scene(&mut self) {
        self.scene.clear();
        for visual in &self.visual {
            let Some(node) = self.tree.node(visual.id) else {
                continue;
            };
            let paint = &node.props.paint;
            let background = if self.hover == Some(visual.id) {
                paint.hover_background.or(paint.background)
            } else {
                paint.background
            };
            if let Some(color) = background {
                self.scene
                    .fill(intersection(visual.bounds, visual.clip), color);
            }
            let border = if self.focus == Some(visual.id) {
                paint.focus_border.or(paint.border)
            } else {
                paint.border
            };
            if let Some(border) = border {
                let bounds = visual.bounds;
                let width = border
                    .width
                    .min(bounds.width * 0.5)
                    .min(bounds.height * 0.5);
                for rect in [
                    Rect::new(bounds.x, bounds.y, bounds.width, width),
                    Rect::new(
                        bounds.x,
                        bounds.y + bounds.height - width,
                        bounds.width,
                        width,
                    ),
                    Rect::new(
                        bounds.x,
                        bounds.y + width,
                        width,
                        bounds.height - 2.0 * width,
                    ),
                    Rect::new(
                        bounds.x + bounds.width - width,
                        bounds.y + width,
                        width,
                        bounds.height - 2.0 * width,
                    ),
                ] {
                    self.scene
                        .fill(intersection(rect, visual.clip), border.color);
                }
            }
        }
    }
}

fn valid_rect(rect: Rect) -> bool {
    rect.x.is_finite()
        && rect.y.is_finite()
        && rect.width.is_finite()
        && rect.height.is_finite()
        && (rect.x + rect.width).is_finite()
        && (rect.y + rect.height).is_finite()
        && rect.width >= 0.0
        && rect.height >= 0.0
}

fn intersection(left: Rect, right: Rect) -> Rect {
    let x = left.x.max(right.x);
    let y = left.y.max(right.y);
    Rect::new(
        x,
        y,
        (left.x + left.width).min(right.x + right.width).max(x) - x,
        (left.y + left.height).min(right.y + right.height).max(y) - y,
    )
}

fn valid_input(input: &InputEvent) -> bool {
    let valid = |point: &Point| point.x.is_finite() && point.y.is_finite();
    match input {
        InputEvent::PointerMoved { position }
        | InputEvent::PointerDown { position, .. }
        | InputEvent::PointerUp { position, .. } => valid(position),
        InputEvent::Wheel {
            position, delta, ..
        } => valid(position) && valid(delta),
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Border, Color, Modifiers};
    use std::{cell::RefCell, rc::Rc};

    #[derive(Default)]
    struct StubLayout {
        boxes: HashMap<WidgetId, Rect>,
        calls: usize,
        fail: bool,
        omit: Option<WidgetId>,
    }

    impl LayoutEngine for StubLayout {
        fn compute(
            &mut self,
            tree: &UiTree,
            viewport: Size,
        ) -> Result<LayoutSnapshot, LayoutError> {
            self.calls += 1;
            if self.fail {
                return Err(LayoutError::Backend("intentional failure".into()));
            }
            let mut result = LayoutSnapshot::default();
            for id in tree.preorder() {
                if self.omit == Some(id) {
                    continue;
                }
                let rect = if id == tree.root() {
                    Rect::new(0.0, 0.0, viewport.width, viewport.height)
                } else {
                    self.boxes
                        .get(&id)
                        .copied()
                        .unwrap_or(Rect::new(10.0, 10.0, 40.0, 40.0))
                };
                result.boxes.insert(id, rect);
            }
            result.measured_nodes = tree.len();
            result.arranged_nodes = tree.len();
            Ok(result)
        }
    }

    fn viewport() -> Size {
        Size::new(100.0, 100.0)
    }
    fn down(x: f32, y: f32) -> InputEvent {
        InputEvent::PointerDown {
            position: Point::new(x, y),
            button: PointerButton::Primary,
            modifiers: Modifiers::default(),
        }
    }
    fn tab(shift: bool) -> InputEvent {
        InputEvent::KeyDown {
            key: Key::Tab,
            modifiers: Modifiers {
                shift,
                ..Modifiers::default()
            },
            repeat: false,
        }
    }
    fn focusable() -> NodeProps {
        NodeProps {
            focusable: true,
            ..NodeProps::default()
        }
    }
    fn background(color: Color) -> Paint {
        Paint {
            background: Some(color),
            ..Paint::default()
        }
    }

    #[test]
    fn route_has_capture_target_bubble_order_without_duplicate_target() {
        let mut tree = UiTree::new();
        let root = tree.root();
        let parent = tree.insert(root, NodeProps::default()).unwrap();
        let child = tree.insert(parent, focusable()).unwrap();
        let mut runtime = UiRuntime::new(tree);
        let deliveries = Rc::new(RefCell::new(Vec::new()));
        for id in [root, parent, child] {
            let deliveries = Rc::clone(&deliveries);
            runtime
                .on(id, move |event, _| {
                    deliveries
                        .borrow_mut()
                        .push((event.current_target, event.phase, event.target))
                })
                .unwrap();
        }
        runtime
            .update(&mut StubLayout::default(), viewport())
            .unwrap();
        let report = runtime.dispatch(down(25.0, 25.0));
        assert_eq!(report.target, Some(child));
        assert_eq!(report.deliveries, 5);
        assert_eq!(
            *deliveries.borrow(),
            [
                (root, EventPhase::Capture, child),
                (parent, EventPhase::Capture, child),
                (child, EventPhase::Target, child),
                (parent, EventPhase::Bubble, child),
                (root, EventPhase::Bubble, child),
            ]
        );
        assert_eq!(runtime.focused(), Some(child));
    }

    #[test]
    fn stopping_propagation_and_preventing_default_are_independent() {
        let mut tree = UiTree::new();
        let root = tree.root();
        let child = tree.insert(root, focusable()).unwrap();
        let mut runtime = UiRuntime::new(tree);
        runtime
            .on(root, |_, context| context.stop_propagation())
            .unwrap();
        runtime
            .on(child, |_, _| {
                panic!("target must not receive a stopped route")
            })
            .unwrap();
        runtime
            .update(&mut StubLayout::default(), viewport())
            .unwrap();
        let report = runtime.dispatch(down(20.0, 20.0));
        assert_eq!(report.deliveries, 1);
        assert!(report.propagation_stopped);
        assert!(!report.default_prevented);
        assert_eq!(runtime.focused(), Some(child));
        runtime.apply(Mutation::Focus(None)).unwrap();
        runtime
            .on(root, |_, context| {
                context.stop_propagation();
                context.prevent_default();
            })
            .unwrap();
        let report = runtime.dispatch(down(20.0, 20.0));
        assert!(report.default_prevented);
        assert_eq!(runtime.focused(), None);
    }

    #[test]
    fn removal_is_deferred_until_the_route_finishes_and_commands_are_fifo() {
        let mut tree = UiTree::new();
        let root = tree.root();
        let parent = tree.insert(root, focusable()).unwrap();
        let child = tree.insert(parent, focusable()).unwrap();
        let mut runtime = UiRuntime::new(tree);
        let phases = Rc::new(RefCell::new(Vec::new()));
        let root_phases = Rc::clone(&phases);
        runtime
            .on(root, move |event, context| {
                root_phases.borrow_mut().push(event.phase);
                if event.phase == EventPhase::Capture {
                    context.enqueue(Mutation::Remove(parent));
                    context.enqueue(Mutation::Focus(Some(child)));
                    context.enqueue(Mutation::SetPaint {
                        node: root,
                        paint: background(Color::rgb(7, 8, 9)),
                    });
                }
            })
            .unwrap();
        for id in [parent, child] {
            let phases = Rc::clone(&phases);
            runtime
                .on(id, move |event, _| phases.borrow_mut().push(event.phase))
                .unwrap();
        }
        runtime
            .update(&mut StubLayout::default(), viewport())
            .unwrap();
        runtime
            .apply(Mutation::CapturePointer(Some(child)))
            .unwrap();
        let report = runtime.dispatch(down(25.0, 25.0));
        assert_eq!(report.deliveries, 5);
        assert_eq!(phases.borrow().len(), 5);
        assert_eq!(report.errors.len(), 1);
        assert!(matches!(report.errors[0], RuntimeError::IneligibleFocus(id) if id == child));
        assert_eq!(runtime.tree().len(), 1);
        assert_eq!(runtime.focused(), None);
        assert_eq!(runtime.captured(), None);
        assert!(!runtime.handlers.contains_key(&parent));
        assert!(!runtime.handlers.contains_key(&child));
        assert_eq!(
            runtime.tree().node(root).unwrap().props.paint.background,
            Some(Color::rgb(7, 8, 9))
        );
    }

    #[test]
    fn reparenting_during_dispatch_preserves_the_original_route() {
        let mut tree = UiTree::new();
        let root = tree.root();
        let old_parent = tree.insert(root, NodeProps::default()).unwrap();
        let new_parent = tree
            .insert(
                root,
                NodeProps {
                    hit_test: false,
                    ..NodeProps::default()
                },
            )
            .unwrap();
        let child = tree.insert(old_parent, focusable()).unwrap();
        let mut runtime = UiRuntime::new(tree);
        let events = Rc::new(RefCell::new(Vec::new()));
        for id in [old_parent, new_parent, child] {
            let events = Rc::clone(&events);
            runtime
                .on(id, move |event, context| {
                    events
                        .borrow_mut()
                        .push((event.current_target, event.phase));
                    if event.current_target == child {
                        context.enqueue(Mutation::Reparent {
                            node: child,
                            parent: new_parent,
                        });
                    }
                })
                .unwrap();
        }
        runtime
            .update(&mut StubLayout::default(), viewport())
            .unwrap();
        runtime.dispatch(down(25.0, 25.0));
        assert_eq!(
            *events.borrow(),
            [
                (old_parent, EventPhase::Capture),
                (child, EventPhase::Target),
                (old_parent, EventPhase::Bubble)
            ]
        );
        assert_eq!(runtime.tree().parent(child), Some(new_parent));
    }

    #[test]
    fn focus_traverses_logical_preorder_wraps_and_skips_disabled_or_hidden_subtrees() {
        let mut tree = UiTree::new();
        let root = tree.root();
        let first = tree.insert(root, focusable()).unwrap();
        let disabled = tree
            .insert(
                root,
                NodeProps {
                    enabled: false,
                    ..focusable()
                },
            )
            .unwrap();
        tree.insert(disabled, focusable()).unwrap();
        let hidden = tree
            .insert(
                root,
                NodeProps {
                    visible: false,
                    ..focusable()
                },
            )
            .unwrap();
        tree.insert(hidden, focusable()).unwrap();
        let last = tree
            .insert(
                root,
                NodeProps {
                    z_index: -100,
                    ..focusable()
                },
            )
            .unwrap();
        let mut runtime = UiRuntime::new(tree);
        runtime
            .update(&mut StubLayout::default(), viewport())
            .unwrap();
        runtime.dispatch(tab(false));
        assert_eq!(runtime.focused(), Some(first));
        runtime.dispatch(tab(false));
        assert_eq!(runtime.focused(), Some(last));
        runtime.dispatch(tab(false));
        assert_eq!(runtime.focused(), Some(first));
        runtime.dispatch(tab(true));
        assert_eq!(runtime.focused(), Some(last));
        runtime
            .on(last, |event, context| {
                if matches!(event.input, InputEvent::KeyDown { key: Key::Tab, .. }) {
                    context.prevent_default();
                }
            })
            .unwrap();
        assert!(runtime.dispatch(tab(false)).default_prevented);
        assert_eq!(runtime.focused(), Some(last));
        runtime.apply(Mutation::Focus(None)).unwrap();
        runtime.dispatch(tab(true));
        assert_eq!(runtime.focused(), Some(last));
    }

    #[test]
    fn primary_down_focuses_the_nearest_eligible_ancestor() {
        let mut tree = UiTree::new();
        let root = tree.root();
        let parent = tree.insert(root, focusable()).unwrap();
        tree.insert(parent, NodeProps::default()).unwrap();
        let mut runtime = UiRuntime::new(tree);
        runtime
            .update(&mut StubLayout::default(), viewport())
            .unwrap();
        runtime.dispatch(InputEvent::PointerDown {
            position: Point::new(25.0, 25.0),
            button: PointerButton::Secondary,
            modifiers: Modifiers::default(),
        });
        assert_eq!(runtime.focused(), None);
        runtime.dispatch(down(25.0, 25.0));
        assert_eq!(runtime.focused(), Some(parent));
    }

    #[test]
    fn capture_routes_outside_hits_but_hover_tracks_actual_hit_and_up_releases() {
        let mut tree = UiTree::new();
        let root = tree.root();
        let child = tree.insert(root, focusable()).unwrap();
        let mut runtime = UiRuntime::new(tree);
        runtime
            .on(child, move |event, context| {
                if matches!(event.input, InputEvent::PointerDown { .. }) {
                    context.enqueue(Mutation::CapturePointer(Some(child)));
                }
                if matches!(event.input, InputEvent::PointerUp { .. }) {
                    context.prevent_default();
                    context.enqueue(Mutation::CapturePointer(Some(child)));
                }
            })
            .unwrap();
        runtime
            .update(&mut StubLayout::default(), viewport())
            .unwrap();
        runtime.dispatch(down(20.0, 20.0));
        assert_eq!(runtime.captured(), Some(child));
        let moved = runtime.dispatch(InputEvent::PointerMoved {
            position: Point::new(90.0, 90.0),
        });
        assert_eq!(moved.target, Some(child));
        assert_eq!(runtime.hovered(), Some(root));
        let up = runtime.dispatch(InputEvent::PointerUp {
            position: Point::new(150.0, 150.0),
            button: PointerButton::Primary,
            modifiers: Modifiers::default(),
        });
        assert_eq!(up.target, Some(child));
        assert_eq!(runtime.hovered(), None);
        assert_eq!(runtime.captured(), None);
    }

    #[test]
    fn blur_and_ancestor_visibility_changes_clear_focus_and_capture() {
        let mut tree = UiTree::new();
        let root = tree.root();
        let parent = tree.insert(root, NodeProps::default()).unwrap();
        let child = tree.insert(parent, focusable()).unwrap();
        let mut runtime = UiRuntime::new(tree);
        runtime
            .update(&mut StubLayout::default(), viewport())
            .unwrap();
        runtime.apply(Mutation::Focus(Some(child))).unwrap();
        runtime
            .apply(Mutation::CapturePointer(Some(child)))
            .unwrap();
        runtime.dispatch(InputEvent::WindowFocused(false));
        assert_eq!(runtime.focused(), None);
        assert_eq!(runtime.captured(), None);
        assert!(runtime.apply(Mutation::Focus(Some(child))).is_err());
        runtime.dispatch(tab(false));
        assert_eq!(runtime.focused(), None);
        runtime.dispatch(InputEvent::WindowFocused(true));
        runtime.apply(Mutation::Focus(Some(child))).unwrap();
        runtime
            .apply(Mutation::CapturePointer(Some(child)))
            .unwrap();
        let mut props = runtime.tree().node(parent).unwrap().props.clone();
        props.visible = false;
        runtime
            .apply(Mutation::SetProps {
                node: parent,
                props,
            })
            .unwrap();
        assert_eq!(runtime.focused(), None);
        assert_eq!(runtime.captured(), None);
        assert!(
            runtime
                .apply(Mutation::CapturePointer(Some(child)))
                .is_err()
        );
        runtime
            .update(&mut StubLayout::default(), viewport())
            .unwrap();
        assert_eq!(runtime.hit_test(Point::new(25.0, 25.0)), Some(root));
    }

    #[test]
    fn parent_translation_scroll_and_clip_match_painted_and_hit_regions() {
        let mut tree = UiTree::new();
        let root = tree.root();
        let parent = tree
            .insert(
                root,
                NodeProps {
                    clip: true,
                    translation: Point::new(5.0, 0.0),
                    scroll: Point::new(0.0, 10.0),
                    ..NodeProps::default()
                },
            )
            .unwrap();
        let child = tree
            .insert(
                parent,
                NodeProps {
                    paint: background(Color::rgb(255, 0, 0)),
                    ..NodeProps::default()
                },
            )
            .unwrap();
        let mut layout = StubLayout::default();
        layout
            .boxes
            .insert(parent, Rect::new(10.0, 10.0, 40.0, 40.0));
        layout.boxes.insert(child, Rect::new(0.0, 30.0, 40.0, 30.0));
        let mut runtime = UiRuntime::new(tree);
        runtime.update(&mut layout, viewport()).unwrap();
        assert_eq!(
            runtime.bounds(child),
            Some(Rect::new(15.0, 30.0, 40.0, 30.0))
        );
        assert_eq!(runtime.scene().rectangles.len(), 1);
        assert_eq!(
            runtime.scene().rectangles[0].bounds,
            Rect::new(15.0, 30.0, 40.0, 20.0)
        );
        assert_eq!(runtime.hit_test(Point::new(15.0, 30.0)), Some(child));
        assert_eq!(runtime.hit_test(Point::new(54.999, 49.999)), Some(child));
        assert_eq!(runtime.hit_test(Point::new(55.0, 35.0)), Some(root));
        assert_eq!(runtime.hit_test(Point::new(20.0, 50.0)), Some(root));
        let mut props = runtime.tree().node(parent).unwrap().props.clone();
        props.translation.x = 10.0;
        runtime
            .apply(Mutation::SetProps {
                node: parent,
                props,
            })
            .unwrap();
        assert_eq!(
            runtime
                .update(&mut layout, viewport())
                .unwrap()
                .layout_passes,
            0
        );
        assert_eq!(runtime.bounds(child).unwrap().x, 20.0);
        assert_eq!(layout.calls, 1);
    }

    #[test]
    fn stable_sibling_z_order_drives_both_paint_and_hit_testing() {
        let mut tree = UiTree::new();
        let root = tree.root();
        let red = tree
            .insert(
                root,
                NodeProps {
                    paint: background(Color::rgb(255, 0, 0)),
                    z_index: 2,
                    ..NodeProps::default()
                },
            )
            .unwrap();
        let green = tree
            .insert(
                root,
                NodeProps {
                    paint: background(Color::rgb(0, 255, 0)),
                    z_index: 1,
                    ..NodeProps::default()
                },
            )
            .unwrap();
        let blue = tree
            .insert(
                root,
                NodeProps {
                    paint: background(Color::rgb(0, 0, 255)),
                    z_index: 2,
                    ..NodeProps::default()
                },
            )
            .unwrap();
        let mut runtime = UiRuntime::new(tree);
        let mut layout = StubLayout::default();
        runtime.update(&mut layout, viewport()).unwrap();
        assert_eq!(runtime.hit_test(Point::new(20.0, 20.0)), Some(blue));
        assert_eq!(
            runtime
                .scene()
                .rectangles
                .iter()
                .map(|rect| rect.color)
                .collect::<Vec<_>>(),
            [
                Color::rgb(0, 255, 0),
                Color::rgb(255, 0, 0),
                Color::rgb(0, 0, 255)
            ]
        );
        let mut props = runtime.tree().node(red).unwrap().props.clone();
        props.z_index = 3;
        runtime
            .apply(Mutation::SetProps { node: red, props })
            .unwrap();
        assert_eq!(
            runtime
                .update(&mut layout, viewport())
                .unwrap()
                .layout_passes,
            0
        );
        assert_eq!(runtime.hit_test(Point::new(20.0, 20.0)), Some(red));
        assert_eq!(
            runtime.scene().rectangles[0].color,
            runtime
                .tree()
                .node(green)
                .unwrap()
                .props
                .paint
                .background
                .unwrap()
        );
    }

    #[test]
    fn paint_hover_focus_and_semantics_do_not_run_layout_and_idle_reuses_scene() {
        let mut tree = UiTree::new();
        let root = tree.root();
        let child = tree
            .insert(
                root,
                NodeProps {
                    paint: Paint {
                        background: Some(Color::rgb(1, 2, 3)),
                        hover_background: Some(Color::rgb(4, 5, 6)),
                        focus_border: Some(Border {
                            width: 2.0,
                            color: Color::rgb(255, 255, 0),
                        }),
                        ..Paint::default()
                    },
                    ..focusable()
                },
            )
            .unwrap();
        let mut runtime = UiRuntime::new(tree);
        let mut layout = StubLayout::default();
        let first = runtime.update(&mut layout, viewport()).unwrap();
        assert_eq!(
            first,
            FrameStats {
                layout_passes: 1,
                measured_nodes: 2,
                arranged_nodes: 2,
                paint_passes: 1,
                painted_nodes: 2,
                semantics_nodes: 2
            }
        );
        let scene_allocation = runtime.scene().rectangles.as_ptr();
        assert_eq!(
            runtime.update(&mut layout, viewport()).unwrap(),
            FrameStats::default()
        );
        assert_eq!(runtime.scene().rectangles.as_ptr(), scene_allocation);
        assert_eq!(runtime.stats(), first);
        runtime.dispatch(InputEvent::PointerMoved {
            position: Point::new(20.0, 20.0),
        });
        let hover = runtime.update(&mut layout, viewport()).unwrap();
        assert_eq!(hover.layout_passes, 0);
        assert_eq!(hover.paint_passes, 1);
        assert_eq!(runtime.scene().rectangles[0].color, Color::rgb(4, 5, 6));
        runtime.apply(Mutation::Focus(Some(child))).unwrap();
        let focus = runtime.update(&mut layout, viewport()).unwrap();
        assert_eq!(focus.layout_passes, 0);
        assert_eq!(focus.semantics_nodes, 1);
        assert_eq!(runtime.scene().rectangles.len(), 5);
        runtime
            .apply(Mutation::SetPaint {
                node: child,
                paint: background(Color::rgb(8, 8, 8)),
            })
            .unwrap();
        assert_eq!(
            runtime
                .update(&mut layout, viewport())
                .unwrap()
                .layout_passes,
            0
        );
        let props = runtime.tree().node(child).unwrap().props.clone();
        runtime
            .apply(Mutation::SetProps { node: child, props })
            .unwrap();
        assert!(!runtime.needs_update());
        assert_eq!(
            runtime.update(&mut layout, viewport()).unwrap(),
            FrameStats::default()
        );
        assert_eq!(layout.calls, 1);
        runtime
            .update(&mut layout, Size::new(200.0, 100.0))
            .unwrap();
        assert_eq!(layout.calls, 2);
        assert_eq!(runtime.stats().layout_passes, 2);
    }

    #[test]
    fn failed_layout_preserves_previous_scene_and_can_be_retried() {
        let mut tree = UiTree::new();
        let child = tree
            .insert(
                tree.root(),
                NodeProps {
                    paint: background(Color::rgb(9, 9, 9)),
                    ..NodeProps::default()
                },
            )
            .unwrap();
        let mut runtime = UiRuntime::new(tree);
        let mut layout = StubLayout::default();
        runtime.update(&mut layout, viewport()).unwrap();
        let previous = runtime.scene().clone();
        let stats = runtime.stats();
        let mut style = runtime.tree().node(child).unwrap().props.style.clone();
        style.gap = 2.0;
        runtime
            .apply(Mutation::SetStyle { node: child, style })
            .unwrap();
        layout.fail = true;
        assert!(matches!(
            runtime.update(&mut layout, viewport()),
            Err(RuntimeError::Layout(_))
        ));
        assert_eq!(runtime.scene(), &previous);
        assert_eq!(runtime.stats(), stats);
        assert!(runtime.needs_update());
        layout.fail = false;
        layout.omit = Some(child);
        assert!(matches!(
            runtime.update(&mut layout, viewport()),
            Err(RuntimeError::InvalidLayout(_))
        ));
        assert_eq!(runtime.scene(), &previous);
        layout.omit = None;
        assert_eq!(
            runtime
                .update(&mut layout, viewport())
                .unwrap()
                .layout_passes,
            1
        );
        let calls = layout.calls;
        assert!(
            runtime
                .update(&mut layout, Size::new(f32::NAN, 100.0))
                .is_err()
        );
        assert_eq!(layout.calls, calls);
    }

    #[test]
    fn invalid_coordinates_do_not_route_and_logical_character_keys_target_focus() {
        let mut tree = UiTree::new();
        let child = tree.insert(tree.root(), focusable()).unwrap();
        let mut runtime = UiRuntime::new(tree);
        let events = Rc::new(RefCell::new(Vec::new()));
        let observed = Rc::clone(&events);
        runtime
            .on(child, move |event, _| {
                observed.borrow_mut().push(event.input.clone())
            })
            .unwrap();
        runtime
            .update(&mut StubLayout::default(), viewport())
            .unwrap();
        let report = runtime.dispatch(down(f32::NAN, 20.0));
        assert_eq!(report.deliveries, 0);
        assert_eq!(report.errors.len(), 1);
        assert_eq!(runtime.focused(), None);
        runtime.apply(Mutation::Focus(Some(child))).unwrap();
        let input = InputEvent::KeyDown {
            key: Key::Character("ж".into()),
            modifiers: Modifiers::default(),
            repeat: false,
        };
        assert_eq!(runtime.dispatch(input.clone()).target, Some(child));
        assert_eq!(*events.borrow(), [input]);
    }

    #[test]
    fn stationary_pointer_hover_follows_geometry_changes_and_pointer_left_clears_it() {
        let mut tree = UiTree::new();
        let root = tree.root();
        let parent = tree
            .insert(
                root,
                NodeProps {
                    hit_test: false,
                    ..NodeProps::default()
                },
            )
            .unwrap();
        let child = tree
            .insert(
                parent,
                NodeProps {
                    paint: Paint {
                        background: Some(Color::rgb(1, 1, 1)),
                        hover_background: Some(Color::rgb(2, 2, 2)),
                        ..Paint::default()
                    },
                    ..NodeProps::default()
                },
            )
            .unwrap();
        let mut runtime = UiRuntime::new(tree);
        let mut layout = StubLayout::default();
        runtime.update(&mut layout, viewport()).unwrap();
        runtime.dispatch(InputEvent::PointerMoved {
            position: Point::new(25.0, 25.0),
        });
        runtime.update(&mut layout, viewport()).unwrap();
        assert_eq!(runtime.hovered(), Some(child));
        assert_eq!(runtime.scene().rectangles[0].color, Color::rgb(2, 2, 2));
        let mut props = runtime.tree().node(parent).unwrap().props.clone();
        props.scroll.y = 50.0;
        runtime
            .apply(Mutation::SetProps {
                node: parent,
                props,
            })
            .unwrap();
        assert_eq!(
            runtime
                .update(&mut layout, viewport())
                .unwrap()
                .layout_passes,
            0
        );
        assert_eq!(runtime.hovered(), Some(root));
        assert_eq!(runtime.scene().rectangles[0].color, Color::rgb(1, 1, 1));
        let mut props = runtime.tree().node(parent).unwrap().props.clone();
        props.scroll.y = 0.0;
        runtime
            .apply(Mutation::SetProps {
                node: parent,
                props,
            })
            .unwrap();
        runtime.update(&mut layout, viewport()).unwrap();
        assert_eq!(runtime.hovered(), Some(child));
        let left = runtime.dispatch(InputEvent::PointerLeft);
        assert_eq!(left.target, Some(child));
        assert_eq!(runtime.hovered(), None);
        runtime.update(&mut layout, viewport()).unwrap();
        assert_eq!(runtime.scene().rectangles[0].color, Color::rgb(1, 1, 1));
        assert_eq!(layout.calls, 1);
    }

    #[test]
    fn removed_node_handler_does_not_reappear_when_its_slot_is_reused() {
        let mut tree = UiTree::new();
        let root = tree.root();
        let removed = tree.insert(root, focusable()).unwrap();
        let mut runtime = UiRuntime::new(tree);
        runtime
            .on(removed, |_, _| panic!("stale handler called"))
            .unwrap();
        runtime.apply(Mutation::Focus(Some(removed))).unwrap();
        runtime
            .apply(Mutation::CapturePointer(Some(removed)))
            .unwrap();
        runtime.apply(Mutation::Remove(removed)).unwrap();
        let replacement = runtime.insert(root, focusable()).unwrap();
        assert_ne!(removed, replacement);
        runtime
            .update(&mut StubLayout::default(), viewport())
            .unwrap();
        let report = runtime.dispatch(down(20.0, 20.0));
        assert_eq!(report.target, Some(replacement));
        assert_eq!(report.deliveries, 0);
        assert_eq!(runtime.focused(), Some(replacement));
        assert_eq!(runtime.captured(), None);
    }

    #[test]
    fn focus_can_be_restored_by_a_window_focus_gain_callback() {
        let mut tree = UiTree::new();
        let root = tree.root();
        let child = tree.insert(root, focusable()).unwrap();
        let mut runtime = UiRuntime::new(tree);
        runtime
            .on(root, move |event, context| {
                if event.input == InputEvent::WindowFocused(true) {
                    context.enqueue(Mutation::Focus(Some(child)));
                }
            })
            .unwrap();
        runtime.dispatch(InputEvent::WindowFocused(false));
        let gained = runtime.dispatch(InputEvent::WindowFocused(true));
        assert!(gained.errors.is_empty());
        assert_eq!(runtime.focused(), Some(child));
    }

    #[test]
    fn accumulated_translation_overflow_reports_an_error_and_preserves_the_scene() {
        let mut tree = UiTree::new();
        let root = tree.root();
        let parent = tree.insert(root, NodeProps::default()).unwrap();
        let child = tree
            .insert(
                parent,
                NodeProps {
                    paint: background(Color::rgb(1, 1, 1)),
                    ..NodeProps::default()
                },
            )
            .unwrap();
        let mut runtime = UiRuntime::new(tree);
        let mut layout = StubLayout::default();
        runtime.update(&mut layout, viewport()).unwrap();
        let scene = runtime.scene().clone();
        for id in [parent, child] {
            let mut props = runtime.tree().node(id).unwrap().props.clone();
            props.translation.x = f32::MAX;
            runtime
                .apply(Mutation::SetProps { node: id, props })
                .unwrap();
        }
        assert!(matches!(
            runtime.update(&mut layout, viewport()),
            Err(RuntimeError::InvalidLayout("global geometry overflow"))
        ));
        assert_eq!(runtime.scene(), &scene);
        let report = runtime.dispatch(down(25.0, 25.0));
        assert_eq!(report.errors.len(), 1);
        assert_eq!(report.deliveries, 0);
        assert!(runtime.needs_update());
    }
}
