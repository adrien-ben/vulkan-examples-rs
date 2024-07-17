mod acceleration_structure;
mod pipeline;
mod shader_binding_table;

pub use acceleration_structure::*;
pub use pipeline::*;
pub use shader_binding_table::*;

use ash::{
    khr::{acceleration_structure as ash_accel_structure, ray_tracing_pipeline},
    vk,
};

use crate::{device::Device, instance::Instance, physical_device::PhysicalDevice};

pub struct RayTracingContext {
    pub pipeline_properties: PhysicalDeviceRayTracingPipelineProperties,
    pub pipeline_fn: ray_tracing_pipeline::Device,
    pub acceleration_structure_properties: PhysicalDeviceAccelerationStructureProperties,
    pub acceleration_structure_fn: ash_accel_structure::Device,
}

unsafe impl Send for RayTracingContext {}
unsafe impl Sync for RayTracingContext {}

impl RayTracingContext {
    pub(crate) fn new(instance: &Instance, pdevice: &PhysicalDevice, device: &Device) -> Self {
        // get rt pipeline properties
        let mut pipeline_properties = vk::PhysicalDeviceRayTracingPipelinePropertiesKHR::default();
        let mut pproperties2 =
            vk::PhysicalDeviceProperties2::default().push_next(&mut pipeline_properties);
        unsafe {
            instance
                .inner
                .get_physical_device_properties2(pdevice.inner, &mut pproperties2)
        };
        let pipeline_properties = pipeline_properties.into();

        let pipeline_fn = ray_tracing_pipeline::Device::new(&instance.inner, &device.inner);

        // get rt acceleration structure properties
        let mut acceleration_structure_properties =
            vk::PhysicalDeviceAccelerationStructurePropertiesKHR::default();
        let mut pproperties2 = vk::PhysicalDeviceProperties2::default()
            .push_next(&mut acceleration_structure_properties);
        unsafe {
            instance
                .inner
                .get_physical_device_properties2(pdevice.inner, &mut pproperties2)
        };
        let acceleration_structure_properties = acceleration_structure_properties.into();

        let acceleration_structure_fn =
            ash_accel_structure::Device::new(&instance.inner, &device.inner);

        Self {
            pipeline_properties,
            pipeline_fn,
            acceleration_structure_properties,
            acceleration_structure_fn,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct PhysicalDeviceRayTracingPipelineProperties {
    pub shader_group_handle_size: u32,
    pub max_ray_recursion_depth: u32,
    pub max_shader_group_stride: u32,
    pub shader_group_base_alignment: u32,
    pub shader_group_handle_capture_replay_size: u32,
    pub max_ray_dispatch_invocation_count: u32,
    pub shader_group_handle_alignment: u32,
    pub max_ray_hit_attribute_size: u32,
}

impl From<vk::PhysicalDeviceRayTracingPipelinePropertiesKHR<'_>>
    for PhysicalDeviceRayTracingPipelineProperties
{
    fn from(p: vk::PhysicalDeviceRayTracingPipelinePropertiesKHR<'_>) -> Self {
        Self {
            shader_group_handle_size: p.shader_group_handle_size,
            max_ray_recursion_depth: p.max_ray_recursion_depth,
            max_shader_group_stride: p.max_shader_group_stride,
            shader_group_base_alignment: p.shader_group_base_alignment,
            shader_group_handle_capture_replay_size: p.shader_group_handle_capture_replay_size,
            max_ray_dispatch_invocation_count: p.max_ray_dispatch_invocation_count,
            shader_group_handle_alignment: p.shader_group_handle_alignment,
            max_ray_hit_attribute_size: p.max_ray_hit_attribute_size,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct PhysicalDeviceAccelerationStructureProperties {
    pub max_geometry_count: u64,
    pub max_instance_count: u64,
    pub max_primitive_count: u64,
    pub max_per_stage_descriptor_acceleration_structures: u32,
    pub max_per_stage_descriptor_update_after_bind_acceleration_structures: u32,
    pub max_descriptor_set_acceleration_structures: u32,
    pub max_descriptor_set_update_after_bind_acceleration_structures: u32,
    pub min_acceleration_structure_scratch_offset_alignment: u32,
}

impl From<vk::PhysicalDeviceAccelerationStructurePropertiesKHR<'_>>
    for PhysicalDeviceAccelerationStructureProperties
{
    fn from(p: vk::PhysicalDeviceAccelerationStructurePropertiesKHR<'_>) -> Self {
        Self {
            max_geometry_count: p.max_geometry_count,
            max_instance_count: p.max_instance_count,
            max_primitive_count: p.max_primitive_count,
            max_per_stage_descriptor_acceleration_structures: p
                .max_per_stage_descriptor_acceleration_structures,
            max_per_stage_descriptor_update_after_bind_acceleration_structures: p
                .max_per_stage_descriptor_update_after_bind_acceleration_structures,
            max_descriptor_set_acceleration_structures: p
                .max_descriptor_set_acceleration_structures,
            max_descriptor_set_update_after_bind_acceleration_structures: p
                .max_descriptor_set_update_after_bind_acceleration_structures,
            min_acceleration_structure_scratch_offset_alignment: p
                .min_acceleration_structure_scratch_offset_alignment,
        }
    }
}
