use parley::{
    Affinity, Alignment, AlignmentOptions, Cursor, FontContext, FontWeight, Layout, LayoutContext,
    PositionedLayoutItem, StyleProperty, TextWrapMode,
};
use rust_desktop_ui_core::{Color, Point, Rect, TextAlign, TextRun};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    fmt,
    ops::Range,
    sync::Arc,
};
use swash::{
    FontRef,
    scale::{Render, ScaleContext, Source, StrikeWith, image::Content},
    zeno::{Format, Vector},
};
use unicode_segmentation::UnicodeSegmentation;

const MAX_LAYOUTS: usize = 1024;
const MAX_GLYPHS: usize = 4096;
const MAX_RASTER_BYTES: usize = 64 * 1024 * 1024;
const MAX_TEXT_BYTES: usize = 1024 * 1024;
const MAX_LAYOUT_TEXT_BYTES: usize = 8 * 1024 * 1024;
const MAX_LAYOUT_CLUSTERS: usize = 262_144;
const MAX_PREPARED_GLYPHS: usize = 65_536;

/// Prepared outputs hold bitmap Arcs independently of the engine cache.
#[derive(Default)]
struct RasterBudget {
    allocations: HashSet<usize>,
    bytes: usize,
}

impl RasterBudget {
    fn include(&mut self, allocation: usize, bytes: usize) -> Result<(), TextError> {
        if self.allocations.contains(&allocation) {
            return Ok(());
        }
        let total = self
            .bytes
            .checked_add(bytes)
            .ok_or(TextError::RasterLimit)?;
        if total > MAX_RASTER_BYTES {
            return Err(TextError::RasterLimit);
        }
        self.allocations.insert(allocation);
        self.bytes = total;
        Ok(())
    }
}

/// Stable within a process for a font resource, glyph, size and raster position.
/// Font variations and quarter-pixel positioning are included in identity.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct GlyphKey {
    font: u64,
    font_index: u32,
    glyph: u32,
    size: u32,
    coords: Arc<[i16]>,
    x_fraction: u8,
    y_fraction: u8,
    embolden: bool,
}

/// Straight RGBA pixels: white RGB with alpha for outlines, original color for emoji.
#[derive(Debug)]
pub struct GlyphBitmap {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct RasterGlyph {
    pub key: GlyphKey,
    /// Physical-pixel top-left, including the font's raster bearing.
    pub position: Point,
    pub clip: Rect,
    pub bitmap: Arc<GlyphBitmap>,
    /// Text color for masks; white with text opacity for color glyphs.
    pub tint: Color,
}

/// A bidi/soft-wrap junction can have two visual carets at the same byte index.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CaretAffinity {
    Upstream,
    #[default]
    Downstream,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TextCaret {
    pub index: usize,
    pub affinity: CaretAffinity,
}

/// The system font actually selected for a shaped range, including fallback.
/// Names are diagnostic strings from the font, not persistent resource keys.
#[derive(Clone, Debug, PartialEq)]
pub struct TextFontRun {
    pub text_range: Range<usize>,
    pub line_index: usize,
    pub font_name: String,
    pub font_size: f32,
    pub is_rtl: bool,
    pub glyph_count: usize,
}

/// A shaped line in global physical pixels, before clipping.
#[derive(Clone, Debug, PartialEq)]
pub struct TextLineMetrics {
    pub text_range: Range<usize>,
    pub bounds: Rect,
    pub baseline: f32,
    pub ascent: f32,
    pub descent: f32,
}

#[derive(Clone, Debug)]
pub struct PreparedText {
    pub glyphs: Vec<RasterGlyph>,
    /// Intrinsic layout dimensions in physical pixels.
    pub width: f32,
    pub height: f32,
    /// Actual shaped .notdef glyphs; nonzero means system-font coverage is incomplete.
    pub missing_glyphs: usize,
    /// Includes spaces, nonpainting glyphs and glyphs outside the clip.
    pub shaped_glyphs: usize,
    /// Glyphs with a bitmap, before clipping. Whitespace need not have one.
    pub rasterized_glyphs: usize,
    /// Color bitmaps, before clipping; these retain their font palette colors.
    pub color_glyphs: usize,
    layout: Arc<Layout<()>>,
    text: Arc<str>,
    graphemes: Arc<[usize]>,
    origin: Point,
    scale: f32,
}

impl PreparedText {
    /// Preserve this affinity when implementing visual bidi arrow navigation.
    pub fn hit_test_caret(&self, position: Point) -> TextCaret {
        if !position.is_finite() {
            return TextCaret::default();
        }
        let cursor = Cursor::from_point(
            &self.layout,
            position.x - self.origin.x,
            position.y - self.origin.y,
        );
        if self.graphemes.binary_search(&cursor.index()).is_ok() {
            return self.owned_cursor(cursor);
        }
        // Shaping clusters can split an extended grapheme (Indic conjuncts,
        // regional indicators without a flag ligature). Choose its nearest
        // visual edge, rather than always snapping to its logical start.
        let before = self.snap(cursor.index());
        let after = self.graphemes[self.graphemes.partition_point(|&i| i <= cursor.index())];
        let candidates = [
            TextCaret {
                index: before,
                affinity: CaretAffinity::Downstream,
            },
            TextCaret {
                index: after,
                affinity: CaretAffinity::Upstream,
            },
        ];
        candidates
            .into_iter()
            .min_by(|&a, &b| {
                caret_distance(self.caret_geometry(a), position)
                    .total_cmp(&caret_distance(self.caret_geometry(b), position))
            })
            .unwrap_or_default()
    }

    pub fn caret_geometry(&self, caret: TextCaret) -> Rect {
        self.global(
            self.parley_cursor(caret)
                .geometry(&self.layout, self.scale.max(1.0)),
        )
    }

    pub fn move_caret_visual(&self, caret: TextCaret, right: bool) -> TextCaret {
        let mut cursor = self.parley_cursor(caret);
        // Advance through shaping clusters until a complete grapheme edge is
        // reached. Rounding each intermediate index down would repeatedly return
        // the same position when a single grapheme contains multiple clusters.
        for _ in 0..self.text.len().saturating_add(2) {
            let next = if right {
                cursor.next_visual(&self.layout)
            } else {
                cursor.previous_visual(&self.layout)
            };
            if next == cursor || self.graphemes.binary_search(&next.index()).is_ok() {
                return self.owned_cursor(next);
            }
            cursor = next;
        }
        caret
    }

    /// Nearest extended-grapheme byte boundary for a global physical point.
    pub fn hit_test(&self, position: Point) -> usize {
        self.hit_test_caret(position).index
    }

    /// Global physical caret geometry. An invalid byte index is clamped down.
    pub fn caret_rect(&self, byte_index: usize) -> Rect {
        let cursor =
            Cursor::from_byte_index(&self.layout, self.snap(byte_index), Affinity::Downstream);
        self.global(cursor.geometry(&self.layout, self.scale.max(1.0)))
    }

    /// Bidi-aware selection fragments in global physical coordinates.
    /// These are not clipped, allowing the host to scroll a caret into view.
    pub fn selection_rects(&self, range: Range<usize>) -> Vec<Rect> {
        let anchor =
            Cursor::from_byte_index(&self.layout, self.snap(range.start), Affinity::Downstream);
        let focus = Cursor::from_byte_index(&self.layout, self.snap(range.end), Affinity::Upstream);
        parley::Selection::new(anchor, focus)
            .geometry(&self.layout)
            .into_iter()
            .map(|(rect, _)| self.global(rect))
            .collect()
    }

    /// A visual arrow movement, unlike TextDocument's logical grapheme movement.
    pub fn move_visual(&self, byte_index: usize, right: bool) -> usize {
        self.move_caret_visual(
            TextCaret {
                index: byte_index,
                affinity: CaretAffinity::Downstream,
            },
            right,
        )
        .index
    }

    pub fn line_count(&self) -> usize {
        self.layout.len()
    }
    pub fn is_rtl(&self) -> bool {
        self.layout.is_rtl()
    }

    /// Inspect the fonts used by shaping; no font bytes or file paths are exposed.
    pub fn font_runs(&self) -> Vec<TextFontRun> {
        self.layout
            .lines()
            .enumerate()
            .flat_map(|(line_index, line)| {
                line.runs().map(move |run| {
                    let font = run.font();
                    let font_name = FontRef::from_index(font.data.data(), font.index as usize)
                        .and_then(|font| {
                            font.localized_strings()
                                .find_by_id(swash::StringId::Full, Some("en"))
                        })
                        .map(|name| name.to_string())
                        .unwrap_or_else(|| "Unnamed font".into());
                    TextFontRun {
                        text_range: run.text_range(),
                        line_index,
                        font_name,
                        font_size: run.font_size(),
                        is_rtl: run.is_rtl(),
                        glyph_count: run.clusters().map(|cluster| cluster.glyphs().count()).sum(),
                    }
                })
            })
            .collect()
    }

    pub fn line_metrics(&self) -> Vec<TextLineMetrics> {
        self.layout
            .lines()
            .map(|line| {
                let metrics = line.metrics();
                TextLineMetrics {
                    text_range: line.text_range(),
                    bounds: Rect::new(
                        self.origin.x + metrics.inline_min_coord,
                        self.origin.y + metrics.block_min_coord,
                        metrics.inline_max_coord - metrics.inline_min_coord,
                        metrics.block_max_coord - metrics.block_min_coord,
                    ),
                    baseline: self.origin.y + metrics.baseline,
                    ascent: metrics.ascent,
                    descent: metrics.descent,
                }
            })
            .collect()
    }

    fn parley_cursor(&self, caret: TextCaret) -> Cursor {
        Cursor::from_byte_index(
            &self.layout,
            self.snap(caret.index),
            match caret.affinity {
                CaretAffinity::Upstream => Affinity::Upstream,
                CaretAffinity::Downstream => Affinity::Downstream,
            },
        )
    }

    fn owned_cursor(&self, cursor: Cursor) -> TextCaret {
        TextCaret {
            index: self.snap(cursor.index()),
            affinity: match cursor.affinity() {
                Affinity::Upstream => CaretAffinity::Upstream,
                Affinity::Downstream => CaretAffinity::Downstream,
            },
        }
    }

    fn snap(&self, index: usize) -> usize {
        self.graphemes[self
            .graphemes
            .partition_point(|&i| i <= index)
            .saturating_sub(1)]
    }
    fn global(&self, rect: parley::BoundingBox) -> Rect {
        Rect::new(
            self.origin.x + rect.x0 as f32,
            self.origin.y + rect.y0 as f32,
            rect.width() as f32,
            rect.height() as f32,
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TextError {
    InvalidRun,
    InvalidScale,
    TextTooLong,
    NoSystemFont,
    UnsupportedFont,
    RasterLimit,
    InvalidBitmap,
}
impl fmt::Display for TextError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidRun => "invalid text bounds, clip, size, weight or color",
            Self::InvalidScale => "text scale must be positive and finite; physical font size must not exceed 1024 pixels",
            Self::TextTooLong => "text run exceeds the one MiB preparation limit",
            Self::NoSystemFont => "system font lookup produced no glyphs for nonempty visible text",
            Self::UnsupportedFont => "a selected system font could not be read by the rasterizer",
            Self::RasterLimit => "glyph raster dimensions or prepared glyph count exceed the supported limit",
            Self::InvalidBitmap => "rasterizer produced inconsistent glyph pixels",
        })
    }
}
impl std::error::Error for TextError {}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TextStats {
    pub layouts: u64,
    pub layout_hits: u64,
    pub rasterizations: u64,
    pub raster_hits: u64,
    pub cached_layouts: usize,
    pub cached_glyphs: usize,
    pub cached_raster_bytes: usize,
    /// UTF-8 bytes retained as layout cache keys (not total allocator usage).
    pub cached_layout_text_bytes: usize,
    pub cached_layout_clusters: usize,
}

struct CachedLayout {
    layout: Arc<Layout<()>>,
    graphemes: Arc<[usize]>,
    clusters: usize,
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct LayoutKey {
    text: Arc<str>,
    size: u32,
    scale: u32,
    width: u32,
    weight: u16,
    align: TextAlign,
    wrap: bool,
}
#[derive(Clone)]
struct CachedGlyph {
    bitmap: Arc<GlyphBitmap>,
    left: i32,
    top: i32,
    color: bool,
}

/// Parley shaping and Swash raster caches shared by a window or application.
/// Layout keys exclude screen position, clip and color, so scrolling reuses shaping.
pub struct TextEngine {
    fonts: FontContext,
    layout_context: LayoutContext<()>,
    scale_context: ScaleContext,
    layouts: HashMap<LayoutKey, Arc<CachedLayout>>,
    layout_order: VecDeque<LayoutKey>,
    glyphs: HashMap<GlyphKey, Option<CachedGlyph>>,
    glyph_order: VecDeque<GlyphKey>,
    font_keys: HashMap<(u64, u32), swash::CacheKey>,
    stats: TextStats,
}

impl Default for TextEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl TextEngine {
    pub fn new() -> Self {
        Self {
            fonts: FontContext::new(),
            layout_context: LayoutContext::new(),
            scale_context: ScaleContext::new(),
            layouts: HashMap::new(),
            layout_order: VecDeque::new(),
            glyphs: HashMap::new(),
            glyph_order: VecDeque::new(),
            font_keys: HashMap::new(),
            stats: TextStats::default(),
        }
    }

    pub fn stats(&self) -> TextStats {
        self.stats
    }

    pub fn clear_caches(&mut self) {
        self.layouts.clear();
        self.layout_order.clear();
        self.glyphs.clear();
        self.glyph_order.clear();
        self.font_keys.clear();
        self.stats.cached_layouts = 0;
        self.stats.cached_glyphs = 0;
        self.stats.cached_raster_bytes = 0;
        self.stats.cached_layout_text_bytes = 0;
        self.stats.cached_layout_clusters = 0;
    }

    pub fn prepare(&mut self, run: &TextRun, scale: f32) -> Result<PreparedText, TextError> {
        if !run.is_valid() {
            return Err(TextError::InvalidRun);
        }
        if !scale.is_finite()
            || scale <= 0.0
            || !((run.font_size * scale).is_finite())
            || run.font_size * scale > 1024.0
        {
            return Err(TextError::InvalidScale);
        }
        if run.text.len() > MAX_TEXT_BYTES {
            return Err(TextError::TextTooLong);
        }
        let bounds = scaled_rect(run.bounds, scale).ok_or(TextError::InvalidScale)?;
        let clip = scaled_rect(run.clip, scale).ok_or(TextError::InvalidScale)?;
        let clip = bounds
            .intersection(clip)
            .unwrap_or(Rect::new(bounds.x, bounds.y, 0.0, 0.0));
        let key = LayoutKey {
            text: Arc::from(run.text.as_str()),
            size: run.font_size.to_bits(),
            scale: scale.to_bits(),
            width: bounds.width.to_bits(),
            weight: run.weight,
            align: run.align,
            wrap: run.wrap,
        };
        let layout = if let Some(layout) = self.layouts.get(&key) {
            self.stats.layout_hits += 1;
            Arc::clone(layout)
        } else {
            let mut builder =
                self.layout_context
                    .ranged_builder(&mut self.fonts, &run.text, scale, true);
            builder.push_default(StyleProperty::FontSize(run.font_size));
            builder.push_default(StyleProperty::FontWeight(FontWeight::new(f32::from(
                run.weight,
            ))));
            builder.push_default(StyleProperty::TextWrapMode(if run.wrap {
                TextWrapMode::Wrap
            } else {
                TextWrapMode::NoWrap
            }));
            let mut layout: Layout<()> = builder.build(&run.text);
            layout.break_all_lines(Some(bounds.width));
            layout.align(
                match run.align {
                    TextAlign::Start => Alignment::Start,
                    TextAlign::Center => Alignment::Center,
                    TextAlign::End => Alignment::End,
                },
                AlignmentOptions::default(),
            );
            if !layout.width().is_finite() || !layout.height().is_finite() {
                return Err(TextError::InvalidRun);
            }
            let mut shaped_glyphs = 0;
            let mut clusters = 0;
            for line in layout.lines() {
                for run in line.runs() {
                    clusters += run.len();
                    for cluster in run.clusters() {
                        shaped_glyphs += cluster.glyphs().count();
                        if shaped_glyphs > MAX_PREPARED_GLYPHS || clusters > MAX_LAYOUT_CLUSTERS {
                            return Err(TextError::RasterLimit);
                        }
                    }
                }
            }
            let layout = Arc::new(CachedLayout {
                layout: Arc::new(layout),
                graphemes: run
                    .text
                    .grapheme_indices(true)
                    .map(|(i, _)| i)
                    .chain(std::iter::once(run.text.len()))
                    .collect(),
                clusters,
            });
            while self.layouts.len() >= MAX_LAYOUTS
                || self.stats.cached_layout_text_bytes + key.text.len() > MAX_LAYOUT_TEXT_BYTES
                || self.stats.cached_layout_clusters + clusters > MAX_LAYOUT_CLUSTERS
            {
                let Some(old) = self.layout_order.pop_front() else {
                    break;
                };
                if let Some(removed) = self.layouts.remove(&old) {
                    self.stats.cached_layout_text_bytes -= old.text.len();
                    self.stats.cached_layout_clusters -= removed.clusters;
                }
            }
            self.layout_order.push_back(key.clone());
            self.layouts.insert(key.clone(), Arc::clone(&layout));
            self.stats.layouts += 1;
            self.stats.cached_layouts = self.layouts.len();
            self.stats.cached_layout_text_bytes += key.text.len();
            self.stats.cached_layout_clusters += clusters;
            layout
        };
        let graphemes = Arc::clone(&layout.graphemes);
        let layout = Arc::clone(&layout.layout);
        let mut glyphs = Vec::new();
        let mut raster_budget = RasterBudget::default();
        let mut missing_glyphs = 0;
        let mut shaped_glyphs = 0usize;
        let mut rasterized_glyphs = 0;
        let mut color_glyphs = 0;
        for line in layout.lines() {
            for item in line.items() {
                let PositionedLayoutItem::GlyphRun(glyph_run) = item else {
                    continue;
                };
                let font_run = glyph_run.run();
                let font = font_run.font();
                let coords: Arc<[i16]> = Arc::from(font_run.normalized_coords());
                for glyph in glyph_run.positioned_glyphs() {
                    shaped_glyphs += 1;
                    if shaped_glyphs > MAX_PREPARED_GLYPHS {
                        return Err(TextError::RasterLimit);
                    }
                    if glyph.id == 0 {
                        missing_glyphs += 1;
                    }
                    let x = bounds.x + glyph.x;
                    let y = bounds.y + glyph.y;
                    if !x.is_finite() || !y.is_finite() {
                        return Err(TextError::InvalidRun);
                    }
                    let key = GlyphKey {
                        font: font.data.id(),
                        font_index: font.index,
                        glyph: glyph.id,
                        size: font_run.font_size().to_bits(),
                        coords: Arc::clone(&coords),
                        x_fraction: ((x - x.floor()) * 4.0).floor() as u8,
                        y_fraction: ((y - y.floor()) * 4.0).floor() as u8,
                        embolden: font_run.synthesis().embolden(),
                    };
                    let image = self.raster(&key, font)?;
                    if let Some(image) = image {
                        rasterized_glyphs += 1;
                        color_glyphs += usize::from(image.color);
                        let position =
                            Point::new(x.floor() + image.left as f32, y.floor() - image.top as f32);
                        let glyph_bounds = Rect::new(
                            position.x,
                            position.y,
                            image.bitmap.width as f32,
                            image.bitmap.height as f32,
                        );
                        if glyph_bounds.intersection(clip).is_none() {
                            continue;
                        }
                        // A key can be rasterized again after cache eviction;
                        // count allocations, not logical glyph keys. Output Arcs
                        // keep every counted address alive until preparation ends.
                        raster_budget.include(
                            Arc::as_ptr(&image.bitmap) as usize,
                            image.bitmap.rgba.len(),
                        )?;
                        let tint = if image.color {
                            Color {
                                r: 1.0,
                                g: 1.0,
                                b: 1.0,
                                a: run.color.a,
                            }
                        } else {
                            run.color
                        };
                        glyphs.push(RasterGlyph {
                            key,
                            position,
                            clip,
                            bitmap: image.bitmap,
                            tint,
                        });
                    }
                }
            }
        }
        if shaped_glyphs == 0
            && run
                .text
                .chars()
                .any(|c| !c.is_whitespace() && !c.is_control())
        {
            return Err(TextError::NoSystemFont);
        }
        Ok(PreparedText {
            glyphs,
            width: layout.width(),
            height: layout.height(),
            missing_glyphs,
            shaped_glyphs,
            rasterized_glyphs,
            color_glyphs,
            layout,
            text: key.text,
            graphemes,
            origin: Point::new(bounds.x, bounds.y),
            scale,
        })
    }

    fn raster(
        &mut self,
        key: &GlyphKey,
        font: &parley::FontData,
    ) -> Result<Option<CachedGlyph>, TextError> {
        if let Some(image) = self.glyphs.get(key) {
            self.stats.raster_hits += 1;
            return Ok(image.clone());
        }
        let mut font_ref = FontRef::from_index(font.data.data(), font.index as usize)
            .ok_or(TextError::UnsupportedFont)?;
        font_ref.key = *self
            .font_keys
            .entry((key.font, key.font_index))
            .or_default();
        let size = f32::from_bits(key.size);
        let mut scaler = self
            .scale_context
            .builder(font_ref)
            .size(size)
            .hint(true)
            .normalized_coords(key.coords.iter())
            .build();
        let sources = [
            Source::ColorOutline(0),
            Source::ColorBitmap(StrikeWith::BestFit),
            Source::Outline,
        ];
        let mut render = Render::new(&sources);
        render
            .format(Format::Alpha)
            .offset(Vector::new(
                f32::from(key.x_fraction) / 4.0,
                f32::from(key.y_fraction) / 4.0,
            ))
            .default_color([255, 255, 255, 255]);
        if key.embolden {
            render.embolden(size * 0.025);
        }
        let glyph_id = u16::try_from(key.glyph).map_err(|_| TextError::UnsupportedFont)?;
        let cached = match render.render(&mut scaler, glyph_id) {
            Some(image) => {
                let width = image.placement.width;
                let height = image.placement.height;
                let pixels = (width as usize)
                    .checked_mul(height as usize)
                    .ok_or(TextError::RasterLimit)?;
                if pixels > 4 * 1024 * 1024 || width > 4096 || height > 4096 {
                    return Err(TextError::RasterLimit);
                }
                let color = image.content == Content::Color;
                let rgba = match image.content {
                    Content::Mask => {
                        if image.data.len() != pixels {
                            return Err(TextError::InvalidBitmap);
                        }
                        image
                            .data
                            .into_iter()
                            .flat_map(|alpha| [255, 255, 255, alpha])
                            .collect()
                    }
                    Content::Color => {
                        if image.data.len() != pixels * 4 {
                            return Err(TextError::InvalidBitmap);
                        }
                        let mut rgba = image.data;
                        // Swash's layered outline compositor produces premultiplied
                        // pixels. PNG bitmap sources already use straight RGBA.
                        if matches!(image.source, Source::ColorOutline(_)) {
                            for pixel in rgba.chunks_exact_mut(4) {
                                let alpha = u32::from(pixel[3]);
                                for channel in &mut pixel[..3] {
                                    *channel = (u32::from(*channel) * 255 + alpha / 2)
                                        .checked_div(alpha)
                                        .unwrap_or(0)
                                        .min(255)
                                        as u8;
                                }
                            }
                        }
                        rgba
                    }
                    Content::SubpixelMask => return Err(TextError::InvalidBitmap),
                };
                Some(CachedGlyph {
                    bitmap: Arc::new(GlyphBitmap {
                        width,
                        height,
                        rgba,
                    }),
                    left: image.placement.left,
                    top: image.placement.top,
                    color,
                })
            }
            None => None,
        };
        let bytes = cached.as_ref().map_or(0, |image| image.bitmap.rgba.len());
        while self.glyphs.len() >= MAX_GLYPHS
            || self.stats.cached_raster_bytes + bytes > MAX_RASTER_BYTES
        {
            let Some(old) = self.glyph_order.pop_front() else {
                break;
            };
            if let Some(Some(image)) = self.glyphs.remove(&old) {
                self.stats.cached_raster_bytes -= image.bitmap.rgba.len();
            }
        }
        self.glyph_order.push_back(key.clone());
        self.glyphs.insert(key.clone(), cached.clone());
        self.stats.rasterizations += 1;
        self.stats.cached_glyphs = self.glyphs.len();
        self.stats.cached_raster_bytes += bytes;
        Ok(cached)
    }
}

fn caret_distance(rect: Rect, point: Point) -> f32 {
    let dx = point.x - rect.x;
    let dy = if point.y < rect.y {
        rect.y - point.y
    } else {
        (point.y - rect.y - rect.height).max(0.0)
    };
    dx.hypot(dy)
}

fn scaled_rect(rect: Rect, scale: f32) -> Option<Rect> {
    let result = Rect::new(
        rect.x * scale,
        rect.y * scale,
        rect.width * scale,
        rect.height * scale,
    );
    result.is_valid().then_some(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn run(text: &str) -> TextRun {
        TextRun::new(
            text,
            Rect::new(10.0, 20.0, 300.0, 150.0),
            Color::rgb(10, 20, 30),
            20.0,
        )
    }

    #[test]
    fn prepared_bitmap_budget_counts_shared_glyphs_once_and_rejects_overflow() {
        let mut budget = RasterBudget::default();
        budget.include(1, MAX_RASTER_BYTES / 2).unwrap();
        budget.include(1, MAX_RASTER_BYTES / 2).unwrap();
        assert_eq!(budget.bytes, MAX_RASTER_BYTES / 2);
        budget.include(2, MAX_RASTER_BYTES / 2).unwrap();
        assert!(matches!(budget.include(3, 1), Err(TextError::RasterLimit)));
        assert_eq!(budget.bytes, MAX_RASTER_BYTES);
        assert_eq!(budget.allocations.len(), 2);
        assert!(matches!(
            budget.include(3, usize::MAX),
            Err(TextError::RasterLimit)
        ));
    }

    #[test]
    fn latin_and_cyrillic_produce_real_mask_pixels_and_reuse_layout_and_raster() {
        let mut engine = TextEngine::new();
        let text = run("Hello, Привет! e\u{301}");
        let prepared = engine.prepare(&text, 1.5).unwrap();
        assert!(!prepared.glyphs.is_empty());
        assert_eq!(prepared.missing_glyphs, 0);
        assert!(prepared.glyphs.iter().any(|glyph| {
            glyph
                .bitmap
                .rgba
                .chunks_exact(4)
                .any(|pixel| pixel[3] > 0 && pixel[3] < 255)
        }));
        assert!(prepared.glyphs.iter().all(|glyph| glyph.bitmap.rgba.len()
            == (glyph.bitmap.width * glyph.bitmap.height * 4) as usize));
        let first = engine.stats();
        let again = engine.prepare(&text, 1.5).unwrap();
        assert_eq!(first.layouts, engine.stats().layouts);
        assert_eq!(first.rasterizations, engine.stats().rasterizations);
        assert!(Arc::ptr_eq(
            &prepared.glyphs[0].bitmap,
            &again.glyphs[0].bitmap
        ));
        let mut moved = text.clone();
        moved.bounds.y += 8.0;
        moved.clip.y += 8.0;
        engine.prepare(&moved, 1.5).unwrap();
        assert_eq!(first.layouts, engine.stats().layouts);
        assert_eq!(first.rasterizations, engine.stats().rasterizations);
    }

    #[test]
    fn grapheme_carets_hit_testing_and_bidi_selection_use_shaped_geometry() {
        let mut engine = TextEngine::new();
        let source = "A e\u{301} Привет";
        let prepared = engine.prepare(&run(source), 1.0).unwrap();
        for (index, _) in source.grapheme_indices(true) {
            let caret = prepared.caret_rect(index);
            assert!(caret.is_valid());
            assert_eq!(
                prepared.hit_test(Point::new(caret.x + 0.1, caret.y + caret.height / 2.0)),
                index
            );
        }
        assert_eq!(prepared.caret_rect(3), prepared.caret_rect(2));
        let rtl = engine.prepare(&run("שלום עולם"), 1.0).unwrap();
        assert!(rtl.is_rtl());
        assert!(rtl.caret_rect(0).x > rtl.caret_rect("שלום עולם".len()).x);
        assert!(!rtl.selection_rects(0.."שלום".len()).is_empty());
    }

    #[test]
    fn visual_navigation_retains_affinity_at_bidi_junctions() {
        let mut engine = TextEngine::new();
        let source = "abc אבג xyz";
        let prepared = engine.prepare(&run(source), 1.0).unwrap();
        let mut caret = prepared.hit_test_caret(Point::new(10.0, 30.0));
        let mut visited = vec![caret];
        for _ in 0..source.len() * 2 {
            let next = prepared.move_caret_visual(caret, true);
            if next == caret {
                break;
            }
            assert!(!visited.contains(&next), "visual cursor must make progress");
            assert!(prepared.caret_geometry(next).is_valid());
            assert!(
                source.grapheme_indices(true).any(|(i, _)| i == next.index)
                    || next.index == source.len()
            );
            visited.push(next);
            caret = next;
        }
        assert!(visited.len() >= source.graphemes(true).count());
        assert_eq!(caret.index, source.len());
        assert!(
            visited
                .iter()
                .any(|c| c.affinity == CaretAffinity::Upstream)
        );
    }

    #[test]
    fn a_two_hundred_label_frame_reuses_shaping_on_redraw_and_scroll() {
        let mut engine = TextEngine::new();
        let labels: Vec<_> = (0..200).map(|i| run(&format!("Книга {i}"))).collect();
        for label in &labels {
            engine.prepare(label, 1.5).unwrap();
        }
        let first = engine.stats();
        assert_eq!(first.layouts, 200);
        for label in &labels {
            let mut scrolled = label.clone();
            scrolled.bounds.y += 8.0;
            scrolled.clip.y += 8.0;
            engine.prepare(&scrolled, 1.5).unwrap();
        }
        assert_eq!(engine.stats().layouts, first.layouts);
        assert_eq!(engine.stats().rasterizations, first.rasterizations);
        assert_eq!(engine.stats().layout_hits, 200);
    }

    #[test]
    fn wrapping_alignment_clipping_and_empty_text_are_explicit() {
        let mut engine = TextEngine::new();
        let mut text = run("One two three four five six seven eight");
        text.bounds.width = 80.0;
        text.clip = text.bounds;
        text.wrap = true;
        let wrapped = engine.prepare(&text, 1.0).unwrap();
        assert!(wrapped.line_count() > 1);
        text.wrap = false;
        assert_eq!(engine.prepare(&text, 1.0).unwrap().line_count(), 1);
        text.text = "Hi".into();
        text.align = TextAlign::Center;
        let centered = engine.prepare(&text, 1.0).unwrap();
        assert!(centered.caret_rect(0).x > text.bounds.x + 10.0);
        text.clip = Rect::new(500.0, 500.0, 10.0, 10.0);
        assert!(engine.prepare(&text, 1.0).unwrap().glyphs.is_empty());
        assert!(engine.prepare(&run(""), 1.0).unwrap().glyphs.is_empty());
        assert!(matches!(
            engine.prepare(&run("ok"), f32::NAN),
            Err(TextError::InvalidScale)
        ));
    }

    #[test]
    fn arabic_cjk_and_emoji_keep_valid_layout_and_report_missing_font_coverage() {
        let mut engine = TextEngine::new();
        for source in ["العربية", "中文 日本語", "👩‍💻 🇷🇺"] {
            let prepared = engine.prepare(&run(source), 1.0).unwrap();
            assert!(prepared.width > 0.0 && prepared.height > 0.0);
            assert!(!prepared.glyphs.is_empty());
            for (index, _) in source.grapheme_indices(true) {
                assert!(prepared.caret_rect(index).is_valid());
            }
            // Coverage depends on installed fonts; missing glyphs are never
            // silently counted as successful script coverage.
            assert!(prepared.missing_glyphs <= source.chars().count());
        }
    }

    #[test]
    fn visual_navigation_crosses_complete_complex_graphemes_in_both_directions() {
        let mut engine = TextEngine::new();
        for source in ["aक्षिb", "a👩‍💻b", "a🇷🇺b", "aक्‍षb", "a\u{301}b", "aกำb"]
        {
            let prepared = engine.prepare(&run(source), 1.5).unwrap();
            for right in [true, false] {
                let mut caret = TextCaret {
                    index: if right { 0 } else { source.len() },
                    affinity: CaretAffinity::Downstream,
                };
                let end = if right { source.len() } else { 0 };
                let mut seen = vec![caret];
                for _ in 0..source.len() * 4 {
                    if caret.index == end {
                        break;
                    }
                    let next = prepared.move_caret_visual(caret, right);
                    assert!(
                        !seen.contains(&next),
                        "navigation stuck for {source:?}, right={right}, visited={seen:?}, next={next:?}"
                    );
                    seen.push(next);
                    caret = next;
                }
                assert_eq!(caret.index, end, "{source:?}, right={right}");
            }
        }
    }

    #[test]
    fn complex_grapheme_hit_testing_selects_the_nearest_outer_edge() {
        let mut engine = TextEngine::new();
        for source in ["aक्षिb", "a🇷🇺b", "aक्‍षb"] {
            let prepared = engine.prepare(&run(source), 1.5).unwrap();
            let bounds: Vec<_> = source.grapheme_indices(true).map(|(i, _)| i).collect();
            let start = prepared.caret_rect(bounds[1]);
            let end = prepared.caret_rect(bounds[2]);
            let y = start.y + start.height * 0.5;
            for (fraction, expected) in [(0.1, bounds[1]), (0.9, bounds[2])] {
                let point = Point::new(start.x + (end.x - start.x) * fraction, y);
                assert_eq!(
                    prepared.hit_test(point),
                    expected,
                    "{source:?} at {fraction}"
                );
            }
            for step in 0..101 {
                let point = Point::new(start.x + (end.x - start.x) * step as f32 / 100.0, y);
                assert!(bounds.contains(&prepared.hit_test(point)), "{source:?}");
            }
        }
    }

    #[test]
    fn fallback_metrics_hit_testing_and_selection_share_dpi_and_translation() {
        let mut engine = TextEngine::new();
        let source = "Aक्षि สวัสดี 中文 👩‍💻 אבגZ";
        let mut text = run(source);
        text.bounds.width = 1200.0;
        text.clip = text.bounds;
        let base = engine.prepare(&text, 1.0).unwrap();
        let fonts = base.font_runs();
        assert_eq!(
            fonts.iter().map(|r| r.glyph_count).sum::<usize>(),
            base.shaped_glyphs
        );
        for scale in [0.75, 1.25, 1.5, 2.0] {
            let prepared = engine.prepare(&text, scale).unwrap();
            assert!((prepared.width - base.width * scale).abs() < 0.1);
            assert!((prepared.height - base.height * scale).abs() < 0.1);
            let scaled_fonts = prepared.font_runs();
            assert_eq!(scaled_fonts.len(), fonts.len());
            for (a, b) in fonts.iter().zip(scaled_fonts) {
                assert_eq!(a.font_name, b.font_name);
                assert_eq!(a.text_range, b.text_range);
                assert!((b.font_size - a.font_size * scale).abs() < 0.1);
            }
            for (index, _) in source.grapheme_indices(true) {
                let a = base.caret_rect(index);
                let b = prepared.caret_rect(index);
                assert!((b.x - a.x * scale).abs() < 0.1);
                // The shaper quantizes line block coordinates to device pixels;
                // font advances remain fractional. Compare within that grid.
                assert!(
                    (b.y - a.y * scale).abs() <= scale.max(1.0),
                    "scale={scale}, a={a:?}, b={b:?}"
                );
                assert!((b.height - a.height * scale).abs() <= 2.0 * scale.max(1.0));
                let hit = prepared.hit_test_caret(Point::new(b.x, b.y + b.height / 2.0));
                assert!((prepared.caret_geometry(hit).x - b.x).abs() < 0.1);
            }
            let base_selection = base.selection_rects(1..source.len() - 1);
            let scaled_selection = prepared.selection_rects(1..source.len() - 1);
            assert_eq!(base_selection.len(), scaled_selection.len());
            for (a, b) in base_selection.iter().zip(scaled_selection) {
                assert!((b.x - a.x * scale).abs() < 0.1);
                assert!((b.width - a.width * scale).abs() < 0.1);
            }
            let mut translated = text.clone();
            translated.bounds.x += 17.25;
            translated.bounds.y += 8.75;
            translated.clip = translated.bounds;
            let moved = engine.prepare(&translated, scale).unwrap();
            for (index, _) in source.grapheme_indices(true) {
                let a = prepared.caret_rect(index);
                let b = moved.caret_rect(index);
                assert!((b.x - a.x - 17.25 * scale).abs() < 0.01);
                assert!((b.y - a.y - 8.75 * scale).abs() < 0.01);
            }
        }
    }

    #[test]
    fn wrapped_bidi_lines_and_selection_are_consistent_with_their_baselines() {
        let mut engine = TextEngine::new();
        let mut text = run("one אבג two العربية three אבג four العربية five\nsix שבע");
        text.wrap = true;
        text.bounds.width = 160.0;
        text.bounds.height = 600.0;
        text.clip = text.bounds;
        let prepared = engine.prepare(&text, 1.5).unwrap();
        let lines = prepared.line_metrics();
        assert!(lines.len() >= 4);
        assert_eq!(lines.len(), prepared.line_count());
        for (i, line) in lines.iter().enumerate() {
            assert!(line.bounds.is_valid());
            assert!(line.baseline >= line.bounds.y);
            assert!(line.baseline <= line.bounds.y + line.bounds.height);
            assert!(line.ascent > 0.0 && line.descent >= 0.0);
            if i > 0 {
                assert!(line.baseline > lines[i - 1].baseline);
            }
        }
        let fragments = prepared.selection_rects(0..text.text.len());
        assert!(fragments.len() >= lines.len());
        for rect in fragments {
            assert!(rect.is_valid());
            assert!(
                lines
                    .iter()
                    .any(|line| (line.bounds.y - rect.y).abs() < 0.1)
            );
        }
        assert!(prepared.selection_rects(3..3).is_empty());
    }

    #[test]
    fn layout_budget_evicts_by_clusters_and_rejects_oversized_layout_before_caching() {
        let mut engine = TextEngine::new();
        let mut text = run(&"a".repeat(MAX_PREPARED_GLYPHS + 1));
        text.clip = Rect::new(-10.0, -10.0, 1.0, 1.0);
        assert!(matches!(
            engine.prepare(&text, 1.0),
            Err(TextError::RasterLimit)
        ));
        assert_eq!(engine.stats().cached_layouts, 0);
        assert_eq!(engine.stats().rasterizations, 0);
        for count in 0..6 {
            text.text = format!("{count}{}", "a".repeat(59_999));
            engine.prepare(&text, 1.0).unwrap();
            assert!(engine.stats().cached_layout_clusters <= MAX_LAYOUT_CLUSTERS);
            assert!(engine.stats().cached_layout_text_bytes <= MAX_LAYOUT_TEXT_BYTES);
        }
        assert_eq!(engine.stats().cached_layouts, 4);
        engine.clear_caches();
        assert_eq!(engine.stats().cached_layouts, 0);
        assert_eq!(engine.stats().cached_layout_clusters, 0);
        assert_eq!(engine.stats().cached_layout_text_bytes, 0);
    }

    #[test]
    fn invalid_or_out_of_range_metrics_fail_before_caching() {
        let mut engine = TextEngine::new();
        for scale in [0.0, -1.0, f32::INFINITY, f32::NAN, 100.0] {
            assert!(matches!(
                engine.prepare(&run("test"), scale),
                Err(TextError::InvalidScale)
            ));
        }
        let mut text = run("test");
        text.bounds.x = f32::MAX;
        assert!(engine.prepare(&text, 2.0).is_err());
        text = run("test");
        text.color.r = f32::NAN;
        assert!(matches!(
            engine.prepare(&text, 1.0),
            Err(TextError::InvalidRun)
        ));
        text = run(&"a".repeat(MAX_TEXT_BYTES + 1));
        assert!(matches!(
            engine.prepare(&text, 1.0),
            Err(TextError::TextTooLong)
        ));
        assert_eq!(engine.stats().cached_layouts, 0);
    }

    #[test]
    fn clipping_does_not_hide_font_coverage_and_raster_diagnostics() {
        let mut engine = TextEngine::new();
        let mut text = run("A👩‍💻");
        let visible = engine.prepare(&text, 1.5).unwrap();
        text.clip = Rect::new(-100.0, -100.0, 10.0, 10.0);
        let hidden = engine.prepare(&text, 1.5).unwrap();
        assert!(hidden.glyphs.is_empty());
        assert_eq!(hidden.shaped_glyphs, visible.shaped_glyphs);
        assert_eq!(hidden.missing_glyphs, visible.missing_glyphs);
        assert_eq!(hidden.rasterized_glyphs, visible.rasterized_glyphs);
        assert_eq!(hidden.color_glyphs, visible.color_glyphs);
        assert_eq!(hidden.font_runs(), visible.font_runs());
    }
}
