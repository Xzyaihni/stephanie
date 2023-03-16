use std::{
	io,
	sync::Arc,
	net::TcpStream
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
	memory::allocator::StandardMemoryAllocator,
	command_buffer::{AutoCommandBufferBuilder, PrimaryAutoCommandBuffer}
};

use winit::event::{
	VirtualKeyCode,
	ButtonId,
	ElementState
};

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

use crate::common::MessagePasser;

pub mod game_state;
pub mod game;


#[derive(Debug)]
pub enum GameInput
{
	KeyboardInput(VirtualKeyCode),
	MouseInput(ButtonId)
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
		builder: &mut AutoCommandBufferBuilder<PrimaryAutoCommandBuffer>,
		layout: Arc<PipelineLayout>,
		aspect: f32,
		address: &str,
		name: &str
	) -> io::Result<Self>
	{
		let camera = Arc::new(RwLock::new(Camera::new(aspect)));

		let allocator = StandardMemoryAllocator::new_default(device.clone());

		let mut resource_uploader = ResourceUploader{
			allocator,
			builder,
			descriptor: Self::descriptor_set_uploader(&device, layout.clone())
		};

		let textures_list = vec!["textures/cracked_stone.png", "textures/asphalt.png", "icon.png"];
		let textures = textures_list.into_iter().map(|name|
		{
			Arc::new(
				RwLock::new(
					Texture::new(&mut resource_uploader, RgbaImage::load(name).unwrap())
				)
			)
		}).collect();

		let stream = TcpStream::connect(address)?;
		let message_passer = MessagePasser::new(stream);

		let object_factory = ObjectFactory::new(
			device.clone(),
			layout.clone(),
			camera.clone(),
			textures
		);

		let game_state = Arc::new(
			RwLock::new(
				GameState::new(camera.clone(), object_factory, message_passer)
			)
		);

		let player_id = GameState::connect(game_state.clone(), name);

		let game = Game::new(game_state.clone(), player_id);

		Ok(Self{device, layout, game_state, game})
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
		self.game_state.write().camera.write().resize(aspect);
	}

	pub fn update(&mut self, dt: f32)
	{
		self.game.update(dt);

		let connected = {
			let mut writer = self.game_state.write();

			writer.entities.update(dt);

			writer.entities.regenerate_buffers();
			writer.release_clicked();

			writer.player_connected()
		};

		if connected
		{
			self.game.player_connected();
		}
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
			_ => None
		};

		dbg!(&matched);

		if let Some(control) = matched
		{
			let previous = self.control(control);

			let new_state = if previous == ControlState::Held && state == ElementState::Released
			{
				ControlState::Clicked
			} else
			{
				if state == ElementState::Pressed
				{
					ControlState::Held
				} else
				{
					ControlState::Released
				}
			};

			self.game_state.write().controls[control as usize] = new_state;
		}
	}

	pub fn draw(&self, builder: &mut AutoCommandBufferBuilder<PrimaryAutoCommandBuffer>)
	{
		self.game_state.write().entities.draw(builder);
	}
}