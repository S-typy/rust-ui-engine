//! Offline-capable geographic layers with explicit application-owned providers.
use super::*;
use rust_desktop_ui_core::{Bitmap, ImagePrimitive};
use std::collections::{HashMap, HashSet};

pub const MAX_MAP_MARKERS: usize = 4096;
pub const MAX_MAP_VERTICES: usize = 16384;
pub const MAX_MAP_VISIBLE_TILES: usize = 256;
pub const MAX_MAP_CACHE_BYTES: usize = 32 * 1024 * 1024;
const MERCATOR_LIMIT: f64 = 85.0511287798066;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GeoPoint {
    pub longitude: f64,
    pub latitude: f64,
}
impl GeoPoint {
    pub fn new(longitude: f64, latitude: f64) -> Result<Self, SpatialError> {
        if !longitude.is_finite()
            || !latitude.is_finite()
            || !(-180.0..=180.0).contains(&longitude)
            || !(-MERCATOR_LIMIT..=MERCATOR_LIMIT).contains(&latitude)
        {
            return Err(SpatialError::InvalidCoordinate);
        }
        Ok(Self {
            longitude,
            latitude,
        })
    }
    fn validate(self) -> Result<(), SpatialError> {
        Self::new(self.longitude, self.latitude).map(|_| ())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MapProjection {
    WebMercator,
    Equirectangular,
}
impl MapProjection {
    pub fn project(self, p: GeoPoint) -> Result<(f64, f64), SpatialError> {
        p.validate()?;
        let x = (p.longitude + 180.0) / 360.0;
        let y = match self {
            Self::WebMercator => {
                0.5 - p.latitude.to_radians().tan().asinh() / (2.0 * std::f64::consts::PI)
            }
            Self::Equirectangular => (90.0 - p.latitude) / 180.0,
        };
        Ok((x, y))
    }
    pub fn unproject(self, x: f64, y: f64) -> Result<GeoPoint, SpatialError> {
        if !x.is_finite() || !y.is_finite() {
            return Err(SpatialError::InvalidCoordinate);
        }
        let latitude = match self {
            Self::WebMercator => (std::f64::consts::PI * (1.0 - 2.0 * y.clamp(0.0, 1.0)))
                .sinh()
                .atan()
                .to_degrees(),
            Self::Equirectangular => 90.0 - 180.0 * y,
        };
        GeoPoint::new(
            x.rem_euclid(1.0) * 360.0 - 180.0,
            latitude.clamp(-MERCATOR_LIMIT, MERCATOR_LIMIT),
        )
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct MapMarker {
    pub id: u64,
    pub position: GeoPoint,
    pub label: String,
}
#[derive(Clone, Debug, PartialEq)]
pub struct MapPolyline {
    pub id: u64,
    pub vertices: Vec<GeoPoint>,
    pub label: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MapProvider {
    pub id: String,
    pub attribution: String,
}
impl MapProvider {
    pub fn new(
        id: impl Into<String>,
        attribution: impl Into<String>,
    ) -> Result<Self, SpatialError> {
        let p = Self {
            id: id.into(),
            attribution: attribution.into(),
        };
        valid_label(&p.id)?;
        valid_label(&p.attribution)?;
        if p.id.is_empty() || p.attribution.is_empty() {
            return Err(SpatialError::InvalidLabel);
        }
        Ok(p)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MapTileKey {
    pub zoom: u8,
    pub x: u32,
    pub y: u32,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MapTileRequest {
    pub generation: u64,
    pub sequence: u64,
    pub provider: String,
    pub key: MapTileKey,
}
#[derive(Clone, Debug)]
pub enum MapTileResult {
    Ready(Bitmap),
    Missing(String),
}
#[derive(Clone, Debug)]
struct TileEntry {
    value: MapTileResult,
    touched: u64,
    bytes: usize,
}
#[derive(Clone, Debug, PartialEq)]
pub enum MapServiceKind {
    Search(String),
    Route { from: GeoPoint, to: GeoPoint },
}
#[derive(Clone, Debug, PartialEq)]
pub struct MapServiceRequest {
    pub generation: u64,
    pub sequence: u64,
    pub provider: String,
    pub kind: MapServiceKind,
}
#[derive(Clone, Debug, PartialEq)]
pub enum MapServiceResult {
    Search(Vec<MapMarker>),
    Route(MapPolyline),
}
/// The application dispatches work to its executor and delivers completion on
/// the UI thread. Cancellation stops publication immediately; the application
/// also receives old tokens so it can cancel its transport or worker.
pub trait MapServices {
    fn request(&mut self, request: MapServiceRequest);
    fn cancel(&mut self, request: &MapServiceRequest);
}
#[derive(Clone, Debug)]
pub struct MapTilePlacement {
    pub key: MapTileKey,
    pub bounds: Rect,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MapDiagnostics {
    pub missing_tiles: usize,
    pub pending_tiles: usize,
    pub ready_tiles: usize,
    pub cache_bytes: usize,
    pub last_error: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Map {
    projection: MapProjection,
    center: GeoPoint,
    zoom: f64,
    provider: MapProvider,
    markers: Vec<MapMarker>,
    vectors: Vec<MapPolyline>,
    selected: Option<u64>,
    generation: u64,
    sequence: u64,
    clock: u64,
    cache: HashMap<MapTileKey, TileEntry>,
    cache_capacity: usize,
    cache_bytes: usize,
    pending: HashMap<MapTileKey, MapTileRequest>,
    service: Option<MapServiceRequest>,
    last_error: Option<String>,
}
impl Map {
    pub fn new(provider: MapProvider) -> Result<Self, SpatialError> {
        MapProvider::new(&provider.id, &provider.attribution)?;
        Ok(Self {
            projection: MapProjection::WebMercator,
            center: GeoPoint::new(0.0, 0.0)?,
            zoom: 2.0,
            provider,
            markers: vec![],
            vectors: vec![],
            selected: None,
            generation: 1,
            sequence: 0,
            clock: 0,
            cache: HashMap::new(),
            cache_capacity: 64,
            cache_bytes: 0,
            pending: HashMap::new(),
            service: None,
            last_error: None,
        })
    }
    pub fn provider(&self) -> &MapProvider {
        &self.provider
    }
    pub fn projection(&self) -> MapProjection {
        self.projection
    }
    pub fn center(&self) -> GeoPoint {
        self.center
    }
    pub fn zoom(&self) -> f64 {
        self.zoom
    }
    pub fn markers(&self) -> &[MapMarker] {
        &self.markers
    }
    pub fn vectors(&self) -> &[MapPolyline] {
        &self.vectors
    }
    pub fn selected(&self) -> Option<u64> {
        self.selected
    }
    fn invalidate(&mut self) -> Result<(), SpatialError> {
        self.generation = self
            .generation
            .checked_add(1)
            .ok_or(SpatialError::Capacity)?;
        self.pending.clear();
        self.service = None;
        Ok(())
    }
    /// Returns cancelled requests; existing accepted cache entries remain usable.
    pub fn cancel_requests(
        &mut self,
    ) -> Result<(Vec<MapTileRequest>, Option<MapServiceRequest>), SpatialError> {
        let old = (
            self.pending.values().cloned().collect(),
            self.service.clone(),
        );
        self.invalidate()?;
        Ok(old)
    }
    pub fn set_provider(&mut self, provider: MapProvider) -> Result<(), SpatialError> {
        MapProvider::new(&provider.id, &provider.attribution)?;
        self.invalidate()?;
        self.provider = provider;
        self.cache.clear();
        self.cache_bytes = 0;
        self.last_error = None;
        Ok(())
    }
    pub fn set_projection(&mut self, projection: MapProjection) -> Result<(), SpatialError> {
        if projection != self.projection {
            self.invalidate()?;
            self.projection = projection;
            self.cache.clear();
            self.cache_bytes = 0;
        }
        Ok(())
    }
    pub fn set_view(&mut self, center: GeoPoint, zoom: f64) -> Result<(), SpatialError> {
        center.validate()?;
        if !zoom.is_finite() || !(0.0..=20.0).contains(&zoom) {
            return Err(SpatialError::InvalidRange);
        }
        if self.center != center || self.zoom != zoom {
            self.invalidate()?;
            self.center = center;
            self.zoom = zoom;
        }
        Ok(())
    }
    pub fn set_cache_capacity(&mut self, capacity: usize) -> Result<(), SpatialError> {
        if !(1..=256).contains(&capacity) {
            return Err(SpatialError::Capacity);
        }
        self.cache_capacity = capacity;
        self.evict();
        Ok(())
    }
    pub fn replace_layers(
        &mut self,
        markers: Vec<MapMarker>,
        vectors: Vec<MapPolyline>,
    ) -> Result<(), SpatialError> {
        if markers.len() > MAX_MAP_MARKERS
            || vectors.len() > 1024
            || vectors.iter().map(|v| v.vertices.len()).sum::<usize>() > MAX_MAP_VERTICES
        {
            return Err(SpatialError::Capacity);
        }
        let mut ids = HashSet::new();
        for m in &markers {
            m.position.validate()?;
            valid_label(&m.label)?;
            if !ids.insert(m.id) {
                return Err(SpatialError::DuplicateIdentity);
            }
        }
        ids.clear();
        for v in &vectors {
            valid_label(&v.label)?;
            if v.vertices.len() < 2 {
                return Err(SpatialError::InvalidCoordinate);
            }
            if !ids.insert(v.id) {
                return Err(SpatialError::DuplicateIdentity);
            }
            for p in &v.vertices {
                p.validate()?;
            }
        }
        self.selected = self
            .selected
            .filter(|id| markers.iter().any(|m| m.id == *id));
        self.markers = markers;
        self.vectors = vectors;
        Ok(())
    }
    pub fn select(&mut self, id: Option<u64>) -> Result<(), SpatialError> {
        if id.is_some_and(|id| !self.markers.iter().any(|m| m.id == id)) {
            return Err(SpatialError::UnknownIdentity);
        }
        self.selected = id;
        Ok(())
    }
    fn world_size(&self) -> f64 {
        256.0 * 2.0f64.powf(self.zoom)
    }
    pub fn screen_point(&self, p: GeoPoint, b: Rect) -> Result<Point, SpatialError> {
        valid_viewport(b)?;
        let (x, y) = self.projection.project(p)?;
        let (cx, cy) = self.projection.project(self.center)?;
        let dx = (x - cx + 0.5).rem_euclid(1.0) - 0.5;
        let s = self.world_size();
        Ok(Point::new(
            (f64::from(b.x + b.width * 0.5) + dx * s) as f32,
            (f64::from(b.y + b.height * 0.5) + (y - cy) * s) as f32,
        ))
    }
    pub fn geo_point(&self, p: Point, b: Rect) -> Result<GeoPoint, SpatialError> {
        valid_viewport(b)?;
        if !p.is_finite() {
            return Err(SpatialError::InvalidCoordinate);
        }
        let (cx, cy) = self.projection.project(self.center)?;
        let s = self.world_size();
        self.projection.unproject(
            cx + f64::from(p.x - b.x - b.width * 0.5) / s,
            cy + f64::from(p.y - b.y - b.height * 0.5) / s,
        )
    }
    pub fn pan(&mut self, delta: Point) -> Result<(), SpatialError> {
        if !delta.is_finite() {
            return Err(SpatialError::InvalidCoordinate);
        }
        let (cx, cy) = self.projection.project(self.center)?;
        let s = self.world_size();
        let next = self
            .projection
            .unproject(cx - f64::from(delta.x) / s, cy - f64::from(delta.y) / s)?;
        self.set_view(next, self.zoom)
    }
    pub fn zoom_at(&mut self, delta: f64, anchor: Point, b: Rect) -> Result<(), SpatialError> {
        if !delta.is_finite() {
            return Err(SpatialError::InvalidRange);
        }
        let geo = self.geo_point(anchor, b)?;
        let (x, y) = self.projection.project(geo)?;
        let zoom = (self.zoom + delta).clamp(0.0, 20.0);
        let s = 256.0 * 2.0f64.powf(zoom);
        let center = self.projection.unproject(
            x - f64::from(anchor.x - b.x - b.width * 0.5) / s,
            y - f64::from(anchor.y - b.y - b.height * 0.5) / s,
        )?;
        self.set_view(center, zoom)
    }
    pub fn hit_test(&self, p: Point, b: Rect) -> Option<u64> {
        if !b.contains(p.x, p.y) || p.y >= b.y + (b.height - 24.0).max(0.0) {
            return None;
        }
        self.markers
            .iter()
            .rev()
            .find(|m| {
                self.screen_point(m.position, b)
                    .is_ok_and(|c| distance(c, p) <= 8.0)
            })
            .map(|m| m.id)
    }
    /// Raster tiles follow the Web Mercator XYZ convention. For the alternate
    /// vector projection raster requests are intentionally disabled.
    pub fn visible_tiles(&self, b: Rect) -> Result<Vec<MapTilePlacement>, SpatialError> {
        valid_viewport(b)?;
        if self.projection != MapProjection::WebMercator {
            return Ok(vec![]);
        }
        let zoom = self.zoom.floor() as u8;
        let count = 1i64 << zoom;
        let tile = 256.0 * 2.0f64.powf(self.zoom - f64::from(zoom));
        let (cx, cy) = self.projection.project(self.center)?;
        let ox = f64::from(b.x + b.width * 0.5) - cx * count as f64 * tile;
        let oy = f64::from(b.y + b.height * 0.5) - cy * count as f64 * tile;
        let left = ((f64::from(b.x) - ox) / tile).floor() as i64;
        let right = ((f64::from(b.x + b.width) - ox) / tile).ceil() as i64;
        let top = (((f64::from(b.y) - oy) / tile).floor() as i64).max(0);
        let bottom = (((f64::from(b.y + b.height) - oy) / tile).ceil() as i64).min(count);
        if (right - left).saturating_mul((bottom - top).max(0)) > MAX_MAP_VISIBLE_TILES as i64 {
            return Err(SpatialError::Capacity);
        }
        let mut tiles = vec![];
        for y in top..bottom {
            for x in left..right {
                tiles.push(MapTilePlacement {
                    key: MapTileKey {
                        zoom,
                        x: x.rem_euclid(count) as u32,
                        y: y as u32,
                    },
                    bounds: Rect::new(
                        (ox + x as f64 * tile) as f32,
                        (oy + y as f64 * tile) as f32,
                        tile as f32,
                        tile as f32,
                    ),
                });
            }
        }
        Ok(tiles)
    }
    pub fn request_tiles(&mut self, b: Rect) -> Result<Vec<MapTileRequest>, SpatialError> {
        let tiles = self.visible_tiles(b)?;
        self.clock = self.clock.checked_add(1).ok_or(SpatialError::Capacity)?;
        let mut requests = vec![];
        for tile in tiles {
            if let Some(e) = self.cache.get_mut(&tile.key) {
                e.touched = self.clock;
                continue;
            }
            if self.pending.contains_key(&tile.key) {
                continue;
            }
            if self.pending.len() >= MAX_MAP_VISIBLE_TILES {
                break;
            }
            self.sequence = self.sequence.checked_add(1).ok_or(SpatialError::Capacity)?;
            let request = MapTileRequest {
                generation: self.generation,
                sequence: self.sequence,
                provider: self.provider.id.clone(),
                key: tile.key,
            };
            self.pending.insert(tile.key, request.clone());
            requests.push(request);
        }
        Ok(requests)
    }
    /// Allows the transport owner to cancel work invalidated by pan, zoom,
    /// projection, provider change or an explicit cancellation.
    pub fn is_current_tile_request(&self, request: &MapTileRequest) -> bool {
        request.generation == self.generation
            && request.provider == self.provider.id
            && self.pending.get(&request.key) == Some(request)
    }
    pub fn is_current_service_request(&self, request: &MapServiceRequest) -> bool {
        request.generation == self.generation && self.service.as_ref() == Some(request)
    }
    pub fn complete_tile(
        &mut self,
        request: &MapTileRequest,
        result: MapTileResult,
    ) -> Result<(), SpatialError> {
        if request.generation != self.generation
            || request.provider != self.provider.id
            || self.pending.get(&request.key) != Some(request)
        {
            return Err(SpatialError::StaleRequest);
        }
        let bytes = match &result {
            MapTileResult::Ready(b) => {
                if b.width() != b.height() || ![128, 256, 512].contains(&b.width()) {
                    return Err(SpatialError::InvalidResource);
                }
                b.pixels().len()
            }
            MapTileResult::Missing(reason) => {
                valid_label(reason)?;
                0
            }
        };
        self.clock = self.clock.checked_add(1).ok_or(SpatialError::Capacity)?;
        if let MapTileResult::Missing(reason) = &result {
            self.last_error = Some(reason.clone());
        }
        self.pending.remove(&request.key);
        if let Some(old) = self.cache.insert(
            request.key,
            TileEntry {
                value: result,
                touched: self.clock,
                bytes,
            },
        ) {
            self.cache_bytes -= old.bytes;
        }
        self.cache_bytes += bytes;
        self.evict();
        Ok(())
    }
    fn evict(&mut self) {
        while self.cache.len() > self.cache_capacity || self.cache_bytes > MAX_MAP_CACHE_BYTES {
            let Some(key) = self
                .cache
                .iter()
                .min_by_key(|(k, e)| (e.touched, k.zoom, k.x, k.y))
                .map(|(k, _)| *k)
            else {
                break;
            };
            self.cache_bytes -= self.cache.remove(&key).unwrap().bytes;
        }
    }
    pub fn retry_missing_tiles(&mut self) {
        self.cache
            .retain(|_, v| matches!(v.value, MapTileResult::Ready(_)));
        self.last_error = None;
    }
    pub fn request_service(
        &mut self,
        kind: MapServiceKind,
    ) -> Result<MapServiceRequest, SpatialError> {
        match &kind {
            MapServiceKind::Search(q) => {
                valid_label(q)?;
                if q.trim().is_empty() {
                    return Err(SpatialError::InvalidLabel);
                }
            }
            MapServiceKind::Route { from, to } => {
                from.validate()?;
                to.validate()?;
            }
        }
        self.sequence = self.sequence.checked_add(1).ok_or(SpatialError::Capacity)?;
        let request = MapServiceRequest {
            generation: self.generation,
            sequence: self.sequence,
            provider: self.provider.id.clone(),
            kind,
        };
        self.service = Some(request.clone());
        Ok(request)
    }
    pub fn complete_service(
        &mut self,
        request: &MapServiceRequest,
        result: MapServiceResult,
    ) -> Result<(), SpatialError> {
        if self.service.as_ref() != Some(request) || request.generation != self.generation {
            return Err(SpatialError::StaleRequest);
        }
        match (&request.kind, result) {
            (MapServiceKind::Search(_), MapServiceResult::Search(markers)) => {
                self.replace_layers(markers, self.vectors.clone())?
            }
            (MapServiceKind::Route { .. }, MapServiceResult::Route(route)) => {
                let mut vectors = self.vectors.clone();
                vectors.retain(|v| v.id != route.id);
                vectors.push(route);
                self.replace_layers(self.markers.clone(), vectors)?;
            }
            _ => return Err(SpatialError::InvalidResource),
        }
        self.service = None;
        Ok(())
    }
    pub fn diagnostics(&self) -> MapDiagnostics {
        MapDiagnostics {
            missing_tiles: self
                .cache
                .values()
                .filter(|e| matches!(e.value, MapTileResult::Missing(_)))
                .count(),
            ready_tiles: self
                .cache
                .values()
                .filter(|e| matches!(e.value, MapTileResult::Ready(_)))
                .count(),
            pending_tiles: self.pending.len(),
            cache_bytes: self.cache_bytes,
            last_error: self.last_error.clone(),
        }
    }
    pub fn table(&self) -> SpatialTable {
        SpatialTable {
            columns: vec!["Marker".into(), "Longitude".into(), "Latitude".into()],
            rows: self
                .markers
                .iter()
                .map(|m| {
                    vec![
                        m.label.clone(),
                        m.position.longitude.to_string(),
                        m.position.latitude.to_string(),
                    ]
                })
                .collect(),
        }
    }
    pub fn summary(&self) -> String {
        let selected = self
            .selected
            .and_then(|id| self.markers.iter().find(|m| m.id == id))
            .map(|m| {
                format!(
                    " Selected: {}, longitude {}, latitude {}.",
                    m.label, m.position.longitude, m.position.latitude
                )
            })
            .unwrap_or_default();
        let d = self.diagnostics();
        format!(
            "Map. Zoom {:.1}. {} markers, {} missing tiles, {} pending. {}.{}",
            self.zoom,
            self.markers.len(),
            d.missing_tiles,
            d.pending_tiles,
            self.provider.attribution,
            selected
        )
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
        for tile in self.visible_tiles(b)? {
            if let Some(TileEntry {
                value: MapTileResult::Ready(bitmap),
                ..
            }) = self.cache.get(&tile.key)
            {
                let mut image = ImagePrimitive::new(bitmap.clone(), tile.bounds);
                image.clip = clip;
                scene.image(image);
            } else {
                scene.rounded_stroke(tile.bounds, clip, 0.0, 1.0, p.border);
                let text = if self.pending.contains_key(&tile.key) {
                    "Loading tile"
                } else {
                    "Missing tile"
                };
                label(
                    scene,
                    text,
                    Rect::new(tile.bounds.x + 8.0, tile.bounds.y + 8.0, 140.0, 22.0),
                    clip,
                    p.muted,
                    11.0,
                );
            }
        }
        for vector in &self.vectors {
            for pair in vector.vertices.windows(2) {
                let a = self.screen_point(pair[0], b)?;
                let c = self.screen_point(pair[1], b)?;
                // Split at wrap boundaries instead of drawing across the entire world.
                if (a.x - c.x).abs() <= self.world_size() as f32 * 0.5 {
                    line(scene, a, c, clip, 2.5, p.accent);
                }
            }
        }
        for marker in &self.markers {
            let c = self.screen_point(marker.position, b)?;
            let bounds = Rect::new(c.x - 8.0, c.y - 8.0, 16.0, 16.0);
            scene.rounded_fill(bounds, clip, 8.0, p.accent);
            scene.rounded_stroke(
                bounds,
                clip,
                8.0,
                if self.selected == Some(marker.id) {
                    3.0
                } else {
                    1.0
                },
                p.surface,
            );
            if self.selected == Some(marker.id) {
                label(
                    scene,
                    &marker.label,
                    Rect::new(c.x + 12.0, c.y - 12.0, 240.0, 24.0),
                    clip,
                    p.foreground,
                    13.0,
                );
            }
        }
        let footer = Rect::new(
            b.x,
            b.y + (b.height - 24.0).max(0.0),
            b.width,
            24.0f32.min(b.height),
        );
        scene.rounded_fill(footer, clip, 0.0, p.surface);
        label(
            scene,
            &self.provider.attribution,
            Rect::new(
                footer.x + 6.0,
                footer.y + 2.0,
                (footer.width - 12.0).max(0.0),
                20.0,
            ),
            clip,
            p.foreground,
            11.0,
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn map() -> Map {
        Map::new(MapProvider::new("offline", "Own offline fixtures").unwrap()).unwrap()
    }
    fn bounds() -> Rect {
        Rect::new(10.0, 20.0, 640.0, 400.0)
    }
    #[test]
    fn projection_roundtrip_and_anchor_zoom_agree() {
        let mut m = map();
        for projection in [MapProjection::WebMercator, MapProjection::Equirectangular] {
            m.set_projection(projection).unwrap();
            for (lon, lat) in [(0.0, 0.0), (37.6, 55.7), (-179.9, -80.0), (179.9, 80.0)] {
                let p = GeoPoint::new(lon, lat).unwrap();
                m.set_view(p, 4.0).unwrap();
                let s = m.screen_point(p, bounds()).unwrap();
                let q = m.geo_point(s, bounds()).unwrap();
                assert!((q.longitude - p.longitude).abs() < 1e-8);
                assert!((q.latitude - p.latitude).abs() < 1e-8);
            }
        }
        m.set_projection(MapProjection::WebMercator).unwrap();
        m.set_view(GeoPoint::new(10.0, 30.0).unwrap(), 4.0).unwrap();
        let anchor = Point::new(220.0, 150.0);
        let before = m.geo_point(anchor, bounds()).unwrap();
        m.zoom_at(1.25, anchor, bounds()).unwrap();
        assert!(distance(anchor, m.screen_point(before, bounds()).unwrap()) < 0.001);
    }
    #[test]
    fn stale_tile_provider_cancel_and_bounded_cache() {
        let mut m = map();
        m.set_cache_capacity(1).unwrap();
        let requests = m.request_tiles(bounds()).unwrap();
        assert!(requests.len() > 1);
        m.complete_tile(
            &requests[0],
            MapTileResult::Ready(Bitmap::new_rgba8(128, 128, vec![255; 128 * 128 * 4]).unwrap()),
        )
        .unwrap();
        m.complete_tile(&requests[1], MapTileResult::Missing("Not available".into()))
            .unwrap();
        assert_eq!(m.cache.len(), 1);
        assert_eq!(m.cache_bytes, 0);
        let old = requests.last().unwrap();
        m.pan(Point::new(2.0, 0.0)).unwrap();
        assert_eq!(
            m.complete_tile(old, MapTileResult::Missing("Late".into())),
            Err(SpatialError::StaleRequest)
        );
        let next = m.request_tiles(bounds()).unwrap();
        m.set_provider(MapProvider::new("other", "Other owner").unwrap())
            .unwrap();
        assert_eq!(
            m.complete_tile(&next[0], MapTileResult::Missing("Late".into())),
            Err(SpatialError::StaleRequest)
        );
        assert_eq!(m.diagnostics().pending_tiles, 0);
    }
    #[test]
    fn invalid_result_can_retry_and_visible_marker_wins() {
        let mut m = map();
        let r = m.request_tiles(bounds()).unwrap().remove(0);
        assert_eq!(
            m.complete_tile(
                &r,
                MapTileResult::Ready(Bitmap::new_rgba8(1, 1, vec![0; 4]).unwrap())
            ),
            Err(SpatialError::InvalidResource)
        );
        m.complete_tile(&r, MapTileResult::Missing("Missing".into()))
            .unwrap();
        m.replace_layers(
            vec![
                MapMarker {
                    id: 1,
                    position: m.center(),
                    label: "First".into(),
                },
                MapMarker {
                    id: 2,
                    position: m.center(),
                    label: "Second".into(),
                },
            ],
            vec![],
        )
        .unwrap();
        let p = m.screen_point(m.center(), bounds()).unwrap();
        assert_eq!(m.hit_test(p, bounds()), Some(2));
        assert_eq!(m.table().rows.len(), 2);
    }
    #[test]
    fn search_and_route_results_are_atomic_and_generation_checked() {
        let mut m = map();
        let a = m
            .request_service(MapServiceKind::Search("A".into()))
            .unwrap();
        let b = m
            .request_service(MapServiceKind::Search("B".into()))
            .unwrap();
        assert_eq!(
            m.complete_service(&a, MapServiceResult::Search(vec![])),
            Err(SpatialError::StaleRequest)
        );
        let marker = MapMarker {
            id: 3,
            position: m.center(),
            label: "Place".into(),
        };
        m.complete_service(&b, MapServiceResult::Search(vec![marker]))
            .unwrap();
        assert_eq!(m.markers.len(), 1);
        let r = m
            .request_service(MapServiceKind::Route {
                from: m.center(),
                to: GeoPoint::new(2.0, 1.0).unwrap(),
            })
            .unwrap();
        assert!(
            m.complete_service(
                &r,
                MapServiceResult::Route(MapPolyline {
                    id: 1,
                    label: "Invalid".into(),
                    vertices: vec![]
                })
            )
            .is_err()
        );
        assert!(m.vectors.is_empty());
        assert_eq!(m.service, Some(r));
    }
    #[test]
    fn cache_byte_budget_attribution_and_transport_cancellation_are_explicit() {
        let mut m = map();
        m.set_cache_capacity(256).unwrap();
        m.set_view(m.center(), 5.0).unwrap();
        let requests = m
            .request_tiles(Rect::new(0.0, 0.0, 2400.0, 1600.0))
            .unwrap();
        assert!(requests.len() > 32);
        let bitmap = Bitmap::new_rgba8(512, 512, vec![255; 512 * 512 * 4]).unwrap();
        for request in requests.iter().take(40) {
            m.complete_tile(request, MapTileResult::Ready(bitmap.clone()))
                .unwrap();
        }
        assert!(m.diagnostics().cache_bytes <= MAX_MAP_CACHE_BYTES);
        assert_eq!(m.diagnostics().ready_tiles, 32);
        let pending = m
            .request_tiles(Rect::new(0.0, 0.0, 2400.0, 1600.0))
            .unwrap();
        let request = pending.first().unwrap();
        assert!(m.is_current_tile_request(request));
        let (cancelled, _) = m.cancel_requests().unwrap();
        assert!(cancelled.contains(request));
        assert!(!m.is_current_tile_request(request));
        assert!(m.summary().contains("Own offline fixtures"));
        let mut scene = Scene::default();
        m.paint(
            &mut scene,
            bounds(),
            bounds(),
            SpatialPalette::from_theme(crate::Theme::FluentLight),
        )
        .unwrap();
        assert!(
            scene
                .texts
                .iter()
                .any(|text| text.text == "Own offline fixtures")
        );
    }
    #[test]
    fn marker_behind_attribution_is_not_selected_and_date_line_wrap_is_consistent() {
        let mut m = map();
        m.set_view(GeoPoint::new(179.5, 20.0).unwrap(), 5.0)
            .unwrap();
        let p = GeoPoint::new(-179.5, 20.0).unwrap();
        let s = m.screen_point(p, bounds()).unwrap();
        let roundtrip = m.geo_point(s, bounds()).unwrap();
        assert!((roundtrip.longitude - p.longitude).abs() < 1e-5);
        let covered = Point::new(bounds().x + 80.0, bounds().y + bounds().height - 8.0);
        let geo = m.geo_point(covered, bounds()).unwrap();
        m.replace_layers(
            vec![MapMarker {
                id: 1,
                position: geo,
                label: "Covered".into(),
            }],
            vec![],
        )
        .unwrap();
        assert_eq!(m.hit_test(covered, bounds()), None);
        assert!(GeoPoint::new(0.0, 90.0).is_err());
        assert!(GeoPoint::new(f64::NAN, 0.0).is_err());
        assert!(MapProvider::new("source", "").is_err());
    }
}
