//! Bounded immutable bitmap uploads and painter-ordered image sampling.
use crate::{GpuError, MAX_RECTANGLES, linear_to_srgb, physical_rect, srgb_to_linear};
use bytemuck::{Pod, Zeroable};
use rust_desktop_ui_core::{Bitmap, ImagePrimitive, ImageSampling, Rect};
use std::collections::{HashMap, HashSet};

const MAX_IMAGE_BYTES: usize = 128 * 1024 * 1024;
const MAX_IMAGE_RESOURCES: usize = 4096;
const SHADER: &str = r#"
struct Viewport { dimensions: vec2<f32>, unused: vec2<f32> }
@group(0) @binding(0) var<uniform> viewport: Viewport;
@group(1) @binding(0) var bitmap: texture_2d<f32>;
@group(1) @binding(1) var bitmap_sampler: sampler;
struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) @interpolate(flat) uv_clip: vec4<f32>,
    @location(2) @interpolate(flat) opacity: f32,
};
@vertex fn vs_main(@builtin(vertex_index) index: u32,
    @location(0) rect: vec4<f32>, @location(1) uv: vec4<f32>,
    @location(2) uv_clip: vec4<f32>, @location(3) opacity: f32) -> VertexOut {
    var corners = array<vec2<f32>, 6>(vec2(0.0,0.0),vec2(1.0,0.0),vec2(0.0,1.0),
        vec2(0.0,1.0),vec2(1.0,0.0),vec2(1.0,1.0));
    let c = corners[index]; let p = rect.xy + c * rect.zw;
    var out: VertexOut;
    out.position = vec4(p.x / viewport.dimensions.x * 2.0 - 1.0,
        1.0 - p.y / viewport.dimensions.y * 2.0, 0.0, 1.0);
    out.uv = uv.xy + c * uv.zw; out.uv_clip = uv_clip; out.opacity = opacity;
    return out;
}
@fragment fn fs_main(input: VertexOut) -> @location(0) vec4<f32> {
    return textureSample(bitmap, bitmap_sampler, clamp(input.uv, input.uv_clip.xy, input.uv_clip.zw)) * input.opacity;
}
"#;

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct ImageInstance {
    rect: [f32; 4],
    uv: [f32; 4],
    uv_clip: [f32; 4],
    opacity: f32,
}

fn image_instance(item: &ImagePrimitive, scale: f32, viewport: Rect) -> Option<ImageInstance> {
    let bounds = physical_rect(item.bounds, scale)?;
    let visible = bounds
        .intersection(physical_rect(item.clip, scale)?)?
        .intersection(viewport)?;
    let w = item.bitmap.width() as f32;
    let h = item.bitmap.height() as f32;
    let s = item.source;
    let half_x = 0.5_f32.min(s.width * 0.5);
    let half_y = 0.5_f32.min(s.height * 0.5);
    // Divide before multiplying, so very small destinations do not overflow.
    Some(ImageInstance {
        rect: [visible.x, visible.y, visible.width, visible.height],
        uv: [
            (s.x + (visible.x - bounds.x) / bounds.width * s.width) / w,
            (s.y + (visible.y - bounds.y) / bounds.height * s.height) / h,
            visible.width / bounds.width * s.width / w,
            visible.height / bounds.height * s.height / h,
        ],
        uv_clip: [
            (s.x + half_x) / w,
            (s.y + half_y) / h,
            (s.x + s.width - half_x) / w,
            (s.y + s.height - half_y) / h,
        ],
        opacity: item.opacity,
    })
}

fn premultiplied_pixels(bitmap: &Bitmap) -> Vec<u8> {
    let mut bytes = bitmap.pixels().to_vec();
    for pixel in bytes.chunks_exact_mut(4) {
        let alpha = f32::from(pixel[3]) / 255.0;
        for channel in &mut pixel[..3] {
            *channel = (linear_to_srgb(srgb_to_linear(f32::from(*channel) / 255.0) * alpha) * 255.0)
                .round() as u8;
        }
    }
    bytes
}

fn visible_resources(
    items: &[ImagePrimitive],
    scale: f32,
    viewport: Rect,
    dimension_limit: u32,
) -> Result<HashSet<u64>, GpuError> {
    let mut ids = HashSet::new();
    let mut bytes = 0usize;
    for item in items {
        if image_instance(item, scale, viewport).is_none() || !ids.insert(item.bitmap.id()) {
            continue;
        }
        if item.bitmap.width() > dimension_limit || item.bitmap.height() > dimension_limit {
            return Err(GpuError::Image(
                "bitmap exceeds GPU texture dimension limit".into(),
            ));
        }
        bytes = bytes
            .checked_add(item.bitmap.pixels().len())
            .ok_or(GpuError::InvalidScene)?;
        if bytes > MAX_IMAGE_BYTES || ids.len() > MAX_IMAGE_RESOURCES {
            return Err(GpuError::Image(
                "visible bitmap set exceeds 128MiB or 4096 resources".into(),
            ));
        }
    }
    Ok(ids)
}

struct GpuBitmap {
    nearest: wgpu::BindGroup,
    linear: wgpu::BindGroup,
}
pub(crate) struct ImageRenderer {
    pipeline: wgpu::RenderPipeline,
    texture_layout: wgpu::BindGroupLayout,
    nearest: wgpu::Sampler,
    linear: wgpu::Sampler,
    buffer: wgpu::Buffer,
    resources: HashMap<u64, GpuBitmap>,
    placements: Vec<Option<(u64, ImageSampling)>>,
    pub(crate) uploaded: usize,
}
impl ImageRenderer {
    pub(crate) fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        viewport_layout: &wgpu::BindGroupLayout,
    ) -> Self {
        let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ui-image-texture-layout"),
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
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ui-images"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ui-image-pipeline-layout"),
            bind_group_layouts: &[Some(viewport_layout), Some(&texture_layout)],
            immediate_size: 0,
        });
        const ATTRIBUTES: [wgpu::VertexAttribute; 4] =
            wgpu::vertex_attr_array![0=>Float32x4,1=>Float32x4,2=>Float32x4,3=>Float32];
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("ui-image-pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<ImageInstance>() as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &ATTRIBUTES,
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
        let sampler = |filter| {
            device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("ui-image-sampler"),
                mag_filter: filter,
                min_filter: filter,
                ..Default::default()
            })
        };
        Self {
            pipeline,
            texture_layout,
            nearest: sampler(wgpu::FilterMode::Nearest),
            linear: sampler(wgpu::FilterMode::Linear),
            buffer: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("ui-image-instances"),
                size: (MAX_RECTANGLES * std::mem::size_of::<ImageInstance>()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            resources: HashMap::new(),
            placements: Vec::new(),
            uploaded: 0,
        }
    }
    pub(crate) fn prepare(
        &mut self,
        items: &[ImagePrimitive],
        scale: f32,
        viewport: Rect,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Result<(), GpuError> {
        let active = visible_resources(
            items,
            scale,
            viewport,
            device.limits().max_texture_dimension_2d,
        )?;
        // Release absent textures before allocations. A frame never evicts any
        // texture referenced by that frame; hidden resources consume no cache.
        self.resources.retain(|id, _| active.contains(id));
        self.uploaded = 0;
        self.placements.clear();
        let mut instances = Vec::with_capacity(items.len());
        for item in items {
            let Some(instance) = image_instance(item, scale, viewport) else {
                instances.push(ImageInstance::zeroed());
                self.placements.push(None);
                continue;
            };
            let id = item.bitmap.id();
            if let std::collections::hash_map::Entry::Vacant(entry) = self.resources.entry(id) {
                let bitmap = &item.bitmap;
                let size = wgpu::Extent3d {
                    width: bitmap.width(),
                    height: bitmap.height(),
                    depth_or_array_layers: 1,
                };
                let texture = device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("ui-bitmap"),
                    size,
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Rgba8UnormSrgb,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                    view_formats: &[],
                });
                queue.write_texture(
                    texture.as_image_copy(),
                    &premultiplied_pixels(bitmap),
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(bitmap.width() * 4),
                        rows_per_image: Some(bitmap.height()),
                    },
                    size,
                );
                let view = texture.create_view(&Default::default());
                let group = |sampler: &wgpu::Sampler| {
                    device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("ui-bitmap-bind-group"),
                        layout: &self.texture_layout,
                        entries: &[
                            wgpu::BindGroupEntry {
                                binding: 0,
                                resource: wgpu::BindingResource::TextureView(&view),
                            },
                            wgpu::BindGroupEntry {
                                binding: 1,
                                resource: wgpu::BindingResource::Sampler(sampler),
                            },
                        ],
                    })
                };
                entry.insert(GpuBitmap {
                    nearest: group(&self.nearest),
                    linear: group(&self.linear),
                });
                self.uploaded += 1;
            }
            self.placements.push(Some((id, item.sampling)));
            instances.push(instance);
        }
        if !instances.is_empty() {
            queue.write_buffer(&self.buffer, 0, bytemuck::cast_slice(&instances));
        }
        Ok(())
    }
    pub(crate) fn draw(&self, pass: &mut wgpu::RenderPass<'_>, index: u32) -> bool {
        let Some((id, sampling)) = self.placements[index as usize] else {
            return false;
        };
        let texture = &self.resources[&id];
        pass.set_pipeline(&self.pipeline);
        pass.set_vertex_buffer(0, self.buffer.slice(..));
        pass.set_bind_group(
            1,
            match sampling {
                ImageSampling::Nearest => &texture.nearest,
                ImageSampling::Linear => &texture.linear,
            },
            &[],
        );
        pass.draw(0..6, index..index + 1);
        true
    }
    pub(crate) fn cached(&self) -> usize {
        self.resources.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn clipping_preserves_source_mapping_and_dpi_does_not_scale_source() {
        let bitmap = Bitmap::new_rgba8(4, 2, vec![255; 32]).unwrap();
        let mut item = ImagePrimitive::new(bitmap, Rect::new(10.0, 20.0, 40.0, 20.0));
        item.source = Rect::new(1.0, 0.0, 2.0, 2.0);
        item.clip = Rect::new(20.0, 20.0, 30.0, 20.0);
        let instance = image_instance(&item, 2.0, Rect::new(0.0, 0.0, 200.0, 200.0)).unwrap();
        assert_eq!(instance.rect, [40.0, 40.0, 60.0, 40.0]);
        assert_eq!(instance.uv, [0.375, 0.0, 0.375, 1.0]);
        assert_eq!(instance.uv_clip, [0.375, 0.25, 0.625, 0.75]);
    }
    #[test]
    fn upload_premultiplies_linear_rgb_and_discards_transparent_color() {
        let b = Bitmap::new_rgba8(2, 1, vec![255, 0, 0, 128, 0, 255, 0, 0]).unwrap();
        assert_eq!(premultiplied_pixels(&b), vec![188, 0, 0, 128, 0, 0, 0, 0]);
    }
    #[test]
    fn resource_budget_deduplicates_ids_and_ignores_hidden_images() {
        let b = Bitmap::new_rgba8(1, 1, vec![0; 4]).unwrap();
        let item = ImagePrimitive::new(b, Rect::new(0.0, 0.0, 10.0, 10.0));
        let viewport = Rect::new(0.0, 0.0, 20.0, 20.0);
        assert_eq!(
            visible_resources(&vec![item.clone(); 5000], 1.0, viewport, 8192)
                .unwrap()
                .len(),
            1
        );
        assert!(visible_resources(std::slice::from_ref(&item), 1.0, viewport, 0).is_err());
        assert!(
            visible_resources(&[item], 1.0, Rect::new(100.0, 100.0, 5.0, 5.0), 0)
                .unwrap()
                .is_empty()
        );
        let distinct: Vec<_> = (0..MAX_IMAGE_RESOURCES + 1)
            .map(|_| ImagePrimitive::new(Bitmap::new_rgba8(1, 1, vec![0; 4]).unwrap(), viewport))
            .collect();
        assert!(visible_resources(&distinct, 1.0, viewport, 8192).is_err());
    }
}
