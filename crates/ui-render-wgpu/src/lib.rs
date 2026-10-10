//! Experimental GPU backend. No external GUI framework is used.
//! Scenes are provided by ui-core; application state stays in the host.
//!
//! Color policy: `ui_core::Color` is straight-alpha sRGB. Instance colors are
//! decoded to linear RGB; an explicitly selected sRGB surface encodes output.
//! Rectangles use straight-alpha blending; glyphs use linear premultiplied
//! samples/tints. The opaque white clear is linear `(1, 1, 1, 1)`. No HDR or
//! wide-gamut conversion is performed.

use bytemuck::{Pod, Zeroable};
mod glyphs;
use glyphs::GlyphRenderer;
use rust_desktop_ui_core::{Color, DrawCommand, Rect, RoundedRect, Scene};
use std::{
    fmt,
    iter::Peekable,
    ops::Range,
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
    @location(1) @interpolate(flat) shape: vec4<f32>,
    @location(2) @interpolate(flat) radius: f32,
};

@vertex
fn vs_main(
    @builtin(vertex_index) index: u32,
    @location(0) rect: vec4<f32>,
    @location(1) color: vec4<f32>,
    @location(2) shape: vec4<f32>,
    @location(3) radius: f32,
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
    out.shape = shape;
    out.radius = radius;
    return out;
}

@fragment
fn fs_main(input: VertexOut) -> @location(0) vec4<f32> {
    // A negative radius marks an ordinary rectangle. Rounded fills retain
    // their original center/half-size even when the raster quad is clipped.
    if input.radius < 0.0 {
        return input.color;
    }
    let q = abs(input.position.xy - input.shape.xy) - input.shape.zw
        + vec2(input.radius);
    // Normalize length to avoid squared-distance overflow for finite but
    // very large coordinates. One physical pixel provides corner coverage.
    let unit = max(input.radius, 1.0);
    let distance = length(max(q, vec2(0.0)) / unit) * unit
        + min(max(q.x, q.y), 0.0) - input.radius;
    let coverage = clamp(0.5 - distance, 0.0, 1.0);
    return vec4(input.color.rgb, input.color.a * coverage);
}
"#;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct RectInstance {
    position_size: [f32; 4],
    rgba: [f32; 4],
    shape: [f32; 4],
    radius: f32,
}

fn srgb_to_linear(channel: f32) -> f32 {
    if channel <= 0.04045 {
        channel / 12.92
    } else {
        ((channel + 0.055) / 1.055).powf(2.4)
    }
}

fn linear_to_srgb(channel: f32) -> f32 {
    if channel <= 0.003_130_8 {
        channel * 12.92
    } else {
        1.055 * channel.powf(1.0 / 2.4) - 0.055
    }
}

fn linear_rgba(color: Color) -> [f32; 4] {
    [
        srgb_to_linear(color.r),
        srgb_to_linear(color.g),
        srgb_to_linear(color.b),
        color.a,
    ]
}

fn srgb_surface_format(formats: &[wgpu::TextureFormat]) -> Result<wgpu::TextureFormat, GpuError> {
    formats
        .iter()
        .copied()
        .find(wgpu::TextureFormat::is_srgb)
        .ok_or(GpuError::UnsupportedSurface)
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct ViewportUniform {
    dimensions: [f32; 2],
    padding: [f32; 2],
}

const MAX_RECTANGLES: usize = 16_384;
const MAX_TEXT_RUNS: usize = 16_384;
const MAX_TEXT_BYTES: usize = 1024 * 1024;

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
    Text(String),
    InvalidScene,
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
            Self::Text(message) => write!(f, "Text rendering failed: {message}"),
            Self::InvalidScene => f.write_str("Scene has invalid geometry, color or painter order"),
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

fn physical_rect(bounds: Rect, scale: f32) -> Option<Rect> {
    let physical = Rect::new(
        bounds.x * scale,
        bounds.y * scale,
        bounds.width * scale,
        bounds.height * scale,
    );
    (bounds.is_valid() && physical.is_valid()).then_some(physical)
}

fn rounded_instance(rectangle: &RoundedRect, scale: f32, viewport: Rect) -> RectInstance {
    // check_scene validates all physical values before this conversion.
    let bounds = physical_rect(rectangle.bounds, scale).unwrap();
    let clip = physical_rect(rectangle.clip, scale).unwrap();
    let visible = Rect::new(
        bounds.x - 0.5,
        bounds.y - 0.5,
        bounds.width + 1.0,
        bounds.height + 1.0,
    )
    .intersection(clip)
    .and_then(|bounds| bounds.intersection(viewport))
    .unwrap_or(Rect::new(0.0, 0.0, 0.0, 0.0));
    RectInstance {
        position_size: [visible.x, visible.y, visible.width, visible.height],
        rgba: linear_rgba(rectangle.color),
        shape: [
            bounds.x + bounds.width * 0.5,
            bounds.y + bounds.height * 0.5,
            bounds.width * 0.5,
            bounds.height * 0.5,
        ],
        radius: rectangle.radius * scale,
    }
}

// Scene's typed arrays are public for inspection. Validate them again at the
// backend boundary because direct mutation can bypass Scene::fill/text.
fn check_scene(scene: &Scene, scale: f32) -> Result<(), GpuError> {
    let rectangle_count = scene
        .rectangles
        .len()
        .checked_add(scene.rounded_rectangles.len())
        .ok_or(GpuError::InvalidScene)?;
    check_frame_inputs(rectangle_count, scale)?;
    if scene.texts.len() > MAX_TEXT_RUNS {
        return Err(GpuError::Text("text run count exceeds 16384".into()));
    }
    if scene.commands().len() != rectangle_count + scene.texts.len() {
        return Err(GpuError::InvalidScene);
    }
    let (mut rectangle, mut rounded_rectangle, mut text) = (0, 0, 0);
    for command in scene.commands() {
        match *command {
            DrawCommand::Rectangle(index) if index == rectangle => rectangle += 1,
            DrawCommand::RoundedRectangle(index) if index == rounded_rectangle => {
                rounded_rectangle += 1;
            }
            DrawCommand::Text(index) if index == text => text += 1,
            _ => return Err(GpuError::InvalidScene),
        }
    }
    if rectangle != scene.rectangles.len()
        || rounded_rectangle != scene.rounded_rectangles.len()
        || text != scene.texts.len()
    {
        return Err(GpuError::InvalidScene);
    }
    for rectangle in &scene.rectangles {
        if physical_rect(rectangle.bounds, scale).is_none()
            || [
                rectangle.color.r,
                rectangle.color.g,
                rectangle.color.b,
                rectangle.color.a,
            ]
            .into_iter()
            .any(|channel| !channel.is_finite() || !(0.0..=1.0).contains(&channel))
        {
            return Err(GpuError::InvalidScene);
        }
    }
    for rectangle in &scene.rounded_rectangles {
        if !rectangle.is_valid()
            || physical_rect(rectangle.bounds, scale).is_none()
            || physical_rect(rectangle.clip, scale).is_none()
            || !(rectangle.radius * scale).is_finite()
        {
            return Err(GpuError::InvalidScene);
        }
    }
    let mut text_bytes = 0usize;
    for text in &scene.texts {
        if !text.is_valid()
            || physical_rect(text.bounds, scale).is_none()
            || physical_rect(text.clip, scale).is_none()
            || !((text.font_size * scale).is_finite())
            || text.font_size * scale > 1024.0
        {
            return Err(GpuError::InvalidScene);
        }
        text_bytes = text_bytes
            .checked_add(text.text.len())
            .filter(|bytes| *bytes <= MAX_TEXT_BYTES)
            .ok_or_else(|| GpuError::Text("frame text exceeds 1048576 UTF-8 bytes".into()))?;
    }
    Ok(())
}

#[derive(Debug, PartialEq, Eq)]
enum DrawBatch {
    Rectangles(Range<u32>),
    RoundedRectangles(Range<u32>),
    Text(Range<u32>),
}

// Only adjacent commands may merge: a popup background must stay after the
// text underneath it and before its own caption.
fn next_batch(
    commands: &mut Peekable<impl Iterator<Item = DrawCommand>>,
    text_range: impl Fn(usize) -> Option<Range<u32>>,
) -> Result<Option<DrawBatch>, GpuError> {
    let Some(command) = commands.next() else {
        return Ok(None);
    };
    Ok(Some(match command {
        DrawCommand::Rectangle(start) => {
            let mut end = start.checked_add(1).ok_or(GpuError::InvalidScene)?;
            while commands.peek() == Some(&DrawCommand::Rectangle(end)) {
                commands.next();
                end = end.checked_add(1).ok_or(GpuError::InvalidScene)?;
            }
            DrawBatch::Rectangles(
                u32::try_from(start).map_err(|_| GpuError::InvalidScene)?
                    ..u32::try_from(end).map_err(|_| GpuError::InvalidScene)?,
            )
        }
        DrawCommand::RoundedRectangle(start) => {
            let mut end = start.checked_add(1).ok_or(GpuError::InvalidScene)?;
            while commands.peek() == Some(&DrawCommand::RoundedRectangle(end)) {
                commands.next();
                end = end.checked_add(1).ok_or(GpuError::InvalidScene)?;
            }
            DrawBatch::RoundedRectangles(
                u32::try_from(start).map_err(|_| GpuError::InvalidScene)?
                    ..u32::try_from(end).map_err(|_| GpuError::InvalidScene)?,
            )
        }
        DrawCommand::Text(index) => {
            let mut range = text_range(index).ok_or(GpuError::InvalidScene)?;
            while let Some(DrawCommand::Text(next)) = commands.peek() {
                let next_range = text_range(*next).ok_or(GpuError::InvalidScene)?;
                if range.end != next_range.start {
                    break;
                }
                range.end = next_range.end;
                commands.next();
            }
            DrawBatch::Text(range)
        }
    }))
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
    glyphs: GlyphRenderer,
    stats: RendererStats,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct RendererStats {
    pub rectangles: usize,
    pub glyphs: usize,
    pub text_runs: usize,
    pub cached_glyphs: usize,
    pub missing_glyphs: usize,
    pub draw_calls: usize,
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
        config.format = srgb_surface_format(&surface.get_capabilities(&adapter).formats)?;
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
        const RECT_ATTRIBUTES: [wgpu::VertexAttribute; 4] = [
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
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 32,
                shader_location: 2,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32,
                offset: 48,
                shader_location: 3,
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
        let glyphs = GlyphRenderer::new(&device, config.format, &viewport_layout);
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
            glyphs,
            stats: RendererStats::default(),
        };
        renderer.check_device()?;
        Ok(renderer)
    }

    pub fn adapter_info(&self) -> &wgpu::AdapterInfo {
        &self.adapter_info
    }

    pub fn stats(&self) -> RendererStats {
        self.stats
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
        check_scene(scene, scale_factor)?;
        let viewport = Rect::new(
            0.0,
            0.0,
            self.config.width as f32,
            self.config.height as f32,
        );
        self.glyphs
            .prepare(&scene.texts, scale_factor, viewport, &self.queue)?;
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
        let mut instances: Vec<RectInstance> = scene
            .rectangles
            .iter()
            .map(|r| {
                // Clamp before the shader's pixel-to-NDC arithmetic. Finite,
                // off-screen f32 coordinates can otherwise overflow there.
                let bounds = physical_rect(r.bounds, scale_factor)
                    .and_then(|bounds| bounds.intersection(viewport))
                    .unwrap_or(Rect::new(0.0, 0.0, 0.0, 0.0));
                RectInstance {
                    position_size: [bounds.x, bounds.y, bounds.width, bounds.height],
                    rgba: linear_rgba(r.color),
                    shape: [0.0; 4],
                    radius: -1.0,
                }
            })
            .collect();
        instances.extend(
            scene
                .rounded_rectangles
                .iter()
                .map(|rectangle| rounded_instance(rectangle, scale_factor, viewport)),
        );
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
            pass.set_bind_group(0, &self.viewport_bind_group, &[]);
            let mut commands = scene.commands().iter().copied().peekable();
            let mut draw_calls = 0;
            while let Some(batch) = next_batch(&mut commands, |index| self.glyphs.range(index))? {
                match batch {
                    DrawBatch::Rectangles(range) => {
                        pass.set_pipeline(&self.pipeline);
                        pass.set_vertex_buffer(0, self.rectangles_buffer.slice(..));
                        pass.draw(0..6, range);
                        draw_calls += 1;
                    }
                    DrawBatch::RoundedRectangles(range) => {
                        let offset = scene.rectangles.len() as u32;
                        pass.set_pipeline(&self.pipeline);
                        pass.set_vertex_buffer(0, self.rectangles_buffer.slice(..));
                        pass.draw(0..6, range.start + offset..range.end + offset);
                        draw_calls += 1;
                    }
                    DrawBatch::Text(range) => {
                        if !range.is_empty() {
                            self.glyphs.draw(&mut pass, range);
                            draw_calls += 1;
                        }
                    }
                }
            }
            self.stats = RendererStats {
                rectangles: instances.len(),
                glyphs: self.glyphs.len(),
                text_runs: scene.texts.len(),
                cached_glyphs: self.glyphs.cached(),
                missing_glyphs: self.glyphs.missing_glyphs,
                draw_calls,
            };
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
    use rust_desktop_ui_core::{Color, TextRun};

    #[test]
    fn srgb_colors_round_trip_without_gamma_changing_alpha() {
        for byte in 0..=255_u8 {
            let encoded = f32::from(byte) / 255.0;
            let decoded = srgb_to_linear(encoded);
            assert!((linear_to_srgb(decoded) - encoded).abs() < 0.000_01);
        }
        let color = Color {
            a: 0.3,
            ..Color::rgb(128, 64, 32)
        };
        let linear = linear_rgba(color);
        assert!((linear[0] - 0.215_860_5).abs() < 0.000_001);
        assert!((linear[1] - 0.051_269_5).abs() < 0.000_001);
        assert_eq!(linear[3], color.a);
        // 50% white over black is blended in linear space, then encoded once.
        let result = linear_to_srgb(0.5);
        assert!((result * 255.0 - 187.516).abs() < 0.001);
        assert_eq!(linear_rgba(Color::rgb(255, 255, 255)), [1.0; 4]);
    }

    #[test]
    fn surface_selection_requires_srgb_encoding_regardless_of_format_order() {
        use wgpu::TextureFormat::{Bgra8Unorm, Bgra8UnormSrgb, Rgba8UnormSrgb};
        assert_eq!(
            srgb_surface_format(&[Bgra8Unorm, Bgra8UnormSrgb]).unwrap(),
            Bgra8UnormSrgb
        );
        assert_eq!(
            srgb_surface_format(&[Rgba8UnormSrgb]).unwrap(),
            Rgba8UnormSrgb
        );
        assert!(matches!(
            srgb_surface_format(&[Bgra8Unorm]),
            Err(GpuError::UnsupportedSurface)
        ));
    }

    #[test]
    fn rounded_quad_clipping_preserves_original_shape_and_dpi_radius() {
        let rectangle = RoundedRect {
            bounds: Rect::new(10.0, 20.0, 100.0, 40.0),
            clip: Rect::new(30.0, 10.0, 100.0, 100.0),
            radius: 4.0,
            color: Color::rgb(128, 64, 32),
        };
        let instance = rounded_instance(&rectangle, 1.5, Rect::new(0.0, 0.0, 120.0, 80.0));
        assert_eq!(instance.position_size, [45.0, 29.5, 75.0, 50.5]);
        assert_eq!(instance.shape, [90.0, 60.0, 75.0, 30.0]);
        assert_eq!(instance.radius, 6.0);
        assert_eq!(instance.rgba, linear_rgba(rectangle.color));
        let hidden = rounded_instance(&rectangle, 1.5, Rect::new(0.0, 0.0, 5.0, 5.0));
        assert_eq!(hidden.position_size, [0.0; 4]);
    }

    #[test]
    fn rounded_geometry_corruption_and_shared_primitive_budget_are_rejected() {
        let bounds = Rect::new(0.0, 0.0, 100.0, 40.0);
        let mut original = mixed_scene();
        original.rounded_fill(bounds, bounds, 4.0, Color::rgb(0, 0, 0));
        assert!(check_scene(&original, 1.5).is_ok());
        for radius in [-1.0, f32::NAN, f32::INFINITY, 21.0] {
            let mut broken = original.clone();
            broken.rounded_rectangles[0].radius = radius;
            assert!(matches!(
                check_scene(&broken, 1.0),
                Err(GpuError::InvalidScene)
            ));
        }
        let mut broken = original.clone();
        broken.rounded_rectangles[0].clip.x = f32::NAN;
        assert!(check_scene(&broken, 1.0).is_err());
        let mut broken = original.clone();
        broken.rounded_rectangles.clear();
        assert!(check_scene(&broken, 1.0).is_err());
        let mut broken = original.clone();
        broken.rounded_rectangles[0].bounds.width = f32::MAX;
        assert!(check_scene(&broken, 2.0).is_err());
        for _ in 0..MAX_RECTANGLES - 1 {
            original.rounded_fill(bounds, bounds, 4.0, Color::rgb(0, 0, 0));
        }
        assert!(matches!(
            check_scene(&original, 1.0),
            Err(GpuError::SceneTooLarge { .. })
        ));
    }

    #[test]
    fn rounded_batches_do_not_cross_text_or_solid_painter_boundaries() {
        let mut commands = [
            DrawCommand::Rectangle(0),
            DrawCommand::RoundedRectangle(0),
            DrawCommand::RoundedRectangle(1),
            DrawCommand::Text(0),
            DrawCommand::RoundedRectangle(2),
            DrawCommand::Rectangle(1),
        ]
        .into_iter()
        .peekable();
        let mut batches = Vec::new();
        while let Some(batch) = next_batch(&mut commands, |_| Some(0..4)).unwrap() {
            batches.push(batch);
        }
        assert_eq!(
            batches,
            [
                DrawBatch::Rectangles(0..1),
                DrawBatch::RoundedRectangles(0..2),
                DrawBatch::Text(0..4),
                DrawBatch::RoundedRectangles(2..3),
                DrawBatch::Rectangles(1..2),
            ]
        );
    }

    fn mixed_scene() -> Scene {
        let mut scene = Scene::default();
        scene.fill(Rect::new(0.0, 0.0, 200.0, 100.0), Color::rgb(255, 255, 255));
        scene.text(TextRun::new(
            "Caption",
            Rect::new(5.0, 5.0, 180.0, 30.0),
            Color::rgb(0, 0, 0),
            14.0,
        ));
        scene
    }

    #[test]
    fn corrupted_public_scene_arrays_are_rejected_before_upload() {
        let original = mixed_scene();
        assert!(check_scene(&original, 1.5).is_ok());
        let mut scene = original.clone();
        scene.rectangles.clear();
        assert!(matches!(
            check_scene(&scene, 1.0),
            Err(GpuError::InvalidScene)
        ));
        // Same total length is insufficient: Rectangle(0) no longer exists.
        scene.texts.push(scene.texts[0].clone());
        assert!(matches!(
            check_scene(&scene, 1.0),
            Err(GpuError::InvalidScene)
        ));
        let mut scene = original.clone();
        scene.rectangles.push(scene.rectangles[0]);
        assert!(matches!(
            check_scene(&scene, 1.0),
            Err(GpuError::InvalidScene)
        ));
        for coordinate in [f32::NAN, f32::INFINITY] {
            let mut scene = original.clone();
            scene.rectangles[0].bounds.x = coordinate;
            assert!(matches!(
                check_scene(&scene, 1.0),
                Err(GpuError::InvalidScene)
            ));
        }
        let mut scene = original.clone();
        scene.rectangles[0].bounds.width = -1.0;
        assert!(check_scene(&scene, 1.0).is_err());
        scene.rectangles[0].bounds = Rect::new(0.0, 0.0, f32::MAX, 10.0);
        assert!(check_scene(&scene, 2.0).is_err());
        let mut scene = original.clone();
        scene.rectangles[0].color.a = f32::NAN;
        assert!(check_scene(&scene, 1.0).is_err());
        let mut scene = original;
        scene.texts[0].clip.x = f32::NAN;
        assert!(check_scene(&scene, 1.0).is_err());
    }

    #[test]
    fn aggregate_text_work_is_bounded_before_shaping() {
        let mut scene = mixed_scene();
        scene.texts[0].text = "x".repeat(MAX_TEXT_BYTES + 1);
        assert!(matches!(check_scene(&scene, 1.0), Err(GpuError::Text(_))));
        let mut scene = Scene::default();
        let run = mixed_scene().texts.remove(0);
        for _ in 0..=MAX_TEXT_RUNS {
            scene.text(run.clone());
        }
        assert!(matches!(check_scene(&scene, 1.0), Err(GpuError::Text(_))));
    }

    #[test]
    fn mixed_batches_preserve_popup_painter_order_and_empty_text() {
        let mut commands = [
            DrawCommand::Rectangle(0),
            DrawCommand::Rectangle(1),
            DrawCommand::Text(0),
            DrawCommand::Text(1),
            DrawCommand::Rectangle(2),
            DrawCommand::Text(2),
            DrawCommand::Text(3),
            DrawCommand::Rectangle(3),
        ]
        .into_iter()
        .peekable();
        let ranges = [0..3, 3..5, 5..5, 5..8];
        let mut batches = Vec::new();
        while let Some(batch) =
            next_batch(&mut commands, |index| ranges.get(index).cloned()).unwrap()
        {
            batches.push(batch);
        }
        assert_eq!(
            batches,
            [
                DrawBatch::Rectangles(0..2),
                DrawBatch::Text(0..5),
                DrawBatch::Rectangles(2..3),
                DrawBatch::Text(5..8),
                DrawBatch::Rectangles(3..4)
            ]
        );
        let mut broken = [DrawCommand::Text(4)].into_iter().peekable();
        assert!(next_batch(&mut broken, |index| ranges.get(index).cloned()).is_err());
    }

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
