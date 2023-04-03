use std::{
	fs,
	collections::HashMap,
	sync::Arc,
	net::TcpStream,
	path::{Path, PathBuf}
};

use parking_lot::RwLock;

use vulkano::{
	device::Device,
	pipeline::PipelineLayout,
	descriptor_set::allocator::StandardDescriptorSetAllocator,
	sampler::{
		Filter,
		Sampler,
		SamplerCreateInfo
	},
	memory::allocator::{FastMemoryAllocator, StandardMemoryAllocator},
	command_buffer::{AutoCommandBufferBuilder, PrimaryAutoCommandBuffer}
};

use winit::event::{
	VirtualKeyCode,
	ButtonId,
	ElementState
};

use image::error::ImageError;

use game::{
	Game,
	ObjectFactory,
	camera::Camera,
	object::{
		resource_uploader::{DescriptorSetUploader, ResourceUploader},
		texture::{RgbaImage, Texture}
	}
};

use game_state::{
	GameState,
	ControlState,
	Control
};

use crate::common::{
	MessagePasser,
	tilemap::TileMap
};

pub use game::object::DrawableEntity;

pub use connections_handler::ConnectionsHandler;
pub use tiles_factory::TilesFactory;

pub mod game_state;
pub mod game;

pub mod connections_handler;
pub mod tiles_factory;

pub mod world_receiver;


pub type BuilderType<'a> = &'a mut AutoCommandBufferBuilder<PrimaryAutoCommandBuffer>;

pub trait GameObject
{
	fn update(&mut self, dt: f32);
	fn regenerate_buffers(&mut self, allocator: &FastMemoryAllocator);
	fn draw(&self, builder: BuilderType);
}

#[derive(Debug)]
pub enum GameInput
{
	KeyboardInput(VirtualKeyCode),
	MouseInput(ButtonId)
}

pub struct ClientInfo
{
	pub address: String,
	pub name: String,
	pub debug_mode: bool
}

pub struct Client
{
	device: Arc<Device>,
	layout: Arc<PipelineLayout>,
	allocator: FastMemoryAllocator,
	game_state: Arc<RwLock<GameState>>,
	game: Game
}

impl Client
{
	pub fn new(
		device: Arc<Device>,
		builder: BuilderType,
		layout: Arc<PipelineLayout>,
		aspect: f32,
		tilemap: TileMap,
		client_info: &ClientInfo
	) -> Result<Self, ImageError>
	{
		let camera = Arc::new(RwLock::new(Camera::new(aspect)));

		let allocator = StandardMemoryAllocator::new_default(device.clone());

		let mut resource_uploader = ResourceUploader{
			allocator,
			builder,
			descriptor: Self::descriptor_set_uploader(&device, layout.clone())
		};

		let textures = Self::recursive_dir(Path::new("textures/")).into_iter().map(|name|
		{
			let image = RgbaImage::load(name.clone()).unwrap();

			let short_path = name.iter().skip(1).fold(PathBuf::new(), |mut acc, part|
			{
				acc.push(part);

				acc
			}).into_os_string().into_string().unwrap();

			(short_path, Arc::new(RwLock::new(Texture::new(&mut resource_uploader, image))))
		}).collect();

		let stream = TcpStream::connect(&client_info.address)?;
		let message_passer = MessagePasser::new(stream);

		let object_factory = ObjectFactory::new(
			device.clone(),
			layout.clone(),
			camera.clone(),
			textures
		);

		let tiles_texture = Arc::new(RwLock::new(tilemap.texture(&mut resource_uploader)?));
		let tiles_factory = ObjectFactory::new(
			device.clone(),
			layout.clone(),
			camera.clone(),
			HashMap::from([(String::new(), tiles_texture)])
		);

		let tiles_factory = TilesFactory::new(tiles_factory, tilemap);

		let game_state = GameState::new(
			camera,
			object_factory,
			tiles_factory,
			message_passer,
			client_info.debug_mode
		);

		let game_state = Arc::new(RwLock::new(game_state));

		let player_id = GameState::connect(game_state.clone(), &client_info.name);

		let game = Game::new(player_id);

		let allocator = FastMemoryAllocator::new_default(device.clone());

		Ok(Self{device, layout, allocator, game_state, game})
	}

	fn recursive_dir(path: &Path) -> impl Iterator<Item=PathBuf>
	{
		let mut collector = Vec::new();

		Self::recursive_dir_inner(path, &mut collector);

		collector.into_iter()
	}

	fn recursive_dir_inner(path: &Path, collector: &mut Vec<PathBuf>)
	{
		fs::read_dir(path).unwrap().flatten().for_each(|entry|
		{
			let path = entry.path();
			if path.is_dir()
			{
				Self::recursive_dir_inner(&path, collector);
			} else
			{
				collector.push(entry.path());
			}
		})
	}

	pub fn swap_pipeline(&mut self, layout: Arc<PipelineLayout>)
	{
		self.layout = layout;

		let descriptor_set_uploader = Self::descriptor_set_uploader(
			&self.device,
			self.layout.clone()
		);

		self.game_state.write().swap_pipeline(&descriptor_set_uploader);
	}

	fn descriptor_set_uploader(
		device: &Arc<Device>,
		layout: Arc<PipelineLayout>
	) -> DescriptorSetUploader
	{
		let allocator = StandardDescriptorSetAllocator::new(device.clone());
		let descriptor_layout = layout.set_layouts().get(0).unwrap().clone();
		let sampler = Sampler::new(
			device.clone(),
			SamplerCreateInfo{
				mag_filter: Filter::Nearest,
				min_filter: Filter::Nearest,
				..Default::default()
			}
		).unwrap();

		DescriptorSetUploader{allocator, layout: descriptor_layout, sampler}
	}

	pub fn resize(&mut self, aspect: f32)
	{
		self.game_state.write().resize(aspect);
	}

	pub fn running(&self) -> bool
	{
		self.game_state.read().running
	}

	fn control(&mut self, button: Control) -> ControlState
	{
		let mut state = self.game_state.write();
		let current = state.controls.get_mut(button as usize).unwrap();

		match current
		{
			ControlState::Clicked =>
			{
				*current = ControlState::Released;

				ControlState::Clicked
			},
			_ => *current
		}
	}

	pub fn send_input(&mut self, input: GameInput, state: ElementState)
	{
		let matched = match input
		{
			GameInput::KeyboardInput(VirtualKeyCode::D) => Some(Control::MoveRight),
			GameInput::KeyboardInput(VirtualKeyCode::A) => Some(Control::MoveLeft),
			GameInput::KeyboardInput(VirtualKeyCode::S) => Some(Control::MoveDown),
			GameInput::KeyboardInput(VirtualKeyCode::W) => Some(Control::MoveUp),
			GameInput::MouseInput(3) => Some(Control::MainAction),
			GameInput::MouseInput(1) => Some(Control::SecondaryAction),
			GameInput::KeyboardInput(VirtualKeyCode::Equals) => Some(Control::ZoomIn),
			GameInput::KeyboardInput(VirtualKeyCode::Minus) => Some(Control::ZoomOut),
			GameInput::KeyboardInput(VirtualKeyCode::Key0) => Some(Control::ZoomReset),
			_ => None
		};

		if let Some(control) = matched
		{
			let previous = self.control(control);

			let new_state = if previous == ControlState::Held && state == ElementState::Released
			{
				ControlState::Clicked
			} else if previous != ControlState::Locked && state == ElementState::Pressed
			{
				ControlState::Held
			} else
			{
				ControlState::Released
			};

			self.game_state.write().controls[control as usize] = new_state;
		}
	}

	pub fn mouse_moved(&mut self, position: (f64, f64))
	{
		self.game_state.write().mouse_position = position.into();
	}

	pub fn update(&mut self, dt: f32)
	{
		{
			let mut writer = self.game_state.write();

			self.game.update(&mut writer, dt);

			writer.update(dt);
			writer.release_clicked();

			if writer.player_connected()
			{
				self.game.on_player_connected(&mut writer);
			}

			if self.game.player_exists(&mut writer)
			{
				self.game.camera_sync(&mut writer);
			}
		}

		self.regenerate_buffers();
	}

	pub fn regenerate_buffers(&mut self)
	{
		self.game_state.write().regenerate_buffers(&self.allocator);
	}

	pub fn draw(&self, builder: BuilderType)
	{
		self.game_state.write().draw(builder);
	}
}