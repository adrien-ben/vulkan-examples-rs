use std::sync::Arc;

use anyhow::Result;
use ash::vk;

use crate::{device::VkDevice, VkContext, VkDescriptorSetLayout};

pub struct VkPipelineLayout {
    device: Arc<VkDevice>,
    pub(crate) inner: vk::PipelineLayout,
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

impl VkContext {
    pub fn create_pipeline_layout(
        &self,
        descriptor_set_layouts: &[&VkDescriptorSetLayout],
    ) -> Result<VkPipelineLayout> {
        VkPipelineLayout::new(self.device.clone(), descriptor_set_layouts)
    }
}

impl Drop for VkPipelineLayout {
    fn drop(&mut self) {
        unsafe { self.device.inner.destroy_pipeline_layout(self.inner, None) };
    }
}
