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
        device_features: &VkDeviceFeatures,
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
            .ray_tracing_pipeline(device_features.ray_tracing_pipeline);
        let mut acceleration_struct_feature =
            vk::PhysicalDeviceAccelerationStructureFeaturesKHR::builder()
                .acceleration_structure(device_features.acceleration_structure);
        let mut vulkan_12_features = vk::PhysicalDeviceVulkan12Features::builder()
            .runtime_descriptor_array(device_features.runtime_descriptor_array)
            .buffer_device_address(device_features.buffer_device_address);
        let mut vulkan_13_features = vk::PhysicalDeviceVulkan13Features::builder()
            .dynamic_rendering(device_features.dynamic_rendering)
            .synchronization2(device_features.synchronization2);

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

#[derive(Debug, Clone, Copy, Default)]
pub struct VkDeviceFeatures {
    pub ray_tracing_pipeline: bool,
    pub acceleration_structure: bool,
    pub runtime_descriptor_array: bool,
    pub buffer_device_address: bool,
    pub dynamic_rendering: bool,
    pub synchronization2: bool,
}

impl VkDeviceFeatures {
    pub fn is_compatible_with(&self, requirements: &Self) -> bool {
        (!requirements.ray_tracing_pipeline || self.ray_tracing_pipeline)
            && (!requirements.acceleration_structure || self.acceleration_structure)
            && (!requirements.runtime_descriptor_array || self.runtime_descriptor_array)
            && (!requirements.buffer_device_address || self.buffer_device_address)
            && (!requirements.dynamic_rendering || self.dynamic_rendering)
            && (!requirements.synchronization2 || self.synchronization2)
    }
}
