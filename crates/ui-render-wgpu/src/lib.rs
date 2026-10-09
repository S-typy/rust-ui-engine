//! Experimental GPU backend. No external GUI framework is used.
//! Scenes are provided by ui-core; application state stays in the host.

use bytemuck::{Pod, Zeroable};
use rust_desktop_ui_core::Scene;
use std::{
    fmt,
    sync::{Arc, Mutex},
};
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

/// A frame is counted as presented only after submitting and presenting it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RenderOutcome {
    Presented {
        rectangles: usize,
    },
    Skipped(SkipReason),
    /// Drop this renderer and create a new one for the same window.
    RecoveryRequired(RecoveryReason),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SkipReason {
    ZeroSized,
    Timeout,
    Occluded,
    Reconfigured,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecoveryReason {
    SurfaceLost,
    DeviceLost,
}

/// Errors which the host must report instead of panicking or using software rendering.
#[derive(Debug)]
pub enum GpuError {
    CreateSurface(String),
    RequestAdapter(String),
    SoftwareAdapter(String),
    RequestDevice(String),
    UnsupportedSurface,
    SurfaceTooLarge { size: PhysicalSize<u32>, limit: u32 },
    InvalidScaleFactor(f32),
    SceneTooLarge { rectangles: usize, capacity: usize },
    DeviceLost(String),
    Backend(String),
    SurfaceValidation,
}

impl fmt::Display for GpuError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CreateSurface(message) => write!(f, "Cannot create GPU surface: {message}"),
            Self::RequestAdapter(message) => write!(f, "No compatible GPU adapter: {message}"),
            Self::SoftwareAdapter(name) => write!(
                f,
                "Adapter '{name}' uses software rendering; a hardware GPU is required"
            ),
            Self::RequestDevice(message) => write!(f, "Cannot create GPU device: {message}"),
            Self::UnsupportedSurface => f.write_str("GPU cannot render to this window"),
            Self::SurfaceTooLarge { size, limit } => write!(
                f,
                "Window size {}x{} exceeds the GPU texture dimension limit {limit}",
                size.width, size.height
            ),
            Self::InvalidScaleFactor(scale) => write!(
                f,
                "DPI scale factor must be finite and positive, got {scale}"
            ),
            Self::SceneTooLarge {
                rectangles,
                capacity,
            } => write!(
                f,
                "Scene has {rectangles} rectangles; current batch capacity is {capacity}"
            ),
            Self::DeviceLost(message) => write!(f, "GPU device lost: {message}"),
            Self::Backend(message) => write!(f, "GPU backend error: {message}"),
            Self::SurfaceValidation => f.write_str("GPU surface validation failed"),
        }
    }
}

impl std::error::Error for GpuError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Preparation {
    ZeroSized,
    Configure,
    Acquire,
    Recover(RecoveryReason),
}

/// Only records transitions; GPU calls stay in `GpuRenderer`.
struct SurfaceState {
    size: PhysicalSize<u32>,
    needs_configure: bool,
    recovery: Option<RecoveryReason>,
}

impl SurfaceState {
    fn new(size: PhysicalSize<u32>) -> Self {
        Self {
            size,
            needs_configure: true,
            recovery: None,
        }
    }

    fn resize(&mut self, size: PhysicalSize<u32>, limit: u32) -> Result<(), GpuError> {
        if size.width > limit || size.height > limit {
            return Err(GpuError::SurfaceTooLarge { size, limit });
        }
        if self.size != size {
            self.size = size;
            self.needs_configure = true;
        }
        Ok(())
    }

    fn preparation(&self) -> Preparation {
        if let Some(reason) = self.recovery {
            Preparation::Recover(reason)
        } else if self.size.width == 0 || self.size.height == 0 {
            Preparation::ZeroSized
        } else if self.needs_configure {
            Preparation::Configure
        } else {
            Preparation::Acquire
        }
    }
}

#[derive(Default)]
struct DeviceHealth {
    lost: Option<String>,
    error: Option<String>,
}

fn check_frame_inputs(rectangles: usize, scale_factor: f32) -> Result<(), GpuError> {
    if !scale_factor.is_finite() || scale_factor <= 0.0 {
        return Err(GpuError::InvalidScaleFactor(scale_factor));
    }
    if rectangles > MAX_RECTANGLES {
        return Err(GpuError::SceneTooLarge {
            rectangles,
            capacity: MAX_RECTANGLES,
        });
    }
    Ok(())
}

fn check_adapter(
    device_type: wgpu::DeviceType,
    backend: wgpu::Backend,
    name: &str,
) -> Result<(), GpuError> {
    if device_type == wgpu::DeviceType::Cpu || backend == wgpu::Backend::Noop {
        return Err(GpuError::SoftwareAdapter(name.to_owned()));
    }
    Ok(())
}

pub struct GpuRenderer {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    viewport_buffer: wgpu::Buffer,
    viewport_bind_group: wgpu::BindGroup,
    rectangles_buffer: wgpu::Buffer,
    adapter_info: wgpu::AdapterInfo,
    state: SurfaceState,
    health: Arc<Mutex<DeviceHealth>>,
}

impl GpuRenderer {
    pub async fn new(window: Arc<Window>) -> Result<Self, GpuError> {
        let physical = window.inner_size();
        let descriptor = wgpu::InstanceDescriptor::new_without_display_handle_from_env();
        if !descriptor
            .backends
            .intersects(wgpu::Instance::enabled_backend_features())
        {
            return Err(GpuError::RequestAdapter(format!(
                "Selected backends {:?} are unavailable in this build/platform",
                descriptor.backends
            )));
        }
        let instance = wgpu::Instance::new(descriptor);
        let surface = instance
            .create_surface(window.clone())
            .map_err(|e| GpuError::CreateSurface(e.to_string()))?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
                ..Default::default()
            })
            .await
            .map_err(|e| GpuError::RequestAdapter(e.to_string()))?;
        let gpu_info = adapter.get_info();
        check_adapter(gpu_info.device_type, gpu_info.backend, &gpu_info.name)?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default())
            .await
            .map_err(|e| GpuError::RequestDevice(e.to_string()))?;
        let health = Arc::new(Mutex::new(DeviceHealth::default()));
        let error_health = health.clone();
        device.on_uncaptured_error(Arc::new(move |error: wgpu::Error| {
            let mut health = error_health
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            health.error.get_or_insert_with(|| error.to_string());
        }));
        let lost_health = health.clone();
        device.set_device_lost_callback(move |reason, message| {
            let mut health = lost_health
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            health.lost = Some(format!("{reason:?}: {message}"));
        });
        let mut state = SurfaceState::new(physical);
        state.resize(physical, device.limits().max_texture_dimension_2d)?;
        let mut config = surface
            .get_default_config(&adapter, physical.width.max(1), physical.height.max(1))
            .ok_or(GpuError::UnsupportedSurface)?;
        config.usage = wgpu::TextureUsages::RENDER_ATTACHMENT;
        // Configuration is deferred until a nonzero frame; minimized windows stay parked.
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
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: viewport_buffer.as_entire_binding(),
            }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ui-render-layout"),
            bind_group_layouts: &[Some(&viewport_layout)],
            immediate_size: 0,
        });
        const RECT_ATTRIBUTES: [wgpu::VertexAttribute; 2] = [
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 0,
                shader_location: 0,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 16,
                shader_location: 1,
            },
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
        let renderer = Self {
            window,
            surface,
            device,
            queue,
            config,
            pipeline,
            viewport_buffer,
            viewport_bind_group,
            rectangles_buffer,
            adapter_info: gpu_info,
            state,
            health,
        };
        renderer.check_device()?;
        Ok(renderer)
    }

    pub fn adapter_info(&self) -> &wgpu::AdapterInfo {
        &self.adapter_info
    }

    /// Records the latest size. The next render configures a nonzero surface.
    pub fn resize(&mut self, size: PhysicalSize<u32>) -> Result<(), GpuError> {
        self.state
            .resize(size, self.device.limits().max_texture_dimension_2d)
    }

    fn check_device(&self) -> Result<(), GpuError> {
        let health = self
            .health
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(message) = &health.lost {
            return Err(GpuError::DeviceLost(message.clone()));
        }
        if let Some(message) = &health.error {
            return Err(GpuError::Backend(message.clone()));
        }
        Ok(())
    }

    fn configure(&mut self) -> Result<(), GpuError> {
        self.config.width = self.state.size.width;
        self.config.height = self.state.size.height;
        self.surface.configure(&self.device, &self.config);
        self.check_device()?;
        self.state.needs_configure = false;
        Ok(())
    }

    pub fn render(&mut self, scene: &Scene, scale_factor: f32) -> Result<RenderOutcome, GpuError> {
        match self.render_frame(scene, scale_factor) {
            Err(GpuError::DeviceLost(message)) => {
                if self.state.recovery != Some(RecoveryReason::DeviceLost) {
                    eprintln!("GPU device lost: {message}");
                }
                self.state.recovery = Some(RecoveryReason::DeviceLost);
                Ok(RenderOutcome::RecoveryRequired(RecoveryReason::DeviceLost))
            }
            result => result,
        }
    }

    fn render_frame(
        &mut self,
        scene: &Scene,
        scale_factor: f32,
    ) -> Result<RenderOutcome, GpuError> {
        self.check_device()?;
        match self.state.preparation() {
            Preparation::ZeroSized => return Ok(RenderOutcome::Skipped(SkipReason::ZeroSized)),
            Preparation::Recover(reason) => return Ok(RenderOutcome::RecoveryRequired(reason)),
            Preparation::Configure => self.configure()?,
            Preparation::Acquire => {}
        }
        check_frame_inputs(scene.rectangles.len(), scale_factor)?;
        let (frame, suboptimal) = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame) => (frame, false),
            wgpu::CurrentSurfaceTexture::Suboptimal(frame) => (frame, true),
            wgpu::CurrentSurfaceTexture::Timeout => {
                return Ok(RenderOutcome::Skipped(SkipReason::Timeout));
            }
            wgpu::CurrentSurfaceTexture::Occluded => {
                return Ok(RenderOutcome::Skipped(SkipReason::Occluded));
            }
            wgpu::CurrentSurfaceTexture::Outdated => {
                self.configure()?;
                return Ok(RenderOutcome::Skipped(SkipReason::Reconfigured));
            }
            wgpu::CurrentSurfaceTexture::Lost => {
                self.state.recovery = Some(RecoveryReason::SurfaceLost);
                return Ok(RenderOutcome::RecoveryRequired(RecoveryReason::SurfaceLost));
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                self.check_device()?;
                return Err(GpuError::SurfaceValidation);
            }
        };
        let instances: Vec<RectInstance> = scene
            .rectangles
            .iter()
            .map(|r| RectInstance {
                position_size: [
                    r.bounds.x * scale_factor,
                    r.bounds.y * scale_factor,
                    r.bounds.width * scale_factor,
                    r.bounds.height * scale_factor,
                ],
                rgba: [r.color.r, r.color.g, r.color.b, r.color.a],
            })
            .collect();
        let dimensions = ViewportUniform {
            dimensions: [self.config.width as f32, self.config.height as f32],
            padding: [0.0; 2],
        };
        self.queue
            .write_buffer(&self.viewport_buffer, 0, bytemuck::bytes_of(&dimensions));
        if !instances.is_empty() {
            self.queue
                .write_buffer(&self.rectangles_buffer, 0, bytemuck::cast_slice(&instances));
        }
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("ui-frame"),
            });
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
        self.check_device()?;
        self.window.pre_present_notify();
        self.queue.present(frame);
        self.check_device()?;
        // No reconfiguration while a SurfaceTexture is alive. Present consumes it first.
        self.state.needs_configure = suboptimal;
        Ok(RenderOutcome::Presented {
            rectangles: instances.len(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_sized_window_stays_parked_until_restored() {
        for size in [
            PhysicalSize::new(0, 0),
            PhysicalSize::new(0, 800),
            PhysicalSize::new(1200, 0),
        ] {
            let mut state = SurfaceState::new(size);
            assert_eq!(state.preparation(), Preparation::ZeroSized);
            state.resize(PhysicalSize::new(1200, 800), 8192).unwrap();
            assert_eq!(state.preparation(), Preparation::Configure);
            state.needs_configure = false;
            assert_eq!(state.preparation(), Preparation::Acquire);
            state.resize(size, 8192).unwrap();
            assert_eq!(state.preparation(), Preparation::ZeroSized);
        }
    }

    #[test]
    fn rapid_resize_uses_latest_size_and_rejects_unsupported_sizes() {
        let mut state = SurfaceState::new(PhysicalSize::new(800, 600));
        state.needs_configure = false;
        for size in [
            PhysicalSize::new(900, 700),
            PhysicalSize::new(1000, 800),
            PhysicalSize::new(1200, 900),
        ] {
            state.resize(size, 8192).unwrap();
        }
        assert_eq!(state.size, PhysicalSize::new(1200, 900));
        assert_eq!(state.preparation(), Preparation::Configure);
        assert!(matches!(
            state.resize(PhysicalSize::new(8193, 900), 8192),
            Err(GpuError::SurfaceTooLarge { .. })
        ));
        assert_eq!(state.size, PhysicalSize::new(1200, 900));
    }

    #[test]
    fn resize_cannot_clear_a_lost_surface_or_device() {
        for reason in [RecoveryReason::SurfaceLost, RecoveryReason::DeviceLost] {
            let mut state = SurfaceState::new(PhysicalSize::new(800, 600));
            state.recovery = Some(reason);
            for size in [PhysicalSize::new(0, 0), PhysicalSize::new(1000, 800)] {
                state.resize(size, 8192).unwrap();
                assert_eq!(state.preparation(), Preparation::Recover(reason));
            }
            assert_eq!(
                SurfaceState::new(state.size).preparation(),
                Preparation::Configure
            );
        }
    }

    #[test]
    fn bad_frame_inputs_are_rejected_before_upload() {
        for scale in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            assert!(matches!(
                check_frame_inputs(1, scale),
                Err(GpuError::InvalidScaleFactor(_))
            ));
        }
        assert!(check_frame_inputs(MAX_RECTANGLES, 1.5).is_ok());
        assert!(matches!(
            check_frame_inputs(MAX_RECTANGLES + 1, 1.0),
            Err(GpuError::SceneTooLarge { .. })
        ));
    }

    #[test]
    fn software_and_noop_adapters_are_not_accepted_as_gpu_rendering() {
        for backend in [
            wgpu::Backend::Vulkan,
            wgpu::Backend::Dx12,
            wgpu::Backend::Metal,
        ] {
            assert!(matches!(
                check_adapter(wgpu::DeviceType::Cpu, backend, "software"),
                Err(GpuError::SoftwareAdapter(_))
            ));
            assert!(check_adapter(wgpu::DeviceType::IntegratedGpu, backend, "integrated").is_ok());
            assert!(check_adapter(wgpu::DeviceType::DiscreteGpu, backend, "discrete").is_ok());
        }
        assert!(matches!(
            check_adapter(wgpu::DeviceType::Other, wgpu::Backend::Noop, "noop"),
            Err(GpuError::SoftwareAdapter(_))
        ));
    }

    #[test]
    fn same_size_resize_preserves_pending_reconfiguration() {
        let size = PhysicalSize::new(800, 600);
        let mut state = SurfaceState::new(size);
        state.needs_configure = false;
        state.resize(size, 8192).unwrap();
        assert_eq!(state.preparation(), Preparation::Acquire);
        // A suboptimal frame schedules configuration only after it has been consumed.
        state.needs_configure = true;
        state.resize(size, 8192).unwrap();
        assert_eq!(state.preparation(), Preparation::Configure);
        state.resize(PhysicalSize::new(0, 0), 8192).unwrap();
        assert_eq!(state.preparation(), Preparation::ZeroSized);
        state.resize(size, 8192).unwrap();
        assert_eq!(state.preparation(), Preparation::Configure);
    }
}
