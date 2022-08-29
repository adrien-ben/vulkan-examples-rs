use std::sync::Arc;

use anyhow::Result;
use ash::vk;

use crate::{device::VkDevice, VkCommandBuffer, VkFence, VkSemaphore};

#[derive(Debug, Clone, Copy)]
pub struct VkQueueFamily {
    pub index: u32,
    pub(crate) inner: vk::QueueFamilyProperties,
    supports_present: bool,
}

impl VkQueueFamily {
    pub(crate) fn new(
        index: u32,
        inner: vk::QueueFamilyProperties,
        supports_present: bool,
    ) -> Self {
        Self {
            index,
            inner,
            supports_present,
        }
    }

    pub fn supports_compute(&self) -> bool {
        self.inner.queue_flags.contains(vk::QueueFlags::COMPUTE)
    }

    pub fn supports_graphics(&self) -> bool {
        self.inner.queue_flags.contains(vk::QueueFlags::GRAPHICS)
    }

    pub fn supports_present(&self) -> bool {
        self.supports_present
    }

    pub fn has_queues(&self) -> bool {
        self.inner.queue_count > 0
    }

    pub fn supports_timestamp_queries(&self) -> bool {
        self.inner.timestamp_valid_bits > 0
    }
}

pub struct VkQueue {
    device: Arc<VkDevice>,
    pub inner: vk::Queue,
}

impl VkQueue {
    pub(crate) fn new(device: Arc<VkDevice>, inner: vk::Queue) -> Self {
        Self { device, inner }
    }

    pub fn submit(
        &self,
        command_buffer: &VkCommandBuffer,
        wait_semaphore: Option<VkSemaphoreSubmitInfo>,
        signal_semaphore: Option<VkSemaphoreSubmitInfo>,
        fence: &VkFence,
    ) -> Result<()> {
        let wait_semaphore_submit_info = wait_semaphore.map(|s| {
            vk::SemaphoreSubmitInfo::builder()
                .semaphore(s.semaphore.inner)
                .stage_mask(s.stage_mask)
        });

        let signal_semaphore_submit_info = signal_semaphore.map(|s| {
            vk::SemaphoreSubmitInfo::builder()
                .semaphore(s.semaphore.inner)
                .stage_mask(s.stage_mask)
        });

        let cmd_buffer_submit_info =
            vk::CommandBufferSubmitInfo::builder().command_buffer(command_buffer.inner);

        let submit_info = vk::SubmitInfo2::builder()
            .command_buffer_infos(std::slice::from_ref(&cmd_buffer_submit_info));

        let submit_info = match wait_semaphore_submit_info.as_ref() {
            Some(info) => submit_info.wait_semaphore_infos(std::slice::from_ref(info)),
            None => submit_info,
        };

        let submit_info = match signal_semaphore_submit_info.as_ref() {
            Some(info) => submit_info.signal_semaphore_infos(std::slice::from_ref(info)),
            None => submit_info,
        };

        unsafe {
            self.device.inner.queue_submit2(
                self.inner,
                std::slice::from_ref(&submit_info),
                fence.inner,
            )?
        };

        Ok(())
    }
}

pub struct VkSemaphoreSubmitInfo<'a> {
    pub semaphore: &'a VkSemaphore,
    pub stage_mask: vk::PipelineStageFlags2,
}
