use std::mem::size_of;
use std::time::Duration;

use app::anyhow::Result;
use app::vulkan::ash::vk;
use app::vulkan::gpu_allocator::MemoryLocation;
use app::vulkan::utils::create_gpu_only_buffer_from_data;
use app::vulkan::{
    Vertex, VkBuffer, VkBufferBarrier, VkComputePipeline, VkComputePipelineCreateInfo, VkContext,
    VkDescriptorPool, VkDescriptorSet, VkDescriptorSetLayout, VkGraphicsPipeline,
    VkGraphicsPipelineCreateInfo, VkGraphicsShaderCreateInfo, VkPipelineLayout,
    VkWriteDescriptorSet, VkWriteDescriptorSetKind,
};
use app::App;
use gui::imgui::{Condition, Slider, Ui, Window};
use rand::Rng;

const WIDTH: u32 = 1024;
const HEIGHT: u32 = 576;
const APP_NAME: &str = "GPU Particles";

const MAX_PARTICLE_COUNT: u32 = 256 * 4048;

fn main() -> Result<()> {
    app::run::<Particles>(APP_NAME, WIDTH, HEIGHT, false)
}
struct Particles {
    particle_count: u32,
    particles_buffer: VkBuffer,
    ubo_buffer: VkBuffer,
    _descriptor_pool: VkDescriptorPool,
    _compute_descriptor_layout: VkDescriptorSetLayout,
    compute_descriptor_set: VkDescriptorSet,
    compute_pipeline_layout: VkPipelineLayout,
    compute_pipeline: VkComputePipeline,
    graphics_pipeline_layout: VkPipelineLayout,
    graphics_pipeline: VkGraphicsPipeline,
}

impl App for Particles {
    type Gui = Gui;

    fn new(base: &mut app::BaseApp<Self>) -> Result<Self> {
        let context = &mut base.context;

        let particles_buffer = create_particle_buffer(context)?;
        let ubo_buffer = context.create_buffer(
            vk::BufferUsageFlags::UNIFORM_BUFFER,
            MemoryLocation::CpuToGpu,
            size_of::<ParticleConfig>() as _,
        )?;

        let descriptor_pool = context.create_descriptor_pool(
            1,
            &[
                vk::DescriptorPoolSize {
                    ty: vk::DescriptorType::STORAGE_BUFFER,
                    descriptor_count: 1,
                },
                vk::DescriptorPoolSize {
                    ty: vk::DescriptorType::UNIFORM_BUFFER,
                    descriptor_count: 1,
                },
            ],
        )?;

        let compute_descriptor_layout = context.create_descriptor_set_layout(&[
            vk::DescriptorSetLayoutBinding {
                binding: 0,
                descriptor_count: 1,
                descriptor_type: vk::DescriptorType::STORAGE_BUFFER,
                stage_flags: vk::ShaderStageFlags::COMPUTE,
                ..Default::default()
            },
            vk::DescriptorSetLayoutBinding {
                binding: 1,
                descriptor_count: 1,
                descriptor_type: vk::DescriptorType::UNIFORM_BUFFER,
                stage_flags: vk::ShaderStageFlags::COMPUTE,
                ..Default::default()
            },
        ])?;

        let compute_descriptor_set = descriptor_pool.allocate_set(&compute_descriptor_layout)?;

        compute_descriptor_set.update(&[
            VkWriteDescriptorSet {
                binding: 0,
                kind: VkWriteDescriptorSetKind::StorageBuffer {
                    buffer: &particles_buffer,
                },
            },
            VkWriteDescriptorSet {
                binding: 1,
                kind: VkWriteDescriptorSetKind::UniformBuffer {
                    buffer: &ubo_buffer,
                },
            },
        ]);

        let compute_pipeline_layout =
            context.create_pipeline_layout(&[&compute_descriptor_layout])?;

        let compute_pipeline = context.create_compute_pipeline(
            &compute_pipeline_layout,
            VkComputePipelineCreateInfo {
                shader_source: &include_bytes!("../shaders/shader.comp.spv")[..],
            },
        )?;

        let graphics_pipeline_layout = context.create_pipeline_layout(&[])?;

        let graphics_pipeline = create_graphics_pipeline(
            context,
            &graphics_pipeline_layout,
            base.swapchain.extent,
            base.swapchain.format,
        )?;

        Ok(Self {
            particle_count: 0,
            particles_buffer,
            ubo_buffer,
            _descriptor_pool: descriptor_pool,
            _compute_descriptor_layout: compute_descriptor_layout,
            compute_descriptor_set,
            compute_pipeline_layout,
            compute_pipeline,
            graphics_pipeline_layout,
            graphics_pipeline,
        })
    }

    fn update(
        &mut self,
        _: &app::BaseApp<Self>,
        gui: &mut <Self as App>::Gui,
        _: usize,
        delta_time: Duration,
    ) -> Result<()> {
        self.particle_count = gui.particle_count;

        self.ubo_buffer.copy_data_to_buffer(&[ParticleConfig {
            particle_count: self.particle_count,
            elapsed: delta_time.as_secs_f32(),
        }])?;

        Ok(())
    }

    fn record_raster_commands(
        &self,
        base: &app::BaseApp<Self>,
        buffer: &app::vulkan::VkCommandBuffer,
        image_index: usize,
    ) -> Result<()> {
        buffer.bind_compute_pipeline(&self.compute_pipeline);
        buffer.bind_descriptor_sets(
            vk::PipelineBindPoint::COMPUTE,
            &self.compute_pipeline_layout,
            0,
            &[&self.compute_descriptor_set],
        );
        buffer.dispatch(MAX_PARTICLE_COUNT / 256, 1, 1);

        buffer.pipeline_buffer_barriers(&[VkBufferBarrier {
            buffer: &self.particles_buffer,
            src_access_mask: vk::AccessFlags2::SHADER_WRITE,
            src_stage_mask: vk::PipelineStageFlags2::COMPUTE_SHADER,
            dst_access_mask: vk::AccessFlags2::VERTEX_ATTRIBUTE_READ,
            dst_stage_mask: vk::PipelineStageFlags2::VERTEX_ATTRIBUTE_INPUT,
        }]);

        buffer.begin_rendering(
            &base.swapchain.views[image_index],
            base.swapchain.extent,
            vk::AttachmentLoadOp::CLEAR,
        );
        buffer.bind_graphics_pipeline(&self.graphics_pipeline);
        buffer.bind_vertex_buffer(&self.particles_buffer);
        buffer.draw(self.particle_count);
        buffer.end_rendering();

        Ok(())
    }

    fn on_recreate_swapchain(&mut self, base: &app::BaseApp<Self>) -> Result<()> {
        self.graphics_pipeline = create_graphics_pipeline(
            &base.context,
            &self.graphics_pipeline_layout,
            base.swapchain.extent,
            base.swapchain.format,
        )?;

        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
struct Gui {
    particle_count: u32,
}

impl app::Gui for Gui {
    fn new() -> Result<Self> {
        Ok(Gui {
            particle_count: MAX_PARTICLE_COUNT / 10,
        })
    }

    fn build(&mut self, ui: &Ui) {
        Window::new("Particles")
            .size([300.0, 100.0], Condition::FirstUseEver)
            .build(ui, || {
                Slider::new("Particle count", 0, MAX_PARTICLE_COUNT)
                    .build(ui, &mut self.particle_count);
            });
    }
}

#[derive(Clone, Copy)]
#[allow(dead_code)]
struct ParticleConfig {
    particle_count: u32,
    elapsed: f32,
}

#[derive(Clone, Copy)]
#[allow(dead_code)]
struct Particle {
    position: [f32; 4],
    velocity: [f32; 4],
    color: [f32; 4],
}

impl Vertex for Particle {
    fn bindings() -> Vec<vk::VertexInputBindingDescription> {
        vec![vk::VertexInputBindingDescription {
            binding: 0,
            stride: 48,
            input_rate: vk::VertexInputRate::VERTEX,
        }]
    }

    fn attributes() -> Vec<vk::VertexInputAttributeDescription> {
        vec![
            vk::VertexInputAttributeDescription {
                binding: 0,
                location: 0,
                format: vk::Format::R32G32B32_SFLOAT,
                offset: 0,
            },
            vk::VertexInputAttributeDescription {
                binding: 0,
                location: 1,
                format: vk::Format::R32G32B32A32_SFLOAT,
                offset: 32,
            },
        ]
    }
}

fn create_particle_buffer(context: &VkContext) -> Result<VkBuffer> {
    let colors = [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [1.0, 0.0, 1.0, 0.0],
    ];

    let mut rng = rand::thread_rng();
    let mut particles = Vec::with_capacity(MAX_PARTICLE_COUNT as usize);
    for _ in 0..MAX_PARTICLE_COUNT {
        particles.push(Particle {
            position: [
                rng.gen_range(-1.0..1.0f32),
                rng.gen_range(-1.0..1.0f32),
                rng.gen_range(-1.0..1.0f32),
                0.0,
            ],
            velocity: [
                rng.gen_range(-1.0..1.0f32),
                rng.gen_range(-1.0..1.0f32),
                rng.gen_range(-1.0..1.0f32),
                rng.gen_range(0.5..1.0f32),
            ],
            color: colors[rng.gen_range(0..colors.len())],
        });
    }

    let vertex_buffer = create_gpu_only_buffer_from_data(
        context,
        vk::BufferUsageFlags::VERTEX_BUFFER | vk::BufferUsageFlags::STORAGE_BUFFER,
        &particles,
    )?;

    Ok(vertex_buffer)
}

fn create_graphics_pipeline(
    context: &VkContext,
    layout: &VkPipelineLayout,
    extent: vk::Extent2D,
    color_attachement_format: vk::Format,
) -> Result<VkGraphicsPipeline> {
    context.create_graphics_pipeline::<Particle>(
        layout,
        VkGraphicsPipelineCreateInfo {
            shaders: &[
                VkGraphicsShaderCreateInfo {
                    source: &include_bytes!("../shaders/shader.vert.spv")[..],
                    stage: vk::ShaderStageFlags::VERTEX,
                },
                VkGraphicsShaderCreateInfo {
                    source: &include_bytes!("../shaders/shader.frag.spv")[..],
                    stage: vk::ShaderStageFlags::FRAGMENT,
                },
            ],
            primitive_topology: vk::PrimitiveTopology::POINT_LIST,
            extent,
            color_attachement_format,
        },
    )
}
