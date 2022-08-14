use std::mem::size_of_val;

use anyhow::Result;
use ash::vk;
use gpu_allocator::MemoryLocation;

use crate::{VkBuffer, VkContext};

pub fn compute_aligned_size(size: u32, alignment: u32) -> u32 {
    (size + (alignment - 1)) & !(alignment - 1)
}

pub fn read_shader_from_bytes(bytes: &[u8]) -> Result<Vec<u32>> {
    let mut cursor = std::io::Cursor::new(bytes);
    Ok(ash::util::read_spv(&mut cursor)?)
}

pub fn create_gpu_only_buffer_from_data<T: Copy>(
    context: &VkContext,
    usage: vk::BufferUsageFlags,
    data: &[T],
) -> Result<VkBuffer> {
    let size = size_of_val(data) as _;
    let staging_buffer = context.create_buffer(
        vk::BufferUsageFlags::TRANSFER_SRC,
        MemoryLocation::CpuToGpu,
        size,
    )?;
    staging_buffer.copy_data_to_buffer(data)?;

    let buffer = context.create_buffer(
        usage | vk::BufferUsageFlags::TRANSFER_DST,
        MemoryLocation::GpuOnly,
        size,
    )?;

    context.execute_one_time_commands(|cmd_buffer| {
        cmd_buffer.copy_buffer(&staging_buffer, &buffer);
    })?;

    Ok(buffer)
}
