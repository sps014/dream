use super::{Transfer, context::failure};
use crate::{DrawingError, Result, gpu::GpuSurface, gpu_color::ColorFilter};
use ash::vk;
use dream_host_abi::drawing_gpu::Target;
use std::sync::Arc;
use wgpu::hal::api::Vulkan;

pub(super) struct Allocation {
    pub raw: ash::Device,
    pub image: vk::Image,
    memory: vk::DeviceMemory,
    pub semaphore: vk::Semaphore,
    pub family: u32,
    _device: wgpu::Device,
}
impl Drop for Allocation {
    fn drop(&mut self) {
        unsafe {
            self.raw.destroy_semaphore(self.semaphore, None);
            self.raw.destroy_image(self.image, None);
            self.raw.free_memory(self.memory, None);
        }
    }
}

pub(super) unsafe fn import(surface: &GpuSurface, target: &Target) -> Result<Transfer> {
    let hal = unsafe { surface.device.as_hal::<Vulkan>() }
        .ok_or_else(|| DrawingError::Render("missing Vulkan device".into()))?;
    let raw = hal.raw_device().clone();
    let allocation = Allocation {
        raw: raw.clone(),
        image: vk::Image::null(),
        memory: vk::DeviceMemory::null(),
        semaphore: vk::Semaphore::null(),
        family: hal.queue_family_index(),
        _device: surface.device.clone(),
    };
    let mut allocation = allocation;
    let bgra = target.format >= 2;
    let format = if bgra {
        wgpu::TextureFormat::Bgra8Unorm
    } else {
        wgpu::TextureFormat::Rgba8Unorm
    };
    let mut external = vk::ExternalMemoryImageCreateInfo::default()
        .handle_types(vk::ExternalMemoryHandleTypeFlags::OPAQUE_WIN32);
    let usage = vk::ImageUsageFlags::TRANSFER_SRC
        | vk::ImageUsageFlags::TRANSFER_DST
        | if bgra {
            vk::ImageUsageFlags::COLOR_ATTACHMENT
        } else {
            vk::ImageUsageFlags::empty()
        };
    let info = vk::ImageCreateInfo::default()
        .push_next(&mut external)
        .image_type(vk::ImageType::TYPE_2D)
        .format(if bgra {
            vk::Format::B8G8R8A8_UNORM
        } else {
            vk::Format::R8G8B8A8_UNORM
        })
        .extent(vk::Extent3D {
            width: target.width,
            height: target.height,
            depth: 1,
        })
        .mip_levels(1)
        .array_layers(1)
        .samples(vk::SampleCountFlags::TYPE_1)
        .tiling(vk::ImageTiling::OPTIMAL)
        .usage(usage)
        .sharing_mode(vk::SharingMode::EXCLUSIVE)
        .initial_layout(vk::ImageLayout::UNDEFINED);
    allocation.image = unsafe { raw.create_image(&info, None) }.map_err(failure)?;
    let requirements = unsafe { raw.get_image_memory_requirements(allocation.image) };
    if target.memory_type >= 32
        || requirements.memory_type_bits & (1 << target.memory_type) == 0
        || target.memory_size < requirements.size
        || target.memory_size > 256 * 1024 * 1024
    {
        return Err(DrawingError::Invalid(
            "invalid Vulkan drawing memory requirements".into(),
        ));
    }
    let mut imported = vk::ImportMemoryWin32HandleInfoKHR::default()
        .handle_type(vk::ExternalMemoryHandleTypeFlags::OPAQUE_WIN32)
        .handle(target.texture as isize);
    let mut dedicated = vk::MemoryDedicatedAllocateInfo::default().image(allocation.image);
    let info = vk::MemoryAllocateInfo::default()
        .allocation_size(target.memory_size)
        .memory_type_index(target.memory_type)
        .push_next(&mut imported)
        .push_next(&mut dedicated);
    allocation.memory = unsafe { raw.allocate_memory(&info, None) }.map_err(failure)?;
    unsafe { raw.bind_image_memory(allocation.image, allocation.memory, 0) }.map_err(failure)?;
    let mut timeline =
        vk::SemaphoreTypeCreateInfo::default().semaphore_type(vk::SemaphoreType::TIMELINE);
    allocation.semaphore = unsafe {
        raw.create_semaphore(
            &vk::SemaphoreCreateInfo::default().push_next(&mut timeline),
            None,
        )
    }
    .map_err(failure)?;
    let extension =
        ash::khr::external_semaphore_win32::Device::new(hal.shared_instance().raw_instance(), &raw);
    let info = vk::ImportSemaphoreWin32HandleInfoKHR::default()
        .semaphore(allocation.semaphore)
        .handle_type(vk::ExternalSemaphoreHandleTypeFlags::OPAQUE_WIN32)
        .handle(target.fence as isize);
    unsafe { extension.import_semaphore_win32_handle(&info) }.map_err(failure)?;
    let allocation = Arc::new(allocation);
    let retained = allocation.clone();
    let hal_desc = wgpu::hal::TextureDescriptor {
        label: Some("Dream Vulkan drawing transfer"),
        size: surface.target.size(),
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: if bgra {
            wgpu::TextureUses::COLOR_TARGET
        } else {
            wgpu::TextureUses::COPY_DST
        },
        memory_flags: wgpu::hal::MemoryFlags::empty(),
        view_formats: vec![],
    };
    let imported = unsafe {
        hal.texture_from_raw(
            allocation.image,
            &hal_desc,
            Some(Box::new(move || drop(retained))),
            wgpu::hal::vulkan::TextureMemory::External,
        )
    };
    drop(hal);
    let texture = unsafe {
        surface.device.create_texture_from_hal::<Vulkan>(
            imported,
            &wgpu::TextureDescriptor {
                label: hal_desc.label,
                size: hal_desc.size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: hal_desc.dimension,
                format,
                usage: if bgra {
                    wgpu::TextureUsages::RENDER_ATTACHMENT
                } else {
                    wgpu::TextureUsages::COPY_DST
                },
                view_formats: &[],
            },
            wgpu::TextureUses::STORAGE_READ_ONLY,
        )
    };
    let blit = bgra.then(|| {
        (
            ColorFilter::copy(&surface.device),
            surface.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Dream Vulkan copy parameters"),
                size: 16,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
        )
    });
    Ok(Transfer {
        id: target.id,
        texture,
        allocation,
        blit,
    })
}
