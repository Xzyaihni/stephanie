use std::{
	sync::Arc
};

use super::{
	world_generator::{WorldGenerator, WorldChunk}
};

use crate::common::world::{
	LocalPos,
	GlobalPos,
	Pos3,
	Chunk,
	overmap::{Overmap, OvermapIndexing, ChunksContainer}
};


#[derive(Debug)]
struct Indexer
{
	pub size: Pos3<usize>,
	pub player_position: GlobalPos
}

impl Indexer
{
	pub fn new(size: Pos3<usize>, player_position: GlobalPos) -> Self
	{
		Self{size, player_position}
	}
}

impl OvermapIndexing for Indexer
{
	fn size(&self) -> Pos3<usize>
	{
		self.size
	}

	fn player_position(&self) -> GlobalPos
	{
		self.player_position
	}
}

#[derive(Debug)]
pub struct ServerOvermap
{
	world_generator: Arc<WorldGenerator>,
	world_chunks: ChunksContainer<Option<WorldChunk>>,
	indexer: Indexer
}

impl ServerOvermap
{
	pub fn new(
		world_generator: Arc<WorldGenerator>,
		size: Pos3<usize>,
		player_position: Pos3<f32>
	) -> Self
	{
		let indexer = Indexer::new(size, player_position.rounded());
		let world_chunks = ChunksContainer::new(size, |_| None);

		let mut this = Self{
			world_generator,
			world_chunks,
			indexer
		};

		this.generate_missing();

		this
	}

	pub fn generate_chunk(&mut self, pos: GlobalPos) -> Chunk
	{
		let margin = 1;
		let padding = 1;

		let over_edge = |value, limit| -> i32
		{
			if value < padding
			{
				(value - padding) - margin
			} else if value >= (limit as i32 - padding)
			{
				value - (limit as i32 - padding) + 1 + margin
			} else
			{
				0
			}
		};

		let GlobalPos(difference) = self.to_local_unconverted(pos);

		let size = self.indexer.size;
		let shift_offset = GlobalPos::new(
			over_edge(difference.x, size.x),
			over_edge(difference.y, size.y),
			over_edge(difference.z, size.z)
		);

		let non_shifted = shift_offset.0.x == 0 && shift_offset.0.y == 0 && shift_offset.0.z == 0;

		if non_shifted
		{
			self.generate_existing_chunk(self.to_local(pos).unwrap())
		} else
		{
			self.shift_overmap_by(shift_offset);

			self.generate_existing_chunk(self.to_local(pos).unwrap())
		}.expect("chunk must not touch any edges")
	}

	fn shift_overmap_by(&mut self, shift_offset: GlobalPos)
	{
		let new_player_position = self.indexer.player_position + shift_offset;

		self.indexer.player_position = new_player_position;

		self.position_offset(shift_offset);
	}

	fn generate_existing_chunk(&self, local_pos: LocalPos) -> Option<Chunk>
	{
		let group = local_pos.always_group();
		if group.is_none()
		{
			eprintln!("out of range {}", local_pos.pos);
		}

		group.map(|group|
		{
			let group = group.map(|position| self.world_chunks[position].unwrap());

			self.world_generator.generate_chunk(group)
		})
	}
}

impl Overmap<WorldChunk> for ServerOvermap
{
	fn remove(&mut self, pos: LocalPos)
	{
		self.world_chunks[pos] = None;
	}

	fn swap(&mut self, a: LocalPos, b: LocalPos)
	{
		self.world_chunks.swap(a, b);
	}

	fn get_local(&self, pos: LocalPos) -> &Option<WorldChunk>
	{
		&self.world_chunks[pos]
	}

	fn mark_ungenerated(&mut self, _pos: LocalPos) {}

	fn generate_missing(&mut self)
	{
		self.world_generator.generate_missing(&mut self.world_chunks, |pos|
		{
			self.indexer.to_global(pos)
		});
	}
}

impl OvermapIndexing for ServerOvermap
{
	fn size(&self) -> Pos3<usize>
	{
		self.indexer.size
	}

	fn player_position(&self) -> GlobalPos
	{
		self.indexer.player_position
	}
}