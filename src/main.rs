use std::{
    env,
    process,
    thread,
    sync::{mpsc, Arc}
};

use vulkano::{
    VulkanLibrary,
    swapchain::Surface,
    device::{
        Device,
        DeviceCreateInfo,
        DeviceExtensions,
        Queue,
        QueueCreateInfo,
        physical::{
            PhysicalDevice,
            PhysicalDeviceType
        }
    },
    instance::{Instance, InstanceCreateInfo}
};

use vulkano_win::VkSurfaceBuild;

use winit::{
    window::{Icon, WindowBuilder},
    event_loop::EventLoop
};

use server::Server;

use client::game::object::texture::RgbaImage;

mod common;

mod server;
mod client;

mod window;


fn get_physical(
    surface: Arc<Surface>,
    instance: Arc<Instance>,
    device_extensions: &DeviceExtensions
) -> (Arc<PhysicalDevice>, u32)
{
    instance.enumerate_physical_devices()
        .expect("cant enumerate devices,,,,")
        .filter(|device| device.supported_extensions().contains(device_extensions))
        .filter_map(|device|
        {
            device.queue_family_properties()
                .iter()
                .enumerate()
                .position(|(index, queue)|
                {
                    queue.queue_flags.graphics
                        && device.surface_support(index as u32, &surface).unwrap_or(false)
                })
                .map(|index| (device, index as u32))
        }).min_by_key(|(device, _)|
        {
            match device.properties().device_type
            {
                PhysicalDeviceType::DiscreteGpu => 0,
                PhysicalDeviceType::IntegratedGpu => 1,
                PhysicalDeviceType::VirtualGpu => 2,
                PhysicalDeviceType::Cpu => 3,
                _ => 4
            }
        }).expect("nyo rendering device...")
}

fn create_device(
    surface: Arc<Surface>,
    instance: Arc<Instance>
) -> (Arc<PhysicalDevice>, (Arc<Device>, impl Iterator<Item=Arc<Queue>> + ExactSizeIterator))
{
    let device_extensions = DeviceExtensions{
        khr_swapchain: true,
        ..DeviceExtensions::empty()
    };

    let (physical_device, queue_family_index) =
        get_physical(surface, instance, &device_extensions);

    (physical_device.clone(), Device::new(
        physical_device,
        DeviceCreateInfo{
            queue_create_infos: vec![QueueCreateInfo{
                queue_family_index,
                ..Default::default()
            }],
            enabled_extensions: device_extensions,
            ..Default::default()
        }).expect("couldnt create device...."))
}

fn error_and_quit(message: &str) -> !
{
    eprintln!("{message}\n");

    eprintln!("usage: {} [player_name] [mode] [address]", env::args().next().unwrap());
    eprintln!("modes:");
    eprintln!("    host (default), connect");

    process::exit(1)
}

fn main()
{
    let mut args = env::args().skip(1);
    let name = args.next().unwrap_or_else(|| "test".to_owned());
    let mode = args.next().unwrap_or_else(|| "host".to_owned());

    let address;
    match mode.to_lowercase().as_str()
    {
        "host" =>
        {
            let (tx, rx) = mpsc::channel();

            thread::spawn(move ||
            {
                let mut server = Server::new("0.0.0.0", 16).unwrap();

                let port = server.port();
                tx.send(port).unwrap();

                server.run()
            });

            let port = rx.recv().unwrap();

            address = "127.0.0.1".to_owned() + &format!(":{port}");
            println!("listening on port {port}");
        },
        "connect" =>
        {
            address = args.next().unwrap_or_else(|| error_and_quit("no connect address provided"));
        },
        _ => error_and_quit(&format!("unknown mode: {mode}"))
    }

    let library = VulkanLibrary::new().expect("nyo vulkan? ;-;");

    let enabled_extensions = vulkano_win::required_extensions(&library);
    let instance = Instance::new(
        library,
        InstanceCreateInfo{
            enabled_extensions,
            ..Default::default()
        }
    ).expect("cant create vulkan instance..");

    let icon_texture = RgbaImage::load("icon.png");
    let icon = icon_texture.ok().map(|texture|
    {
        Icon::from_rgba(texture.data.to_vec(), texture.width, texture.height).ok()
    }).flatten();

    let event_loop = EventLoop::new();
    let surface = WindowBuilder::new()
        .with_title("very cool new game, nobody ever created something like this")
        .with_window_icon(icon)
        .build_vk_surface(&event_loop, instance.clone())
        .unwrap();

    let (physical_device, (device, queues)) = create_device(surface.clone(), instance.clone());

    window::run(surface, event_loop, physical_device, device, queues.collect(), address, name);
}
