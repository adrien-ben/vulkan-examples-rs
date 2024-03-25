use std::mem::{offset_of, size_of, size_of_val};
use std::path::Path;
use std::time::Duration;

use app::anyhow::Result;
use app::glam::Mat4;
use app::vulkan::ash::vk::{self, PipelineBindPoint};
use app::vulkan::gpu_allocator::MemoryLocation;
use app::vulkan::utils::create_gpu_only_buffer_from_data;
use app::vulkan::{
    Buffer, CommandBuffer, Context, DescriptorPool, DescriptorSet, DescriptorSetLayout,
    GraphicsPipeline, GraphicsPipelineCreateInfo, GraphicsShaderCreateInfo, Image, ImageBarrier,
    ImageView, PipelineLayout, Sampler, WriteDescriptorSet, WriteDescriptorSetKind,
};
use app::{App, AppConfig, BaseApp};
use gui::egui;
use rfd::FileDialog;

const WIDTH: u32 = 1024;
const HEIGHT: u32 = 576;
const APP_NAME: &str = "Hdr skybox";

fn main() -> Result<()> {
    app::run::<Triangle>(
        APP_NAME,
        WIDTH,
        HEIGHT,
        AppConfig {
            preferred_swapchain_format: Some(vk::SurfaceFormatKHR {
                format: vk::Format::R16G16B16A16_SFLOAT,
                color_space: vk::ColorSpaceKHR::EXTENDED_SRGB_LINEAR_EXT,
            }),
            required_instance_extensions: &["VK_EXT_swapchain_colorspace"],
            ..Default::default()
        },
    )
}

struct Triangle {
    vertex_buffer: Buffer,
    index_buffer: Buffer,
    texture: Texture,
    ubo_buffer: Buffer,
    _dsl: DescriptorSetLayout,
    _descriptor_pool: DescriptorPool,
    descriptor_set: DescriptorSet,
    pipeline_layout: PipelineLayout,
    pipeline: GraphicsPipeline,
}

impl App for Triangle {
    type Gui = Gui;

    fn new(base: &mut BaseApp<Self>) -> Result<Self> {
        let context = &mut base.context;

        let vertex_buffer = create_vertex_buffer(context)?;
        let index_buffer = create_index_buffer(context)?;

        let texture = create_texture(context, "assets/images/studio_2k.hdr")?;

        let ubo_buffer = context.create_buffer(
            vk::BufferUsageFlags::UNIFORM_BUFFER,
            MemoryLocation::CpuToGpu,
            size_of::<GraphicsUbo>() as _,
        )?;

        let dsl = create_dsl(context)?;
        let (descriptor_pool, descriptor_set) =
            create_descriptor_sets(context, &dsl, &texture, &ubo_buffer)?;
        let pipeline_layout = context.create_pipeline_layout(&[&dsl])?;

        let pipeline = create_pipeline(context, &pipeline_layout, base.swapchain.format)?;

        Ok(Self {
            vertex_buffer,
            index_buffer,
            texture,
            ubo_buffer,
            _dsl: dsl,
            _descriptor_pool: descriptor_pool,
            descriptor_set,
            pipeline_layout,
            pipeline,
        })
    }

    fn on_recreate_swapchain(&mut self, _: &BaseApp<Self>) -> Result<()> {
        Ok(())
    }

    fn update(
        &mut self,
        base: &BaseApp<Self>,
        ui: &mut <Self as App>::Gui,
        _: usize,
        _: Duration,
    ) -> Result<()> {
        if ui.open_file_picker {
            if let Some(file) = FileDialog::new().pick_file() {
                log::info!("Loading new environment from file {file:?}");
                match create_texture(&base.context, file) {
                    Ok(texture) => {
                        self.descriptor_set.update(&[WriteDescriptorSet {
                            binding: 1,
                            kind: WriteDescriptorSetKind::CombinedImageSampler {
                                view: &texture.view,
                                sampler: &texture.sampler,
                                layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                            },
                        }]);

                        self.texture = texture;
                    }
                    Err(e) => {
                        log::error!("Failed to load environment: {e}");
                    }
                }
            }
        }

        self.ubo_buffer.copy_data_to_buffer(&[GraphicsUbo {
            view_proj_matrix: base.camera.projection_matrix() * base.camera.view_matrix_at_center(),
        }])?;

        Ok(())
    }

    fn record_raster_commands(
        &self,
        base: &BaseApp<Self>,
        buffer: &CommandBuffer,
        image_index: usize,
    ) -> Result<()> {
        buffer.begin_rendering(
            &base.swapchain.views[image_index],
            base.swapchain.extent,
            vk::AttachmentLoadOp::CLEAR,
            None,
        );
        buffer.bind_graphics_pipeline(&self.pipeline);
        buffer.bind_descriptor_sets(
            PipelineBindPoint::GRAPHICS,
            &self.pipeline_layout,
            0,
            &[&self.descriptor_set],
        );
        buffer.bind_vertex_buffer(&self.vertex_buffer);
        buffer.bind_index_buffer(&self.index_buffer, vk::IndexType::UINT16);
        buffer.set_viewport(base.swapchain.extent);
        buffer.set_scissor(base.swapchain.extent);
        buffer.draw_indexed(36);
        buffer.end_rendering();

        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
struct Gui {
    open_file_picker: bool,
}

impl app::Gui for Gui {
    fn new() -> Result<Self> {
        Ok(Gui {
            open_file_picker: false,
        })
    }

    fn build(&mut self, ctx: &egui::Context) {
        egui::Window::new("Settings").show(ctx, |ui| {
            self.open_file_picker = ui.button("Pick HDRi file").clicked();
        });
    }
}

#[derive(Clone, Copy)]
#[allow(dead_code)]
#[repr(C)]
struct GraphicsUbo {
    view_proj_matrix: Mat4,
}

#[derive(Debug, Clone, Copy)]
#[repr(C)]
#[allow(dead_code)]
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
    let vertices: [Vertex; 8] = [
        Vertex {
            position: [-1.0, -1.0, -1.0],
        },
        Vertex {
            position: [1.0, -1.0, -1.0],
        },
        Vertex {
            position: [-1.0, 1.0, -1.0],
        },
        Vertex {
            position: [1.0, 1.0, -1.0],
        },
        Vertex {
            position: [-1.0, -1.0, 1.0],
        },
        Vertex {
            position: [1.0, -1.0, 1.0],
        },
        Vertex {
            position: [-1.0, 1.0, 1.0],
        },
        Vertex {
            position: [1.0, 1.0, 1.0],
        },
    ];

    let vertex_buffer =
        create_gpu_only_buffer_from_data(context, vk::BufferUsageFlags::VERTEX_BUFFER, &vertices)?;

    Ok(vertex_buffer)
}

fn create_index_buffer(context: &Context) -> Result<Buffer> {
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

struct Texture {
    _image: Image,
    view: ImageView,
    sampler: Sampler,
}

fn create_texture<P>(context: &Context, path: P) -> Result<Texture>
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

    let view = image.create_image_view()?;
    let sampler = context.create_sampler(&Default::default())?;

    Ok(Texture {
        _image: image,
        view,
        sampler,
    })
}

fn create_dsl(context: &Context) -> Result<DescriptorSetLayout> {
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
    Ok(dsl)
}

fn create_descriptor_sets(
    context: &Context,
    dsl: &DescriptorSetLayout,
    texture: &Texture,
    ubo_buffer: &Buffer,
) -> Result<(DescriptorPool, DescriptorSet)> {
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

    let pool = context.create_descriptor_pool(2, &pool_sizes)?;
    let set = pool.allocate_set(dsl)?;

    set.update(&[
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

    Ok((pool, set))
}

fn create_pipeline(
    context: &Context,
    layout: &PipelineLayout,
    color_attachment_format: vk::Format,
) -> Result<GraphicsPipeline> {
    context.create_graphics_pipeline::<Vertex>(
        layout,
        GraphicsPipelineCreateInfo {
            shaders: &[
                GraphicsShaderCreateInfo {
                    source: &include_bytes!("../shaders/shader.vert.spv")[..],
                    stage: vk::ShaderStageFlags::VERTEX,
                },
                GraphicsShaderCreateInfo {
                    source: &include_bytes!("../shaders/shader.frag.spv")[..],
                    stage: vk::ShaderStageFlags::FRAGMENT,
                },
            ],
            primitive_topology: vk::PrimitiveTopology::TRIANGLE_LIST,
            extent: None,
            color_attachment_format,
            color_attachment_blend: None,
            dynamic_states: Some(&[vk::DynamicState::SCISSOR, vk::DynamicState::VIEWPORT]),
        },
    )
}
