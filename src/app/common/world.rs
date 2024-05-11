use yanyaengine::game_object::*;

use crate::{
	client::{
		TilesFactory,
		world_receiver::WorldReceiver
	},
	common::{
        Entity,
        message::Message
    }
};

pub use overmap::chunk::{
	self,
	CHUNK_SIZE,
	CHUNK_VISUAL_SIZE,
	TILE_SIZE,
	Pos3,
	Chunk,
	ChunkLocal,
	GlobalPos,
	LocalPos,
	PosDirection,
	DirectionsGroup,
	MaybeGroup,
	AlwaysGroup,
	tile::Tile
};

pub use client_overmap::TilePos;

use client_overmap::ClientOvermap;
use visual_overmap::VisualOvermap;

pub mod overmap;

mod client_overmap;
mod visual_overmap;


pub const CLIENT_OVERMAP_SIZE: usize = 5;
pub const CLIENT_OVERMAP_SIZE_Z: usize = 2;

#[derive(Debug, Clone)]
pub struct ChunkWithEntities
{
    pub chunk: Chunk,
    pub entities: Vec<Entity>
}

#[derive(Debug)]
pub struct World
{
	overmap: ClientOvermap
}

impl World
{
	pub fn new(
		world_receiver: WorldReceiver,
		tiles_factory: TilesFactory,
		camera_size: (f32, f32),
		player_position: Pos3<f32>
	) -> Self
	{
		let size = Self::overmap_size();

		let visual_overmap = VisualOvermap::new(tiles_factory, size, camera_size, player_position);
		let overmap = ClientOvermap::new(
			world_receiver,
			visual_overmap,
			size,
			player_position
		);

		Self{overmap}
	}

    pub fn overmap_size() -> Pos3<usize>
    {
        Pos3::new(CLIENT_OVERMAP_SIZE, CLIENT_OVERMAP_SIZE, CLIENT_OVERMAP_SIZE_Z)
    }

	pub fn zoom_limits() -> (f32, f32)
	{
		//make the camera smaller by 3 tiles so theres time for the missing chunks to load
		let padding = 3;

		let padding = TILE_SIZE * padding as f32;

		let max_scale = (CLIENT_OVERMAP_SIZE - 1) as f32 * CHUNK_VISUAL_SIZE - padding;
		let min_scale = 0.2;

		(min_scale, max_scale)
	}

    pub fn tile(&self, index: TilePos) -> Option<&Tile>
    {
        self.overmap.tile(index)
    }

    pub fn player_tile(&self) -> TilePos
    {
        self.overmap.player_tile()
    }

	pub fn update(&mut self, dt: f32)
	{
		self.overmap.update(dt);
	}

	pub fn rescale(&mut self, size: (f32, f32))
	{
		self.overmap.rescale(size);
	}

	pub fn camera_moved(&mut self, pos: Pos3<f32>)
	{
		self.overmap.camera_moved(pos);
	}

	pub fn handle_message(&mut self, message: Message) -> Option<Message>
	{
		match message
		{
			Message::ChunkSync{pos, chunk} =>
			{
				self.overmap.set(pos, chunk);
				None
			},
			_ => Some(message)
		}
	}
}

impl GameObject for World
{
	fn update_buffers(&mut self, info: &mut UpdateBuffersInfo)
    {
		self.overmap.update_buffers(info);
    }

	fn draw(&self, info: &mut DrawInfo)
    {
		self.overmap.draw(info);
    }
}
