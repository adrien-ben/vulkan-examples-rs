use anyhow::Result;
use ash::{extensions::khr::Surface as AshSurface, vk, Entry};
use raw_window_handle::HasRawWindowHandle;

use crate::instance::Instance;

pub struct Surface {
    pub(crate) inner: AshSurface,
    pub surface_khr: vk::SurfaceKHR,
}

impl Surface {
    pub(crate) fn new(
        entry: &Entry,
        instance: &Instance,
        window: &dyn HasRawWindowHandle,
    ) -> Result<Self> {
        let inner = AshSurface::new(entry, &instance.inner);
        let surface_khr =
            unsafe { ash_window::create_surface(entry, &instance.inner, &window, None)? };

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
