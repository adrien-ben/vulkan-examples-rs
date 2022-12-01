use std::mem::size_of;
use std::time::{Duration, Instant};

use app::anyhow::Result;
use app::glam::{vec3, Mat4};
use app::vulkan::ash::vk;
use app::vulkan::gpu_allocator::MemoryLocation;
use app::vulkan::utils::create_gpu_only_buffer_from_data;
use app::vulkan::{
    Buffer, BufferBarrier, CommandBuffer, ComputePipeline, ComputePipelineCreateInfo, Context,
    DescriptorPool, DescriptorSet, DescriptorSetLayout, GraphicsPipeline,
    GraphicsPipelineCreateInfo, GraphicsShaderCreateInfo, PipelineLayout, Vertex,
    WriteDescriptorSet, WriteDescriptorSetKind,
};
use app::{log, App, BaseApp};
use gui::imgui::{Condition, Ui};
use rand::Rng;

const WIDTH: u32 = 1024;
const HEIGHT: u32 = 576;
const APP_NAME: &str = "GPU Particles";

const DISPATCH_GROUP_SIZE_X: u32 = 256;
const MAX_PARTICLE_COUNT: u32 = DISPATCH_GROUP_SIZE_X * 32_768; // 8M particles
const MIN_PARTICLE_SIZE: f32 = 1.0;
const MAX_PARTICLE_SIZE: f32 = 3.0;
const MIN_ATTRACTOR_STRENGTH: u32 = 0;
const MAX_ATTRACTOR_STRENGTH: u32 = 100;

fn main() -> Result<()> {
    app::run::<Particles>(APP_NAME, WIDTH, HEIGHT, false)
}
struct Particles {
    particle_count: u32,
    attractor_center: [f32; 3],
    particles_buffer: Buffer,
    compute_ubo_buffer: Buffer,
    _compute_descriptor_pool: DescriptorPool,
    _compute_descriptor_layout: DescriptorSetLayout,
    compute_descriptor_set: DescriptorSet,
    compute_pipeline_layout: PipelineLayout,
    compute_pipeline: ComputePipeline,
    graphics_ubo_buffer: Buffer,
    _graphics_descriptor_pool: DescriptorPool,
    _graphics_descriptor_layout: DescriptorSetLayout,
    graphics_descriptor_set: DescriptorSet,
    graphics_pipeline_layout: PipelineLayout,
    graphics_pipeline: GraphicsPipeline,
}

impl App for Particles {
    type Gui = Gui;

    fn new(base: &mut BaseApp<Self>) -> Result<Self> {
        let context = &mut base.context;

        let particles_buffer = create_particle_buffer(context)?;
        let compute_ubo_buffer = context.create_buffer(
            vk::BufferUsageFlags::UNIFORM_BUFFER,
            MemoryLocation::CpuToGpu,
            size_of::<ComputeUbo>() as _,
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
            WriteDescriptorSet {
                binding: 0,
                kind: WriteDescriptorSetKind::StorageBuffer {
                    buffer: &particles_buffer,
                },
            },
            WriteDescriptorSet {
                binding: 1,
                kind: WriteDescriptorSetKind::UniformBuffer {
                    buffer: &compute_ubo_buffer,
                },
            },
        ]);

        let compute_pipeline_layout =
            context.create_pipeline_layout(&[&compute_descriptor_layout])?;

        let compute_pipeline = context.create_compute_pipeline(
            &compute_pipeline_layout,
            ComputePipelineCreateInfo {
                shader_source: &include_bytes!("../shaders/shader.comp.spv")[..],
            },
        )?;

        let graphics_ubo_buffer = context.create_buffer(
            vk::BufferUsageFlags::UNIFORM_BUFFER,
            MemoryLocation::CpuToGpu,
            size_of::<GraphicsUbo>() as _,
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

        graphics_descriptor_set.update(&[WriteDescriptorSet {
            binding: 0,
            kind: WriteDescriptorSetKind::UniformBuffer {
                buffer: &graphics_ubo_buffer,
            },
        }]);

        let graphics_pipeline_layout =
            context.create_pipeline_layout(&[&graphics_descriptor_layout])?;

        let graphics_pipeline =
            create_graphics_pipeline(context, &graphics_pipeline_layout, base.swapchain.format)?;

        base.camera.position.z = 2.0;
        base.camera.z_far = 100.0;

        Ok(Self {
            particle_count: 0,
            attractor_center: [0.0; 3],
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
        base: &BaseApp<Self>,
        gui: &mut <Self as App>::Gui,
        _: usize,
        delta_time: Duration,
    ) -> Result<()> {
        self.particle_count = gui.particle_count;
        self.attractor_center = gui
            .new_attractor_position
            .take()
            .unwrap_or(self.attractor_center);

        self.compute_ubo_buffer.copy_data_to_buffer(&[ComputeUbo {
            attractor_center: [
                self.attractor_center[0],
                self.attractor_center[1],
                self.attractor_center[2],
                0.0,
            ],
            color1: gui.color1,
            color2: gui.color2,
            color3: gui.color3,
            attractor_strength: gui.attractor_strength,
            particle_count: self.particle_count,
            elapsed: delta_time.as_secs_f32(),
        }])?;

        self.graphics_ubo_buffer
            .copy_data_to_buffer(&[GraphicsUbo {
                view_proj_matrix: base.camera.projection_matrix() * base.camera.view_matrix(),
                particle_size: gui.particle_size,
            }])?;

        Ok(())
    }

    fn record_raster_commands(
        &self,
        base: &BaseApp<Self>,
        buffer: &CommandBuffer,
        image_index: usize,
    ) -> Result<()> {
        buffer.bind_compute_pipeline(&self.compute_pipeline);
        buffer.bind_descriptor_sets(
            vk::PipelineBindPoint::COMPUTE,
            &self.compute_pipeline_layout,
            0,
            &[&self.compute_descriptor_set],
        );
        buffer.dispatch(self.particle_count / DISPATCH_GROUP_SIZE_X, 1, 1);

        buffer.pipeline_buffer_barriers(&[BufferBarrier {
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
        buffer.set_viewport(base.swapchain.extent);
        buffer.set_scissor(base.swapchain.extent);
        buffer.draw(self.particle_count / DISPATCH_GROUP_SIZE_X * DISPATCH_GROUP_SIZE_X);
        buffer.end_rendering();

        Ok(())
    }

    fn on_recreate_swapchain(&mut self, _: &BaseApp<Self>) -> Result<()> {
        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
struct Gui {
    particle_count: u32,
    particle_size: f32,
    attractor_position: [f32; 3],
    new_attractor_position: Option<[f32; 3]>,
    attractor_strength: u32,
    color1: [f32; 4],
    color2: [f32; 4],
    color3: [f32; 4],
}

impl app::Gui for Gui {
    fn new() -> Result<Self> {
        Ok(Gui {
            particle_count: MAX_PARTICLE_COUNT / 20,
            particle_size: MIN_PARTICLE_SIZE,
            attractor_position: [0.0; 3],
            new_attractor_position: None,
            attractor_strength: MAX_ATTRACTOR_STRENGTH / 10,
            color1: [1.0, 0.0, 0.0, 1.0],
            color2: [0.0, 1.0, 0.0, 1.0],
            color3: [0.0, 0.0, 1.0, 1.0],
        })
    }

    fn build(&mut self, ui: &Ui) {
        ui.window("Particles")
            .position([5.0, 5.0], Condition::FirstUseEver)
            .size([300.0, 250.0], Condition::FirstUseEver)
            .resizable(false)
            .movable(false)
            .build(|| {
                ui.text("Particles");
                ui.slider("Count", 0, MAX_PARTICLE_COUNT, &mut self.particle_count);
                ui.slider_config("Size", MIN_PARTICLE_SIZE, MAX_PARTICLE_SIZE)
                    .display_format("%.1f")
                    .build(&mut self.particle_size);
                ui.color_edit4_config("Color 1", &mut self.color1)
                    .alpha(false)
                    .tooltip(false)
                    .build();
                ui.color_edit4_config("Color 2", &mut self.color2)
                    .alpha(false)
                    .tooltip(false)
                    .build();
                ui.color_edit4_config("Color 3", &mut self.color3)
                    .alpha(false)
                    .tooltip(false)
                    .build();
                ui.text("Attractor");
                ui.slider(
                    "Strength",
                    MIN_ATTRACTOR_STRENGTH,
                    MAX_ATTRACTOR_STRENGTH,
                    &mut self.attractor_strength,
                );
                ui.input_float3("Position", &mut self.attractor_position)
                    .build();
                if ui.button("Apply") {
                    self.new_attractor_position = Some(self.attractor_position);
                }
                ui.same_line();
                if ui.button("Randomize") {
                    let mut rng = rand::thread_rng();
                    let new_position = [
                        rng.gen_range(-1.0..1.0),
                        rng.gen_range(-1.0..1.0),
                        rng.gen_range(-1.0..1.0),
                    ];
                    self.attractor_position = new_position;
                    self.new_attractor_position = Some(new_position);
                }
                ui.same_line();
                if ui.button("Reset") {
                    let new_position = [0.0; 3];
                    self.attractor_position = new_position;
                    self.new_attractor_position = Some(new_position);
                }
            });
    }
}

#[derive(Clone, Copy)]
#[allow(dead_code)]
struct ComputeUbo {
    attractor_center: [f32; 4],
    color1: [f32; 4],
    color2: [f32; 4],
    color3: [f32; 4],
    attractor_strength: u32,
    particle_count: u32,
    elapsed: f32,
}

#[derive(Clone, Copy)]
#[allow(dead_code)]
struct GraphicsUbo {
    view_proj_matrix: Mat4,
    particle_size: f32,
}

#[derive(Clone, Copy)]
#[allow(dead_code)]
struct Particle {
    // position 0, 1, 2 - pad 3
    position: [f32; 4],
    // velocity 0, 1, 2 - pad 3
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

fn create_particle_buffer(context: &Context) -> Result<Buffer> {
    let start = Instant::now();

    let num_cpus = num_cpus::get();
    let particles_per_cpu = (MAX_PARTICLE_COUNT as f32 / num_cpus as f32).ceil() as usize;
    let remaining = MAX_PARTICLE_COUNT as usize % particles_per_cpu;

    let mut handles = vec![];
    for i in 0..num_cpus {
        handles.push(std::thread::spawn(move || {
            let mut rng = rand::thread_rng();

            let particle_count = if i == num_cpus - 1 && remaining != 0 {
                remaining
            } else {
                particles_per_cpu
            };

            let mut particles = Vec::with_capacity(particle_count);

            for _ in 0..particle_count {
                let p = vec3(
                    rng.gen_range(-1.0..1.0f32),
                    rng.gen_range(-1.0..1.0f32),
                    rng.gen_range(-1.0..1.0f32),
                )
                .normalize()
                    * rng.gen_range(0.1..1.0f32);

                particles.push(Particle {
                    position: [p.x, p.y, p.z, 0.0],
                    velocity: [
                        rng.gen_range(-1.0..1.0f32),
                        rng.gen_range(-1.0..1.0f32),
                        rng.gen_range(-1.0..1.0f32),
                        0.0,
                    ],
                    color: [1.0, 1.0, 1.0, 1.0],
                });
            }

            particles
        }));
    }

    let particles = handles
        .into_iter()
        .map(|h| h.join())
        .collect::<std::result::Result<Vec<_>, _>>()
        .unwrap()
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();

    let vertex_buffer = create_gpu_only_buffer_from_data(
        context,
        vk::BufferUsageFlags::VERTEX_BUFFER | vk::BufferUsageFlags::STORAGE_BUFFER,
        &particles,
    )?;

    let time = Instant::now() - start;
    log::info!("Generated particles in {time:?}");

    Ok(vertex_buffer)
}

fn create_graphics_pipeline(
    context: &Context,
    layout: &PipelineLayout,
    color_attachement_format: vk::Format,
) -> Result<GraphicsPipeline> {
    context.create_graphics_pipeline::<Particle>(
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
            primitive_topology: vk::PrimitiveTopology::POINT_LIST,
            extent: None,
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
            dynamic_states: Some(&[vk::DynamicState::SCISSOR, vk::DynamicState::VIEWPORT]),
        },
    )
}
