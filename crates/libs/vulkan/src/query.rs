use std::sync::Arc;

use anyhow::Result;
use ash::vk;

use crate::{VkContext, VkDevice};

pub struct VkTimestampQueryPool<const C: usize> {
    device: Arc<VkDevice>,
    pub(crate) inner: vk::QueryPool,
}

impl<const C: usize> VkTimestampQueryPool<C> {
    pub(crate) fn new(device: Arc<VkDevice>) -> Result<Self> {
        let create_info = vk::QueryPoolCreateInfo::builder()
            .query_type(vk::QueryType::TIMESTAMP)
            .query_count(C as _);

        let inner = unsafe { device.inner.create_query_pool(&create_info, None)? };

        Ok(Self { device, inner })
    }
}

impl VkContext {
    pub fn create_timestamp_query_pool<const C: usize>(&self) -> Result<VkTimestampQueryPool<C>> {
        VkTimestampQueryPool::new(self.device.clone())
    }
}

impl<const C: usize> Drop for VkTimestampQueryPool<C> {
    fn drop(&mut self) {
        unsafe {
            self.device.inner.destroy_query_pool(self.inner, None);
        }
    }
}

impl<const C: usize> VkTimestampQueryPool<C> {
    pub fn reset_all(&self) {
        unsafe {
            self.device.inner.reset_query_pool(self.inner, 0, C as _);
        }
    }

    pub fn wait_for_all_results(&self) -> Result<[u64; C]> {
        let mut data = [0u64; C];

        unsafe {
            self.device.inner.get_query_pool_results(
                self.inner,
                0,
                C as _,
                &mut data,
                vk::QueryResultFlags::WAIT | vk::QueryResultFlags::TYPE_64,
            )?;
        }

        Ok(data)
    }
}
