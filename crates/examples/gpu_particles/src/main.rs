use std::mem::size_of;
use std::time::Duration;

use app::anyhow::Result;
use app::glam::Mat4;
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
    attractor_center: [f32; 3],
    timer: f32,
    particles_buffer: VkBuffer,
    compute_ubo_buffer: VkBuffer,
    _compute_descriptor_pool: VkDescriptorPool,
    _compute_descriptor_layout: VkDescriptorSetLayout,
    compute_descriptor_set: VkDescriptorSet,
    compute_pipeline_layout: VkPipelineLayout,
    compute_pipeline: VkComputePipeline,
    graphics_ubo_buffer: VkBuffer,
    _graphics_descriptor_pool: VkDescriptorPool,
    _graphics_descriptor_layout: VkDescriptorSetLayout,
    graphics_descriptor_set: VkDescriptorSet,
    graphics_pipeline_layout: VkPipelineLayout,
    graphics_pipeline: VkGraphicsPipeline,
}

impl App for Particles {
    type Gui = Gui;

    fn new(base: &mut app::BaseApp<Self>) -> Result<Self> {
        let context = &mut base.context;

        let particles_buffer = create_particle_buffer(context)?;
        let compute_ubo_buffer = context.create_buffer(
            vk::BufferUsageFlags::UNIFORM_BUFFER,
            MemoryLocation::CpuToGpu,
            size_of::<ParticleConfig>() as _,
        )?;

        let compute_descriptor_pool = context.create_descriptor_pool(
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

        let compute_descriptor_set =
            compute_descriptor_pool.allocate_set(&compute_descriptor_layout)?;

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
                    buffer: &compute_ubo_buffer,
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

        let graphics_ubo_buffer = context.create_buffer(
            vk::BufferUsageFlags::UNIFORM_BUFFER,
            MemoryLocation::CpuToGpu,
            size_of::<CameraUbo>() as _,
        )?;

        let graphics_descriptor_pool = context.create_descriptor_pool(
            1,
            &[vk::DescriptorPoolSize {
                ty: vk::DescriptorType::UNIFORM_BUFFER,
                descriptor_count: 1,
            }],
        )?;

        let graphics_descriptor_layout =
            context.create_descriptor_set_layout(&[vk::DescriptorSetLayoutBinding {
                binding: 0,
                descriptor_count: 1,
                descriptor_type: vk::DescriptorType::UNIFORM_BUFFER,
                stage_flags: vk::ShaderStageFlags::VERTEX,
                ..Default::default()
            }])?;

        let graphics_descriptor_set =
            graphics_descriptor_pool.allocate_set(&graphics_descriptor_layout)?;

        graphics_descriptor_set.update(&[VkWriteDescriptorSet {
            binding: 0,
            kind: VkWriteDescriptorSetKind::UniformBuffer {
                buffer: &graphics_ubo_buffer,
            },
        }]);

        let graphics_pipeline_layout =
            context.create_pipeline_layout(&[&graphics_descriptor_layout])?;

        let graphics_pipeline = create_graphics_pipeline(
            context,
            &graphics_pipeline_layout,
            base.swapchain.extent,
            base.swapchain.format,
        )?;

        base.camera.z_far = 100.0;

        Ok(Self {
            particle_count: 0,
            attractor_center: [0.0; 3],
            timer: 0.0,
            particles_buffer,
            compute_ubo_buffer,
            _compute_descriptor_pool: compute_descriptor_pool,
            _compute_descriptor_layout: compute_descriptor_layout,
            compute_descriptor_set,
            compute_pipeline_layout,
            compute_pipeline,
            graphics_ubo_buffer,
            _graphics_descriptor_pool: graphics_descriptor_pool,
            _graphics_descriptor_layout: graphics_descriptor_layout,
            graphics_descriptor_set,
            graphics_pipeline_layout,
            graphics_pipeline,
        })
    }

    fn update(
        &mut self,
        base: &app::BaseApp<Self>,
        gui: &mut <Self as App>::Gui,
        _: usize,
        delta_time: Duration,
    ) -> Result<()> {
        self.particle_count = gui.particle_count;

        self.timer += delta_time.as_secs_f32();
        if self.timer > 10.0 {
            let mut rng = rand::thread_rng();
            self.attractor_center = [
                rng.gen_range(-1.0..1.0),
                rng.gen_range(-1.0..1.0),
                rng.gen_range(-1.0..1.0),
            ];

            self.timer = 0.0;
        }

        self.compute_ubo_buffer
            .copy_data_to_buffer(&[ParticleConfig {
                attractor_center: self.attractor_center,
                attractor_strength: gui.attractor_strength,
                particle_count: self.particle_count,
                elapsed: delta_time.as_secs_f32(),
            }])?;

        self.graphics_ubo_buffer.copy_data_to_buffer(&[CameraUbo {
            view_proj_matrix: base.camera.projection_matrix() * base.camera.view_matrix(),
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
        buffer.dispatch(self.particle_count, 1, 1);

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
            Some([0.0, 0.0, 0.0, 1.0]),
        );
        buffer.bind_graphics_pipeline(&self.graphics_pipeline);
        buffer.bind_descriptor_sets(
            vk::PipelineBindPoint::GRAPHICS,
            &self.graphics_pipeline_layout,
            0,
            &[&self.graphics_descriptor_set],
        );
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
    attractor_strength: f32,
}

impl app::Gui for Gui {
    fn new() -> Result<Self> {
        Ok(Gui {
            particle_count: MAX_PARTICLE_COUNT / 10,
            attractor_strength: 9.81,
        })
    }

    fn build(&mut self, ui: &Ui) {
        Window::new("Particles")
            .size([300.0, 100.0], Condition::FirstUseEver)
            .build(ui, || {
                Slider::new("Particle count", 0, MAX_PARTICLE_COUNT)
                    .build(ui, &mut self.particle_count);
                ui.input_float("Attractor's strength", &mut self.attractor_strength)
                    .build();
            });
    }
}

#[derive(Clone, Copy)]
#[allow(dead_code)]
struct ParticleConfig {
    attractor_center: [f32; 3],
    attractor_strength: f32,
    particle_count: u32,
    elapsed: f32,
}

#[derive(Clone, Copy)]
#[allow(dead_code)]
struct CameraUbo {
    view_proj_matrix: Mat4,
}

#[derive(Clone, Copy)]
#[allow(dead_code)]
struct Particle {
    // position 0, 1, 2 - pad 3
    position: [f32; 4],
    // direction 0, 1, 2 - pad 3
    direction: [f32; 4],
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
        [1.0, 0.0, 0.0, 1.0],
        [0.0, 1.0, 0.0, 1.0],
        [0.0, 0.0, 1.0, 1.0],
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
            direction: [
                rng.gen_range(-1.0..1.0f32),
                rng.gen_range(-1.0..1.0f32),
                rng.gen_range(-1.0..1.0f32),
                0.0,
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
            color_attachment_format: color_attachement_format,
            color_attachment_blend: Some(vk::PipelineColorBlendAttachmentState {
                blend_enable: vk::TRUE,
                src_color_blend_factor: vk::BlendFactor::ONE,
                dst_color_blend_factor: vk::BlendFactor::ONE,
                color_blend_op: vk::BlendOp::ADD,
                src_alpha_blend_factor: vk::BlendFactor::ONE,
                dst_alpha_blend_factor: vk::BlendFactor::ZERO,
                alpha_blend_op: vk::BlendOp::ADD,
                color_write_mask: vk::ColorComponentFlags::RGBA,
            }),
        },
    )
}
