//! Transactional shape/port graph with bounded routing and history.
use super::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};

pub const MAX_DIAGRAM_NODES: usize = 1024;
pub const MAX_DIAGRAM_EDGES: usize = 4096;
const MAX_DIAGRAM_HISTORY: usize = 64;
const MAX_DOCUMENT_BYTES: usize = 8 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiagramRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}
impl DiagramRect {
    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }
    pub fn bounds(self) -> Rect {
        Rect::new(self.x, self.y, self.width, self.height)
    }
    fn validate(self) -> Result<(), SpatialError> {
        if !self.bounds().is_valid()
            || self.x.abs() > 1e6
            || self.y.abs() > 1e6
            || !(32.0..=8192.0).contains(&self.width)
            || !(24.0..=8192.0).contains(&self.height)
        {
            Err(SpatialError::InvalidCoordinate)
        } else {
            Ok(())
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiagramShape {
    Rectangle,
    RoundedRectangle,
    Ellipse,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DiagramPort {
    North,
    East,
    South,
    West,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiagramNode {
    pub id: u64,
    pub label: String,
    pub shape: DiagramShape,
    pub bounds: DiagramRect,
    pub locked: bool,
}
impl DiagramNode {
    pub fn port(&self, port: DiagramPort) -> Point {
        let r = self.bounds;
        match port {
            DiagramPort::North => Point::new(r.x + r.width * 0.5, r.y),
            DiagramPort::East => Point::new(r.x + r.width, r.y + r.height * 0.5),
            DiagramPort::South => Point::new(r.x + r.width * 0.5, r.y + r.height),
            DiagramPort::West => Point::new(r.x, r.y + r.height * 0.5),
        }
    }
    pub fn hit_test(&self, p: Point) -> bool {
        let r = self.bounds;
        if !r.bounds().contains(p.x, p.y) {
            return false;
        }
        if self.shape != DiagramShape::Ellipse {
            return true;
        }
        let x = (p.x - r.x - r.width * 0.5) / (r.width * 0.5);
        let y = (p.y - r.y - r.height * 0.5) / (r.height * 0.5);
        x * x + y * y <= 1.0
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiagramEndpoint {
    pub node: u64,
    pub port: DiagramPort,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiagramConnector {
    pub id: u64,
    pub from: DiagramEndpoint,
    pub to: DiagramEndpoint,
    pub label: String,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiagramGraph {
    pub nodes: Vec<DiagramNode>,
    pub connectors: Vec<DiagramConnector>,
}
impl DiagramGraph {
    pub fn validate(&self) -> Result<(), SpatialError> {
        if self.nodes.len() > MAX_DIAGRAM_NODES || self.connectors.len() > MAX_DIAGRAM_EDGES {
            return Err(SpatialError::Capacity);
        }
        let mut ids = HashSet::new();
        for n in &self.nodes {
            if !ids.insert(n.id) {
                return Err(SpatialError::DuplicateIdentity);
            }
            valid_label(&n.label)?;
            n.bounds.validate()?;
        }
        let mut edge_ids = HashSet::new();
        for e in &self.connectors {
            if !edge_ids.insert(e.id) {
                return Err(SpatialError::DuplicateIdentity);
            }
            if !ids.contains(&e.from.node) || !ids.contains(&e.to.node) {
                return Err(SpatialError::InvalidGraph);
            }
            valid_label(&e.label)?;
        }
        Ok(())
    }
    pub fn node(&self, id: u64) -> Option<&DiagramNode> {
        self.nodes.iter().find(|n| n.id == id)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiagramTool {
    Select,
    Rectangle,
    RoundedRectangle,
    Ellipse,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiagramHit {
    Node(u64),
    Connector(u64),
    Resize(u64),
}
#[derive(Clone, Debug, PartialEq)]
pub enum DiagramEdit {
    Insert(DiagramNode),
    Connect(DiagramConnector),
    RemoveNodes(Vec<u64>),
    RemoveConnectors(Vec<u64>),
    Move {
        nodes: Vec<u64>,
        delta: Point,
    },
    Resize {
        node: u64,
        bounds: DiagramRect,
    },
    Rename {
        node: u64,
        label: String,
    },
    /// Deterministic topological columns; cyclic leftovers occupy a final column.
    Layout,
}
#[derive(Clone, Debug, PartialEq)]
pub struct DiagramRoute {
    pub connector: u64,
    pub points: Vec<Point>,
    pub blocked: bool,
}
#[derive(Clone, Debug)]
pub struct Diagram {
    graph: DiagramGraph,
    selected: BTreeSet<u64>,
    undo: VecDeque<DiagramGraph>,
    redo: VecDeque<DiagramGraph>,
    pan: Point,
    zoom: f32,
    read_only: bool,
    tool: DiagramTool,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
    version: u32,
    graph: DiagramGraph,
}
impl Diagram {
    pub fn new(graph: DiagramGraph) -> Result<Self, SpatialError> {
        graph.validate()?;
        Ok(Self {
            graph,
            selected: BTreeSet::new(),
            undo: VecDeque::new(),
            redo: VecDeque::new(),
            pan: Point::ZERO,
            zoom: 1.0,
            read_only: false,
            tool: DiagramTool::Select,
        })
    }
    pub fn graph(&self) -> &DiagramGraph {
        &self.graph
    }
    pub fn selected(&self) -> &BTreeSet<u64> {
        &self.selected
    }
    pub fn is_read_only(&self) -> bool {
        self.read_only
    }
    pub fn set_read_only(&mut self, value: bool) {
        self.read_only = value;
    }
    pub fn tool(&self) -> DiagramTool {
        self.tool
    }
    pub fn set_tool(&mut self, tool: DiagramTool) {
        self.tool = tool;
    }
    pub fn zoom(&self) -> f32 {
        self.zoom
    }
    pub fn pan_offset(&self) -> Point {
        self.pan
    }
    pub fn select(&mut self, nodes: impl IntoIterator<Item = u64>) -> Result<(), SpatialError> {
        let candidate: BTreeSet<_> = nodes.into_iter().collect();
        if candidate.iter().any(|id| self.graph.node(*id).is_none()) {
            return Err(SpatialError::UnknownIdentity);
        }
        self.selected = candidate;
        Ok(())
    }
    pub fn set_view(&mut self, pan: Point, zoom: f32) -> Result<(), SpatialError> {
        if !pan.is_finite()
            || pan.x.abs() > 1e7
            || pan.y.abs() > 1e7
            || !zoom.is_finite()
            || !(0.000001..=8.0).contains(&zoom)
        {
            return Err(SpatialError::InvalidCoordinate);
        }
        self.pan = pan;
        self.zoom = zoom;
        Ok(())
    }
    /// Center all node bounds inside the viewport, keeping logical padding.
    /// Selection, graph and history are unchanged. Empty graphs reset the view.
    /// Call again after layout/resize when the host is in fit mode; explicit
    /// pan/zoom can leave fit mode without changing document state.
    pub fn fit_to_view(&mut self, viewport: Rect, padding: f32) -> Result<(), SpatialError> {
        valid_viewport(viewport)?;
        if !padding.is_finite()
            || padding < 0.0
            || padding * 2.0 >= viewport.width.min(viewport.height)
        {
            return Err(SpatialError::InvalidRange);
        }
        if self.graph.nodes.is_empty() {
            return self.set_view(Point::ZERO, 1.0);
        }
        let mut left = f32::INFINITY;
        let mut top = f32::INFINITY;
        let mut right = f32::NEG_INFINITY;
        let mut bottom = f32::NEG_INFINITY;
        for node in &self.graph.nodes {
            let r = node.bounds;
            left = left.min(r.x);
            top = top.min(r.y);
            right = right.max(r.x + r.width);
            bottom = bottom.max(r.y + r.height);
        }
        let zoom = ((viewport.width - 2.0 * padding) / (right - left))
            .min((viewport.height - 2.0 * padding) / (bottom - top))
            .clamp(0.000001, 1.0);
        self.set_view(
            Point::new(
                (viewport.width - (right - left) * zoom) * 0.5 - left * zoom,
                (viewport.height - (bottom - top) * zoom) * 0.5 - top * zoom,
            ),
            zoom,
        )
    }
    pub fn world_point(&self, p: Point, b: Rect) -> Point {
        Point::new(
            (p.x - b.x - self.pan.x) / self.zoom,
            (p.y - b.y - self.pan.y) / self.zoom,
        )
    }
    pub fn screen_point(&self, p: Point, b: Rect) -> Point {
        Point::new(
            b.x + self.pan.x + p.x * self.zoom,
            b.y + self.pan.y + p.y * self.zoom,
        )
    }
    pub fn zoom_at(&mut self, factor: f32, anchor: Point, b: Rect) -> Result<(), SpatialError> {
        valid_viewport(b)?;
        if !factor.is_finite() || factor <= 0.0 || !anchor.is_finite() {
            return Err(SpatialError::InvalidRange);
        }
        let world = self.world_point(anchor, b);
        let zoom = (self.zoom * factor).clamp(0.000001, 8.0);
        self.set_view(
            Point::new(
                anchor.x - b.x - world.x * zoom,
                anchor.y - b.y - world.y * zoom,
            ),
            zoom,
        )
    }
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty() && !self.read_only
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty() && !self.read_only
    }
    fn save_undo(&mut self, old: DiagramGraph) {
        if self.undo.len() == MAX_DIAGRAM_HISTORY {
            self.undo.pop_front();
        }
        self.undo.push_back(old);
        self.redo.clear();
    }
    pub fn edit(&mut self, edit: DiagramEdit) -> Result<(), SpatialError> {
        if self.read_only {
            return Err(SpatialError::ReadOnly);
        }
        let mut next = self.graph.clone();
        match edit {
            DiagramEdit::Insert(node) => next.nodes.push(node),
            DiagramEdit::Connect(edge) => next.connectors.push(edge),
            DiagramEdit::RemoveNodes(nodes) => {
                let ids: HashSet<_> = nodes.into_iter().collect();
                for id in &ids {
                    let n = next.node(*id).ok_or(SpatialError::UnknownIdentity)?;
                    if n.locked {
                        return Err(SpatialError::ReadOnly);
                    }
                }
                next.nodes.retain(|n| !ids.contains(&n.id));
                next.connectors
                    .retain(|e| !ids.contains(&e.from.node) && !ids.contains(&e.to.node));
            }
            DiagramEdit::RemoveConnectors(edges) => {
                let ids: HashSet<_> = edges.into_iter().collect();
                if ids
                    .iter()
                    .any(|id| !next.connectors.iter().any(|e| e.id == *id))
                {
                    return Err(SpatialError::UnknownIdentity);
                }
                next.connectors.retain(|e| !ids.contains(&e.id));
            }
            DiagramEdit::Move { nodes, delta } => {
                if !delta.is_finite() {
                    return Err(SpatialError::InvalidCoordinate);
                }
                let ids: HashSet<_> = nodes.into_iter().collect();
                for id in ids {
                    let n = next
                        .nodes
                        .iter_mut()
                        .find(|n| n.id == id)
                        .ok_or(SpatialError::UnknownIdentity)?;
                    if n.locked {
                        return Err(SpatialError::ReadOnly);
                    }
                    n.bounds.x += delta.x;
                    n.bounds.y += delta.y;
                }
            }
            DiagramEdit::Resize { node, bounds } => {
                let n = next
                    .nodes
                    .iter_mut()
                    .find(|n| n.id == node)
                    .ok_or(SpatialError::UnknownIdentity)?;
                if n.locked {
                    return Err(SpatialError::ReadOnly);
                }
                n.bounds = bounds;
            }
            DiagramEdit::Rename { node, label } => {
                let n = next
                    .nodes
                    .iter_mut()
                    .find(|n| n.id == node)
                    .ok_or(SpatialError::UnknownIdentity)?;
                if n.locked {
                    return Err(SpatialError::ReadOnly);
                }
                n.label = label;
            }
            DiagramEdit::Layout => Self::layout(&mut next),
        }
        next.validate()?;
        if next != self.graph {
            let old = std::mem::replace(&mut self.graph, next);
            self.save_undo(old);
            self.selected.retain(|id| self.graph.node(*id).is_some());
        }
        Ok(())
    }
    fn layout(graph: &mut DiagramGraph) {
        let mut indegree: HashMap<u64, usize> = graph.nodes.iter().map(|n| (n.id, 0)).collect();
        let mut outgoing: HashMap<u64, Vec<u64>> = HashMap::new();
        for e in &graph.connectors {
            if e.from.node != e.to.node {
                *indegree.get_mut(&e.to.node).unwrap() += 1;
                outgoing.entry(e.from.node).or_default().push(e.to.node);
            }
        }
        let mut queue: BTreeSet<u64> = indegree
            .iter()
            .filter_map(|(id, n)| (*n == 0).then_some(*id))
            .collect();
        let mut depths: HashMap<u64, usize> = HashMap::new();
        while let Some(id) = queue.pop_first() {
            let depth = *depths.entry(id).or_default();
            for next in outgoing.get(&id).into_iter().flatten() {
                let d = depths.entry(*next).or_default();
                *d = (*d).max(depth + 1);
                let degree = indegree.get_mut(next).unwrap();
                *degree -= 1;
                if *degree == 0 {
                    queue.insert(*next);
                }
            }
        }
        let final_depth = depths.values().copied().max().unwrap_or(0) + 1;
        let depth_of = |id: u64| {
            if indegree[&id] > 0 {
                final_depth
            } else {
                *depths.get(&id).unwrap_or(&0)
            }
        };
        let mut widths = vec![0.0f32; final_depth + 1];
        for node in &graph.nodes {
            let depth = depth_of(node.id);
            widths[depth] = widths[depth].max(node.bounds.width);
        }
        let mut left = vec![24.0f32; widths.len()];
        for index in 1..left.len() {
            left[index] = left[index - 1] + widths[index - 1] + 64.0;
        }
        let mut rows: HashMap<usize, f32> = HashMap::new();
        for node in &mut graph.nodes {
            let depth = depth_of(node.id);
            let row = rows.entry(depth).or_insert(24.0);
            if !node.locked {
                node.bounds.x = left[depth];
                node.bounds.y = *row;
            }
            *row += node.bounds.height + 48.0;
        }
    }
    pub fn undo(&mut self) -> Result<bool, SpatialError> {
        if self.read_only {
            return Err(SpatialError::ReadOnly);
        }
        let Some(graph) = self.undo.pop_back() else {
            return Ok(false);
        };
        self.redo
            .push_back(std::mem::replace(&mut self.graph, graph));
        self.selected.retain(|id| self.graph.node(*id).is_some());
        Ok(true)
    }
    pub fn redo(&mut self) -> Result<bool, SpatialError> {
        if self.read_only {
            return Err(SpatialError::ReadOnly);
        }
        let Some(graph) = self.redo.pop_back() else {
            return Ok(false);
        };
        self.undo
            .push_back(std::mem::replace(&mut self.graph, graph));
        self.selected.retain(|id| self.graph.node(*id).is_some());
        Ok(true)
    }
    /// Strict versioned JSON; unknown members, duplicate IDs, nonfinite/invalid
    /// bounds and dangling endpoints fail before replacing the live document.
    pub fn to_json(&self) -> Result<String, SpatialError> {
        serde_json::to_string_pretty(&Document {
            version: 1,
            graph: self.graph.clone(),
        })
        .map_err(|_| SpatialError::InvalidDocument)
    }
    pub fn from_json(value: &str) -> Result<Self, SpatialError> {
        if value.len() > MAX_DOCUMENT_BYTES {
            return Err(SpatialError::Capacity);
        }
        let d: Document = serde_json::from_str(value).map_err(|_| SpatialError::InvalidDocument)?;
        if d.version != 1 {
            return Err(SpatialError::InvalidDocument);
        }
        Self::new(d.graph)
    }
    pub fn restore_json(&mut self, value: &str) -> Result<(), SpatialError> {
        if self.read_only {
            return Err(SpatialError::ReadOnly);
        }
        let next = Self::from_json(value)?;
        if self.graph != next.graph {
            let old = std::mem::replace(&mut self.graph, next.graph);
            self.save_undo(old);
            self.selected.retain(|id| self.graph.node(*id).is_some());
        }
        Ok(())
    }
    fn escape(node: &DiagramNode, port: DiagramPort) -> Point {
        let p = node.port(port);
        match port {
            DiagramPort::North => Point::new(p.x, p.y - 16.0),
            DiagramPort::East => Point::new(p.x + 16.0, p.y),
            DiagramPort::South => Point::new(p.x, p.y + 16.0),
            DiagramPort::West => Point::new(p.x - 16.0, p.y),
        }
    }
    fn segment_blocked(&self, a: Point, b: Point) -> bool {
        self.segment_blocked_except(a, b, None)
    }
    fn segment_blocked_except(&self, a: Point, b: Point, except: Option<u64>) -> bool {
        self.graph
            .nodes
            .iter()
            .filter(|n| Some(n.id) != except)
            .any(|n| {
                let r = n.bounds;
                let left = r.x - 4.0;
                let right = r.x + r.width + 4.0;
                let top = r.y - 4.0;
                let bottom = r.y + r.height + 4.0;
                if (a.x - b.x).abs() < 0.01 {
                    a.x > left && a.x < right && a.y.max(b.y) > top && a.y.min(b.y) < bottom
                } else if (a.y - b.y).abs() < 0.01 {
                    a.y > top && a.y < bottom && a.x.max(b.x) > left && a.x.min(b.x) < right
                } else {
                    true
                }
            })
    }
    pub fn route(&self, id: u64) -> Result<DiagramRoute, SpatialError> {
        let e = self
            .graph
            .connectors
            .iter()
            .find(|e| e.id == id)
            .ok_or(SpatialError::UnknownIdentity)?;
        let from = self.graph.node(e.from.node).unwrap();
        let to = self.graph.node(e.to.node).unwrap();
        let start = from.port(e.from.port);
        let end = to.port(e.to.port);
        let a = Self::escape(from, e.from.port);
        let b = Self::escape(to, e.to.port);
        let blocked_exit = self.segment_blocked_except(start, a, Some(from.id))
            || self.segment_blocked_except(b, end, Some(to.id));
        let mut candidates = vec![
            vec![a, Point::new(b.x, a.y), b],
            vec![a, Point::new(a.x, b.y), b],
        ];
        if !blocked_exit
            && let Some(path) = candidates
                .iter()
                .find(|path| path.windows(2).all(|p| !self.segment_blocked(p[0], p[1])))
        {
            let mut points = vec![start];
            points.extend(path.iter().copied());
            points.push(end);
            points.dedup();
            return Ok(DiagramRoute {
                connector: id,
                points,
                blocked: false,
            });
        }
        let mut xs = vec![(a.x + b.x) * 0.5];
        let mut ys = vec![(a.y + b.y) * 0.5];
        // Bounded candidate corridors include nearby obstacles and global extents.
        let min_x = self
            .graph
            .nodes
            .iter()
            .map(|n| n.bounds.x)
            .reduce(f32::min)
            .unwrap_or(0.0)
            - 24.0;
        let max_x = self
            .graph
            .nodes
            .iter()
            .map(|n| n.bounds.x + n.bounds.width)
            .reduce(f32::max)
            .unwrap_or(0.0)
            + 24.0;
        let min_y = self
            .graph
            .nodes
            .iter()
            .map(|n| n.bounds.y)
            .reduce(f32::min)
            .unwrap_or(0.0)
            - 24.0;
        let max_y = self
            .graph
            .nodes
            .iter()
            .map(|n| n.bounds.y + n.bounds.height)
            .reduce(f32::max)
            .unwrap_or(0.0)
            + 24.0;
        xs.extend([min_x, max_x]);
        ys.extend([min_y, max_y]);
        let mut nearby: Vec<_> = self
            .graph
            .nodes
            .iter()
            .map(|n| {
                let r = n.bounds;
                let c = Point::new(r.x + r.width * 0.5, r.y + r.height * 0.5);
                (distance(c, a) + distance(c, b), r)
            })
            .collect();
        nearby.sort_by(|a, b| a.0.total_cmp(&b.0));
        for (_, r) in nearby.into_iter().take(24) {
            xs.extend([r.x - 12.0, r.x + r.width + 12.0]);
            ys.extend([r.y - 12.0, r.y + r.height + 12.0]);
        }
        for x in xs {
            candidates.push(vec![a, Point::new(x, a.y), Point::new(x, b.y), b]);
        }
        for y in ys {
            candidates.push(vec![a, Point::new(a.x, y), Point::new(b.x, y), b]);
        }
        let best = candidates
            .into_iter()
            .filter(|path| {
                !blocked_exit && path.windows(2).all(|p| !self.segment_blocked(p[0], p[1]))
            })
            .min_by(|a, b| {
                a.windows(2)
                    .map(|p| distance(p[0], p[1]))
                    .sum::<f32>()
                    .total_cmp(&b.windows(2).map(|p| distance(p[0], p[1])).sum::<f32>())
            });
        let blocked = best.is_none();
        let mut points = vec![start];
        points.extend(best.unwrap_or_else(|| vec![a, Point::new(b.x, a.y), b]));
        points.push(end);
        points.dedup();
        Ok(DiagramRoute {
            connector: id,
            points,
            blocked,
        })
    }
    pub fn hit_test(&self, p: Point, b: Rect) -> Option<DiagramHit> {
        if !b.contains(p.x, p.y) {
            return None;
        }
        let w = self.world_point(p, b);
        for n in self.graph.nodes.iter().rev() {
            let r = n.bounds;
            if self.selected.contains(&n.id)
                && !n.locked
                && !self.read_only
                && (w.x - r.x - r.width).abs() <= 7.0 / self.zoom
                && (w.y - r.y - r.height).abs() <= 7.0 / self.zoom
            {
                return Some(DiagramHit::Resize(n.id));
            }
            if n.hit_test(w) {
                return Some(DiagramHit::Node(n.id));
            }
        }
        for e in self.graph.connectors.iter().rev() {
            if let Ok(route) = self.route(e.id)
                && route
                    .points
                    .windows(2)
                    .any(|p| segment_distance(w, p[0], p[1]) <= 5.0 / self.zoom)
            {
                return Some(DiagramHit::Connector(e.id));
            }
        }
        None
    }
    pub fn table(&self) -> SpatialTable {
        SpatialTable {
            columns: vec!["Shape".into(), "X".into(), "Y".into(), "Connections".into()],
            rows: self
                .graph
                .nodes
                .iter()
                .map(|n| {
                    vec![
                        n.label.clone(),
                        n.bounds.x.to_string(),
                        n.bounds.y.to_string(),
                        self.graph
                            .connectors
                            .iter()
                            .filter(|e| e.from.node == n.id || e.to.node == n.id)
                            .map(|e| e.label.as_str())
                            .collect::<Vec<_>>()
                            .join(", "),
                    ]
                })
                .collect(),
        }
    }
    pub fn summary(&self) -> String {
        let selected = self
            .graph
            .nodes
            .iter()
            .filter(|n| self.selected.contains(&n.id))
            .take(4)
            .map(|n| n.label.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            "Diagram: {} shapes, {} connectors. Selected {}: {}. {}",
            self.graph.nodes.len(),
            self.graph.connectors.len(),
            self.selected.len(),
            selected,
            if self.read_only {
                "Read only."
            } else {
                "Arrows move; Shift+arrows resize; Delete removes; Control+Z/Y undo/redo."
            }
        )
    }
    fn connector_label_bounds(&self, route: &DiagramRoute, b: Rect, caption: &str) -> Option<Rect> {
        if caption.is_empty() {
            return None;
        }
        // Port exits split a straight route into several short pieces. Merge
        // those pieces before choosing a segment, never a vertex near its end.
        let mut points: Vec<Point> = Vec::with_capacity(route.points.len());
        for point in &route.points {
            let point = self.screen_point(*point, b);
            while points.len() >= 2 {
                let a = points[points.len() - 2];
                let c = points[points.len() - 1];
                let same_horizontal = (a.y - c.y).abs() < 0.01
                    && (c.y - point.y).abs() < 0.01
                    && (c.x - a.x) * (point.x - c.x) >= 0.0;
                let same_vertical = (a.x - c.x).abs() < 0.01
                    && (c.x - point.x).abs() < 0.01
                    && (c.y - a.y) * (point.y - c.y) >= 0.0;
                if !same_horizontal && !same_vertical {
                    break;
                }
                points.pop();
            }
            points.push(point);
        }
        let mut segments: Vec<_> = points.windows(2).collect();
        segments.sort_by(|a, c| distance(c[0], c[1]).total_cmp(&distance(a[0], a[1])));
        let desired_width = (caption.chars().count() as f32 * 7.0 + 4.0).clamp(24.0, 180.0);
        let height = 18.0;
        for segment in segments {
            let a = segment[0];
            let c = segment[1];
            let length = distance(a, c);
            let horizontal = (a.y - c.y).abs() < 0.01;
            if length < if horizontal { 32.0 } else { height + 8.0 } {
                continue;
            }
            let width = if horizontal {
                desired_width.min(length - 8.0)
            } else {
                desired_width
            };
            for fraction in [0.5, 0.25, 0.75] {
                let center = Point::new(a.x + (c.x - a.x) * fraction, a.y + (c.y - a.y) * fraction);
                for side in [-1.0, 1.0] {
                    let candidate = if horizontal {
                        Rect::new(
                            (center.x - width * 0.5)
                                .max(a.x.min(c.x) + 4.0)
                                .min(a.x.max(c.x) - width - 4.0),
                            if side < 0.0 {
                                center.y - height - 4.0
                            } else {
                                center.y + 4.0
                            },
                            width,
                            height,
                        )
                    } else {
                        Rect::new(
                            if side < 0.0 {
                                center.x + 4.0
                            } else {
                                center.x - width - 4.0
                            },
                            (center.y - height * 0.5)
                                .max(a.y.min(c.y) + 4.0)
                                .min(a.y.max(c.y) - height - 4.0),
                            width,
                            height,
                        )
                    };
                    if candidate.x < b.x
                        || candidate.y < b.y
                        || candidate.x + candidate.width > b.x + b.width
                        || candidate.y + candidate.height > b.y + b.height
                    {
                        continue;
                    }
                    let overlaps_node = self.graph.nodes.iter().any(|node| {
                        let origin = self.screen_point(Point::new(node.bounds.x, node.bounds.y), b);
                        candidate
                            .intersection(Rect::new(
                                origin.x - 2.0,
                                origin.y - 2.0,
                                node.bounds.width * self.zoom + 4.0,
                                node.bounds.height * self.zoom + 4.0,
                            ))
                            .is_some()
                    });
                    if !overlaps_node {
                        return Some(candidate);
                    }
                }
            }
        }
        // Dense/offscreen routes retain their complete labels in table data.
        // Painting no caption is preferable to putting it underneath a node.
        None
    }
    pub fn paint(
        &self,
        scene: &mut Scene,
        b: Rect,
        clip: Rect,
        p: SpatialPalette,
    ) -> Result<(), SpatialError> {
        valid_viewport(b)?;
        scene.rounded_fill(b, clip, 6.0, p.background);
        let step = 24.0 * self.zoom;
        if step >= 8.0 {
            let mut x = b.x + self.pan.x.rem_euclid(step);
            let mut count = 0;
            while x < b.x + b.width && count < 4096 {
                line(
                    scene,
                    Point::new(x, b.y),
                    Point::new(x, b.y + b.height),
                    clip,
                    0.5,
                    p.border,
                );
                x += step;
                count += 1;
            }
            let mut y = b.y + self.pan.y.rem_euclid(step);
            while y < b.y + b.height && count < 4096 {
                line(
                    scene,
                    Point::new(b.x, y),
                    Point::new(b.x + b.width, y),
                    clip,
                    0.5,
                    p.border,
                );
                y += step;
                count += 1;
            }
        }
        for e in &self.graph.connectors {
            let route = self.route(e.id)?;
            let color = if route.blocked { p.muted } else { p.accent };
            for pair in route.points.windows(2) {
                line(
                    scene,
                    self.screen_point(pair[0], b),
                    self.screen_point(pair[1], b),
                    clip,
                    2.0,
                    color,
                );
            }
            if let Some(end) = route.points.last() {
                let end = self.screen_point(*end, b);
                scene.rounded_fill(
                    Rect::new(end.x - 3.0, end.y - 3.0, 6.0, 6.0),
                    clip,
                    3.0,
                    color,
                );
            }
            let caption = if route.blocked {
                format!("{} [route blocked]", e.label)
            } else {
                e.label.clone()
            };
            if self.zoom >= 0.5
                && let Some(bounds) = self.connector_label_bounds(&route, b, &caption)
            {
                label(scene, &caption, bounds, clip, color, 11.0);
            }
        }
        for n in &self.graph.nodes {
            let origin = self.screen_point(Point::new(n.bounds.x, n.bounds.y), b);
            let r = Rect::new(
                origin.x,
                origin.y,
                n.bounds.width * self.zoom,
                n.bounds.height * self.zoom,
            );
            if r.intersection(clip).is_none() {
                continue;
            }
            if n.shape == DiagramShape::Ellipse {
                paint_ellipse(scene, r, clip, p.surface, p.border);
            } else {
                let radius = if n.shape == DiagramShape::RoundedRectangle {
                    8.0 * self.zoom
                } else {
                    0.0
                };
                scene.rounded_fill(r, clip, radius, p.surface);
                scene.rounded_stroke(r, clip, radius, 1.0, p.border);
            }
            if r.width >= 48.0 && r.height >= 24.0 {
                label(
                    scene,
                    &n.label,
                    Rect::new(
                        r.x + 8.0,
                        r.y + (r.height - 22.0) * 0.5,
                        (r.width - 16.0).max(0.0),
                        22.0,
                    ),
                    r.intersection(clip).unwrap(),
                    p.foreground,
                    (13.0 * self.zoom).clamp(8.0, 26.0),
                );
            }
            if self.selected.contains(&n.id) {
                scene.rounded_stroke(r, clip, 2.0, 2.0, p.accent);
                for port in [
                    DiagramPort::North,
                    DiagramPort::East,
                    DiagramPort::South,
                    DiagramPort::West,
                ] {
                    let c = self.screen_point(n.port(port), b);
                    scene.rounded_fill(
                        Rect::new(c.x - 3.0, c.y - 3.0, 6.0, 6.0),
                        clip,
                        3.0,
                        p.accent,
                    );
                }
                if !n.locked && !self.read_only {
                    scene.rounded_fill(
                        Rect::new(r.x + r.width - 5.0, r.y + r.height - 5.0, 10.0, 10.0),
                        clip,
                        1.0,
                        p.accent,
                    );
                }
            }
        }
        Ok(())
    }
}
fn segment_distance(p: Point, a: Point, b: Point) -> f32 {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let denom = dx * dx + dy * dy;
    if denom <= f32::EPSILON {
        return distance(p, a);
    }
    let t = ((p.x - a.x) * dx + (p.y - a.y) * dy) / denom;
    distance(
        p,
        Point::new(a.x + t.clamp(0.0, 1.0) * dx, a.y + t.clamp(0.0, 1.0) * dy),
    )
}
fn paint_ellipse(scene: &mut Scene, r: Rect, clip: Rect, fill: Color, border: Color) {
    scene.ellipse(r, clip, None, fill);
    scene.ellipse(r, clip, Some(1.0), border);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fit_large_graph_preserves_document_history_and_centers_extents() {
        let nodes = (0..500)
            .map(|i| {
                node(
                    i,
                    -50000.0 + i as f32 * 180.0,
                    -3000.0 + (i % 20) as f32 * 100.0,
                )
            })
            .collect();
        let mut diagram = Diagram::new(DiagramGraph {
            nodes,
            connectors: vec![],
        })
        .unwrap();
        diagram.select([499]).unwrap();
        let before = diagram.to_json().unwrap();
        for b in [
            Rect::new(30.0, 40.0, 640.0, 400.0),
            Rect::new(10.0, 10.0, 230.0, 150.0),
        ] {
            diagram.fit_to_view(b, 20.0).unwrap();
            for n in &diagram.graph().nodes {
                let start = diagram.screen_point(Point::new(n.bounds.x, n.bounds.y), b);
                let end = diagram.screen_point(
                    Point::new(n.bounds.x + n.bounds.width, n.bounds.y + n.bounds.height),
                    b,
                );
                assert!(
                    start.x >= b.x + 19.9
                        && start.y >= b.y + 19.9
                        && end.x <= b.x + b.width - 19.9
                        && end.y <= b.y + b.height - 19.9
                );
            }
            assert_eq!(
                diagram.selected().iter().copied().collect::<Vec<_>>(),
                [499]
            );
            assert!(!diagram.can_undo());
        }
        assert_eq!(
            diagram.graph(),
            Diagram::from_json(&before).unwrap().graph()
        );
        let pan = diagram.pan_offset();
        assert!(
            diagram
                .fit_to_view(Rect::new(0.0, 0.0, 20.0, 20.0), 20.0)
                .is_err()
        );
        assert_eq!(diagram.pan_offset(), pan);
    }
    fn node(id: u64, x: f32, y: f32) -> DiagramNode {
        DiagramNode {
            id,
            label: format!("Node {id}"),
            shape: DiagramShape::RoundedRectangle,
            bounds: DiagramRect::new(x, y, 120.0, 60.0),
            locked: false,
        }
    }
    fn graph() -> DiagramGraph {
        DiagramGraph {
            nodes: vec![node(1, 0.0, 0.0), node(2, 220.0, 0.0)],
            connectors: vec![DiagramConnector {
                id: 1,
                from: DiagramEndpoint {
                    node: 1,
                    port: DiagramPort::East,
                },
                to: DiagramEndpoint {
                    node: 2,
                    port: DiagramPort::West,
                },
                label: "Connection".into(),
            }],
        }
    }
    #[test]
    fn deletion_and_move_keep_endpoint_identity_and_undo() {
        let mut d = Diagram::new(graph()).unwrap();
        let old = d.graph.clone();
        d.edit(DiagramEdit::Move {
            nodes: vec![1],
            delta: Point::new(20.0, 50.0),
        })
        .unwrap();
        assert_eq!(d.graph.connectors, old.connectors);
        assert!(!d.route(1).unwrap().blocked);
        d.edit(DiagramEdit::RemoveNodes(vec![1])).unwrap();
        assert!(d.graph.connectors.is_empty());
        d.undo().unwrap();
        assert_eq!(d.graph.connectors, old.connectors);
        d.undo().unwrap();
        assert_eq!(d.graph, old);
        d.redo().unwrap();
        assert_eq!(d.graph.nodes[0].bounds.x, 20.0);
    }
    #[test]
    fn invalid_transaction_restore_and_locked_batch_leave_graph_unchanged() {
        let mut g = graph();
        g.nodes[1].locked = true;
        let mut d = Diagram::new(g.clone()).unwrap();
        assert!(
            d.edit(DiagramEdit::Move {
                nodes: vec![1, 2],
                delta: Point::new(10.0, 0.0)
            })
            .is_err()
        );
        assert_eq!(d.graph, g);
        assert!(!d.can_undo());
        assert!(
            d.restore_json("{\"version\":2,\"graph\":{\"nodes\":[],\"connectors\":[]}}")
                .is_err()
        );
        assert_eq!(d.graph, g);
        let mut dangling = g.clone();
        dangling.connectors[0].to.node = 99;
        assert!(Diagram::new(dangling).is_err());
    }
    #[test]
    fn routing_avoids_obstacle_or_explicitly_diagnoses_blockage() {
        let mut g = graph();
        g.nodes[1].bounds.x = 400.0;
        g.nodes.push(node(3, 180.0, 0.0));
        let d = Diagram::new(g).unwrap();
        let route = d.route(1).unwrap();
        assert!(!route.blocked);
        for pair in route.points[1..route.points.len() - 1].windows(2) {
            assert!(!d.segment_blocked(pair[0], pair[1]));
        }
    }
    #[test]
    fn connector_caption_uses_free_segment_and_never_paints_under_nodes() {
        let viewport = Rect::new(10.0, 20.0, 900.0, 700.0);
        let mut g = graph();
        g.nodes[0].bounds = DiagramRect::new(40.0, 40.0, 120.0, 60.0);
        g.nodes[1].bounds = DiagramRect::new(225.0, 40.0, 120.0, 60.0);
        g.connectors[0].label = "Данные".into();
        for (vertical, obstacle) in [(false, false), (false, true), (true, false)] {
            let mut graph = g.clone();
            if vertical {
                graph.nodes[1].bounds = DiagramRect::new(40.0, 200.0, 120.0, 60.0);
                graph.connectors[0].from.port = DiagramPort::South;
                graph.connectors[0].to.port = DiagramPort::North;
            } else if obstacle {
                let mut other = node(3, 168.0, 42.0);
                other.bounds.width = 40.0;
                other.bounds.height = 24.0;
                graph.nodes.push(other);
            }
            let mut diagram = Diagram::new(graph).unwrap();
            for zoom in [1.0, 1.5, 2.0] {
                diagram.set_view(Point::new(7.0, 5.0), zoom).unwrap();
                let mut scene = Scene::default();
                diagram
                    .paint(
                        &mut scene,
                        viewport,
                        viewport,
                        SpatialPalette::from_theme(crate::Theme::Light),
                    )
                    .unwrap();
                let caption = scene
                    .texts
                    .iter()
                    .find(|run| run.text == "Данные")
                    .expect("caption has room on the route");
                assert_eq!(caption.bounds.intersection(viewport), Some(caption.bounds));
                assert_eq!(caption.clip, caption.bounds);
                for node in &diagram.graph.nodes {
                    let origin =
                        diagram.screen_point(Point::new(node.bounds.x, node.bounds.y), viewport);
                    let bounds = Rect::new(
                        origin.x,
                        origin.y,
                        node.bounds.width * zoom,
                        node.bounds.height * zoom,
                    );
                    assert!(
                        caption.bounds.intersection(bounds).is_none(),
                        "{vertical}/{obstacle}/{zoom}: {caption:?} overlaps {bounds:?}"
                    );
                }
                if !vertical {
                    let start = diagram
                        .screen_point(diagram.graph.nodes[0].port(DiagramPort::East), viewport);
                    let end = diagram
                        .screen_point(diagram.graph.nodes[1].port(DiagramPort::West), viewport);
                    assert!(caption.bounds.x > start.x);
                    assert!(caption.bounds.x + caption.bounds.width < end.x);
                    if obstacle {
                        assert!(
                            caption.bounds.y > start.y,
                            "obstructed upper candidate must use the lower side"
                        );
                    }
                }
            }
        }
    }
    #[test]
    fn large_graph_serialization_layout_selection_and_history_are_bounded() {
        let nodes = (0..500)
            .map(|i| node(i, (i % 20) as f32 * 180.0, (i / 20) as f32 * 100.0))
            .collect();
        let connectors = (0..499)
            .filter(|i| i % 20 != 19)
            .map(|i| DiagramConnector {
                id: i,
                from: DiagramEndpoint {
                    node: i,
                    port: DiagramPort::East,
                },
                to: DiagramEndpoint {
                    node: i + 1,
                    port: DiagramPort::West,
                },
                label: String::new(),
            })
            .collect();
        let mut d = Diagram::new(DiagramGraph { nodes, connectors }).unwrap();
        let restored = Diagram::from_json(&d.to_json().unwrap()).unwrap();
        assert_eq!(restored.graph, d.graph);
        for e in &d.graph.connectors {
            assert!(!d.route(e.id).unwrap().blocked);
        }
        d.edit(DiagramEdit::Layout).unwrap();
        assert_eq!(d.graph.nodes.len(), 500);
        assert_eq!(d.table().rows.len(), 500);
        for _ in 0..80 {
            d.edit(DiagramEdit::Move {
                nodes: vec![0],
                delta: Point::new(1.0, 0.0),
            })
            .unwrap();
        }
        assert_eq!(d.undo.len(), MAX_DIAGRAM_HISTORY);
    }
    #[test]
    fn viewer_and_zoom_keep_data_unchanged() {
        let mut d = Diagram::new(graph()).unwrap();
        let b = Rect::new(0.0, 0.0, 600.0, 400.0);
        let a = Point::new(100.0, 80.0);
        let world = d.world_point(a, b);
        d.zoom_at(2.0, a, b).unwrap();
        assert!(distance(a, d.screen_point(world, b)) < 0.001);
        d.set_read_only(true);
        assert_eq!(
            d.edit(DiagramEdit::RemoveNodes(vec![1])),
            Err(SpatialError::ReadOnly)
        );
        assert!(!d.can_undo());
    }
    #[test]
    fn port_exit_obstruction_is_diagnosed_and_variable_sizes_do_not_overlap_after_layout() {
        let mut g = graph();
        g.nodes.push(node(3, 125.0, 10.0));
        let d = Diagram::new(g).unwrap();
        assert!(d.route(1).unwrap().blocked);
        let mut g = graph();
        g.nodes[0].bounds.width = 350.0;
        g.nodes[0].bounds.height = 170.0;
        g.nodes.push(node(3, 40.0, 40.0));
        let mut d = Diagram::new(g).unwrap();
        d.edit(DiagramEdit::Layout).unwrap();
        let nodes = &d.graph().nodes;
        for (index, node) in nodes.iter().enumerate() {
            for other in &nodes[index + 1..] {
                assert!(
                    node.bounds
                        .bounds()
                        .intersection(other.bounds.bounds())
                        .is_none()
                );
            }
        }
        let before = d.graph().clone();
        let rect = before.nodes[0].bounds;
        d.edit(DiagramEdit::Resize {
            node: 1,
            bounds: DiagramRect::new(rect.x, rect.y, 200.0, 100.0),
        })
        .unwrap();
        assert_eq!(d.graph().connectors, before.connectors);
        d.undo().unwrap();
        assert_eq!(d.graph(), &before);
    }
    #[test]
    fn strict_json_unknown_fields_duplicate_ids_and_nonfinite_coordinates_are_rejected() {
        let d = Diagram::new(graph()).unwrap();
        let json = d.to_json().unwrap();
        assert!(
            Diagram::from_json(&json.replacen(
                "\"version\": 1",
                "\"unexpected\": true, \"version\": 1",
                1
            ))
            .is_err()
        );
        let mut g = graph();
        g.nodes.push(g.nodes[0].clone());
        assert!(Diagram::new(g).is_err());
        let mut g = graph();
        g.nodes[0].bounds.x = f32::INFINITY;
        assert!(Diagram::new(g).is_err());
        let mut g = graph();
        g.connectors[0].from.node = 999;
        assert!(Diagram::new(g).is_err());
    }
}
