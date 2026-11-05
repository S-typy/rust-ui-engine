//! Piecewise linear, signed bands. Subdivisions are geometry, never observations.
use super::*;

// Each clipped convex strip has at most eight edges. This keeps a single
// diagram below the renderer's existing path budget, including series batching.
const MAX_STRIPS: usize = 8192;

fn crossing(a: f64, b: f64) -> Option<f64> {
    if (a < 0.0 && b > 0.0) || (a > 0.0 && b < 0.0) {
        let scale = a.abs().max(b.abs());
        let left = a.abs() / scale;
        let t = left / (left + b.abs() / scale);
        (t > 0.0 && t < 1.0).then_some(t)
    } else {
        None
    }
}

fn add_crossing(knots: &mut Vec<f64>, a: f64, b: f64) {
    if let Some(t) = crossing(a, b) {
        let index = knots.binary_search_by(|v| v.total_cmp(&t));
        if let Err(index) = index {
            knots.insert(index, t);
        }
    }
}

pub(super) fn validate(series: &[ChartSeries]) -> Result<(), ChartError> {
    let Some(first) = series.first() else {
        return Ok(());
    };
    if series.iter().any(|s| {
        s.points.len() != first.points.len()
            || s.points.iter().zip(&first.points).any(|(a, b)| a.x != b.x)
    }) {
        return Err(ChartError::InvalidTopology);
    }
    let mut strips = 0usize;
    for column in 1..first.points.len() {
        let mut knots = vec![0.0, 1.0];
        for s in series {
            let (Some(a), Some(b)) = (s.points[column - 1].y, s.points[column].y) else {
                break;
            };
            add_crossing(&mut knots, a, b);
            strips += knots.len() - 1;
            if strips > MAX_STRIPS {
                return Err(ChartError::Capacity);
            }
        }
    }
    Ok(())
}

fn interpolate(a: f64, b: f64, t: f64) -> f64 {
    // Weighted endpoints avoid overflowing b-a for very wide finite inputs.
    if t == 0.0 {
        a
    } else if t == 1.0 {
        b
    } else {
        (1.0 - t) * a + t * b
    }
}

pub(super) fn geometry(model: &ChartModel, output: &mut ChartGeometry) -> Result<(), ChartError> {
    let Some(first) = model.series.first() else {
        return Ok(());
    };
    let plot = output.plot;
    let position = |x: f64, y: f64| {
        Point::new(
            (f64::from(plot.x) + x * f64::from(plot.width)) as f32,
            (f64::from(plot.y) + (1.0 - y) * f64::from(plot.height)) as f32,
        )
    };
    for column in 1..first.points.len() {
        let left = model.x_range.fraction(first.points[column - 1].x)?;
        let right = model.x_range.fraction(first.points[column].x)?;
        let mut knots = vec![0.0, 1.0];
        let mut values = Vec::new();
        for s in &model.series {
            let (Some(a), Some(b)) = (s.points[column - 1].y, s.points[column].y) else {
                break;
            };
            add_crossing(&mut knots, a, b);
            for interval in knots.windows(2) {
                let (lo, hi) = (interval[0], interval[1]);
                let mid = (lo + hi) * 0.5;
                let positive = interpolate(a, b, mid) >= 0.0;
                let mut base = (0.0, 0.0);
                for &(p, q) in &values {
                    if (interpolate(p, q, mid) >= 0.0) == positive {
                        base.0 += interpolate(p, q, lo);
                        base.1 += interpolate(p, q, hi);
                    }
                }
                let end = (
                    base.0 + interpolate(a, b, lo),
                    base.1 + interpolate(a, b, hi),
                );
                let x0 = interpolate(left, right, lo);
                let x1 = interpolate(left, right, hi);
                let bottom = (
                    model.y_range.fraction(base.0)?,
                    model.y_range.fraction(base.1)?,
                );
                let top = (
                    model.y_range.fraction(end.0)?,
                    model.y_range.fraction(end.1)?,
                );
                cartesian::band(
                    &mut output.areas,
                    plot,
                    [(x0, bottom.0), (x0, top.0), (x1, top.1), (x1, bottom.1)],
                    (
                        ChartPointKey {
                            series: s.id,
                            point: s.points[column - 1].id,
                        },
                        ChartPointKey {
                            series: s.id,
                            point: s.points[column].id,
                        },
                    ),
                    (left + right) * 0.5,
                );
                if let Some((start, end)) = clip_segment((x0, top.0), (x1, top.1)) {
                    output.segments.push(ChartLine {
                        series: s.id,
                        start: position(start.0, start.1),
                        end: position(end.0, end.1),
                    });
                }
            }
            values.push((a, b));
        }
    }
    Ok(())
}
