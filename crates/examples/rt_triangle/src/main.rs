use app::anyhow::Result;
use app::{App, ImageAndView};
use ash::vk::{self, Packed24_8};
use std::mem::size_of;
use vulkan::utils::*;
use vulkan::*;

const WIDTH: u32 = 1024;
const HEIGHT: u32 = 576;
const APP_NAME: &str = "Triangle advanced";

fn main() -> Result<()> {
    app::run::<Triangle>(APP_NAME, WIDTH, HEIGHT)
}

struct Triangle {
    _bottom_as: BottomAS,
    _top_as: TopAS,
    pipeline_res: PipelineRes,
    sbt: VkShaderBindingTable,
    descriptor_res: DescriptorRes,
}

impl App for Triangle {
    type Gui = ();

    fn new(base: &mut app::BaseApp<Self>) -> Result<Self> {
        let context = &mut base.context;

        let bottom_as = create_bottom_as(context)?;

        let top_as = create_top_as(context, &bottom_as)?;

        let pipeline_res = create_pipeline(context)?;

        let sbt = context.create_shader_binding_table(&pipeline_res.pipeline)?;

        let descriptor_res = create_descriptor_sets(
            context,
            &pipeline_res,
            &top_as,
            base.storage_images.as_slice(),
        )?;

        Ok(Self {
            _bottom_as: bottom_as,
            _top_as: top_as,
            pipeline_res,
            sbt,
            descriptor_res,
        })
    }

    fn update(&self, _: &app::BaseApp<Self>, _: &mut <Self as App>::Gui, _: usize) -> Result<()> {
        Ok(())
    }

    fn record_command(
        &self,
        base: &app::BaseApp<Self>,
        buffer: &VkCommandBuffer,
        image_index: usize,
    ) -> Result<()> {
        let static_set = &self.descriptor_res.static_set;
        let dynamic_set = &self.descriptor_res.dynamic_sets[image_index];

        buffer.bind_pipeline(
            vk::PipelineBindPoint::RAY_TRACING_KHR,
            &self.pipeline_res.pipeline,
        );

        buffer.bind_descriptor_sets(
            vk::PipelineBindPoint::RAY_TRACING_KHR,
            &self.pipeline_res.pipeline_layout,
            0,
            &[static_set, dynamic_set],
        );

        buffer.trace_rays(
            &self.sbt,
            base.swapchain.extent.width,
            base.swapchain.extent.height,
        );

        Ok(())
    }

    fn on_recreate_swapchain(&self, storage_images: &[app::ImageAndView]) -> Result<()> {
        storage_images.iter().enumerate().for_each(|(index, img)| {
            let set = &self.descriptor_res.dynamic_sets[index];

            set.update(&[VkWriteDescriptorSet {
                binding: 1,
                kind: VkWriteDescriptorSetKind::StorageImage {
                    layout: vk::ImageLayout::GENERAL,
                    view: &img.view,
                },
            }]);
        });

        Ok(())
    }
}

struct BottomAS {
    inner: VkAccelerationStructure,
    _vertex_buffer: VkBuffer,
    _index_buffer: VkBuffer,
}

struct TopAS {
    inner: VkAccelerationStructure,
    _instance_buffer: VkBuffer,
}

struct PipelineRes {
    pipeline: VkRTPipeline,
    pipeline_layout: VkPipelineLayout,
    static_dsl: VkDescriptorSetLayout,
    dynamic_dsl: VkDescriptorSetLayout,
}

struct DescriptorRes {
    _pool: VkDescriptorPool,
    static_set: VkDescriptorSet,
    dynamic_sets: Vec<VkDescriptorSet>,
}

fn create_bottom_as(context: &mut VkContext) -> Result<BottomAS> {
    // Triangle geo
    #[derive(Debug, Clone, Copy)]
    #[allow(dead_code)]
    struct Vertex {
        pos: [f32; 2],
    }

    const VERTICES: [Vertex; 3] = [
        Vertex { pos: [-1.0, 1.0] },
        Vertex { pos: [1.0, 1.0] },
        Vertex { pos: [0.0, -1.0] },
    ];

    let vertex_buffer = create_gpu_only_buffer_from_data(
        context,
        vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS
            | vk::BufferUsageFlags::ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR,
        &VERTICES,
    )?;
    let vertex_buffer_addr = vertex_buffer.get_device_address();

    const INDICES: [u16; 3] = [0, 1, 2];

    let index_buffer = create_gpu_only_buffer_from_data(
        context,
        vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS
            | vk::BufferUsageFlags::ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR,
        &INDICES,
    )?;
    let index_buffer_addr = index_buffer.get_device_address();

    let as_geo_triangles_data = vk::AccelerationStructureGeometryTrianglesDataKHR::builder()
        .vertex_format(vk::Format::R32G32_SFLOAT)
        .vertex_data(vk::DeviceOrHostAddressConstKHR {
            device_address: vertex_buffer_addr,
        })
        .vertex_stride(size_of::<Vertex>() as _)
        .index_type(vk::IndexType::UINT16)
        .index_data(vk::DeviceOrHostAddressConstKHR {
            device_address: index_buffer_addr,
        })
        .max_vertex(INDICES.len() as _)
        .build();

    let as_struct_geo = vk::AccelerationStructureGeometryKHR::builder()
        .geometry_type(vk::GeometryTypeKHR::TRIANGLES)
        .flags(vk::GeometryFlagsKHR::OPAQUE)
        .geometry(vk::AccelerationStructureGeometryDataKHR {
            triangles: as_geo_triangles_data,
        })
        .build();

    let build_range_info = vk::AccelerationStructureBuildRangeInfoKHR::builder()
        .first_vertex(0)
        .primitive_count(1)
        .primitive_offset(0)
        .transform_offset(0)
        .build();

    let inner = context.create_bottom_level_acceleration_structure(
        &[as_struct_geo],
        &[build_range_info],
        &[1],
    )?;

    Ok(BottomAS {
        inner,
        _vertex_buffer: vertex_buffer,
        _index_buffer: index_buffer,
    })
}

fn create_top_as(context: &mut VkContext, bottom_as: &BottomAS) -> Result<TopAS> {
    #[rustfmt::skip]
    let transform_matrix = vk::TransformMatrixKHR { matrix: [
        1.0, 0.0, 0.0, 0.0,
        0.0, 1.0, 0.0, 0.0,
        0.0, 0.0, 1.0, 0.0
    ]};

    let as_instance = vk::AccelerationStructureInstanceKHR {
        transform: transform_matrix,
        instance_custom_index_and_mask: Packed24_8::new(0, 0xFF),
        instance_shader_binding_table_record_offset_and_flags: Packed24_8::new(
            0,
            vk::GeometryInstanceFlagsKHR::TRIANGLE_FACING_CULL_DISABLE
                .as_raw()
                .try_into()
                .unwrap(),
        ),
        acceleration_structure_reference: vk::AccelerationStructureReferenceKHR {
            device_handle: bottom_as.inner.address,
        },
    };

    let instance_buffer = create_gpu_only_buffer_from_data(
        context,
        vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS
            | vk::BufferUsageFlags::ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR,
        &[as_instance],
    )?;
    let instance_buffer_addr = instance_buffer.get_device_address();

    let as_struct_geo = vk::AccelerationStructureGeometryKHR::builder()
        .geometry_type(vk::GeometryTypeKHR::INSTANCES)
        .flags(vk::GeometryFlagsKHR::OPAQUE)
        .geometry(vk::AccelerationStructureGeometryDataKHR {
            instances: vk::AccelerationStructureGeometryInstancesDataKHR::builder()
                .array_of_pointers(false)
                .data(vk::DeviceOrHostAddressConstKHR {
                    device_address: instance_buffer_addr,
                })
                .build(),
        })
        .build();

    let build_range_info = vk::AccelerationStructureBuildRangeInfoKHR::builder()
        .first_vertex(0)
        .primitive_count(1)
        .primitive_offset(0)
        .transform_offset(0)
        .build();

    let inner = context.create_top_level_acceleration_structure(
        &[as_struct_geo],
        &[build_range_info],
        &[1],
    )?;

    Ok(TopAS {
        inner,
        _instance_buffer: instance_buffer,
    })
}

fn create_pipeline(context: &VkContext) -> Result<PipelineRes> {
    // descriptor and pipeline layouts
    let static_layout_bindings = [vk::DescriptorSetLayoutBinding::builder()
        .binding(0)
        .descriptor_type(vk::DescriptorType::ACCELERATION_STRUCTURE_KHR)
        .descriptor_count(1)
        .stage_flags(vk::ShaderStageFlags::RAYGEN_KHR | vk::ShaderStageFlags::CLOSEST_HIT_KHR)
        .build()];

    let dynamic_layout_bindings = [vk::DescriptorSetLayoutBinding::builder()
        .binding(1)
        .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
        .descriptor_count(1)
        .stage_flags(vk::ShaderStageFlags::RAYGEN_KHR)
        .build()];

    let static_dsl = context.create_descriptor_set_layout(&static_layout_bindings)?;
    let dynamic_dsl = context.create_descriptor_set_layout(&dynamic_layout_bindings)?;
    let dsls = [&static_dsl, &dynamic_dsl];

    let pipeline_layout = context.create_pipeline_layout(&dsls)?;

    // Shaders
    let shaders_create_info = [
        VkRTShaderCreateInfo {
            source: &include_bytes!("../shaders/raygen.rgen.spv")[..],
            stage: vk::ShaderStageFlags::RAYGEN_KHR,
            group: VkRTShaderGroup::RayGen,
        },
        VkRTShaderCreateInfo {
            source: &include_bytes!("../shaders/miss.rmiss.spv")[..],
            stage: vk::ShaderStageFlags::MISS_KHR,
            group: VkRTShaderGroup::Miss,
        },
        VkRTShaderCreateInfo {
            source: &include_bytes!("../shaders/closesthit.rchit.spv")[..],
            stage: vk::ShaderStageFlags::CLOSEST_HIT_KHR,
            group: VkRTShaderGroup::ClosestHit,
        },
    ];

    let pipeline_create_info = VkRTPipelineCreateInfo {
        shaders: &shaders_create_info,
        max_ray_recursion_depth: 1,
    };

    let pipeline = context.create_ray_tracing_pipeline(&pipeline_layout, &pipeline_create_info)?;

    Ok(PipelineRes {
        pipeline,
        pipeline_layout,
        static_dsl,
        dynamic_dsl,
    })
}

fn create_descriptor_sets(
    context: &VkContext,
    pipeline_res: &PipelineRes,
    top_as: &TopAS,
    storage_imgs: &[ImageAndView],
) -> Result<DescriptorRes> {
    let set_count = storage_imgs.len() as u32;

    let pool_sizes = [
        vk::DescriptorPoolSize::builder()
            .ty(vk::DescriptorType::ACCELERATION_STRUCTURE_KHR)
            .descriptor_count(1)
            .build(),
        vk::DescriptorPoolSize::builder()
            .ty(vk::DescriptorType::STORAGE_IMAGE)
            .descriptor_count(set_count)
            .build(),
    ];

    let pool = context.create_descriptor_pool(set_count + 1, &pool_sizes)?;

    let static_set = pool.allocate_set(&pipeline_res.static_dsl)?;
    let dynamic_sets = pool.allocate_sets(&pipeline_res.dynamic_dsl, set_count)?;

    static_set.update(&[VkWriteDescriptorSet {
        binding: 0,
        kind: VkWriteDescriptorSetKind::AccelerationStructure {
            acceleration_structure: &top_as.inner,
        },
    }]);

    dynamic_sets.iter().enumerate().for_each(|(index, set)| {
        set.update(&[VkWriteDescriptorSet {
            binding: 1,
            kind: VkWriteDescriptorSetKind::StorageImage {
                layout: vk::ImageLayout::GENERAL,
                view: &storage_imgs[index].view,
            },
        }]);
    });

    Ok(DescriptorRes {
        _pool: pool,
        dynamic_sets,
        static_set,
    })
}
