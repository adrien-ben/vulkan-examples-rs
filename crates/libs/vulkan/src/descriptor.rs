use std::sync::Arc;

use anyhow::Result;
use ash::vk;

use crate::{device::Device, AccelerationStructure, Buffer, Context, ImageView, Sampler};

pub struct DescriptorSetLayout {
    device: Arc<Device>,
    pub(crate) inner: vk::DescriptorSetLayout,
}

impl DescriptorSetLayout {
    pub(crate) fn new(
        device: Arc<Device>,
        bindings: &[vk::DescriptorSetLayoutBinding],
    ) -> Result<Self> {
        let dsl_info = vk::DescriptorSetLayoutCreateInfo::default().bindings(bindings);
        let inner = unsafe { device.inner.create_descriptor_set_layout(&dsl_info, None)? };

        Ok(Self { device, inner })
    }
}

impl Drop for DescriptorSetLayout {
    fn drop(&mut self) {
        unsafe {
            self.device
                .inner
                .destroy_descriptor_set_layout(self.inner, None);
        }
    }
}

pub struct DescriptorPool {
    device: Arc<Device>,
    pub(crate) inner: vk::DescriptorPool,
}

impl DescriptorPool {
    pub(crate) fn new(
        device: Arc<Device>,
        max_sets: u32,
        pool_sizes: &[vk::DescriptorPoolSize],
    ) -> Result<Self> {
        let pool_info = vk::DescriptorPoolCreateInfo::default()
            .max_sets(max_sets)
            .pool_sizes(pool_sizes);
        let inner = unsafe { device.inner.create_descriptor_pool(&pool_info, None)? };

        Ok(Self { device, inner })
    }

    pub fn allocate_sets(
        &self,
        layout: &DescriptorSetLayout,
        count: u32,
    ) -> Result<Vec<DescriptorSet>> {
        let layouts = (0..count).map(|_| layout.inner).collect::<Vec<_>>();
        let sets_alloc_info = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(self.inner)
            .set_layouts(&layouts);
        let sets = unsafe {
            self.device
                .inner
                .allocate_descriptor_sets(&sets_alloc_info)?
        };
        let sets = sets
            .into_iter()
            .map(|inner| DescriptorSet {
                device: self.device.clone(),
                inner,
            })
            .collect::<Vec<_>>();

        Ok(sets)
    }

    pub fn allocate_set(&self, layout: &DescriptorSetLayout) -> Result<DescriptorSet> {
        Ok(self.allocate_sets(layout, 1)?.into_iter().next().unwrap())
    }
}

impl Drop for DescriptorPool {
    fn drop(&mut self) {
        unsafe { self.device.inner.destroy_descriptor_pool(self.inner, None) };
    }
}

pub struct DescriptorSet {
    device: Arc<Device>,
    pub(crate) inner: vk::DescriptorSet,
}

impl DescriptorSet {
    // TODO: see how to re-implement version from above (1 call to update_descriptor_sets)
    pub fn update(&self, writes: &[WriteDescriptorSet]) {
        use WriteDescriptorSetKind::*;

        for write in writes {
            match write.kind {
                StorageImage { view, layout } => {
                    let img_info = vk::DescriptorImageInfo::default()
                        .image_view(view.inner)
                        .image_layout(layout);

                    let wds = vk::WriteDescriptorSet::default()
                        .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
                        .dst_binding(write.binding)
                        .dst_set(self.inner)
                        .image_info(std::slice::from_ref(&img_info));

                    unsafe { self.device.inner.update_descriptor_sets(&[wds], &[]) };
                }
                AccelerationStructure {
                    acceleration_structure,
                } => {
                    let mut write_set_as =
                        vk::WriteDescriptorSetAccelerationStructureKHR::default()
                            .acceleration_structures(std::slice::from_ref(
                                &acceleration_structure.inner,
                            ));

                    let wds = vk::WriteDescriptorSet::default()
                        .descriptor_type(vk::DescriptorType::ACCELERATION_STRUCTURE_KHR)
                        .descriptor_count(1)
                        .dst_binding(write.binding)
                        .dst_set(self.inner)
                        .push_next(&mut write_set_as);

                    unsafe { self.device.inner.update_descriptor_sets(&[wds], &[]) };
                }
                UniformBuffer { buffer } => {
                    let buffer_info = vk::DescriptorBufferInfo::default()
                        .buffer(buffer.inner)
                        .range(vk::WHOLE_SIZE);

                    let wds = vk::WriteDescriptorSet::default()
                        .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                        .dst_binding(write.binding)
                        .dst_set(self.inner)
                        .buffer_info(std::slice::from_ref(&buffer_info));

                    unsafe { self.device.inner.update_descriptor_sets(&[wds], &[]) };
                }
                UniformBufferDynamic {
                    buffer,
                    byte_stride,
                } => {
                    let buffer_info = vk::DescriptorBufferInfo::default()
                        .buffer(buffer.inner)
                        .range(byte_stride);

                    let wds = vk::WriteDescriptorSet::default()
                        .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER_DYNAMIC)
                        .dst_binding(write.binding)
                        .dst_set(self.inner)
                        .buffer_info(std::slice::from_ref(&buffer_info));

                    unsafe { self.device.inner.update_descriptor_sets(&[wds], &[]) };
                }
                StorageBuffer { buffer } => {
                    let buffer_info = vk::DescriptorBufferInfo::default()
                        .buffer(buffer.inner)
                        .range(vk::WHOLE_SIZE);

                    let wds = vk::WriteDescriptorSet::default()
                        .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                        .dst_binding(write.binding)
                        .dst_set(self.inner)
                        .buffer_info(std::slice::from_ref(&buffer_info));

                    unsafe { self.device.inner.update_descriptor_sets(&[wds], &[]) };
                }
                CombinedImageSampler {
                    view,
                    sampler,
                    layout,
                } => {
                    let img_info = vk::DescriptorImageInfo::default()
                        .image_view(view.inner)
                        .sampler(sampler.inner)
                        .image_layout(layout);

                    let wds = vk::WriteDescriptorSet::default()
                        .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                        .dst_binding(write.binding)
                        .dst_set(self.inner)
                        .image_info(std::slice::from_ref(&img_info));

                    unsafe { self.device.inner.update_descriptor_sets(&[wds], &[]) };
                }
            };
        }
    }
}

impl Context {
    pub fn create_descriptor_set_layout(
        &self,
        bindings: &[vk::DescriptorSetLayoutBinding],
    ) -> Result<DescriptorSetLayout> {
        DescriptorSetLayout::new(self.device.clone(), bindings)
    }

    pub fn create_descriptor_pool(
        &self,
        max_sets: u32,
        pool_sizes: &[vk::DescriptorPoolSize],
    ) -> Result<DescriptorPool> {
        DescriptorPool::new(self.device.clone(), max_sets, pool_sizes)
    }
}

#[derive(Clone, Copy)]
pub struct WriteDescriptorSet<'a> {
    pub binding: u32,
    pub kind: WriteDescriptorSetKind<'a>,
}

#[derive(Clone, Copy)]
pub enum WriteDescriptorSetKind<'a> {
    StorageImage {
        view: &'a ImageView,
        layout: vk::ImageLayout,
    },
    AccelerationStructure {
        acceleration_structure: &'a AccelerationStructure,
    },
    UniformBuffer {
        buffer: &'a Buffer,
    },
    UniformBufferDynamic {
        buffer: &'a Buffer,
        byte_stride: vk::DeviceSize,
    },
    StorageBuffer {
        buffer: &'a Buffer,
    },
    CombinedImageSampler {
        view: &'a ImageView,
        sampler: &'a Sampler,
        layout: vk::ImageLayout,
    },
}
