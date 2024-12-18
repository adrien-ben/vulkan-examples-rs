use std::mem::{offset_of, size_of};
use std::time::Duration;

use app::anyhow::Result;
use app::glam::{Mat4, Vec3};
use app::vulkan::ash::vk::{self, PipelineBindPoint};
use app::vulkan::gpu_allocator::MemoryLocation;
use app::vulkan::utils::{compute_aligned_size_of, create_gpu_only_buffer_from_data};
use app::vulkan::{
    Buffer, ClearValue, ColorAttachmentsInfo, Context, DepthInfo, DescriptorPool, DescriptorSet,
    DescriptorSetLayout, GraphicsPipeline, GraphicsPipelineCreateInfo, GraphicsShaderCreateInfo,
    Image, ImageBarrier, ImageView, PipelineLayout, RenderingAttachment, Sampler,
    WriteDescriptorSet, WriteDescriptorSetKind,
};
use app::{App, AppConfig, BaseApp};
use gui::egui::{self, Widget};

const WIDTH: u32 = 1920;
const HEIGHT: u32 = 1080;
const APP_NAME: &str = "Weighted, Blended Order-Independent Transparency";

const MAX_INSTANCES: usize = 10;

const DEPTH_BUFFER_FORMAT: vk::Format = vk::Format::D32_SFLOAT;
const WEIGHT_COLORS_FB_FORMAT: vk::Format = vk::Format::R16G16B16A16_SFLOAT;
const REVEAL_FB_FORMAT: vk::Format = vk::Format::R8_UNORM;

fn main() -> Result<()> {
    app::run::<Triangle>(
        APP_NAME,
        WIDTH,
        HEIGHT,
        AppConfig {
            enable_independent_blend: true,
            ..Default::default()
        },
    )
}
struct Triangle {
    instances: Vec<InstanceUbo>,

    frame_ubo: Buffer,
    instance_ubo: Buffer,
    ubo_alignment: vk::DeviceSize,
    vertex_buffer: Buffer,
    opaque_pass: Pass,
    depth_buffer: Texture,

    transparent_pass: Pass,
    weighted_colors_fb: Texture,
    reveal_fb: Texture,

    quad_vertex_buffer: Buffer,

    composite_pass: Pass,
}

impl App for Triangle {
    type Gui = Gui;

    fn new(base: &mut BaseApp) -> Result<Self> {
        let context = &mut base.context;
        base.camera.position = Vec3::new(1.6, 0.06, 1.95);
        base.camera.direction = -base.camera.position;

        let frame_ubo = context.create_buffer(
            vk::BufferUsageFlags::UNIFORM_BUFFER,
            MemoryLocation::CpuToGpu,
            size_of::<FrameUbo>() as _,
        )?;
        let ubo_alignment = context
            .physical_device_limits()
            .min_uniform_buffer_offset_alignment;
        let instance_ubo = context.create_buffer(
            vk::BufferUsageFlags::UNIFORM_BUFFER,
            MemoryLocation::CpuToGpu,
            MAX_INSTANCES as vk::DeviceSize * compute_aligned_size_of::<InstanceUbo>(ubo_alignment),
        )?;

        let vertex_buffer = create_vertex_buffer(context)?;

        let geometry_pass =
            create_opaque_pass(context, &frame_ubo, &instance_ubo, base.swapchain.format)?;

        let transparent_pass = create_transparent_pass(context, &frame_ubo, &instance_ubo)?;

        let depth_buffer = Texture::create_framebuffer(
            context,
            vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT,
            base.swapchain.extent,
            DEPTH_BUFFER_FORMAT,
            vk::ImageAspectFlags::DEPTH,
            false,
        )?;

        let weighted_colors_fb = Texture::create_framebuffer(
            context,
            vk::ImageUsageFlags::COLOR_ATTACHMENT,
            base.swapchain.extent,
            WEIGHT_COLORS_FB_FORMAT,
            vk::ImageAspectFlags::COLOR,
            true,
        )?;

        let reveal_fb = Texture::create_framebuffer(
            context,
            vk::ImageUsageFlags::COLOR_ATTACHMENT,
            base.swapchain.extent,
            REVEAL_FB_FORMAT,
            vk::ImageAspectFlags::COLOR,
            true,
        )?;

        let quad_vertex_buffer = create_quad_vertex_buffer(context)?;
        let composite_pass = create_composite_pass(
            context,
            &weighted_colors_fb,
            &reveal_fb,
            base.swapchain.format,
        )?;

        Ok(Self {
            instances: vec![],

            frame_ubo,
            instance_ubo,
            ubo_alignment,
            vertex_buffer,
            opaque_pass: geometry_pass,
            depth_buffer,

            transparent_pass,
            weighted_colors_fb,
            reveal_fb,

            quad_vertex_buffer,
            composite_pass,
        })
    }

    fn on_recreate_swapchain(&mut self, base: &BaseApp) -> Result<()> {
        self.depth_buffer = Texture::create_framebuffer(
            &base.context,
            vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT,
            base.swapchain.extent,
            DEPTH_BUFFER_FORMAT,
            vk::ImageAspectFlags::DEPTH,
            false,
        )?;

        self.weighted_colors_fb = Texture::create_framebuffer(
            &base.context,
            vk::ImageUsageFlags::COLOR_ATTACHMENT,
            base.swapchain.extent,
            WEIGHT_COLORS_FB_FORMAT,
            vk::ImageAspectFlags::COLOR,
            true,
        )?;

        self.reveal_fb = Texture::create_framebuffer(
            &base.context,
            vk::ImageUsageFlags::COLOR_ATTACHMENT,
            base.swapchain.extent,
            REVEAL_FB_FORMAT,
            vk::ImageAspectFlags::COLOR,
            true,
        )?;

        self.composite_pass.descriptor_set.update(&[
            WriteDescriptorSet {
                binding: 0,
                kind: WriteDescriptorSetKind::CombinedImageSampler {
                    view: &self.weighted_colors_fb.view,
                    sampler: self
                        .weighted_colors_fb
                        .sampler
                        .as_ref()
                        .expect("weighted_colors_fb should have a sampler"),
                    layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                },
            },
            WriteDescriptorSet {
                binding: 1,
                kind: WriteDescriptorSetKind::CombinedImageSampler {
                    view: &self.reveal_fb.view,
                    sampler: self
                        .reveal_fb
                        .sampler
                        .as_ref()
                        .expect("reveal_fb should have a sampler"),
                    layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                },
            },
        ]);

        Ok(())
    }

    fn update(
        &mut self,
        base: &mut BaseApp,
        ui: &mut <Self as App>::Gui,
        _: usize,
        _: Duration,
    ) -> Result<()> {
        self.instances.clear();
        self.instances.extend_from_slice(&ui.instances);

        self.instance_ubo
            .copy_data_to_buffer_with_alignment(&self.instances, self.ubo_alignment)?;

        self.frame_ubo.copy_data_to_buffer(&[FrameUbo {
            view_proj_matrix: base.camera.projection_matrix() * base.camera.view_matrix(),
        }])?;

        Ok(())
    }

    fn record_raster_commands(&self, base: &BaseApp, image_index: usize) -> Result<()> {
        let buffer = &base.command_buffers[image_index];

        buffer.pipeline_image_barriers(&[
            ImageBarrier {
                image: &self.weighted_colors_fb.image,
                old_layout: vk::ImageLayout::UNDEFINED,
                new_layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                src_access_mask: vk::AccessFlags2::SHADER_READ,
                dst_access_mask: vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
                src_stage_mask: vk::PipelineStageFlags2::FRAGMENT_SHADER,
                dst_stage_mask: vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
            },
            ImageBarrier {
                image: &self.reveal_fb.image,
                old_layout: vk::ImageLayout::UNDEFINED,
                new_layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                src_access_mask: vk::AccessFlags2::SHADER_READ,
                dst_access_mask: vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
                src_stage_mask: vk::PipelineStageFlags2::FRAGMENT_SHADER,
                dst_stage_mask: vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
            },
        ]);

        // opaque pass
        buffer.begin_rendering(
            &[RenderingAttachment {
                view: &base.swapchain.views[image_index],
                load_op: vk::AttachmentLoadOp::CLEAR,
                clear_value: Some(ClearValue::ColorFloat([0.0, 0.0, 0.0, 1.0])),
            }],
            Some(RenderingAttachment {
                view: &self.depth_buffer.view,
                load_op: vk::AttachmentLoadOp::CLEAR,
                clear_value: Some(ClearValue::Depth(1.0)),
            }),
            base.swapchain.extent,
        );

        buffer.bind_graphics_pipeline(&self.opaque_pass.pipeline);
        buffer.bind_vertex_buffer(&self.vertex_buffer);
        buffer.set_viewport(base.swapchain.extent);
        buffer.set_scissor(base.swapchain.extent);

        for (i, _) in self
            .instances
            .iter()
            .enumerate()
            .filter(|(_, i)| i.color[3] == 1.0)
        {
            let offset = compute_aligned_size_of::<InstanceUbo>(self.ubo_alignment) as u32;
            buffer.bind_descriptor_sets_with_dynamic_offsets(
                PipelineBindPoint::GRAPHICS,
                &self.opaque_pass.pipeline_layout,
                0,
                &[&self.opaque_pass.descriptor_set],
                &[i as u32 * offset],
            );
            buffer.draw(6);
        }

        buffer.end_rendering();

        // transparent pass
        buffer.begin_rendering(
            &[
                RenderingAttachment {
                    view: &self.weighted_colors_fb.view,
                    load_op: vk::AttachmentLoadOp::CLEAR,
                    clear_value: Some(ClearValue::ColorFloat([0.0; 4])),
                },
                RenderingAttachment {
                    view: &self.reveal_fb.view,
                    load_op: vk::AttachmentLoadOp::CLEAR,
                    clear_value: Some(ClearValue::ColorFloat([1.0; 4])),
                },
            ],
            Some(RenderingAttachment {
                view: &self.depth_buffer.view,
                load_op: vk::AttachmentLoadOp::LOAD,
                clear_value: None,
            }),
            base.swapchain.extent,
        );

        buffer.bind_graphics_pipeline(&self.transparent_pass.pipeline);
        buffer.bind_vertex_buffer(&self.vertex_buffer);
        buffer.set_viewport(base.swapchain.extent);
        buffer.set_scissor(base.swapchain.extent);

        for (i, _) in self
            .instances
            .iter()
            .enumerate()
            .filter(|(_, i)| i.color[3] < 1.0)
        {
            let offset = compute_aligned_size_of::<InstanceUbo>(self.ubo_alignment) as u32;
            buffer.bind_descriptor_sets_with_dynamic_offsets(
                PipelineBindPoint::GRAPHICS,
                &self.transparent_pass.pipeline_layout,
                0,
                &[&self.transparent_pass.descriptor_set],
                &[i as u32 * offset],
            );
            buffer.draw(6);
        }

        buffer.end_rendering();

        // composite pass
        buffer.pipeline_image_barriers(&[
            ImageBarrier {
                image: &self.weighted_colors_fb.image,
                old_layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                new_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                src_access_mask: vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
                dst_access_mask: vk::AccessFlags2::SHADER_READ,
                src_stage_mask: vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
                dst_stage_mask: vk::PipelineStageFlags2::FRAGMENT_SHADER,
            },
            ImageBarrier {
                image: &self.reveal_fb.image,
                old_layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                new_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                src_access_mask: vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
                dst_access_mask: vk::AccessFlags2::SHADER_READ,
                src_stage_mask: vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
                dst_stage_mask: vk::PipelineStageFlags2::FRAGMENT_SHADER,
            },
        ]);

        buffer.begin_rendering(
            &[RenderingAttachment {
                view: &base.swapchain.views[image_index],
                load_op: vk::AttachmentLoadOp::LOAD,
                clear_value: None,
            }],
            None,
            base.swapchain.extent,
        );

        buffer.bind_graphics_pipeline(&self.composite_pass.pipeline);
        buffer.bind_vertex_buffer(&self.quad_vertex_buffer);
        buffer.set_viewport(base.swapchain.extent);
        buffer.set_scissor(base.swapchain.extent);

        buffer.bind_descriptor_sets(
            PipelineBindPoint::GRAPHICS,
            &self.composite_pass.pipeline_layout,
            0,
            &[&self.composite_pass.descriptor_set],
        );
        buffer.draw(6);

        buffer.end_rendering();

        Ok(())
    }
}

struct Gui {
    instances: Vec<InstanceUbo>,
    new_instance: InstanceUbo,
}

impl app::Gui for Gui {
    fn new(_: &BaseApp) -> Result<Self> {
        Ok(Self {
            instances: vec![
                InstanceUbo::new([1.0, 1.0, 1.0, 0.5], [0.0, 0.0, 0.0]),
                InstanceUbo::new([1.0, 0.0, 0.0, 0.5], [0.3, 0.0, -0.2]),
                InstanceUbo::new([0.0, 0.0, 1.0, 0.5], [-0.3, 0.0, 0.2]),
            ],
            new_instance: InstanceUbo::new([1.0, 1.0, 1.0, 1.0], [0.0, 0.0, 0.0]),
        })
    }

    fn build(&mut self, ctx: &egui::Context) {
        let mut instance_index_to_remove = None;

        egui::SidePanel::left("cfg").show(ctx, |ui| {
            for (i, instance) in self.instances.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    ui.color_edit_button_rgba_unmultiplied(&mut instance.color);
                    egui::DragValue::new(&mut instance.position[0])
                        .speed(0.1)
                        .prefix("x: ")
                        .ui(ui);
                    egui::DragValue::new(&mut instance.position[1])
                        .speed(0.1)
                        .prefix("y: ")
                        .ui(ui);
                    egui::DragValue::new(&mut instance.position[2])
                        .speed(0.1)
                        .prefix("z: ")
                        .ui(ui);

                    if ui.button("❌").clicked() {
                        instance_index_to_remove.replace(i);
                    }
                });
            }

            ui.separator();
            ui.label("Add instance");
            ui.horizontal(|ui| {
                ui.color_edit_button_rgba_unmultiplied(&mut self.new_instance.color);
                egui::DragValue::new(&mut self.new_instance.position[0])
                    .prefix("x: ")
                    .ui(ui);
                egui::DragValue::new(&mut self.new_instance.position[1])
                    .prefix("y: ")
                    .ui(ui);
                egui::DragValue::new(&mut self.new_instance.position[2])
                    .prefix("z: ")
                    .ui(ui);
                ui.add_enabled_ui(self.instances.len() < MAX_INSTANCES, |ui| {
                    if ui.button("➕").clicked() {
                        self.instances.push(self.new_instance);
                    }
                });
            });
        });

        if let Some(i) = instance_index_to_remove.take() {
            self.instances.remove(i);
        }
    }
}

#[derive(Debug, Clone, Copy)]
#[repr(C)]
#[allow(dead_code)]
struct QuadVertex {
    position: [f32; 2],
    uv: [f32; 2],
}

impl app::vulkan::Vertex for QuadVertex {
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
    let vertices: [QuadVertex; 6] = [
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
            position: [-1.0, -1.0],
            uv: [0.0, 0.0],
        },
        QuadVertex {
            position: [1.0, 1.0],
            uv: [1.0, 1.0],
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

struct Texture {
    image: Image,
    view: ImageView,
    sampler: Option<Sampler>,
}

impl Texture {
    fn create_framebuffer(
        context: &Context,
        usage: vk::ImageUsageFlags,
        extent: vk::Extent2D,
        format: vk::Format,
        aspect_mask: vk::ImageAspectFlags,
        sampled: bool,
    ) -> Result<Self> {
        let usage = if sampled {
            usage | vk::ImageUsageFlags::SAMPLED
        } else {
            usage
        };
        let image = context.create_image(
            usage,
            MemoryLocation::GpuOnly,
            format,
            extent.width,
            extent.height,
        )?;

        let view = image.create_image_view(aspect_mask)?;

        let sampler = sampled
            .then(|| context.create_sampler(&Default::default()))
            .transpose()?;

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

#[derive(Clone, Copy)]
#[allow(dead_code)]
#[repr(C)]
struct FrameUbo {
    view_proj_matrix: Mat4,
}

#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
#[repr(C)]
struct InstanceUbo {
    color: [f32; 4],
    position: [f32; 3],
}

impl InstanceUbo {
    const fn new(color: [f32; 4], position: [f32; 3]) -> Self {
        Self { color, position }
    }
}

#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
#[repr(C)]
struct Vertex {
    position: [f32; 3],
}

impl app::vulkan::Vertex for Vertex {
    fn bindings() -> Vec<vk::VertexInputBindingDescription> {
        vec![vk::VertexInputBindingDescription {
            binding: 0,
            stride: size_of::<Vertex>() as _,
            input_rate: vk::VertexInputRate::VERTEX,
        }]
    }

    fn attributes() -> Vec<vk::VertexInputAttributeDescription> {
        vec![vk::VertexInputAttributeDescription {
            binding: 0,
            location: 0,
            format: vk::Format::R32G32B32_SFLOAT,
            offset: offset_of!(Vertex, position) as _,
        }]
    }
}

fn create_vertex_buffer(context: &Context) -> Result<Buffer> {
    let vertices: [Vertex; 6] = [
        Vertex {
            position: [-1.0, -1.0, 0.0],
        },
        Vertex {
            position: [1.0, -1.0, 0.0],
        },
        Vertex {
            position: [-1.0, 1.0, 0.0],
        },
        Vertex {
            position: [-1.0, 1.0, 0.0],
        },
        Vertex {
            position: [1.0, -1.0, 0.0],
        },
        Vertex {
            position: [1.0, 1.0, 0.0],
        },
    ];

    let vertex_buffer =
        create_gpu_only_buffer_from_data(context, vk::BufferUsageFlags::VERTEX_BUFFER, &vertices)?;

    Ok(vertex_buffer)
}

fn create_opaque_pass(
    context: &Context,
    frame_ubo: &Buffer,
    instance_ubo: &Buffer,
    color_attachment_format: vk::Format,
) -> Result<Pass> {
    let bindings = [
        vk::DescriptorSetLayoutBinding::default()
            .binding(0)
            .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::VERTEX),
        vk::DescriptorSetLayoutBinding::default()
            .binding(1)
            .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER_DYNAMIC)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::VERTEX),
    ];
    let dsl = context.create_descriptor_set_layout(&bindings)?;

    let pool_sizes = [
        vk::DescriptorPoolSize::default()
            .ty(vk::DescriptorType::UNIFORM_BUFFER)
            .descriptor_count(1),
        vk::DescriptorPoolSize::default()
            .ty(vk::DescriptorType::UNIFORM_BUFFER_DYNAMIC)
            .descriptor_count(1),
    ];

    let descriptor_pool = context.create_descriptor_pool(1, &pool_sizes)?;
    let descriptor_set = descriptor_pool.allocate_set(&dsl)?;

    descriptor_set.update(&[
        WriteDescriptorSet {
            binding: 0,
            kind: WriteDescriptorSetKind::UniformBuffer { buffer: frame_ubo },
        },
        WriteDescriptorSet {
            binding: 1,
            kind: WriteDescriptorSetKind::UniformBufferDynamic {
                buffer: instance_ubo,
                byte_stride: size_of::<InstanceUbo>() as _,
            },
        },
    ]);

    let pipeline_layout = context.create_pipeline_layout(&[&dsl])?;

    let pipeline = context.create_graphics_pipeline::<Vertex>(
        &pipeline_layout,
        GraphicsPipelineCreateInfo {
            shaders: &[
                GraphicsShaderCreateInfo {
                    source: &include_bytes!("../shaders/geom.vert.spv")[..],
                    stage: vk::ShaderStageFlags::VERTEX,
                },
                GraphicsShaderCreateInfo {
                    source: &include_bytes!("../shaders/shader.frag.spv")[..],
                    stage: vk::ShaderStageFlags::FRAGMENT,
                },
            ],
            primitive_topology: vk::PrimitiveTopology::TRIANGLE_LIST,
            cull_mode: vk::CullModeFlags::NONE,
            extent: None,
            color_attachments: ColorAttachmentsInfo {
                formats: &[color_attachment_format],
                blends: &[vk::PipelineColorBlendAttachmentState {
                    color_write_mask: vk::ColorComponentFlags::RGBA,
                    ..Default::default()
                }],
            },
            depth: Some(DepthInfo {
                format: DEPTH_BUFFER_FORMAT,
                enable_depth_test: true,
                enable_depth_write: true,
            }),
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

fn create_transparent_pass(
    context: &Context,
    frame_ubo: &Buffer,
    instance_ubo: &Buffer,
) -> Result<Pass> {
    let bindings = [
        vk::DescriptorSetLayoutBinding::default()
            .binding(0)
            .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::VERTEX),
        vk::DescriptorSetLayoutBinding::default()
            .binding(1)
            .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER_DYNAMIC)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::VERTEX),
    ];
    let dsl = context.create_descriptor_set_layout(&bindings)?;

    let pool_sizes = [
        vk::DescriptorPoolSize::default()
            .ty(vk::DescriptorType::UNIFORM_BUFFER)
            .descriptor_count(1),
        vk::DescriptorPoolSize::default()
            .ty(vk::DescriptorType::UNIFORM_BUFFER_DYNAMIC)
            .descriptor_count(1),
    ];

    let descriptor_pool = context.create_descriptor_pool(1, &pool_sizes)?;
    let descriptor_set = descriptor_pool.allocate_set(&dsl)?;

    descriptor_set.update(&[
        WriteDescriptorSet {
            binding: 0,
            kind: WriteDescriptorSetKind::UniformBuffer { buffer: frame_ubo },
        },
        WriteDescriptorSet {
            binding: 1,
            kind: WriteDescriptorSetKind::UniformBufferDynamic {
                buffer: instance_ubo,
                byte_stride: size_of::<InstanceUbo>() as _,
            },
        },
    ]);

    let pipeline_layout = context.create_pipeline_layout(&[&dsl])?;

    let pipeline = context.create_graphics_pipeline::<Vertex>(
        &pipeline_layout,
        GraphicsPipelineCreateInfo {
            shaders: &[
                GraphicsShaderCreateInfo {
                    source: &include_bytes!("../shaders/geom.vert.spv")[..],
                    stage: vk::ShaderStageFlags::VERTEX,
                },
                GraphicsShaderCreateInfo {
                    source: &include_bytes!("../shaders/wboit.frag.spv")[..],
                    stage: vk::ShaderStageFlags::FRAGMENT,
                },
            ],
            primitive_topology: vk::PrimitiveTopology::TRIANGLE_LIST,
            cull_mode: vk::CullModeFlags::NONE,
            extent: None,
            color_attachments: ColorAttachmentsInfo {
                formats: &[WEIGHT_COLORS_FB_FORMAT, REVEAL_FB_FORMAT],
                blends: &[
                    vk::PipelineColorBlendAttachmentState {
                        blend_enable: vk::TRUE,
                        src_color_blend_factor: vk::BlendFactor::ONE,
                        dst_color_blend_factor: vk::BlendFactor::ONE,
                        color_blend_op: vk::BlendOp::ADD,
                        src_alpha_blend_factor: vk::BlendFactor::ONE,
                        dst_alpha_blend_factor: vk::BlendFactor::ONE,
                        alpha_blend_op: vk::BlendOp::ADD,
                        color_write_mask: vk::ColorComponentFlags::RGBA,
                    },
                    vk::PipelineColorBlendAttachmentState {
                        blend_enable: vk::TRUE,
                        src_color_blend_factor: vk::BlendFactor::ZERO,
                        dst_color_blend_factor: vk::BlendFactor::ONE_MINUS_SRC_COLOR,
                        color_blend_op: vk::BlendOp::ADD,
                        color_write_mask: vk::ColorComponentFlags::RGBA,
                        ..Default::default()
                    },
                ],
            },
            depth: Some(DepthInfo {
                format: DEPTH_BUFFER_FORMAT,
                enable_depth_test: true,
                enable_depth_write: false,
            }),
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

fn create_composite_pass(
    context: &Context,
    weighted_colors_fb: &Texture,
    reveal_fb: &Texture,
    color_attachment_format: vk::Format,
) -> Result<Pass> {
    let bindings = [
        vk::DescriptorSetLayoutBinding::default()
            .binding(0)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT),
        vk::DescriptorSetLayoutBinding::default()
            .binding(1)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT),
    ];
    let dsl = context.create_descriptor_set_layout(&bindings)?;

    let pool_sizes = [vk::DescriptorPoolSize::default()
        .ty(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
        .descriptor_count(2)];

    let descriptor_pool = context.create_descriptor_pool(1, &pool_sizes)?;
    let descriptor_set = descriptor_pool.allocate_set(&dsl)?;

    descriptor_set.update(&[
        WriteDescriptorSet {
            binding: 0,
            kind: WriteDescriptorSetKind::CombinedImageSampler {
                view: &weighted_colors_fb.view,
                sampler: weighted_colors_fb
                    .sampler
                    .as_ref()
                    .expect("weighted_colors_fb should have a sampler"),
                layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            },
        },
        WriteDescriptorSet {
            binding: 1,
            kind: WriteDescriptorSetKind::CombinedImageSampler {
                view: &reveal_fb.view,
                sampler: reveal_fb
                    .sampler
                    .as_ref()
                    .expect("reveal_fb should have a sampler"),
                layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            },
        },
    ]);

    let pipeline_layout = context.create_pipeline_layout(&[&dsl])?;

    let pipeline = context.create_graphics_pipeline::<QuadVertex>(
        &pipeline_layout,
        GraphicsPipelineCreateInfo {
            shaders: &[
                GraphicsShaderCreateInfo {
                    source: &include_bytes!("../shaders/fullscreen.vert.spv")[..],
                    stage: vk::ShaderStageFlags::VERTEX,
                },
                GraphicsShaderCreateInfo {
                    source: &include_bytes!("../shaders/composite.frag.spv")[..],
                    stage: vk::ShaderStageFlags::FRAGMENT,
                },
            ],
            primitive_topology: vk::PrimitiveTopology::TRIANGLE_LIST,
            cull_mode: vk::CullModeFlags::NONE,
            extent: None,
            color_attachments: ColorAttachmentsInfo {
                formats: &[color_attachment_format],
                blends: &[vk::PipelineColorBlendAttachmentState {
                    blend_enable: vk::TRUE,
                    src_color_blend_factor: vk::BlendFactor::SRC_ALPHA,
                    dst_color_blend_factor: vk::BlendFactor::ONE_MINUS_SRC_ALPHA,
                    color_blend_op: vk::BlendOp::ADD,
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
