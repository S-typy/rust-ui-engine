//! Bounded GPU winding fills, with one blend for all contours of a path.
use crate::{GpuError, linear_rgba, physical_rect};
use bytemuck::{Pod, Zeroable};
use rust_desktop_ui_core::{FillPath, FillRule, MAX_FILL_EDGES, Rect};
use std::ops::Range;

#[cfg(test)]
#[path = "fill_gpu_tests.rs"]
mod gpu_tests;

pub(crate) const MAX_FILL_PATHS: usize = 512;
// Keep f32 crossing arithmetic near subpixel precision. Merely preventing
// overflow would admit distant endpoints whose subtraction erases the viewport.
const MAX_PHYSICAL_FILL_COORDINATE: f32 = 1.0e6;
const SHADER: &str = include_str!("fills.wgsl");

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct FillInstance {
    position_size: [f32; 4],
    rgba: [f32; 4],
    count: u32,
    even_odd: u32,
    padding: [u32; 2],
    segments: [[f32; 4]; MAX_FILL_EDGES],
}

pub(crate) fn valid_physical_path(path: &FillPath, scale: f32) -> bool {
    path.is_valid()
        && scale.is_finite()
        && scale > 0.0
        && physical_rect(path.bounds().unwrap(), scale)
            .is_some_and(|r| r.width > 0.0 && r.height > 0.0)
        && physical_rect(path.clip, scale).is_some()
        && path.contours.iter().flatten().all(|p| {
            [p.x, p.y].into_iter().all(|v| {
                let physical = v * scale;
                physical.is_finite() && physical.abs() <= MAX_PHYSICAL_FILL_COORDINATE
            })
        })
}

fn instance(path: &FillPath, scale: f32, viewport: Rect) -> FillInstance {
    let bounds = physical_rect(path.bounds().unwrap(), scale).unwrap();
    let clip = physical_rect(path.clip, scale).unwrap();
    let visible = Rect::new(
        bounds.x - 0.5,
        bounds.y - 0.5,
        bounds.width + 1.0,
        bounds.height + 1.0,
    )
    .intersection(clip)
    .and_then(|r| r.intersection(viewport))
    .unwrap_or(Rect::new(0.0, 0.0, 0.0, 0.0));
    let mut item = FillInstance {
        position_size: [visible.x, visible.y, visible.width, visible.height],
        rgba: linear_rgba(path.color),
        count: path.edge_count().unwrap() as u32,
        even_odd: u32::from(path.rule == FillRule::EvenOdd),
        padding: [0; 2],
        segments: [[0.0; 4]; MAX_FILL_EDGES],
    };
    for (target, (a, b)) in item.segments.iter_mut().zip(path.edges()) {
        *target = [a.x * scale, a.y * scale, b.x * scale, b.y * scale];
    }
    item
}

pub(crate) struct FillRenderer {
    pipeline: wgpu::RenderPipeline,
    buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
}
impl FillRenderer {
    pub(crate) fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        viewport: &wgpu::BindGroupLayout,
    ) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ui-fill-storage-layout"),
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
            label: Some("ui-fill-paths"),
            size: (MAX_FILL_PATHS * std::mem::size_of::<FillInstance>()) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ui-fill-storage"),
            layout: &layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            }],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ui-fills"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ui-fill-pipeline-layout"),
            bind_group_layouts: &[Some(viewport), Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("ui-fills-pipeline"),
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
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
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
        paths: &[FillPath],
        scale: f32,
        viewport: Rect,
        queue: &wgpu::Queue,
    ) -> Result<(), GpuError> {
        if paths.len() > MAX_FILL_PATHS || !paths.iter().all(|p| valid_physical_path(p, scale)) {
            return Err(GpuError::InvalidScene);
        }
        if !paths.is_empty() {
            let instances: Vec<_> = paths.iter().map(|p| instance(p, scale, viewport)).collect();
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
    use rust_desktop_ui_core::{Color, Point, Scene};
    fn path() -> FillPath {
        FillPath {
            contours: vec![vec![
                Point::new(10.0, 10.0),
                Point::new(30.0, 10.0),
                Point::new(10.0, 30.0),
            ]],
            clip: Rect::new(15.0, 0.0, 100.0, 100.0),
            rule: FillRule::EvenOdd,
            color: Color::rgb(255, 0, 0),
        }
    }
    #[test]
    fn fill_upload_preserves_endpoints_and_clips_only_the_quad() {
        let p = path();
        assert!(valid_physical_path(&p, 1.5));
        let upload = instance(&p, 1.5, Rect::new(0.0, 0.0, 60.0, 60.0));
        assert_eq!(upload.segments[0], [15.0, 15.0, 45.0, 15.0]);
        assert_eq!(upload.segments[2], [15.0, 45.0, 15.0, 15.0]);
        assert_eq!(upload.position_size, [22.5, 14.5, 23.0, 31.0]);
        assert_eq!((upload.count, upload.even_odd), (3, 1));
        assert_eq!(std::mem::size_of::<FillInstance>(), 4144);
    }
    #[test]
    fn physical_bounds_and_public_array_corruption_fail_before_upload() {
        let mut boundary = path();
        boundary.contours[0][0].x = MAX_PHYSICAL_FILL_COORDINATE;
        assert!(valid_physical_path(&boundary, 1.0));
        assert!(!valid_physical_path(&boundary, 1.5));
        boundary.contours[0][0].x = MAX_PHYSICAL_FILL_COORDINATE.next_up();
        assert!(!valid_physical_path(&boundary, 1.0));
        for scale in [0.0, -1.0, f32::NAN, f32::INFINITY, f32::MAX] {
            assert!(!valid_physical_path(&path(), scale));
        }
        let mut scene = Scene::default();
        for _ in 0..MAX_FILL_PATHS {
            scene.fill_path(path()).unwrap();
        }
        assert!(crate::check_scene(&scene, 1.0).is_ok());
        scene.fill_path(path()).unwrap();
        assert!(crate::check_scene(&scene, 1.0).is_err());
        let mut scene = Scene::default();
        scene.fill_path(path()).unwrap();
        scene.fill_paths[0].contours[0][0].x = 1.0e19;
        assert!(crate::check_scene(&scene, 1.0).is_err());
        scene.fill_paths.clear();
        assert!(crate::check_scene(&scene, 1.0).is_err());
    }
}
