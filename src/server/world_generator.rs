use std::{
	io,
	fs::{self, File},
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
			LocalPos,
			tile::Tile
		}
	}
};


#[derive(Debug)]
pub struct WorldGenerator
{
	message_handler: Arc<RwLock<ConnectionsHandler>>,
	tilemap: TileMap,
	world_name: String
}

impl WorldGenerator
{
	pub fn new(message_handler: Arc<RwLock<ConnectionsHandler>>, tilemap: TileMap) -> Self
	{
		let world_name = "default".to_owned();
		let this = Self{message_handler, tilemap, world_name};

		fs::create_dir_all(this.world_path()).unwrap();

		this
	}

	pub fn send_chunk(&mut self, pos: GlobalPos)
	{
		let chunk = self.load_chunk(pos);

		self.message_handler.write().send_message(Message::ChunkSync{pos, chunk});
	}

	fn load_chunk(&mut self, pos: GlobalPos) -> Chunk
	{
		let loaded_chunk = self.load_chunk_from_save(pos);

		match loaded_chunk
		{
			Some(x) => x,
			None =>
			{
				let chunk = self.generate_chunk(pos);
				self.save_chunk(pos, &chunk);

				chunk
			}
		}
	}

	fn generate_chunk(&mut self, pos: GlobalPos) -> Chunk
	{
		let mut chunk = Chunk::new();

		for y in 0..CHUNK_SIZE
		{
			for x in 0..CHUNK_SIZE
			{
				let tile_index = fastrand::usize(..self.tilemap.len());

				chunk.set_tile(LocalPos::new(x, y, 0), Tile::new(tile_index));
			}
		}

		chunk
	}

	fn load_chunk_from_save(&self, pos: GlobalPos) -> Option<Chunk>
	{
		match File::open(self.chunk_path(pos))
		{
			Ok(file) =>
			{
				Some(bincode::deserialize_from(file).unwrap())
			},
			Err(ref err) if err.kind() == io::ErrorKind::NotFound =>
			{
				None
			},
			Err(err) => panic!("error loading chunk from file: {:?}", err)
		}
	}

	fn save_chunk(&self, pos: GlobalPos, chunk: &Chunk)
	{
		let file = File::create(self.chunk_path(pos)).unwrap();

		bincode::serialize_into(file, chunk).unwrap();
	}

	fn chunk_path(&self, pos: GlobalPos) -> String
	{
		let chunks_directory = self.world_path();
		format!("{chunks_directory}/chunk {} {} {}", pos.0.x, pos.0.y, pos.0.z)
	}

	fn world_path(&self) -> String
	{
		format!("worlds/{}/chunks", self.world_name)
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