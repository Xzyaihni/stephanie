use std::{
	io,
	fs::File,
	path::Path
};

use serde::{Serialize, Deserialize};

use super::ChunkSaver;

use crate::common::{
	TileMap,
	world::{
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

#[derive(Debug)]
pub struct ChunkGenerator
{
	tilemap: TileMap
}

impl ChunkGenerator
{
	pub fn new(tilemap: TileMap) -> Self
	{
		Self{tilemap}
	}

	pub fn generate_chunk<'a>(
		&self,
		group: AlwaysGroup<&'a str>
	) -> Chunk
	{
		let filled_with = |name|
		{
			let tile = self.tilemap.tile_named(name).unwrap();

			use crate::common::world::Tile;
			let mut chunk = vec![Tile::none(); CHUNK_SIZE.pow(3)];
			for z in 0..CHUNK_SIZE
			{
				for y in 0..CHUNK_SIZE
				{
					for x in 0..CHUNK_SIZE
					{
						let generate = z == 0;

						if generate
						{
							chunk[z * CHUNK_SIZE.pow(2) + y * CHUNK_SIZE + x] = tile;
						}
					}
				}
			}

			Chunk::from(chunk.into_boxed_slice())
		};

		let fill_with = match group.this
		{
			"park" => "grass",
			"building" => "concrete",
			"road_vertical" | "road_horizontal" | "road_intersection" => "asphalt",
			_ => ""
		};

		if !fill_with.is_empty()
		{
			filled_with(fill_with)
		} else
		{
			Chunk::new()
		}
	}
}

#[derive(Debug)]
pub struct WorldGenerator
{
	chunk_generator: ChunkGenerator,
	chunk_saver: ChunkSaver<WorldChunk>,
	chunk_rules: Vec<ChunkRule>
}

impl WorldGenerator
{
	pub fn new<P: AsRef<Path>>(
		chunk_saver: ChunkSaver<WorldChunk>,
		tilemap: TileMap,
		path: P
	) -> Result<Self, ParseError>
	{
		let chunk_generator = ChunkGenerator::new(tilemap);
		let chunk_rules = serde_json::from_reader::<_, Vec<ChunkRule>>(File::open(path)?)?;

		Ok(Self{chunk_generator, chunk_saver, chunk_rules})
	}

	pub fn generate_missing<const SIZE: usize, F>(
		&self,
		world_chunks: &mut ChunksContainer<SIZE, Option<WorldChunk>>,
		mut to_global: F
	)
	where
		F: FnMut(LocalPos<SIZE>) -> GlobalPos
	{
		self.load_missing(world_chunks, &mut to_global);
		self.generate_wave_collapse(world_chunks, to_global);
	}

	fn generate_wave_collapse<const SIZE: usize, F>(
		&self,
		world_chunks: &mut ChunksContainer<SIZE, Option<WorldChunk>>,
		mut to_global: F
	)
	where
		F: FnMut(LocalPos<SIZE>) -> GlobalPos
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

	fn wave_collapse<const SIZE: usize, F>(
		&self,
		pos: LocalPos<SIZE>,
		mut to_global: F
	) -> WorldChunk
	where
		F: FnMut(LocalPos<SIZE>) -> GlobalPos
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

	fn load_missing<const SIZE: usize, F>(
		&self,
		world_chunks: &mut ChunksContainer<SIZE, Option<WorldChunk>>,
		mut to_global: F
	)
	where
		F: FnMut(LocalPos<SIZE>) -> GlobalPos
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