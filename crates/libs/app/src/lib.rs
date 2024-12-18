pub extern crate anyhow;
pub extern crate glam;
pub extern crate log;
pub extern crate vulkan;

mod camera;

use anyhow::Result;
use ash::vk::{self};
use camera::{Camera, Controls};
use glam::vec3;
use gpu_allocator::MemoryLocation;
use gui::{
    egui::{self, Align2, ClippedPrimitive, FullOutput, TextureId},
    GuiContext,
};
use std::{
    fs::OpenOptions,
    time::{Duration, Instant},
};
use vulkan::*;
use winit::{
    application::ApplicationHandler,
    dpi::PhysicalSize,
    event::{ElementState, KeyEvent, MouseButton, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::Window,
};

const IN_FLIGHT_FRAMES: u32 = 2;

pub struct BaseApp {
    raytracing_enabled: bool,
    pub swapchain: Swapchain,
    pub command_pool: CommandPool,
    pub storage_images: Vec<ImageAndView>,
    pub command_buffers: Vec<CommandBuffer>,
    in_flight_frames: InFlightFrames,
    pub camera: Camera,
    stats_display_mode: StatsDisplayMode,

    pub gui_context: GuiContext,

    pub context: Context, // make sure it's dropped last

    requested_swapchain_format: Option<vk::SurfaceFormatKHR>,
}

#[derive(Debug, Default, Copy, Clone)]
pub struct AppConfig<'a, 'b> {
    pub enable_raytracing: bool,
    pub required_instance_extensions: &'a [&'b str],
    pub enable_independent_blend: bool,
}

pub trait App: Sized {
    type Gui: Gui;

    fn new(base: &mut BaseApp) -> Result<Self>;

    fn update(
        &mut self,
        base: &mut BaseApp,
        gui: &mut Self::Gui,
        image_index: usize,
        delta_time: Duration,
    ) -> Result<()>;

    fn record_raytracing_commands(
        &self,
        base: &BaseApp,
        buffer: &CommandBuffer,
        image_index: usize,
    ) -> Result<()> {
        // prevents reports of unused parameters without needing to use #[allow]
        let _ = base;
        let _ = buffer;
        let _ = image_index;

        Ok(())
    }

    fn record_raster_commands(&self, base: &BaseApp, image_index: usize) -> Result<()> {
        // prevents reports of unused parameters without needing to use #[allow]
        let _ = base;
        let _ = image_index;

        Ok(())
    }

    fn on_recreate_swapchain(&mut self, base: &BaseApp) -> Result<()>;
}

pub trait Gui: Sized {
    fn new(base: &BaseApp) -> Result<Self>;

    fn build(&mut self, ui: &egui::Context);
}

impl Gui for () {
    fn new(base: &BaseApp) -> Result<Self> {
        // prevents reports of unused parameters without needing to use #[allow]
        let _ = base;

        Ok(())
    }

    fn build(&mut self, _ui: &egui::Context) {}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StatsDisplayMode {
    None,
    Basic,
    Full,
}

impl StatsDisplayMode {
    fn next(self) -> Self {
        match self {
            Self::None => Self::Basic,
            Self::Basic => Self::Full,
            Self::Full => Self::None,
        }
    }
}

pub fn run<A: App + 'static>(
    app_name: &str,
    width: u32,
    height: u32,
    app_config: AppConfig,
) -> Result<()> {
    let log_to_file = std::env::args().any(|a| "--log-to-file" == a);
    setup_logs(app_name, log_to_file);

    let mut wrapper = AppWrapper::<A> {
        app_name,
        width,
        height,
        app_config,

        controls: Controls::default(),
        is_swapchain_dirty: false,
        last_frame: Instant::now(),
        frame_stats: FrameStats::default(),

        base_app: None,
        window: None,
        app: None,
        gui: None,
    };

    let event_loop = EventLoop::new().unwrap();
    event_loop.set_control_flow(ControlFlow::Poll);
    event_loop.run_app(&mut wrapper)?;

    Ok(())
}

struct AppWrapper<'a, A: App> {
    app_name: &'a str,
    width: u32,
    height: u32,
    app_config: AppConfig<'a, 'a>, // FIXME: lifetimes ?

    controls: Controls,
    is_swapchain_dirty: bool,
    last_frame: Instant,
    frame_stats: FrameStats,

    window: Option<Window>,
    app: Option<A>,
    gui: Option<A::Gui>,
    base_app: Option<BaseApp>,
}

impl<A: App> ApplicationHandler for AppWrapper<'_, A> {
    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        let window = create_window(event_loop, self.app_name, self.width, self.height)
            .expect("Failed to create window");
        let mut base_app = BaseApp::new(&window, self.app_name, self.app_config)
            .expect("Failed to create base app");

        self.window = Some(window);
        self.gui = Some(A::Gui::new(&base_app).expect("Failed to create gui"));
        self.app = Some(A::new(&mut base_app).expect("Failed to create application"));
        self.base_app = Some(base_app);
    }

    fn new_events(&mut self, _: &ActiveEventLoop, _: winit::event::StartCause) {
        let now = Instant::now();
        let frame_time = now - self.last_frame;
        self.last_frame = now;

        self.frame_stats.set_frame_time(frame_time);

        self.controls = self.controls.reset();
    }

    fn window_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        _: winit::window::WindowId,
        event: WindowEvent,
    ) {
        let base_app = self.base_app.as_mut().unwrap();

        base_app
            .gui_context
            .handle_event(self.window.as_ref().unwrap(), &event);

        self.controls = self.controls.handle_window_event(&event);

        match event {
            // On resize
            WindowEvent::Resized(..) => {
                self.is_swapchain_dirty = true;
            }
            // Keyboard
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        state,
                        physical_key,
                        ..
                    },
                ..
            } => {
                if matches!(physical_key, PhysicalKey::Code(KeyCode::KeyR))
                    && state == ElementState::Pressed
                {
                    base_app.toggle_stats();
                }
            }
            // Mouse
            WindowEvent::MouseInput { state, button, .. } => {
                if button == MouseButton::Right {
                    if state == ElementState::Pressed {
                        self.window.as_ref().unwrap().set_cursor_visible(false);
                    } else {
                        self.window.as_ref().unwrap().set_cursor_visible(true);
                    }
                }
            }
            // Exit app on request to close window
            WindowEvent::CloseRequested => event_loop.exit(),
            _ => (),
        }
    }

    fn device_event(
        &mut self,
        _: &ActiveEventLoop,
        _: winit::event::DeviceId,
        event: winit::event::DeviceEvent,
    ) {
        self.controls = self.controls.handle_device_event(&event);
    }

    fn about_to_wait(&mut self, _: &ActiveEventLoop) {
        let base_app = self.base_app.as_mut().unwrap();

        if self.is_swapchain_dirty || base_app.requested_swapchain_format.is_some() {
            let dim = self.window.as_ref().unwrap().inner_size();
            let format = base_app.requested_swapchain_format.take();

            if dim.width > 0 && dim.height > 0 {
                base_app
                    .recreate_swapchain(dim.width, dim.height, format)
                    .expect("Failed to recreate swapchain");
                self.app
                    .as_mut()
                    .unwrap()
                    .on_recreate_swapchain(base_app)
                    .expect("Error on recreate swapchain callback");
            } else {
                return;
            }
        }

        base_app.camera = base_app
            .camera
            .update(&self.controls, self.frame_stats.frame_time);

        self.is_swapchain_dirty = base_app
            .draw(
                self.window.as_ref().unwrap(),
                self.app.as_mut().unwrap(),
                self.gui.as_mut().unwrap(),
                &mut self.frame_stats,
            )
            .expect("Failed to tick");
    }

    fn exiting(&mut self, _: &ActiveEventLoop) {
        self.base_app
            .as_mut()
            .unwrap()
            .wait_for_gpu()
            .expect("Failed to wait for gpu when exiting")
    }
}

fn setup_logs(app_name: &str, log_to_file: bool) {
    use simplelog::*;

    const LEVEL: log::LevelFilter = log::LevelFilter::Debug;

    let mut loggers: Vec<Box<dyn SharedLogger>> = vec![TermLogger::new(
        LEVEL,
        Config::default(),
        TerminalMode::Mixed,
        ColorChoice::Auto,
    )];
    if log_to_file {
        loggers.push(WriteLogger::new(
            LEVEL,
            Config::default(),
            OpenOptions::new()
                .create(true)
                .truncate(true)
                .write(true)
                .open(format!("{app_name}.log"))
                .expect("log file"),
        ));
    }

    CombinedLogger::init(loggers).expect("logger");
}

fn create_window(
    evt_loop: &ActiveEventLoop,
    app_name: &str,
    width: u32,
    height: u32,
) -> Result<Window> {
    log::debug!("Creating window");
    evt_loop.set_control_flow(ControlFlow::Poll);

    let window = evt_loop.create_window(
        Window::default_attributes()
            .with_title(app_name)
            .with_inner_size(PhysicalSize::new(width, height))
            .with_resizable(true),
    )?;

    Ok(window)
}

impl BaseApp {
    fn new(window: &Window, app_name: &str, app_config: AppConfig) -> Result<Self> {
        log::info!("Create application");

        let AppConfig {
            enable_raytracing,
            required_instance_extensions,
            enable_independent_blend,
        } = app_config;

        // Vulkan context
        let mut required_extensions = vec!["VK_KHR_swapchain"];
        if app_config.enable_raytracing {
            required_extensions.push("VK_KHR_ray_tracing_pipeline");
            required_extensions.push("VK_KHR_acceleration_structure");
            required_extensions.push("VK_KHR_deferred_host_operations");
        }

        let mut context = ContextBuilder::new(window, window)
            .vulkan_version(VERSION_1_3)
            .app_name(app_name)
            .required_instance_extensions(required_instance_extensions)
            .required_device_extensions(&required_extensions)
            .required_device_features(DeviceFeatures {
                ray_tracing_pipeline: enable_raytracing,
                acceleration_structure: enable_raytracing,
                runtime_descriptor_array: enable_raytracing,
                buffer_device_address: enable_raytracing,
                dynamic_rendering: true,
                synchronization2: true,
                independent_blend: enable_independent_blend,
            })
            .with_raytracing_context(enable_raytracing)
            .build()?;

        let command_pool = context.create_command_pool(
            context.graphics_queue_family,
            Some(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER),
        )?;

        let swapchain = Swapchain::new(
            &context,
            window.inner_size().width,
            window.inner_size().height,
        )?;

        let storage_images = if enable_raytracing {
            create_storage_images(&mut context, swapchain.extent, swapchain.images.len())?
        } else {
            vec![]
        };

        let command_buffers = create_command_buffers(&command_pool, &swapchain)?;

        let in_flight_frames = InFlightFrames::new(&context, IN_FLIGHT_FRAMES)?;

        let camera = Camera::new(
            vec3(0.0, 0.0, 1.0),
            vec3(0.0, 0.0, -1.0),
            60.0,
            window.inner_size().width as f32 / window.inner_size().height as f32,
            0.1,
            10.0,
        );

        let gui_context =
            GuiContext::new(&context, swapchain.format, window, IN_FLIGHT_FRAMES as _)?;

        Ok(Self {
            raytracing_enabled: enable_raytracing,
            context,
            command_pool,
            swapchain,
            storage_images,
            command_buffers,
            in_flight_frames,
            camera,
            stats_display_mode: StatsDisplayMode::Basic,
            gui_context,

            requested_swapchain_format: None,
        })
    }

    pub fn request_swapchain_format_change(&mut self, format: vk::SurfaceFormatKHR) {
        self.requested_swapchain_format = Some(format);
    }

    fn recreate_swapchain(
        &mut self,
        width: u32,
        height: u32,
        format: Option<vk::SurfaceFormatKHR>,
    ) -> Result<()> {
        log::debug!("Recreating the swapchain");

        self.wait_for_gpu()?;

        // Swapchain and dependent resources
        self.swapchain
            .update(&self.context, width, height, format)?;

        // Recreate storage image for RT and update descriptor set
        if self.raytracing_enabled {
            let storage_images = create_storage_images(
                &mut self.context,
                self.swapchain.extent,
                self.swapchain.images.len(),
            )?;
            let _ = std::mem::replace(&mut self.storage_images, storage_images);
        }

        // Update ui renderer
        if let Some(format) = format {
            self.gui_context.update_framebuffer_params(format.format)?;
        }

        // Update camera aspect ration
        self.camera.aspect_ratio = width as f32 / height as f32;

        Ok(())
    }

    pub fn wait_for_gpu(&self) -> Result<()> {
        self.context.device_wait_idle()
    }

    fn draw<B: App>(
        &mut self,
        window: &Window,
        base_app: &mut B,
        gui: &mut B::Gui,
        frame_stats: &mut FrameStats,
    ) -> Result<bool> {
        // Drawing the frame
        self.in_flight_frames.next();
        self.in_flight_frames.fence().wait(None)?;

        // Can't get for gpu time on the first frames or vkGetQueryPoolResults gets stuck
        // due to VK_QUERY_RESULT_WAIT_BIT
        let gpu_time = (frame_stats.total_frame_count >= IN_FLIGHT_FRAMES)
            .then(|| self.in_flight_frames.gpu_frame_time_ms())
            .transpose()?
            .unwrap_or_default();
        frame_stats.set_gpu_time_time(gpu_time);
        frame_stats.tick();

        let next_image_result = self
            .swapchain
            .acquire_next_image(u64::MAX, self.in_flight_frames.image_available_semaphore());
        let image_index = match next_image_result {
            Ok(AcquiredImage { index, .. }) => index as usize,
            Err(err) => match err.downcast_ref::<vk::Result>() {
                Some(&vk::Result::ERROR_OUT_OF_DATE_KHR) => return Ok(true),
                _ => panic!("Error while acquiring next image. Cause: {}", err),
            },
        };
        self.in_flight_frames.fence().reset()?;

        // UI
        if !self.in_flight_frames.gui_textures_to_free().is_empty() {
            self.gui_context
                .free_textures(self.in_flight_frames.gui_textures_to_free())?;
        }

        let raw_input = self.gui_context.take_input(window);

        let FullOutput {
            platform_output,
            textures_delta,
            shapes,
            pixels_per_point,
            ..
        } = self.gui_context.run(raw_input, |ctx| {
            gui.build(ctx);
            self.build_perf_ui(ctx, frame_stats);
        });

        self.gui_context
            .handle_platform_output(window, platform_output);

        if !textures_delta.free.is_empty() {
            self.in_flight_frames
                .set_gui_textures_to_free(textures_delta.free);
        }

        if !textures_delta.set.is_empty() {
            self.gui_context
                .set_textures(
                    self.context.graphics_queue.inner,
                    self.context.command_pool.inner,
                    textures_delta.set.as_slice(),
                )
                .expect("Failed to update texture");
        }

        let primitives = self.gui_context.tessellate(shapes, pixels_per_point);

        base_app.update(self, gui, image_index, frame_stats.frame_time)?;

        self.record_command_buffer(image_index, base_app, pixels_per_point, &primitives)?;

        let command_buffer = &self.command_buffers[image_index];
        self.context.graphics_queue.submit(
            command_buffer,
            Some(SemaphoreSubmitInfo {
                semaphore: self.in_flight_frames.image_available_semaphore(),
                stage_mask: vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
            }),
            Some(SemaphoreSubmitInfo {
                semaphore: self.in_flight_frames.render_finished_semaphore(),
                stage_mask: vk::PipelineStageFlags2::ALL_COMMANDS,
            }),
            self.in_flight_frames.fence(),
        )?;

        let signal_semaphores = [self.in_flight_frames.render_finished_semaphore()];
        let present_result = self.swapchain.queue_present(
            image_index as _,
            &signal_semaphores,
            &self.context.present_queue,
        );
        match present_result {
            Ok(true) => return Ok(true),
            Err(err) => match err.downcast_ref::<vk::Result>() {
                Some(&vk::Result::ERROR_OUT_OF_DATE_KHR) => return Ok(true),
                _ => panic!("Failed to present queue. Cause: {}", err),
            },
            _ => {}
        }

        Ok(false)
    }

    fn build_perf_ui(&self, ctx: &gui::egui::Context, frame_stats: &mut FrameStats) {
        if matches!(
            self.stats_display_mode,
            StatsDisplayMode::Basic | StatsDisplayMode::Full
        ) {
            egui::Window::new("Frame stats")
                .anchor(Align2::RIGHT_TOP, [-5.0, 5.0])
                .collapsible(false)
                .interactable(false)
                .resizable(false)
                .drag_to_scroll(false)
                .show(ctx, |ui| {
                    ui.label("Framerate");
                    ui.label(format!("{} fps", frame_stats.fps_counter));
                    ui.label("Frametimes");
                    ui.label(format!("all - {:?}", frame_stats.frame_time));
                    ui.label(format!("cpu - {:?}", frame_stats.cpu_time));
                    ui.label(format!("gpu - {:?}", frame_stats.gpu_time));
                });
        }

        if matches!(self.stats_display_mode, StatsDisplayMode::Full) {
            egui::TopBottomPanel::bottom("frametime_graphs").show(ctx, |ui| {
                build_frametime_plot(ui, "Frames", &frame_stats.frame_time_ms_log.0);
                ui.add_space(5.0);
                build_frametime_plot(ui, "CPU", &frame_stats.cpu_time_ms_log.0);
                ui.add_space(5.0);
                build_frametime_plot(ui, "GPU", &frame_stats.gpu_time_ms_log.0);
            });
        }
    }

    fn record_command_buffer<B: App>(
        &mut self,
        image_index: usize,
        base_app: &B,
        pixels_per_point: f32,
        primitives: &[ClippedPrimitive],
    ) -> Result<()> {
        self.command_buffers[image_index].reset()?;

        self.command_buffers[image_index].begin(None)?;

        self.command_buffers[image_index]
            .reset_all_timestamp_queries_from_pool(self.in_flight_frames.timing_query_pool());

        self.command_buffers[image_index].write_timestamp(
            vk::PipelineStageFlags2::NONE,
            self.in_flight_frames.timing_query_pool(),
            0,
        );

        if self.raytracing_enabled {
            base_app.record_raytracing_commands(
                self,
                &self.command_buffers[image_index],
                image_index,
            )?;

            let storage_image = &self.storage_images[image_index].image;
            // Copy ray tracing result into swapchain
            self.command_buffers[image_index].pipeline_image_barriers(&[
                ImageBarrier {
                    image: &self.swapchain.images[image_index],
                    old_layout: vk::ImageLayout::UNDEFINED,
                    new_layout: vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    src_access_mask: vk::AccessFlags2::empty(),
                    dst_access_mask: vk::AccessFlags2::TRANSFER_WRITE,
                    src_stage_mask: vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
                    dst_stage_mask: vk::PipelineStageFlags2::TRANSFER,
                },
                ImageBarrier {
                    image: storage_image,
                    old_layout: vk::ImageLayout::GENERAL,
                    new_layout: vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                    src_access_mask: vk::AccessFlags2::SHADER_WRITE,
                    dst_access_mask: vk::AccessFlags2::TRANSFER_READ,
                    src_stage_mask: vk::PipelineStageFlags2::RAY_TRACING_SHADER_KHR,
                    dst_stage_mask: vk::PipelineStageFlags2::TRANSFER,
                },
            ]);

            self.command_buffers[image_index].copy_image(
                storage_image,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                &self.swapchain.images[image_index],
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            );

            self.command_buffers[image_index].pipeline_image_barriers(&[
                ImageBarrier {
                    image: &self.swapchain.images[image_index],
                    old_layout: vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    new_layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                    src_access_mask: vk::AccessFlags2::TRANSFER_WRITE,
                    dst_access_mask: vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
                    src_stage_mask: vk::PipelineStageFlags2::TRANSFER,
                    dst_stage_mask: vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
                },
                ImageBarrier {
                    image: storage_image,
                    old_layout: vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                    new_layout: vk::ImageLayout::GENERAL,
                    src_access_mask: vk::AccessFlags2::TRANSFER_READ,
                    dst_access_mask: vk::AccessFlags2::SHADER_WRITE,
                    src_stage_mask: vk::PipelineStageFlags2::TRANSFER,
                    dst_stage_mask: vk::PipelineStageFlags2::RAY_TRACING_SHADER_KHR,
                },
            ]);
        } else {
            self.command_buffers[image_index].pipeline_image_barriers(&[ImageBarrier {
                image: &self.swapchain.images[image_index],
                old_layout: vk::ImageLayout::UNDEFINED,
                new_layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                src_access_mask: vk::AccessFlags2::empty(),
                dst_access_mask: vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
                src_stage_mask: vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
                dst_stage_mask: vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
            }]);
        }

        // Rasterization
        base_app.record_raster_commands(self, image_index)?;

        // UI
        self.command_buffers[image_index].begin_rendering(
            &[RenderingAttachment {
                view: &self.swapchain.views[image_index],
                load_op: vk::AttachmentLoadOp::DONT_CARE,
                clear_value: None,
            }],
            None,
            self.swapchain.extent,
        );

        self.gui_context.renderer.cmd_draw(
            self.command_buffers[image_index].inner,
            self.swapchain.extent,
            pixels_per_point,
            primitives,
        )?;

        self.command_buffers[image_index].end_rendering();

        self.command_buffers[image_index].pipeline_image_barriers(&[ImageBarrier {
            image: &self.swapchain.images[image_index],
            old_layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            new_layout: vk::ImageLayout::PRESENT_SRC_KHR,
            src_access_mask: vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
            dst_access_mask: vk::AccessFlags2::empty(),
            src_stage_mask: vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
            dst_stage_mask: vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
        }]);

        self.command_buffers[image_index].write_timestamp(
            vk::PipelineStageFlags2::TOP_OF_PIPE,
            self.in_flight_frames.timing_query_pool(),
            1,
        );

        self.command_buffers[image_index].end()?;

        Ok(())
    }

    fn toggle_stats(&mut self) {
        self.stats_display_mode = self.stats_display_mode.next();
    }
}

fn create_storage_images(
    context: &mut Context,
    extent: vk::Extent2D,
    count: usize,
) -> Result<Vec<ImageAndView>> {
    let mut images = Vec::with_capacity(count);

    for _ in 0..count {
        let image = context.create_image(
            vk::ImageUsageFlags::TRANSFER_SRC | vk::ImageUsageFlags::STORAGE,
            MemoryLocation::GpuOnly,
            vk::Format::R8G8B8A8_UNORM,
            extent.width,
            extent.height,
        )?;

        let view = image.create_image_view(vk::ImageAspectFlags::COLOR)?;

        context.execute_one_time_commands(|cmd_buffer| {
            cmd_buffer.pipeline_image_barriers(&[ImageBarrier {
                image: &image,
                old_layout: vk::ImageLayout::UNDEFINED,
                new_layout: vk::ImageLayout::GENERAL,
                src_access_mask: vk::AccessFlags2::NONE,
                dst_access_mask: vk::AccessFlags2::SHADER_WRITE,
                src_stage_mask: vk::PipelineStageFlags2::NONE,
                dst_stage_mask: vk::PipelineStageFlags2::RAY_TRACING_SHADER_KHR,
            }]);
        })?;

        images.push(ImageAndView { image, view })
    }

    Ok(images)
}

fn create_command_buffers(pool: &CommandPool, swapchain: &Swapchain) -> Result<Vec<CommandBuffer>> {
    pool.allocate_command_buffers(vk::CommandBufferLevel::PRIMARY, swapchain.images.len() as _)
}

pub struct ImageAndView {
    pub view: ImageView,
    pub image: Image,
}

struct InFlightFrames {
    per_frames: Vec<PerFrame>,
    current_frame: usize,
}

struct PerFrame {
    image_available_semaphore: Semaphore,
    render_finished_semaphore: Semaphore,
    fence: Fence,
    timing_query_pool: TimestampQueryPool<2>,
    gui_textures_to_free: Vec<TextureId>,
}

impl InFlightFrames {
    fn new(context: &Context, frame_count: u32) -> Result<Self> {
        let sync_objects = (0..frame_count)
            .map(|_i| {
                let image_available_semaphore = context.create_semaphore()?;
                let render_finished_semaphore = context.create_semaphore()?;
                let fence = context.create_fence(Some(vk::FenceCreateFlags::SIGNALED))?;

                let timing_query_pool = context.create_timestamp_query_pool()?;
                let gui_textures_to_free = Vec::new();

                Ok(PerFrame {
                    image_available_semaphore,
                    render_finished_semaphore,
                    fence,
                    timing_query_pool,
                    gui_textures_to_free,
                })
            })
            .collect::<Result<Vec<_>>>()?;

        Ok(Self {
            per_frames: sync_objects,
            current_frame: 0,
        })
    }

    fn next(&mut self) {
        self.current_frame = (self.current_frame + 1) % self.per_frames.len();
    }

    fn image_available_semaphore(&self) -> &Semaphore {
        &self.per_frames[self.current_frame].image_available_semaphore
    }

    fn render_finished_semaphore(&self) -> &Semaphore {
        &self.per_frames[self.current_frame].render_finished_semaphore
    }

    fn fence(&self) -> &Fence {
        &self.per_frames[self.current_frame].fence
    }

    fn timing_query_pool(&self) -> &TimestampQueryPool<2> {
        &self.per_frames[self.current_frame].timing_query_pool
    }

    fn gui_textures_to_free(&self) -> &[TextureId] {
        &self.per_frames[self.current_frame].gui_textures_to_free
    }

    fn set_gui_textures_to_free(&mut self, ids: Vec<TextureId>) {
        self.per_frames[self.current_frame].gui_textures_to_free = ids;
    }

    fn gpu_frame_time_ms(&self) -> Result<Duration> {
        let result = self.timing_query_pool().wait_for_all_results()?;
        let time = Duration::from_nanos(result[1].saturating_sub(result[0]));

        Ok(time)
    }
}

#[derive(Debug)]
struct FrameStats {
    // we collect gpu timings the frame after it was computed
    // so we keep frame times for the two last frames
    previous_frame_time: Duration,
    frame_time: Duration,
    cpu_time: Duration,
    gpu_time: Duration,
    frame_time_ms_log: Queue<f32>,
    cpu_time_ms_log: Queue<f32>,
    gpu_time_ms_log: Queue<f32>,
    total_frame_count: u32,
    frame_count: u32,
    fps_counter: u32,
    timer: Duration,
}

impl Default for FrameStats {
    fn default() -> Self {
        Self {
            previous_frame_time: Default::default(),
            frame_time: Default::default(),
            cpu_time: Default::default(),
            gpu_time: Default::default(),
            frame_time_ms_log: Queue::new(FrameStats::MAX_LOG_SIZE),
            cpu_time_ms_log: Queue::new(FrameStats::MAX_LOG_SIZE),
            gpu_time_ms_log: Queue::new(FrameStats::MAX_LOG_SIZE),
            total_frame_count: Default::default(),
            frame_count: Default::default(),
            fps_counter: Default::default(),
            timer: Default::default(),
        }
    }
}

impl FrameStats {
    const ONE_SEC: Duration = Duration::from_secs(1);
    const MAX_LOG_SIZE: usize = 1000;

    fn tick(&mut self) {
        // compute cpu time
        self.cpu_time = self.previous_frame_time.saturating_sub(self.gpu_time);

        // push log
        self.frame_time_ms_log
            .push(self.previous_frame_time.as_millis() as _);
        self.cpu_time_ms_log.push(self.cpu_time.as_millis() as _);
        self.gpu_time_ms_log.push(self.gpu_time.as_millis() as _);

        // increment counter
        self.total_frame_count += 1;
        self.frame_count += 1;
        self.timer += self.frame_time;

        // reset counter if a sec has passed
        if self.timer > FrameStats::ONE_SEC {
            self.fps_counter = self.frame_count;
            self.frame_count = 0;
            self.timer -= FrameStats::ONE_SEC;
        }
    }

    fn set_frame_time(&mut self, frame_time: Duration) {
        self.previous_frame_time = self.frame_time;
        self.frame_time = frame_time;
    }

    fn set_gpu_time_time(&mut self, gpu_time: Duration) {
        self.gpu_time = gpu_time;
    }
}

#[derive(Debug)]
struct Queue<T>(Vec<T>, usize);

impl<T> Queue<T> {
    fn new(max_size: usize) -> Self {
        Self(Vec::with_capacity(max_size), max_size)
    }

    fn push(&mut self, value: T) {
        if self.0.len() == self.1 {
            self.0.remove(0);
        }
        self.0.push(value);
    }
}

fn build_frametime_plot(ui: &mut egui::Ui, id: impl std::hash::Hash, points: &[f32]) {
    let points: egui_plot::PlotPoints = points
        .iter()
        .enumerate()
        .map(|(i, v)| [i as f64, *v as f64])
        .collect();

    egui_plot::Plot::new(id)
        // .width(width)
        .height(80.0)
        .allow_boxed_zoom(false)
        .allow_double_click_reset(false)
        .allow_drag(false)
        .allow_scroll(false)
        .allow_zoom(false)
        .show_axes([false, true])
        .show(ui, |plot| {
            plot.line(egui_plot::Line::new(points));
        });
}
