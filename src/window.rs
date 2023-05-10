use std::{
    time::Instant,
    sync::Arc
};

use vulkano::{
    format::Format,
    shader::EntryPoint,
    sync::{
        FlushError,
        GpuFuture,
        future::{JoinFuture, FenceSignalFuture}
    },
    pipeline::{
        Pipeline,
        PipelineLayout,
        GraphicsPipeline,
        StateMode,
        graphics::{
            color_blend::ColorBlendState,
            rasterization::{CullMode, RasterizationState},
            input_assembly::InputAssemblyState,
            vertex_input::Vertex,
            viewport::{Viewport, ViewportState}
        }
    },
    image::{
        ImageUsage,
        SwapchainImage,
        view::ImageView
    },
    swapchain::{
        self,
        AcquireError,
        Surface,
        SurfaceCapabilities,
        CompositeAlpha,
        PresentFuture,
        Swapchain,
        SwapchainAcquireFuture,
        SwapchainCreateInfo,
        SwapchainCreationError,
        SwapchainPresentInfo
    },
    device::{
        Device,
        physical::PhysicalDevice,
        Queue
    },
    render_pass::{
        Subpass,
        RenderPass,
        Framebuffer,
        FramebufferCreateInfo
    },
    command_buffer::{
        AutoCommandBufferBuilder,
        PrimaryAutoCommandBuffer,
        CommandBufferExecFuture,
        CommandBufferUsage,
        SubpassContents,
        RenderPassBeginInfo,
        allocator::StandardCommandBufferAllocator
    }
};

use winit::{
    dpi::PhysicalSize,
    window::Window,
    event::{
        Event,
        WindowEvent,
        DeviceEvent,
        KeyboardInput
    },
    event_loop::{ControlFlow, EventLoop}
};

use crate::{
    common::TileMap,
    client::{
        ClientInfo,
        Client,
        GameInput,
        GameObject,
        game::object::ObjectVertex
    }
};

mod default_vertex
{
    vulkano_shaders::shader!
    {
        ty: "vertex",
        path: "shaders/default.vert"
    }
}

mod default_fragment
{
    vulkano_shaders::shader!
    {
        ty: "fragment",
        path: "shaders/default.frag"
    }
}


pub fn framebuffers(
    images: impl Iterator<Item=Arc<SwapchainImage>>,
    render_pass: Arc<RenderPass>
) -> Vec<Arc<Framebuffer>>
{
    images.map(|image|
    {
        let view = ImageView::new_default(image).unwrap();
        Framebuffer::new(
            render_pass.clone(),
            FramebufferCreateInfo{
                attachments: vec![view],
                ..Default::default()
            }
        ).unwrap()
    }).collect::<Vec<_>>()
}

pub fn generate_pipeline(
    vertex_entry: EntryPoint,
    fragment_entry: EntryPoint,
    viewport: Viewport,
    subpass: Subpass,
    device: Arc<Device>
) -> Arc<GraphicsPipeline>
{
    GraphicsPipeline::start()
        .vertex_input_state(ObjectVertex::per_vertex())
        .vertex_shader(vertex_entry, ())
        .input_assembly_state(InputAssemblyState::new())
        .viewport_state(ViewportState::viewport_fixed_scissor_irrelevant([viewport]))
        .fragment_shader(fragment_entry, ())
        .color_blend_state(ColorBlendState::new(subpass.num_color_attachments()).blend_alpha())
        .rasterization_state(RasterizationState{
            cull_mode: StateMode::Fixed(CullMode::Back),
            ..Default::default()
        })
        .render_pass(subpass)
        .build(device)
        .unwrap()
}

pub fn generate_pipelines(
    viewport: Viewport,
    render_pass: Arc<RenderPass>,
    device: Arc<Device>
) -> Vec<Arc<GraphicsPipeline>>
{
    let subpass = Subpass::from(render_pass, 0).unwrap();

    vec![
        generate_pipeline(
            default_vertex::load(device.clone()).unwrap().entry_point("main").unwrap(),
            default_fragment::load(device.clone()).unwrap().entry_point("main").unwrap(),
            viewport,
            subpass,
            device
        )
    ]
}

pub fn default_builder(
    allocator: &StandardCommandBufferAllocator,
    queue_family_index: u32
) -> AutoCommandBufferBuilder<PrimaryAutoCommandBuffer>
{
    AutoCommandBufferBuilder::primary(
        allocator,
        queue_family_index,
        CommandBufferUsage::OneTimeSubmit
    ).unwrap()
}

struct RenderInfo
{
    pub device: Arc<Device>,
    pub swapchain: Arc<Swapchain>,
    pub framebuffers: Vec<Arc<Framebuffer>>,
    pub pipelines: Vec<Arc<GraphicsPipeline>>,
    pub viewport: Viewport,
    pub surface: Arc<Surface>,
    pub render_pass: Arc<RenderPass>
}

impl RenderInfo
{
    pub fn new(
        device: Arc<Device>,
        surface: Arc<Surface>,
        capabilities: SurfaceCapabilities,
        image_format: Format,
        composite_alpha: CompositeAlpha
    ) -> Self
    {
        let dimensions = Self::surface_size(&surface);

        let image_count = capabilities.min_image_count.max(2);
        let min_image_count = match capabilities.max_image_count
        {
            None => image_count,
            Some(max_images) => image_count.min(max_images)
        };

        let (swapchain, images) = Swapchain::new(
            device.clone(),
            surface.clone(),
            SwapchainCreateInfo{
                min_image_count,
                image_format: Some(image_format),
                image_extent: dimensions.into(),
                image_usage: ImageUsage::COLOR_ATTACHMENT,
                composite_alpha,
                ..Default::default()
            }
        ).unwrap();

        let render_pass = vulkano::single_pass_renderpass!(
            device.clone(),
            attachments: {
                color: {
                    load: Clear,
                    store: Store,
                    format: image_format,
                    samples: 1,
                }
            },
            pass: {
                color: [color],
                depth_stencil: {}
            }
        ).unwrap();

        let framebuffers = framebuffers(images.into_iter(), render_pass.clone());

        let viewport = Viewport{
            origin: [0.0, 0.0],
            dimensions: dimensions.into(),
            depth_range: 0.0..1.0
        };


        let pipelines = generate_pipelines(viewport.clone(), render_pass.clone(), device.clone());

        Self{device, swapchain, framebuffers, pipelines, viewport, surface, render_pass}
    }

    pub fn recreate(
        &mut self,
        surface: Arc<Surface>,
        redraw_window: bool
    ) -> Result<(), SwapchainCreationError>
    {
        let dimensions = Self::surface_size(&surface);

        let (new_swapchain, new_images) = self.swapchain.recreate(SwapchainCreateInfo{
            image_extent: dimensions.into(),
            ..self.swapchain.create_info()
        })?;

        self.swapchain = new_swapchain;
        self.framebuffers = framebuffers(new_images.into_iter(), self.render_pass.clone());

        if redraw_window
        {
            self.viewport.dimensions = dimensions.into();

            self.pipelines = generate_pipelines(
                self.viewport.clone(),
                self.render_pass.clone(),
                self.device.clone()
            );
        }

        Ok(())
    }

    pub fn aspect(&self) -> f32
    {
        let size: [f32; 2] = Self::surface_size(&self.surface).into();

        size[0] / size[1]
    }

    pub fn surface_size(surface: &Arc<Surface>) -> PhysicalSize<u32>
    {
        let window = surface.object().unwrap().downcast_ref::<Window>().unwrap();

        window.inner_size()
    }
}

pub fn run(
    surface: Arc<Surface>,
    event_loop: EventLoop<()>,
    physical_device: Arc<PhysicalDevice>,
    device: Arc<Device>,
    queues: Vec<Arc<Queue>>,
    tilemap: TileMap,
    client_info: ClientInfo
)
{
    let capabilities = physical_device.surface_capabilities(&surface, Default::default())
        .unwrap();

    let composite_alpha =
    {
        let supported = capabilities.supported_composite_alpha;

        let preferred = CompositeAlpha::Opaque;
        let supports_preferred = supported.contains_enum(preferred);

        if supports_preferred
        {
            preferred
        } else
        {
            supported.into_iter().next().unwrap()
        }
    };

    let image_format = physical_device.surface_formats(&surface, Default::default())
        .unwrap()[0].0;

    let mut render_info = RenderInfo::new(
        device.clone(), surface.clone(),
        capabilities, image_format, composite_alpha
    );

    let command_allocator =
        StandardCommandBufferAllocator::new(device.clone(), Default::default());

    let queue = queues[0].clone();

    let fences_amount = render_info.framebuffers.len();
    let mut fences = vec![None; fences_amount].into_boxed_slice();
    let mut previous_frame_index = 0;

    let layout = render_info.pipelines[0].layout().clone();
    let mut client: Option<Client> = None;

    let mut tilemap = Some(tilemap);

    let (mut width, mut height): (f64, f64) = RenderInfo::surface_size(&surface).into();

    let mut previous_time = Instant::now();

    let mut recreate_swapchain = false;
    let mut window_resized = false;

    event_loop.run(move |event, _, control_flow|
    {
        match event
        {
            Event::WindowEvent{
                event: WindowEvent::CloseRequested,
                ..
            } =>
            {
                *control_flow = ControlFlow::Exit;
            },
            Event::WindowEvent{
                event: WindowEvent::Resized(_),
                ..
            } =>
            {
                window_resized = true;
            },
            Event::WindowEvent{
                event: WindowEvent::CursorMoved{position, ..},
                   ..
            } =>
            {
                if let Some(ref mut client) = client
                {
                    let position = (position.x / width, position.y / height);

                    client.mouse_moved(position);
                }
            },
            Event::DeviceEvent{
                event: DeviceEvent::Button{
                    button,
                    state
                },
                ..
            } =>
            {
                if let Some(ref mut client) = client
                {
                    client.send_input(GameInput::MouseInput(button), state);
                }
            },
            Event::DeviceEvent{
                event: DeviceEvent::Key(input),
                ..
            } =>
            {
                if let Some(ref mut client) = client
                {
                    let KeyboardInput{virtual_keycode: button, state, ..} = input;

                    if let Some(button) = button
                    {
                        client.send_input(GameInput::KeyboardInput(button), state);
                    }
                }
            },
            Event::MainEventsCleared =>
            {
                let mut builder = default_builder(&command_allocator, queue.queue_family_index());

                if client.is_none()
                {
                    match Client::new(
                        device.clone(),
                        &mut builder,
                        layout.clone(),
                        fences_amount,
                        render_info.aspect(),
                        tilemap.take().unwrap(),
                        &client_info
                    )
                    {
                        Ok(x) =>
                        {
                            client = Some(x);
                        },
                        Err(err) =>
                        {
                            eprintln!("client error: {err:?}");
                            *control_flow = ControlFlow::Exit;
                        }
                    }
                }

                let client = client.as_mut().unwrap();
                if client.running()
                {
                    let acquired =
                        match swapchain::acquire_next_image(render_info.swapchain.clone(), None)
                        {
                            Ok(x) => Some(x),
                            Err(AcquireError::OutOfDate) =>
                            {
                                None
                            },
                            Err(e) => panic!("error getting next image >-< ({:?})", e)
                        };

                    if let Some((image_index, suboptimal, acquire_future)) = acquired
                    {
                        let image_index = image_index as usize;

                        let command_buffer = run_frame(
                            builder,
                            layout.clone(),
                            &mut render_info,
                            image_index,
                            client,
                            &mut previous_time
                        );

                        recreate_swapchain |= suboptimal;
                        recreate_swapchain |= execute_builder(
                            device.clone(),
                            queue.clone(),
                            render_info.swapchain.clone(),
                            &mut fences,
                            FrameData{
                                command_buffer,
                                image_index,
                                previous_frame_index,
                                acquire_future
                            }
                        );

                        previous_frame_index = image_index;
                    }
                } else
                {
                    eprintln!("server closed, exiting");
                    *control_flow = ControlFlow::Exit;
                }
            },
            Event::RedrawEventsCleared =>
            {
                if recreate_swapchain || window_resized
                {
                    recreate_swapchain = false;

                    match render_info.recreate(surface.clone(), window_resized)
                    {
                        Ok(_) => (),
                        Err(SwapchainCreationError::ImageExtentNotSupported{..}) => return,
                        Err(e) => panic!("couldnt recreate swapchain ; -; ({:?})", e)
                    }

                    if let Some(ref mut client) = client
                    {
                        client.swap_pipeline(render_info.pipelines[0].layout().clone());
                        if window_resized
                        {
                            (width, height) = RenderInfo::surface_size(&surface).into();

                            client.resize(render_info.aspect());
                        }
                    }

                    window_resized = false;
                }
            },
            _ => ()
        }
    });
}

type FutureInner = PresentFuture<CommandBufferExecFuture<JoinFuture<Box<dyn GpuFuture>, SwapchainAcquireFuture>>>;
type FutureType = Option<Arc<FenceSignalFuture<FutureInner>>>;

struct FrameData
{
    command_buffer: PrimaryAutoCommandBuffer,
    image_index: usize,
    previous_frame_index: usize,
    acquire_future: SwapchainAcquireFuture
}

fn run_frame(
    mut builder: AutoCommandBufferBuilder<PrimaryAutoCommandBuffer>,
    layout: Arc<PipelineLayout>,
    render_info: &mut RenderInfo,
    image_index: usize,
    client: &mut Client,
    previous_time: &mut Instant
) -> PrimaryAutoCommandBuffer
{
    client.update_buffers(&mut builder, image_index);

    builder.begin_render_pass(
        RenderPassBeginInfo{
            clear_values: vec![Some([0.0, 0.0, 0.0, 1.0].into())],
            ..RenderPassBeginInfo::framebuffer(
                render_info.framebuffers[image_index].clone()
            )
        },
        SubpassContents::Inline
    ).unwrap().bind_pipeline_graphics(render_info.pipelines[0].clone());

    let delta_time = previous_time.elapsed().as_secs_f32();
    *previous_time = Instant::now();

    client.update(delta_time);
    client.draw(&mut builder, layout, image_index);

    builder.end_render_pass().unwrap();

    builder.build().unwrap()
}

fn execute_builder(
    device: Arc<Device>,
    queue: Arc<Queue>,
    swapchain: Arc<Swapchain>,
    fences: &mut [FutureType],
    frame_data: FrameData
) -> bool
{
    let FrameData{
        command_buffer,
        image_index,
        previous_frame_index,
        acquire_future
    } = frame_data;

    if let Some(fence) = &fences[image_index]
    {
        fence.wait(None).unwrap();
    }

    let previous_fence = match fences[previous_frame_index].clone()
    {
        Some(fence) => fence.boxed(),
        None =>
        {
            let mut now = vulkano::sync::now(device);
            now.cleanup_finished();

            now.boxed()
        }
    };

    let fence = previous_fence
        .join(acquire_future)
        .then_execute(queue.clone(), command_buffer)
        .unwrap()
        .then_swapchain_present(
            queue,
            SwapchainPresentInfo::swapchain_image_index(
                swapchain,
                image_index as u32
            )
        ).then_signal_fence_and_flush();

    let mut recreate_swapchain = false;
    fences[image_index] = match fence
    {
        Ok(fence) => Some(Arc::new(fence)),
        Err(FlushError::OutOfDate) =>
        {
            recreate_swapchain = true;
            None
        },
        Err(e) =>
        {
            eprintln!("error flushing future ;; ({:?})", e);
            None
        }
    };

    recreate_swapchain
}