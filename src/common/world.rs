use crate::{
	client::{
		GameObject,
		game_object_types::*,
		TilesFactory,
		world_receiver::WorldReceiver
	},
	common::message::Message
};

pub use overmap::chunk::{
	self,
	CHUNK_SIZE,
	CHUNK_VISUAL_SIZE,
	TILE_SIZE,
	Pos3,
	Chunk,
	GlobalPos,
	LocalPos,
	PosDirection,
	InclusiveGroup,
	tile::Tile
};

use overmap::Overmap;

use client_overmap::ClientOvermap;
use visual_overmap::VisualOvermap;

pub mod overmap;

mod client_overmap;
mod visual_overmap;


pub const CLIENT_OVERMAP_SIZE: usize = 5;

#[derive(Debug)]
pub struct World
{
	overmap: ClientOvermap<CLIENT_OVERMAP_SIZE>
}

impl World
{
	pub fn new(
		world_receiver: WorldReceiver,
		tiles_factory: TilesFactory,
		size: (f32, f32),
		player_position: Pos3<f32>
	) -> Self
	{
		let visual_overmap = VisualOvermap::new(tiles_factory, size, player_position);
		let overmap = ClientOvermap::new(world_receiver, visual_overmap, player_position);

		Self{overmap}
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

	pub fn rescale(&mut self, size: (f32, f32))
	{
		self.overmap.rescale(size);
	}

	pub fn player_moved(&mut self, pos: Pos3<f32>)
	{
		self.overmap.player_moved(pos);
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
	fn update(&mut self, dt: f32)
	{
		self.overmap.update(dt);
	}

	fn draw(&self, allocator: AllocatorType, builder: BuilderType, layout: LayoutType)
	{
		self.overmap.draw(allocator, builder, layout);
	}
}