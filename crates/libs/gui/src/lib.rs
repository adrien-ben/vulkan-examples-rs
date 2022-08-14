pub extern crate imgui;
pub extern crate imgui_rs_vulkan_renderer;
pub extern crate imgui_winit_support;

use std::time::Duration;

use anyhow::Result;
use imgui::{Context, DrawData, FontConfig, FontSource};
use imgui_rs_vulkan_renderer::{Options, Renderer};
use imgui_winit_support::{HiDpiMode, WinitPlatform};
use vulkan::{ash::vk, VkCommandBuffer, VkCommandPool, VkContext};
use winit::{event::Event, window::Window};

pub struct GuiContext {
    pub imgui: Context,
    pub platform: WinitPlatform,
    pub renderer: Renderer,
}

impl GuiContext {
    pub fn new(
        context: &VkContext,
        command_pool: &VkCommandPool,
        format: vk::Format,
        window: &Window,
        in_flight_frames: usize,
    ) -> Result<Self> {
        let mut imgui = Context::create();
        imgui.set_ini_filename(None);

        let mut platform = WinitPlatform::init(&mut imgui);

        let hidpi_factor = platform.hidpi_factor();
        let font_size = (13.0 * hidpi_factor) as f32;
        imgui.fonts().add_font(&[
            FontSource::DefaultFontData {
                config: Some(FontConfig {
                    size_pixels: font_size,
                    ..FontConfig::default()
                }),
            },
            FontSource::TtfData {
                data: include_bytes!("../../../../assets/fonts/mplus-1p-regular.ttf"),
                size_pixels: font_size,
                config: Some(FontConfig {
                    rasterizer_multiply: 1.75,
                    ..FontConfig::default()
                }),
            },
        ]);
        imgui.io_mut().font_global_scale = (1.0 / hidpi_factor) as f32;
        platform.attach_window(imgui.io_mut(), window, HiDpiMode::Rounded);

        let gui_renderer = Renderer::with_gpu_allocator(
            context.allocator.clone(),
            context.device.inner.clone(),
            context.graphics_queue.inner,
            command_pool.inner,
            format,
            &mut imgui,
            Some(Options {
                in_flight_frames,
                ..Default::default()
            }),
        )?;

        Ok(Self {
            imgui,
            platform,
            renderer: gui_renderer,
        })
    }

    pub fn handle_event<T>(&mut self, window: &Window, event: &Event<T>) {
        self.platform
            .handle_event(self.imgui.io_mut(), window, event);
    }

    pub fn update_delta_time(&mut self, delta: Duration) {
        self.imgui.io_mut().update_delta_time(delta);
    }

    pub fn cmd_draw(&mut self, buffer: &VkCommandBuffer, draw_data: &DrawData) -> Result<()> {
        self.renderer.cmd_draw(buffer.inner, draw_data)?;

        Ok(())
    }
}
