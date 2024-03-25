use std::ffi::{c_void, CStr, CString};

use anyhow::Result;
use ash::{extensions::ext::DebugUtils, vk, Entry, Instance as AshInstance};
use raw_window_handle::HasRawDisplayHandle;

use crate::{physical_device::PhysicalDevice, surface::Surface, Version};

pub struct Instance {
    pub(crate) inner: AshInstance,
    debug_utils: DebugUtils,
    debug_utils_messenger: vk::DebugUtilsMessengerEXT,
    physical_devices: Vec<PhysicalDevice>,
}

impl Instance {
    pub(crate) fn new(
        entry: &Entry,
        display_handle: &dyn HasRawDisplayHandle,
        api_version: Version,
        app_name: &str,
        required_extensions: &[&str],
    ) -> Result<Self> {
        // Vulkan instance
        let app_name = CString::new(app_name)?;

        let app_info = vk::ApplicationInfo::builder()
            .application_name(app_name.as_c_str())
            .api_version(api_version.make_api_version());

        // Extension support
        let mut extension_names =
            ash_window::enumerate_required_extensions(display_handle.raw_display_handle())?
                .to_vec();
        extension_names.push(DebugUtils::name().as_ptr());

        let required_extensions_c = required_extensions
            .iter()
            .map(|e| CString::new(*e).unwrap())
            .collect::<Vec<_>>();
        required_extensions_c.iter().for_each(|e| {
            extension_names.push(e.as_ptr());
        });

        check_extensions_support(entry, &extension_names)?;

        let instance_create_info = vk::InstanceCreateInfo::builder()
            .application_info(&app_info)
            .enabled_extension_names(&extension_names);

        let inner = unsafe { entry.create_instance(&instance_create_info, None)? };

        // Vulkan debug report
        let create_info = vk::DebugUtilsMessengerCreateInfoEXT::builder()
            .flags(vk::DebugUtilsMessengerCreateFlagsEXT::empty())
            .message_severity(
                vk::DebugUtilsMessageSeverityFlagsEXT::INFO
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
        surface: &Surface,
    ) -> Result<&[PhysicalDevice]> {
        if self.physical_devices.is_empty() {
            let physical_devices = unsafe { self.inner.enumerate_physical_devices()? };

            let mut physical_devices = physical_devices
                .into_iter()
                .map(|pd| PhysicalDevice::new(&self.inner, surface, pd))
                .collect::<Result<Vec<_>>>()?;

            physical_devices.sort_by_key(|pd| match pd.device_type {
                vk::PhysicalDeviceType::DISCRETE_GPU => 0,
                vk::PhysicalDeviceType::INTEGRATED_GPU => 1,
                _ => 2,
            });

            self.physical_devices = physical_devices;
        }

        Ok(&self.physical_devices)
    }
}

fn check_extensions_support(entry: &Entry, required: &[*const i8]) -> Result<()> {
    let supported = entry.enumerate_instance_extension_properties(None)?;

    for r in required {
        let r = unsafe { CStr::from_ptr(*r) };

        if !supported.iter().any(|s| {
            let s = unsafe { CStr::from_ptr(s.extension_name.as_ptr()) };
            r == s
        }) {
            return Err(anyhow::anyhow!("Unsupported extension: {:?}", r));
        }
    }

    Ok(())
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

impl Drop for Instance {
    fn drop(&mut self) {
        unsafe {
            self.debug_utils
                .destroy_debug_utils_messenger(self.debug_utils_messenger, None);
            self.inner.destroy_instance(None);
        }
    }
}
