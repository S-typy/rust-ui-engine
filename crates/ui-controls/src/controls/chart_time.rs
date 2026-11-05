//! Bounded UTC calendar ticks over exact integer millisecond observations.
use super::*;
use rust_desktop_ui_data::{DateValue, ScheduleInstant, TimeValue};

const MIN: f64 = -62_135_596_800_000.0;
const MAX: f64 = 253_402_300_799_999.0;
const DAY: i64 = 86_400_000;

pub(super) fn validate(value: f64) -> Result<(), ChartError> {
    if value.is_finite() && (MIN..=MAX).contains(&value) {
        Ok(())
    } else {
        Err(ChartError::InvalidRange)
    }
}
fn local(value: f64) -> Result<(rust_desktop_ui_data::ScheduleLocal, u16), ChartError> {
    validate(value)?;
    // Floor (including before 1970) describes the containing millisecond.
    let millis = value.floor() as i64;
    Ok((
        ScheduleInstant(millis.div_euclid(1000))
            .utc()
            .map_err(|_| ChartError::InvalidRange)?,
        millis.rem_euclid(1000) as u16,
    ))
}
fn midnight(year: u16, month: u8) -> Result<f64, ChartError> {
    let date = DateValue::new(year, month, 1).map_err(|_| ChartError::InvalidRange)?;
    let time = TimeValue::new(0, 0, 0, 0).unwrap();
    Ok((ScheduleInstant::from_utc(date, time)
        .map_err(|_| ChartError::InvalidRange)?
        .0
        * 1000) as f64)
}
pub(super) fn automatic(values: impl Iterator<Item = f64>) -> Result<ChartRange, ChartError> {
    let mut range: Option<ChartRange> = None;
    for value in values {
        validate(value)?;
        match &mut range {
            Some(r) => {
                r.minimum = r.minimum.min(value);
                r.maximum = r.maximum.max(value);
            }
            None => {
                range = Some(ChartRange {
                    minimum: value,
                    maximum: value,
                })
            }
        }
    }
    let mut range = range.unwrap_or(ChartRange {
        minimum: 0.0,
        maximum: DAY as f64,
    });
    if range.minimum == range.maximum {
        range.minimum = (range.minimum - 1000.0).max(MIN);
        range.maximum = (range.maximum + 1000.0).min(MAX);
    }
    ChartScale::UtcMilliseconds.domain(range)?;
    Ok(range)
}

fn fixed_ticks(range: ChartRange, step: i64) -> Vec<f64> {
    let mut v = (range.minimum / step as f64).ceil() * step as f64;
    let mut result = Vec::new();
    // Six is an overflow sentinel, not an unbounded scan of all dates.
    while v <= range.maximum && result.len() < 6 {
        result.push(v);
        v += step as f64;
    }
    result
}
fn month_ticks(range: ChartRange, step: u32) -> Result<Vec<f64>, ChartError> {
    let (date, _) = local(range.minimum)?;
    let d = date.date();
    let mut index = u32::from(d.year() - 1) * 12 + u32::from(d.month() - 1);
    index = index.div_ceil(step) * step;
    let mut result = Vec::new();
    while index < 9999 * 12 && result.len() < 6 {
        let v = midnight((index / 12 + 1) as u16, (index % 12 + 1) as u8)?;
        if v > range.maximum {
            break;
        }
        if v >= range.minimum {
            result.push(v);
        }
        index += step;
    }
    Ok(result)
}
pub(super) fn ticks(range: ChartRange) -> Result<Vec<f64>, ChartError> {
    for step in [
        1,
        2,
        5,
        10,
        20,
        50,
        100,
        200,
        500,
        1000,
        2000,
        5000,
        10000,
        15000,
        30000,
        60000,
        120000,
        300000,
        600000,
        900000,
        1800000,
        3600000,
        10800000,
        21600000,
        43200000,
        DAY,
        2 * DAY,
        7 * DAY,
        14 * DAY,
    ] {
        let values = fixed_ticks(range, step);
        if !values.is_empty() && values.len() <= 5 {
            return Ok(values);
        }
    }
    for step in [
        1, 2, 3, 6, 12, 24, 60, 120, 240, 600, 1200, 2400, 6000, 12000, 24000, 60000,
    ] {
        let values = month_ticks(range, step)?;
        if !values.is_empty() && values.len() <= 5 {
            return Ok(values);
        }
    }
    Err(ChartError::InvalidRange)
}
pub(super) fn format(value: f64) -> Result<String, ChartError> {
    let (local, millis) = local(value)?;
    Ok(format!("{} {}.{millis:03} UTC", local.date(), local.time()))
}
pub(super) fn tick_label(range: ChartRange, value: f64) -> Result<String, ChartError> {
    let (local, millis) = local(value)?;
    let (date, time) = (local.date(), local.time());
    let span = range.maximum - range.minimum;
    let spacing = ticks(range)?
        .windows(2)
        .map(|pair| pair[1] - pair[0])
        .reduce(f64::min)
        .unwrap_or(span);
    Ok(
        if span >= (366 * DAY) as f64 && spacing >= (28 * DAY) as f64 {
            format!("{:02}.{:04}", date.month(), date.year())
        } else if spacing >= DAY as f64 {
            format!("{:02}.{:02}.{:04}", date.day(), date.month(), date.year())
        } else if span >= DAY as f64 {
            format!(
                "{:02}.{:02} {:02}:{:02}",
                date.day(),
                date.month(),
                time.hour(),
                time.minute()
            )
        } else if spacing >= 1000.0 {
            format!(
                "{:02}:{:02}:{:02}",
                time.hour(),
                time.minute(),
                time.second()
            )
        } else {
            format!(
                "{:02}:{:02}:{:02}.{millis:03}",
                time.hour(),
                time.minute(),
                time.second()
            )
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn model(values: &[f64]) -> ChartModel {
        ChartModel::new(
            vec![ChartSeries {
                id: 1,
                label: "UTC".into(),
                points: values
                    .iter()
                    .enumerate()
                    .map(|(i, x)| ChartPoint {
                        id: i as i64,
                        x: *x,
                        y: (i != 1).then_some(i as f64),
                    })
                    .collect(),
            }],
            ChartSettings {
                x_scale: ChartScale::UtcMilliseconds,
                ..Default::default()
            },
        )
        .unwrap()
    }
    #[test]
    fn time_values_preserve_epoch_negative_milliseconds_and_calendar_boundaries() {
        for (v, text) in [
            (0.0, "1970-01-01 00:00:00.000 UTC"),
            (-1.0, "1969-12-31 23:59:59.999 UTC"),
            (1_709_164_800_123.0, "2024-02-29 00:00:00.123 UTC"),
            (MIN, "0001-01-01 00:00:00.000 UTC"),
            (MAX, "9999-12-31 23:59:59.999 UTC"),
        ] {
            assert_eq!(ChartScale::UtcMilliseconds.format_value(v).unwrap(), text);
        }
        for v in [MIN - 1.0, MAX + 1.0, f64::INFINITY, f64::NAN] {
            assert!(ChartScale::UtcMilliseconds.format_value(v).is_err());
        }
    }
    #[test]
    fn time_calendar_ticks_follow_real_months_and_leap_days() {
        let range = ChartRange {
            minimum: midnight(2024, 1).unwrap(),
            maximum: midnight(2024, 7).unwrap(),
        };
        let t = ticks(range).unwrap();
        assert_eq!(
            t.iter()
                .map(|v| local(*v).unwrap().0.date().month())
                .collect::<Vec<_>>(),
            [1, 3, 5, 7]
        );
        assert_eq!(t[1] - t[0], 60.0 * DAY as f64);
        assert_eq!(t[2] - t[1], 61.0 * DAY as f64);
        let leap = ChartRange {
            minimum: 1_709_078_400_000.0,
            maximum: 1_709_337_600_000.0,
        };
        let days = ticks(leap).unwrap();
        assert_eq!(days.len(), 4);
        assert_eq!(tick_label(leap, days[1]).unwrap(), "29.02.2024");
        assert_eq!(tick_label(leap, days[2]).unwrap(), "01.03.2024");
    }
    #[test]
    fn time_ticks_are_bounded_ordered_and_inside_extreme_and_subsecond_ranges() {
        for range in [
            ChartRange {
                minimum: MIN,
                maximum: MAX,
            },
            ChartRange {
                minimum: MIN,
                maximum: MIN + 1.0,
            },
            ChartRange {
                minimum: MAX - 1.0,
                maximum: MAX,
            },
            ChartRange {
                minimum: -999.5,
                maximum: -990.5,
            },
            ChartRange {
                minimum: 0.25,
                maximum: 1.25,
            },
            ChartRange {
                minimum: 0.0,
                maximum: 1000.0,
            },
            ChartRange {
                minimum: 0.0,
                maximum: 2.0 * DAY as f64,
            },
            ChartRange {
                minimum: 1_709_164_800_000.0,
                maximum: 1_709_164_800_099.0,
            },
        ] {
            let t = ChartScale::UtcMilliseconds.ticks(range).unwrap();
            assert!(!t.is_empty() && t.len() <= 5);
            assert!(t.windows(2).all(|p| p[0] < p[1]));
            assert!(
                t.iter()
                    .all(|v| v.fract() == 0.0 && *v >= range.minimum && *v <= range.maximum)
            );
            let labels = t
                .iter()
                .map(|v| tick_label(range, *v).unwrap())
                .collect::<std::collections::HashSet<_>>();
            assert_eq!(labels.len(), t.len());
        }
        assert!(
            ChartScale::UtcMilliseconds
                .domain(ChartRange {
                    minimum: 0.0,
                    maximum: 0.5
                })
                .is_err()
        );
    }
    #[test]
    fn time_navigation_and_live_replace_preserve_raw_keys_nulls_and_selection() {
        let mut m = model(&[0.0, 1000.0, 2000.0, 3000.0]);
        let key = ChartPointKey {
            series: 1,
            point: 2,
        };
        m.select(Some(key)).unwrap();
        let source = m.series().to_vec();
        let plot = Rect::new(0.0, 0.0, 600.0, 300.0);
        assert_eq!(m.geometry(plot).unwrap().segments.len(), 1);
        let point = m
            .geometry(plot)
            .unwrap()
            .points
            .iter()
            .find(|p| p.key == key)
            .unwrap()
            .position;
        m.zoom_at(2.0, (2.0 / 3.0, 2.0 / 3.0)).unwrap();
        let after = m
            .geometry(plot)
            .unwrap()
            .points
            .iter()
            .find(|p| p.key == key)
            .unwrap()
            .position;
        assert!((point.x - after.x).abs() < 0.01 && (point.y - after.y).abs() < 0.01);
        m.pan(0.1, 0.0).unwrap();
        assert_eq!(m.series(), source);
        assert_eq!(m.selected(), Some(key));
        assert!(
            m.describe(key)
                .unwrap()
                .contains("1970-01-01 00:00:02.000 UTC")
        );
        let before = m.clone();
        assert!(
            m.set_view_ranges(
                Some(ChartRange {
                    minimum: MAX,
                    maximum: MAX + 1.0
                }),
                None
            )
            .is_err()
        );
        assert_eq!(m.series(), before.series());
        assert_eq!(m.settings(), before.settings());
        assert_eq!(m.x_range(), before.x_range());
        assert_eq!(m.y_range(), before.y_range());
        assert_eq!(m.selected(), before.selected());
        let mut bad = source.clone();
        bad[0].points[0].x = 0.25;
        assert!(m.replace(bad, m.settings().clone()).is_err());
        assert_eq!(m.series(), before.series());
        assert_eq!(m.settings(), before.settings());
        assert_eq!(m.x_range(), before.x_range());
        assert_eq!(m.y_range(), before.y_range());
        assert_eq!(m.selected(), before.selected());
        m.set_view_ranges(None, None).unwrap();
        assert_eq!(
            m.x_range(),
            ChartRange {
                minimum: 0.0,
                maximum: 3000.0
            }
        );
    }
    #[test]
    fn time_auto_bounds_support_empty_singleton_and_independent_y_axis() {
        assert_eq!(
            model(&[]).x_range(),
            ChartRange {
                minimum: 0.0,
                maximum: DAY as f64
            }
        );
        for value in [MIN, 0.0, MAX] {
            let m = model(&[value]);
            assert!(m.x_range().minimum <= value && m.x_range().maximum >= value);
            assert!(m.x_range().maximum - m.x_range().minimum >= 1000.0);
        }
        let mut s = model(&[0.0, 1000.0, 2000.0]).series().to_vec();
        for p in &mut s[0].points {
            p.y = p.y.map(|v| v * 1000.0);
        }
        let m = ChartModel::new(
            s,
            ChartSettings {
                y_scale: ChartScale::UtcMilliseconds,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(
            m.describe(ChartPointKey {
                series: 1,
                point: 2
            })
            .unwrap()
            .contains("Y 1970-01-01 00:00:02.000 UTC")
        );
        for view in [
            ChartView::Bar,
            ChartView::Area,
            ChartView::StackedBar,
            ChartView::StackedArea,
        ] {
            assert!(
                ChartModel::new(
                    m.series().to_vec(),
                    ChartSettings {
                        view,
                        x_scale: ChartScale::UtcMilliseconds,
                        ..Default::default()
                    }
                )
                .is_err()
            );
        }
    }
}
