//! Shared painted regions, selection anchors and hit testing for chart families.
use super::{chart_family::*, chart_model::*};
use rust_desktop_ui_core::{Color, FillPath, FillRule, Point, Rect};
use std::f64::consts::{FRAC_PI_2, TAU};

#[derive(Clone, Debug, PartialEq)]
pub enum ChartRegionShape {
    Rectangle(Rect),
    Polygon(Vec<Point>),
}
#[derive(Clone, Debug, PartialEq)]
pub struct ChartRegion {
    pub key: ChartPointKey,
    pub shape: ChartRegionShape,
    pub anchor: Point,
    pub color_index: usize,
    pub depth: usize,
    /// Heatmap intensity in `[0,1]`, independent of screen coordinates.
    pub intensity: Option<f32>,
    pub label: String,
}
impl ChartRegion {
    pub fn contains(&self, point: Point, clip: Rect) -> bool {
        match &self.shape {
            ChartRegionShape::Rectangle(bounds) => {
                clip.contains(point.x, point.y) && bounds.contains(point.x, point.y)
            }
            ChartRegionShape::Polygon(points) => {
                self.path(points, clip, Color::rgb(0, 0, 0)).contains(point)
            }
        }
    }
    pub(super) fn path(&self, points: &[Point], clip: Rect, color: Color) -> FillPath {
        FillPath {
            contours: vec![points.to_vec()],
            clip,
            rule: FillRule::NonZero,
            color,
        }
    }
}
fn key(id: i64) -> ChartPointKey {
    ChartPointKey {
        series: 0,
        point: id,
    }
}
fn center(r: Rect) -> Point {
    Point::new(r.x + r.width * 0.5, r.y + r.height * 0.5)
}
fn rectangle(id: i64, bounds: Rect, color: usize, label: String) -> ChartRegion {
    ChartRegion {
        key: key(id),
        shape: ChartRegionShape::Rectangle(bounds),
        anchor: center(bounds),
        color_index: color,
        depth: 0,
        intensity: None,
        label,
    }
}
fn polar(center: Point, radius: f64, angle: f64) -> Point {
    Point::new(
        (f64::from(center.x) + radius * angle.cos()) as f32,
        (f64::from(center.y) + radius * angle.sin()) as f32,
    )
}
fn sector(
    id: i64,
    center: Point,
    radii: (f64, f64),
    angles: (f64, f64),
    color: usize,
) -> Option<ChartRegion> {
    let (inner, outer) = radii;
    let (start, end) = angles;
    if end <= start || outer <= inner {
        return None;
    }
    // Up to 124 samples per arc, two arcs stay within the shared 256-edge budget.
    let steps = ((end - start) / TAU * 124.0).ceil().clamp(2.0, 124.0) as usize;
    let mut points = Vec::with_capacity(2 * (steps + 1));
    for i in 0..=steps {
        points.push(polar(
            center,
            outer,
            start + (end - start) * i as f64 / steps as f64,
        ));
    }
    if inner == 0.0 && end - start >= TAU - 1e-10 {
        points.pop();
    } else if inner == 0.0 {
        points.push(center);
    } else {
        for i in (0..=steps).rev() {
            points.push(polar(
                center,
                inner,
                start + (end - start) * i as f64 / steps as f64,
            ));
        }
    }
    let region = ChartRegion {
        key: key(id),
        shape: ChartRegionShape::Polygon(points),
        anchor: polar(center, (inner + outer) * 0.5, (start + end) * 0.5),
        color_index: color,
        depth: 0,
        intensity: None,
        label: String::new(),
    };
    // Very small finite fractions can collapse at f32 precision. Do not publish
    // an invalid path or a phantom keyboard target in that case.
    let ChartRegionShape::Polygon(points) = &region.shape else {
        unreachable!()
    };
    region
        .path(
            points,
            Rect::new(
                center.x - outer as f32,
                center.y - outer as f32,
                outer as f32 * 2.0,
                outer as f32 * 2.0,
            ),
            Color::rgb(0, 0, 0),
        )
        .is_valid()
        .then_some(region)
}
pub(super) fn geometry(
    family: &ChartFamily,
    view: ChartView,
    plot: Rect,
) -> Result<Vec<ChartRegion>, ChartError> {
    match family {
        ChartFamily::Pie(slices) => {
            let total = sum(slices.iter().map(|s| s.value))?;
            let mut result = vec![];
            let mut start = -FRAC_PI_2;
            for (i, slice) in slices.iter().enumerate() {
                if slice.value == 0.0 || total == 0.0 {
                    continue;
                }
                let end = start + slice.value / total * TAU;
                if let Some(region) = sector(
                    slice.id,
                    center(plot),
                    (0.0, f64::from(plot.width.min(plot.height)) * 0.5),
                    (start, end),
                    i,
                ) {
                    result.push(region);
                }
                start = end;
            }
            Ok(result)
        }
        ChartFamily::Heatmap {
            columns,
            rows,
            cells,
        } => {
            if columns.is_empty() || rows.is_empty() {
                return Ok(vec![]);
            }
            let lo = cells
                .iter()
                .filter_map(|c| c.value)
                .reduce(f64::min)
                .unwrap_or(0.0);
            let hi = cells
                .iter()
                .filter_map(|c| c.value)
                .reduce(f64::max)
                .unwrap_or(0.0);
            let width = plot.width / columns.len() as f32;
            let height = plot.height / rows.len() as f32;
            let mut result = vec![];
            for cell in cells {
                let Some(value) = cell.value else {
                    continue;
                };
                let column = columns
                    .iter()
                    .position(|c| c.id == cell.column)
                    .ok_or(ChartError::InvalidTopology)?;
                let row = rows
                    .iter()
                    .position(|r| r.id == cell.row)
                    .ok_or(ChartError::InvalidTopology)?;
                let mut region = rectangle(
                    cell.id,
                    Rect::new(
                        plot.x + column as f32 * width,
                        plot.y + row as f32 * height,
                        width,
                        height,
                    ),
                    0,
                    ChartModel::format_number(value),
                );
                region.intensity = Some(if hi == lo {
                    0.5
                } else {
                    ((value - lo) / (hi - lo)) as f32
                });
                result.push(region);
            }
            Ok(result)
        }
        ChartFamily::Hierarchy(nodes) => {
            let info = hierarchy(nodes)?;
            let mut result = vec![];
            if view == ChartView::Treemap {
                tree_rects(nodes, &info, &info.roots, plot, 0, None, &mut result)?;
            } else {
                let depth = info.depths.iter().max().copied().unwrap_or(0) + 1;
                let width = f64::from(plot.width.min(plot.height)) * 0.5 / depth as f64;
                let mut context = Radial {
                    nodes,
                    info: &info,
                    center: center(plot),
                    width,
                    result: &mut result,
                };
                context.add(&info.roots, (-FRAC_PI_2, TAU - FRAC_PI_2), 0, None)?;
            }
            Ok(result)
        }
        ChartFamily::Flow { nodes, links } => flow_regions(nodes, links, plot),
    }
}
fn tree_rects(
    nodes: &[ChartHierarchyNode],
    info: &HierarchyInfo,
    indices: &[usize],
    bounds: Rect,
    depth: usize,
    branch: Option<usize>,
    result: &mut Vec<ChartRegion>,
) -> Result<(), ChartError> {
    let total = sum(indices.iter().map(|i| info.weights[*i]))?;
    if total == 0.0 || bounds.width <= 0.0 || bounds.height <= 0.0 {
        return Ok(());
    }
    let horizontal = bounds.width >= bounds.height;
    let mut offset = 0.0f64;
    for (order, &i) in indices.iter().enumerate() {
        if info.weights[i] == 0.0 {
            continue;
        }
        let next = (offset + info.weights[i] / total).min(1.0);
        let rectangle_bounds = if horizontal {
            Rect::new(
                bounds.x + bounds.width * offset as f32,
                bounds.y,
                bounds.width * (next - offset) as f32,
                bounds.height,
            )
        } else {
            Rect::new(
                bounds.x,
                bounds.y + bounds.height * offset as f32,
                bounds.width,
                bounds.height * (next - offset) as f32,
            )
        };
        offset = next;
        if rectangle_bounds.width <= 0.0 || rectangle_bounds.height <= 0.0 {
            continue;
        }
        let color = if depth == 1 && info.roots.len() == 1 {
            order
        } else {
            branch.unwrap_or(order)
        };
        let mut region = rectangle(nodes[i].id, rectangle_bounds, color, nodes[i].label.clone());
        region.depth = depth;
        let has_children = !info.children[i].is_empty();
        if has_children {
            // Branch header remains independently visible/selectable; descendants
            // occupy an inset area and never overwrite their parent's anchor.
            let header = rectangle_bounds.height.min(22.0);
            region.anchor = Point::new(
                rectangle_bounds.x + rectangle_bounds.width * 0.5,
                rectangle_bounds.y + header * 0.5,
            );
            result.push(region);
            if depth + 1 < MAX_CHART_DEPTH
                && rectangle_bounds.width > 6.0
                && rectangle_bounds.height > header + 6.0
            {
                tree_rects(
                    nodes,
                    info,
                    &info.children[i],
                    Rect::new(
                        rectangle_bounds.x + 3.0,
                        rectangle_bounds.y + header,
                        rectangle_bounds.width - 6.0,
                        rectangle_bounds.height - header - 3.0,
                    ),
                    depth + 1,
                    Some(color),
                    result,
                )?;
            }
        } else {
            result.push(region);
        }
    }
    Ok(())
}
struct Radial<'a> {
    nodes: &'a [ChartHierarchyNode],
    info: &'a HierarchyInfo,
    center: Point,
    width: f64,
    result: &'a mut Vec<ChartRegion>,
}
impl Radial<'_> {
    fn add(
        &mut self,
        indices: &[usize],
        angles: (f64, f64),
        depth: usize,
        branch: Option<usize>,
    ) -> Result<(), ChartError> {
        let total = sum(indices.iter().map(|i| self.info.weights[*i]))?;
        if total == 0.0 {
            return Ok(());
        }
        let mut start = angles.0;
        for (order, &i) in indices.iter().enumerate() {
            if self.info.weights[i] == 0.0 {
                continue;
            }
            let end = (start + self.info.weights[i] / total * (angles.1 - angles.0)).min(angles.1);
            let color = if depth == 1 && self.info.roots.len() == 1 {
                order
            } else {
                branch.unwrap_or(order)
            };
            if let Some(mut region) = sector(
                self.nodes[i].id,
                self.center,
                (self.width * depth as f64, self.width * (depth + 1) as f64),
                (start, end),
                color,
            ) {
                region.depth = depth;
                self.result.push(region);
            }
            self.add(&self.info.children[i], (start, end), depth + 1, Some(color))?;
            start = end;
        }
        Ok(())
    }
}
fn flow_regions(
    nodes: &[ChartFlowNode],
    links: &[ChartFlowLink],
    plot: Rect,
) -> Result<Vec<ChartRegion>, ChartError> {
    let info = flow(nodes, links)?;
    let ranks = info.ranks.iter().max().copied().unwrap_or(0) + 1;
    let mut columns = vec![vec![]; ranks];
    for (i, rank) in info.ranks.iter().enumerate() {
        if info.weights[i] > 0.0 {
            columns[*rank].push(i);
        }
    }
    let largest_column = columns.iter().map(Vec::len).max().unwrap_or(0);
    let gap = 50.0f32
        .min(plot.height * 0.12)
        .min(plot.height / (largest_column + 1) as f32 * 0.35);
    // Reduce in f64 before multiplying by pixel extents. Direct pixel/weight
    // division overflows for perfectly valid subnormal observations.
    let unit = info.weights.iter().copied().reduce(f64::max).unwrap_or(0.0);
    if unit == 0.0 {
        return Ok(vec![]);
    }
    let weights: Vec<_> = info.weights.iter().map(|w| w / unit).collect();
    let mut scale = f64::INFINITY;
    for indices in &columns {
        let total = sum(indices.iter().map(|i| weights[*i]))?;
        if total > 0.0 {
            scale = scale.min(
                f64::from(plot.height * 0.65 - gap * indices.len().saturating_sub(1) as f32)
                    / total,
            );
        }
    }
    if !scale.is_finite() {
        return Ok(vec![]);
    }
    let width = 16.0f32.min(plot.width / (ranks * 3) as f32);
    let mut rects = vec![Rect::new(0.0, 0.0, 0.0, 0.0); nodes.len()];
    for (rank, indices) in columns.iter().enumerate() {
        let total_height = sum(indices.iter().map(|i| weights[*i]))? * scale
            + f64::from(gap) * indices.len().saturating_sub(1) as f64;
        let mut y = f64::from(plot.y) + (f64::from(plot.height) - total_height) * 0.5;
        let x = if ranks == 1 {
            plot.x + (plot.width - width) * 0.5
        } else {
            plot.x + (plot.width - width) * rank as f32 / (ranks - 1) as f32
        };
        for &i in indices {
            let height = weights[i] * scale;
            rects[i] = Rect::new(x, y as f32, width, height as f32);
            y += height + f64::from(gap);
        }
    }
    let mut incoming = vec![0.0; nodes.len()];
    let mut outgoing = incoming.clone();
    let mut result = vec![];
    for (index, link) in links.iter().enumerate() {
        let (a, b) = info.endpoints[index];
        let left = rects[a];
        let right = rects[b];
        let start = Point::new(left.x + left.width, left.y + (outgoing[a] * scale) as f32);
        let end = Point::new(right.x, right.y + (incoming[b] * scale) as f32);
        outgoing[a] += link.value / unit;
        incoming[b] += link.value / unit;
        let height = (link.value / unit * scale) as f32;
        if height <= 0.0 || end.x <= start.x {
            continue;
        }
        let curve = |t: f32, offset: f32| {
            let middle = (start.x + end.x) * 0.5;
            let u = 1.0 - t;
            Point::new(
                u * u * u * start.x + 3.0 * u * t * middle + t * t * t * end.x,
                u * u * (u + 3.0 * t) * start.y + t * t * (3.0 * u + t) * end.y + offset,
            )
        };
        let mut points: Vec<_> = (0..=24).map(|i| curve(i as f32 / 24.0, 0.0)).collect();
        points.extend((0..=24).rev().map(|i| curve(i as f32 / 24.0, height)));
        result.push(ChartRegion {
            key: ChartPointKey {
                series: 1,
                point: link.id,
            },
            shape: ChartRegionShape::Polygon(points),
            anchor: curve(0.5, height * 0.5),
            color_index: a,
            depth: 0,
            intensity: None,
            label: String::new(),
        });
    }
    // Nodes paint last, and hit testing uses reverse order.
    for (i, node) in nodes.iter().enumerate() {
        if rects[i].width > 0.0 && rects[i].height > 0.0 {
            result.push(rectangle(node.id, rects[i], i, node.label.clone()));
        }
    }
    Ok(result)
}
