//! Typed non-Cartesian chart data. No synthetic X/Y coordinates are published.
use super::chart_model::*;
use std::collections::{HashMap, HashSet};

pub const MAX_CHART_HIERARCHY_NODES: usize = 128;
pub const MAX_CHART_FLOW_LINKS: usize = 128;
pub const MAX_CHART_FLOW_NODES: usize = 64;
pub const MAX_CHART_DEPTH: usize = 8;

#[derive(Clone, Debug, PartialEq)]
pub struct ChartSlice {
    pub id: i64,
    pub label: String,
    pub value: f64,
}
#[derive(Clone, Debug, PartialEq)]
pub struct HeatmapAxisItem {
    pub id: i64,
    pub label: String,
}
#[derive(Clone, Debug, PartialEq)]
pub struct HeatmapCell {
    pub id: i64,
    pub column: i64,
    pub row: i64,
    /// A missing observation has no painted or selectable region.
    pub value: Option<f64>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct ChartHierarchyNode {
    pub id: i64,
    pub parent: Option<i64>,
    pub label: String,
    /// Leaves have a nonnegative value. Branch values are derived from leaves.
    pub value: Option<f64>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct ChartFlowNode {
    pub id: i64,
    pub label: String,
}
#[derive(Clone, Debug, PartialEq)]
pub struct ChartFlowLink {
    pub id: i64,
    pub source: i64,
    pub target: i64,
    pub value: f64,
}
/// Bounded observations for a Pie, Heatmap, Treemap/Sunburst or Sankey view.
/// Keys use namespace 0, except Sankey links which use namespace 1.
#[derive(Clone, Debug, PartialEq)]
pub enum ChartFamily {
    Pie(Vec<ChartSlice>),
    Heatmap {
        columns: Vec<HeatmapAxisItem>,
        rows: Vec<HeatmapAxisItem>,
        cells: Vec<HeatmapCell>,
    },
    Hierarchy(Vec<ChartHierarchyNode>),
    Flow {
        nodes: Vec<ChartFlowNode>,
        links: Vec<ChartFlowLink>,
    },
}

pub(super) fn valid_label(value: &str) -> Result<(), ChartError> {
    if value.len() > 256
        || value
            .chars()
            .any(|c| c.is_control() || matches!(c, '\u{2028}' | '\u{2029}'))
    {
        return Err(ChartError::InvalidLabel);
    }
    Ok(())
}
fn finite(value: f64) -> Result<(), ChartError> {
    if !value.is_finite() {
        return Err(ChartError::NonFinite);
    }
    if value < 0.0 {
        return Err(ChartError::InvalidWeight);
    }
    Ok(())
}
fn identities<'a>(
    items: impl Iterator<Item = (i64, &'a str)>,
    capacity: usize,
) -> Result<(), ChartError> {
    let mut keys = HashSet::new();
    for (id, name) in items {
        valid_label(name)?;
        if !keys.insert(id) {
            return Err(ChartError::DuplicateIdentity);
        }
        if keys.len() > capacity {
            return Err(ChartError::Capacity);
        }
    }
    Ok(())
}
pub(super) fn sum(values: impl Iterator<Item = f64>) -> Result<f64, ChartError> {
    let mut result = 0.0;
    for value in values {
        finite(value)?;
        result += value;
        if !result.is_finite() {
            return Err(ChartError::CoordinateOverflow);
        }
    }
    Ok(result)
}
impl ChartFamily {
    pub(super) fn validate(&self, view: ChartView) -> Result<(), ChartError> {
        match (self, view) {
            (Self::Pie(slices), ChartView::Pie) => {
                identities(
                    slices.iter().map(|s| (s.id, s.label.as_str())),
                    MAX_CHART_SERIES,
                )?;
                sum(slices.iter().map(|s| s.value))?;
            }
            (
                Self::Heatmap {
                    columns,
                    rows,
                    cells,
                },
                ChartView::Heatmap,
            ) => {
                identities(columns.iter().map(|s| (s.id, s.label.as_str())), 64)?;
                identities(rows.iter().map(|s| (s.id, s.label.as_str())), 64)?;
                if cells.len() > MAX_CHART_POINTS {
                    return Err(ChartError::Capacity);
                }
                let columns: HashSet<_> = columns.iter().map(|v| v.id).collect();
                let rows: HashSet<_> = rows.iter().map(|v| v.id).collect();
                let mut keys = HashSet::new();
                let mut positions = HashSet::new();
                for cell in cells {
                    if !keys.insert(cell.id) || !positions.insert((cell.column, cell.row)) {
                        return Err(ChartError::DuplicateIdentity);
                    }
                    if !columns.contains(&cell.column) || !rows.contains(&cell.row) {
                        return Err(ChartError::InvalidTopology);
                    }
                    if cell.value.is_some_and(|v| !v.is_finite()) {
                        return Err(ChartError::NonFinite);
                    }
                }
                let values: Vec<_> = cells.iter().filter_map(|c| c.value).collect();
                if let (Some(lo), Some(hi)) = (
                    values.iter().copied().reduce(f64::min),
                    values.iter().copied().reduce(f64::max),
                ) && !(hi - lo).is_finite()
                {
                    return Err(ChartError::CoordinateOverflow);
                }
            }
            (Self::Hierarchy(nodes), ChartView::Treemap | ChartView::Sunburst) => {
                hierarchy(nodes)?;
            }
            (Self::Flow { nodes, links }, ChartView::Sankey) => {
                flow(nodes, links)?;
            }
            _ => return Err(ChartError::InvalidView),
        }
        Ok(())
    }
    pub fn labels(&self) -> Vec<&str> {
        match self {
            Self::Pie(slices) => slices.iter().map(|s| s.label.as_str()).collect(),
            Self::Heatmap { columns, rows, .. } => columns
                .iter()
                .chain(rows)
                .map(|s| s.label.as_str())
                .collect(),
            Self::Hierarchy(nodes) => nodes.iter().map(|s| s.label.as_str()).collect(),
            Self::Flow { nodes, .. } => nodes.iter().map(|s| s.label.as_str()).collect(),
        }
    }
    pub(super) fn describe(&self, key: ChartPointKey) -> Option<String> {
        let number = ChartModel::format_number;
        match self {
            Self::Pie(slices) if key.series == 0 => {
                let s = slices.iter().find(|s| s.id == key.point && s.value > 0.0)?;
                let total = sum(slices.iter().map(|s| s.value)).ok()?;
                Some(format!(
                    "{}: {}; {}%",
                    s.label,
                    number(s.value),
                    number(s.value / total * 100.0)
                ))
            }
            Self::Heatmap {
                columns,
                rows,
                cells,
            } if key.series == 0 => {
                let c = cells.iter().find(|c| c.id == key.point)?;
                Some(format!(
                    "{} / {}: {}",
                    rows.iter().find(|r| r.id == c.row)?.label,
                    columns.iter().find(|r| r.id == c.column)?.label,
                    number(c.value?)
                ))
            }
            Self::Hierarchy(nodes) if key.series == 0 => {
                let info = hierarchy(nodes).ok()?;
                let index = nodes.iter().position(|n| n.id == key.point)?;
                if info.weights[index] <= 0.0 {
                    return None;
                }
                let mut path = vec![nodes[index].label.as_str()];
                let mut current = info.parents[index];
                while let Some(index) = current {
                    path.push(&nodes[index].label);
                    current = info.parents[index];
                }
                path.reverse();
                Some(format!(
                    "{}: {}",
                    path.join(" / "),
                    number(info.weights[index])
                ))
            }
            Self::Flow { nodes, links } => {
                if key.series == 0 {
                    let index = nodes.iter().position(|n| n.id == key.point)?;
                    let info = flow(nodes, links).ok()?;
                    if info.weights[index] <= 0.0 {
                        return None;
                    }
                    Some(format!(
                        "{}: вход {}; выход {}",
                        nodes[index].label,
                        number(info.incoming[index]),
                        number(info.outgoing[index])
                    ))
                } else if key.series == 1 {
                    let link = links.iter().find(|l| l.id == key.point)?;
                    Some(format!(
                        "{} → {}: {}",
                        nodes.iter().find(|n| n.id == link.source)?.label,
                        nodes.iter().find(|n| n.id == link.target)?.label,
                        number(link.value)
                    ))
                } else {
                    None
                }
            }
            _ => None,
        }
    }
}

pub(super) struct HierarchyInfo {
    pub parents: Vec<Option<usize>>,
    pub children: Vec<Vec<usize>>,
    pub roots: Vec<usize>,
    pub weights: Vec<f64>,
    pub depths: Vec<usize>,
}
pub(super) fn hierarchy(nodes: &[ChartHierarchyNode]) -> Result<HierarchyInfo, ChartError> {
    identities(
        nodes.iter().map(|n| (n.id, n.label.as_str())),
        MAX_CHART_HIERARCHY_NODES,
    )?;
    let map: HashMap<_, _> = nodes.iter().enumerate().map(|(i, n)| (n.id, i)).collect();
    let mut info = HierarchyInfo {
        parents: vec![None; nodes.len()],
        children: vec![vec![]; nodes.len()],
        roots: vec![],
        weights: vec![0.0; nodes.len()],
        depths: vec![0; nodes.len()],
    };
    for (i, n) in nodes.iter().enumerate() {
        if let Some(parent) = n.parent {
            let p = *map.get(&parent).ok_or(ChartError::InvalidTopology)?;
            info.parents[i] = Some(p);
            info.children[p].push(i);
        } else {
            info.roots.push(i);
        }
    }
    if info.roots.len() > MAX_CHART_SERIES {
        return Err(ChartError::Capacity);
    }
    if info
        .children
        .iter()
        .any(|children| children.len() > MAX_CHART_SERIES)
    {
        return Err(ChartError::Capacity);
    }
    for i in 0..nodes.len() {
        let mut current = info.parents[i];
        while let Some(p) = current {
            info.depths[i] += 1;
            if p == i || info.depths[i] >= MAX_CHART_DEPTH {
                return Err(ChartError::InvalidTopology);
            }
            current = info.parents[p];
        }
    }
    let mut order: Vec<_> = (0..nodes.len()).collect();
    order.sort_by_key(|i| std::cmp::Reverse(info.depths[*i]));
    for i in order {
        info.weights[i] = if info.children[i].is_empty() {
            let value = nodes[i].value.ok_or(ChartError::InvalidWeight)?;
            finite(value)?;
            value
        } else {
            if nodes[i].value.is_some() {
                return Err(ChartError::InvalidWeight);
            }
            sum(info.children[i].iter().map(|c| info.weights[*c]))?
        };
    }
    sum(info.roots.iter().map(|i| info.weights[*i]))?;
    Ok(info)
}
pub(super) struct FlowInfo {
    pub ranks: Vec<usize>,
    pub incoming: Vec<f64>,
    pub outgoing: Vec<f64>,
    pub weights: Vec<f64>,
    pub endpoints: Vec<(usize, usize)>,
}
pub(super) fn flow(
    nodes: &[ChartFlowNode],
    links: &[ChartFlowLink],
) -> Result<FlowInfo, ChartError> {
    identities(
        nodes.iter().map(|n| (n.id, n.label.as_str())),
        MAX_CHART_FLOW_NODES,
    )?;
    if links.len() > MAX_CHART_FLOW_LINKS {
        return Err(ChartError::Capacity);
    }
    let map: HashMap<_, _> = nodes.iter().enumerate().map(|(i, n)| (n.id, i)).collect();
    let mut info = FlowInfo {
        ranks: vec![0; nodes.len()],
        incoming: vec![0.0; nodes.len()],
        outgoing: vec![0.0; nodes.len()],
        weights: vec![0.0; nodes.len()],
        endpoints: vec![],
    };
    let mut keys = HashSet::new();
    let mut degree = vec![0; nodes.len()];
    for link in links {
        if !keys.insert(link.id) {
            return Err(ChartError::DuplicateIdentity);
        }
        finite(link.value)?;
        if link.value == 0.0 {
            return Err(ChartError::InvalidWeight);
        }
        let a = *map.get(&link.source).ok_or(ChartError::InvalidTopology)?;
        let b = *map.get(&link.target).ok_or(ChartError::InvalidTopology)?;
        if a == b {
            return Err(ChartError::InvalidTopology);
        }
        info.outgoing[a] += link.value;
        info.incoming[b] += link.value;
        info.endpoints.push((a, b));
        degree[b] += 1;
    }
    let mut queue: std::collections::VecDeque<_> =
        (0..nodes.len()).filter(|i| degree[*i] == 0).collect();
    let mut seen = 0;
    while let Some(a) = queue.pop_front() {
        seen += 1;
        for &(source, b) in &info.endpoints {
            if source == a {
                info.ranks[b] = info.ranks[b].max(info.ranks[a] + 1);
                if info.ranks[b] >= MAX_CHART_DEPTH {
                    return Err(ChartError::InvalidTopology);
                }
                degree[b] -= 1;
                if degree[b] == 0 {
                    queue.push_back(b);
                }
            }
        }
    }
    if seen != nodes.len() {
        return Err(ChartError::InvalidTopology);
    }
    for i in 0..nodes.len() {
        info.weights[i] = info.incoming[i].max(info.outgoing[i]);
    }
    sum(info.incoming.iter().chain(&info.outgoing).copied())?;
    Ok(info)
}
