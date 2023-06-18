use std::{
	io,
	fmt,
	fs::File,
	path::Path
};

use parking_lot::Mutex;

use rlua::{
	Lua,
	StdLib
};

use serde::{Serialize, Deserialize};

use super::ChunkSaver;

use crate::common::{
	TileMap,
	world::{
		Pos3,
		Chunk,
		LocalPos,
		GlobalPos,
		AlwaysGroup,
		DirectionsGroup,
		CHUNK_SIZE,
		overmap::ChunksContainer
	}
};


#[derive(Debug, Serialize, Deserialize)]
pub struct NeighborChance
{
	pub weight: usize,
	pub name: String
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ChunkRule
{
	pub name: String,
	pub neighbors: DirectionsGroup<Vec<NeighborChance>>
}

#[derive(Debug)]
pub enum ParseError
{
	Io(io::Error),
	Json(serde_json::Error)
}

impl From<io::Error> for ParseError
{
	fn from(value: io::Error) -> Self
	{
		ParseError::Io(value)
	}
}

impl From<serde_json::Error> for ParseError
{
	fn from(value: serde_json::Error) -> Self
	{
		ParseError::Json(value)
	}
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct WorldChunk
{
	id: usize
}

impl WorldChunk
{
	pub fn new(id: usize) -> Self
	{
		Self{id}
	}

	#[allow(dead_code)]
	pub fn none() -> Self
	{
		Self{id: 0}
	}

	pub fn id(&self) -> usize
	{
		self.id
	}
}

struct GenerateState
{
	lua: Mutex<Lua>
}

impl GenerateState
{
	pub fn new() -> Self
	{
		let lua = Mutex::new(Lua::new_with(StdLib::empty()));

		Self{lua}
	}
}

pub struct ChunkGenerator
{
	tilemap: TileMap,
	states: Box<[GenerateState]>
}

impl ChunkGenerator
{
	pub fn new(tilemap: TileMap, chunk_rules: &[ChunkRule]) -> Self
	{
		// yea im not writing chunk generation generically over size or anything like that
		assert_eq!(CHUNK_SIZE, 16);

		let states = chunk_rules.iter().map(|rule|
		{
			GenerateState::new()
		}).collect::<Vec<_>>().into_boxed_slice();

		Self{tilemap, states}
	}

	pub fn generate_chunk<'a>(
		&self,
		group: AlwaysGroup<&'a str>
	) -> Chunk
	{
		let mut chunk = Chunk::new();

		for z in 0..CHUNK_SIZE
		{
			for y in 0..CHUNK_SIZE
			{
				for x in 0..CHUNK_SIZE
				{
					let pos = crate::common::world::ChunkLocal::new(x, y, z);
					chunk[pos] = self.tilemap.tile_named("concrete").unwrap();
				}
			}
		}

		chunk
	}
}

impl fmt::Debug for ChunkGenerator
{
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result
	{
		f.debug_struct("ChunkGenerator")
			.field("tilemap", &self.tilemap)
			.finish()
	}
}

#[derive(Debug)]
pub struct WorldGenerator
{
	chunk_generator: ChunkGenerator,
	chunk_saver: ChunkSaver<WorldChunk>,
	size: Pos3<usize>,
	chunk_rules: Box<[ChunkRule]>
}

impl WorldGenerator
{
	pub fn new<P: AsRef<Path>>(
		chunk_saver: ChunkSaver<WorldChunk>,
		tilemap: TileMap,
		size: Pos3<usize>,
		path: P
	) -> Result<Self, ParseError>
	{
		let chunk_rules = serde_json::from_reader::<_, Vec<ChunkRule>>(File::open(path)?)?
			.into_boxed_slice();

		let chunk_generator = ChunkGenerator::new(tilemap, &chunk_rules);

		Ok(Self{chunk_generator, chunk_saver, size, chunk_rules})
	}

	pub fn generate_missing<F>(
		&self,
		world_chunks: &mut ChunksContainer<Option<WorldChunk>>,
		mut to_global: F
	)
	where
		F: FnMut(LocalPos) -> GlobalPos
	{
		self.load_missing(world_chunks, &mut to_global);
		self.generate_wave_collapse(world_chunks, to_global);
	}

	fn generate_wave_collapse<F>(
		&self,
		world_chunks: &mut ChunksContainer<Option<WorldChunk>>,
		mut to_global: F
	)
	where
		F: FnMut(LocalPos) -> GlobalPos
	{
		/*loop
		{
			break;
		}*/
		world_chunks.iter_mut().filter(|(_, chunk)| chunk.is_none())
			.for_each(|(pos, chunk)|
			{
				let generated_chunk = self.wave_collapse(pos, &mut to_global);

				*chunk = Some(generated_chunk);
			});
	}

	fn wave_collapse<F>(
		&self,
		pos: LocalPos,
		mut to_global: F
	) -> WorldChunk
	where
		F: FnMut(LocalPos) -> GlobalPos
	{
		let global_pos = to_global(pos);
		let generate = global_pos.0.z == -1 || global_pos.0.z == 0;

		let chunk = if generate
		{
			WorldChunk::new(fastrand::usize(1..self.chunk_rules.len()))
		} else
		{
			WorldChunk::none()
		};

		self.chunk_saver.save(global_pos, &chunk);

		chunk
	}

	fn load_missing<F>(
		&self,
		world_chunks: &mut ChunksContainer<Option<WorldChunk>>,
		mut to_global: F
	)
	where
		F: FnMut(LocalPos) -> GlobalPos
	{
		world_chunks.iter_mut().filter(|(_, chunk)| chunk.is_none())
			.for_each(|(pos, chunk)|
			{
				let loaded_chunk = self.chunk_saver.load(to_global(pos));

				loaded_chunk.map(|loaded_chunk|
				{
					*chunk = Some(loaded_chunk);
				});
			});
	}

	pub fn generate_chunk(
		&self,
		group: AlwaysGroup<WorldChunk>
	) -> Chunk
	{
		self.chunk_generator.generate_chunk(group.map(|world_chunk|
		{
			&self.chunk_rules[world_chunk.id()].name[..]
		}))
	}
}