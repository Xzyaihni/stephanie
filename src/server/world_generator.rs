use std::{
	sync::Arc
};

use parking_lot::RwLock;

use crate::{
	server::ConnectionsHandler,
	common::{
		TileMap,
		EntityPasser,
		message::Message,
		world::chunk::{
			CHUNK_SIZE,
			Chunk,
			GlobalPos,
			LocalPos
		}
	}
};


#[derive(Debug)]
pub struct WorldGenerator
{
	message_handler: Arc<RwLock<ConnectionsHandler>>,
	tilemap: TileMap
}

impl WorldGenerator
{
	pub fn new(message_handler: Arc<RwLock<ConnectionsHandler>>, tilemap: TileMap) -> Self
	{
		Self{message_handler, tilemap}
	}

	pub fn send_chunk(&mut self, pos: GlobalPos)
	{
		let chunk = self.load_chunk(pos).unwrap_or_else(|| self.generate_chunk(pos));

		self.message_handler.write().send_message(Message::ChunkSync{pos, chunk});
	}

	fn load_chunk(&mut self, pos: GlobalPos) -> Option<Chunk>
	{
		eprintln!("no chunk loading for now!!");
		None
	}

	fn generate_chunk(&mut self, pos: GlobalPos) -> Chunk
	{
		let mut chunk = Chunk::new();

		for y in 0..CHUNK_SIZE
		{
			for x in 0..CHUNK_SIZE
			{
				let is_stone = fastrand::bool();

				let tile = self.tilemap.tile_named(if is_stone
				{
					"stone"
				} else
				{
					"asphalt"
				}).unwrap();

				chunk.set_tile(LocalPos::new(x, y, 0), tile);
			}
		}

		chunk
	}

	pub fn handle_message(&mut self, message: Message) -> Option<Message>
	{
		match message
		{
			Message::ChunkRequest{pos} =>
			{
				self.send_chunk(pos);
				None
			},
			_ => Some(message)
		}
	}
}