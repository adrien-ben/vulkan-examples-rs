use std::sync::Arc;

use anyhow::Result;
use ash::vk;

use crate::{device::VkDevice, VkContext};

pub struct VkSampler {
    device: Arc<VkDevice>,
    pub(crate) inner: vk::Sampler,
}

impl VkSampler {
    pub(crate) fn new(device: Arc<VkDevice>, create_info: &vk::SamplerCreateInfo) -> Result<Self> {
        let inner = unsafe { device.inner.create_sampler(create_info, None)? };

        Ok(Self { device, inner })
    }
}

impl VkContext {
    pub fn create_sampler(&self, create_info: &vk::SamplerCreateInfo) -> Result<VkSampler> {
        VkSampler::new(self.device.clone(), create_info)
    }
}

impl Drop for VkSampler {
    fn drop(&mut self) {
        unsafe {
            self.device.inner.destroy_sampler(self.inner, None);
        }
    }
}
