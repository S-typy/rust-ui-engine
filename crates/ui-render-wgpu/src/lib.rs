//! Experimental GPU backend. No external GUI framework is used.
//! UI state and scene are provided by ui-core; this module only renders rectangles.

use std::sync::Arc;
use bytemuck::{Pod, Zeroable};
use rust_desktop_ui_core::Scene;
use wgpu::util::DeviceExt;
use winit::{dpi::PhysicalSize, window::Window};

// GPU shaders are WGSL, as required by wgpu/WebGPU. All host/UI code is Rust.
const RECT_SHADER: &str = r#"
struct Viewport { dimensions: vec2<f32>, unused: vec2<f32> }
@group(0) @binding(0) var<uniform> viewport: Viewport;

struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@vertex
fn vs_main(
    @builtin(vertex_index) index: u32,
    @location(0) rect: vec4<f32>,
    @location(1) color: vec4<f32>,
) -> VertexOut {
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 0.0), vec2<f32>(0.0, 1.0),
        vec2<f32>(0.0, 1.0), vec2<f32>(1.0, 0.0), vec2<f32>(1.0, 1.0),
    );
    let pixel = rect.xy + corners[index] * rect.zw;
    var out: VertexOut;
    out.position = vec4<f32>(
        pixel.x / viewport.dimensions.x * 2.0 - 1.0,
        1.0 - pixel.y / viewport.dimensions.y * 2.0,
        0.0,
        1.0,
    );
    out.color = color;
    return out;
}

@fragment
fn fs_main(input: VertexOut) -> @location(0) vec4<f32> {
    return input.color;
}
"#;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct RectInstance {
    position_size: [f32; 4],
    rgba: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct ViewportUniform {
    dimensions: [f32; 2],
    padding: [f32; 2],
}

const MAX_RECTANGLES: usize = 16_384;

pub struct GpuRenderer {
    _window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    viewport_buffer: wgpu::Buffer,
    viewport_bind_group: wgpu::BindGroup,
    rectangles_buffer: wgpu::Buffer,
}

impl GpuRenderer {
    pub async fn new(window: Arc<Window>) -> Result<Self, String> {
        let physical = window.inner_size();
        let instance = wgpu::Instance::default();
        let surface = instance.create_surface(window.clone()).map_err(|e| e.to_string())?;
        let adapter = instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        }).await.map_err(|e| e.to_string())?;
        let gpu_info = adapter.get_info();
        eprintln!("Using GPU: {} ({:?}, {:?})", gpu_info.name, gpu_info.device_type, gpu_info.backend);
        if gpu_info.device_type == wgpu::DeviceType::Cpu {
            return Err("Only software GPU adapter was found; a hardware GPU is required".into());
        }
        let (device, queue) = adapter.request_device(&wgpu::DeviceDescriptor::default())
            .await.map_err(|e| e.to_string())?;
        let mut config = surface.get_default_config(&adapter, physical.width.max(1), physical.height.max(1))
            .ok_or_else(|| "GPU cannot render to this window".to_string())?;
        config.usage = wgpu::TextureUsages::RENDER_ATTACHMENT;
        surface.configure(&device, &config);
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ui-rectangles"),
            source: wgpu::ShaderSource::Wgsl(RECT_SHADER.into()),
        });
        let viewport_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("ui-viewport"),
            contents: bytemuck::bytes_of(&ViewportUniform {
                dimensions: [config.width as f32, config.height as f32],
                padding: [0.0; 2],
            }),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let viewport_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ui-viewport-layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let viewport_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ui-viewport-bind-group"),
            layout: &viewport_layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: viewport_buffer.as_entire_binding() }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ui-render-layout"),
            bind_group_layouts: &[Some(&viewport_layout)],
            immediate_size: 0,
        });
        const RECT_ATTRIBUTES: [wgpu::VertexAttribute; 2] = [
            wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x4, offset: 0, shader_location: 0 },
            wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x4, offset: 16, shader_location: 1 },
        ];
        let rect_vertex_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<RectInstance>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &RECT_ATTRIBUTES,
        };
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("ui-rectangles-pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(rect_vertex_layout)],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        let rectangles_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ui-batched-rectangles"),
            size: (MAX_RECTANGLES * std::mem::size_of::<RectInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Ok(Self { _window: window, surface, device, queue, config,
            pipeline, viewport_buffer, viewport_bind_group, rectangles_buffer })
    }

    pub fn resize(&mut self, size: PhysicalSize<u32>) {
        if size.width == 0 || size.height == 0 { return; }
        self.config.width = size.width;
        self.config.height = size.height;
        self.surface.configure(&self.device, &self.config);
    }

    pub fn render(&mut self, scene: &Scene, scale_factor: f32) -> Result<usize, String> {
        if scene.rectangles.len() > MAX_RECTANGLES {
            return Err(format!("Scene exceeds current batch capacity {MAX_RECTANGLES}"));
        }
        let instances: Vec<RectInstance> = scene.rectangles.iter().map(|r| RectInstance {
            position_size: [
                r.bounds.x * scale_factor, r.bounds.y * scale_factor,
                r.bounds.width * scale_factor, r.bounds.height * scale_factor,
            ],
            rgba: [r.color.r, r.color.g, r.color.b, r.color.a],
        }).collect();
        let dimensions = ViewportUniform {
            dimensions: [self.config.width as f32, self.config.height as f32],
            padding: [0.0; 2],
        };
        self.queue.write_buffer(&self.viewport_buffer, 0, bytemuck::bytes_of(&dimensions));
        if !instances.is_empty() {
            self.queue.write_buffer(&self.rectangles_buffer, 0, bytemuck::cast_slice(&instances));
        }
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame) | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => return Ok(0),
            wgpu::CurrentSurfaceTexture::Outdated => {
                self.surface.configure(&self.device, &self.config);
                return Ok(0);
            }
            wgpu::CurrentSurfaceTexture::Lost => return Err("GPU surface lost; reinitialize GPU".into()),
            wgpu::CurrentSurfaceTexture::Validation => return Err("GPU surface validation error".into()),
        };
        let view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("ui-frame") });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("ui-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::WHITE),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.viewport_bind_group, &[]);
            pass.set_vertex_buffer(0, self.rectangles_buffer.slice(..));
            pass.draw(0..6, 0..instances.len() as u32);
        }
        self.queue.submit([encoder.finish()]);
        self.queue.present(frame);
        Ok(instances.len())
    }
}
