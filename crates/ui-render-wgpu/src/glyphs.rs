//! A bounded GPU atlas and instanced glyph renderer. CPU shaping is in ui-text.
use crate::GpuError;
use bytemuck::{Pod, Zeroable};
use rust_desktop_ui_core::{Rect, TextRun};
use rust_desktop_ui_text::{GlyphBitmap, GlyphKey, TextEngine};
use std::{
    collections::{HashMap, HashSet},
    hash::Hash,
    ops::Range,
};

const ATLAS_SIZE: u32 = 2048;
const MAX_GLYPHS: usize = 65_536;
const MAX_FRAME_RASTER_BYTES: usize = 64 * 1024 * 1024;
const SHADER: &str = r#"
struct Viewport { dimensions: vec2<f32>, unused: vec2<f32> }
@group(0) @binding(0) var<uniform> viewport: Viewport;
@group(1) @binding(0) var atlas: texture_2d<f32>;
@group(1) @binding(1) var atlas_sampler: sampler;
struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) tint: vec4<f32>,
};
@vertex fn vs_main(@builtin(vertex_index) index: u32,
    @location(0) rect: vec4<f32>, @location(1) uv: vec4<f32>,
    @location(2) tint: vec4<f32>) -> VertexOut {
    var corners = array<vec2<f32>, 6>(vec2(0.0,0.0),vec2(1.0,0.0),vec2(0.0,1.0),
        vec2(0.0,1.0),vec2(1.0,0.0),vec2(1.0,1.0));
    let c = corners[index];
    let p = rect.xy + c * rect.zw;
    var out: VertexOut;
    out.position = vec4(p.x / viewport.dimensions.x * 2.0 - 1.0,
        1.0 - p.y / viewport.dimensions.y * 2.0, 0.0, 1.0);
    out.uv = uv.xy + c * uv.zw;
    out.tint = tint;
    return out;
}
@fragment fn fs_main(input: VertexOut) -> @location(0) vec4<f32> {
    let color = textureSample(atlas, atlas_sampler, input.uv);
    return vec4(color.rgb * input.tint.rgb * input.tint.a, color.a * input.tint.a);
}
"#;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GlyphInstance {
    rect: [f32; 4],
    uv: [f32; 4],
    tint: [f32; 4],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Slot {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

#[derive(Default)]
struct Shelf {
    x: u32,
    y: u32,
    height: u32,
}
impl Shelf {
    fn allocate(&mut self, width: u32, height: u32, limit: u32) -> Option<Slot> {
        let padded_width = width.checked_add(2)?;
        let padded_height = height.checked_add(2)?;
        if padded_width > limit || padded_height > limit {
            return None;
        }
        let (x, y, shelf_height) = if self.x.checked_add(padded_width)? > limit {
            (0, self.y.checked_add(self.height)?, 0)
        } else {
            (self.x, self.y, self.height)
        };
        if y.checked_add(padded_height)? > limit {
            return None;
        }
        let slot = Slot {
            x: x + 1,
            y: y + 1,
            width,
            height,
        };
        self.x = x + padded_width;
        self.y = y;
        self.height = shelf_height.max(padded_height);
        Some(slot)
    }
}

struct AtlasCache<K> {
    slots: HashMap<K, Slot>,
    shelf: Shelf,
}

impl<K> Default for AtlasCache<K> {
    fn default() -> Self {
        Self {
            slots: HashMap::new(),
            shelf: Shelf::default(),
        }
    }
}

impl<K: Eq + Hash + Clone> AtlasCache<K> {
    fn prepare<'a>(
        &mut self,
        glyphs: impl Iterator<Item = (&'a K, &'a GlyphBitmap)> + Clone,
        limit: u32,
        mut upload: impl FnMut(&GlyphBitmap, Slot) -> Result<(), GpuError>,
    ) -> Result<(), GpuError>
    where
        K: 'a,
    {
        // A single reset is allowed before this frame's UVs are constructed.
        // No eviction may happen after instances referring to slots exist.
        for _ in 0..2 {
            let mut full = false;
            for (key, bitmap) in glyphs.clone() {
                if bitmap.width == 0 || bitmap.height == 0 || self.slots.contains_key(key) {
                    continue;
                }
                let Some(slot) = self.shelf.allocate(bitmap.width, bitmap.height, limit) else {
                    full = true;
                    break;
                };
                upload(bitmap, slot)?;
                self.slots.insert(key.clone(), slot);
            }
            if !full {
                return Ok(());
            }
            self.slots.clear();
            self.shelf = Shelf::default();
        }
        Err(GpuError::Text(
            "visible glyph set exceeds the 2048x2048 atlas".into(),
        ))
    }
}

fn clipped_instance(
    bounds: Rect,
    clip: Rect,
    viewport: Rect,
    slot: Slot,
    tint: [f32; 4],
) -> Option<GlyphInstance> {
    let visible = bounds.intersection(clip)?.intersection(viewport)?;
    let size = ATLAS_SIZE as f32;
    Some(GlyphInstance {
        rect: [visible.x, visible.y, visible.width, visible.height],
        uv: [
            (slot.x as f32 + visible.x - bounds.x) / size,
            (slot.y as f32 + visible.y - bounds.y) / size,
            visible.width / size,
            visible.height / size,
        ],
        tint,
    })
}

// Premultiply before linear filtering, including transparent atlas padding.
// This avoids dark fringes and squared coverage at fractional glyph edges.
fn padded_pixels(bitmap: &GlyphBitmap, slot: Slot) -> Result<Vec<u8>, GpuError> {
    if slot.width > ATLAS_SIZE - 2
        || slot.height > ATLAS_SIZE - 2
        || bitmap.width != slot.width
        || bitmap.height != slot.height
        || bitmap.rgba.len() != slot.width as usize * slot.height as usize * 4
    {
        return Err(GpuError::Text("invalid RGBA glyph bitmap".into()));
    }
    let width = slot.width + 2;
    let height = slot.height + 2;
    let mut pixels = vec![0; (width * height * 4) as usize];
    for row in 0..slot.height as usize {
        let destination = (row + 1) * width as usize * 4 + 4;
        let source = row * slot.width as usize * 4;
        for (input, output) in bitmap.rgba[source..source + slot.width as usize * 4]
            .chunks_exact(4)
            .zip(pixels[destination..destination + slot.width as usize * 4].chunks_exact_mut(4))
        {
            let alpha = u16::from(input[3]);
            for channel in 0..3 {
                output[channel] = ((u16::from(input[channel]) * alpha + 127) / 255) as u8;
            }
            output[3] = input[3];
        }
    }
    Ok(pixels)
}

fn upload_bitmap(
    atlas: &wgpu::Texture,
    bitmap: &GlyphBitmap,
    slot: Slot,
    queue: &wgpu::Queue,
) -> Result<(), GpuError> {
    let pixels = padded_pixels(bitmap, slot)?;
    let width = slot.width + 2;
    let height = slot.height + 2;
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: atlas,
            mip_level: 0,
            origin: wgpu::Origin3d {
                x: slot.x - 1,
                y: slot.y - 1,
                z: 0,
            },
            aspect: wgpu::TextureAspect::All,
        },
        &pixels,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(width * 4),
            rows_per_image: Some(height),
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    Ok(())
}

pub(crate) struct GlyphRenderer {
    engine: TextEngine,
    atlas: wgpu::Texture,
    bind_group: wgpu::BindGroup,
    pipeline: wgpu::RenderPipeline,
    buffer: wgpu::Buffer,
    cache: AtlasCache<GlyphKey>,
    instances: Vec<GlyphInstance>,
    ranges: Vec<Range<u32>>,
    pub missing_glyphs: usize,
}

impl GlyphRenderer {
    pub fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        viewport_layout: &wgpu::BindGroupLayout,
    ) -> Self {
        let atlas = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("ui-glyph-atlas"),
            size: wgpu::Extent3d {
                width: ATLAS_SIZE,
                height: ATLAS_SIZE,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = atlas.create_view(&Default::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("ui-glyph-sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ui-glyph-layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ui-glyph-bind-group"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ui-text-pipeline-layout"),
            bind_group_layouts: &[Some(viewport_layout), Some(&layout)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ui-text-wgsl"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let attributes = wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4, 2 => Float32x4];
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("ui-text-pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<GlyphInstance>() as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &attributes,
                })],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ui-glyph-instances"),
            size: (MAX_GLYPHS * std::mem::size_of::<GlyphInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            engine: TextEngine::new(),
            atlas,
            bind_group,
            pipeline,
            buffer,
            cache: AtlasCache::default(),
            instances: Vec::new(),
            ranges: Vec::new(),
            missing_glyphs: 0,
        }
    }

    pub fn prepare(
        &mut self,
        texts: &[TextRun],
        scale: f32,
        viewport: Rect,
        queue: &wgpu::Queue,
    ) -> Result<(), GpuError> {
        self.instances.clear();
        self.ranges.clear();
        self.missing_glyphs = 0;
        let mut runs = Vec::with_capacity(texts.len());
        let mut glyph_count = 0;
        let mut raster_allocations = HashSet::new();
        let mut raster_bytes = 0usize;
        for text in texts {
            let run = self
                .engine
                .prepare(text, scale)
                .map_err(|error| GpuError::Text(error.to_string()))?;
            glyph_count += run.glyphs.len();
            if glyph_count > MAX_GLYPHS {
                return Err(GpuError::Text("visible glyph count exceeds 65536".into()));
            }
            for glyph in &run.glyphs {
                // Equal glyph keys can own different allocations after the
                // CPU cache evicts and rasterizes them again in a later run.
                if raster_allocations.insert(std::sync::Arc::as_ptr(&glyph.bitmap) as usize) {
                    raster_bytes = raster_bytes
                        .checked_add(glyph.bitmap.rgba.len())
                        .filter(|bytes| *bytes <= MAX_FRAME_RASTER_BYTES)
                        .ok_or_else(|| {
                            GpuError::Text("frame glyph rasters exceed 64 MiB".into())
                        })?;
                }
            }
            self.missing_glyphs += run.missing_glyphs;
            runs.push(run);
        }
        self.cache.prepare(
            runs.iter()
                .flat_map(|run| &run.glyphs)
                .map(|glyph| (&glyph.key, glyph.bitmap.as_ref())),
            ATLAS_SIZE,
            |bitmap, slot| upload_bitmap(&self.atlas, bitmap, slot, queue),
        )?;
        for run in &runs {
            let start = self.instances.len() as u32;
            for glyph in &run.glyphs {
                let Some(slot) = self.cache.slots.get(&glyph.key).copied() else {
                    continue;
                };
                let bounds = Rect::new(
                    glyph.position.x,
                    glyph.position.y,
                    slot.width as f32,
                    slot.height as f32,
                );
                if let Some(instance) = clipped_instance(
                    bounds,
                    glyph.clip,
                    viewport,
                    slot,
                    [glyph.tint.r, glyph.tint.g, glyph.tint.b, glyph.tint.a],
                ) {
                    self.instances.push(instance);
                }
            }
            self.ranges.push(start..self.instances.len() as u32);
        }
        if !self.instances.is_empty() {
            queue.write_buffer(&self.buffer, 0, bytemuck::cast_slice(&self.instances));
        }
        Ok(())
    }

    pub fn range(&self, text: usize) -> Option<Range<u32>> {
        self.ranges.get(text).cloned()
    }
    pub fn len(&self) -> usize {
        self.instances.len()
    }
    pub fn cached(&self) -> usize {
        self.cache.slots.len()
    }
    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>, range: Range<u32>) {
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(1, &self.bind_group, &[]);
        pass.set_vertex_buffer(0, self.buffer.slice(..));
        pass.draw(0..6, range);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clipping_crops_geometry_and_uvs_by_the_same_physical_distance() {
        let slot = Slot {
            x: 101,
            y: 201,
            width: 20,
            height: 16,
        };
        let bounds = Rect::new(10.0, 12.0, 20.0, 16.0);
        let tint = [0.2, 0.4, 0.6, 0.5];
        let instance = clipped_instance(
            bounds,
            Rect::new(15.0, 10.0, 100.0, 100.0),
            Rect::new(0.0, 0.0, 25.0, 24.0),
            slot,
            tint,
        )
        .unwrap();
        assert_eq!(instance.rect, [15.0, 12.0, 10.0, 12.0]);
        assert_eq!(
            instance.uv.map(|value| value * ATLAS_SIZE as f32),
            [106.0, 201.0, 10.0, 12.0]
        );
        assert_eq!(instance.tint, tint);
        assert!(
            clipped_instance(bounds, Rect::new(0.0, 0.0, 5.0, 5.0), bounds, slot, tint).is_none()
        );
    }

    #[test]
    fn atlas_pixels_filter_premultiplied_coverage_without_dark_fringes() {
        let slot = Slot {
            x: 1,
            y: 1,
            width: 2,
            height: 1,
        };
        let bitmap = GlyphBitmap {
            width: 2,
            height: 1,
            rgba: vec![255, 128, 64, 128, 255, 255, 255, 0],
        };
        let pixels = padded_pixels(&bitmap, slot).unwrap();
        assert_eq!(pixels.len(), 4 * 3 * 4);
        assert!(pixels[..20].iter().all(|byte| *byte == 0));
        assert_eq!(pixels[20..24], [128, 64, 32, 128]);
        assert!(pixels[24..].iter().all(|byte| *byte == 0));
        // Half a texel between the colored edge and transparent padding:
        // filtering halves RGB and alpha together, then premultiplied blending
        // adds RGB directly. Applying source alpha again would darken it.
        let sampled_red = f32::from(pixels[20]) / 255.0 * 0.5;
        let sampled_alpha = f32::from(pixels[23]) / 255.0 * 0.5;
        let opacity = 0.4;
        let on_white = sampled_red * opacity + (1.0 - sampled_alpha * opacity);
        assert!((on_white - 1.0).abs() < 1e-6);
        assert!(
            padded_pixels(
                &GlyphBitmap {
                    width: 2,
                    height: 1,
                    rgba: vec![0; 7]
                },
                slot
            )
            .is_err()
        );
    }

    #[test]
    fn atlas_reset_reuploads_all_live_keys_before_uvs_and_bounds_storage() {
        let mut cache = AtlasCache::<u32>::default();
        let mut uploaded = HashMap::new();
        for frame in 0..100 {
            let items = (frame..frame + 4)
                .map(|key| {
                    (
                        key,
                        GlyphBitmap {
                            width: 2,
                            height: 2,
                            rgba: vec![key as u8; 16],
                        },
                    )
                })
                .collect::<Vec<_>>();
            cache
                .prepare(
                    items.iter().map(|(key, image)| (key, image)),
                    8,
                    |image, slot| {
                        uploaded.insert((slot.x, slot.y), image.rgba[0]);
                        Ok(())
                    },
                )
                .unwrap();
            assert_eq!(cache.slots.len(), 4);
            for (key, _) in &items {
                let slot = cache.slots[key];
                assert_eq!(uploaded[&(slot.x, slot.y)], *key as u8);
                assert!(slot.x + slot.width < 8 && slot.y + slot.height < 8);
            }
        }
        let too_large = GlyphBitmap {
            width: 7,
            height: 1,
            rgba: vec![255; 28],
        };
        assert!(
            cache
                .prepare(std::iter::once((&1000, &too_large)), 8, |_, _| Ok(()))
                .is_err()
        );
        assert!(cache.slots.is_empty());
        let valid = GlyphBitmap {
            width: 2,
            height: 2,
            rgba: vec![255; 16],
        };
        cache
            .prepare(std::iter::once((&1001, &valid)), 8, |_, _| Ok(()))
            .unwrap();
        assert_eq!(
            cache.slots[&1001],
            Slot {
                x: 1,
                y: 1,
                width: 2,
                height: 2
            }
        );
    }
    #[test]
    fn atlas_packing_preserves_padding_and_rejects_oversized_glyphs() {
        let mut shelf = Shelf::default();
        let first = shelf.allocate(4, 3, 12).unwrap();
        let second = shelf.allocate(4, 3, 12).unwrap();
        let third = shelf.allocate(4, 3, 12).unwrap();
        assert_eq!((first.x, first.y), (1, 1));
        assert_eq!((second.x, second.y), (7, 1));
        assert_eq!((third.x, third.y), (1, 6));
        shelf.allocate(4, 3, 12).unwrap();
        assert!(shelf.allocate(4, 3, 12).is_none());
        assert!(Shelf::default().allocate(u32::MAX, 1, 12).is_none());
    }
}
