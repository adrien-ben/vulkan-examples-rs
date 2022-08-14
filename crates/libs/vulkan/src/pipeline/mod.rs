mod shader;

pub use shader::*;

use std::{ffi::CString, sync::Arc};

use anyhow::Result;
use ash::vk;

use crate::{device::VkDevice, VkContext, VkDescriptorSetLayout, VkRayTracingContext};

pub struct VkRTPipelineCreateInfo<'a> {
    pub shaders: &'a [VkRTShaderCreateInfo<'a>],
    pub max_ray_recursion_depth: u32,
}

pub struct VkPipelineLayout {
    device: Arc<VkDevice>,
    pub(crate) inner: vk::PipelineLayout,
}

pub struct VkRTPipeline {
    device: Arc<VkDevice>,
    pub(crate) inner: vk::Pipeline,
    pub(crate) shader_group_info: VkRTShaderGroupInfo,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct VkRTShaderGroupInfo {
    pub group_count: u32,
    pub raygen_shader_count: u32,
    pub miss_shader_count: u32,
    pub hit_shader_count: u32,
}

impl VkPipelineLayout {
    pub(crate) fn new(
        device: Arc<VkDevice>,
        descriptor_set_layouts: &[&VkDescriptorSetLayout],
    ) -> Result<Self> {
        let layouts = descriptor_set_layouts
            .iter()
            .map(|l| l.inner)
            .collect::<Vec<_>>();

        let pipe_layout_info = vk::PipelineLayoutCreateInfo::builder().set_layouts(&layouts);
        let inner = unsafe {
            device
                .inner
                .create_pipeline_layout(&pipe_layout_info, None)?
        };

        Ok(Self { device, inner })
    }
}

impl VkRTPipeline {
    pub(crate) fn new_ray_tracing(
        device: Arc<VkDevice>,
        ray_tracing: &VkRayTracingContext,
        layout: &VkPipelineLayout,
        create_info: &VkRTPipelineCreateInfo,
    ) -> Result<Self> {
        let mut shader_group_info = VkRTShaderGroupInfo {
            group_count: create_info.shaders.len() as _,
            ..Default::default()
        };

        let mut modules = vec![];
        let mut stages = vec![];
        let mut groups = vec![];

        let entry_point_name = CString::new("main").unwrap();

        for (shader_index, shader) in create_info.shaders.iter().enumerate() {
            let module = VkShaderModule::from_bytes(device.clone(), shader.source)?;

            let stage = vk::PipelineShaderStageCreateInfo::builder()
                .stage(shader.stage)
                .module(module.inner)
                .name(&entry_point_name)
                .build();

            match shader.group {
                VkRTShaderGroup::RayGen => shader_group_info.raygen_shader_count += 1,
                VkRTShaderGroup::Miss => shader_group_info.miss_shader_count += 1,
                VkRTShaderGroup::ClosestHit => shader_group_info.hit_shader_count += 1,
            };

            let mut group = vk::RayTracingShaderGroupCreateInfoKHR::builder()
                .ty(vk::RayTracingShaderGroupTypeKHR::GENERAL)
                .general_shader(vk::SHADER_UNUSED_KHR)
                .closest_hit_shader(vk::SHADER_UNUSED_KHR)
                .any_hit_shader(vk::SHADER_UNUSED_KHR)
                .intersection_shader(vk::SHADER_UNUSED_KHR);
            group = match shader.group {
                VkRTShaderGroup::RayGen | VkRTShaderGroup::Miss => {
                    group.general_shader(shader_index as _)
                }
                VkRTShaderGroup::ClosestHit => group
                    .ty(vk::RayTracingShaderGroupTypeKHR::TRIANGLES_HIT_GROUP)
                    .closest_hit_shader(shader_index as _),
            };

            modules.push(module);
            stages.push(stage);
            groups.push(group.build());
        }

        let pipe_info = vk::RayTracingPipelineCreateInfoKHR::builder()
            .layout(layout.inner)
            .stages(&stages)
            .groups(&groups)
            .max_pipeline_ray_recursion_depth(2);

        let inner = unsafe {
            ray_tracing.pipeline_fn.create_ray_tracing_pipelines(
                vk::DeferredOperationKHR::null(),
                vk::PipelineCache::null(),
                std::slice::from_ref(&pipe_info),
                None,
            )?[0]
        };

        Ok(Self {
            device,
            inner,
            shader_group_info,
        })
    }
}

impl VkContext {
    pub fn create_pipeline_layout(
        &self,
        descriptor_set_layouts: &[&VkDescriptorSetLayout],
    ) -> Result<VkPipelineLayout> {
        VkPipelineLayout::new(self.device.clone(), descriptor_set_layouts)
    }

    pub fn create_ray_tracing_pipeline(
        &self,
        layout: &VkPipelineLayout,
        create_info: &VkRTPipelineCreateInfo,
    ) -> Result<VkRTPipeline> {
        VkRTPipeline::new_ray_tracing(self.device.clone(), &self.ray_tracing, layout, create_info)
    }
}

impl Drop for VkPipelineLayout {
    fn drop(&mut self) {
        unsafe { self.device.inner.destroy_pipeline_layout(self.inner, None) };
    }
}

impl Drop for VkRTPipeline {
    fn drop(&mut self) {
        unsafe { self.device.inner.destroy_pipeline(self.inner, None) };
    }
}
