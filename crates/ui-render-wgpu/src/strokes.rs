//! Fixed-work stroke unions: one raster quad and one blend per path.
use crate::{GpuError, MAX_RECTANGLES, linear_rgba, physical_rect};
use bytemuck::{Pod, Zeroable};
use rust_desktop_ui_core::{MAX_STROKE_SEGMENTS, Rect, StrokePath};
use std::ops::Range;

// With endpoints in this range, coordinate differences and their squared dot
// products remain finite in WGSL f32 (at most 8e36, below f32::MAX).
const MAX_PHYSICAL_STROKE_COORDINATE: f32 = 1.0e18;

const SHADER: &str = r#"
struct Viewport { dimensions: vec2<f32>, unused: vec2<f32> }
@group(0) @binding(0) var<uniform> viewport: Viewport;

struct Stroke {
    rect: vec4<f32>,
    color: vec4<f32>,
    half_width: f32,
    count: u32,
    unused: vec2<f32>,
    segments: array<vec4<f32>, 16>,
}
@group(1) @binding(0) var<storage, read> paths: array<Stroke>;

struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) @interpolate(flat) path: u32,
}

@vertex
fn vs_main(@builtin(vertex_index) vertex: u32, @builtin(instance_index) instance: u32) -> VertexOut {
    let corners = array<vec2<f32>, 6>(
        vec2(0.0, 0.0), vec2(1.0, 0.0), vec2(0.0, 1.0),
        vec2(0.0, 1.0), vec2(1.0, 0.0), vec2(1.0, 1.0),
    );
    let rect = paths[instance].rect;
    let pixel = rect.xy + corners[vertex] * rect.zw;
    var out: VertexOut;
    out.position = vec4(pixel.x / viewport.dimensions.x * 2.0 - 1.0,
        1.0 - pixel.y / viewport.dimensions.y * 2.0, 0.0, 1.0);
    out.path = instance;
    return out;
}

@fragment
fn fs_main(input: VertexOut) -> @location(0) vec4<f32> {
    let path = paths[input.path];
    var distance = 3.0e18;
    for (var i = 0u; i < min(path.count, 16u); i += 1u) {
        let segment = path.segments[i];
        let vector = segment.zw - segment.xy;
        let relative = input.position.xy - segment.xy;
        // A zero-length segment is a round dot. A tiny denominator clamp also
        // avoids division by zero for subnormal finite endpoint differences.
        let fraction = clamp(dot(relative, vector) / max(dot(vector, vector), 1.0e-20), 0.0, 1.0);
        distance = min(distance, length(relative - fraction * vector) - path.half_width);
    }
    // Distances are physical pixels: a one-pixel AA transition at every DPI.
    // Union precedes coverage and blending, so intersecting translucent arms
    // have exactly the same alpha as a single arm.
    let coverage = min(clamp(0.5 - distance, 0.0, 1.0), path.half_width * 2.0);
    return vec4(path.color.rgb, path.color.a * coverage);
}
"#;

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct StrokeInstance {
    position_size: [f32; 4],
    rgba: [f32; 4],
    half_width: f32,
    count: u32,
    padding: [f32; 2],
    segments: [[f32; 4]; MAX_STROKE_SEGMENTS],
}

pub(crate) fn valid_physical_path(path: &StrokePath, scale: f32) -> bool {
    let safe = |value: f32| value.is_finite() && value.abs() <= MAX_PHYSICAL_STROKE_COORDINATE;
    path.is_valid()
        && scale.is_finite()
        && scale > 0.0
        && safe(path.width * scale)
        && path.width * scale > 0.0
        && physical_rect(path.bounds().unwrap(), scale).is_some()
        && physical_rect(path.clip, scale).is_some()
        && path.segments.iter().all(|segment| {
            [
                segment.start.x,
                segment.start.y,
                segment.end.x,
                segment.end.y,
            ]
            .into_iter()
            .all(|value| safe(value * scale))
        })
}

fn instance(path: &StrokePath, scale: f32, viewport: Rect) -> StrokeInstance {
    // The scene boundary validates physical geometry before this conversion.
    let bounds = physical_rect(path.bounds().unwrap(), scale).unwrap();
    let clip = physical_rect(path.clip, scale).unwrap();
    let visible = Rect::new(
        bounds.x - 0.5,
        bounds.y - 0.5,
        bounds.width + 1.0,
        bounds.height + 1.0,
    )
    .intersection(clip)
    .and_then(|rect| rect.intersection(viewport))
    .unwrap_or(Rect::new(0.0, 0.0, 0.0, 0.0));
    let mut instance = StrokeInstance {
        position_size: [visible.x, visible.y, visible.width, visible.height],
        rgba: linear_rgba(path.color),
        half_width: path.width * scale * 0.5,
        count: path.segments.len() as u32,
        padding: [0.0; 2],
        segments: [[0.0; 4]; MAX_STROKE_SEGMENTS],
    };
    for (target, segment) in instance.segments.iter_mut().zip(&path.segments) {
        *target = [
            segment.start.x * scale,
            segment.start.y * scale,
            segment.end.x * scale,
            segment.end.y * scale,
        ];
    }
    instance
}

pub(crate) struct StrokeRenderer {
    pipeline: wgpu::RenderPipeline,
    buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
}

impl StrokeRenderer {
    pub(crate) fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        viewport: &wgpu::BindGroupLayout,
    ) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ui-stroke-storage-layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ui-stroke-paths"),
            size: (MAX_RECTANGLES * std::mem::size_of::<StrokeInstance>()) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ui-stroke-storage"),
            layout: &layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            }],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ui-strokes"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ui-stroke-pipeline-layout"),
            bind_group_layouts: &[Some(viewport), Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("ui-strokes-pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
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
        Self {
            pipeline,
            buffer,
            bind_group,
        }
    }

    pub(crate) fn prepare(
        &self,
        paths: &[StrokePath],
        scale: f32,
        viewport: Rect,
        queue: &wgpu::Queue,
    ) -> Result<(), GpuError> {
        if paths.len() > MAX_RECTANGLES
            || !paths.iter().all(|path| valid_physical_path(path, scale))
        {
            return Err(GpuError::InvalidScene);
        }
        if !paths.is_empty() {
            let instances: Vec<_> = paths
                .iter()
                .map(|path| instance(path, scale, viewport))
                .collect();
            queue.write_buffer(&self.buffer, 0, bytemuck::cast_slice(&instances));
        }
        Ok(())
    }

    pub(crate) fn draw(&self, pass: &mut wgpu::RenderPass<'_>, range: Range<u32>) {
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(1, &self.bind_group, &[]);
        pass.draw(0..6, range);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_desktop_ui_core::{Color, Point, StrokeSegment};

    #[test]
    fn stroke_instances_keep_original_endpoints_under_clip_and_dpi() {
        let path = StrokePath {
            segments: vec![StrokeSegment {
                start: Point::new(10.0, 20.0),
                end: Point::new(50.0, 40.0),
            }],
            clip: Rect::new(20.0, 0.0, 100.0, 100.0),
            width: 2.0,
            color: Color::rgb(128, 64, 32),
        };
        assert!(valid_physical_path(&path, 1.5));
        let instance = instance(&path, 1.5, Rect::new(0.0, 0.0, 60.0, 60.0));
        assert_eq!(instance.position_size, [30.0, 28.0, 30.0, 32.0]);
        assert_eq!(instance.segments[0], [15.0, 30.0, 75.0, 60.0]);
        assert_eq!(instance.half_width, 1.5);
        assert_eq!(instance.count, 1);
        assert_eq!(instance.rgba, linear_rgba(path.color));
        assert_eq!(std::mem::size_of::<StrokeInstance>(), 304);
    }

    #[test]
    fn physical_stroke_limits_reject_overflow_and_allow_degenerate_dots() {
        let mut path = StrokePath {
            segments: vec![StrokeSegment {
                start: Point::new(5.0, 5.0),
                end: Point::new(5.0, 5.0),
            }],
            clip: Rect::new(0.0, 0.0, 100.0, 100.0),
            width: 2.0,
            color: Color::rgb(0, 0, 0),
        };
        assert!(valid_physical_path(&path, 2.0));
        for scale in [0.0, -1.0, f32::NAN, f32::INFINITY, f32::MAX] {
            assert!(!valid_physical_path(&path, scale));
        }
        path.segments[0].end.x = MAX_PHYSICAL_STROKE_COORDINATE;
        assert!(valid_physical_path(&path, 1.0));
        assert!(!valid_physical_path(&path, 2.0));
        path.segments[0].end.x = f32::MAX;
        assert!(!valid_physical_path(&path, 1.0));
    }

    /// WGPU_BACKEND selects dx12/vulkan. Runs production storage layout, shader,
    /// batching and linear blending at three physical pixel scales.
    #[test]
    #[ignore = "requires a native hardware GPU"]
    fn stroke_path_gpu_union_caps_diagonals_dpi_clip_and_painter_order() {
        use crate::{
            DrawBatch, RectInstance, ViewportUniform, check_adapter, check_scene, next_batch,
            rectangle_pipeline,
        };
        use rust_desktop_ui_core::Scene;
        use std::sync::Arc;
        use wgpu::util::DeviceExt;

        fn block_on<F: std::future::Future>(future: F) -> F::Output {
            struct WakeThread(std::thread::Thread);
            impl std::task::Wake for WakeThread {
                fn wake(self: Arc<Self>) {
                    self.0.unpark();
                }
            }
            let waker = std::task::Waker::from(Arc::new(WakeThread(std::thread::current())));
            let mut context = std::task::Context::from_waker(&waker);
            let mut future = std::pin::pin!(future);
            let started = std::time::Instant::now();
            loop {
                if let std::task::Poll::Ready(result) = future.as_mut().poll(&mut context) {
                    return result;
                }
                assert!(
                    started.elapsed() < std::time::Duration::from_secs(30),
                    "GPU initialization timed out"
                );
                std::thread::park_timeout(std::time::Duration::from_millis(100));
            }
        }
        fn segment(x1: f32, y1: f32, x2: f32, y2: f32) -> StrokeSegment {
            StrokeSegment {
                start: Point::new(x1, y1),
                end: Point::new(x2, y2),
            }
        }
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let adapter = block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            ..Default::default()
        }))
        .expect("hardware adapter");
        let info = adapter.get_info();
        check_adapter(info.device_type, info.backend, &info.name).unwrap();
        println!(
            "stroke-path hardware: {} {:?} {:?}",
            info.name, info.backend, info.device_type
        );
        let (device, queue) =
            block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
        let format = wgpu::TextureFormat::Rgba8UnormSrgb;
        let viewport_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("path-test-viewport-layout"),
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
        let strokes = StrokeRenderer::new(&device, format, &viewport_layout);
        let rectangles = rectangle_pipeline(&device, format, &viewport_layout);
        let mut scene = Scene::default();
        let clip = Rect::new(0.0, 0.0, 128.0, 96.0);
        let half_red = Color {
            a: 0.5,
            ..Color::rgb(255, 0, 0)
        };
        scene.stroke_path(
            &[
                segment(12.0, 24.0, 36.0, 24.0),
                segment(24.0, 12.0, 24.0, 36.0),
            ],
            clip,
            6.0,
            half_red,
        );
        scene.stroke_path(
            &[segment(48.0, 12.0, 72.0, 36.0)],
            clip,
            1.5,
            Color::rgb(0, 255, 0),
        );
        scene.stroke_path(
            &[segment(84.0, 12.0, 108.0, 36.0)],
            Rect::new(96.0, 0.0, 16.0, 48.0),
            4.0,
            Color::rgb(0, 0, 255),
        );
        scene.stroke_path(
            &[segment(24.0, 64.0, 24.0, 64.0)],
            clip,
            8.0,
            Color::rgb(255, 255, 0),
        );
        scene.stroke_path(
            &[
                segment(48.0, 64.0, 60.0, 64.0),
                segment(60.0, 64.0, 60.0, 76.0),
            ],
            clip,
            4.0,
            Color {
                a: 0.5,
                ..Color::rgb(255, 255, 255)
            },
        );
        scene.fill(Rect::new(84.0, 56.0, 32.0, 32.0), Color::rgb(0, 255, 0));
        scene.stroke_path(&[segment(88.0, 72.0, 112.0, 72.0)], clip, 8.0, half_red);
        scene.fill(Rect::new(98.0, 56.0, 4.0, 32.0), Color::rgb(0, 0, 255));

        for scale in [1.0_f32, 1.5, 2.0] {
            check_scene(&scene, scale).unwrap();
            let dimensions = [(128.0 * scale) as u32, (96.0 * scale) as u32];
            let viewport = Rect::new(0.0, 0.0, dimensions[0] as f32, dimensions[1] as f32);
            let viewport_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("path-test-viewport"),
                contents: bytemuck::bytes_of(&ViewportUniform {
                    dimensions: [dimensions[0] as f32, dimensions[1] as f32],
                    padding: [0.0; 2],
                }),
                usage: wgpu::BufferUsages::UNIFORM,
            });
            let viewport_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("path-test-viewport-group"),
                layout: &viewport_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: viewport_buffer.as_entire_binding(),
                }],
            });
            let instances: Vec<_> = scene
                .rectangles
                .iter()
                .map(|rectangle| {
                    let bounds = physical_rect(rectangle.bounds, scale).unwrap();
                    RectInstance {
                        position_size: [bounds.x, bounds.y, bounds.width, bounds.height],
                        rgba: linear_rgba(rectangle.color),
                        shape: [0.0; 4],
                        radius: -1.0,
                        stroke_width: -1.0,
                    }
                })
                .collect();
            let vertices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("path-test-rectangles"),
                contents: bytemuck::cast_slice(&instances),
                usage: wgpu::BufferUsages::VERTEX,
            });
            strokes
                .prepare(&scene.stroke_paths, scale, viewport, &queue)
                .unwrap();
            let size = wgpu::Extent3d {
                width: dimensions[0],
                height: dimensions[1],
                depth_or_array_layers: 1,
            };
            let target = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("path-test-target"),
                size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            let view = target.create_view(&wgpu::TextureViewDescriptor::default());
            let readback = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("path-test-readback"),
                size: u64::from(dimensions[0] * dimensions[1] * 4),
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
            let mut encoder =
                device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("path-test-pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                pass.set_bind_group(0, &viewport_group, &[]);
                let mut commands = scene.commands().iter().copied().peekable();
                while let Some(batch) = next_batch(&mut commands, |_| None).unwrap() {
                    match batch {
                        DrawBatch::StrokePaths(range) => strokes.draw(&mut pass, range),
                        DrawBatch::Rectangles(range) => {
                            pass.set_pipeline(&rectangles);
                            pass.set_vertex_buffer(0, vertices.slice(..));
                            pass.draw(0..6, range);
                        }
                        _ => panic!("unexpected test command"),
                    }
                }
            }
            encoder.copy_texture_to_buffer(
                target.as_image_copy(),
                wgpu::TexelCopyBufferInfo {
                    buffer: &readback,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(dimensions[0] * 4),
                        rows_per_image: Some(dimensions[1]),
                    },
                },
                size,
            );
            let submission = queue.submit([encoder.finish()]);
            let (sender, receiver) = std::sync::mpsc::channel();
            readback
                .slice(..)
                .map_async(wgpu::MapMode::Read, move |result| {
                    sender.send(result).unwrap();
                });
            device
                .poll(wgpu::PollType::Wait {
                    submission_index: Some(submission),
                    timeout: Some(std::time::Duration::from_secs(30)),
                })
                .unwrap();
            receiver
                .recv_timeout(std::time::Duration::from_secs(1))
                .unwrap()
                .unwrap();
            let data = readback.slice(..).get_mapped_range().unwrap();
            let pixel = |x: usize, y: usize| -> &[u8] {
                let offset = (y * dimensions[0] as usize + x) * 4;
                &data[offset..offset + 4]
            };
            for (label, x, y, expected) in [
                ("cross single alpha", 24.0, 24.0, [188, 0, 0, 255]),
                ("single arm same alpha", 30.0, 24.0, [188, 0, 0, 255]),
                ("round cap extends endpoint", 10.0, 24.0, [188, 0, 0, 255]),
                ("outside cap", 8.0, 24.0, [0, 0, 0, 255]),
                (
                    "round cap excludes square corner",
                    9.0,
                    21.0,
                    [0, 0, 0, 255],
                ),
                ("diagonal center", 60.0, 24.0, [0, 255, 0, 255]),
                ("clipped diagonal", 90.0, 18.0, [0, 0, 0, 255]),
                ("visible original diagonal", 100.0, 28.0, [0, 0, 255, 255]),
                ("degenerate dot", 24.0, 64.0, [255, 255, 0, 255]),
                ("dot excludes square corner", 28.0, 68.0, [0, 0, 0, 255]),
                (
                    "joined segments single alpha",
                    60.0,
                    64.0,
                    [188, 188, 188, 255],
                ),
                ("path above prior rectangle", 92.0, 72.0, [188, 188, 0, 255]),
                ("rectangle above prior path", 100.0, 72.0, [0, 0, 255, 255]),
            ] {
                let actual = pixel((x * scale) as usize, (y * scale) as usize);
                for (&actual, expected) in actual.iter().zip(expected) {
                    assert!(
                        i16::from(actual).abs_diff(expected) <= 2,
                        "DPI {scale} {label}: {actual}, expected {expected}"
                    );
                }
                println!("PASS DPI {scale} {label}: {actual:?}");
            }
            let diagonal_x = (60.0 * scale) as usize;
            let diagonal_y = (24.0 * scale) as usize;
            assert!(
                (1..=5).any(|offset| {
                    let green = pixel(diagonal_x + offset, diagonal_y)[1];
                    green > 0 && green < 255
                }),
                "DPI {scale}: diagonal must contain antialiased edge coverage"
            );
            assert_eq!(pixel(diagonal_x + 6, diagonal_y), [0, 0, 0, 255]);
            println!("PASS DPI {scale} diagonal antialiasing and exterior");
            drop(data);
            readback.unmap();
        }
    }
}
