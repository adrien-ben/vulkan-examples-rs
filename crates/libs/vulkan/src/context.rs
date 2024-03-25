use std::sync::{Arc, Mutex};

use anyhow::Result;
use ash::{vk, Entry};
use gpu_allocator::{
    vulkan::{Allocator, AllocatorCreateDesc},
    AllocatorDebugSettings,
};
use raw_window_handle::{HasRawDisplayHandle, HasRawWindowHandle};

use crate::{
    device::{Device, DeviceFeatures},
    instance::Instance,
    physical_device::PhysicalDevice,
    queue::{Queue, QueueFamily},
    surface::Surface,
    CommandBuffer, CommandPool, RayTracingContext, Version, VERSION_1_0,
};

pub struct Context {
    pub allocator: Arc<Mutex<Allocator>>,
    pub command_pool: CommandPool,
    pub ray_tracing: Option<Arc<RayTracingContext>>,
    pub graphics_queue: Queue,
    pub present_queue: Queue,
    pub device: Arc<Device>,
    pub present_queue_family: QueueFamily,
    pub graphics_queue_family: QueueFamily,
    pub physical_device: PhysicalDevice,
    pub(crate) supported_surface_formats: Vec<vk::SurfaceFormatKHR>,
    pub surface: Surface,
    pub instance: Instance,
    _entry: Entry,
}

pub struct ContextBuilder<'a> {
    window_handle: &'a dyn HasRawWindowHandle,
    display_handle: &'a dyn HasRawDisplayHandle,
    vulkan_version: Version,
    app_name: &'a str,
    required_instance_extensions: &'a [&'a str],
    required_device_extensions: &'a [&'a str],
    required_device_features: DeviceFeatures,
    with_raytracing_context: bool,
}

impl<'a> ContextBuilder<'a> {
    pub fn new(
        window_handle: &'a dyn HasRawWindowHandle,
        display_handle: &'a dyn HasRawDisplayHandle,
    ) -> Self {
        Self {
            window_handle,
            display_handle,
            vulkan_version: VERSION_1_0,
            app_name: "",
            required_instance_extensions: &[],
            required_device_extensions: &[],
            required_device_features: Default::default(),
            with_raytracing_context: false,
        }
    }

    pub fn vulkan_version(self, vulkan_version: Version) -> Self {
        Self {
            vulkan_version,
            ..self
        }
    }

    pub fn app_name(self, app_name: &'a str) -> Self {
        Self { app_name, ..self }
    }

    pub fn required_instance_extensions(self, required_instance_extensions: &'a [&str]) -> Self {
        Self {
            required_instance_extensions,
            ..self
        }
    }

    pub fn required_device_extensions(self, required_device_extensions: &'a [&str]) -> Self {
        Self {
            required_device_extensions,
            ..self
        }
    }

    pub fn required_device_features(self, required_device_features: DeviceFeatures) -> Self {
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

    pub fn build(self) -> Result<Context> {
        Context::new(self)
    }
}

impl Context {
    fn new(
        ContextBuilder {
            window_handle,
            display_handle,
            vulkan_version,
            app_name,
            required_instance_extensions,
            required_device_extensions,
            required_device_features,
            with_raytracing_context,
        }: ContextBuilder,
    ) -> Result<Self> {
        // Vulkan instance
        let entry = unsafe { Entry::load()? };
        let mut instance = Instance::new(
            &entry,
            display_handle,
            vulkan_version,
            app_name,
            required_instance_extensions,
        )?;

        // Vulkan surface
        let surface = Surface::new(&entry, &instance, window_handle, display_handle)?;

        let physical_devices = instance.enumerate_physical_devices(&surface)?;
        let (physical_device, graphics_queue_family, present_queue_family) =
            select_suitable_physical_device(
                physical_devices,
                required_device_extensions,
                &required_device_features,
            )?;
        log::info!("Selected physical device: {:?}", physical_device.name);

        let supported_surface_formats = unsafe {
            surface
                .inner
                .get_physical_device_surface_formats(physical_device.inner, surface.surface_khr)?
        };

        let queue_families = [graphics_queue_family, present_queue_family];
        let device = Arc::new(Device::new(
            &instance,
            &physical_device,
            &queue_families,
            required_device_extensions,
            &required_device_features,
            with_raytracing_context,
        )?);
        let graphics_queue = device.get_queue(graphics_queue_family, 0);
        let present_queue = device.get_queue(present_queue_family, 0);

        let ray_tracing = with_raytracing_context.then(|| {
            let ray_tracing =
                Arc::new(RayTracingContext::new(&instance, &physical_device, &device));
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

        let command_pool = CommandPool::new(
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
            allocation_sizes: Default::default(),
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
            supported_surface_formats,
            surface,
            instance,
            _entry: entry,
        })
    }
}

fn select_suitable_physical_device(
    devices: &[PhysicalDevice],
    required_extensions: &[&str],
    required_device_features: &DeviceFeatures,
) -> Result<(PhysicalDevice, QueueFamily, QueueFamily)> {
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

impl Context {
    pub fn device_wait_idle(&self) -> Result<()> {
        unsafe { self.device.inner.device_wait_idle()? };

        Ok(())
    }

    pub fn execute_one_time_commands<R, F: FnOnce(&CommandBuffer) -> R>(
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

    pub fn supported_surface_formats(&self) -> &[vk::SurfaceFormatKHR] {
        &self.supported_surface_formats
    }
}
