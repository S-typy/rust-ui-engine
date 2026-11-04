use super::*;
use crate::{
    DrawBatch, RectInstance, ViewportUniform, check_adapter, check_scene, next_batch,
    rectangle_pipeline,
};
use rust_desktop_ui_core::{Color, Point, Scene};
use std::sync::Arc;
use wgpu::util::DeviceExt;
fn polygon(points: &[(f32, f32)]) -> Vec<Point> {
    points.iter().map(|&(x, y)| Point::new(x, y)).collect()
}
fn square(x: f32, y: f32, size: f32) -> Vec<Point> {
    polygon(&[(x, y), (x + size, y), (x + size, y + size), (x, y + size)])
}
#[test]
#[ignore = "requires a native hardware GPU"]
fn filled_contours_gpu_holes_overlap_clip_dpi_order() {
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
        "fill-path hardware: {} {:?} {:?}",
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
    let fills = FillRenderer::new(&device, format, &viewport_layout);
    let rectangles = rectangle_pipeline(&device, format, &viewport_layout);
    let mut scene = Scene::default();
    let clip = Rect::new(0.0, 0.0, 128.0, 96.0);
    let red = Color {
        a: 0.5,
        ..Color::rgb(255, 0, 0)
    };
    // Same direction overlap: both the middle and internal contour edge stay at alpha .5.
    scene
        .fill_path(FillPath {
            contours: vec![square(8.0, 8.0, 32.0), square(16.0, 16.0, 16.0)],
            clip,
            rule: FillRule::NonZero,
            color: red,
        })
        .unwrap();
    // Same geometry, even-odd hole.
    scene
        .fill_path(FillPath {
            contours: vec![square(48.0, 8.0, 32.0), square(56.0, 16.0, 16.0)],
            clip,
            rule: FillRule::EvenOdd,
            color: red,
        })
        .unwrap();
    // Reversed inner contour must also produce a hole with nonzero winding.
    let mut inner = square(96.0, 16.0, 16.0);
    inner.reverse();
    scene
        .fill_path(FillPath {
            contours: vec![square(88.0, 8.0, 32.0), inner],
            clip,
            rule: FillRule::NonZero,
            color: red,
        })
        .unwrap();
    scene
        .fill_path(FillPath {
            contours: vec![polygon(&[(8.0, 56.0), (40.0, 56.0), (8.0, 88.0)])],
            clip,
            rule: FillRule::NonZero,
            color: Color::rgb(0, 255, 0),
        })
        .unwrap();
    scene
        .fill_path(FillPath {
            contours: vec![square(48.0, 56.0, 32.0)],
            clip: Rect::new(60.0, 52.0, 20.0, 40.0),
            rule: FillRule::NonZero,
            color: Color::rgb(0, 0, 255),
        })
        .unwrap();
    scene.fill(Rect::new(88.0, 56.0, 32.0, 32.0), Color::rgb(0, 255, 0));
    scene
        .fill_path(FillPath {
            contours: vec![square(88.0, 56.0, 32.0)],
            clip,
            rule: FillRule::NonZero,
            color: red,
        })
        .unwrap();
    scene.fill(Rect::new(100.0, 56.0, 4.0, 32.0), Color::rgb(0, 0, 255));
    let mut reversed = square(4.0, 42.0, 8.0);
    reversed.reverse();
    scene
        .fill_path(FillPath {
            contours: vec![square(4.0, 42.0, 8.0), reversed],
            clip,
            rule: FillRule::NonZero,
            color: Color::rgb(255, 255, 255),
        })
        .unwrap();
    let mut maximum = vec![Point::new(44.0, 42.0); 253];
    maximum.extend([
        Point::new(54.0, 42.0),
        Point::new(54.0, 52.0),
        Point::new(44.0, 52.0),
    ]);
    scene
        .fill_path(FillPath {
            contours: vec![maximum],
            clip,
            rule: FillRule::NonZero,
            color: Color::rgb(0, 255, 255),
        })
        .unwrap();
    scene
        .fill_path(FillPath {
            contours: vec![polygon(&[
                (-250000.0, -250040.0),
                (250000.0, 249960.0),
                (250000.0, -250040.0),
            ])],
            clip: Rect::new(80.0, 42.0, 40.0, 12.0),
            rule: FillRule::NonZero,
            color: Color::rgb(255, 255, 255),
        })
        .unwrap();
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
        fills
            .prepare(&scene.fill_paths, scale, viewport, &queue)
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
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
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
                    DrawBatch::FillPaths(range) => fills.draw(&mut pass, range),
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
            ("cancelled contours", 8.0, 46.0, [0, 0, 0, 255]),
            ("cancelled edge", 4.0, 46.0, [0, 0, 0, 255]),
            (
                "256 edges and degenerate segments",
                49.0,
                47.0,
                [0, 255, 255, 255],
            ),
            ("closing last edge", 43.0, 47.0, [0, 0, 0, 255]),
            (
                "distant endpoints inside",
                100.0,
                46.0,
                [255, 255, 255, 255],
            ),
            ("distant endpoints outside", 83.0, 52.0, [0, 0, 0, 255]),
            (
                "distant diagonal subpixel coverage",
                88.0,
                48.0,
                [188, 188, 188, 255],
            ),
            ("overlap single blend", 24.0, 24.0, [188, 0, 0, 255]),
            ("internal edge has no seam", 16.0, 24.0, [188, 0, 0, 255]),
            ("outside", 4.0, 24.0, [0, 0, 0, 255]),
            ("even odd hole", 64.0, 24.0, [0, 0, 0, 255]),
            ("even odd ring", 51.0, 24.0, [188, 0, 0, 255]),
            ("nonzero reversed hole", 104.0, 24.0, [0, 0, 0, 255]),
            ("nonzero ring", 91.0, 24.0, [188, 0, 0, 255]),
            ("triangle inside", 12.0, 60.0, [0, 255, 0, 255]),
            ("triangle outside", 35.0, 80.0, [0, 0, 0, 255]),
            ("clip excludes", 55.0, 64.0, [0, 0, 0, 255]),
            ("clip preserves path", 65.0, 64.0, [0, 0, 255, 255]),
            ("path above rectangle", 92.0, 64.0, [188, 188, 0, 255]),
            ("rectangle above path", 102.0, 64.0, [0, 0, 255, 255]),
        ] {
            let actual = pixel((x * scale) as usize, (y * scale) as usize);
            for (&a, e) in actual.iter().zip(expected) {
                assert!(
                    i16::from(a).abs_diff(e) <= 2,
                    "DPI {scale} {label}: {actual:?}"
                );
            }
            println!("PASS DPI {scale} {label}: {actual:?}");
        }
        let x = (24.0 * scale) as usize;
        let y = (72.0 * scale) as usize;
        assert!(
            (-2..=2).any(|dy| {
                let g = pixel(x, (y as i32 + dy) as usize)[1];
                g > 0 && g < 255
            }),
            "triangle diagonal antialiasing"
        );
        drop(data);
        readback.unmap();
    }
}
