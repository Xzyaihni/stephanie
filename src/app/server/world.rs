use std::{
    path::PathBuf,
    sync::Arc
};

use parking_lot::{Mutex, RwLock};

use slab::Slab;

use crate::{
	server::ConnectionsHandler,
	common::{
		TileMap,
        WorldChunkSaver,
        ChunkSaver,
        SaveLoad,
		EntityPasser,
		message::Message,
		world::{
			CLIENT_OVERMAP_SIZE,
			CLIENT_OVERMAP_SIZE_Z,
			Chunk,
			GlobalPos,
			Pos3
		}
	}
};

use world_generator::WorldGenerator;

use server_overmap::ServerOvermap;

pub use world_generator::ParseError;

pub mod world_generator;
mod server_overmap;


pub const SERVER_OVERMAP_SIZE: usize = CLIENT_OVERMAP_SIZE + 1;
pub const SERVER_OVERMAP_SIZE_Z: usize = CLIENT_OVERMAP_SIZE_Z + 1;

type OvermapsType = Arc<RwLock<Slab<ServerOvermap<WorldChunkSaver>>>>;

#[derive(Debug)]
pub struct World
{
	message_handler: Arc<RwLock<ConnectionsHandler>>,
	world_name: String,
	world_generator: Arc<Mutex<WorldGenerator<WorldChunkSaver>>>,
	chunk_saver: ChunkSaver,
	overmaps: OvermapsType
}

impl World
{
	pub fn new(
		message_handler: Arc<RwLock<ConnectionsHandler>>,
		tilemap: TileMap
	) -> Result<Self, ParseError>
	{
		let world_name = "default".to_owned();

        let world_path = Self::world_path_associated(&world_name);
		let chunk_saver = ChunkSaver::new(world_path.join("chunks"), 100);

		let world_generator = {
			let chunk_saver = WorldChunkSaver::new(world_path.join("world_chunks"), 100);

			WorldGenerator::new(chunk_saver, tilemap, "world_generation/")
		}?;

		let world_generator = Arc::new(Mutex::new(world_generator));

		let overmaps = Arc::new(RwLock::new(Slab::new()));

		Ok(Self{
			message_handler,
			world_name,
			world_generator,
			chunk_saver,
			overmaps
		})
	}

	pub fn add_player(&mut self, position: Pos3<f32>) -> usize
	{
		let size = Pos3::new(SERVER_OVERMAP_SIZE, SERVER_OVERMAP_SIZE, SERVER_OVERMAP_SIZE_Z);
		let overmap = ServerOvermap::new(
			self.world_generator.clone(),
			size,
			position
		);

		self.overmaps.write().insert(overmap)
	}

	pub fn remove_player(&mut self, id: usize)
	{
		self.overmaps.write().remove(id);
	}

	pub fn send_chunk(&mut self, id: usize, pos: GlobalPos)
	{
		let chunk = self.load_chunk(id, pos);

        let message = Message::ChunkSync{pos, chunk};

		self.message_handler.write().send_single(id, message);
	}

	fn load_chunk(&mut self, id: usize, pos: GlobalPos) -> Chunk
	{
		let loaded_chunk = self.chunk_saver.load(pos);

		loaded_chunk.unwrap_or_else(||
		{
			let chunk = self.overmaps.write()[id].generate_chunk(pos);

			self.chunk_saver.save(pos, chunk.clone());

			chunk
		})
	}

	#[allow(dead_code)]
	fn world_path(&self) -> PathBuf
	{
		Self::world_path_associated(&self.world_name)
	}

	fn world_path_associated(name: &str) -> PathBuf
	{
		PathBuf::from("worlds").join(name)
	}

	pub fn handle_message(&mut self, id: usize, message: Message) -> Option<Message>
	{
		match message
		{
			Message::ChunkRequest{pos} =>
			{
				self.send_chunk(id, pos);
				None
			},
			_ => Some(message)
		}
	}
}
