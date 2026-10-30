mod context;
mod import;

use crate::{
    DrawingError, Picture, Result, Surface, effects::Effect, gpu::GpuSurface,
    gpu_color::ColorFilter,
};
use ash::vk;
use dream_host_abi::drawing_gpu::Target;
use std::sync::Arc;
use wgpu::hal::api::Vulkan;

pub(super) struct Transfer {
    id: u64,
    texture: wgpu::Texture,
    allocation: Arc<import::Allocation>,
    blit: Option<(ColorFilter, wgpu::Buffer)>,
}

pub(super) unsafe fn copy(
    surface: &mut Surface,
    target: &Target,
    picture: Option<&Picture>,
) -> Result<bool> {
    let Surface::Gpu(current) = surface else {
        return Ok(false);
    };
    if target.width != u32::from(current.width)
        || target.height != u32::from(current.height)
        || target.format > 3
        || target.texture == 0
        || target.fence == 0
        || target.wait_value.checked_add(1) != Some(target.signal_value)
    {
        return Err(DrawingError::Invalid(
            "invalid shared Vulkan drawing target".into(),
        ));
    }
    current.check()?;
    let compatible = unsafe { current.device.as_hal::<Vulkan>() }.is_some_and(|hal| {
        context::matches(&hal, target)
            && hal
                .enabled_device_extensions()
                .contains(&ash::khr::external_semaphore_win32::NAME)
    });
    if !compatible {
        let Some(context) = context::create(target)? else {
            return Ok(false);
        };
        let mut replacement = GpuSurface::with_context(current.width, current.height, context)?;
        if let Some(picture) = picture {
            replacement.render(picture)?;
        }
        **current = replacement;
    }
    if current
        .vulkan_transfer
        .as_ref()
        .is_none_or(|transfer| transfer.id != target.id)
    {
        current.vulkan_transfer = Some(unsafe { import::import(current, target)? });
    }
    let transfer = current
        .vulkan_transfer
        .as_ref()
        .ok_or_else(|| DrawingError::Render("missing Vulkan transfer".into()))?;
    let mut commands = Vec::with_capacity(4);
    commands.push(ownership(current, transfer, true)?);
    let mut encoder = current.device.create_command_encoder(&Default::default());
    if let Some((blit, parameters)) = &transfer.blit {
        commands.push(blit.encode(
            &current.device,
            &current.queue,
            &current.target,
            &transfer.texture,
            parameters,
            &Effect::Brightness(1.),
        ));
    } else {
        encoder.copy_texture_to_texture(
            current.target.as_image_copy(),
            transfer.texture.as_image_copy(),
            current.target.size(),
        );
    }
    encoder.transition_resources(
        std::iter::empty(),
        std::iter::once(wgpu::TextureTransition {
            texture: &transfer.texture,
            selector: None,
            state: wgpu::TextureUses::STORAGE_READ_ONLY,
        }),
    );
    commands.push(encoder.finish());
    commands.push(ownership(current, transfer, false)?);
    {
        let queue = unsafe { current.queue.as_hal::<Vulkan>() }
            .ok_or_else(|| DrawingError::Render("missing Vulkan queue".into()))?;
        queue.add_wait_semaphore(
            transfer.allocation.semaphore,
            Some(target.wait_value),
            vk::PipelineStageFlags::ALL_COMMANDS,
        );
        queue.add_signal_semaphore(transfer.allocation.semaphore, Some(target.signal_value));
    }
    current.queue.submit(commands);
    current.check()?;
    Ok(true)
}

fn ownership(
    surface: &GpuSurface,
    transfer: &Transfer,
    acquire: bool,
) -> Result<wgpu::CommandBuffer> {
    let allocation = &transfer.allocation;
    let mut encoder = surface.device.create_command_encoder(&Default::default());
    unsafe {
        encoder.as_hal_mut::<Vulkan, _, _>(|hal| {
            let hal =
                hal.ok_or_else(|| DrawingError::Render("missing Vulkan transfer encoder".into()))?;
            let barrier = vk::ImageMemoryBarrier::default()
                .image(allocation.image)
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                })
                .old_layout(vk::ImageLayout::GENERAL)
                .new_layout(vk::ImageLayout::GENERAL)
                .src_queue_family_index(if acquire {
                    vk::QUEUE_FAMILY_EXTERNAL
                } else {
                    allocation.family
                })
                .dst_queue_family_index(if acquire {
                    allocation.family
                } else {
                    vk::QUEUE_FAMILY_EXTERNAL
                })
                .src_access_mask(if acquire {
                    vk::AccessFlags::empty()
                } else {
                    vk::AccessFlags::MEMORY_WRITE
                })
                .dst_access_mask(if acquire {
                    vk::AccessFlags::MEMORY_WRITE
                } else {
                    vk::AccessFlags::empty()
                });
            allocation.raw.cmd_pipeline_barrier(
                hal.raw_handle(),
                vk::PipelineStageFlags::ALL_COMMANDS,
                vk::PipelineStageFlags::ALL_COMMANDS,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[barrier],
            );
            Ok(())
        })
    }?;
    Ok(encoder.finish())
}
