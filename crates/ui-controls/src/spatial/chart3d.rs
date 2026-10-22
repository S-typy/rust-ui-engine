//! Perspective/orthographic scatter data. Markers are depth-sorted, camera-facing
//! discs, not triangle meshes or intersecting spheres. Their constant camera
//! depth makes paint-order occlusion and picking exactly the same operation.
use super::*;
use crate::{ChartModel, ChartPointKey};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vec3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}
impl Vec3 {
    pub const fn new(x: f64, y: f64, z: f64) -> Self {
        Self { x, y, z }
    }
    fn dot(self, b: Self) -> f64 {
        self.x * b.x + self.y * b.y + self.z * b.z
    }
    fn length(self) -> f64 {
        self.dot(self).sqrt()
    }
    fn finite(self) -> bool {
        [self.x, self.y, self.z].into_iter().all(f64::is_finite)
    }
    fn normalized(self) -> Option<Self> {
        let n = self.length();
        (self.finite() && n.is_finite() && n > 1e-12)
            .then(|| Self::new(self.x / n, self.y / n, self.z / n))
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Projection3D {
    Perspective,
    Orthographic,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera3D {
    /// Radians around the vertical Y axis.
    pub azimuth: f64,
    pub elevation: f64,
    pub distance: f64,
    pub field_of_view: f64,
    pub projection: Projection3D,
}
impl Default for Camera3D {
    fn default() -> Self {
        Self {
            azimuth: 0.55,
            elevation: 0.35,
            distance: 4.5,
            field_of_view: 0.9,
            projection: Projection3D::Perspective,
        }
    }
}
impl Camera3D {
    pub fn validate(self) -> Result<(), SpatialError> {
        if ![
            self.azimuth,
            self.elevation,
            self.distance,
            self.field_of_view,
        ]
        .into_iter()
        .all(f64::is_finite)
            || self.azimuth.abs() > 1e6
            || !(-1.45..=1.45).contains(&self.elevation)
            || !(2.5..=30.0).contains(&self.distance)
            || !(0.2..=2.0).contains(&self.field_of_view)
        {
            return Err(SpatialError::InvalidRange);
        }
        Ok(())
    }
    fn basis(self) -> (Vec3, Vec3, Vec3) {
        let (sa, ca) = self.azimuth.sin_cos();
        let (se, ce) = self.elevation.sin_cos();
        (
            Vec3::new(ca, 0.0, -sa),
            Vec3::new(-sa * se, ce, -ca * se),
            Vec3::new(sa * ce, se, ca * ce),
        )
    }
    fn project(self, p: Vec3, b: Rect) -> Option<(Point, f64, f32)> {
        let (right, up, toward) = self.basis();
        let depth = self.distance - p.dot(toward);
        if depth <= 0.1 {
            return None;
        }
        let scale = f64::from(b.width.min(b.height)) * 0.5 / (self.field_of_view * 0.5).tan();
        let perspective = if self.projection == Projection3D::Perspective {
            depth
        } else {
            self.distance
        };
        let x = f64::from(b.x + b.width * 0.5) + p.dot(right) * scale / perspective;
        let y = f64::from(b.y + b.height * 0.5) - p.dot(up) * scale / perspective;
        Some((
            Point::new(x as f32, y as f32),
            depth,
            (scale / perspective) as f32,
        ))
    }
}
#[derive(Clone, Debug)]
pub struct ProjectedPoint3D {
    pub key: ChartPointKey,
    pub center: Point,
    pub depth: f64,
    pub radius: f32,
    pub series_index: usize,
}
#[derive(Clone, Debug)]
pub struct Chart3DGeometry {
    pub viewport: Rect,
    /// Far-to-near, stable series/input order breaks equal-depth ties.
    pub points: Vec<ProjectedPoint3D>,
    pub axes: Vec<(Point, Point, String)>,
}
impl Chart3DGeometry {
    /// No hit slop can select a marker hidden by a nearer opaque disc.
    pub fn hit_test(&self, p: Point) -> Option<ChartPointKey> {
        if !self.viewport.contains(p.x, p.y) {
            return None;
        }
        self.points
            .iter()
            .rev()
            .find(|v| distance(v.center, p) <= v.radius)
            .map(|p| p.key)
    }
}
#[derive(Clone, Debug)]
pub struct Chart3D {
    base: ChartModel,
    z: HashMap<ChartPointKey, f64>,
    z_range: (f64, f64),
    camera: Camera3D,
    light: Vec3,
}
impl Chart3D {
    /// Reuses accepted series identity, labels, X/Y ranges and selection.
    /// Every non-null point must have exactly one finite Z coordinate.
    pub fn new(
        base: ChartModel,
        z: impl IntoIterator<Item = (ChartPointKey, f64)>,
    ) -> Result<Self, SpatialError> {
        let mut values = HashMap::new();
        for (key, value) in z {
            if !value.is_finite() {
                return Err(SpatialError::InvalidCoordinate);
            }
            if values.insert(key, value).is_some() {
                return Err(SpatialError::DuplicateIdentity);
            }
            if base.point(key).is_none_or(|p| p.y.is_none()) {
                return Err(SpatialError::UnknownIdentity);
            }
        }
        let count = base
            .series()
            .iter()
            .flat_map(|s| &s.points)
            .filter(|p| p.y.is_some())
            .count();
        if values.len() != count {
            return Err(SpatialError::UnknownIdentity);
        }
        let mut low = values.values().copied().reduce(f64::min).unwrap_or(-1.0);
        let mut high = values.values().copied().reduce(f64::max).unwrap_or(1.0);
        if low == high {
            let pad = low.abs().max(1.0) * 0.05;
            low -= pad;
            high += pad;
        }
        if !low.is_finite() || !high.is_finite() || !(high - low).is_finite() || low >= high {
            return Err(SpatialError::InvalidRange);
        }
        Ok(Self {
            base,
            z: values,
            z_range: (low, high),
            camera: Default::default(),
            light: Vec3::new(-0.4, 0.8, 0.5),
        })
    }
    /// Atomically replace data while retaining camera, lighting and selection
    /// by identity. A missing/null selected point clears selection. Rejected
    /// coordinates leave both document and view unchanged.
    pub fn replace_data(
        &mut self,
        base: ChartModel,
        z: impl IntoIterator<Item = (ChartPointKey, f64)>,
    ) -> Result<(), SpatialError> {
        let mut candidate = Self::new(base, z)?;
        candidate.camera = self.camera;
        candidate.light = self.light;
        candidate.select(self.selected().filter(|key| candidate.z.contains_key(key)))?;
        *self = candidate;
        Ok(())
    }
    /// Accepted Z coordinates, keyed by stable series/point identity.
    pub fn z_values(&self) -> impl Iterator<Item = (ChartPointKey, f64)> + '_ {
        self.z.iter().map(|(key, value)| (*key, *value))
    }
    pub fn data(&self) -> &ChartModel {
        &self.base
    }
    pub fn camera(&self) -> Camera3D {
        self.camera
    }
    pub fn set_camera(&mut self, camera: Camera3D) -> Result<(), SpatialError> {
        camera.validate()?;
        self.camera = camera;
        Ok(())
    }
    pub fn set_light(&mut self, light: Vec3) -> Result<(), SpatialError> {
        self.light = light.normalized().ok_or(SpatialError::InvalidCoordinate)?;
        Ok(())
    }
    pub fn selected(&self) -> Option<ChartPointKey> {
        self.base.selected()
    }
    pub fn select(&mut self, key: Option<ChartPointKey>) -> Result<(), SpatialError> {
        self.base
            .select(key)
            .map_err(|_| SpatialError::UnknownIdentity)
    }
    pub fn orbit(&mut self, dx: f64, dy: f64) -> Result<(), SpatialError> {
        if !dx.is_finite() || !dy.is_finite() {
            return Err(SpatialError::InvalidCoordinate);
        }
        let mut c = self.camera;
        c.azimuth = (c.azimuth + dx).rem_euclid(std::f64::consts::TAU);
        c.elevation = (c.elevation + dy).clamp(-1.45, 1.45);
        self.set_camera(c)
    }
    pub fn zoom(&mut self, factor: f64) -> Result<(), SpatialError> {
        if !factor.is_finite() || factor <= 0.0 {
            return Err(SpatialError::InvalidRange);
        }
        let mut c = self.camera;
        c.distance = (c.distance * factor).clamp(2.5, 30.0);
        self.set_camera(c)
    }
    pub fn geometry(&self, viewport: Rect) -> Result<Chart3DGeometry, SpatialError> {
        valid_viewport(viewport)?;
        let xr = self.base.x_range();
        let yr = self.base.y_range();
        let (zl, zh) = self.z_range;
        let mut points = Vec::new();
        for (si, s) in self.base.series().iter().enumerate() {
            for p in &s.points {
                let Some(y) = p.y else {
                    continue;
                };
                let key = ChartPointKey {
                    series: s.id,
                    point: p.id,
                };
                let v = Vec3::new(
                    (p.x - xr.minimum) / (xr.maximum - xr.minimum) * 2.0 - 1.0,
                    (y - yr.minimum) / (yr.maximum - yr.minimum) * 2.0 - 1.0,
                    (self.z[&key] - zl) / (zh - zl) * 2.0 - 1.0,
                );
                // Explicit data ranges clip points in model space before projection.
                if [v.x, v.y, v.z]
                    .into_iter()
                    .any(|n| !(-1.000001..=1.000001).contains(&n))
                {
                    continue;
                }
                if let Some((center, depth, scale)) = self.camera.project(v, viewport) {
                    let radius = (scale * 0.045).clamp(3.0, 18.0);
                    if Rect::new(
                        center.x - radius,
                        center.y - radius,
                        radius * 2.0,
                        radius * 2.0,
                    )
                    .intersection(viewport)
                    .is_some()
                    {
                        points.push(ProjectedPoint3D {
                            key,
                            center,
                            depth,
                            radius,
                            series_index: si,
                        });
                    }
                }
            }
        }
        points.sort_by(|a, b| b.depth.total_cmp(&a.depth));
        let origin = Vec3::new(-1.0, -1.0, -1.0);
        let axes = [
            (Vec3::new(1.0, -1.0, -1.0), "X"),
            (Vec3::new(-1.0, 1.0, -1.0), "Y"),
            (Vec3::new(-1.0, -1.0, 1.0), "Z"),
        ]
        .into_iter()
        .filter_map(|(end, name)| {
            Some((
                self.camera.project(origin, viewport)?.0,
                self.camera.project(end, viewport)?.0,
                name.into(),
            ))
        })
        .collect();
        Ok(Chart3DGeometry {
            viewport,
            points,
            axes,
        })
    }
    pub fn table(&self) -> SpatialTable {
        SpatialTable {
            columns: vec![
                "Series".into(),
                "Point".into(),
                "X".into(),
                "Y".into(),
                "Z".into(),
            ],
            rows: self
                .base
                .series()
                .iter()
                .flat_map(|s| {
                    s.points.iter().filter_map(move |p| {
                        p.y.map(|y| {
                            vec![
                                s.label.clone(),
                                p.id.to_string(),
                                p.x.to_string(),
                                y.to_string(),
                                self.z[&ChartPointKey {
                                    series: s.id,
                                    point: p.id,
                                }]
                                    .to_string(),
                            ]
                        })
                    })
                })
                .collect(),
        }
    }
    pub fn summary(&self) -> String {
        if let Some(key) = self.selected() {
            let p = self.base.point(key).unwrap();
            let s = self
                .base
                .series()
                .iter()
                .find(|s| s.id == key.series)
                .unwrap();
            format!(
                "{}: X {}, Y {}, Z {}",
                s.label,
                p.x,
                p.y.unwrap(),
                self.z[&key]
            )
        } else {
            format!(
                "3D scatter: {} series, {} points. Arrows select; Control+arrows orbit; Page Up/Down zoom.",
                self.base.series().len(),
                self.z.len()
            )
        }
    }
    pub fn paint(
        &self,
        scene: &mut Scene,
        viewport: Rect,
        clip: Rect,
        p: SpatialPalette,
    ) -> Result<(), SpatialError> {
        let g = self.geometry(viewport)?;
        scene.rounded_fill(viewport, clip, 6.0, p.surface);
        for (a, b, name) in &g.axes {
            line(scene, *a, *b, clip, 1.0, p.border);
            label(
                scene,
                name,
                Rect::new(b.x + 4.0, b.y, 24.0, 20.0),
                clip,
                p.muted,
                12.0,
            );
        }
        let (_, _, normal) = self.camera.basis();
        let light = self.light.normalized().unwrap();
        let shade = (0.4 + 0.6 * normal.dot(light).max(0.0)) as f32;
        let colors = [
            Color::rgb(0, 121, 212),
            Color::rgb(220, 110, 30),
            Color::rgb(0, 154, 112),
            Color::rgb(154, 95, 204),
        ];
        for point in &g.points {
            let mut color = colors[point.series_index % colors.len()];
            color.r *= shade;
            color.g *= shade;
            color.b *= shade;
            let r = point.radius;
            let b = Rect::new(point.center.x - r, point.center.y - r, r * 2.0, r * 2.0);
            scene.rounded_fill(b, clip, r, color);
            if self.selected() == Some(point.key) {
                scene.rounded_stroke(b, clip, r, 2.0, p.foreground);
            }
        }
        // Selected annotation is an overlay; point geometry remains unchanged.
        if let Some(key) = self.selected()
            && let Some(point) = g.points.iter().find(|p| p.key == key)
        {
            label(
                scene,
                &self.summary(),
                Rect::new(
                    point.center.x + point.radius + 4.0,
                    point.center.y - 12.0,
                    260.0,
                    24.0,
                ),
                clip,
                p.foreground,
                12.0,
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ChartPoint, ChartSeries, ChartSettings};
    #[test]
    fn replacement_preserves_camera_light_selection_and_rejects_bad_z_atomically() {
        let mut chart = model();
        let key = ChartPointKey {
            series: 1,
            point: 2,
        };
        let selected = Some(key);
        chart.select(selected).unwrap();
        chart.orbit(0.7, -0.2).unwrap();
        chart.set_light(Vec3::new(1.0, 0.0, 0.0)).unwrap();
        let before = chart.clone();
        assert!(
            chart
                .replace_data(before.data().clone(), [(key, f64::NAN)])
                .is_err()
        );
        assert_eq!(chart.camera(), before.camera());
        assert_eq!(chart.selected(), selected);
        chart
            .replace_data(before.data().clone(), before.z_values())
            .unwrap();
        assert_eq!(chart.camera(), before.camera());
        assert_eq!(chart.light, before.light);
        assert_eq!(chart.selected(), selected);
        chart
            .replace_data(
                ChartModel::new(vec![], ChartSettings::default()).unwrap(),
                [],
            )
            .unwrap();
        assert_eq!(chart.selected(), None);
        assert_eq!(chart.camera(), before.camera());
    }
    fn model() -> Chart3D {
        let base = ChartModel::new(
            vec![ChartSeries {
                id: 1,
                label: "Series".into(),
                points: vec![
                    ChartPoint {
                        id: 1,
                        x: 0.0,
                        y: Some(0.0),
                    },
                    ChartPoint {
                        id: 2,
                        x: 0.0,
                        y: Some(0.0),
                    },
                ],
            }],
            ChartSettings::default(),
        )
        .unwrap();
        Chart3D::new(
            base,
            [
                (
                    ChartPointKey {
                        series: 1,
                        point: 1,
                    },
                    -1.0,
                ),
                (
                    ChartPointKey {
                        series: 1,
                        point: 2,
                    },
                    1.0,
                ),
            ],
        )
        .unwrap()
    }
    #[test]
    fn front_disc_pick_matches_scene_painter_order() {
        let mut m = model();
        m.set_camera(Camera3D {
            azimuth: 0.0,
            elevation: 0.0,
            ..Default::default()
        })
        .unwrap();
        let g = m.geometry(Rect::new(0.0, 0.0, 500.0, 400.0)).unwrap();
        assert_eq!(g.points[1].key.point, 2);
        assert_eq!(g.hit_test(g.points[0].center).unwrap().point, 2);
        let mut scene = Scene::default();
        m.paint(
            &mut scene,
            g.viewport,
            g.viewport,
            SpatialPalette::from_theme(crate::Theme::FluentLight),
        )
        .unwrap();
        assert_eq!(scene.rounded_rectangles.len(), 3);
    }
    #[test]
    fn camera_reverses_depth_and_rejects_invalid_atomically() {
        let mut m = model();
        let old = m.camera();
        assert!(
            m.set_camera(Camera3D {
                distance: f64::NAN,
                ..old
            })
            .is_err()
        );
        assert_eq!(m.camera(), old);
        m.set_camera(Camera3D {
            azimuth: std::f64::consts::PI,
            elevation: 0.0,
            ..old
        })
        .unwrap();
        let g = m.geometry(Rect::new(0.0, 0.0, 400.0, 300.0)).unwrap();
        assert_eq!(g.points.last().unwrap().key.point, 1);
    }
    #[test]
    fn table_contains_hidden_points_and_selection_survives_resize() {
        let mut m = model();
        let k = ChartPointKey {
            series: 1,
            point: 1,
        };
        m.select(Some(k)).unwrap();
        assert_eq!(m.table().rows.len(), 2);
        assert_eq!(m.table().rows[0].len(), 5);
        for b in [
            Rect::new(0.0, 0.0, 800.0, 500.0),
            Rect::new(0.0, 0.0, 80.0, 60.0),
        ] {
            assert_eq!(m.geometry(b).unwrap().points.len(), 2);
        }
        assert_eq!(m.selected(), Some(k));
        assert!(m.set_light(Vec3::new(0.0, 0.0, 0.0)).is_err());
    }
}
