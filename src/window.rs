use std::{
    time::Instant,
    sync::Arc
};

use vulkano::{
    format::Format,
    shader::EntryPoint,
    sync::{
        FlushError,
        GpuFuture
    },
    pipeline::{
        Pipeline,
        GraphicsPipeline,
        StateMode,
        graphics::{
            color_blend::ColorBlendState,
            depth_stencil::DepthStencilState,
            rasterization::{CullMode, RasterizationState},
            input_assembly::InputAssemblyState,
            vertex_input::BuffersDefinition,
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
        Swapchain,
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

use crate::client::{
    Client,
    GameInput,
    game
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
        .vertex_input_state(BuffersDefinition::new().vertex::<game::object::Vertex>())
        .vertex_shader(vertex_entry, ())
        .input_assembly_state(InputAssemblyState::new())
        .viewport_state(ViewportState::viewport_fixed_scissor_irrelevant([viewport]))
        .fragment_shader(fragment_entry, ())
        .color_blend_state(ColorBlendState::new(subpass.num_color_attachments()).blend_alpha())
        .depth_stencil_state(DepthStencilState::simple_depth_test())
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
    let subpass = Subpass::from(render_pass.clone(), 0).unwrap();

    vec![
        generate_pipeline(
            default_vertex::load(device.clone()).unwrap().entry_point("main").unwrap(),
            default_fragment::load(device.clone()).unwrap().entry_point("main").unwrap(),
            viewport,
            subpass.clone(),
            device.clone()
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

        let (swapchain, images) = Swapchain::new(
            device.clone(),
            surface.clone(),
            SwapchainCreateInfo{
                min_image_count: capabilities.min_image_count,
                image_format: Some(image_format),
                image_extent: dimensions.into(),
                image_usage: ImageUsage{
                    color_attachment: true,
                    ..Default::default()
                },
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

    fn surface_size(surface: &Arc<Surface>) -> PhysicalSize<u32>
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
    address: String,
    name: String
)
{
    let capabilities = physical_device.surface_capabilities(&surface, Default::default())
        .unwrap();

    let composite_alpha = capabilities.supported_composite_alpha.iter().next().unwrap();
    let image_format = physical_device.surface_formats(&surface, Default::default())
        .unwrap()[0].0;

    let mut render_info = RenderInfo::new(
        device.clone(), surface.clone(),
        capabilities, image_format, composite_alpha
    );

    let command_allocator =
        StandardCommandBufferAllocator::new(device.clone(), Default::default());

    let queue = queues[0].clone();

    let layout = render_info.pipelines[0].layout().clone();
    let mut client: Option<Client> = None;

    let mut previous_time = Instant::now();

    let mut recreate_swapchain = false;
    let mut window_resized = false;

    let mut focused = false;

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
                event: WindowEvent::Focused(state),
                   ..
            } =>
            {
                focused = state;
            },
            Event::DeviceEvent{
                event: DeviceEvent::Button{
                    button,
                    state
                },
                ..
            } =>
            {
                if focused
                {
                    if let Some(ref mut client) = client
                    {
                        client.send_input(GameInput::MouseInput(button), state);
                    }
                }
            },
            Event::DeviceEvent{
                event: DeviceEvent::Key(input),
                ..
            } =>
            {
                if focused
                {
                    if let Some(ref mut client) = client
                    {
                        let KeyboardInput{virtual_keycode: button, state, ..} = input;

                        if let Some(button) = button
                        {
                            client.send_input(GameInput::KeyboardInput(button), state);
                        }
                    }
                }
            },
            Event::MainEventsCleared =>
            {
                let (image_index, suboptimal, acquire_future) =
                    match swapchain::acquire_next_image(render_info.swapchain.clone(), None)
                    {
                        Ok(x) => x,
                        Err(AcquireError::OutOfDate) =>
                        {
                            recreate_swapchain = true;
                            return;
                        },
                        Err(e) => panic!("error getting next image >-< ({:?})", e)
                    };

                if suboptimal {recreate_swapchain = true}

                let mut builder = default_builder(&command_allocator, queue.queue_family_index());

                if client.is_none()
                {
                    client = Some(
                        Client::new(
                            device.clone(),
                            &mut builder,
                            layout.clone(),
                            render_info.aspect(),
                            &address, &name
                        ).unwrap()
                    );
                }

                builder.begin_render_pass(
                    RenderPassBeginInfo{
                        clear_values: vec![Some([0.0, 0.0, 0.0, 1.0].into())],
                        ..RenderPassBeginInfo::framebuffer(
                            render_info.framebuffers[image_index as usize].clone()
                        )
                    },
                    SubpassContents::Inline
                ).unwrap().bind_pipeline_graphics(render_info.pipelines[0].clone());

                let delta_time = previous_time.elapsed().as_secs_f32();
                previous_time = Instant::now();

                client.as_mut().unwrap().update(delta_time);

                client.as_ref().unwrap().draw(&mut builder);

                builder.end_render_pass().unwrap();

                let command_buffer = builder.build().unwrap();

                let execution = vulkano::sync::now(device.clone())
                    .join(acquire_future)
                    .then_execute(queue.clone(), command_buffer)
                    .unwrap()
                    .then_swapchain_present(
                        queue.clone(),
                        SwapchainPresentInfo::swapchain_image_index(
                            render_info.swapchain.clone(),
                            image_index
                        )
                    )
                    .then_signal_fence_and_flush();

                match execution
                {
                    Ok(future) => future.wait(None).unwrap(),
                    Err(FlushError::OutOfDate) =>
                    {
                        recreate_swapchain = true;
                    },
                    Err(e) => eprintln!("error flushing future ;; ({:?})", e)
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