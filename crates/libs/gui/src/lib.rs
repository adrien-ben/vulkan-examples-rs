pub extern crate egui;
pub extern crate egui_ash_renderer;
pub extern crate egui_winit;

use anyhow::Result;
use egui::{
    epaint::{ClippedShape, ImageDelta},
    ClippedPrimitive, Context as EguiContext, FullOutput, PlatformOutput, RawInput, TextureId,
    ViewportId,
};
use egui_ash_renderer::{DynamicRendering, Options, Renderer};
use egui_winit::State as EguiWinit;
use vulkan::{ash::vk, CommandBuffer, Context as VkContext};
use winit::{event::WindowEvent, window::Window};

pub struct GuiContext {
    pub egui: EguiContext,
    pub egui_winit: EguiWinit,
    pub renderer: Renderer,
}

impl GuiContext {
    pub fn new(
        context: &VkContext,
        format: vk::Format,
        window: &Window,
        in_flight_frames: usize,
    ) -> Result<Self> {
        let egui = EguiContext::default();
        let platform = EguiWinit::new(egui.clone(), ViewportId::ROOT, &window, None, None);

        let gui_renderer = Renderer::with_gpu_allocator(
            context.allocator.clone(),
            context.device.inner.clone(),
            DynamicRendering {
                color_attachment_format: format,
                depth_attachment_format: None,
            },
            Options {
                in_flight_frames,
                srgb_framebuffer: true,
                ..Default::default()
            },
        )?;

        Ok(Self {
            egui,
            egui_winit: platform,
            renderer: gui_renderer,
        })
    }

    pub fn handle_event(&mut self, window: &Window, event: &WindowEvent) {
        let _ = self.egui_winit.on_window_event(window, event);
    }

    pub fn take_input(&mut self, window: &Window) -> RawInput {
        self.egui_winit.take_egui_input(window)
    }

    pub fn run(&self, new_input: RawInput, run_ui: impl FnOnce(&egui::Context)) -> FullOutput {
        self.egui.run(new_input, run_ui)
    }

    pub fn handle_platform_output(&mut self, window: &Window, platform_output: PlatformOutput) {
        self.egui_winit
            .handle_platform_output(window, platform_output)
    }

    pub fn set_textures(
        &mut self,
        queue: vk::Queue,
        command_pool: vk::CommandPool,
        textures_delta: &[(TextureId, ImageDelta)],
    ) -> Result<()> {
        self.renderer
            .set_textures(queue, command_pool, textures_delta)?;

        Ok(())
    }

    pub fn free_textures(&mut self, ids: &[TextureId]) -> Result<()> {
        self.renderer.free_textures(ids)?;

        Ok(())
    }

    pub fn tessellate(
        &self,
        shapes: Vec<ClippedShape>,
        pixels_per_point: f32,
    ) -> Vec<ClippedPrimitive> {
        self.egui.tessellate(shapes, pixels_per_point)
    }

    pub fn cmd_draw(
        &mut self,
        command_buffer: &CommandBuffer,
        extent: vk::Extent2D,
        pixels_per_point: f32,
        primitives: &[ClippedPrimitive],
    ) -> Result<()> {
        self.renderer
            .cmd_draw(command_buffer.inner, extent, pixels_per_point, primitives)?;

        Ok(())
    }
}
