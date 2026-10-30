use crate::{DrawingError, Result, gpu::GpuContext};
use ash::vk;
use dream_host_abi::drawing_gpu::Target;
use wgpu::hal::api::Vulkan;

pub(super) fn matches(device: &wgpu::hal::vulkan::Device, target: &Target) -> bool {
    let mut ids = vk::PhysicalDeviceIDProperties::default();
    let mut properties = vk::PhysicalDeviceProperties2::default().push_next(&mut ids);
    unsafe {
        device
            .shared_instance()
            .raw_instance()
            .get_physical_device_properties2(device.raw_physical_device(), &mut properties)
    };
    ids.device_uuid == target.device_uuid && ids.driver_uuid == target.driver_uuid
}

pub(super) fn create(target: &Target) -> Result<Option<GpuContext>> {
    let mut desc = wgpu::InstanceDescriptor::new_without_display_handle();
    desc.backends = wgpu::Backends::VULKAN;
    let instance = wgpu::Instance::new(desc);
    for adapter in pollster::block_on(instance.enumerate_adapters(wgpu::Backends::VULKAN)) {
        let Some(hal) = (unsafe { adapter.as_hal::<Vulkan>() }) else {
            continue;
        };
        let raw_instance = hal.shared_instance().raw_instance();
        let physical = hal.raw_physical_device();
        let mut ids = vk::PhysicalDeviceIDProperties::default();
        let mut properties = vk::PhysicalDeviceProperties2::default().push_next(&mut ids);
        unsafe { raw_instance.get_physical_device_properties2(physical, &mut properties) };
        if ids.device_uuid != target.device_uuid || ids.driver_uuid != target.driver_uuid {
            continue;
        }
        let extensions = unsafe { raw_instance.enumerate_device_extension_properties(physical) }
            .map_err(failure)?;
        let required = [
            ash::khr::external_memory_win32::NAME,
            ash::khr::external_semaphore_win32::NAME,
        ];
        if required.iter().any(|name| {
            !extensions.iter().any(|ext| {
                ext.extension_name_as_c_str()
                    .is_ok_and(|value| value == *name)
            })
        }) {
            return Ok(None);
        }
        let families =
            unsafe { raw_instance.get_physical_device_queue_family_properties(physical) };
        // wgpu-hal currently opens queue family zero. Do not assume it supports rendering.
        if families.first().is_none_or(|family| {
            !family
                .queue_flags
                .contains(vk::QueueFlags::GRAPHICS | vk::QueueFlags::COMPUTE)
        }) {
            return Ok(None);
        }
        let features = adapter.features() & (wgpu::Features::SHADER_F16 | wgpu::Features::SUBGROUP);
        let descriptor = wgpu::DeviceDescriptor {
            required_features: features,
            ..Default::default()
        };
        let opened = unsafe {
            hal.open_with_callback(
                features,
                &descriptor.required_limits,
                &descriptor.memory_hints,
                Some(Box::new(move |args| {
                    for name in required {
                        if !args.extensions.contains(&name) {
                            args.extensions.push(name);
                        }
                    }
                })),
            )
        }
        .map_err(|error| DrawingError::Render(error.to_string()))?;
        drop(hal);
        let (device, queue) =
            unsafe { adapter.create_device_from_hal::<Vulkan>(opened, &descriptor) }
                .map_err(|error| DrawingError::Render(error.to_string()))?;
        return Ok(Some(GpuContext::from_device(device, queue)));
    }
    Ok(None)
}

pub(super) fn failure(error: vk::Result) -> DrawingError {
    DrawingError::Render(format!("Vulkan drawing transfer: {error:?}"))
}
