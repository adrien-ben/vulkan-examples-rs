mod acceleration_structure;
mod pipeline;
mod shader_binding_table;

pub use acceleration_structure::*;
pub use pipeline::*;
pub use shader_binding_table::*;

use ash::{
    extensions::khr::{AccelerationStructure, RayTracingPipeline},
    vk,
};

use crate::{device::VkDevice, instance::VkInstance, physical_device::VkPhysicalDevice};

pub struct VkRayTracingContext {
    pub pipeline_properties: vk::PhysicalDeviceRayTracingPipelinePropertiesKHR,
    pub pipeline_fn: RayTracingPipeline,
    pub acceleration_structure_properties: vk::PhysicalDeviceAccelerationStructurePropertiesKHR,
    pub acceleration_structure_fn: AccelerationStructure,
}

impl VkRayTracingContext {
    pub(crate) fn new(
        instance: &VkInstance,
        pdevice: &VkPhysicalDevice,
        device: &VkDevice,
    ) -> Self {
        let pipeline_properties =
            unsafe { RayTracingPipeline::get_properties(&instance.inner, pdevice.inner) };
        let pipeline_fn = RayTracingPipeline::new(&instance.inner, &device.inner);

        let acceleration_structure_properties =
            unsafe { AccelerationStructure::get_properties(&instance.inner, pdevice.inner) };
        let acceleration_structure_fn = AccelerationStructure::new(&instance.inner, &device.inner);

        Self {
            pipeline_properties,
            pipeline_fn,
            acceleration_structure_properties,
            acceleration_structure_fn,
        }
    }
}
