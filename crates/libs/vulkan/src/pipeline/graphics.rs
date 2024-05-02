use std::{ffi::CString, sync::Arc};

use anyhow::Result;
use ash::vk;

use crate::{device::Device, Context, PipelineLayout, ShaderModule};

pub struct GraphicsPipeline {
    device: Arc<Device>,
    pub(crate) inner: vk::Pipeline,
}

#[derive(Debug, Clone, Copy)]
pub struct GraphicsPipelineCreateInfo<'a> {
    pub shaders: &'a [GraphicsShaderCreateInfo<'a>],
    pub primitive_topology: vk::PrimitiveTopology,
    pub cull_mode: vk::CullModeFlags,
    pub extent: Option<vk::Extent2D>,
    pub color_attachments: ColorAttachmentsInfo<'a>,
    pub depth: Option<DepthInfo>,
    pub dynamic_states: Option<&'a [vk::DynamicState]>,
}

#[derive(Debug, Clone, Copy)]
pub struct ColorAttachmentsInfo<'a> {
    pub formats: &'a [vk::Format],
    pub blends: &'a [vk::PipelineColorBlendAttachmentState],
}

#[derive(Debug, Clone, Copy)]
pub struct DepthInfo {
    pub format: vk::Format,
    pub enable_depth_test: bool,
    pub enable_depth_write: bool,
}

pub trait Vertex {
    fn bindings() -> Vec<vk::VertexInputBindingDescription>;
    fn attributes() -> Vec<vk::VertexInputAttributeDescription>;
}

#[derive(Debug, Clone, Copy)]
pub struct GraphicsShaderCreateInfo<'a> {
    pub source: &'a [u8],
    pub stage: vk::ShaderStageFlags,
}

impl GraphicsPipeline {
    pub(crate) fn new<V: Vertex>(
        device: Arc<Device>,
        layout: &PipelineLayout,
        create_info: GraphicsPipelineCreateInfo,
    ) -> Result<Self> {
        // shaders
        let mut shader_modules = vec![];
        let mut shader_stages_infos = vec![];

        let entry_point_name = CString::new("main").unwrap();

        for shader in create_info.shaders.iter() {
            let module = ShaderModule::from_bytes(device.clone(), shader.source)?;

            let stage = vk::PipelineShaderStageCreateInfo::builder()
                .stage(shader.stage)
                .module(module.inner)
                .name(&entry_point_name)
                .build();

            shader_modules.push(module);
            shader_stages_infos.push(stage);
        }

        // vertex
        let vertex_bindings = V::bindings();
        let vertex_attributes = V::attributes();
        let vertex_input_info = vk::PipelineVertexInputStateCreateInfo::builder()
            .vertex_binding_descriptions(&vertex_bindings)
            .vertex_attribute_descriptions(&vertex_attributes);

        let input_assembly_info = vk::PipelineInputAssemblyStateCreateInfo::builder()
            .topology(create_info.primitive_topology)
            .primitive_restart_enable(false);

        // viewport/scissors
        let viewports = create_info
            .extent
            .map(|e| {
                vec![vk::Viewport {
                    x: 0.0,
                    y: 0.0,
                    width: e.width as _,
                    height: e.height as _,
                    min_depth: 0.0,
                    max_depth: 1.0,
                }]
            })
            .unwrap_or_default();
        let scissors = create_info
            .extent
            .map(|e| {
                vec![vk::Rect2D {
                    offset: vk::Offset2D { x: 0, y: 0 },
                    extent: e,
                }]
            })
            .unwrap_or_default();

        let viewport_info = vk::PipelineViewportStateCreateInfo::builder()
            .viewports(&viewports)
            .viewport_count(1)
            .scissors(&scissors)
            .scissor_count(1);

        // raster
        let rasterizer_info = vk::PipelineRasterizationStateCreateInfo::builder()
            .depth_clamp_enable(false)
            .rasterizer_discard_enable(false)
            .polygon_mode(vk::PolygonMode::FILL)
            .line_width(1.0)
            .cull_mode(create_info.cull_mode)
            .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
            .depth_bias_enable(false)
            .depth_bias_constant_factor(0.0)
            .depth_bias_clamp(0.0)
            .depth_bias_slope_factor(0.0);

        // msaa
        let multisampling_info = vk::PipelineMultisampleStateCreateInfo::builder()
            .sample_shading_enable(false)
            .rasterization_samples(vk::SampleCountFlags::TYPE_1)
            .min_sample_shading(1.0)
            .alpha_to_coverage_enable(false)
            .alpha_to_one_enable(false);

        // blending
        let color_blending_info = vk::PipelineColorBlendStateCreateInfo::builder()
            .logic_op_enable(false)
            .logic_op(vk::LogicOp::COPY)
            .attachments(create_info.color_attachments.blends)
            .blend_constants([0.0, 0.0, 0.0, 0.0]);

        // depth
        let depth_stencil_info = create_info.depth.map(|d| {
            vk::PipelineDepthStencilStateCreateInfo::builder()
                .depth_test_enable(d.enable_depth_test)
                .depth_write_enable(d.enable_depth_write)
                .depth_compare_op(vk::CompareOp::LESS_OR_EQUAL)
                .depth_bounds_test_enable(false)
                .min_depth_bounds(0.0)
                .max_depth_bounds(1.0)
                .stencil_test_enable(false)
                .front(Default::default())
                .back(Default::default())
        });

        // dynamic states
        let dynamic_state_info = vk::PipelineDynamicStateCreateInfo::builder()
            .dynamic_states(create_info.dynamic_states.unwrap_or(&[]));

        // dynamic rendering
        let mut rendering_info = vk::PipelineRenderingCreateInfo::builder()
            .color_attachment_formats(create_info.color_attachments.formats);
        if let Some(d) = create_info.depth {
            rendering_info = rendering_info.depth_attachment_format(d.format);
        }

        let mut pipeline_info = vk::GraphicsPipelineCreateInfo::builder()
            .stages(&shader_stages_infos)
            .vertex_input_state(&vertex_input_info)
            .input_assembly_state(&input_assembly_info)
            .viewport_state(&viewport_info)
            .rasterization_state(&rasterizer_info)
            .multisample_state(&multisampling_info)
            .color_blend_state(&color_blending_info)
            .dynamic_state(&dynamic_state_info)
            .layout(layout.inner)
            .push_next(&mut rendering_info);

        // depth
        if let Some(info) = &depth_stencil_info {
            pipeline_info = pipeline_info.depth_stencil_state(info);
        }

        let inner = unsafe {
            device
                .inner
                .create_graphics_pipelines(
                    vk::PipelineCache::null(),
                    std::slice::from_ref(&pipeline_info),
                    None,
                )
                .map_err(|e| e.1)?[0]
        };

        Ok(Self { device, inner })
    }
}

impl Context {
    pub fn create_graphics_pipeline<V: Vertex>(
        &self,
        layout: &PipelineLayout,
        create_info: GraphicsPipelineCreateInfo,
    ) -> Result<GraphicsPipeline> {
        GraphicsPipeline::new::<V>(self.device.clone(), layout, create_info)
    }
}

impl Drop for GraphicsPipeline {
    fn drop(&mut self) {
        unsafe { self.device.inner.destroy_pipeline(self.inner, None) };
    }
}
