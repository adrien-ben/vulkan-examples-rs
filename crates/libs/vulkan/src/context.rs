use std::sync::{Arc, Mutex};

use anyhow::Result;
use ash::{vk, Entry};
use gpu_allocator::{
    vulkan::{Allocator, AllocatorCreateDesc},
    AllocatorDebugSettings,
};
use raw_window_handle::HasRawWindowHandle;

use crate::{
    device::{VkDevice, VkDeviceFeatures},
    instance::VkInstance,
    physical_device::VkPhysicalDevice,
    queue::{VkQueue, VkQueueFamily},
    surface::VkSurface,
    VkCommandBuffer, VkCommandPool, VkRayTracingContext, VkVersion, VERSION_1_0,
};

pub struct VkContext {
    pub allocator: Arc<Mutex<Allocator>>,
    pub command_pool: VkCommandPool,
    pub ray_tracing: Option<Arc<VkRayTracingContext>>,
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

pub struct VkContextBuilder<'a> {
    window: &'a dyn HasRawWindowHandle,
    vulkan_version: VkVersion,
    app_name: &'a str,
    required_extensions: &'a [&'a str],
    required_device_features: VkDeviceFeatures,
    with_raytracing_context: bool,
}

impl<'a> VkContextBuilder<'a> {
    pub fn new(window: &'a dyn HasRawWindowHandle) -> Self {
        Self {
            window,
            vulkan_version: VERSION_1_0,
            app_name: "",
            required_extensions: &[],
            required_device_features: Default::default(),
            with_raytracing_context: false,
        }
    }

    pub fn vulkan_version(self, vulkan_version: VkVersion) -> Self {
        Self {
            vulkan_version,
            ..self
        }
    }

    pub fn app_name(self, app_name: &'a str) -> Self {
        Self { app_name, ..self }
    }

    pub fn required_extensions(self, required_extensions: &'a [&str]) -> Self {
        Self {
            required_extensions,
            ..self
        }
    }

    pub fn required_device_features(self, required_device_features: VkDeviceFeatures) -> Self {
        Self {
            required_device_features,
            ..self
        }
    }

    pub fn with_raytracing_context(self, with_raytracing_context: bool) -> Self {
        Self {
            with_raytracing_context,
            ..self
        }
    }

    pub fn build(self) -> Result<VkContext> {
        VkContext::new(self)
    }
}

impl VkContext {
    fn new(
        VkContextBuilder {
            window,
            vulkan_version,
            app_name,
            required_extensions,
            required_device_features,
            with_raytracing_context,
        }: VkContextBuilder,
    ) -> Result<Self> {
        // Vulkan instance
        let entry = Entry::linked();
        let mut instance = VkInstance::new(&entry, window, vulkan_version, app_name)?;

        // Vulkan surface
        let surface = VkSurface::new(&entry, &instance, &window)?;

        let physical_devices = instance.enumerate_physical_devices(&surface)?;
        let (physical_device, graphics_queue_family, present_queue_family) =
            select_suitable_physical_device(
                physical_devices,
                required_extensions,
                &required_device_features,
            )?;
        log::info!("Selected physical device: {:?}", physical_device.name);

        let queue_families = [graphics_queue_family, present_queue_family];
        let device = Arc::new(VkDevice::new(
            &instance,
            &physical_device,
            &queue_families,
            required_extensions,
            &required_device_features,
        )?);
        let graphics_queue = device.get_queue(graphics_queue_family, 0);
        let present_queue = device.get_queue(present_queue_family, 0);

        let ray_tracing = with_raytracing_context.then(|| {
            let ray_tracing = Arc::new(VkRayTracingContext::new(
                &instance,
                &physical_device,
                &device,
            ));
            log::debug!(
                "Ray tracing pipeline properties {:#?}",
                ray_tracing.pipeline_properties
            );
            log::debug!(
                "Acceleration structure properties {:#?}",
                ray_tracing.acceleration_structure_properties
            );
            ray_tracing
        });

        let command_pool = VkCommandPool::new(
            device.clone(),
            ray_tracing.clone(),
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
            buffer_device_address: required_device_features.buffer_device_address,
        })?;

        Ok(Self {
            allocator: Arc::new(Mutex::new(allocator)),
            command_pool,
            ray_tracing,
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
    required_device_features: &VkDeviceFeatures,
) -> Result<(VkPhysicalDevice, VkQueueFamily, VkQueueFamily)> {
    log::debug!("Choosing Vulkan physical device");

    let mut graphics = None;
    let mut present = None;

    let device = devices
        .iter()
        .find(|device| {
            // Does device has graphics and present queues
            for family in device.queue_families.iter().filter(|f| f.has_queues()) {
                if family.supports_graphics()
                    && family.supports_compute()
                    && family.supports_timestamp_queries()
                    && graphics.is_none()
                {
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
                && device
                    .supported_device_features
                    .is_compatible_with(required_device_features)
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
