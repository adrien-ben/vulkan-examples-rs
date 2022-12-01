use anyhow::Result;
use ash::{extensions::khr::Surface as AshSurface, vk, Entry};
use raw_window_handle::{HasRawDisplayHandle, HasRawWindowHandle};

use crate::instance::Instance;

pub struct Surface {
    pub(crate) inner: AshSurface,
    pub surface_khr: vk::SurfaceKHR,
}

impl Surface {
    pub(crate) fn new(
        entry: &Entry,
        instance: &Instance,
        window_handle: &dyn HasRawWindowHandle,
        display_handle: &dyn HasRawDisplayHandle,
    ) -> Result<Self> {
        let inner = AshSurface::new(entry, &instance.inner);
        let surface_khr = unsafe {
            ash_window::create_surface(
                entry,
                &instance.inner,
                display_handle.raw_display_handle(),
                window_handle.raw_window_handle(),
                None,
            )?
        };

        Ok(Self { inner, surface_khr })
    }
}

impl Drop for Surface {
    fn drop(&mut self) {
        unsafe {
            self.inner.destroy_surface(self.surface_khr, None);
        }
    }
}
