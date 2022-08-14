use std::sync::Arc;

use anyhow::Result;
use ash::vk;

use crate::{device::VkDevice, utils::read_shader_from_bytes, VkContext};

pub struct VkShaderModule {
    device: Arc<VkDevice>,
    pub(crate) inner: vk::ShaderModule,
}

impl VkShaderModule {
    pub(crate) fn from_bytes(device: Arc<VkDevice>, source: &[u8]) -> Result<Self> {
        let source = read_shader_from_bytes(source)?;

        let create_info = vk::ShaderModuleCreateInfo::builder().code(&source);
        let inner = unsafe { device.inner.create_shader_module(&create_info, None)? };

        Ok(Self { device, inner })
    }
}

impl VkContext {
    pub fn create_shader_module(&self, source: &[u8]) -> Result<VkShaderModule> {
        VkShaderModule::from_bytes(self.device.clone(), source)
    }
}

impl Drop for VkShaderModule {
    fn drop(&mut self) {
        unsafe {
            self.device.inner.destroy_shader_module(self.inner, None);
        }
    }
}
