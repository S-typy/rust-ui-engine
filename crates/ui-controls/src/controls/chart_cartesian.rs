//! Cartesian accumulation and clipped area strips. No observations are synthesized.
use super::*;
use rust_desktop_ui_core::{Color, FillPath, FillRule};
use std::collections::HashMap;

pub(super) type Stacks = Vec<Vec<Option<(f64, f64)>>>;

pub(super) fn validate(series: &[ChartSeries], view: ChartView) -> Result<Stacks, ChartError> {
    if matches!(view, ChartView::Area | ChartView::StackedArea) {
        if series
            .iter()
            .any(|s| s.points.windows(2).any(|p| p[0].x >= p[1].x))
        {
            return Err(ChartError::InvalidTopology);
        }
        if view == ChartView::Area {
            return Ok(vec![]);
        }
        stacked_area::validate(series)?;
    }
    if !matches!(view, ChartView::StackedBar | ChartView::StackedArea) {
        return Ok(vec![]);
    }
    let mut totals: HashMap<u64, (f64, f64)> = HashMap::new();
    let mut result = Vec::with_capacity(series.len());
    let mut unknown = HashSet::new();
    for s in series {
        let mut categories = HashSet::new();
        let mut row = Vec::with_capacity(s.points.len());
        for p in &s.points {
            let x = if p.x == 0.0 { 0 } else { p.x.to_bits() };
            if !categories.insert(x) {
                return Err(ChartError::DuplicateCategory);
            }
            let span = if view == ChartView::StackedArea && (p.y.is_none() || unknown.contains(&x))
            {
                unknown.insert(x);
                None
            } else if let Some(y) = p.y {
                let totals = totals.entry(x).or_default();
                let accumulated = if y >= 0.0 {
                    &mut totals.0
                } else {
                    &mut totals.1
                };
                let base = *accumulated;
                let end = base + y;
                if !end.is_finite() {
                    return Err(ChartError::NonFinite);
                }
                // Reject a nonzero observation lost entirely in a larger stack.
                if y != 0.0 && end == base {
                    return Err(ChartError::CoordinateOverflow);
                }
                *accumulated = end;
                Some((base, end))
            } else {
                None
            };
            row.push(span);
        }
        result.push(row);
    }
    Ok(result)
}

/// One clipped strip between adjacent area observations, possibly split at zero.
/// Pointer selection returns the nearer endpoint by X; an endpoint may lie
/// outside the viewport. NULL gaps have no strip.
#[derive(Clone, Debug, PartialEq)]
pub struct ChartArea {
    pub start: ChartPointKey,
    pub end: ChartPointKey,
    pub contour: Vec<Point>,
    /// Midpoint of original observations in plot coordinates, before clipping.
    pub split_x: f64,
}
impl ChartArea {
    pub(super) fn hit_test(&self, point: Point, plot: Rect) -> Option<ChartPointKey> {
        let path = FillPath {
            contours: vec![self.contour.clone()],
            clip: plot,
            color: Color::rgb(0, 0, 0),
            rule: FillRule::NonZero,
        };
        path.contains(point)
            .then_some(if f64::from(point.x) <= self.split_x {
                self.start
            } else {
                self.end
            })
    }
}

type XY = (f64, f64);

// Clip a convex strip to the normalized plot before conversion to f32.
fn clip(mut polygon: Vec<XY>) -> Vec<XY> {
    for (axis, edge, sign) in [(0, 0.0, 1.0), (0, 1.0, -1.0), (1, 0.0, 1.0), (1, 1.0, -1.0)] {
        let value = |p: XY| if axis == 0 { p.0 } else { p.1 };
        let mut output = Vec::new();
        if polygon.is_empty() {
            return polygon;
        }
        let mut previous = *polygon.last().unwrap();
        let mut previous_inside = (value(previous) - edge) * sign >= 0.0;
        for current in polygon {
            let inside = (value(current) - edge) * sign >= 0.0;
            if inside != previous_inside {
                let t = (edge - value(previous)) / (value(current) - value(previous));
                output.push((
                    previous.0 + t * (current.0 - previous.0),
                    previous.1 + t * (current.1 - previous.1),
                ));
            }
            if inside {
                output.push(current);
            }
            previous = current;
            previous_inside = inside;
        }
        polygon = output;
    }
    polygon
}

#[allow(clippy::too_many_arguments)]
pub(super) fn area(
    output: &mut Vec<ChartArea>,
    plot: Rect,
    a: XY,
    b: XY,
    baseline: f64,
    start: ChartPointKey,
    end: ChartPointKey,
) {
    // A sign change would make a bow-tie quadrilateral. Split at the crossing
    // so both filled portions use ordinary convex clipping.
    let split = (a.1 < baseline && b.1 > baseline) || (a.1 > baseline && b.1 < baseline);
    let intervals = if split {
        let t = (baseline - a.1) / (b.1 - a.1);
        let crossing = (a.0 + t * (b.0 - a.0), baseline);
        vec![(a, crossing), (crossing, b)]
    } else {
        vec![(a, b)]
    };
    for (left, right) in intervals {
        band(
            output,
            plot,
            [(left.0, baseline), left, right, (right.0, baseline)],
            (start, end),
            (a.0 + b.0) * 0.5,
        );
    }
}

pub(super) fn band(
    output: &mut Vec<ChartArea>,
    plot: Rect,
    corners: [XY; 4],
    keys: (ChartPointKey, ChartPointKey),
    split_x: f64,
) {
    let polygon = clip(corners.to_vec());
    let mut contour: Vec<_> = polygon
        .into_iter()
        .map(|p| {
            Point::new(
                (f64::from(plot.x) + p.0.clamp(0.0, 1.0) * f64::from(plot.width)) as f32,
                (f64::from(plot.y) + (1.0 - p.1.clamp(0.0, 1.0)) * f64::from(plot.height)) as f32,
            )
        })
        .collect();
    contour.dedup();
    if contour.first() == contour.last() {
        contour.pop();
    }
    let path = FillPath {
        contours: vec![contour.clone()],
        clip: plot,
        rule: FillRule::NonZero,
        color: Color::rgb(0, 0, 0),
    };
    if !path.is_valid() {
        return;
    }
    output.push(ChartArea {
        start: keys.0,
        end: keys.1,
        contour,
        split_x: f64::from(plot.x) + split_x * f64::from(plot.width),
    });
}
