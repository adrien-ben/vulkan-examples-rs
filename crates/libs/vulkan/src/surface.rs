use anyhow::Result;
use ash::{extensions::khr::Surface, vk, Entry};
use raw_window_handle::HasRawWindowHandle;

use crate::instance::VkInstance;

pub struct VkSurface {
    pub(crate) inner: Surface,
    pub surface_khr: vk::SurfaceKHR,
}

impl VkSurface {
    pub(crate) fn new(
        entry: &Entry,
        instance: &VkInstance,
        window: &dyn HasRawWindowHandle,
    ) -> Result<Self> {
        let inner = Surface::new(entry, &instance.inner);
        let surface_khr =
            unsafe { ash_window::create_surface(entry, &instance.inner, &window, None)? };

        Ok(Self { inner, surface_khr })
    }
}

impl Drop for VkSurface {
    fn drop(&mut self) {
        unsafe {
            self.inner.destroy_surface(self.surface_khr, None);
        }
    }
}
