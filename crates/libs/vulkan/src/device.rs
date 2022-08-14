use std::{ffi::CString, sync::Arc};

use anyhow::Result;
use ash::{vk, Device};

use crate::{
    instance::VkInstance,
    physical_device::VkPhysicalDevice,
    queue::{VkQueue, VkQueueFamily},
};

pub struct VkDevice {
    pub inner: Device,
}

impl VkDevice {
    pub(crate) fn new(
        instance: &VkInstance,
        physical_device: &VkPhysicalDevice,
        queue_families: &[VkQueueFamily],
        required_extensions: &[&str],
        enable_ray_tracing: bool,
    ) -> Result<Self> {
        let queue_priorities = [1.0f32];

        let queue_create_infos = {
            let mut indices = queue_families.iter().map(|f| f.index).collect::<Vec<_>>();
            indices.dedup();

            indices
                .iter()
                .map(|index| {
                    vk::DeviceQueueCreateInfo::builder()
                        .queue_family_index(*index)
                        .queue_priorities(&queue_priorities)
                        .build()
                })
                .collect::<Vec<_>>()
        };

        let device_extensions_ptrs = required_extensions
            .iter()
            .map(|e| CString::new(*e))
            .collect::<Result<Vec<_>, _>>()?;
        let device_extensions_ptrs = device_extensions_ptrs
            .iter()
            .map(|e| e.as_ptr())
            .collect::<Vec<_>>();

        let mut ray_tracing_feature = vk::PhysicalDeviceRayTracingPipelineFeaturesKHR::builder()
            .ray_tracing_pipeline(enable_ray_tracing);
        let mut acceleration_struct_feature =
            vk::PhysicalDeviceAccelerationStructureFeaturesKHR::builder()
                .acceleration_structure(enable_ray_tracing);
        let mut vulkan_12_features = vk::PhysicalDeviceVulkan12Features::builder()
            .runtime_descriptor_array(enable_ray_tracing)
            .buffer_device_address(enable_ray_tracing);
        let mut vulkan_13_features = vk::PhysicalDeviceVulkan13Features::builder()
            .dynamic_rendering(true)
            .synchronization2(true);

        let mut features = vk::PhysicalDeviceFeatures2::builder()
            .features(vk::PhysicalDeviceFeatures::default())
            .push_next(&mut acceleration_struct_feature)
            .push_next(&mut ray_tracing_feature)
            .push_next(&mut vulkan_12_features)
            .push_next(&mut vulkan_13_features);

        let device_create_info = vk::DeviceCreateInfo::builder()
            .queue_create_infos(&queue_create_infos)
            .enabled_extension_names(&device_extensions_ptrs)
            .push_next(&mut features);

        let inner = unsafe {
            instance
                .inner
                .create_device(physical_device.inner, &device_create_info, None)?
        };

        Ok(Self { inner })
    }

    pub fn get_queue(self: &Arc<Self>, queue_family: VkQueueFamily, queue_index: u32) -> VkQueue {
        let inner = unsafe { self.inner.get_device_queue(queue_family.index, queue_index) };
        VkQueue::new(self.clone(), inner)
    }
}

impl Drop for VkDevice {
    fn drop(&mut self) {
        unsafe {
            self.inner.destroy_device(None);
        }
    }
}
