use std::sync::Arc;

use parking_lot::{Mutex, RwLock};

use slab::Slab;

use crate::{
	server::ConnectionsHandler,
	common::{
		TileMap,
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

use world_generator::{WorldGenerator, WorldChunk};

use server_overmap::ServerOvermap;

use chunk_saver::Saver;

pub use chunk_saver::ChunkSaver;
pub use world_generator::ParseError;

pub mod chunk_saver;
pub mod world_generator;
mod server_overmap;


pub const SERVER_OVERMAP_SIZE: usize = CLIENT_OVERMAP_SIZE + 1;
pub const SERVER_OVERMAP_SIZE_Z: usize = CLIENT_OVERMAP_SIZE_Z + 1;

type SaverType = ChunkSaver<WorldChunk>;
type OvermapsType = Arc<RwLock<Slab<ServerOvermap<SaverType>>>>;

#[derive(Debug)]
pub struct World
{
	message_handler: Arc<RwLock<ConnectionsHandler>>,
	world_name: String,
	world_generator: Arc<Mutex<WorldGenerator<SaverType>>>,
	chunk_saver: ChunkSaver<Chunk>,
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

		let chunk_saver = {
			let parent_path = format!("{}/chunks", Self::world_path_associated(&world_name));

			ChunkSaver::new(parent_path)
		};

		let world_generator = {
			let chunk_saver = {
				let parent_path =
					format!("{}/world_chunks", Self::world_path_associated(&world_name));

				ChunkSaver::new(parent_path)
			};

			WorldGenerator::new(chunk_saver, tilemap, "world_generation/city.json")
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

		self.message_handler.write().send_single(id, Message::ChunkSync{pos, chunk});
	}

	fn load_chunk(&mut self, id: usize, pos: GlobalPos) -> Chunk
	{
		let loaded_chunk = self.chunk_saver.load(pos);

		loaded_chunk.unwrap_or_else(||
		{
			let chunk = self.overmaps.write()[id].generate_chunk(pos);

			self.chunk_saver.save(pos, &chunk);
			chunk
		})
	}

	#[allow(dead_code)]
	fn world_path(&self) -> String
	{
		Self::world_path_associated(&self.world_name)
	}

	fn world_path_associated(name: &str) -> String
	{
		format!("worlds/{name}")
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
