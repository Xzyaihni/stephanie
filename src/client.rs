use std::{
	fs,
	sync::Arc,
	net::TcpStream,
	collections::HashMap,
	path::{Path, PathBuf}
};

use parking_lot::RwLock;

use vulkano::{
	device::Device,
	buffer::{
		BufferUsage,
		Subbuffer,
		allocator::{SubbufferAllocator, SubbufferAllocatorCreateInfo}
	},
	pipeline::PipelineLayout,
	descriptor_set::allocator::StandardDescriptorSetAllocator,
	sampler::{
		Filter,
		Sampler,
		SamplerCreateInfo
	},
	memory::allocator::StandardMemoryAllocator
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
		ObjectVertex,
		model::Model,
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

use game_object_types::*;

pub use game::object::DrawableEntity;

pub use connections_handler::ConnectionsHandler;
pub use tiles_factory::{TilesFactory, ChunkInfo};

pub mod game_state;
pub mod game;

pub mod connections_handler;
pub mod tiles_factory;

pub mod world_receiver;


pub mod game_object_types
{
	use std::sync::Arc;

	use vulkano::{
		pipeline::PipelineLayout,
		command_buffer::{AutoCommandBufferBuilder, PrimaryAutoCommandBuffer}
	};

	pub type BuilderType<'a> = &'a mut AutoCommandBufferBuilder<PrimaryAutoCommandBuffer>;
	pub type LayoutType = Arc<PipelineLayout>;
}

pub trait GameObject
{
	fn update(&mut self, dt: f32);
	fn update_buffers(&mut self, builder: BuilderType, index: usize);
	fn draw(&self, builder: BuilderType, layout: LayoutType, index: usize);
}

#[derive(Debug, Clone)]
pub struct ObjectAllocator
{
	allocator: Arc<SubbufferAllocator>,
	frames: usize
}

impl ObjectAllocator
{
	pub fn new(device: Arc<Device>, frames: usize) -> Self
	{
		let allocator = StandardMemoryAllocator::new_default(device);
		let allocator = SubbufferAllocator::new(
			Arc::new(allocator),
			SubbufferAllocatorCreateInfo{
				buffer_usage: BufferUsage::VERTEX_BUFFER | BufferUsage::TRANSFER_DST,
				..Default::default()
			}
		);

		let allocator = Arc::new(allocator);

		Self{allocator, frames}
	}

	pub fn subbuffers(&self, model: &Model) -> Box<[Subbuffer<[ObjectVertex]>]>
	{
		(0..self.frames).map(|_|
		{
			self.allocator.allocate_slice(model.vertices.len() as u64).unwrap()
		}).collect::<Vec<_>>().into_boxed_slice()
	}

	pub fn subbuffers_amount(&self) -> usize
	{
		self.frames
	}
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
	game_state: Arc<RwLock<GameState>>,
	game: Game
}

impl Client
{
	pub fn new(
		device: Arc<Device>,
		builder: BuilderType,
		layout: Arc<PipelineLayout>,
		frames: usize,
		aspect: f32,
		tilemap: TileMap,
		client_info: &ClientInfo
	) -> Result<Self, ImageError>
	{
		let camera = Arc::new(RwLock::new(Camera::new(aspect)));

		let allocator = StandardMemoryAllocator::new_default(device.clone());
		let mut resource_uploader = ResourceUploader{
			allocator: &allocator,
			builder,
			descriptor: Self::descriptor_set_uploader(&device, layout.clone())
		};

		let textures = Self::all_textures(&mut resource_uploader, "textures/");

		let object_allocator = ObjectAllocator::new(device.clone(), frames);

		let object_factory = ObjectFactory::new(
			camera.clone(),
			object_allocator.clone(),
			textures
		);

		let tiles_factory = TilesFactory::new(
			camera.clone(),
			object_allocator,
			&mut resource_uploader,
			tilemap
		)?;

		let stream = TcpStream::connect(&client_info.address)?;
		let message_passer = MessagePasser::new(stream);

		let game_state = GameState::new(
			camera,
			object_factory,
			tiles_factory,
			message_passer,
			&client_info
		);

		let game = Game::new(game_state.player_id());
		let game_state = Arc::new(RwLock::new(game_state));

		Ok(Self{device, layout, game_state, game})
	}

	fn all_textures<P: AsRef<Path>>(
		resource_uploader: &mut ResourceUploader,
		folder: P
	) -> HashMap<String, Arc<RwLock<Texture>>>
	{
		Self::recursive_dir(folder.as_ref()).into_iter().map(|name|
		{
			let image = RgbaImage::load(name.clone()).unwrap();

			let short_path = name.iter().skip(1).fold(PathBuf::new(), |mut acc, part|
			{
				acc.push(part);

				acc
			}).into_os_string().into_string().unwrap();

			(short_path, Arc::new(RwLock::new(Texture::new(resource_uploader, image))))
		}).collect()
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
			GameInput::KeyboardInput(VirtualKeyCode::Space) => Some(Control::Jump),
			GameInput::KeyboardInput(VirtualKeyCode::LControl) => Some(Control::Crouch),
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
}

impl GameObject for Client
{
	fn update(&mut self, dt: f32)
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

	fn update_buffers(&mut self, builder: BuilderType, index: usize)
	{
		self.game_state.write().update_buffers(builder, index);
	}

	fn draw(&self, builder: BuilderType, layout: LayoutType, index: usize)
	{
		self.game_state.read().draw(builder, layout, index);
	}
}