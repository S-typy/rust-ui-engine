//! Explicit hardware tests; never silently pass on a software adapter.
use super::*;
use rust_desktop_ui_core::{Bitmap, ImagePrimitive, ImageSampling, Point};
use std::time::Duration;

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
    let start = std::time::Instant::now();
    loop {
        if let std::task::Poll::Ready(value) = future.as_mut().poll(&mut context) {
            return value;
        }
        assert!(
            start.elapsed() < Duration::from_secs(30),
            "GPU initialization timeout"
        );
        std::thread::park_timeout(Duration::from_millis(10));
    }
}

fn test_scene() -> Scene {
    let mut scene = Scene::default();
    let pixel = |rgba| Bitmap::new_rgba8(1, 1, rgba).unwrap();
    // Linear blending: half-red image above opaque blue.
    scene.fill(Rect::new(0.0, 0.0, 16.0, 16.0), Color::rgb(0, 0, 255));
    scene.image(ImagePrimitive::new(
        pixel(vec![255, 0, 0, 128]),
        Rect::new(0.0, 0.0, 16.0, 16.0),
    ));
    // Later solid must remain above that image.
    scene.fill(Rect::new(0.0, 0.0, 4.0, 4.0), Color::rgb(0, 255, 0));
    let stripe = Bitmap::new_rgba8(2, 1, vec![255, 0, 0, 255, 0, 255, 0, 0]).unwrap();
    let mut image = ImagePrimitive::new(stripe.clone(), Rect::new(16.0, 0.0, 16.0, 16.0));
    image.sampling = ImageSampling::Nearest;
    scene.image(image);
    // Filtered transparent green must not contaminate red.
    scene.image(ImagePrimitive::new(
        stripe,
        Rect::new(32.0, 0.0, 16.0, 16.0),
    ));
    // Crop the middle white pixel, without bleeding adjacent black/red pixels.
    let mut cropped = ImagePrimitive::new(
        Bitmap::new_rgba8(3, 1, vec![0, 0, 0, 255, 255, 255, 255, 255, 255, 0, 0, 255]).unwrap(),
        Rect::new(48.0, 0.0, 16.0, 16.0),
    );
    cropped.source = Rect::new(1.0, 0.0, 1.0, 1.0);
    scene.image(cropped);
    let mut view = Scene::default();
    let mut clipped = ImagePrimitive::new(pixel(vec![255; 4]), Rect::new(0.0, 0.0, 16.0, 8.0));
    clipped.opacity = 0.25;
    view.image(clipped);
    scene
        .append_transformed_view(
            &view,
            Point::new(4.0, 20.0),
            2.0,
            Rect::new(12.0, 20.0, 16.0, 16.0),
        )
        .unwrap();
    scene
}

fn readback_scene(device: &wgpu::Device, queue: &wgpu::Queue, scene: &Scene) -> Vec<u8> {
    check_scene(scene, 1.0).unwrap();
    let viewport = Rect::new(0.0, 0.0, 64.0, 64.0);
    let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("image-test-viewport"),
        contents: bytemuck::bytes_of(&ViewportUniform {
            dimensions: [64.0, 64.0],
            padding: [0.0; 2],
        }),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("image-test-layout"),
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
    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: buffer.as_entire_binding(),
        }],
    });
    let mut images = ImageRenderer::new(device, wgpu::TextureFormat::Rgba8UnormSrgb, &layout);
    images
        .prepare(&scene.images, 1.0, viewport, device, queue)
        .unwrap();
    assert_eq!(images.uploaded, 4);
    images
        .prepare(&scene.images, 1.0, viewport, device, queue)
        .unwrap();
    assert_eq!(
        images.uploaded, 0,
        "immutable resources must be reused on the second frame"
    );
    let pipeline = rectangle_pipeline(device, wgpu::TextureFormat::Rgba8UnormSrgb, &layout);
    let instances: Vec<_> = scene
        .rectangles
        .iter()
        .map(|r| RectInstance {
            position_size: [r.bounds.x, r.bounds.y, r.bounds.width, r.bounds.height],
            rgba: linear_rgba(r.color),
            shape: [0.0; 4],
            radius: -1.0,
            stroke_width: -1.0,
        })
        .collect();
    let vertices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: bytemuck::cast_slice(&instances),
        usage: wgpu::BufferUsages::VERTEX,
    });
    let size = wgpu::Extent3d {
        width: 64,
        height: 64,
        depth_or_array_layers: 1,
    };
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("image-test-target"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = target.create_view(&Default::default());
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 64 * 64 * 4,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("image-test-pass"),
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
        pass.set_bind_group(0, &group, &[]);
        let mut commands = scene.commands().iter().copied().peekable();
        while let Some(batch) = next_batch(&mut commands, |_| None).unwrap() {
            match batch {
                DrawBatch::Rectangles(range) => {
                    pass.set_pipeline(&pipeline);
                    pass.set_vertex_buffer(0, vertices.slice(..));
                    pass.draw(0..6, range);
                }
                DrawBatch::Image(index) => {
                    images.draw(&mut pass, index);
                }
                _ => unreachable!(),
            }
        }
    }
    encoder.copy_texture_to_buffer(
        target.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(256),
                rows_per_image: Some(64),
            },
        },
        size,
    );
    let submission = queue.submit([encoder.finish()]);
    let (tx, rx) = std::sync::mpsc::channel();
    readback
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            tx.send(result).unwrap();
        });
    device
        .poll(wgpu::PollType::Wait {
            submission_index: Some(submission),
            timeout: Some(Duration::from_secs(30)),
        })
        .unwrap();
    rx.recv_timeout(Duration::from_secs(2)).unwrap().unwrap();
    let data = readback.slice(..).get_mapped_range().unwrap().to_vec();
    readback.unmap();
    for (label, x, y, expected) in [
        ("painter order overlay", 1, 1, [0, 255, 0, 255]),
        ("half-alpha red over blue", 8, 8, [188, 0, 187, 255]),
        ("nearest opaque", 19, 8, [255, 0, 0, 255]),
        ("nearest transparent", 29, 8, [0, 0, 0, 255]),
        ("linear no green fringe", 39, 8, [198, 0, 0, 255]),
        ("source crop first", 48, 8, [255, 255, 255, 255]),
        ("source crop last", 63, 8, [255, 255, 255, 255]),
        ("scaled image left clip", 11, 24, [0, 0, 0, 255]),
        ("scaled opacity", 12, 24, [137, 137, 137, 255]),
        ("scaled image right clip", 28, 24, [0, 0, 0, 255]),
    ] {
        let offset = (y * 64 + x) * 4;
        let actual = &data[offset..offset + 4];
        for (actual, expected) in actual.iter().zip(expected) {
            assert!(
                i16::from(*actual).abs_diff(expected) <= 2,
                "{label} channel {actual}, expected {expected}"
            );
        }
        println!("PASS {label}: {actual:?}");
    }
    images.prepare(&[], 1.0, viewport, device, queue).unwrap();
    assert_eq!(images.cached(), 0, "absent textures must be released");
    data
}

#[test]
#[ignore = "requires a native hardware GPU"]
fn bitmap_gpu_sampling_compositing_and_controlled_device_recovery() {
    let instance =
        wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let adapter = block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        force_fallback_adapter: false,
        ..Default::default()
    }))
    .unwrap();
    let info = adapter.get_info();
    check_adapter(info.device_type, info.backend, &info.name).unwrap();
    println!(
        "bitmap/recovery hardware: {} {:?} {:?}",
        info.name, info.backend, info.device_type
    );
    let scene = test_scene();
    let (device, queue) = block_on(adapter.request_device(&Default::default())).unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    device.set_device_lost_callback(move |reason, message| {
        let _ = tx.send((reason, message));
    });
    let before = readback_scene(&device, &queue, &scene);
    device.destroy();
    let _ = device.poll(wgpu::PollType::Poll);
    let (reason, message) = rx
        .recv_timeout(Duration::from_secs(10))
        .expect("real device-loss callback after controlled destroy");
    println!("controlled device destruction callback: {reason:?}: {message}");
    drop(queue);
    drop(device);
    let (device, queue) = block_on(adapter.request_device(&Default::default())).unwrap();
    let after = readback_scene(&device, &queue, &scene);
    assert_eq!(
        before, after,
        "retained Scene must re-upload identically on a replacement device"
    );
    println!("PASS exact pixels after device recreation; physical driver loss not simulated");
}
