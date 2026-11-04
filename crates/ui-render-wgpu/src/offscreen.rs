//! Hardware rendering of the same retained Scene into an opaque bitmap.
use crate::*;
use rust_desktop_ui_core::{Bitmap, MAX_BITMAP_BYTES, MAX_BITMAP_DIMENSION};
use std::time::Duration;

#[cfg(test)]
#[path = "offscreen_gpu_tests.rs"]
mod gpu_tests;

/// Reusable windowless GPU renderer for document output and image export.
/// Uses the same frame pipelines, clipping, blending and limits as native windows.
/// The white background makes the returned straight-alpha sRGB bitmap opaque.
/// Call from a worker: readback may block for up to thirty seconds per submission.
pub struct OffscreenRenderer {
    _instance: wgpu::Instance,
    device: wgpu::Device,
    queue: wgpu::Queue,
    frame: frame::FrameRenderer,
    info: wgpu::AdapterInfo,
    health: Arc<Mutex<DeviceHealth>>,
    stats: RendererStats,
}

fn dimensions(size: [u32; 2], device_limit: u32) -> Result<(u32, u64), GpuError> {
    let [width, height] = size;
    let bytes = u64::from(width)
        .checked_mul(u64::from(height))
        .and_then(|n| n.checked_mul(4));
    if width == 0
        || height == 0
        || width > device_limit.min(MAX_BITMAP_DIMENSION)
        || height > device_limit.min(MAX_BITMAP_DIMENSION)
        || bytes.is_none_or(|n| n > MAX_BITMAP_BYTES as u64)
    {
        return Err(GpuError::Image(
            "output exceeds nonzero 8192px / 64MiB bitmap limits".into(),
        ));
    }
    let stride = (width * 4).div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
        * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    Ok((stride, u64::from(stride) * u64::from(height)))
}

impl OffscreenRenderer {
    /// Opens a hardware adapter. Software/Noop backends are rejected.
    pub async fn new() -> Result<Self, GpuError> {
        let descriptor = wgpu::InstanceDescriptor::new_without_display_handle_from_env();
        if !descriptor
            .backends
            .intersects(wgpu::Instance::enabled_backend_features())
        {
            return Err(GpuError::RequestAdapter(
                "selected backend is unavailable".into(),
            ));
        }
        let instance = wgpu::Instance::new(descriptor);
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                force_fallback_adapter: false,
                ..Default::default()
            })
            .await
            .map_err(|e| GpuError::RequestAdapter(e.to_string()))?;
        let info = adapter.get_info();
        check_adapter(info.device_type, info.backend, &info.name)?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default())
            .await
            .map_err(|e| GpuError::RequestDevice(e.to_string()))?;
        let health = Arc::new(Mutex::new(DeviceHealth::default()));
        let errors = health.clone();
        device.on_uncaptured_error(Arc::new(move |error: wgpu::Error| {
            errors
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .error
                .get_or_insert_with(|| error.to_string());
        }));
        let lost = health.clone();
        device.set_device_lost_callback(move |reason, message| {
            lost.lock().unwrap_or_else(|p| p.into_inner()).lost =
                Some(format!("{reason:?}: {message}"));
        });
        let frame = frame::FrameRenderer::new(&device, wgpu::TextureFormat::Rgba8UnormSrgb);
        let renderer = Self {
            _instance: instance,
            device,
            queue,
            frame,
            info,
            health,
            stats: RendererStats::default(),
        };
        renderer.check_device()?;
        Ok(renderer)
    }
    pub fn adapter_info(&self) -> &wgpu::AdapterInfo {
        &self.info
    }
    pub fn stats(&self) -> RendererStats {
        self.stats
    }
    /// Preflight physical output dimensions without allocating GPU resources.
    pub fn validate_size(size: [u32; 2]) -> Result<(), GpuError> {
        dimensions(size, MAX_BITMAP_DIMENSION).map(|_| ())
    }
    fn check_device(&self) -> Result<(), GpuError> {
        let health = self.health.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(message) = &health.lost {
            return Err(GpuError::DeviceLost(message.clone()));
        }
        if let Some(message) = &health.error {
            return Err(GpuError::Backend(message.clone()));
        }
        Ok(())
    }

    /// Render logical Scene coordinates at `scale` into physical pixel dimensions.
    /// Invalid input returns before submission; success returns a new bitmap ID.
    pub fn render(
        &mut self,
        scene: &Scene,
        size: [u32; 2],
        scale: f32,
    ) -> Result<Bitmap, GpuError> {
        self.check_device()?;
        let (stride, bytes) = dimensions(size, self.device.limits().max_texture_dimension_2d)?;
        self.frame
            .prepare(scene, scale, size, &self.device, &self.queue)?;
        let extent = wgpu::Extent3d {
            width: size[0],
            height: size[1],
            depth_or_array_layers: 1,
        };
        let target = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("ui-offscreen-output"),
            size: extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = target.create_view(&wgpu::TextureViewDescriptor::default());
        let readback = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ui-offscreen-readback"),
            size: bytes,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("ui-offscreen-frame"),
            });
        let stats = self.frame.encode(scene, &view, &mut encoder, 1)?;
        encoder.copy_texture_to_buffer(
            target.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(stride),
                    rows_per_image: Some(size[1]),
                },
            },
            extent,
        );
        let submission = self.queue.submit([encoder.finish()]);
        let (sender, receiver) = std::sync::mpsc::channel();
        readback
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                let _ = sender.send(result);
            });
        self.device
            .poll(wgpu::PollType::Wait {
                submission_index: Some(submission),
                timeout: Some(Duration::from_secs(30)),
            })
            .map_err(|e| GpuError::Backend(format!("offscreen completion: {e}")))?;
        self.check_device()?;
        receiver
            .recv_timeout(Duration::from_secs(1))
            .map_err(|e| GpuError::Backend(e.to_string()))?
            .map_err(|e| GpuError::Backend(e.to_string()))?;
        let mapped = readback
            .slice(..)
            .get_mapped_range()
            .map_err(|e| GpuError::Backend(e.to_string()))?;
        let mut pixels = Vec::with_capacity(size[0] as usize * size[1] as usize * 4);
        for row in mapped.chunks_exact(stride as usize) {
            pixels.extend_from_slice(&row[..size[0] as usize * 4]);
        }
        drop(mapped);
        readback.unmap();
        let bitmap = Bitmap::new_rgba8(size[0], size[1], pixels)
            .map_err(|e| GpuError::Image(e.to_string()))?;
        self.stats = stats;
        Ok(bitmap)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn output_limits_bound_allocation_and_padded_readback() {
        assert_eq!(dimensions([65, 2], 8192).unwrap(), (512, 1024));
        assert_eq!(dimensions([4096, 4096], 8192).unwrap(), (16384, 67108864));
        for size in [
            [0, 1],
            [1, 0],
            [8193, 1],
            [4097, 4096],
            [u32::MAX, u32::MAX],
        ] {
            assert!(dimensions(size, 8192).is_err());
        }
        assert!(dimensions([1025, 1], 1024).is_err());
    }
}
