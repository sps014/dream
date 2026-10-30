use crate::{
    DrawingError, Result, Surface, effects::Effect, gpu::GpuSurface, gpu_color::ColorFilter,
};
use dream_host_abi::drawing_gpu::Target;
use windows::Win32::{
    Foundation::HANDLE,
    Graphics::Direct3D12::{ID3D12Fence, ID3D12Resource},
};

pub(super) struct Transfer {
    id: u64,
    texture: wgpu::Texture,
    fence: ID3D12Fence,
    blit: Option<(ColorFilter, wgpu::Buffer)>,
}

fn failure(error: windows::core::Error) -> DrawingError {
    DrawingError::Render(error.to_string())
}

impl Surface {
    /// # Safety
    /// Shared handles must remain valid through this call and describe the supplied target.
    /// The generation must uniquely identify this texture/fence pair and never be recycled.
    pub unsafe fn copy_native(&mut self, target: &Target) -> Result<bool> {
        match self {
            Self::Cpu(_) => Ok(false),
            Self::Gpu(surface) => unsafe { surface.copy_shared(target) },
        }
    }
}

impl GpuSurface {
    unsafe fn copy_shared(&mut self, target: &Target) -> Result<bool> {
        self.check()?;
        let Some(device) = (unsafe { self.device.as_hal::<wgpu::hal::api::Dx12>() }) else {
            return Ok(false);
        };
        let adapter = unsafe { device.raw_device().GetAdapterLuid() };
        let adapter = (u64::from(adapter.HighPart as u32) << 32) | u64::from(adapter.LowPart);
        if target.adapter != adapter {
            return Ok(false);
        }
        if target.width != u32::from(self.width)
            || target.height != u32::from(self.height)
            || target.format > 3
            || target.texture == 0
            || target.fence == 0
        {
            return Err(DrawingError::Invalid(
                "invalid shared drawing target".into(),
            ));
        }
        if self
            .transfer
            .as_ref()
            .is_none_or(|transfer| transfer.id != target.id)
        {
            let mut resource: Option<ID3D12Resource> = None;
            let mut fence: Option<ID3D12Fence> = None;
            unsafe {
                device
                    .raw_device()
                    .OpenSharedHandle(HANDLE(target.texture as *mut _), &mut resource)
                    .map_err(failure)?;
                device
                    .raw_device()
                    .OpenSharedHandle(HANDLE(target.fence as *mut _), &mut fence)
                    .map_err(failure)?;
            }
            let resource =
                resource.ok_or_else(|| DrawingError::Render("missing shared texture".into()))?;
            let fence = fence.ok_or_else(|| DrawingError::Render("missing shared fence".into()))?;
            let bgra = target.format >= 2;
            let format = if bgra {
                wgpu::TextureFormat::Bgra8Unorm
            } else {
                wgpu::TextureFormat::Rgba8Unorm
            };
            let size = self.target.size();
            let raw = unsafe {
                wgpu::hal::dx12::Device::texture_from_raw(
                    resource,
                    format,
                    wgpu::TextureDimension::D2,
                    size,
                    1,
                    1,
                )
            };
            let texture = unsafe {
                self.device.create_texture_from_hal::<wgpu::hal::api::Dx12>(
                    raw,
                    &wgpu::TextureDescriptor {
                        label: Some("Dream shared drawing transfer"),
                        size,
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: wgpu::TextureDimension::D2,
                        format,
                        usage: if bgra {
                            wgpu::TextureUsages::RENDER_ATTACHMENT
                        } else {
                            wgpu::TextureUsages::COPY_DST
                        },
                        view_formats: &[],
                    },
                    wgpu::TextureUses::PRESENT,
                )
            };
            let blit = bgra.then(|| {
                (
                    ColorFilter::copy(&self.device),
                    self.device.create_buffer(&wgpu::BufferDescriptor {
                        label: Some("Dream copy parameters"),
                        size: 16,
                        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                        mapped_at_creation: false,
                    }),
                )
            });
            self.transfer = Some(Transfer {
                id: target.id,
                texture,
                fence,
                blit,
            });
        }
        drop(device);
        let transfer = self
            .transfer
            .as_ref()
            .ok_or_else(|| DrawingError::Render("missing transfer".into()))?;
        let mut encoder = self.device.create_command_encoder(&Default::default());
        let mut commands = Vec::with_capacity(2);
        if let Some((blit, parameters)) = &transfer.blit {
            commands.push(blit.encode(
                &self.device,
                &self.queue,
                &self.target,
                &transfer.texture,
                parameters,
                &Effect::Brightness(1.),
            ));
        } else {
            encoder.copy_texture_to_texture(
                self.target.as_image_copy(),
                transfer.texture.as_image_copy(),
                self.target.size(),
            );
        }
        encoder.transition_resources(
            std::iter::empty(),
            std::iter::once(wgpu::TextureTransition {
                texture: &transfer.texture,
                selector: None,
                state: wgpu::TextureUses::PRESENT,
            }),
        );
        commands.push(encoder.finish());
        // These fences belong to the next submit on the actual render queue, not its presentation queue.
        {
            let queue = unsafe { self.queue.as_hal::<wgpu::hal::api::Dx12>() }
                .ok_or_else(|| DrawingError::Unavailable("missing D3D12 queue".into()))?;
            queue.add_wait_fence(transfer.fence.clone(), target.wait_value);
            queue.add_signal_fence(transfer.fence.clone(), target.signal_value);
        }
        self.queue.submit(commands);
        self.check()?;
        Ok(true)
    }
}
