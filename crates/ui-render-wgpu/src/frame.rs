//! Shared painter-ordered frame resources for native surfaces and bitmap output.
use crate::*;

pub(crate) struct FrameRenderer {
    pipeline: wgpu::RenderPipeline,
    viewport_buffer: wgpu::Buffer,
    viewport_bind_group: wgpu::BindGroup,
    rectangles_buffer: wgpu::Buffer,
    glyphs: GlyphRenderer,
    strokes: StrokeRenderer,
    fills: FillRenderer,
    images: ImageRenderer,
}
impl FrameRenderer {
    pub(crate) fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let viewport_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("ui-viewport"),
            contents: bytemuck::bytes_of(&ViewportUniform {
                dimensions: [1.0, 1.0],
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
        let pipeline = rectangle_pipeline(device, format, &viewport_layout);
        let rectangles_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ui-batched-rectangles"),
            size: (MAX_RECTANGLES * std::mem::size_of::<RectInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let glyphs = GlyphRenderer::new(device, format, &viewport_layout);
        let strokes = StrokeRenderer::new(device, format, &viewport_layout);
        let fills = FillRenderer::new(device, format, &viewport_layout);
        let images = ImageRenderer::new(device, format, &viewport_layout);
        Self {
            pipeline,
            viewport_buffer,
            viewport_bind_group,
            rectangles_buffer,
            glyphs,
            strokes,
            fills,
            images,
        }
    }
    pub(crate) fn prepare(
        &mut self,
        scene: &Scene,
        scale_factor: f32,
        dimensions: [u32; 2],
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Result<usize, GpuError> {
        let [width, height] = dimensions;
        check_scene(scene, scale_factor)?;
        let viewport = Rect::new(0.0, 0.0, width as f32, height as f32);
        self.glyphs.prepare(scene, scale_factor, viewport, queue)?;
        self.strokes
            .prepare(&scene.stroke_paths, scale_factor, viewport, queue)?;
        self.fills
            .prepare(&scene.fill_paths, scale_factor, viewport, queue)?;
        self.images
            .prepare(&scene.images, scale_factor, viewport, device, queue)?;
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
                    stroke_width: -1.0,
                }
            })
            .collect();
        instances.extend(
            scene
                .rounded_rectangles
                .iter()
                .map(|rectangle| rounded_instance(rectangle, scale_factor, viewport)),
        );
        instances.extend(
            scene
                .ellipses
                .iter()
                .map(|item| ellipse_instance(item, scale_factor, viewport)),
        );
        let dimensions = ViewportUniform {
            dimensions: [width as f32, height as f32],
            padding: [0.0; 2],
        };
        queue.write_buffer(&self.viewport_buffer, 0, bytemuck::bytes_of(&dimensions));
        if !instances.is_empty() {
            queue.write_buffer(&self.rectangles_buffer, 0, bytemuck::cast_slice(&instances));
        }
        Ok(instances.len())
    }
    pub(crate) fn encode(
        &self,
        scene: &Scene,
        view: &wgpu::TextureView,
        encoder: &mut wgpu::CommandEncoder,
        generation: u64,
    ) -> Result<RendererStats, GpuError> {
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("ui-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
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
                    DrawBatch::Image(index) => {
                        if self.images.draw(&mut pass, index) {
                            draw_calls += 1;
                        }
                    }
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
                    DrawBatch::Ellipses(range) => {
                        let offset =
                            (scene.rectangles.len() + scene.rounded_rectangles.len()) as u32;
                        pass.set_pipeline(&self.pipeline);
                        pass.set_vertex_buffer(0, self.rectangles_buffer.slice(..));
                        pass.draw(0..6, range.start + offset..range.end + offset);
                        draw_calls += 1;
                    }
                    DrawBatch::StrokePaths(range) => {
                        self.strokes.draw(&mut pass, range);
                        draw_calls += 1;
                    }
                    DrawBatch::FillPaths(range) => {
                        self.fills.draw(&mut pass, range);
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
            Ok(RendererStats {
                device_generation: generation,
                images: scene.images.len(),
                cached_images: self.images.cached(),
                uploaded_images: self.images.uploaded,
                rectangles: scene.rectangles.len()
                    + scene.rounded_rectangles.len()
                    + scene.ellipses.len(),
                stroke_paths: scene.stroke_paths.len(),
                fill_paths: scene.fill_paths.len(),
                glyphs: self.glyphs.len(),
                text_runs: scene.texts.len(),
                cached_glyphs: self.glyphs.cached(),
                missing_glyphs: self.glyphs.missing_glyphs,
                draw_calls,
            })
        }
    }
}
