use std::{
	sync::Arc,
	net::TcpStream
};

use parking_lot::RwLock;

use vulkano::{
	device::Device,
	buffer::{
		BufferUsage,
		Subbuffer,
		allocator::{SubbufferAllocator, SubbufferAllocatorCreateInfo}
	},
	memory::allocator::StandardMemoryAllocator
};

use winit::event::{
	VirtualKeyCode,
	ButtonId
};

use image::error::ImageError;

use yanyaengine::{
    Control,
	camera::Camera,
	object::{
		ObjectVertex,
		model::Model
	},
    game_object::*
};

use game::Game;

use game_state::GameState;

use crate::common::{
	MessagePasser,
	tilemap::TileMap
};

pub use game::DrawableEntity;

pub use connections_handler::ConnectionsHandler;
pub use tiles_factory::{TilesFactory, ChunkInfo};

pub mod game_state;
pub mod game;

pub mod connections_handler;
pub mod tiles_factory;

pub mod world_receiver;


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

pub struct ClientInitInfo
{
    pub client_info: ClientInfo,
    pub tilemap: TileMap
}

pub struct ClientInfo
{
	pub address: String,
	pub name: String,
	pub debug_mode: bool
}

pub struct Client
{
	game_state: Arc<RwLock<GameState>>,
	game: Game
}

impl Client
{
	pub fn new(
        info: InitPartialInfo,
        client_init_info: ClientInitInfo
	) -> Result<Self, ImageError>
	{
		let camera = Camera::new(info.aspect);
        let mut info = InitInfo::new(info, &camera);

        let camera = Arc::new(RwLock::new(camera));

		let tiles_factory = TilesFactory::new(&mut info, client_init_info.tilemap)?;

		let stream = TcpStream::connect(&client_init_info.client_info.address)?;
		let message_passer = MessagePasser::new(stream);

		let game_state = GameState::new(
			camera,
            info.object_info.partial.assets,
			info.object_info.partial.object_factory,
			tiles_factory,
			message_passer,
		    &client_init_info.client_info
		);

		let game = Game::new(game_state.player_id());
		let game_state = Arc::new(RwLock::new(game_state));

		Ok(Self{game_state, game})
	}

	pub fn resize(&mut self, aspect: f32)
	{
		self.game_state.write().resize(aspect);
	}

	pub fn running(&self) -> bool
	{
		self.game_state.read().running
	}

    pub fn update(&mut self, dt: f32)
    {
		let mut writer = self.game_state.write();

		self.game.update(&mut writer, dt);

		writer.update(dt);

		if writer.player_connected()
		{
			self.game.on_player_connected(&mut writer);
		}

		if self.game.player_exists(&mut writer)
		{
			self.game.camera_sync(&mut writer);
		}
    }

    pub fn update_buffers(&mut self, partial_info: UpdateBuffersPartialInfo)
    {
	    self.game_state.write().update_buffers(partial_info);
    }

    pub fn draw(&mut self, mut info: DrawInfo)
    {
	    self.game_state.read().draw(&mut info);
    }

	pub fn input(&mut self, control: Control)
	{
        self.game_state.write().input(control);
	}

	pub fn mouse_move(&mut self, position: (f64, f64))
	{
		self.game_state.write().mouse_position = position.into();
	}
}
