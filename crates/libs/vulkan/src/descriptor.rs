use std::sync::Arc;

use anyhow::Result;
use ash::vk;

use crate::{
    device::VkDevice, VkAccelerationStructure, VkBuffer, VkContext, VkImageView, VkSampler,
};

pub struct VkDescriptorSetLayout {
    device: Arc<VkDevice>,
    pub(crate) inner: vk::DescriptorSetLayout,
}

impl VkDescriptorSetLayout {
    pub(crate) fn new(
        device: Arc<VkDevice>,
        bindings: &[vk::DescriptorSetLayoutBinding],
    ) -> Result<Self> {
        let dsl_info = vk::DescriptorSetLayoutCreateInfo::builder().bindings(bindings);
        let inner = unsafe { device.inner.create_descriptor_set_layout(&dsl_info, None)? };

        Ok(Self { device, inner })
    }
}

impl Drop for VkDescriptorSetLayout {
    fn drop(&mut self) {
        unsafe {
            self.device
                .inner
                .destroy_descriptor_set_layout(self.inner, None);
        }
    }
}

pub struct VkDescriptorPool {
    device: Arc<VkDevice>,
    pub(crate) inner: vk::DescriptorPool,
}

impl VkDescriptorPool {
    pub(crate) fn new(
        device: Arc<VkDevice>,
        max_sets: u32,
        pool_sizes: &[vk::DescriptorPoolSize],
    ) -> Result<Self> {
        let pool_info = vk::DescriptorPoolCreateInfo::builder()
            .max_sets(max_sets)
            .pool_sizes(pool_sizes);
        let inner = unsafe { device.inner.create_descriptor_pool(&pool_info, None)? };

        Ok(Self { device, inner })
    }

    pub fn allocate_sets(
        &self,
        layout: &VkDescriptorSetLayout,
        count: u32,
    ) -> Result<Vec<VkDescriptorSet>> {
        let layouts = (0..count).map(|_| layout.inner).collect::<Vec<_>>();
        let sets_alloc_info = vk::DescriptorSetAllocateInfo::builder()
            .descriptor_pool(self.inner)
            .set_layouts(&layouts);
        let sets = unsafe {
            self.device
                .inner
                .allocate_descriptor_sets(&sets_alloc_info)?
        };
        let sets = sets
            .into_iter()
            .map(|inner| VkDescriptorSet {
                device: self.device.clone(),
                inner,
            })
            .collect::<Vec<_>>();

        Ok(sets)
    }

    pub fn allocate_set(&self, layout: &VkDescriptorSetLayout) -> Result<VkDescriptorSet> {
        Ok(self.allocate_sets(layout, 1)?.into_iter().next().unwrap())
    }
}

impl Drop for VkDescriptorPool {
    fn drop(&mut self) {
        unsafe { self.device.inner.destroy_descriptor_pool(self.inner, None) };
    }
}

pub struct VkDescriptorSet {
    device: Arc<VkDevice>,
    pub(crate) inner: vk::DescriptorSet,
}

impl VkDescriptorSet {
    pub fn update(&self, writes: &[VkWriteDescriptorSet]) {
        use VkWriteDescriptorSetKind::*;

        // these Vec are here to keep structure internal to WriteDescriptorSet (DescriptorImageInfo, DescriptorBufferInfo, ...) alive
        let mut img_infos = vec![];
        let mut buffer_infos = vec![];
        let mut as_infos = vec![];

        let descriptor_writes = writes
            .iter()
            .map(|write| match write.kind {
                StorageImage { view, layout } => {
                    let img_info = vk::DescriptorImageInfo::builder()
                        .image_view(view.inner)
                        .image_layout(layout);

                    img_infos.push(img_info);

                    vk::WriteDescriptorSet::builder()
                        .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
                        .dst_binding(write.binding)
                        .dst_set(self.inner)
                        .image_info(std::slice::from_ref(img_infos.last().unwrap()))
                        .build()
                }
                AccelerationStructure {
                    acceleration_structure,
                } => {
                    let write_set_as = vk::WriteDescriptorSetAccelerationStructureKHR::builder()
                        .acceleration_structures(std::slice::from_ref(
                            &acceleration_structure.inner,
                        ));

                    as_infos.push(write_set_as);

                    let mut write = vk::WriteDescriptorSet::builder()
                        .descriptor_type(vk::DescriptorType::ACCELERATION_STRUCTURE_KHR)
                        .dst_binding(write.binding)
                        .dst_set(self.inner)
                        .push_next(as_infos.last_mut().unwrap())
                        .build();
                    write.descriptor_count = 1;

                    write
                }
                UniformBuffer { buffer } => {
                    let buffer_info = vk::DescriptorBufferInfo::builder()
                        .buffer(buffer.inner)
                        .range(vk::WHOLE_SIZE);

                    buffer_infos.push(buffer_info);

                    vk::WriteDescriptorSet::builder()
                        .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                        .dst_binding(write.binding)
                        .dst_set(self.inner)
                        .buffer_info(std::slice::from_ref(buffer_infos.last().unwrap()))
                        .build()
                }
                StorageBuffer { buffer } => {
                    let buffer_info = vk::DescriptorBufferInfo::builder()
                        .buffer(buffer.inner)
                        .range(vk::WHOLE_SIZE);

                    buffer_infos.push(buffer_info);

                    vk::WriteDescriptorSet::builder()
                        .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                        .dst_binding(write.binding)
                        .dst_set(self.inner)
                        .buffer_info(std::slice::from_ref(buffer_infos.last().unwrap()))
                        .build()
                }
                CombinedImageSampler {
                    view,
                    sampler,
                    layout,
                } => {
                    let img_info = vk::DescriptorImageInfo::builder()
                        .image_view(view.inner)
                        .sampler(sampler.inner)
                        .image_layout(layout);

                    img_infos.push(img_info);

                    vk::WriteDescriptorSet::builder()
                        .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                        .dst_binding(write.binding)
                        .dst_set(self.inner)
                        .image_info(std::slice::from_ref(img_infos.last().unwrap()))
                        .build()
                }
            })
            .collect::<Vec<_>>();

        unsafe {
            self.device
                .inner
                .update_descriptor_sets(&descriptor_writes, &[])
        };
    }
}

impl VkContext {
    pub fn create_descriptor_set_layout(
        &self,
        bindings: &[vk::DescriptorSetLayoutBinding],
    ) -> Result<VkDescriptorSetLayout> {
        VkDescriptorSetLayout::new(self.device.clone(), bindings)
    }

    pub fn create_descriptor_pool(
        &self,
        max_sets: u32,
        pool_sizes: &[vk::DescriptorPoolSize],
    ) -> Result<VkDescriptorPool> {
        VkDescriptorPool::new(self.device.clone(), max_sets, pool_sizes)
    }
}

#[derive(Clone, Copy)]
pub struct VkWriteDescriptorSet<'a> {
    pub binding: u32,
    pub kind: VkWriteDescriptorSetKind<'a>,
}

#[derive(Clone, Copy)]
pub enum VkWriteDescriptorSetKind<'a> {
    StorageImage {
        view: &'a VkImageView,
        layout: vk::ImageLayout,
    },
    AccelerationStructure {
        acceleration_structure: &'a VkAccelerationStructure,
    },
    UniformBuffer {
        buffer: &'a VkBuffer,
    },
    StorageBuffer {
        buffer: &'a VkBuffer,
    },
    CombinedImageSampler {
        view: &'a VkImageView,
        sampler: &'a VkSampler,
        layout: vk::ImageLayout,
    },
}
