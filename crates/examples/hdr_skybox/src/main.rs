use std::mem::{offset_of, size_of, size_of_val};
use std::path::Path;
use std::time::Duration;

use app::anyhow::Result;
use app::glam::Mat4;
use app::vulkan::ash::vk::{self, PipelineBindPoint};
use app::vulkan::gpu_allocator::MemoryLocation;
use app::vulkan::utils::create_gpu_only_buffer_from_data;
use app::vulkan::{
    Buffer, ColorAttachmentsInfo, CommandBuffer, Context, DescriptorPool, DescriptorSet,
    DescriptorSetLayout, GraphicsPipeline, GraphicsPipelineCreateInfo, GraphicsShaderCreateInfo,
    Image, ImageBarrier, ImageView, PipelineLayout, RenderingAttachment, Sampler, Vertex,
    WriteDescriptorSet, WriteDescriptorSetKind,
};
use app::{App, AppConfig, BaseApp};
use gui::egui;
use rfd::FileDialog;

const WIDTH: u32 = 1920;
const HEIGHT: u32 = 1080;
const APP_NAME: &str = "Hdr skybox";

const MIN_NITS: f32 = 0.0;
const MAX_NITS: f32 = 2000.0;

const HDR_FRAMEBUFFER_FORMAT: vk::Format = vk::Format::R16G16B16A16_SFLOAT;

const SDR_SURFACE_FORMAT: vk::SurfaceFormatKHR = vk::SurfaceFormatKHR {
    format: vk::Format::R8G8B8A8_SRGB,
    color_space: vk::ColorSpaceKHR::SRGB_NONLINEAR,
};
const HDR_SURFACE_FORMAT: vk::SurfaceFormatKHR = vk::SurfaceFormatKHR {
    format: vk::Format::R16G16B16A16_SFLOAT,
    color_space: vk::ColorSpaceKHR::EXTENDED_SRGB_LINEAR_EXT,
};

fn main() -> Result<()> {
    app::run::<Skybox>(
        APP_NAME,
        WIDTH,
        HEIGHT,
        AppConfig {
            required_instance_extensions: &["VK_EXT_swapchain_colorspace"],
            ..Default::default()
        },
    )
}

struct Skybox {
    hdr_enabled: bool,
    app_mode: AppMode,

    skybox_vertex_buffer: Buffer,
    skybox_index_buffer: Buffer,
    skybox_texture: Texture,
    skybox_pass_ubo: Buffer,
    skybox_pass_framebuffer: Texture,
    skybox_pass: Pass,

    quad_vertex_buffer: Buffer,
    quad_index_buffer: Buffer,

    tonemap_pass_ubo: Buffer,
    tonemap_pass: Pass,

    calibration_pass_ubo: Buffer,
    calibration_pass: Pass,
}

impl App for Skybox {
    type Gui = Gui;

    fn new(base: &mut BaseApp<Self>) -> Result<Self> {
        let context = &mut base.context;

        // skybox
        let skybox_vertex_buffer = create_skybox_vertex_buffer(context)?;
        let skybox_index_buffer = create_skybox_index_buffer(context)?;

        let skybox_texture = Texture::from_hdr_file(context, "assets/images/studio_2k.hdr")?;

        let skybox_pass_ubo = context.create_buffer(
            vk::BufferUsageFlags::UNIFORM_BUFFER,
            MemoryLocation::CpuToGpu,
            size_of::<SkyboxUbo>() as _,
        )?;

        let skybox_pass_framebuffer =
            Texture::framebuffer(context, base.swapchain.extent, HDR_FRAMEBUFFER_FORMAT)?;

        let skybox_pass = create_skybox_pass(
            context,
            &skybox_texture,
            &skybox_pass_ubo,
            skybox_pass_framebuffer.image.format,
        )?;

        // fullscreen quad geom
        let quad_vertex_buffer = create_quad_vertex_buffer(context)?;
        let quad_index_buffer = create_quad_index_buffer(context)?;

        // tonemap pass
        let tonemap_pass_ubo = context.create_buffer(
            vk::BufferUsageFlags::UNIFORM_BUFFER,
            MemoryLocation::CpuToGpu,
            size_of::<TonemapUbo>() as _,
        )?;
        let tonemap_pass = create_tonemap_pass(
            context,
            &tonemap_pass_ubo,
            &skybox_pass_framebuffer,
            HDR_FRAMEBUFFER_FORMAT,
        )?;

        // calibration pass
        let calibration_pass_ubo = context.create_buffer(
            vk::BufferUsageFlags::UNIFORM_BUFFER,
            MemoryLocation::CpuToGpu,
            size_of::<CalibrationUbo>() as _,
        )?;
        let calibration_pass =
            create_calibration_pass(context, &calibration_pass_ubo, HDR_FRAMEBUFFER_FORMAT)?;

        Ok(Self {
            hdr_enabled: false,
            app_mode: AppMode::Scene,

            skybox_vertex_buffer,
            skybox_index_buffer,
            skybox_texture,
            skybox_pass_ubo,
            skybox_pass,
            skybox_pass_framebuffer,

            quad_vertex_buffer,
            quad_index_buffer,

            tonemap_pass_ubo,
            tonemap_pass,

            calibration_pass_ubo,
            calibration_pass,
        })
    }

    fn on_recreate_swapchain(&mut self, base: &BaseApp<Self>) -> Result<()> {
        // rebuilt framebuffers
        self.skybox_pass_framebuffer =
            Texture::framebuffer(&base.context, base.swapchain.extent, HDR_FRAMEBUFFER_FORMAT)?;

        // update descriptors sets
        self.tonemap_pass
            .descriptor_set
            .update(&[WriteDescriptorSet {
                binding: 0,
                kind: WriteDescriptorSetKind::CombinedImageSampler {
                    view: &self.skybox_pass_framebuffer.view,
                    sampler: &self.skybox_pass_framebuffer.sampler,
                    layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                },
            }]);

        // rebuild pipelines
        let format = if self.hdr_enabled {
            HDR_FRAMEBUFFER_FORMAT
        } else {
            base.swapchain.format
        };
        self.tonemap_pass.pipeline = create_tonemap_pass_pipeline(
            &base.context,
            &self.tonemap_pass.pipeline_layout,
            format,
        )?;

        self.calibration_pass.pipeline = create_calibration_pass_pipeline(
            &base.context,
            &self.calibration_pass.pipeline_layout,
            format,
        )?;

        Ok(())
    }

    fn update(
        &mut self,
        base: &mut BaseApp<Self>,
        ui: &mut <Self as App>::Gui,
        _: usize,
        _: Duration,
    ) -> Result<()> {
        // toggle hdr
        if self.hdr_enabled != ui.enable_hdr {
            self.hdr_enabled = ui.enable_hdr;

            // reset to scene mode and no tone mapper
            ui.app_mode = AppMode::Scene;
            ui.tonemap_mode = TonemapMode::None;

            // request swapchain chang
            let new_format = if self.hdr_enabled {
                HDR_SURFACE_FORMAT
            } else {
                SDR_SURFACE_FORMAT
            };
            base.request_swapchain_format_change(new_format);
        }

        // open file dialog to select an hdr file
        if ui.open_file_picker {
            if let Some(file) = FileDialog::new().pick_file() {
                log::info!("Loading new environment from file {file:?}");
                match Texture::from_hdr_file(&base.context, file) {
                    Ok(texture) => {
                        self.skybox_pass
                            .descriptor_set
                            .update(&[WriteDescriptorSet {
                                binding: 1,
                                kind: WriteDescriptorSetKind::CombinedImageSampler {
                                    view: &texture.view,
                                    sampler: &texture.sampler,
                                    layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                                },
                            }]);

                        self.skybox_texture = texture;
                    }
                    Err(e) => {
                        log::error!("Failed to load environment: {e}");
                    }
                }
            }
        }

        // update app mode
        self.app_mode = ui.app_mode;

        // update UBOs
        self.skybox_pass_ubo.copy_data_to_buffer(&[SkyboxUbo {
            view_proj_matrix: base.camera.projection_matrix() * base.camera.view_matrix_at_center(),
        }])?;

        self.tonemap_pass_ubo.copy_data_to_buffer(&[TonemapUbo {
            tonemap_mode: ui.tonemap_mode as u32,
        }])?;

        if let AppMode::Calibration(mode) = self.app_mode {
            let calibration_ubo = match mode {
                CalibrationMode::MinNits => CalibrationUbo {
                    user_nits: ui.calibration_min_nits,
                    reference_nits: MIN_NITS,
                },
                CalibrationMode::MaxNits => CalibrationUbo {
                    user_nits: ui.calibration_max_nits,
                    reference_nits: MAX_NITS,
                },
            };
            self.calibration_pass_ubo
                .copy_data_to_buffer(&[calibration_ubo])?;
        }

        Ok(())
    }

    fn record_raster_commands(&self, base: &BaseApp<Self>, image_index: usize) -> Result<()> {
        match self.app_mode {
            AppMode::Scene => {
                // skybox pass outputs to an hdr framebuffer the used for tonemapping
                self.cmd_skybox_pass(&base.command_buffers[image_index]);

                // tonemap pass outputs to hdr framebuffer
                self.cmd_tonemap_pass(
                    &base.command_buffers[image_index],
                    &base.swapchain.views[image_index],
                    base.swapchain.extent,
                );
            }
            AppMode::Calibration(_) => {
                // calibration pass outputs to hdr framebuffer
                self.cmd_calibration_pass(
                    &base.command_buffers[image_index],
                    &base.swapchain.views[image_index],
                    base.swapchain.extent,
                );
            }
        }

        Ok(())
    }
}

impl Skybox {
    fn cmd_skybox_pass(&self, buffer: &CommandBuffer) {
        buffer.pipeline_image_barriers(&[ImageBarrier {
            image: &self.skybox_pass_framebuffer.image,
            old_layout: vk::ImageLayout::UNDEFINED,
            new_layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            src_access_mask: vk::AccessFlags2::SHADER_READ,
            dst_access_mask: vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
            src_stage_mask: vk::PipelineStageFlags2::FRAGMENT_SHADER,
            dst_stage_mask: vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
        }]);

        let extent = self.skybox_pass_framebuffer.image.extent2d();

        buffer.begin_rendering(
            &[RenderingAttachment {
                view: &self.skybox_pass_framebuffer.view,
                load_op: vk::AttachmentLoadOp::DONT_CARE,
                clear_value: None,
            }],
            None,
            extent,
        );
        self.skybox_pass.bind(buffer);
        buffer.bind_vertex_buffer(&self.skybox_vertex_buffer);
        buffer.bind_index_buffer(&self.skybox_index_buffer, vk::IndexType::UINT16);
        buffer.set_viewport(extent);
        buffer.set_scissor(extent);
        buffer.draw_indexed(36);
        buffer.end_rendering();
    }

    fn cmd_tonemap_pass(
        &self,
        buffer: &CommandBuffer,
        target_view: &ImageView,
        target_extent: vk::Extent2D,
    ) {
        buffer.pipeline_image_barriers(&[ImageBarrier {
            image: &self.skybox_pass_framebuffer.image,
            old_layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            new_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            src_access_mask: vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
            dst_access_mask: vk::AccessFlags2::SHADER_READ,
            src_stage_mask: vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
            dst_stage_mask: vk::PipelineStageFlags2::FRAGMENT_SHADER,
        }]);

        self.cmd_fullscreen_pass(buffer, &self.tonemap_pass, target_view, target_extent);
    }

    fn cmd_calibration_pass(
        &self,
        buffer: &CommandBuffer,
        target_view: &ImageView,
        target_extent: vk::Extent2D,
    ) {
        self.cmd_fullscreen_pass(buffer, &self.calibration_pass, target_view, target_extent);
    }

    fn cmd_fullscreen_pass(
        &self,
        buffer: &CommandBuffer,
        pass: &Pass,
        target_view: &ImageView,
        target_extent: vk::Extent2D,
    ) {
        buffer.begin_rendering(
            &[RenderingAttachment {
                view: target_view,
                load_op: vk::AttachmentLoadOp::DONT_CARE,
                clear_value: None,
            }],
            None,
            target_extent,
        );

        pass.bind(buffer);
        buffer.bind_vertex_buffer(&self.quad_vertex_buffer);
        buffer.bind_index_buffer(&self.quad_index_buffer, vk::IndexType::UINT16);
        buffer.set_viewport(target_extent);
        buffer.set_scissor(target_extent);
        buffer.draw_indexed(6);
        buffer.end_rendering();
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum TonemapMode {
    None = 0,
    ACESFilmRec2020,
    ACESFilm,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum AppMode {
    Scene,
    Calibration(CalibrationMode),
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum CalibrationMode {
    MinNits,
    MaxNits,
}

#[derive(Debug, Clone, Copy)]
struct Gui {
    supports_hdr: bool,
    enable_hdr: bool,
    open_file_picker: bool,
    app_mode: AppMode,
    tonemap_mode: TonemapMode,
    calibration_min_nits: f32,
    calibration_max_nits: f32,
}

impl app::Gui for Gui {
    fn new<A: App>(base: &BaseApp<A>) -> Result<Self> {
        let supports_hdr = base
            .context
            .supported_surface_formats()
            .contains(&HDR_SURFACE_FORMAT);

        Ok(Gui {
            supports_hdr,
            enable_hdr: false,
            open_file_picker: false,
            app_mode: AppMode::Scene,
            tonemap_mode: TonemapMode::None,
            calibration_min_nits: 0.0,
            calibration_max_nits: 200.0,
        })
    }

    fn build(&mut self, ctx: &egui::Context) {
        egui::Window::new("Settings").show(ctx, |ui| {
            ui.add_enabled_ui(self.supports_hdr, |ui| {
                ui.checkbox(&mut self.enable_hdr, "Enable HDR");
            });

            self.open_file_picker = ui.button("Pick HDRi file").clicked();
            ui.label(format!("Min nits: {}", self.calibration_min_nits));
            ui.label(format!("Max nits: {}", self.calibration_max_nits));

            ui.separator();
            ui.label("Mode");
            ui.radio_value(&mut self.app_mode, AppMode::Scene, "Scene");
            if self.enable_hdr {
                ui.radio_value(
                    &mut self.app_mode,
                    AppMode::Calibration(CalibrationMode::MinNits),
                    "Calibration min nits",
                );
                ui.radio_value(
                    &mut self.app_mode,
                    AppMode::Calibration(CalibrationMode::MaxNits),
                    "Calibration max nits",
                );
            }

            if let AppMode::Scene = self.app_mode {
                ui.separator();
                ui.label("Tonemapper");
                ui.radio_value(&mut self.tonemap_mode, TonemapMode::None, "None");
                if self.enable_hdr {
                    ui.radio_value(
                        &mut self.tonemap_mode,
                        TonemapMode::ACESFilmRec2020,
                        "ACESFilmRec2020",
                    );
                } else {
                    ui.radio_value(&mut self.tonemap_mode, TonemapMode::ACESFilm, "ACESFilm");
                }
            }

            if let AppMode::Calibration(mode) = self.app_mode {
                ui.separator();
                ui.label("Calibration");
                match mode {
                    CalibrationMode::MinNits => {
                        ui.add(
                            egui::Slider::new(&mut self.calibration_min_nits, 0.0..=100.0)
                                .integer()
                                .text("Min nits"),
                        );
                    }
                    CalibrationMode::MaxNits => {
                        ui.add(
                            egui::Slider::new(&mut self.calibration_max_nits, 0.0..=2000.0)
                                .integer()
                                .text("Max nits"),
                        );
                    }
                }
            }
        });
    }
}

struct Texture {
    image: Image,
    view: ImageView,
    sampler: Sampler,
}

impl Texture {
    fn from_hdr_file<P>(context: &Context, path: P) -> Result<Self>
    where
        P: AsRef<Path>,
    {
        let img = image::open(path)?;
        let width = img.width();
        let height = img.height();
        let pixels = img.into_rgba32f().into_raw();

        let staging = context.create_buffer(
            vk::BufferUsageFlags::TRANSFER_SRC,
            MemoryLocation::CpuToGpu,
            size_of_val(pixels.as_slice()) as _,
        )?;

        staging.copy_data_to_buffer(&pixels)?;

        let image = context.create_image(
            vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::SAMPLED,
            MemoryLocation::GpuOnly,
            vk::Format::R32G32B32A32_SFLOAT,
            width,
            height,
        )?;

        context.execute_one_time_commands(|cmd| {
            cmd.pipeline_image_barriers(&[ImageBarrier {
                image: &image,
                old_layout: vk::ImageLayout::UNDEFINED,
                new_layout: vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                src_access_mask: vk::AccessFlags2::NONE,
                dst_access_mask: vk::AccessFlags2::TRANSFER_WRITE,
                src_stage_mask: vk::PipelineStageFlags2::NONE,
                dst_stage_mask: vk::PipelineStageFlags2::TRANSFER,
            }]);

            cmd.copy_buffer_to_image(&staging, &image, vk::ImageLayout::TRANSFER_DST_OPTIMAL);

            cmd.pipeline_image_barriers(&[ImageBarrier {
                image: &image,
                old_layout: vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                new_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                src_access_mask: vk::AccessFlags2::TRANSFER_WRITE,
                dst_access_mask: vk::AccessFlags2::SHADER_READ,
                src_stage_mask: vk::PipelineStageFlags2::TRANSFER,
                dst_stage_mask: vk::PipelineStageFlags2::FRAGMENT_SHADER,
            }]);
        })?;

        let view = image.create_image_view(vk::ImageAspectFlags::COLOR)?;
        let sampler = context.create_sampler(&Default::default())?;

        Ok(Self {
            image,
            view,
            sampler,
        })
    }

    fn framebuffer(context: &Context, extent: vk::Extent2D, format: vk::Format) -> Result<Self> {
        let image = context.create_image(
            vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED,
            MemoryLocation::GpuOnly,
            format,
            extent.width,
            extent.height,
        )?;

        let view = image.create_image_view(vk::ImageAspectFlags::COLOR)?;

        let sampler = context.create_sampler(&Default::default())?;

        Ok(Self {
            image,
            view,
            sampler,
        })
    }
}

struct Pass {
    _dsl: DescriptorSetLayout,
    _descriptor_pool: DescriptorPool,
    descriptor_set: DescriptorSet,
    pipeline_layout: PipelineLayout,
    pipeline: GraphicsPipeline,
}

impl Pass {
    fn bind(&self, buffer: &CommandBuffer) {
        buffer.bind_graphics_pipeline(&self.pipeline);
        buffer.bind_descriptor_sets(
            PipelineBindPoint::GRAPHICS,
            &self.pipeline_layout,
            0,
            &[&self.descriptor_set],
        );
    }
}

#[derive(Clone, Copy)]
#[allow(dead_code)]
#[repr(C)]
struct SkyboxUbo {
    view_proj_matrix: Mat4,
}

#[derive(Debug, Clone, Copy)]
#[repr(C)]
#[allow(dead_code)]
struct SkyboxVertex {
    position: [f32; 3],
}

impl Vertex for SkyboxVertex {
    fn bindings() -> Vec<vk::VertexInputBindingDescription> {
        vec![vk::VertexInputBindingDescription {
            binding: 0,
            stride: size_of::<SkyboxVertex>() as _,
            input_rate: vk::VertexInputRate::VERTEX,
        }]
    }

    fn attributes() -> Vec<vk::VertexInputAttributeDescription> {
        vec![vk::VertexInputAttributeDescription {
            binding: 0,
            location: 0,
            format: vk::Format::R32G32B32_SFLOAT,
            offset: offset_of!(SkyboxVertex, position) as _,
        }]
    }
}

fn create_skybox_vertex_buffer(context: &Context) -> Result<Buffer> {
    let vertices: [SkyboxVertex; 8] = [
        SkyboxVertex {
            position: [-1.0, -1.0, -1.0],
        },
        SkyboxVertex {
            position: [1.0, -1.0, -1.0],
        },
        SkyboxVertex {
            position: [-1.0, 1.0, -1.0],
        },
        SkyboxVertex {
            position: [1.0, 1.0, -1.0],
        },
        SkyboxVertex {
            position: [-1.0, -1.0, 1.0],
        },
        SkyboxVertex {
            position: [1.0, -1.0, 1.0],
        },
        SkyboxVertex {
            position: [-1.0, 1.0, 1.0],
        },
        SkyboxVertex {
            position: [1.0, 1.0, 1.0],
        },
    ];

    let vertex_buffer =
        create_gpu_only_buffer_from_data(context, vk::BufferUsageFlags::VERTEX_BUFFER, &vertices)?;

    Ok(vertex_buffer)
}

fn create_skybox_index_buffer(context: &Context) -> Result<Buffer> {
    #[rustfmt::skip]
    let indices: [u16; 36] = [
        0, 1, 2, 2, 1, 3,
        4, 0, 6, 6, 0, 2,
        5, 4, 7, 7, 4, 6,
        1, 5, 3, 3, 5, 7,
        2, 3, 6, 6, 3, 7, 
        4, 5, 0, 0, 5, 1,
    ];
    let buffer =
        create_gpu_only_buffer_from_data(context, vk::BufferUsageFlags::INDEX_BUFFER, &indices)?;
    Ok(buffer)
}

fn create_skybox_pass(
    context: &Context,
    texture: &Texture,
    ubo_buffer: &Buffer,
    color_attachment_format: vk::Format,
) -> Result<Pass> {
    let bindings = [
        vk::DescriptorSetLayoutBinding::builder()
            .binding(0)
            .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::VERTEX)
            .build(),
        vk::DescriptorSetLayoutBinding::builder()
            .binding(1)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT)
            .build(),
    ];
    let dsl = context.create_descriptor_set_layout(&bindings)?;

    let pool_sizes = [
        vk::DescriptorPoolSize::builder()
            .ty(vk::DescriptorType::UNIFORM_BUFFER)
            .descriptor_count(1)
            .build(),
        vk::DescriptorPoolSize::builder()
            .ty(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .descriptor_count(1)
            .build(),
    ];

    let descriptor_pool = context.create_descriptor_pool(2, &pool_sizes)?;
    let descriptor_set = descriptor_pool.allocate_set(&dsl)?;

    descriptor_set.update(&[
        WriteDescriptorSet {
            binding: 0,
            kind: WriteDescriptorSetKind::UniformBuffer { buffer: ubo_buffer },
        },
        WriteDescriptorSet {
            binding: 1,
            kind: WriteDescriptorSetKind::CombinedImageSampler {
                view: &texture.view,
                sampler: &texture.sampler,
                layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            },
        },
    ]);

    let pipeline_layout = context.create_pipeline_layout(&[&dsl])?;

    let pipeline = context.create_graphics_pipeline::<SkyboxVertex>(
        &pipeline_layout,
        GraphicsPipelineCreateInfo {
            shaders: &[
                GraphicsShaderCreateInfo {
                    source: &include_bytes!("../shaders/skybox.vert.spv")[..],
                    stage: vk::ShaderStageFlags::VERTEX,
                },
                GraphicsShaderCreateInfo {
                    source: &include_bytes!("../shaders/skybox.frag.spv")[..],
                    stage: vk::ShaderStageFlags::FRAGMENT,
                },
            ],
            primitive_topology: vk::PrimitiveTopology::TRIANGLE_LIST,
            cull_mode: vk::CullModeFlags::BACK,
            extent: None,
            color_attachments: ColorAttachmentsInfo {
                formats: &[color_attachment_format],
                blends: &[vk::PipelineColorBlendAttachmentState {
                    color_write_mask: vk::ColorComponentFlags::RGBA,
                    ..Default::default()
                }],
            },
            depth: None,
            dynamic_states: Some(&[vk::DynamicState::SCISSOR, vk::DynamicState::VIEWPORT]),
        },
    )?;

    Ok(Pass {
        _dsl: dsl,
        _descriptor_pool: descriptor_pool,
        descriptor_set,
        pipeline_layout,
        pipeline,
    })
}

#[derive(Debug, Clone, Copy)]
#[repr(C)]
#[allow(dead_code)]
struct QuadVertex {
    position: [f32; 2],
    uv: [f32; 2],
}

impl Vertex for QuadVertex {
    fn bindings() -> Vec<vk::VertexInputBindingDescription> {
        vec![vk::VertexInputBindingDescription {
            binding: 0,
            stride: size_of::<QuadVertex>() as _,
            input_rate: vk::VertexInputRate::VERTEX,
        }]
    }

    fn attributes() -> Vec<vk::VertexInputAttributeDescription> {
        vec![
            vk::VertexInputAttributeDescription {
                binding: 0,
                location: 0,
                format: vk::Format::R32G32_SFLOAT,
                offset: offset_of!(QuadVertex, position) as _,
            },
            vk::VertexInputAttributeDescription {
                binding: 0,
                location: 1,
                format: vk::Format::R32G32_SFLOAT,
                offset: offset_of!(QuadVertex, uv) as _,
            },
        ]
    }
}

fn create_quad_vertex_buffer(context: &Context) -> Result<Buffer> {
    let vertices: [QuadVertex; 4] = [
        QuadVertex {
            position: [-1.0, 1.0],
            uv: [0.0, 1.0],
        },
        QuadVertex {
            position: [1.0, 1.0],
            uv: [1.0, 1.0],
        },
        QuadVertex {
            position: [-1.0, -1.0],
            uv: [0.0, 0.0],
        },
        QuadVertex {
            position: [1.0, -1.0],
            uv: [1.0, 0.0],
        },
    ];

    let vertex_buffer =
        create_gpu_only_buffer_from_data(context, vk::BufferUsageFlags::VERTEX_BUFFER, &vertices)?;

    Ok(vertex_buffer)
}

fn create_quad_index_buffer(context: &Context) -> Result<Buffer> {
    #[rustfmt::skip]
    let indices: [u16; 6] = [
        0, 1, 2, 2, 1, 3,
    ];
    let buffer =
        create_gpu_only_buffer_from_data(context, vk::BufferUsageFlags::INDEX_BUFFER, &indices)?;
    Ok(buffer)
}

#[derive(Clone, Copy)]
#[allow(dead_code)]
#[repr(C)]
struct TonemapUbo {
    tonemap_mode: u32,
}

fn create_tonemap_pass(
    context: &Context,
    ubo: &Buffer,
    skybox_framebuffer: &Texture,
    color_attachment_format: vk::Format,
) -> Result<Pass> {
    let bindings = [
        vk::DescriptorSetLayoutBinding::builder()
            .binding(0)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT)
            .build(),
        vk::DescriptorSetLayoutBinding::builder()
            .binding(1)
            .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT)
            .build(),
    ];
    let dsl = context.create_descriptor_set_layout(&bindings)?;

    let pool_sizes = [
        vk::DescriptorPoolSize::builder()
            .ty(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .descriptor_count(1)
            .build(),
        vk::DescriptorPoolSize::builder()
            .ty(vk::DescriptorType::UNIFORM_BUFFER)
            .descriptor_count(1)
            .build(),
    ];

    let descriptor_pool = context.create_descriptor_pool(1, &pool_sizes)?;
    let descriptor_set = descriptor_pool.allocate_set(&dsl)?;

    descriptor_set.update(&[
        WriteDescriptorSet {
            binding: 0,
            kind: WriteDescriptorSetKind::CombinedImageSampler {
                view: &skybox_framebuffer.view,
                sampler: &skybox_framebuffer.sampler,
                layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            },
        },
        WriteDescriptorSet {
            binding: 1,
            kind: WriteDescriptorSetKind::UniformBuffer { buffer: ubo },
        },
    ]);

    let pipeline_layout = context.create_pipeline_layout(&[&dsl])?;

    let pipeline =
        create_tonemap_pass_pipeline(context, &pipeline_layout, color_attachment_format)?;

    Ok(Pass {
        _dsl: dsl,
        _descriptor_pool: descriptor_pool,
        descriptor_set,
        pipeline_layout,
        pipeline,
    })
}

fn create_tonemap_pass_pipeline(
    context: &Context,
    layout: &PipelineLayout,
    color_attachment_format: vk::Format,
) -> Result<GraphicsPipeline> {
    let pipeline = context.create_graphics_pipeline::<QuadVertex>(
        layout,
        GraphicsPipelineCreateInfo {
            shaders: &[
                GraphicsShaderCreateInfo {
                    source: &include_bytes!("../shaders/fullscreen.vert.spv")[..],
                    stage: vk::ShaderStageFlags::VERTEX,
                },
                GraphicsShaderCreateInfo {
                    source: &include_bytes!("../shaders/tonemap.frag.spv")[..],
                    stage: vk::ShaderStageFlags::FRAGMENT,
                },
            ],
            primitive_topology: vk::PrimitiveTopology::TRIANGLE_LIST,
            cull_mode: vk::CullModeFlags::BACK,
            extent: None,
            color_attachments: ColorAttachmentsInfo {
                formats: &[color_attachment_format],
                blends: &[vk::PipelineColorBlendAttachmentState {
                    color_write_mask: vk::ColorComponentFlags::RGBA,
                    ..Default::default()
                }],
            },
            depth: None,
            dynamic_states: Some(&[vk::DynamicState::SCISSOR, vk::DynamicState::VIEWPORT]),
        },
    )?;

    Ok(pipeline)
}

#[derive(Clone, Copy)]
#[allow(dead_code)]
#[repr(C)]
struct CalibrationUbo {
    user_nits: f32,
    reference_nits: f32,
}

fn create_calibration_pass(
    context: &Context,
    ubo: &Buffer,
    color_attachment_format: vk::Format,
) -> Result<Pass> {
    let bindings = [vk::DescriptorSetLayoutBinding::builder()
        .binding(0)
        .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
        .descriptor_count(1)
        .stage_flags(vk::ShaderStageFlags::FRAGMENT)
        .build()];
    let dsl = context.create_descriptor_set_layout(&bindings)?;

    let pool_sizes = [vk::DescriptorPoolSize::builder()
        .ty(vk::DescriptorType::UNIFORM_BUFFER)
        .descriptor_count(1)
        .build()];

    let descriptor_pool = context.create_descriptor_pool(1, &pool_sizes)?;
    let descriptor_set = descriptor_pool.allocate_set(&dsl)?;

    descriptor_set.update(&[WriteDescriptorSet {
        binding: 0,
        kind: WriteDescriptorSetKind::UniformBuffer { buffer: ubo },
    }]);

    let pipeline_layout = context.create_pipeline_layout(&[&dsl])?;

    let pipeline =
        create_calibration_pass_pipeline(context, &pipeline_layout, color_attachment_format)?;

    Ok(Pass {
        _dsl: dsl,
        _descriptor_pool: descriptor_pool,
        descriptor_set,
        pipeline_layout,
        pipeline,
    })
}

fn create_calibration_pass_pipeline(
    context: &Context,
    layout: &PipelineLayout,
    color_attachment_format: vk::Format,
) -> Result<GraphicsPipeline> {
    let pipeline = context.create_graphics_pipeline::<QuadVertex>(
        layout,
        GraphicsPipelineCreateInfo {
            shaders: &[
                GraphicsShaderCreateInfo {
                    source: &include_bytes!("../shaders/fullscreen.vert.spv")[..],
                    stage: vk::ShaderStageFlags::VERTEX,
                },
                GraphicsShaderCreateInfo {
                    source: &include_bytes!("../shaders/calibration.frag.spv")[..],
                    stage: vk::ShaderStageFlags::FRAGMENT,
                },
            ],
            primitive_topology: vk::PrimitiveTopology::TRIANGLE_LIST,
            cull_mode: vk::CullModeFlags::BACK,
            extent: None,
            color_attachments: ColorAttachmentsInfo {
                formats: &[color_attachment_format],
                blends: &[vk::PipelineColorBlendAttachmentState {
                    color_write_mask: vk::ColorComponentFlags::RGBA,
                    ..Default::default()
                }],
            },
            depth: None,
            dynamic_states: Some(&[vk::DynamicState::SCISSOR, vk::DynamicState::VIEWPORT]),
        },
    )?;

    Ok(pipeline)
}
