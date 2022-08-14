use std::{ffi::CString, sync::Arc};

use anyhow::Result;
use ash::vk;

use crate::{device::VkDevice, VkContext};

use crate::{VkPipelineLayout, VkRayTracingContext, VkShaderModule};

pub struct VkRTPipelineCreateInfo<'a> {
    pub shaders: &'a [VkRTShaderCreateInfo<'a>],
    pub max_ray_recursion_depth: u32,
}

pub struct VkRTShaderCreateInfo<'a> {
    pub source: &'a [u8],
    pub stage: vk::ShaderStageFlags,
    pub group: VkRTShaderGroup,
}

#[derive(Debug, Clone, Copy)]
pub enum VkRTShaderGroup {
    RayGen,
    Miss,
    ClosestHit,
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

impl VkRTPipeline {
    pub(crate) fn new(
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
    pub fn create_ray_tracing_pipeline(
        &self,
        layout: &VkPipelineLayout,
        create_info: &VkRTPipelineCreateInfo,
    ) -> Result<VkRTPipeline> {
        let ray_tracing = self.ray_tracing.as_ref().expect(
            "Cannot call VkContext::create_ray_tracing_pipeline when ray tracing is not enabled",
        );

        VkRTPipeline::new(self.device.clone(), ray_tracing, layout, create_info)
    }
}

impl Drop for VkRTPipeline {
    fn drop(&mut self) {
        unsafe { self.device.inner.destroy_pipeline(self.inner, None) };
    }
}
