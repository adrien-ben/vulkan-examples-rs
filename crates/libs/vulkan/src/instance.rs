use std::ffi::{c_void, CStr, CString};

use anyhow::Result;
use ash::{extensions::ext::DebugUtils, vk, Entry, Instance};
use raw_window_handle::HasRawWindowHandle;

use crate::{physical_device::VkPhysicalDevice, surface::VkSurface, VkVersion};

pub struct VkInstance {
    pub(crate) inner: Instance,
    debug_utils: DebugUtils,
    debug_utils_messenger: vk::DebugUtilsMessengerEXT,
    physical_devices: Vec<VkPhysicalDevice>,
}

impl VkInstance {
    pub(crate) fn new(
        entry: &Entry,
        window: &dyn HasRawWindowHandle,
        api_version: VkVersion,
        app_name: Option<&str>,
    ) -> Result<Self> {
        // Vulkan instance
        let app_name = app_name.unwrap_or("no name");
        let app_name = CString::new(app_name)?;

        let app_info = vk::ApplicationInfo::builder()
            .application_name(app_name.as_c_str())
            .api_version(api_version.make_api_version());

        let mut extension_names = ash_window::enumerate_required_extensions(&window)?.to_vec();
        extension_names.push(DebugUtils::name().as_ptr());

        let instance_create_info = vk::InstanceCreateInfo::builder()
            .application_info(&app_info)
            .enabled_extension_names(&extension_names);

        let inner = unsafe { entry.create_instance(&instance_create_info, None)? };

        // Vulkan debug report
        let create_info = vk::DebugUtilsMessengerCreateInfoEXT::builder()
            .flags(vk::DebugUtilsMessengerCreateFlagsEXT::empty())
            .message_severity(
                vk::DebugUtilsMessageSeverityFlagsEXT::VERBOSE
                    | vk::DebugUtilsMessageSeverityFlagsEXT::INFO
                    | vk::DebugUtilsMessageSeverityFlagsEXT::WARNING
                    | vk::DebugUtilsMessageSeverityFlagsEXT::ERROR,
            )
            .message_type(
                vk::DebugUtilsMessageTypeFlagsEXT::GENERAL
                    | vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION
                    | vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE,
            )
            .pfn_user_callback(Some(vulkan_debug_callback));
        let debug_utils = DebugUtils::new(entry, &inner);
        let debug_utils_messenger =
            unsafe { debug_utils.create_debug_utils_messenger(&create_info, None)? };

        Ok(Self {
            inner,
            debug_utils,
            debug_utils_messenger,
            physical_devices: vec![],
        })
    }

    pub(crate) fn enumerate_physical_devices(
        &mut self,
        surface: &VkSurface,
    ) -> Result<&[VkPhysicalDevice]> {
        if self.physical_devices.is_empty() {
            let physical_devices = unsafe { self.inner.enumerate_physical_devices()? };

            let physical_devices = physical_devices
                .into_iter()
                .map(|pd| VkPhysicalDevice::new(&self.inner, surface, pd))
                .collect::<Result<_>>()?;

            self.physical_devices = physical_devices;
        }

        Ok(&self.physical_devices)
    }
}

unsafe extern "system" fn vulkan_debug_callback(
    flag: vk::DebugUtilsMessageSeverityFlagsEXT,
    typ: vk::DebugUtilsMessageTypeFlagsEXT,
    p_callback_data: *const vk::DebugUtilsMessengerCallbackDataEXT,
    _: *mut c_void,
) -> vk::Bool32 {
    use vk::DebugUtilsMessageSeverityFlagsEXT as Flag;

    let message = CStr::from_ptr((*p_callback_data).p_message);
    match flag {
        Flag::VERBOSE => log::debug!("{:?} - {:?}", typ, message),
        Flag::INFO => log::info!("{:?} - {:?}", typ, message),
        Flag::WARNING => log::warn!("{:?} - {:?}", typ, message),
        _ => log::error!("{:?} - {:?}", typ, message),
    }
    vk::FALSE
}

impl Drop for VkInstance {
    fn drop(&mut self) {
        unsafe {
            self.debug_utils
                .destroy_debug_utils_messenger(self.debug_utils_messenger, None);
            self.inner.destroy_instance(None);
        }
    }
}
