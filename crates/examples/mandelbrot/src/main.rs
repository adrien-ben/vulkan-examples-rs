use std::mem::{offset_of, size_of};
use std::time::Duration;

use app::anyhow::Result;
use app::vulkan::ash::vk;
use app::vulkan::utils::create_gpu_only_buffer_from_data;
use app::vulkan::{
    Buffer, Context, GraphicsPipeline, GraphicsPipelineCreateInfo, GraphicsShaderCreateInfo,
    PipelineLayout,
};
use app::{App, BaseApp};

const WIDTH: u32 = 1024;
const HEIGHT: u32 = 576;
const APP_NAME: &str = "Mandelbrot";

fn main() -> Result<()> {
    app::run::<Mandelbrot>(APP_NAME, WIDTH, HEIGHT, Default::default())
}
struct Mandelbrot {
    vertex_buffer: Buffer,
    _pipeline_layout: PipelineLayout,
    pipeline: GraphicsPipeline,
}

impl App for Mandelbrot {
    type Gui = ();

    fn new(base: &mut BaseApp<Self>) -> Result<Self> {
        let context = &mut base.context;

        let vertex_buffer = create_vertex_buffer(context)?;

        let pipeline_layout = context.create_pipeline_layout(&[])?;

        let pipeline = create_pipeline(context, &pipeline_layout, base.swapchain.format)?;

        Ok(Self {
            vertex_buffer,
            _pipeline_layout: pipeline_layout,
            pipeline,
        })
    }

    fn on_recreate_swapchain(&mut self, _: &BaseApp<Self>) -> Result<()> {
        Ok(())
    }

    fn update(
        &mut self,
        _: &mut BaseApp<Self>,
        _: &mut <Self as App>::Gui,
        _: usize,
        _: Duration,
    ) -> Result<()> {
        Ok(())
    }

    fn record_raster_commands(&self, base: &BaseApp<Self>, image_index: usize) -> Result<()> {
        let buffer = &base.command_buffers[image_index];

        buffer.begin_rendering(
            &base.swapchain.views[image_index],
            None,
            base.swapchain.extent,
            vk::AttachmentLoadOp::CLEAR,
            None,
        );
        buffer.bind_graphics_pipeline(&self.pipeline);
        buffer.bind_vertex_buffer(&self.vertex_buffer);
        buffer.set_viewport(base.swapchain.extent);
        buffer.set_scissor(base.swapchain.extent);
        buffer.draw(6);
        buffer.end_rendering();

        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
#[repr(C)]
struct Vertex {
    position: [f32; 2],
    uv: [f32; 2],
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
        vec![
            vk::VertexInputAttributeDescription {
                binding: 0,
                location: 0,
                format: vk::Format::R32G32_SFLOAT,
                offset: offset_of!(Vertex, position) as _,
            },
            vk::VertexInputAttributeDescription {
                binding: 0,
                location: 1,
                format: vk::Format::R32G32_SFLOAT,
                offset: offset_of!(Vertex, uv) as _,
            },
        ]
    }
}

fn create_vertex_buffer(context: &Context) -> Result<Buffer> {
    let vertices: [Vertex; 6] = [
        Vertex {
            position: [-1.0, 1.0],
            uv: [-1.0, -1.0],
        },
        Vertex {
            position: [1.0, 1.0],
            uv: [1.0, -1.0],
        },
        Vertex {
            position: [-1.0, -1.0],
            uv: [-1.0, 1.0],
        },
        Vertex {
            position: [-1.0, -1.0],
            uv: [-1.0, 1.0],
        },
        Vertex {
            position: [1.0, 1.0],
            uv: [1.0, -1.0],
        },
        Vertex {
            position: [1.0, -1.0],
            uv: [1.0, 1.0],
        },
    ];

    let vertex_buffer =
        create_gpu_only_buffer_from_data(context, vk::BufferUsageFlags::VERTEX_BUFFER, &vertices)?;

    Ok(vertex_buffer)
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
            cull_mode: vk::CullModeFlags::BACK,
            extent: None,
            color_attachment_format,
            color_attachment_blend: None,
            depth_attachment_format: None,
            dynamic_states: Some(&[vk::DynamicState::SCISSOR, vk::DynamicState::VIEWPORT]),
        },
    )
}
