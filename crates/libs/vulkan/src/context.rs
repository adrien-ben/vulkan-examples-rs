use std::sync::{Arc, Mutex};

use anyhow::Result;
use ash::{vk, Entry};
use gpu_allocator::{
    vulkan::{Allocator, AllocatorCreateDesc},
    AllocatorDebugSettings,
};
use raw_window_handle::HasRawWindowHandle;

use crate::{
    device::VkDevice,
    instance::VkInstance,
    physical_device::VkPhysicalDevice,
    queue::{VkQueue, VkQueueFamily},
    surface::VkSurface,
    VkCommandBuffer, VkCommandPool, VkRayTracingContext, VkVersion,
};

pub struct VkContext {
    pub allocator: Arc<Mutex<Allocator>>,
    pub command_pool: VkCommandPool,
    pub ray_tracing: Arc<VkRayTracingContext>,
    pub graphics_queue: VkQueue,
    pub present_queue: VkQueue,
    pub device: Arc<VkDevice>,
    pub present_queue_family: VkQueueFamily,
    pub graphics_queue_family: VkQueueFamily,
    pub physical_device: VkPhysicalDevice,
    pub surface: VkSurface,
    pub instance: VkInstance,
    _entry: Entry,
}

impl VkContext {
    pub fn new(
        window: &dyn HasRawWindowHandle,
        api_version: VkVersion,
        app_name: Option<&str>,
        required_extensions: &[&str],
    ) -> Result<Self> {
        // Vulkan instance
        let entry = Entry::linked();
        let mut instance = VkInstance::new(&entry, window, api_version, app_name)?;

        // Vulkan surface
        let surface = VkSurface::new(&entry, &instance, &window)?;

        let physical_devices = instance.enumerate_physical_devices(&surface)?;
        let (physical_device, graphics_queue_family, present_queue_family) =
            select_suitable_physical_device(physical_devices, required_extensions)?;
        log::info!("Selected physical device: {:?}", physical_device.name);

        let queue_families = [graphics_queue_family, present_queue_family];
        let device = Arc::new(VkDevice::new(
            &instance,
            &physical_device,
            &queue_families,
            required_extensions,
        )?);
        let graphics_queue = device.get_queue(graphics_queue_family, 0);
        let present_queue = device.get_queue(present_queue_family, 0);

        let rt_context = Arc::new(VkRayTracingContext::new(
            &instance,
            &physical_device,
            &device,
        ));
        log::debug!(
            "Ray tracing pipeline properties {:#?}",
            rt_context.pipeline_properties
        );
        log::debug!(
            "Acceleration structure properties {:#?}",
            rt_context.acceleration_structure_properties
        );

        let command_pool = VkCommandPool::new(
            device.clone(),
            rt_context.clone(),
            graphics_queue_family,
            Some(vk::CommandPoolCreateFlags::TRANSIENT),
        )?;

        // Gpu allocator
        let allocator = Allocator::new(&AllocatorCreateDesc {
            instance: instance.inner.clone(),
            device: device.inner.clone(),
            physical_device: physical_device.inner,
            debug_settings: AllocatorDebugSettings {
                log_allocations: true,
                log_frees: true,
                ..Default::default()
            },
            buffer_device_address: true,
        })?;

        Ok(Self {
            allocator: Arc::new(Mutex::new(allocator)),
            command_pool,
            ray_tracing: rt_context,
            present_queue,
            graphics_queue,
            device,
            present_queue_family,
            graphics_queue_family,
            physical_device,
            surface,
            instance,
            _entry: entry,
        })
    }
}

fn select_suitable_physical_device(
    devices: &[VkPhysicalDevice],
    required_extensions: &[&str],
) -> Result<(VkPhysicalDevice, VkQueueFamily, VkQueueFamily)> {
    log::debug!("Choosing Vulkan physical device");

    let mut graphics = None;
    let mut present = None;

    let device = devices
        .iter()
        .find(|device| {
            // Does device has graphics and present queues
            for family in device.queue_families.iter().filter(|f| f.has_queues()) {
                if family.supports_graphics() && family.supports_compute() && graphics.is_none() {
                    graphics = Some(*family);
                }

                if family.supports_present() && present.is_none() {
                    present = Some(*family);
                }

                if graphics.is_some() && present.is_some() {
                    break;
                }
            }

            // Does device support desired extensions
            let extention_support = device.supports_extensions(required_extensions);

            graphics.is_some()
                && present.is_some()
                && extention_support
                && !device.supported_surface_formats.is_empty()
                && !device.supported_present_modes.is_empty()
                && device.supports_dynamic_rendering
                && device.supports_synchronization2
        })
        .ok_or_else(|| anyhow::anyhow!("Could not find a suitable device"))?;

    Ok((device.clone(), graphics.unwrap(), present.unwrap()))
}

impl VkContext {
    pub fn device_wait_idle(&self) -> Result<()> {
        unsafe { self.device.inner.device_wait_idle()? };

        Ok(())
    }

    pub fn execute_one_time_commands<R, F: FnOnce(&VkCommandBuffer) -> R>(
        &self,
        executor: F,
    ) -> Result<R> {
        let command_buffer = self
            .command_pool
            .allocate_command_buffer(vk::CommandBufferLevel::PRIMARY)?;

        // Begin recording
        command_buffer.begin(Some(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT))?;

        // Execute user function
        let executor_result = executor(&command_buffer);

        // End recording
        command_buffer.end()?;

        // Submit and wait
        let fence = self.create_fence(None)?;
        self.graphics_queue
            .submit(&command_buffer, None, None, &fence)?;
        fence.wait(None)?;

        // Free
        self.command_pool.free_command_buffer(&command_buffer)?;

        Ok(executor_result)
    }
}
