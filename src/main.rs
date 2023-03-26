use std::{
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
    event_loop::{DeviceEventFilter, EventLoop}
};

use argparse::{ArgumentParser, StoreOption, StoreTrue, Store};

use common::TileMap;

use server::Server;

use client::{
    ClientInfo,
    game::object::texture::RgbaImage
};

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

fn main()
{
    let deferred_parse = || TileMap::parse("tiles/tiles.json", "textures/");

    let name = "stephanie #1".to_owned();
    let mut client_info = ClientInfo{address: String::new(), name, debug_mode: false};

    let mut address = None;

    let mut port = None;

    {
        let mut parser = ArgumentParser::new();

        parser.refer(&mut client_info.name)
            .add_option(&["-n", "--name"], Store, "player name");

        parser.refer(&mut address)
            .add_option(&["-a", "--address"], StoreOption, "connection address");

        parser.refer(&mut port)
            .add_option(&["-p", "--port"], StoreOption, "hosting port");

        parser.refer(&mut client_info.debug_mode)
            .add_option(&["-d", "--debug"], StoreTrue, "enable debug mode");

        parser.parse_args_or_exit();
    }

    if let Some(address) = address
    {
        client_info.address = address;
    } else
    {
        let (tx, rx) = mpsc::channel();

        thread::spawn(move ||
        {
            match deferred_parse()
            {
                Ok(tilemap) =>
                {
                    let port = port.unwrap_or(0);
                    let mut server = Server::new(tilemap, &format!("0.0.0.0:{port}"), 16).unwrap();

                    let port = server.port();
                    tx.send(port).unwrap();

                    server.run()
                },
                Err(err) => panic!("error parsing tilemap: {:?}", err)
            }
        });

        let port = rx.recv().unwrap();

        client_info.address = "127.0.0.1".to_owned() + &format!(":{port}");
        println!("listening on port {port}");
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
    let icon = icon_texture.ok().and_then(|texture|
    {
        Icon::from_rgba(texture.data.to_vec(), texture.width, texture.height).ok()
    });

    let event_loop = EventLoop::new();
    event_loop.set_device_event_filter(DeviceEventFilter::Unfocused);

    let surface = WindowBuilder::new()
        .with_title("very cool new game, nobody ever created something like this")
        .with_window_icon(icon)
        .build_vk_surface(&event_loop, instance.clone())
        .unwrap();

    let (physical_device, (device, queues)) = create_device(surface.clone(), instance);

    window::run(
        surface,
        event_loop,
        physical_device,
        device,
        queues.collect(),
        deferred_parse().unwrap(),
        client_info
    );
}
